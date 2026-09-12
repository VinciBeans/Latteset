//! Latexmk 执行器（modules.md §2.6 / 设计决策 D2）。
//!
//! 超时检测、进程树杀、PDF 拷贝全在这里；调度器无时钟、无进程概念。

use async_trait::async_trait;
use std::path::Path;
use texpresso_core::log_parser::parse_log;
use texpresso_core::project::FileSystem;
use texpresso_core::scheduler::CompileRunner;
use texpresso_core::types::{CompileOutcome, CompileRequest, ErrorEntry, ErrorKind};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// 编译中间目录（design.md：统一收纳 tmp/，与 core 忽略规则一致）。
const OUT_DIR: &str = "tmp";

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

#[async_trait]
impl CompileRunner for LatexmkRunner {
    async fn compile(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome {
        let stem = root_stem(&req.root_file);
        let tmp_dir = req.project_root.join(OUT_DIR);
        let pdf_dst = req.project_root.join(format!("{stem}.pdf"));

        // 命令构造（modules.md §2.6 算法）：cwd = 项目根，相对 input/include 才能解析
        let mut cmd = tokio::process::Command::new("latexmk");
        cmd.arg(req.engine.latexmk_flag())
            .arg(format!("-outdir={OUT_DIR}"))
            .arg("-synctex=1")
            .arg("-interaction=nonstopmode")
            .arg(latexmk_input(&req.root_file, &req.project_root))
            .current_dir(&req.project_root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            // 进程孤儿防护：若编译 future 被丢弃（应用退出/任务取消），随 future 一并杀掉 latexmk。
            // 避免子进程残留、持续写 tmp/ 或占用 PDF 锁。（树杀仍由 kill_tree 负责，这里是兜底。）
            .kill_on_drop(true);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return CompileOutcome::IoError {
                    message: format!("无法启动 latexmk（TeX Live 未安装？）：{e}"),
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
                CompileOutcome::Timeout
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
                        Ok(_) => CompileOutcome::Success { pdf_path: pdf_dst },
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
                                .map(|m| ErrorEntry {
                                    message: m.message,
                                    file: m.file,
                                    line: m.line,
                                    kind: ErrorKind::ContentError,
                                })
                                .collect();
                            debug!(
                                count = errors.len(),
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
    //! 在有 TeX Live/MiKTeX 的机器上运行：`cargo test -p texpresso -- --ignored`
    //! 这是 Windows 验收清单的自动化抓手（modules.md §8），验证：
    //! - latexmk 真实编译全链路（命令构造/产物位置/PDF 拷贝）；
    //! - 内容错误 → .log 解析链路；
    //! - 超时树杀 / 取消终止；
    //! - SyncTeX CLI 输出契约（ADR-0008 最大风险点）。

    use super::*;
    use crate::sync_cli::SyncTexCli;
    use std::time::Duration;
    use texpresso_core::synctex::{SourcePosition, SyncTexProvider, SyncTexPosition};

    struct TempProject {
        dir: std::path::PathBuf,
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
            fs: std::sync::Arc::new(crate::fs_impl::TokioFs),
        };
        let outcome = runner
            .compile(req(&project), tokio_util::sync::CancellationToken::new())
            .await;

        match outcome {
            CompileOutcome::Success { pdf_path } => {
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
            fs: std::sync::Arc::new(crate::fs_impl::TokioFs),
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

    /// 超时路径：1ms 超时必触发 → 树杀 → Timeout。
    #[tokio::test]
    #[ignore]
    async fn compile_timeout_kills_tree() {
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
            fs: std::sync::Arc::new(crate::fs_impl::TokioFs),
        };
        let mut request = req(&project);
        request.timeout = Duration::from_millis(1); // 必超时
        let outcome = runner
            .compile(request, tokio_util::sync::CancellationToken::new())
            .await;
        assert_eq!(outcome, CompileOutcome::Timeout);
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
            fs: std::sync::Arc::new(crate::fs_impl::TokioFs),
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
            fs: std::sync::Arc::new(crate::fs_impl::TokioFs),
        };
        let request = CompileRequest {
            root_file: project.dir.join("中文主文件.tex"),
            project_root: project.dir.clone(),
            engine: texpresso_core::types::Engine::XeLaTeX,
            timeout: Duration::from_secs(60),
        };
        let outcome = runner
            .compile(request, tokio_util::sync::CancellationToken::new())
            .await;

        let pdf_path = match outcome {
            CompileOutcome::Success { pdf_path } => pdf_path,
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
            fs: std::sync::Arc::new(crate::fs_impl::TokioFs),
        };
        let outcome = runner
            .compile(req(&project), tokio_util::sync::CancellationToken::new())
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
