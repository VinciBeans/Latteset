// StatusBar 单测：锁 ADR-0015 之后状态栏的两件事——
// ① 引擎名是常量 Tectonic（不读设置、不拉清单）；形态位已随 v3 删除，状态栏不再显示形态；
// ② 两套提示 chip：`Settings.notices`（持久，点击 dismiss_notice 写盘）与
//    `compile-notice` 落进 noticesStore 的运行时提示（非持久，点击清空）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import StatusBar from "../StatusBar.vue";
import { useCompileStore } from "../../stores/compile";
import { useEditorStore } from "../../stores/editor";
import { useNoticesStore } from "../../stores/notices";
import { usePreviewStore } from "../../stores/preview";
import { useProjectStore } from "../../stores/project";
import { useSettingsStore } from "../../stores/settings";
import type { ProjectInfo, Settings } from "../../bindings";

/** 后端 `Settings.notices` 的一项（`{id, level, message}`）。 */
type Notice = { id: string; level: "info" | "warning"; message: string };
type SettingsWithNotices = Settings & { notices: Notice[] };

/** mock 自己的"后端状态"（每个用例前由 mountBar 重置）。 */
let stored: SettingsWithNotices;
/** 最近一次 `update_settings` 的 patch（dismiss_notice 断言用）。 */
let lastPatch: Record<string, unknown> | null = null;

vi.mock("../../services/ipc", () => ({
  ipc: {
    // A-11：确认"采用磁盘版本"后 editor.acceptExternal 会读盘
    readFile: vi.fn(async () => "disk version"),
    updateSettings: vi.fn(async (patch: Record<string, unknown>) => {
      lastPatch = patch;
      // 真实后端会把这条通知从磁盘设置里删掉 ⇒ 桩也照做，点击后 chip 应当消失
      if (typeof patch.dismiss_notice === "string") {
        stored.notices = stored.notices.filter((n) => n.id !== patch.dismiss_notice);
      }
      return structuredClone(stored);
    }),
  },
}));

beforeEach(() => {
  lastPatch = null;
});

async function mountBar(notices: Notice[] = [], runtime: string[] = [], attach = false) {
  stored = {
    schema_version: 3,
    compile: { mode: "continuous", debounce_ms: 500, timeout_secs: 120 },
    root_file: null,
    notices,
  };
  const pinia = createPinia();
  setActivePinia(pinia);
  useSettingsStore().setSettings(structuredClone(stored));
  // 运行时提示来自 `compile-notice`（events.ts → noticesStore）：这里直接写 store，模拟事件已到
  const noticesStore = useNoticesStore();
  for (const text of runtime) noticesStore.push(text);
  const w = mount(StatusBar, {
    props: { cursorLine: 1, cursorCol: 1 },
    global: { plugins: [pinia] },
    ...(attach ? { attachTo: document.body } : {}),
  });
  await flushPromises();
  return w;
}

describe("StatusBar：引擎（ADR-0015）", () => {
  it("引擎名是常量 Tectonic（设置里没有 engine 字段也照常显示），且不再有形态 chip", async () => {
    const w = await mountBar();
    expect(w.find(".engine").text()).toBe("Tectonic");
    expect(w.find(".engine").attributes("title")).toContain("当前引擎：Tectonic");
    // 形态位（`.form`）已随 v3 删除：状态栏不再显示"子进程 / 库内嵌"
    expect(w.find(".form").exists()).toBe(false);
    w.unmount();
  });
});

describe("StatusBar：持久提示 chip（Settings.notices）", () => {
  it("warning 用琥珀色 chip 渲染；点击发 dismiss_notice 并消失", async () => {
    const w = await mountBar([
      { id: "engine-unified", level: "warning", message: "引擎已统一为内嵌 Tectonic，原引擎设置不再生效" },
    ]);
    const chip = w.find(".notice");
    expect(chip.exists()).toBe(true);
    expect(chip.classes()).toContain("warn");
    expect(chip.text()).toContain("引擎已统一为内嵌 Tectonic");

    await chip.trigger("click");
    await flushPromises();
    expect(lastPatch).toEqual({ dismiss_notice: "engine-unified" });
    expect(w.find(".notice").exists()).toBe(false);
    w.unmount();
  });

  it("info 用中性 chip（不是错误样式，也不进错误列表）", async () => {
    const w = await mountBar([{ id: "n1", level: "info", message: "本次编译使用内嵌宏包集" }]);
    const chip = w.find(".notice");
    expect(chip.classes()).toContain("info");
    expect(chip.classes()).not.toContain("warn");
    expect(chip.classes()).not.toContain("open-error");
    w.unmount();
  });

  it("没有通知时不渲染 chip", async () => {
    const w = await mountBar();
    expect(w.find(".notice").exists()).toBe(false);
    w.unmount();
  });
});

describe("StatusBar：运行时提示（compile-notice → noticesStore）", () => {
  it("点击不销毁（非持久但也不清空）：展开全文 + 请求展开问题面板（C-02）", async () => {
    const w = await mountBar([], ["文档调用了 biber，但本机缺失或版本不匹配"]);
    const noticesStore = useNoticesStore();
    const chip = w.find(".notice.runtime");
    expect(chip.exists()).toBe(true);
    expect(chip.text()).toContain("biber");

    await chip.trigger("click");
    await flushPromises();
    // 点击语义从"清空"改为"展开看全文"：chip 仍在、store 仍在
    expect(noticesStore.runtime).toHaveLength(1);
    expect(w.find(".notice.runtime").exists()).toBe(true);
    const pop = w.find('[data-testid="notice-popover"]');
    expect(pop.exists()).toBe(true);
    expect(pop.text()).toContain("biber");
    expect(w.emitted("expand-problems")).toBeTruthy();
    // 再点一次收起全文
    await chip.trigger("click");
    await flushPromises();
    expect(w.find('[data-testid="notice-popover"]').exists()).toBe(false);
    w.unmount();
  });

  it("编译成功不清运行时提示（只有编译期结束/用户交互才谈得上清）", async () => {
    const w = await mountBar([], ["需要 makeindex"]);
    const noticesStore = useNoticesStore();
    expect(w.find(".notice.runtime").exists()).toBe(true);

    useCompileStore().setStatus("success", null, false);
    await flushPromises();
    expect(noticesStore.runtime).toHaveLength(1);
    expect(w.find(".notice.runtime").exists()).toBe(true);
    w.unmount();
  });
});

// A-01：未打开项目不渲染 phase；打开但未编译 →「未编译」；成功才「就绪」
describe("StatusBar：未编译态（A-01）", () => {
  function openProject(rootFile: string | null = "E:/proj/main.tex") {
    useProjectStore().project = {
      root: "E:/proj",
      root_file: rootFile,
      root_candidates: [],
    } as ProjectInfo;
  }

  it("未打开项目：不渲染 phase chip", async () => {
    const w = await mountBar();
    expect(w.find(".phase").exists()).toBe(false);
    w.unmount();
  });

  it("已打开但未编译：显示「未编译」，且用中性灰（不是成功的绿色）", async () => {
    const w = await mountBar();
    openProject();
    await flushPromises();
    const phase = w.find(".phase");
    expect(phase.exists()).toBe(true);
    expect(phase.text()).toBe("未编译");
    expect(phase.classes()).toContain("idle");
    expect(phase.classes()).not.toContain("success");
    w.unmount();
  });

  it("编译成功后才显示「就绪」；编译中显示进行态", async () => {
    const w = await mountBar();
    openProject();
    await flushPromises();
    const compile = useCompileStore();

    compile.setStatus("running", null, false);
    await flushPromises();
    expect(w.find(".phase").text()).toBe("排版中…");

    compile.setStatus("success", null, false);
    await flushPromises();
    expect(w.find(".phase").text()).toBe("就绪");
    expect(w.find(".phase").classes()).toContain("success");
    w.unmount();
  });
});

// A-04：流式页数只出现一处（编译中预览已在预览面板显示 N 页时，状态栏不再重复）
describe("StatusBar：页数不重复（A-04）", () => {
  it("有编译中预览时不显示「已排版 N 页」；没有预览时才显示", async () => {
    const w = await mountBar();
    const compile = useCompileStore();
    const preview = usePreviewStore();

    compile.setStatus("running", null, false);
    compile.setProgress(7);
    await flushPromises();
    expect(w.find(".pages").exists()).toBe(true);

    preview.onCompilePreview({ path: "E:/proj/tmp/main.preview.pdf", pages: 7 });
    await flushPromises();
    expect(w.find(".pages").exists()).toBe(false);
    w.unmount();
  });
});

// A-11：外部修改 chip 点击后先二次确认（采用磁盘 = 放弃本地修改，不可逆）
describe("StatusBar：外部修改二次确认（A-11）", () => {
  it("点 chip 只弹确认框；确认后才读盘采用磁盘版本", async () => {
    const w = await mountBar();
    const editor = useEditorStore();
    const path = "E:/proj/main.tex";
    editor.externalConflict.add(path);
    await flushPromises();

    const chip = w.find(".conflict");
    expect(chip.exists()).toBe(true);
    await chip.trigger("click");
    await flushPromises();
    // 未确认前：仍未读盘，冲突仍在
    const ipcMod = await import("../../services/ipc");
    expect(ipcMod.ipc.readFile).not.toHaveBeenCalled();
    expect(editor.externalConflict.has(path)).toBe(true);
    expect(w.find('[data-testid="confirm-dialog"]').exists()).toBe(true);

    await w.find('[data-testid="confirm-ok"]').trigger("click");
    await flushPromises();
    expect(ipcMod.ipc.readFile).toHaveBeenCalledWith(path);
    expect(editor.externalConflict.has(path)).toBe(false);
    expect(w.find('[data-testid="confirm-dialog"]').exists()).toBe(false);
    w.unmount();
  });

  it("取消则保留本地与冲突标记", async () => {
    const w = await mountBar();
    const editor = useEditorStore();
    const path = "E:/proj/ch.tex";
    editor.externalConflict.add(path);
    await flushPromises();
    await w.find(".conflict").trigger("click");
    await flushPromises();
    await w.find('[data-testid="confirm-cancel"]').trigger("click");
    await flushPromises();
    expect(editor.externalConflict.has(path)).toBe(true);
    w.unmount();
  });
});

// A-12：编译模式入口是带可见说明的菜单；title 只作补充
describe("StatusBar：编译模式菜单（A-12）", () => {
  it("菜单里两个模式各自带可见说明；点选后写设置", async () => {
    const w = await mountBar();
    await w.find(".mode").trigger("click");
    await flushPromises();
    const items = w.findAll(".mode-item");
    expect(items).toHaveLength(2);
    const text = w.find(".mode-menu").text();
    expect(text).toContain("编辑停顿后自动编译");
    expect(text).toContain("点「编译」或保存文件时编译");
    expect(w.find(".mode").attributes("title")).toContain("当前：连续编译");

    await items[1].trigger("click");
    await flushPromises();
    expect(lastPatch).toEqual({ mode: "on_save" });
    expect(w.find(".mode-menu").exists()).toBe(false);
    w.unmount();
  });

  it("键盘可达：ArrowDown 移动焦点、Esc 关闭并把焦点还给按钮（A-12）", async () => {
    // 焦点断言需要真实挂到 document（happy-dom 里未附加的节点 focus() 不生效）
    const w = await mountBar([], [], true);
    await w.find(".mode").trigger("click");
    await flushPromises();
    const menu = w.find(".mode-menu");
    // 打开即聚焦第一项
    expect(document.activeElement?.textContent).toContain("连续编译");

    await menu.trigger("keydown", { key: "ArrowDown" });
    expect(document.activeElement?.textContent).toContain("保存时编译");

    await menu.trigger("keydown", { key: "Escape" });
    await flushPromises();
    expect(w.find(".mode-menu").exists()).toBe(false);
    expect(document.activeElement).toBe(w.find(".mode").element);
    w.unmount();
  });

  it("点击菜单外部即关闭（A-12）", async () => {
    const w = await mountBar();
    await w.find(".mode").trigger("click");
    await flushPromises();
    expect(w.find(".mode-menu").exists()).toBe(true);
    document.dispatchEvent(new Event("pointerdown"));
    await flushPromises();
    expect(w.find(".mode-menu").exists()).toBe(false);
    w.unmount();
  });
});
