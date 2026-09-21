import { describe, it, expect } from "vitest";
import { basename, isUnder, normalizePath, relativize, remapUnder, samePath, toSlashes } from "../paths";

// 路径工具（roadmap ㊿）：前端**唯一一份**口径。这些用例是"漏一处就静默错"的那类回归锁
// —— 分隔符、大小写、前缀边界（`a/b` 不能匹配 `a/bc`）。
describe("paths：分隔符归一", () => {
  it("toSlashes 只把反斜杠换成正斜杠", () => {
    expect(toSlashes("E:\\Works\\proj\\main.tex")).toBe("E:/Works/proj/main.tex");
    expect(toSlashes("E:/proj/main.tex")).toBe("E:/proj/main.tex");
  });

  it("normalizePath 折叠重复斜杠、剥 `.`、合并 `..`", () => {
    expect(normalizePath("E:/proj//./a/../main.tex")).toBe("E:/proj/main.tex");
    expect(normalizePath("/home/u/../u/a.tex")).toBe("/home/u/a.tex");
    // 绝对路径下越界的 `..` 丢弃（不产出 `/..`）
    expect(normalizePath("/a/../../b")).toBe("/b");
  });

  it("basename 去掉尾斜杠后取最后一段", () => {
    expect(basename("E:/proj/chapters/")).toBe("chapters");
    expect(basename("E:/proj/main.tex")).toBe("main.tex");
    expect(basename("main.tex")).toBe("main.tex");
  });
});

describe("paths：同一路径与前缀判定（分隔符无关）", () => {
  it("samePath 忽略分隔符与尾斜杠", () => {
    expect(samePath("E:\\p\\main.tex", "E:/p/main.tex")).toBe(true);
    expect(samePath("E:/p/chapters/", "E:/p/chapters")).toBe(true);
    expect(samePath("E:/p/a.tex", "E:/p/b.tex")).toBe(false);
  });

  it("isUnder 含自身与子孙，且不被同前缀的兄弟目录骗过", () => {
    expect(isUnder("E:/p/chapters/intro.tex", "E:/p/chapters")).toBe(true);
    expect(isUnder("E:/p/chapters", "E:\\p\\chapters")).toBe(true);
    // 关键边界：`chapters-backup` 不是 `chapters` 的子项（靠 `/` 分隔判定，不是裸 startsWith）
    expect(isUnder("E:/p/chapters-backup/a.tex", "E:/p/chapters")).toBe(false);
    expect(isUnder("E:/p/main.tex", "E:/p/chapters")).toBe(false);
  });
});

describe("paths：项目内相对路径", () => {
  const root = "E:/Works/proj";

  it("根内给相对路径（正斜杠）", () => {
    expect(relativize("E:/Works/proj/chapters/a.tex", root)).toBe("chapters/a.tex");
    expect(relativize("E:\\Works\\proj\\main.tex", root)).toBe("main.tex");
  });

  it("**不在根内给空串**（而不是原样返回绝对路径）", () => {
    // 这条是刚修掉的隐患：原实现返回绝对路径 ⇒ 被当成相对路径发给 update_settings，
    // 用户看到的是"root_file 指向的文件不存在"，与真实原因（路径越界）无关。
    expect(relativize("E:/other/a.tex", root)).toBe("");
    expect(relativize("E:/Works/proj-backup/a.tex", root)).toBe("");
  });

  it("根自身给空串（它没有「相对」可言）", () => {
    expect(relativize(root, root)).toBe("");
    expect(relativize("", root)).toBe("");
  });
});

describe("paths：改名时的前缀重映射", () => {
  it("命中自身与子孙；未命中原样返回", () => {
    expect(remapUnder("E:/p/a.tex", "E:/p/a.tex", "E:/p/b.tex")).toBe("E:/p/b.tex");
    expect(remapUnder("E:/p/ch/a.tex", "E:/p/ch", "E:/p/ch2")).toBe("E:/p/ch2/a.tex");
    expect(remapUnder("E:/p/chX/a.tex", "E:/p/ch", "E:/p/ch2")).toBe("E:/p/chX/a.tex");
  });

  it("分隔符混写也能重映射（改名收口不能漏标签）", () => {
    expect(remapUnder("E:\\p\\ch\\a.tex", "E:/p/ch", "E:/p/ch2")).toBe("E:/p/ch2/a.tex");
  });
});
