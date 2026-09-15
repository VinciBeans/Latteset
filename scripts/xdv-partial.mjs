#!/usr/bin/env node
// 「页前缀 + 合成 postamble」→ **部分 XDV**：流式预览（边编边出图）的输入构造器。
//
// 为什么需要它：编译期只有前缀，而 `xdvipdfmx`（外部）与库内的 `XdvipdfmxEngine` 都要求一个
// 带 postamble 的完整 DVI。本脚本把 `[0 .. 第 N 页 eop]` 缝成"看起来完整"的 XDV：
//
//   pre … 各页（bop…eop）  ‖  post(248)+28B（改 last_bop / 页数 t）  字体定义  post_post(249)+指针+id
//
// 三条必须照做的（当年实测踩过的坑，见 docs/research/stage2-streaming-feasibility.md §3 与
// docs/research/tex-ide-roadmap-priority.md:193）：
//   ① 字体定义要**同时收** `define_native_font`(252) 与经典 `fnt_def`(243–246)，放在 post 头之后；
//   ② `post.last_bop` 必须指前缀里**最后一个** bop；`t` 必须是**前缀页数**；
//   ③ `post_post` 的指针要指本文件里的 post 偏移；id 是**1 字节**（XDV=7），尾部补 `0xDF` 到 4 字节对齐。
//
// 用法：
//   node scripts/xdv-partial.mjs <full.xdv> --pages N --out <partial.xdv> [--json]
//   node scripts/xdv-partial.mjs <full.xdv> --ratio 0.5 --out <partial.xdv>
//   node scripts/xdv-partial.mjs <full.xdv> --pages all --out <rebuilt.xdv>   # 自检：整份重建
//
// 自检口径（先跑 `--pages all`）：重建件交 `xdvipdfmx` 应得到**与原文件相同的页数**。

import { readFileSync, writeFileSync } from 'node:fs';
import { parseXdv, payloadLength } from './xdv-report.mjs';

const POST = 248;
const POST_POST = 249;
const NATIVE_FONT = 252;
const FNT_DEF = new Set([243, 244, 245, 246]);

const argv = process.argv.slice(2);
const file = argv.find((a) => !a.startsWith('--') && !/^\d+(\.\d+)?$/.test(a) && !a.endsWith('.xdv.tmp'));
const out = argv.includes('--out') ? argv[argv.indexOf('--out') + 1] : null;
const pagesArg = argv.includes('--pages') ? argv[argv.indexOf('--pages') + 1] : null;
const ratioArg = argv.includes('--ratio') ? Number(argv[argv.indexOf('--ratio') + 1]) : null;
const asJson = argv.includes('--json');

if (!file || !out) {
  console.error('用法：node scripts/xdv-partial.mjs <full.xdv> (--pages N|all|--ratio f) --out <partial.xdv> [--json]');
  process.exit(2);
}

const t0 = Date.now();
const buf = readFileSync(file);
const r = parseXdv(buf);
const all = r.pages;
if (all.length === 0) {
  console.error('解析不出任何完整页 —— 这份 XDV 可能本身就不完整');
  process.exit(1);
}

/** 取样页数：`all` / 数字 / ratio。 */
let take;
if (pagesArg === 'all') take = all.length;
else if (pagesArg) take = Math.max(1, Math.min(all.length, Number(pagesArg)));
else if (ratioArg) take = Math.max(1, Math.min(all.length, Math.round(all.length * ratioArg)));
else take = all.length;

const prefixEnd = all[take - 1].eopOffset + 1; // eop 字节含在内
const prefix = buf.subarray(0, prefixEnd);

// ① 收前缀里的字体定义（前缀里它们只出现在页之前的那一段，但整段走一遍最稳）
const fontDefs = [];
for (let p = 0; p < prefix.length; ) {
  const len = payloadLength(prefix, p);
  if (len === null) break; // 末尾不完整：前缀本来就切在页边界上，正常不该走到
  const op = prefix[p];
  if (FNT_DEF.has(op) || op === NATIVE_FONT) fontDefs.push(prefix.subarray(p, p + 1 + len));
  p += 1 + len;
}
const fontBytes = Buffer.concat(fontDefs);

// ② 原文件的 post 头（num/den/mag + l/u/s 是"目前为止的最大值"，照抄整份的即可）
const lastEop = all[all.length - 1].eopOffset;
let realPost = lastEop + 1;
if (buf[realPost] !== POST) {
  realPost = -1;
  for (let p = lastEop + 1; p < Math.min(buf.length, lastEop + 4096); p++) {
    if (buf[p] === POST) { realPost = p; break; }
  }
}
const postBase = realPost >= 0 ? buf.subarray(realPost + 1, realPost + 29) : Buffer.alloc(28);

// ③ post 头：整份的 payload + 覆盖 last_bop 与页数
//    ⚠ 顺序是 **post 头 → 字体定义 → post_post**（实测真实 XDV 的收尾就是这样；写成
//    "字体定义在 post 之前"会被 xdvipdfmx 判成 `Tried to select a font that hasn't been defined`）。
const postOffset = prefix.length;
const post = Buffer.alloc(29);
post[0] = POST;
postBase.copy(post, 1);
post.writeInt32BE(all[take - 1].bopOffset, 1); // last_bop
post.writeUInt16BE(take, 27); // t = 页数

// ④ post_post：指针 + 1 字节 id(7) + 0xDF 补齐到 4 字节对齐（至少 4 个）
const pp = Buffer.alloc(1 + 4 + 1);
pp[0] = POST_POST;
pp.writeInt32BE(postOffset, 1);
pp[5] = 7;
let pad = 4;
while ((prefix.length + post.length + fontBytes.length + pp.length + pad) % 4 !== 0) pad++;
const outBuf = Buffer.concat([prefix, post, fontBytes, pp, Buffer.alloc(pad, 0xdf)]);

writeFileSync(out, outBuf);
const report = {
  case: 'xdv-partial',
  source: file,
  source_bytes: buf.length,
  complete_pages_in_source: all.length,
  pages_included: take,
  prefix_bytes: prefix.length,
  font_defs: fontDefs.length,
  font_defs_bytes: fontBytes.length,
  post_offset: postOffset,
  post_post_offset: postOffset + post.length + fontBytes.length,
  last_bop: all[take - 1].bopOffset,
  out_bytes: outBuf.length,
  pad_df: pad,
  build_ms: Date.now() - t0,
  out,
};
if (asJson) console.log(JSON.stringify(report));
else console.log(JSON.stringify(report, null, 2));
