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

    /// 引擎可执行文件名（roadmap ㉘ 的 Quick 路径直调它，不经 latexmk）。
    pub fn binary_name(&self) -> &'static str {
        match self {
            Engine::XeLaTeX => "xelatex",
            Engine::PdfLaTeX => "pdflatex",
            Engine::LuaLaTeX => "lualatex",
        }
    }

    /// 该引擎是否产出 **XDV**（页级复用 A/B/C 的唯一输入）。
    ///
    /// 只有 XeTeX 有 XDV 这个概念：`xelatex -no-pdf` 写 `tmp/<stem>.xdv`，PDF 再由 `xdvipdfmx`
    /// 按需转换。另外两个引擎**没有**这个中间产物，实测（2026-09，见
    /// [现代引擎实测](../../../docs/research/modern-engines-zh.md) §3.1）：
    /// - **pdfLaTeX**：`-no-pdf` 是**未识别选项**（`unrecognized option '-no-pdf'`）→ Quick 直接跑不起来；
    /// - **LuaLaTeX**：`-no-pdf` 被**静默忽略**（仍直接写 PDF），且中文下不可能有 DVI/XDV
    ///   （`luatexja`：`DVI output is not supported in LuaTeX-ja`）。
    ///
    /// ⇒ 这两个引擎的 Quick 必须走"引擎自己写 PDF"的形态：不加 `-no-pdf`、不做 `xdvipdfmx` 转换，
    /// 也没有页哈希可用（下游按"无法判定"保守全量刷新，见 `PdfUpdated.pages == 0` 的语义）。
    pub fn writes_xdv(&self) -> bool {
        matches!(self, Engine::XeLaTeX)
    }
}

/// 编译强度（roadmap ㉘）。
///
/// 实测依据（`scripts/bench-single-pass.mjs`）：直调引擎单趟比完整 latexmk 快 **40% 中位**
/// （六档 27.8%–55.5% 全部达标），因为省掉 latexmk 的外层机制（perl 启动 + `.fdb_latexmk`
/// 依赖库读写）与它判断出的收敛趟；两个路径的**实际工作量本质相同**（1 趟 LaTeX + 1 次转换）。
///
/// **代价**：单趟的目录/交叉引用页码落后一趟（实测：插 400 行后 `.toc` 中某章页码 29 → 41，
/// 即该趟 PDF 显示的是旧值 29）。→ 因此 `Quick` 只用于**编辑期**，必须由 `Full` 收敛兜底，
/// 且 UI 需提示"引用待更新"（见 design.md §延迟预算实测附节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileKind {
    /// 单趟直调引擎：快，但引用/目录可能落后一趟。编辑期使用。
    Quick,
    /// 完整 latexmk：多趟收敛 + bibtex/biber/索引。首编、手动编译、空闲收敛使用。
    Full,
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
    /// 本次编译是否为**草稿**（`CompileKind::Quick`）——roadmap ㉘。
    /// 前端据此提示"引用待更新"，并在停手后触发一次完整收敛。
    /// 失败/排队阶段同样携带，避免前端在收敛完成前误清提示。
    pub draft: bool,
}

/// pdf-updated 事件载荷。
///
/// `changed_pages` / `pages`（2026-09，docs/research/incremental-edit-x-dvi.md 的 B/C 两个功能点）：
/// 本次编译相对**上一次**变化的页号（1-based、升序）。前端语义：
/// - `pages > 0` 且 `changed_pages` **非空** → 只有这些页需要重绘，其余页复用 canvas 位图；
/// - `pages > 0` 且 `changed_pages` **为空** → 逐页字节完全相同 → **跳过重载**（新旧 PDF 逐页等价，
///   刷新只会白重绘并可能引起视觉跳动）；
/// - `pages == 0` → **无法判定**（XDV 缺失/损坏/首次运行拿不到）→ 按全量刷新处理。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PdfUpdated {
    pub path: String,
    /// 本次变化的页号（1-based）；空表 ≠ "无法判定"——后者看 `pages == 0`。
    pub changed_pages: Vec<u32>,
    /// 本次编译的页数；`0` 表示页信息不可用。
    pub pages: u32,
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

/// `synctex_inverse` 命令输出（roadmap ⑤/㉒）。
///
/// 为什么不是一个裸的 `SourcePositionDto`：反向定位**可能没有可跳转的源码**——
/// 命中生成产物（`tmp/main.toc` 等）、系统宏包（`article.cls`），或尚未产生同步数据。
/// 这些情况既不该静默失败（用户点了没反应），也不该把生成文件当源码打开。
/// 故：`source = None` 时用 `note` 说明原因；`note` 也可在成功时补充"已回落到最近源码行"。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct InverseResultDto {
    /// 可跳转的源码位置；`None` = 该处没有可打开的源码。
    pub source: Option<SourcePositionDto>,
    /// 给用户看的一句话（失败原因 / 回落说明）；`None` = 正常直连，无需提示。
    pub note: Option<String>,
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
    /// 编译强度（roadmap ㉘）：编辑触发 = Quick；首编/手动/空闲收敛 = Full。
    /// runner 在 Quick 但**尚无构建产物**时会自动升级为 Full（见 infra::runner）。
    pub kind: CompileKind,
}

/// 编译结果：runner 的输出，调度器据此走决策表（modules.md §2.3）。
#[derive(Debug, Clone, PartialEq)]
pub enum CompileOutcome {
    /// 编译成功且 PDF 已拷贝到项目根。
    ///
    /// `kind` 是**实际执行**的强度（roadmap ㉘）：请求为 `Quick` 但项目尚无构建产物时，
    /// runner 会自动升级为 `Full`（否则引用全是 `??`）。状态事件据此报 `draft`，
    /// 只有真跑了单趟才提示"引用待更新"。
    Success {
        pdf_path: PathBuf,
        kind: CompileKind,
        /// 本次产出的 XDV **页哈希**（顺序即页号）。
        ///
        /// 空表示**无法判定**（未产出 XDV / 读取失败 / 不是 XDV）——调度器与前端都按
        /// "保守全量刷新"处理。用途见 docs/research/incremental-edit-x-dvi.md：
        /// 页哈希全同 ⇒ 这次编译的排版结果逐页未变 ⇒ 可跳过预览重载（B）与 PDF 转换（A）。
        page_hashes: Vec<u64>,
    },
    /// 超时强制终止（runner 已树杀进程）。
    ///
    /// 携带**证据化**的错误条目（roadmap ㉕）：超时不再静默重试（同一上限重跑一遍
    /// 只是再等一个完整超时窗口），改为把「为什么慢 / 疑似卡住 + 怎么改 + 一键提高超时重试」
    /// 直接摆到错误列表里。证据（首编、源文件数、日志页码）由 runner 现场采集。
    Timeout { entry: ErrorEntry },
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
    fn engine_binary_names() {
        // Quick 路径直调这些可执行文件（roadmap ㉘）
        assert_eq!(Engine::XeLaTeX.binary_name(), "xelatex");
        assert_eq!(Engine::PdfLaTeX.binary_name(), "pdflatex");
        assert_eq!(Engine::LuaLaTeX.binary_name(), "lualatex");
    }

    #[test]
    fn only_xelatex_writes_xdv() {
        // 页级复用（A/B/C）依赖 `-no-pdf` + `xdvipdfmx`，而这两样只有 XeTeX 有：
        // pdflatex 会把 `-no-pdf` 当未识别选项，lualatex 静默忽略它（见方法文档）。
        assert!(Engine::XeLaTeX.writes_xdv());
        assert!(!Engine::PdfLaTeX.writes_xdv());
        assert!(!Engine::LuaLaTeX.writes_xdv());
    }

    #[test]
    fn dto_roundtrip_serde() {
        let dto = CompileStatusDto {
            phase: CompilePhase::Failed,
            kind: Some(FailureKind::Timeout),
            draft: true,
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"draft\":true"), "draft 必须进 DTO：{json}");
        let back: CompileStatusDto = serde_json::from_str(&json).unwrap();
        assert_eq!(back, dto);
    }

    #[test]
    fn engine_serde_snake_case() {
        let json = serde_json::to_string(&Engine::XeLaTeX).unwrap();
        assert_eq!(json, "\"xelatex\"");
        let back: Engine = serde_json::from_str("\"lualatex\"").unwrap();
        assert_eq!(back, Engine::LuaLaTeX);
    }
}
