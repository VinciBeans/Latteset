// SettingsPanel 单测：锁两条**只在界面上成立**的契约——
// ① Tectonic 形态段只在引擎真的选了 Tectonic 时出现（引擎不是它时摆着只会让人以为"改了没生效"）；
// ② bundle 在磁盘上存的是上游认的 `file:///…`，界面上要显示成普通目录路径（用户不该看 URL 前缀）。
// 路径的**归一化**（`E:\…` → `file:///E:/…`）在 core，由 `latteset-core` 的单测覆盖，不在这里重复。
import { describe, it, expect, vi } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import SettingsPanel from "../SettingsPanel.vue";
import { useSettingsStore } from "../../stores/settings";
import type { Settings } from "../../bindings";

/** 后端 settings.json 的当前值（mock 自己的状态；每个用例前重置）。
 *  `tectonic` 在绑定里是可选字段（后端 `#[serde(default)]` 为兼容旧 settings.json），
 *  但这里是我们自己造的完整对象 ⇒ 收紧成必填，免得每处都写 `?.`。 */
type PanelSettings = Omit<Settings, "tectonic"> & { tectonic: NonNullable<Settings["tectonic"]> };
let stored: PanelSettings;

function freshSettings(
  engine: Settings["compile"]["engine"],
  bundle: string | null,
  libForm = false,
): PanelSettings {
  return {
    schema_version: 1,
    compile: { mode: "continuous", debounce_ms: 500, timeout_secs: 120, engine },
    tectonic: { lib_form: libForm, bundle, cache_dir: null },
    root_file: null,
  };
}

// 桩，不是后端复刻：只把本文件用到的键写回（engine / bundle），其余原样返回。
vi.mock("../../services/ipc", () => ({
  ipc: {
    getSettings: vi.fn(async () => structuredClone(stored)),
    libFormAvailable: vi.fn(async () => false),
    updateSettings: vi.fn(async (patch: Record<string, unknown>) => {
      if (patch.engine !== undefined) {
        stored.compile.engine = patch.engine as Settings["compile"]["engine"];
      }
      if (patch.bundle !== undefined) {
        stored.tectonic.bundle = patch.bundle === "" ? null : (patch.bundle as string);
      }
      return structuredClone(stored);
    }),
  },
}));

/** 面板假定 store 已由 App 初始化过 ⇒ 这里直接把设置塞进 store，再挂载。 */
async function mountPanel(
  engine: Settings["compile"]["engine"],
  bundle: string | null = null,
  libForm = false,
) {
  stored = freshSettings(engine, bundle, libForm);
  const pinia = createPinia();
  setActivePinia(pinia);
  useSettingsStore().setSettings(structuredClone(stored));
  const w = mount(SettingsPanel, { global: { plugins: [pinia] } });
  await flushPromises(); // 等 onMounted 里的 libFormAvailable
  return w;
}

describe("SettingsPanel：Tectonic 段的显示条件", () => {
  it("引擎不是 Tectonic 时，整段不渲染", async () => {
    for (const engine of ["xelatex", "lualatex", "pdflatex"] as const) {
      const w = await mountPanel(engine);
      // 先确认面板真的渲染出来了，否则"找不到"是无意义的（空面板也能过）
      expect(w.find("#set-engine").exists(), `${engine}：面板没渲染`).toBe(true);
      expect(w.text()).toContain("编译");
      expect(w.find("#set-form").exists(), `${engine} 下不该有驱动形态`).toBe(false);
      expect(w.find("#set-bundle-source").exists()).toBe(false);
      expect(w.find("#set-cache").exists()).toBe(false);
      expect(w.text()).not.toContain("Tectonic 引擎形态");
      w.unmount();
    }
  });

  it("引擎是 Tectonic 时，三项都在", async () => {
    const w = await mountPanel("tectonic");
    expect(w.text()).toContain("Tectonic 引擎形态");
    expect(w.find("#set-form").exists()).toBe(true);
    expect(w.find("#set-bundle-source").exists()).toBe(true);
    expect(w.find("#set-cache").exists()).toBe(true);
    w.unmount();
  });

  it("运行时把引擎切到 Tectonic，段随之出现（不必重开面板）", async () => {
    const w = await mountPanel("xelatex");
    expect(w.find("#set-form").exists()).toBe(false);
    await w.find("#set-engine").setValue("tectonic");
    await flushPromises();
    expect(w.find("#set-form").exists()).toBe(true);
    w.unmount();
  });
});

// 段被隐藏 + 设置仍然存着 = 隐形开关：设置面必须在引擎那一栏把它讲出来。
describe("SettingsPanel：引擎不是 Tectonic 时的「存了但不生效」提示", () => {
  const notice = (w: ReturnType<typeof mount>) => w.find(".field-hint.warn");

  it("存过库内嵌而引擎是 XeLaTeX ⇒ 提示出现并点名是哪几项", async () => {
    const w = await mountPanel("xelatex", null, true);
    expect(notice(w).exists()).toBe(true);
    expect(notice(w).text()).toContain("库内嵌");
    expect(notice(w).text()).toContain("仅在选择 Tectonic 引擎时生效");
    w.unmount();
  });

  it("宏包集/缓存目录也算「存过」", async () => {
    const w = await mountPanel("pdflatex", "file:///E:/bundles/tex");
    expect(notice(w).exists()).toBe(true);
    expect(notice(w).text()).toContain("宏包集");
    w.unmount();
  });

  it("什么都没存过 ⇒ 不打扰（不出现提示）", async () => {
    const w = await mountPanel("xelatex");
    expect(notice(w).exists()).toBe(false);
    w.unmount();
  });

  it("引擎就是 Tectonic ⇒ 设置本来就可见，不该再提示", async () => {
    const w = await mountPanel("tectonic", null, true);
    expect(notice(w).exists()).toBe(false);
    w.unmount();
  });
});

describe("SettingsPanel：bundle 路径的展示形态", () => {
  it("磁盘上的 file:/// URL 显示成普通 Windows 路径", async () => {
    const w = await mountPanel("tectonic", "file:///E:/Works/tex-presso/test_file/tectonic-bundle");
    const text = w.text();
    expect(text).toContain("当前：E:/Works/tex-presso/test_file/tectonic-bundle");
    expect(text).not.toContain("file:///");
    w.unmount();
  });

  it("POSIX 形态的 file:/// 只剥掉前缀、保留根斜杠", async () => {
    const w = await mountPanel("tectonic", "file:///home/u/bundle");
    expect(w.text()).toContain("当前：/home/u/bundle");
    w.unmount();
  });

  it("相对路径（上游认的第二种给法）原样显示", async () => {
    const w = await mountPanel("tectonic", "bundles/local");
    expect(w.text()).toContain("当前：bundles/local");
    w.unmount();
  });

  it("输入框回填的也是无前缀形态（与「当前：」一致）", async () => {
    const w = await mountPanel("tectonic", "file:///E:/Works/bundle");
    const input = w.find<HTMLInputElement>("#set-bundle-path");
    expect(input.exists()).toBe(true);
    expect(input.element.value).toBe("E:/Works/bundle");
    w.unmount();
  });
});
