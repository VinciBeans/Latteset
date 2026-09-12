// ㉘ 收益评估：编辑期「单趟 xelatex」能否替代「完整 latexmk」（roadmap ㉘ 第 1 步）。
//
// 为什么单独一个脚本：这是**一次性的收益评估**，不是回归工具。日常回归用 `bench.mjs`
// （六档预算判定）；本脚本回答一个具体问题——若编辑时只跑单趟、停手后再跑完整 latexmk 收敛，
// 编辑到出图的延迟能降多少？
//
// 口径：
//   基线 A = 完整 latexmk（与 runner 一致：cwd=项目根、`-xelatex -outdir=tmp -synctex=1 -interaction=nonstopmode`）
//   候选 B = 单趟 xelatex（`-interaction=nonstopmode -synctex=1 -output-directory=tmp`）——一次 invoke 直接产出 PDF
//   两者都是"先完整构建建立 aux 状态 → 追加一行注释 → 计时"，编辑内容等价。
//   每档 N 次采样取中位数；**A/B 顺序按采样序号交替**，抵消缓存与顺序偏差。
//
// 判定（roadmap ㉘ 预设阈值）：**中位节省 < 20% 则不做**——不值得引入"引用显示为旧值"的状态不一致。
//
// 用法：node scripts/bench-single-pass.mjs [--samples=3] [--tiers=tiny,multifile] [--timeout=600]

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync, rmSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const BENCH_ROOT = join(ROOT, "test_file", "projects", "bench");
const MANIFEST = join(BENCH_ROOT, "bench-manifest.json");
const RESEARCH_DIR = join(ROOT, "test_file", "research");

const arg = (n, d) => {
  const hit = process.argv.find((a) => a.startsWith(`--${n}=`));
  return hit ? hit.slice(n.length + 3) : d;
};
const SAMPLES = Number(arg("samples", "3"));
const TIMEOUT_MS = Number(arg("timeout", "600")) * 1000;
const ONLY = arg("tiers", "").split(",").map((s) => s.trim()).filter(Boolean);
const THRESHOLD_PCT = 20;

function run(cmd, args, cwd) {
  const t0 = process.hrtime.bigint();
  const r = spawnSync(cmd, args, { cwd, stdio: "ignore", timeout: TIMEOUT_MS, shell: false });
  const ms = Number(process.hrtime.bigint() - t0) / 1e6;
  if (r.error?.code === "ETIMEDOUT") {
    // latexmk 在 Windows 是 latexmk.exe → runscript.tlu → perl 三层，只杀直接子进程会留孤儿
    for (const img of ["perl.exe", "xelatex.exe", "latexmk.exe"]) {
      spawnSync("taskkill", ["/F", "/IM", img], { stdio: "ignore", shell: false });
    }
    return { ms, code: -1, timeout: true };
  }
  return { ms, code: r.status ?? -1, timeout: false };
}

const fullLatexmk = (t) =>
  run("latexmk", ["-xelatex", "-outdir=tmp", "-synctex=1", "-interaction=nonstopmode", t.main], t.dir);
const singlePass = (t) =>
  run("xelatex", ["-interaction=nonstopmode", "-synctex=1", "-output-directory=tmp", t.main], t.dir);

function cleanBuild(t) {
  rmSync(join(t.dir, "tmp"), { recursive: true, force: true });
  rmSync(join(t.dir, t.main.replace(/\.tex$/, ".pdf")), { force: true });
}

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};

function measure(tier) {
  const target = join(tier.dir, tier.editTarget);
  if (!existsSync(target)) return { tier: tier.name, skipped: `缺少 editTarget: ${tier.editTarget}` };
  const original = readFileSync(target, "utf8");

  // 建立 aux 状态（模拟"用户已有构建产物，正在编辑"）
  cleanBuild(tier);
  const settle = fullLatexmk(tier);
  if (settle.code !== 0) return { tier: tier.name, failed: `初始完整构建失败（exit ${settle.code}）` };

  const fulls = [];
  const singles = [];
  for (let i = 0; i < SAMPLES; i++) {
    const edit = `\n% single-pass probe ${i}\n`;
    // 顺序交替：偶数次 A→B，奇数次 B→A（抵消缓存/顺序偏差）
    const order = i % 2 === 0 ? ["A", "B"] : ["B", "A"];
    for (const arm of order) {
      writeFileSync(target, original + edit, "utf8");
      const r = arm === "A" ? fullLatexmk(tier) : singlePass(tier);
      writeFileSync(target, original, "utf8");
      if (r.code !== 0) return { tier: tier.name, failed: `${arm} 失败（exit ${r.code}${r.timeout ? " 超时" : ""}）` };
      (arm === "A" ? fulls : singles).push(r.ms);
    }
  }

  const full_ms = median(fulls);
  const single_ms = median(singles);
  const saving_ms = full_ms - single_ms;
  const saving_pct = (saving_ms / full_ms) * 100;
  return {
    tier: tier.name,
    full_ms: Math.round(full_ms),
    single_ms: Math.round(single_ms),
    saving_ms: Math.round(saving_ms),
    saving_pct: Math.round(saving_pct * 10) / 10,
    verdict: saving_pct >= THRESHOLD_PCT ? "值得做" : "不值得",
    samples: SAMPLES,
  };
}

function main() {
  if (!existsSync(MANIFEST)) {
    console.error("缺少 manifest，请先运行：node scripts/gen-bench-projects.mjs");
    process.exit(1);
  }
  let tiers = JSON.parse(readFileSync(MANIFEST, "utf8")).tiers.filter((t) => !t.external);
  if (ONLY.length) tiers = tiers.filter((t) => ONLY.includes(t.name));

  console.log(`㉘ 收益评估：单趟 xelatex vs 完整 latexmk（${tiers.length} 档 × ${SAMPLES} 次，A/B 交替）`);
  console.log(`判定阈值：中位节省 ≥ ${THRESHOLD_PCT}% 才值得做\n`);

  const results = [];
  for (const t of tiers) {
    process.stdout.write(`  测量 ${t.name.padEnd(14)} ... `);
    const r = measure(t);
    results.push(r);
    if (r.failed) console.log(`失败：${r.failed}`);
    else if (r.skipped) console.log(`跳过：${r.skipped}`);
    else
      console.log(
        `完整 ${r.full_ms}ms / 单趟 ${r.single_ms}ms → 省 ${r.saving_ms}ms (${r.saving_pct}%)  [${r.verdict}]`
      );
  }

  const ok = results.filter((r) => !r.failed && !r.skipped);
  console.log("\n| 档 | 完整 latexmk | 单趟 xelatex | 节省 | 节省% | 判定 |");
  console.log("|---|---:|---:|---:|---:|---|");
  for (const r of ok) {
    console.log(`| \`${r.tier}\` | ${r.full_ms}ms | ${r.single_ms}ms | ${r.saving_ms}ms | ${r.saving_pct}% | ${r.verdict} |`);
  }
  const avg = ok.length ? ok.reduce((a, r) => a + r.saving_pct, 0) / ok.length : 0;
  const worth = ok.filter((r) => r.verdict === "值得做").length;
  console.log(
    `\n平均节省 ${avg.toFixed(1)}%；${worth}/${ok.length} 档达阈值(${THRESHOLD_PCT}%) → 整体判定：${avg >= THRESHOLD_PCT ? "值得做" : "不值得"}`
  );

  mkdirSync(RESEARCH_DIR, { recursive: true });
  const out = join(RESEARCH_DIR, "bench-single-pass.json");
  writeFileSync(
    out,
    JSON.stringify(
      { generatedAt: new Date().toISOString(), thresholdPct: THRESHOLD_PCT, samples: SAMPLES, avgSavingPct: Math.round(avg * 10) / 10, results },
      null,
      2
    ),
    "utf8"
  );
  console.log(`原始报告：${out}`);
}

main();
