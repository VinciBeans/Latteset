// Latteset 应用壳（ADR-0006：仅接线，业务逻辑在 latteset-core）。

mod commands;
mod events;
mod runner;

use commands::AppState;
use events::{
    build_emitter, CompileErrorsEvent, CompileNoticeEvent, CompilePreviewEvent, CompileProgressEvent,
    CompileStatusEvent, ErrorsUpdatedEvent, FilesChangedEvent, PdfUpdatedEvent,
    SettingsChangedEvent, TauriProgress, TauriSink,
};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use latteset_core::scheduler::Scheduler;
use latteset_core::settings::Settings;
use latteset_infra::{
    fs::TokioFs, storage::SettingsStorage,
    watch::{spawn_watcher, WatchState},
};
use tokio::sync::RwLock;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // T18 启动分段测量的起点：进程进入 run() 的第一行（后续分段见 setup / RunEvent::Ready）。
    let startup_t0 = std::time::Instant::now();
    let builder = build_specta_builder();

    // 生成 TS 绑定（调试构建每次启动时刷新 src/bindings.ts）
    #[cfg(debug_assertions)]
    {
        use specta_typescript::Typescript;
        builder
            .export(Typescript::default(), "../src/bindings.ts")
            .expect("导出 TypeScript 绑定失败");
    }

    // MCP Bridge 插件（仅调试构建）：供 tauri server MCP（driver_session / webview_* / ipc_* / read_logs）
    // 连接到本应用。仅 debug build 注册，不进入生产（插件自身也是 debug-only）。
    #[cfg(debug_assertions)]
    let app_builder = tauri::Builder::default().plugin(tauri_plugin_mcp_bridge::init());
    #[cfg(not(debug_assertions))]
    let app_builder = tauri::Builder::default();

    app_builder
        .plugin(tauri_plugin_log::Builder::new().skip_logger().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            // T18 启动分段：setup 段起点（setup 内所有装配工作）。
            let setup_t0 = std::time::Instant::now();
            builder.mount_events(app);

            // 日志：tracing（结构化）+ plugin-log 仅作通道（skip_logger）
            #[cfg(debug_assertions)]
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::DEBUG)
                .with_target(false)
                .init();
            #[cfg(not(debug_assertions))]
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .with_target(false)
                .init();

            // ---- 基础设施装配（ADR-0010：具体实现在 latteset-infra，本层只注入 trait 位）----
            let fs: Arc<dyn latteset_core::project::FileSystem> = Arc::new(TokioFs);
            // SyncTeX：**默认自解析**（`.synctex.gz` 自己解），不依赖系统 `synctex` 二进制 ——
            // 那个二进制来自 TeX Live，而 ⑫ 的业务前提是"干净 Windows 机器零预装可用"。
            // `LATTESET_SYNCTEX=cli` 可切回系统二进制（A/B 复核用，见 infra::synctex::default_provider）。
            let sync: Arc<dyn latteset_core::synctex::SyncTexProvider> =
                latteset_infra::synctex::default_provider();

            // 全局设置目录（app_config_dir）
            let config_dir = app
                .path()
                .app_config_dir()
                .expect("无法解析应用配置目录");
            // 目录创建不在这里做：监视/写入都在基础设施层，谁用谁负责（ADR-0010）
            let global_settings_path = config_dir.join("settings.json");
            let storage = Arc::new(SettingsStorage::new(global_settings_path));

            // 调度器（D1：actor，状态收容 task 内；emitter 接 tauri 事件）
            let emitter = build_emitter(app.handle());
            // 编译进行中的流式反馈（阶段 2）：页进度 + 编译中的错误，直接走 tauri 事件
            let progress: Arc<dyn latteset_core::scheduler::CompileProgress> =
                Arc::new(TauriProgress {
                    app: app.handle().clone(),
                });
            // 共享状态（设置要在 runner 之前建：薄壳每趟读它决定 bundle/cache）
            let project: Arc<RwLock<Option<latteset_core::project::ProjectState>>> =
                Arc::new(RwLock::new(None));
            let settings: Arc<RwLock<Settings>> = Arc::new(RwLock::new(
                tauri::async_runtime::block_on(storage.load_global(fs.as_ref())),
            ));
            // **标题栏定色赶在窗口画出来之前**：前端要等 WebView 起来 + 一次 IPC 往返才知道主题，
            // 只靠那条路，深色用户每次启动会先看到一条白标题栏（实测）。这里直接读磁盘设置，
            // `system` 交给系统跟（`set_theme(None)`）。
            {
                let theme = settings
                    .try_read()
                    .map(|s| s.ui.theme)
                    .unwrap_or_default();
                if let Some(w) = app.get_webview_window("main") {
                    commands::apply_window_theme(&w, theme);
                }
            }
            let overrides: Arc<RwLock<latteset_core::settings::ProjectOverrides>> =
                Arc::new(RwLock::new(Default::default()));

            // 装配点（阶段② D1：「薄壳 + 每趟读设置」）：只装一个内嵌 runner。
            // 它**每趟编译**读一次 `settings.tectonic` 的 bundle/cache 重建 `TectonicLibRunner`
            // ⇒ 资源设置改完即生效、不需要重启；形态唯一（内嵌 Tectonic，ADR-0015 决策 2），
            // 没有形态分支、没有子进程回退。
            //
            // bundle 默认值（ADR-0016 候选 A）：**随安装包分发的内置精简集**优先（离线可用）；
            // 缺失/损坏时显式报错 + "修复安装 / 显式切在线"，**不静默改用网络**（决策 2）。
            let shipped_bundle = commands::shipped_bundle_path(app.handle());
            if let Some(path) = &shipped_bundle {
                if !path.exists() {
                    tracing::error!(
                        path = %path.display(),
                        "内置精简宏包集缺失：安装包应带 tectonic-bundle.zip；不会回退网络（ADR-0016 决策 2）"
                    );
                }
            }
            // ---- 资源解析（T22 B3：runner / 缓存 GC / 首启预热共用同一口径）----
            // cache：显式设置 > `LATTESET_TECTONIC_CACHE` > 宿主 app_cache_dir；
            // bundle：显式设置 > 环境变量 > 随包内置精简集 > 在线兜底。
            // 三处各写一遍曾导致 GUI 的 GC 用了 `app_cache_dir()` 而 runner/预热用设置值
            // ⇒ 用户自定义 cache_dir 时 GC 清不到（B3 缺陷）；统一到壳里的这一次解析。
            let (resource_cache, resource_bundle) = {
                let (explicit_cache, explicit_bundle) = settings
                    .try_read()
                    .ok()
                    .map(|s| (s.tectonic.cache_dir.clone(), s.tectonic.bundle.clone()))
                    .unwrap_or((None, None));
                let cache = latteset_tectonic::resolve_cache_dir(
                    explicit_cache,
                    latteset_tectonic::cache_dir_from_env(),
                    app.path().app_cache_dir().ok(),
                );
                let explicit = explicit_bundle
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(|v| latteset_tectonic::BundleSource::Explicit(v.to_owned()));
                let env = std::env::var_os(latteset_tectonic::BUNDLE_ENV)
                    .map(|_| latteset_tectonic::bundle_from_env());
                let bundle =
                    latteset_tectonic::resolve_source(explicit, env, shipped_bundle.as_deref());
                (cache, bundle)
            };
            let runner = commands::build_compile_runner(
                fs.clone(),
                progress,
                settings.clone(),
                app.path().app_cache_dir().ok(),
                shipped_bundle.clone(),
            );
            // T16 / T22 B3 缓存 GC：启动时（此时还没有编译在跑）按"当前 digest + 最近 N"清掉旧
            // format/data/hash。**当前 digest 由同一 bundle 解析器给出**（随包/显式/在线各自的
            // digest；在线只读缓存、零网络）；解析不出 ⇒ `gc_cache` 整体不动（宁可不清理，也不猜）。
            // 上限可配：LATTESET_CACHE_KEEP（默认 3，见 latteset-tectonic::cache_keep_from_env）。
            if let Some(cache) = resource_cache.clone() {
                let keep = latteset_tectonic::cache_keep_from_env();
                let bundle = resource_bundle.clone();
                std::thread::spawn(move || {
                    let current = latteset_tectonic::current_bundle_digest(&bundle, &cache);
                    if current.is_none() {
                        tracing::debug!("缓存 GC：解析不出当前 bundle digest ⇒ 本次不动缓存（保守）");
                    }
                    let report = latteset_tectonic::gc_cache(&cache, keep, current.as_deref());
                    if report.deleted_formats + report.deleted_data + report.deleted_hashes > 0 {
                        tracing::info!(
                            deleted_formats = report.deleted_formats,
                            deleted_data = report.deleted_data,
                            deleted_hashes = report.deleted_hashes,
                            freed_bytes = report.freed_bytes,
                            kept = ?report.kept_digests,
                            current_digest = ?current,
                            "缓存 GC 完成（保留当前 digest + 最近 N；LATTESET_CACHE_KEEP）"
                        );
                    }
                });
            } else {
                tracing::debug!("没有缓存目录 ⇒ 跳过缓存 GC");
            }
            // setup 闭包不是 tokio 上下文：用 tauri 的 runtime（任何线程可用）
            let (scheduler, scheduler_task) = Scheduler::create(runner, emitter);
            tauri::async_runtime::spawn(scheduler_task.run());

            // 监视任务（事件出口与运行时句柄由本层注入，infra 不认识 Tauri）
            let watch_state = Arc::new(WatchState {
                project: project.clone(),
                settings: settings.clone(),
                scheduler: scheduler.clone(),
                storage: storage.clone(),
                // 外部改 settings.json 后要重解析 root_file（roadmap ㉑）→ 需要读盘能力
                fs: fs.clone(),
                overrides: overrides.clone(),
                sink: Arc::new(TauriSink {
                    app: app.handle().clone(),
                }),
                rt: tauri::async_runtime::handle().inner().clone(),
            });
            let watch_handle = spawn_watcher(config_dir, watch_state);
            tracing::info!(
                startup_ms = startup_t0.elapsed().as_millis() as u64,
                watch_ms = setup_t0.elapsed().as_millis() as u64,
                "startup 分段：watcher 已启动"
            );

            // ---- T18 首启 format 预热（产品决定 2026-11-20：`.fmt` 不随包，首启后台生成）----
            //
            // 目的：把 ~1s 的冷缓存首编等待藏到"用户打开项目/打字"期间。策略与 runner 的 bundle
            // 解析同源（设置 > 环境 > 内置精简集 > 在线兜底）；**只在本地 bundle 可开、或在线
            // bundle 的缓存已有 digest 时**才预热（启动期不联网）。失败静默、不影响启动与首编；
            // 与用户首编的互斥在 `latteset-tectonic::runner` 的预热闸门里（`wait_for_prewarm`）。
            //
            // 日志分段：`进程 → setup 完成 → webview Ready`（setup/Ready 两处 info）；
            // `open_project / 首帧可编辑 / 首编完成` 属前端时间点，由 GUI 验收（T19）测，不在此伪造。
            tracing::info!(
                startup_ms = startup_t0.elapsed().as_millis() as u64,
                setup_ms = setup_t0.elapsed().as_millis() as u64,
                "startup 分段：setup 完成"
            );
            // 预热输入必须在 `app.manage` 之前算好（settings 的 Arc 随后被移交给 AppState）；
            // 与缓存 GC 共用同一份资源解析结果（T22 B3）。
            let prewarm_inputs: Option<(latteset_tectonic::BundleSource, PathBuf)> =
                resource_cache.clone().map(|cache| (resource_bundle.clone(), cache));

            app.manage(AppState {
                fs,
                scheduler,
                sync,
                project,
                settings,
                overrides,
                storage,
                watch: watch_handle,
                app: app.handle().clone(),
                outline: tokio::sync::Mutex::new(latteset_core::outline::OutlineCache::new()),
            });

            match prewarm_inputs {
                Some((bundle, cache)) => {
                    let prewarm_started = std::time::Instant::now();
                    tauri::async_runtime::spawn_blocking(move || {
                        let outcome = latteset_tectonic::runner::prewarm_format(&bundle, &cache);
                        let prewarm_ms = prewarm_started.elapsed().as_millis() as u64;
                        match outcome {
                            latteset_tectonic::runner::PrewarmOutcome::Generated => {
                                tracing::info!(prewarm_ms, "startup 分段：首启 format 预热完成")
                            }
                            latteset_tectonic::runner::PrewarmOutcome::AlreadyCached => {
                                tracing::debug!(prewarm_ms, "startup 分段：format 预热命中已有缓存")
                            }
                            latteset_tectonic::runner::PrewarmOutcome::SkippedNoBundle => {
                                tracing::debug!(
                                    prewarm_ms,
                                    "startup 分段：没有可用的本地/已缓存 bundle，跳过预热（不联网）"
                                )
                            }
                            latteset_tectonic::runner::PrewarmOutcome::Failed(msg) => {
                                tracing::warn!(
                                    prewarm_ms,
                                    %msg,
                                    "startup 分段：format 预热失败（静默：首编会自行生成）"
                                )
                            }
                        }
                    });
                }
                None => tracing::debug!("startup 分段：没有缓存目录，跳过 format 预热"),
            }
            tracing::info!(
                startup_ms = startup_t0.elapsed().as_millis() as u64,
                "startup 分段：setup 末尾（后台预热已派发）"
            );
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |_app_handle, event| {
            // webview 就绪：进程启动后的关键可见节点（之后才是前端首帧可编辑/首编）。
            if matches!(event, tauri::RunEvent::Ready) {
                tracing::info!(
                    startup_ms = startup_t0.elapsed().as_millis() as u64,
                    "startup 分段：webview Ready（进程→Ready）"
                );
            }
        });
}

/// specta builder：命令 + 事件（modules.md §4 契约的单一事实来源）。
fn build_specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            commands::open_project,
            commands::get_project,
            commands::list_dir,
            commands::read_file,
            commands::save_all,
            commands::create_dir,
            commands::delete_path,
            commands::rename_path,
            commands::new_file_skeleton,
            commands::get_outline,
            commands::compile_now,
            commands::abort_compile,
            commands::synctex_forward,
            commands::synctex_inverse,
            commands::get_settings,
            commands::update_settings,
            commands::set_window_theme,
            commands::compile_math,
            commands::open_licenses,
        ])
        .events(tauri_specta::collect_events![
            CompileStatusEvent,
            CompileProgressEvent,
            CompileErrorsEvent,
            // 编译中的部分 PDF（roadmap ㉞）：与 pdf-updated 分开，中间态不顶权威终态
            CompilePreviewEvent,
            // 能力边界提示（ADR-0015 决策 4）：biber 是外部程序（内嵌引擎不运行外部工具 ⇒ 不会调用
            // biber、引用可能未解析、不生成 `.bbl`；backend=bibtex 走内建 BibtexEngine 不受影响），
            // makeindex/glossaries 不会被自动运行 —— 检出即显式说明，不静默降级
            CompileNoticeEvent,
            ErrorsUpdatedEvent,
            PdfUpdatedEvent,
            FilesChangedEvent,
            SettingsChangedEvent,
        ])
}

#[cfg(test)]
mod tests {
    /// 手动运行导出前端绑定：`cargo test -p latteset -- --ignored export_bindings`
    #[test]
    #[ignore]
    fn export_bindings() {
        use specta_typescript::Typescript;
        super::build_specta_builder()
            .export(Typescript::default(), "../src/bindings.ts")
            .expect("导出 TypeScript 绑定失败");
    }
}
