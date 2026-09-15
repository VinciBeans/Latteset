//! 文件监视与触发组合（modules.md §7）。
//!
//! - notify 事件 → 规范化 → 分类（.tex / settings.json / 其余）；
//! - .tex 变化 → 组合层翻译成 CompileRequest → 调度器（设计决策 D3）；
//! - 旁路广播 files-changed（前端文件树防抖重建）；
//! - **不依赖 Tauri**：结果经 [`WatchSink`] 回调，由 src-tauri 决定发什么事件（ADR-0010）。

use crate::fs::strip_verbatim;
use crate::storage::SettingsStorage;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use latteset_core::compose::{compile_request_for_change, ComposeContext};
use latteset_core::project::{is_tree_excluded, ProjectState};
use latteset_core::scheduler::SchedulerHandle;
use latteset_core::settings::Settings;
use latteset_core::types::FilesChanged;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// 监视任务命令（模块间通信只经此通道，信息局部性）。
pub enum WatchCommand {
    /// 切换项目根（打开项目时调用；None = 关闭）。
    SetProjectRoot(Option<PathBuf>),
}

/// 外部唯一入口：只暴露"切换项目根"意图。
#[derive(Clone)]
pub struct WatchHandle {
    tx: mpsc::UnboundedSender<WatchCommand>,
}

impl WatchHandle {
    pub fn set_project_root(&self, root: Option<PathBuf>) {
        let _ = self.tx.send(WatchCommand::SetProjectRoot(root));
    }
}

/// 监视结果出口：基础设施层不知道 Tauri 事件的存在，由上层实现本 trait 转译。
pub trait WatchSink: Send + Sync + 'static {
    /// 文件变化旁路（文件树刷新 / 外部修改判定；structural = 增/删/重命名）。
    fn files_changed(&self, payload: FilesChanged);
    /// 设置热更新（外部编辑 settings.json 后重算的有效设置）。
    fn settings_changed(&self, settings: Settings);
}

pub struct WatchState {
    pub project: Arc<RwLock<Option<ProjectState>>>,
    pub settings: Arc<RwLock<Settings>>,
    pub scheduler: SchedulerHandle,
    pub storage: Arc<SettingsStorage>,
    /// 文件系统（读盘一律经它；roadmap ㉑ 的 root_file 重解析要用）。
    pub fs: Arc<dyn latteset_core::project::FileSystem>,
    /// 项目覆盖（update_settings 的 root_file 写这里）。
    pub overrides: Arc<RwLock<latteset_core::settings::ProjectOverrides>>,
    /// 结果出口（由上层注入）。
    pub sink: Arc<dyn WatchSink>,
    /// 事件处理用的异步运行时句柄：watch 线程是普通 std 线程，没有 runtime 上下文。
    pub rt: tokio::runtime::Handle,
}

/// 启动监视任务：维护 notify watcher，把事件路由到分类处理。
pub fn spawn_watcher(
    global_settings_dir: PathBuf,
    state: Arc<WatchState>,
) -> WatchHandle {
    let (tx, mut rx) = mpsc::unbounded_channel::<WatchCommand>();

    std::thread::spawn(move || {
        use notify::{RecommendedWatcher, RecursiveMode, Watcher};

        debug!("watch 线程启动");
        let (event_tx, event_rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
        let mut watcher: RecommendedWatcher =
            match notify::recommended_watcher(move |res| {
                let _ = event_tx.send(res);
            }) {
                Ok(w) => w,
                Err(e) => {
                    error!("notify 初始化失败，文件监视不可用：{e}");
                    return;
                }
            };
        debug!("notify watcher 初始化成功");

        // 固定监视全局设置目录（热更新，modules.md §6）。
        // 目录不存在时 notify 会直接失败（首次启动还没写过设置），由本层确保其存在——
        // 这是基础设施的职责，上层不必碰文件系统（ADR-0010）。
        if let Err(e) = std::fs::create_dir_all(&global_settings_dir) {
            warn!("创建设置目录失败（{}）：{e}", global_settings_dir.display());
        }
        if let Err(e) = watcher.watch(&global_settings_dir, RecursiveMode::NonRecursive) {
            warn!("监视全局设置目录失败：{e}");
        }

        let mut project_root: Option<PathBuf> = None;

        loop {
            // 非阻塞消费命令
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    WatchCommand::SetProjectRoot(Some(root)) => {
                        debug!("watch 命令：切换项目根 → {}", root.display());
                        if let Some(old) = &project_root {
                            let _ = watcher.unwatch(old);
                        }
                        match watcher.watch(&root, RecursiveMode::Recursive) {
                            Ok(()) => {
                                project_root = Some(root);
                                debug!("开始监视项目：{}", project_root.as_ref().unwrap().display());
                            }
                            Err(e) => warn!("监视项目失败（{}）：{e}", root.display()),
                        }
                    }
                    WatchCommand::SetProjectRoot(None) => {
                        if let Some(old) = project_root.take() {
                            let _ = watcher.unwatch(&old);
                        }
                    }
                }
            }

            // 阻塞等事件（带超时：定期醒来处理命令通道——
            // 修复：此前 recv() 无限阻塞，SetProjectRoot 命令永远得不到处理，
            // 项目根从未注册，watch 链路静默失效）
            match event_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(Ok(event)) => handle_event(&event, state.clone(), project_root.as_deref()),
                Ok(Err(e)) => warn!("notify 事件错误：{e}"),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break, // 通道关闭
            }
        }
    });

    WatchHandle { tx }
}

/// 事件分类与路由（modules.md §7 过滤规则）。
/// Access（打开/读取）事件不触发编译与树刷新——只有内容变化类事件才处理。
fn should_process(event: &notify::Event) -> bool {
    !matches!(event.kind, notify::EventKind::Access(_))
}

/// 是否结构变化（增/删/重命名）——文件树需重建；内容修改（自动保存等）为 false。
fn is_structural_event(event: &notify::Event) -> bool {
    matches!(
        event.kind,
        notify::EventKind::Create(_)
            | notify::EventKind::Remove(_)
            | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
    )
}

fn handle_event(event: &notify::Event, state: Arc<WatchState>, project_root: Option<&Path>) {
    debug!("watch 原始事件: {:?} kind={:?}", event.paths, event.kind);
    if !should_process(event) {
        debug!("Access 事件跳过（不触发编译）");
        return;
    }
    let paths = normalize_event_paths(event);
    for raw in paths {
        // Windows：notify 事件路径可能带 \\?\ 前缀（取决于 watch 根形式），统一剥离，
        // 与 state.project.root（已剥离）保持一致，否则 starts_with 校验永远失败。
        let path = strip_verbatim(&raw);
        // 1. 设置文件（全局或项目）→ 热更新
        if is_settings_path(&path, &state, project_root) {
            handle_settings_change(&path, state.clone());
            continue;
        }
        // 2. 项目内任何**非忽略**路径（tmp/、隐藏项除外）→ 尝试触发编译 + 广播。
        //
        // roadmap ㉜：**不再只认 `.tex`** —— `.bib`/图片/`.cls`/`.sty` 都是 latexmk 的输入
        // （实测：改被 `\bibliography` 引用的 `refs.bib` 时全部 tmp 产物 mtime 纹丝不动 =
        // 用户"改了没反应"）。"到底算不算输入"的最终判定在 core
        // （`compose::compile_request_for_change` → `is_compile_trigger`）：那里拿得到根文件名，
        // 也才排除得了**本次编译自己写在项目根的产物**（根 `<stem>.pdf`，不排除会自激）。
        // 本层是同步的 notify 回调线程、读不到项目状态，所以只做 tmp/ 与隐藏项这道粗过滤。
        let Some(root) = project_root else {
            debug!("无项目根，跳过: {}", path.display());
            continue;
        };
        if is_tree_excluded(&path, root) {
            debug!("忽略路径: {}", path.display());
            continue;
        }
        debug!("触发编译: {}", path.display());
        trigger_compile(&path, state.clone());
        // 3. 文件树刷新广播（内容修改 structural=false ⇒ 不重建树）
        broadcast_files_changed(&[path], &state, is_structural_event(event));
    }
}

/// rename 事件同时产出 from/to；这里都视为"变化"。
fn normalize_event_paths(event: &notify::Event) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let notify::EventKind::Modify(notify::event::ModifyKind::Name(
        notify::event::RenameMode::Both,
    )) = event.kind
    {
        // from/to 成对出现，取 to（新路径）
        if let Some(to) = event.paths.last() {
            out.push(to.clone());
        }
        return out;
    }
    out.extend(event.paths.iter().cloned());
    out
}

fn is_settings_path(path: &Path, state: &WatchState, project_root: Option<&Path>) -> bool {
    let file_name = path.file_name().and_then(|n| n.to_str());
    if file_name != Some("settings.json") {
        return false;
    }
    if path == state.storage.global_path() {
        return true;
    }
    if let Some(root) = project_root {
        if path == crate::storage::project_overrides_path(root) {
            return true;
        }
    }
    false
}

/// 设置热更新（D6）：自写盘 hash 过滤 → 重载 → 广播 settings-changed。
///
/// **非原子写的竞态（2026-09 实测，roadmap ㉑）**：外部工具（PowerShell `Set-Content`、记事本、
/// 许多编辑器）是"截断 → 写入 → 关闭"，watcher 会收到**两个** Modify 事件，而中间存在一个
/// 文件为空/被占用的窗口：第一次读拿到空内容（解析失败），第二次读可能撞上共享冲突
/// （`read_to_string` 失败）——旧实现在这两种情况下都只是"忽略这次修改"，于是用户看到
/// 「手改了 settings.json 但应用没反应」。现在改为**短重试**（3 次 × 150ms），并在最终失败时
/// 打警告（不再静默）。
fn handle_settings_change(path: &Path, state: Arc<WatchState>) {
    let path = path.to_path_buf();
    // 先取出句柄：async move 会整体接管 state，直接 state.rt.spawn(...) 会同时借用又移动
    let rt = state.rt.clone();
    rt.spawn(async move {
        let is_global = path == state.storage.global_path();
        const ATTEMPTS: u32 = 3;
        let mut last_problem = String::from("读取失败");
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                // 给写者一点时间完成"截断 → 写入 → 关闭"
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
            let content = match tokio::fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(e) => {
                    last_problem = format!("读取失败：{e}");
                    continue;
                }
            };
            if state.storage.is_self_write(&path, &content) {
                debug!("设置自写盘，跳过重载：{}", path.display());
                return;
            }
            if is_global {
                match serde_json::from_str::<Settings>(&content) {
                    Ok(parsed) => {
                        let overrides = state.overrides.read().await.clone();
                        *state.settings.write().await = SettingsStorage::effective(&parsed, &overrides);
                    }
                    Err(e) => {
                        last_problem = format!("解析失败：{e}");
                        continue;
                    }
                }
            } else {
                match serde_json::from_str::<latteset_core::settings::ProjectOverrides>(&content) {
                    Ok(parsed) => {
                        *state.overrides.write().await = parsed;
                        // **必须与磁盘上的纯全局合并**（不是与 state.settings）：
                        // state.settings 是"全局 + 覆盖"的有效值，拿它当基数会让**清掉**覆盖失效
                        // ——例如外部把 root_file 从 Some 改成"没有这一项"时，旧值会一直粘着
                        // （2026-09 实测：清覆盖后 chip 不回来）。命令面（open_project/update_settings）
                        // 一直读纯全局，这里与它对齐。
                        let global = state.storage.load_global(state.fs.as_ref()).await;
                        let overrides = state.overrides.read().await.clone();
                        *state.settings.write().await = SettingsStorage::effective(&global, &overrides);
                    }
                    Err(e) => {
                        last_problem = format!("解析失败：{e}");
                        continue;
                    }
                }
            }
            // 成功读到并应用
            let s = state.settings.read().await.clone();
            sync_project_root_file(&state).await;
            state.sink.settings_changed(s);
            return;
        }
        warn!(
            path = %path.display(),
            attempts = ATTEMPTS,
            "设置热更新放弃（{last_problem}）——若是编辑器正在写入，保存完成后会再触发一次"
        );
    });
}

/// 把（外部改过的）设置里的 `root_file` 同步进内存 `ProjectState`（roadmap ㉑）。
///
/// 为什么需要：`update_settings` 会同步内存（②-1 修过），但**外部直接编辑**
/// `.latteset/settings.json` / 全局 `settings.json` 走的是这条热更新路径——此前只重算了
/// `state.settings`，`ProjectState.root_file` 仍是旧值，症状是「手改配置设了根文件，编译仍报
/// 『未确定根文件』，必须重开项目」。
///
/// 语义与 `update_settings` 对齐：
/// - 设了覆盖 → 按 D8（canonicalize + 根内校验）解析成绝对路径后写入；解析失败只警告**不落盘**，
///   保持原值（避免把内存状态改成必然失败的路径）；
/// - 清掉覆盖（`null`）→ 回到自动探测（唯一候选即根文件，多候选/无候选则为 None，由前端再选）。
async fn sync_project_root_file(state: &Arc<WatchState>) {
    let Some(project) = state.project.read().await.clone() else { return };
    let settings = state.settings.read().await.clone();
    let current = project.root_file.clone();

    let resolved = match settings.root_file.clone() {
        Some(rel) => {
            let joined = project.root.join(&rel);
            match latteset_core::project::resolve_in_project(state.fs.as_ref(), &project.root, &joined).await {
                Ok(abs) => Some(abs),
                Err(e) => {
                    warn!(path = %rel.display(), "外部设置里的 root_file 不可用，保持原根文件：{e:?}");
                    return;
                }
            }
        }
        None => match latteset_core::project::detect_root(state.fs.as_ref(), &project.root).await {
            Ok(resolution) => resolution.unique(),
            Err(e) => {
                warn!("清掉 root_file 覆盖后重新探测失败：{e}");
                return;
            }
        },
    };

    if resolved == current {
        return; // 无变化（应用自己写盘也会走到这里）→ 不动内存、不打日志
    }
    info!(
        from = ?current.as_ref().map(|p| p.display().to_string()),
        to = ?resolved.as_ref().map(|p| p.display().to_string()),
        "设置热更新：内存 root_file 已同步"
    );
    if let Some(p) = state.project.write().await.as_mut() {
        p.root_file = resolved;
    }
}

/// 组合层翻译（D3）：变化路径 → CompileRequest → 调度器。
///
/// 路径**不限于 `.tex`**（roadmap ㉜）；是否真的触发由 core 判定，理由见 `handle_event` 第 2 步。
fn trigger_compile(path: &Path, state: Arc<WatchState>) {
    let path = path.to_path_buf();
    let rt = state.rt.clone();
    rt.spawn(async move {
        let project = state.project.read().await.clone();
        let Some(project) = project else { return };
        let settings = state.settings.read().await.clone();
        let ctx = ComposeContext {
            project: &project,
            settings: &settings,
        };
        match compile_request_for_change(ctx, &path) {
            Some(req) => {
                debug!("构造编译请求: root={} engine={:?}", req.root_file.display(), req.engine);
                state.scheduler.compile(req);
            }
            // 不触发是**常态**（本次编译自己的 `<stem>.pdf`、`notes.md` 这类非输入）⇒
            // 只到 debug；原因由 core 的 `compile_request_for_change` 逐条打出来（㉛）。
            None => debug!("变化不触发编译：{}（root_file={:?}）", path.display(), project.root_file),
        }
    });
}

fn broadcast_files_changed(paths: &[PathBuf], state: &WatchState, structural: bool) {
    let paths: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    state.sink.files_changed(FilesChanged { paths, structural });
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, DataChange, ModifyKind, RemoveKind, RenameMode};
    use notify::EventKind;

    fn ev(kind: EventKind, paths: &[&str]) -> notify::Event {
        let mut e = notify::Event::new(kind);
        for p in paths {
            e = e.add_path(Path::new(p).to_path_buf());
        }
        e
    }

    #[test]
    fn should_process_skips_access() {
        assert!(!should_process(&ev(EventKind::Access(AccessKind::Read), &[])));
        assert!(should_process(&ev(EventKind::Create(CreateKind::File), &["a.tex"])));
    }

    #[test]
    fn structural_flag_matches_kind() {
        // 增/删/重命名 → true（文件树重建）；内容修改 → false（跳过重建）
        assert!(is_structural_event(&ev(EventKind::Create(CreateKind::File), &["a"])));
        assert!(is_structural_event(&ev(EventKind::Remove(RemoveKind::File), &["a"])));
        assert!(is_structural_event(&ev(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            &["a"]
        )));
        assert!(!is_structural_event(&ev(
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            &["a"]
        )));
    }

    #[test]
    fn normalize_rename_both_takes_to_path() {
        let e = ev(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            &["from.tex", "to.tex"],
        );
        assert_eq!(normalize_event_paths(&e), vec![PathBuf::from("to.tex")]);
    }

    #[test]
    fn normalize_keeps_all_paths_for_other_kinds() {
        let e = ev(EventKind::Create(CreateKind::File), &["a.tex", "b.tex"]);
        assert_eq!(
            normalize_event_paths(&e),
            vec![PathBuf::from("a.tex"), PathBuf::from("b.tex")]
        );
    }
}
