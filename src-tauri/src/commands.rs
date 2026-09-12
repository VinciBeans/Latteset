//! 命令面实现（modules.md §8）：DTO 进出、无业务逻辑。
//!
//! 安全底线（设计决策 D8）：路径类参数一律校验在项目根内
//! （自建命令没有 Tauri 权限模型兜底，必须自守）。

use crate::events::SettingsChangedEvent;
use tauri_specta::Event;
use texpresso_infra::storage::SettingsStorage;
use serde::Serialize;
use specta::Type;
use std::path::{Path, PathBuf};
use tauri::State;
use texpresso_core::compose::compile_request_manual;
use texpresso_core::project::{
    collect_tex_files, find_candidates, is_tex_file, resolve, resolve_creatable_in_project,
    resolve_in_project, resolve_project_root, PathError, ProjectState, RootResolution,
};
use texpresso_core::settings::{apply_patch, validate_overrides, ProjectOverrides, Settings, SettingsPatch};
use texpresso_core::synctex::{classify_inverse_target, InverseTarget, SourcePosition};
use texpresso_core::types::{
    DirEntryInfo, FileContent, InverseResultDto, OutlineNode, ProjectInfo, SourcePositionDto,
    SyncTexTarget,
};
use texpresso_core::project::FileSystem;
use texpresso_core::scheduler::SchedulerHandle;
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
    pub sync: Arc<dyn texpresso_core::synctex::SyncTexProvider>,
    pub project: Arc<RwLock<Option<ProjectState>>>,
    pub settings: Arc<RwLock<Settings>>,
    pub overrides: Arc<RwLock<ProjectOverrides>>,
    pub storage: Arc<SettingsStorage>,
    pub watch: texpresso_infra::watch::WatchHandle,
    pub app: tauri::AppHandle,
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
    let files = collect_tex_files(state.fs.as_ref(), root)
        .await
        .map_err(|e| CmdError::Internal(format!("扫描项目失败：{e}")))?;
    // 探测需要文件内容（\documentclass 声明 / \input 引用）：一次读入后闭包只查表，
    // core 的 find_candidates 保持纯函数（读盘一律经 FileSystem trait）。
    let mut contents: HashMap<PathBuf, String> = HashMap::with_capacity(files.len());
    for f in &files {
        if let Ok(text) = state.fs.read_to_string(f).await {
            contents.insert(f.clone(), text);
        }
    }
    Ok(resolve(find_candidates(&files, root, |p| {
        contents.get(p).cloned()
    })))
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
            if texpresso_core::project::is_tree_excluded(&entry.path, &root) {
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
    state.fs.read_to_string(&path).await.map_err(CmdError::from)
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

// ---------------------------------------------------------------- 大纲

/// 文档大纲（源结构树）：解析在 core `outline` 模块（2026-09-03 从前端下沉）。
/// 输入：打开标签的实时缓冲（**缓冲优先**，未落盘也反映）+ 无根文件时的兜底文件列表；
/// 项目根/根文件取当前项目状态。输出按文档顺序嵌套（file:line 定位用）。
#[tauri::command]
#[specta::specta]
pub async fn get_outline(
    buffers: Vec<FileContent>,
    files: Option<Vec<String>>,
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
        map.insert(texpresso_core::outline::normalize_path(&fc.path), fc.content.clone());
    }
    let fallback = files.map(|v| v.into_iter().map(PathBuf::from).collect::<Vec<_>>());
    Ok(texpresso_core::outline::load(
        &texpresso_core::outline::OutlineContext {
            root: &project.root,
            root_file: project.root_file.as_deref(),
            buffers: &map,
            fallback_files: fallback.as_deref(),
        },
        state.fs.as_ref(),
    )
    .await)
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
    let ctx = texpresso_core::compose::ComposeContext {
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

// ---------------------------------------------------------------- SyncTeX

fn pdf_path_for_root(project: &ProjectState) -> PathBuf {
    let stem = project
        .root_file
        .as_deref()
        .and_then(|f| f.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".into());
    project.root.join(format!("{stem}.pdf"))
}

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
        .forward(&src, &pdf_path_for_root(&project))
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
/// 为什么要这一层（2026-09 实测）：`synctex edit` 在生成内容上会返回生成它的中间文件——
/// 点 `multifile` 第 3 页目录区得到 `tmp/main.toc:15`，同一屏往上 50pt 却是 `main.tex:37`。
/// 直接把 `tmp/main.toc` 当跳转目标会打开一屏用户没写过的内容。
///
/// 策略：先按点击点定位；命中"非源码"时在**附近小范围探测**（y 上下 40/80pt），
/// 取第一个项目内源码（首个命中的偏移最小，故就是"最近"的那个）；仍无则忽略并给出提示。
/// 探测只在"没拿到源码"时发生，正常点击的延迟不变。
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
    let pdf = pdf_path_for_root(&project);

    // 候选点：原点优先，其后按偏移量从小到大（"最近"即第一个命中源码的点）
    const Y_OFFSETS: [f32; 5] = [0.0, -40.0, 40.0, -80.0, 80.0];
    let mut last_err: Option<String> = None;
    let mut first_target: Option<InverseTarget> = None;

    for (i, dy) in Y_OFFSETS.iter().enumerate() {
        let pos = texpresso_core::synctex::SyncTexPosition {
            page,
            x,
            y: y + dy,
        };
        match state.sync.inverse(&pos, &pdf).await {
            Ok(hit) => match classify_inverse_target(&project.root, &hit.file) {
                InverseTarget::Source(file) => {
                    let note = if i == 0 {
                        None
                    } else {
                        Some(format!(
                            "此处是自动生成的内容，已回落到最近的源码（{}:{}{}）",
                            file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                            hit.line,
                            if *dy < 0.0 { "，向上探测" } else { "，向下探测" }
                        ))
                    };
                    return Ok(InverseResultDto {
                        source: Some(SourcePositionDto {
                            file: file.to_string_lossy().into_owned(),
                            line: hit.line,
                            column: hit.column,
                        }),
                        note,
                    });
                }
                other => {
                    // 记录**第一次**命中的非源码目标（原点那次最有信息量，用于提示文案）
                    if first_target.is_none() {
                        first_target = Some(other);
                    }
                }
            },
            Err(e) => {
                if last_err.is_none() {
                    last_err = Some(e.to_string());
                }
            }
        }
    }

    // 全失败（多半是"还没编译过"或同步数据被占用）
    if first_target.is_none() {
        if let Some(err) = last_err {
            return Ok(InverseResultDto {
                source: None,
                note: Some(sync_unavailable_note(&state, &project, &pdf, &err).await),
            });
        }
    }

    // 拿到了非源码目标但附近没有源码：忽略跳转，只给提示
    let note = match first_target {
        Some(InverseTarget::Generated(f)) => Some(format!(
            "此处来自自动生成的文件 {}（目录/参考文献/索引等由 LaTeX 生成），没有对应的源码行",
            f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        )),
        Some(InverseTarget::OutsideProject(f)) => Some(format!(
            "此处来自项目外的文件 {}（系统宏包/文档类），无法在编辑器里打开",
            f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        )),
        _ => Some("此处没有对应的源码位置".to_string()),
    };
    Ok(InverseResultDto { source: None, note })
}

/// 同步不可用时的提示：结合文件系统状态说清"是没编译过还是别的原因"（不猜）。
async fn sync_unavailable_note(
    state: &AppState,
    project: &ProjectState,
    pdf: &Path,
    err: &str,
) -> String {
    let stem = pdf.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let synctex = project.root.join("tmp").join(format!("{stem}.synctex.gz"));
    if !state.fs.exists(pdf).await.unwrap_or(false) {
        return "同步失败：还没有编译产物，先点「编译」".to_string();
    }
    if !state.fs.exists(&synctex).await.unwrap_or(false) {
        return "同步失败：没有找到 .synctex.gz（编译时未生成同步数据），重新编译一次即可".to_string();
    }
    format!("同步失败：{err}")
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
