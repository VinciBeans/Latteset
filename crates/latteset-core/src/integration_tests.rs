//! 综合集成测试（任务 4）：模块间配合全链路。
//!
//! 覆盖 design.md 验收链路：打开项目 → 探测根文件 → 文件变化 → 编译 → 错误 → 修复 → 恢复。
//! 真实模块：project（FakeFS 注入）→ settings → compose → scheduler（FakeRunner 注入）→ Emitter。
//! 验证：模块只通过接口协作、事件序列正确、信息局部性成立。

use crate::compose::{compile_request_for_change, ComposeContext};
use crate::log_parser::{diagnose, parse_log};
use crate::project::{collect_tex_files, find_candidates, resolve, ProjectState};
use crate::scheduler::Scheduler;
use crate::settings::Settings;
use crate::testutil::{event_log_emitter, wait_until, EventLog, FakeFS, FakeRunner};
use crate::types::{
    CompileKind, CompileOutcome, CompilePhase, CompileRequest, CompileStatusDto, ErrorEntry,
    ErrorKind, FailureKind,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 模拟 open_project 流程（modules.md §8）：扫描 → 探测 → 项目状态。
async fn open_project(fs: &FakeFS, root: &Path) -> ProjectState {
    let files = collect_tex_files(fs, root).await.expect("扫描失败");
    let candidates = find_candidates(&files, root, |p| fs.file(p).map(str::to_owned));
    let resolution = resolve(candidates);
    let root_file = match resolution {
        crate::project::RootResolution::Unique(p) => Some(p),
        _ => None, // Multiple/None 由前端介入（弹窗/手动指定）
    };
    ProjectState {
        root: root.to_path_buf(),
        root_file,
    }
}

fn status(phase: CompilePhase, kind: Option<FailureKind>) -> CompileStatusDto {
    CompileStatusDto { phase, kind, draft: false }
}

/// 草稿（Quick）状态：roadmap ㉘ 的编辑触发路径。
fn draft_status(phase: CompilePhase, kind: Option<FailureKind>) -> CompileStatusDto {
    CompileStatusDto { phase, kind, draft: true }
}

/// 场景 1：打开 → 探测 → 子文件变化 → 编译成功（design.md 验收链路主路径）。
#[tokio::test]
async fn open_detect_compile_full_chain() {
    let mut fs = FakeFS::new();
    fs.put_file(
        "proj/main.tex",
        "\\documentclass{article}\n\\input{chapters/intro}\n",
    );
    fs.put_file("proj/chapters/intro.tex", "chapter content");

    // 1. 打开项目：探测出唯一根文件
    let project = open_project(&fs, Path::new("proj")).await;
    assert_eq!(project.root_file.as_deref(), Some(Path::new("proj/main.tex")));

    // 2. 组合层：子文件变化 → 编译请求
    let settings = Settings::default();
    let req = compile_request_for_change(
        ComposeContext {
            project: &project,
            settings: &settings,
        },
        Path::new("proj/chapters/intro.tex"),
    )
    .expect("子文件变化应触发编译");

    // 3. 调度器执行（FakeRunner 返回成功 + PDF）
    let runner = Arc::new(FakeRunner::with_results(vec![CompileOutcome::Success {
        pdf_path: PathBuf::from("proj/main.pdf"),
        kind: CompileKind::Quick,
    }]));
    let log = Arc::new(EventLog::new());
    let (handle, scheduler) = Scheduler::create(runner.clone(), event_log_emitter(log.clone()));
    tokio::spawn(scheduler.run());

    handle.compile(req);
    wait_until(|| log.statuses().len() >= 2).await;

    // roadmap ㉘：change 触发是草稿（Quick），事件必须带 draft=true
    assert_eq!(
        log.statuses(),
        vec![draft_status(CompilePhase::Running, None), draft_status(CompilePhase::Success, None)]
    );
    assert_eq!(log.pdfs(), vec![PathBuf::from("proj/main.pdf")]);
    assert_eq!(runner.calls().len(), 1);
    assert_eq!(runner.calls()[0].root_file, PathBuf::from("proj/main.tex"));
}

/// 场景 2：多候选根文件 → 探测返回 Multiple（前端弹窗后手动覆盖，modules.md §5.4）。
#[tokio::test]
async fn multi_candidate_then_manual_override() {
    let mut fs = FakeFS::new();
    fs.put_file("proj/a.tex", "\\documentclass{article}");
    fs.put_file("proj/b.tex", "\\documentclass{book}");

    let files = collect_tex_files(&fs, Path::new("proj")).await.unwrap();
    let resolution = resolve(find_candidates(&files, Path::new("proj"), |p| fs.file(p).map(str::to_owned)));
    assert!(matches!(
        resolution,
        crate::project::RootResolution::Multiple(_)
    ));

    // 前端弹窗选择 b.tex → 覆盖写进设置（modules.md §6 root_file 键）
    let settings = Settings {
        root_file: Some(PathBuf::from("proj/b.tex")),
        ..Settings::default()
    };
    let project = ProjectState {
        root: PathBuf::from("proj"),
        root_file: settings.root_file.clone(),
    };

    let req = compile_request_for_change(
        ComposeContext {
            project: &project,
            settings: &settings,
        },
        Path::new("proj/b.tex"),
    )
    .expect("覆盖后应触发");
    assert_eq!(req.root_file, PathBuf::from("proj/b.tex"));
}

/// 场景 3：连续输入（连发 3 次变化）→ 合并队列只执行最新一次（ADR-0001 合并语义）。
#[tokio::test]
async fn continuous_typing_merges_to_latest() {
    let mut fs = FakeFS::new();
    fs.put_file("proj/main.tex", "\\documentclass{article}");
    let project = open_project(&fs, Path::new("proj")).await;
    let settings = Settings::default();

    let runner = Arc::new(FakeRunner::with_hold_and_results(vec![
        CompileOutcome::Success { pdf_path: PathBuf::from("p.pdf"), kind: CompileKind::Quick },
        CompileOutcome::Success { pdf_path: PathBuf::from("p.pdf"), kind: CompileKind::Quick },
    ]));
    let log = Arc::new(EventLog::new());
    let (handle, scheduler) = Scheduler::create(runner.clone(), event_log_emitter(log.clone()));
    tokio::spawn(scheduler.run());

    // 模拟输入停顿间的三次自动保存（modules.md §5.1 触发链）
    for _ in 0..3 {
        let req = compile_request_for_change(
            ComposeContext { project: &project, settings: &settings },
            Path::new("proj/main.tex"),
        )
        .unwrap();
        handle.compile(req);
    }
    wait_until(|| runner.calls().len() == 1).await;
    assert_eq!(log.statuses().iter().filter(|s| s.phase == CompilePhase::Queued).count(), 1);

    runner.release();
    wait_until(|| runner.calls().len() == 2).await;
    runner.release();
    wait_until(|| log.statuses().len() >= 4).await;

    // 只执行了 2 次编译（第 2/3 次变化合并）
    assert_eq!(runner.calls().len(), 2);
    // roadmap ㉘：三次都来自「文件变化」，故全程 draft=true
    assert_eq!(
        log.statuses(),
        vec![
            draft_status(CompilePhase::Running, None),
            draft_status(CompilePhase::Queued, None),
            draft_status(CompilePhase::Running, None),
            draft_status(CompilePhase::Success, None),
        ]
    );
}

/// 场景 4：错误 → 修改 → 修复 → 恢复（design.md Windows 验收清单全链路）。
/// 真实 .log 解析参与：runner 结果来自 parse_log 的输出。
#[tokio::test]
async fn error_then_fix_then_recover() {
    let bad_log = "! LaTeX Error: File `nope.sty' not found.\nl.5 \\usepackage{nope}\n";
    let errors: Vec<ErrorEntry> = parse_log(bad_log)
        .into_iter()
        .map(|m| {
            let diagnosis = diagnose(&m);
            ErrorEntry {
                message: m.message,
                file: m.file,
                line: m.line,
                kind: ErrorKind::ContentError,
                diagnosis,
            }
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, Some(5));
    // roadmap ④：真实缺包错误应带上「缺少宏包 nope.sty」的诊断
    let diag = errors[0].diagnosis.as_ref().expect("缺包错误应有诊断");
    assert!(diag.cause.contains("nope.sty"), "诊断原因：{}", diag.cause);

    let mut fs = FakeFS::new();
    fs.put_file("proj/main.tex", "\\documentclass{article}");
    let project = open_project(&fs, Path::new("proj")).await;
    let settings = Settings::default();

    let runner = Arc::new(FakeRunner::with_hold_and_results(vec![
        CompileOutcome::ContentError { errors: errors.clone() },
        CompileOutcome::Success { pdf_path: PathBuf::from("proj/main.pdf"), kind: CompileKind::Quick },
    ]));
    let log = Arc::new(EventLog::new());
    let (handle, scheduler) = Scheduler::create(runner.clone(), event_log_emitter(log.clone()));
    tokio::spawn(scheduler.run());

    let ctx = ComposeContext { project: &project, settings: &settings };

    // 第一次编译：内容错误
    handle.compile(compile_request_for_change(ctx, Path::new("proj/main.tex")).unwrap());
    wait_until(|| runner.calls().len() == 1).await;
    runner.release();
    wait_until(|| log.statuses().contains(&draft_status(CompilePhase::Failed, Some(FailureKind::ContentError)))).await;
    assert_eq!(log.errors(), vec![errors.clone()]);

    // 用户修复（第二次变化）：成功恢复
    handle.compile(compile_request_for_change(ctx, Path::new("proj/main.tex")).unwrap());
    wait_until(|| runner.calls().len() == 2).await;
    runner.release();
    wait_until(|| log.statuses().len() >= 4).await;
    wait_until(|| log.pdfs().len() == 1).await;

    // roadmap ㉘：两次都由文件变化触发 → 全程 draft=true
    assert_eq!(
        log.statuses(),
        vec![
            draft_status(CompilePhase::Running, None),
            draft_status(CompilePhase::Failed, Some(FailureKind::ContentError)),
            draft_status(CompilePhase::Running, None),
            draft_status(CompilePhase::Success, None),
        ]
    );
    assert_eq!(log.pdfs(), vec![PathBuf::from("proj/main.pdf")]);
}

/// 场景 5：手动终止 → 后续手动编译直接执行（design.md 手动终止语义）。
#[tokio::test]
async fn abort_then_manual_compile() {
    let mut fs = FakeFS::new();
    fs.put_file("proj/main.tex", "\\documentclass{article}");
    let project = open_project(&fs, Path::new("proj")).await;
    let settings = Settings::default();

    let runner = Arc::new(FakeRunner::with_hold_and_results(vec![
        CompileOutcome::Success { pdf_path: PathBuf::from("p.pdf"), kind: CompileKind::Full },
    ]));
    let log = Arc::new(EventLog::new());
    let (handle, scheduler) = Scheduler::create(runner.clone(), event_log_emitter(log.clone()));
    tokio::spawn(scheduler.run());

    handle.compile(compile_request_manual_for(&project, &settings));
    wait_until(|| runner.calls().len() == 1).await;

    handle.abort();
    wait_until(|| log.statuses().contains(&status(CompilePhase::Failed, Some(FailureKind::Aborted)))).await;

    // 终止后手动编译：直接开跑
    handle.compile(compile_request_manual_for(&project, &settings));
    wait_until(|| runner.calls().len() == 2).await;
    runner.release();
    wait_until(|| log.statuses().len() >= 4).await;
    assert_eq!(
        log.statuses()[3],
        status(CompilePhase::Success, None)
    );
}

/// 场景 6：信息局部性检验——外部修改不经过前端也能触发（watch 路径 = compose 路径）。
#[tokio::test]
async fn external_edit_triggers_same_path() {
    // 前端自动保存与外部编辑器修改在架构上是同一路径（modules.md §5.1/ADR-0007）：
    // 都是"磁盘变化 → compose → 请求"。此处验证 compose 对"外部文件"（非前端打开）同样产出请求。
    let project = ProjectState {
        root: PathBuf::from("proj"),
        root_file: Some(PathBuf::from("proj/main.tex")),
    };
    let settings = Settings::default();
    let ctx = ComposeContext { project: &project, settings: &settings };

    // 外部编辑器修改了子文件（前端从未打开过它）
    let req = compile_request_for_change(ctx, Path::new("proj/notes/chap2.tex"));
    assert!(req.is_some(), "外部修改必须触发编译（文件系统是唯一真相）");
}

fn compile_request_manual_for(project: &ProjectState, settings: &Settings) -> CompileRequest {
    crate::compose::compile_request_manual(ComposeContext { project, settings }).unwrap()
}
