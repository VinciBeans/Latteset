// 生成性能基准工程（roadmap ③）。fixture 不入库，入库的是本脚本——`test_file/projects/` 已 gitignore。
//
// 六档全部**纯文本、无二进制资产**（图用 TikZ 画），保证任意机器可复现：
//   tiny         article 15 行                     —— 小文档下限
//   small-article 期刊论文典型（图表 + \cite→bibtex）—— 小文档
//   multifile   ctexbook + \include 20 章（中文）   —— 多文件 / 中英边界
//   graphics    30 个 TikZ 图 + 长表格              —— 重图（正文不多但编译慢）
//   large       约 300 页纯文本                     —— 大文档主体
//   thesis      ctexbook 学位论文（目录/公式/bibtex）—— ㉉5 关心的"首编总耗时"
//
// 用法：
//   node scripts/gen-bench-projects.mjs            # 已存在则跳过（幂等）
//   node scripts/gen-bench-projects.mjs --force    # 强制重建
//
// 产物：
//   test_file/projects/bench/<tier>/**            工程本体
//   test_file/projects/bench/bench-manifest.json  档位清单（bench.mjs 读它）

import { mkdirSync, writeFileSync, existsSync, rmSync, cpSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const BENCH_ROOT = resolve(HERE, "..", "test_file", "projects", "bench");
const force = process.argv.includes("--force");

/** 确定性伪随机（线性同余）：保证每次生成的内容逐字节一致，测量才可回归。 */
function makeRng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 0x100000000;
  };
}

/** 中文/英文填充句（可复现：由 rng 选词，不用 Date/Math.random）。 */
const CN_WORDS = ["排版", "编译", "文档", "章节", "公式", "参考文献", "图表", "结论", "方法", "实验", "分析", "讨论"];
const EN_WORDS = ["typesetting", "compilation", "document", "section", "formula", "reference", "figure", "conclusion", "method", "experiment"];

function filler(rng, words, count) {
  const out = [];
  for (let i = 0; i < count; i++) out.push(words[Math.floor(rng() * words.length)]);
  return out.join("");
}

/** 生成一段中文段落（约 lines 行）。 */
function cnParagraphs(rng, lines) {
  const out = [];
  for (let i = 0; i < lines; i++) out.push(filler(rng, CN_WORDS, 28) + "。");
  return out.join("\n\n");
}

/** 生成一段英文段落。 */
function enParagraphs(rng, lines) {
  const out = [];
  for (let i = 0; i < lines; i++) out.push(filler(rng, EN_WORDS, 10) + " " + filler(rng, EN_WORDS, 10) + ".");
  return out.join("\n\n");
}

function tikzFigure(name) {
  return String.raw`\begin{figure}[htbp]
\centering
\begin{tikzpicture}[scale=0.9]
  \draw[thick] (0,0) -- (3,0) -- (3,2) -- (0,2) -- cycle;
  \draw[thick] (1,0.4) -- (1,1.6) -- (2,1.6) -- (2,0.4) -- cycle;
  \foreach \x in {0.5,1.5,2.5} { \draw (\x,2) -- (\x,2.4); }
  \node at (1.5,-0.4) {${name}};
\end{tikzpicture}
\caption{Benchmark figure ${name}}
\end{figure}`;
}

// ---------------------------------------------------------------- 各档构造

const TIERS = {};

TIERS.tiny = {
  class: "small",
  editTarget: "main.tex",
  files: {
    "main.tex": String.raw`\documentclass{article}
\begin{document}
\section{Tiny}
Hello TeXPresso benchmark.
\end{document}
`,
  },
};

TIERS["small-article"] = () => {
  const rng = makeRng(1001);
  const body = [1, 2, 3].map((i) => String.raw`\section{Section ${i}}
${enParagraphs(rng, 4)}

${i === 2 ? tikzFigure(i) : ""}`).join("\n\n");
  return {
    class: "small",
    editTarget: "main.tex",
    files: {
      "main.tex": String.raw`\documentclass{article}
\usepackage{graphicx}
\usepackage{tikz}
\usepackage{booktabs}
\begin{document}
\title{Benchmark: journal-style article}
\author{TeXPresso}
\maketitle
\begin{abstract}
${enParagraphs(rng, 2)}
\end{abstract}
\tableofcontents
${body}

\begin{table}[htbp]
\centering
\begin{tabular}{lrrr}
\toprule
Item & A & B & C \\
\midrule
${Array.from({ length: 12 }, (_, i) => `row${i} & ${i} & ${i * 2} & ${i * 3} \\\\`).join("\n")}
\bottomrule
\end{tabular}
\caption{Benchmark table}
\end{table}

As shown in~\cite{knuth1984} and~\cite{lamport1994}, ${enParagraphs(rng, 3)}

\bibliographystyle{plain}
\bibliography{refs}
\end{document}
`,
      "refs.bib": String.raw`@book{knuth1984, author={Donald E. Knuth}, title={The TeXbook}, year={1984}, publisher={Addison-Wesley}}
@book{lamport1994, author={Leslie Lamport}, title={LaTeX: A Document Preparation System}, year={1994}, publisher={Addison-Wesley}}
`,
    },
  };
};

TIERS.multifile = () => {
  const rng = makeRng(2002);
  const N = 20;
  const files = {
    "main.tex": String.raw`\documentclass[UTF8]{ctexbook}
\usepackage{tikz}
\title{多文件基准工程}
\author{TeXPresso}
\begin{document}
\maketitle
\tableofcontents
${Array.from({ length: N }, (_, i) => String.raw`\include{chapters/ch${String(i + 1).padStart(2, "0")}}`).join("\n")}
\end{document}
`,
  };
  for (let i = 1; i <= N; i++) {
    const id = String(i).padStart(2, "0");
    files[`chapters/ch${id}.tex`] = String.raw`\chapter{第 ${i} 章 基准章节}
${cnParagraphs(rng, 6)}

\section{小节 ${i}.1}
${cnParagraphs(rng, 6)}

\section{小节 ${i}.2}
${cnParagraphs(rng, 6)}
`;
  }
  return { class: "small", editTarget: "chapters/ch03.tex", files };
};

TIERS.graphics = () => {
  const rng = makeRng(3003);
  const figs = Array.from({ length: 30 }, (_, i) => tikzFigure(i + 1)).join("\n\n");
  const table = String.raw`\begin{table}[htbp]
\centering
\begin{tabular}{lrrrrrr}
\hline
${Array.from({ length: 60 }, (_, i) => `r${i} & ${i} & ${i + 1} & ${i + 2} & ${i + 3} & ${i + 4} & ${i + 5} \\\\`).join("\n")}
\hline
\end{tabular}
\caption{Long benchmark table}
\end{table}`;
  return {
    class: "large",
    editTarget: "main.tex",
    files: {
      "main.tex": String.raw`\documentclass{article}
\usepackage{tikz}
\usepackage{booktabs}
\begin{document}
\section{Graphics-heavy benchmark}
${enParagraphs(rng, 5)}

${figs}

${table}

${enParagraphs(rng, 5)}
\end{document}
`,
    },
  };
};

TIERS.large = () => {
  const rng = makeRng(4004);
  const N = 120; // 每节约 2.5 页 → 约 300 页
  const body = Array.from({ length: N }, (_, i) => String.raw`\section{Section ${i + 1}}
${enParagraphs(rng, 22)}`).join("\n\n");
  return {
    class: "large",
    editTarget: "main.tex",
    files: {
      "main.tex": String.raw`\documentclass{article}
\begin{document}
\tableofcontents
${body}
\end{document}
`,
    },
  };
};

TIERS.thesis = () => {
  const rng = makeRng(5005);
  const N = 8;
  const files = {
    "main.tex": String.raw`\documentclass[UTF8,oneside]{ctexbook}
\usepackage{amsmath,amssymb}
\usepackage{tikz}
\title{学位论文基准（合成）}
\author{TeXPresso}
\begin{document}
\maketitle
\frontmatter
\tableofcontents
\mainmatter
${Array.from({ length: N }, (_, i) => String.raw`\include{chapters/ch${String(i + 1).padStart(2, "0")}}`).join("\n")}
\backmatter
\bibliographystyle{plain}
\bibliography{refs}
\end{document}
`,
    "refs.bib": Array.from({ length: 30 }, (_, i) => `@article{ref${i}, author={Author ${i}}, title={Title ${i}}, journal={J}, year={20${String(i).padStart(2, "0")}}}`).join("\n"),
  };
  for (let i = 1; i <= N; i++) {
    const id = String(i).padStart(2, "0"); // 补零：与 editTarget `chapters/ch03.tex` 对齐
    files[`chapters/ch${id}.tex`] = String.raw`\chapter{第 ${i} 章 研究内容}
${cnParagraphs(rng, 8)}

\section{方法}
${cnParagraphs(rng, 8)}

\begin{equation}
E = mc^2 + \sum_{k=1}^{n} \frac{\alpha_k}{\beta_k}
\end{equation}

\section{实验}
${cnParagraphs(rng, 8)}
本文引用~\cite{ref${i}} 与~\cite{ref${(i % 30) + 1}}。

${tikzFigure(i)}
`;
  }
  return { class: "large", editTarget: "chapters/ch03.tex", files };
};

// ---------------------------------------------------------------- 落盘

function writeTier(name, spec, root) {
  const dir = join(root, name);
  for (const [rel, content] of Object.entries(spec.files)) {
    const p = join(dir, rel);
    mkdirSync(dirname(p), { recursive: true });
    writeFileSync(p, content, "utf8");
  }
  return { name, dir, main: "main.tex", editTarget: spec.editTarget, class: spec.class };
}

// 可选真实档：把本机 TeX Live 里的**真实学位论文模板样例**复制进来测量。
// 合成档保证跨机器可复现；真实档回答"真实论文到底多慢"（⑲ 实测 hithesis 首编 >90s）。
// 只在本机存在该样例时才登记；bench.mjs 默认不跑 external 档（需 --with-real）。
const REAL_TEMPLATES = [
  {
    name: "thesis-real-hithesis",
    src: "C:/texlive/2026/texmf-dist/doc/xelatex/hithesis",
    main: "main.tex",
    editTarget: "body/introduction.tex",
    // 该模板自带 latexmkrc（覆写 $pdflatex 为 xelatex + --shell-escape，并自带 cp）——
    // 正好是 roadmap ㉖ 要验证的"模板 rc 与产品构建约定的交互"，测量本身就是证据。
    note: "hithesis 样例（自带 latexmkrc：--shell-escape + 自建 cp）",
  },
];

function writeExternalTier(t, root) {
  if (!existsSync(t.src)) return null;
  const dir = join(root, t.name);
  if (existsSync(dir) && !force) return { ...t, dir, external: true, generated: false };
  rmSync(dir, { recursive: true, force: true });
  cpSync(t.src, dir, { recursive: true });
  return { ...t, dir, external: true, generated: true };
}

function main() {
  if (force && existsSync(BENCH_ROOT)) {
    rmSync(BENCH_ROOT, { recursive: true, force: true });
    console.log(`已清空 ${BENCH_ROOT}`);
  }
  mkdirSync(BENCH_ROOT, { recursive: true });

  const manifest = [];
  for (const [name, def] of Object.entries(TIERS)) {
    const dir = join(BENCH_ROOT, name);
    if (existsSync(dir) && !force) {
      // 幂等：已生成则只登记（spec 求值有成本，跳过）
      const spec = typeof def === "function" ? null : def;
      manifest.push({
        name,
        dir,
        main: "main.tex",
        editTarget: spec ? spec.editTarget : "main.tex",
        class: spec ? spec.class : "unknown",
        generated: false,
      });
      console.log(`跳过（已存在）: ${name}`);
      continue;
    }
    const spec = typeof def === "function" ? def() : def;
    const entry = writeTier(name, spec, BENCH_ROOT);
    manifest.push({ ...entry, generated: true });
    const bytes = Object.values(spec.files).reduce((a, s) => a + Buffer.byteLength(s, "utf8"), 0);
    console.log(`生成: ${name.padEnd(14)} ${Object.keys(spec.files).length} 文件 / ${(bytes / 1024).toFixed(0)} KB / class=${spec.class}`);
  }

  // manifest 始终重写（幂等路径下也要保证 editTarget/class 正确）
  const full = Object.keys(TIERS).map((name) => {
    const spec = typeof TIERS[name] === "function" ? TIERS[name]() : TIERS[name];
    return { name, dir: join(BENCH_ROOT, name), main: "main.tex", editTarget: spec.editTarget, class: spec.class };
  });
  for (const t of REAL_TEMPLATES) {
    const entry = writeExternalTier(t, BENCH_ROOT);
    if (!entry) {
      console.log(`跳过真实档 ${t.name}（本机无样例：${t.src}）`);
      continue;
    }
    full.push({
      name: t.name,
      dir: entry.dir,
      main: t.main,
      editTarget: t.editTarget,
      class: "large",
      external: true,
      note: t.note,
    });
    console.log(`真实档: ${t.name} ${entry.generated ? "已复制" : "已存在"}`);
  }
  const manifestPath = join(BENCH_ROOT, "bench-manifest.json");
  writeFileSync(manifestPath, JSON.stringify({ generatedAt: new Date().toISOString(), tiers: full }, null, 2), "utf8");
  console.log(`\nmanifest: ${manifestPath}`);
}

main();
