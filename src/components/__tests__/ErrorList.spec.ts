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
      cause: "编译在 120s 内没有跑完，但日志显示已经排版到第 37 页——是在推进，只是比上限慢",
      hint: `把编译超时提高到 ${suggested}s 后重试`,
      suggested_timeout_secs: suggested,
    },
  };
}

describe("ErrorList（超时一键重试，roadmap ㉕）", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    const settings = useSettingsStore();
    settings.settings = {
      schema_version: 1,
      compile: { mode: "continuous", debounce_ms: 500, timeout_secs: 120, engine: "xelatex" },
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

  it("非超时诊断不带 suggested 字段时不渲染按钮", async () => {
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
          hint: "执行 tlmgr install nope 安装",
          suggested_timeout_secs: null,
        },
      },
    ]);
    const { el } = mount();
    await nextTick();
    expect(el.querySelector(".retry-btn")).toBeNull();
    expect(el.querySelector(".entry")).toBeTruthy();
  });
});
