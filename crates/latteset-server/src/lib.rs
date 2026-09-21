//! Latteset 无 GUI 交互层（roadmap ⑥）：**headless 服务 + CLI + MCP**。
//!
//! 目标：让 harness / Agent 不经 GUI 直接驱动「读 → 改 → 编译验证 → 修」闭环（[cli-mcp-plan.md]）。
//!
//! 设计要点（命令面与 [cli-mcp-plan.md] §1/§2 一致；落地时的三处偏差见该文 §4，旧编号对照见 §6）：
//!
//! 1. **一次性命令语义**：本进程不 spawn watcher、不跑 scheduler actor——`compile` 直接实例化
//!    [`LatexmkRunner`] 跑一趟并**同步返回结果**。AI 要的是"跑一次、拿结果"，不需要合并队列/防抖；
//!    代价是**没有超时重试/手动终止**（超时树杀仍在 runner 内生效），AI 场景可接受。
//! 2. **状态是"会话内"的**：`status`/`errors` 返回**本进程最近一次**编译结果（MCP server 常驻时
//!    就是本次会话）。*偏差*：原计划 P0-1 想让 GUI 侧也写一份快照，但 GUI 有事件流、且 CLI 是
//!    独立进程读不到那份内存——所以快照只放在这里，GUI 与 core 均零改动。
//! 3. **耗时在调用处测**：`elapsed_ms` 由本层用 `Instant` 计时。*偏差*：不给 `CompileOutcome` 加字段
//!    （那会牵动 core 全部构造点与测试），收益相同。
//! 4. **路径/SyncTeX 策略共用 core**：`pdf_path_for_root` / `resolve_inverse`（含 ㉒ 的生成产物回落）
//!    都在 `latteset_core::synctex`，GUI 命令面与这里调同一份，避免行为漂移（原计划 P0-3）。
//! 5. **临时文件目录约定**：与 GUI 相同——中间文件 `tmp/`、PDF 副本在项目根、`.synctex.gz` 在 `tmp/`。
//!    **同一项目不要同时用 GUI 和 CLI 编译**（两路 latexmk 会抢 `tmp/`；见 cli-mcp-plan §5）。
//!
//! [cli-mcp-plan.md]: ../../docs/cli-mcp-plan.md

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use latteset_core::outline::{load_cached, normalize_path, OutlineInput};
use latteset_core::project::{
    detect_root, is_tree_excluded, resolve_creatable_in_project, resolve_in_project, resolve_project_root,
    FileSystem, ProjectState,
};
use latteset_core::scheduler::CompileRunner;
use latteset_core::settings::Settings;
use latteset_core::synctex::{pdf_path_for_root, resolve_inverse, synctex_data_path, SourcePosition,
    SyncTexPosition, SyncTexProvider};
use latteset_core::types::{
    CompileKind, CompileOutcome, CompileRequest, ErrorEntry, OutlineNode,
};
use latteset_infra::fs::TokioFs;
use latteset_infra::runner::LatexmkRunner;
use latteset_infra::storage::SettingsStorage;

pub mod mcp;

/// 服务错误：`{code, message}`，与 GUI 命令面的 `CmdError` 同形（Agent 只需处理一套错误形状）。
#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[serde(tag = "code", content = "message")]
pub enum ServerError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Internal(String),
}

impl ServerError {
    pub fn exit_code(&self) -> i32 {
        match self {
            // 用法/环境类问题（路径不存在、未打开项目…）与内部错误都算失败；agent 看 JSON 里的 code
            ServerError::NotFound(_) | ServerError::Invalid(_) => 2,
            ServerError::Internal(_) => 3,
        }
    }
}

/// 公式预览（roadmap ㊸ 切片 2 的 headless 入口；GUI 的悬停浮层在切片 3 接同一套 core/infra）。
#[derive(Debug, Clone, Serialize)]
pub struct MathPreview {
    pub ok: bool,
    /// 本次调用的墙钟。**首次**（冷）与**同一公式二次**（走 runner 的 A 闸门）差别很大。
    pub elapsed_ms: u64,
    /// 缓存键 = `<项目>/tmp/snippet/<键>/` 的目录名（`core::snippet::snippet_key`）。
    pub key: String,
    /// 片段产物（`<项目>/tmp/snippet/<键>/main.pdf`）。
    pub pdf_path: Option<String>,
    /// `auto` = 悬停即编译；`button` = 首次片段编译 > 3 s（外部工具等），该项目退回按钮式。
    pub mode: String,
    /// 失败原因（**不留白**：公式排不出来时给一句话）。
    pub message: Option<String>,
}

/// 编译报告（`compile` 的返回体，也是 `status`/`errors` 的数据源）。
#[derive(Debug, Clone, Serialize)]
pub struct CompileReport {
    /// `success` / `failed` / `timeout` / `aborted` / `io_error`
    pub status: String,
    /// 失败细类（内容错误/超时/已终止）；成功为 null。
    pub failure: Option<String>,
    /// 结构化错误（内容错误时来自 `.log` 解析 + 诊断）。
    pub errors: Vec<ErrorEntry>,
    /// PDF 产物路径（成功时）。
    pub pdf_path: Option<String>,
    /// 墙钟耗时（毫秒）——AI 可据此判断项目规模/是否超时。
    pub elapsed_ms: u64,
    pub engine: String,
    pub root_file: String,
    /// 本次实际强度：`full`（latexmk 收敛）或 `quick`（单趟直调引擎，引用可能落后一趟）。
    pub kind: String,
    /// 请求的是 `Quick`，但**实际**跑成了 Full ⇒ true。两种来源：① 首编无产物被 runner 自动升级；
    /// ② 库形态下编辑触发档在收敛预算内跑到了稳定（见 `CONVERGENCE_SUPPORTED`）。
    /// 判据只有一条：`请求 Quick ∧ 上报 Full`（字段名沿用既有 JSON 契约，不改名）。
    pub upgraded_from_quick: bool,
}

/// 打开项目的结果。
#[derive(Debug, Clone, Serialize)]
pub struct ProjectReport {
    pub root: String,
    pub root_file: Option<String>,
    pub root_candidates: Vec<String>,
    pub settings: Settings,
}

/// 一个 headless 会话：进程内持有项目/设置/最近编译结果。
///
/// 单线程使用（CLI 每个进程只跑一条命令；MCP server 串行处理请求），故不需要锁——
/// 这也让"每条命令都是显式、可预测的"这一点保持简单。
pub struct Session {
    fs: Arc<dyn FileSystem>,
    sync: Arc<dyn SyncTexProvider>,
    storage: Arc<SettingsStorage>,
    project: Option<ProjectState>,
    settings: Settings,
    last: Option<CompileReport>,
    /// 传给 runner 的超时（秒）；来自生效设置。
    timeout_secs: u64,
    /// 大纲增量缓存（roadmap ⑦a）：MCP server 常驻时复用未变化文件的扫描结果；
    /// 项目根变化由 core 内部自动作废。CLI 每进程一条命令，等于不复用（无害）。
    outline_cache: latteset_core::outline::OutlineCache,
}

impl Session {
    /// 构造会话：`config_dir` 传**与 GUI 相同的**应用配置目录（Windows 下 `%APPDATA%\com.latteset.app`）。
    pub fn new(config_dir: PathBuf) -> Self {
        let fs: Arc<dyn FileSystem> = Arc::new(TokioFs);
        let storage = Arc::new(SettingsStorage::new(config_dir.join("settings.json")));
        Self {
            fs,
            // SyncTeX 默认**自解析**（与 GUI 侧同一个工厂，避免两条入口行为漂移）：
            // 不依赖系统 `synctex` 二进制（它来自 TeX Live）。`LATTESET_SYNCTEX=cli` 可切回。
            sync: latteset_infra::synctex::default_provider(),
            storage,
            project: None,
            settings: Settings::default(),
            last: None,
            timeout_secs: 120,
            outline_cache: latteset_core::outline::OutlineCache::new(),
        }
    }

    /// 用系统默认位置构造（Windows/macOS/Linux 对齐 Tauri 的 `app_config_dir`）。
    pub fn with_default_config() -> Self {
        Self::new(default_config_dir())
    }

    pub fn fs(&self) -> Arc<dyn FileSystem> {
        self.fs.clone()
    }

    pub fn project(&self) -> Option<&ProjectState> {
        self.project.as_ref()
    }

    fn project_or_err(&self) -> Result<&ProjectState, ServerError> {
        self.project
            .as_ref()
            .ok_or_else(|| ServerError::Invalid("尚未打开项目：先执行 project_open".into()))
    }

    /// 打开项目（等价 GUI 的 `open_project`）：解析项目根 + 读项目覆盖 + 合并设置 + 探测根文件。
    pub async fn open_project(&mut self, folder: &Path) -> Result<ProjectReport, ServerError> {
        let root = resolve_project_root(self.fs.as_ref(), folder)
            .await
            .map_err(|e| path_err(e, folder, "项目目录"))?;
        let overrides = self.storage.load_overrides(self.fs.as_ref(), &root).await;
        let global = self.storage.load_global(self.fs.as_ref()).await;
        let settings = SettingsStorage::effective(&global, &overrides);
        self.timeout_secs = settings.compile.timeout_secs as u64;

        let (root_file, candidates) = match settings.root_file.clone() {
            Some(rel) => {
                let joined = root.join(&rel);
                let resolved = resolve_in_project(self.fs.as_ref(), &root, &joined)
                    .await
                    .map_err(|e| path_err(e, &rel, "root_file 指向的文件"))?;
                (Some(resolved), Vec::new())
            }
            None => {
                let resolution = detect_root(self.fs.as_ref(), &root)
                    .await
                    .map_err(|e| ServerError::Internal(format!("扫描项目失败：{e}")))?;
                (resolution.unique(), resolution.candidates())
            }
        };

        self.project = Some(ProjectState {
            root: root.clone(),
            root_file: root_file.clone(),
        });
        self.settings = settings.clone();
        Ok(ProjectReport {
            root: root.to_string_lossy().into_owned(),
            root_file: root_file.map(|p| p.to_string_lossy().into_owned()),
            root_candidates: candidates
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            settings,
        })
    }

    /// 当前项目报告（与 GUI 的 `get_project` 同语义）：未用覆盖时重新探测候选。
    pub async fn project_report(&self) -> Result<ProjectReport, ServerError> {
        let project = self.project_or_err()?;
        let candidates = if project.root_file.is_some() {
            Vec::new() // 已定根文件（含手动覆盖）→ 不再探测（与 GUI 一致）
        } else {
            detect_root(self.fs.as_ref(), &project.root)
                .await
                .map_err(|e| ServerError::Internal(format!("扫描项目失败：{e}")))?
                .candidates()
        };
        Ok(ProjectReport {
            root: project.root.to_string_lossy().into_owned(),
            root_file: project.root_file.as_ref().map(|p| p.to_string_lossy().into_owned()),
            root_candidates: candidates
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            settings: self.settings.clone(),
        })
    }

    /// 生效设置（只读）。
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// 最近一次编译结果（本进程内）。
    pub fn last_report(&self) -> Option<&CompileReport> {
        self.last.as_ref()
    }

    /// 编译一次并**等待结果**（`compile --wait` 的实现）。
    ///
    /// `quick = true` 走单趟直调引擎（快，但目录/引用可能落后一趟）；默认 Full（多趟 + bib/索引）。
    pub async fn compile(&mut self, quick: bool) -> Result<CompileReport, ServerError> {
        let project = self
            .project
            .clone()
            .ok_or_else(|| ServerError::Invalid("尚未打开项目：先执行 project_open".into()))?;
        let root_file = project.root_file.clone().ok_or_else(|| {
            ServerError::Invalid(format!(
                "未确定根文件（候选中 {} 个）：请手工指定后再编译",
                project.root.display()
            ))
        })?;

        let request = CompileRequest {
            root_file: root_file.clone(),
            project_root: project.root.clone(),
            engine: self.settings.compile.engine,
            timeout: std::time::Duration::from_secs(self.timeout_secs),
            kind: if quick { CompileKind::Quick } else { CompileKind::Full },
        };
        // headless 不消费流式反馈（CLI 一条命令一个 JSON；MCP 暂无 notifications）：
        // 传 NoProgress，行为与接线前一致（引擎输出仍被读取，只是不产出事件）。
        let runner = build_runner(
            self.fs.clone(),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let started = Instant::now();
        let outcome = runner
            .compile(request, tokio_util::sync::CancellationToken::new())
            .await;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let pdf = pdf_path_for_root(&project.root, Some(&root_file));
        let report = match outcome {
            CompileOutcome::Success { kind, .. } => CompileReport {
                status: "success".into(),
                failure: None,
                errors: Vec::new(),
                pdf_path: Some(pdf.to_string_lossy().into_owned()),
                elapsed_ms,
                engine: engine_name(&self.settings),
                root_file: root_file.to_string_lossy().into_owned(),
                kind: kind_name(kind),
                upgraded_from_quick: quick && kind == CompileKind::Full,
            },
            CompileOutcome::ContentError { errors } => CompileReport {
                status: "failed".into(),
                failure: Some("content_error".into()),
                errors,
                pdf_path: None,
                elapsed_ms,
                engine: engine_name(&self.settings),
                root_file: root_file.to_string_lossy().into_owned(),
                kind: kind_name(if quick { CompileKind::Quick } else { CompileKind::Full }),
                upgraded_from_quick: false,
            },
            CompileOutcome::Timeout { entry } => CompileReport {
                status: "timeout".into(),
                failure: Some("timeout".into()),
                errors: vec![entry],
                pdf_path: None,
                elapsed_ms,
                engine: engine_name(&self.settings),
                root_file: root_file.to_string_lossy().into_owned(),
                kind: kind_name(if quick { CompileKind::Quick } else { CompileKind::Full }),
                upgraded_from_quick: false,
            },
            CompileOutcome::Aborted => CompileReport {
                status: "aborted".into(),
                failure: Some("aborted".into()),
                errors: Vec::new(),
                pdf_path: None,
                elapsed_ms,
                engine: engine_name(&self.settings),
                root_file: root_file.to_string_lossy().into_owned(),
                kind: kind_name(if quick { CompileKind::Quick } else { CompileKind::Full }),
                upgraded_from_quick: false,
            },
            CompileOutcome::IoError { message } => CompileReport {
                status: "io_error".into(),
                failure: Some("io".into()),
                errors: vec![ErrorEntry {
                    message,
                    file: None,
                    line: None,
                    kind: latteset_core::types::ErrorKind::Io,
                    diagnosis: None,
                }],
                pdf_path: None,
                elapsed_ms,
                engine: engine_name(&self.settings),
                root_file: root_file.to_string_lossy().into_owned(),
                kind: kind_name(if quick { CompileKind::Quick } else { CompileKind::Full }),
                upgraded_from_quick: false,
            },
        };
        self.last = Some(report.clone());
        Ok(report)
    }

    /// **公式预览**（roadmap ㊸ 切片 2）：把这一个公式连项目导言区装成独立小文档编译，返回单页产物。
    ///
    /// 隔离（ADR/§6.11.5 的硬要求）：落点 `<项目>/tmp/snippet/<键>/`，**独立的 project_root**、
    /// **不经调度器**（不产出 `compile-status`/`pdf-updated`，也不碰权威 `<stem>.pdf` 与主编译的
    /// `tmp/main.*`）。`mode=button` 表示首次片段编译超过 3 s（多为 latexmk 拖起外部工具），
    /// 该项目应退回"悬停出按钮"而不是自动编译。
    pub async fn compile_math(&mut self, formula: &str) -> Result<MathPreview, ServerError> {
        let project = self
            .project
            .clone()
            .ok_or_else(|| ServerError::Invalid("尚未打开项目：先执行 project_open".into()))?;
        let root_file = project.root_file.clone().ok_or_else(|| {
            ServerError::Invalid(format!(
                "未确定根文件（候选中 {} 个）：请手工指定后再做公式预览",
                project.root.display()
            ))
        })?;
        let source = self
            .fs
            .read_to_string(&root_file)
            .await
            .map_err(|e| {
                ServerError::Internal(format!("读根文件失败（{}）：{e}", root_file.display()))
            })?;
        let key = latteset_core::snippet::snippet_key(
            source.split("\\begin{document}").next().unwrap_or(&source),
            formula,
        );
        let document = latteset_core::snippet::build_snippet_document(&source, &project.root, formula)
            .map_err(|e| ServerError::Invalid(e.to_string()))?;
        // 建目录/写文档是文件系统操作 ⇒ 落在 infra（ADR-0010）。
        let snippet_root =
            latteset_infra::snippet::prepare_snippet_dir(&project.root, &key, &document).map_err(
                |e| {
                    ServerError::Internal(format!(
                        "准备片段目录失败（{}）：{e}",
                        project.root.display()
                    ))
                },
            )?;
        let entry = snippet_root.join("main.tex");

        let request = CompileRequest {
            root_file: entry,
            project_root: snippet_root,
            engine: self.settings.compile.engine,
            // 片段应当秒级完成；给一个短上限，卡住就放弃而不是拖住编辑器（与 A 同口径）。
            timeout: std::time::Duration::from_secs(30),
            kind: CompileKind::Quick,
        };
        let runner = build_runner(
            self.fs.clone(),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let started = Instant::now();
        let outcome = runner
            .compile(request, tokio_util::sync::CancellationToken::new())
            .await;
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let mode = if elapsed_ms > 3_000 { "button" } else { "auto" };

        let (ok, pdf_path, message) = match outcome {
            CompileOutcome::Success { pdf_path, .. } => {
                (true, Some(pdf_path.to_string_lossy().into_owned()), None)
            }
            // 片段单独编译失败是**常见**情形（宏定义在正文里、`\cite` 没有 aux、`\input` 了正文才
            // 定义的东西）——如实给第一条错误，别装作有预览（也不留白）。
            CompileOutcome::ContentError { errors } => (
                false,
                None,
                Some(format!(
                    "片段无法单独编译：{}",
                    errors.first().map(|e| e.message.as_str()).unwrap_or("（无细节）")
                )),
            ),
            CompileOutcome::Timeout { .. } => (false, None, Some("片段编译超时（30 s）".into())),
            CompileOutcome::Aborted => (false, None, Some("片段编译已中止".into())),
            CompileOutcome::IoError { message } => (false, None, Some(message)),
        };
        Ok(MathPreview {
            ok,
            elapsed_ms,
            key,
            pdf_path,
            mode: mode.into(),
            message,
        })
    }

    /// 文档大纲（源结构树）：与 GUI 同一实现（`core::outline`），headless 直接读盘、无缓冲。
    ///
    /// 走**增量缓存**（roadmap ⑦a）：headless 没有编辑器缓冲，每次读盘取内容，但内容未变的文件
    /// 复用上次的扫描结果——常驻的 MCP server 连续查询大纲时不再整张 include 图重扫。
    pub async fn outline(&mut self) -> Result<Vec<OutlineNode>, ServerError> {
        let project = self.project_or_err()?.clone();
        let buffers = std::collections::HashMap::new();
        let fallback = if project.root_file.is_none() {
            Some(self.list_tex_files().await?)
        } else {
            None
        };
        Ok(load_cached(
            &OutlineInput {
                root: &project.root,
                root_file: project.root_file.as_deref(),
                changed_buffers: &buffers,
                open_paths: None,
                fallback_files: fallback.as_deref(),
            },
            &mut self.outline_cache,
            self.fs.as_ref(),
        )
        .await)
    }

    /// 项目内 `.tex` 文件（绝对路径，排除 `tmp/` 与隐藏项）——与根文件探测同一套排除规则。
    pub async fn list_tex_files(&self) -> Result<Vec<PathBuf>, ServerError> {
        let project = self.project_or_err()?;
        latteset_core::project::collect_tex_files(self.fs.as_ref(), &project.root)
            .await
            .map_err(|e| ServerError::Internal(format!("扫描项目失败：{e}")))
    }

    /// 项目文件树（递归；`all = false` 时只列 `.tex`，`true` 时列全部非排除文件）。
    pub async fn tree(&self, all: bool) -> Result<Vec<String>, ServerError> {
        let project = self.project_or_err()?;
        let mut out = Vec::new();
        let mut stack = vec![project.root.clone()];
        while let Some(dir) = stack.pop() {
            let entries = self
                .fs
                .read_dir(&dir)
                .await
                .map_err(|e| ServerError::Internal(format!("读目录失败（{}）：{e}", dir.display())))?;
            for e in entries {
                if is_tree_excluded(&e.path, &project.root) {
                    continue;
                }
                if e.is_dir {
                    stack.push(e.path);
                } else if all || e.path.extension().is_some_and(|x| x.eq_ignore_ascii_case("tex")) {
                    out.push(e.path.to_string_lossy().into_owned());
                }
            }
        }
        out.sort();
        Ok(out)
    }

    /// 读文件（D8：必须在项目根内）。
    pub async fn read(&self, path: &Path) -> Result<String, ServerError> {
        let project = self.project_or_err()?;
        let resolved = resolve_in_project(self.fs.as_ref(), &project.root, path)
            .await
            .map_err(|e| path_err(e, path, "文件"))?;
        self.fs.read_to_string(&resolved).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                ServerError::Invalid(format!(
                    "{} 不是 UTF-8 编码（中文旧文件常见 GBK/GB18030）；请先转成 UTF-8 再改",
                    resolved.display()
                ))
            } else {
                ServerError::Internal(format!("读文件失败（{}）：{e}", resolved.display()))
            }
        })
    }

    /// 写文件（D8：目标可以不存在，但**父目录必须在项目内且已存在**）。
    ///
    /// 与 GUI 的 `save_all` 同一契约：不代建目录（Agent 要新建子目录时先建目录再写）。
    pub async fn write(&self, path: &Path, content: &str) -> Result<PathBuf, ServerError> {
        let project = self.project_or_err()?;
        let target = resolve_creatable_in_project(self.fs.as_ref(), &project.root, path)
            .await
            .map_err(|e| match e {
                latteset_core::project::PathError::NotFound => ServerError::NotFound(format!(
                    "写入路径的父目录不存在：{}（本工具不代建目录：先在项目内创建该目录，或写到已存在的目录）",
                    path.display()
                )),
                other => path_err(other, path, "写入路径"),
            })?;
        self.fs
            .write(&target, content)
            .await
            .map_err(|e| ServerError::Internal(format!("写文件失败（{}）：{e}", target.display())))?;
        Ok(target)
    }

    /// 源码 → PDF（正向定位）。
    pub async fn synctex_forward(
        &self,
        file: &Path,
        line: u32,
        column: u32,
    ) -> Result<SyncTexPosition, ServerError> {
        let project = self.project_or_err()?;
        let pdf = pdf_path_for_root(&project.root, project.root_file.as_deref());
        let src = SourcePosition {
            file: file.to_path_buf(),
            line,
            column: column as i32,
        };
        self.sync
            .forward(&src, &pdf)
            .await
            .map_err(|e| self.sync_err(e, &pdf, project))
    }

    /// PDF → 源码（反向定位；命中生成产物时按 core 策略就近回落，roadmap ㉒）。
    pub async fn synctex_inverse(
        &self,
        page: u32,
        x: f32,
        y: f32,
    ) -> Result<latteset_core::synctex::InverseResolution, ServerError> {
        let project = self.project_or_err()?;
        let pdf = pdf_path_for_root(&project.root, project.root_file.as_deref());
        // 同步数据缺失时给更准确的话（"先编译一次"），而不是笼统的 CLI 报错
        if !self.fs.exists(&pdf).await.unwrap_or(false) {
            return Ok(latteset_core::synctex::InverseResolution {
                source: None,
                note: Some("同步失败：还没有编译产物，先执行 compile".into()),
            });
        }
        let resolved = resolve_inverse(self.sync.as_ref(), &project.root, &pdf, page, x, y).await;
        if resolved.source.is_none() && !self.fs.exists(&synctex_data_path(&project.root, project.root_file.as_deref())).await.unwrap_or(false) {
            return Ok(latteset_core::synctex::InverseResolution {
                source: None,
                note: Some("同步失败：没有找到 .synctex.gz（编译时未生成同步数据），重新编译一次即可".into()),
            });
        }
        Ok(resolved)
    }

    fn sync_err(&self, e: latteset_core::synctex::SyncTexError, pdf: &Path, project: &ProjectState) -> ServerError {
        let _ = (pdf, project);
        ServerError::Internal(format!("SyncTeX：{e}"))
    }

    /// 归一化项目内路径（供 MCP 的 buffers 等场景使用，与 GUI 的 `resolvePath` 同源）。
    pub fn normalize(&self, path: &Path) -> PathBuf {
        PathBuf::from(normalize_path(&path.to_string_lossy()))
    }
}

/// 组装编译 runner（headless 的**装配点**，与 `src-tauri/src/lib.rs` 的 GUI 装配点同口径）。
///
/// 默认 = **子进程形态**（`LatexmkRunner`）；只有在构建时打开 `tectonic-lib` 特性**且**运行期
/// 开关 `LATTESET_TECTONIC_LIB=1` 时才用 Tectonic 库形态（方案 §3.5：默认关闭）。
/// **D1：失败不自动回退** —— 这里只决定装配哪个 runner，运行中失败就是失败。
///
/// 库形态的两条配置走环境变量（设置面的形态位属后续收口）：
/// - `LATTESET_TECTONIC_BUNDLE`：bundle 来源（本地目录 bundle 走 `file:///…` 或相对路径）；
/// - `LATTESET_TECTONIC_CACHE`：产品缓存目录（`bundles/` + `formats/` 的父目录）。
///
/// 缓存目录不注入会把 ~24 MB 的 `.fmt` 扔进用户项目（方案 §5.2 / HL-11 硬约束），所以这里的
/// 兜底也必须是**空的**缓存目录而不是项目目录：headless 用配置目录同级的 `cache`。
fn build_runner(
    fs: Arc<dyn FileSystem>,
    progress: Arc<dyn latteset_core::scheduler::CompileProgress>,
) -> Arc<dyn CompileRunner> {
    #[cfg(feature = "tectonic-lib")]
    {
        if latteset_tectonic::lib_form_enabled() {
            let mut lib = latteset_tectonic::TectonicLibRunner::new(fs.clone(), progress);
            let cache = latteset_tectonic::cache_dir_from_env().or_else(|| {
                let dir = default_config_dir().parent().map(|p| p.join("cache"));
                dir
            });
            if let Some(cache) = cache {
                lib = lib.with_cache_dir(cache);
            }
            return Arc::new(lib.with_bundle(latteset_tectonic::bundle_from_env()));
        }
    }
    Arc::new(LatexmkRunner::new(fs, progress))
}

/// 引擎名 = core 的 [`Engine::binary_name`]（**唯一副本**；这里曾手写一份同值的 match）。
fn engine_name(s: &Settings) -> String {
    s.compile.engine.binary_name().to_string()
}

/// 编译强度字面量 = core 的 [`CompileKind::as_str`]（同上：别两处各写一遍）。
fn kind_name(k: CompileKind) -> String {
    k.as_str().to_string()
}

fn path_err(e: latteset_core::project::PathError, path: &Path, what: &str) -> ServerError {
    use latteset_core::project::PathError;
    match e {
        PathError::NotFound => ServerError::NotFound(format!("{what}不存在：{}", path.display())),
        PathError::RootUnavailable => {
            ServerError::Internal(format!("项目根不可访问：{}", path.display()))
        }
        PathError::Outside => ServerError::Invalid(format!("{what}在项目外：{}", path.display())),
        PathError::NotADirectory => ServerError::Invalid(format!("不是目录：{}", path.display())),
    }
}

/// 与 GUI 相同的应用配置目录（Tauri `app_config_dir`）：
/// Windows `%APPDATA%\<identifier>`、macOS `~/Library/Application Support/<identifier>`、
/// Linux `$XDG_CONFIG_HOME/<identifier>`（无则 `~/.config/<identifier>`）。
///
/// 环境变量 `LATTESET_CONFIG_DIR` 可覆盖（测试/CI 用；GUI 侧无此变量，因此不影响一致性）。
pub fn default_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("LATTESET_CONFIG_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    const ID: &str = "com.latteset.app";
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join(ID);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Library/Application Support").join(ID);
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.trim().is_empty() {
                return PathBuf::from(xdg).join(ID);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".config").join(ID);
        }
    }
    PathBuf::from(".").join(ID)
}

/// 只影响未指定 `--project` 的情况：把相对路径解析成绝对路径（避免 cwd 漂移）。
pub fn absolutize(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(p)
    }
}

/// 供 CLI/MCP 共用的"未打开项目时自动打开"辅助。
pub async fn ensure_open(session: &mut Session, project: Option<&Path>) -> Result<ProjectReport, ServerError> {
    match project {
        Some(p) => session.open_project(&absolutize(p)).await,
        None => {
            let cwd = std::env::current_dir().map_err(|e| ServerError::Internal(format!("取当前目录失败：{e}")))?;
            session.open_project(&cwd).await
        }
    }
}

#[cfg(test)]
mod tests;
