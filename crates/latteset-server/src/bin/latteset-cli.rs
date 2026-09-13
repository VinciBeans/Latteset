//! `latteset-cli`：headless 命令行入口（roadmap ⑥）。
//!
//! 约定（面向 Agent/脚本）：
//! - **stdout 只有 JSON**（一条命令一个 JSON 对象）；日志走 stderr。
//! - 退出码：0 = 命令成功（`compile` 仅当 `status == "success"`）；1 = 编译未通过；2 = 用法/路径类错误；3 = 内部错误。
//! - 每个进程只跑一条命令（一次性语义）：不 spawn watcher、不跑调度器。
//!
//! **为什么不用 clap**：本 crate 要保持"零新增依赖"（沙箱/离线环境也能构建），而 CLI 参数面很小——
//! 手写解析在这里 ≈80 行且可单测（见文件末尾的 tests），比引一个参数解析框架更划算。

use std::path::PathBuf;
use latteset_server::{ensure_open, ServerError, Session};

const USAGE: &str = "\
latteset-cli —— Latteset headless CLI（读 → 改 → 编译验证 → 修）

用法：
  latteset-cli [--project <目录>] [--config-dir <目录>] <命令> [参数]

命令：
  open                              打开项目：根文件 / 候选 / 生效设置
  compile [--quick]                 编译一次并等待结果（默认完整 latexmk；--quick 单趟直调引擎）
  status                            最近一次编译的状态（本进程内）
  errors                            最近一次编译的结构化错误（含「原因 + 怎么改」诊断）
  outline                           文档大纲（章/节 → 文件:行号）
  tree [--all]                      项目文件列表（默认只列 .tex）
  read <路径>                       读文件
  write <路径> <内容|->             写文件（`-` = 从 stdin 读）
  forward <文件> <行> [--column N]  源码位置 → PDF 页码/坐标
  inverse <页> [--x N] [--y N]      PDF 坐标 → 源码位置
  settings                          生效设置
  help                              本帮助

全局参数：
  --project <目录>     项目目录（省略则用当前目录）
  --config-dir <目录>  应用配置目录（默认与 GUI 相同；也可用环境变量 LATTESET_CONFIG_DIR）
";

#[derive(Debug, PartialEq)]
enum Cmd {
    Help,
    Open,
    Compile { quick: bool },
    Status,
    Errors,
    Outline,
    Tree { all: bool },
    Read { path: String },
    Write { path: String, content: String },
    Forward { file: String, line: u32, column: u32 },
    Inverse { page: u32, x: f32, y: f32 },
    Settings,
}

#[derive(Debug, PartialEq)]
struct Args {
    project: Option<PathBuf>,
    config_dir: Option<PathBuf>,
    cmd: Cmd,
}

fn parse(raw: &[String]) -> Result<Args, String> {
    let mut rest: Vec<String> = Vec::new();
    let mut project = None;
    let mut config_dir = None;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--project" => {
                project = Some(PathBuf::from(take_value(raw, &mut i, "--project")?));
            }
            "--config-dir" => {
                config_dir = Some(PathBuf::from(take_value(raw, &mut i, "--config-dir")?));
            }
            "--help" | "-h" => return Ok(Args { project, config_dir, cmd: Cmd::Help }),
            other => rest.push(other.to_string()),
        }
        i += 1;
    }

    let Some(name) = rest.first().cloned() else {
        return Ok(Args { project, config_dir, cmd: Cmd::Help });
    };
    let tail = &rest[1..];
    // 命令内的小旗标（位置无关，值型参数单独取）
    let has = |flag: &str| tail.iter().any(|a| a == flag);
    let value_of = |flag: &str| -> Option<String> {
        tail.iter().position(|a| a == flag).and_then(|p| tail.get(p + 1)).cloned()
    };
    let positional: Vec<String> = {
        let mut out = Vec::new();
        let mut skip = false;
        for (idx, a) in tail.iter().enumerate() {
            if skip {
                skip = false;
                continue;
            }
            if a.starts_with("--") {
                // 值型旗标吃掉下一个 token
                if matches!(a.as_str(), "--column" | "--x" | "--y") && tail.get(idx + 1).is_some() {
                    skip = true;
                }
                continue;
            }
            out.push(a.clone());
        }
        out
    };
    let number = |flag: &str, dflt: f32| -> Result<f32, String> {
        match value_of(flag) {
            Some(v) => v.parse::<f32>().map_err(|_| format!("{flag} 需要一个数字，得到 {v}")),
            None => Ok(dflt),
        }
    };

    let cmd = match name.as_str() {
        "help" => Cmd::Help,
        "open" => Cmd::Open,
        "compile" => Cmd::Compile { quick: has("--quick") },
        "status" => Cmd::Status,
        "errors" => Cmd::Errors,
        "outline" => Cmd::Outline,
        "tree" => Cmd::Tree { all: has("--all") },
        "read" => Cmd::Read {
            path: positional.first().cloned().ok_or("read 需要 <路径>")?,
        },
        "write" => {
            let path = positional.first().cloned().ok_or("write 需要 <路径>")?;
            let content = positional.get(1).cloned().ok_or("write 需要 <内容>（用 - 从 stdin 读）")?;
            Cmd::Write { path, content }
        }
        "forward" => {
            let file = positional.first().cloned().ok_or("forward 需要 <文件>")?;
            let line: u32 = positional
                .get(1)
                .ok_or("forward 需要 <行号>")?
                .parse()
                .map_err(|_| "forward 的 <行号> 必须是整数".to_string())?;
            Cmd::Forward {
                file,
                line,
                column: number("--column", 1.0)? as u32,
            }
        }
        "inverse" => {
            let page: u32 = positional
                .first()
                .ok_or("inverse 需要 <页号>")?
                .parse()
                .map_err(|_| "inverse 的 <页号> 必须是整数".to_string())?;
            Cmd::Inverse {
                page,
                x: number("--x", 0.0)?,
                y: number("--y", 0.0)?,
            }
        }
        "settings" => Cmd::Settings,
        other => return Err(format!("未知命令：{other}")),
    };
    Ok(Args { project, config_dir, cmd })
}

fn take_value(raw: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    raw.get(*i).cloned().ok_or_else(|| format!("{flag} 需要一个值"))
}

#[tokio::main]
async fn main() {
    // 日志走 stderr：stdout 留给 JSON（脚本/Agent 直接管道）
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse(&raw) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("参数错误：{e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    if args.cmd == Cmd::Help {
        println!("{USAGE}");
        return;
    }

    let mut session = match &args.config_dir {
        Some(dir) => Session::new(dir.clone()),
        None => Session::with_default_config(),
    };

    match run(&mut session, &args).await {
        Ok(value) => println!("{}", to_json(&value)),
        Err(e) => {
            let code = e.exit_code();
            println!("{}", to_json(&serde_json::json!({ "ok": false, "error": e })));
            std::process::exit(code);
        }
    }
}

fn to_json(v: &serde_json::Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".into())
}

async fn run(session: &mut Session, args: &Args) -> Result<serde_json::Value, ServerError> {
    use serde_json::json;

    // 需要项目的命令先打开（或切到 --project）
    match &args.cmd {
        Cmd::Open => {
            let report = ensure_open(session, args.project.as_deref()).await?;
            return Ok(json!({ "ok": true, "project": report }));
        }
        Cmd::Help => unreachable!("help 在 main 里提前返回"),
        Cmd::Compile { quick } => {
            ensure_open(session, args.project.as_deref()).await?;
            let report = session.compile(*quick).await?;
            let value = json!({ "ok": report.status == "success", "compile": report });
            if report.status != "success" {
                // 编译未通过：JSON 照常输出（含错误与诊断），退出码 1 便于脚本判断
                println!("{}", to_json(&value));
                std::process::exit(1);
            }
            return Ok(value);
        }
        _ => {
            ensure_open(session, args.project.as_deref()).await?;
        }
    }

    Ok(match &args.cmd {
        Cmd::Status => match session.last_report() {
            Some(r) => json!({ "ok": true, "status": r.status, "elapsed_ms": r.elapsed_ms, "kind": r.kind, "pdf_path": r.pdf_path }),
            None => json!({ "ok": true, "status": "none", "note": "本进程没有编译过（compile 的结果直接取自其输出）" }),
        },
        Cmd::Errors => match session.last_report() {
            Some(r) => json!({ "ok": true, "status": r.status, "errors": r.errors }),
            None => json!({ "ok": true, "status": "none", "errors": [] }),
        },
        Cmd::Outline => json!({ "ok": true, "outline": session.outline().await? }),
        Cmd::Tree { all } => {
            let files = session.tree(*all).await?;
            json!({ "ok": true, "count": files.len(), "files": files })
        }
        Cmd::Read { path } => {
            let resolved = project_path(session, path)?;
            let content = session.read(&resolved).await?;
            json!({ "ok": true, "path": resolved.to_string_lossy(), "bytes": content.len(), "content": content })
        }
        Cmd::Write { path, content } => {
            let resolved = project_path(session, path)?;
            let body = if content == "-" {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .map_err(|e| ServerError::Internal(format!("读 stdin 失败：{e}")))?;
                buf
            } else {
                content.clone()
            };
            let written = session.write(&resolved, &body).await?;
            json!({ "ok": true, "path": written.to_string_lossy(), "bytes": body.len() })
        }
        Cmd::Forward { file, line, column } => {
            let resolved = project_path(session, file)?;
            let pos = session.synctex_forward(&resolved, *line, *column).await?;
            json!({ "ok": true, "page": pos.page, "x": pos.x, "y": pos.y })
        }
        Cmd::Inverse { page, x, y } => {
            let resolved = session.synctex_inverse(*page, *x, *y).await?;
            json!({
                "ok": resolved.source.is_some(),
                "source": resolved.source.map(|s| json!({ "file": s.file.to_string_lossy(), "line": s.line, "column": s.column })),
                "note": resolved.note,
            })
        }
        Cmd::Settings => json!({ "ok": true, "settings": session.settings() }),
        Cmd::Open | Cmd::Compile { .. } | Cmd::Help => unreachable!("上面已处理"),
    })
}

/// 路径参数：绝对路径直接用；相对路径相对项目根（与 GUI 的"项目内路径"口径一致）。
fn project_path(session: &Session, path: &str) -> Result<PathBuf, ServerError> {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        return Ok(p);
    }
    let root = session
        .project()
        .map(|proj| proj.root.clone())
        .ok_or_else(|| ServerError::Invalid("尚未打开项目".into()))?;
    Ok(root.join(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(args: &[&str]) -> Args {
        parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()).expect("应能解析")
    }

    #[test]
    fn parses_global_flags_anywhere() {
        let parsed = a(&["compile", "--project", "E:/proj", "--quick"]);
        assert_eq!(parsed.project, Some(PathBuf::from("E:/proj")));
        assert_eq!(parsed.cmd, Cmd::Compile { quick: true });
    }

    #[test]
    fn no_command_prints_help() {
        assert_eq!(a(&[]).cmd, Cmd::Help);
        assert_eq!(a(&["--project", "x"]).cmd, Cmd::Help);
    }

    #[test]
    fn write_accepts_stdin_marker_and_spaces_in_content() {
        assert_eq!(
            a(&["write", "chapters/a.tex", "-"]).cmd,
            Cmd::Write { path: "chapters/a.tex".into(), content: "-".into() }
        );
        assert_eq!(
            a(&["write", "a.tex", "hello world"]).cmd,
            Cmd::Write { path: "a.tex".into(), content: "hello world".into() }
        );
    }

    #[test]
    fn forward_takes_line_and_optional_column() {
        assert_eq!(
            a(&["forward", "main.tex", "42"]).cmd,
            Cmd::Forward { file: "main.tex".into(), line: 42, column: 1 }
        );
        assert_eq!(
            a(&["forward", "main.tex", "42", "--column", "7"]).cmd,
            Cmd::Forward { file: "main.tex".into(), line: 42, column: 7 }
        );
    }

    #[test]
    fn inverse_takes_page_and_optional_xy() {
        assert_eq!(a(&["inverse", "3"]).cmd, Cmd::Inverse { page: 3, x: 0.0, y: 0.0 });
        assert_eq!(
            a(&["inverse", "3", "--x", "100", "--y", "650.5"]).cmd,
            Cmd::Inverse { page: 3, x: 100.0, y: 650.5 }
        );
    }

    #[test]
    fn tree_flag_and_default() {
        assert_eq!(a(&["tree"]).cmd, Cmd::Tree { all: false });
        assert_eq!(a(&["tree", "--all"]).cmd, Cmd::Tree { all: true });
    }

    #[test]
    fn missing_arguments_are_reported_with_hint() {
        let err = parse(&["read".to_string()]).unwrap_err();
        assert!(err.contains("read 需要"), "{err}");
        let err = parse(&["forward".to_string(), "main.tex".to_string()]).unwrap_err();
        assert!(err.contains("行号"), "{err}");
        let err = parse(&["nope".to_string()]).unwrap_err();
        assert!(err.contains("未知命令"), "{err}");
    }

    #[test]
    fn config_dir_flag_is_global() {
        let parsed = a(&["--config-dir", "E:/cfg", "settings"]);
        assert_eq!(parsed.config_dir, Some(PathBuf::from("E:/cfg")));
        assert_eq!(parsed.cmd, Cmd::Settings);
    }
}
