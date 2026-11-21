// ErrorList 单测（roadmap ㉕）：超时条目渲染「提高到 Ns 并重试」并真的改设置 + 重跑。
// 诊断建议值来自后端（core `diagnose_timeout`），前端只负责渲染与执行——这里锁定这条链路，
// 并守卫"非超时条目不得出现按钮"（避免给内容错误也长出一个没有意义的动作）。
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { createApp, h, nextTick } from "vue";
import { setActivePinia, createPinia } from "pinia";
import ErrorList from "../ErrorList.vue";
import { useCompileStore } from "../../stores/compile";
import { useSettingsStore } from "../../stores/settings";
import { ipc } from "../../services/ipc";
import { MAX_DISPLAY } from "../../services/errorGroups";
import type { ErrorEntry } from "../../bindings";

vi.mock("../../services/ipc", () => ({
  ipc: {
    compileNow: vi.fn(async () => null),
    updateSettings: vi.fn(async (patch: unknown) => patch),
    getSettings: vi.fn(async () => null),
  },
}));

/** 在真实组件实例里挂载（store 依赖注入 + onMounted 等钩子正常）。 */
function mount() {
  const el = document.createElement("div");
  const app = createApp({ render: () => h(ErrorList) });
  app.mount(el);
  return { el, app };
}

function timeoutEntry(suggested: number | null): ErrorEntry {
  return {
    message: "编译超过 120s 上限，已强制终止（进程树已杀）。证据：首次编译，11 个 .tex 源文件",
    file: null,
    line: null,
    kind: "timeout",
    diagnosis: {
      kind: "compile_timeout",
      cause: "编译在 120s 内没有跑完，但引擎输出显示已经排版到第 37 页——是在推进，只是比上限慢（首次编译，11 个 .tex 源文件）",
      hint: `把编译超时提高到 ${suggested}s 后重试`,
      suggested_timeout_secs: suggested,
      missing_file: null,
    },
  };
}

describe("ErrorList（超时一键重试，roadmap ㉕）", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    const settings = useSettingsStore();
    settings.settings = {
      schema_version: 1,
      compile: { mode: "continuous", debounce_ms: 500, timeout_secs: 120 },
      root_file: null,
    } as never;
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("超时条目渲染按钮，标签为后端建议值", async () => {
    const compile = useCompileStore();
    compile.setErrors([timeoutEntry(900)]);
    const { el } = mount();
    await nextTick();
    const btn = el.querySelector<HTMLButtonElement>(".retry-btn");
    expect(btn, "超时条目必须有重试按钮").toBeTruthy();
    expect(btn!.textContent).toContain("900");
    // 诊断的两行式展示保持（原因 + 建议）
    expect(el.querySelector(".headline")!.textContent).toContain("在推进");
    expect(el.querySelector(".hint")!.textContent).toContain("900");
  });

  it("点击按钮：先改设置再重跑（顺序不能反，否则后端仍按旧上限编译）", async () => {
    const compile = useCompileStore();
    const settings = useSettingsStore();
    const order: string[] = [];
    vi.mocked(ipc.updateSettings).mockImplementation(async () => {
      order.push("updateSettings");
      return null as never;
    });
    vi.mocked(ipc.compileNow).mockImplementation(async () => {
      order.push("compileNow");
      return null;
    });
    vi.spyOn(settings, "update").mockImplementation(async (patch) => {
      order.push(`update:${JSON.stringify(patch)}`);
      return null as never;
    });

    compile.setErrors([timeoutEntry(900)]);
    const { el } = mount();
    await nextTick();
    el.querySelector<HTMLButtonElement>(".retry-btn")!.click();
    await nextTick();
    await Promise.resolve();

    expect(order).toEqual(['update:{"timeout_secs":900}', "compileNow"]);
    expect(ipc.compileNow).toHaveBeenCalledTimes(1);
  });

  it("无建议值时不渲染按钮（内容错误等条目不受影响）", async () => {
    const compile = useCompileStore();
    compile.setErrors([timeoutEntry(null)]);
    const { el } = mount();
    await nextTick();
    expect(el.querySelector(".retry-btn")).toBeNull();
  });

  it("缺包诊断渲染「切到在线完整宏包集」按钮（T16 补包通道），点击后写在线地址并重编", async () => {
    const order: string[] = [];
    vi.spyOn(ipc, "compileNow").mockImplementation(async () => {
      order.push("compileNow");
      return null as never;
    });
    const settings = useSettingsStore();
    vi.spyOn(settings, "update").mockImplementation(async (patch) => {
      order.push(`update:${JSON.stringify(patch)}`);
      return null as never;
    });
    const compile = useCompileStore();
    compile.setErrors([
      {
        message: "! LaTeX Error: File `nope.sty' not found.",
        file: "main.tex",
        line: 5,
        kind: "content_error",
        diagnosis: {
          kind: "missing_package",
          cause: "缺少宏包文件 nope.sty",
          hint: "在「设置 → 引擎 → 宏包集来源」切到「在线完整宏包集」（需联网）后重新编译",
          suggested_timeout_secs: null,
          missing_file: "nope.sty",
        },
      },
    ]);
    const { el } = mount();
    await nextTick();
    const btn = el.querySelector<HTMLButtonElement>(".retry-btn");
    expect(btn).toBeTruthy();
    expect(btn!.textContent).toContain("在线完整宏包集");
    btn!.click();
    await nextTick();
    await Promise.resolve();

    expect(order[0]).toMatch(/^update:\{"bundle":"https:\/\/[^"]+"\}$/);
    expect(order[1]).toBe("compileNow");
    expect(ipc.compileNow).toHaveBeenCalledTimes(1);
  });

  it("非超时、非缺包诊断不渲染按钮", async () => {
    const compile = useCompileStore();
    compile.setErrors([
      {
        message: "! Undefined control sequence.",
        file: "main.tex",
        line: 3,
        kind: "content_error",
        diagnosis: {
          kind: "undefined_control_sequence",
          cause: "命令 \\foo 未定义",
          hint: "检查拼写",
          suggested_timeout_secs: null,
          missing_file: null,
        },
      },
    ]);
    const { el } = mount();
    await nextTick();
    expect(el.querySelector(".retry-btn")).toBeNull();
    expect(el.querySelector(".entry")).toBeTruthy();
  });
});

// A-02：空态三态（未编译 / 排版中 / 无问题）——此前未编译也显示绿色对勾「没有发现错误」
describe("ErrorList：空态三态（A-02）", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("未编译：中性灰圈 +「还没有编译」", async () => {
    const { el } = mount();
    await nextTick();
    expect(el.querySelector(".empty")!.textContent).toContain("还没有编译");
    const icon = el.querySelector(".empty-icon")!;
    expect(icon.classList.contains("idle")).toBe(true);
    expect(icon.textContent).toBe("○");
  });

  it("排版中：显示「排版中…」", async () => {
    useCompileStore().setStatus("running", null, false);
    const { el } = mount();
    await nextTick();
    expect(el.querySelector(".empty")!.textContent).toContain("排版中…");
  });

  it("编译成功且无错误：绿勾 +「没有发现问题」", async () => {
    useCompileStore().setStatus("success", null, false);
    const { el } = mount();
    await nextTick();
    expect(el.querySelector(".empty")!.textContent).toContain("没有发现问题");
    const icon = el.querySelector(".empty-icon")!;
    expect(icon.classList.contains("idle")).toBe(false);
    expect(icon.textContent).toBe("✓");
  });
});

// A-18：折叠头/头部计数与列表同口径（去重后的组数），不是原始条数
describe("ErrorList：计数口径（A-18）", () => {
  it("同源错误合并后，头部计数 = 组数而非条数", async () => {
    setActivePinia(createPinia());
    const compile = useCompileStore();
    const dup = (): ErrorEntry => ({
      message: "! Undefined control sequence.",
      file: "main.tex",
      line: 3,
      kind: "content_error",
      diagnosis: null,
    });
    compile.setErrors([dup(), dup(), { ...dup(), file: "other.tex" }]);
    const { el } = mount();
    await nextTick();
    expect(el.querySelector(".head-count")!.textContent).toBe("2");
  });

  it("B-08：超上限时「已隐藏 N 条重复问题」+「显示全部」，点开后全部渲染", async () => {
    setActivePinia(createPinia());
    const compile = useCompileStore();
    const many: ErrorEntry[] = Array.from({ length: MAX_DISPLAY + 2 }, (_, i) => ({
      message: `err-${i}`,
      file: "main.tex",
      line: i,
      kind: "content_error",
      diagnosis: null,
    }));
    compile.setErrors(many);
    const { el } = mount();
    await nextTick();
    const more = el.querySelector(".more")!;
    expect(more.textContent).toContain("已隐藏 2 条重复问题");
    // MAX_DISPLAY 本身不进文案（只给"隐藏了多少"）
    expect(more.textContent).not.toContain(String(MAX_DISPLAY));

    (el.querySelector(".show-all") as HTMLButtonElement).click();
    await nextTick();
    expect(el.querySelector(".more")).toBeNull();
    expect(el.querySelectorAll(".entry")).toHaveLength(MAX_DISPLAY + 2);
  });
});
