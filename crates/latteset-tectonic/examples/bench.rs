//! 库形态**常驻档**基准入口（方案 §6 的 P4 复核命令 / LIB-1 / LIB-2）。
//!
//! 本示例只做一件 CLI 做不到的事：**在同一个进程里连续编译 N 次**。这正是"常驻"的定义 ——
//! `latteset-cli` 是一次性语义（一个进程一条命令），所以"每次起进程"的地板只能在它上面量，
//! 而"同进程连续编译"必须有个循环，这个循环就是本示例。
//!
//! 用法（由 `scripts/bench-tectonic-lib.mjs --mode resident` 驱动，也可直接跑）：
//!
//! ```text
//! cargo run -q --release -p latteset-tectonic --example bench -- \
//!     --root <项目目录> [--runs 5] [--quick] [--json <out.json>]
//! ```
//!
//! bundle / 缓存目录沿用产品那三个环境变量（[`crate::BUNDLE_ENV`] / [`crate::CACHE_ENV`] /
//! [`crate::LIB_FORM_ENV`]），这样基准量与产品实际行为是同一套配置。
//!
//! **它不是产品路径**：`FileSystem` 用一个 `std::fs` 直读的极简实现（产品用 `latteset-infra`
//! 的 `TokioFs`）—— 但 runner 在库形态下只经 `read_bytes` 读根文件，其余输入走自持 I/O 层，
//! 所以这个替身不影响被测的那条路径。
//!
//! 输出（stdout 只有一行 JSON，与其它复核脚本同口径）：
//! - `runs[]`：每次 `compile()` 的墙钟（ms）+ 结果状态 + 页数；
//! - `median_ms` / `min_ms` / `max_ms`；
//! - `bundle_open_ms`（LIB-2 的"bundle 缓存"段：`open_bundle` + 取 digest）；
//! - `format_read_ms`（LIB-2 的"format 加载"段里**属于我们 I/O 层**的那部分：读 `.fmt` 文件）。

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use latteset_core::project::{DirEntry, FileSystem};
use latteset_core::scheduler::{CompileRunner, NoProgress};
use latteset_core::types::{CompileKind, CompileOutcome, CompileRequest};
use latteset_tectonic::{TectonicLibRunner, bundle_digest, open_bundle};

/// 极简 `FileSystem`（示例专用）：只实现 runner 会用到的那几个方法，其余按需直读。
struct LocalFs;

#[async_trait]
impl FileSystem for LocalFs {
    async fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(path)? {
            let e = e?;
            out.push(DirEntry { path: e.path(), is_dir: e.file_type()?.is_dir() });
        }
        Ok(out)
    }
    async fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }
    async fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }
    async fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
    async fn is_dir(&self, path: &Path) -> io::Result<bool> {
        Ok(path.is_dir())
    }
    async fn exists(&self, path: &Path) -> io::Result<bool> {
        Ok(path.exists())
    }
    async fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        std::fs::write(path, contents)
    }
    async fn create_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir(path)
    }
    async fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
    async fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(path)
    }
    async fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
}

struct Args {
    root: PathBuf,
    runs: usize,
    quick: bool,
    json: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut root = None;
    let mut runs = 5usize;
    let mut quick = false;
    let mut json = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => root = Some(PathBuf::from(it.next().ok_or("--root 缺值")?)),
            "--runs" => {
                runs = it
                    .next()
                    .ok_or("--runs 缺值")?
                    .parse()
                    .map_err(|e| format!("--runs 不是数字：{e}"))?
            }
            "--quick" => quick = true,
            "--json" => json = Some(PathBuf::from(it.next().ok_or("--json 缺值")?)),
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("未知参数：{other}")),
        }
    }
    Ok(Args { root: root.ok_or("缺少 --root <项目目录>")?, runs, quick, json })
}

const USAGE: &str = "\
用法：cargo run -q --release -p latteset-tectonic --example bench -- \
      --root <项目目录> [--runs 5] [--quick] [--json <out.json>]

环境：LATTESET_TECTONIC_BUNDLE / LATTESET_TECTONIC_CACHE（与产品同一套）。";

fn main() {
    // 日志：runner 每轮会打 `passes=` / `converged=` / 分阶段耗时 —— 常驻档的退化到底是
    // "趟数变多"还是"同样的活变慢"，必须能从日志里读出来。级别由 RUST_LOG 控制（默认 info）。
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}");
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };

    // ── LIB-2 段 2：bundle 打开 + 取 digest（常驻档这一段的收益 = 只做一次）────────────
    let bundle = latteset_tectonic::bundle_from_env();
    let t0 = Instant::now();
    let digest_probe = match open_bundle(&bundle, false, latteset_tectonic::cache_dir_from_env()) {
        Ok(Some(mut b)) => bundle_digest(&mut *b).ok(),
        _ => None,
    };
    let bundle_open_ms = t0.elapsed().as_secs_f64() * 1e3;
    // 释放探针 bundle（真编译会再开一个；这里只为量"打开一次"的成本）
    drop(digest_probe);

    // ── LIB-2 段 3：format 文件读取（属于我们 I/O 层的那部分；引擎解析在 C 侧，本示例量不到）──
    let format_read_ms = digest_probe_format_read(&args.root);

    // ── 被测：同进程连续编译 N 次 ────────────────────────────────────────────────
    let root_file = detect_root(&args.root).unwrap_or_else(|| {
        eprintln!("无法在 {} 里找到根文件（main.tex / <dir>.tex）", args.root.display());
        std::process::exit(2);
    });
    let fs: Arc<dyn FileSystem> = Arc::new(LocalFs);
    let mut runner = TectonicLibRunner::new(fs, Arc::new(NoProgress)).with_bundle(bundle);
    if let Some(cache) = latteset_tectonic::cache_dir_from_env() {
        runner = runner.with_cache_dir(cache);
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("建 tokio 运行时");

    let mut runs = Vec::new();
    for i in 0..args.runs {
        let req = CompileRequest {
            root_file: root_file.clone(),
            project_root: args.root.clone(),
            engine: latteset_core::types::Engine::Tectonic,
            timeout: Duration::from_secs(300),
            kind: if args.quick { CompileKind::Quick } else { CompileKind::Full },
        };
        let t = Instant::now();
        let outcome = rt.block_on(runner.compile(req, tokio_util::sync::CancellationToken::new()));
        let ms = t.elapsed().as_secs_f64() * 1e3;
        let (status, pages, kind) = match &outcome {
            CompileOutcome::Success { page_hashes, kind, .. } => {
                ("success", page_hashes.len(), format!("{kind:?}"))
            }
            CompileOutcome::ContentError { errors } => ("content_error", 0, format!("{} 条错误", errors.len())),
            CompileOutcome::Timeout { .. } => ("timeout", 0, "-".to_owned()),
            CompileOutcome::Aborted => ("aborted", 0, "-".to_owned()),
            CompileOutcome::IoError { message } => {
                eprintln!("第 {} 次编译 io_error：{message}", i + 1);
                ("io_error", 0, "-".to_owned())
            }
        };
        runs.push(serde_json::json!({ "index": i + 1, "ms": round(ms), "status": status, "pages": pages, "kind": kind }));
    }

    let mut times: Vec<f64> = runs.iter().filter_map(|r| r["ms"].as_f64()).collect();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if times.is_empty() { 0.0 } else { times[times.len() / 2] };
    // 常驻档的**稳态**数字：第 2 次起（第 1 次含冷启动/首次装载，不是常驻的稳态）
    let mut steady: Vec<f64> = runs.iter().skip(1).filter_map(|r| r["ms"].as_f64()).collect();
    steady.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let steady_median = if steady.is_empty() { median } else { steady[steady.len() / 2] };

    let report = serde_json::json!({
        "mode": "resident",
        "runs": runs,
        "median_ms": round(median),
        "steady_median_ms": round(steady_median),
        "min_ms": round(times.first().copied().unwrap_or(0.0)),
        "max_ms": round(times.last().copied().unwrap_or(0.0)),
        "bundle_open_ms": round(bundle_open_ms),
        "format_read_ms": round(format_read_ms),
        "root_file": root_file.to_string_lossy(),
        "quick": args.quick,
    });
    let text = serde_json::to_string_pretty(&report).expect("序列化报告");
    println!("{text}");
    if let Some(path) = args.json {
        if let Err(e) = std::fs::write(&path, &text) {
            eprintln!("写 --json 失败（{}）：{e}", path.display());
        }
    }
}

fn round(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// 根文件探测（示例专用简化版：`main.tex` → `<目录名>.tex` → 唯一 `.tex`）。
fn detect_root(dir: &Path) -> Option<PathBuf> {
    let main = dir.join("main.tex");
    if main.is_file() {
        return Some(main);
    }
    if let Some(name) = dir.file_name() {
        let cand = dir.join(format!("{}.tex", name.to_string_lossy()));
        if cand.is_file() {
            return Some(cand);
        }
    }
    let mut tex: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "tex"))
        .collect();
    if tex.len() == 1 {
        tex.pop()
    } else {
        None
    }
}

/// 读一次产品缓存里的 `.fmt`（LIB-2 的"format 加载"段里属于我们 I/O 层的部分）。
///
/// 引擎侧对 dump 的**解析**发生在 C 里（`tt_engine_xetex_main`），本示例量不到 —— 如实只报
/// 我们这一侧的字节读取。找不到缓存目录/摘要 ⇒ 返回 0（不猜）。
fn digest_probe_format_read(root: &Path) -> f64 {
    let Some(cache) = latteset_tectonic::cache_dir_from_env() else {
        return 0.0;
    };
    let formats = cache.join("formats");
    let Ok(entries) = std::fs::read_dir(&formats) else {
        return 0.0;
    };
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "fmt") {
            if let Ok(md) = e.metadata() {
                if let Ok(t) = md.modified() {
                    if newest.as_ref().is_none_or(|(nt, _)| t > *nt) {
                        newest = Some((t, p));
                    }
                }
            }
        }
    }
    let Some((_, p)) = newest else {
        return 0.0;
    };
    let _ = root;
    let t = Instant::now();
    let n = std::fs::read(&p).map(|b| b.len()).unwrap_or(0);
    let ms = t.elapsed().as_secs_f64() * 1e3;
    eprintln!(
        "[bench] format 读取：{} ({:.2} MB) → {:.2} ms（引擎解析那部分未测）",
        p.file_name().unwrap_or_default().to_string_lossy(),
        n as f64 / 1e6,
        ms
    );
    ms
}
