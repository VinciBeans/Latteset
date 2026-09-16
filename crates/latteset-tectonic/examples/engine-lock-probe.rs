//! 桥接引擎**并发性**探测：同一进程里多线程各跑一次 XDV→PDF，看墙钟是「相加」还是「取最大」。
//!
//! 为什么需要它：roadmap ㉞（流式出图）原设计是「旁观线程在排版进行中增量出图」。实测里同一次
//! 转换在编译内要 3.87 s，而单独跑只要 ~0.15 s。`tectonic_bridge_core` 的
//! `CoreBridgeLauncher::with_global_lock` 用 `static ENGINE_LOCK: Mutex<u8>` 把**整个 C 引擎调用**
//! 串起来（源码注释：C 侧错误上报走 setjmp/longjmp，跨 FFI 是 UB，所以全局只能有一个引擎在跑）。
//! 本示例把这个推断变成可复现的读数：串行 ⇒ 并发墙钟 ≈ 各次之和；并行 ⇒ ≈ 最慢的那次。
//!
//! 用法：
//!   cargo run -q --release -p latteset-tectonic --example engine-lock-probe -- \
//!       --xdv <file.xdv> [--threads 2] [--runs 1] [--pages N]
//!
//! 环境（与产品同一套）：`LATTESET_TECTONIC_BUNDLE` / `LATTESET_TECTONIC_CACHE`。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use latteset_core::scheduler::NoProgress;
use latteset_tectonic::io::{TectonicIo, new_capture};
use latteset_tectonic::status::ProgressStatus;
use tectonic_bridge_core::{CoreBridgeLauncher, MinimalDriver};
use tectonic_engine_xdvipdfmx::XdvipdfmxEngine;

struct Args {
    xdv: PathBuf,
    threads: usize,
    runs: usize,
    pages: Option<usize>,
}

fn parse_args() -> Result<Args, String> {
    let (mut xdv, mut threads, mut runs, mut pages) = (None, 2usize, 1usize, None);
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--xdv" => xdv = Some(PathBuf::from(it.next().ok_or("--xdv 缺值")?)),
            "--threads" => threads = it.next().ok_or("--threads 缺值")?.parse().map_err(|e| format!("{e}"))?,
            "--runs" => runs = it.next().ok_or("--runs 缺值")?.parse().map_err(|e| format!("{e}"))?,
            "--pages" => pages = Some(it.next().ok_or("--pages 缺值")?.parse().map_err(|e| format!("{e}"))?),
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("未知参数：{other}")),
        }
    }
    Ok(Args { xdv: xdv.ok_or("缺少 --xdv <file.xdv>")?, threads, runs, pages })
}

/// 一次完整转换（现开 bundle + 注入 + 引擎），返回 (open_ms, engine_ms, pdf_bytes)。
fn convert_once(xdv: &[u8]) -> Result<(u128, u128, usize), String> {
    let t_open = Instant::now();
    let source = latteset_tectonic::bundle_from_env();
    let bundle = latteset_tectonic::open_bundle(&source, false, latteset_tectonic::cache_dir_from_env())
        .map_err(|e| format!("打不开 bundle：{e}"))?;
    let open_ms = t_open.elapsed().as_millis();

    let shared = new_capture();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut io = TectonicIo::new(
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        None,
        bundle,
        None,
        None,
        shared.clone(),
        requests,
        None,
    );
    io.inject("probe.xdv", xdv.to_vec());

    let progress: Arc<dyn latteset_core::scheduler::CompileProgress> = Arc::new(NoProgress);
    let mut status = ProgressStatus::new(progress, Arc::new(Mutex::new(Vec::new())));
    let mut driver = MinimalDriver::new(io);
    let t_engine = Instant::now();
    let result = {
        let mut launcher = CoreBridgeLauncher::new(&mut driver, &mut status);
        let mut engine = XdvipdfmxEngine::default();
        engine.process(&mut launcher, "probe.xdv", "probe.pdf")
    };
    let engine_ms = t_engine.elapsed().as_millis();
    result.map_err(|e| format!("{e:#}").replace('"', "'").replace('\n', " "))?;
    let n = {
        let c = shared.lock().unwrap_or_else(|e| e.into_inner());
        c.files.get("probe.pdf").map_or(0, Vec::len)
    };
    Ok((open_ms, engine_ms, n))
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}");
            }
            eprintln!("用法：--xdv <file.xdv> [--threads N] [--runs N] [--pages N]");
            std::process::exit(2);
        }
    };

    let full = std::fs::read(&args.xdv).unwrap_or_else(|e| {
        eprintln!("读不了 {}：{e}", args.xdv.display());
        std::process::exit(2);
    });
    let spans = latteset_core::xdv::complete_pages(&full);
    let take = args.pages.unwrap_or(spans.len()).clamp(1, spans.len().max(1));
    let prefix = &full[..spans[take - 1].end];
    let xdv = latteset_core::xdv::synthesize_postamble(prefix)
        .expect("有完整页就该合成得出来")
        .bytes;
    println!(
        "页数={} 注入 XDV={} B 线程={} 每线程次数={}",
        take,
        xdv.len(),
        args.threads,
        args.runs
    );

    // ① 冷身：第一次转换要现开 bundle / 摸缓存，不计入基线
    let _ = convert_once(&xdv);

    // ② 单线程基线
    let t = Instant::now();
    let (open_ms, engine_ms, pdf) = convert_once(&xdv).expect("单线程基线转换失败");
    let solo_wall = t.elapsed().as_millis();
    println!("单线程：open={open_ms}ms engine={engine_ms}ms wall={solo_wall}ms pdf={pdf}B");

    // ③ 并发：N 线程各 runs 次，记录每次的 engine 耗时与整段墙钟
    let xdv = Arc::new(xdv);
    let t_all = Instant::now();
    let handles: Vec<_> = (0..args.threads)
        .map(|i| {
            let xdv = xdv.clone();
            let runs = args.runs;
            std::thread::spawn(move || {
                let mut out = Vec::new();
                for r in 0..runs {
                    match convert_once(&xdv) {
                        Ok((o, e, _)) => out.push((o, e)),
                        Err(msg) => {
                            eprintln!("线程 {i} 第 {r} 次失败：{msg}");
                            return (i, out, false);
                        }
                    }
                }
                (i, out, true)
            })
        })
        .collect();
    let mut per_thread = Vec::new();
    let mut ok = true;
    for h in handles {
        let (i, out, good) = h.join().expect("线程 panic");
        ok &= good;
        let engines: Vec<u128> = out.iter().map(|(_, e)| *e).collect();
        println!("线程 {i}：engine={engines:?}ms open={:?}ms", out.iter().map(|(o, _)| *o).collect::<Vec<_>>());
        per_thread.push(engines);
    }
    let wall = t_all.elapsed().as_millis();

    let all: Vec<u128> = per_thread.iter().flatten().copied().collect();
    let sum: u128 = all.iter().sum();
    let max = all.iter().copied().max().unwrap_or(0);
    let calls = all.len();
    // 判据看**墙钟**：真并行时并发墙钟 ≈ 单次（多出来的只是调度噪声）；被全局锁串行时 ≈ 次数×单次。
    // （别拿 wall 比「各次耗时之和」——串行时每个线程自己测到的时长里就含着排队，那个和是虚高的。）
    let expected_serial = (calls as u128) * engine_ms;
    let verdict = if !ok {
        "failed"
    } else if wall as f64 <= 1.6 * engine_ms as f64 {
        "parallel"
    } else if wall as f64 >= 0.7 * expected_serial as f64 {
        "serialized"
    } else {
        "partial"
    };
    println!(
        "{{\"case\":\"engine-lock-probe\",\"threads\":{},\"calls\":{calls},\"solo_engine_ms\":{engine_ms},\
         \"sum_engine_ms\":{sum},\"max_engine_ms\":{max},\"concurrent_wall_ms\":{wall},\
         \"expected_serial_ms\":{expected_serial},\"verdict\":\"{verdict}\"}}",
        args.threads
    );
    if !ok {
        std::process::exit(1);
    }
}
