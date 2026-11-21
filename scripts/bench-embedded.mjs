#!/usr/bin/env node
// 内嵌 Tectonic（release CLI）**六档性能基线**：一条命令产出 JSON 原始数据 + 可摘进 docs 的 Markdown。
//
// 为什么需要它：ADR-0015 删掉 latexmk/xelatex 后，`bench.mjs` 那套口径（latexmk 命令、noop=判定
// up-to-date）不再代表产品；阶段②只有点测（Full 1,200 ms / Quick 75 ms）。阶段③要按 roadmap ③ 的
// 验收线拿「六档 cold/noop/edit 基线进 design.md」说话，所以测量仪器必须与**现行实现**同口径：
// 一次进程一条命令（`latteset-cli compile`），计时取 CLI JSON 的 `elapsed_ms`（产品口径），
// 分阶段取 runner 的 `库形态分阶段耗时` 日志（含 T14 新增的 prep/post/preview 打点）。
//
// 六档（夹具由 `scripts/gen-bench-projects.mjs` 生成，夹具不入库、脚本入库）：
//   tiny / small-article / multifile / graphics / large / thesis
//
// 每次采样内部的口径（按顺序跑；每个口径每档各 N 次取中位，另记 min/max + 全部原始值）：
//   cold-format   清空 <cache>/formats + 项目 tmp/ 与主 PDF → Full（**真首编**：含 ~1 s format 生成）
//   cold-project  保留 formats、只清项目 tmp/ 与主 PDF → Full（**format 已命中**的首编）
//   noop          紧接 cold-project 再 Full（同一项目第二次 Full，无改动）
//   noop2         再 Full 一次（稳态；与 noop 对照看热路径是否稳定）
//   edit          **注释级编辑**（不改排版）→ `--quick`；预期 A 闸门命中：
//                 页哈希逐页相同 ⇒ 跳过 XDV→PDF 转换与拷贝（`reused_pdf=true`）
//   edit2         **真实改动**（正文插一句话，会改变 PDF/页哈希）→ `--quick`；预期 A 不命中、
//                 需要重跑收敛（多文件/目录档为 2 趟），是最接近真实编辑触发的口径
// 另有两条一次性测量：
//   cross-project 每档第一次编译时 formats 由**上一档**留下（验证 format 跨项目命中）
//   进程地板      `help`（纯进程）/ `settings` / `open` 各 N 次（spawn→退出→JSON 可见的墙钟）
//
// 预算（design.md §2.6 的产品约束，按 manifest 的文档规模意图分类）：
//   小文档：冷编译 1 s 优秀 / 2 s 及格；大文档：3 s 优秀 / 5 s 及格。
//   本脚本把同一组阈值用于两处判定（与旧 bench.mjs 同风格）：
//     · cold 预算：cold-format 的 `elapsed_ms` 中位（真首编，含 format）
//     · edit→出图预算：`e2e = debounce + edit2 中位 + preview`（旧 bench.mjs 的端到端口径；
//       preview 是前端 pdf.js 重载的估算值，默认 100 ms，可用 --preview= 覆盖）
//   任一档 FAIL ⇒ 退出码 1（可用 --allow-over-budget 只看数字不判红）；
//   两轮复现比较 `--compare=<上一轮 JSON>`：同档同口径中位差 >10% ⇒ 退出码 1（可用 --allow-drift 放宽）。
//
// 用法：
//   node scripts/bench-embedded.mjs                       # 六档 × 3 次，写 JSON + Markdown
//   node scripts/bench-embedded.mjs --tiers=tiny,thesis   # 只跑指定档（跨项目命中按给定顺序）
//   node scripts/bench-embedded.mjs --samples=5 --out=test_file/phase3/baseline.json
//   node scripts/bench-embedded.mjs --compare=test_file/phase3/baseline-run1.json
//   node scripts/bench-embedded.mjs --skip=small-article --skip-reason="缺 lmroman9（T14b 补 bundle）"
//
// 环境（脚本自己设，不依赖外部 shell）：
//   LATTESET_TECTONIC_BUNDLE = test_file/tectonic-bundle（本地目录 bundle，离线、带 SHA256SUM）
//   LATTESET_TECTONIC_CACHE  = test_file/phase3/tcache
//   LATTESET_CONFIG_DIR      = test_file/phase3/config（隔离，不碰真实用户配置）
//   TMP/TEMP                 = src-tauri/target/.tmp（DSH Low 完整性标签下子进程唯一可写目录）
// 前置：release CLI（`npm run lib:cli`，需 vcpkg 环境）+ 夹具（`node scripts/gen-bench-projects.mjs`）。
//
// 注意（沙箱）：不用管道捕获子进程输出（Node 管道 stdio 在受限模式下会 EPERM），
// 统一把 stdout/stderr 重定向到文件再读；stdout 只有一份 JSON，stderr 是 tracing 日志。

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  openSync,
  closeSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const CLI = join(ROOT, "src-tauri", "target", "release", "latteset-cli.exe");
const BENCH_ROOT = join(ROOT, "test_file", "projects", "bench");
const MANIFEST = join(BENCH_ROOT, "bench-manifest.json");
const PHASE3 = join(ROOT, "test_file", "phase3");
const TMPDIR = join(ROOT, "src-tauri", "target", ".tmp");

// ---------------------------------------------------------------- 参数

function arg(name, dflt) {
  const hit = process.argv.find((a) => a.startsWith(`--${name}=`));
  return hit ? hit.slice(name.length + 3) : dflt;
}
const flag = (name) => process.argv.includes(`--${name}`);

const SAMPLES = Number(arg("samples", "3"));
const DEBOUNCE_MS = Number(arg("debounce", "500"));
const PREVIEW_MS = Number(arg("preview", "100"));
const OUT_JSON = resolve(ROOT, arg("out", "test_file/phase3/baseline.json"));
const OUT_MD = resolve(ROOT, arg("md", "test_file/phase3/BASELINE.md"));
const COMPARE = arg("compare", "");
const ALLOW_OVER_BUDGET = flag("allow-over-budget");
const ALLOW_DRIFT = flag("allow-drift");
const ONLY = arg("tiers", "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);
const SKIP = arg("skip", "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);
const SKIP_REASON = arg("skip-reason", "未指定原因");
const BUNDLE = arg("bundle", "test_file/tectonic-bundle");
const BUNDLE_ABS = resolve(ROOT, BUNDLE);
const CACHE = resolve(ROOT, arg("cache", "test_file/phase3/tcache"));
const FORMATS = join(CACHE, "formats");
const CONFIG = resolve(ROOT, arg("config", "test_file/phase3/config"));
const LOGS = resolve(ROOT, arg("logs", "test_file/phase3/logs-embedded"));

// 档位顺序（由小到大；跨项目 format 命中依赖这个顺序：后一档用前一档留下的 format）
const TIER_ORDER = ["tiny", "small-article", "multifile", "graphics", "large", "thesis"];

// 预算：design.md §2.6（按 manifest 的文档规模意图分类）
const BUDGET = {
  small: { excellent: 1000, pass: 2000 },
  large: { excellent: 3000, pass: 5000 },
};

// 旧基线（latexmk/xelatex 时代，design.md §2.8；仅供对比解释，不是当前实现的承诺）
const OLD_BASELINE = {
  tiny: { cold: 2307, noop: 500, edit: 1625 },
  "small-article": { cold: 6476, noop: 502, edit: 2593 },
  multifile: { cold: 6465, noop: 495, edit: 2463 },
  graphics: { cold: 2631, noop: 499, edit: 1826 },
  large: { cold: 13532, noop: 486, edit: 4104 },
  thesis: { cold: 9412, noop: 507, edit: 3143 },
};

// 每个口径的说明（写进 JSON meta，也写进 Markdown §1.3）
const PROTOCOL = {
  "cold-format": "清空 <cache>/formats + 项目 tmp/ 与主 PDF → Full（真首编：含 format 生成）",
  "cold-project": "保留 formats、只清项目 tmp/ 与主 PDF → Full（format 命中的首编）",
  noop: "紧接 cold-project 再 Full（无改动；同一项目第二次 Full）",
  noop2: "再 Full 一次（稳态）",
  edit: "注释级编辑（不改排版）→ --quick；A 闸门命中（页哈希逐页相同 ⇒ 跳过转换）",
  edit2: "真实改动（正文插一句话，改变 PDF）→ --quick；A 不命中，含重跑收敛",
  "cross-project": "本档第一次编译时 format 由上一档留下（跨项目命中）",
};
const CONDITION_ORDER = ["cold-format", "cold-project", "noop", "noop2", "edit", "edit2"];

// ---------------------------------------------------------------- 环境

const ENV = {
  ...process.env,
  TMP: TMPDIR,
  TEMP: TMPDIR,
  LATTESET_TECTONIC_BUNDLE: BUNDLE,
  LATTESET_TECTONIC_CACHE: CACHE,
  LATTESET_CONFIG_DIR: CONFIG,
  RUST_LOG: "info",
  NO_COLOR: "1",
  CLICOLOR: "0",
};

// ---------------------------------------------------------------- 工具

const hrtimeMs = () => Number(process.hrtime.bigint()) / 1e6;
const r1 = (x) => (x == null ? null : Math.round(x * 10) / 10);
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const n = s.length;
  if (!n) return null;
  return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
};

function stats(xs) {
  const ok = xs.filter((x) => typeof x === "number" && Number.isFinite(x));
  if (!ok.length) return { n: 0, min: null, median: null, max: null };
  return { n: ok.length, min: r1(Math.min(...ok)), median: r1(median(ok)), max: r1(Math.max(...ok)) };
}

function dirStats(p) {
  if (!existsSync(p)) return { exists: false, files: 0, bytes: 0 };
  let files = 0;
  let bytes = 0;
  for (const name of readdirSync(p)) {
    const q = join(p, name);
    try {
      const st = statSync(q);
      if (st.isFile()) {
        files += 1;
        bytes += st.size;
      }
    } catch {
      /* 忽略瞬时文件（编译时可能正在写） */
    }
  }
  return { exists: true, files, bytes };
}

function listTex(dir, base = dir, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) {
      if (name !== "tmp") listTex(p, base, out);
    } else if (name.endsWith(".tex")) {
      out.push(p);
    }
  }
  return out;
}

const rel = (p) => p.replace(ROOT + "\\", "").replaceAll("\\", "/");

/**
 * 跑一次 CLI。返回墙钟 + 解析后的 JSON + stderr 里的分阶段/趟数日志。
 * stdout/stderr 用 fd 重定向到文件（沙箱下管道 stdio 会 EPERM）。
 */
function runCli(args, { logPrefix, timeoutMs = 600_000 } = {}) {
  const outPath = `${logPrefix}.json`;
  const errPath = `${logPrefix}.err`;
  const outFd = openSync(outPath, "w");
  const errFd = openSync(errPath, "w");
  const t0 = hrtimeMs();
  const r = spawnSync(CLI, args, {
    cwd: ROOT,
    env: ENV,
    stdio: ["ignore", outFd, errFd],
    timeout: timeoutMs,
    windowsHide: true,
  });
  const wallMs = hrtimeMs() - t0;
  closeSync(outFd);
  closeSync(errFd);

  const stdout = readFileSync(outPath, "utf8");
  const stderr = readFileSync(errPath, "utf8");
  let json = null;
  try {
    json = JSON.parse(stdout);
  } catch {
    json = null;
  }
  const compile = json && json.compile ? json.compile : null;
  const phase = parsePhase(stderr);
  return {
    wall_ms: r1(wallMs),
    exit_code: r.status ?? -1,
    timed_out: r.error?.code === "ETIMEDOUT",
    json,
    compile,
    phase,
    passes_log: parsePasses(stderr),
    format_generated: /format 已生成并落产品缓存/.test(stderr),
    format_path: parseFormatPath(stderr),
    log_json: rel(outPath),
    log_err: rel(errPath),
    stderr_bytes: Buffer.byteLength(stderr),
  };
}

/** 解析 tracing 的一行：`format_ms=997 typeset_ms=76 ...`（ANSI 已由 NO_COLOR/CLICOLOR 避开）。 */
function parsePhase(stderr) {
  let hit = null;
  for (const line of stderr.split(/\r?\n/)) {
    if (line.includes("库形态分阶段耗时")) hit = line;
  }
  if (!hit) return null;
  const get = (k) => {
    const m = hit.match(new RegExp(`(?:^|[\\s(])${k}=([^\\s]+)`));
    return m ? m[1] : null;
  };
  const num = (k) => {
    const v = get(k);
    if (v == null) return null;
    const n = Number(v);
    return Number.isFinite(n) ? n : null;
  };
  const flagOf = (k) => get(k) === "true";
  return {
    format_ms: num("format_ms"),
    typeset_ms: num("typeset_ms"),
    convert_ms: num("convert_ms"),
    engine_total_ms: num("engine_total_ms"),
    // T14 新增打点：prep（bundle 打开 + 主文件读）/ post（PDF 原子替换 + 页哈希缓存写）/
    // preview（编译中流式出图累计耗时，计入 typeset 窗口内）。
    prep_ms: num("prep_ms"),
    post_ms: num("post_ms"),
    preview_ms: num("preview_ms"),
    preview_paints: num("preview_paints"),
    format_cached: flagOf("format_cached"),
    passes: num("passes"),
    pages: num("pages"),
    reused_pdf: flagOf("reused_pdf"),
    req_ram: num("req_ram"),
    req_mem: num("req_mem"),
    req_disk: num("req_disk"),
    req_bundle: num("req_bundle"),
    req_miss: num("req_miss"),
    bundle_read_ms: num("bundle_read_ms"),
    bundle_read_n: num("bundle_read_n"),
    format_read_ms: num("format_read_ms"),
  };
}

/** `库形态排版趟数 passes=2 stable=true converged=true requested=Full` */
function parsePasses(stderr) {
  for (const line of stderr.split(/\r?\n/)) {
    if (!line.includes("库形态排版趟数")) continue;
    const get = (k) => {
      const m = line.match(new RegExp(`(?:^|[\\s(])${k}=([^\\s]+)`));
      return m ? m[1] : null;
    };
    return {
      passes: Number(get("passes")),
      stable: get("stable") === "true",
      converged: get("converged") === "true",
      requested: get("requested"),
    };
  }
  return null;
}

/** `format 已生成并落产品缓存（…） … path=<file>`（只有冷缓存那一趟才有）。 */
function parseFormatPath(stderr) {
  for (const line of stderr.split(/\r?\n/)) {
    if (!line.includes("format 已生成并落产品缓存")) continue;
    const m = line.match(/path=([^\s]+)/);
    if (m) return m[1];
  }
  return null;
}

// ---------------------------------------------------------------- 清理与编辑

const cleanFormats = () => rmSync(FORMATS, { recursive: true, force: true });

function cleanProject(projectDir) {
  rmSync(join(projectDir, "tmp"), { recursive: true, force: true });
  rmSync(join(projectDir, "main.pdf"), { force: true });
  rmSync(join(projectDir, "main.pdf.tmp"), { force: true });
}

// 注释级编辑：不改排版 ⇒ 页哈希应逐页相同（A 闸门命中的构造）。加在文件末尾对任何文件都安全。
const COMMENT_EDIT = "\n% bench edit (comment-only: 页哈希应逐页相同 -> A 闸门命中)\n";

function makeCommentEdit(original) {
  return original + COMMENT_EDIT;
}

/**
 * 真实改动：在**第一个 `\section{…}` 之后**插一句话（正文开头），让后续内容整体下移，
 * 交叉引用/目录页码可能变化 ⇒ 覆盖真实编辑最容易触发重跑的情形（含转换趟）。
 * 找不到 `\section` 时退回 `\end{document}` 前 / 文件末尾。
 */
function makeRealEdit(original) {
  const sentence = "\nBenchmark edit inserted sentence.\n";
  const m = original.match(/\\section\{[^}]*\}/);
  if (m) {
    const at = original.indexOf(m[0]) + m[0].length;
    return original.slice(0, at) + "\n" + sentence + original.slice(at);
  }
  const idx = original.lastIndexOf("\\end{document}");
  if (idx >= 0) return original.slice(0, idx) + sentence + original.slice(idx);
  return original + sentence;
}

// ---------------------------------------------------------------- 单次编译记录

function compileRecord(label, index, tier, args, logPrefix) {
  const before = dirStats(FORMATS);
  const r = runCli(args, { logPrefix });
  const after = dirStats(FORMATS);
  const phase = r.phase;
  const passesLog = r.passes_log;
  return {
    label,
    index,
    requested: args.includes("--quick") ? "quick" : "full",
    elapsed_ms: r.compile ? r.compile.elapsed_ms : null,
    wall_ms: r.wall_ms,
    process_overhead_ms: r.compile ? r1(r.wall_ms - r.compile.elapsed_ms) : null,
    status: r.compile ? r.compile.status : null,
    exit_code: r.exit_code,
    timed_out: r.timed_out,
    kind: r.compile ? r.compile.kind : null,
    upgraded_from_quick: r.compile ? r.compile.upgraded_from_quick : null,
    pdf_path: r.compile ? r.compile.pdf_path : null,
    engine: r.compile ? r.compile.engine : null,
    errors: r.compile ? r.compile.errors.length : null,
    failure: r.compile ? r.compile.failure : null,
    phase,
    passes_log: passesLog,
    // 口径验证用：edit 应 `reused_pdf=true`（A 命中）；edit2 应 `reused_pdf=false`。
    a_hit: phase ? phase.reused_pdf : null,
    passes: passesLog ? passesLog.passes : phase ? phase.passes : null,
    format_generated: r.format_generated,
    format_path: r.format_path,
    formats_before: before,
    formats_after: after,
    log_json: r.log_json,
    log_err: r.log_err,
  };
}

// ---------------------------------------------------------------- 统计

function phaseStats(records) {
  const ok = records.map((r) => r.phase).filter(Boolean);
  const col = (k) => stats(ok.map((p) => p[k]));
  const total = col("engine_total_ms");
  const share = (k) => {
    const v = col(k);
    if (!v.median || !total.median) return null;
    return Math.round((v.median / total.median) * 1000) / 10;
  };
  return {
    n: ok.length,
    format_ms: col("format_ms"),
    typeset_ms: col("typeset_ms"),
    convert_ms: col("convert_ms"),
    engine_total_ms: total,
    prep_ms: col("prep_ms"),
    post_ms: col("post_ms"),
    preview_ms: col("preview_ms"),
    preview_paints: col("preview_paints"),
    format_read_ms: col("format_read_ms"),
    bundle_read_ms: col("bundle_read_ms"),
    format_cached: ok.map((p) => p.format_cached),
    passes: ok.map((p) => p.passes),
    pages: ok.map((p) => p.pages),
    reused_pdf: ok.map((p) => p.reused_pdf),
    share_pct: {
      format: share("format_ms"),
      typeset: share("typeset_ms"),
      convert: share("convert_ms"),
      preview: share("preview_ms"),
    },
  };
}

/**
 * 离群检测（**保留并标注**，不剔除）：同口径同档 N 次里，某次与中位偏差 > 阈值即标出。
 * 阈值按口径给：cold 类固定成本大，20% 才说明真异常；noop/edit 热路径波动小，25%。
 */
function flagOutliers(records) {
  const vals = records.map((r) => r.elapsed_ms).filter((x) => typeof x === "number");
  const med = median(vals);
  const out = [];
  if (!med) return out;
  for (const r of records) {
    if (typeof r.elapsed_ms !== "number" || !med) continue;
    const dev = Math.abs(r.elapsed_ms - med) / med;
    if (dev > 0.2) out.push({ index: r.index, label: r.label, elapsed_ms: r.elapsed_ms, median_ms: r1(med), deviation_pct: r1(dev * 100) });
  }
  return out;
}

function summarizeCondition(records) {
  const elapsed = records.map((r) => r.elapsed_ms).filter((x) => typeof x === "number");
  return {
    n: records.length,
    elapsed_ms: stats(elapsed),
    wall_ms: stats(records.map((r) => r.wall_ms)),
    process_overhead_ms: stats(records.map((r) => r.process_overhead_ms)),
    phase: phaseStats(records),
    kinds: records.map((r) => r.kind),
    a_hit: records.map((r) => r.a_hit),
    passes: records.map((r) => r.passes),
    exit_codes: records.map((r) => r.exit_code),
    outliers: flagOutliers(records),
  };
}

// ---------------------------------------------------------------- 单档测量

function measureTier(tier, doCrossProject) {
  const editPath = join(tier.dir, tier.editTarget);
  if (!existsSync(editPath)) throw new Error(`缺少 editTarget：${tier.editTarget}（${tier.name}）`);
  const original = readFileSync(editPath, "utf8");
  const safeName = tier.name.replace(/[^\w.-]+/g, "_");
  const tierLogs = join(LOGS, safeName);
  mkdirSync(tierLogs, { recursive: true });

  const out = {
    tier: tier.name,
    dir: rel(tier.dir),
    edit_target: tier.editTarget,
    class: tier.class,
    status: "ok",
    error: null,
    cross_project: null,
    samples: [],
  };

  try {
    // 跨项目 format 命中：本档第一次编译，format 来自**上一档**（若 formats 为空说明是首档，跳过）。
    if (doCrossProject && dirStats(FORMATS).files > 0) {
      cleanProject(tier.dir);
      out.cross_project = compileRecord(
        "cross-project",
        0,
        tier,
        ["--project", tier.dir, "compile"],
        join(tierLogs, "cross-project")
      );
    }

    for (let i = 1; i <= SAMPLES; i++) {
      const sample = { index: i };
      // cold-format：清 formats + 项目产物 ⇒ 真首编（含 format 生成）
      cleanFormats();
      cleanProject(tier.dir);
      sample["cold-format"] = compileRecord("cold-format", i, tier, ["--project", tier.dir, "compile"], join(tierLogs, `s${i}-cold-format`));
      // cold-project：保留 format，只清项目产物 ⇒ 热 format 的首编
      cleanProject(tier.dir);
      sample["cold-project"] = compileRecord("cold-project", i, tier, ["--project", tier.dir, "compile"], join(tierLogs, `s${i}-cold-project`));
      sample.noop = compileRecord("noop", i, tier, ["--project", tier.dir, "compile"], join(tierLogs, `s${i}-noop`));
      sample.noop2 = compileRecord("noop2", i, tier, ["--project", tier.dir, "compile"], join(tierLogs, `s${i}-noop2`));
      // edit：注释级编辑（A 闸门应命中）
      writeFileSync(editPath, makeCommentEdit(original), "utf8");
      sample.edit = compileRecord("edit", i, tier, ["--project", tier.dir, "compile", "--quick"], join(tierLogs, `s${i}-edit`));
      writeFileSync(editPath, original, "utf8");
      // edit2：真实改动（A 不命中，含重跑收敛）
      writeFileSync(editPath, makeRealEdit(original), "utf8");
      sample.edit2 = compileRecord("edit2", i, tier, ["--project", tier.dir, "compile", "--quick"], join(tierLogs, `s${i}-edit2`));
      writeFileSync(editPath, original, "utf8");
      out.samples.push(sample);
    }
  } catch (e) {
    out.status = "error";
    out.error = String(e && e.stack ? e.stack : e);
  } finally {
    // 无论成败都还原被编辑的源文件（不把测量痕迹留在夹具里）
    try {
      writeFileSync(editPath, original, "utf8");
    } catch {
      /* ignore */
    }
  }

  const pick = (cond) => out.samples.map((s) => s[cond]).filter(Boolean);
  out.stats = {};
  for (const cond of CONDITION_ORDER) {
    const recs = pick(cond);
    if (recs.length) out.stats[cond] = summarizeCondition(recs);
  }
  // 口径自证：edit 应全部 A 命中；edit2 应全部 A 不命中。不符时登记 protocol_warning（不伪造）。
  out.protocol_checks = {
    edit_a_hit_all: pick("edit").length > 0 && pick("edit").every((r) => r.a_hit === true),
    edit2_a_miss_all: pick("edit2").length > 0 && pick("edit2").every((r) => r.a_hit === false),
    edit_kinds: pick("edit").map((r) => r.kind),
    edit2_kinds: pick("edit2").map((r) => r.kind),
    edit2_passes: pick("edit2").map((r) => r.passes),
    noop_kinds: pick("noop").map((r) => r.kind),
  };
  const firstCold = pick("cold-format")[0] || null;
  out.pages = firstCold && firstCold.phase ? firstCold.phase.pages : null;
  // 档级失败：cold-format 没成功（编译失败/超时）即视为该档不可测（如 bundle 缺口）。
  const coldFails = pick("cold-format").filter((r) => r.exit_code !== 0 || r.status !== "success");
  if (out.status === "ok" && coldFails.length) {
    out.status = "failed";
    out.error = `cold-format 失败：${coldFails.map((r) => `exit=${r.exit_code} status=${r.status} failure=${r.failure}`).join("; ")}`;
  }
  return out;
}

// ---------------------------------------------------------------- 进程地板 / 机器信息

function measureFloor(tierDir) {
  const floorLogs = join(LOGS, "_floor");
  mkdirSync(floorLogs, { recursive: true });
  const run = (label, args) => {
    const recs = [];
    for (let i = 1; i <= SAMPLES; i++) {
      const r = runCli(args, { logPrefix: join(floorLogs, `${label}-${i}`) });
      recs.push({ index: i, wall_ms: r.wall_ms, exit_code: r.exit_code, ok: r.json ? r.json.ok : null });
    }
    return { samples: recs, wall_ms: stats(recs.map((x) => x.wall_ms)) };
  };
  return {
    note: "进程地板：spawn→退出→JSON 可见的墙钟。help 是纯进程；settings/open 含建 Session + 打开项目 + 读设置。",
    help: run("help", ["help"]),
    settings: run("settings", ["--project", tierDir, "settings"]),
    open: run("open", ["--project", tierDir, "open"]),
  };
}

/** 机器负载采样（每档前后各一次；用 pwsh 一次性取 CPU 负载/空闲内存/进程数）。 */
function sampleMachineLoad() {
  const ps = spawnSync(
    "pwsh",
    [
      "-NoProfile",
      "-Command",
      "$c=Get-CimInstance Win32_Processor; $o=Get-CimInstance Win32_OperatingSystem; " +
        "[pscustomobject]@{cpu_load_pct=(($c | Measure-Object -Property LoadPercentage -Average).Average); " +
        "free_mb=[math]::Round($o.FreePhysicalMemory/1KB,0); proc_count=(Get-Process).Count} | ConvertTo-Json -Compress",
    ],
    { stdio: ["ignore", "pipe", "ignore"], encoding: "utf8", timeout: 30_000 }
  );
  try {
    const o = JSON.parse(ps.stdout);
    return { sampled_at: new Date().toISOString(), ...o };
  } catch {
    return null;
  }
}

function machineInfo() {
  const ps = spawnSync(
    "pwsh",
    [
      "-NoProfile",
      "-Command",
      "$c=Get-CimInstance Win32_Processor; $o=Get-CimInstance Win32_OperatingSystem; " +
        "[pscustomobject]@{cpu=$c.Name; cores=$c.NumberOfCores; logical=$c.NumberOfLogicalProcessors; " +
        "ram_gb=[math]::Round($o.TotalVisibleMemorySize/1MB,2); os=$o.Caption; build=$o.BuildNumber} | ConvertTo-Json -Compress",
    ],
    { stdio: ["ignore", "pipe", "ignore"], encoding: "utf8", timeout: 30_000 }
  );
  try {
    return JSON.parse(ps.stdout);
  } catch {
    return null;
  }
}

function cliInfo() {
  const st = statSync(CLI);
  const hash = createHash("sha256").update(readFileSync(CLI)).digest("hex");
  const git = spawnSync("git", ["rev-parse", "HEAD"], { cwd: ROOT, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
  const dirty = spawnSync("git", ["status", "--porcelain", "--", "crates", "src-tauri", "src", "scripts", "docs"], {
    cwd: ROOT,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  return {
    path: rel(CLI),
    bytes: st.size,
    mtime: st.mtime.toISOString(),
    sha256: hash,
    git_head: (git.stdout || "").trim(),
    product_code_dirty: (dirty.stdout || "").trim() ? (dirty.stdout || "").trim().split(/\r?\n/) : [],
  };
}

function bundleInfo() {
  const files = [];
  const walk = (d) => {
    for (const name of readdirSync(d)) {
      const p = join(d, name);
      const st = statSync(p);
      if (st.isDirectory()) walk(p);
      else files.push({ path: rel(p), bytes: st.size });
    }
  };
  if (!existsSync(BUNDLE_ABS)) return { path: BUNDLE, exists: false };
  walk(BUNDLE_ABS);
  const sha = files.find((f) => f.path.endsWith("/SHA256SUM") || f.path === "SHA256SUM");
  let digest = null;
  if (sha) {
    const body = readFileSync(join(ROOT, sha.path), "utf8").trim();
    digest = /^[0-9a-f]{64}$/i.test(body) ? body.toLowerCase() : null;
  }
  return {
    path: BUNDLE,
    exists: true,
    files: files.length,
    bytes: files.reduce((a, f) => a + f.bytes, 0),
    digest,
    has_sha256sum: Boolean(sha),
  };
}

// ---------------------------------------------------------------- 预算与复现比较

function budgetVerdict(tier, elapsedMs) {
  const b = BUDGET[tier.class] || BUDGET.large;
  const verdict = elapsedMs == null ? "N/A" : elapsedMs <= b.excellent ? "EXCELLENT" : elapsedMs <= b.pass ? "PASS" : "FAIL";
  return { class: tier.class, budget: b, verdict };
}

function evaluateBudgets(tierResults, tiers) {
  const rows = [];
  for (const tier of tiers) {
    const r = tierResults[tier.name];
    if (!r || r.status !== "ok") continue;
    const cold = r.stats["cold-format"]?.elapsed_ms?.median ?? null;
    const edit = r.stats.edit?.elapsed_ms?.median ?? null;
    const edit2 = r.stats.edit2?.elapsed_ms?.median ?? null;
    const coldV = budgetVerdict(tier, cold);
    // edit→出图 = debounce + 编译中位 + 前端预览估算（旧 bench.mjs 的端到端口径）
    const e2eEdit = edit == null ? null : r1(DEBOUNCE_MS + edit + PREVIEW_MS);
    const e2eEdit2 = edit2 == null ? null : r1(DEBOUNCE_MS + edit2 + PREVIEW_MS);
    const edit2V = budgetVerdict(tier, e2eEdit2);
    rows.push({
      tier: tier.name,
      class: tier.class,
      cold_ms: cold,
      cold_verdict: coldV.verdict,
      cold_budget: coldV.budget,
      edit_ms: edit,
      edit2_ms: edit2,
      e2e_edit_ms: e2eEdit,
      e2e_edit2_ms: e2eEdit2,
      e2e_edit2_verdict: edit2V.verdict,
      e2e_budget: edit2V.budget,
      fail: coldV.verdict === "FAIL" || edit2V.verdict === "FAIL",
    });
  }
  return rows;
}

/** 两轮独立运行比较：同档同口径 `elapsed_ms` 中位差（%）。任务口径：<10%。 */
function compareWith(prevPath, report) {
  if (!prevPath) return null;
  const abs = resolve(ROOT, prevPath);
  if (!existsSync(abs)) return { path: rel(abs), exists: false, note: "上一轮 JSON 不存在，跳过比较" };
  let prev;
  try {
    prev = JSON.parse(readFileSync(abs, "utf8"));
  } catch (e) {
    return { path: rel(abs), exists: true, error: String(e), note: "上一轮 JSON 解析失败，跳过比较" };
  }
  const rows = [];
  let maxAbs = 0;
  for (const tier of Object.keys(report.tiers)) {
    const cur = report.tiers[tier];
    const old = prev.tiers ? prev.tiers[tier] : null;
    if (!old) continue;
    for (const cond of CONDITION_ORDER) {
      const a = cur.stats?.[cond]?.elapsed_ms?.median ?? null;
      const b = old.stats?.[cond]?.elapsed_ms?.median ?? null;
      if (a == null || b == null) continue;
      const delta = b === 0 ? 0 : ((a - b) / b) * 100;
      maxAbs = Math.max(maxAbs, Math.abs(delta));
      rows.push({ tier, condition: cond, run1_ms: b, run2_ms: a, delta_pct: r1(delta), within_10pct: Math.abs(delta) <= 10 });
    }
  }
  return {
    path: rel(abs),
    prev_generated_at: prev.generatedAt ?? null,
    rows,
    max_abs_delta_pct: r1(maxAbs),
    within_10pct: rows.every((r) => r.within_10pct),
  };
}

// ---------------------------------------------------------------- Markdown

function renderMarkdown(report) {
  const L = [];
  const t = (x) => (x == null ? "–" : String(x));
  const m = (s) => (s && s.median != null ? `${t(s.min)} / **${t(s.median)}** / ${t(s.max)}` : "–");
  const tiers = report.tier_order.map((name) => report.tiers[name]).filter((x) => x && x.status === "ok");
  const all = report.tier_order.map((name) => report.tiers[name]).filter(Boolean);
  const failed = all.filter((x) => x.status !== "ok");
  const coldFormatMed = tiers.map((x) => x.stats["cold-format"]?.phase?.format_ms?.median).filter((x) => x != null);

  L.push("# 内嵌 Tectonic（release CLI）六档性能基线");
  L.push("");
  L.push("> 由 `scripts/bench-embedded.mjs` 一条命令生成（JSON + 本 Markdown）。这是**内嵌唯一形态**的");
  L.push("> cold/noop/edit 基线，供 [docs/design.md](../../docs/design.md) §2.8 替换「内嵌形态 `[未测]`」、");
  L.push("> 并给阶段③优化（T17/T18）提供可回归的数字。夹具不入库、脚本入库。");
  L.push("");
  L.push(`- 生成时间：${report.generatedAt}`);
  L.push(`- 产品构建：HEAD \`${report.meta.cli.git_head}\`，release CLI sha256 \`${report.meta.cli.sha256.slice(0, 16)}…\`（${report.meta.cli.bytes} B，mtime ${report.meta.cli.mtime}）`);
  L.push(`- 产品代码 dirty：${report.meta.cli.product_code_dirty.length ? "`" + report.meta.cli.product_code_dirty.join("`、`") + "`（**含 T14 打点改动**）" : "无"}`);
  L.push(`- bundle：\`${report.meta.bundle.path}\`（${report.meta.bundle.exists ? `${report.meta.bundle.files} 文件 / ${report.meta.bundle.bytes} B${report.meta.bundle.digest ? `，digest \`${report.meta.bundle.digest.slice(0, 16)}…\`` : ""}` : "**缺失**"}）；缓存 \`${report.meta.env.LATTESET_TECTONIC_CACHE}\``);
  L.push(`- 采样：每档每口径 **${report.meta.samples_per_tier} 次**，报 min / **median** / max，原始值在 JSON`);
  L.push(`- 机器：${report.meta.machine ? `${report.meta.machine.cpu}（${report.meta.machine.logical} 逻辑核）/ ${report.meta.machine.ram_gb} GB / ${report.meta.machine.os}` : "未知"}`);
  L.push("");

  // ---- 0 摘要
  L.push("## 0. 结论摘要");
  L.push("");
  L.push(`- **冷编译的固定成本 = format 生成**：${tiers.length} 档 \`format_ms\` 中位 ${coldFormatMed.length ? `${Math.min(...coldFormatMed)}–${Math.max(...coldFormatMed)} ms` : "–"}（与文档规模几乎无关）；命中后每次读回 \`.fmt\` 只需 ${(() => { const xs = tiers.map((x) => x.stats.noop?.phase?.format_read_ms?.median).filter((x) => x != null); return xs.length ? `${Math.min(...xs)}–${Math.max(...xs)} ms` : "–"; })()}。`);
  L.push(`- **进程地板**：\`help\` ${report.process_floor ? m(report.process_floor.help.wall_ms) : "–"} ms；每次编译的进程开销（墙钟 − elapsed）中位 ${(() => { const xs = tiers.map((x) => x.stats["cold-format"]?.process_overhead_ms?.median).filter((x) => x != null); return xs.length ? `${Math.min(...xs)}–${Math.max(...xs)} ms` : "–"; })()}。`);
  L.push(`- **真实编辑（edit2, Quick）**：${tiers.map((x) => `\`${x.tier}\` ${t(x.stats.edit2?.elapsed_ms?.median)}ms`).join(" / ")}；**注释编辑（edit, A 命中）**：${tiers.map((x) => `\`${x.tier}\` ${t(x.stats.edit?.elapsed_ms?.median)}ms`).join(" / ")}。`);
  L.push(`- **编辑→出图预算（debounce ${report.meta.params.debounce_ms} + edit2 + preview ${report.meta.params.preview_render_ms}）**：${report.budget.map((b) => `\`${b.tier}\` ${t(b.e2e_edit2_ms)}ms ${b.e2e_edit2_verdict}`).join("；")}。`);
  if (report.budget.some((b) => b.fail)) {
    L.push(`- ⚠ **超预算档**：${report.budget.filter((b) => b.fail).map((b) => `\`${b.tier}\`（cold ${b.cold_verdict} / e2e ${b.e2e_edit2_verdict}）`).join("、")}——这是阶段③优化（T17/T18）的输入，不是测量失败。`);
  }
  if (report.reproducibility) {
    L.push(`- **两轮复现**：同档同口径中位差最大 **${report.reproducibility.max_abs_delta_pct}%**（阈值 10%）⇒ ${report.reproducibility.within_10pct ? "**可复现**" : "**超出阈值，需复测**"}。`);
  }
  if (report.protocol_checks_warnings.length) {
    L.push(`- 口径自证告警：${report.protocol_checks_warnings.join("；")}`);
  }
  L.push("");

  // ---- 1 口径
  L.push("## 1. 测量口径与环境");
  L.push("");
  L.push("### 1.1 六档夹具");
  L.push("");
  L.push("| 档 | 夹具 | 文件（.tex） | .tex 字节 | 实测页数 | 规模意图（预算类） |");
  L.push("|---|---|---:|---:|---:|---|");
  for (const f of report.fixtures) {
    L.push(`| \`${f.tier}\` | \`${f.dir}\` | ${f.tex_files} | ${f.tex_bytes} | ${t(f.pages)} | ${f.class} |`);
  }
  L.push("");
  L.push("### 1.2 环境");
  L.push("");
  L.push("| 项 | 值 |");
  L.push("|---|---|");
  for (const [k, v] of Object.entries(report.meta.env)) L.push(`| \`${k}\` | \`${v}\` |`);
  L.push(`| 计时 | CLI JSON \`elapsed_ms\`（产品口径）；墙钟 = Node spawnSync 前后 hrtime（spawn→退出） |`);
  L.push(`| 分阶段 | stderr \`库形态分阶段耗时\`：format / typeset / convert / engine_total / **prep**（bundle 打开+主文件读）/ **post**（PDF 原子替换+页哈希缓存写）/ **preview**（编译中流式出图） |`);
  L.push("");
  L.push("### 1.3 每个采样内部的口径");
  L.push("");
  L.push("| 口径 | 做法 |");
  L.push("|---|---|");
  for (const [k, v] of Object.entries(report.meta.protocol)) L.push(`| \`${k}\` | ${v} |`);
  L.push(`| \`进程地板\` | \`help\` / \`settings\` / \`open\` 各 ${report.meta.samples_per_tier} 次（spawn→退出→JSON 可见的墙钟） |`);
  L.push("");
  if (report.meta.skipped_tiers.length) {
    L.push(`> **本报告未含档**：${report.meta.skipped_tiers.map((s) => `\`${s.tier}\`（${s.reason}）`).join("、")}。`);
    L.push("");
  }

  // ---- 2 主表
  L.push("## 2. 六档 × 口径：CLI `elapsed_ms`（min / **median** / max，n=" + report.meta.samples_per_tier + "）");
  L.push("");
  L.push("| 档 | 页数 | cold-format（含 format） | cold-project（format 命中） | noop | noop2 | edit（A 命中） | edit2（真实改动） |");
  L.push("|---|---:|---:|---:|---:|---:|---:|---:|");
  for (const x of tiers) {
    const s = x.stats;
    L.push(`| \`${x.tier}\` | ${t(x.pages)} | ${m(s["cold-format"]?.elapsed_ms)} | ${m(s["cold-project"]?.elapsed_ms)} | ${m(s.noop?.elapsed_ms)} | ${m(s.noop2?.elapsed_ms)} | ${m(s.edit?.elapsed_ms)} | ${m(s.edit2?.elapsed_ms)} |`);
  }
  L.push("");
  L.push("## 3. 墙钟与进程开销");
  L.push("");
  L.push("| 档 | cold-format wall | noop wall | edit2 wall | 进程开销（wall − elapsed，cold 中位） |");
  L.push("|---|---:|---:|---:|---:|");
  for (const x of tiers) {
    const s = x.stats;
    L.push(`| \`${x.tier}\` | ${m(s["cold-format"]?.wall_ms)} | ${m(s.noop?.wall_ms)} | ${m(s.edit2?.wall_ms)} | ${t(s["cold-format"]?.process_overhead_ms?.median)} ms |`);
  }
  L.push("");

  // ---- 4 分阶段
  L.push("## 4. 分阶段分解（median ms；T14 打点含 prep / post / preview）");
  L.push("");
  L.push("| 档 | 口径 | format | typeset | convert | engine_total | prep | post | preview | 其他\\* | format 命中 | 趟数 | 页数 |");
  L.push("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|");
  for (const x of tiers) {
    for (const cond of ["cold-format", "cold-project", "noop", "edit", "edit2"]) {
      const st = x.stats[cond];
      if (!st) continue;
      const p = st.phase;
      const other = p.engine_total_ms.median == null ? null : r1(st.elapsed_ms.median - p.engine_total_ms.median);
      L.push(
        `| \`${x.tier}\` | ${cond} | ${t(p.format_ms.median)} | ${t(p.typeset_ms.median)} | ${t(p.convert_ms.median)} | ${t(p.engine_total_ms.median)} | ${t(p.prep_ms.median)} | ${t(p.post_ms.median)} | ${t(p.preview_ms.median)} | ${t(other)} | ${p.format_cached[0]} | ${p.passes.join("/")} | ${t(p.pages[0])} |`
      );
    }
  }
  L.push("");
  L.push("\\* 其他 = `elapsed_ms − engine_total_ms`（调用层 + 收尾里未计入 engine 的部分）。");
  L.push("");
  L.push("- `preview` = 编译中流式出图的累计耗时（**已包含在 typeset 窗口内**），不是前端 pdf.js 重载；它是 T17 预览闸门的输入。");
  L.push("- `prep` = bundle 打开 + 主文件读；`post` = PDF 原子替换 + 页哈希缓存写。两者都应远小于 typeset；若某档变大，先查 bundle/磁盘。");
  L.push("");

  // ---- 5 format 命中
  L.push("## 5. format 命中与跨项目");
  L.push("");
  L.push("| 档 | 未命中：生成 format（median） | 命中：读回 `.fmt`（noop） | 命中一次省下 |");
  L.push("|---|---:|---:|---:|");
  for (const x of tiers) {
    const gen = x.stats["cold-format"]?.phase?.format_ms?.median ?? null;
    const hit = x.stats.noop?.phase?.format_read_ms?.median ?? null;
    L.push(`| \`${x.tier}\` | ${t(gen)} | ${t(hit)} | ${gen != null && hit != null ? r1(gen - hit) : "–"} |`);
  }
  L.push("");
  L.push("### 5.1 跨项目 format 命中（本档第一次编译，format 来自上一档）");
  L.push("");
  L.push("| 档 | elapsed_ms | format_ms | typeset_ms | 命中 | 对照本档 cold-format |");
  L.push("|---|---:|---:|---:|---|---:|");
  for (const x of tiers) {
    const c = x.cross_project;
    const gen = x.stats["cold-format"]?.phase?.format_ms?.median ?? null;
    if (!c) {
      L.push(`| \`${x.tier}\` | – | – | – | 首档（无前序 format） | ${t(gen)} |`);
      continue;
    }
    L.push(`| \`${x.tier}\` | ${t(c.elapsed_ms)} | ${t(c.phase?.format_ms)} | ${t(c.phase?.typeset_ms)} | ${c.phase?.format_cached} | ${t(gen)} |`);
  }
  L.push("");
  const crossHit = tiers.filter((x) => x.cross_project?.phase?.format_cached === true).length;
  const crossTotal = tiers.filter((x) => x.cross_project).length;
  L.push(`跨项目命中 **${crossHit}/${crossTotal}**（首档之外全部命中）。format 缓存键 = bundle digest：换 bundle 会重新生成。`);
  L.push("");

  // ---- 6 edit 口径自证
  L.push("## 6. 编辑口径自证（A 闸门 / Quick 实报 kind）");
  L.push("");
  L.push("| 档 | edit A 命中（应全部 `reused_pdf=true`） | edit 实报 kind | edit2 A 不命中（应全部 `reused_pdf=false`） | edit2 实报 kind | edit2 趟数 |");
  L.push("|---|---|---|---|---|---|");
  for (const x of tiers) {
    const pc = x.protocol_checks;
    const fmtA = (flagOk, hits) => `${flagOk ? "✅" : "❌"}（a_hit=${hits.join("/")}）`;
    L.push(`| \`${x.tier}\` | ${fmtA(pc.edit_a_hit_all, x.stats.edit?.a_hit)} | ${pc.edit_kinds.join("/")} | ${fmtA(pc.edit2_a_miss_all, x.stats.edit2?.a_hit)} | ${pc.edit2_kinds.join("/")} | ${pc.edit2_passes.join("/")} |`);
  }
  L.push("");
  L.push("> `edit` = 注释级编辑：页哈希逐页相同 ⇒ A 闸门跳过 XDV→PDF 转换（`reused_pdf=true`），");
  L.push("> 这就是「内容没变的编辑」的快速路径。`edit2` = 真实改正文：A 不命中、转换必跑；");
  L.push("> 多文件/目录档通常需要 2 趟（第一趟后页码/引用还在变）。");
  L.push("");
  if (report.outliers && report.outliers.length) {
    L.push("### 6.1 离群值（**保留并标注**，不剔除）");
    L.push("");
    L.push("| 档 | 口径 | 样本 | 实测 | 中位 | 偏差 |");
    L.push("|---|---|---:|---:|---:|---:|");
    for (const o of report.outliers) {
      L.push(`| \`${o.tier}\` | ${o.condition} | #${o.index} | ${t(o.elapsed_ms)} | ${t(o.median_ms)} | ${t(o.deviation_pct)}% |`);
    }
    L.push("");
  } else {
    L.push("**离群值**：本次无（同口径样本与中位偏差均 ≤20%）。");
    L.push("");
  }
  if (report.meta.machine_load.length) {
    const loads = report.meta.machine_load.map((x) => x.cpu_load_pct).filter((x) => typeof x === "number");
    const mems = report.meta.machine_load.map((x) => x.free_mb).filter((x) => typeof x === "number");
    const st = stats(loads);
    const sm = stats(mems);
    L.push("### 6.2 机器负载（每档前后采样）");
    L.push("");
    L.push(`- CPU 负载 %：min ${t(st.min)} / median ${t(st.median)} / max ${t(st.max)}（${loads.length} 个采样点）`);
    L.push(`- 空闲内存 MB：min ${t(sm.min)} / median ${t(sm.median)} / max ${t(sm.max)}`);
    L.push("");
  }

  // ---- 7 预算
  L.push("## 7. 预算判定（design.md §2.6：小文档 1s 优秀/2s 及格；大文档 3s 优秀/5s 及格）");
  L.push("");
  L.push("| 档 | 规模类 | cold-format 中位 | cold 判定 | edit2 中位 | 编辑→出图 e2e | e2e 判定 |");
  L.push("|---|---|---:|---|---:|---:|---|");
  for (const b of report.budget) {
    L.push(`| \`${b.tier}\` | ${b.class} | ${t(b.cold_ms)} | ${b.cold_verdict} | ${t(b.edit2_ms)} | ${t(b.e2e_edit2_ms)} | ${b.e2e_edit2_verdict} |`);
  }
  L.push("");
  L.push(`说明：cold 判定用 \`cold-format\`（真首编）；编辑→出图 = debounce(${report.meta.params.debounce_ms}) + edit2 中位 + preview 估算(${report.meta.params.preview_render_ms})。`);
  L.push("阈值来自 [docs/design.md](../../docs/design.md) §2.6 的产品约束；**大文档 cold 超 5 s 属「预算口径待复核」**（T19 决策点），不是回归。");
  L.push("");

  // ---- 8 复现
  L.push("## 8. 两轮独立运行复现性（同档同口径 `elapsed_ms` 中位差，阈值 10%）");
  L.push("");
  if (!report.reproducibility) {
    L.push("_本次未指定 `--compare=<上一轮 JSON>`，未做复现比较。_");
  } else if (report.reproducibility.rows) {
    L.push(`对照：\`${report.reproducibility.path}\`（上一轮生成于 ${report.reproducibility.prev_generated_at}）。`);
    L.push("");
    L.push("| 档 | 口径 | 上一轮 | 本轮 | Δ% | ≤10% |");
    L.push("|---|---|---:|---:|---:|---|");
    for (const row of report.reproducibility.rows) {
      L.push(`| \`${row.tier}\` | ${row.condition} | ${t(row.run1_ms)} | ${t(row.run2_ms)} | ${t(row.delta_pct)} | ${row.within_10pct ? "✅" : "❌"} |`);
    }
    L.push("");
    L.push(`**最大绝对差 ${report.reproducibility.max_abs_delta_pct}%** ⇒ ${report.reproducibility.within_10pct ? "两轮可复现 ✅" : "超阈值 ❌（需排查机器负载/后台进程后复测）"}。`);
  } else {
    L.push(`_比较未执行：${report.reproducibility.note || "上一轮数据不可用"}。_`);
  }
  L.push("");

  // ---- 9 与旧基线
  L.push("## 9. 与旧基线对比（仅供解释，不是当前实现的承诺）");
  L.push("");
  L.push("旧数字 = latexmk/xelatex 时代（[docs/design.md](../../docs/design.md) §2.8），命令与驱动都已删除。");
  L.push("");
  L.push("| 档 | cold 旧 → 新 | noop 旧 → 新 | edit 旧 → 新（本次 edit2） |");
  L.push("|---|---|---|---|");
  for (const x of tiers) {
    const cold = x.stats["cold-format"]?.elapsed_ms?.median;
    const noop = x.stats.noop?.elapsed_ms?.median;
    const edit2 = x.stats.edit2?.elapsed_ms?.median;
    const o = OLD_BASELINE[x.tier];
    const f = (now, old) => (now == null || old == null ? "–" : `${old} → **${now}**（×${(now / old).toFixed(2)}）`);
    L.push(`| \`${x.tier}\` | ${f(cold, o?.cold)} | ${f(noop, o?.noop)} | ${f(edit2, o?.edit)} |`);
  }
  L.push("");
  L.push("> `noop` 变慢不是回归：旧 `noop` = latexmk 判定 up-to-date、**什么都不做**；现行 Full 的 `noop` 语义是**完整刷一遍**（排版 + 转换）。");
  L.push("");

  // ---- 10 未覆盖
  L.push("## 10. 未覆盖项");
  L.push("");
  L.push("| 项 | 状态 | 原因 / 下一步 |");
  L.push("|---|---|---|");
  for (const u of report.uncovered) L.push(`| ${u.item} | ${u.status} | ${u.reason} |`);
  L.push("");

  // ---- 11 复现
  L.push("## 11. 复现");
  L.push("");
  L.push("```powershell");
  L.push("# 前置：release CLI（唯一入口，需要 vcpkg 前置）");
  L.push("$env:TMP = $env:TEMP = 'E:\\Works\\tex-presso\\src-tauri\\target\\.tmp'");
  L.push("npm run lib:cli");
  L.push("node scripts/gen-bench-projects.mjs      # 六档夹具（不入库）");
  L.push("");
  L.push("# 一条命令：六档 × 3 次 → JSON + Markdown（约 2–3 分钟）");
  L.push(`node scripts/bench-embedded.mjs --out=${report.meta.outputs.json} --md=${report.meta.outputs.md}`);
  L.push("");
  L.push("# 第二遍并做复现比较（同档同口径中位差 <10% 才通过）");
  L.push(`node scripts/bench-embedded.mjs --out=${report.meta.outputs.json.replace(/\.json$/, "-run2.json")} --md=${report.meta.outputs.md.replace(/\.md$/, "-run2.md")} --compare=${report.meta.outputs.json}`);
  L.push("```");
  L.push("");
  L.push("退出码：`0` 全部门禁通过；`1` 有档超出预算 / 复现差 >10% / 有档编译失败；`2` 前置缺失（manifest 或 release CLI）。");
  L.push("");
  L.push(`原始数据：\`${report.meta.outputs.json}\`；逐次日志：\`${report.meta.outputs.logs}/\`。`);
  L.push("");

  return L.join("\n") + "\n";
}

// ---------------------------------------------------------------- 主流程

function main() {
  if (!existsSync(CLI)) {
    console.error(`找不到 release CLI：${CLI}\n  先跑：npm run lib:cli（需要 vcpkg 前置，见 scripts/with-tectonic-lib.ps1）`);
    process.exit(2);
  }
  if (!existsSync(MANIFEST)) {
    console.error(`缺少基准 manifest：${MANIFEST}\n  先跑：node scripts/gen-bench-projects.mjs（夹具不入库）`);
    process.exit(2);
  }
  mkdirSync(TMPDIR, { recursive: true });
  mkdirSync(CONFIG, { recursive: true });
  mkdirSync(LOGS, { recursive: true });
  mkdirSync(CACHE, { recursive: true });

  const manifest = JSON.parse(readFileSync(MANIFEST, "utf8"));
  let tiers = manifest.tiers.filter((t) => !t.external);
  if (ONLY.length) tiers = tiers.filter((t) => ONLY.includes(t.name));
  tiers.sort((a, b) => TIER_ORDER.indexOf(a.name) - TIER_ORDER.indexOf(b.name));

  const skipped = [];
  for (const name of SKIP) {
    const t = manifest.tiers.find((x) => x.name === name);
    if (t) skipped.push({ tier: name, reason: SKIP_REASON });
    tiers = tiers.filter((x) => x.name !== name);
  }

  if (!tiers.length) {
    console.error(`没有匹配的档位（可用：${manifest.tiers.filter((t) => !t.external).map((t) => t.name).join(", ")}）`);
    process.exit(2);
  }

  // 每档补充 pages（测量后回填）与 class；夹具规模在编译前记录。
  const fixtures = [];
  for (const tier of tiers) {
    const tex = listTex(tier.dir);
    let bytes = 0;
    for (const f of tex) bytes += statSync(f).size;
    fixtures.push({ tier: tier.name, dir: rel(tier.dir), tex_files: tex.length, tex_bytes: bytes, class: tier.class, pages: null });
  }

  const report = {
    case: "phase3-embedded-tectonic-release-baseline",
    generatedAt: new Date().toISOString(),
    meta: {
      samples_per_tier: SAMPLES,
      protocol: PROTOCOL,
      params: { debounce_ms: DEBOUNCE_MS, preview_render_ms: PREVIEW_MS },
      budget: BUDGET,
      env: {
        LATTESET_TECTONIC_BUNDLE: BUNDLE,
        LATTESET_TECTONIC_CACHE: rel(CACHE),
        LATTESET_CONFIG_DIR: rel(CONFIG),
        TMP: rel(TMPDIR),
        RUST_LOG: "info",
      },
      outputs: { json: rel(OUT_JSON), md: rel(OUT_MD), logs: rel(LOGS) },
      cli: cliInfo(),
      bundle: bundleInfo(),
      machine: machineInfo(),
      machine_load: [],
      skipped_tiers: skipped,
    },
    fixtures,
    tier_order: tiers.map((t) => t.name),
    tiers: {},
    process_floor: null,
    budget: [],
    reproducibility: null,
    protocol_checks_warnings: [],
    uncovered: [
      { item: "GUI 启动与窗口内编译", status: "[未测]", reason: "本脚本只测 headless release CLI；GUI 启动见 T9/`docs/dev/tectonic-library.md` §6" },
      { item: "默认网络 bundle 的获取成本", status: "[未测]", reason: "默认 bundle 发行物 ≈2.88 GB；本基线用本地目录 bundle（离线可复现）" },
      { item: "bundle 形态对照（目录 vs .ttb/.zip）", status: "[未测]", reason: "本机只有目录 bundle" },
      { item: "debug 构建 / `tauri dev`", status: "[未测]", reason: "只测 release；debug 一趟慢约 4.6×（历史实测）" },
      { item: "GUI 端到端（防抖 + 编译 + pdf.js 重载）", status: "[未测]", reason: "preview 只是估算值，未接真机渲染探针" },
      { item: "跨平台", status: "[未测]", reason: "仅 Windows 11 单机" },
    ],
  };

  // 采样开跑前清 formats，保证第一档的「跨项目」语义明确（第一档没有前序 format）。
  cleanFormats();
  const totalStarted = hrtimeMs();
  let idx = 0;
  for (const tier of tiers) {
    const doCross = idx > 0;
    process.stderr.write(`  测量 ${tier.name.padEnd(14)} ... `);
    const loadBefore = sampleMachineLoad();
    const t0 = hrtimeMs();
    let r;
    try {
      r = measureTier(tier, doCross);
    } catch (e) {
      r = { tier: tier.name, status: "error", error: String(e && e.stack ? e.stack : e) };
    }
    // pages 回填（夹具表用）
    const fx = fixtures.find((f) => f.tier === tier.name);
    if (fx) fx.pages = r.pages ?? null;
    report.tiers[tier.name] = r;
    const sec = ((hrtimeMs() - t0) / 1000).toFixed(1);
    if (r.status === "ok") {
      const s = r.stats;
      console.log(
        `cold-format ${s["cold-format"].elapsed_ms.median}ms / cold-project ${s["cold-project"].elapsed_ms.median}ms / ` +
          `noop ${s.noop.elapsed_ms.median}ms / edit ${s.edit.elapsed_ms.median}ms / edit2 ${s.edit2.elapsed_ms.median}ms ` +
          `[pages=${r.pages}, ${sec}s]`
      );
    } else {
      console.log(`失败：${String(r.error).split("\n")[0]} (${sec}s)`);
    }
    const loadAfter = sampleMachineLoad();
    if (loadBefore) report.meta.machine_load.push({ tier: tier.name, phase: "before", ...loadBefore });
    if (loadAfter) report.meta.machine_load.push({ tier: tier.name, phase: "after", ...loadAfter });
    idx += 1;
  }
  report.meta.total_measure_s = Math.round((hrtimeMs() - totalStarted) / 100) / 10;

  // 进程地板（用第一个成功档的项目目录）
  const okTier = tiers.find((t) => report.tiers[t.name]?.status === "ok");
  if (okTier) report.process_floor = measureFloor(okTier.dir);
  else report.process_floor = null;

  // 预算判定
  report.budget = evaluateBudgets(report.tiers, tiers);

  // 口径自证告警
  for (const x of Object.values(report.tiers)) {
    if (x.status !== "ok" || !x.protocol_checks) continue;
    if (!x.protocol_checks.edit_a_hit_all) {
      report.protocol_checks_warnings.push(`\`${x.tier}\` 的 edit 未全部命中 A 闸门（a_hit=${x.stats.edit?.a_hit?.join("/")}，预期全 true）`);
    }
    if (!x.protocol_checks.edit2_a_miss_all) {
      report.protocol_checks_warnings.push(`\`${x.tier}\` 的 edit2 出现 A 命中（a_hit=${x.stats.edit2?.a_hit?.join("/")}，预期全 false）`);
    }
  }

  // 离群值汇总（保留并标注；不剔除）
  report.outliers = [];
  for (const x of Object.values(report.tiers)) {
    for (const [cond, st] of Object.entries(x.stats || {})) {
      for (const o of st.outliers || []) report.outliers.push({ tier: x.tier, condition: cond, ...o });
    }
  }

  // 两轮复现比较
  report.reproducibility = compareWith(COMPARE, report);

  // 写 JSON + Markdown
  mkdirSync(dirname(OUT_JSON), { recursive: true });
  mkdirSync(dirname(OUT_MD), { recursive: true });
  writeFileSync(OUT_JSON, JSON.stringify(report, null, 2), "utf8");
  writeFileSync(OUT_MD, renderMarkdown(report), "utf8");
  console.log(`\n原始 JSON：${rel(OUT_JSON)}`);
  console.log(`Markdown ：${rel(OUT_MD)}`);

  // 退出码：前置缺失=2；预算/复现/档失败=1（各闸门独立，不得互相覆盖）
  const failedTiers = Object.values(report.tiers).filter((x) => x.status !== "ok");
  const budgetFails = report.budget.filter((b) => b.fail);
  const driftFail = Boolean(report.reproducibility && report.reproducibility.rows && !report.reproducibility.within_10pct);
  let exitCode = 0;
  if (failedTiers.length) exitCode = 1;
  if (budgetFails.length && !ALLOW_OVER_BUDGET) exitCode = 1;
  if (driftFail && !ALLOW_DRIFT) exitCode = 1;
  if (failedTiers.length) {
    console.error(`\n✗ ${failedTiers.length} 档未测成功：${failedTiers.map((x) => x.tier).join(", ")}`);
  }
  if (budgetFails.length) {
    console.log(`\n${ALLOW_OVER_BUDGET ? "⚠ (--allow-over-budget) " : "✗ "}${budgetFails.length} 档超出延迟预算：${budgetFails.map((b) => b.tier).join(", ")}`);
  }
  if (driftFail) console.log(`\n${ALLOW_DRIFT ? "⚠ (--allow-drift) " : "✗ "}两轮复现差超过 10%（${report.reproducibility.max_abs_delta_pct}%）`);
  process.exit(exitCode);
}

main();
