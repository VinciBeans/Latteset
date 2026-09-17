//! SyncTeX 数据模型（modules.md §5）。

use std::path::PathBuf;

/// 源码位置（反向定位结果）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePosition {
    pub file: PathBuf,
    pub line: u32,
    /// 列号；-1 = 未知（synctex 1.21 实测输出 Column:-1，ADR-0008 风险落地）。
    pub column: i32,
}

/// PDF 位置（正向定位结果）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyncTexPosition {
    pub page: u32,
    pub x: f32,
    pub y: f32,
}

/// SyncTeX 错误。
///
/// 变体的区别是**语义**，调用方据此决定"要不要等 / 要不要试别的候选位置"（roadmap ㊱）：
/// - [`SyncTexError::Io`]：设备/进程层的瞬时失败（CLI 起不来、退出码非 0）—— **值得退避重试**；
/// - [`SyncTexError::Busy`]：**同步数据正被重写**（读被占用、gzip 半截、解不出页面）—— 值得重试
///   （这正是编译期竞争的表现），但它是**数据级**问题：反向定位换 y 候选不会变好；
/// - [`SyncTexError::Unavailable`]：**同步数据不存在** —— 引擎写 `.synctex` 是"写 `(busy)` 再改名"
///   （改名原子）⇒ 读不到就是**从没编译过**（或 `tmp/` 被清），属**确定性**失败：不重试、也不试候选；
/// - [`SyncTexError::Parse`]：**确定性**的"没有这段映射"（"同步数据里没有这个源文件""第 N 行没有
///   对应位置"）—— 重试一万次也是同一个答案，必须立刻返回（否则一次失败查询白等 0.6 s）。
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum SyncTexError {
    #[error("同步失败：{0}")]
    Io(String),
    #[error("同步数据正被重写：{0}")]
    Busy(String),
    #[error("同步数据不存在：{0}")]
    Unavailable(String),
    #[error("输出解析失败：{0}")]
    Parse(String),
}
