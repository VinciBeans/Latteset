//! tauri 事件封装（modules.md §4 事件面契约，specta 生成 TS 类型）。

use serde::{Deserialize, Serialize};
use tauri_specta::Event;
use specta::Type;
use tauri::AppHandle;
use latteset_core::scheduler::{CompileProgress, Emitter};
use latteset_core::settings::Settings;
use latteset_core::types::{
    CompileStatusDto, ErrorEntry, FilesChanged, PdfUpdated,
};
use latteset_infra::watch::WatchSink;
use std::path::Path;
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "compile-status")]
pub struct CompileStatusEvent(pub CompileStatusDto);

/// 编译**进行中**的进度（roadmap「阶段 2 · 流式输出」）：已排版页数。
/// 只在数值变大时发出；编译终态仍由 `compile-status` 给出。
#[derive(Serialize, Deserialize, Clone, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "compile-progress")]
pub struct CompileProgressEvent(pub CompileProgressDto);

/// 进度载荷（**非权威**中间态；前端只在"排版中"时展示）。
#[derive(Serialize, Deserialize, Clone, Type)]
pub struct CompileProgressDto {
    /// 引擎已输出的最大页码（`[N]` 标记）。
    pub pages: u32,
}

/// 编译**进行中**的「部分 PDF」（roadmap ㉞ 流式出图）：把已完成页缝成一份可预览的 PDF。
///
/// 与 [`PdfUpdatedEvent`] **刻意分成两个事件**（理由与 [`CompileErrorsEvent`] 同款）：后者是
/// **权威终态**（`CompileOutcome::Success` 那一份），前者是中间态；中间态一旦晚于终态抵达，
/// 共用事件名就会把权威结果顶掉。
///
/// 消费方（前端）三条约束：
/// - 只在"编译中"接受它（`compile-status` 的终态一到就不再理会——正确性只由终态 PDF 表达）；
/// - `path` 与权威 PDF **不是同一个文件**（`tmp/` 下的部分产物，页数与内容随后续趟增长）
///   ⇒ 不得把它写进"当前文档"的权威槽位，也不该让它按"换文档"处理（同一次编译里它会被
///   原子替换成新内容，路径不变）；
/// - `pages` **只增不减**（出图闸门要求页数有涨才发）⇒ 用它丢弃晚到的旧帧。
///
/// 只有**库形态**会发这个事件：子进程档不落 XDV、无从合成（能力位如实显示"不可用"，
/// 不静默不生效）。
#[derive(Serialize, Deserialize, Clone, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "compile-preview")]
pub struct CompilePreviewEvent(pub CompilePreviewDto);

/// 部分 PDF 载荷（**非权威**中间态）。
#[derive(Serialize, Deserialize, Clone, Type)]
pub struct CompilePreviewDto {
    /// 部分 PDF 的绝对路径：`tmp/<stem>.preview.pdf`（同一路径会被**原子替换**成更新的内容）。
    pub path: String,
    /// 该文件包含的页数（= 已完成页数；只增不减）。
    pub pages: u32,
}

#[derive(Serialize, Deserialize, Clone, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "errors-updated")]
pub struct ErrorsUpdatedEvent(pub Vec<ErrorEntry>);

/// 编译**进行中**解析到的致命错误（roadmap「阶段 2 · 流式输出」）。
///
/// 与 [`ErrorsUpdatedEvent`] **刻意分成两个事件**：后者是**权威终态**（编译结束的完整清单，
/// 含警告），前者是运行中的中间态（只有致命错误）。真机实测（2026-09，长文档超时用例）到混用的
/// 后果：终态写出「错误行 = 1」之后，列表又变回 30 条实时条目。机制上是收尾期的补发——runner
/// 拿到 outcome 后仍要置 stop 并 join 尾随 `.log` 的任务（有 600ms 上限），那次补读照样发事件；
/// 它一旦晚于终态抵达前端，就把权威列表顶掉了。分开后前端按 `phase` 守卫：只在"排版中"接受
/// 实时错误，终态一到便不再理会。
#[derive(Serialize, Deserialize, Clone, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "compile-errors")]
pub struct CompileErrorsEvent(pub Vec<ErrorEntry>);

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

/// 编译进行中的反馈出口（core `CompileProgress` → tauri 事件）。
///
/// 进度 → `compile-progress`，错误 → `compile-errors`，部分 PDF → `compile-preview`
/// （**都不复用**终态事件名：中间态可能晚于终态抵达，共用名字就会把它顶掉——见
/// [`CompileErrorsEvent`]）。
pub struct TauriProgress {
    pub app: AppHandle,
}

impl CompileProgress for TauriProgress {
    fn pages(&self, pages: u32) {
        let _ = CompileProgressEvent(CompileProgressDto { pages }).emit(&self.app);
    }

    fn errors(&self, errors: &[ErrorEntry]) {
        let _ = CompileErrorsEvent(errors.to_vec()).emit(&self.app);
    }

    fn partial_pdf(&self, path: &Path, pages: u32) {
        let _ = CompilePreviewEvent(CompilePreviewDto {
            path: path.to_string_lossy().into_owned(),
            pages,
        })
        .emit(&self.app);
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
        // PDF 就绪：载荷现在是完整 DTO（含 changed_pages / pages）——调度器算好页级差异后直接透传，
        // 接线层不再自己重建 `PdfUpdated`（差异信息只有调度器知道，接线层重建不了）。
        Arc::new(move |payload: PdfUpdated| {
            let _ = PdfUpdatedEvent(payload).emit(&handle3);
        }),
    )
}
