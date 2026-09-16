//! 流式出图（roadmap ㉞）：编译期把**已完成页**变成「部分 PDF」，让长编译也看得见进度。
//!
//! **为什么在趟边界出图，而不是起个旁观线程**（实测约束）：
//! `tectonic_bridge_core::CoreBridgeLauncher::with_global_lock` 用 `static ENGINE_LOCK: Mutex<u8>`
//! 把**整个 C 引擎调用**串起来（源码注释：C 侧错误上报走 setjmp/longjmp，跨 FFI 是 UB，所以全局
//! 只能有一个引擎在跑）⇒ **同一进程里排版趟与转换不可能并行**。读数见
//! `examples/engine-lock-probe.rs`（24 逻辑核机器）：单次 133 ms；4 线程并发墙钟 526 ms ≈ 4×133，
//! 每个线程自己是 133/262/391/523 ms（排队序号的整倍数）—— 完全串行、零并行收益。
//!
//! 于是"旁观线程按节拍看捕获表"这条路是不通的：它只能在趟与趟之间的空隙里跑，而届时它的快照
//! **按定义已经过期**（实测那一版：帧落在终态 PDF 之前约 0.1 s，等于没用）。
//! 真正有价值的时机是**趟边界**：
//! - **新鲜**：第 1 趟就已经把整份文档排出来了（引用/目录页码可能落后），页数此后基本不变；
//! - **便宜**：此刻没有趟在跑 ⇒ 锁是空的，转换不必排队，也不会把谁往后拖；
//! - **确定**：不需要轮询、不需要线程、不存在"快照与转换之间又跑了半趟"的竞态。
//!
//! 代价有界的两条闸门（roadmap §6.6）：
//! 1. **只在还会再跑一趟时才出图**（调用点就在 `break` 之后），且第 1 趟墙钟要够长
//!    （[`MIN_FIRST_PASS`]）—— 短编译（一趟就收敛、或快得没必要）一次都不跑，零代价；
//! 2. **页数没涨就不重做**、**总共最多 [`MAX_UPDATES`] 次**：一条编译通常只出 1 帧。
//!
//! 产物落在 `tmp/<stem>.preview.pdf`（原子替换）。**它不是最终产物**：终态仍由
//! `CompileOutcome::Success.pdf_path` 给出；`tmp/` 又是忽略路径 ⇒ 写它不会反过来触发编译。
//!
//! ⚠ **不得**把它喂给页哈希缓存 / A 闸门：那些只认权威产物（中间态会污染基线）。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use latteset_core::scheduler::CompileProgress;
use tectonic_bridge_core::{CoreBridgeLauncher, MinimalDriver};
use tectonic_engine_xdvipdfmx::XdvipdfmxEngine;
use tectonic_status_base::NoopStatusBackend;
use tracing::{debug, warn};

use crate::bundle::{BundleSource, open_bundle};
use crate::io::{SharedCapture, TectonicIo, new_capture};

/// 第 1 趟至少跑这么久才值得出图：它是后面几趟的时长预测量，太短说明整体很快就完、
/// 用户并不会等，那 0.1 s 的转换就是纯开销。
const MIN_FIRST_PASS: Duration = Duration::from_millis(500);
/// 单次编译最多出几帧（总开销上限）。
const MAX_UPDATES: usize = 8;

/// 流式出图开关。默认**开**；`LATTESET_PREVIEW_STREAM=0|false|off|no` 关掉。
///
/// 为什么留一个开关：它是"编译期额外干活"的通道，A/B 复核（对比开/关时的编译墙钟与产物）需要
/// 一个不重新编译二进制的对照手段；与 `LATTESET_TECTONIC_LIB` / `_BUNDLE` / `_CACHE` 同一层。
pub fn stream_enabled() -> bool {
    match std::env::var("LATTESET_PREVIEW_STREAM") {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no"
        ),
        Err(_) => true,
    }
}

/// 出图要用的上下文（runner 在排版循环之前组装一次）。
pub struct PreviewContext {
    pub bundle_source: BundleSource,
    pub only_cached: bool,
    pub cache_dir: Option<PathBuf>,
    pub project_root: PathBuf,
    pub tmp_dir: PathBuf,
    pub stem: String,
    /// 捕获表里的 XDV 名（`<stem>.xdv`）。
    pub xdv_name: String,
    pub shared: SharedCapture,
    pub progress: Arc<dyn CompileProgress>,
    /// 编译的取消位：编译被中止时不再出图。
    pub cancel: Arc<AtomicBool>,
}

/// 趟边界出图器（状态只有"上次出了几页"和"出过几次"）。
pub struct PartialPainter {
    ctx: PreviewContext,
    last_pages: usize,
    updates: usize,
}

impl PartialPainter {
    pub fn new(ctx: PreviewContext) -> Self {
        Self { ctx, last_pages: 0, updates: 0 }
    }

    /// 在**确定还会再跑下一趟**的趟边界调一次；不值得/出不了就安静返回。
    ///
    /// `first_pass` 是第 1 趟的墙钟（成本闸门，见 [`MIN_FIRST_PASS`]）。
    pub fn paint(&mut self, first_pass: Duration) {
        if self.updates >= MAX_UPDATES || first_pass < MIN_FIRST_PASS {
            return;
        }
        if self.ctx.cancel.load(Ordering::Relaxed) {
            return;
        }

        // 快照当前前缀（锁内只做一次 clone，别抱着锁合成）
        let prefix = {
            let c = self.ctx.shared.lock().unwrap_or_else(|e| e.into_inner());
            c.files.get(&self.ctx.xdv_name).cloned()
        };
        let Some(prefix) = prefix else {
            debug!("流式出图：捕获表里还没有 XDV");
            return;
        };
        let Some(partial) = latteset_core::xdv::synthesize_postamble(&prefix) else {
            debug!(bytes = prefix.len(), "流式出图：还没有完整页");
            return; // 一页完整页都还没有
        };
        if partial.pages <= self.last_pages {
            return; // 页数没涨：同一份内容不必再转一次
        }

        let Some(pdf) = self.convert(partial.bytes) else {
            // 打不开 bundle / 转换失败：中间态是尽力而为的通道，说一声就放弃
            warn!("流式出图：部分 PDF 生成失败，本条通道放弃（编译本身不受影响）");
            self.updates = MAX_UPDATES;
            return;
        };

        let path = self.ctx.tmp_dir.join(format!("{}.preview.pdf", self.ctx.stem));
        if let Err(e) = write_atomic(&path, &pdf) {
            debug!(path = %path.display(), "流式出图：写部分 PDF 失败（下次再试）：{e}");
            return;
        }
        debug!(
            pages = partial.pages,
            bytes = pdf.len(),
            first_pass_ms = first_pass.as_millis() as u64,
            path = %path.display(),
            "流式出图：趟边界已出部分 PDF"
        );
        self.ctx.progress.partial_pdf(&path, partial.pages as u32);
        self.last_pages = partial.pages;
        self.updates += 1;
    }

    /// 把（已补过 postamble 的）XDV 转成 PDF —— 与产品库形态同一个引擎、同一层 I/O。
    ///
    /// **每次出图现开一份 bundle**：`TectonicIo` 按值收走 bundle（不可共享），而出图次数被
    /// [`MAX_UPDATES`] 与页数闸门限得很死（实测 `open_ms=0`），多付的几十毫秒可以接受；
    /// 换来的是这里不需要任何"bundle 池"之类的机制。
    fn convert(&self, xdv: Vec<u8>) -> Option<Vec<u8>> {
        let t_open = Instant::now();
        let bundle = match open_bundle(
            &self.ctx.bundle_source,
            self.ctx.only_cached,
            self.ctx.cache_dir.clone(),
        ) {
            Ok(b) => b,
            Err(e) => {
                debug!("流式出图：转换前打不开 bundle：{e}");
                return None;
            }
        };
        let open_ms = t_open.elapsed().as_millis();
        let shared = new_capture();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut io = TectonicIo::new(
            self.ctx.project_root.clone(),
            None, // 不镜像到 tmp/：中间态的中间产物没有价值
            bundle,
            None,
            None,
            shared.clone(),
            requests,
            Some(self.ctx.cancel.clone()),
        );
        let xdv_bytes = xdv.len();
        io.inject("preview.xdv", xdv);

        let mut status = NoopStatusBackend::default();
        let mut driver = MinimalDriver::new(io);
        let pdf_name = "preview.pdf";
        let t_engine = Instant::now();
        let result = {
            let mut launcher = CoreBridgeLauncher::new(&mut driver, &mut status);
            let mut engine = XdvipdfmxEngine::default();
            engine.process(&mut launcher, "preview.xdv", pdf_name)
        };
        let engine_ms = t_engine.elapsed().as_millis();
        if let Err(e) = result {
            debug!(engine_ms, "流式出图：引擎报错：{e:#}");
            return None;
        }
        let pdf = {
            let c = shared.lock().unwrap_or_else(|e| e.into_inner());
            c.files.get(pdf_name).cloned()
        };
        debug!(open_ms, engine_ms, xdv_bytes, "流式出图：一次转换的分段耗时");
        pdf.filter(|p| !p.is_empty())
    }
}

/// 原子替换（临时名 → rename）：前端可能正在读上一份，不能写坏它。
fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("pdf.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
