//! tauri 事件封装（modules.md §4 事件面契约，specta 生成 TS 类型）。

use serde::{Deserialize, Serialize};
use tauri_specta::Event;
use specta::Type;
use tauri::AppHandle;
use texpresso_core::scheduler::Emitter;
use texpresso_core::settings::Settings;
use texpresso_core::types::{
    CompileStatusDto, ErrorEntry, FilesChanged, PdfUpdated,
};
use texpresso_infra::watch::WatchSink;
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "compile-status")]
pub struct CompileStatusEvent(pub CompileStatusDto);

#[derive(Serialize, Deserialize, Clone, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "errors-updated")]
pub struct ErrorsUpdatedEvent(pub Vec<ErrorEntry>);

#[derive(Serialize, Deserialize, Clone, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "pdf-updated")]
pub struct PdfUpdatedEvent(pub PdfUpdated);

#[derive(Serialize, Deserialize, Clone, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "files-changed")]
pub struct FilesChangedEvent(pub FilesChanged);

#[derive(Serialize, Deserialize, Clone, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "settings-changed")]
pub struct SettingsChangedEvent(pub Settings);

/// 监视结果出口 → tauri 事件（ADR-0010：基础设施层不认识事件，形态在这里定型）。
pub struct TauriSink {
    pub app: AppHandle,
}

impl WatchSink for TauriSink {
    fn files_changed(&self, payload: FilesChanged) {
        let _ = FilesChangedEvent(payload).emit(&self.app);
    }

    fn settings_changed(&self, settings: Settings) {
        let _ = SettingsChangedEvent(settings).emit(&self.app);
    }
}

/// 把调度器输出接到 tauri 事件（scheduler 不知道 tauri 存在——依赖注入）。
pub fn build_emitter(app: &AppHandle) -> Emitter {
    let handle = app.clone();
    let handle2 = app.clone();
    let handle3 = app.clone();
    Emitter::new(
        Arc::new(move |dto: CompileStatusDto| {
            let _ = CompileStatusEvent(dto).emit(&handle);
        }),
        Arc::new(move |errors: Vec<ErrorEntry>| {
            let _ = ErrorsUpdatedEvent(errors).emit(&handle2);
        }),
        Arc::new(move |path: std::path::PathBuf| {
            let _ = PdfUpdatedEvent(PdfUpdated {
                path: path.to_string_lossy().into_owned(),
            })
            .emit(&handle3);
        }),
    )
}
