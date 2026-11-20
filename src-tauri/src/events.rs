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
    /// 已排版页数。内嵌引擎没有 stdout `[N]` 可扫，由 runner 在**每趟排版收尾**从捕获的
    /// `<stem>.xdv` 数完整页（`core::xdv::complete_pages`）—— 与终态页哈希同一份权威数据。
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
/// 只有**内嵌引擎**会有这个通道（XDV 在我们的捕获表里才能合成部分 PDF）；阶段②起它就是唯一形态
/// ⇒ 该事件恒可用（不再有"另一形态不发"的口径）。
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

/// 编译**进行中**的提示（ADR-0015 决策 4：能力边界要**显式说明**，不静默降级）。
///
/// 典型来源：runner 检出文档的构建步骤需要外部工具 —— **biber** 是外部程序（不随本产品分发；
/// 内嵌引擎**不运行外部工具** ⇒ 不会调用 biber、不生成 `.bbl`，引用/参考文献可能停在未解析；
/// `backend=bibtex` 的文档由内建 `BibtexEngine` 生成 `.bbl`，不受影响），
/// **makeindex / glossaries** 则不会被自动运行（那一步被跳过，索引/术语表为空）。
/// 经 `CompileProgress::notice` 上报一句话，前端按"提示"样式展示（状态栏/报告面板）。
///
/// 注意它**不是错误本身**：被跳过的步骤（索引/术语表）编译仍可能成功；biber 请求未执行时
/// `.log` 会报 `Please (re)run Biber` 这类内容错误，另有错误入口。
///
/// 与 `compile-errors` 分开：那是致命错误的中间态，这是能力边界的告知。
#[derive(Serialize, Deserialize, Clone, Type, tauri_specta::Event)]
#[tauri_specta(event_name = "compile-notice")]
pub struct CompileNoticeEvent {
    pub message: String,
}

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

    /// 能力边界提示（ADR-0015 决策 4）：日志与事件**都给** ——
    /// 日志留给排障（编译失败时那句话往往就是根因），事件让用户当场看见"这个构建步骤需要外部工具
    /// （biber 是外部程序，内嵌引擎不运行外部工具 ⇒ 不会调用 biber、引用可能未解析、不生成 `.bbl`；
    /// backend=bibtex 由内建 BibtexEngine 生成 `.bbl`）或本引擎不会自动跑（索引/术语表）"。
    fn notice(&self, msg: &str) {
        tracing::warn!("{msg}");
        let _ = CompileNoticeEvent {
            message: msg.to_string(),
        }
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
