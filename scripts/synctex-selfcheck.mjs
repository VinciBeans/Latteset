#!/usr/bin/env node
// SyncTeX **自解析 vs 系统 CLI** 的逐点对拍（ADR-0008 实现替换的验收载体）。
//
// 为什么需要它：`scripts/synctex-report.mjs` 的同步精度基线（三组样本、往返跳到位 34/34）是在
// **系统 `synctex` CLI** 上量的。把实现换成自解析之后，"不劣于基线"这句话只有在**同一批样本**上
// 逐点比过才算数 —— 本脚本就做这件事：同一份 `.synctex.gz`、同一批查询，两侧各答一遍。
//
// 用法：
//   node scripts/synctex-selfcheck.mjs [--projects=a,b] [--samples=12] [--tol=3] [--json <out>]
//
// 两侧：
//   cli  —— `synctex view -i "line:0:<file>" -o <pdf> -d <tmp> -x` / `synctex edit -o "p:x:y:<pdf>" -d <tmp>`
//   self —— `cargo run -q --release -p latteset-infra --example synctex-selfcheck -- --pdf <pdf>`（stdin 喂查询）
//
// 指标（与 report 同口径）：
//   forward  同页率 + Δx/Δy 分布（自解析对 CLI）
//   inverse  同文件率 + 同行率（容差内）/ 行差分布
//   roundtrip 两侧各自的"往返回到同一源文件、同一处源码"成功率
//
// 沙箱注意：**不用管道**——CLI 的 stdout 重定向到临时文件（Node 的 pipe 在受限沙箱会 EPERM）。

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, openSync, closeSync, readFileSync, rmSync, readdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const arg = (n, d) => {
  const hit = process.argv.find((a) => a.startsWith(`--${n}=`));
  return hit ? hit.slice(n.length + 3) : d;
};
const ROOT = process.cwd();
const TOL = Number(arg("tol", "3"));
const MAX_SAMPLES = Number(arg("samples", "12"));
const PROJECTS = (arg("projects", "") ? arg("projects", "").split(",") : [
  "test_file/projects/multifile",
  "test_file/projects/beamer工程",
  "test_file/projects/bench/large",
]).map((p) => path.resolve(ROOT, p));

const SAMPLE_PATTERNS = [
  /^\s*\\section\b/, /^\s*\\subsection\b/, /^\s*\\chapter\b/,
  /^\s*\\frametitle\b/, /^\s*\\begin\{frame\}/, /^\s*\\label\b/, /^\s*\\part\b/,
];
const GENERATED_EXT = new Set([
  ".toc", ".aux", ".bbl", ".blg", ".nav", ".snm", ".out", ".lof", ".lot",
  ".idx", ".ind", ".glo", ".gls", ".bcf", ".run", ".xml", ".vrb",
]);

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

function collectSamples(project) {
  const perFile = [];
  for (const f of listTexSources(project)) {
    const lines = readFileSync(f, "utf8").split(/\r?\n/);
    const hits = [];
    lines.forEach((l, i) => { if (SAMPLE_PATTERNS.some((re) => re.test(l))) hits.push(i + 1); });
    if (hits.length) perFile.push({ file: f, hits });
  }
  const out = [];
  for (let round = 0; out.length < MAX_SAMPLES; round++) {
    let added = false;
    for (const { file, hits } of perFile) {
      if (round >= hits.length) continue;
      out.push({ file, line: hits[round] });
      added = true;
      if (out.length >= MAX_SAMPLES) break;
    }
    if (!added) break;
  }
  return out;
}

/** 跑一次 synctex，stdout 落文件（避开 Node 管道的沙箱 EPERM）。 */
function runSynctex(args, project) {
  const tmpDir = path.join(project, "tmp");
  mkdirSync(tmpDir, { recursive: true });
  const outFile = path.join(tmpDir, `.selfcheck-synctex.txt`);
  const fd = openSync(outFile, "w");
  let res;
  try {
    res = spawnSync("synctex", args, { cwd: project, stdio: ["ignore", fd, fd], shell: false, timeout: 60000 });
  } finally {
    closeSync(fd);
  }
  const text = existsSync(outFile) ? readFileSync(outFile, "utf8") : "";
  rmSync(outFile, { force: true });
  return { ok: !res.error && res.status === 0, text };
}

function parseForward(text) {
  let page = null, x = null, y = null;
  for (const raw of text.split(/\r?\n/)) {
    const s = raw.trim();
    if (s.startsWith("Output:")) { page = x = y = null; continue; }
    if (page === null && s.startsWith("Page:")) page = Number(s.slice(5).trim());
    if (x === null && s.startsWith("x:")) x = Number(s.slice(2).trim());
    if (y === null && s.startsWith("y:")) y = Number(s.slice(2).trim());
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
  return file && Number.isFinite(line) ? { file, line } : null;
}

const norm = (p) => path.resolve(p).replace(/\\/g, "/").replace(/\/\.\//g, "/").toLowerCase();
const isSource = (p) => !GENERATED_EXT.has(path.extname(p).toLowerCase());
const median = (xs) => {
  if (!xs.length) return null;
  const s = [...xs].sort((a, b) => a - b);
  return Math.round(s[Math.floor(s.length / 2)] * 100) / 100;
};

function reportProject(project) {
  const rootFile = ["main.tex", "thesis.tex"].map((n) => path.join(project, n)).find(existsSync);
  if (!rootFile) return { project, skipped: "找不到 main.tex/thesis.tex" };
  const stem = path.basename(rootFile, ".tex");
  const pdf = path.join(project, `${stem}.pdf`);
  const synctexData = path.join(project, "tmp", `${stem}.synctex.gz`);
  if (!existsSync(synctexData)) return { project, skipped: "缺 tmp/<stem>.synctex.gz（先编译一次）" };
  const tmpDir = path.join(project, "tmp");
  const samples = collectSamples(project);
  if (!samples.length) return { project, skipped: "样本模式一条都没命中" };

  // ── self 侧：一个进程答完所有查询 ──
  const queries = [];
  for (const s of samples) queries.push(`F\t${s.file}\t${s.line}`);
  queries.push("#SPLIT#"); // 占位：先问 forward，拿到 CLI 的页/坐标后再问 inverse
  const exe = path.join(ROOT, "src-tauri", "target", "release", "examples", "synctex-selfcheck.exe");
  if (!existsSync(exe)) return { project, skipped: `缺 ${exe}（先 cargo build --release -p latteset-infra --example synctex-selfcheck）` };

  const askSelf = (lines) => {
    const r = spawnSync(exe, ["--pdf", pdf], { input: lines.join("\n") + "\n", encoding: "utf8", timeout: 120000 });
    if (r.error || r.status !== 0) return null;
    return (r.stdout || "").split(/\r?\n/).filter(Boolean);
  };

  const selfFwd = askSelf(queries.slice(0, samples.length)) || [];
  const cliFwd = samples.map((s) => parseForward(runSynctex(
    ["view", "-i", `${s.line}:0:${s.file}`, "-o", pdf, "-d", tmpDir, "-x"], project).text));

  // ── 用**各自**的 forward 结果做 inverse（比对的是"这条链路"而不是单步）──
  const invQueries = [];
  const cliInvRefs = [];
  samples.forEach((s, i) => {
    const sf = selfFwd[i]?.startsWith("OK") ? selfFwd[i].split("\t").slice(1) : null;
    if (sf) invQueries.push(`I\t${sf[0]}\t${sf[1]}\t${sf[2]}`);
    else invQueries.push(`I\t0\t0\t0`);
    const cf = cliFwd[i];
    cliInvRefs.push(cf);
  });
  const selfInv = askSelf(invQueries) || [];
  const cliInv = cliInvRefs.map((cf) => cf
    ? parseInverse(runSynctex(["edit", "-o", `${cf.page}:${cf.x}:${cf.y}:${pdf}`, "-d", tmpDir], project).text)
    : null);

  // ── 汇总 ──
  let fwdOk = 0, fwdSamePage = 0;
  const dx = [], dy = [];
  samples.forEach((s, i) => {
    const sf = selfFwd[i]?.startsWith("OK") ? selfFwd[i].split("\t").slice(1).map(Number) : null;
    const cf = cliFwd[i];
    if (sf) fwdOk++;
    if (sf && cf && sf[0] === cf.page) {
      fwdSamePage++;
      dx.push(Math.abs(sf[1] - cf.x));
      dy.push(Math.abs(sf[2] - cf.y));
    }
  });

  let invOk = 0, invSameFile = 0, invSameLine = 0;
  const lineDiffs = [];
  samples.forEach((s, i) => {
    const si = selfInv[i]?.startsWith("OK") ? selfInv[i].split("\t").slice(1) : null;
    const ci = cliInv[i];
    if (si) invOk++;
    if (si && ci) {
      if (norm(si[0]) === norm(ci.file)) {
        invSameFile++;
        const d = Math.abs(Number(si[1]) - ci.line);
        lineDiffs.push(d);
        if (d <= TOL) invSameLine++;
      }
    }
  });

  // 两侧各自的往返（独立于对方）
  const rt = (fwd, inv) => {
    let same = 0, jump = 0, n = 0;
    samples.forEach((s, i) => {
      const f = fwd[i];
      const iv = inv[i];
      if (!f || !iv) return;
      n++;
      if (norm(iv.file) === norm(s.file)) {
        same++;
        if (Math.abs(iv.line - s.line) <= TOL) jump++;
      }
    });
    return { n, same, jump };
  };

  return {
    project: path.relative(ROOT, project),
    samples: samples.length,
    forward: { ok: fwdOk, same_page: fwdSamePage, dx_median: median(dx), dy_median: median(dy), dx_max: dx.length ? Math.round(Math.max(...dx) * 100) / 100 : null, dy_max: dy.length ? Math.round(Math.max(...dy) * 100) / 100 : null },
    inverse: { ok: invOk, same_file: invSameFile, within_tol: invSameLine, line_diff_median: median(lineDiffs), line_diff_max: lineDiffs.length ? Math.max(...lineDiffs) : null },
    roundtrip_cli: rt(cliFwd, cliInv),
    roundtrip_self: rt(
      selfFwd.map((l) => (l?.startsWith("OK") ? (() => { const p = l.split("\t").slice(1).map(Number); return { page: p[0], x: p[1], y: p[2] }; })() : null)),
      selfInv.map((l) => (l?.startsWith("OK") ? (() => { const p = l.split("\t").slice(1); return { file: p[0], line: Number(p[1]) }; })() : null)),
    ),
  };
}

const results = PROJECTS.map(reportProject);
const out = { case: "synctex-selfcheck", tolerance: TOL, projects: results };
console.log(JSON.stringify(out, null, 2));
const jsonOut = arg("json", "");
if (jsonOut) writeFileSync(jsonOut, JSON.stringify(out, null, 2));
