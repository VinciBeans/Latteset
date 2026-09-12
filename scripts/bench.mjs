// 性能基准测量（roadmap ③）：一条命令产出「冷编译 / 空跑 / 编辑触发 / 端到端」报告。
//
// 口径严格对齐产品：调用方式与 runner 一致（cwd=项目根、`-xelatex -outdir=tmp -synctex=1
// -interaction=nonstopmode`），不额外加产品没传的参数。
//
// 三个耗时指标：
//   cold_ms  删 tmp/ 与 PDF 后首跑        —— 「首编总耗时」（㉉5 超时默认值的直接输入）
//   noop_ms  紧接再跑（latexmk 判定最新）  —— 验证增量语义没退化
//   edit_ms  改动一个源文件后重跑          —— 编辑触发的真实代价（ADR-0005：整份单遍）
//
// 端到端（与 design.md §延迟预算 对齐）：
//   e2e_ms = debounce + edit_ms + preview
//   其中 debounce 默认 500ms（产品默认）、preview 默认 100ms（design.md 实测的小文档重载量级）；
//   两者可用 --debounce/--preview 覆盖，真机 MCP 跑出的实测值应通过参数传入。
//
// 预算判定用的是 **design.md 的口径**：按 cold_ms 分类（≤2s 小文档 / >5s 大文档），
// 而非 manifest 里的意图分类；两者不一致时报告会标出。
//   小文档：1s 优秀 / 2s 及格      大文档：3s 优秀 / 5s 及格
//
// 用法：
//   node scripts/bench.mjs                          # 全档，3 次取中位数
//   node scripts/bench.mjs --tiers=tiny,multifile   # 只跑指定档
//   node scripts/bench.mjs --samples=5 --preview=118
//   node scripts/bench.mjs --force-gen              # 先重建 fixture
//
// 产物：test_file/research/bench-report.json（原始，gitignore）+ stdout 的 Markdown 摘要
// 退出码：任一档 FAIL → 1（供本地回归/将来 CI 用）
//
// 注意（沙箱）：本脚本用 `stdio: 'ignore'` 启动 latexmk——不用管道，因此**不需要提权**；
// 失败原因从 tmp/<stem>.log 读，不依赖子进程输出。

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const BENCH_ROOT = join(ROOT, "test_file", "projects", "bench");
const MANIFEST = join(BENCH_ROOT, "bench-manifest.json");
const RESEARCH_DIR = join(ROOT, "test_file", "research");

// ---------------------------------------------------------------- 参数

function arg(name, dflt) {
  const hit = process.argv.find((a) => a.startsWith(`--${name}=`));
  return hit ? hit.slice(name.length + 3) : dflt;
}
const flag = (name) => process.argv.includes(`--${name}`);

const SAMPLES = Number(arg("samples", "3"));
const DEBOUNCE_MS = Number(arg("debounce", "500"));
const PREVIEW_MS = Number(arg("preview", "100"));
const TIMEOUT_MS = Number(arg("timeout", "600")) * 1000;
const NO_WARMUP = flag("no-warmup");
const ONLY = arg("tiers", "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);

// 预算（design.md §延迟预算）：按 cold 耗时分类
const BUDGET = {
  small: { excellent: 1000, pass: 2000 },
  large: { excellent: 3000, pass: 5000 },
};

// ---------------------------------------------------------------- latexmk

/** 与 src-tauri/runner 一致的一次编译；返回 {ms, code, timeout}。 */
function compileOnce(tier) {
  const t0 = process.hrtime.bigint();
  const r = spawnSync(
    "latexmk",
    ["-xelatex", "-outdir=tmp", "-synctex=1", "-interaction=nonstopmode", tier.main],
    { cwd: tier.dir, stdio: "ignore", timeout: TIMEOUT_MS, shell: false }
  );
  const ms = Number(process.hrtime.bigint() - t0) / 1e6;
  // spawnSync 超时会杀掉**直接子进程**，但 latexmk 在 Windows 上是
  // `latexmk.exe → runscript.tlu → perl` 三层：孙进程 perl 可能残留，需显式清理。
  if (r.error?.code === "ETIMEDOUT") {
    try {
      spawnSync("taskkill", ["/F", "/IM", "perl.exe"], { stdio: "ignore", shell: false });
      spawnSync("taskkill", ["/F", "/IM", "xelatex.exe"], { stdio: "ignore", shell: false });
      spawnSync("taskkill", ["/F", "/IM", "latexmk.exe"], { stdio: "ignore", shell: false });
    } catch { /* best-effort */ }
    return { ms, code: -1, timeout: true };
  }
  return { ms, code: r.status ?? -1, error: r.error?.code, timeout: false };
}

/** 失败时从 .log 取首条 `!` 行（不依赖子进程 stdout）。 */
function logTail(tier) {
  const stem = tier.main.replace(/\.tex$/, "");
  const p = join(tier.dir, "tmp", `${stem}.log`);
  if (!existsSync(p)) return "(无 log)";
  const m = readFileSync(p, "utf8").match(/^!.*$/m);
  return m ? m[0].trim() : "(log 中无 ! 行)";
}

function cleanBuild(tier) {
  rmSync(join(tier.dir, "tmp"), { recursive: true, force: true });
  const stem = tier.main.replace(/\.tex$/, "");
  rmSync(join(tier.dir, `${stem}.pdf`), { force: true });
}

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};

// ---------------------------------------------------------------- 单档测量

function measure(tier) {
  const target = join(tier.dir, tier.editTarget);
  if (!existsSync(target)) {
    return { tier: tier.name, skipped: `缺少 editTarget: ${tier.editTarget}` };
  }
  const original = readFileSync(target, "utf8");

  // warm-up：排掉文件系统冷缓存与首次字体缓存（不计入）。
  // `--no-warmup` 用于真实模板档——它们单次编译可达分钟级，预热代价过高。
  if (!NO_WARMUP) {
    cleanBuild(tier);
    compileOnce(tier);
  }

  const cold = [];
  const noop = [];
  const edit = [];
  for (let i = 0; i < SAMPLES; i++) {
    // cold：必须每轮都清干净
    cleanBuild(tier);
    const c = compileOnce(tier);
    if (c.timeout) return { tier: tier.name, timeout: true, failed: `cold 编译超时（>${TIMEOUT_MS / 1000}s 未收敛）` };
    if (c.code !== 0) return { tier: tier.name, failed: `cold 编译失败（exit ${c.code}）: ${logTail(tier)}` };
    cold.push(c.ms);

    // noop：紧接着再跑（latexmk 应判定 up-to-date）
    const n = compileOnce(tier);
    if (n.code !== 0) return { tier: tier.name, failed: `noop 编译失败（exit ${n.code}）: ${logTail(tier)}` };
    noop.push(n.ms);

    // edit：改一个源文件后重跑，测完还原（保证下一轮从同一基线开始）
    writeFileSync(target, `${original}\n% bench edit ${i}\n`, "utf8");
    const e = compileOnce(tier);
    writeFileSync(target, original, "utf8");
    if (e.code !== 0) return { tier: tier.name, failed: `edit 编译失败（exit ${e.code}）: ${logTail(tier)}` };
    edit.push(e.ms);
  }

  const cold_ms = median(cold);
  const edit_ms = median(edit);
  const e2e_ms = DEBOUNCE_MS + edit_ms + PREVIEW_MS;
  // 分类用 **manifest 的文档规模意图**，不用 cold_ms。
  // 为什么不用 design.md 的字面口径（按 cold ≤2s / >5s 分类）：实测发现 tiny（6 行文档）的
  // cold 就有 2.4s——固定开销（latexmk perl 启动 + xelatex 启动 + 字体加载）本身就超过 2s，
  // 于是"小文档"会被判成大文档、套用更宽松的预算，得出"6 行文档 EXCELLENT"的荒谬结论。
  // 规模意图分类才有意义；cold_ms 作为独立指标记录（它同时是 ㉉5 首编超时的输入）。
  const budget = BUDGET[tier.class] ?? BUDGET.large;
  const verdict = e2e_ms <= budget.excellent ? "EXCELLENT" : e2e_ms <= budget.pass ? "PASS" : "FAIL";
  // 反直觉记录：意图 small 但 cold 已 >2s（固定开销吃掉预算）时标出，供预算口径复核
  const coldContradictsIntent = tier.class === "small" && cold_ms > 2000;

  return {
    tier: tier.name,
    intendedClass: tier.class,
    coldContradictsIntent,
    cold_ms: Math.round(cold_ms),
    noop_ms: Math.round(median(noop)),
    edit_ms: Math.round(edit_ms),
    e2e_ms: Math.round(e2e_ms),
    budget,
    verdict,
    samples: SAMPLES,
  };
}

// ---------------------------------------------------------------- 主流程

function main() {
  if (flag("force-gen")) {
    const g = spawnSync(process.execPath, [join(HERE, "gen-bench-projects.mjs"), "--force"], { stdio: "inherit" });
    if (g.status !== 0) process.exit(g.status ?? 1);
  }
  if (!existsSync(MANIFEST)) {
    console.error("缺少 manifest，请先运行：node scripts/gen-bench-projects.mjs");
    process.exit(1);
  }
  const manifest = JSON.parse(readFileSync(MANIFEST, "utf8"));
  let tiers = manifest.tiers;
  // 选择顺序：显式 --tiers= 最优先（可以点名 real 档）；否则默认排除 external 档
  // （真实模板依赖本机 TeX Live 样例且编译很慢），需 --with-real 才纳入。
  if (ONLY.length) {
    tiers = tiers.filter((t) => ONLY.includes(t.name));
  } else if (!flag("with-real")) {
    tiers = tiers.filter((t) => !t.external);
  }
  if (!tiers.length) {
    console.error(
      `没有匹配的档位（可用：${manifest.tiers.map((t) => t.name + (t.external ? "[real]" : "")).join(", ")}）`
    );
    process.exit(1);
  }

  console.log(`基准测量：${tiers.length} 档 × ${SAMPLES} 次取中位数`);
  console.log(`端到端口径：debounce ${DEBOUNCE_MS}ms + edit + preview ${PREVIEW_MS}ms\n`);

  const results = [];
  for (const tier of tiers) {
    process.stdout.write(`  测量 ${tier.name.padEnd(14)} ... `);
    const r = measure(tier);
    results.push(r);
    if (r.failed) console.log(`失败：${r.failed}`);
    else if (r.skipped) console.log(`跳过：${r.skipped}`);
    else
      console.log(
        `cold ${r.cold_ms}ms / noop ${r.noop_ms}ms / edit ${r.edit_ms}ms → e2e ${r.e2e_ms}ms  [${r.verdict}]`
      );
  }

  // ---- 报告
  const ok = results.filter((r) => !r.failed && !r.skipped);
  const rows = [
    "| 档 | 冷编译 | 空跑 | 编辑触发 | 端到端(估) | 分类 | 判定 |",
    "|---|---:|---:|---:|---:|---|---|",
    ...ok.map(
      (r) =>
        `| \`${r.tier}\` | ${r.cold_ms}ms | ${r.noop_ms}ms | ${r.edit_ms}ms | ${r.e2e_ms}ms | ${r.intendedClass}${r.coldContradictsIntent ? " ⚠️cold>2s" : ""} | ${r.verdict} |`
    ),
  ];
  const md = rows.join("\n");
  console.log(`\n${md}`);
  console.log(
    `\n预算口径（design.md）：小文档 1s 优秀 / 2s 及格；大文档 3s 优秀 / 5s 及格；端到端 = debounce + edit + preview。`
  );
  if (ok.some((r) => r.coldContradictsIntent)) {
    console.log(
      `⚠️ 有档位「意图小文档但冷编译已 >2s」——说明固定开销（latexmk/xelatex 启动 + 字体加载）本身就吃掉了小文档预算，预算口径需复核。`
    );
  }

  const failCount = ok.filter((r) => r.verdict === "FAIL").length;
  const report = {
    generatedAt: new Date().toISOString(),
    platform: `${process.platform} ${process.arch}`,
    node: process.version,
    params: { SAMPLES, DEBOUNCE_MS, PREVIEW_MS, TIMEOUT_MS },
    budget: BUDGET,
    results,
    failCount,
  };
  mkdirSync(RESEARCH_DIR, { recursive: true });
  const outJson = join(RESEARCH_DIR, "bench-report.json");
  writeFileSync(outJson, JSON.stringify(report, null, 2), "utf8");
  console.log(`\n原始报告：${outJson}`);
  if (failCount) console.log(`⚠️ ${failCount} 档超出延迟预算（FAIL）`);
  process.exit(failCount ? 1 : 0);
}

main();
