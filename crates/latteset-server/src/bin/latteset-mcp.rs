//! `latteset-mcp`：MCP server（stdio 传输），把 headless 服务暴露成 Agent 可直接调用的工具。
//!
//! 用法（harness 侧配置成 stdio MCP server 即可）：
//! ```text
//! latteset-mcp [--project <dir>] [--config-dir <dir>]
//! ```
//! - 一行一条 JSON-RPC 消息（MCP stdio 约定）；**stdout 只有协议消息**，日志一律 stderr。
//! - `--project` 只影响"未显式传 project 参数"的工具调用（预打开该项目，工具调用可省略 `project`）。

use std::path::PathBuf;
use latteset_server::mcp::handle_message;
use latteset_server::{absolutize, Session};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const USAGE: &str = "\
latteset-mcp —— Latteset MCP server（stdio）

用法：latteset-mcp [--project <目录>] [--config-dir <目录>]

  --project <目录>     预打开项目（省略时由工具参数 project / 当前目录决定）
  --config-dir <目录>  应用配置目录（默认与 GUI 相同；也可用 LATTESET_CONFIG_DIR）
  --help               本帮助

协议：MCP over stdio（一行一条 JSON-RPC 2.0 消息）。工具：project_open / compile /
compile_get_status / compile_get_errors / outline_get / project_tree / file_read /
file_write / synctex_forward / synctex_inverse / settings_get。
";

#[tokio::main]
async fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut project: Option<PathBuf> = None;
    let mut config_dir: Option<PathBuf> = None;
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--project" => {
                i += 1;
                match raw.get(i) {
                    Some(v) => project = Some(PathBuf::from(v)),
                    None => {
                        eprintln!("参数错误：--project 需要一个值");
                        std::process::exit(2);
                    }
                }
            }
            "--config-dir" => {
                i += 1;
                match raw.get(i) {
                    Some(v) => config_dir = Some(PathBuf::from(v)),
                    None => {
                        eprintln!("参数错误：--config-dir 需要一个值");
                        std::process::exit(2);
                    }
                }
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return;
            }
            other => {
                eprintln!("参数错误：未知参数 {other}\n\n{USAGE}");
                std::process::exit(2);
            }
        }
        i += 1;
    }

    // 日志走 stderr；MCP 的 stdout 只放协议消息
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let mut session = match &config_dir {
        Some(dir) => Session::new(dir.clone()),
        None => Session::with_default_config(),
    };
    if let Some(project) = &project {
        if let Err(e) = session.open_project(&absolutize(project)).await {
            // 预打开失败不致命：工具调用仍可带 project 参数重试
            eprintln!("[latteset-mcp] 预打开项目失败：{e}");
        }
    }

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                // 协议层错误：按 JSON-RPC 回 -32700
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": serde_json::Value::Null,
                    "error": { "code": -32700, "message": format!("JSON 解析失败：{e}") }
                });
                let _ = write_line(&mut stdout, &resp).await;
                continue;
            }
        };
        if let Some(resp) = handle_message(&mut session, msg).await {
            if write_line(&mut stdout, &resp).await.is_err() {
                break; // 对端已关闭
            }
        }
    }
}

async fn write_line<W: AsyncWriteExt + Unpin>(
    out: &mut W,
    value: &serde_json::Value,
) -> std::io::Result<()> {
    let mut text = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    text.push('\n');
    out.write_all(text.as_bytes()).await?;
    out.flush().await
}
