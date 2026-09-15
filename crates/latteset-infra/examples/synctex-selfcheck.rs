//! SyncTeX **自解析 vs 系统 CLI** 的逐点对拍入口（`scripts/synctex-selfcheck.mjs` 驱动）。
//!
//! 为什么要有它：ADR-0008 原本选的是"走系统 `synctex` CLI"，而 `docs/modules.md` 的同步精度基线
//! （`scripts/synctex-report.mjs` 的三组样本，往返跳到位 34/34）**是在 CLI 上量的**。把实现换成
//! 自解析（`SyncTexSelf`）之后，唯一可信的说法是"在**同一批样本**上与 CLI 逐点比过"——本示例就是
//! 那个比对里属于 Rust 的一侧：一个进程读多条查询，避免每条查询都起一次进程。
//!
//! 用法：
//! ```text
//! cargo run -q --release -p latteset-infra --example synctex-selfcheck -- --pdf <path.pdf>
//! ```
//! stdin 每行一条查询（`\t` 分隔）：
//! - `F\t<源文件绝对路径>\t<行号>` → 输出 `OK\t<page>\t<x>\t<y>` 或 `ERR\t<消息>`
//! - `I\t<页号>\t<x>\t<y>`         → 输出 `OK\t<源文件>\t<行号>` 或 `ERR\t<消息>`
//!
//! 之所以从 stdin 读、而不是给每条查询单独起进程：三组样本是 36 个正反向点，起 36 次进程的
//! 启动开销会把测量掩盖掉。

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use latteset_core::synctex::{SourcePosition, SyncTexPosition, SyncTexProvider};

fn main() {
    let mut pdf: Option<PathBuf> = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--pdf" => pdf = it.next().map(PathBuf::from),
            "-h" | "--help" => {
                eprintln!("用法：--pdf <path.pdf>，查询从 stdin 读（见文件头注释）");
                return;
            }
            other => {
                eprintln!("未知参数：{other}");
                std::process::exit(2);
            }
        }
    }
    let Some(pdf) = pdf else {
        eprintln!("缺少 --pdf <path.pdf>");
        std::process::exit(2);
    };

    // 固定用**自解析**实现（本示例的目的就是量它；不读 LATTESET_SYNCTEX，避免复核时被环境带偏）。
    let provider: Arc<dyn SyncTexProvider> = Arc::new(latteset_infra::synctex::SyncTexSelf);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("建 tokio 运行时");

    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        let answer = match parts.as_slice() {
            ["F", file, line_no] => {
                let src = SourcePosition {
                    file: PathBuf::from(file),
                    line: line_no.parse().unwrap_or(0),
                    column: 0,
                };
                match rt.block_on(provider.forward(&src, &pdf)) {
                    Ok(SyncTexPosition { page, x, y }) => format!("OK\t{page}\t{x}\t{y}"),
                    Err(e) => format!("ERR\t{e}"),
                }
            }
            ["I", page, x, y] => {
                let pos = SyncTexPosition {
                    page: page.parse().unwrap_or(0),
                    x: x.parse().unwrap_or(0.0),
                    y: y.parse().unwrap_or(0.0),
                };
                match rt.block_on(provider.inverse(&pos, &pdf)) {
                    Ok(SourcePosition { file, line, .. }) => {
                        format!("OK\t{}\t{line}", file.display())
                    }
                    Err(e) => format!("ERR\t{e}"),
                }
            }
            _ => "ERR\t查询格式不认识".to_owned(),
        };
        let _ = writeln!(out, "{answer}");
    }
}
