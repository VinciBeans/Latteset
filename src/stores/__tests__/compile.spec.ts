// compileStore 单测（modules.md §9.2 / design.md 错误列表时机）：phase/kind/errors/hasError 转换。
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
  });

  it("running 清空错误与 hasError（编译中清空）", () => {
    const c = useCompileStore();
    c.setErrors([ERR]);
    c.setStatus("running", null);
    expect(c.errors).toEqual([]);
    expect(c.hasError).toBe(false);
  });

  it("failed 置 hasError 并记录 kind", () => {
    const c = useCompileStore();
    c.setErrors([ERR]);
    c.setStatus("failed", "content_error");
    expect(c.hasError).toBe(true);
    expect(c.kind).toBe("content_error");
  });

  it("success 清空错误：无 running 前置时旧错误不残留（回归：清除失败残留）", () => {
    const c = useCompileStore();
    c.setErrors([ERR]);
    c.setStatus("success", null);
    expect(c.hasError).toBe(false);
    expect(c.errors).toEqual([]);
    expect(c.kind).toBe(null);
  });
});
