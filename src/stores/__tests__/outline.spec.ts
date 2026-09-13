// outlineStore 单测（roadmap ⑦a）：**只提交变化过的缓冲**（`lastSent` 差分）+ 上报打开的标签集合、
// 进行中的刷新合并，以及"项目根变化必须全量重发"这条不变量（后端缓存按项目根作废，漏了会让脏缓冲
// 回落到磁盘旧内容）。
import { describe, it, expect, beforeEach, vi } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { useOutlineStore } from "../outline";
import { useEditorStore } from "../editor";
import { useProjectStore } from "../project";
import { ipc } from "../../services/ipc";
import type { OutlineNode } from "../../bindings";

type Buffers = { path: string; content: string }[];

vi.mock("../../services/ipc", () => ({
  ipc: {
    getOutline: vi.fn(async (_buffers: Buffers, _files: string[] | null, _openPaths: string[]) => [] as OutlineNode[]),
  },
}));

const getOutline = vi.mocked(ipc.getOutline);

/** 每次调用的实参快照（buffers / openPaths）。 */
function calls() {
  return getOutline.mock.calls.map((c) => ({ buffers: c[0], openPaths: c[2] }));
}

describe("outlineStore.refresh（增量提交）", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    getOutline.mockClear();
  });

  function setupProject(root = "E:/proj") {
    const project = useProjectStore();
    project.project = { root, root_file: `${root}/main.tex` } as never;
    return project;
  }

  it("首次刷新提交全部打开缓冲与标签集合", async () => {
    setupProject();
    const editor = useEditorStore();
    editor.buffers.set("E:/proj/main.tex", "\\section{A}");
    editor.buffers.set("E:/proj/ch.tex", "\\section{B}");

    await useOutlineStore().refresh();

    expect(calls()).toHaveLength(1);
    expect(calls()[0].buffers.map((b) => b.path).sort()).toEqual(["E:/proj/ch.tex", "E:/proj/main.tex"]);
    expect([...calls()[0].openPaths].sort()).toEqual(["E:/proj/ch.tex", "E:/proj/main.tex"]);
  });

  it("第二次刷新（内容未变）不再重发任何缓冲", async () => {
    setupProject();
    const editor = useEditorStore();
    editor.buffers.set("E:/proj/main.tex", "\\section{A}");
    const outline = useOutlineStore();
    await outline.refresh();

    await outline.refresh();

    expect(calls()).toHaveLength(2);
    expect(calls()[1].buffers).toEqual([]);
    expect(calls()[1].openPaths).toEqual(["E:/proj/main.tex"]);
  });

  it("只有一个文件变了 → 只提交那一个", async () => {
    setupProject();
    const editor = useEditorStore();
    editor.buffers.set("E:/proj/main.tex", "\\section{A}");
    editor.buffers.set("E:/proj/ch.tex", "\\section{B}");
    const outline = useOutlineStore();
    await outline.refresh();

    editor.buffers.set("E:/proj/ch.tex", "\\section{B 改了}");
    await outline.refresh();

    expect(calls()[1].buffers).toEqual([{ path: "E:/proj/ch.tex", content: "\\section{B 改了}" }]);
  });

  it("标签关闭 → 不再出现在 openPaths（后端据此淘汰缓冲）", async () => {
    setupProject();
    const editor = useEditorStore();
    editor.buffers.set("E:/proj/main.tex", "\\section{A}");
    editor.buffers.set("E:/proj/ch.tex", "\\section{B}");
    const outline = useOutlineStore();
    await outline.refresh();

    editor.buffers.delete("E:/proj/ch.tex");
    await outline.refresh();

    expect(calls()[1].openPaths).toEqual(["E:/proj/main.tex"]);
  });

  it("项目根变化 → 即使内容未变也必须全量重发（后端缓存已作废）", async () => {
    const project = setupProject("E:/proj");
    const editor = useEditorStore();
    editor.buffers.set("E:/proj/main.tex", "\\section{A}");
    const outline = useOutlineStore();
    await outline.refresh();

    // 同一个缓冲路径（内容未变）在新项目根下重新打开
    project.project = { root: "E:/other", root_file: "E:/other/main.tex" } as never;
    await outline.refresh();

    expect(calls()[1].buffers).toEqual([{ path: "E:/proj/main.tex", content: "\\section{A}" }]);
  });

  it("未打开项目 → 清空 items 与差分记录", async () => {
    const project = useProjectStore();
    project.project = { root: "E:/proj", root_file: "E:/proj/main.tex" } as never;
    const editor = useEditorStore();
    editor.buffers.set("E:/proj/main.tex", "\\section{A}");
    const outline = useOutlineStore();
    await outline.refresh();
    outline.items = [{ title: "A" } as never];

    project.project = null;
    await outline.refresh();

    expect(outline.items).toEqual([]);
    expect(calls()).toHaveLength(1); // 未打开项目 → 不发 IPC
  });

  it("并发刷新合并为「当前 + 补一次」，不发第三次（结构事件风暴不再连发）", async () => {
    setupProject();
    const editor = useEditorStore();
    editor.buffers.set("E:/proj/main.tex", "\\section{A}");
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    getOutline.mockImplementationOnce(async () => {
      await gate;
      return [];
    });
    const outline = useOutlineStore();
    const first = outline.refresh(); // 进行中
    await Promise.resolve();
    const second = outline.refresh(); // 进行中 → 只记"待补一次"
    const third = outline.refresh(); // 同上（不再排队第三次）
    release();
    await Promise.all([first, second, third]);

    expect(calls()).toHaveLength(2); // 第一次 + 补的一次
    // 补的那次：内容未变 → 不再重发缓冲（差分仍然生效）
    expect(calls()[1].buffers).toEqual([]);
  });
});
