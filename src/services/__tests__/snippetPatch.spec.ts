// 片段定位单测（草稿层真实排版实验 (A)）：改动行 → 段落。
import { describe, it, expect } from "vitest";
import { firstChangedLine, paragraphAt, snippetForChange } from "../snippetPatch";

describe("firstChangedLine", () => {
  it("行内改动定位到该行", () => {
    expect(firstChangedLine("a\nb\nc", "a\nbX\nc")).toBe(1);
  });
  it("完全相同 → -1", () => {
    expect(firstChangedLine("a\nb", "a\nb")).toBe(-1);
  });
  it("末尾新增行 → 指向新行", () => {
    expect(firstChangedLine("a\nb", "a\nb\nc")).toBe(2);
  });
  it("中间插入行 → 指向插入处", () => {
    expect(firstChangedLine("a\nb", "a\nX\nb")).toBe(1);
  });
});

describe("paragraphAt", () => {
  const DOC = ["\\documentclass{article}", "\\begin{document}", "", "第一段第一行", "第一段第二行", "", "第二段", ""].join("\n");

  it("撑到上下空行边界", () => {
    expect(paragraphAt(DOC, 3)).toBe("第一段第一行\n第一段第二行");
    expect(paragraphAt(DOC, 6)).toBe("第二段");
  });
  it("空白处返回 null", () => {
    expect(paragraphAt(DOC, 2)).toBeNull();
  });
  it("越界返回 null", () => {
    expect(paragraphAt(DOC, 999)).toBeNull();
  });
  it("超长段落按改动行居中截断", () => {
    const long = Array.from({ length: 200 }, (_, i) => `行${i}`).join("\n");
    const got = paragraphAt(long, 100, 10);
    expect(got).not.toBeNull();
    const lines = (got as string).split("\n");
    expect(lines.length).toBe(10);
    expect(lines).toContain("行100");
  });
});

describe("snippetForChange", () => {
  it("只在改动处取段落", () => {
    const prev = "\\documentclass{article}\n\\begin{document}\n\nAAA\n\nBBB\n";
    const next = "\\documentclass{article}\n\\begin{document}\n\nAAA edited\n\nBBB\n";
    expect(snippetForChange(prev, next)).toBe("AAA edited");
  });
  it("无改动 → null", () => {
    expect(snippetForChange("x", "x")).toBeNull();
  });
});
