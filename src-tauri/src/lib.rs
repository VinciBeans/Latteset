// TeXPresso 应用壳（ADR-0006：仅接线，业务逻辑在 texpresso-core）。

mod commands;
mod events;
mod fs_impl;
mod runner;
mod storage;
mod sync_cli;
mod watch;

use commands::AppState;
use events::{build_emitter, CompileStatusEvent, ErrorsUpdatedEvent, FilesChangedEvent, PdfUpdatedEvent, SettingsChangedEvent};
use std::sync::Arc;
use tauri::Manager;
use texpresso_core::scheduler::Scheduler;
use texpresso_core::settings::Settings;
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

            // ---- 基础设施装配（§1 全局状态清单）----
            let fs: Arc<dyn texpresso_core::project::FileSystem> = Arc::new(fs_impl::TokioFs);
            let sync: Arc<dyn texpresso_core::synctex::SyncTexProvider> = Arc::new(sync_cli::SyncTexCli);

            // 全局设置目录（app_config_dir）
            let config_dir = app
                .path()
                .app_config_dir()
                .expect("无法解析应用配置目录");
            std::fs::create_dir_all(&config_dir).ok();
            let global_settings_path = config_dir.join("settings.json");
            let storage = Arc::new(storage::SettingsStorage::new(global_settings_path));

            // 调度器（D1：actor，状态收容 task 内；emitter 接 tauri 事件）
            let emitter = build_emitter(app.handle());
            let runner: Arc<dyn texpresso_core::scheduler::CompileRunner> =
                Arc::new(runner::LatexmkRunner { fs: fs.clone() });
            // setup 闭包不是 tokio 上下文：用 tauri 的 runtime（任何线程可用）
            let (scheduler, scheduler_task) = Scheduler::create(runner, emitter);
            tauri::async_runtime::spawn(scheduler_task.run());

            // 共享状态
            let project: Arc<RwLock<Option<texpresso_core::project::ProjectState>>> =
                Arc::new(RwLock::new(None));
            let settings: Arc<RwLock<Settings>> = Arc::new(RwLock::new(
                tauri::async_runtime::block_on(storage.load_global(fs.as_ref())),
            ));
            let overrides: Arc<RwLock<texpresso_core::settings::ProjectOverrides>> =
                Arc::new(RwLock::new(Default::default()));

            // 监视任务
            let watch_state = Arc::new(watch::WatchState {
                project: project.clone(),
                settings: settings.clone(),
                scheduler: scheduler.clone(),
                storage: storage.clone(),
                overrides: overrides.clone(),
                app: app.handle().clone(),
            });
            let watch_handle = watch::spawn_watcher(config_dir, watch_state);

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
        ])
        .events(tauri_specta::collect_events![
            CompileStatusEvent,
            ErrorsUpdatedEvent,
            PdfUpdatedEvent,
            FilesChangedEvent,
            SettingsChangedEvent,
        ])
}

#[cfg(test)]
mod tests {
    /// 手动运行导出前端绑定：`cargo test -p texpresso -- --ignored export_bindings`
    #[test]
    #[ignore]
    fn export_bindings() {
        use specta_typescript::Typescript;
        super::build_specta_builder()
            .export(Typescript::default(), "../src/bindings.ts")
            .expect("导出 TypeScript 绑定失败");
    }
}
