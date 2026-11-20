// SettingsPanel 单测：锁三条**只在界面上成立**的契约——
// ① 设置分页（标签栏把外观/编译/引擎/项目分开，别混成一长条）；
// ② 引擎页只剩 Tectonic 的资源（ADR-0015：引擎不可选、形态固定为内嵌——没有下拉、没有清单、
//    没有"存了但不生效"的形态开关，只剩 bundle / cache 两项资源）；
// ③ bundle 在磁盘上存的是上游认的 `file:///…`，界面上要显示成普通目录路径（用户不该看 URL 前缀）。
// 路径的**归一化**（`E:\…` → `file:///E:/…`）在 core，由 `latteset-core` 的单测覆盖，不在这里重复。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import SettingsPanel from "../SettingsPanel.vue";
import { useSettingsStore } from "../../stores/settings";
import type { Settings } from "../../bindings";

/** 后端 settings.json 的当前值（mock 自己的状态；每个用例前重置）。
 *  `tectonic` 在绑定里是可选字段（后端 `#[serde(default)]` 为兼容旧 settings.json），
 *  但这里是我们自己造的完整对象 ⇒ 收紧成必填，免得每处都写 `?.`。
 *  `notices` 是后端的一次性提示（`{id, level, message}`）：绑定重生成前先按结构写进夹具，
 *  这样后端加字段后本文件不必再动。 */
type PanelSettings = Omit<Settings, "tectonic"> & {
  tectonic: NonNullable<Settings["tectonic"]>;
  notices?: { id: string; level: string; message: string }[];
};
let stored: PanelSettings;
/** 最近一次 `update_settings` 的 patch（"恢复默认不再写 engine 键"就是靠它断言的）。 */
let lastPatch: Record<string, unknown> | null = null;

function freshSettings(bundle: string | null): PanelSettings {
  return {
    schema_version: 3,
    compile: { mode: "continuous", debounce_ms: 500, timeout_secs: 120 },
    tectonic: { bundle, cache_dir: null },
    root_file: null,
    notices: [],
  };
}

// 桩，不是后端复刻：只把本文件用到的键写回（bundle），其余原样返回。
vi.mock("../../services/ipc", () => ({
  ipc: {
    getSettings: vi.fn(async () => structuredClone(stored)),
    updateSettings: vi.fn(async (patch: Record<string, unknown>) => {
      lastPatch = patch;
      if (patch.bundle !== undefined) {
        stored.tectonic.bundle = patch.bundle === "" ? null : (patch.bundle as string);
      }
      return structuredClone(stored);
    }),
  },
}));

beforeEach(() => {
  lastPatch = null;
});

/** 点某个标签（标签栏是唯一入口，测试也走这条路，不直接改内部状态）。 */
async function openTab(w: ReturnType<typeof mount>, label: string) {
  const btn = w.findAll(".tab-btn").find((b) => b.text().startsWith(label));
  if (!btn) throw new Error(`没有「${label}」标签，现有：${w.findAll(".tab-btn").map((b) => b.text())}`);
  await btn.trigger("click");
  await flushPromises();
}

/**
 * 面板假定 store 已由 App 初始化过 ⇒ 这里直接把设置塞进 store，再挂载。
 * `tab` 显式指定要看哪一页：`activeTab` 是**模块级**状态（故意跨开关保留），
 * 所以每个用例都必须自己声明，否则会串味。
 */
async function mountPanel(bundle: string | null = null, tab = "引擎（Tectonic）") {
  stored = freshSettings(bundle);
  const pinia = createPinia();
  setActivePinia(pinia);
  useSettingsStore().setSettings(structuredClone(stored));
  const w = mount(SettingsPanel, { global: { plugins: [pinia] } });
  await flushPromises(); // 等 onMounted 里的资源回填
  await openTab(w, tab);
  return w;
}

describe("SettingsPanel：标签栏分页", () => {
  it("四个标签都在（引擎页点名 Tectonic），且第一页是「外观」", async () => {
    const w = await mountPanel(null, "外观");
    expect(w.findAll(".tab-btn").map((b) => b.text())).toEqual([
      "外观",
      "编译",
      "引擎（Tectonic）",
      "项目",
    ]);
    expect(w.find(".tab-btn.on").text()).toBe("外观");
    // 分页生效：别的页的字段**不在 DOM 里**（不是藏起来）
    expect(w.find("#set-theme").exists()).toBe(true);
    expect(w.find("#set-bundle-source").exists()).toBe(false);
    expect(w.find("#set-root").exists()).toBe(false);
    // 引擎页不再有"存了但不生效"的提示点（ADR-0015：没有别的引擎可选了）
    expect(w.find(".tab-dot").exists()).toBe(false);
    w.unmount();
  });

  it("每页只渲染自己那组字段", async () => {
    const cases: [string, string, string][] = [
      ["编译", "#set-mode", "#set-bundle-source"],
      ["引擎（Tectonic）", "#set-bundle-source", "#set-mode"],
      ["项目", "#set-root", "#set-bundle-source"],
    ];
    for (const [tab, present, absent] of cases) {
      const w = await mountPanel(null, tab);
      expect(w.find(present).exists(), `${tab} 页应有 ${present}`).toBe(true);
      expect(w.find(absent).exists(), `${tab} 页不该有 ${absent}`).toBe(false);
      w.unmount();
    }
  });

  it("方向键切页（tablist 的键盘约定）", async () => {
    const w = await mountPanel(null, "外观");
    await w.find(".settings-tabs").trigger("keydown", { key: "ArrowRight" });
    expect(w.find(".tab-btn.on").text()).toBe("编译");
    await w.find(".settings-tabs").trigger("keydown", { key: "ArrowRight" });
    expect(w.find(".tab-btn.on").text()).toBe("引擎（Tectonic）");
    await w.find(".settings-tabs").trigger("keydown", { key: "ArrowLeft" });
    expect(w.find(".tab-btn.on").text()).toBe("编译");
    await w.find(".settings-tabs").trigger("keydown", { key: "End" });
    expect(w.find(".tab-btn.on").text()).toBe("项目");
    w.unmount();
  });

  it("记住上次看的那页（关掉重开不必再点一次）", async () => {
    const first = await mountPanel(null, "项目");
    expect(first.find(".tab-btn.on").text()).toBe("项目");
    first.unmount(); // 面板是 v-if 挂载的：关掉即卸载
    const second = await mountPanel(null, "项目");
    expect(second.find(".tab-btn.on").text()).toBe("项目");
    second.unmount();
  });
});

// ADR-0015：引擎只剩 Tectonic ⇒ 页签里没有"选谁"，也没有形态开关，只剩 bundle / cache 两项资源。
describe("SettingsPanel：引擎页（唯一引擎，ADR-0015）", () => {
  it("没有引擎下拉、没有引擎清单文案", async () => {
    const w = await mountPanel();
    expect(w.find("#set-engine").exists()).toBe(false);
    expect(w.find("select#set-engine").exists()).toBe(false);
    expect(w.text()).not.toContain("TeX 引擎");
    expect(w.text()).not.toContain("（不可用）");
    w.unmount();
  });

  it("页标题点名 Tectonic，并说明引擎固定为内嵌（不再暗示可选）", async () => {
    const w = await mountPanel();
    expect(w.find("#tabpanel-engine .sec-title").text()).toBe("编译引擎（Tectonic）");
    expect(w.text()).toContain("引擎固定为");
    expect(w.text()).toContain("（内嵌）");
    w.unmount();
  });

  it("两项资源都在：宏包集 / 缓存目录（形态设置已随 v3 删除）", async () => {
    for (const bundle of [null, "file:///E:/bundles/tex"] as const) {
      const w = await mountPanel(bundle);
      // 形态开关已删除：不存在"驱动形态"按钮组
      expect(w.find("#set-form").exists()).toBe(false);
      expect(w.find("#set-bundle-source").exists()).toBe(true);
      expect(w.find("#set-bundle-path").exists()).toBe(!!bundle);
      expect(w.find("#set-cache").exists()).toBe(true);
      w.unmount();
    }
  });

  it("「恢复默认」不再写 engine 键（字段已从设置里删除）", async () => {
    const w = await mountPanel();
    const reset = w.findAll("button").find((b) => b.text() === "恢复默认")!;
    await reset.trigger("click");
    await flushPromises();
    expect(lastPatch, "恢复默认应发过一次 patch").not.toBeNull();
    expect(Object.keys(lastPatch!)).not.toContain("engine");
    expect(lastPatch).toMatchObject({ mode: "continuous", debounce_ms: 500, timeout_secs: 120 });
    w.unmount();
  });

  it("Tectonic 段只在自己的页里（切到别的页不该带走它）", async () => {
    const w = await mountPanel(null, "引擎（Tectonic）");
    expect(w.find("#set-bundle-source").exists()).toBe(true);
    await openTab(w, "编译");
    expect(w.find("#set-bundle-source").exists()).toBe(false);
    expect(w.find("#set-mode").exists()).toBe(true);
    w.unmount();
  });
});

describe("SettingsPanel：bundle 路径的展示形态", () => {
  it("磁盘上的 file:/// URL 显示成普通 Windows 路径", async () => {
    const w = await mountPanel("file:///E:/Works/tex-presso/test_file/tectonic-bundle");
    const text = w.text();
    expect(text).toContain("当前：E:/Works/tex-presso/test_file/tectonic-bundle");
    expect(text).not.toContain("file:///");
    w.unmount();
  });

  it("POSIX 形态的 file:/// 只剥掉前缀、保留根斜杠", async () => {
    const w = await mountPanel("file:///home/u/bundle");
    expect(w.text()).toContain("当前：/home/u/bundle");
    w.unmount();
  });

  it("相对路径（上游认的第二种给法）原样显示", async () => {
    const w = await mountPanel("bundles/local");
    expect(w.text()).toContain("当前：bundles/local");
    w.unmount();
  });

  it("输入框回填的也是无前缀形态（与「当前：」一致）", async () => {
    // 输入框要在 bundle 区可见时才有值：先选「本地目录」
    const w = await mountPanel("file:///E:/Works/bundle");
    const localBtn = w.findAll("#set-bundle-source .seg-btn")[1];
    await localBtn.trigger("click");
    await flushPromises();
    const input = w.find<HTMLInputElement>("#set-bundle-path");
    expect(input.exists()).toBe(true);
    expect(input.element.value).toBe("E:/Works/bundle");
    w.unmount();
  });
});
