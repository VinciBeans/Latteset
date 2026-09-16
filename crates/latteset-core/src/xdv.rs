//! XDV 页索引（roadmap §5.7；docs/research/incremental-edit-x-dvi.md 的机制底座）。
//!
//! 只回答一个问题：**这次编译的每一页，字节内容与上一次是否相同**。
//! 三个用途（B/C/A 三个功能点）：
//! - **B**：页哈希全同 → 不发 `pdf-updated`（跳过预览重载，PDF 逐字节等价）；
//! - **C**：给出"变化页集合"→ 预览只重绘这些页，其余复用 canvas 位图；
//! - **A**：页哈希全同 → 跳过 `xdvipdfmx` 转换，直接复用上一次的 PDF 文件。
//!
//! 设计对照上游 `incdvi.c` 的两阶段结构：这里只做**第一阶段**——只算指令长度、遇 BOP/EOP 记页
//! 边界，不解释指令内容。长度表与 `scripts/xdv-report.mjs`（研究/诊断工具，Node）逐条一致；
//! 那个工具用于产出研究数据，本模块是**产品路径**。
//!
//! **页哈希口径 = V1（2026-09，已知债 #26）**：哈希范围是"`bop` 头去掉尾部 4 B `prev`" + 页体。
//! `prev` 是**上一页 `bop` 的文件偏移**（派生量），任何"前面某页变长"都会让它整体平移，从而把
//! **内容未变的页**也标成"变了"——42 用例矩阵实测：长度类编辑产生 Σ375 页假阳性、raw 精确率
//! 仅 9.64%（改 V1 后假阴性 0、V1 集合 = 渲染真值集合 42/42）。机制、口径定义与量化见
//! `docs/research/page-hash-prev-quant.md`；`scripts/xdv-report.mjs` 仍以 raw 为默认口径（研究工具）。
//!
//! **容错**：末尾截断、未知 opcode → 丢弃该页（前 N 页仍然可用——页包自包含，见 G2 报告 §3.3）。
//! 两个边界判据是 2026-09 修 bug 时确定的，复现它们的行为很重要：
//! ① 消费一条指令前校验 `end <= len`；② 页完整的**唯一**判据是"见到 EOP"。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::types::Engine;

/// XDV 结构标记。
const BOP: u8 = 139;
const EOP: u8 = 140;
const POST: u8 = 248;
const POST_POST: u8 = 249;

/// `define_native_font` 的可选字段位（每个额外占 4 字节）。
const NATIVE_COLORED: u16 = 0x0200;
const NATIVE_EXTEND: u16 = 0x1000;
const NATIVE_SLANT: u16 = 0x2000;
const NATIVE_EMBOLDEN: u16 = 0x4000;

/// 未知 opcode 的哨兵（与"长度 0"区分开）。
const UNKNOWN_OP: usize = usize::MAX;

fn be(buf: &[u8], p: usize, n: usize) -> usize {
    let mut v = 0usize;
    for i in 0..n {
        v = (v << 8) | buf[p + i] as usize;
    }
    v
}

/// 单条指令的**载荷长度**（不含 opcode 字节本身）。
///
/// - `None`：数据不足（末尾截断）——调用方应停在下一条指令的起点；
/// - `Some(UNKNOWN_OP)`：未定义的 opcode——调用方丢弃该页；
/// - `Some(len)`：载荷长度。
fn payload_len(buf: &[u8], p: usize) -> Option<usize> {
    let n = buf.len();
    let op = *buf.get(p)?;
    // 定长指令：顺带校验"整条指令落在缓冲区内"（研究工具同款；不校验会让扫描越过末尾）
    let fixed = |len: usize| -> Option<usize> {
        if p + 1 + len > n {
            None
        } else {
            Some(len)
        }
    };
    match op {
        0..=127 => Some(0),                  // set_char_0..127
        128..=131 => fixed((op - 127) as usize), // set1..4
        132 => fixed(8),                     // set_rule
        133..=136 => fixed((op - 132) as usize), // put1..4
        137 => fixed(8),                     // put_rule
        138 => Some(0),                      // nop
        139 => fixed(44),                    // bop：10×i32 counters + i32 prev
        140 => Some(0),                      // eop
        141 | 142 => Some(0),                // push / pop
        143..=146 => fixed((op - 142) as usize), // right1..4
        147 => Some(0),                      // w0
        148..=151 => fixed((op - 147) as usize),
        152 => Some(0),                      // x0
        153..=156 => fixed((op - 152) as usize),
        157..=160 => fixed((op - 156) as usize), // down1..4
        161 => Some(0),                      // y0
        162..=165 => fixed((op - 161) as usize),
        166 => Some(0),                      // z0
        167..=170 => fixed((op - 166) as usize),
        171..=234 => Some(0),                // fnt_num_0..63
        235..=238 => fixed((op - 234) as usize), // fnt1..4
        239..=242 => {
            // xxx1..4：先长度再载荷（specials）
            let size_bytes = (op - 238) as usize;
            if p + 1 + size_bytes > n {
                return None;
            }
            let len = be(buf, p + 1, size_bytes);
            fixed(size_bytes + len)
        }
        243..=246 => {
            // fnt_def1..4：fontnum + checksum/scale/design + area/name 长度 + 两个字符串
            let size_bytes = (op - 242) as usize;
            let q = p + 1 + size_bytes + 12;
            if q + 2 > n {
                return None;
            }
            let area = buf[q] as usize;
            let name = buf[q + 1] as usize;
            let end = q + 2 + area + name;
            if end > n {
                return None;
            }
            Some(end - p - 1)
        }
        247 => {
            // pre：version + num + den + mag + k + comment
            if p + 15 > n {
                return None;
            }
            let k = buf[p + 14] as usize;
            fixed(14 + k)
        }
        248 => fixed(28),                    // post（定长）
        249 => fixed(9),                     // post_post（4 字节回指 + id + 至少 4 个 0xDF）
        252 => {
            // define_native_font
            if p + 12 > n {
                return None;
            }
            let flags = be(buf, p + 9, 2) as u16;
            let name_len = buf[p + 11] as usize;
            let mut len = 4 + 4 + 2 + 1 + name_len + 4; // font_num + size + flags + nameLen + name + faceIndex
            for f in [NATIVE_COLORED, NATIVE_EXTEND, NATIVE_SLANT, NATIVE_EMBOLDEN] {
                if flags & f != 0 {
                    len += 4;
                }
            }
            fixed(len)
        }
        253 => {
            // set_glyphs：width(i32) + n(u16) + n×x(i32) + n×y(i32) + n×gid(u16)
            if p + 7 > n {
                return None;
            }
            let cnt = be(buf, p + 5, 2);
            fixed(6 + cnt * 10)
        }
        254 => {
            // set_text_and_glyphs：nChars(u16) + chars + width(i32) + nGlyphs(u16) + x[] + y[] + gid[]
            if p + 3 > n {
                return None;
            }
            let n_chars = be(buf, p + 1, 2);
            let q = p + 3 + n_chars * 2;
            if q + 6 > n {
                return None;
            }
            let n_glyphs = be(buf, q + 4, 2);
            fixed(q + 6 + n_glyphs * 10 - p - 1)
        }
        _ => Some(UNKNOWN_OP),
    }
}

/// `bop` 头长度：1 opcode + 10×i32 计数器 + i32 `prev`（载荷 44 B，见 `payload_len` 的 `139` 分支）。
const BOP_LEN: usize = 45;
/// `bop` 尾部的 `prev` 字段长度。
///
/// `prev` 是**上一页 `bop` 的文件偏移**（页 1 为 −1），属**派生量**：任何"前面某页变长"都会让它
/// 整体平移，从而把**内容未变的页**也标成"变了"——实测（42 用例矩阵）长度类编辑由此产生
/// Σ375 页假阳性、raw 精确率仅 9.64%。故页哈希口径丢掉这 4 B，**保留** opcode 与 10×i32 计数器
/// （其中 `c0` = `\count0` 页号）。机制、量化与假阴性证据见 `docs/research/page-hash-prev-quant.md`
/// 与 `docs/modules.md` 已知债 #26。
const BOP_PREV_LEN: usize = 4;

/// 单页的哈希（口径 **V1**：`bop` 头去掉 `prev`，其余原样）。
fn hash_page(slice: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    if slice.len() >= BOP_LEN {
        slice[..BOP_LEN - BOP_PREV_LEN].hash(&mut h); // opcode + 计数器
        slice[BOP_LEN..].hash(&mut h); // 页体（含结尾的 EOP）
    } else {
        // 正常路径不可达（调用点已校验 `p + 45 <= len`）；真出现结构异常时按整段算，方向保守。
        slice.hash(&mut h);
    }
    h.finish()
}

/// 一页在文件里的字节范围：`bop` 起点 → `eop` **之后**的下一个字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSpan {
    pub bop: usize,
    pub end: usize,
}

/// 走一遍 XDV，给出**完整页**的范围（容错规则与 [`page_hashes`] 完全同一套）。
///
/// 存在的意义是"页边界只有一份实现"：`page_hashes`（B/C/A 三个功能点）与
/// [`synthesize_postamble`]（流式出图，roadmap ㉞）都必须认同"哪几页是完整的"——
/// 两处各写一份必然漂移，而漂移的后果是"能算哈希的页"与"能拿去出图的页"对不上号。
pub fn complete_pages(bytes: &[u8]) -> Vec<PageSpan> {
    let mut out = Vec::new();
    let mut p = 0usize;
    // 1) 前导：pre（可选；容忍缺失，与工具一致）
    if bytes.first() == Some(&247) {
        match payload_len(bytes, 0) {
            Some(len) if len != UNKNOWN_OP => p = 1 + len,
            _ => return out, // 前导不完整 → 没有完整页
        }
    }
    // 2) 逐页
    while p < bytes.len() {
        let op = bytes[p];
        if op == POST || op == POST_POST {
            break;
        }
        if op == 0xdf || op == 0 {
            p += 1; // 页间填充
            continue;
        }
        // 页**前**的字体定义：真实 XeTeX 的 XDV 是"首次用到某字体就在当前页里写"，所以顶层一般
        // 不该出现；但换引擎/换实现后可能变（Tectonic 的 XDV 没逐个核过）⇒ 容忍它、跳过继续找页，
        // 而不是把整份前缀判成损坏（代价是"页数变少"，那会让合成件缺页）。
        if FONT_DEF_OPS.contains(&op) || op == NATIVE_FONT_OP {
            match payload_len(bytes, p) {
                Some(len) if len != UNKNOWN_OP => {
                    p += 1 + len;
                    continue;
                }
                _ => break,
            }
        }
        if op != BOP {
            break; // 结构损坏：保留已解出的页
        }
        let bop = p;
        if p + BOP_LEN > bytes.len() {
            break;
        }
        p += BOP_LEN;
        let mut saw_eop = false;
        while p < bytes.len() {
            let o = bytes[p];
            if o == EOP {
                p += 1;
                saw_eop = true;
                break;
            }
            match payload_len(bytes, p) {
                None => break,             // 截断：停在这条指令的起点
                Some(UNKNOWN_OP) => break, // 未知 opcode：丢弃该页
                Some(len) => p += 1 + len,
            }
        }
        // 页完整的唯一判据：见到 EOP（"恰好扫到缓冲区末尾"不算）
        if !saw_eop {
            break;
        }
        out.push(PageSpan { bop, end: p });
    }
    out
}

/// 每页的字节哈希，顺序即页号（下标 +1 = 页号）。
///
/// 返回 `Vec::new()` 表示**没有可用的页信息**（文件缺失/不是 XDV/首屏就损坏）——调用方
/// 必须把这当作"无法判定"（前端保守地全量刷新），而不是"零页"。
pub fn page_hashes(bytes: &[u8]) -> Vec<u64> {
    complete_pages(bytes)
        .iter()
        .map(|s| hash_page(&bytes[s.bop..s.end]))
        .collect()
}

/// `post` 头：opcode + 28 B 定长载荷。
const POST_LEN: usize = 29;
/// `post_post` 的固定部分：opcode + i32 回指指针 + 1 B id（**XDV 的 id 是 1 字节**，见实测）。
const POST_POST_LEN: usize = 6;
/// XDV 的 `post_post` id。
const XDV_ID: u8 = 7;
/// 合成的 `post` 头之后紧跟的字体定义 opcode（`fnt_def1..4` 与 `define_native_font`）。
///
/// **两类都要收**（实测踩过）：只收 `define_native_font`(252) 时 xdvipdfmx 会报
/// `Tried to select a font that hasn't been defined`。
const FONT_DEF_OPS: std::ops::RangeInclusive<u8> = 243..=246;
const NATIVE_FONT_OP: u8 = 252;

/// 合成结果：缝好的**完整 XDV** + 它包含的页数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialXdv {
    pub bytes: Vec<u8>,
    pub pages: usize,
}

/// 把「页前缀」缝成一份**看起来完整**的 XDV（补 postamble），交给 xdvipdfmx 出**部分 PDF**。
///
/// 用途（roadmap ㉞「流式出图」）：编译期只有前缀，而 `xdvipdfmx`（外部与库内同一套 C）
/// 要求文件含 postamble —— 补上之后就能"边编边出图"。整条路的成本与实测（合成 9–12 ms、
/// 进程内转换 92–131 ms/次、页数全对）见 `docs/research/dvi-preview-feasibility.md` §10。
///
/// 三条规则都是当年实测踩出来的（Node 原型 `scripts/xdv-partial.mjs` 同款）：
/// 1. **顺序 = `post` 头 → 字体定义 → `post_post`**。写成"字体定义在 post 之前"会被
///    xdvipdfmx 判成 `Tried to select a font that hasn't been defined`；
/// 2. `last_bop` = 最后一页的 `bop` 偏移，`t` = 页数；
/// 3. `post_post` 的指针指**本文件里**的 `post` 偏移、id 是 1 字节，尾部补 `0xDF` 到 4 字节
///    对齐（至少 4 个）。
///
/// `num`/`den`/`mag` 从 `pre` 抄（DVI 规范里两处都有）；`l`/`u`/`s`（最大高/宽/栈深）是
/// **提示量**、不参与寻页，置 0（实测转换结果与逐字段复制整份 post 时一致）。
///
/// 返回 `None`：**没有 `pre`、或一页完整页都没有**。调用方必须把它当"无法判定"——
/// 不要拿空前缀去造一份空 PDF（那会让前端显示一份 0 页的文档）。
pub fn synthesize_postamble(prefix: &[u8]) -> Option<PartialXdv> {
    // pre 必须存在：num/den/mag 只能从它抄（布局见 `payload_len` 的 247 分支）
    if prefix.first() != Some(&247) {
        return None;
    }
    match payload_len(prefix, 0) {
        Some(len) if len != UNKNOWN_OP => {}
        _ => return None, // 前导不完整
    }
    let pages = complete_pages(prefix);
    let last = *pages.last()?;
    // 只取到最后一页的 eop：后面的半页/填充不属于产物
    let body = &prefix[..last.end];

    // 字体定义（收整条指令的原始字节，原样搬到 post 头之后）
    let mut fonts: Vec<&[u8]> = Vec::new();
    let mut p = 0usize;
    while p < body.len() {
        let op = body[p];
        let Some(len) = payload_len(body, p) else { break };
        if len == UNKNOWN_OP {
            break;
        }
        if FONT_DEF_OPS.contains(&op) || op == NATIVE_FONT_OP {
            fonts.push(&body[p..p + 1 + len]);
        }
        p += 1 + len;
    }

    let font_bytes: usize = fonts.iter().map(|f| f.len()).sum();
    // `post` 紧跟在最后一页之后：**字体定义在它后面**（顺序 = body → post 头 → 字体 → post_post）。
    // 这里踩过一次：写成 `body.len() + font_bytes` 会让 post_post 的回指指针落到字体区中间，
    // xdvipdfmx 直接判 `Something is wrong. Are you sure this is a DVI file?`。
    let post_at = body.len();
    let mut out = Vec::with_capacity(post_at + POST_LEN + font_bytes + POST_POST_LEN + 8);
    out.extend_from_slice(body);

    let mut post = [0u8; POST_LEN];
    post[0] = POST;
    post[1..5].copy_from_slice(&(last.bop as i32).to_be_bytes()); // last_bop
    post[5..9].copy_from_slice(&prefix[2..6]); // num（pre：version 1 B 之后）
    post[9..13].copy_from_slice(&prefix[6..10]); // den
    post[13..17].copy_from_slice(&prefix[10..14]); // mag
    // l/u/s 留 0；t = 页数
    post[27..29].copy_from_slice(&(pages.len() as i16).to_be_bytes());
    out.extend_from_slice(&post);

    for f in &fonts {
        out.extend_from_slice(f);
    }

    out.push(POST_POST);
    out.extend_from_slice(&(post_at as i32).to_be_bytes());
    out.push(XDV_ID);
    // 尾部 0xDF：至少 4 个，且补到 4 字节对齐（与真实文件的收尾一致）
    let mut pad = 4usize;
    while (out.len() + pad) % 4 != 0 {
        pad += 1;
    }
    out.extend(std::iter::repeat(0xdf).take(pad));

    Some(PartialXdv { bytes: out, pages: pages.len() })
}

/// 比较两轮的页哈希，给出**变化页号**（1-based）。
///
/// 语义（调用方依赖这三条，别改）：
/// - `prev` 为 `None` 或与 `cur` 页数不同 → 视为**全部变化**（含"首次编译"与"页数变了"）；
/// - `cur` 为空 → 表示**无法判定**（XDV 缺失/损坏）：这里返回空表，调用方应以 `pages == 0`
///   作为"未知"信号让前端全量刷新（**不要**在这里伪造页号——伪造会把"未知"和"零页变化"混淆）；
/// - 其余情况逐页比对，返回内容变了的下标 +1。
pub fn changed_pages(prev: Option<&[u64]>, cur: &[u64]) -> Vec<u32> {
    match prev {
        Some(p) if !cur.is_empty() && p.len() == cur.len() => cur
            .iter()
            .enumerate()
            .filter(|(i, h)| p.get(*i) != Some(*h))
            .map(|(i, _)| i as u32 + 1)
            .collect(),
        _ => (1..=cur.len() as u32).collect(),
    }
}

/// 页哈希缓存的**口径版本**（写进文件首行；读侧不匹配即当"无法判定"）。
///
/// 2026-09 页哈希口径改为 **V1**（`bop` 头去掉 4 B `prev`，见
/// `docs/research/page-hash-prev-quant.md` 与 `docs/modules.md` 已知债 #26）：老缓存里存的是 raw
/// 口径哈希，与新口径逐页都不相等。这里用**首行标记**把两者一次性区分开——老文件首行是 16 进制
/// 哈希，读侧取不到 `v1` ⇒ `None` ⇒ 只退化**一轮**（该轮多转换一次 + 视窗全量重绘，随后写回新格式
/// 即自愈），**不会**被误判成"逐页相同"。反向（回滚老二进制读新文件）同样只退化一轮。
///
/// **口径只此一处定义**（2026-09 从 `latteset-infra` 提到 core）：两个 runner 都要用它 ——
/// 子进程档在 `latteset-infra`，库形态档在 `latteset-tectonic`（按 ADR-0012 **不依赖 infra**）。
/// 漏写标记的代价是"永远首轮"，A 面每轮白跑 0.65–0.94 s，所以不允许两处各写一份。
pub const PAGES_CACHE_VERSION: &str = "v1";

/// 页哈希缓存文件路径：`<tmp>/<stem>.<引擎>.pages`。
///
/// 为什么**带引擎名**（2026-09，已知债 #25）：页哈希的口径与引擎绑定。同一个 `main.tex` 换引擎后
/// 页内容必然不同；若共用一份缓存，跨引擎的"逐页相同"判断就是拿两套口径比大小。今天只有产 XDV 的
/// 引擎会写它，但按引擎分文件后，将来给别的引擎接**引擎内逐页指纹**时两套哈希不会互相污染。
pub fn pages_cache_path(tmp_dir: &Path, stem: &str, engine: Engine) -> PathBuf {
    tmp_dir.join(format!("{stem}.{}.pages", engine.binary_name()))
}

/// 解析页哈希缓存文本。
///
/// **首行必须是** [`PAGES_CACHE_VERSION`]；老格式（首行直接是 16 进制哈希）与任何截断/损坏都当
/// `None` = **无法判定** → 不做任何"跳过"优化（保守，只多转换一轮）。
pub fn parse_pages_cache(text: &str) -> Option<Vec<u64>> {
    let mut lines = text.lines();
    if lines.next()?.trim() != PAGES_CACHE_VERSION {
        return None;
    }
    let mut out = Vec::new();
    for line in lines {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        out.push(u64::from_str_radix(t, 16).ok()?);
    }
    (!out.is_empty()).then_some(out)
}

/// 生成页哈希缓存文本（首行口径标记，其后每页一行 `{h:016x}`）。
///
/// 空表 ⇒ `None`：**不写缓存**，免得把"无法判定"当成"零页"带给下一轮。
pub fn format_pages_cache(hashes: &[u64]) -> Option<String> {
    if hashes.is_empty() {
        return None;
    }
    let mut body = String::with_capacity(PAGES_CACHE_VERSION.len() + 1 + hashes.len() * 17);
    body.push_str(PAGES_CACHE_VERSION);
    body.push('\n');
    for h in hashes {
        body.push_str(&format!("{h:016x}\n"));
    }
    Some(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小 XDV：`pre` + `n` 页（每页 `bop`+44B 计数器/prev+1B 页体+`eop`）+ `post`+`post_post`。
    /// 与 `latteset-tectonic` 的测试夹具同构（那边也有一份 `tiny_xdv`）。
    fn tiny_xdv(pages: usize) -> Vec<u8> {
        let mut v = pre_bytes();
        for i in 0..pages {
            v.push(BOP);
            v.extend_from_slice(&[0u8; 44]);
            v.push(i as u8);
            v.push(EOP);
        }
        v.push(POST);
        v.extend_from_slice(&[0u8; 28]);
        v.push(POST_POST);
        v.extend_from_slice(&[0u8; 4]);
        v.push(XDV_ID);
        v.extend_from_slice(&[0xdf; 4]);
        v
    }

    /// `pre`：opcode + version(1) + num/den/mag(各 4) + k(1) + comment。
    fn pre_bytes() -> Vec<u8> {
        let mut v = vec![247u8, 7];
        v.extend_from_slice(&[0, 0, 0, 1]); // num
        v.extend_from_slice(&[0, 0, 0, 1]); // den
        v.extend_from_slice(&[0, 0, 3, 232]); // mag = 1000
        v.push(0); // k = 0（无注释）
        v
    }

    /// 一条 `fnt_def1`（opcode 243）：fontnum(1) + checksum/scale/design(各 4) + area/name 长度 + 两串。
    fn fnt_def1(fontnum: u8) -> Vec<u8> {
        let mut v = vec![243u8, fontnum];
        v.extend_from_slice(&[0u8; 12]);
        v.push(0); // area 长度
        v.push(1); // name 长度
        v.push(b'x');
        v
    }

    /// 页走查：前 N 页完整、末页截断 ⇒ 只数完整页（与 `page_hashes` 同一条规则）。
    #[test]
    fn complete_pages_counts_only_finished_pages() {
        let full = tiny_xdv(3);
        let spans = complete_pages(&full);
        assert_eq!(spans.len(), 3);
        // 砍掉**第 3 页的 eop**（不是文件尾的填充）⇒ 只剩 2 页完整
        let truncated = &full[..spans[2].end - 1];
        let kept = complete_pages(truncated);
        assert_eq!(kept.len(), 2, "eop 缺失的页不算完整页");
        assert!(kept[1].bop >= kept[0].end, "页边界单调");
    }

    /// `page_hashes` 与 `complete_pages` 必须认同同一批页（重构后不许漂移）。
    #[test]
    fn page_hashes_and_complete_pages_agree() {
        let full = tiny_xdv(4);
        assert_eq!(page_hashes(&full).len(), complete_pages(&full).len());
        let truncated = &full[..full.len() - 2];
        assert_eq!(page_hashes(truncated).len(), complete_pages(truncated).len());
    }

    /// 合成：产出以 `pre` 开头、以 `post_post` 收尾，页数正确，`last_bop` 指向最后一页。
    #[test]
    fn synthesized_prefix_is_a_well_formed_xdv() {
        let prefix = tiny_xdv(2);
        let out = synthesize_postamble(&prefix).expect("两页完整 ⇒ 应能合成");
        assert_eq!(out.pages, 2);
        assert_eq!(out.bytes[0], 247, "仍以 pre 开头");
        assert_eq!(out.bytes.len() % 4, 0, "尾部补齐到 4 字节对齐");

        // 找到我们写的 post：在最后一页 eop 之后
        let spans = complete_pages(&out.bytes);
        assert_eq!(spans.len(), 2, "合成件自身必须仍解析出 2 页");
        let post_at = spans.last().unwrap().end;
        assert_eq!(out.bytes[post_at], POST);
        let last_bop = i32::from_be_bytes(out.bytes[post_at + 1..post_at + 5].try_into().unwrap());
        assert_eq!(last_bop as usize, spans.last().unwrap().bop, "last_bop 指向最后一页");
        let t = i16::from_be_bytes(out.bytes[post_at + 27..post_at + 29].try_into().unwrap());
        assert_eq!(t, 2, "t = 页数");
        // num/den/mag 从 pre 抄
        assert_eq!(&out.bytes[post_at + 5..post_at + 9], &[0, 0, 0, 1]);
        assert_eq!(&out.bytes[post_at + 13..post_at + 17], &[0, 0, 3, 232]);

        // post_post：指针必须指回 post
        let pp_at = out.bytes.len() - 1 - trailing_df(&out.bytes);
        let pp = out.bytes.iter().rposition(|b| *b == POST_POST).expect("应有 post_post");
        assert_eq!(pp, pp_at - 5, "post_post 在补齐之前");
        let back = i32::from_be_bytes(out.bytes[pp + 1..pp + 5].try_into().unwrap());
        assert_eq!(back as usize, post_at, "回指指针必须指向 post");
        assert_eq!(out.bytes[pp + 5], XDV_ID, "id 是 1 字节");
    }

    /// 尾部 0xDF 的个数（用于反推 post_post 的位置）。
    fn trailing_df(bytes: &[u8]) -> usize {
        bytes.iter().rev().take_while(|b| **b == 0xdf).count()
    }

    /// 字体定义要**原样搬到 post 头之后**（两类都收）：漏掉就会被 xdvipdfmx 判
    /// "选了未定义的字体"。
    #[test]
    fn font_definitions_are_carried_after_the_post_header() {
        let mut prefix = pre_bytes();
        prefix.push(BOP);
        prefix.extend_from_slice(&[0u8; 44]);
        // 字体定义**在页内**（真实布局：XeTeX 首次用到该字体时才写，那时已经在页里了）
        prefix.extend_from_slice(&fnt_def1(3));
        prefix.push(9); // 一点页体
        prefix.push(EOP);

        let spans_in = complete_pages(&prefix);
        assert_eq!(spans_in.len(), 1, "夹具本身应解析出 1 页（否则测的不是合成）");
        let out = synthesize_postamble(&prefix).expect("有一页完整页");
        let spans = complete_pages(&out.bytes);
        let post_at = spans.last().unwrap().end;
        assert_eq!(out.bytes[post_at], POST);
        // post 头之后紧跟的就是那条字体定义（同一串字节）
        let def = fnt_def1(3);
        assert_eq!(&out.bytes[post_at + POST_LEN..post_at + POST_LEN + def.len()], &def[..]);

        // ⚠ **回指指针必须指 post 头本身**，不是"post + 字体字节"。
        // 这条断言是回归守卫：夹具里**必须有字体定义**（font_bytes > 0），否则两条写法等价、
        // 测试抓不到——实测就是这么漏过去的（真机上 xdvipdfmx 报
        // `Something is wrong. Are you sure this is a DVI file?`）。
        assert!(def.len() > 0, "夹具必须带字体定义，否则这条断言没有区分力");
        let pp = out.bytes.iter().rposition(|b| *b == POST_POST).expect("应有 post_post");
        let back = i32::from_be_bytes(out.bytes[pp + 1..pp + 5].try_into().unwrap());
        assert_eq!(back as usize, post_at, "回指指针要指到 post 头（踩过的坑）");
        assert_eq!(out.bytes[back as usize], POST, "指针必须落在一个 post 指令上");
    }

    /// 没有 `pre`、或一页完整页都没有 ⇒ `None`（调用方据此保持"无法判定"，不造空 PDF）。
    #[test]
    fn synthesis_refuses_prefixes_without_pre_or_without_pages() {
        assert!(synthesize_postamble(&[]).is_none());
        assert!(synthesize_postamble(&[BOP]).is_none());
        // 有 pre 但没有完整页
        assert!(synthesize_postamble(&pre_bytes()).is_none());
        // 有 pre + 半页
        let mut half = pre_bytes();
        half.push(BOP);
        half.extend_from_slice(&[0u8; 44]);
        assert!(synthesize_postamble(&half).is_none());
    }

    /// 已经带 postamble 的完整文件再合成一次：仍能解析出同样的页数（幂等到"页"这一层）。
    #[test]
    fn synthesis_on_a_complete_file_keeps_page_count() {
        let full = tiny_xdv(3);
        let out = synthesize_postamble(&full).expect("完整文件也应能合成");
        assert_eq!(out.pages, 3);
        assert_eq!(page_hashes(&out.bytes).len(), 3);
    }

    /// 页哈希缓存往返：`format_pages_cache` → `parse_pages_cache` 必须无损。
    #[test]
    fn pages_cache_round_trips() {
        let hashes = vec![0u64, 1, 0xdead_beef, u64::MAX];
        let text = format_pages_cache(&hashes).expect("非空表要给文本");
        assert!(text.starts_with(PAGES_CACHE_VERSION), "首行必须是口径标记：{text}");
        assert_eq!(parse_pages_cache(&text), Some(hashes));
    }

    /// 空表**不写**缓存：否则下一轮会把"无法判定"当成"零页"。
    #[test]
    fn empty_hashes_are_not_cached() {
        assert_eq!(format_pages_cache(&[]), None);
    }

    /// 口径不符 / 损坏 / 空 ⇒ `None` = 无法判定（保守：不做任何"跳过"优化）。
    #[test]
    fn legacy_or_broken_cache_is_rejected() {
        // 2026-09 之前的 raw 缓存：首行直接是 16 进制哈希
        assert_eq!(parse_pages_cache("00000000deadbeef\n"), None);
        // 标记对但内容空 ⇒ 依然是"无法判定"
        assert_eq!(parse_pages_cache("v1\n"), None);
        // 标记对但有一行不是十六进制 ⇒ 整份作废（不猜）
        assert_eq!(parse_pages_cache("v1\n00ff\nzz\n"), None);
        // 完全空
        assert_eq!(parse_pages_cache(""), None);
    }

    /// 路径**带引擎名**（已知债 #25：页哈希口径与引擎绑定，跨引擎共缓存就是拿两套口径比大小）。
    #[test]
    fn pages_cache_path_is_engine_scoped() {
        let tmp = std::path::Path::new("/tmp/x");
        let xe = pages_cache_path(tmp, "main", Engine::XeLaTeX);
        let tec = pages_cache_path(tmp, "main", Engine::Tectonic);
        assert_eq!(xe.file_name().unwrap().to_string_lossy(), "main.xelatex.pages");
        assert_eq!(tec.file_name().unwrap().to_string_lossy(), "main.tectonic.pages");
        assert_ne!(xe, tec);
    }

    /// 最小合法 XDV：pre + n 页（每页 = bop + [body] + eop）+ post。
    fn minimal_xdv(bodies: &[&[u8]]) -> Vec<u8> {
        let mut v = vec![247u8, 7];
        v.extend_from_slice(&[0u8; 12]); // num / den / mag
        v.push(0); // comment 长度 0
        for body in bodies {
            v.push(BOP);
            v.extend_from_slice(&[0u8; 44]);
            v.extend_from_slice(body);
            v.push(EOP);
        }
        v.push(POST);
        v.extend_from_slice(&[0u8; 28]);
        v
    }

    #[test]
    fn counts_pages_and_hashes_bodies() {
        let xdv = minimal_xdv(&[&[138], &[138, 1, 2], &[138]]);
        let h = page_hashes(&xdv);
        assert_eq!(h.len(), 3, "应解出 3 页");
        assert_ne!(h[0], h[1], "页体不同 → 哈希不同");
        assert_eq!(h[0], h[2], "页体相同 → 哈希相同（第 1/3 页都是 nop）");
    }

    #[test]
    fn identical_builds_give_identical_hashes() {
        let a = minimal_xdv(&[&[138], &[1, 2, 3]]);
        let b = minimal_xdv(&[&[138], &[1, 2, 3]]);
        assert_eq!(page_hashes(&a), page_hashes(&b));
    }

    #[test]
    fn truncated_mid_instruction_drops_that_page() {
        // 第 2 页含 set_rule（opcode 132 + 8 字节载荷 = height/width）
        let mut xdv = minimal_xdv(&[&[138], &[132, 1, 2, 3, 4, 5, 6, 7, 8]]);
        let full = page_hashes(&xdv);
        assert_eq!(full.len(), 2, "两页都应完整解出");
        // 截到第 2 页 set_rule 的载荷中间（只留 3 字节 < 8）
        let page2_body = 1 + 14 + (1 + 44 + 1 + 1) + 1 + 44; // pre + 第 1 页 + 第 2 页 bop
        xdv.truncate(page2_body + 1 + 3);
        let cut = page_hashes(&xdv);
        assert_eq!(cut.len(), 1, "半个页不能算完整页（回归 2026-09 的静默错误）");
        assert_eq!(cut[0], full[0], "已完整页的哈希不受截断影响（前缀性质）");
    }

    #[test]
    fn truncation_exactly_at_page_end_keeps_page() {
        // 只留第 1 页（bop…eop），其余全切掉
        let xdv = minimal_xdv(&[&[138], &[138]]);
        let first_page_len = 1 + 14 + (1 + 44 + 1 + 1); // pre + 第 1 页
        let h = page_hashes(&xdv[..first_page_len]);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0], page_hashes(&xdv)[0]);
    }

    #[test]
    fn truncation_at_buffer_end_without_eop_is_not_a_page() {
        // 页体完整扫到缓冲区末尾、但没有 EOP → 不算一页（第二个静默错误）
        let xdv = minimal_xdv(&[&[138]]);
        let no_eop = &xdv[..1 + 14 + 1 + 44 + 1]; // pre + bop + 一条 nop
        assert!(page_hashes(no_eop).is_empty());
    }

    #[test]
    fn unknown_opcode_drops_page_and_stops() {
        let mut xdv = minimal_xdv(&[&[138], &[138]]);
        // 在第 1 页体里塞一个未定义 opcode（0xF0 = 240，XDV 未定义）
        let inject_at = 1 + 14 + 1 + 44;
        xdv.insert(inject_at, 240);
        let h = page_hashes(&xdv);
        assert!(h.is_empty(), "未知 opcode 所在页及之后都丢弃");
    }

    #[test]
    fn tolerates_missing_preamble() {
        let mut v = Vec::new();
        v.push(BOP);
        v.extend_from_slice(&[0u8; 44]);
        v.push(138);
        v.push(EOP);
        assert_eq!(page_hashes(&v).len(), 1);
    }

    #[test]
    fn garbage_input_yields_no_pages() {
        assert!(page_hashes(&[]).is_empty());
        assert!(page_hashes(b"not an xdv file at all").is_empty());
        assert!(page_hashes(&[247]).is_empty(), "只有 pre 的开头");
    }

    #[test]
    fn changed_pages_semantics() {
        let a = vec![1u64, 2, 3];
        // 首次（无基线）→ 全部
        assert_eq!(changed_pages(None, &a), vec![1, 2, 3]);
        // 逐页相同 → 空（B 功能点的判据）
        assert_eq!(changed_pages(Some(&a), &a), Vec::<u32>::new());
        // 第 2 页变了
        assert_eq!(changed_pages(Some(&a), &[1, 9, 3]), vec![2]);
        // 页数变了 → 全部
        assert_eq!(changed_pages(Some(&a), &[1, 2]), vec![1, 2]);
        // 无法判定（cur 为空）→ 保守全刷（以 cur 长度计，为 0 页时返回空）
        assert_eq!(changed_pages(Some(&a), &[]), Vec::<u32>::new());
    }

    /// 带**真实 `prev` 链**的 XDV（`minimal_xdv` 的加强版）：`prev_i` = 第 i−1 页 `bop` 的偏移，
    /// 页 1 = −1（与 XeTeX 实际产物一致）。返回 (bytes, 每页 `bop` 偏移)。
    fn chain_xdv(bodies: &[&[u8]]) -> (Vec<u8>, Vec<usize>) {
        let mut v = vec![247u8, 7];
        v.extend_from_slice(&[0u8; 12]); // num / den / mag
        v.push(0); // comment 长度 0
        let mut offsets = Vec::new();
        let mut prev: i32 = -1;
        for body in bodies {
            let bop = v.len();
            offsets.push(bop);
            v.push(BOP);
            v.extend_from_slice(&[0u8; 40]); // 10×i32 计数器（c0 = \count0）
            v.extend_from_slice(&prev.to_be_bytes());
            v.extend_from_slice(body);
            v.push(EOP);
            prev = bop as i32;
        }
        v.push(POST);
        v.extend_from_slice(&[0u8; 28]);
        (v, offsets)
    }

    /// 页内 EOP 的下标（页体长度已知时）。
    fn eop_index(bop: usize, body_len: usize) -> usize {
        bop + 45 + body_len
    }

    /// **旧口径**（2026-09 之前）：哈希整页，含 45 B `bop` 头（`prev` 在内）。
    /// 只用于对拍"修掉了什么"——产品路径已不用它（见 [`hash_page`] 的口径注释）。
    fn old_scope_hash(bytes: &[u8], bop: usize, eop: usize) -> u64 {
        let mut h = DefaultHasher::new();
        bytes[bop..=eop].hash(&mut h);
        h.finish()
    }

    /// 已知债 #26 的核心修复：`prev` 改动**不再**被当成"页内容变了"。
    #[test]
    fn prev_only_change_is_not_a_page_change() {
        let bodies: &[&[u8]] = &[&[1, 2], &[3, 4, 5], &[6]];
        let (a, off_a) = chain_xdv(bodies);
        let (mut b, off_b) = chain_xdv(bodies);
        // 只改第 2 页 `prev` 的最高字节（页内偏移 41）——页体一字未动
        b[off_b[1] + 41] = 0x7f;
        assert_ne!(a, b, "两份 XDV 的字节确实不同");
        assert_eq!(
            page_hashes(&a),
            page_hashes(&b),
            "V1：逐页相同 ⇒ B 可跳过重载"
        );
        assert_eq!(
            changed_pages(Some(&page_hashes(&a)), &page_hashes(&b)),
            Vec::<u32>::new()
        );
        // 旧口径（含 prev）会把第 2 页判成"变了"——#26 的病根（回归锚点：改回去即复发）
        assert_ne!(
            old_scope_hash(&a, off_a[1], eop_index(off_a[1], bodies[1].len())),
            old_scope_hash(&b, off_b[1], eop_index(off_b[1], bodies[1].len())),
            "旧口径确实会判第 2 页变了"
        );
    }

    /// 前面某页变长 ⇒ 后续页的 `prev` 整体平移：V1 只报**那一页**，旧口径会把第 3 页起全部打脏
    /// （实测 25/26 页，见 `docs/research/page-hash-prev-quant.md` §1）。
    #[test]
    fn page_growth_only_marks_the_grown_page() {
        let b1: &[u8] = &[1, 2, 3];
        let b1_long: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]; // 长 10 B
        let b2: &[u8] = &[138, 7, 7];
        let b3: &[u8] = &[4, 4, 4];
        let b4: &[u8] = &[5, 5];
        let (a, off_a) = chain_xdv(&[b1, b2, b3, b4]);
        let (b, off_b) = chain_xdv(&[b1_long, b2, b3, b4]);
        let (ha, hb) = (page_hashes(&a), page_hashes(&b));
        assert_eq!((ha.len(), hb.len()), (4, 4));
        assert_eq!(
            changed_pages(Some(&ha), &hb),
            vec![1],
            "V1：只报内容真的变了的第 1 页"
        );
        assert_eq!(
            ha[1], hb[1],
            "第 2 页的 prev 指向第 1 页起点（未移动）⇒ 逐字节相同"
        );
        assert_eq!(ha[2], hb[2], "V1：第 3 页的 prev 位移被忽略");
        assert_eq!(ha[3], hb[3], "V1：第 4 页的 prev 位移被忽略");
        // 旧口径的现场：第 3、4 页的 prev 指向移动了的页 ⇒ 被判"变了"
        assert_ne!(
            old_scope_hash(&a, off_a[2], eop_index(off_a[2], b3.len())),
            old_scope_hash(&b, off_b[2], eop_index(off_b[2], b3.len())),
            "旧口径确实会把第 3 页打脏"
        );
        assert_ne!(
            old_scope_hash(&a, off_a[3], eop_index(off_a[3], b4.len())),
            old_scope_hash(&b, off_b[3], eop_index(off_b[3], b4.len())),
            "旧口径确实会把第 4 页打脏"
        );
    }
}
