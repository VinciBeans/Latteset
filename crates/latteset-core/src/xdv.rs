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
//! **容错**：末尾截断、未知 opcode → 丢弃该页（前 N 页仍然可用——页包自包含，见 G2 报告 §3.3）。
//! 两个边界判据是 2026-09 修 bug 时确定的，复现它们的行为很重要：
//! ① 消费一条指令前校验 `end <= len`；② 页完整的**唯一**判据是"见到 EOP"。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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

fn hash_page(slice: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    slice.hash(&mut h);
    h.finish()
}

/// 每页的字节哈希，顺序即页号（下标 +1 = 页号）。
///
/// 返回 `Vec::new()` 表示**没有可用的页信息**（文件缺失/不是 XDV/首屏就损坏）——调用方
/// 必须把这当作"无法判定"（前端保守地全量刷新），而不是"零页"。
pub fn page_hashes(bytes: &[u8]) -> Vec<u64> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
