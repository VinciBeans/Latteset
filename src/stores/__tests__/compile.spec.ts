// compileStore 单测（modules.md §9.2 / design.md 错误列表时机）：phase/kind/errors/hasError/draft 转换。
import { describe, it, expect, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { useCompileStore } from "../compile";
import { compileButtonText } from "../../services/compileStatus";
import type { ErrorEntry } from "../../bindings";

const ERR: ErrorEntry = { message: "x", file: null, line: null, kind: "content_error", diagnosis: null };

describe("compileStore", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("初始为 success 无错误", () => {
    const c = useCompileStore();
    expect(c.phase).toBe("success");
    expect(c.errors).toEqual([]);
    expect(c.hasError).toBe(false);
    expect(c.draft).toBe(false);
  });

  it("running 清空错误与 hasError（编译中清空）", () => {
    const c = useCompileStore();
    c.setErrors([ERR]);
    c.setStatus("running", null, true);
    expect(c.errors).toEqual([]);
    expect(c.hasError).toBe(false);
  });

  it("failed 置 hasError 并记录 kind", () => {
    const c = useCompileStore();
    c.setErrors([ERR]);
    c.setStatus("failed", "content_error", true);
    expect(c.hasError).toBe(true);
    expect(c.kind).toBe("content_error");
  });

  it("success 清空错误：无 running 前置时旧错误不残留（回归：清除失败残留）", () => {
    const c = useCompileStore();
    c.setErrors([ERR]);
    c.setStatus("success", null, false);
    expect(c.hasError).toBe(false);
    expect(c.errors).toEqual([]);
    expect(c.kind).toBe(null);
  });

  // ---- draft（roadmap ㉘）：表示「屏幕上的 PDF 是否为草稿」 ----
  it("success 按事件更新 draft（草稿→收敛后清除）", () => {
    const c = useCompileStore();
    c.setStatus("success", null, true);
    expect(c.draft).toBe(true);
    c.setStatus("success", null, false);
    expect(c.draft).toBe(false);
  });

  it("running/queued 不改 draft：屏幕上的 PDF 还是上一次的", () => {
    const c = useCompileStore();
    c.setStatus("success", null, true);
    c.setStatus("running", null, false); // 新编译在跑，但还没产出新 PDF
    expect(c.draft).toBe(true);
    c.setStatus("queued", null, false);
    expect(c.draft).toBe(true);
  });

  it("failed 不改 draft：失败不产出新 PDF，草稿提示不该被误清", () => {
    const c = useCompileStore();
    c.setStatus("success", null, true);
    c.setStatus("failed", "content_error", false);
    expect(c.draft).toBe(true);
  });

  // ---- pages（阶段 2 · 流式输出）：编译中已排版页数 ----
  it("running 时接受进度，且只增不减（多趟重启不回跳）", () => {
    const c = useCompileStore();
    c.setStatus("running", null, false);
    c.setProgress(3);
    expect(c.pages).toBe(3);
    c.setProgress(1); // 第二趟重新从第 1 页开始 → 忽略
    expect(c.pages).toBe(3);
    c.setProgress(12);
    expect(c.pages).toBe(12);
  });

  it("非 running 忽略进度（终态之后到达的旧事件不污染状态）", () => {
    const c = useCompileStore();
    c.setStatus("success", null, false);
    c.setProgress(42);
    expect(c.pages).toBe(0);
  });

  it("新的编译开始与结束都把页数归零", () => {
    const c = useCompileStore();
    c.setStatus("running", null, false);
    c.setProgress(7);
    c.setStatus("success", null, false);
    expect(c.pages).toBe(0);
    c.setStatus("running", null, false);
    expect(c.pages).toBe(0);
  });

  // ---- 流式错误（阶段 2）：编译中的致命错误，非权威 ----
  it("running 时接受流式错误（编译没结束就能看见）", () => {
    const c = useCompileStore();
    c.setStatus("running", null, false);
    c.setLiveErrors([ERR]);
    expect(c.errors).toEqual([ERR]);
  });

  it("终态之后到达的流式错误被忽略（回归：晚到的中间态不得顶掉权威列表）", () => {
    const c = useCompileStore();
    c.setStatus("running", null, false);
    c.setLiveErrors([ERR]);
    // 编译结束：权威列表（如超时诊断）写入
    const authoritative: ErrorEntry = { ...ERR, message: "超时", kind: "timeout" };
    c.setStatus("failed", "timeout", false);
    c.setErrors([authoritative]);
    // 收尾期补发的实时事件随后抵达 → 必须被丢弃
    c.setLiveErrors([ERR, ERR]);
    expect(c.errors).toEqual([authoritative]);
  });

  it("流式错误不改 hasError：结论只由终态给", () => {
    const c = useCompileStore();
    c.setStatus("running", null, false);
    c.setLiveErrors([ERR]);
    expect(c.hasError).toBe(false);
  });
});

// A-01：`phase` 初值是 success（后端没有 idle 相位），必须另有"编译过没有"的派生标志，
// 否则未编译就显示「就绪」（绿色对勾）——状态栏/空态都靠它区分。
describe("compileStore：hasCompiled（A-01）", () => {
  it("初始为 false；queued/running 仍不算编译过", () => {
    const c = useCompileStore();
    expect(c.hasCompiled).toBe(false);
    c.setStatus("queued", null, false);
    expect(c.hasCompiled).toBe(false);
    c.setStatus("running", null, false);
    expect(c.hasCompiled).toBe(false);
  });

  it("终态 success / failed 都置 true（失败也算编译过）", () => {
    const c = useCompileStore();
    c.setStatus("success", null, false);
    expect(c.hasCompiled).toBe(true);

    const d = useCompileStore();
    d.setStatus("failed", "timeout", false);
    expect(d.hasCompiled).toBe(true);
  });

  it("reset 清空跨项目编译态（B1）：phase 回初值、hasCompiled/errors/draft/pages 归零", () => {
    const c = useCompileStore();
    c.setStatus("success", null, true);
    c.setErrors([ERR]);
    expect(c.hasCompiled).toBe(true);

    c.reset();
    expect(c.phase).toBe("success");
    expect(c.hasCompiled).toBe(false);
    expect(c.errors).toEqual([]);
    expect(c.hasError).toBe(false);
    expect(c.draft).toBe(false);
    expect(c.pages).toBe(0);
  });
});

// A-18（PLAN 原文）：工具栏按钮的进行态文案必须与状态栏 phase 同口径——
// 后端有 `queued` 与 `running` 两种进行态，排队中不能说成"排版中"。
describe("工具栏编译按钮文案（A-18）", () => {
  it("queued → 排队中…；running → 排版中…；其余 → 编译", () => {
    expect(compileButtonText("queued")).toBe("排队中…");
    expect(compileButtonText("running")).toBe("排版中…");
    expect(compileButtonText("success")).toBe("编译");
    expect(compileButtonText("failed")).toBe("编译");
  });
});
