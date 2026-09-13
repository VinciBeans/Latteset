//! 解析结果模型（modules.md §4）。

/// 消息类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Error,
    Warning,
}

/// 单条结构化日志消息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogMessage {
    pub kind: MessageKind,
    /// 聚合后的可读消息（跨行聚合）。
    pub message: String,
    /// 出错时栈顶文件（若可确定）。
    pub file: Option<String>,
    /// "l.<n>" 行号（仅 Error 有效）。
    pub line: Option<u32>,
    /// 起始原始行（快照测试与调试用）。
    pub raw: String,
}
