//! 调度器 actor（modules.md §2.5，设计决策 D1）。
//!
//! 状态收容：`queue`、`running`、`cancel` 全部是 task 私有字段——
//! 调度状态没有任何一份在 task 之外。外部只有 [`SchedulerHandle`]，
//! 连读都读不到（比锁状态机更彻底：无共享可变状态，自然无锁）。

use super::policy::{decide, Decide};
use super::queue::Queue;
use super::runner::CompileRunner;
use crate::types::{
    CompileKind, CompileOutcome, CompilePhase, CompileRequest, CompileStatusDto, ErrorEntry,
    ErrorKind, PdfUpdated,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// 事件输出：三个通道闭包（由 src-tauri 注入为 tauri 事件；测试注入收集器）。
/// scheduler 不知道 tauri 存在（信息局部性：依赖注入而非全局）。
pub struct Emitter {
    on_status: Arc<dyn Fn(CompileStatusDto) + Send + Sync>,
    on_errors: Arc<dyn Fn(Vec<ErrorEntry>) + Send + Sync>,
    /// PDF 就绪通道。载荷含**变化页集合**（2026-09，B/C 功能点）：调度器在这里做跨轮比对，
    /// 前端据此跳过无谓重载（空集合）或只重绘变化页。
    on_pdf: Arc<dyn Fn(PdfUpdated) + Send + Sync>,
}

impl Emitter {
    pub fn new(
        on_status: Arc<dyn Fn(CompileStatusDto) + Send + Sync>,
        on_errors: Arc<dyn Fn(Vec<ErrorEntry>) + Send + Sync>,
        on_pdf: Arc<dyn Fn(PdfUpdated) + Send + Sync>,
    ) -> Self {
        Self {
            on_status,
            on_errors,
            on_pdf,
        }
    }

    pub(crate) fn status(&self, dto: CompileStatusDto) {
        (self.on_status)(dto);
    }

    pub(crate) fn errors(&self, errors: Vec<ErrorEntry>) {
        (self.on_errors)(errors);
    }

    pub(crate) fn pdf(&self, payload: PdfUpdated) {
        (self.on_pdf)(payload);
    }
}

/// 调度命令。`JobFinished` 是内部命令（pub(crate)）：外部无法伪造任务完成。
pub(crate) enum SchedulerCommand {
    Compile(CompileRequest),
    Abort,
    JobFinished(CompileOutcome),
}

/// 外部唯一入口：只暴露两个意图（编译 / 终止），不暴露内部状态。
#[derive(Clone)]
pub struct SchedulerHandle {
    tx: mpsc::UnboundedSender<SchedulerCommand>,
}

impl SchedulerHandle {
    pub fn compile(&self, req: CompileRequest) {
        let _ = self.tx.send(SchedulerCommand::Compile(req));
    }

    pub fn abort(&self) {
        let _ = self.tx.send(SchedulerCommand::Abort);
    }
}

/// 运行中任务。
struct RunningJob {
    request: CompileRequest,
    handle: tokio::task::JoinHandle<CompileOutcome>,
    /// 已收到手动终止：on_finished 时即使 runner 忽略 cancel 返回了 Success/Timeout，
    /// 也按 Aborted 呈现（设计语义：终止 = 停 + 清队，不误报成功/错误/PDF）。
    aborted: bool,
}

pub struct Scheduler {
    rx: mpsc::UnboundedReceiver<SchedulerCommand>,
    runner: Arc<dyn CompileRunner>,
    emitter: Emitter,
    queue: Queue,
    running: Option<RunningJob>,
    cancel: Option<CancellationToken>,
    /// 上一次成功编译的**页哈希**（顺序 = 页号），用于算"本次变化了哪些页"。
    ///
    /// 放在 actor 内（而不是 runner 里）：runner 是**无状态**的（`&self` 不可变，一次调用完全
    /// 独立），跨轮状态属于调度语义——与 `queue`/`running` 同类，收容在 task 内不外露。
    /// `None` = 还没有可比对的基线（首次编译 → 全部页视为变化）。
    last_page_hashes: Option<Vec<u64>>,
}

impl Scheduler {
    /// 构造调度器（不假设 runtime 上下文——由接线方决定如何运行，见 [`Scheduler::run`]）。
    ///
    /// 返回（外部句柄, 调度器本体）：调用方负责 `spawn(scheduler.run())`，
    /// 例如 src-tauri 用 `tauri::async_runtime::spawn`，测试用 `tokio::spawn`。
    /// 这样 core 不依赖任何特定 runtime 的存在（修复：Tauri setup 非 tokio 上下文）。
    pub fn create(
        runner: Arc<dyn CompileRunner>,
        emitter: Emitter,
    ) -> (SchedulerHandle, Scheduler) {
        let (tx, rx) = mpsc::unbounded_channel();
        let scheduler = Scheduler {
            rx,
            runner,
            emitter,
            queue: Queue::new(),
            running: None,
            cancel: None,
            last_page_hashes: None,
        };
        (SchedulerHandle { tx }, scheduler)
    }

    /// 主循环：由接线方在合适的 runtime 上 spawn。
    pub async fn run(mut self) {
        while let Some(cmd) = self.next().await {
            self.handle(cmd).await;
        }
    }

    /// 运行中：select 命令通道与任务完成；空闲：只等命令。
    async fn next(&mut self) -> Option<SchedulerCommand> {
        if let Some(job) = self.running.as_mut() {
            tokio::select! {
                cmd = self.rx.recv() => cmd,
                out = &mut job.handle => Some(match out {
                    Ok(outcome) => SchedulerCommand::JobFinished(outcome),
                    Err(e) => SchedulerCommand::JobFinished(CompileOutcome::IoError {
                        message: format!("编译任务异常终止：{e}"),
                    }),
                }),
            }
        } else {
            self.rx.recv().await
        }
    }

    async fn handle(&mut self, cmd: SchedulerCommand) {
        match cmd {
            SchedulerCommand::Compile(req) => {
                if self.running.is_none() {
                    self.start(req);
                } else {
                    // 合并：最多一个等待条目，总是最新
                    // 事件纪律：仅当队列从空变非空才广播 Queued（状态无变化不重复发）
                    let was_empty = self.queue.is_empty();
                    let replaced = self.queue.take();
                    let draft = req.kind == CompileKind::Quick;
                    // roadmap ㉛：把"为什么这次要排队/被合并掉"打出来——用户看到的
                    // 「点了没反应」「白编了一次」几乎都能从这几行日志还原
                    match &replaced {
                        Some(old) => debug!(
                            old_root = %old.root_file.display(),
                            new_root = %req.root_file.display(),
                            kind = ?req.kind,
                            "编译请求合并：队列中的旧请求被最新请求替换（合并队列只留一个待办）"
                        ),
                        None => debug!(
                            root = %req.root_file.display(),
                            kind = ?req.kind,
                            "编译器忙：新请求入队等待（运行中的编译不打断）"
                        ),
                    }
                    self.queue.push(req);
                    if was_empty {
                        self.emitter.status(CompileStatusDto {
                            phase: CompilePhase::Queued,
                            kind: None,
                            draft,
                        });
                    }
                }
            }
            SchedulerCommand::Abort => {
                // 手动终止：停运行 + 清队列（design.md）
                let dropped = self.queue.take().is_some();
                debug!(had_pending = dropped, "手动终止：停运行中的编译并清空等待队列");
                if let Some(job) = self.running.as_mut() {
                    job.aborted = true;
                }
                if let Some(c) = self.cancel.take() {
                    c.cancel();
                }
                self.queue.clear();
            }
            SchedulerCommand::JobFinished(outcome) => self.on_finished(outcome).await,
        }
    }

    fn start(&mut self, req: CompileRequest) {
        let runner = self.runner.clone();
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        // roadmap ㉘：把强度带进状态事件，前端据此提示"引用待更新"
        let draft = req.kind == CompileKind::Quick;
        // roadmap ㉛：编译生命周期日志——"这次到底编没编、编的是哪份、什么强度"一眼可见
        debug!(
            root = %req.root_file.display(),
            kind = ?req.kind,
            timeout_s = req.timeout.as_secs(),
            "开始编译"
        );
        let req_for_task = req.clone();
        let handle = tokio::spawn(async move { runner.compile(req_for_task, token).await });
        self.running = Some(RunningJob {
            request: req,
            handle,
            aborted: false,
        });
        self.cancel = Some(cancel);
        self.emitter.status(CompileStatusDto {
            phase: CompilePhase::Running,
            kind: None,
            draft,
        });
    }

    async fn on_finished(&mut self, outcome: CompileOutcome) {
        let Some(running) = self.running.take() else {
            return;
        };
        self.cancel = None;

        // 手动终止守卫：abort 后即使 runner 忽略 cancel 返回 Success/Timeout，也按 Aborted 呈现；
        // 这样错误条目/PDF 就绪不会误发，决策表也按 Aborted 处理。
        let outcome = if running.aborted {
            CompileOutcome::Aborted
        } else {
            outcome
        };

        // 内容与 IO 错误：先广播错误列表（前端在收到 Running 时清空）
        match &outcome {
            CompileOutcome::ContentError { errors } => self.emitter.errors(errors.clone()),
            // 超时同样进错误列表（roadmap ㉕）：条目由 runner 现场构造，带证据化诊断
            // （为什么慢 / 疑似卡住 + 一键提高超时重试）。此前超时**什么都不显示**，
            // 用户只看到状态栏一个「失败 · 超时」，不知道下一步做什么。
            CompileOutcome::Timeout { entry } => self.emitter.errors(vec![entry.clone()]),
            CompileOutcome::IoError { message } => self.emitter.errors(vec![ErrorEntry {
                message: message.clone(),
                file: None,
                line: None,
                kind: ErrorKind::Io,
                // IO 类错误不是 `.log` 解析出来的，无诊断（roadmap ④ 只覆盖内容错误）
                diagnosis: None,
            }]),
            _ => {}
        }
        // 成功：广播 PDF 就绪，并带上**本次变化的页号**（2026-09，docs/research/incremental-edit-x-dvi.md）：
        // 空变化表 + pages > 0 ⇒ 这次编译的排版结果与上次逐页字节相同 ⇒ 前端可跳过重载（B）、
        // 且只重绘变化页（C）。比对基线是**上一次成功编译**的页哈希（进程内状态，见字段注释）。
        if let CompileOutcome::Success {
            pdf_path,
            page_hashes,
            ..
        } = &outcome
        {
            let changed = crate::xdv::changed_pages(self.last_page_hashes.as_deref(), page_hashes);
            self.last_page_hashes = Some(page_hashes.clone());
            debug!(
                pages = page_hashes.len(),
                changed = changed.len(),
                "PDF 就绪：已算出与上一轮的页级差异（changed=0 且 pages>0 表示逐页未变）"
            );
            self.emitter.pdf(PdfUpdated {
                path: pdf_path.to_string_lossy().into_owned(),
                changed_pages: changed,
                pages: page_hashes.len() as u32,
            });
        }

        let has_pending = !self.queue.is_empty();
        // roadmap ㉘：终态同样携带 draft——收敛（Full）成功前前端不该清"引用待更新"提示。
        // 成功用**实际执行**的强度（runner 可能把 Quick 升级为 Full，此时引用不回退，不该提示）；
        // 其余终态无从得知实际强度，按请求强度报（保守：宁可多提示一次）。
        let draft = match &outcome {
            CompileOutcome::Success { kind, .. } => *kind == CompileKind::Quick,
            _ => running.request.kind == CompileKind::Quick,
        };
        match decide(&outcome, has_pending) {
            Decide::StartPending => {
                let req = self.queue.take().expect("has_pending 与队列一致");
                debug!(
                    finished = %running.request.root_file.display(),
                    next = %req.root_file.display(),
                    "上一次编译结束：直接执行等待中的最新请求（不重复跑旧内容）"
                );
                self.start(req);
            }
            Decide::FinishOk => {
                info!(
                    root = %running.request.root_file.display(),
                    draft,
                    "编译成功（无等待请求）"
                );
                self.emitter.status(CompileStatusDto {
                    phase: CompilePhase::Success,
                    kind: None,
                    draft,
                });
            }
            Decide::Fail(kind) => {
                warn!(
                    root = %running.request.root_file.display(),
                    ?kind,
                    "编译失败（不重试，等待用户操作）"
                );
                self.emitter.status(CompileStatusDto {
                    phase: CompilePhase::Failed,
                    kind: Some(kind),
                    draft,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // PathBuf 只被测试用例用到（生产代码里不再直接构造路径），所以 import 放在测试模块内，
    // 避免 lib 目标出现 unused import 警告。
    use std::path::PathBuf;
    use crate::testutil::{event_log_emitter, wait_until, EventLog, FakeRunner};
    use crate::types::{Engine, ErrorKind, FailureKind};
    use std::sync::Arc;
    use std::time::Duration;

    fn req(name: &str) -> CompileRequest {
        CompileRequest {
            root_file: PathBuf::from(name),
            project_root: PathBuf::from("proj"),
            engine: Engine::XeLaTeX,
            timeout: Duration::from_secs(120),
            // 测试默认用 Full（既有断言期望 draft:false）；Quick 有专门用例
            kind: CompileKind::Full,
        }
    }

    /// 编辑触发的草稿请求（roadmap ㉘）。
    fn quick_req(name: &str) -> CompileRequest {
        CompileRequest {
            kind: CompileKind::Quick,
            ..req(name)
        }
    }

    fn error_entry(msg: &str) -> ErrorEntry {
        ErrorEntry {
            message: msg.into(),
            file: None,
            line: None,
            kind: ErrorKind::ContentError,
            diagnosis: None,
        }
    }

    /// 超时条目（roadmap ㉕）：`kind = Timeout`，实际由 runner 现场构造（含诊断）。
    fn timeout_entry(msg: &str) -> ErrorEntry {
        ErrorEntry {
            message: msg.into(),
            file: None,
            line: None,
            kind: ErrorKind::Timeout,
            diagnosis: None,
        }
    }

    /// 启动调度器 + 收集器 + 假 runner。
    fn setup(runner: FakeRunner) -> (SchedulerHandle, Arc<EventLog>, Arc<FakeRunner>) {
        let runner = Arc::new(runner);
        let log = Arc::new(EventLog::new());
        let (handle, scheduler) = Scheduler::create(runner.clone(), event_log_emitter(log.clone()));
        tokio::spawn(scheduler.run());
        (handle, log, runner)
    }

    fn running_dto() -> CompileStatusDto {
        CompileStatusDto {
            phase: CompilePhase::Running,
            kind: None,
            draft: false,
        }
    }

    fn success_dto() -> CompileStatusDto {
        CompileStatusDto {
            phase: CompilePhase::Success,
            kind: None,
            draft: false,
        }
    }

    fn failed_dto(kind: FailureKind) -> CompileStatusDto {
        CompileStatusDto {
            phase: CompilePhase::Failed,
            kind: Some(kind),
            draft: false,
        }
    }

    // ---- roadmap ㉘：草稿强度必须贯穿事件 ----

    #[tokio::test]
    async fn quick_request_marks_draft_in_every_status() {
        let (h, log, _) = setup(FakeRunner::with_results(vec![CompileOutcome::Success {
            pdf_path: PathBuf::from("proj/main.pdf"),
            kind: CompileKind::Quick,
        page_hashes: Vec::new() }]));
        h.compile(quick_req("main.tex"));
        wait_until(|| log.statuses().len() >= 2).await;
        let st = log.statuses();
        assert_eq!(st.len(), 2);
        // Running 与 Success 都必须带 draft=true，否则前端会在收敛前误清「引用待更新」
        assert!(st.iter().all(|s| s.draft), "草稿强度必须贯穿事件：{st:?}");
        assert_eq!(st[0].phase, CompilePhase::Running);
        assert_eq!(st[1].phase, CompilePhase::Success);
    }

    // ---- 页级差异（2026-09，docs/research/incremental-edit-x-dvi.md 的 B/C）----
    //
    // 契约：pdf-updated 事件必须带上"相对上一次成功编译的变化页"。
    // 前端据此跳过无谓重载（空表）或只重绘变化页——所以这三条语义是产品行为的判据。

    /// 造一个"成功 + 指定页哈希"的 runner 结果。
    fn ok_with_pages(hashes: Vec<u64>) -> CompileOutcome {
        CompileOutcome::Success {
            pdf_path: PathBuf::from("proj/main.pdf"),
            kind: CompileKind::Full,
            page_hashes: hashes,
        }
    }

    #[tokio::test]
    async fn pdf_event_reports_changed_pages_across_rounds() {
        let runner = FakeRunner::with_results(vec![
            ok_with_pages(vec![1, 2, 3]), // 第 1 轮：无基线 → 全部
            ok_with_pages(vec![1, 9, 3]), // 第 2 轮：只有第 2 页变
            ok_with_pages(vec![1, 9, 3]), // 第 3 轮：逐页相同 → 空表
            ok_with_pages(vec![1, 9]),    // 第 4 轮：页数变了 → 全部
        ]);
        let (h, log, _) = setup(runner);
        for expected_len in 1..=4 {
            h.compile(quick_req("main.tex"));
            wait_until(|| log.pdf_events().len() >= expected_len).await;
        }
        let ev = log.pdf_events();
        assert_eq!(ev[0].changed_pages, vec![1, 2, 3], "首次编译：无基线 → 全部页");
        assert_eq!(ev[0].pages, 3);
        assert_eq!(ev[1].changed_pages, vec![2], "只有第 2 页的字节变了");
        assert!(ev[2].changed_pages.is_empty(), "逐页相同 → 空表（B 的跳过判据）");
        assert_eq!(ev[2].pages, 3, "空表必须与 pages>0 一起出现才算'无变化'");
        assert_eq!(ev[3].changed_pages, vec![1, 2], "页数变化 → 全部重绘");
        assert_eq!(ev[3].pages, 2);
    }

    #[tokio::test]
    async fn pdf_event_without_page_info_is_unknown_not_no_change() {
        // 空 page_hashes = 无法判定（XDV 读不到）→ pages == 0，前端据此全量刷新。
        // 这条判据必须与"零页变化"区分开，否则会错误地跳过刷新。
        let runner = FakeRunner::with_results(vec![ok_with_pages(vec![1, 2]), ok_with_pages(Vec::new())]);
        let (h, log, _) = setup(runner);
        h.compile(quick_req("main.tex"));
        wait_until(|| log.pdf_events().len() >= 1).await;
        h.compile(quick_req("main.tex"));
        wait_until(|| log.pdf_events().len() >= 2).await;
        let ev = log.pdf_events();
        assert_eq!(ev[1].pages, 0, "无法判定 → pages = 0");
        assert!(ev[1].changed_pages.is_empty());
    }

    #[tokio::test]
    async fn full_request_reports_not_draft() {
        let (h, log, _) = setup(FakeRunner::with_results(vec![CompileOutcome::Success {
            pdf_path: PathBuf::from("proj/main.pdf"),
            kind: CompileKind::Full,
        page_hashes: Vec::new() }]));
        h.compile(req("main.tex"));
        wait_until(|| log.statuses().len() >= 2).await;
        assert!(log.statuses().iter().all(|s| !s.draft), "Full 应报 draft=false");
    }

    #[tokio::test]
    async fn quick_upgraded_to_full_reports_not_draft() {
        // 请求是 Quick（编辑触发），但 runner 因「项目尚无构建产物」升级为 Full。
        // 此时引用不回退，成功态必须报 draft=false——否则前端会提示"引用待更新"
        // 并多跑一次无意义的收敛，而实际输出已收敛（roadmap ㉘）。
        let (h, log, _) = setup(FakeRunner::with_results(vec![CompileOutcome::Success {
            pdf_path: PathBuf::from("proj/main.pdf"),
            kind: CompileKind::Full,
        page_hashes: Vec::new() }]));
        h.compile(quick_req("main.tex"));
        wait_until(|| log.statuses().len() >= 2).await;
        let st = log.statuses();
        assert_eq!(st[0].phase, CompilePhase::Running);
        assert!(st[0].draft, "运行中只能按请求强度报：{st:?}");
        assert!(!st[1].draft, "实际跑了 Full，成功态不该报草稿：{st:?}");
    }

    // ---- 基础路径 ----

    #[tokio::test]
    async fn single_compile_success() {
        let (h, log, _) = setup(FakeRunner::with_results(vec![CompileOutcome::Success {
            pdf_path: PathBuf::from("proj/main.pdf"),
            kind: CompileKind::Full,
        page_hashes: Vec::new() }]));
        h.compile(req("main.tex"));
        wait_until(|| log.statuses().len() >= 2).await;
        assert_eq!(
            log.statuses(),
            vec![running_dto(), success_dto()]
        );
        assert_eq!(log.pdfs(), vec![PathBuf::from("proj/main.pdf")]);
    }

    // ---- 合并语义 ----

    #[tokio::test]
    async fn merge_while_running_keeps_latest_only() {
        let (h, log, runner) = setup(FakeRunner::with_hold());
        h.compile(req("a.tex"));
        wait_until(|| runner.calls().len() == 1).await;

        // 运行中连来两个请求：合并为最新一个
        h.compile(req("b.tex"));
        h.compile(req("c.tex"));
        wait_until(|| {
            log.statuses().contains(&CompileStatusDto {
                phase: CompilePhase::Queued,
                kind: None,
                draft: false,
            })
        })
        .await;
        assert_eq!(
            log.statuses()
                .iter()
                .filter(|s| s.phase == CompilePhase::Queued)
                .count(),
            1,
            "合并语义：只发一次 Queued"
        );

        // 放行 a：完成 → 启动合并后的 c（c 又挂起）
        runner.release();
        wait_until(|| runner.calls().len() == 2).await;
        assert_eq!(runner.calls()[1].root_file, PathBuf::from("c.tex"));
        // 放行 c：完成
        runner.release();
        wait_until(|| log.statuses().len() >= 4).await;

        assert_eq!(
            log.statuses(),
            vec![
                running_dto(),
                CompileStatusDto {
                    phase: CompilePhase::Queued,
                    kind: None,
                    draft: false,
                },
                running_dto(),
                success_dto(),
            ]
        );
    }

    // ---- 超时（roadmap ㉕：不自动重试，改为立刻失败 + 广播证据化错误条目） ----

    #[tokio::test]
    async fn timeout_fails_immediately_without_retry() {
        let (h, log, runner) = setup(FakeRunner::with_results(vec![
            CompileOutcome::Timeout {
                entry: timeout_entry("编译超过 120s 上限"),
            },
            CompileOutcome::Success {
                pdf_path: PathBuf::from("proj/main.pdf"),
                kind: CompileKind::Full,
            page_hashes: Vec::new() },
        ]));
        h.compile(req("main.tex"));
        wait_until(|| log.statuses().contains(&failed_dto(FailureKind::Timeout))).await;
        assert_eq!(
            log.statuses(),
            vec![running_dto(), failed_dto(FailureKind::Timeout)]
        );
        // 关键行为：**只跑了一次**（旧版会静默重试，用户白等一个完整超时窗口）
        assert_eq!(runner.calls().len(), 1, "超时不得自动重试");
        // 且超时必须进错误列表（旧版什么都不显示）
        assert_eq!(
            log.errors(),
            vec![vec![timeout_entry("编译超过 120s 上限")]],
            "超时应广播带诊断的条目"
        );
    }

    #[tokio::test]
    async fn timeout_with_pending_starts_pending_without_failing() {
        // 等待中的是新内容：直接跑它，不为旧内容展示失败（与内容错误同语义）
        let (h, log, runner) = setup(FakeRunner::with_hold_and_results(vec![
            CompileOutcome::Timeout {
                entry: timeout_entry("旧内容超时"),
            },
            CompileOutcome::Success {
                pdf_path: PathBuf::from("proj/main.pdf"),
                kind: CompileKind::Full,
            page_hashes: Vec::new() },
        ]));
        h.compile(req("a.tex"));
        wait_until(|| runner.calls().len() == 1).await;
        h.compile(req("b.tex"));
        runner.release();
        wait_until(|| runner.calls().len() == 2).await;
        runner.release();
        wait_until(|| log.statuses().len() >= 4).await;
        assert_eq!(
            log.statuses(),
            vec![
                running_dto(),
                CompileStatusDto {
                    phase: CompilePhase::Queued,
                    kind: None,
                    draft: false,
                },
                running_dto(),
                success_dto(),
            ]
        );
        assert!(!log.statuses().iter().any(|s| s.phase == CompilePhase::Failed));
    }

    // ---- 内容错误 ----

    #[tokio::test]
    async fn content_error_no_pending_fails_and_emits_errors() {
        let (h, log, _) = setup(FakeRunner::with_results(vec![CompileOutcome::ContentError {
            errors: vec![error_entry("bad syntax")],
        }]));
        h.compile(req("main.tex"));
        wait_until(|| log.statuses().contains(&failed_dto(FailureKind::ContentError))).await;
        assert_eq!(
            log.statuses(),
            vec![running_dto(), failed_dto(FailureKind::ContentError)]
        );
        assert_eq!(log.errors(), vec![vec![error_entry("bad syntax")]]);
    }

    #[tokio::test]
    async fn content_error_with_pending_skips_error_state_and_starts_pending() {
        let (h, log, runner) = setup(FakeRunner::with_hold_and_results(vec![
            CompileOutcome::ContentError {
                errors: vec![error_entry("bad syntax")],
            },
            CompileOutcome::Success {
                pdf_path: PathBuf::from("proj/main.pdf"),
                kind: CompileKind::Full,
            page_hashes: Vec::new() },
        ]));
        h.compile(req("a.tex"));
        wait_until(|| runner.calls().len() == 1).await;
        h.compile(req("b.tex"));

        // 放行 a：返回内容错误 → 错误已广播，但决策是直接开跑 b（不展示 Failed）
        runner.release();
        wait_until(|| runner.calls().len() == 2).await;
        wait_until(|| log.errors().len() == 1).await;
        assert_eq!(log.errors(), vec![vec![error_entry("bad syntax")]]);
        // 放行 b：成功
        runner.release();
        wait_until(|| log.statuses().len() >= 4).await;
        assert_eq!(
            log.statuses(),
            vec![
                running_dto(),
                CompileStatusDto {
                    phase: CompilePhase::Queued,
                    kind: None,
                    draft: false,
                },
                running_dto(),
                success_dto(),
            ]
        );
    }

    // ---- 手动终止 ----

    #[tokio::test]
    async fn abort_during_running_fails_aborted() {
        let (h, log, runner) = setup(FakeRunner::with_hold());
        h.compile(req("main.tex"));
        wait_until(|| runner.calls().len() == 1).await;
        h.abort();
        wait_until(|| log.statuses().contains(&failed_dto(FailureKind::Aborted))).await;
        assert_eq!(
            log.statuses(),
            vec![running_dto(), failed_dto(FailureKind::Aborted)]
        );
        assert!(log.pdfs().is_empty());
    }

    #[tokio::test]
    async fn abort_clears_pending_and_later_request_runs_fresh() {
        let (h, log, runner) = setup(FakeRunner::with_hold());
        h.compile(req("a.tex"));
        wait_until(|| runner.calls().len() == 1).await;
        h.compile(req("b.tex")); // 排队
        h.abort(); // 清空队列 + 取消运行中
        wait_until(|| log.statuses().contains(&failed_dto(FailureKind::Aborted))).await;

        // b 已被清掉：新请求 c 直接开跑
        h.compile(req("c.tex"));
        runner.release();
        wait_until(|| runner.calls().len() == 2).await;
        assert_eq!(runner.calls()[1].root_file, PathBuf::from("c.tex"));
    }

    /// 忽略取消契约的 runner：挂起直到 `release()`，然后**忽略 cancel** 返回 Success
    /// （模拟真实 runner 不遵守 D2 取消契约的极端情况）。
    struct CancelIgnoringRunner {
        hold: Arc<tokio::sync::Notify>,
    }

    impl CancelIgnoringRunner {
        fn new() -> Self {
            Self {
                hold: Arc::new(tokio::sync::Notify::new()),
            }
        }
        fn release(&self) {
            self.hold.notify_one();
        }
    }

    #[async_trait::async_trait]
    impl crate::scheduler::CompileRunner for CancelIgnoringRunner {
        async fn compile(
            &self,
            _req: CompileRequest,
            _cancel: tokio_util::sync::CancellationToken,
        ) -> CompileOutcome {
            self.hold.notified().await; // 挂起直到 release（期间忽略 cancel）
            CompileOutcome::Success {
                pdf_path: PathBuf::from("out.pdf"),
                kind: CompileKind::Full,
            page_hashes: Vec::new() }
        }
    }

    #[tokio::test]
    async fn abort_overrides_stale_success_to_aborted() {
        // 用户 abort 后 runner 忽略 cancel 返回 Success——调度器仍应呈现 Aborted，
        // 且不误发 errors/pdf（termination 语义：停 + 清队，不报成功/PDF）。
        let concrete = Arc::new(CancelIgnoringRunner::new());
        let releaser = concrete.clone();
        let log = Arc::new(EventLog::new());
        let runner: Arc<dyn crate::scheduler::CompileRunner> = concrete;
        let (handle, scheduler) = Scheduler::create(runner, event_log_emitter(log.clone()));
        tokio::spawn(scheduler.run());
        handle.compile(req("main.tex"));
        wait_until(|| log.statuses().contains(&running_dto())).await;
        handle.abort(); // 运行中终止：running.aborted = true
        releaser.release(); // 放行 → runner 返回 Success，应被 override 为 Aborted
        wait_until(|| log.statuses().contains(&failed_dto(FailureKind::Aborted))).await;
        assert!(log.pdfs().is_empty(), "aborted 不应发 pdf-updated");
        assert_eq!(
            log.statuses(),
            vec![running_dto(), failed_dto(FailureKind::Aborted)]
        );
    }

    // ---- IO 错误 ----

    #[tokio::test]
    async fn io_error_fails_and_emits_io_entry() {
        let (h, log, _) = setup(FakeRunner::with_results(vec![CompileOutcome::IoError {
            message: "latexmk 无法启动".into(),
        }]));
        h.compile(req("main.tex"));
        wait_until(|| log.statuses().contains(&failed_dto(FailureKind::ContentError))).await;
        let errors = log.errors();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0][0].kind, ErrorKind::Io);
        assert!(errors[0][0].message.contains("latexmk"));
    }
}
