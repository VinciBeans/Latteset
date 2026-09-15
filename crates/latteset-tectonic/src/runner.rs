//! `TectonicLibRunner`：与 `LatexmkRunner` 同一个 [`CompileRunner`] trait 的**库形态**实现。
//!
//! 调用序列 = 方案 §3.4 的路径 B 九步（逐步对应见 `run_engines` 里的注释）：
//! 自持输出层 → `MinimalDriver` → `CoreBridgeLauncher` → `TexEngine` / `XdvipdfmxEngine`
//! （+ 条件档的 `XdvParser`）。
//!
//! **与子进程档的已知差异（必须一起读，别只看成功路径）**：
//! 1. **没有进程可树杀 → 协作式中止**（t24 / V-06）：取消置位后，我们的 `IoProvider` 在
//!    **下一个 I/O 回调**处返回错误（[`crate::io::CANCEL_MESSAGE`]），引擎随即中止该趟
//!    ——不再是"放弃等待、引擎跑完当前趟"。**中止延迟的上界 = 一次输出写 / 输入打开回调的间隔**
//!    （实测数字 `[未测到]`：本机没有可用 bundle，跑不了真编；补测前提同 P0/t10）。
//! 2. **页哈希显式不支持**（本阶段）：见 [`crate::PAGE_HASH_NOTE`]。
//! 3. **`.log` 落盘**：靠本层把输出镜像到 `tmp/`（≡ 子进程档的 `--keep-logs`）。
//! 4. **超时不重试、失败不回退**（D1/㉕ 同口径）：库形态失败**不**自动切回子进程。
//! 5. **不跑 bib 趟**（t24 / V-03，**如实收窄声明**）：本 crate 没有 `tectonic_engine_bibtex`
//!    依赖（本机 registry/ vendor 都没有它，加依赖会让本模块无法离线复现）⇒ 带 `\cite` /
//!    `\bibliography` / `\addbibresource` 的文档**不等价**于子进程档 Full：`.bbl` 不由我们生成，
//!    引用可能显示为未解析。检出引用时**显式 `warn!`**（不静默），见 [`needs_bib_pass`]。
//! 6. **单趟、不收敛**（t24 / V-04）：实际只跑一趟 TeX + 一次转换 ⇒ `CompileOutcome.kind`
//!    必须报 [`CompileKind::Quick`]（先前固定报 `Full` 是错的：那会让 ㉘ 的"引用待更新"/
//!    `draft` 语义失效）。真正收敛后再回来改成 `Full`。
//! 7. **PDF 落盘是原子替换**（t24 / V-05）：`{stem}.pdf.tmp` → `rename` 覆盖，与子进程档
//!    `crates/latteset-infra/src/runner.rs:615-630` 同口径（失败清理临时文件、旧 PDF 不被截断）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use async_trait::async_trait;
use latteset_core::log_parser::{MessageKind, diagnose, parse_log};
use latteset_core::project::FileSystem;
use latteset_core::scheduler::{CompileProgress, CompileRunner};
use latteset_core::types::{CompileKind, CompileOutcome, CompileRequest, ErrorEntry, ErrorKind};
use tectonic_bridge_core::{CoreBridgeLauncher, MinimalDriver};
use tectonic_engine_xdvipdfmx::XdvipdfmxEngine;
use tectonic_engine_xetex::{TexEngine, TexOutcome};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::bundle::{BundleSource, bundle_digest, open_bundle};
use crate::io::{SharedCapture, TectonicIo, new_capture};
use crate::status::ProgressStatus;
use crate::{PAGE_HASH_NOTE, PAGE_HASH_SUPPORTED};

/// 中间产物目录（与子进程档 `-o tmp` 一致）。
const OUT_DIR: &str = "tmp";

/// 是否需要 bib 趟（V-03 的**反例自证**）：出现下列任一条命令即视为"引用需要外部工具解析"。
///
/// 口径是**宁可多报**（注释里的 `\cite` 也算）：本函数只决定"要不要给用户一条可见警告"，
/// 误报的代价是一行日志，漏报的代价是"引用没解析却看着像成功"。
pub fn needs_bib_pass(src: &str) -> bool {
    ["\\cite", "\\bibliography", "\\addbibresource", "\\printbibliography", "\\nocite"]
        .iter()
        .any(|k| src.contains(k))
}

/// 检出引用但本轮不跑 bib 时的警告文案（V-03：如实收窄声明，不静默）。
pub const BIB_PASS_MISSING_NOTE: &str =
    "库形态本轮不跑 bib 趟（无 tectonic_engine_bibtex 依赖）⇒ 该文档的引用**可能未解析**，\
     与子进程档 Full 不等价（V-03）";

/// 库形态 runner。
pub struct TectonicLibRunner {
    pub fs: Arc<dyn FileSystem>,
    pub progress: Arc<dyn CompileProgress>,
    /// bundle 来源（默认兜底地址；本地 bundle 走 `BundleSource::Explicit`）。
    pub bundle: BundleSource,
    /// 产品缓存目录：`<cache>/bundles` 与 `<cache>/formats` 的父目录。
    /// **必须显式注入**（方案 §5.2 硬约束：format 默认落项目目录 ⇒ 不注入就会往用户项目扔 `.fmt`）。
    pub cache_dir: Option<PathBuf>,
    /// 只读缓存（等价子进程档的 `-C`；冷缓存会被上游拒绝 ⇒ 默认 false）。
    pub only_cached: bool,
}

impl TectonicLibRunner {
    pub fn new(fs: Arc<dyn FileSystem>, progress: Arc<dyn CompileProgress>) -> Self {
        Self {
            fs,
            progress,
            bundle: BundleSource::Default,
            cache_dir: None,
            only_cached: false,
        }
    }

    pub fn with_bundle(mut self, bundle: BundleSource) -> Self {
        self.bundle = bundle;
        self
    }

    pub fn with_cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(dir.into());
        self
    }

    pub fn with_only_cached(mut self, only_cached: bool) -> Self {
        self.only_cached = only_cached;
        self
    }
}

/// 根文件相对项目根的名字（posix 分隔；与 infra `latexmk_input` 同口径）。
fn input_name(root_file: &Path, project_root: &Path) -> String {
    root_file
        .strip_prefix(project_root)
        .unwrap_or(root_file)
        .to_string_lossy()
        .replace('\\', "/")
}

fn root_stem(root_file: &Path) -> String {
    root_file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".to_owned())
}

/// `.log` 文本 → 错误条目（与 infra 的 `entries_from_log` 同口径：终态含警告，
/// 诊断走 core 的 `diagnose`）。库形态的日志来自**我们 I/O 层捕获的 `<stem>.log`**。
fn entries_from_log(text: &str, warnings: bool) -> Vec<ErrorEntry> {
    parse_log(text)
        .into_iter()
        .filter(|m| warnings || m.kind == MessageKind::Error)
        .map(|m| ErrorEntry {
            message: m.message.clone(),
            file: m.file.clone(),
            line: m.line,
            kind: ErrorKind::ContentError,
            diagnosis: diagnose(&m),
        })
        .collect()
}

/// 引擎状态环的尾部（失败时补一行证据，与 IoError 分支同口径）。
fn log_tail(log_ring: &Arc<Mutex<Vec<String>>>) -> String {
    let ring = log_ring.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let tail: Vec<String> = ring.iter().rev().take(5).cloned().collect();
    if tail.is_empty() {
        String::new()
    } else {
        format!("；引擎状态尾部：{}", tail.join(" | "))
    }
}

/// 捕获表里某个输出已写的字节数。
fn capture_len(shared: &SharedCapture, name: &str) -> usize {
    shared.lock().unwrap_or_else(|e| e.into_inner()).files.get(name).map_or(0, Vec::len)
}

/// 从捕获表里**取走** dump 出来的 format（上游 `driver.rs:1825-1838`：遍历内存层所有 `*.fmt`）。
///
/// "取走"而不是"读一眼"：format 趟的产物不该留在捕获表里被后续逻辑当成项目产物
/// （上游在 format 趟结束后清空整个内存层，`driver.rs:1840-1841`）。
fn take_dumped_format(shared: &SharedCapture) -> Option<(String, Vec<u8>)> {
    let mut c = shared.lock().unwrap_or_else(|e| e.into_inner());
    let name = c.files.keys().find(|k| k.ends_with(".fmt")).cloned()?;
    let data = c.files.remove(&name)?;
    Some((name, data))
}

/// 输入请求的**分类计数**：`(内存直喂, 产物层, 项目磁盘, bundle, 未命中)`。
///
/// 用途：定位耗时（P2 复核）。库形态比子进程慢在哪，一眼要看的是"每类各被问了多少次"，
/// 而不是"总共多少次"（实测 CJK 1 页 727 次，但 5 次未命中 ⇒ 瓶颈不在 I/O 命中率）。
fn request_counts(requests: &[String]) -> (usize, usize, usize, usize, usize) {
    let count = |p: &str| requests.iter().filter(|r| r.starts_with(p)).count();
    (
        count("ram:"),
        count("mem:"),
        count("disk:"),
        count("bundle:"),
        count("missing:") + count("nobundle:"),
    )
}

/// 输入请求摘要（只给前几条缺失 + 总数；失败时用它说明"缺什么"）。
fn summarize(requests: &[String]) -> String {
    let missing: Vec<&str> = requests
        .iter()
        .filter(|r| r.starts_with("missing:") || r.starts_with("nobundle:"))
        .map(String::as_str)
        .take(5)
        .collect();
    if missing.is_empty() {
        format!("{} 条（无缺失标记）", requests.len())
    } else {
        format!("{} 条，其中缺失 {} 条：{}", requests.len(), missing.len(), missing.join(", "))
    }
}

/// 阻塞段：**在这里**才开 bundle（`Box<dyn Bundle>` 不是 `Send`，不能跨线程搬进
/// `spawn_blocking`；而 `BundleSource`/`PathBuf`/`Vec<u8>` 都是 `Send`），然后建 launcher、
/// 跑三趟（format → 排版 → 转换）、收尾落盘。
#[allow(clippy::too_many_arguments)]
fn run_engines(
    bundle_source: BundleSource,
    only_cached: bool,
    cache_dir: Option<PathBuf>,
    project_root: PathBuf,
    tmp_dir: PathBuf,
    root_bytes: Vec<u8>,
    stem: String,
    name: String,
    pdf_dst: PathBuf,
    progress: Arc<dyn CompileProgress>,
    log_ring: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<String>>>,
    shared: SharedCapture,
    cancel: Arc<AtomicBool>,
) -> CompileOutcome {
    let xdv_name = format!("{stem}.xdv");
    let pdf_name = format!("{stem}.pdf");
    let log_name = format!("{stem}.log");

    // ② bundle：`detect_bundle(source, only_cached, Some(产品缓存目录))`（§3.4 第 2 步）。
    let mut bundle = match open_bundle(&bundle_source, only_cached, cache_dir.clone()) {
        Ok(b) => b,
        Err(msg) => return CompileOutcome::IoError { message: msg },
    };
    // ②b **bundle digest**（V-02）：format 缓存的键。取不到就显式失败 —— 不做裸名回退，
    //    否则"换 bundle"会复用旧 format（正确性风险）。
    let digest = match bundle.as_mut() {
        Some(b) => match bundle_digest(&mut **b) {
            Ok(d) => Some(d),
            Err(msg) => return CompileOutcome::IoError { message: msg },
        },
        None => None,
    };
    // format 缓存落点先算好（在生产 `io` 之前，`cache_dir`/`digest` 还没被 move 走）：format 趟
    // 收 dump 时要用它，与 `io.format_path` 走**同一个**函数，避免两处各拼一次名字。
    //
    // ⚠ 目录口径：`<缓存根>/formats/`（与 `bundles/` 并列）——这正是上游 `FormatCache` 的布局，
    // 也解释器侧 `input_open_format` 的查找位置。先前这里用了缓存根，dump 会落到 `<缓存根>/`
    // 而排版趟去 `<缓存根>/formats/` 找 ⇒ 报 `cannot open the format file "latex"`。
    //
    // **算不出不算错**：`io.format_present()` 会因此判"未命中"，真正要生成 format 时（format 趟）
    // 才在下面显式报错——文案里说清是缺缓存目录还是缺 bundle digest（`BundleSource::None` 就是
    // 没 digest 的那种，此时本就建不出 LaTeX format）。
    let format_cache_dir = cache_dir.map(|d| d.join("formats"));
    let format_target = crate::io::format_cache_path(
        format_cache_dir.as_deref(),
        digest.as_deref(),
        crate::FORMAT_NAME,
    )
    .ok();
    let mut io = TectonicIo::new(
        project_root,
        Some(tmp_dir),
        bundle,
        digest,
        format_cache_dir,
        shared.clone(),
        requests.clone(),
        Some(cancel),
    );
    // 判据 5（V-03）：**先看文档要不要 bib 趟**，要就可见地警告（本轮不跑 bib，必须如实收窄声明）。
    let wants_bib = needs_bib_pass(&String::from_utf8_lossy(&root_bytes));
    if wants_bib {
        warn!(name = %name, "{BIB_PASS_MISSING_NOTE}");
    }
    // 判据 3：主文件**直喂内存**（方案 §4.1 HL-1）。
    io.inject(&name, root_bytes);
    let format_cached = io.format_present();
    // ③ format 趟开关（上游 `enter_format_mode`，`src/driver.rs:263-267`）：未命中缓存时，
    //    initex 趟的主输入必须是**合成的** `\input tectonic-format-latex.tex`，不是用户文档。
    //    标志要在 driver 拿走 `io` 之前挂上（之后 runner 只有 `&mut driver`，够不到 `io`）。
    let format_pass = Arc::new(AtomicBool::new(!format_cached));
    io.set_format_pass(format_pass.clone());

    // ① 自持输出层 → ② MinimalDriver → ③ CoreBridgeLauncher（`engine-spike/src/bin/xdv2pdf.rs:180-186`
    //    是实测同构的调用形态）。
    let mut driver = MinimalDriver::new(io);
    let mut status = ProgressStatus::new(progress, log_ring.clone());
    // 分阶段计时（P2 的复核证据：库形态的钱花在哪一趟）。命中 format 缓存时 format 趟为 0。
    let t_start = Instant::now();
    let mut phase_ms = (0u128, 0u128, 0u128);
    let engine_result: Result<(), String> = {
        let mut launcher = CoreBridgeLauncher::new(&mut driver, &mut status);
        (|| -> Result<(), String> {
            // ③ format 自管（§3.4 第 3 步；上游 `driver.rs:1779-1844` 的 `make_format_pass`）：
            //    命中即复用；否则 initex 趟生成 → 收 dump → 落产品缓存。
            if !format_cached {
                let mut init = TexEngine::default();
                // 上游同款三件套：halt_on_error（否则 Errors 只会体现在返回值里、日志被刷掉）、
                // initex_mode、以及显式 build_date（D-2：默认 `UNIX_EPOCH` 会把 `\today` 印成 1970）。
                init.halt_on_error_mode(true);
                init.initex_mode(true);
                init.build_date(SystemTime::now());
                // 主输入由 format 趟开关顶替（见上）；format 名传占位串；dumped 文件名由引擎按
                // input_file_name 取名（实测 `texput.fmt`）⇒ 收集按 `*.fmt` 后缀，不按名字。
                let outcome = init
                    .process(&mut launcher, crate::io::FORMAT_DUMP_PLACEHOLDER, "texput")
                    .map_err(|e| format!("生成 format 失败（initex 趟）：{e:#}"))?;
                // 上游 `driver.rs:1807-1819`：`Ok(Errors)` 也算失败 —— 先前忽略 `TexOutcome`
                // 会把"initex 没产出 format"当成功，接着排版趟必然报一堆 Undefined control sequence。
                if matches!(outcome, TexOutcome::Errors) {
                    let tail = log_tail(&log_ring);
                    return Err(format!(
                        "生成 format 失败：initex 趟报 Errors（LaTeX 宏没建起来）{tail}"
                    ));
                }
                // 排版趟要换回用户文档作为主输入。
                format_pass.store(false, Ordering::Relaxed);
                // 收 dump 并落产品缓存（上游 `driver.rs:1825-1838`：遍历内存层里所有 `*.fmt`）。
                let dumped = take_dumped_format(&shared);
                let Some((dump_name, bytes)) = dumped else {
                    return Err(
                        "initex 趟没有产出任何 `*.fmt` ⇒ 拒绝继续：没有 format 的排版趟只会报一堆 \
                         Undefined control sequence（引擎无 dump 时会静默返回成功）"
                            .to_owned(),
                    );
                };
                let target = format_target.as_ref().ok_or_else(|| {
                    "库形态需要生成 format，但算不出 format 缓存落点（未注入缓存目录，\
                     或拿不到 bundle digest）⇒ 拒绝把 format 写进项目目录（方案 §5.2 硬约束）"
                        .to_owned()
                })?;
                if let Some(parent) = target.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return Err(format!("创建 format 缓存目录失败（{}）：{e}", parent.display()));
                    }
                }
                if let Err(e) = std::fs::write(target, &bytes) {
                    return Err(format!("写 format 缓存失败（{}）：{e}", target.display()));
                }
                info!(
                    dump = %dump_name,
                    bytes = bytes.len(),
                    path = %target.display(),
                    "format 已生成并落产品缓存（V-02：文件名带 bundle digest）"
                );
            }
            phase_ms.0 = t_start.elapsed().as_millis();

            // ⑤ 排版趟（§3.4 第 4 步）。上游同款（`driver.rs:1874-1885`）：
            //    halt_on_error（默认已是 true，显式写出来）、synctex、显式 build_date。
            //    **D-2 / D4 纪律**：`TexEngine::default()` 的 build_date 是 `UNIX_EPOCH`
            //    （`engine_xetex/src/lib.rs:92-96`），忘写就会把 `\today` 静默印成 1970-01-01；
            //    库内**禁止** `build_date_from_env`（进程级全局，会污染同进程其它步骤）。
            let mut tex = TexEngine::default();
            tex.halt_on_error_mode(true);
            tex.synctex(true); // ≡ 子进程档的 `--synctex`
            tex.build_date(SystemTime::now());
            tex.process(&mut launcher, crate::FORMAT_NAME, &name)
                .map_err(|e| format!("排版趟失败：{e:#}"))?;
            phase_ms.1 = t_start.elapsed().as_millis();
            // 上游 `driver.rs:1900-1904`：TeX 没产出预期输出文件时要**明说**（多因文档为空），
            // 否则错误会以"XDV→PDF 失败"的形式出现、指向错误的方向。
            if capture_len(&shared, &xdv_name) == 0 {
                return Err(format!(
                    "排版趟没有产出 `{xdv_name}`（空文档或引擎提前停下）；I/O 层输入请求：{}",
                    summarize(
                        &requests.lock().unwrap_or_else(|e| e.into_inner()).clone()
                    )
                ));
            }

            // ⑥ 转换趟（§3.4 第 7 步）：XDV/PDF 以**我们 I/O 层的名字**可达；t5 §2.2 实测整份可行。
            let mut pdf_engine = XdvipdfmxEngine::default();
            pdf_engine
                .process(&mut launcher, &xdv_name, &pdf_name)
                .map_err(|e| format!("XDV→PDF 失败：{e:#}"))?;
            phase_ms.2 = t_start.elapsed().as_millis();
            Ok(())
        })()
    };
    drop(driver);

    // ⑧ 收尾（§3.4 第 9 步）：产物落点与子进程档一致。`tmp/` 里的副本由 I/O 层的镜像写完；
    //    这里取内存文件表，把 PDF 拷到项目根、把 `.log` 作为权威错误来源。
    let files: HashMap<String, Vec<u8>> = {
        let c = shared.lock().unwrap_or_else(|e| e.into_inner());
        c.files.clone()
    };
    let requests = requests.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let log_text = files
        .get(&log_name)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();

    match engine_result {
        Ok(()) => {
            let pdf = files.get(&pdf_name).map(Vec::as_slice).unwrap_or(&[]);
            if pdf.is_empty() {
                return CompileOutcome::IoError {
                    message: format!(
                        "库形态编译成功但 I/O 层没拿到 `{pdf_name}`（输入请求：{}）",
                        summarize(&requests)
                    ),
                };
            }
            // V-05：**原子替换**（写 `{stem}.pdf.tmp` → `rename` 覆盖）——与子进程档
            // `crates/latteset-infra/src/runner.rs:615-630` 同口径，失败时旧 PDF 保持完整。
            let pdf_tmp = pdf_dst.with_extension("pdf.tmp");
            if let Err(e) = std::fs::write(&pdf_tmp, pdf).and_then(|()| std::fs::rename(&pdf_tmp, &pdf_dst)) {
                // V-05：失败时清理临时文件，**旧 PDF 不被截断**（与子进程档同口径）。
                let _ = std::fs::remove_file(&pdf_tmp);
                return CompileOutcome::IoError {
                    message: format!(
                        "拷贝 PDF 到项目根失败（原子替换 {} → {}）：{e}",
                        pdf_tmp.display(),
                        pdf_dst.display()
                    ),
                };
            }
            if !PAGE_HASH_SUPPORTED {
                // 判据 6：不能拿就必须**显式登记**，不许静默跳过。
                info!("{PAGE_HASH_NOTE}");
            }
            debug!(
                pdf_bytes = pdf.len(),
                log_bytes = log_text.len(),
                input_requests = requests.len(),
                "库形态编译完成（页哈希显式不支持：空表 = 无法判定）"
            );
            // 分阶段耗时（命中缓存时 format_ms = 0）：P2/P4 复核用的一行证据。
            let (n_ram, n_mem, n_disk, n_bundle, n_miss) = request_counts(&requests);
            info!(
                format_ms = phase_ms.0,
                typeset_ms = phase_ms.1.saturating_sub(phase_ms.0),
                convert_ms = phase_ms.2.saturating_sub(phase_ms.1),
                engine_total_ms = phase_ms.2,
                format_cached,
                req_ram = n_ram,
                req_mem = n_mem,
                req_disk = n_disk,
                req_bundle = n_bundle,
                req_miss = n_miss,
                "库形态分阶段耗时"
            );
            CompileOutcome::Success {
                pdf_path: pdf_dst,
                // V-04：**按实际趟数报**。本轮只跑一趟 TeX + 一次转换（不收敛、不跑 bib），
                // 语义 = 子进程档的 `-r 0`（Quick）⇒ 必须报 `Quick`：这样 ㉘ 的 `draft` /
                // 「引用待更新」才会亮，用户不会把"引用没解析"当成"已收敛"。
                // 真正实现多趟收敛 + bib 之后再改成 `Full`（见模块头注 5/6）。
                kind: CompileKind::Quick,
                page_hashes: Vec::new(),
            }
        }
        Err(msg) => {
            // 判据 4：失败不静默——优先给结构化错误列表（`.log` 是权威），否则 IoError + 输入请求证据。
            let errors = entries_from_log(&log_text, true);
            if !errors.is_empty() {
                debug!(count = errors.len(), "库形态失败：已从 I/O 层捕获的 .log 解析出错误条目");
                CompileOutcome::ContentError { errors }
            } else {
                let ring = log_ring.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let tail: Vec<String> = ring.iter().rev().take(5).cloned().collect();
                CompileOutcome::IoError {
                    message: format!(
                        "{msg}；I/O 层输入请求：{}；引擎状态尾部：{}",
                        summarize(&requests),
                        if tail.is_empty() { "（无）".to_owned() } else { tail.join(" | ") }
                    ),
                }
            }
        }
    }
}

#[async_trait]
impl CompileRunner for TectonicLibRunner {
    async fn compile(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome {
        let stem = root_stem(&req.root_file);
        let name = input_name(&req.root_file, &req.project_root);
        let tmp_dir = req.project_root.join(OUT_DIR);
        let pdf_dst = req.project_root.join(format!("{stem}.pdf"));

        // 方案 §5.4 / P-G4：输出目录由产品自建（Tectonic 自己不会建 `-o` 目录）。
        if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
            debug!(dir = %tmp_dir.display(), "创建 tmp/ 目录失败（继续尝试编译）：{e}");
        }

        // 判据 3：主文件**直喂内存**（方案 §4.1 HL-1）。读取在 async 侧（走注入的 FileSystem）。
        let root_bytes = match self.fs.read_bytes(&req.root_file).await {
            Ok(b) => b,
            Err(e) => {
                return CompileOutcome::IoError {
                    message: format!("读取根文件失败（{}）：{e}", req.root_file.display()),
                };
            }
        };

        // bundle 的**打开**放到阻塞段里做（`Box<dyn Bundle>` 不是 `Send`，见 `run_engines` 头注）。
        let shared: SharedCapture = new_capture();
        let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log_ring: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        // V-06：协作式取消标志。置位后 `TectonicIo` 的每个入口立刻报错 ⇒ 引擎在**下一个
        // I/O 回调**处中止，而不是跑完当前趟（这是 LIB-9「下个回调报错」口径的落地）。
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let cancel_for_task = cancel_flag.clone();
        let progress = self.progress.clone();
        let bundle_source = self.bundle.clone();
        let only_cached = self.only_cached;
        let cache_dir = self.cache_dir.clone();
        let project_root = req.project_root.clone();
        let blocking = tokio::task::spawn_blocking(move || {
            run_engines(
                bundle_source,
                only_cached,
                cache_dir,
                project_root,
                tmp_dir,
                root_bytes,
                stem,
                name,
                pdf_dst,
                progress,
                log_ring,
                requests,
                shared,
                cancel_for_task,
            )
        });

        // 超时/取消：库形态**无法树杀**，但**不是**"只能放弃等待"——置协作式取消标志后，
        // `TectonicIo` 会在下一个 I/O 回调处返回错误，引擎随即中止该趟（V-06 / LIB-9）。
        // 上界 = 一次输出写 / 输入打开回调的间隔；**实测延迟 `[未测到]`**（本机没有可用 bundle、
        // 跑不了真编；补测前提 = P0/t10 的本地 bundle）。`ENGINE_LOCK` 仍保证进程内不并发两个引擎。
        tokio::select! {
            joined = blocking => match joined {
                Ok(outcome) => outcome,
                Err(e) => CompileOutcome::IoError {
                    message: format!("库形态编译线程异常终止（panic）：{e}"),
                },
            },
            _ = tokio::time::sleep(req.timeout) => {
                warn!(timeout = req.timeout.as_secs(), "库形态编译超时：置协作式取消标志（引擎在下一个 I/O 回调处中止）");
                cancel_flag.store(true, Ordering::Relaxed);
                CompileOutcome::Timeout {
                    entry: ErrorEntry {
                        message: format!(
                            "库形态编译超过 {}s 上限：库形态没有子进程可树杀，已置协作式取消标志 \
                             （引擎会在下一个 I/O 回调处中止；中止延迟上界 = 一次输出写/输入打开回调的间隔，\
                             实测数字 [未测到]）。建议提高超时后重试，或切回子进程形态。",
                            req.timeout.as_secs()
                        ),
                        file: None,
                        line: None,
                        kind: ErrorKind::Timeout,
                        diagnosis: None,
                    },
                }
            }
            _ = cancel.cancelled() => {
                warn!("库形态编译收到手动终止：置协作式取消标志（引擎在下一个 I/O 回调处中止）");
                cancel_flag.store(true, Ordering::Relaxed);
                CompileOutcome::Aborted
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// V-03 的反例自证：**带 `\cite` 的夹具必须被标为"需要 bib、本轮不等价"**。
    #[test]
    fn citation_fixtures_are_flagged_as_needing_a_bib_pass() {
        let with_cite = "\\documentclass{article}\n\\begin{document}\n见 \\cite{ref1}。\n\\end{document}\n";
        assert!(needs_bib_pass(with_cite), "带 \\cite 的文档必须被检出（否则会静默假等价）");

        let with_bib = "\\bibliography{refs}\n\\bibliographystyle{plain}\n";
        assert!(needs_bib_pass(with_bib), "\\bibliography 也要检出");

        let biblatex = "\\addbibresource{refs.bib}\n\\printbibliography\n";
        assert!(needs_bib_pass(biblatex), "biblatex 口径也要检出");

        // 反例：不带任何引用命令的文档不该被误标（否则警告会变成噪音）
        let plain = "\\documentclass{article}\n\\begin{document}\n你好。\n\\end{document}\n";
        assert!(!needs_bib_pass(plain), "无引用的文档不该报 bib 缺失");
    }

    /// V-03 的声明文案必须点明"不等价"，不能只说"少跑一步"。
    #[test]
    fn bib_note_states_the_inequivalence() {
        assert!(BIB_PASS_MISSING_NOTE.contains("可能未解析"), "{BIB_PASS_MISSING_NOTE}");
        assert!(BIB_PASS_MISSING_NOTE.contains("不等价"), "{BIB_PASS_MISSING_NOTE}");
    }
}

