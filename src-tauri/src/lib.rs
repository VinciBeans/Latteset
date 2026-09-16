// Latteset 应用壳（ADR-0006：仅接线，业务逻辑在 latteset-core）。

mod commands;
mod events;
mod runner_switch;

use commands::AppState;
use events::{
    build_emitter, CompileErrorsEvent, CompilePreviewEvent, CompileProgressEvent,
    CompileStatusEvent, ErrorsUpdatedEvent, FilesChangedEvent, PdfUpdatedEvent,
    SettingsChangedEvent, TauriProgress, TauriSink,
};
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
            // 共享状态（设置要在 runner 之前建：`SwitchableRunner` 每趟读它选形态）
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

            // 装配点（方案 §3.2 / §4.1「装配点单一」）：装一个**可切换** runner ——
            // 它每趟编译读一次 `settings.tectonic`（形态 / bundle / 缓存目录），
            // **改设置即生效、不需要重启**（方案 §3.5 的设置面收口）。
            // **D1：失败不静默回退** —— 选了库形态而没编进特性时显式报错（见 runner_switch）。
            let runner: Arc<dyn latteset_core::scheduler::CompileRunner> =
                Arc::new(runner_switch::SwitchableRunner::new(
                    fs.clone(),
                    progress,
                    settings.clone(),
                    app.path().app_cache_dir().ok(),
                ));
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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
            commands::get_outline,
            commands::compile_now,
            commands::abort_compile,
            commands::synctex_forward,
            commands::synctex_inverse,
            commands::get_settings,
            commands::update_settings,
            commands::lib_form_available,
            commands::engine_form,
            commands::set_window_theme,
        ])
        .events(tauri_specta::collect_events![
            CompileStatusEvent,
            CompileProgressEvent,
            CompileErrorsEvent,
            // 编译中的部分 PDF（roadmap ㉞）：与 pdf-updated 分开，中间态不顶权威终态
            CompilePreviewEvent,
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
