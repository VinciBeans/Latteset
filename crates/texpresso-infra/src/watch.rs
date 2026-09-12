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
use texpresso_core::compose::{compile_request_for_change, ComposeContext};
use texpresso_core::project::{is_ignored, is_tree_excluded, ProjectState};
use texpresso_core::scheduler::SchedulerHandle;
use texpresso_core::settings::Settings;
use texpresso_core::types::FilesChanged;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, warn};

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
    /// 项目覆盖（update_settings 的 root_file 写这里）。
    pub overrides: Arc<RwLock<texpresso_core::settings::ProjectOverrides>>,
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
        // 2. .tex（排除 tmp/、隐藏项）→ 编译触发 + 广播
        let Some(root) = project_root else {
            debug!("无项目根，跳过: {}", path.display());
            continue;
        };
        if is_ignored(&path, root) {
            debug!("忽略路径: {}", path.display());
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("tex") {
            debug!("触发编译: {}", path.display());
            trigger_compile(&path, state.clone());
        }
        // 3. 其余（含非 .tex、目录变化）→ 文件树刷新广播
        if !is_tree_excluded(&path, root) {
            broadcast_files_changed(&[path], &state, is_structural_event(event));
        }
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
fn handle_settings_change(path: &Path, state: Arc<WatchState>) {
    let path = path.to_path_buf();
    // 先取出句柄：async move 会整体接管 state，直接 state.rt.spawn(...) 会同时借用又移动
    let rt = state.rt.clone();
    rt.spawn(async move {
        let is_global = path == state.storage.global_path();
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(_) => return,
        };
        if state.storage.is_self_write(&path, &content) {
            debug!("设置自写盘，跳过重载：{}", path.display());
            return;
        }
        if is_global {
            if let Ok(parsed) = serde_json::from_str::<Settings>(&content) {
                let overrides = state.overrides.read().await.clone();
                *state.settings.write().await = SettingsStorage::effective(&parsed, &overrides);
            } else {
                warn!("全局设置解析失败，忽略外部修改：{}", path.display());
                return;
            }
        } else {
            if let Ok(parsed) = serde_json::from_str::<texpresso_core::settings::ProjectOverrides>(&content) {
                *state.overrides.write().await = parsed;
                let merged = SettingsStorage::effective(&state.settings.read().await.clone(), &state.overrides.read().await.clone());
                *state.settings.write().await = merged;
            } else {
                warn!("项目设置解析失败，忽略外部修改：{}", path.display());
                return;
            }
        }
        let s = state.settings.read().await.clone();
        state.sink.settings_changed(s);
    });
}

/// 组合层翻译（D3）：.tex 变化 → CompileRequest → 调度器。
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
            None => warn!("变化不触发编译：{}（root_file={:?}）", path.display(), project.root_file),
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
