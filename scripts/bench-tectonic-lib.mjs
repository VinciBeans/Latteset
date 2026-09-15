#!/usr/bin/env node
// Tectonic 库形态 vs 子进程形态的运行时常量复核入口（方案 §6 的 P4 复核命令；LIB-1 / LIB-2）。
//
// 判据（方案 §6 P4 的验收）：**库形态同档中位 < 子进程地板 × 0.8，且绝对节省 ≥ 100 ms**。
// 本脚本只负责"量"和"判"：它把两条命令各跑 N 次、取中位，然后按上面那条判据给出结论。
//
// 用法：
//   node scripts/bench-tectonic-lib.mjs --runs 3 --mode fresh \
//        --subproc "tectonic -k --keep-logs --synctex -o tmp {root}" \
//        --lib "cargo run -q --release -p latteset-tectonic --example bench -- --root {root}"
//   可选：--json <out.json> 落盘原始样本；--warmup N（默认 1 次不计入）。
//
// 口径（必须与方案一起读）：
// - `--mode fresh`：每次都用干净工作目录（首编）；`--mode resident`：复用同一进程/同一目录（常驻档）。
//   常驻档的具体载荷由 `--lib` 里给的命令决定（本脚本不猜实现形态）。
// - 所有时间都是**墙钟**；样本数、中位、最小/最大都打印出来，避免只报平均数。
// - 命令模板里的 `{root}` 会被替换成 `--root <dir>`（缺省时用 `--work <dir>` 指定的临时目录）。

import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

function parseArgs(argv) {
  const out = { runs: 3, warmup: 1, mode: "fresh", subproc: null, lib: null, json: null, work: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--runs") out.runs = Number(argv[++i]);
    else if (a === "--warmup") out.warmup = Number(argv[++i]);
    else if (a === "--mode") out.mode = argv[++i];
    else if (a === "--subproc") out.subproc = argv[++i];
    else if (a === "--lib") out.lib = argv[++i];
    else if (a === "--json") out.json = argv[++i];
    else if (a === "--work") out.work = argv[++i];
    else if (a === "-h" || a === "--help") out.help = true;
    else throw new Error(`未知参数：${a}`);
  }
  return out;
}

const USAGE = `用法：node scripts/bench-tectonic-lib.mjs --runs 3 --mode fresh|resident
      [--subproc "<cmd>"] [--lib "<cmd>"] [--warmup 1] [--json <out.json>] [--work <dir>]

命令模板可用占位符 {root}（= 本次样本的工作目录）。`;

/** 命令模板 → argv（只做空白切分；带引号的复杂命令请自行包一层脚本）。 */
function toArgv(template, root) {
  return template
    .replaceAll("{root}", root)
    .split(/\s+/)
    .filter(Boolean);
}

function timeOnce(template, root) {
  const argv = toArgv(template, root);
  const t0 = process.hrtime.bigint();
  const r = spawnSync(argv[0], argv.slice(1), { cwd: root, encoding: "utf8" });
  const ms = Number(process.hrtime.bigint() - t0) / 1e6;
  return { ms, code: r.status, error: r.error ? r.error.message : null };
}

function median(xs) {
  const s = [...xs].sort((a, b) => a - b);
  const n = s.length;
  return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
}

function runSide(label, template, args, workRoot) {
  if (!template) return null;
  const samples = [];
  for (let i = 0; i < args.warmup; i++) timeOnce(template, workRoot);
  for (let i = 0; i < args.runs; i++) {
    // `fresh`：每个样本一个干净目录（避免上一趟的中间产物被复用）
    const root = args.mode === "fresh" && i > 0 ? mkdtempSync(join(tmpdir(), "latteset-bench-")) : workRoot;
    const s = timeOnce(template, root);
    samples.push({ ...s, root });
    if (root !== workRoot) rmSync(root, { recursive: true, force: true });
  }
  const ms = samples.map((s) => s.ms);
  const failed = samples.filter((s) => s.code !== 0);
  return {
    label,
    runs: samples.length,
    failed: failed.length,
    median_ms: Math.round(median(ms) * 10) / 10,
    min_ms: Math.round(Math.min(...ms) * 10) / 10,
    max_ms: Math.round(Math.max(...ms) * 10) / 10,
    samples,
  };
}

function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (e) {
    console.error(e.message);
    console.error(USAGE);
    process.exit(2);
  }
  if (args.help) {
    console.log(USAGE);
    return;
  }
  if (!args.subproc && !args.lib) {
    console.error("至少给一条命令：--subproc / --lib");
    console.error(USAGE);
    process.exit(2);
  }
  if (!["fresh", "resident"].includes(args.mode)) {
    console.error(`--mode 只支持 fresh|resident，收到 ${args.mode}`);
    process.exit(2);
  }
  const workRoot = args.work ? resolve(args.work) : mkdtempSync(join(tmpdir(), "latteset-bench-"));
  const report = {
    case: "bench-tectonic-lib",
    mode: args.mode,
    runs: args.runs,
    warmup: args.warmup,
    work_root: workRoot,
    subproc: runSide("subproc", args.subproc, args, workRoot),
    lib: runSide("lib", args.lib, args, workRoot),
  };

  if (report.subproc && report.lib) {
    const floor = report.subproc.median_ms;
    const lib = report.lib.median_ms;
    const saved = floor - lib;
    report.judgement = {
      // 方案 §6 P4 / LIB-1 / LIB-2 的判据原文
      threshold: "库形态中位 < 子进程中位 × 0.8 且绝对节省 ≥ 100 ms",
      saved_ms: Math.round(saved * 10) / 10,
      ratio: floor > 0 ? Math.round((lib / floor) * 1000) / 1000 : null,
      pass: Boolean(report.subproc.failed === 0 && report.lib.failed === 0 && lib < floor * 0.8 && saved >= 100),
    };
  } else {
    report.judgement = {
      threshold: "需要 --subproc 与 --lib 两侧都给命令才能判 LIB-1/LIB-2",
      pass: null,
    };
  }

  console.log(JSON.stringify(report, null, 2));
  if (args.json) writeFileSync(args.json, JSON.stringify(report, null, 2));
  // 只给一侧时是"基线测量"，不算失败；两侧齐了才按判据给退出码。
  if (report.judgement.pass === false) process.exit(1);
}

main();
