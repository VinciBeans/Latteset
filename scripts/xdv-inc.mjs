#!/usr/bin/env node
// 阶段 3「增量输出解析」的原型与验证工具（roadmap §5.3 阶段 3 / G2 报告 §10-B.8 留下的"追加式解析器"）。
//
// 上游要求（texpresso-live-rendering-roadmap.md §阶段 3）：
//   ① 维护 offset（已扫描到的字节位置）；② 只做指令长度计算，不解释内容；
//   ③ 遇到结构标记（BOP/EOP）记录字节偏移进页索引；④ **缓冲区变短（回滚）时丢弃末页、offset 退回上一页边界**。
//   性能护栏：成本 ∝ **新增字节数**，而不是 ∝ 总输出大小。验收：每轮解析耗时常数级。
//
// 用法：
//   node scripts/xdv-inc.mjs <file.xdv> --selftest          # 对拍：增量结果 vs 全量 parseXdv（追加/细粒度/回滚三种序列）
//   node scripts/xdv-inc.mjs <file.xdv> --cost[=chunkBytes] # 成本：按 flush 粒度模拟，增量累计 vs 每轮全量累计
//   node scripts/xdv-inc.mjs <file.xdv> --watch=200         # 编译期真实尾随：每轮增量 vs 每轮全量
//
// 结论见 docs/research/stage3-incremental-output-parsing.md。

import { readFileSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';
import { payloadLength, parseXdv } from './xdv-report.mjs';

const BOP = 139;
const EOP = 140;
const POST = 248;
const POST_POST = 249;

/**
 * 追加式 XDV 页索引。
 *
 * 与 `parseXdv(buf)` 的关系：对**同一份缓冲区**，两者在"完整页"上必须逐字段一致
 * （page / bopOffset / eopOffset / bytes / hash）——`--selftest` 就是拿这条做对拍。
 * 差别只在**成本模型**：本类只扫描 `[scanned, buf.length)` 的新增区间，全量版每轮从头扫。
 *
 * 状态机（`state`）：
 *   `pre`     —— 还没扫过 pre（可能跨多轮才凑够字节）
 *   `between` —— 页间（等待 BOP）
 *   `inPage`  —— 页体内（`curBop` 记录当前页的 BOP 偏移）
 *   `done`    —— 遇到 post / post_post
 */
export function createIncXdvIndex() {
  /** 已扫描到的字节位置。**必须停在"下一条指令的起点"**，否则下轮无法续扫。 */
  let scanned = 0;
  let state = 'pre';
  let preamble = null;
  /** 完整页（含 hash），顺序与全量解析一致。 */
  let pages = [];
  /** 当前页 BOP 偏移（-1 = 不在页内）。 */
  let curBop = -1;
  let errors = [];
  /** 命中过的"未知 opcode"（该页作废，与全量解析一致）。 */
  let badOp = null;
  // 统计：累计扫描字节（证明"成本 ∝ 新增字节"用）
  let scannedBytes = 0;
  let updates = 0;
  let totalMs = 0;

  const finishPage = (buf, eopAt) => {
    const body = buf.subarray(curBop, eopAt + 1);
    pages.push({
      page: pages.length + 1,
      bopOffset: curBop,
      eopOffset: eopAt,
      bytes: body.length,
      hash: createHash('sha1').update(body).digest('hex').slice(0, 16),
    });
    curBop = -1;
    state = 'between';
  };

  /** 缓冲变短（回滚）→ 丢弃被截断的页，位置退回到"仍在缓冲区内的最后一个页边界"。 */
  const rollback = (len) => {
    while (pages.length && pages[pages.length - 1].eopOffset >= len) pages.pop();
    curBop = -1;
    badOp = null;
    if (pages.length) {
      scanned = pages[pages.length - 1].eopOffset + 1;
      state = 'between';
    } else {
      // 连第一页都不完整：回到前导之后（前导仍在则保留）
      scanned = preamble ? preEnd : 0;
      state = preamble ? 'between' : 'pre';
    }
  };

  let preEnd = 0; // pre 结束后的位置（回滚时用）

  return {
    /**
     * 喂入"到目前为止写出的全部字节"（追加或截断后的完整缓冲区）。
     * @returns {{newPages: number, scannedNow: number, ms: number}}
     */
    update(buf) {
      const t0 = performance.now();
      const before = pages.length;
      if (buf.length < scanned) rollback(buf.length);

      let p = scanned;
      let scannedNow = 0;
      while (p < buf.length) {
        if (state === 'pre') {
          const len = payloadLength(buf, p);
          // 边界必须自己校验：payloadLength 对**定长指令**不检查缓冲区长（全量解析靠"扫过头就自然结束"兜底，
          // 增量解析不能——越界推进会让下一轮从页中间续扫，永久丢页，见报告 §"增量解析器的第一个坑"）。
          if (len === null || len < 0 || p + 1 + len > buf.length) break;
          if (buf[p] === 247 && len >= 0) {
            preamble = {
              version: buf[p + 1],
              num: buf.readUInt32BE(p + 2),
              den: buf.readUInt32BE(p + 6),
              mag: buf.readUInt32BE(p + 10),
              comment: buf.subarray(p + 15, p + 15 + buf[p + 14]).toString('latin1'),
            };
            p += 1 + len;
            preEnd = p;
            state = 'between';
          } else {
            // 不是 pre：直接进入页间（容忍缺前导，与全量解析一致）
            preEnd = 0;
            state = 'between';
          }
          continue;
        }

        const op = buf[p];
        if (state === 'between') {
          if (op === BOP) {
            if (p + 45 > buf.length) break; // BOP 不完整
            curBop = p;
            p += 45;
            state = 'inPage';
            continue;
          }
          if (op === POST || op === POST_POST) {
            state = 'done';
            break;
          }
          if (op === 0xdf || op === 0) {
            p += 1; // 页间填充
            continue;
          }
          errors.push(`期望 BOP 但遇到 0x${op.toString(16)} @${p}`);
          state = 'done';
          break;
        }

        // inPage
        if (op === EOP) {
          p += 1;
          finishPage(buf, p - 1);
          continue;
        }
        const len = payloadLength(buf, p);
        if (len === null) break; // 页体尾部不完整：停在指令起点，等下一轮
        if (len < 0) {
          errors.push(`页 ${pages.length + 1} 处出现未知 opcode 0x${op.toString(16)} @${p}（丢弃该页）`);
          badOp = op;
          // 与全量解析一致：该页作废，停止扫描
          curBop = -1;
          state = 'done';
          break;
        }
        const end = p + 1 + len;
        if (end > buf.length) break; // 指令本体越界 → 停在指令起点，等下一轮（见上面 pre 分支的说明）
        p = end;
      }

      scannedNow = p - scanned;
      scannedBytes += scannedNow;
      scanned = p;
      updates += 1;
      const ms = performance.now() - t0;
      totalMs += ms;
      return { newPages: pages.length - before, scannedNow, ms };
    },

    /** 当前索引快照（与 `parseXdv(buf).pages` 逐字段可比）。 */
    snapshot() {
      return { pages: pages.map((p) => ({ ...p })), preamble, errors: [...errors], state, scanned, complete: state === 'done', badOp };
    },

    stats() {
      return { updates, scannedBytes, totalMs, msPerUpdate: updates ? totalMs / updates : 0 };
    },
  };
}

/** 对拍：给定缓冲区，比较增量快照与全量解析的完整页序列。 */
function compare(buf, inc) {
  const full = parseXdv(buf);
  const snap = inc.snapshot();
  const keys = ['page', 'bopOffset', 'eopOffset', 'bytes', 'hash'];
  if (snap.pages.length !== full.pages.length) {
    return { ok: false, why: `页数不同：增量 ${snap.pages.length} vs 全量 ${full.pages.length}` };
  }
  for (let i = 0; i < full.pages.length; i++) {
    for (const k of keys) {
      if (snap.pages[i][k] !== full.pages[i][k]) {
        return { ok: false, why: `第 ${i + 1} 页 ${k} 不同：增量 ${snap.pages[i][k]} vs 全量 ${full.pages[i][k]}` };
      }
    }
  }
  return { ok: true, pages: full.pages.length };
}

// ---------------------------------------------------------------- CLI

const isMain = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) await cli();

async function cli() {
  const argv = process.argv.slice(2);
  const file = argv.find((a) => !a.startsWith('--'));
  if (!file) {
    console.error('用法：node scripts/xdv-inc.mjs <file.xdv> --selftest | --cost[=chunkBytes] | --watch=ms');
    process.exit(2);
  }
  const flag = (name, dflt = null) => {
    const hit = argv.find((a) => a.startsWith(`--${name}=`));
    return hit ? hit.slice(name.length + 3) : dflt;
  };
  const buf = readFileSync(file);
  console.log(`文件：${file}（${(buf.length / 1048576).toFixed(2)} MB）`);

  // ---------------- 对拍
  if (argv.includes('--selftest')) {
    const cases = [];
    // ① 一次性喂全量
    cases.push(['一次性喂全量', [buf.length]]);
    // ② 按 0.5MB 分片追加（G2 实测的 flush 粒度）
    const chunk = 512 * 1024;
    cases.push([`按 ${chunk / 1024}KB 分片追加`, Array.from({ length: Math.ceil(buf.length / chunk) }, (_, i) => Math.min((i + 1) * chunk, buf.length))]);
    // ③ 细粒度 4KB 分片（最坏情况：每轮只前进一条指令）
    const fine = 4096;
    cases.push([`按 ${fine / 1024}KB 细粒度追加`, Array.from({ length: Math.ceil(buf.length / fine) }, (_, i) => Math.min((i + 1) * fine, buf.length))]);
    // ④ 回滚：长到 60% → 截断回 30% → 再长到 60%（模拟"编译中途重写缓冲区"）
    const p60 = Math.floor(buf.length * 0.6), p30 = Math.floor(buf.length * 0.3);
    cases.push(['回滚（60% → 30% → 60%）', [p30, p60, ...Array.from({ length: Math.ceil((buf.length - p60) / chunk) }, (_, i) => Math.min(p60 + (i + 1) * chunk, buf.length))]]);

    let allOk = true;
    for (const [name, steps] of cases) {
      const inc = createIncXdvIndex();
      let last = { ok: true };
      for (const len of steps) {
        inc.update(buf.subarray(0, len));
        last = compare(buf.subarray(0, len), inc);
        if (!last.ok) break;
      }
      // 末态必须与"完整文件的全量解析"一致
      const fin = compare(buf, inc);
      const ok = last.ok && fin.ok;
      allOk &&= ok;
      const st = inc.stats();
      console.log(
        `  ${ok ? '✅' : '❌'} ${name}：${steps.length} 轮 → ${fin.ok ? `${fin.pages} 页一致` : fin.why}；累计扫描 ${(st.scannedBytes / 1048576).toFixed(2)} MB / 耗时 ${st.totalMs.toFixed(1)}ms`
      );
      if (!ok) console.log(`     中间步骤失败：${last.ok ? fin.why : last.why}`);
    }
    process.exit(allOk ? 0 : 1);
  }

  // ---------------- 成本
  if (argv.includes('--cost') || flag('cost')) {
    const chunk = Number(flag('cost', String(512 * 1024))) || 512 * 1024;
    const steps = Array.from({ length: Math.ceil(buf.length / chunk) }, (_, i) => Math.min((i + 1) * chunk, buf.length));
    const inc = createIncXdvIndex();
    let incMs = 0;
    for (const len of steps) incMs += inc.update(buf.subarray(0, len)).ms;
    // 对照：每轮全量解析（现状 watch 的做法）
    let fullMs = 0;
    for (const len of steps) fullMs += parseXdv(buf.subarray(0, len)).parseMs;
    const st = inc.stats();
    const r = parseXdv(buf);
    console.log(`\n模拟 ${steps.length} 轮 flush（每轮 +${(chunk / 1024).toFixed(0)}KB，与 G2 实测的 ~0.5MB/110ms 同粒度）：`);
    console.log(`  增量索引：累计扫描 ${(st.scannedBytes / 1048576).toFixed(2)} MB / 累计 ${incMs.toFixed(2)}ms（每轮 ${(incMs / steps.length).toFixed(3)}ms）`);
    console.log(`  每轮全量：累计重扫 ${(steps.reduce((a, l) => a + l, 0) / 1048576).toFixed(2)} MB / 累计 ${fullMs.toFixed(2)}ms（每轮 ${(fullMs / steps.length).toFixed(3)}ms）`);
    console.log(`  → 增量扫描字节数 / 全量 = ${(st.scannedBytes / steps.reduce((a, l) => a + l, 0)).toFixed(3)}；耗时比 = ${(incMs / fullMs).toFixed(3)}`);
    console.log(`  单次全量（4.38MB 级）= ${r.parseMs.toFixed(2)}ms；页数 ${r.pages.length}`);
    process.exit(0);
  }

  // ---------------- 编译期真实尾随
  const watchMs = flag('watch');
  if (watchMs) {
    const interval = Number(watchMs) || 200;
    const budget = Number(flag('budget', '120000'));
    const t0 = Date.now();
    const inc = createIncXdvIndex();
    const rows = [];
    let lastSize = 0;
    let stable = 0;
    let lastIncPages = 0;
    while (Date.now() - t0 < budget) {
      let size = 0;
      try { size = statSync(file).size; } catch { /* 还没创建 */ }
      if (size > 0) {
        let b = null;
        try { b = readFileSync(file); } catch { /* 写者占用 */ }
        if (b) {
          const tInc = performance.now();
          const r1 = inc.update(b);
          const incMs = performance.now() - tInc;
          const tFull = performance.now();
          const full = parseXdv(b);
          const fullMs = performance.now() - tFull;
          rows.push({ atMs: Date.now() - t0, mb: b.length / 1048576, incMs, fullMs, newPages: r1.newPages, incPages: inc.snapshot().pages.length, fullPages: full.pages.length });
          lastIncPages = inc.snapshot().pages.length;
        }
      }
      await new Promise((r) => setTimeout(r, interval));
      if (size === lastSize) { stable += 1; if (stable >= 3 && lastIncPages > 0) break; } else { stable = 0; }
      lastSize = size;
    }
    console.log(`\n编译期尾随（每 ${interval}ms，共 ${rows.length} 轮）：`);
    console.log(`  @ms     文件MB   增量ms   全量ms   本轮新页  增量页数/全量页数`);
    for (const r of rows) {
      console.log(`  ${String(r.atMs).padStart(6)}  ${r.mb.toFixed(2).padStart(8)}  ${r.incMs.toFixed(3).padStart(7)}  ${r.fullMs.toFixed(3).padStart(7)}  ${String(r.newPages).padStart(8)}  ${r.incPages}/${r.fullPages}`);
    }
    const st = inc.stats();
    console.log(`  累计：增量扫描 ${(st.scannedBytes / 1048576).toFixed(2)} MB / ${st.totalMs.toFixed(1)}ms；全量累计 ${rows.reduce((a, r) => a + r.fullMs, 0).toFixed(1)}ms`);
    process.exit(0);
  }

  // ---------------- 前缀一致性（G2「半成品可读」的加强验证）
  if (argv.includes('--prefix') || flag('prefix')) {
    const grid = Number(flag('prefix')) || 4096;
    const full = parseXdv(buf);
    const fullHash = new Map(full.pages.map((p) => [p.page, p.hash]));
    let checked = 0;
    let pagesSeen = 0;
    let bad = 0;
    let firstBad = null;
    let maxPages = 0;
    for (let len = 1; len <= buf.length; len += grid) {
      const r = parseXdv(buf.subarray(0, len));
      checked += 1;
      maxPages = Math.max(maxPages, r.pages.length);
      for (const pg of r.pages) {
        pagesSeen += 1;
        if (fullHash.get(pg.page) !== pg.hash) {
          bad += 1;
          if (!firstBad) firstBad = { len, page: pg.page, got: pg.hash, expect: fullHash.get(pg.page) };
        }
      }
    }
    console.log(`\n前缀一致性（每 ${grid} 字节一个截断点，共 ${checked} 点 / 累计比对 ${pagesSeen} 页）：`);
    console.log(`  与完整文件同名页 hash **不一致**的页：${bad}${firstBad ? `（首个：len=${firstBad.len} 第 ${firstBad.page} 页）` : ''}`);
    console.log(`  截断前缀最多解出 ${maxPages} 页（完整文件 ${full.pages.length} 页）`);
    console.log(`  判据：任何截断点解出的每一页都必须与完整文件逐字节相同（G2 §3.2 的加强版：G2 只测了 6 个点）`);
    process.exit(bad === 0 ? 0 : 1);
  }

  console.error('需要 --selftest / --cost / --prefix / --watch=ms 之一');
  process.exit(2);
}
