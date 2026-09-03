// texParse.ts：LaTeX 文本行解析辅助（纯函数，不依赖 Monaco/Tauri），
// 供折叠 / 代码片段补全共用，便于单测（modules.md §6、design.md §测试）。
// 注：文档大纲的同类剥离逻辑已下沉 Rust（crates/texpresso-core/src/outline.rs，
// 2026-09-03，cli-mcp-plan.md P1-4），本文件不再服务于大纲。
//
// 背景：Monaco 片段引擎把 `\` 当转义（`\$`/`\}`/`\\`），而折叠/大纲要正确区分
// 「真环境/章节命令」与「注释 / \verb 字面量里的假匹配」，故把这类行级剥离子程序集中于此。

/**
 * 剥离 LaTeX 行内的 `\verb` 跨度与真注释，返回「代码」部分。
 * ① 移除非行内 `\verb*?<delim>…<delim>` 字面量（其内容含 `%`/`{...}` 不应被当作注释/环境/章节）；
 * ② 在「真注释」`%` 处截断。判定（C5）：`%` 前为**连续奇数个反斜杠** → 转义字面 `%`（如 `\%`、`\\\%`），
 *    不作注释；**偶数个（含 0）** → 真注释（如 `\\%`——`\\` 换行+`%` 注释，`%` 才是注释）。
 *    注：JS 正则 lookbehind 不支持变长，故用扫描数反斜杠。
 * 结果用于匹配 `\begin`/`\section` 等结构命令。
 */
export function stripTexComment(line: string): string {
  // \verb 的定界符是紧跟其后的任意非字母、非空白、非反斜杠字符（常为 |）。
  // (.*?)\1 非贪婪吃到下一个相同定界符。P3：先 includes 短路，跳过多数无 \verb 行的正则。
  // 注：未闭合 \verb（畸形 LaTeX）仍不剥离——可接受边界。
  const noVerbatim = line.includes("\\verb")
    ? line.replace(/\\verb\*?([^a-zA-Z\s\\])(.*?)\1/g, "")
    : line;
  // 找到第一个「真注释 %」（其前为偶数个连续反斜杠，含 0），截断；全部是转义 % 则原样返回。
  for (let i = 0; i < noVerbatim.length; i++) {
    if (noVerbatim[i] !== "%") continue;
    let bs = 0;
    for (let j = i - 1; j >= 0 && noVerbatim[j] === "\\"; j--) bs++;
    if (bs % 2 === 0) return noVerbatim.slice(0, i);
  }
  return noVerbatim;
}

/**
 * 代码片段补全时是否应剥掉片段自带的前导 `\`：当「词首前一字符」为 `\` 时返回 true。
 * 归属说明：这是**代码片段补全（latexSuggest.ts）专用**的启发式——Monaco 补全在替换
 * 片段时需知当前词是否带命令前缀 `\`。由于它依赖 Monaco `getWordUntilPosition` 的
 * 「当前词不含 `\`」语义（`\sec`→word=`sec`，startColumn=2），放公用 texParse 仅为
 * 便于单测锁定，业务上归 completion 所有。
 * 依赖：Monaco `getWordUntilPosition` 的当前词**不含** `\`（如 `\sec`→word=`sec`，startColumn=2），
 * 故用 startColumn 前一字符判断即可。若某日 `wordPattern` 把 `\` 纳入词字符，
 * 此处语义会变（建议列表可能被清空）——由 texParse.spec.ts 锁定该判定，改 wordPattern 时需同步。
 */
export function shouldStripLeadBackslash(line: string, startColumn: number): boolean {
  return startColumn > 1 && line[startColumn - 2] === "\\";
}
