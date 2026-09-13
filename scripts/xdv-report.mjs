#!/usr/bin/env node
// XDV/DVI 页索引与页级差分（G2「字节偏移重同步」的可执行判据）。
//
// 用途：
//   node scripts/xdv-report.mjs <file.xdv>                     # 页索引摘要（页数/字节区间/页哈希/解析耗时）
//   node scripts/xdv-report.mjs <file.xdv> --json              # 机器可读
//   node scripts/xdv-report.mjs <file.xdv> --opcodes           # 追加 opcode 直方图与前若干 specials
//   node scripts/xdv-report.mjs <file.xdv> --truncate-at=N     # 按"前 N 字节"解析（模拟编译中被读到的半成品）
//   node scripts/xdv-report.mjs <a.xdv> --diff <b.xdv>         # 页级差分：哪些页的字节内容变了
//   node scripts/xdv-report.mjs <file.xdv> --watch=200         # 轮询增长中的文件，打印"第 N 页何时可用"
//
// 为什么能这么解析（G2 的机制）：DVI/XDV 的页包是**自包含**的——BOP(139) 带 10 个计数 + 前页偏移，
// 页体由**长度可算的定长指令**组成，EOP(140) 收尾。因此：
//   - 只要能从任意 BOP 起"只算长度不解释内容"地走完一页，就能建立页索引（上游 incdvi.c 的两阶段结构）；
//   - 末尾页在 EOP 落盘时即完整，**不需要 postamble**（dvisvgm 要求 postamble，故它读不了半成品）；
//   - 被截断的文件：丢掉最后不完整页即可（上游 incdvi 的回滚规则）。
// opcode 表来源：Tectonic 的 MIT 实现 `tectonic_xdv`（SetGlyphs=253 / SetTextAndGlyphs=254 /
// DefineNativeFont=252 / Noop=138 / PutRule=137），并补上它未列出的经典 put1..put4(133..136)。
import { readFileSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';

const OPNAME = new Map();
{
  const put = (name, ...codes) => codes.forEach((c) => OPNAME.set(c, name));
  put('set_char', ...Array.from({ length: 128 }, (_, i) => i));
  put('set1', 128); put('set2', 129); put('set3', 130); put('set4', 131);
  put('set_rule', 132);
  put('put1', 133); put('put2', 134); put('put3', 135); put('put4', 136);
  put('put_rule', 137); put('nop', 138);
  put('bop', 139); put('eop', 140); put('push', 141); put('pop', 142);
  put('right1', 143); put('right2', 144); put('right3', 145); put('right4', 146);
  put('w0', 147); put('w1', 148); put('w2', 149); put('w3', 150); put('w4', 151);
  put('x0', 152); put('x1', 153); put('x2', 154); put('x3', 155); put('x4', 156);
  put('down1', 157); put('down2', 158); put('down3', 159); put('down4', 160);
  put('y0', 161); put('y1', 162); put('y2', 163); put('y3', 164); put('y4', 165);
  put('z0', 166); put('z1', 167); put('z2', 168); put('z3', 169); put('z4', 170);
  put('fnt_num', ...Array.from({ length: 64 }, (_, i) => 171 + i));
  put('fnt1', 235); put('fnt2', 236); put('fnt3', 237); put('fnt4', 238);
  put('xxx1', 239); put('xxx2', 240); put('xxx3', 241); put('xxx4', 242);
  put('fnt_def1', 243); put('fnt_def2', 244); put('fnt_def3', 245); put('fnt_def4', 246);
  put('pre', 247); put('post', 248); put('post_post', 249);
  put('define_native_font', 252); put('set_glyphs', 253); put('set_text_and_glyphs', 254);
}

const NATIVE_FLAGS = { COLORED: 0x0200, EXTEND: 0x1000, SLANT: 0x2000, EMBOLDEN: 0x4000 };
const be = (b, p, n) => (n === 1 ? b[p] : n === 2 ? b.readUInt16BE(p) : n === 3 ? (b[p] << 16) | (b[p + 1] << 8) | b[p + 2] : b.readUInt32BE(p));

/** 单条指令的**载荷长度**（不含 opcode 本身）；返回 null = 数据不足（末尾不完整）。 */
function payloadLength(buf, p) {
  const op = buf[p];
  if (op <= 127) return 0; // set_char_0..127
  if (op >= 128 && op <= 131) return op - 127; // set1..4
  if (op === 132) return 8; // set_rule: height+width
  if (op >= 133 && op <= 136) return op - 132; // put1..4
  if (op === 137) return 8; // put_rule
  if (op === 138) return 0; // nop
  if (op === 139) return 44; // bop: 10×i32 counters + i32 prev
  if (op === 140) return 0; // eop
  if (op === 141 || op === 142) return 0; // push / pop
  if (op >= 143 && op <= 146) return op - 142; // right1..4
  if (op === 147) return 0; // w0
  if (op >= 148 && op <= 151) return op - 147; // w1..4
  if (op === 152) return 0; // x0
  if (op >= 153 && op <= 156) return op - 152; // x1..4
  if (op >= 157 && op <= 160) return op - 156; // down1..4
  if (op === 161) return 0; // y0
  if (op >= 162 && op <= 165) return op - 161; // y1..4
  if (op === 166) return 0; // z0
  if (op >= 167 && op <= 170) return op - 166; // z1..4
  if (op >= 171 && op <= 234) return 0; // fnt_num_0..63
  if (op >= 235 && op <= 238) return op - 234; // fnt1..4
  if (op >= 239 && op <= 242) { // xxx1..4：先长度再载荷
    const sizeBytes = op - 238;
    if (p + 1 + sizeBytes > buf.length) return null;
    const len = be(buf, p + 1, sizeBytes);
    return sizeBytes + len;
  }
  if (op >= 243 && op <= 246) { // fnt_def1..4：fontnum + checksum/scale/design + area/name 长度 + 两个字符串
    const sizeBytes = op - 242;
    let q = p + 1 + sizeBytes + 12;
    if (q + 2 > buf.length) return null;
    const areaLen = buf[q];
    const nameLen = buf[q + 1];
    q += 2 + areaLen + nameLen;
    if (q > buf.length) return null;
    return q - p - 1;
  }
  if (op === 247) { // pre
    if (p + 15 > buf.length) return null;
    const k = buf[p + 14];
    return 14 + k;
  }
  if (op === 248) return 4 + 4 + 4 + 4 + 4 + 4 + 2 + 2; // post（定长 28）
  if (op === 249) return 4 + 1 + 4; // post_post（4 字节指针 + id + 至少 4 个 0xDF）
  if (op === 252) { // define_native_font
    if (p + 11 > buf.length) return null;
    const flags = be(buf, p + 9, 2); // font_num(4) + size(4) 之后
    const nameLen = buf[p + 11];
    let len = 4 + 4 + 2 + 1 + nameLen + 4; // font_num+size+flags+nameLen+name+faceIndex
    if (flags & NATIVE_FLAGS.COLORED) len += 4;
    if (flags & NATIVE_FLAGS.EXTEND) len += 4;
    if (flags & NATIVE_FLAGS.SLANT) len += 4;
    if (flags & NATIVE_FLAGS.EMBOLDEN) len += 4;
    return p + 1 + len > buf.length ? null : len;
  }
  if (op === 253) { // set_glyphs: width(i32) + n(u16) + n×x(i32) + n×y(i32) + n×gid(u16)
    if (p + 7 > buf.length) return null;
    const n = be(buf, p + 5, 2);
    const len = 6 + n * 10;
    return p + 1 + len > buf.length ? null : len;
  }
  if (op === 254) { // set_text_and_glyphs: nChars(u16) + chars + width(i32) + nGlyphs(u16) + x[] + y[] + gid[]
    if (p + 3 > buf.length) return null;
    const nChars = be(buf, p + 1, 2);
    const q = p + 3 + nChars * 2;
    if (q + 6 > buf.length) return null;
    const nGlyphs = be(buf, q + 4, 2);
    const len = q + 6 + nGlyphs * 10 - p - 1;
    return p + 1 + len > buf.length ? null : len;
  }
  return -1; // 未知 opcode（XDV 未定义）
}

/**
 * 解析页索引。**容忍**：末尾不完整页、缺 postamble、未知 opcode（该页作废但之前的页保留）。
 * @returns {{pages: Array, errors: string[], parseMs: number, bytes: number, preamble: object|null}}
 */
export function parseXdv(buf, { opcodes = false, hashLength = 0 } = {}) {
  const t0 = performance.now();
  const errors = [];
  let pages = [];
  let preamble = null;
  let p = 0;
  let hist = new Map();
  let specials = [];
  let fonts = [];

  // 1) 前导：pre（可选，容忍缺）
  if (buf[p] === 247) {
    const len = payloadLength(buf, p);
    if (len !== null) {
      preamble = {
        version: buf[p + 1],
        num: be(buf, p + 2, 4),
        den: be(buf, p + 6, 4),
        mag: be(buf, p + 10, 4),
        comment: buf.slice(p + 15, p + 15 + buf[p + 14]).toString('latin1'),
      };
      p += 1 + len;
    }
  }

  // 2) 逐页：只在 BetweenPages 状态接受 BOP
  let pageIndex = 0;
  while (p < buf.length) {
    const op = buf[p];
    if (op !== 139) {
      // 页外只允许 post（248）/post_post（249）/空白；其它视为结构损坏
      if (op === 248 || op === 249) break;
      // 兼容：某些写入器在页间留 0xDF 填充
      if (op === 0xdf || op === 0) { p += 1; continue; }
      errors.push(`期望 BOP 但遇到 0x${op.toString(16)} @${p}`);
      break;
    }
    const bopAt = p;
    if (p + 45 > buf.length) { errors.push(`BOP @${bopAt} 不完整（尾部被截断）`); break; }
    const counters = [];
    for (let i = 0; i < 10; i++) counters.push(buf.readInt32BE(p + 1 + i * 4));
    const prev = buf.readInt32BE(p + 41);
    p += 45;

    // 页体：只算长度
    let truncated = false;
    let unknownOp = null;
    while (p < buf.length) {
      const o = buf[p];
      if (o === 140) { p += 1; break; } // eop
      const len = payloadLength(buf, p);
      if (len === null) { truncated = true; break; }
      if (len < 0) { unknownOp = o; break; }
      if (opcodes) {
        const name = OPNAME.get(o) ?? `0x${o.toString(16)}`;
        hist.set(name, (hist.get(name) ?? 0) + 1);
        if (o >= 239 && o <= 242 && specials.length < 12) {
          const sizeBytes = o - 238;
          const l = be(buf, p + 1, sizeBytes);
          specials.push(buf.slice(p + 1 + sizeBytes, p + 1 + sizeBytes + l).toString('latin1').slice(0, 100));
        }
        if (o === 252 && fonts.length < 12) {
          const nameLen = buf[p + 11];
          fonts.push({ id: be(buf, p + 1, 4), size: be(buf, p + 5, 4), flags: be(buf, p + 9, 2), name: buf.slice(p + 12, p + 12 + nameLen).toString('latin1') });
        }
      }
      p += 1 + len;
    }
    if (truncated) { errors.push(`页 ${pageIndex + 1} 尾部被截断（丢弃该页）`); break; }
    if (unknownOp !== null) { errors.push(`页 ${pageIndex + 1} 处出现未知 opcode 0x${unknownOp.toString(16)} @${p}（丢弃该页）`); break; }

    const eopAt = p - 1;
    pageIndex += 1;
    const body = buf.slice(bopAt, eopAt + 1);
    pages.push({
      page: pageIndex,
      bopOffset: bopAt,
      eopOffset: eopAt,
      bytes: body.length,
      counters,
      prev,
      hash: createHash('sha1').update(body).digest('hex').slice(0, 16),
    });
  }
  const parseMs = performance.now() - t0;
  return {
    pages,
    errors,
    parseMs,
    bytes: buf.length,
    preamble,
    ...(opcodes ? { opcodeHistogram: Object.fromEntries([...hist.entries()].sort((a, b) => b[1] - a[1])), specials, fonts } : {}),
  };
}

/** 轮询增长中的文件：返回"每一页何时首次完整可见"。 */
async function watch(file, intervalMs, budgetMs) {
  const t0 = Date.now();
  const seen = new Map();
  const timeline = [];
  while (Date.now() - t0 < budgetMs) {
    let size = 0;
    try { size = statSync(file).size; } catch { /* 尚未创建 */ }
    if (size > 0) {
      let buf = null;
      try { buf = readFileSync(file); } catch { /* 被写者占用：本轮跳过 */ }
      if (buf) {
        const r = parseXdv(buf);
        for (const pg of r.pages) {
          if (!seen.has(pg.page)) {
            seen.set(pg.page, true);
            timeline.push({ page: pg.page, atMs: Date.now() - t0, fileBytes: buf.length });
          }
        }
      }
    }
    await new Promise((r) => setTimeout(r, intervalMs));
    // 编译结束（文件大小稳定 400ms 且已有页）→ 收尾
    if (timeline.length && Date.now() - t0 > 600) {
      const st = statSync(file);
      if (st.size === lastSize) { stableCount += 1; if (stableCount >= 3) break; } else { stableCount = 0; }
      lastSize = st.size;
    }
  }
  return timeline;
}
let lastSize = -1, stableCount = 0;

// ---------------------------------------------------------------- CLI（仅直接运行时；被 import 时只导出 parseXdv）
const isMain = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) await cli();
async function cli() {
const argv = process.argv.slice(2);
const file = argv.find((a) => !a.startsWith('--'));
if (!file) {
  console.error('用法：node scripts/xdv-report.mjs <file.xdv> [--json] [--opcodes] [--truncate-at=N] [--diff other.xdv] [--watch=ms]');
  process.exit(2);
}
const flag = (name, dflt = null) => {
  const hit = argv.find((a) => a.startsWith(`--${name}=`));
  return hit ? hit.slice(name.length + 3) : dflt;
};
const truncateAt = flag('truncate-at');
const diffWith = flag('diff');
const watchMs = flag('watch');
const json = argv.includes('--json');

if (watchMs) {
  const tl = await watch(file, Number(watchMs) || 200, Number(flag('budget', '60000')));
  console.log(`页可用时间线（每 ${watchMs}ms 轮询）：`);
  for (const t of tl) console.log(`  第 ${String(t.page).padStart(3)} 页完整可见 @ ${String(t.atMs).padStart(5)}ms（文件 ${(t.fileBytes / 1048576).toFixed(2)} MB）`);
  console.log(`共 ${tl.length} 页；总预算 ${flag('budget', '60000')}ms`);
  process.exit(0);
}

let buf = readFileSync(file);
if (truncateAt) buf = buf.subarray(0, Number(truncateAt));
const r = parseXdv(buf, { opcodes: argv.includes('--opcodes') });

if (diffWith) {
  const other = parseXdv(readFileSync(diffWith));
  const n = Math.max(r.pages.length, other.pages.length);
  const changed = [];
  for (let i = 0; i < n; i++) {
    const a = r.pages[i], b = other.pages[i];
    if (!a || !b || a.hash !== b.hash) changed.push(i + 1);
  }
  const out = { file, other: diffWith, pagesA: r.pages.length, pagesB: other.pages.length, changedPages: changed, changedCount: changed.length };
  if (json) console.log(JSON.stringify(out, null, 2));
  else {
    console.log(`页级差分：A=${r.pages.length} 页 / B=${other.pages.length} 页`);
    console.log(`内容变化的页：${changed.length} 页` + (changed.length ? ` → ${changed.slice(0, 40).join(',')}${changed.length > 40 ? ' …' : ''}` : '（逐页字节完全一致）'));
  }
  process.exit(0);
}

const summary = {
  file,
  bytes: r.bytes,
  pages: r.pages.length,
  parseMs: +r.parseMs.toFixed(2),
  throughputMBps: +(r.bytes / 1048576 / (r.parseMs / 1000)).toFixed(1),
  preamble: r.preamble,
  errors: r.errors,
  pageBytes: { min: Math.min(...r.pages.map((p) => p.bytes)), max: Math.max(...r.pages.map((p) => p.bytes)), avg: Math.round(r.pages.reduce((s, p) => s + p.bytes, 0) / Math.max(1, r.pages.length)) },
};
if (json) {
  console.log(JSON.stringify({ ...summary, pagesDetail: r.pages.map(({ page, bopOffset, eopOffset, bytes, hash }) => ({ page, bopOffset, eopOffset, bytes, hash })) }, null, 2));
} else {
  console.log(`文件：${summary.file}（${(summary.bytes / 1048576).toFixed(2)} MB）`);
  console.log(`pre: version=${r.preamble?.version} num=${r.preamble?.num} den=${r.preamble?.den} mag=${r.preamble?.mag}`);
  console.log(`页数：**${summary.pages}**（页字节 min/avg/max = ${summary.pageBytes.min}/${summary.pageBytes.avg}/${summary.pageBytes.max}）`);
  console.log(`解析：${summary.parseMs}ms（${summary.throughputMBps} MB/s）${truncateAt ? `，按前 ${truncateAt} 字节` : ''}`);
  if (summary.errors.length) console.log(`异常：${summary.errors.join('; ')}`);
  if (argv.includes('--pages')) for (const p of r.pages.slice(0, 20)) console.log(`  第 ${p.page} 页 @${p.bopOffset}..${p.eopOffset}（${p.bytes}B，hash=${p.hash}）`);
  if (argv.includes('--opcodes')) {
    console.log('opcode 直方图：', JSON.stringify(r.opcodeHistogram));
    if (r.fonts?.length) for (const f of r.fonts) console.log(`  字体 #${f.id} size=${f.size} flags=0x${f.flags.toString(16)} ${f.name}`);
    if (r.specials?.length) for (const s of r.specials.slice(0, 6)) console.log(`  special: ${JSON.stringify(s)}`);
  }
}
}
