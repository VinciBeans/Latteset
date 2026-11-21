#!/usr/bin/env node
// 阶段③ T16：把"代表性文档集"目录 bundle 打成**随安装包分发的产品精简宏包集**（单个 `.zip`）。
//
// 形态选择（任务书 T16 第 1 条要求记录理由）：
// - 上游 `tectonic_bundles::detect_bundle` 对**本地文件**按扩展名分派：`.zip` → `ZipBundle`、
//   `.ttb` → `TTBFsBundle`、目录 → `DirBundle`；`.ttb` 在 Tectonic 0.17 的 CLI 里**没有**公开的
//   生成入口（`tectonic bundle create` 不存在），自研 `.ttb` 写入器要新增一份二进制格式代码，
//   收益只是"和上游同形态"。⇒ 选 **zip**：单文件（NSIS 里 1 个资源文件，不是 500+ 个）、
//   标准工具链可读、`ZipBundle` 是上游一等公民（解压按需 seek，冷编实测与 DirBundle 同量级）。
// - 体积实测见 `test_file/phase3/PRODUCT-BUNDLE.md`：52 MiB 目录 → zip ≈ 45 MiB（字体已压缩，
//   deflate 收益主要来自 .sty/.tex/.map）；NSIS 安装包上限 200 MB（产品负责人 2026-11-20 给的数）。
//
// 用法（仓库根执行）：
//   node scripts/make-bundle.mjs                 # 从 test_file/tectonic-bundle 生成产品 zip
//   node scripts/make-bundle.mjs --verify        # 只校验既有 zip（CI 用；不重新生成）
//   node scripts/make-bundle.mjs --from=<dir> --out=<zip> --max-zip-mb=120
//
// 输出（stdout JSON + `test_file/phase3/product-bundle.json`）：
//   { ok, digest, files, bytes, zipBytes, zipSha256, out, source, maxZipBytes }
//
// 可复现：zip 内条目按文件名排序、时间戳固定为 1980-01-01、deflate level 固定 ⇒ 同一输入产出
// 同一 zip（`zipSha256` 可比对）。bundle 的 `digest` 仍是目录内容的规范摘要（与测试夹具同一算法：
// 对 "sha256␠␠文件名" 行按文件名排序拼接再 sha256），zip 只是外层容器。

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateRawSync, inflateRawSync } from "node:zlib";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const DEFAULT_FROM = join(ROOT, "test_file", "tectonic-bundle");
const DEFAULT_OUT = join(ROOT, "src-tauri", "resources", "tectonic-bundle.zip");
const REPORT = join(ROOT, "test_file", "phase3", "product-bundle.json");

const argv = process.argv.slice(2);
const arg = (name, dflt) => {
  const hit = argv.find((a) => a.startsWith(`--${name}=`));
  return hit ? hit.slice(name.length + 3) : dflt;
};
const FROM = resolve(ROOT, arg("from", DEFAULT_FROM));
const OUT = resolve(ROOT, arg("out", DEFAULT_OUT));
const MAX_ZIP_MB = Number(arg("max-zip-mb", "120"));
const VERIFY_ONLY = argv.includes("--verify");

const log = (m) => console.log(`[make-bundle] ${m}`);
const sha256 = (buf) => createHash("sha256").update(buf).digest("hex");
const posix = (p) => p.split("\\").join("/");

function walkFiles(dir, base = dir, out = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, e.name);
    if (e.isDirectory()) walkFiles(full, base, out);
    else if (e.isFile()) out.push(posix(relative(base, full)));
  }
  return out;
}

/** 与 scripts/gen-bench-bundle.mjs 相同的 digest 口径（目录内容的规范摘要）。 */
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
  return { digest: sha256(lines.join("")), files: names.length, bytes };
}

// ---------------------------------------------------------------- 确定性 zip 写入

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[i] = c >>> 0;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

/** 固定 DOS 时间戳（1980-01-01 00:00:00）⇒ 产物与机器时钟无关。 */
const DOS_TIME = 0;
const DOS_DATE = (1 << 5) | 1; // 1980-01-01

function dosDateTime() {
  return { time: DOS_TIME, date: DOS_DATE };
}

function buildZip(dir, names) {
  const parts = [];
  const central = [];
  let offset = 0;
  for (const name of names) {
    const raw = readFileSync(join(dir, name));
    const crc = crc32(raw);
    // 已压缩格式（otf/pfb/ttf 等）deflate 收益为负 ⇒ 存 store，避免白花时间
    const deflated = deflateRawSync(raw, { level: 6 });
    const useDeflate = deflated.length < raw.length;
    const data = useDeflate ? deflated : raw;
    const method = useDeflate ? 8 : 0;
    const nameBuf = Buffer.from(posix(name), "utf8");
    const { time, date } = dosDateTime();

    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4); // version needed
    local.writeUInt16LE(0, 6); // flags（UTF-8 名不乱码：名字都是 ASCII）
    local.writeUInt16LE(method, 8);
    local.writeUInt16LE(time, 10);
    local.writeUInt16LE(date, 12);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(raw.length, 22);
    local.writeUInt16LE(nameBuf.length, 26);
    local.writeUInt16LE(0, 28); // extra len
    parts.push(local, nameBuf, data);

    const cd = Buffer.alloc(46);
    cd.writeUInt32LE(0x02014b50, 0);
    cd.writeUInt16LE(20, 4); // version made by
    cd.writeUInt16LE(20, 6); // version needed
    cd.writeUInt16LE(0, 8); // flags
    cd.writeUInt16LE(method, 10);
    cd.writeUInt16LE(time, 12);
    cd.writeUInt16LE(date, 14);
    cd.writeUInt32LE(crc, 16);
    cd.writeUInt32LE(data.length, 20);
    cd.writeUInt32LE(raw.length, 24);
    cd.writeUInt16LE(nameBuf.length, 28);
    cd.writeUInt16LE(0, 30); // extra
    cd.writeUInt16LE(0, 32); // comment
    cd.writeUInt16LE(0, 34); // disk
    cd.writeUInt16LE(0, 36); // internal attrs
    cd.writeUInt32LE(0, 38); // external attrs
    cd.writeUInt32LE(offset, 42);
    central.push(Buffer.concat([cd, nameBuf]));
    offset += local.length + nameBuf.length + data.length;
  }
  const centralBuf = Buffer.concat(central);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(0, 4);
  eocd.writeUInt16LE(0, 6);
  eocd.writeUInt16LE(names.length, 8);
  eocd.writeUInt16LE(names.length, 10);
  eocd.writeUInt32LE(centralBuf.length, 12);
  eocd.writeUInt32LE(offset, 16);
  eocd.writeUInt16LE(0, 20);
  return Buffer.concat([...parts, centralBuf, eocd]);
}

/** 轻量校验：解析 EOCD + 中央目录，确认 zip 完整、条目数、SHA256SUM 内容 = digest。 */
function verifyZip(zipPath, expect) {
  const buf = readFileSync(zipPath);
  const eocdIdx = buf.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocdIdx < 0) throw new Error("zip 缺 EOCD");
  const count = buf.readUInt16LE(eocdIdx + 10);
  const cdSize = buf.readUInt32LE(eocdIdx + 12);
  const cdOff = buf.readUInt32LE(eocdIdx + 16);
  let p = cdOff;
  const names = [];
  let sumBytes = 0;
  let sha256sumContent = null;
  let sha256sumSize = 0;
  for (let i = 0; i < count; i++) {
    if (buf.readUInt32LE(p) !== 0x02014b50) throw new Error(`中央目录第 ${i} 条签名坏`);
    const method = buf.readUInt16LE(p + 10);
    const compSize = buf.readUInt32LE(p + 20);
    const uncompSize = buf.readUInt32LE(p + 24);
    const nameLen = buf.readUInt16LE(p + 28);
    const extraLen = buf.readUInt16LE(p + 30);
    const commentLen = buf.readUInt16LE(p + 32);
    const localOff = buf.readUInt32LE(p + 42);
    const name = buf.subarray(p + 46, p + 46 + nameLen).toString("utf8");
    names.push(name);
    sumBytes += uncompSize;
    if (name === "SHA256SUM") {
      const ln = buf.readUInt16LE(localOff + 26);
      const le = buf.readUInt16LE(localOff + 28);
      const start = localOff + 30 + ln + le;
      const data = buf.subarray(start, start + compSize);
      sha256sumSize = uncompSize;
      sha256sumContent = method === 8 ? inflateRawSync(data).toString("utf8") : data.toString("utf8");
    }
    p += 46 + nameLen + extraLen + commentLen;
  }
  if (p !== cdOff + cdSize) throw new Error("中央目录长度对不上（zip 损坏）");
  if (expect) {
    if (count !== expect.files + 1) throw new Error(`条目数 ${count} != ${expect.files + 1}（含 SHA256SUM）`);
    if (sha256sumContent?.trim() !== expect.digest) {
      throw new Error(`zip 内 SHA256SUM=${JSON.stringify(sha256sumContent)} != ${expect.digest}`);
    }
    if (sumBytes !== expect.bytes + sha256sumSize) {
      throw new Error(`解压后字节 ${sumBytes} != ${expect.bytes} + SHA256SUM(${sha256sumSize})`);
    }
  }
  return { count, sumBytes, sha256sumContent: sha256sumContent?.trim() ?? null, names };
}

// ---------------------------------------------------------------- main

function main() {
  if (VERIFY_ONLY) {
    if (!existsSync(OUT)) throw new Error(`zip 不存在：${OUT}（先跑 node scripts/make-bundle.mjs）`);
    const meta = existsSync(REPORT) ? JSON.parse(readFileSync(REPORT, "utf8")) : null;
    const v = verifyZip(OUT, meta);
    const zipBytes = statSync(OUT).size;
    const ok = !meta || v.sha256sumContent === meta.digest;
    log(`verify ${OUT}: entries=${v.count} bytes=${zipBytes} digest=${v.sha256sumContent}`);
    const result = { ok, mode: "verify", out: posix(relative(ROOT, OUT)), ...v, zipBytes };
    console.log(JSON.stringify(result, null, 2));
    if (!ok) process.exit(1);
    return;
  }

  if (!existsSync(join(FROM, "SHA256SUM"))) throw new Error(`源目录不是合法 DirBundle（缺 SHA256SUM）：${FROM}`);
  const stored = readFileSync(join(FROM, "SHA256SUM"), "utf8").trim();
  const { digest, files, bytes } = computeDigest(FROM);
  if (stored !== digest) {
    throw new Error(
      `源 bundle 的 SHA256SUM 与内容不一致：文件写 ${stored}，重算 ${digest}（源目录被改过；先跑 scripts/gen-bench-bundle.mjs 重建）`,
    );
  }
  log(`源：${FROM}（digest=${digest} files=${files} bytes=${bytes}）`);

  const names = walkFiles(FROM).sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
  const zip = buildZip(FROM, names);
  mkdirSync(dirname(OUT), { recursive: true });
  writeFileSync(OUT, zip);
  const zipBytes = zip.length;
  const zipSha256 = sha256(zip);
  mkdirSync(dirname(REPORT), { recursive: true });
  const result = {
    ok: zipBytes <= MAX_ZIP_MB * 1024 * 1024,
    mode: "build",
    from: posix(relative(ROOT, FROM)),
    out: posix(relative(ROOT, OUT)),
    digest,
    files,
    bytes,
    zipBytes,
    zipSha256,
    maxZipBytes: MAX_ZIP_MB * 1024 * 1024,
    generatedAt: new Date().toISOString(),
  };
  writeFileSync(REPORT, JSON.stringify(result, null, 2));
  log(`产物：${OUT}（zip=${(zipBytes / 1024 / 1024).toFixed(2)} MiB / 解压后=${(bytes / 1024 / 1024).toFixed(2)} MiB）`);
  log(`digest=${digest} zipSha256=${zipSha256}`);
  console.log(JSON.stringify(result, null, 2));
  if (!result.ok) {
    console.error(`[make-bundle] zip 超过 --max-zip-mb=${MAX_ZIP_MB}（实际 ${(zipBytes / 1024 / 1024).toFixed(2)} MiB）`);
    process.exit(1);
  }
}

try {
  main();
} catch (e) {
  console.error(`[make-bundle] 失败：${e?.stack ?? e}`);
  process.exit(1);
}
