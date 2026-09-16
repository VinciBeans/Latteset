//! 「页前缀 → 部分 PDF」的**合成 + 进程内转换**验证入口（roadmap ㉞ 流式出图）。
//!
//! 它只做两件事：读一份**完整** XDV → 截到前 N 页（用产品同一套页边界：
//! `latteset_core::xdv::complete_pages`）→ 补 postamble（`synthesize_postamble`）
//! → 喂**进程内** `XdvipdfmxEngine` → 落 PDF。**不做**产品接线（事件、阈值、前端）。
//!
//! 用法：
//!   cargo run -q --release -p latteset-tectonic --example xdv-partial -- \
//!       --xdv <file.xdv> [--pages N | --ratio 0.5] [--out <pdf>] [--json]
//!
//! 环境（与产品同一套）：`LATTESET_TECTONIC_BUNDLE` / `LATTESET_TECTONIC_CACHE`。
//! stdout 只有一行 JSON：页数 / 合成字节 / 合成耗时 / 转换耗时 / PDF 字节数。
//!
//! 判据（与 `docs/research/dvi-preview-feasibility.md` §10 同口径）：
//! ① 转换 `ok`；② 产出 PDF 的页数 == 前缀页数；③ 单次转换 ~0.1 s 量级。

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use latteset_core::project::{DirEntry, FileSystem};
use latteset_core::scheduler::NoProgress;
use latteset_tectonic::io::{TectonicIo, new_capture};
use latteset_tectonic::status::ProgressStatus;
use tectonic_bridge_core::{CoreBridgeLauncher, MinimalDriver};
use tectonic_engine_xdvipdfmx::XdvipdfmxEngine;

/// 极简 `FileSystem`（示例专用；与 `examples/bench.rs` 同款）。
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
}

struct Args {
    xdv: PathBuf,
    pages: Option<usize>,
    ratio: Option<f64>,
    out: Option<PathBuf>,
    out_xdv: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut xdv = None;
    let mut pages = None;
    let mut ratio = None;
    let mut out = None;
    let mut out_xdv = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--xdv" => xdv = Some(PathBuf::from(it.next().ok_or("--xdv 缺值")?)),
            "--pages" => pages = Some(it.next().ok_or("--pages 缺值")?.parse().map_err(|e| format!("{e}"))?),
            "--ratio" => ratio = Some(it.next().ok_or("--ratio 缺值")?.parse().map_err(|e| format!("{e}"))?),
            "--out" => out = Some(PathBuf::from(it.next().ok_or("--out 缺值")?)),
            "--out-xdv" => out_xdv = Some(PathBuf::from(it.next().ok_or("--out-xdv 缺值")?)),
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("未知参数：{other}")),
        }
    }
    Ok(Args { xdv: xdv.ok_or("缺少 --xdv <file.xdv>")?, pages, ratio, out, out_xdv })
}

const USAGE: &str = "\
用法：cargo run -q --release -p latteset-tectonic --example xdv-partial -- \
      --xdv <file.xdv> [--pages N | --ratio 0.5] [--out <pdf>] [--json]";

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
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

    let full = std::fs::read(&args.xdv).unwrap_or_else(|e| {
        eprintln!("读不了 {}：{e}", args.xdv.display());
        std::process::exit(2);
    });
    let spans = latteset_core::xdv::complete_pages(&full);
    if spans.is_empty() {
        eprintln!("这份 XDV 解析不出完整页：{}", args.xdv.display());
        std::process::exit(1);
    }
    // 取前 N 页（默认全部）：前缀 = [0, 第 N 页 eop)
    let take = args
        .pages
        .or_else(|| args.ratio.map(|r| ((spans.len() as f64 * r).round() as usize).max(1)))
        .unwrap_or(spans.len())
        .clamp(1, spans.len());
    let prefix = &full[..spans[take - 1].end];

    // ① 合成（产品路径：`latteset_core::xdv::synthesize_postamble`）
    let t_synth = Instant::now();
    let partial = latteset_core::xdv::synthesize_postamble(prefix)
        .expect("有完整页就该合成得出来（前缀来自上面同一套页边界）");
    let synth_ms = t_synth.elapsed().as_millis();
    assert_eq!(partial.pages, take, "合成件页数必须等于前缀页数");

    // ② 进程内转换（与产品库形态同一个引擎、同一层 I/O）
    let shared = new_capture();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let cache = latteset_tectonic::cache_dir_from_env();
    let bundle_source = latteset_tectonic::bundle_from_env();
    let bundle = match latteset_tectonic::open_bundle(&bundle_source, false, cache.clone()) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("打不开 bundle：{e}");
            std::process::exit(2);
        }
    };
    let digest = match bundle.as_ref() {
        Some(_) => None, // 本示例不碰 format 缓存；digest 只给 format 用
        None => None,
    };
    let mut io = TectonicIo::new(
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        None,
        bundle,
        digest,
        None,
        shared.clone(),
        requests,
        None,
    );
    io.inject("main.xdv", partial.bytes.clone());

    let progress: Arc<dyn latteset_core::scheduler::CompileProgress> = Arc::new(NoProgress);
    let log_ring = Arc::new(Mutex::new(Vec::new()));
    let mut status = ProgressStatus::new(progress, log_ring);
    let mut driver = MinimalDriver::new(io);
    let t_conv = Instant::now();
    let result = {
        let mut launcher = CoreBridgeLauncher::new(&mut driver, &mut status);
        let mut engine = XdvipdfmxEngine::default();
        engine.process(&mut launcher, "main.xdv", "main.pdf")
    };
    let convert_ms = t_conv.elapsed().as_millis();

    let pdf = {
        let c = shared.lock().unwrap_or_else(|e| e.into_inner());
        c.files.get("main.pdf").cloned().unwrap_or_default()
    };
    let (ok, err) = match &result {
        Ok(()) => (true, String::new()),
        Err(e) => (false, format!("{e:#}").replace('"', "'").replace('\n', " ")),
    };
    if let Some(out) = &args.out {
        if !pdf.is_empty() {
            std::fs::write(out, &pdf).unwrap_or_else(|e| eprintln!("写 {} 失败：{e}", out.display()));
        }
    }
    if let Some(out) = &args.out_xdv {
        std::fs::write(out, &partial.bytes)
            .unwrap_or_else(|e| eprintln!("写 {} 失败：{e}", out.display()));
    }

    println!(
        "{{\"case\":\"xdv-partial\",\"ok\":{ok},\"source_pages\":{},\"prefix_pages\":{take},\
         \"prefix_bytes\":{},\"synth_bytes\":{},\"synth_ms\":{synth_ms},\"convert_ms\":{convert_ms},\
         \"pdf_bytes\":{},\"error\":\"{}\"}}",
        spans.len(),
        prefix.len(),
        partial.bytes.len(),
        pdf.len(),
        err
    );
    if !ok {
        std::process::exit(1);
    }
    let _ = LocalFs; // 保留：本示例不读项目文件，但需要与产品同款的 FileSystem 类型在场
}
