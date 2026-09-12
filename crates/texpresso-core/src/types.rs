//! 跨层类型。
//!
//! 分两类：
//! - **DTO**（`specta::Type` + `Serialize`）：跨 IPC 边界进出前端（modules.md §10 契约）；
//! - **内部类型**（`CompileRequest` / `CompileOutcome`）：只在 core 内流转，不跨 IPC。

use crate::log_parser::Diagnosis;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::time::Duration;

// ---------------------------------------------------------------- DTO

/// TeX 引擎（默认 xelatex，见 design.md）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum Engine {
    #[serde(rename = "xelatex")]
    XeLaTeX,
    #[serde(rename = "pdflatex")]
    PdfLaTeX,
    #[serde(rename = "lualatex")]
    LuaLaTeX,
}

impl Engine {
    /// latexmk 引擎开关（modules.md §2.6）。
    pub fn latexmk_flag(&self) -> &'static str {
        match self {
            Engine::XeLaTeX => "-xelatex",
            Engine::PdfLaTeX => "-pdf",
            Engine::LuaLaTeX => "-lualatex",
        }
    }
}

/// 编译模式（CONTEXT.md：连续编译 / 保存触发编译）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CompileMode {
    Continuous,
    OnSave,
}

/// 错误分类（design.md 失败语义 + IO 兜底）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    ContentError,
    Timeout,
    Aborted,
    Io,
}

/// 错误列表条目（modules.md §4 契约）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ErrorEntry {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub kind: ErrorKind,
    /// 诊断（roadmap ④）：把 `.log` 原始报错翻译成「原因 + 怎么改」。
    /// `None` = 匹配不到已知模式，前端降级为原文 + 行号（宁可不说，也不瞎说）。
    pub diagnosis: Option<Diagnosis>,
}

/// 编译阶段（modules.md §2.5 事件契约）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CompilePhase {
    Queued,
    Running,
    Success,
    Failed,
}

/// 失败原因（与 design.md 失败语义三路对应；Aborted = 手动终止）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Timeout,
    ContentError,
    Aborted,
}

/// compile-status 事件载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CompileStatusDto {
    pub phase: CompilePhase,
    pub kind: Option<FailureKind>,
}

/// pdf-updated 事件载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PdfUpdated {
    pub path: String,
}

/// files-changed 事件载荷。
/// `structural`：是否为结构变化（增/删/重命名）。内容修改（如自动保存）为 false——
/// 文件树因此可跳过重建（modules.md §12「文件树增量刷新」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct FilesChanged {
    pub paths: Vec<String>,
    pub structural: bool,
}

/// 文件写入载荷（save_all 命令输入）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

/// 打开项目后的项目信息（open_project / get_project 命令输出）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ProjectInfo {
    pub root: PathBuf,
    pub root_file: Option<PathBuf>,
    /// 根文件探测得到的候选（升序，项目内绝对路径）。
    ///
    /// 语义（roadmap P0-②-1）：**仅当未使用手动覆盖时**计算——
    /// - `Unique(p)` → `[p]`（探测到唯一根文件，`root_file` 即它）；
    /// - `Multiple(list)` → 全部候选（`root_file` 为 None，**由前端让用户选**）；
    /// - `None` → 空（`root_file` 为 None，前端退回"列出全部 .tex"）；
    /// - 手动覆盖生效 → 空（用户已指定，不再探测）。
    ///
    /// 此前 `Multiple` 的候选列表在命令层被直接丢弃，前端只拿到 `root_file: null`
    /// 且仅有 `console.warn`——表现为「打开项目没反应」。
    pub root_candidates: Vec<PathBuf>,
}

/// 文件树条目（list_dir 命令输出）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DirEntryInfo {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// 文档大纲节点（get_outline 命令输出；解析逻辑见 [`crate::outline`]，2026-09-03 从
/// 前端 `src/stores/outline.ts` 下沉，语义等价、单测锁定）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct OutlineNode {
    /// 标题（`\section[short]{title}` 取 `{...}` 内内容并 trim）。
    pub title: String,
    /// 结构层级：0=part,1=chapter,2=section,3=subsection,4=subsubsection,5=paragraph,6=subparagraph。
    pub level: u32,
    /// 项目内绝对路径（已归一化，与前端存储键一致）。
    pub file: String,
    /// 1-based 行号。
    pub line: u32,
    /// 文件基名（界面显示用：`file:line`）。
    #[serde(rename = "fileBase")]
    pub file_base: String,
    /// 子节点（按文档顺序）。
    pub children: Vec<OutlineNode>,
}

/// SyncTeX 正向定位结果（源码 → PDF）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct SyncTexTarget {
    pub page: u32,
    pub x: f32,
    pub y: f32,
}

/// SyncTeX 反向定位结果（PDF → 源码）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourcePositionDto {
    pub file: String,
    pub line: u32,
    /// 列号；-1 = 未知（synctex 1.21 输出契约）。
    pub column: i32,
}

// ---------------------------------------------------------------- 内部类型

/// 编译请求：调度器唯一认识的输入（modules.md §2.4 / D3）。
///
/// 由组合层构造并**携带全部所需信息**（引擎、超时在构造时从设置快照拷贝），
/// 调度器与 runner 不读任何外部状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileRequest {
    pub root_file: PathBuf,
    pub project_root: PathBuf,
    pub engine: Engine,
    pub timeout: Duration,
}

/// 编译结果：runner 的输出，调度器据此走决策表（modules.md §2.3）。
#[derive(Debug, Clone, PartialEq)]
pub enum CompileOutcome {
    /// 编译成功且 PDF 已拷贝到项目根。
    Success { pdf_path: PathBuf },
    /// 超时强制终止（runner 已树杀进程）。
    Timeout,
    /// 内容错误：进程非零退出，.log 已解析为错误条目。
    ContentError { errors: Vec<ErrorEntry> },
    /// 收到取消信号（手动终止，runner 已树杀进程）。
    Aborted,
    /// IO 失败（无法启动 latexmk、PDF 拷贝失败等）。
    IoError { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_latexmk_flags() {
        assert_eq!(Engine::XeLaTeX.latexmk_flag(), "-xelatex");
        assert_eq!(Engine::PdfLaTeX.latexmk_flag(), "-pdf");
        assert_eq!(Engine::LuaLaTeX.latexmk_flag(), "-lualatex");
    }

    #[test]
    fn engine_serde_snake_case() {
        let json = serde_json::to_string(&Engine::XeLaTeX).unwrap();
        assert_eq!(json, "\"xelatex\"");
        let back: Engine = serde_json::from_str("\"lualatex\"").unwrap();
        assert_eq!(back, Engine::LuaLaTeX);
    }

    #[test]
    fn dto_roundtrip_serde() {
        let dto = CompileStatusDto {
            phase: CompilePhase::Failed,
            kind: Some(FailureKind::Timeout),
        };
        let json = serde_json::to_string(&dto).unwrap();
        let back: CompileStatusDto = serde_json::from_str(&json).unwrap();
        assert_eq!(back, dto);
    }
}
