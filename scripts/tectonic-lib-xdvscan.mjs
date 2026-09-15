#!/usr/bin/env node
// Tectonic 库形态的 XDV 页表复核入口（方案 §6 的 P5 复核命令）。
//
// 算法本体在 `crates/latteset-tectonic/examples/xdvscan.rs`（自
// `test_file/research-tectonic-lib/engine-spike/src/bin/xdvscan.rs` 移植，产出格式一致），
// 本脚本只做：参数校验 → 驱动 `cargo run --example xdvscan` → 打印 JSON → 按 `--expect-pages`
// 做一次独立比对（页数不符即非零退出）。
//
// 用法：
//   node scripts/tectonic-lib-xdvscan.mjs --pages-xdv <main.xdv> [--chunk 16384]
//        [--out <pages.json>] [--expect-pages N] [--refine] [--refine-budget-ms N] [--release]
//
// **精确页表默认关（V-01，2026-09-15）**：refine 趟的复杂度是「页数 × 窗口 × 文件长」，t23 复审
// 实测 large（4.59 MB / 125 页）**240 s 内跑不完（CPU 224.2 s）** ⇒ 默认走 `--no-refine`
// （只出页数 + 流式时间线，`page_ranges` 退回 `[0,0]`，P5/P6 命令不再挂住）。
// 需要精确页表时显式 `--refine [--refine-budget-ms N]`（默认 20000 ms）：超时即停在已收敛页数上，
// 并在 JSON 里如实报 `refine_mode: "partial"` + `refine_resolved_pages`。
//
// 说明：需要 `crates/latteset-tectonic` 能构建（原生链 vcpkg + freetype2/harfbuzz/ICU/graphite2）。
//
// **受限沙箱（DSH）下无法用本脚本**：Node 侧 `spawnSync("cargo")` 会被沙箱拒（EPERM），
// 但**不是** stub 坏掉（`node --check` 通过）。替代入口（参数等价）：
//   cargo run -p latteset-tectonic --example xdvscan -- --xdv <main.xdv> --chunk 16384 \
//        [--pages-json <out>] [--refine --refine-budget-ms 20000]
//
// 环境（原生链）：`VCPKG_ROOT` / `VCPKG_DEFAULT_TRIPLET` / `VCPKG_DEFAULT_HOST_TRIPLET` /
// `VCPKGRS_TRIPLET` = `x64-windows-static-release`、`TECTONIC_DEP_BACKEND=vcpkg`、
// `RUSTFLAGS=-Ctarget-feature=+crt-static`
// （`VCPKGRS_TRIPLET` 在 `--release` 下尤其不能省：vcpkg-rs 会自己算成 `x64-windows-static`（未装）
//  ⇒ build script panic。**静态 ICU 的 `advapi32` 等系统库已由 `crates/latteset-tectonic/build.rs`
//  补发，不再需要 `-Clink-arg=advapi32.lib`**。）

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

function parseArgs(argv) {
  const out = {
    chunk: 16384,
    out: null,
    expectPages: null,
    release: false,
    xdv: null,
    refine: false, // V-01：精确页表默认关
    refineBudgetMs: null,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--pages-xdv" || a === "--xdv") out.xdv = argv[++i];
    else if (a === "--chunk") out.chunk = Number(argv[++i]);
    else if (a === "--out" || a === "--pages-json") out.out = argv[++i];
    else if (a === "--expect-pages") out.expectPages = Number(argv[++i]);
    else if (a === "--release") out.release = true;
    else if (a === "--refine") out.refine = true;
    else if (a === "--no-refine") out.refine = false;
    else if (a === "--refine-budget-ms") out.refineBudgetMs = Number(argv[++i]);
    else if (a === "-h" || a === "--help") out.help = true;
    else throw new Error(`未知参数：${a}`);
  }
  return out;
}

const USAGE = `用法：node scripts/tectonic-lib-xdvscan.mjs --pages-xdv <main.xdv> [--chunk 16384]
      [--out <pages.json>] [--expect-pages N] [--refine] [--refine-budget-ms N] [--release]
  精确页表默认关（V-01：large 上 refine 240s 跑不完）；需要时加 --refine [--refine-budget-ms N]。`;

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
  if (!args.xdv) {
    console.error("缺少 --pages-xdv <main.xdv>");
    console.error(USAGE);
    process.exit(2);
  }
  const xdv = resolve(args.xdv);
  if (!existsSync(xdv)) {
    console.error(`XDV 不存在：${xdv}`);
    process.exit(2);
  }

  const cargoArgs = ["run", "-q", "-p", "latteset-tectonic", "--example", "xdvscan"];
  if (args.release) cargoArgs.push("--release");
  cargoArgs.push("--", "--xdv", xdv, "--chunk", String(args.chunk));
  // V-01：默认把 `--no-refine` 明确传下去（不靠 example 的默认值），需要精确页表时再打开。
  cargoArgs.push(args.refine ? "--refine" : "--no-refine");
  if (args.refine && args.refineBudgetMs !== null) {
    cargoArgs.push("--refine-budget-ms", String(args.refineBudgetMs));
  }
  if (args.out) cargoArgs.push("--pages-json", resolve(args.out));

  const r = spawnSync("cargo", cargoArgs, { encoding: "utf8" });
  if (r.error) {
    console.error(`无法启动 cargo：${r.error.message}`);
    process.exit(2);
  }
  const stdout = (r.stdout || "").trim();
  const line = stdout.split(/\r?\n/).filter(Boolean).pop() || "";
  if (!line.startsWith("{")) {
    console.error(`xdvscan 没有产出 JSON（exit=${r.status}）`);
    console.error(stdout.slice(-2000));
    console.error((r.stderr || "").slice(-2000));
    process.exit(1);
  }
  let report;
  try {
    report = JSON.parse(line);
  } catch {
    console.error(`xdvscan JSON 解析失败：${line.slice(0, 400)}`);
    process.exit(1);
  }
  console.log(JSON.stringify(report, null, 2));

  if (report.ok !== true) {
    console.error(`xdvscan 判定失败：${report.error ?? "unknown"}`);
    process.exit(1);
  }
  if (args.expectPages !== null && report.pages !== args.expectPages) {
    console.error(`页数不符：期望 ${args.expectPages}，实测 ${report.pages}`);
    process.exit(1);
  }
  // V-01 的诚实口径：默认（或预算用尽）时不声称页表精确。
  if (report.refine_mode !== "exact") {
    console.error(
      `注意：本次 refine_mode=${report.refine_mode}（已收敛 ${report.refine_resolved_pages ?? 0}/${report.pages} 页）` +
        `⇒ page_ranges ${report.refine_mode === "off" ? "恒为 [0,0]，只保证页数与流式时间线" : "仅前若干页精确"}；` +
        `需要精确页表用 --refine [--refine-budget-ms N]。`,
    );
  }
  if (args.out) console.error(`页表已写入 ${resolve(args.out)}`);
}

main();
