import { describe, it, expect } from "vitest";
import { fuzzyMatch, fuzzyScore } from "../fuzzy";

// 搜索栏（roadmap ㊾）的匹配口径：子串优先、子序列兜底。这几条都是"用户会怎么打"的样本。
describe("fuzzy：文件树搜索的匹配口径", () => {
  it("空查询匹配一切（搜索栏清空 = 回到完整树）", () => {
    expect(fuzzyMatch("chapters/intro.tex", "")).toBe(true);
    expect(fuzzyMatch("chapters/intro.tex", "   ")).toBe(true);
  });

  it("子串命中，忽略大小写", () => {
    expect(fuzzyMatch("chapters/intro.tex", "chap")).toBe(true);
    expect(fuzzyMatch("chapters/Intro.tex", "intro")).toBe(true);
    expect(fuzzyMatch("main.tex", "MAIN")).toBe(true);
    expect(fuzzyMatch("main.tex", "xyz")).toBe(false);
  });

  it("子序列也算命中（模糊的本意）", () => {
    expect(fuzzyMatch("chapters/intro.tex", "cit")).toBe(true); // c-hapters/i-ntro(.t-ex)
    expect(fuzzyMatch("main.tex", "mtx")).toBe(true);
    expect(fuzzyMatch("main.tex", "xmt")).toBe(false); // 顺序反了就不是子序列
  });

  it("文件名上的命中优先于路径上的命中", () => {
    const inName = fuzzyScore("chapters/intro.tex", "intro")!;
    const inDir = fuzzyScore("chapters/intro.tex", "chapters")!;
    expect(inName).toBeGreaterThan(inDir);
  });

  it("子串优先于子序列（同一份候选里更直白的排前面）", () => {
    const substr = fuzzyScore("notes.tex", "nt")!; // n-o-t-e-s：nt 是子序列
    const exact = fuzzyScore("nt.tex", "nt")!; // 子串
    expect(exact).toBeGreaterThan(substr);
  });

  it("全不命中返回 null（不是 0 —— 0 会被当成匹配上了）", () => {
    expect(fuzzyScore("main.tex", "zzz")).toBeNull();
  });
});
