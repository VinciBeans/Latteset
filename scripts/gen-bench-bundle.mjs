#!/usr/bin/env node
// 阶段③ T14b：生成"基线测试 bundle"（覆盖代表性文档集的本地 DirBundle）。
//
// 产物：`test_file/tectonic-bundle/`（424+ 文件、带 SHA256SUM 的目录 bundle）。
// 来源：Tectonic 上游 default bundle（`default_bundle_v33.tar`，与格式版本 33 同源）。
//       引擎以 HTTP Range 从该 tar 按需取文件（只取代表性文档集用到的，不下载 2.8 GB 全量），
//       取到的文件落在 `<cache>/data/<digest>/`；本脚本把这些文件 + 旧夹具并集落成目录 bundle。
//
// 用 release CLI 从上游 bundle 按需取文件；**夹具先由 scripts/gen-bench-projects.mjs 生成**
// （六档 + beamer + biblatex/biber + biblatex/bibtex；都 gitignore）。本脚本在夹具缺失时会
// 自动先跑一次 gen-bench-projects，保证干净 checkout（CI 代表集缓存未命中）能自足重建。
//
// 用法（全部命令在仓库根执行；本机 DSH 沙箱需要 TMP 指向工作区，脚本自己设置）：
//   node scripts/gen-bench-bundle.mjs --phase=gen       # 网络取文件 + 组装 staging（慢，首次 5–15 分钟）
//   node scripts/gen-bench-bundle.mjs --phase=install   # 把 staging 合并进 test_file/tectonic-bundle
//   node scripts/gen-bench-bundle.mjs --phase=validate  # 用本地 DirBundle 重编全部夹具 + validate-pdf
//   node scripts/gen-bench-bundle.mjs --phase=all       # 三步串跑（默认 gen 会复用已有 cache）
//
// 可复现性：夹具名单、bundle URL、pin 的格式版本都写在本文件；`--seed-cache` 可把上次生成的
// cache 拷进来避免重复联网。输出 digest 的定义见 computeDigest()（对 "sha256␠␠文件名" 行按文件名
// 排序后拼串再 sha256）——与上游 tar 的 digest 不同，因为这个目录 bundle 是**子集**，digest 只用于
// 本产物自身（format 缓存键）与可复现核对。
//
// 环境：release CLI（`src-tauri/target/release/latteset-cli.exe`，由 `npm run lib:cli` 生成）。
//       必须存在；脚本不自动构建（构建入口是 scripts/with-tectonic-lib.ps1）。

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  closeSync,
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const CLI = join(ROOT, "src-tauri", "target", "release", "latteset-cli.exe");
const TMPDIR = join(ROOT, "src-tauri", "target", ".tmp");
const STAGE = join(ROOT, "test_file", ".bundle-gen");
const CACHE = join(STAGE, "cache");
const CONFIG = join(STAGE, "config");
const PROJECTS = join(STAGE, "proj");
const LOGS = join(STAGE, "logs");
const STAGED_BUNDLE = join(STAGE, "bundle");
const TARGET = join(ROOT, "test_file", "tectonic-bundle");
const VALIDATE_CACHE = join(STAGE, "validate-cache");
const VALIDATE_CONFIG = join(STAGE, "validate-config");
const VALIDATE_PROJ = join(STAGE, "validate-proj");
const VALIDATE_LOGS = join(STAGE, "validate-logs");
const SUMMARY = join(STAGE, "summary.json");

// 上游 default bundle 的 URL 由 tectonic_bundles::get_fallback_bundle_url(FORMAT_SERIAL=33)
// 生成（``{prefix}/default_bundle_v{33}.tar``）；下面这条就是它，pin 在脚本里便于核对。
const BUNDLE_URL = "https://relay.fullyjustified.net/default_bundle_v33.tar";
const FORMAT_SERIAL = 33;

// ---------------------------------------------------------------- 夹具名单
//
// 覆盖目标（任务书验收）：六档（tiny / small-article / multifile / graphics / large / thesis）
// + beamer + biblatex/biber + bibtex + 中文默认（Fandol 四款）+ lmodern 全字号。
// 其中 `small-article` 用生成器**原版**（带 abstract：10pt 下 \small ⇒ 9pt ⇒ 需要 lmroman9），
// 不再需要 T14 早期 harness 的 fixtures/small 覆盖件。
const FIXTURES = [
  { name: "tiny", src: "test_file/projects/bench/tiny" },
  { name: "small-article", src: "test_file/projects/bench/small-article" },
  { name: "multifile", src: "test_file/projects/bench/multifile" },
  { name: "graphics", src: "test_file/projects/bench/graphics" },
  { name: "large", src: "test_file/projects/bench/large" },
  { name: "thesis", src: "test_file/projects/bench/thesis" },
  { name: "beamer", src: "test_file/projects/beamer工程" },
  { name: "biber", src: "test_file/acc/fixtures/biber工程" },
  { name: "bibtex", src: "test_file/acc/fixtures/bibtex工程" },
  { name: "chinese-fandol4", gen: "fandol4" },
  { name: "lmodern-sizes", gen: "lmodernSizes" },
];

// 生成型夹具（不依赖 test_file 里的历史夹具，保证脚本自足可复现）
const GENERATED = {
  // 中文默认路径：ctexart 默认 fontset=fandol；显式使用四款字体族 + 粗体，
  // 覆盖 FandolSong-Regular / FandolSong-Bold / FandolKai / FandolHei / FandolFang。
  fandol4: String.raw`\documentclass[UTF8]{ctexart}
\begin{document}
\section{中文字体四款}

宋体：你好，Latteset 内嵌 Tectonic 基线夹具。\par
{\heiti 黑体：覆盖 FandolHei 字形。}\par
{\kaishu 楷体：覆盖 FandolKai 字形。}\par
{\fangsong 仿宋：覆盖 FandolFang 字形。}\par
{\bfseries 宋体加粗：覆盖 FandolSong-Bold 字形。}\par

中文默认段落，含数字 123 与标点：，。！？《》——用于文本层校验。

\end{document}
`,
  // lmodern 光学字号全覆盖：article 10pt 下 \tiny…\Huge 会落到 5/6/7/8/9/10/12/17pt 的
  // lmroman* 字体；abstract 在 10pt 下用 9pt（就是 T14 早期 harness 缺失的 lmroman9）。
  lmodernSizes: String.raw`\documentclass{article}
\begin{document}
\begin{abstract}
Abstract at 9pt (small in 10pt article) --- exercises lmroman9.
\end{abstract}
\noindent{\tiny tiny 5}\par
\noindent{\scriptsize scriptsize 7}\par
\noindent{\footnotesize footnotesize 8}\par
\noindent{\small small 9}\par
\noindent{\normalsize normal 10}\par
\noindent{\large large 12}\par
\noindent{\Large Large 14}\par
\noindent{\LARGE LARGE 17}\par
\noindent{\huge huge 20}\par
\noindent{\Huge Huge 25}\par
\end{document}
`,
};

// 覆盖清单：生成后必须存在，否则算失败（验收口径的静态部分）
const REQUIRED = [
  // 六档常用
  "latex.ltx",
  "xelatex.ini",
  "article.cls",
  "book.cls",
  "pdftex.map",
  "hyperref.sty",
  "graphicx.sty",
  "booktabs.sty",
  "pgf.sty",
  "tikz.sty",
  // lmodern 全字号（小字号 9pt 是 T14 早期缺口）
  "lmroman5-regular.otf",
  "lmroman6-regular.otf",
  "lmroman7-regular.otf",
  "lmroman8-regular.otf",
  "lmroman9-regular.otf",
  "lmroman10-regular.otf",
  "lmroman12-regular.otf",
  "lmroman17-regular.otf",
  // beamer
  "beamer.cls",
  "beamerbaseframe.sty",
  "beamerbasefont.sty",
  // biblatex / biber 文档（内嵌形态不跑外部 biber，但要能载入 biblatex）
  "biblatex.sty",
  "biblatex.def",
  "blx-bibtex.def",
  "english.lbx",
  "numeric.bbx",
  "numeric.cbx",
  "biblatex.bst",
  "etoolbox.sty",
  // 中文默认（ctex + Fandol 四款）
  "ctexart.cls",
  "xeCJK.sty",
  "fontspec-xetex.sty",
  "FandolSong-Regular.otf",
  "FandolSong-Bold.otf",
  "FandolKai-Regular.otf",
  "FandolHei-Regular.otf",
  "FandolFang-Regular.otf",
];

// ---------------------------------------------------------------- 参数
const argv = process.argv.slice(2);
const arg = (name, dflt) => {
  const hit = argv.find((a) => a.startsWith(`--${name}=`));
  return hit ? hit.slice(name.length + 3) : dflt;
};
const PHASE = arg("phase", "all");
const SEED_CACHE = arg("seed-cache", "");
const TIMEOUT_SECS = Number(arg("timeout-secs", "900"));
const KEEP_STAGE = argv.includes("--keep-stage");

// ---------------------------------------------------------------- 工具
const sha256 = (buf) => createHash("sha256").update(buf).digest("hex");
const posix = (p) => p.split("\\").join("/");
const exists = (p) => existsSync(p);

function log(msg) {
  console.log(`[gen-bench-bundle] ${msg}`);
}

function walkFiles(dir, base = dir, out = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, e.name);
    if (e.isDirectory()) walkFiles(full, base, out);
    else if (e.isFile()) out.push(posix(relative(base, full)));
  }
  return out;
}

function rmrf(p) {
  if (existsSync(p)) rmSync(p, { recursive: true, force: true });
}

/** 环境：DSH 沙箱的 Low 完整性标签要求 TMP 在可写目录（见 docs/dev/build-env REPORT）。 */
function baseEnv() {
  const env = { ...process.env };
  env.TMP = TMPDIR;
  env.TEMP = TMPDIR;
  env.NO_COLOR = "1";
  env.CLICOLOR = "0";
  env.RUST_LOG = "info";
  return env;
}

function settingsFor(bundle, cacheDir) {
  return {
    schema_version: 3,
    compile: {
      mode: "continuous",
      debounce_ms: 500,
      new_file_wizard: true,
      timeout_secs: TIMEOUT_SECS,
    },
    tectonic: { bundle, cache_dir: cacheDir },
    root_file: null,
    ui: { theme: "light" },
  };
}

function writeSettings(dir, bundle, cacheDir) {
  rmrf(dir);
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "settings.json"), JSON.stringify(settingsFor(bundle, cacheDir), null, 2));
}

function prepareProject(fx) {
  const dst = join(PROJECTS, fx.name);
  rmrf(dst);
  mkdirSync(dst, { recursive: true });
  if (fx.gen) {
    writeFileSync(join(dst, "main.tex"), GENERATED[fx.gen]);
    return dst;
  }
  const src = join(ROOT, fx.src);
  if (!existsSync(src)) throw new Error(`夹具不存在：${fx.src}`);
  cpSync(src, dst, {
    recursive: true,
    filter: (s) => {
      const name = basename(s);
      // 跳过编译中间物与自带 PDF：避免旧产物干扰（tmp/、.latteset/、顶层 *.pdf）
      if (name === "tmp" || name === ".latteset") return false;
      if (name.toLowerCase().endsWith(".pdf") && dirname(s) === src) return false;
      if (name === "main.fdb_latexmk" || name === "main.fls") return false;
      return true;
    },
  });
  return dst;
}

/** 跑一次 release CLI 编译，stdout/stderr 重定向到文件（DSH 沙箱下 Node 管道 stdio 会 EPERM）。 */
function compileOnce(projDir, configDir, logStem) {
  mkdirSync(dirname(logStem), { recursive: true });
  const outPath = `${logStem}.json`;
  const errPath = `${logStem}.err`;
  const outFd = openSync(outPath, "w");
  const errFd = openSync(errPath, "w");
  const t0 = process.hrtime.bigint();
  const r = spawnSync(CLI, ["--config-dir", configDir, "--project", projDir, "compile"], {
    cwd: ROOT,
    env: baseEnv(),
    stdio: ["ignore", outFd, errFd],
  });
  closeSync(outFd);
  closeSync(errFd);
  const wallMs = Number(process.hrtime.bigint() - t0) / 1e6;
  let report = null;
  try {
    report = JSON.parse(readFileSync(outPath, "utf8"));
  } catch {
    /* 保留原始文件供排查 */
  }
  const compile = report?.compile ?? null;
  const pdf = compile?.pdf_path ? resolve(compile.pdf_path) : null;
  return {
    exit: r.status,
    wallMs,
    status: compile?.status ?? null,
    elapsedMs: compile?.elapsed_ms ?? null,
    kind: compile?.kind ?? null,
    pdf,
    json: outPath,
    err: errPath,
  };
}

// ---------------------------------------------------------------- gen
function pickCacheDataDir() {
  const dataRoot = join(CACHE, "data");
  if (!existsSync(dataRoot)) throw new Error(`缓存里没有 data/：${dataRoot}（网络 bundle 没取到文件？）`);
  const dirs = readdirSync(dataRoot, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => join(dataRoot, e.name));
  if (!dirs.length) throw new Error(`缓存 data/ 下没有 digest 目录：${dataRoot}`);
  // 取文件最多的那个（正常情况下只有一个 digest）
  dirs.sort((a, b) => walkFiles(b).length - walkFiles(a).length);
  return { dir: dirs[0], digest: basename(dirs[0]) };
}

/** 夹具缺失（干净 checkout / CI 缓存未命中）⇒ 先由 scripts/gen-bench-projects.mjs 确定性生成。 */
function ensureFixtures() {
  const missing = FIXTURES.filter((fx) => fx.src && !existsSync(join(ROOT, fx.src)));
  if (!missing.length) return;
  log(`夹具缺失 ${missing.length} 个（${missing.map((f) => f.name).join(", ")}）⇒ 先跑 scripts/gen-bench-projects.mjs`);
  const r = spawnSync(process.execPath, [join(ROOT, "scripts", "gen-bench-projects.mjs")], {
    cwd: ROOT,
    env: baseEnv(),
    stdio: "inherit",
  });
  if (r.status !== 0) throw new Error(`gen-bench-projects 失败（exit ${r.status}）`);
  for (const fx of missing) {
    if (!existsSync(join(ROOT, fx.src))) throw new Error(`生成后仍缺夹具：${fx.src}`);
  }
  log("夹具已生成");
}

function gen() {
  if (!existsSync(CLI)) {
    throw new Error(`缺 release CLI：${CLI}（先跑 npm run lib:cli，入口 scripts/with-tectonic-lib.ps1）`);
  }
  ensureFixtures();
  if (KEEP_STAGE) log("--keep-stage：保留 staging/cache");
  else rmrf(STAGE);
  for (const d of [CACHE, CONFIG, PROJECTS, LOGS, TMPDIR, STAGED_BUNDLE]) mkdirSync(d, { recursive: true });

  // 把上次的 cache 拷进来（省一次联网；format 与已取文件都复用）
  if (SEED_CACHE) {
    const seed = resolve(ROOT, SEED_CACHE);
    if (!existsSync(seed)) throw new Error(`--seed-cache 不存在：${seed}`);
    log(`seed cache：${seed} → ${CACHE}`);
    cpSync(seed, CACHE, { recursive: true });
  }

  // 网络 bundle + 独立 config（bundle=URL、cache=STAGE/cache、timeout=--timeout-secs）
  writeSettings(CONFIG, BUNDLE_URL, CACHE);
  log(`bundle 源：${BUNDLE_URL}（format serial=${FORMAT_SERIAL}）`);
  log(`CLI：${CLI}`);
  log(`夹具 ${FIXTURES.length} 个：${FIXTURES.map((f) => f.name).join(", ")}`);

  const results = [];
  for (const fx of FIXTURES) {
    const proj = prepareProject(fx);
    const stem = join(LOGS, fx.name);
    log(`→ 编译 ${fx.name} …`);
    const r = compileOnce(proj, CONFIG, stem);
    const ok = r.exit === 0 && r.status === "success" && r.pdf && existsSync(r.pdf);
    results.push({ ...fx, ...r, ok });
    log(
      `  ${ok ? "PASS" : "FAIL"} ${fx.name}: exit=${r.exit} status=${r.status} elapsed=${r.elapsedMs}ms ` +
        `wall=${Math.round(r.wallMs)}ms pdf=${r.pdf ? statSync(r.pdf).size : 0}B`,
    );
  }

  // 组装 staging bundle：旧夹具（并集基底）+ 本次网络缓存取到的文件
  const base = TARGET;
  if (existsSync(join(base, "SHA256SUM"))) {
    for (const name of walkFiles(base)) {
      if (name === "SHA256SUM") continue;
      const dst = join(STAGED_BUNDLE, name);
      mkdirSync(dirname(dst), { recursive: true });
      copyFileSync(join(base, name), dst);
    }
    log(`基底：${base}（${walkFiles(base).length} 文件，取并集）`);
  }
  const { dir: dataDir, digest: upstreamDigest } = pickCacheDataDir();
  const fetched = walkFiles(dataDir);
  for (const name of fetched) {
    const dst = join(STAGED_BUNDLE, name);
    mkdirSync(dirname(dst), { recursive: true });
    copyFileSync(join(dataDir, name), dst);
  }
  log(`网络缓存：${dataDir}（${fetched.length} 文件，upstream digest=${upstreamDigest}）`);

  // 覆盖清单静态检查
  const names = new Set(walkFiles(STAGED_BUNDLE));
  const missing = REQUIRED.filter((n) => !names.has(n));
  if (missing.length) {
    log(`✗ 覆盖清单缺 ${missing.length} 项：${missing.join(", ")}`);
  } else {
    log(`✓ 覆盖清单 ${REQUIRED.length} 项全部命中`);
  }

  // 写 SHA256SUM（digest 定义见文件头）
  const { digest, files, bytes, manifestLines } = computeDigest(STAGED_BUNDLE);
  writeFileSync(join(STAGED_BUNDLE, "SHA256SUM"), digest + "\n");
  const summary = {
    ok: missing.length === 0 && results.every((r) => r.ok),
    phase: "gen",
    source: BUNDLE_URL,
    formatSerial: FORMAT_SERIAL,
    upstreamDigest,
    digest,
    files,
    bytes,
    manifestLines,
    stagedBundle: STAGED_BUNDLE,
    cache: CACHE,
    seedCache: SEED_CACHE || null,
    fixtures: results.map((r) => ({
      name: r.name,
      ok: r.ok,
      exit: r.exit,
      status: r.status,
      elapsedMs: r.elapsedMs,
      pdfBytes: r.pdf && existsSync(r.pdf) ? statSync(r.pdf).size : 0,
      json: posix(relative(ROOT, r.json)),
      err: posix(relative(ROOT, r.err)),
    })),
    missingRequired: missing,
  };
  writeFileSync(SUMMARY, JSON.stringify(summary, null, 2));
  log(`staging 完成：digest=${digest} files=${files} bytes=${bytes}（manifest ${manifestLines} 行）`);
  log(`summary：${SUMMARY}`);
  if (!summary.ok) process.exitCode = 1;
}

/** digest = sha256( 对每个文件 "sha256  <相对路径>\n"，按相对路径（UTF-8 字节序）排序后拼接 )。 */
function computeDigest(dir) {
  const names = walkFiles(dir)
    .filter((n) => n !== "SHA256SUM")
    .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
  let bytes = 0;
  const lines = [];
  for (const name of names) {
    const buf = readFileSync(join(dir, name));
    bytes += buf.length;
    lines.push(`${sha256(buf)}  ${name}\n`);
  }
  const manifest = lines.join("");
  return { digest: sha256(manifest), files: names.length, bytes, manifestLines: lines.length };
}

// ---------------------------------------------------------------- install
function install() {
  if (!existsSync(join(STAGED_BUNDLE, "SHA256SUM"))) {
    throw new Error(`staging bundle 不存在或不完整：${STAGED_BUNDLE}（先跑 --phase=gen）`);
  }
  const staged = new Set(walkFiles(STAGED_BUNDLE));
  mkdirSync(TARGET, { recursive: true });
  // 先删 target 里 staging 没有的文件（保证 dir 内容 = digest 口径），SHA256SUM 最后写
  for (const name of walkFiles(TARGET)) {
    if (name !== "SHA256SUM" && !staged.has(name)) rmSync(join(TARGET, name), { force: true });
  }
  let copied = 0;
  for (const name of staged) {
    if (name === "SHA256SUM") continue;
    const dst = join(TARGET, name);
    mkdirSync(dirname(dst), { recursive: true });
    copyFileSync(join(STAGED_BUNDLE, name), dst);
    copied++;
  }
  copyFileSync(join(STAGED_BUNDLE, "SHA256SUM"), join(TARGET, "SHA256SUM"));
  const { digest, files, bytes } = computeDigest(TARGET);
  const written = readFileSync(join(TARGET, "SHA256SUM"), "utf8").trim();
  if (written !== digest) throw new Error(`SHA256SUM 校验失败：文件写的是 ${written}，重算是 ${digest}`);
  log(`install 完成：${TARGET} files=${files} bytes=${bytes} digest=${digest}（复制 ${copied} 文件）`);
  return { target: TARGET, digest, files, bytes };
}

// ---------------------------------------------------------------- validate
function validate() {
  if (!existsSync(join(TARGET, "SHA256SUM"))) throw new Error(`目标 bundle 不完整：${TARGET}`);
  ensureFixtures();
  rmrf(VALIDATE_CACHE);
  rmrf(VALIDATE_PROJ);
  mkdirSync(VALIDATE_CACHE, { recursive: true });
  mkdirSync(VALIDATE_PROJ, { recursive: true });
  // 用 file:/// URL（绝对路径直写会被上游当 URL scheme；见 crates/latteset-tectonic/src/bundle.rs）
  const bundleUrl = `file:///${posix(TARGET).replace(/^\//, "")}`;
  writeSettings(VALIDATE_CONFIG, bundleUrl, VALIDATE_CACHE);
  log(`validate：bundle=${bundleUrl} cache=${VALIDATE_CACHE}`);

  const results = [];
  for (const fx of FIXTURES) {
    const proj = join(VALIDATE_PROJ, fx.name);
    rmrf(proj);
    mkdirSync(proj, { recursive: true });
    if (fx.gen) writeFileSync(join(proj, "main.tex"), GENERATED[fx.gen]);
    else {
      const src = join(ROOT, fx.src);
      cpSync(src, proj, {
        recursive: true,
        filter: (s) => {
          const name = basename(s);
          if (name === "tmp" || name === ".latteset") return false;
          if (name.toLowerCase().endsWith(".pdf") && dirname(s) === src) return false;
          return true;
        },
      });
    }
    const stem = join(VALIDATE_LOGS, fx.name);
    const r = compileOnce(proj, VALIDATE_CONFIG, stem);
    let pdfCheck = null;
    if (r.pdf && existsSync(r.pdf)) {
      pdfCheck = runValidatePdf(r.pdf, stem);
    }
    const ok = r.exit === 0 && r.status === "success" && pdfCheck?.ok === true;
    results.push({ name: fx.name, ok, ...r, pdfCheck });
    log(
      `  ${ok ? "PASS" : "FAIL"} ${fx.name}: exit=${r.exit} status=${r.status} ` +
        `elapsed=${r.elapsedMs}ms pdfCheck=${pdfCheck ? (pdfCheck.ok ? "ok" : "fail") : "none"}`,
    );
  }
  const summary = {
    ok: results.every((r) => r.ok),
    phase: "validate",
    bundle: TARGET,
    digest: readFileSync(join(TARGET, "SHA256SUM"), "utf8").trim(),
    fixtures: results.map((r) => ({
      name: r.name,
      ok: r.ok,
      exit: r.exit,
      status: r.status,
      elapsedMs: r.elapsedMs,
      pdfBytes: r.pdf && existsSync(r.pdf) ? statSync(r.pdf).size : 0,
      pdfChecks: r.pdfCheck?.checks ?? null,
      json: posix(relative(ROOT, r.json)),
      err: posix(relative(ROOT, r.err)),
    })),
  };
  writeFileSync(join(STAGE, "validate-summary.json"), JSON.stringify(summary, null, 2));
  log(`validate 完成：ok=${summary.ok}（${results.filter((r) => r.ok).length}/${results.length}）`);
  if (!summary.ok) process.exitCode = 1;
}

/** 用仓库的 scripts/validate-pdf.mjs 复核产物；返回 {ok, checks}。 */
function runValidatePdf(pdfPath, stem) {
  const outPath = `${stem}.validate.json`;
  const errPath = `${stem}.validate.err`;
  const outFd = openSync(outPath, "w");
  const errFd = openSync(errPath, "w");
  const r = spawnSync(process.execPath, [join(ROOT, "scripts", "validate-pdf.mjs"), "--pdf", pdfPath, "--json"], {
    cwd: ROOT,
    env: baseEnv(),
    stdio: ["ignore", outFd, errFd],
  });
  closeSync(outFd);
  closeSync(errFd);
  try {
    const report = JSON.parse(readFileSync(outPath, "utf8"));
    const noFffd = report.checks?.find((c) => c.name === "无 U+FFFD");
    return { ok: r.status === 0 && report.ok === true && noFffd?.ok === true, checks: report.checks ?? [], json: outPath };
  } catch {
    return { ok: false, checks: [], json: null, err: errPath };
  }
}

// ---------------------------------------------------------------- main
try {
  if (PHASE === "gen" || PHASE === "all") gen();
  if (PHASE === "install" || PHASE === "all") install();
  if (PHASE === "validate" || PHASE === "all") validate();
  if (!["gen", "install", "validate", "all"].includes(PHASE)) {
    console.error(`未知 --phase=${PHASE}（gen|install|validate|all）`);
    process.exit(2);
  }
} catch (e) {
  console.error(`[gen-bench-bundle] 失败：${e?.stack ?? e}`);
  process.exit(1);
}
