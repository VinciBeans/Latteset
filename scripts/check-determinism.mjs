// 构建确定性检查（roadmap ㉚）：同一份源码连跑两次，PDF 是否**逐字节**相同？
//
// 为什么重要：任何"输出 diff / 只重排变化页"的优化，前提都是"内容没变 → 字节没变"。
// 否则每次编译 PDF 都变，diff 分不清"真的改了"还是"引擎抖了一下"。
//
// 用法：
//   node scripts/check-determinism.mjs                  # 默认三档样本 × Full/Quick 两条路径
//   node scripts/check-determinism.mjs --without-epoch   # 不固定 SOURCE_DATE_EPOCH（复现问题）
//   node scripts/check-determinism.mjs --projects=a,b --gap=6000
//
// 2026-09 基线（本机 TeX Live 2026 / xelatex）：
//   - 不固定 SOURCE_DATE_EPOCH：**两份 PDF 不同**，差异只有 trailer 的 `/ID`（实测 64 字节），
//     `/CreationDate`/`/ModDate` 本来就不写；
//   - 固定 SOURCE_DATE_EPOCH（runner 的默认行为）：**SHA-256 完全一致**；
//   - 副作用实测：XeTeX 的 `\today` 不受该变量影响（仍打印构建当天日期）。
//
// 沙箱注意：只用 stdio:'ignore' 与文件读，不用管道（受限沙箱下 Node 的 pipe 会 EPERM）。

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, readFileSync, rmSync, statSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

const arg = (name, dflt) => {
  const hit = process.argv.find((a) => a.startsWith(`--${name}=`));
  return hit ? hit.slice(name.length + 3) : dflt;
};
const flag = (name) => process.argv.includes(`--${name}`);

const ROOT = process.cwd();
const GAP_MS = Number(arg("gap", "6000")); // 跨过"秒级时间戳相同"的窗口，避免假阴性
const WITH_EPOCH = !flag("without-epoch");
const PROJECTS = (arg("projects", "")
  ? arg("projects", "").split(",")
  : [
      "test_file/projects/multifile",
      "test_file/projects/beamer工程",
      "test_file/projects/bench/thesis",
    ]
).map((p) => path.resolve(ROOT, p));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const sha256 = (p) => createHash("sha256").update(readFileSync(p)).digest("hex");

/** 与 runner 一致的两条编译路径（产物都落在 tmp/<stem>.pdf）。 */
const PATHS = {
  full: (stem) => ["latexmk", ["-xelatex", "-outdir=tmp", "-synctex=1", "-interaction=nonstopmode", `${stem}.tex`]],
  quick: (stem) => ["xelatex", ["-interaction=nonstopmode", "-synctex=1", "-output-directory=tmp", `${stem}.tex`]],
};

function runOnce(project, stem, pathName, { clean }) {
  const [cmd, args] = PATHS[pathName](stem);
  const env = { ...process.env };
  if (WITH_EPOCH) env.SOURCE_DATE_EPOCH = "0";
  else delete env.SOURCE_DATE_EPOCH;
  if (clean) rmSync(path.join(project, "tmp"), { recursive: true, force: true });
  const r = spawnSync(cmd, args, { cwd: project, stdio: "ignore", timeout: 900000, shell: false, env });
  const pdf = path.join(project, "tmp", `${stem}.pdf`);
  return { status: r.status, pdf, exists: existsSync(pdf) };
}

/** 逐字节比较，返回首个差异偏移与差异字节数（用于说明"差在哪"）。 */
function byteDiff(aPath, bPath) {
  const a = readFileSync(aPath);
  const b = readFileSync(bPath);
  if (a.length !== b.length) return { lengthDiff: [a.length, b.length], first: -1, count: -1 };
  let first = -1;
  let count = 0;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) {
      if (first < 0) first = i;
      count++;
    }
  }
  return { lengthDiff: null, first, count };
}

console.log(
  `构建确定性检查：SOURCE_DATE_EPOCH ${WITH_EPOCH ? "= 0（与 runner 一致）" : "未设置（复现问题）"}｜两次编译间隔 ${GAP_MS}ms\n`,
);

let failed = false;
for (const project of PROJECTS) {
  const stem = ["main", "thesis"].find((n) => existsSync(path.join(project, `${n}.tex`)));
  if (!stem) {
    console.log(`✗ ${path.relative(ROOT, project)}：找不到 main.tex`);
    failed = true;
    continue;
  }
  for (const pathName of Object.keys(PATHS)) {
    // Quick 路径（编辑触发）本来就要求已有构建产物（runner 会在无 .aux 时升级为 Full），
    // 所以先跑一次 Full 建出 tmp/，再连跑两次 Quick 比对 —— 与产品里的实际时序一致。
    if (pathName === "quick") {
      const seed = runOnce(project, stem, "full", { clean: true });
      if (!seed.exists) {
        console.log(`✗ ${path.relative(ROOT, project)} [quick]：前置 Full 编译失败（exit=${seed.status}）`);
        failed = true;
        continue;
      }
    }
    const clean = pathName === "full";
    const r1 = runOnce(project, stem, pathName, { clean });
    if (!r1.exists) {
      console.log(`✗ ${path.relative(ROOT, project)} [${pathName}]：编译没有产出 PDF（exit=${r1.status}）`);
      failed = true;
      continue;
    }
    // 第一份产物要存到 tmp/ 之外——第二次编译可能清空 tmp/
    const first = path.join(os.tmpdir(), `latteset-determinism-${path.basename(project)}-${pathName}-1.pdf`);
    copyFileSync(r1.pdf, first);
    await sleep(GAP_MS);
    const r2 = runOnce(project, stem, pathName, { clean });
    if (!r2.exists) {
      console.log(`✗ ${path.relative(ROOT, project)} [${pathName}]：第二次编译没有产出 PDF（exit=${r2.status}）`);
      failed = true;
      continue;
    }
    const h1 = sha256(first);
    const h2 = sha256(r2.pdf);
    const same = h1 === h2;
    const size = statSync(r2.pdf).size;
    let detail = "";
    if (!same) {
      const d = byteDiff(first, r2.pdf);
      detail = d.lengthDiff
        ? `（长度 ${d.lengthDiff[0]} vs ${d.lengthDiff[1]}）`
        : `（首个差异 @${d.first}，共 ${d.count} 字节不同）`;
      copyFileSync(r2.pdf, path.join(os.tmpdir(), `latteset-determinism-${path.basename(project)}-${pathName}-2.pdf`));
    }
    console.log(
      `${same ? "✓" : "✗"} ${path.relative(ROOT, project)} [${pathName}]：${same ? "逐字节一致" : "不一致"}${detail}` +
        `　sha=${h1.slice(0, 12)}${same ? "" : " vs " + h2.slice(0, 12)}　${(size / 1024).toFixed(0)}KB`,
    );
    rmSync(first, { force: true });
    if (!same) failed = true;
  }
}

console.log(
  failed
    ? "\n结论：存在非确定性（设 SOURCE_DATE_EPOCH 后仍不一致时，需查引擎/宏包是否会写入时钟信息）"
    : "\n结论：同一份源码重复构建**逐字节一致** → 「输出 diff / 只重排变化页」的字节前提成立",
);
process.exit(failed ? 1 : 0);
