#!/usr/bin/env node
// 阶段③ T21（G-L3）：第三方声明（THIRD-PARTY-NOTICES）与 GPL 字体源码随包。
//
// 两种模式：
//   node scripts/make-notices.mjs            # 从**已入库**的 data/ + LICENSES/ + sources/ 生成
//                                            #   resources/licenses/THIRD-PARTY-NOTICES.md + manifest.json
//   node scripts/make-notices.mjs --check    # CI/复核：重算并与已入库产物逐字节比对（不写文件）
//   node scripts/make-notices.mjs --collect  # 开发机一次性采集：从本机 TeX Live / cargo registry /
//                                            #   vcpkg installed / Tectonic checkout / 已下载的字体源码
//                                            #   生成 data/、LICENSES/、sources/、patches/ 输入（需联网/本机素材）
//
// 为什么分成"采集"与"生成"两段：
// - **生成**必须离线、确定性、可被 CI 复核 ⇒ 只读入库的 `src-tauri/resources/licenses/**`。
// - **采集**依赖本机 TeX Live 2026（tlpdb 做 bundle 包→许可映射）、cargo registry（crate 许可文本）、
//   vcpkg 安装树（port 的 copyright 全文）、Tectonic checkout（TL patch 原文）与 CTAN/GitHub 的字体源码；
//   这些在 CI 上没有。采集结果全部入库（清单 + 文本 + 源码），换 bundle/依赖时重跑采集并提交 diff。
//
// 关键口径（docs/dev/tectonic-licensing.md §5 / §9）：
// - GPL 字体（Fandol）按 GPLv3 §6 随安装包附 **Corresponding Source**（cidfont.ps/AFM/features/…）；
// - 随包 bundle 只含代表集，逐包许可表来自 TL 2026 tlpdb 映射（126 包 + 10 个无归属文件已人工归属）；
// - 被 patch 的 TL 文件：只随包 `latex.ltx`（LPPL1.3c），变更说明 + diff + 原件获取方式；其余 3 个
//   patch 对应的文件不在随包集合内，如实登记。

import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
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
const LIC_ROOT = join(ROOT, "src-tauri", "resources", "licenses");
const LICENSES = join(LIC_ROOT, "LICENSES");
const SOURCES = join(LIC_ROOT, "sources");
const PATCHES = join(LIC_ROOT, "patches");
const DATA = join(LIC_ROOT, "data");
const NOTICE = join(LIC_ROOT, "THIRD-PARTY-NOTICES.md");
const MANIFEST = join(LIC_ROOT, "manifest.json");
const BUNDLE_DIR = join(ROOT, "test_file", "tectonic-bundle");
const BUNDLE_ZIP = join(ROOT, "src-tauri", "resources", "tectonic-bundle.zip");
const PATCH_SRC = join(ROOT, "test_file", "tectonic-src", "bundles", "bundles", "texlive2024-0312", "patches", "texlive");
const TL = "C:\\texlive\\2026\\texmf-dist";
const TLPDB = "C:\\texlive\\2026\\tlpkg\\texlive.tlpdb";
const VCPKG_SHARE = join(ROOT, "test_file", "vcpkg", "installed", "x64-windows-static-release", "share");
const FANDOL_SRC = join(ROOT, "test_file", "phase3", "t21-src", "fandol-src");
const FANDOL_COMMIT = "b93300821373d3092e378e57ae22ecb8e9082c62";
const FANDOL_REPO = "https://github.com/Yixf-Self/fandol-fonts";

const args = process.argv.slice(2);
const CHECK = args.includes("--check");
const COLLECT = args.includes("--collect");
const log = (m) => console.log(`[make-notices] ${m}`);
const sha256 = (buf) => createHash("sha256").update(buf).digest("hex");

const posix = (p) => p.split("\\").join("/");
const rel = (p) => posix(relative(LIC_ROOT, p));

function walk(dir, base = dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, e.name);
    if (e.isDirectory()) walk(full, base, out);
    else if (e.isFile()) out.push(posix(relative(base, full)));
  }
  return out;
}
const readJson = (p) => JSON.parse(readFileSync(p, "utf8"));
const writeJson = (p, v) => {
  mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, JSON.stringify(v, null, 2) + "\n");
};

// ---------------------------------------------------------------- collect

/** 采集模式：只在本机（有 TL/registry/vcpkg/tectonic-src/已下载字体源码）运行。 */
function collect() {
  log("--collect：从本机素材采集（需要 TeX Live 2026 / cargo registry / vcpkg installed / tectonic-src）");
  rmrf(LICENSES);
  rmrf(SOURCES);
  rmrf(PATCHES);
  rmrf(DATA);
  for (const d of [LICENSES, SOURCES, PATCHES, DATA]) mkdirSync(d, { recursive: true });

  // ---- 1) meta（版本锚点 = G-L9 的 diff 点）
  const zipBytes = existsSync(BUNDLE_ZIP) ? statSync(BUNDLE_ZIP).size : null;
  const bundleSum = readFileSync(join(BUNDLE_DIR, "SHA256SUM"), "utf8").trim();
  const product = existsSync(join(ROOT, "test_file", "phase3", "product-bundle.json"))
    ? readJson(join(ROOT, "test_file", "phase3", "product-bundle.json"))
    : null;
  const meta = {
    generatedAt: new Date().toISOString(),
    auditDate: "2026-11-20",
    engine: { name: "tectonic", version: "0.17.0", formatSerial: 33, license: "MIT" },
    bundle: {
      product: "src-tauri/resources/tectonic-bundle.zip",
      digest: bundleSum,
      files: 517,
      uncompressedBytes: 54550792,
      zipBytes,
      zipSha256: product?.zipSha256 ?? null,
      source: "https://relay.fullyjustified.net/default_bundle_v33.tar",
      texLiveLineage: "tectonic-src/bundles/bundles/texlive2024-0312",
    },
    vcpkg: {
      triplet: "x64-windows-static-release",
      pinRev: "a62ce77d56ee07513b4b67de1ec2daeaebfae51a",
    },
    licensingDoc: "docs/dev/tectonic-licensing.md",
  };
  writeJson(join(DATA, "meta.json"), meta);

  // ---- 2) bundle 包→许可（来自 T16 对 TL 2026 tlpdb 的映射结果；committed 快照）
  const audit = readJson(join(ROOT, "test_file", "audit-bundle-map.json"));
  const packages = Object.entries(audit.packages)
    .map(([name, v]) => ({ name, license: v.license, files: v.count }))
    .sort((a, b) => a.name.localeCompare(b.name));
  writeJson(join(DATA, "bundle-packages.json"), {
    note:
      "TL 2026 tlpdb 按 basename 映射随包代表集（517 文件）；license 取 catalogue-license。同名文件可能命中多个包 ⇒ 全部列出（audit 脚本口径）；10 个无归属文件见 unmatched-files.json。",
    mappedAt: new Date().toISOString(),
    bundleDigest: bundleSum,
    packageCount: packages.length,
    packages,
  });
  const unmatchedNotes = {
    "SHA256SUM": "本产品目录 bundle 的 digest 文件（非 TL 内容）",
    "dehyphn-x-2022-03-16.pat": "TL hyph-utf8 生成物（TeX Live 构建期由 dehyphn-x 生成）；来源 TL 2024-03-12，许可见 hyph-utf8（LPPL/GPL 组合，逐文件头声明）",
    "dehyphn-x-2022-03-16.tex": "同上",
    "dehypht-x-2022-03-16.pat": "同上",
    "dehypht-x-2022-03-16.tex": "同上",
    "hyph-quote-rm.tex": "TL hyph-utf8（文件头声明，见 LICENSES/gpl-2.0.txt / knuth 相关声明）",
    "kanjix.map": "TL uptex-base 生成物（map 数据，来源 TL 2024-03-12；许可随 uptex-base）",
    "pdftex.map": "TL pdftex 生成物（由 updmap 汇聚各 fontmap；逐包许可见表内各字体/包）",
    "pgfkeysfiltered.code.tex": "TL pgf 生成物（pgf 许可 LPPL1.3c + GPL2 + FDL）",
    "tectonic-format-latex.tex": "Tectonic 自带（MIT，Tectonic 项目）",
  };
  writeJson(join(DATA, "unmatched-files.json"), {
    note: "按 basename 映射不到 TL 包的文件（生成物/产品文件），人工归属如下。",
    files: audit.unmatched.map((name) => ({ name, attribution: unmatchedNotes[name] ?? "见 TL 上游（未归属，发布前复核）" })),
  });

  // ---- 3) Rust 依赖清单 + 去重许可文本
  log("采集 Rust 依赖（cargo metadata）…");
  const metaRun = spawnSync(cargoBin(), ["metadata", "--format-version", "1", "--locked", "--filter-platform", "x86_64-pc-windows-msvc"], {
    cwd: ROOT,
    encoding: "utf8",
    maxBuffer: 128 * 1024 * 1024,
  });
  if (metaRun.status !== 0) throw new Error(`cargo metadata 失败：${metaRun.stderr?.slice(0, 500)}`);
  const cargoMeta = JSON.parse(metaRun.stdout);
  const workspaceIds = new Set(cargoMeta.workspace_members);
  const resolveNode = new Map(cargoMeta.resolve.nodes.map((n) => [n.id, n]));
  const reachable = new Set();
  // 只跟 **normal** 依赖（真正链接进二进制的 crate）；build/dev 依赖不进发行物，不列清单 A。
  const walkDeps = (id) => {
    if (reachable.has(id)) return;
    reachable.add(id);
    for (const d of resolveNode.get(id)?.deps ?? []) {
      const kinds = d.dep_kinds ?? [];
      const isRuntime = kinds.length === 0 || kinds.some((k) => k.kind === null || k.kind === "normal");
      if (isRuntime) walkDeps(d.pkg);
    }
  };
  // 根是 workspace（virtual）；从 workspace 成员出发
  for (const id of workspaceIds) if (resolveNode.has(id)) walkDeps(id);
  const pkgs = cargoMeta.packages.filter(
    (p) => reachable.has(p.id) && !workspaceIds.has(p.id) && !p.source?.startsWith("path+"),
  );
  const rustTexts = new Map(); // sha12 -> {file, textRelative}
  const rustDeps = [];
  for (const p of pkgs) {
    const dir = dirname(p.manifest_path);
    const licenseFiles = (existsSync(dir) ? readdirSync(dir) : [])
      .filter((n) => /^(LICEN[CS]E|COPYING|NOTICE|COPYRIGHT)/i.test(n))
      .filter((n) => statSync(join(dir, n)).isFile())
      .sort();
    const texts = [];
    for (const f of licenseFiles) {
      const buf = readFileSync(join(dir, f));
      const h = sha256(buf).slice(0, 12);
      if (!rustTexts.has(h)) {
        const target = join(LICENSES, "rust", `${h}-${f}`);
        mkdirSync(dirname(target), { recursive: true });
        writeFileSync(target, buf);
        rustTexts.set(h, target);
      }
      texts.push(rel(rustTexts.get(h)));
    }
    rustDeps.push({
      name: p.name,
      version: p.version,
      license: p.license ?? "（见 license_file）",
      repository: p.repository ?? null,
      licenseFiles: texts,
    });
  }
  rustDeps.sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
  writeJson(join(DATA, "rust-deps.json"), {
    note: "清单 A：随二进制链接的 crates.io 依赖（不含 workspace 成员）。许可文本按文件内容去重存于 LICENSES/rust/。",
    count: rustDeps.length,
    crates: rustDeps,
  });

  // ---- 4) C 依赖（vcpkg 11 port）的 copyright 全文
  const ports = [
    "freetype", "harfbuzz", "graphite2", "icu", "fontconfig", "libpng",
    "brotli", "bzip2", "dirent", "expat", "zlib",
  ];
  const cDeps = [];
  for (const port of ports) {
    const copyright = join(VCPKG_SHARE, port, "copyright");
    if (!existsSync(copyright)) throw new Error(`缺 vcpkg copyright：${copyright}`);
    const target = join(LICENSES, "c-deps", `${port}.txt`);
    mkdirSync(dirname(target), { recursive: true });
    copyFileSync(copyright, target);
    cDeps.push({ port, copyright: rel(target), bytes: statSync(copyright).size });
  }
  writeJson(join(DATA, "c-deps.json"), {
    note: "清单 B：vcpkg x64-windows-static-release 静态链接的 C/C++ 依赖（11 个直接 port；传递依赖已并入各 port 的 copyright 文本）。",
    triplet: meta.vcpkg.triplet,
    pinRev: meta.vcpkg.pinRev,
    ports: cDeps,
  });

  // ---- 5) 被 patch 的 TL 文件（G-L6）
  const patchFiles = [
    {
      file: "latex.ltx",
      patch: "latex.ltx.diff",
      bundled: true,
      originalSource:
        "TeX Live 2024-03-12 的 latex-base（CTAN macros/latex/base，2024 快照；Tectonic bundles/bundles/texlive2024-0312 取自该版本）",
      license: "LPPL 1.3c",
      changeSummary:
        "Tectonic 对 latex.ltx 打补丁以支持其「格式化/预编译 format」与跨平台确定性输出（见 patches/latex.ltx.diff）；文件名保持 latex.ltx（LPPL 1.3c 允许分发修改版，但须明确标注为修改版并附变更说明）。",
      licenseImpact:
        "修改版仍按 LPPL 1.3c 分发（LICENSES/lppl-1.3c.txt）；变更说明与 diff 随包，原件可从上方 originalSource 获取。",
    },
    {
      file: "listings.sty",
      patch: "listings.sty.diff",
      bundled: false,
      originalSource: "CTAN macros/latex/contrib/listings（TL 2024-03-12）",
      license: "LPPL 1.3",
      changeSummary: "不在随包代表集内（未分发），仅登记 Tectonic 对该文件的 patch 存在。",
      licenseImpact: "未随包 ⇒ 无分发义务；补足完整 bundle 时需重新审计。",
    },
    {
      file: "fontawesome.sty",
      patch: "fontawesome.sty.diff",
      bundled: false,
      originalSource: "CTAN fonts/fontawesome（TL 2024-03-12）",
      license: "LPPL 1.3c / OFL（字体部分）",
      changeSummary: "不在随包代表集内（未分发），仅登记 patch 存在。",
      licenseImpact: "未随包 ⇒ 无分发义务。",
    },
    {
      file: "fithesis-mu-base.sty",
      patch: "fithesis-mu-base.sty.diff",
      bundled: false,
      originalSource: "CTAN macros/latex/contrib/fithesis（TL 2024-03-12）",
      license: "LPPL 1.3c",
      changeSummary: "不在随包代表集内（未分发），仅登记 patch 存在。",
      licenseImpact: "未随包 ⇒ 无分发义务。",
    },
  ];
  for (const p of patchFiles) {
    const src = join(PATCH_SRC, p.patch);
    if (!existsSync(src)) throw new Error(`缺 patch：${src}`);
    mkdirSync(PATCHES, { recursive: true });
    copyFileSync(src, join(PATCHES, p.patch));
    p.patchPath = rel(join(PATCHES, p.patch));
    p.patchSha256 = sha256(readFileSync(src));
    if (p.bundled && existsSync(join(BUNDLE_DIR, p.file))) {
      p.bundledSha256 = sha256(readFileSync(join(BUNDLE_DIR, p.file)));
    }
  }
  writeJson(join(DATA, "patched-tl-files.json"), {
    note: "G-L6：Tectonic 对 TeX Live 的 patch 清单（来自 tectonic-src bundles/bundles/texlive2024-0312/patches/texlive/）。随包集合中实际只包含 latex.ltx。",
    files: patchFiles,
  });

  // ---- 6) GPL 字体 Corresponding Source（随包）
  log("采集 GPL 字体源码（Fandol）…");
  const FANDOL_FONTS = [
    "FandolSong-Regular", "FandolSong-Bold", "FandolKai-Regular",
    "FandolHei-Regular", "FandolHei-Bold", "FandolFang-Regular",
  ];
  const fandolOut = join(SOURCES, "fandol");
  mkdirSync(fandolOut, { recursive: true });
  const fontSources = [];
  for (const font of FANDOL_FONTS) {
    const from = join(FANDOL_SRC, font);
    const to = join(fandolOut, font);
    mkdirSync(to, { recursive: true });
    const files = readdirSync(from).filter((n) => n !== `${font}.otf`).sort();
    for (const f of files) copyFileSync(join(from, f), join(to, f));
    fontSources.push({
      binary: `${font}.otf`,
      sourceDir: rel(to),
      files: files.map((f) => ({ name: f, bytes: statSync(join(to, f)).size })),
      buildTool: "AFDKO / makeotf（cidfont.ps + AFM + features + fontinfo → OTF）；AFM 头注明 FontForge 生成",
    });
  }
  copyFileSync(join(FANDOL_SRC, "COPYING"), join(fandolOut, "COPYING"));
  writeFileSync(
    join(fandolOut, "SOURCE.md"),
    `# Fandol 字体 Corresponding Source\n\n` +
      `随本安装包分发的 6 个 Fandol OTF 的 **Corresponding Source**（GPLv3 §6）由以下上游源码提供：\n\n` +
      `- 仓库：${FANDOL_REPO}\n` +
      `- 提交（pin）：\`${FANDOL_COMMIT}\`\n` +
      `- 上游项目页：https://fandol.github.io/（作者：Clerk Ma & Jie Su）\n` +
      `- 字体二进制来源：CTAN \`fonts/fandol\`（与上游 OTF 同源；本包只改分包方式，不改字体内容）\n\n` +
      `每个 \`<font>/\` 目录含 \`cidfont.ps\`（CID Type1 轮廓）、\`AFM\`（度量）、\`features\`（OpenType 特性）、\`fontinfo\`、` +
      `\`cidfontinfo\`、\`FontMenuNameDB\`（AFDKO 构建输入）。构建工具链为 Adobe AFDKO（\`makeotf\`），` +
      `对应二进制与源码映射见 \`data/sources.json\`。\n\n` +
      `许可见 \`COPYING\`（GPLv3 + 字体嵌入例外：把本字体嵌入文档不会使文档本身受 GPL 约束）。\n` +
      `复现：用 AFDKO 的 \`makeotf\` 以各目录为工作目录生成 \`<font>.otf\`；产物哈希随包记录在 manifest.json。\n`,
  );

  log("采集 zapfding AFM（pzdr.tfm 的 Corresponding Source）…");
  const zapfOut = join(SOURCES, "zapfding");
  mkdirSync(zapfOut, { recursive: true });
  const pzdrAfm = join(TL, "fonts", "afm", "adobe", "zapfding", "pzdr.afm");
  copyFileSync(pzdrAfm, join(zapfOut, "pzdr.afm"));
  writeFileSync(
    join(zapfOut, "SOURCE.md"),
    `# zapfding（pzdr.tfm）Corresponding Source\n\n` +
      `随包分发的 \`pzdr.tfm\`（TL zapfding 包，GPL 许可）由 \`pzdr.afm\` 经 dvips 的 \`afm2tfm\` 生成。\n` +
      `本目录附 AFM（度量源数据，取自 TeX Live 2026 \`fonts/afm/adobe/zapfding/pzdr.afm\`）。\n\n` +
      `- 生成命令：\`afm2tfm pzdr.afm -v pzdr.vpl -o pzdr.tfm\`（dvips 构建链，参数以 TL 2024-03-12 的 zapfding 包为准）\n` +
      `- 生成器（GPL）源码：https://github.com/TeX-Live/texlive-source 的 \`texk/dvips/\`（TeX Live 2024 分支）\n` +
      `- 注意：\`pzdr.tfm\` 只含 TFM 度量，不含字体轮廓；ZapfDingbats 轮廓由 PDF 阅读器/系统字体提供。\n`,
  );
  const sources = {
    note: "GPL/LGPL 组件的 Corresponding Source（随安装包）。字体二进制 ↔ 源码映射见下。",
    fonts: [
      ...fontSources.map((f) => ({ ...f, package: "fandol", license: "GPL-3.0-or-later WITH font-exception", sourceRoot: rel(fandolOut) })),
      {
        binary: "pzdr.tfm",
        package: "zapfding",
        license: "GPL (TL zapfding)",
        sourceDir: rel(zapfOut),
        files: [{ name: "pzdr.afm", bytes: statSync(join(zapfOut, "pzdr.afm")).size }],
        buildTool: "dvips afm2tfm（GPL）",
      },
    ],
    selfSourced: [
      { package: "cjk", files: ["CJKfntef.sty"], note: "LaTeX 源文件即 preferred form（TL source/latex/cjk 有对应 dtx），随包文件本身可修改" },
      { package: "cslatex", files: ["fonttext.cfg"], note: "配置数据文件；TL 2024-03-12 src cslatex.dtx 可再生成（上游 CTAN systems/cslatex）" },
      { package: "dvipdfmx", files: ["ckx.map"], note: "map 数据文件；生成器 dvipdfmx 源码见 https://github.com/TeX-Live/texlive-source texk/dvipdfmx（GPL）" },
      { package: "hyphen-hungarian", files: ["hyph-hu.tex", "loadhyph-hu.tex"], note: "patterns 文本即对应源码（hyph-utf8，GPL）" },
      { package: "hyphen-serbian", files: ["hyph-sh-cyrl.tex", "hyph-sh-latn.tex", "loadhyph-sr-cyrl.tex", "loadhyph-sr-latn.tex"], note: "patterns 文本即对应源码（GPL）" },
      { package: "hyphen-italian", files: ["hyph-it.tex", "hyph-quote-it.tex", "loadhyph-it.tex"], note: "patterns 文本即对应源码（LGPL）" },
    ],
    totalBytes: walk(SOURCES).reduce((s, f) => s + statSync(join(SOURCES, f)).size, 0),
  };
  writeJson(join(DATA, "sources.json"), sources);

  // ---- 7) 规范许可证文本（canonical，供 NOTICE 引用）
  const licenseTexts = [
    { id: "MIT-Tectonic", from: join(ROOT, "test_file", "tectonic-src", "LICENSE"), to: "mit-tectonic.txt", note: "Tectonic 项目 MIT 全文" },
    { id: "LPPL-1.3c", from: join(TL, "doc", "latex", "base", "lppl.txt"), to: "lppl-1.3c.txt", note: "LPPL 1.3c 2008-05-04" },
    { id: "GPL-2.0", from: join(TL, "doc", "latex", "beamer", "licenses", "gpl-2.0.txt"), to: "gpl-2.0.txt", note: "GPLv2 全文" },
    { id: "GPL-3.0", from: join(TL, "doc", "fonts", "aesupp", "gpl-3.0.txt"), to: "gpl-3.0.txt", note: "GPLv3 全文" },
    { id: "LGPL-2.1", from: join(TL, "doc", "latex", "songbook", "LesserGPL.txt"), to: "lgpl-2.1.txt", note: "LGPL 2.1 全文" },
    { id: "GFL-1.0", from: join(TL, "doc", "fonts", "lm", "GUST-FONT-LICENSE.TXT"), to: "gfl-1.0.txt", note: "GUST Font License 1.0（= LPPL1.3c + 改名请求）" },
    { id: "OFL-1.1", from: join(TL, "doc", "fonts", "amsfonts", "OFL.txt"), to: "ofl-1.1.txt", note: "SIL OFL 1.1 + AMS Reserved Font Names（随包 OFL 内容为 amsfonts）" },
    { id: "GFDL-1.3", from: join(TL, "doc", "latex", "beamer", "licenses", "fdl.txt"), to: "gfdl-1.3.txt", note: "GNU FDL 1.3（beamer/pgf 文档部分）" },
    { id: "GPL-3.0-Fandol", from: join(FANDOL_SRC, "COPYING"), to: "gpl-3.0-fandol.txt", note: "GPLv3 全文 + Fandol 字体嵌入例外（Fandol COPYING 原文）" },
    { id: "MPL-2.0", from: findRegistryLicenseText("Mozilla Public License Version 2.0"), to: "mpl-2.0.txt", note: "MPL 2.0 全文（取自 cargo registry 中 MPL-2.0 crate 的许可文件，去重后）" },
    {
      id: "Knuth-TeX-fonts",
      from: join(TL, "fonts", "source", "public", "cm", "cmbase.mf"),
      to: "knuth-tex-fonts.txt",
      note: "Knuth TeX/CM 字体许可声明（逐字取自 TL 的 CM 源文件 cmbase.mf 文件头；Knuth 许可的实质就是「未改名不得修改」的短声明 + 允许未修改分发）",
      // 只抽文件头的许可声明，不把 29KB 的 METAFONT 源码当许可文本
      extract: (text) =>
        text
          .split("\n")
          .filter((l) => /MUST NOT BE MODIFIED|OFFICIAL COMPUTER MODERN SOURCE/.test(l))
          .join("\n") +
        "\n\n（以上为 Computer Modern / TeX 字体许可声明原文，摘自 TeX Live 2026 " +
        "fonts/source/public/cm/cmbase.mf 文件头。该许可要求：可以复制与分发**未修改**的文件；" +
        "修改后必须更改文件名。完整上游发布见 CTAN systems/knuth（https://ctan.org/pkg/knuth）。" +
        "本安装包内的 CM 字体文件（cmr*.pfb/tfm、cmmi*、cmsy*、cmex* 等）未做任何修改。）\n",
    },
  ];
  mkdirSync(LICENSES, { recursive: true });
  for (const t of licenseTexts) {
    if (!t.from) throw new Error(`缺许可文本来源：${t.id}`);
    if (!existsSync(t.from)) throw new Error(`缺许可文本：${t.from}`);
    if (t.extract) {
      writeFileSync(join(LICENSES, t.to), t.extract(readFileSync(t.from, "utf8")));
    } else {
      copyFileSync(t.from, join(LICENSES, t.to));
    }
  }
  writeJson(join(DATA, "license-texts.json"), {
    note: "规范许可证全文（canonical）。Rust crate 的逐 crate 许可文本去重存 LICENSES/rust/，C 依赖见 LICENSES/c-deps/。",
    texts: licenseTexts.map((t) => ({
      id: t.id,
      file: t.from ? rel(join(LICENSES, t.to)) : null,
      note: t.note,
      sha256: t.from ? sha256(readFileSync(join(LICENSES, t.to))) : null,
    })),
  });
  log(`采集完成：Rust crate=${rustDeps.length}，C port=${ports.length}，Fandol 字体源=${fontSources.length}，LICENSES 文件=${walk(LICENSES).length}`);
  return true;
}

function firstExisting(...ps) {
  return ps.find((p) => p && existsSync(p)) ?? null;
}

/** 在 cargo registry 的 crate 源码里找含指定许可正文的文件（用于 MPL/Unicode/CDLA 等规范全文）。 */
function findRegistryLicenseText(needle) {
  const srcRoot = join(process.env.USERPROFILE ?? process.env.HOME ?? "", ".cargo", "registry", "src");
  if (!existsSync(srcRoot)) return null;
  for (const reg of readdirSync(srcRoot)) {
    const regDir = join(srcRoot, reg);
    let crates = [];
    try {
      crates = readdirSync(regDir);
    } catch {
      continue;
    }
    for (const crate of crates.slice(0, 4000)) {
      const dir = join(regDir, crate);
      let entries = [];
      try {
        entries = readdirSync(dir);
      } catch {
        continue;
      }
      for (const f of entries) {
        if (!/^(LICEN[CS]E|COPYING)/i.test(f)) continue;
        const p = join(dir, f);
        try {
          if (statSync(p).isFile() && readFileSync(p, "utf8").includes(needle)) return p;
        } catch {
          /* 跳过二进制/不可读 */
        }
      }
    }
  }
  return null;
}

/** 极简限流 cargo 可执行（PATH 上有 cargo；本机 DSH 环境也满足）。 */
function cargoBin() {
  return process.platform === "win32" ? "cargo.exe" : "cargo";
}

function rmrf(p) {
  if (existsSync(p)) rmSync(p, { recursive: true, force: true });
}

// ---------------------------------------------------------------- generate

function buildNotice() {
  const meta = readJson(join(DATA, "meta.json"));
  const bundlePkgs = readJson(join(DATA, "bundle-packages.json"));
  const unmatched = readJson(join(DATA, "unmatched-files.json"));
  const rust = readJson(join(DATA, "rust-deps.json"));
  const cDeps = readJson(join(DATA, "c-deps.json"));
  const patched = readJson(join(DATA, "patched-tl-files.json"));
  const sources = readJson(join(DATA, "sources.json"));
  const licenseTexts = readJson(join(DATA, "license-texts.json"));

  const licFile = (id) => licenseTexts.texts.find((t) => t.id === id)?.file ?? null;
  const L = [];
  const h = (n, s) => L.push(`${"#".repeat(n)} ${s}`, "");
  const p = (s = "") => L.push(s);

  h(1, "Latteset 第三方声明（THIRD-PARTY-NOTICES）");
  p(`本安装包包含第三方软件与字体。本文件列出其版权与许可信息；完整许可文本见同目录 \`LICENSES/\`，`);
  p(`GPL/LGPL 组件的对应源码见 \`sources/\`，被修改过的 TeX Live 文件补丁见 \`patches/\`。`);
  p();
  p(`- 生成时间：${meta.generatedAt}`);
  p(`- 审计日期：${meta.auditDate}（G-L1..G-L9，见仓库 \`${meta.licensingDoc}\`）`);
  p(`- 引擎：${meta.engine.name} ${meta.engine.version}（format serial ${meta.engine.formatSerial}，MIT）`);
  p(`- 内置精简宏包集（bundle）：digest \`${meta.bundle.digest}\`，${meta.bundle.files} 文件 / ${meta.bundle.uncompressedBytes} B（zip ${meta.bundle.zipBytes} B，sha256 \`${meta.bundle.zipSha256}\`）`);
  p(`- bundle 来源：${meta.bundle.source}（TeX Live 血统：${meta.bundle.texLiveLineage}）`);
  p(`- C 依赖：vcpkg ${meta.vcpkg.triplet}（pin \`${meta.vcpkg.pinRev}\`）`);
  p();
  p(`> 若本文件与随包文件不一致，以 \`manifest.json\` 记录的 sha256 为准（\`node scripts/make-notices.mjs --check\` 可复核）。`);
  p();

  h(2, "Tectonic（MIT）");
  p("本产品的 TeX 引擎 Tectonic 以 MIT 许可分发：");
  p();
  p("```");
  p(readFileSync(join(LIC_ROOT, licFile("MIT-Tectonic")), "utf8").trim());
  p("```");
  p();

  h(2, "Rust 依赖（清单 A）");
  p(`随二进制链接的 crates.io 运行时依赖共 **${rust.count}** 个（不含本仓库 workspace 成员）。逐 crate 的许可标识与许可文本路径：`);
  p();
  const byLicense = new Map();
  for (const c of rust.crates) {
    const key = c.license;
    if (!byLicense.has(key)) byLicense.set(key, []);
    byLicense.get(key).push(c);
  }
  p("| 许可标识 | crate 数 | crate（name version） |");
  p("|---|---|---|");
  for (const [lic, list] of [...byLicense.entries()].sort((a, b) => b[1].length - a[1].length)) {
    p(`| \`${lic}\` | ${list.length} | ${list.slice(0, 12).map((c) => `\`${c.name} ${c.version}\``).join("、")}${list.length > 12 ? "、…" : ""} |`);
  }
  p();
  p("> 逐个 crate 的版权行/许可原文：见 `LICENSES/rust/`（按文件内容去重，路径与 crate 的对应关系见 `data/rust-deps.json`）。");
  p("> MPL-2.0 组件（如 `windows`/`winapi` 系、`option-ext` 等）按 MPL-2.0 §3.1 以**未修改**形式静态链接；其源码可从 crates.io 对应版本获取（`Cargo.lock` 钉版本）。");
  p();

  h(2, "C/C++ 依赖（清单 B：vcpkg 静态链接，11 个 port）");
  p("| port | 许可摘要 | 版权/许可全文 |");
  p("|---|---|---|");
  for (const c of cDeps.ports) {
    p(`| \`${c.port}\` | 见全文（${c.bytes} B） | \`${c.copyright}\` |`);
  }
  p();
  p("- FreeType 采用 **FTL**（FreeType License，BSD 风格）或其 GPLv2 双许可；本产品选择 **FTL**，因此不触发 GPL 传染。");
  p("- fontconfig / graphite2 为 **MPL-2.0**；本产品未修改其源码，按 MPL-2.0 静态链接。");
  p("- ICU 为 Unicode License；harfbuzz/brotli/dirent/expat/zlib/bzip2/libpng 为宽松许可。全文见上表文件。");
  p("- `fontconfig` 的 `nls` 特性**未开启**（避免引入 LGPL 的 gettext-libintl，见许可审计 §3 约束②）。");
  p();

  h(2, "内置精简宏包集（bundle）逐包许可证");
  p(`随包 bundle 共 ${bundlePkgs.packageCount} 个 TL 包（归属自 TL 2026 tlpdb 映射；10 个生成物/产品文件见文末）。许可分布与全文：`);
  p();
  const licHist = new Map();
  for (const b of bundlePkgs.packages) {
    const key = b.license ?? "(none)";
    licHist.set(key, (licHist.get(key) ?? 0) + 1);
  }
  p("| 许可证 | 包数 | 对应规范全文 |");
  p("|---|---|---|");
  const licMap = {
    "lppl1.3c": "LPPL-1.3c",
    "lppl1.3": "LPPL-1.3c",
    "lppl1.3c_gpl2_fdl": "LPPL-1.3c + GPL-2.0 + GFDL-1.3",
    "lppl1.3c_gpl2": "LPPL-1.3c + GPL-2.0",
    lppl: "LPPL-1.3c",
    gpl2: "GPL-2.0",
    gpl: "GPL-2.0",
    gpl2plus: "GPL-2.0",
    lgpl: "LGPL-2.1",
    ofl: "OFL-1.1",
    gfl: "GFL-1.0",
    knuth: "Knuth（见 §6 说明）",
    pd: "公有领域（无义务）",
    mit: "LICENSES/rust/ 或上游（见 §3）",
    x11: "X11/MIT 风格（见 §3 LICENSES/rust/）",
    other_free: "各包自带的 free 许可（见包名与 TL 上游）",
    none: "无 catalogue-license 字段：文件头声明见 §5.3 表",
  };
  for (const [lic, n] of [...licHist.entries()].sort((a, b) => b[1] - a[1])) {
    const ref = licMap[lic.replace(/ /g, "_")] ?? licMap[lic] ?? "见该包 TL 上游";
    p(`| \`${lic}\` | ${n} | ${ref} |`);
  }
  p();
  p("### 5.1 完整文本文件");
  p();
  for (const id of ["lppl-1.3c", "gpl-2.0", "gpl-3.0", "lgpl-2.1", "gfl-1.0", "ofl-1.1", "gfdl-1.3", "mpl-2.0"]) {
    const f = licFile(id);
    if (f) p(`- \`${f}\` — ${id}`);
  }
  p();
  p("### 5.2 字体类许可（G-L5）");
  p();
  p("- **Fandol**（6 个 OTF）：GPLv3 + 字体嵌入例外 ⇒ 见 §7 的 Corresponding Source；全文 `LICENSES/gpl-3.0-fandol.txt`。");
  p("- **Latin Modern**（lmroman/lmsans/lmmono，GFL-1.0）：GFL 全文 `LICENSES/gfl-1.0.txt`；本产品未改字体名、未修改字体文件。");
  p("- **AMS fonts**（amsfonts，OFL-1.1）：`LICENSES/ofl-1.1.txt`（含 Reserved Font Names 清单，未改名）。");
  p("- **Computer Modern 系**（cm/mm/cm-super 的 TFN/PFB，Knuth 许可）：按 §6 说明分发；未修改。");
  p("- **zapfding**（pzdr.tfm，GPL）：源码见 §7；AFM 随包。");
  p();
  p("### 5.3 无 catalogue-license 文件（47 项）的文件头声明");
  p();
  p("TL 的 hyph-utf8 系列（`hyphen-*` patterns）与 `tlperl.windows`（Unicode 数据文件）在 tlpdb 里无 `catalogue-license` 字段，但**逐文件头**均有版权/许可声明（GPL 或 LPPL 或 Unicode 许可）。审计脚本 `test_file/audit_bundle_heads.py` 的输出（47/47 覆盖，0 项无声明）见许可审计 §8.3；随包文本：`dehyph*-x-2022-03-16.*`、`hyph-quote-rm.tex`、`SpecialCasing.txt`、`CaseFolding.txt`、`UnicodeData.txt` 等。");
  p();

  h(2, "Knuth 字体许可（Computer Modern / TeX 字体）");
  p("随包 bundle 含 Computer Modern 家族（`cmr10.pfb`、`cmmi*.pfb`、`cmsy*.pfb` 等，tlpdb 许可证字段 `knuth`）。");
  p("Knuth 的 TeX 字体许可允许**未修改**分发并用于排版文档，但要求：① 分发不得修改文件内容；② 必须附带本声明（本文件即声明）；");
  p("③ 完整许可原文以上游为准：CTAN `systems/knuth/dist/` 的 `license` 文本（https://ctan.org/pkg/knuth）。");
  p("本产品**未修改**这些字体文件（逐文件 sha256 由 `data/bundle-packages.json` + bundle SHA256SUM 锚定）。");
  p();

  h(2, "GPL 字体的 Corresponding Source（随包）");
  p("GPL 许可要求随二进制提供**对应源码**（GPLv3 §6）。本安装包 `licenses/sources/` 内附：");
  p();
  p("| 随包二进制 | 包/许可 | 源码目录 | 文件数 / 字节 |");
  p("|---|---|---|---|");
  for (const s of sources.fonts) {
    const bytes = s.files.reduce((a, b) => a + b.bytes, 0);
    p(`| \`${s.binary}\` | ${s.package} / ${s.license} | \`${s.sourceDir}/\` | ${s.files.length} / ${bytes} |`);
  }
  p();
  p("- Fandol 源码来自上游仓库 `" + FANDOL_REPO + "`（pin commit `" + FANDOL_COMMIT + "`），含 `cidfont.ps`/`AFM`/`features`/`fontinfo` 等 AFDKO 构建输入；见 `sources/fandol/SOURCE.md`。");
  p("- 字体嵌入例外：Fandol 的 GPL 附「将字体嵌入文档不使文档受 GPL 约束」例外（`LICENSES/gpl-3.0-fandol.txt` 文末）。");
  p("- 可复现：用 Adobe AFDKO `makeotf` 在各字目录生成 OTF；构建工具与步骤见 SOURCE.md。");
  p();
  p("其余 GPL/LGPL 组件（CJKfntef.sty / fonttext.cfg / ckx.map / hyphen 系列 patterns）的 preferred form 即文本文件本身或其上游 TL 源（cslatex.dtx、dvipdfmx、hyph-utf8），见 `data/sources.json` 的 `selfSourced`。");
  p();

  h(2, "被修改的 TeX Live 文件（G-L6）");
  p("Tectonic 对 TeX Live 打补丁；本随包代表集里**实际包含**的 patch 文件如下（原件获取方式与变更说明）：");
  p();
  for (const f of patched.files) {
    p(`### ${f.file}${f.bundled ? "（随包）" : "（未随包）"}`);
    p();
    p(`- 许可：${f.license}`);
    p(`- 变更说明：${f.changeSummary}`);
    p(`- 许可影响：${f.licenseImpact}`);
    p(`- 补丁：\`${f.patchPath}\`（sha256 \`${f.patchSha256}\`）`);
    if (f.bundledSha256) p(`- 随包文件 sha256：\`${f.bundledSha256}\``);
    p(`- 原件：${f.originalSource}`);
    p();
  }
  p("> LPPL 1.3c 允许分发修改版，但要求明确标注为修改版并给出变更说明（本文件即该声明）；补丁与原件获取方式随包。");
  p();

  h(2, "无法归属的文件（10 项）");
  p("下列文件按 basename 映射不到 TL 包（多为生成物/产品文件），人工归属如下：");
  p();
  p("| 文件 | 归属 |");
  p("|---|---|");
  for (const f of unmatched.files) p(`| \`${f.name}\` | ${f.attribution} |`);
  p();

  h(2, "声明获取与联系方式");
  p("- 本文件与全文位于安装目录的 `licenses/` 下；源码在 `licenses/sources/`。");
  p("- 复审与生成：仓库 `scripts/make-notices.mjs`（`--check` 可对随包物复核；`--collect` 用于换 bundle/依赖后重采集）。");
  p("- 许可审计与门禁记录：仓库 `docs/dev/tectonic-licensing.md`（G-L1..G-L9）与 `docs/adr/0016-ship-bundle-offline.md`。");
  p();

  return L.join("\n");
}

function buildManifest() {
  const files = walk(LIC_ROOT).filter((f) => f !== "manifest.json");
  const entries = files
    .sort()
    .map((f) => ({ path: f, bytes: statSync(join(LIC_ROOT, f)).size, sha256: sha256(readFileSync(join(LIC_ROOT, f))) }));
  return {
    generatedBy: "scripts/make-notices.mjs",
    notice: "THIRD-PARTY-NOTICES.md",
    files: entries,
    totalFiles: entries.length,
    totalBytes: entries.reduce((a, b) => a + b.bytes, 0),
  };
}

function generate() {
  mkdirSync(LIC_ROOT, { recursive: true });
  const noticeText = buildNotice();
  writeFileSync(NOTICE, noticeText);
  const manifest = buildManifest();
  writeJson(MANIFEST, manifest);
  log(`生成完成：${rel(NOTICE)}（${statSync(NOTICE).size} B），manifest 共 ${manifest.totalFiles} 文件 / ${manifest.totalBytes} B`);
}

function check() {
  const expectedNotice = buildNotice();
  if (!existsSync(NOTICE)) throw new Error("缺 THIRD-PARTY-NOTICES.md（先跑 node scripts/make-notices.mjs）");
  const actual = readFileSync(NOTICE, "utf8");
  if (actual !== expectedNotice) {
    throw new Error("THIRD-PARTY-NOTICES.md 与 data/LICENSES/sources 不一致（重跑 node scripts/make-notices.mjs 并提交）");
  }
  const manifest = buildManifest();
  const committed = readJson(MANIFEST);
  const same = sha256(JSON.stringify(manifest)) === sha256(JSON.stringify(committed));
  if (!same) throw new Error("manifest.json 与当前 licenses/ 内容不一致（重跑 make-notices 并提交）");
  // 关键完整性：GPL 字体源码必须存在
  for (const s of readJson(join(DATA, "sources.json")).fonts) {
    if (!existsSync(join(LIC_ROOT, s.sourceDir))) throw new Error(`缺 GPL 字体源码目录：${s.sourceDir}`);
    for (const f of s.files) {
      if (!existsSync(join(LIC_ROOT, s.sourceDir, f.name))) throw new Error(`缺源码文件：${s.sourceDir}/${f.name}`);
    }
  }
  for (const t of readJson(join(DATA, "license-texts.json")).texts) {
    if (t.file && !existsSync(join(LIC_ROOT, t.file))) throw new Error(`缺许可文本：${t.file}`);
  }
  log(
    `--check 通过：NOTICE ${actual.length} B；licenses 共 ${manifest.totalFiles} 文件 / ` +
      `${(manifest.totalBytes / 1024 / 1024).toFixed(2)} MiB；sources ${readJson(join(DATA, "sources.json")).fonts.length} 组`,
  );
}

try {
  if (COLLECT) {
    collect();
    generate();
    check();
  } else if (CHECK) {
    check();
  } else {
    generate();
    check();
  }
} catch (e) {
  console.error(`[make-notices] 失败：${e?.stack ?? e}`);
  process.exit(1);
}
