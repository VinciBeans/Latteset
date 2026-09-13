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
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum SyncTexError {
    #[error("同步失败：{0}")]
    Io(String),
    #[error("输出解析失败：{0}")]
    Parse(String),
}
