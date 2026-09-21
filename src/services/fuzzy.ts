// 文件树的模糊匹配（roadmap ㊾）：纯函数、无依赖 ⇒ 可单测。
//
// 两条口径（都做，谁命中算谁）：
// 1. **子串**（忽略大小写）——最常见、最符合直觉（打 `chap` 命中 `chapters/intro.tex`）；
// 2. **子序列**（按顺序出现，可跳过字符）——这才是"模糊"（打 `cit` 命中 `chapters/intro.tex`）。
//
// 刻意**不做**拼音/编辑距离：那要带字典或打分表，而这里的规模是"一个项目几百个文件"，
// 排序用"命中位置越靠前越优先"就够了 —— 别为一个输入框引入一套匹配引擎。

/** 子串命中（忽略大小写）；返回命中位置，未命中返回 -1。 */
export function substringScore(haystack: string, needle: string): number {
  if (!needle) return -1;
  return haystack.toLowerCase().indexOf(needle.toLowerCase());
}

/** 子序列命中（按顺序、可跳过）；返回"跨度"（越小越紧凑），未命中返回 -1。 */
export function subsequenceScore(haystack: string, needle: string): number {
  if (!needle) return -1;
  const h = haystack.toLowerCase();
  const n = needle.toLowerCase();
  let i = 0;
  let first = -1;
  let last = -1;
  for (let k = 0; k < h.length && i < n.length; k++) {
    if (h[k] === n[i]) {
      if (first < 0) first = k;
      last = k;
      i++;
    }
  }
  if (i < n.length) return -1; // 没走完 ⇒ 不是子序列
  return last - first + 1;
}

/**
 * 匹配打分：**越大越优先**，`null` = 不匹配。
 *
 * 排序意图：文件名上的子串 > 路径上的子串 > 紧凑的子序列；同档按命中位置靠前优先。
 */
export function fuzzyScore(relPath: string, query: string): number | null {
  const q = query.trim();
  if (!q) return 0;
  const name = relPath.slice(relPath.lastIndexOf("/") + 1);

  const inName = substringScore(name, q);
  if (inName >= 0) return 3000 - inName;

  const inPath = substringScore(relPath, q);
  if (inPath >= 0) return 2000 - inPath;

  const span = subsequenceScore(name, q);
  if (span >= 0) return 1000 - span;

  const pathSpan = subsequenceScore(relPath, q);
  if (pathSpan >= 0) return 500 - pathSpan;

  return null;
}

/** 是否匹配（`fuzzyScore` 的布尔包装）。 */
export function fuzzyMatch(relPath: string, query: string): boolean {
  return fuzzyScore(relPath, query) !== null;
}
