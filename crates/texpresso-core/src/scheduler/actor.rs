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
    ErrorKind,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// 事件输出：三个通道闭包（由 src-tauri 注入为 tauri 事件；测试注入收集器）。
/// scheduler 不知道 tauri 存在（信息局部性：依赖注入而非全局）。
pub struct Emitter {
    on_status: Arc<dyn Fn(CompileStatusDto) + Send + Sync>,
    on_errors: Arc<dyn Fn(Vec<ErrorEntry>) + Send + Sync>,
    on_pdf: Arc<dyn Fn(PathBuf) + Send + Sync>,
}

impl Emitter {
    pub fn new(
        on_status: Arc<dyn Fn(CompileStatusDto) + Send + Sync>,
        on_errors: Arc<dyn Fn(Vec<ErrorEntry>) + Send + Sync>,
        on_pdf: Arc<dyn Fn(PathBuf) + Send + Sync>,
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

    pub(crate) fn pdf(&self, path: PathBuf) {
        (self.on_pdf)(path);
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
                    let draft = req.kind == CompileKind::Quick;
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
        // 成功：广播 PDF 就绪
        if let CompileOutcome::Success { pdf_path, .. } = &outcome {
            self.emitter.pdf(pdf_path.clone());
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
                self.start(req);
            }
            Decide::FinishOk => {
                self.emitter.status(CompileStatusDto {
                    phase: CompilePhase::Success,
                    kind: None,
                    draft,
                });
            }
            Decide::Fail(kind) => {
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
        }]));
        h.compile(quick_req("main.tex"));
        wait_until(|| log.statuses().len() >= 2).await;
        let st = log.statuses();
        assert_eq!(st.len(), 2);
        // Running 与 Success 都必须带 draft=true，否则前端会在收敛前误清「引用待更新」
        assert!(st.iter().all(|s| s.draft), "草稿强度必须贯穿事件：{st:?}");
        assert_eq!(st[0].phase, CompilePhase::Running);
        assert_eq!(st[1].phase, CompilePhase::Success);
    }

    #[tokio::test]
    async fn full_request_reports_not_draft() {
        let (h, log, _) = setup(FakeRunner::with_results(vec![CompileOutcome::Success {
            pdf_path: PathBuf::from("proj/main.pdf"),
            kind: CompileKind::Full,
        }]));
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
        }]));
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
        }]));
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
            },
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
            },
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
            },
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
            }
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
