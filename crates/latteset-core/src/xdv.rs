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

/// 每页的字节哈希，顺序即页号（下标 +1 = 页号）。
///
/// 返回 `Vec::new()` 表示**没有可用的页信息**（文件缺失/不是 XDV/首屏就损坏）——调用方
/// 必须把这当作"无法判定"（前端保守地全量刷新），而不是"零页"。
pub fn page_hashes(bytes: &[u8]) -> Vec<u64> {    let mut out = Vec::new();
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
        if op != BOP {
            break; // 结构损坏：保留已解出的页
        }
        let bop = p;
        if p + 45 > bytes.len() {
            break;
        }
        p += 45;
        let mut saw_eop = false;
        while p < bytes.len() {
            let o = bytes[p];
            if o == EOP {
                p += 1;
                saw_eop = true;
                break;
            }
            match payload_len(bytes, p) {
                None => break,                    // 截断：停在这条指令的起点
                Some(UNKNOWN_OP) => break,        // 未知 opcode：丢弃该页
                Some(len) => p += 1 + len,
            }
        }
        // 页完整的唯一判据：见到 EOP（"恰好扫到缓冲区末尾"不算）
        if !saw_eop {
            break;
        }
        out.push(hash_page(&bytes[bop..p]));
    }
    out
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
