//! 日志解析（modules.md §4）。

pub mod diagnosis;
pub mod model;
pub mod progress;
pub mod scan;
#[cfg(test)]
pub mod real_error_corpus;

pub use diagnosis::{
    diagnose, diagnose_timeout, source_release_hint, Diagnosis, DiagnosisKind, TimeoutEvidence,
};
pub use model::{LogMessage, MessageKind};
pub use progress::pages_typeset;
pub use scan::parse_log;

/// 解码 `.log` 字节为文本：**能严格解就严格解，不能则 lossy**（非法字节 → U+FFFD）。
///
/// 为什么需要它（2026-09 实测，roadmap P0-①）：`.log` **不保证是合法 UTF-8**。
/// 源 `.tex` 若是 GBK（中文用户常见的遗留编码），引擎会把原始字节回显进日志：
/// - xelatex 自己做了替换（日志仍是合法 UTF-8）；
/// - **pdflatex 不会**——实测 GBK 源在 `pdflatex` 下产出的 `.log` 含 73 个非法字节，
///   严格 UTF-8 读取直接失败。
///
/// 后果对比：严格读取失败 → 前端只看到「编译失败且无法读取日志」；lossy 解码 → 日志中
/// `! LaTeX Error: …` 与 `l.<n>` 等 **ASCII 骨架完好**，错误列表仍能给出消息与行号。
/// 诊断信息"有损"远好于"没有"。
pub fn decode_log(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

#[cfg(test)]
mod decode_tests {
    use super::*;

    #[test]
    fn valid_utf8_passes_through_unchanged() {
        let text = "! LaTeX Error: File `nope.sty' not found.\nl.5 \\usepackage{nope}\n中文注释\n";
        assert_eq!(decode_log(text.as_bytes()), text);
    }

    #[test]
    fn invalid_bytes_do_not_lose_error_skeleton() {
        // 复刻实测：GBK 源（中文注释）经 pdflatex 回显进日志 → 含 GBK 字节 0xD6 0xD0 ...
        let mut bytes = Vec::from(&b"! LaTeX Error: Invalid UTF-8 byte sequence.\n"[..]);
        bytes.extend_from_slice(&[0xD6, 0xD0, 0xCE, 0xC4]); // "中文" 的 GBK 编码
        bytes.extend_from_slice(b"\nl.3 ");
        bytes.extend_from_slice(&[0xD6, 0xD0, 0xCE, 0xC4]);
        bytes.extend_from_slice(b"\n");

        let decoded = decode_log(&bytes);
        // 不得 panic，且 ASCII 骨架完好 —— 错误列表据此给出行号
        assert!(decoded.contains("! LaTeX Error: Invalid UTF-8 byte sequence."));
        assert!(decoded.contains("l.3 "));
        assert!(decoded.contains('\u{FFFD}'), "非法字节应替换为 U+FFFD");

        // 解码结果必须仍能被 parse_log 解析出行号（端到端判据）
        let msgs = parse_log(&decoded);
        assert_eq!(msgs.len(), 1, "应解析出一条错误：{msgs:?}");
        assert_eq!(msgs[0].line, Some(3));
    }

    #[test]
    fn empty_input_is_empty() {
        assert_eq!(decode_log(b""), "");
    }
}

#[cfg(test)]
mod diagnosis_tests;

