// 错误分组 / 截断口径（A-18）：状态栏折叠头「N 个问题」与错误列表必须用同一份结果。
import { describe, it, expect } from "vitest";
import { groupErrors, MAX_DISPLAY } from "../errorGroups";
import type { ErrorEntry } from "../../bindings";

function entry(over: Partial<ErrorEntry> = {}): ErrorEntry {
  return {
    message: "! Undefined control sequence.",
    file: "main.tex",
    line: 3,
    kind: "content_error",
    diagnosis: null,
    ...over,
  };
}

describe("errorGroups（A-18：计数与列表同源）", () => {
  it("同源错误（文件 + 展示文本相同）合并为一组并累计 count", () => {
    const g = groupErrors([entry(), entry(), entry({ file: "other.tex" })]);
    expect(g.total).toBe(2);
    expect(g.items).toHaveLength(2);
    expect(g.items[0].count).toBe(2);
    expect(g.hidden).toBe(0);
  });

  it("超过去重展示上限：items 截断到 MAX_DISPLAY，hidden 报差额，total 仍是去重总数", () => {
    const many = Array.from({ length: MAX_DISPLAY + 5 }, (_, i) =>
      entry({ message: `err-${i}`, line: i }),
    );
    const g = groupErrors(many);
    expect(g.items).toHaveLength(MAX_DISPLAY);
    expect(g.hidden).toBe(5);
    expect(g.total).toBe(MAX_DISPLAY + 5); // 折叠头/列表头部显示这个数，不是原始条数
  });

  it("有诊断时用诊断 cause 作分组键（不同 cause 不误合并）", () => {
    const withCause = (cause: string) =>
      entry({
        diagnosis: {
          kind: "compile_timeout",
          cause,
          hint: "h",
          suggested_timeout_secs: null,
          missing_file: null,
        } as never,
      });
    expect(groupErrors([withCause("a"), withCause("a"), withCause("b")]).total).toBe(2);
  });
});
