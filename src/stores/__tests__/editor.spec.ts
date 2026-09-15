// editorStore 单测（modules.md §9.2 / §5.5）：openFile 去重、onFilesChanged 自保存过滤/冲突。
// mock services/ipc，避免加载真实 Tauri IPC。
import { describe, it, expect, beforeEach, vi } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { useEditorStore } from "../editor";
import { useProjectStore } from "../project";
import { ipc } from "../../services/ipc";

vi.mock("../../services/ipc", () => ({
  ipc: {
    readFile: vi.fn(async (p: string) => `content-of:${p}`),
    saveAll: vi.fn(async () => []),
  },
}));

const ROOT = "E:/Works/tex-presso/test_file/projects/multifile";
const MAIN = `${ROOT}/main.tex`;

describe("editorStore", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    const project = useProjectStore();
    project.project = { root: ROOT } as never;
  });

  it("openFile：打开一次生成单标签", async () => {
    const editor = useEditorStore();
    await editor.openFile(MAIN);
    expect(editor.tabs.map((t) => t.path)).toEqual([MAIN]);
    expect(editor.activePath).toBe(MAIN);
  });

  it("openFile：`./` 绝对路径归一为已开标签路径，不重复开（回归：resolvePath 修复）", async () => {
    const editor = useEditorStore();
    await editor.openFile(MAIN);
    await editor.openFile(`${ROOT}/./main.tex`); // synctex 反向返回的格式
    expect(editor.tabs.map((t) => t.path)).toEqual([MAIN]); // 仍 1 个
    expect(editor.activePath).toBe(MAIN);
  });

  it("openFile：并发调用不重复开标签（去重竞态回归：await 前 some 判断不够）", async () => {
    const editor = useEditorStore();
    // 两个 openFile 在 await 前都通过了初始 some 判断；靠 await 后复检去重
    const p1 = editor.openFile(MAIN);
    const p2 = editor.openFile(MAIN);
    await Promise.all([p1, p2]);
    expect(editor.tabs.map((t) => t.path)).toEqual([MAIN]);
  });

  it("onFilesChanged：已打开且不脏 → 静默重载 buffer", async () => {
    const editor = useEditorStore();
    await editor.openFile(MAIN);
    editor.buffers.set(MAIN, "old");
    await editor.onFilesChanged([MAIN]);
    expect(editor.buffers.get(MAIN)).toBe(`content-of:${MAIN}`);
    expect(editor.externalConflict.has(MAIN)).toBe(false);
  });

  it("onFilesChanged：已打开且脏 → 保留本地 + 冲突标记（components 冲突对话框数据源）", async () => {
    const editor = useEditorStore();
    await editor.openFile(MAIN);
    editor.markDirty(MAIN, "local-edit");
    await editor.onFilesChanged([MAIN]);
    expect(editor.buffers.get(MAIN)).toBe("local-edit"); // 不覆盖本地
    expect(editor.externalConflict.has(MAIN)).toBe(true);
  });

  it("onFilesChanged：最近自保存（<2s）→ 忽略，不重载", async () => {
    const editor = useEditorStore();
    await editor.openFile(MAIN);
    editor.buffers.set(MAIN, "saved-content");
    editor.markSaved([MAIN]); // lastSaved = now
    await editor.onFilesChanged([MAIN]);
    expect(editor.buffers.get(MAIN)).toBe("saved-content"); // 未重载
  });

  it("onFilesChanged：未打开 → 忽略", async () => {
    const editor = useEditorStore();
    await editor.onFilesChanged([`${ROOT}/other.tex`]);
    expect(editor.tabs.length).toBe(0);
  });

  it("closeTab：移除标签并清理脏/缓冲", async () => {
    const editor = useEditorStore();
    await editor.openFile(MAIN);
    editor.markDirty(MAIN, "x");
    editor.closeTab(MAIN);
    expect(editor.tabs.length).toBe(0);
    expect(editor.dirty.has(MAIN)).toBe(false);
    expect(editor.buffers.has(MAIN)).toBe(false);
  });

  // ---- roadmap ㉓：打开失败不再静默（非 UTF-8 源文件等）----

  it("openFile：读盘失败 → 不开标签、设 openError（此前是无人接的 rejection）", async () => {
    vi.mocked(ipc.readFile).mockRejectedValueOnce({
      code: "Invalid",
      message: "旧文件.tex 不是 UTF-8 编码（中文旧文件常见 GBK/GB18030），为避免保存时损坏原文件，编辑器不打开它。",
    });
    const editor = useEditorStore();
    await editor.openFile(`${ROOT}/旧文件.tex`);
    expect(editor.tabs.length).toBe(0);
    expect(editor.activePath).toBeNull();
    expect(editor.openError).toContain("不是 UTF-8 编码");
  });

  it("openFile：失败后再次成功打开 → 清掉 openError", async () => {
    vi.mocked(ipc.readFile).mockRejectedValueOnce({ code: "Invalid", message: "坏了" });
    const editor = useEditorStore();
    await editor.openFile(`${ROOT}/bad.tex`);
    expect(editor.openError).toBe("坏了");
    await editor.openFile(MAIN);
    expect(editor.openError).toBeNull();
    expect(editor.tabs.map((t) => t.path)).toEqual([MAIN]);
  });

  // ---- 回归：同一次自保存会来多条事件 ----
  // Windows notify 对**一次**写盘投递 2 条（实测同一毫秒、都在 `markSaved` 之前到达）。
  // 把 `lastSaved` 当"消费一次"的令牌时，第二条会撞上"仍脏" ⇒ 误报「外部修改」；
  // 而那个提示的动作是 `acceptExternal`（**放弃本地**）⇒ 误报 + 一点击就丢输入。
  // ⚠ 这三条会改共享 mock 的返回值，所以放在文件**末尾**，避免影响上面的用例。

  it("onFilesChanged：同一次自保存的两条事件（markSaved 尚未跑）→ 不得误报冲突", async () => {
    const editor = useEditorStore();
    await editor.openFile(MAIN);
    // 真实保存时序：markSaving 先落（乐观记录），磁盘已是新内容，但脏标记还没清
    const saved = "probe-A";
    editor.buffers.set(MAIN, saved);
    editor.markDirty(MAIN, saved);
    editor.markSaving([MAIN]);
    vi.mocked(ipc.readFile).mockResolvedValue(saved); // 磁盘 == 缓冲（自己刚写的）
    await editor.onFilesChanged([MAIN, MAIN]); // notify 的两条事件，同一批逐条投递
    expect(editor.externalConflict.has(MAIN)).toBe(false);
    expect(editor.buffers.get(MAIN)).toBe(saved); // 本地未被覆盖
  });

  it("onFilesChanged：窗口之外，磁盘与缓冲一致 → 仍不算冲突（不靠时间猜）", async () => {
    const editor = useEditorStore();
    await editor.openFile(MAIN);
    editor.markDirty(MAIN, "同内容");
    vi.mocked(ipc.readFile).mockResolvedValue("同内容");
    await editor.onFilesChanged([MAIN]); // 无 lastSaved ⇒ withinSelfSave 为假
    expect(editor.externalConflict.has(MAIN)).toBe(false);
  });

  it("onFilesChanged：窗口之外，磁盘与缓冲不一致 → 仍然要报冲突（别把真冲突修没了）", async () => {
    const editor = useEditorStore();
    await editor.openFile(MAIN);
    editor.markDirty(MAIN, "我的未保存输入");
    vi.mocked(ipc.readFile).mockResolvedValue("外部工具写的内容");
    await editor.onFilesChanged([MAIN]);
    expect(editor.externalConflict.has(MAIN)).toBe(true);
    expect(editor.buffers.get(MAIN)).toBe("我的未保存输入"); // 本地不丢
  });

  // ---- 回归：外部重载不是用户编辑（真机实测 2/2，见 troubleshooting.md）----
  // 干净分支读盘期间，前一条事件的重载会经 EditorPane 的 `setValue` 把 tab 标脏；
  // 若复检只看脏标记，第二条事件就误报「外部修改」——而磁盘与缓冲其实完全一致。

  it("onFilesChanged：读盘期间被重载标脏，但磁盘==缓冲 → 不得误报冲突", async () => {
    const editor = useEditorStore();
    await editor.openFile(MAIN);
    const reloaded = `content-of:${MAIN}`;
    let release!: (v: string) => void;
    vi.mocked(ipc.readFile).mockImplementationOnce(() => new Promise<string>((r) => { release = r; }));
    const pending = editor.onFilesChanged([MAIN]); // 干净分支：await 读盘挂起
    editor.markDirty(MAIN, reloaded);              // 重载的 setValue 把它标脏（内容 = 磁盘）
    release(reloaded);
    await pending;
    expect(editor.externalConflict.has(MAIN)).toBe(false);
    expect(editor.buffers.get(MAIN)).toBe(reloaded);
  });

  it("onFilesChanged：读盘期间用户输入了别的内容 → 仍然报冲突，本地不丢", async () => {
    const editor = useEditorStore();
    await editor.openFile(MAIN);
    let release!: (v: string) => void;
    vi.mocked(ipc.readFile).mockImplementationOnce(() => new Promise<string>((r) => { release = r; }));
    const pending = editor.onFilesChanged([MAIN]);
    editor.markDirty(MAIN, "读盘期间敲的字");
    release("外部工具写的内容");
    await pending;
    expect(editor.externalConflict.has(MAIN)).toBe(true);
    expect(editor.buffers.get(MAIN)).toBe("读盘期间敲的字");
  });
});
