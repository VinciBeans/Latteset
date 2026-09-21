// SettingsPanel 单测：锁三条**只在界面上成立**的契约——
// ① 设置分页（roadmap：标签栏把外观/编译/引擎/项目分开，别混成一长条）；
// ② Tectonic 形态段只出现在**引擎页**、且引擎真的选了 Tectonic 时（引擎不是它时摆着只会让人以为"改了没生效"）；
// ③ bundle 在磁盘上存的是上游认的 `file:///…`，界面上要显示成普通目录路径（用户不该看 URL 前缀）。
// 路径的**归一化**（`E:\…` → `file:///E:/…`）在 core，由 `latteset-core` 的单测覆盖，不在这里重复。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import SettingsPanel from "../SettingsPanel.vue";
import { useSettingsStore } from "../../stores/settings";
import { __resetEngineCatalogForTest } from "../../services/engines";
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
// `listEngines`（roadmap ㊻）必须给：**引擎选项现在由后端清单渲染**，桩里缺它 ⇒ 面板退化成
// "只有当前引擎那一个选项" ⇒ 连 `setValue("tectonic")` 都选不动（CI 就是这么红过一次的）。
vi.mock("../../services/ipc", () => ({
  ipc: {
    getSettings: vi.fn(async () => structuredClone(stored)),
    libFormAvailable: vi.fn(async () => false),
    listEngines: vi.fn(async () =>
      (catalogOverride ?? ENGINE_CATALOG).map((e) => ({
        ...e,
        available: engineUnavailable !== e.id,
        reason: engineUnavailable === e.id ? `桩：本机没有 ${e.label}` : null,
      }))
    ),
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

/** 后端清单的桩（顺序与后端 `ENGINE_ORDER` 一致：Tectonic 第一）。 */
const ENGINE_CATALOG = [
  { id: "tectonic", label: "Tectonic", hint: "免装 TeX Live" },
  { id: "xelatex", label: "XeLaTeX", hint: "需要 TeX Live" },
  { id: "lualatex", label: "LuaLaTeX", hint: "Lua 脚本" },
  { id: "pdflatex", label: "pdfLaTeX", hint: "传统引擎" },
] as const;

/** 把某个引擎标成不可用（`null` = 都可用）；`catalogOverride` 可整套替换清单（模拟 env 收窄）。 */
let engineUnavailable: string | null = null;
let catalogOverride: { id: string; label: string; hint: string }[] | null = null;

// 清单在 `services/engines.ts` 里按会话缓存 ⇒ 每个用例前清一次，否则第一个用例的清单会串到后面
beforeEach(() => {
  engineUnavailable = null;
  catalogOverride = null;
  __resetEngineCatalogForTest();
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
async function mountPanel(
  engine: Settings["compile"]["engine"],
  bundle: string | null = null,
  libForm = false,
  tab = "引擎",
) {
  stored = freshSettings(engine, bundle, libForm);
  const pinia = createPinia();
  setActivePinia(pinia);
  useSettingsStore().setSettings(structuredClone(stored));
  const w = mount(SettingsPanel, { global: { plugins: [pinia] } });
  await flushPromises(); // 等 onMounted 里的 libFormAvailable
  await openTab(w, tab);
  return w;
}

describe("SettingsPanel：标签栏分页", () => {
  it("四个标签都在，且第一页是「外观」", async () => {
    const w = await mountPanel("xelatex", null, false, "外观");
    expect(w.findAll(".tab-btn").map((b) => b.text())).toEqual(["外观", "编译", "引擎", "项目"]);
    expect(w.find(".tab-btn.on").text()).toBe("外观");
    // 分页生效：别的页的字段**不在 DOM 里**（不是藏起来）
    expect(w.find("#set-theme").exists()).toBe(true);
    expect(w.find("#set-engine").exists()).toBe(false);
    expect(w.find("#set-root").exists()).toBe(false);
    w.unmount();
  });

  it("每页只渲染自己那组字段", async () => {
    const cases: [string, string, string][] = [
      ["编译", "#set-mode", "#set-engine"],
      ["引擎", "#set-engine", "#set-mode"],
      ["项目", "#set-root", "#set-engine"],
    ];
    for (const [tab, present, absent] of cases) {
      const w = await mountPanel("tectonic", null, false, tab);
      expect(w.find(present).exists(), `${tab} 页应有 ${present}`).toBe(true);
      expect(w.find(absent).exists(), `${tab} 页不该有 ${absent}`).toBe(false);
      w.unmount();
    }
  });

  it("方向键切页（tablist 的键盘约定）", async () => {
    const w = await mountPanel("xelatex", null, false, "外观");
    await w.find(".settings-tabs").trigger("keydown", { key: "ArrowRight" });
    expect(w.find(".tab-btn.on").text()).toBe("编译");
    await w.find(".settings-tabs").trigger("keydown", { key: "ArrowRight" });
    expect(w.find(".tab-btn.on").text()).toBe("引擎");
    await w.find(".settings-tabs").trigger("keydown", { key: "ArrowLeft" });
    expect(w.find(".tab-btn.on").text()).toBe("编译");
    await w.find(".settings-tabs").trigger("keydown", { key: "End" });
    expect(w.find(".tab-btn.on").text()).toBe("项目");
    w.unmount();
  });

  it("记住上次看的那页（关掉重开不必再点一次）", async () => {
    const first = await mountPanel("xelatex", null, false, "项目");
    expect(first.find(".tab-btn.on").text()).toBe("项目");
    first.unmount(); // 面板是 v-if 挂载的：关掉即卸载
    const second = await mountPanel("xelatex", null, false, "项目");
    expect(second.find(".tab-btn.on").text()).toBe("项目");
    second.unmount();
  });

  it("引擎页有「存了但不生效」的设置时，标签上有点（跨页唯一的线索）", async () => {
    const w = await mountPanel("xelatex", null, true, "外观"); // 存过库内嵌、引擎是 XeLaTeX
    const engineTab = w.findAll(".tab-btn").find((b) => b.text().startsWith("引擎"))!;
    expect(engineTab.find(".tab-dot").exists()).toBe(true);
    const appearanceTab = w.findAll(".tab-btn").find((b) => b.text().startsWith("外观"))!;
    expect(appearanceTab.find(".tab-dot").exists()).toBe(false);
    w.unmount();
  });
});

// 引擎清单（roadmap ㊻）：**选项由后端给**（顺序 / 名字 / 可用性），前端不许自己写一份。
// 这三条都是真机上出过的现场的回归锁 —— 尤其最后一条：清单里没有当前引擎时下拉会**显示空白**。
describe("SettingsPanel：引擎清单来自后端", () => {
  const options = (w: ReturnType<typeof mount>) =>
    w.findAll<HTMLOptionElement>("#set-engine option").map((o) => ({
      text: o.text().trim(),
      value: (o.element as HTMLOptionElement).value,
      disabled: (o.element as HTMLOptionElement).disabled,
    }));

  it("选项 = 后端清单，顺序照给（Tectonic 第一）", async () => {
    const w = await mountPanel("tectonic");
    expect(options(w).map((o) => o.value)).toEqual(["tectonic", "xelatex", "lualatex", "pdflatex"]);
    // 名字只放名字，不带说明（原生下拉的弹层按最长选项撑宽，塞说明会冲出面板）
    expect(options(w).map((o) => o.text)).toEqual(["Tectonic", "XeLaTeX", "LuaLaTeX", "pdfLaTeX"]);
    w.unmount();
  });

  it("不可用的引擎禁用并给出原因（不是悄悄藏掉）", async () => {
    engineUnavailable = "xelatex";
    const w = await mountPanel("tectonic");
    const xe = options(w).find((o) => o.value === "xelatex")!;
    expect(xe.disabled, "不可用要禁用").toBe(true);
    expect(xe.text).toContain("不可用");
    // 当前选中的是可用的 Tectonic ⇒ 说明行是它的说明，不是那条原因
    expect(w.find(".field-hint.err").exists()).toBe(false);
    w.unmount();
  });

  it("当前引擎不可用 ⇒ 说明行改为原因（带 err 样式）", async () => {
    engineUnavailable = "tectonic";
    const w = await mountPanel("tectonic");
    expect(w.find(".field-hint.err").text()).toContain("本机没有 Tectonic");
    w.unmount();
  });

  it("当前引擎不在清单里（被 LATTESET_TEX_ENGINES 收窄）⇒ 补只读项，下拉不空白", async () => {
    catalogOverride = [{ id: "pdflatex", label: "pdfLaTeX", hint: "传统引擎" }];
    const w = await mountPanel("xelatex");
    const opts = options(w);
    expect(opts.map((o) => o.value)).toEqual(["xelatex", "pdflatex"]);
    const current = opts[0];
    expect(current.disabled, "补出来的那条不可选").toBe(true);
    expect(current.text).toContain("不可用");
    // 关键：`<select>` 的值必须是 xelatex（没有它浏览器会显示空白，用户看不出现在用的是什么）
    expect((w.find<HTMLSelectElement>("#set-engine").element as HTMLSelectElement).value).toBe("xelatex");
    expect(w.find(".field-hint.err").text()).toContain("不在本次构建开放的清单里");
    w.unmount();
  });
});

describe("SettingsPanel：Tectonic 段的显示条件（引擎页内）", () => {
  it("引擎不是 Tectonic 时，整段不渲染", async () => {
    for (const engine of ["xelatex", "lualatex", "pdflatex"] as const) {
      const w = await mountPanel(engine);
      // 先确认面板真的渲染出来了，否则"找不到"是无意义的（空面板也能过）
      expect(w.find("#set-engine").exists(), `${engine}：面板没渲染`).toBe(true);
      expect(w.text()).toContain("引擎");
      expect(w.find("#set-form").exists(), `${engine} 下不该有驱动形态`).toBe(false);
      expect(w.find("#set-bundle-source").exists()).toBe(false);
      expect(w.find("#set-cache").exists()).toBe(false);
      expect(w.text()).not.toContain("Tectonic 形态与资源");
      w.unmount();
    }
  });

  it("引擎是 Tectonic 时，三项都在", async () => {
    const w = await mountPanel("tectonic");
    expect(w.text()).toContain("Tectonic 形态与资源");
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

  it("Tectonic 段只在自己的页里（切到别的页不该带走它）", async () => {
    const w = await mountPanel("tectonic", null, false, "引擎");
    expect(w.find("#set-form").exists()).toBe(true);
    await openTab(w, "编译");
    expect(w.find("#set-form").exists()).toBe(false);
    expect(w.find("#set-mode").exists()).toBe(true);
    w.unmount();
  });
});

// 段被隐藏 + 设置仍然存着 = 隐形开关：设置面必须在引擎页把它讲出来。
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
    // 输入框要在 bundle 区可见时才有值：先选「本地目录」
    const w = await mountPanel("tectonic", "file:///E:/Works/bundle");
    const localBtn = w.findAll("#set-bundle-source .seg-btn")[1];
    await localBtn.trigger("click");
    await flushPromises();
    const input = w.find<HTMLInputElement>("#set-bundle-path");
    expect(input.exists()).toBe(true);
    expect(input.element.value).toBe("E:/Works/bundle");
    w.unmount();
  });
});
