//! 命令面实现（modules.md §8）：DTO 进出、无业务逻辑。
//!
//! 安全底线（设计决策 D8）：路径类参数一律校验在项目根内
//! （自建命令没有 Tauri 权限模型兜底，必须自守）。

use crate::events::SettingsChangedEvent;
use tauri_specta::Event;
use latteset_infra::storage::SettingsStorage;
use serde::Serialize;
use specta::Type;
use std::path::{Path, PathBuf};
use tauri::{Manager, State};
use latteset_core::compose::compile_request_manual;
use latteset_core::engine::EngineInfo;
use latteset_core::newfile::{DocKind, DocLanguage};
use latteset_core::project::{
    is_tex_file, resolve_creatable_in_project, resolve_in_project, resolve_project_root, PathError,
    ProjectState, RootResolution,
};
use latteset_core::settings::{apply_patch, validate_overrides, ProjectOverrides, Settings, SettingsPatch};
use latteset_core::synctex::SourcePosition;
use latteset_core::types::{
    DirEntryInfo, FileContent, InverseResultDto, OutlineNode, ProjectInfo, SourcePositionDto,
    SyncTexTarget,
};
use latteset_core::project::FileSystem;
use latteset_core::scheduler::SchedulerHandle;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// 命令错误契约：{ code, message }（modules.md §4 错误模型）。
#[derive(Debug, Error, Serialize, Type)]
#[serde(tag = "code", content = "message")]
pub enum CmdError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Internal(String),
}

impl From<std::io::Error> for CmdError {
    fn from(e: std::io::Error) -> Self {
        CmdError::Internal(e.to_string())
    }
}

/// 应用状态（Tauri managed state，§1 全局状态清单）。
pub struct AppState {
    pub fs: Arc<dyn FileSystem>,
    pub scheduler: SchedulerHandle,
    pub sync: Arc<dyn latteset_core::synctex::SyncTexProvider>,
    pub project: Arc<RwLock<Option<ProjectState>>>,
    pub settings: Arc<RwLock<Settings>>,
    pub overrides: Arc<RwLock<ProjectOverrides>>,
    pub storage: Arc<SettingsStorage>,
    pub watch: latteset_infra::watch::WatchHandle,
    pub app: tauri::AppHandle,
    /// 大纲增量缓存（roadmap ⑦a）：按内容指纹复用未变化文件的扫描结果 + 记住打开缓冲。
    /// 单写者（命令面）；项目根变化时 core 内部自动作废，无需外部清理。
    pub outline: tokio::sync::Mutex<latteset_core::outline::OutlineCache>,
}

/// 路径策略（D8）在 core `project::paths`：命令面只负责把失败原因翻译成对外错误契约。
fn path_error(e: PathError, path: &Path, what: &str) -> CmdError {
    match e {
        PathError::NotFound => CmdError::NotFound(format!("{what}不存在：{}", path.display())),
        PathError::RootUnavailable => {
            CmdError::Internal(format!("项目根不可访问：{}", path.display()))
        }
        PathError::Outside => CmdError::Invalid(format!("{what}在项目外：{}", path.display())),
        PathError::NotADirectory => CmdError::Invalid(format!("不是目录：{}", path.display())),
    }
}

/// 公式预览的结果（roadmap ㊸ 切片 3）。`hit=false` = 光标不在公式里 ⇒ 前端**不显示**浮层
/// （这是最常见的情况，不该当成错误）。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MathPreviewDto {
    /// 光标是否落在数学公式里（扫描器口径见 `core::math`）。
    pub hit: bool,
    /// 片段是否编译成功（`hit=true` 才有意义）。
    pub ok: bool,
    /// 本次墙钟（毫秒）—— 首次与"同一公式二次"（走 A 闸门）差别很大。
    pub elapsed_ms: u32,
    /// 缓存键 = `<项目>/tmp/snippet/<键>/` 的目录名。
    pub key: String,
    /// 片段产物路径（成功时）。
    pub pdf_path: Option<String>,
    /// `auto` / `button`：首次片段编译 > 3 s ⇒ 该项目退回"悬停出按钮"。
    pub mode: String,
    /// 失败原因（**不留白**：排不出来时给一句话）。
    pub message: Option<String>,
    /// 命中的公式正文（含定界符；调试与日志用）。
    pub formula: Option<String>,
}

/// 公式预览（roadmap ㊸ 切片 3）：把**光标所在的那一个公式**连项目导言区单独编译成单页 PDF。
///
/// 隔离（硬要求）：落点 `<项目>/tmp/snippet/<键>/`，**独立 project_root**、**不经调度器**
/// （不产出 `compile-status`/`pdf-updated`，也不碰权威 `<stem>.pdf` 与主编译的 `tmp/main.*`）。
/// 形态：**Tectonic 专属**（ADR-0014）；非 Tectonic 形态下这条命令仍可用，但按 ADR 不做优化适配。
#[tauri::command]
#[specta::specta]
pub async fn compile_math(
    text: String,
    offset: u32,
    state: State<'_, AppState>,
) -> Result<MathPreviewDto, CmdError> {
    use latteset_core::scheduler::{CompileRunner, NoProgress};
    use latteset_core::types::{CompileKind, CompileOutcome};

    let project = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目，无法做公式预览".into()))?;
    let root_file = project
        .root_file
        .clone()
        .ok_or_else(|| CmdError::Invalid("未确定根文件，无法做公式预览".into()))?;

    // 光标在哪个公式里：扫描器在 core（与 headless 同一份口径）。
    let Some(expr) = latteset_core::math::math_at(&text, offset as usize) else {
        return Ok(MathPreviewDto {
            hit: false,
            ok: false,
            elapsed_ms: 0,
            key: String::new(),
            pdf_path: None,
            mode: "auto".into(),
            message: None,
            formula: None,
        });
    };

    // 片段文档要项目自己的导言区 ⇒ 以**项目根文件**为准（编辑器缓冲可能未存盘，但导言区口径不受影响）。
    let source = state.fs.read_to_string(&root_file).await?;
    let key = latteset_core::snippet::snippet_key(
        source.split("\\begin{document}").next().unwrap_or(&source),
        &expr.text,
    );
    let document = latteset_core::snippet::build_snippet_document(&source, &project.root, &expr.text)
        .map_err(|e| CmdError::Invalid(e.to_string()))?;
    // 建目录/写文档是文件系统操作 ⇒ 落在 infra（ADR-0010）。
    let snippet_root = latteset_infra::snippet::prepare_snippet_dir(&project.root, &key, &document)?;
    let entry = snippet_root.join("main.tex");

    let settings = state.settings.read().await.clone();
    let runner = crate::runner_switch::SwitchableRunner::new(
        state.fs.clone(),
        Arc::new(NoProgress),
        state.settings.clone(),
        state.app.path().app_cache_dir().ok(),
    );
    let request = latteset_core::types::CompileRequest {
        root_file: entry,
        project_root: snippet_root,
        engine: settings.compile.engine,
        // 片段应当秒级完成；给一个短上限，卡住就放弃而不是拖住编辑器（与 A 同口径）。
        timeout: std::time::Duration::from_secs(30),
        kind: CompileKind::Quick,
    };
    let started = std::time::Instant::now();
    let outcome = runner
        .compile(request, tokio_util::sync::CancellationToken::new())
        .await;
    let elapsed_ms = started.elapsed().as_millis().min(u32::MAX as u128) as u32;
    let mode = if elapsed_ms > 3_000 { "button" } else { "auto" };

    let (ok, pdf_path, message) = match outcome {
        CompileOutcome::Success { pdf_path, .. } => {
            (true, Some(pdf_path.to_string_lossy().into_owned()), None)
        }
        // 片段单独编译失败是**常见**情形（宏定义在正文里、`\cite` 没有 aux）——如实给第一条错误。
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
    debug!(key = %key, elapsed_ms, ok, "公式预览完成");
    Ok(MathPreviewDto {
        hit: true,
        ok,
        elapsed_ms,
        key,
        pdf_path,
        mode: mode.into(),
        message,
        formula: Some(expr.text),
    })
}

/// 路径校验（D8）：canonicalize 后必须落在项目根内（IO 走 FileSystem trait，不碰 OS API）。
async fn validate_in_project(state: &AppState, path: &Path) -> Result<PathBuf, CmdError> {
    let project = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目".into()))?;
    resolve_in_project(state.fs.as_ref(), &project.root, path)
        .await
        .map_err(|e| path_error(e, path, "路径"))
}

/// 根文件探测（modules.md §5.4）：收集 .tex → 读内容 → 候选收敛。
///
/// 抽成独立函数供两处复用：`open_project`（无覆盖时）与 `update_settings`
/// （清除 root_file 覆盖时回到"自动探测"语义——必须与重新打开项目的结果一致）。
async fn detect_root(
    state: &AppState,
    root: &Path,
) -> Result<RootResolution, CmdError> {
    // IO 编排在 core（`project::detect_root`），命令面只做错误翻译——watch 的热更新路径共用同一份实现
    latteset_core::project::detect_root(state.fs.as_ref(), root)
        .await
        .map_err(|e| CmdError::Internal(format!("扫描项目失败：{e}")))
}

/// 当前项目信息（`get_project` 与 `open_project` 共用）。
fn project_info(project: &ProjectState, root_candidates: Vec<PathBuf>) -> ProjectInfo {
    ProjectInfo {
        root: project.root.clone(),
        root_file: project.root_file.clone(),
        root_candidates,
    }
}

// ---------------------------------------------------------------- 项目

#[tauri::command]
#[specta::specta]
pub async fn open_project(folder: String, state: State<'_, AppState>) -> Result<ProjectInfo, CmdError> {
    let folder = PathBuf::from(&folder);
    // 校验目录存在且可读（canonicalize + is_dir 均经 FileSystem trait）
    let canonical = resolve_project_root(state.fs.as_ref(), &folder)
        .await
        .map_err(|e| path_error(e, &folder, "目录"))?;

    // 项目设置：覆盖 + 合并（modules.md §6）
    // global 必须读**纯全局**（磁盘 settings.json），不能读 state.settings——后者可能已是
    // 上一个项目合并后的 effective，否则打开第二个项目会继承第一个项目的覆盖值（root_file/mode）。
    // 此处与 update_settings 的 load_global 保持一致（见本文件 update_settings）。
    let overrides = state.storage.load_overrides(state.fs.as_ref(), &canonical).await;
    let global = state.storage.load_global(state.fs.as_ref()).await;
    let settings = SettingsStorage::effective(&global, &overrides);
    *state.overrides.write().await = overrides;
    *state.settings.write().await = settings.clone();

    // 根文件：手动覆盖优先；否则探测（modules.md §5.4）
    let (root_file, root_candidates) = match settings.root_file.clone() {
        Some(override_path) => {
            // 路径安全（D8）：root_file 覆盖解析后必须落在项目根内且为 .tex。
            // 仅 `starts_with` 对含 `..` 的路径不够（词法匹配），须 canonicalize 解析后再判。
            let joined = canonical.join(&override_path);
            let resolved = resolve_in_project(state.fs.as_ref(), &canonical, &joined)
                .await
                .map_err(|e| path_error(e, &override_path, "root_file 指向的文件"))?;
            if !is_tex_file(&resolved) {
                return Err(CmdError::Invalid(format!(
                    "root_file 不是 .tex 文件：{}",
                    override_path.display()
                )));
            }
            // 用户已显式指定 → 不再探测，候选为空
            (Some(resolved), Vec::new())
        }
        None => {
            let resolution = detect_root(&state, &canonical).await?;
            // 路径来自 FileSystem::read_dir / canonicalize——基础设施层已剥离 verbatim 前缀
            (resolution.unique(), resolution.candidates())
        }
    };

    // canonicalize 已在基础设施层剥掉 Windows verbatim 前缀（infra::fs）
    let root = canonical.clone();
    let project = ProjectState {
        root: root.clone(),
        root_file,
    };
    *state.project.write().await = Some(project.clone());
    state.watch.set_project_root(Some(root.clone()));
    info!(
        "打开项目：{}（根文件 {:?}，候选 {} 个）",
        root.display(),
        project.root_file.as_ref().map(|p| p.display().to_string()),
        root_candidates.len()
    );

    Ok(project_info(&project, root_candidates))
}

/// 当前项目信息（只读）：前端在 root_file 变化后重新同步用（roadmap P0-②-1）。
/// `root_candidates` 不在内存里保存，重新探测以获得与 `open_project` 一致的语义。
#[tauri::command]
#[specta::specta]
pub async fn get_project(state: State<'_, AppState>) -> Result<ProjectInfo, CmdError> {
    let project = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目".into()))?;
    // 已手动指定根文件 → 候选为空（与 open_project 一致）；否则重探测
    let root_candidates = if project.root_file.is_some() {
        Vec::new()
    } else {
        detect_root(&state, &project.root).await?.candidates()
    };
    Ok(project_info(&project, root_candidates))
}

#[tauri::command]
#[specta::specta]
pub async fn list_dir(path: String, state: State<'_, AppState>) -> Result<Vec<DirEntryInfo>, CmdError> {
    let path = validate_in_project(&state, Path::new(&path)).await?;
    let root = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目".into()))?
        .root;
    // 递归收集（排除 tmp/ 与隐藏项；树需要所有扩展名）
    let mut out = Vec::new();
    let mut stack = vec![path];
    while let Some(dir) = stack.pop() {
        for entry in state.fs.read_dir(&dir).await? {
            if latteset_core::project::is_tree_excluded(&entry.path, &root) {
                continue;
            }
            out.push(DirEntryInfo {
                name: entry
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path: entry.path.clone(),
                is_dir: entry.is_dir,
            });
            if entry.is_dir {
                stack.push(entry.path);
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- 文件

#[tauri::command]
#[specta::specta]
pub async fn read_file(path: String, state: State<'_, AppState>) -> Result<String, CmdError> {
    let path = validate_in_project(&state, Path::new(&path)).await?;
    match state.fs.read_to_string(&path).await {
        Ok(text) => Ok(text),
        // 非 UTF-8（中文旧文件常见 GBK/GB18030）：给出**能行动**的中文提示——
        // 此前是英文 IO 错误「stream did not contain valid UTF-8」，用户看不懂也不知道怎么办。
        // 之所以不"容错解码后照常打开"：编辑器保存时会把替换字符（U+FFFD）写回磁盘，
        // 等于**静默损坏**用户文件——宁可拒绝打开并说清怎么转码（roadmap ㉓）。
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            Err(CmdError::Invalid(format!(
                "{name} 不是 UTF-8 编码（中文旧文件常见 GBK/GB18030），为避免保存时损坏原文件，编辑器不打开它。\
                 处理办法：用记事本/VS Code 等另存为 UTF-8（或 GB18030 → UTF-8 转换）后再打开"
            )))
        }
        Err(e) => Err(CmdError::from(e)),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn save_all(files: Vec<FileContent>, state: State<'_, AppState>) -> Result<(), CmdError> {
    for f in &files {
        save_content(&state, Path::new(&f.path), &f.content).await?;
    }
    Ok(())
}

async fn save_content(state: &AppState, path: &Path, content: &str) -> Result<(), CmdError> {
    // 目标可能不存在：对父目录 canonicalize 后拼接
    let project = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目".into()))?;
    let target = resolve_creatable_in_project(state.fs.as_ref(), &project.root, path)
        .await
        .map_err(|e| path_error(e, path, "路径"))?;
    state.fs.write(&target, content).await?;
    Ok(())
}

/// 新建**单层**目录（roadmap ㊺ 待办②）：软件内建文件夹，不必跳出到资源管理器。
///
/// 路径校验与写文件同一条路（D8：`resolve_creatable_in_project` ⇒ 落点必须在项目根内，
/// 且**父目录已存在**——建多层属于上层策略，前端也不会给出带 `/` 的目录名）。
/// 目标已存在 ⇒ **明确报错**而不是静默成功：静默成功会让用户以为建了一个其实没建的目录。
#[tauri::command]
#[specta::specta]
pub async fn create_dir(path: String, state: State<'_, AppState>) -> Result<(), CmdError> {
    let project = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目".into()))?;
    let target = resolve_creatable_in_project(state.fs.as_ref(), &project.root, Path::new(&path))
        .await
        .map_err(|e| path_error(e, Path::new(&path), "目录"))?;
    match state.fs.create_dir(&target).await {
        Ok(()) => {
            debug!("新建目录：{}", target.display());
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // 提示里用**项目内相对路径**：面板只有 256 px 宽，绝对路径要折三行才看得完
            let shown = target
                .strip_prefix(&project.root)
                .unwrap_or(&target)
                .display()
                .to_string();
            Err(CmdError::Invalid(format!("已存在同名文件或文件夹：{shown}")))
        }
        Err(e) => Err(CmdError::Internal(format!(
            "建目录失败（{}）：{e}",
            target.display()
        ))),
    }
}

// ---------------------------------------------------------------- 删除 / 改名（文件管理补齐）

/// 删除文件或**整个目录**（roadmap ㊼）。
///
/// 安全边界（三条，缺一条就是数据事故）：
/// 1. **必须已存在**且**在项目根内**（`resolve_in_project`，D8）；
/// 2. **不能是项目根本身**（否则一次点击删掉整个项目）；
/// 3. 目录递归删除 —— 前端已弹过二次确认并把"里面有多少东西"讲清楚。
///
/// 删除后项目状态由前端收口（关掉相关标签、根文件被删则重探）。
#[tauri::command]
#[specta::specta]
pub async fn delete_path(path: String, state: State<'_, AppState>) -> Result<(), CmdError> {
    let project = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目".into()))?;
    // 项目根：`resolve_in_project` 内部已经 canonicalize 过一次（失败会给 RootUnavailable），
    // 这里再取一次是为了拿"项目根本身"来做边界比对（禁止删/改项目根）。
    let target = resolve_in_project(state.fs.as_ref(), &project.root, Path::new(&path))
        .await
        .map_err(|e| path_error(e, Path::new(&path), "路径"))?;
    let root = state.fs.canonicalize(&project.root).await?;
    if target == root {
        return Err(CmdError::Invalid("不能删除项目根目录".into()));
    }    let is_dir = state.fs.is_dir(&target).await.unwrap_or(false);
    if is_dir {
        state.fs.remove_dir_all(&target).await?;
    } else {
        state.fs.remove_file(&target).await?;
    }
    info!("已删除{}：{}", if is_dir { "目录" } else { "文件" }, target.display());
    Ok(())
}

/// 改名（roadmap ㊽）：**同目录内**改个名字（不做跨目录移动）。
///
/// 规则：
/// - `from` 必须已存在且在项目根内，且**不是项目根本身**；
/// - `to` 的父目录必须已存在、落在项目根内，且**必须与 `from` 同目录**（v1 不做移动）；
/// - `to` 已存在 ⇒ 明确报错（**不覆盖** —— 覆盖式改名会静默毁掉一个文件）。
///
/// 返回**改名后的绝对路径**：前端要拿它去重映射打开的标签与根文件。
#[tauri::command]
#[specta::specta]
pub async fn rename_path(
    from: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<String, CmdError> {
    let project = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目".into()))?;
    let src = resolve_in_project(state.fs.as_ref(), &project.root, Path::new(&from))
        .await
        .map_err(|e| path_error(e, Path::new(&from), "原路径"))?;
    let root = state.fs.canonicalize(&project.root).await?;
    if src == root {
        return Err(CmdError::Invalid("不能给项目根目录改名".into()));
    }
    let dst = resolve_creatable_in_project(state.fs.as_ref(), &root, Path::new(&to))
        .await
        .map_err(|e| path_error(e, Path::new(&to), "新路径"))?;
    if dst.parent() != src.parent() {
        return Err(CmdError::Invalid(
            "改名只能在本目录内（暂不支持跨目录移动）".into(),
        ));
    }
    if state.fs.exists(&dst).await.unwrap_or(false) {
        let shown = dst.strip_prefix(&root).unwrap_or(&dst).display().to_string();
        return Err(CmdError::Invalid(format!("已存在同名文件或文件夹：{shown}")));
    }
    state.fs.rename(&src, &dst).await?;
    info!("已改名：{} → {}", src.display(), dst.display());
    Ok(dst.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------- 大纲

/// 文档大纲（源结构树）：解析在 core `outline` 模块（2026-09-03 从前端下沉）。
///
/// 输入（roadmap ⑦a 起为增量语义）：
/// - `buffers`：**本次新增/变更过**的打开缓冲（未列出的沿用上次上报的内容）——前端只发改动过的，
///   避免每次编译成功都把全部打开文件的全文推一遍（实测 462KB 一档 ≈10ms/文件的往返成本）；
/// - `open_paths`：当前打开的标签（后端据此淘汰已关闭文件的缓冲；不在此列的缓冲回到读盘）；
/// - `files`：无根文件时的兜底文件列表；项目根/根文件取当前项目状态。
///
/// 未变化的文件由 core 的 `OutlineCache` 按内容指纹复用扫描结果——**每次仍重新取内容**
/// （缓冲优先，否则读盘），所以外部改动不会被缓存钉住。
#[tauri::command]
#[specta::specta]
pub async fn get_outline(
    buffers: Vec<FileContent>,
    files: Option<Vec<String>>,
    open_paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<OutlineNode>, CmdError> {
    let project = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目".into()))?;
    let mut map = HashMap::with_capacity(buffers.len());
    for fc in &buffers {
        map.insert(latteset_core::outline::normalize_path(&fc.path), fc.content.clone());
    }
    let fallback = files.map(|v| v.into_iter().map(PathBuf::from).collect::<Vec<_>>());
    let mut cache = state.outline.lock().await;
    Ok(latteset_core::outline::load_cached(
        &latteset_core::outline::OutlineInput {
            root: &project.root,
            root_file: project.root_file.as_deref(),
            changed_buffers: &map,
            open_paths: Some(&open_paths),
            fallback_files: fallback.as_deref(),
        },
        &mut cache,
        state.fs.as_ref(),
    )
    .await)
}

// ---------------------------------------------------------------- 新建文件

/// 新建 `.tex` 的**最小骨架**（roadmap ㊺ §6.13.1-A）：新手向导选完"语言 × 类型 × 标题"之后，
/// 前端把这段内容交给 `save_all` 落盘。
///
/// 为什么由后端给内容而不是前端拼：这张"语言 × 类型"表在 `core::newfile`（与将来 headless 的
/// 入口共用同一份），前端只传选择值 —— 否则两边各拼一份，早晚会飘。
/// 无副作用、不需要项目上下文，所以它不进 `AppState`。
#[tauri::command]
#[specta::specta]
pub fn new_file_skeleton(lang: DocLanguage, kind: DocKind, title: Option<String>) -> String {
    latteset_core::newfile::document_skeleton(lang, kind, title.as_deref().unwrap_or_default())
}

// ---------------------------------------------------------------- 引擎清单

/// 引擎清单（roadmap ㊻）：**顺序、名字、说明与本机可用性都由后端给** —— 前端不再各写一份。
///
/// - 可用性 = PATH 上找得到该引擎需要的可执行文件（`latteset_infra::probe`，只查文件、不起进程）；
/// - Tectonic 额外认一条"库形态已编入"（库内嵌不调外部 `tectonic.exe`，见 ADR-0012）；
/// - `LATTESET_TEX_ENGINES=xelatex,tectonic` 可**覆盖清单与顺序**（CI / 裁剪发布用），
///   但它不能让不可用的引擎变可用（可用性是探测出来的事实）。
///
/// 不缓存：一次面板打开几趟目录查询，比缓存失效的复杂度便宜得多。
#[tauri::command]
#[specta::specta]
pub fn list_engines() -> Vec<EngineInfo> {
    latteset_core::engine::engine_list(
        &|name| latteset_infra::probe::find_in_path(name).is_some(),
        crate::runner_switch::LIB_FORM_COMPILED_IN,
        latteset_infra::probe::env_tex_engines().as_deref(),
    )
}

// ---------------------------------------------------------------- 编译

#[tauri::command]
#[specta::specta]
pub async fn compile_now(state: State<'_, AppState>) -> Result<(), CmdError> {
    let project = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目".into()))?;
    let settings = state.settings.read().await.clone();
    let ctx = latteset_core::compose::ComposeContext {
        project: &project,
        settings: &settings,
    };
    match compile_request_manual(ctx) {
        Some(req) => {
            debug!("手动编译: root={}", req.root_file.display());
            state.scheduler.compile(req);
            Ok(())
        }
        None => Err(CmdError::Invalid("未确定根文件，无法编译".into())),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn abort_compile(state: State<'_, AppState>) -> Result<(), CmdError> {
    state.scheduler.abort();
    Ok(())
}

/// 本次构建是否编入了 Tectonic **库形态**（`tectonic-lib` 特性）。
///
/// 设置面据此**禁用**该选项并说明原因，而不是让用户选了之后到编译时才炸 —— 那正是"功能开不出来"
/// 的另一面。（选了但没编进来时 runner 会显式报错，D1：不静默回退到子进程。）
#[tauri::command]
#[specta::specta]
pub fn lib_form_available() -> bool {
    crate::runner_switch::LIB_FORM_COMPILED_IN
}

/// 引擎形态（库形态方案 §6 P7 的「状态栏可见形态位」）。
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EngineForm {
    /// 子进程档：`latexmk` 驱动 xelatex/lualatex/pdflatex，或直调 `tectonic.exe`。
    Subprocess,
    /// Tectonic **库内嵌**。
    Lib,
    /// 形态位要库内嵌，但**本次构建没编入** `tectonic-lib` ⇒ 下一趟编译会显式失败（D1 不静默回退）。
    LibUnavailable,
}

/// 状态栏用的「实际形态」载荷。
#[derive(Debug, Clone, Serialize, Type)]
pub struct EngineFormDto {
    pub form: EngineForm,
    /// 形态位是否由 `LATTESET_TECTONIC_LIB` 强制（**它压过设置**，所以设置面选子进程也可能是库形态）。
    pub env_forced: bool,
    /// 本次构建是否编入库形态。
    pub lib_compiled_in: bool,
}

/// 当前**实际**会用的引擎形态。
///
/// **判定只做一次、就在后端**：这里直接调用 runner 用的那个纯函数
/// （[`latteset_core::settings::TectonicSettings::use_library_form`]），所以状态栏不可能与编译实际
/// 行为不一致。前端**不得**从 `compile.engine` 自己推形态 —— `LATTESET_TECTONIC_LIB` 能压过设置，
/// 只看设置会报出与实际不符的形态。
#[tauri::command]
#[specta::specta]
pub async fn engine_form(state: State<'_, AppState>) -> Result<EngineFormDto, CmdError> {
    let s = state.settings.read().await;
    let env_forced = crate::runner_switch::lib_form_forced_by_env();
    let wants_lib = s.tectonic.use_library_form(s.compile.engine, env_forced);
    Ok(EngineFormDto {
        form: if !wants_lib {
            EngineForm::Subprocess
        } else if crate::runner_switch::LIB_FORM_COMPILED_IN {
            EngineForm::Lib
        } else {
            EngineForm::LibUnavailable
        },
        env_forced,
        lib_compiled_in: crate::runner_switch::LIB_FORM_COMPILED_IN,
    })
}

// ---------------------------------------------------------------- 窗口外观

/// 把**原生标题栏**也纳入主题（Windows 走 DWM 沉浸式深色，经 tao 的 `set_theme`）。
///
/// 传的是**设置值**（`light|dark|system`）而不是解析后的深浅：`system` 交给系统去跟
/// （`set_theme(None)`，tao 自己监听系统主题变化）—— 若前端把解析结果推过来，用户切系统主题后
/// 标题栏就会卡在旧值上。
///
/// 前端在主题变化时调用；**窗口刚出现那一下由 `setup` 直接从磁盘设置定色**（见 `lib.rs`）——
/// 只靠这条命令会先闪一条白标题栏。
#[tauri::command]
#[specta::specta]
pub async fn set_window_theme(
    window: tauri::WebviewWindow,
    theme: latteset_core::settings::UiTheme,
) -> Result<(), CmdError> {
    apply_window_theme(&window, theme);
    Ok(())
}

/// 把主题落到系统层。失败**不致命**（平台不支持 / 无标题栏时只是外观没跟上），只记一条 warn。
///
/// **两条路都走**：
/// 1. `window.set_theme(..)`：Tauri 官方口径，也是非 Windows 平台唯一的路（菜单/内部状态跟着走）；
/// 2. Windows 上**再自己写一次 DWM 属性**：实测 `set_theme(Light)` **关不掉**已打开的沉浸式深色
///    （命令返回 Ok，但 `DWMWA_USE_IMMERSIVE_DARK_MODE` 仍是 1，标题栏卡在深色）⇒ 由我们决定
///    "深/浅"这个布尔，两个方向都写。
pub fn apply_window_theme(window: &tauri::WebviewWindow, theme: latteset_core::settings::UiTheme) {
    use latteset_core::settings::UiTheme;
    let t = match theme {
        UiTheme::Light => Some(tauri::Theme::Light),
        UiTheme::Dark => Some(tauri::Theme::Dark),
        UiTheme::System => None, // None = 跟随系统
    };
    if let Err(e) = window.set_theme(t) {
        tracing::warn!("设置窗口主题失败（标题栏可能没跟上）：{e}");
    }
    #[cfg(windows)]
    {
        let dark = match theme {
            UiTheme::Dark => true,
            UiTheme::Light => false,
            // `system` 在 Tauri 里没有"读当前系统深浅"的公开 API ⇒ 读注册表（与 tao 同源）
            UiTheme::System => !os_prefers_light(),
        };
        if let Ok(hwnd) = window.hwnd() {
            set_immersive_dark_titlebar(hwnd.0 as isize, dark);
        }
    }
}

/// 读 Windows 的"应用浅色"偏好（`AppsUseLightTheme`，0 = 深色）。
///
/// 读不到时**按浅色**处理（与 tao 的兜底一致）：宁可标题栏浅一点，也不要让浅色用户看到深色条。
#[cfg(windows)]
fn os_prefers_light() -> bool {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    use windows::core::w;
    let mut value: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    ok.is_ok() && value != 0
}

/// 直接写 `DWMWA_USE_IMMERSIVE_DARK_MODE`（`true` = 深色标题栏）。
///
/// 属性号 20 是 Win10 20H1+ / Win11；老版本（1809–1909）是 19 ⇒ 20 失败时回退 19。
/// 两次都失败只记 debug：这是纯外观，不该冒泡成错误。
#[cfg(windows)]
fn set_immersive_dark_titlebar(hwnd: isize, dark: bool) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
    let value: i32 = if dark { 1 } else { 0 };
    let h = HWND(hwnd as *mut _);
    let size = std::mem::size_of::<i32>() as u32;
    for attr in [20u32, 19u32] {
        let r = unsafe {
            DwmSetWindowAttribute(
                h,
                DWMWINDOWATTRIBUTE(attr as i32),
                &value as *const i32 as *const _,
                size,
            )
        };
        if r.is_ok() {
            tracing::debug!(dark, attr, "标题栏深色属性已写入");
            return;
        }
    }
    tracing::debug!(dark, "标题栏深色属性写入失败（该 Windows 版本可能不支持）");
}

// ---------------------------------------------------------------- SyncTeX

#[tauri::command]
#[specta::specta]
pub async fn synctex_forward(
    file: String,
    line: u32,
    column: u32,
    state: State<'_, AppState>,
) -> Result<SyncTexTarget, CmdError> {
    let project = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目".into()))?;
    let src = SourcePosition {
        file: PathBuf::from(&file),
        line,
        column: column as i32,
    };
    state
        .sync
        .forward(
            &src,
            &latteset_core::synctex::pdf_path_for_root(&project.root, project.root_file.as_deref()),
        )
        .await
        .map(|p| SyncTexTarget {
            page: p.page,
            x: p.x,
            y: p.y,
        })
        .map_err(|e| CmdError::Internal(e.to_string()))
}

/// PDF 点击 → 源码（roadmap ⑤/㉒）：**只回落到项目内真实源码**。
///
/// 策略本身（就近探测 + 生成产物分类 + 提示文案）在 core [`latteset_core::synctex::resolve_inverse`]，
/// 与 headless CLI/MCP 共用一份（roadmap ⑥-P0-3：避免 GUI 与 CLI 行为漂移）；
/// 这里只做 DTO 映射，并在"完全拿不到映射"时结合文件系统状态补一句更准确的话。
#[tauri::command]
#[specta::specta]
pub async fn synctex_inverse(
    page: u32,
    x: f32,
    y: f32,
    state: State<'_, AppState>,
) -> Result<InverseResultDto, CmdError> {
    let project = state
        .project
        .read()
        .await
        .clone()
        .ok_or_else(|| CmdError::Invalid("尚未打开项目".into()))?;
    let pdf = latteset_core::synctex::pdf_path_for_root(&project.root, project.root_file.as_deref());
    let resolved =
        latteset_core::synctex::resolve_inverse(state.sync.as_ref(), &project.root, &pdf, page, x, y)
            .await;

    Ok(match resolved.source {
        Some(pos) => InverseResultDto {
            source: Some(SourcePositionDto {
                file: pos.file.to_string_lossy().into_owned(),
                line: pos.line,
                column: pos.column,
            }),
            note: resolved.note,
        },
        None => InverseResultDto {
            source: None,
            // 全落空且是"同步不可用"类失败：用文件系统状态说清是没编译过还是别的原因（不猜）
            note: Some(match resolved.note {
                Some(note) if note.starts_with("同步失败：") => {
                    sync_unavailable_note(&state, &project, &pdf, &note).await
                }
                other => other.unwrap_or_else(|| "此处没有对应的源码位置".to_string()),
            }),
        },
    })
}

/// 同步不可用时的提示：结合文件系统状态说清"是没编译过还是别的原因"（不猜）。
async fn sync_unavailable_note(
    state: &AppState,
    project: &ProjectState,
    pdf: &Path,
    err: &str,
) -> String {
    let synctex =
        latteset_core::synctex::synctex_data_path(&project.root, project.root_file.as_deref());
    if !state.fs.exists(pdf).await.unwrap_or(false) {
        return "同步失败：还没有编译产物，先点「编译」".to_string();
    }
    if !state.fs.exists(&synctex).await.unwrap_or(false) {
        return "同步失败：没有找到 .synctex.gz（编译时未生成同步数据），重新编译一次即可".to_string();
    }
    err.to_string()
}

// ---------------------------------------------------------------- 设置

#[tauri::command]
#[specta::specta]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, CmdError> {
    Ok(state.settings.read().await.clone())
}

/// 局部更新（modules.md §6）：mode/debounce/timeout/engine → 全局文件；
/// root_file → 项目覆盖文件；随后重算有效设置并广播 settings-changed。
#[tauri::command]
#[specta::specta]
pub async fn update_settings(
    patch: SettingsPatch,
    state: State<'_, AppState>,
) -> Result<Settings, CmdError> {
    let project = state.project.read().await.clone();

    // 1. root_file 走项目覆盖
    if let Some(new_override) = &patch.root_file {
        let mut overrides = state.overrides.write().await.clone();
        overrides.root_file = new_override.clone();
        validate_overrides(&overrides)
            .map_err(|errs| CmdError::Invalid(errs.join("；")))?;

        // 解析为项目内绝对路径（D8）：失败即拒绝、**不落盘**——否则会存下一个下次打开
        // 必然失败的覆盖值，而用户当场看不到任何反馈（roadmap P0-②-1 顺手修）。
        // 清除覆盖（`null`）→ 回到「自动探测」，与重新打开项目同语义（复用 detect_root）。
        let resolved: Option<PathBuf> = match (new_override, &project) {
            (Some(rel), Some(project)) => {
                let joined = project.root.join(rel);
                let path = resolve_in_project(state.fs.as_ref(), &project.root, &joined)
                    .await
                    .map_err(|e| path_error(e, rel, "root_file 指向的文件"))?;
                if !is_tex_file(&path) {
                    return Err(CmdError::Invalid(format!(
                        "root_file 不是 .tex 文件：{}",
                        rel.display()
                    )));
                }
                Some(path)
            }
            // 无项目打开：只校验形式，覆盖值留待下次 open_project 解析
            (Some(_), None) => None,
            (None, Some(project)) => detect_root(&state, &project.root).await?.unique(),
            (None, None) => None,
        };

        if let Some(project) = &project {
            state.storage.save_overrides(&project.root, &overrides).await;
        }
        *state.overrides.write().await = overrides;

        // 同步**内存**项目状态：此前只更新覆盖与有效设置，`ProjectState.root_file` 保持旧值，
        // 症状是「在设置里指定了根文件 → 编译仍报『未确定根文件，无法编译』」，必须重开项目才生效
        // （roadmap P0-②-1 的阻断项：选择候选后必须当场可用）。
        if project.is_some() {
            let mut guard = state.project.write().await;
            if let Some(current) = guard.as_mut() {
                current.root_file = resolved.clone();
                debug!(
                    "root_file 已同步（内存）：{:?}",
                    current.root_file.as_ref().map(|p| p.display().to_string())
                );
            }
        }
    }

    // 2. 其余字段走全局设置
    //    注意：必须读**纯全局**（磁盘 settings.json），而不能用 state.settings（合并后的
    //    effective）——否则 effective 里的项目覆盖值会被当作“全局”参与 merge 的继承语义，
    //    清除（root_file=null）后生效值仍是旧的（清不掉），正是“覆盖无法清除”的根因。
    let mut global = state.storage.load_global(state.fs.as_ref()).await;
    let mut patch_global = patch.clone();
    patch_global.root_file = None; // root_file 已在上一步处理
    if patch_global != SettingsPatch::default() {
        apply_patch(&mut global, &patch_global)
            .map_err(|errs| CmdError::Invalid(errs.join("；")))?;
        state.storage.save_global(&global).await;
    }

    // 3. 重算有效设置
    let overrides = state.overrides.read().await.clone();
    let effective = SettingsStorage::effective(&global, &overrides);
    *state.settings.write().await = effective.clone();

    let _ = SettingsChangedEvent(effective.clone()).emit(&state.app);
    debug!("设置已更新：{:?}", effective.compile);
    Ok(effective)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(root: &str, root_file: Option<&str>) -> ProjectState {
        ProjectState {
            root: PathBuf::from(root),
            root_file: root_file.map(PathBuf::from),
        }
    }

    /// 便捷包装：`(root, root_file)` → PDF 路径。逻辑本体在 core（与 headless CLI/MCP 共用）。
    fn pdf_path_for_root(p: &ProjectState) -> PathBuf {
        latteset_core::synctex::pdf_path_for_root(&p.root, p.root_file.as_deref())
    }

    #[test]
    fn pdf_path_uses_root_stem_at_project_root() {
        assert_eq!(
            pdf_path_for_root(&project(r"C:\proj", Some(r"C:\proj\main.tex"))),
            PathBuf::from(r"C:\proj\main.pdf")
        );
    }

    #[test]
    fn pdf_path_falls_back_to_main_when_no_root() {
        assert_eq!(
            pdf_path_for_root(&project(r"C:\proj", None)),
            PathBuf::from(r"C:\proj\main.pdf")
        );
    }

    #[test]
    fn pdf_path_flattens_nested_root_to_project_root() {
        // 产物约定：PDF 打到项目根（design.md）。嵌套根文件（css/thesis.tex）也平铺到项目根。
        assert_eq!(
            pdf_path_for_root(&project(r"C:\proj", Some(r"C:\proj\css\thesis.tex"))),
            PathBuf::from(r"C:\proj\thesis.pdf")
        );
    }

    #[test]
    fn pdf_path_keeps_chinese_stem() {
        // 中文根文件 → 产物名沿用中文 stem（与 runner 的 pdf_dst 计算必须一致，否则预览找不到文件）
        assert_eq!(
            pdf_path_for_root(&project(
                r"E:\项目\中文测试工程",
                Some(r"E:\项目\中文测试工程\中文主文件.tex")
            )),
            PathBuf::from(r"E:\项目\中文测试工程\中文主文件.pdf")
        );
        // 中文子目录下的根文件同样平铺到项目根
        assert_eq!(
            pdf_path_for_root(&project(
                r"E:\项目\中文测试工程",
                Some(r"E:\项目\中文测试工程\章节\第一章.tex")
            )),
            PathBuf::from(r"E:\项目\中文测试工程\第一章.pdf")
        );
    }
}
