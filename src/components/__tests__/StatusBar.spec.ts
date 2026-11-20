// StatusBar 单测：锁 ADR-0015 之后状态栏的两件事——
// ① 引擎名是常量 Tectonic（不读设置、不拉清单）；形态位已随 v3 删除，状态栏不再显示形态；
// ② 两套提示 chip：`Settings.notices`（持久，点击 dismiss_notice 写盘）与
//    `compile-notice` 落进 noticesStore 的运行时提示（非持久，点击清空）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import StatusBar from "../StatusBar.vue";
import { useCompileStore } from "../../stores/compile";
import { useNoticesStore } from "../../stores/notices";
import { useSettingsStore } from "../../stores/settings";
import type { Settings } from "../../bindings";

/** 后端 `Settings.notices` 的一项（`{id, level, message}`）。 */
type Notice = { id: string; level: "info" | "warning"; message: string };
type SettingsWithNotices = Settings & { notices: Notice[] };

/** mock 自己的"后端状态"（每个用例前由 mountBar 重置）。 */
let stored: SettingsWithNotices;
/** 最近一次 `update_settings` 的 patch（dismiss_notice 断言用）。 */
let lastPatch: Record<string, unknown> | null = null;

vi.mock("../../services/ipc", () => ({
  ipc: {
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

async function mountBar(notices: Notice[] = [], runtime: string[] = []) {
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
  it("渲染 chip，点击清空（非持久）", async () => {
    const w = await mountBar([], ["文档调用了 biber，但本机缺失或版本不匹配"]);
    const noticesStore = useNoticesStore();
    const chip = w.find(".notice.runtime");
    expect(chip.exists()).toBe(true);
    expect(chip.text()).toContain("biber");

    await chip.trigger("click");
    await flushPromises();
    expect(noticesStore.runtime).toEqual([]);
    expect(w.find(".notice.runtime").exists()).toBe(false);
    w.unmount();
  });

  it("编译成功不清运行时提示（只有用户点击才清）", async () => {
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
