// SyncTeX 双向定位报告（roadmap ⑤ 的 DoD 工具）：三组样本 × 正反向 × 往返"跳到位"。
//
// 为什么需要它：SyncTeX 是"点 PDF 跳源码 / 从源码找 PDF"这条核心体验的底座，但它的失败往往
// 是**静默**的（映射到生成文件、坐标偏移、编译中竞争）。本项目要的是**可复现的成功率数字**，
// 而不是"点了一下看着还行"。
//
// 用法：
//   node scripts/synctex-report.mjs                     # 默认三组样本
//   node scripts/synctex-report.mjs --tolerance=5        # 往返行号容差（默认 3）
//   node scripts/synctex-report.mjs --projects=a,b       # 指定工程（相对仓库根或绝对路径）
//   node scripts/synctex-report.mjs --recompile          # 先重编（默认缺产物才编）
//
// 指标（每个工程）：
//   forward  源码行 →(synctex view)→ PDF 页/坐标        成功率
//   inverse  PDF 点 →(synctex edit)→ 源码文件/行        成功率
//   same     往返回到**同一个源文件**的比例（生成文件如 .toc/.nav 记为 miss）
//   jump     往返回到**同一处源码**（行号差 ≤ 容差）的比例 —— 这才是"跳到位"
//
// 工程约定：产物在 `tmp/<stem>.synctex.gz`，PDF 在项目根 `<stem>.pdf`（与 runner 一致）。
// 沙箱注意：**不用管道**——synctex stdout 重定向到临时文件（Node 的 pipe 在受限沙箱会 EPERM），
// 因此本脚本无需提权即可运行（与 scripts/bench.mjs 同一约定）。

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, openSync, closeSync, readFileSync, rmSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const arg = (name, dflt) => {
  const hit = process.argv.find((a) => a.startsWith(`--${name}=`));
  return hit ? hit.slice(name.length + 3) : dflt;
};
const flag = (name) => process.argv.includes(`--${name}`);

const TOLERANCE = Number(arg("tolerance", "3"));
const RECOMPILE = flag("recompile");
const ROOT = process.cwd();
const DEFAULT_PROJECTS = [
  "test_file/projects/multifile",
  "test_file/projects/beamer工程",
  "test_file/projects/bench/large",
];
const PROJECTS = (arg("projects", "") ? arg("projects", "").split(",") : DEFAULT_PROJECTS)
  .map((p) => path.resolve(ROOT, p));

/** 生成文件（不属于用户源码）——反向命中它们时不算"回到源码"。 */
const GENERATED_EXT = new Set([
  ".toc", ".aux", ".bbl", ".blg", ".nav", ".snm", ".out", ".lof", ".lot",
  ".idx", ".ind", ".glo", ".gls", ".bcf", ".run", ".xml", ".vrb",
]);

/** 收集样本位置：标题/标签/环境起点——这些是用户真正会点的地方。 */
const SAMPLE_PATTERNS = [
  /^\s*\\section\b/,
  /^\s*\\subsection\b/,
  /^\s*\\chapter\b/,
  /^\s*\\frametitle\b/,
  /^\s*\\begin\{frame\}/,
  /^\s*\\label\b/,
  /^\s*\\part\b/,
];

function listTexSources(dir) {
  const out = [];
  const walk = (d) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      if (e.name === "tmp" || e.name.startsWith(".")) continue;
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.toLowerCase().endsWith(".tex")) out.push(p);
    }
  };
  walk(dir);
  return out.sort();
}

/** 每个工程最多取 MAX_SAMPLES 个样本，按文件轮转（覆盖多文件）。 */
function collectSamples(project) {
  const MAX_SAMPLES = Number(arg("samples", "12"));
  const perFile = [];
  for (const f of listTexSources(project)) {
    const lines = readFileSync(f, "utf8").split(/\r?\n/);
    const hits = [];
    lines.forEach((l, i) => {
      if (SAMPLE_PATTERNS.some((re) => re.test(l))) hits.push(i + 1);
    });
    if (hits.length) perFile.push({ file: f, hits });
  }
  const samples = [];
  for (let round = 0; samples.length < MAX_SAMPLES; round++) {
    let added = false;
    for (const { file, hits } of perFile) {
      if (round >= hits.length) continue;
      samples.push({ file, line: hits[round] });
      added = true;
      if (samples.length >= MAX_SAMPLES) break;
    }
    if (!added) break;
  }
  return samples;
}

function compile(project, rootFile) {
  const stem = path.basename(rootFile, ".tex");
  const started = Date.now();
  const r = spawnSync(
    "latexmk",
    ["-xelatex", "-outdir=tmp", "-synctex=1", "-interaction=nonstopmode", path.basename(rootFile)],
    { cwd: project, stdio: "ignore", shell: false, timeout: 900000 },
  );
  return { ms: Date.now() - started, status: r.status, stem };
}

/**
 * 跑一次 synctex 并把 stdout 收敛成文本。
 * **不用管道**：stdout 直接写文件（受限沙箱下 Node 的 pipe 会 EPERM）。
 */
function runSynctex(args, project, tag) {
  mkdirSync(path.join(project, "tmp"), { recursive: true });
  const outFile = path.join(project, "tmp", `.synctex-report-${tag}.txt`);
  const fd = openSync(outFile, "w");
  let res;
  try {
    res = spawnSync("synctex", args, { cwd: project, stdio: ["ignore", fd, fd], shell: false, timeout: 60000 });
  } finally {
    closeSync(fd);
  }
  const text = existsSync(outFile) ? readFileSync(outFile, "utf8") : "";
  rmSync(outFile, { force: true });
  return { ok: !res.error && res.status === 0, status: res.status, error: res.error?.code, text };
}

function parseForward(text) {
  let page = null, x = null, y = null;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (line.startsWith("Output:")) { page = x = y = null; continue; }
    if (page === null && line.startsWith("Page:")) page = Number(line.slice(5).trim());
    if (x === null && line.startsWith("x:")) x = Number(line.slice(2).trim());
    if (y === null && line.startsWith("y:")) y = Number(line.slice(2).trim());
    if (page !== null && Number.isFinite(x) && Number.isFinite(y)) return { page, x, y };
  }
  return null;
}

function parseInverse(text) {
  let file = null, line = null;
  for (const raw of text.split(/\r?\n/)) {
    const s = raw.trim();
    if (s.startsWith("Input:")) file = s.slice(6).trim();
    else if (s.startsWith("File:")) file = s.slice(5).trim();
    else if (s.startsWith("Line:")) line = Number(s.slice(5).trim());
  }
  if (!file || !Number.isFinite(line)) return null;
  return { file, line };
}

/** synctex 返回的路径形态不统一（正斜杠 / `./` 混入），比较前归一化。 */
const norm = (p) => path.resolve(p).replace(/\\/g, "/").replace(/\/\.\//g, "/").toLowerCase();

function reportProject(project) {
  const rootFile = ["main.tex", "thesis.tex"].map((n) => path.join(project, n)).find(existsSync);
  if (!rootFile) return { project, skipped: "找不到 main.tex" };
  const stem = path.basename(rootFile, ".tex");
  const pdf = path.join(project, `${stem}.pdf`);
  const synctexData = path.join(project, "tmp", `${stem}.synctex.gz`);

  let build = null;
  if (RECOMPILE || !existsSync(pdf) || !existsSync(synctexData)) {
    build = compile(project, rootFile);
  }
  if (!existsSync(synctexData)) return { project, skipped: `缺 ${path.relative(ROOT, synctexData)}（编译失败？）` };

  const samples = collectSamples(project);
  const rows = [];
  for (const [i, s] of samples.entries()) {
    const fwd = runSynctex(
      ["view", "-i", `${s.line}:1:${s.file}`, "-o", pdf, "-d", path.join(project, "tmp"), "-x"],
      project, `f${i}`,
    );
    const target = fwd.ok ? parseForward(fwd.text) : null;
    if (!target) {
      rows.push({ ...s, forward: false, reason: fwd.text.trim().split(/\r?\n/).pop() || fwd.error || "解析失败" });
      continue;
    }
    const inv = runSynctex(
      ["edit", "-o", `${target.page}:${target.x}:${target.y}:${pdf}`, "-d", path.join(project, "tmp")],
      project, `i${i}`,
    );
    const back = inv.ok ? parseInverse(inv.text) : null;
    if (!back) {
      rows.push({ ...s, forward: true, target, inverse: false, reason: inv.text.trim().split(/\r?\n/).pop() || inv.error || "解析失败" });
      continue;
    }
    const same = norm(back.file) === norm(s.file);
    const generated = GENERATED_EXT.has(path.extname(back.file).toLowerCase());
    const delta = same ? Math.abs(back.line - s.line) : null;
    rows.push({ ...s, forward: true, target, inverse: true, back, same, generated, delta, jump: same && delta <= TOLERANCE });
  }

  const total = rows.length;
  const fwdOk = rows.filter((r) => r.forward).length;
  const invOk = rows.filter((r) => r.inverse).length;
  const same = rows.filter((r) => r.same).length;
  const jump = rows.filter((r) => r.jump).length;
  const generated = rows.filter((r) => r.generated).length;
  const deltas = rows.filter((r) => r.delta !== null).map((r) => r.delta).sort((a, b) => a - b);
  return {
    project, rootFile, pdf, build, total, fwdOk, invOk, same, jump, generated, deltas, rows,
    maxPages: readPageHint(project, stem),
    skipped: null,
  };
}

/** 从 .log 里读页数（"Output written on … (N pages"）——用来解释大文档场景。 */
function readPageHint(project, stem) {
  const log = path.join(project, "tmp", `${stem}.log`);
  if (!existsSync(log)) return null;
  const m = readFileSync(log, "utf8").match(/Output written on [^(]*\((\d+) pages?/);
  return m ? Number(m[1]) : null;
}

const pct = (a, b) => (b === 0 ? "n/a" : `${((a / b) * 100).toFixed(1)}%`);

const results = [];
for (const project of PROJECTS) {
  if (!existsSync(project) || !statSync(project).isDirectory()) {
    console.log(`跳过（不存在）：${path.relative(ROOT, project)}`);
    continue;
  }
  const r = reportProject(project);
  results.push(r);
}

console.log(`SyncTeX 双向定位报告（容差 ${TOLERANCE} 行）`);
console.log(`样本来源：\\section / \\subsection / \\chapter / \\frametitle / \\begin{frame} / \\label / \\part\n`);
let hardFail = false;
for (const r of results) {
  const name = path.relative(ROOT, r.project);
  if (r.skipped) {
    console.log(`✗ ${name}：${r.skipped}`);
    hardFail = true;
    continue;
  }
  if (r.build) console.log(`  编译：exit=${r.build.status} ${(r.build.ms / 1000).toFixed(1)}s`);
  console.log(`■ ${name}`);
  console.log(`  样本 ${r.total} 个${r.maxPages ? `（文档 ${r.maxPages} 页）` : ""}`);
  console.log(`  正向 源码→PDF ：${r.fwdOk}/${r.total} ${pct(r.fwdOk, r.total)}`);
  console.log(`  反向 PDF→源码 ：${r.invOk}/${r.total} ${pct(r.invOk, r.total)}${r.generated ? `（其中 ${r.generated} 个落在生成文件上）` : ""}`);
  console.log(`  往返同文件    ：${r.same}/${r.total} ${pct(r.same, r.total)}`);
  console.log(`  往返跳到位    ：${r.jump}/${r.total} ${pct(r.jump, r.total)}（行号差 ≤ ${TOLERANCE}）`);
  if (r.deltas.length) {
    const med = r.deltas[Math.floor(r.deltas.length / 2)];
    console.log(`  行号差中位数  ：${med}（原始 ${r.deltas.join(",")}）`);
  }
  const misses = r.rows.filter((x) => !x.forward || !x.inverse || !x.jump);
  if (misses.length) {
    console.log("  未达标明细：");
    for (const m of misses) {
      const what = !m.forward ? "正向失败" : !m.inverse ? "反向失败" : m.generated ? "落在生成文件" : `行号差 ${m.delta}`;
      const where = `${path.relative(r.project, m.file)}:${m.line}`;
      const got = m.back ? ` → ${path.relative(r.project, m.back.file)}:${m.back.line}` : "";
      console.log(`    - ${where}：${what}${got}${m.reason ? `（${m.reason}）` : ""}`);
    }
  }
  if (r.fwdOk < r.total || r.invOk < r.total) hardFail = true;
  console.log("");
}

const totals = results.filter((r) => !r.skipped).reduce(
  (a, r) => ({ t: a.t + r.total, f: a.f + r.fwdOk, i: a.i + r.invOk, s: a.s + r.same, j: a.j + r.jump, g: a.g + r.generated }),
  { t: 0, f: 0, i: 0, s: 0, j: 0, g: 0 },
);
if (totals.t) {
  console.log(`合计：正向 ${totals.f}/${totals.t} ${pct(totals.f, totals.t)}｜反向 ${totals.i}/${totals.t} ${pct(totals.i, totals.t)}｜同文件 ${totals.s}/${totals.t} ${pct(totals.s, totals.t)}｜跳到位 ${totals.j}/${totals.t} ${pct(totals.j, totals.t)}｜生成文件命中 ${totals.g}`);
}
process.exit(hardFail ? 1 : 0);
