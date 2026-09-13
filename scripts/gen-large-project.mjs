// 生成「多文件大项目」编辑器侧性能夹具（roadmap ⑦c）。fixture 不入库，入库的是本脚本。
//
// 为什么单独一个生成器（不复用 bench 档）：⑦c 要回答的是**编辑器侧**在"文件数多 × 单文件大"下的
// 表现（打开标签耗时/内存、include 图解析、文件树规模），而 ③ 的 bench 档里 `multifile` 只有
// 94KB 总量、`large`/`thesis` 是单文件或 8 文件。三档刻意设计成能分离两个变量：
//
//   multi8    8 文件 × 256KB ≈ 2 MB    —— 文件数少、单文件大
//   multi20  20 文件 × 256KB ≈ 5 MB    —— **主档**：文件数 = 调研 #4410 的症状（20 文件）
//   multi40  40 文件 × 128KB ≈ 5 MB    —— 与 multi20 **总量相同、文件数翻倍**（分离"文件数"变量）
//
// 用法：
//   node scripts/gen-large-project.mjs            # 幂等：已存在则跳过
//   node scripts/gen-large-project.mjs --force    # 强制重建
//
// 产物：
//   test_file/projects/editor/<tier>/**            工程本体（ctexbook，可被 xelatex 编译）
//   test_file/projects/editor/editor-manifest.json 档位清单（editor-report.mjs 读它）

import { mkdirSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const EDITOR_ROOT = resolve(HERE, "..", "test_file", "projects", "editor");
const force = process.argv.includes("--force");

/** 确定性伪随机（线性同余，与 gen-bench-projects.mjs 同款）：内容可再生成、测量可回归。 */
function makeRng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 0x100000000;
  };
}

const CN_WORDS = ["排版", "编译", "文档", "章节", "公式", "参考文献", "图表", "结论", "方法", "实验", "分析", "讨论"];

function filler(rng, count) {
  const out = [];
  for (let i = 0; i < count; i++) out.push(CN_WORDS[Math.floor(rng() * CN_WORDS.length)]);
  return out.join("");
}

/** 一段中文正文（约 170 字节）。 */
function para(rng) {
  return filler(rng, 28) + "。";
}

/**
 * 生成一个"k 章 × 每章约 targetKB"的 ctexbook 工程。
 * 章节文件放在 chapters/ 并用 `\include`（与 bench/multifile 同构：latexmk 会建 tmp/chapters/）。
 */
function buildTier(name, chapters, targetKB, seed) {
  const rng = makeRng(seed);
  // 每段约 170 字节 → 段数 = targetKB*1024/170
  const parasPerChapter = Math.max(1, Math.round((targetKB * 1024) / 170));
  const files = {
    "main.tex": String.raw`\documentclass[UTF8,a4paper,12pt]{ctexbook}
\usepackage{amsmath,amssymb}
\title{多文件大项目夹具（${name}）}
\author{TeXPresso}
\begin{document}
\maketitle
\tableofcontents
${Array.from({ length: chapters }, (_, i) => String.raw`\include{chapters/ch${String(i + 1).padStart(2, "0")}}`).join("\n")}
\end{document}
`,
    "refs.bib": Array.from(
      { length: 20 },
      (_, i) => `@article{ref${i}, author={Author ${i}}, title={Title ${i}}, journal={J}, year={20${String(i).padStart(2, "0")}}}`
    ).join("\n"),
  };
  for (let i = 1; i <= chapters; i++) {
    const id = String(i).padStart(2, "0");
    const body = Array.from({ length: parasPerChapter }, () => para(rng)).join("\n\n");
    files[`chapters/ch${id}.tex`] = String.raw`\chapter{第 ${i} 章 性能夹具}
\label{ch:${i}}

\section{正文}
${body}

本文引用~\cite{ref${i % 20}}，并参见第~\ref{ch:${Math.max(1, i - 1)}}~章。
`;
  }
  return { name, chapters, targetKB, files, editTarget: "chapters/ch03.tex" };
}

const TIERS = [
  buildTier("multi8", 8, 256, 7001),
  buildTier("multi20", 20, 256, 7002),
  buildTier("multi40", 40, 128, 7003),
];

function main() {
  mkdirSync(EDITOR_ROOT, { recursive: true });
  const manifest = [];
  for (const tier of TIERS) {
    const dir = join(EDITOR_ROOT, tier.name);
    const bytes = Object.values(tier.files).reduce((a, s) => a + Buffer.byteLength(s, "utf8"), 0);
    let generated = false;
    if (existsSync(dir) && !force) {
      console.log(`跳过（已存在）: ${tier.name}`);
    } else {
      rmSync(dir, { recursive: true, force: true });
      for (const [rel, content] of Object.entries(tier.files)) {
        const p = join(dir, rel);
        mkdirSync(dirname(p), { recursive: true });
        writeFileSync(p, content, "utf8");
      }
      generated = true;
      console.log(
        `生成: ${tier.name.padEnd(9)} ${Object.keys(tier.files).length} 文件（${tier.chapters} 章 × ${tier.targetKB}KB 目标）/ ${(bytes / 1024 / 1024).toFixed(2)} MB`
      );
    }
    manifest.push({
      name: tier.name,
      dir,
      main: "main.tex",
      editTarget: tier.editTarget,
      chapters: tier.chapters,
      bytes,
      generated,
    });
  }
  const manifestPath = join(EDITOR_ROOT, "editor-manifest.json");
  writeFileSync(manifestPath, JSON.stringify({ generatedAt: new Date().toISOString(), tiers: manifest }, null, 2), "utf8");
  console.log(`\nmanifest: ${manifestPath}`);
}

main();
