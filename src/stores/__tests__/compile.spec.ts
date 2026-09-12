// compileStore 单测（modules.md §9.2 / design.md 错误列表时机）：phase/kind/errors/hasError/draft 转换。
import { describe, it, expect, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { useCompileStore } from "../compile";
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
});
