//! IO 抽象：core 唯一的文件访问入口（modules.md §3.2 / 设计决策 D4）。

use async_trait::async_trait;
use std::io;
use std::path::PathBuf;

/// 目录项（携带 is_dir，避免二次 stat）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// core 唯一的 IO 抽象。src-tauri 用 tokio::fs 实现；测试用 FakeFS。
#[async_trait]
pub trait FileSystem: Send + Sync {
    /// 非递归列出目录子项。
    async fn read_dir(&self, path: &std::path::Path) -> io::Result<Vec<DirEntry>>;
    /// 读文本文件（严格 UTF-8；非 UTF-8 即报错）。
    async fn read_to_string(&self, path: &std::path::Path) -> io::Result<String>;

    /// 读文本文件（**容错解码**）：非法 UTF-8 字节替换为 U+FFFD，不报错。
    ///
    /// 用于诊断类产物（`.log`）：**不保证合法 UTF-8** —— GBK 源文件会让 pdflatex
    /// 把原始字节回显进日志（见 [`crate::log_parser::decode_log`] 的实测说明）。
    /// 此时"有损但可用"远好于"读取失败、用户拿不到任何错误信息"。
    ///
    /// 默认实现退化为严格读取；真实实现（TokioFs）覆盖为 lossy 解码。
    async fn read_to_string_lossy(&self, path: &std::path::Path) -> io::Result<String> {
        self.read_to_string(path).await
    }
}
