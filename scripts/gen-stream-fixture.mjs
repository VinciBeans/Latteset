// 生成「流式输出」真机验证用的长文档 fixture（roadmap 阶段 2，2026-09）。
//
// 用法（目标目录须在 .gitignore 覆盖的 test_file/projects/ 下）：
//   node scripts/gen-stream-fixture.mjs test_file/projects/_stream-lab
//   node scripts/gen-stream-fixture.mjs test_file/projects/_stream-lab --error
//
// 产物：main.tex + ch_01..10.tex（10 章 ctexbook，约 400KB / 160 页；本机 Full 编译 ≈8s）。
// `--error` 在第 5 章末尾插入 `\undefinedmacrohere`，用来验证"编译还没结束就能看见错误"。
//
// 为什么章节用 `\input` 而不是 `\include{子目录/文件}`：后者会让 TeX 往 `tmp/子目录/` 写 .aux，
// 而该目录不存在 → 触发与流式无关的另一条失败路径（见 docs/troubleshooting.md）。
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2];
const withError = process.argv.includes("--error");
if (!dir) throw new Error("用法: node scripts/gen-stream-fixture.mjs <项目目录> [--error]");
mkdirSync(dir, { recursive: true });

const S = [
  "长文档的排版性能是编辑器体验的分水岭。",
  "中文用户常见的文档形态是学位论文与教材，通常有数十页正文与上百个交叉引用。",
  "TeX 的多趟收敛机制正是为此设计：第一趟收集标签与页码，后续趟次据此回填。",
  "当引用尚未收敛时，页面上的编号会短暂显示为问号，这属于正常现象。",
  "编辑器若能在编译过程中即时反馈进度，用户就不必在无输出的等待中反复猜测。",
  "实测表明，把引擎输出接入管道并逐行解析，可以在进程结束前拿到页码与错误。",
  "日志文件按页刷新，因此比块缓冲的标准输出更适合作为实时反馈的来源。",
  "把这两条通道合并去重后，界面既能显示已排版页数，也能尽早暴露致命错误。",
];

const para = (k) => Array.from({ length: 4 }, (_, i) => S[(k + i) % S.length]).join("");

let main = "\\documentclass[UTF8,a4paper,12pt]{ctexbook}\n\\begin{document}\n\\tableofcontents\n";

for (let c = 1; c <= 10; c++) {
  const n = String(c).padStart(2, "0");
  let body = `\\chapter{第 ${c} 章 长文档排版实践}\\label{ch:${c}}\n`;
  const sections = ["背景与动机", "方法与实现", "小结"];
  let k = c;
  sections.forEach((title, si) => {
    if (withError && c === 5 && si === 2) body += "\\undefinedmacrohere\n";
    body += `\\section{${title}}\\label{sec:${c}-${si + 1}}\n`;
    const count = si === 2 ? 20 : 40;
    for (let p = 0; p < count; p++) body += para(k++) + "\n\n";
  });
  body += `本章要点可参见第 \\ref{ch:${c}} 章与第 \\ref{sec:${c}-1} 节。\n`;
  writeFileSync(join(dir, `ch_${n}.tex`), body, "utf8");
  main += `\\input{ch_${n}}\n`;
}
main += "\\end{document}\n";
writeFileSync(join(dir, "main.tex"), main, "utf8");

console.log(`fixture 已生成: ${dir}${withError ? "（含 \u005cundefinedmacrohere）" : ""}`);
