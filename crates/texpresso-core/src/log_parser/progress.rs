//! 从 `.log` 里读「进度证据」（roadmap ㉕）。
//!
//! 超时诊断要回答的第一个问题是"它是**慢**还是**卡住**"。唯一可靠的证据是引擎自己
//! 打出的页输出标记：TeX 每输出一页就写一个 `[N]`。有页输出 = 在推进，只是比上限慢。
//!
//! **坑（实测）**：`[N]` 会被 `max_print_line`（默认 79）**折行**——真实日志里能看到
//! `[6` / 空行 / `]` 分散在多行，甚至夹着 `第二章` 之类的正文回显。逐行正则或
//! `\[\d+\]` 会漏掉一半（实测 `multifile` 15 页只匹配到 7 个），故匹配必须**容忍空白**。

use std::sync::LazyLock;

/// 页输出标记：`[3]`，也容忍折行后的 `[6\n\n\n\n]`。
static PAGE_MARKER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\[\s*(\d{1,4})\s*\]").expect("page marker regex"));

/// 已排版出的最大页码；日志里没有任何页输出时返回 `None`。
///
/// 语义是"**至少**排到了第 N 页"：日志按缓冲落盘，被树杀时可能丢掉最后一段输出。
pub fn pages_typeset(log: &str) -> Option<u32> {
    PAGE_MARKER
        .captures_iter(log)
        .filter_map(|c| c[1].parse::<u32>().ok())
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_page_markers() {
        assert_eq!(pages_typeset("(./main.aux)\n[1] [2] [3]\n"), Some(3));
    }

    #[test]
    fn tolerates_page_markers_wrapped_by_max_print_line() {
        // 复刻真实日志（test_file/projects/multifile/tmp/main.log）：[6 与 ] 被折行隔开
        let log = "\\openout2 = `chapters/intro.aux'.\n[3] [4] [5] [6\n\n\n\n\n] 第二章\n[7]\n";
        assert_eq!(pages_typeset(log), Some(7));
    }

    #[test]
    fn takes_max_not_last() {
        // 页码可能因 `\include` 交错出现（子文件 aux 回显），取最大才是真实进度
        assert_eq!(pages_typeset("[15] [9] [10] [11]\n"), Some(15));
    }

    #[test]
    fn empty_brackets_and_long_numbers_are_ignored() {
        assert_eq!(pages_typeset("[] [12345] (./x.aux)\n"), None);
    }

    #[test]
    fn no_page_output_returns_none() {
        assert_eq!(pages_typeset("This is XeTeX\n(./main.aux)\n"), None);
    }
}
