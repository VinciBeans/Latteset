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

/// **增量**页标记扫描器（roadmap「阶段 2 · 流式输出」）：把引擎输出**按块**喂进来，
/// 回报新出现的最大页码。成本只与**新增字节**成正比（不是每次重扫全部输出）。
///
/// 为什么不是"逐块独立匹配"：`[N]` 会被 `max_print_line` 折行，`[6` 与 `]` 之间可能隔着
/// 若干换行甚至正文回显（见上）。故每次把上一块的**尾部残留**（[`CARRY`] 字节）接到新块前面，
/// 再整体匹配——既避免跨块漏检，也不需要保留全部历史输出。
pub struct PageMarkerScanner {
    carry: String,
    max: Option<u32>,
}

/// 跨块残留窗口：折行标记的两半之间不会超过这个量级（实测最多隔几行 + 一小段正文回显）。
const CARRY: usize = 4096;

impl Default for PageMarkerScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl PageMarkerScanner {
    pub fn new() -> Self {
        Self {
            carry: String::new(),
            max: None,
        }
    }

    /// 追加一块输出；**页码变大**时返回新的最大页码，否则 `None`（调用方据此决定要不要发事件）。
    pub fn push(&mut self, chunk: &str) -> Option<u32> {
        if chunk.is_empty() {
            return None;
        }
        let mut text = String::with_capacity(self.carry.len() + chunk.len());
        text.push_str(&self.carry);
        text.push_str(chunk);
        let found = pages_typeset(&text);
        self.carry = tail_chars(&text, CARRY);
        let next = match (self.max, found) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, b) => b,
        };
        if next != self.max {
            self.max = next;
            return next;
        }
        None
    }

    /// 当前已知的最大页码。
    pub fn max(&self) -> Option<u32> {
        self.max
    }
}

/// 取字符串末尾 `n` 个**字符**（不切坏 UTF-8）。
fn tail_chars(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        return s.to_string();
    }
    s.chars().skip(count - n).collect()
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

    // ---------------- PageMarkerScanner（增量，阶段 2 流式输出）

    #[test]
    fn scanner_reports_only_when_page_grows() {
        let mut s = PageMarkerScanner::new();
        assert_eq!(s.push("[1] [2]"), Some(2));
        assert_eq!(s.push("[3]"), Some(3));
        assert_eq!(s.push("正文，无标记\n"), None); // 没变大 → 不发事件
        assert_eq!(s.push("[2] [1]"), None); // 旧页码（乱序回显）不算前进
        assert_eq!(s.max(), Some(3));
    }

    #[test]
    fn scanner_handles_marker_split_across_chunks() {
        // 数字与 `]` 被切开
        let mut s = PageMarkerScanner::new();
        assert_eq!(s.push("[12"), None);
        assert_eq!(s.push("3]"), Some(123));
        // 折行（max_print_line）跨块的场景：`[6` 与 `]` 之间隔若干行 + 正文回显
        let mut s2 = PageMarkerScanner::new();
        assert_eq!(s2.push("[6\n\n"), None);
        assert_eq!(s2.push("\n\n] 第二章"), Some(6));
    }

    #[test]
    fn scanner_survives_long_stream_without_keeping_history() {
        let mut s = PageMarkerScanner::new();
        for i in 1..=500u32 {
            let got = s.push(&format!("[{i}] 正文片段 {}\n", "x".repeat(200)));
            assert_eq!(got, Some(i));
        }
        assert_eq!(s.max(), Some(500));
        // 内部只保留尾部窗口，不随输出线性增长
        assert!(s.carry.chars().count() <= CARRY);
    }
}
