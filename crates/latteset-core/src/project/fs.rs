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

    /// 规范化绝对路径（解析 `.`/`..`/软链接）。
    ///
    /// 契约：返回**对外可用形态**——Windows 上必须剥掉 `\\?\` verbatim 前缀
    /// （前端 resolvePath 只认盘符/斜杠开头，见 infra::fs::strip_verbatim）。
    async fn canonicalize(&self, path: &std::path::Path) -> io::Result<PathBuf>;

    /// 目标是否为目录（打开项目时校验用；避免调用方自己 stat）。
    async fn is_dir(&self, path: &std::path::Path) -> io::Result<bool>;

    /// 路径是否存在（文件或目录）。
    ///
    /// 用途（roadmap ㉘）：runner 判断"是否已有构建产物（`tmp/<stem>.aux`）"以决定
    /// Quick 单趟能否成立——避免调用方绕过 trait 直接 `std::fs::metadata`。
    async fn exists(&self, path: &std::path::Path) -> io::Result<bool>;

    /// 写入（覆盖）文本文件。父目录必须已存在——建目录属于上层策略。
    async fn write(&self, path: &std::path::Path, contents: &str) -> io::Result<()>;

    /// **从 `offset` 起**读新增内容（容错解码），返回新文本与新的偏移量（字节）。
    ///
    /// 用途（roadmap「阶段 2 · 流式输出」）：编译期间**尾随 `tmp/<stem>.log`**。
    /// 实测：引擎的 stdout 在非 TTY 下是 4KB **块缓冲**（页标记与错误要攒够一块才落盘），
    /// 而 `.log` 是**按页 flush** 的——所以"边编译边报进度/报错"必须读日志文件。
    ///
    /// 契约：
    /// - `offset` 超过文件长度（文件被重写/截断）时，从 0 重新开始（`offset` 回落到新长度）；
    /// - 返回的 `offset` 是**原始字节**偏移（不是字符数），即便文本经过有损解码；
    /// - 文件不存在 → `NotFound`（调用方自行决定是否重试）。
    ///
    /// 默认实现退化为"读全量再从 offset 切"（正确但每次全读）；真实实现（TokioFs）
    /// 覆盖为 seek + 读增量。
    async fn read_appended(
        &self,
        path: &std::path::Path,
        offset: u64,
    ) -> io::Result<(String, u64)> {
        let text = self.read_to_string_lossy(path).await?;
        let bytes = text.as_bytes();
        let start = usize::try_from(offset).unwrap_or(0).min(bytes.len());
        Ok((String::from_utf8_lossy(&bytes[start..]).into_owned(), bytes.len() as u64))
    }
}
