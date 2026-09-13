// .fls / .fdb_latexmk 依赖报告（G1 研究配套工具）。
//
// 用法：
//   node scripts/fls-report.mjs <tmp/main.fls>
//   node scripts/fls-report.mjs <tmp/main.fls> --fdb <tmp/main.fdb_latexmk>
//
// 回答两个不同的问题：
//   ① `.fls`（`-recorder` 产出）：这次编译**引擎实际打开了**哪些文件——含 TeX Live 宏包、系统字体、中间产物。
//   ② `.fdb_latexmk`（latexmk 产出）：latexmk 认为产物**依赖**哪些文件，带 mtime/size/md5，且含 bibtex/biber 子步骤。
//
// 两者的差集很关键：`.bib` 只出现在后者（bibtex 是独立进程，不写进 xelatex 的 .fls）。
// 结论与实测见 docs/research/g1-read-interception-feasibility.md。

import { readFileSync } from "node:fs";

const args = process.argv.slice(2);
const flsPath = args.find((a) => !a.startsWith("--"));
const fdbIdx = args.indexOf("--fdb");
const fdbPath = fdbIdx >= 0 ? args[fdbIdx + 1] : null;

if (!flsPath) {
  console.error("用法: node scripts/fls-report.mjs <tmp/main.fls> [--fdb <tmp/main.fdb_latexmk>]");
  process.exit(2);
}

const norm = (p) => p.replace(/\\/g, "/").replace(/^\.\//, "");
const lower = (p) => norm(p).toLowerCase();
const isTexlive = (p) => /texlive|\/texmf(-|$)/i.test(p);
const isSystemFont = (p) => /\/windows\/fonts\//i.test(p);
const isIntermediate = (rel) =>
  rel.startsWith("tmp/") || /\.(aux|log|toc|out|bbl|blg|xdv|fls|fdb_latexmk|synctex\.gz|synctex\(busy\))$/i.test(rel);

/** 读 .fls：PWD 行给项目根，其余是 INPUT/OUTPUT。 */
function parseFls(text) {
  const pwd = /^PWD (.+)$/m.exec(text)?.[1] ?? "";
  const root = lower(norm(pwd)).replace(/\/$/, "");
  const inputs = [];
  const outputs = [];
  for (const line of text.split("\n")) {
    const m = /^(INPUT|OUTPUT) (.+)$/.exec(line.trim());
    if (!m) continue;
    (m[1] === "INPUT" ? inputs : outputs).push(norm(m[2]));
  }
  return { root, inputs, outputs };
}

function classify(paths, root) {
  const buckets = { project: [], font: [], texlive: [], outside: [] };
  for (const p of paths) {
    const l = lower(p);
    // .fls 里项目内文件可能是相对路径（相对 PWD 行）——相对即项目内
    const abs = /^[a-z]:\//.test(l) || l.startsWith("/");
    if (isTexlive(l)) buckets.texlive.push(p);
    else if (isSystemFont(l)) buckets.font.push(p);
    else if (!abs) buckets.project.push(norm(p));
    else if (root && l.startsWith(root + "/")) buckets.project.push(norm(p).slice(root.length + 1));
    else buckets.outside.push(p);
  }
  return buckets;
}

/** 读 .fdb_latexmk：`[命令] ...` 开步骤，缩进的 `"路径" mtime size md5` 是依赖。 */
function parseFdb(text) {
  const steps = [];
  const deps = new Map();
  let cur = null;
  for (const raw of text.split("\n")) {
    const line = raw.trimEnd();
    if (!line.trim()) continue;
    if (line.startsWith("[")) {
      cur = { cmd: line, deps: [] };
      steps.push(cur);
      const quoted = line.match(/"[^"]+"/g) ?? [];
      for (const q of quoted) cur.deps.push(q.slice(1, -1));
      continue;
    }
    const m = /^\s+"([^"]+)"\s+(\d+)\s+(\d+)\s+([0-9a-f]{32})?/.exec(line);
    if (m) {
      const [, path, mtime, size, md5] = m;
      deps.set(norm(path), { path: norm(path), mtime: Number(mtime), size: Number(size), md5: md5 ?? "" });
      if (cur) cur.deps.push(path);
    }
  }
  return { steps, deps };
}

// ---------------------------------------------------------------- .fls

const flsText = readFileSync(flsPath, "utf8");
const { root, inputs, outputs } = parseFls(flsText);
const cls = classify(inputs, root);
const srcDeps = cls.project.filter((p) => !isIntermediate(p));
const inter = cls.project.filter(isIntermediate);
const uniq = (a) => [...new Set(a)].sort();

console.log(`=== .fls（引擎实际打开）`);
console.log(`文件: ${flsPath}   ${(flsText.length / 1024).toFixed(1)} KB   ${flsText.split("\n").length} 行`);
console.log(`INPUT ${inputs.length}（去重 ${uniq(inputs).length}）  OUTPUT ${outputs.length}`);
console.log(`项目根: ${norm(root) || "(未记录)"}`);
console.log(`\n项目内源文件输入（${uniq(srcDeps).length}）:`);
for (const p of uniq(srcDeps)) console.log(`  ${p}`);
console.log(`\n中间产物（${uniq(inter).length}）: ${uniq(inter).slice(0, 8).join(" ")}${uniq(inter).length > 8 ? " …" : ""}`);
console.log(`系统字体（${uniq(cls.font).length}）: ${uniq(cls.font).join(" ") || "-"}`);
console.log(`项目外非 TeX Live（${uniq(cls.outside).length}）: ${uniq(cls.outside).slice(0, 5).join(" ") || "-"}`);
console.log(`TeX Live 文件: ${cls.texlive.length}（已折叠）`);

// ---------------------------------------------------------------- .fdb_latexmk（可选）

if (fdbPath) {
  const fdbText = readFileSync(fdbPath, "utf8");
  const { steps, deps } = parseFdb(fdbText);
  const nonTl = [...deps.values()].filter((d) => !isTexlive(d.path));
  const flsSet = new Set(uniq(inputs).map(lower));
  const srcDeps = nonTl.filter((d) => !isIntermediate(norm(d.path)));
  const interDeps = nonTl.filter((d) => isIntermediate(norm(d.path)));
  const stamp = (t) => new Date(t * 1000).toLocaleString("zh-CN", { hour12: false });

  console.log(`\n=== .fdb_latexmk（latexmk 依赖图）`);
  console.log(`文件: ${fdbPath}   ${(fdbText.length / 1024).toFixed(1)} KB`);
  console.log(`步骤 ${steps.length}:`);
  for (const s of steps) console.log(`  ${s.cmd.slice(0, 110)}`);
  console.log(`\n源依赖（非 TeX Live、非中间产物，${srcDeps.length}）:`);
  for (const d of srcDeps.sort((a, b) => a.path.localeCompare(b.path))) {
    console.log(`  ${d.path}   mtime=${stamp(d.mtime)} size=${d.size} md5=${d.md5.slice(0, 12)}`);
  }
  console.log(`中间产物依赖（${interDeps.length}）: ${interDeps.map((d) => d.path).join(" ")}`);
  const onlyFdb = srcDeps.filter((d) => !flsSet.has(lower(d.path)));
  console.log(`\n只在 .fdb_latexmk、不在 .fls 的源依赖（${onlyFdb.length}）—— 这些文件变化同样要触发编译:`);
  for (const d of onlyFdb) console.log(`  ${d.path}`);
}
