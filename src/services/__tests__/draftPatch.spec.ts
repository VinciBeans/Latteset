import { describe, expect, it } from "vitest";
import {
  draftGeometry,
  findAnchor,
  firstChangedLine,
  linesOfPage,
  normalizeForMatch,
  type PdfLine,
} from "../draftPatch";

describe("draftPatch（草案补丁的纯逻辑）", () => {
  it("linesOfPage：同基线的条目聚成一行并按 x 排序拼接", () => {
    const lines = linesOfPage(3, [
      { str: "世界", transform: [1, 0, 0, 1, 60, 700], width: 24, height: 12 },
      { str: "你好", transform: [1, 0, 0, 1, 12, 700], width: 24, height: 12 },
      { str: "第二行", transform: [1, 0, 0, 1, 12, 680], width: 36, height: 12 },
    ]);
    expect(lines).toHaveLength(2);
    const first = lines.find((l) => l.y === 700)!;
    expect(first.text).toBe("你好世界");
    expect(first.x).toBe(12);
    expect(first.w).toBe(72);
    expect(first.size).toBe(12);
    expect(first.page).toBe(3);
  });

  it("linesOfPage：空白条目被忽略；无有效条目返回空", () => {
    expect(linesOfPage(1, [{ str: "   ", transform: [1, 0, 0, 1, 0, 0], width: 5, height: 10 }])).toEqual([]);
  });

  it("findAnchor：整行相等优先，其次包含，再次长前缀；太短/为空不匹配", () => {
    const lines: PdfLine[] = [
      { page: 1, x: 10, y: 700, w: 50, size: 12, text: "文档章节实验讨论" },
      { page: 2, x: 10, y: 500, w: 50, size: 12, text: "文档章节实验讨论排版分析方法" },
    ];
    expect(findAnchor(lines, "文档章节实验讨论排版分析方法")!.page).toBe(2); // 整行相等
    expect(findAnchor(lines, "实验讨论")!.page).toBe(1); // 包含（同分取先出现）
    expect(findAnchor(lines, "文档章节实验")!.page).toBe(1); // 长前缀（≥4）
    expect(findAnchor(lines, "文档")).toBeNull(); // 太短
    expect(findAnchor(lines, "")).toBeNull();
    expect(findAnchor(lines, "完全不同的另一段文字")).toBeNull();
  });

  it("firstChangedLine：给出第一处改动的旧/新行；无改动返回 null", () => {
    const oldT = "a\nb\nc";
    expect(firstChangedLine(oldT, "a\nB\nc")).toEqual({ lineNo: 2, oldLine: "b", newLine: "B" });
    expect(firstChangedLine(oldT, oldT)).toBeNull();
    // 追加一行：旧行为空 ⇒ 调用方无法锚定（v1 限制，静默不画）
    expect(firstChangedLine(oldT, "a\nb\nc\nd")).toEqual({ lineNo: 4, oldLine: "", newLine: "d" });
  });

  it("draftGeometry：盖住宽度取较大者（防旧字露出），溢出比可判定", () => {
    const anchor: PdfLine = { page: 1, x: 0, y: 0, w: 100, size: 10, text: "x" };
    const g1 = draftGeometry(anchor, "短", 10);
    expect(g1.coverW).toBeGreaterThanOrEqual(100);
    expect(g1.overflow).toBeLessThan(1);
    expect(g1.size).toBe(10);
    const g2 = draftGeometry(anchor, "很长的".repeat(20), 10);
    expect(g2.overflow).toBeGreaterThan(1);
  });

  it("normalizeForMatch：去空白与花括号", () => {
    expect(normalizeForMatch("\\section{标题} 与 空格")).toBe("\\section标题与空格");
  });
});
