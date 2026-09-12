//! Latexmk 执行器（modules.md §2.6 / 设计决策 D2；位于基础设施层，ADR-0010）。
//!
//! 超时检测、进程树杀、PDF 拷贝全在这里；调度器无时钟、无进程概念。

use async_trait::async_trait;
use std::path::Path;
use texpresso_core::log_parser::{
    diagnose, diagnose_timeout, pages_typeset, parse_log, source_release_hint, DiagnosisKind,
    TimeoutEvidence,
};
use texpresso_core::project::{collect_tex_files, FileSystem};
use texpresso_core::scheduler::CompileRunner;
use texpresso_core::types::{CompileKind, CompileOutcome, CompileRequest, ErrorEntry, ErrorKind};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// 编译中间目录（design.md：统一收纳 tmp/，与 core 忽略规则一致）。
const OUT_DIR: &str = "tmp";

/// 编译器实现（`CompileRunner` 的唯一实现）。
///
/// 名字是历史遗留：㉘ 之后它按 [`CompileKind`] 驱动**两条**命令——`Full` 用 latexmk、
/// `Quick` 直调引擎单趟。改名会牵动架构图 SVG 与三处文档，暂留此名。
pub struct LatexmkRunner {
    pub fs: std::sync::Arc<dyn FileSystem>,
}

fn root_stem(root_file: &Path) -> String {
    root_file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".to_string())
}

/// latexmk 输入参数：相对项目根的完整路径（不能用 stem——嵌套根文件如 `css/thesis.tex`
/// 只取 stem 会跑成 `latexmk thesis.tex`，在项目根下不存在）。统一用正斜杠，避免 Windows
/// 反斜杠在 latexmk 内被当作转义。latexmk jobname 取输入文件名 basename，故产物仍是
/// `tmp/<stem>.pdf`，与 `pdf_dst`/`pdf_src` 的 stem 计算保持一致。
fn latexmk_input(root_file: &Path, project_root: &Path) -> String {
    root_file
        .strip_prefix(project_root)
        .unwrap_or(root_file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 编译命令构造（modules.md §2.6 算法）：cwd = 项目根，相对 input/include 才能解析。
///
/// 抽成独立函数是为了**可单测**（参数与环境变量都能直接断言，不必真的起进程）。
fn compile_command(req: &CompileRequest, kind: CompileKind) -> tokio::process::Command {
    let mut cmd = match kind {
        // Quick：直调引擎一趟（实测比完整 latexmk 快 40% 中位，见 design.md §延迟预算实测附节）。
        // 与 latexmk 的差异只在外层机制与收敛趟；产物路径与 Full 一致（都在 tmp/<stem>.pdf）。
        CompileKind::Quick => {
            let mut c = tokio::process::Command::new(req.engine.binary_name());
            c.arg("-interaction=nonstopmode")
                .arg("-synctex=1")
                .arg(format!("-output-directory={OUT_DIR}"))
                .arg(latexmk_input(&req.root_file, &req.project_root));
            c
        }
        CompileKind::Full => {
            let mut c = tokio::process::Command::new("latexmk");
            c.arg(req.engine.latexmk_flag())
                .arg(format!("-outdir={OUT_DIR}"))
                .arg("-synctex=1")
                .arg("-interaction=nonstopmode")
                .arg(latexmk_input(&req.root_file, &req.project_root));
            c
        }
    };
    // 构建确定性（roadmap ㉚，2026-09 实测）：不设这个变量时，同一份源码连跑两次产出的 PDF
    // **不是**逐字节相同的——差异只在 trailer 的 `/ID`（64 字节），而 XeTeX 用它做**内容无关**的
    // 随机/时间种子。设成固定值后实测两份 PDF SHA-256 完全一致（见 scripts/check-determinism.mjs）。
    // 这是"输出 diff / 只重排变化页"这类优化的前提：字节不同就分不清"真变了"还是"ID 抖了"。
    // 副作用实测：**XeTeX 的 `\today` 不受它影响**（仍打印构建当天日期），PDF 里也不会因此多出
    // `/CreationDate`——所以固定成 0（reproducible-builds 惯例）只影响 ID，不污染文档内容。
    cmd.env("SOURCE_DATE_EPOCH", EPOCH_FOR_REPRODUCIBLE_BUILD);
    cmd
}

/// 固定时间戳（0 = 1970-01-01，reproducible-builds 惯例）：只用来让引擎的 `/ID` 不随时钟抖动。
const EPOCH_FOR_REPRODUCIBLE_BUILD: &str = "0";

#[async_trait]
impl CompileRunner for LatexmkRunner {
    async fn compile(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome {
        let stem = root_stem(&req.root_file);
        let tmp_dir = req.project_root.join(OUT_DIR);
        let pdf_dst = req.project_root.join(format!("{stem}.pdf"));

        // 强度决策（roadmap ㉘）：编辑触发的 Quick 走「直调引擎单趟」；其余走完整 latexmk。
        // 但 **Quick 需要已有构建产物**才有意义（单趟依赖上一趟的 .aux/.toc）；首次编译无 aux 时
        // 单趟会产出「引用全是 ??」的 PDF——故此处自动升级为 Full。
        // 探测失败（权限/竞态）按「无产物」保守处理：升级为 Full 只是更慢，不会出错。
        let had_aux = self.fs.exists(&tmp_dir.join(format!("{stem}.aux"))).await.unwrap_or(false);
        let mut kind = req.kind;
        if kind == CompileKind::Quick && !had_aux {
            debug!(stem, "无构建产物，Quick 升级为 Full（首编）");
            kind = CompileKind::Full;
        }

        // 命令构造（modules.md §2.6 算法）：cwd = 项目根，相对 input/include 才能解析
        let mut cmd = compile_command(&req, kind);
        match kind {
            CompileKind::Quick => debug!(engine = req.engine.binary_name(), "Quick 编译（单趟直调引擎）"),
            CompileKind::Full => debug!(engine = req.engine.latexmk_flag(), "Full 编译（完整 latexmk 收敛）"),
        }
        cmd.current_dir(&req.project_root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            // 进程孤儿防护：若编译 future 被丢弃（应用退出/任务取消），随 future 一并杀掉子进程。
            // 避免子进程残留、持续写 tmp/ 或占用 PDF 锁。（树杀仍由 kill_tree 负责，这里是兜底。）
            .kill_on_drop(true);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return CompileOutcome::IoError {
                    message: match kind {
                        CompileKind::Quick =>
                            format!("无法启动 {}（TeX Live 未安装？）：{e}", req.engine.binary_name()),
                        CompileKind::Full => format!("无法启动 latexmk（TeX Live 未安装？）：{e}"),
                    },
                }
            }
        };
        let pid = child.id();

        let outcome = tokio::select! {
            _ = tokio::time::sleep(req.timeout) => {
                if let Some(pid) = pid {
                    warn!(pid, "编译超时（{}s），树杀进程", req.timeout.as_secs());
                    kill_tree(pid);
                }
                // roadmap ㉕：超时不静默重试，改为**现场采集证据**给出可操作诊断
                let entry = self.timeout_entry(&req, &stem, &tmp_dir, had_aux).await;
                CompileOutcome::Timeout { entry }
            }
            _ = cancel.cancelled() => {
                if let Some(pid) = pid {
                    warn!(pid, "收到手动终止，树杀进程");
                    kill_tree(pid);
                }
                CompileOutcome::Aborted
            }
            status = child.wait() => match status {
                Ok(s) if s.success() => {
                    // 成功：tmp/<stem>.pdf → 项目根（design.md 产物位置）
                    // 原子拷贝：先写临时文件再 rename，失败时旧 PDF 保留（不被截断/损坏）。
                    let pdf_src = tmp_dir.join(format!("{stem}.pdf"));
                    let pdf_tmp = pdf_dst.with_extension("pdf.tmp");
                    let copy = async {
                        tokio::fs::copy(&pdf_src, &pdf_tmp).await?;
                        tokio::fs::rename(&pdf_tmp, &pdf_dst).await
                    };
                    match copy.await {
                        Ok(_) => CompileOutcome::Success {
                            pdf_path: pdf_dst,
                            kind,
                        },
                        Err(e) => {
                            let _ = tokio::fs::remove_file(&pdf_tmp).await;
                            CompileOutcome::IoError {
                                message: format!("PDF 拷贝失败（{} → {}）：{e}", pdf_src.display(), pdf_dst.display()),
                            }
                        }
                    }
                }
                Ok(_) => {
                    // 非零退出：.log 是权威（modules.md §4：不做流式输出）
                    // 用**容错解码**读取：GBK 源文件经 pdflatex 会把非法 UTF-8 字节写进日志，
                    // 严格读取会导致「编译失败却拿不到任何错误信息」（2026-09 实测，roadmap P0-①）。
                    let log_path = tmp_dir.join(format!("{stem}.log"));
                    match self.fs.read_to_string_lossy(&log_path).await {
                        Ok(text) => {
                            let errors: Vec<ErrorEntry> = parse_log(&text)
                                .into_iter()
                                .map(|m| {
                                    // 诊断在移动 message 之前算（roadmap ④）
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
                            // roadmap ㉗：缺 .cls 且项目里确有 .ins/.dtx 时，把泛化建议换成具体命令
                            let errors = self.enrich_source_release_hints(errors, &req.project_root).await;
                            let diagnosed = errors.iter().filter(|e| e.diagnosis.is_some()).count();
                            debug!(
                                count = errors.len(),
                                diagnosed,
                                log = %log_path.display(),
                                "编译失败：已从 .log 解析出错误条目"
                            );
                            CompileOutcome::ContentError { errors }
                        }
                        Err(e) => {
                            warn!(log = %log_path.display(), "编译失败且无法读取日志：{e}");
                            CompileOutcome::IoError {
                                message: format!("编译失败且无法读取日志（{}）：{e}", log_path.display()),
                            }
                        }
                    }
                }
                Err(e) => CompileOutcome::IoError {
                    message: format!("等待 latexmk 失败：{e}"),
                },
            },
        };
        outcome
    }
}

impl LatexmkRunner {
    /// 源码版模板提示（roadmap ㉗）：编译报"缺 `.cls`"时，看看项目里是不是只有 `.ins`/`.dtx`。
    ///
    /// 判据是**看得见的事实**：项目根目录里存在哪些 `.ins`/`.dtx` 文件（源码版模板的发布形态）。
    /// 具体的"该编哪个文件"由 core 的 [`source_release_hint`] 决定（同名优先、单一候选次之、
    /// 拿不准就不改建议）。
    async fn enrich_source_release_hints(
        &self,
        mut errors: Vec<ErrorEntry>,
        project_root: &Path,
    ) -> Vec<ErrorEntry> {
        let needs_lookup = errors.iter().any(|e| {
            e.diagnosis
                .as_ref()
                .is_some_and(|d| d.kind == DiagnosisKind::MissingClass && d.missing_file.is_some())
        });
        if !needs_lookup {
            return errors;
        }
        // 只扫项目根（源码版模板的 .ins 就在根目录；扫全树代价大且没必要）
        let candidates: Vec<String> = match self.fs.read_dir(project_root).await {
            Ok(entries) => entries
                .into_iter()
                .filter(|e| !e.is_dir)
                .filter_map(|e| e.path.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect(),
            Err(e) => {
                debug!(root = %project_root.display(), "读项目根失败，跳过源码版模板识别：{e}");
                return errors;
            }
        };
        for entry in &mut errors {
            let Some(diag) = entry.diagnosis.as_mut() else { continue };
            if diag.kind != DiagnosisKind::MissingClass {
                continue;
            }
            let Some(missing) = diag.missing_file.clone() else { continue };
            if let Some(hint) = source_release_hint(&missing, &candidates) {
                debug!(missing = %missing, "识别为源码版模板，替换为具体编译命令");
                diag.hint = Some(hint);
            }
        }
        errors
    }

    /// 超时错误条目（roadmap ㉕）：采集**可观察证据** → 证据化诊断 → [`ErrorEntry`]。
    ///
    /// 证据三条：① 本次是否首编（开始时有无 `.aux`）；② 项目 `.tex` 源文件数；
    /// ③ `.log` 里已排版到第几页（推进证据，见 [`pages_typeset`]）。
    /// 三项都是"看得见的事实"，诊断文案不据此猜测因果（宁可说"可能"，也不断言"卡住"）。
    /// 日志另交给 [`diagnose_timeout`]：里面若已有致命错误，超时就只是症状（提高超时救不了）。
    async fn timeout_entry(
        &self,
        req: &CompileRequest,
        stem: &str,
        tmp_dir: &Path,
        had_aux: bool,
    ) -> ErrorEntry {
        let log_path = tmp_dir.join(format!("{stem}.log"));
        // 树杀刚发出，日志可能只落盘了一部分；读得到多少算多少（best-effort）
        let log = self.fs.read_to_string_lossy(&log_path).await.unwrap_or_default();
        let pages = pages_typeset(&log);
        let tex_files = collect_tex_files(self.fs.as_ref(), &req.project_root)
            .await
            .ok()
            .map(|v| v.len());
        let timeout_secs = u32::try_from(req.timeout.as_secs()).unwrap_or(u32::MAX);

        let evidence = TimeoutEvidence {
            timeout_secs,
            first_build: !had_aux,
            tex_files,
        };
        // 诊断看两样东西：本次运行的日志（里面可能已有致命错误 → 超时只是症状）与上面的证据
        let diagnosis = diagnose_timeout(&log, &evidence);
        debug!(
            timeout_secs,
            first_build = evidence.first_build,
            tex_files = ?tex_files,
            pages = ?pages,
            kind = ?diagnosis.kind,
            "编译超时，已采集诊断证据"
        );

        // message 保留"机器看到的事实"，诊断给"人话原因 + 怎么改"（前端首行显示诊断原因，
        // 这条 message 落到 title 里，供排查时对照）
        let mut facts = vec![if had_aux { "增量编译".to_string() } else { "首次编译".to_string() }];
        if let Some(n) = tex_files {
            facts.push(format!("{n} 个 .tex 源文件"));
        }
        if let Some(p) = pages {
            facts.push(format!("日志已排版到第 {p} 页"));
        } else {
            facts.push("日志无页输出".to_string());
        }
        ErrorEntry {
            message: format!(
                "编译超过 {timeout_secs}s 上限，已强制终止（进程树已杀）。证据：{}",
                facts.join("，")
            ),
            file: None,
            line: None,
            kind: ErrorKind::Timeout,
            diagnosis: Some(diagnosis),
        }
    }
}

/// 进程树杀（modules.md §2.6）：
/// Windows：`taskkill /T /F /PID`（Child::kill 只杀直接子进程，latexmk 的子进程会存活）。
/// Unix：先试进程组（-pid），失败再试直接 kill（best-effort，与 LaTeX Workshop 相同的已知局限）。
fn kill_tree(pid: u32) {
    // 阻塞的进程/进程组调用移到阻塞线程池，避免卡住 async 执行器（超时/取消路径的热路径）。
    // 结果 best-effort，丢弃 JoinHandle（仅作后台树杀，不等待）。
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/T", "/F", "/PID", &pid.to_string()])
                .status();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let group = std::process::Command::new("kill")
                .args(["-9", &format!("-{pid}")])
                .status();
            if group.map(|s| !s.success()).unwrap_or(true) {
                let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
            }
        }
    });
}
#[cfg(test)]
mod tests {
    //! 真实环境集成测试（`#[ignore]`：需要系统安装 latexmk + synctex）。
    //!
    //! 在有 TeX Live/MiKTeX 的机器上运行：`cargo test -p texpresso-infra -- --ignored`
    //! 这是 Windows 验收清单的自动化抓手（modules.md §8），验证：
    //! - latexmk 真实编译全链路（命令构造/产物位置/PDF 拷贝）；
    //! - 内容错误 → .log 解析链路；
    //! - 超时树杀 / 取消终止；
    //! - SyncTeX CLI 输出契约（ADR-0008 最大风险点）。

    use super::*;
    use crate::synctex::SyncTexCli;
    use std::path::PathBuf;
    use std::time::Duration;
    use texpresso_core::synctex::{SourcePosition, SyncTexProvider, SyncTexPosition};

    struct TempProject {
        dir: std::path::PathBuf,
    }

    // ---- 命令构造（不需要 latexmk，任何机器都能跑）----

    fn sample_req() -> CompileRequest {
        CompileRequest {
            root_file: PathBuf::from("/proj/css/thesis.tex"),
            project_root: PathBuf::from("/proj"),
            engine: texpresso_core::types::Engine::XeLaTeX,
            timeout: Duration::from_secs(120),
            kind: CompileKind::Quick,
        }
    }

    fn argv(cmd: &tokio::process::Command) -> Vec<String> {
        cmd.as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn quick_command_calls_engine_directly() {
        let cmd = compile_command(&sample_req(), CompileKind::Quick);
        assert_eq!(cmd.as_std().get_program().to_string_lossy(), "xelatex");
        assert_eq!(
            argv(&cmd),
            vec![
                "-interaction=nonstopmode",
                "-synctex=1",
                "-output-directory=tmp",
                // 嵌套根文件用**相对项目根的完整路径**（只取 stem 会在项目根下找不到文件）
                "css/thesis.tex",
            ]
        );
    }

    #[test]
    fn full_command_uses_latexmk_with_engine_flag() {
        let mut req = sample_req();
        req.engine = texpresso_core::types::Engine::LuaLaTeX;
        let cmd = compile_command(&req, CompileKind::Full);
        assert_eq!(cmd.as_std().get_program().to_string_lossy(), "latexmk");
        assert_eq!(
            argv(&cmd),
            vec![
                "-lualatex",
                "-outdir=tmp",
                "-synctex=1",
                "-interaction=nonstopmode",
                "css/thesis.tex",
            ]
        );
    }

    #[test]
    fn both_paths_pin_source_date_epoch_for_reproducible_pdf() {
        // roadmap ㉚：不固定这个变量时，同一份源码两次编译的 PDF 只差 trailer 的 /ID（实测 64 字节），
        // 会让"输出 diff / 只重排变化页"分不清"真变了"和"ID 抖了"。
        for kind in [CompileKind::Quick, CompileKind::Full] {
            let cmd = compile_command(&sample_req(), kind);
            let epoch = cmd
                .as_std()
                .get_envs()
                .find(|(k, _)| k.to_string_lossy() == "SOURCE_DATE_EPOCH")
                .map(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()));
            assert_eq!(epoch, Some(Some("0".to_string())), "{kind:?} 路径必须固定 SOURCE_DATE_EPOCH");
        }
    }

    impl TempProject {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("texpresso-it-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("创建临时项目失败");
            Self { dir }
        }

        fn put(&self, rel: &str, content: &str) {
            let p = self.dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, content).unwrap();
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn latexmk_available() -> bool {
        std::process::Command::new("latexmk")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn req(project: &TempProject) -> CompileRequest {
        CompileRequest {
            root_file: project.dir.join("main.tex"),
            project_root: project.dir.clone(),
            engine: texpresso_core::types::Engine::XeLaTeX,
            timeout: Duration::from_secs(60),
            kind: CompileKind::Full,
        }
    }

    /// 成功路径：编译 → PDF 拷贝到项目根 + tmp/ 中间文件 + SyncTeX 双向可用。
    #[tokio::test]
    #[ignore]
    async fn compile_success_and_synctex() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("success");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nHello TeXPresso\n\\end{document}\n",
        );

        let runner = LatexmkRunner {
            fs: std::sync::Arc::new(crate::fs::TokioFs),
        };
        let outcome = runner
            .compile(req(&project), tokio_util::sync::CancellationToken::new())
            .await;

        match outcome {
            CompileOutcome::Success { pdf_path, .. } => {
                assert_eq!(pdf_path, project.dir.join("main.pdf"));
                assert!(pdf_path.exists(), "PDF 应拷贝到项目根");
                assert!(project.dir.join("tmp/main.log").exists(), "中间文件应收纳在 tmp/");
                assert!(
                    project.dir.join("tmp/main.synctex.gz").exists(),
                    "-synctex=1 应产出 synctex 文件"
                );

                // SyncTeX CLI 输出契约实测（ADR-0008 风险落地）
                if std::process::Command::new("synctex").arg("--version").output().is_err() {
                    eprintln!("跳过 SyncTeX 断言：未安装 synctex");
                    return;
                }
                let cli = SyncTexCli;
                let pos = cli
                    .forward(
                        &SourcePosition {
                            file: project.dir.join("main.tex"),
                            line: 3,
                            column: 1,
                        },
                        &pdf_path,
                    )
                    .await
                    .expect("正向定位应成功");
                assert!(pos.page >= 1);

                let back = cli
                    .inverse(
                        &SyncTexPosition { page: pos.page, x: pos.x, y: pos.y },
                        &pdf_path,
                    )
                    .await
                    .expect("反向定位应成功");
                assert!(back.line >= 1);
            }
            other => panic!("预期成功，得到：{other:?}"),
        }
    }

    /// Quick 路径（roadmap ㉘）：已有构建产物时直调引擎单趟，命令不经 latexmk。
    ///
    /// 断言「真的跑了引擎」而非「请求被接受」：先 Full 建产物（含 .aux/.toc），
    /// 再改正文源码走 Quick → Success 必须报 `kind: Quick`，且 PDF/SyncTeX 齐全
    /// （后续编辑期的双向定位依赖它）。
    #[tokio::test]
    #[ignore]
    async fn quick_path_skips_latexmk_and_reports_quick() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("quick");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\tableofcontents\n\\section{One}\\label{s:one}\nSee \\ref{s:one}.\n\\end{document}\n",
        );
        let runner = LatexmkRunner {
            fs: std::sync::Arc::new(crate::fs::TokioFs),
        };
        let cancel = || tokio_util::sync::CancellationToken::new();

        // 1) 首编 = Full（建 tmp/main.aux，Quick 的前置条件）
        let first = runner.compile(req(&project), cancel()).await;
        assert!(
            matches!(first, CompileOutcome::Success { kind: CompileKind::Full, .. }),
            "首编应为 Full，实得：{first:?}"
        );
        assert!(project.dir.join("tmp/main.aux").exists(), "Full 应产出 .aux");

        // 2) 编辑正文 → Quick（单趟直调 xelatex）
        // 记录 latexmk 依赖库 mtime：Quick 若误走 latexmk，这一趟必然刷新它。
        let fdb = project.dir.join("tmp/main.fdb_latexmk");
        let fdb_before = std::fs::metadata(&fdb).and_then(|m| m.modified()).ok();
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\tableofcontents\n\\section{One}\\label{s:one}\nSee \\ref{s:one}.\nEdited.\n\\end{document}\n",
        );
        let mut quick = req(&project);
        quick.kind = CompileKind::Quick;
        let second = runner.compile(quick, cancel()).await;
        assert!(
            matches!(second, CompileOutcome::Success { kind: CompileKind::Quick, .. }),
            "有产物时编辑触发应跑单趟并报 Quick，实得：{second:?}"
        );
        assert!(project.dir.join("main.pdf").exists(), "Quick 也应产出 PDF");
        assert!(
            project.dir.join("tmp/main.synctex.gz").exists(),
            "Quick 也应产出 synctex（定位不能因强度降级而失效）"
        );
        assert_eq!(
            std::fs::metadata(&fdb).and_then(|m| m.modified()).ok(),
            fdb_before,
            "Quick 不该动 latexmk 依赖库（.fdb_latexmk mtime 变化 = 误走 latexmk）"
        );
    }

    /// Quick 前置条件缺失（无 tmp/<stem>.aux）→ runner 自动升级 Full，避免引用全是 `??`。
    #[tokio::test]
    #[ignore]
    async fn quick_upgrades_to_full_without_artifacts() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("quick-upgrade");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\tableofcontents\n\\section{One}\\label{s:one}\nSee \\ref{s:one}.\n\\end{document}\n",
        );
        let runner = LatexmkRunner {
            fs: std::sync::Arc::new(crate::fs::TokioFs),
        };
        let mut quick = req(&project);
        quick.kind = CompileKind::Quick;
        let outcome = runner.compile(quick, tokio_util::sync::CancellationToken::new()).await;

        match outcome {
            CompileOutcome::Success { kind, .. } => {
                assert_eq!(kind, CompileKind::Full, "无产物时必须升级为 Full");
                // 交叉引用必须已解析（单趟会留下 ??）
                let pdf = std::fs::read(project.dir.join("main.pdf")).expect("应产出 PDF");
                let log = std::fs::read_to_string(project.dir.join("tmp/main.log")).unwrap();
                assert!(
                    !log.contains("There were undefined references"),
                    "升级为 Full 后不应有未解析引用；PDF {} 字节",
                    pdf.len()
                );
            }
            other => panic!("预期成功，得到：{other:?}"),
        }
    }

    /// 内容错误路径：非零退出 → .log 解析 → ContentError。
    #[tokio::test]
    #[ignore]
    async fn compile_content_error() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("error");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\undefinedcommandhere\n\\end{document}\n",
        );

        let runner = LatexmkRunner {
            fs: std::sync::Arc::new(crate::fs::TokioFs),
        };
        let outcome = runner
            .compile(req(&project), tokio_util::sync::CancellationToken::new())
            .await;

        match outcome {
            CompileOutcome::ContentError { errors } => {
                assert!(!errors.is_empty(), "应有解析出的错误条目");
                assert!(errors.iter().any(|e| e.line.is_some()), "错误应带行号：{errors:?}");
            }
            other => panic!("预期内容错误，得到：{other:?}"),
        }
    }

    /// 超时路径：1ms 超时必触发 → 树杀 → Timeout + **证据化条目**（roadmap ㉕）。
    #[tokio::test]
    #[ignore]
    async fn compile_timeout_kills_tree_and_reports_evidence() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("timeout");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nBig\n\\end{document}\n",
        );

        let runner = LatexmkRunner {
            fs: std::sync::Arc::new(crate::fs::TokioFs),
        };
        let mut request = req(&project);
        request.timeout = Duration::from_millis(1); // 必超时
        let outcome = runner
            .compile(request, tokio_util::sync::CancellationToken::new())
            .await;

        match outcome {
            CompileOutcome::Timeout { entry } => {
                // 超时必须带「原因 + 怎么改」——这正是 ㉕ 要修的东西（此前超时不给任何条目）
                assert_eq!(entry.kind, ErrorKind::Timeout);
                let diag = entry.diagnosis.expect("超时条目必须带诊断");
                // 1ms 超时时项目还没有 .aux → 判定为首编 → 直接建议到 900s（避免"注定不够"的一轮）
                let suggested = diag.suggested_timeout_secs.expect("应给出一键重试的超时值");
                assert_eq!(suggested, 900, "首编应跳档到 900s：{diag:?}");
                assert!(
                    entry.message.contains("1s") || entry.message.contains("0s"),
                    "消息应含实际上限：{}",
                    entry.message
                );
                // 树没被杀干净的话这里会留下 latexmk 子进程（kill_tree 的既有断言在别的用例里）
            }
            other => panic!("预期超时，得到：{other:?}"),
        }
    }

    /// 取消路径：提前取消 → Aborted。
    #[tokio::test]
    #[ignore]
    async fn compile_cancel_aborts() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("cancel");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nBig\n\\end{document}\n",
        );

        let runner = LatexmkRunner {
            fs: std::sync::Arc::new(crate::fs::TokioFs),
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel(); // 立即取消
        let outcome = runner.compile(req(&project), cancel).await;
        assert_eq!(outcome, CompileOutcome::Aborted);
    }

    #[test]
    fn root_stem_variants() {
        assert_eq!(root_stem(Path::new(r"C:\proj\main.tex")), "main");
        assert_eq!(root_stem(Path::new("css/thesis.tex")), "thesis");
        assert_eq!(root_stem(Path::new("main")), "main"); // 无扩展名回退
    }

    #[test]
    fn latexmk_input_keeps_nested_relative_path() {
        // H2 回归：嵌套根文件必须用相对项目的完整路径，而非仅 stem（否则 latexmk 在项目根下找不到文件）
        let root = Path::new(r"C:\proj");
        assert_eq!(
            latexmk_input(Path::new(r"C:\proj\css\thesis.tex"), root),
            "css/thesis.tex"
        );
        assert_eq!(
            latexmk_input(Path::new(r"C:\proj\main.tex"), root),
            "main.tex"
        );
        // 不在项目根下 → 回退到完整路径（统一正斜杠）
        assert_eq!(
            latexmk_input(Path::new(r"D:\other\a.tex"), root),
            "D:/other/a.tex"
        );
    }

    // ---------------------------------------------------------------- 中文/非 ASCII 路径（roadmap P0-①）

    #[test]
    fn chinese_path_input_and_stem() {
        // 传给 latexmk 的输入必须是相对项目的正斜杠路径，中文原样保留（不得被转义/丢字）
        let root = Path::new(r"E:\项目\中文测试工程");
        assert_eq!(
            latexmk_input(Path::new(r"E:\项目\中文测试工程\中文主文件.tex"), root),
            "中文主文件.tex"
        );
        assert_eq!(
            latexmk_input(Path::new(r"E:\项目\中文测试工程\章节\第一章.tex"), root),
            "章节/第一章.tex"
        );
        // 产物名沿用 basename 的 stem，中文 stem 需与 pdf_dst 计算一致
        assert_eq!(root_stem(Path::new(r"E:\项目\中文测试工程\中文主文件.tex")), "中文主文件");
    }

    /// 中文路径全链路（真实 latexmk + synctex）：中文目录 + 中文文件名 + 中文子目录。
    ///
    /// 实测基线（2026-09，TeX Live 2026 / Windows，CP65001 与 CP936 均验证）：
    /// 编译 exit 0、`tmp/` 产出 `.log`/`.synctex.gz`、PDF 正常，synctex 双向可用且中文源路径可回传。
    #[tokio::test]
    #[ignore]
    async fn compile_chinese_paths_with_synctex() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        // 目录名含中文（temp_dir()/texpresso-it-中文目录-<pid>）
        let project = TempProject::new("中文目录");
        project.put(
            "中文主文件.tex",
            "\\documentclass[UTF8]{ctexart}\n\\begin{document}\n\\include{章节/第一章}\n\\end{document}\n",
        );
        project.put("章节/第一章.tex", "\\section{子文件章节}\n中文正文。\n");

        let runner = LatexmkRunner {
            fs: std::sync::Arc::new(crate::fs::TokioFs),
        };
        let request = CompileRequest {
            root_file: project.dir.join("中文主文件.tex"),
            project_root: project.dir.clone(),
            engine: texpresso_core::types::Engine::XeLaTeX,
            timeout: Duration::from_secs(60),
            kind: CompileKind::Full,
        };
        let outcome = runner
            .compile(request, tokio_util::sync::CancellationToken::new())
            .await;

        let pdf_path = match outcome {
            CompileOutcome::Success { pdf_path, .. } => pdf_path,
            other => panic!("中文路径应编译成功，实得：{other:?}"),
        };
        assert_eq!(pdf_path, project.dir.join("中文主文件.pdf"));
        assert!(pdf_path.exists(), "PDF 应拷贝到项目根（中文名）");
        assert!(
            project.dir.join("tmp/中文主文件.log").exists(),
            "中文名的 .log 应落在 tmp/"
        );
        assert!(
            project.dir.join("tmp/中文主文件.synctex.gz").exists(),
            "中文名的 synctex 数据应产出"
        );

        if std::process::Command::new("synctex").arg("--version").output().is_err() {
            eprintln!("跳过 SyncTeX 断言：未安装 synctex");
            return;
        }
        let cli = SyncTexCli;
        // 正向：从中文子文件定位 → 页码有效
        let pos = cli
            .forward(
                &SourcePosition {
                    file: project.dir.join("章节/第一章.tex"),
                    line: 1,
                    column: 1,
                },
                &pdf_path,
            )
            .await
            .expect("中文路径正向定位应成功");
        assert!(pos.page >= 1);

        // 反向：PDF → 源码，回传的**中文文件名必须完好**（这是跳回源码的判据）
        let back = cli
            .inverse(
                &SyncTexPosition {
                    page: pos.page,
                    x: pos.x,
                    y: pos.y,
                },
                &pdf_path,
            )
            .await
            .expect("中文路径反向定位应成功");
        let back_name = back
            .file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        assert!(
            back_name == "第一章.tex" || back_name == "中文主文件.tex",
            "反向定位应回传中文源文件名，实得：{}",
            back.file.display()
        );
    }

    /// 中文路径的内容错误：`.log` 解析须带出中文文件名 + 行号（错误列表点击跳转的判据）。
    #[tokio::test]
    #[ignore]
    async fn compile_chinese_path_content_error_keeps_file_and_line() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("中文错误");
        project.put(
            "中文主文件.tex",
            "\\documentclass{article}\n\\begin{document}\n\\undefinedcommandhere\n\\end{document}\n",
        );

        let runner = LatexmkRunner {
            fs: std::sync::Arc::new(crate::fs::TokioFs),
        };
        // 必须编译中文根文件本身：req() 造的是 main.tex，项目里并不存在，
        // latexmk 会因缺少输入而无 .log 可解析（原用例恒失败，2026-09 修正）。
        let request = CompileRequest {
            root_file: project.dir.join("中文主文件.tex"),
            project_root: project.dir.clone(),
            engine: texpresso_core::types::Engine::XeLaTeX,
            timeout: Duration::from_secs(60),
            kind: CompileKind::Full,
        };
        let outcome = runner
            .compile(request, tokio_util::sync::CancellationToken::new())
            .await;

        match outcome {
            CompileOutcome::ContentError { errors } => {
                assert!(!errors.is_empty(), "应有解析出的错误条目");
                let with_line = errors
                    .iter()
                    .find(|e| e.line.is_some())
                    .expect("错误应带行号");
                assert_eq!(with_line.line, Some(3));
                assert!(
                    with_line
                        .file
                        .as_deref()
                        .map(|f| f.contains("中文主文件.tex"))
                        .unwrap_or(false),
                    "错误条目应带中文文件名，实得：{:?}",
                    with_line.file
                );
            }
            other => panic!("预期内容错误，得到：{other:?}"),
        }
    }
}
