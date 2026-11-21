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
//! 2. **页哈希已支持**（2026-09-15，任务 3）：XDV 就在捕获表里，喂
//!    [`latteset_core::xdv::page_hashes`] —— 与子进程档同一函数 ⇒ 页哈希**逐页可比**，
//!    A/B/C 三个功能点在库形态档同样成立（A 只对 Quick，见 ⑥ 段）。
//! 3. **`.log` 落盘**：靠本层把输出镜像到 `tmp/`（≡ 子进程档的 `--keep-logs`）。
//! 4. **超时不重试、失败不回退**（D1/㉕ 同口径）：库形态失败**不**自动切回子进程。
//! 5. **bib 趟已支持，且输入未变时跳过**（2026-09-15 收口 V-03）：`.aux` 出现 `\bibdata` 时跑
//!    `BibtexEngine` 并**重跑一趟 TeX**（上游 `default_pass` 同序）⇒ `\bibliography{...}` + `.bib`
//!    的文档引用会解析成编号、参考文献表会印出来。**输入没变就跳过**那条路见
//!    [`bib_signature`]：bibtex 的输出只取决于引用集合与 `.bib` 状态，两者与产出 `.bbl` 那次一致时
//!    重跑只会得到同一份 `.bbl`，却会连带多跑一趟排版。**biber 仍不支持**（biblatex 需要外部 `biber`，
//!    而本引擎不运行外部工具）：检出 `<stem>.run.xml` 里的 `<binary>biber</binary>` 时显式 `warn!`，
//!    见 [`crate::BIBER_PASS_MISSING_NOTE`]。
//! 6. **收敛判定**（t24 / V-04，2026-09-15 扩到编辑触发档）：重跑循环比较 rerun 相关中间产物，
//!    稳定即算收敛 ⇒ `CompileOutcome.kind` 报 [`CompileKind::Full`]，否则退回
//!    [`CompileKind::Quick`]（先前固定报 `Full` 是错的：那会让 ㉘ 的"引用待更新"/`draft` 语义失效）。
//!    **`Quick` 请求同样走这条判定**，只是多一道预算闸门（`QUICK_CONVERGENCE_BUDGET`）：
//!    预算内收敛就按实际结果报 `Full`（编辑后直接是正确结果），超预算则停在草稿态、交给空闲收敛。
//!    详见 [`crate::CONVERGENCE_SUPPORTED`]。
//! 7. **PDF 落盘是原子替换**（t24 / V-05）：`{stem}.pdf.tmp` → `rename` 覆盖，与子进程档
//!    `crates/latteset-infra/src/runner.rs:615-630` 同口径（失败清理临时文件、旧 PDF 不被截断）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use async_trait::async_trait;
use latteset_core::log_parser::{
    DiagnosisKind, MessageKind, TimeoutEvidence, diagnose, diagnose_timeout, pages_typeset,
    parse_log, source_release_hint,
};
use latteset_core::project::FileSystem;
use latteset_core::scheduler::{CompileProgress, CompileRunner, NoProgress};
use latteset_core::types::{CompileKind, CompileOutcome, CompileRequest, ErrorEntry, ErrorKind};
use tectonic_bridge_core::{CoreBridgeLauncher, MinimalDriver};
use tectonic_bundles::Bundle;
use tectonic_engine_bibtex::{BibtexEngine, BibtexOutcome};
use tectonic_engine_xdvipdfmx::XdvipdfmxEngine;
use tectonic_engine_xetex::{TexEngine, TexOutcome};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::bundle::{BundleSource, bundle_digest, open_bundle};
use crate::io::{SharedCapture, TectonicIo, new_capture};
use crate::status::ProgressStatus;

/// 中间产物目录（与子进程档 `-o tmp` 一致）。
const OUT_DIR: &str = "tmp";

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

/// 捕获表里 XDV 的**已完成页数**（页进度事件的产端，D3③）。
///
/// 内嵌形态没有子进程 stdout 可扫 `[N]`（那是子进程形态的页标记通道），但捕获表里的
/// `<stem>.xdv` 就是**权威页表**：`core::xdv::complete_pages` 直接在字节流上切完整页。
/// 每趟排版收尾调一次（O(XDV 字节)，与一次页哈希同量级），数值变大才发事件。
fn captured_pages(shared: &SharedCapture, xdv_name: &str) -> u32 {
    let bytes = capture_bytes(shared, xdv_name).unwrap_or_default();
    latteset_core::xdv::complete_pages(&bytes).len().try_into().unwrap_or(u32::MAX)
}


/// 重跑上限（上游 `DEFAULT_MAX_TEX_PASSES = 6`）：防"每趟都在变"的文档把编译拖死。
const MAX_TEX_PASSES: usize = 6;

/// **编辑触发档的收敛预算**（2026-09-15）：把「草稿」从固定档变成**判定结果**。
///
/// 编辑触发的请求原先无条件单趟即停（"引用/目录落后一趟"就是草稿的语义，由 ㉘ 的
/// 「引用待更新」+ 空闲收敛兜底）。库形态里这个前提不成立：重跑循环与中间产物指纹本来就在，
/// 而"这一趟到底落没落后"是**可判定**的 ⇒ 让它在预算内自己跑到稳定，收敛就报 `Full`。
/// 前端 `draft` 取自**实际**强度（`scheduler/actor.rs` 终态按 `outcome.kind` 报）⇒ 不亮提示，
/// 且 `useIdleConvergence` 只看 `draft` ⇒ 也不会再排一次多余的 Full。
///
/// 预算取 2000 ms **与前端空闲收敛的 `DELAY_MS` 同值**：编辑触发档只要能在"那 2 s 等待"之内
/// 收敛，就严格优于"先给落后档、等 2 s 再补一趟"。超预算（或第 1 趟本身就慢，见
/// [`QUICK_FIRST_PASS_MAX`]）⇒ 停在草稿态，行为与今天完全一致。
const QUICK_CONVERGENCE_BUDGET: Duration = Duration::from_millis(2000);

/// 编辑触发档最多跑几趟（`Full` 的上限仍是 [`MAX_TEX_PASSES`]）。
/// 取 3 的依据：实测冷启收敛档最坏就是 3 趟（`cite` 夹具 954 ms），再多就该交给空闲收敛。
const QUICK_MAX_PASSES: usize = 3;

/// 编辑触发档"继续追收敛"的**额外前提：第 1 趟本身要够快**（= [`QUICK_CONVERGENCE_BUDGET`] 的一半，
/// 关系由单测钉住 —— `Duration` 的除法不是 const fn，这里只能写字面量）。
///
/// 为什么不能只看总预算：预算是**跑完一趟之后**才检查的。实测反例（125 页 / 462 KB 夹具，
/// 2026-09-15）：一趟 1.7 s，`elapsed < 预算` 成立 ⇒ 又跑了一趟，两次共 **3846 ms**，而中间产物
/// 仍在变 ⇒ 最终还是 `Quick`（草稿态）。也就是说只看总预算时，大文档要**白付一趟**才能回到原行为。
///
/// 所以文档一旦"一趟就接近预算"，就停在草稿态（与 ㉘ 原样一致），正确性交给空闲收敛去追。
/// 1 s 的取舍：一趟 ≤1 s 的文档，"编辑触发档自己收敛"（1–3 趟、≤2 s）严格快于
/// "草稿 + 2 s 等待 + 再补一趟"；一趟 >1 s 的文档，多跑一趟的代价已经吃掉全部好处。
const QUICK_FIRST_PASS_MAX: Duration = Duration::from_millis(1000);

/// 超时后给协作式取消的**收尾等待**：置标志 → 引擎在下一个 I/O 回调处中止 → `.log`/状态环
/// 才完整。超过这个上界就不再等（引擎可能卡在 C 侧一段长计算里，那正是需要如实报告的情形）。
const TIMEOUT_CANCEL_GRACE: Duration = Duration::from_millis(1500);

/// 编辑触发档还能不能再跑一趟（纯函数：单测直接打边界）。
///
/// `passes` 是**已经跑完**的趟数（≥1），`first_pass` 是第 1 趟的墙钟，`loop_elapsed` 从排版循环
/// 开始计时（不含 format 趟 —— 冷缓存生成 `.fmt` 的开销不该吃掉编辑触发档的额度）。
fn quick_pass_allowed(passes: usize, first_pass: Duration, loop_elapsed: Duration) -> bool {
    passes < QUICK_MAX_PASSES
        && first_pass < QUICK_FIRST_PASS_MAX
        && loop_elapsed < QUICK_CONVERGENCE_BUDGET
}

/// 快照 rerun 相关中间产物的内容摘要（名字 → 摘要）。两趟之间比较它决定要不要再跑。
///
/// 覆盖哪些后缀见 [`crate::RERUN_EXTENSIONS`]（那张表同时决定"哪些 `tmp/` 副本可以当输入"）。
fn rerun_snapshot(shared: &SharedCapture) -> std::collections::BTreeMap<String, u64> {
    use std::hash::{Hash, Hasher};
    let c = shared.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = std::collections::BTreeMap::new();
    for (name, data) in c.files.iter() {
        if crate::RERUN_EXTENSIONS.iter().any(|ext| name.ends_with(ext)) {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            data.hash(&mut h);
            out.insert(name.clone(), h.finish());
        }
    }
    out
}

/// 把**上一趟留在 `tmp/` 的中间产物预热进内存层**。
///
/// 为什么需要：重跑判据是"这一趟写出来的 vs 这一趟开始前已有的"，而"开始前已有的"如果不含
/// `tmp/` 里的上一趟副本，就永远是空的 ⇒ **每次编译都必跑 2 趟**（实测：清 tmp 后 2 趟、
/// 不清也是 2 趟）。预热之后，重复编译一份**已经收敛**的文档只需 1 趟。
///
/// 与 [`TectonicIo::disk_path`] 的 `tmp/` 兜底是同一件事的两面：那个让引擎读得到，
/// 这个让重跑判据看得见。两处都用 [`crate::RERUN_EXTENSIONS`]，不会漂移。
///
/// **必须递归**：`\include` 分章时子目录里也有要回读的中间产物（`chapters/chNN.aux`），
/// 只看 `tmp/` 顶层会让它们每趟都算"新出现" ⇒ 多文件档永远收敛不了（实测：28 页 8 章档
/// 去掉递归后每轮都稳定多跑一趟排版）。键要用引擎的**相对名**（正斜杠），
/// 否则 `chapters\ch01.aux` 与引擎写的 `chapters/ch01.aux` 对不上、预热等于没做。
fn seed_previous_intermediates(shared: &SharedCapture, tmp_dir: &Path) {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    let mut stack = vec![tmp_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue; // 首次编译没有 tmp/，正常
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(rel) = path.strip_prefix(tmp_dir) else {
                continue;
            };
            let name = rel.to_string_lossy().replace('\\', "/");
            if crate::RERUN_EXTENSIONS.iter().any(|ext| name.ends_with(ext)) {
                found.push((name, path));
            }
        }
    }
    // 读盘在锁外做完再入库：捕获表的锁在编译期是热的，不该抱着它做 IO。
    let mut c = shared.lock().unwrap_or_else(|e| e.into_inner());
    for (name, path) in found {
        if let Ok(bytes) = std::fs::read(&path) {
            c.files.insert(name, bytes);
        }
    }
}

/// 捕获表里某个输出已写的字节数。
fn capture_len(shared: &SharedCapture, name: &str) -> usize {
    shared.lock().unwrap_or_else(|e| e.into_inner()).files.get(name).map_or(0, Vec::len)
}

/// PDF 产物身份（大小 + mtime 纳秒）：证明"磁盘上的 `<stem>.pdf` 仍是我们上次写出的那一份"。
///
/// 为什么需要它（D-L4b）：A 闸门原来只对 Quick 生效，Full 一定重跑转换（"完整刷新一遍"）。
/// 产品已批准 Full 在**页哈希全同**时也跳过转换；但"页哈希相同"只证明排版结果没变，**不证明**
/// 项目根那个 PDF 还是我们产出的 —— 用户可能手改/替换过它。所以再加一道磁盘身份判据：
/// 上次转换成功后把 PDF 的 `(len, mtime)` 记进 `tmp/<stem>.tectonic.pdfid`；下次只有当磁盘
/// PDF 的身份与记录一致（未被手改/替换/删除）才复用。缺记录（老项目/清过 tmp）⇒ 保守转换一次。
///
/// **已知限制（T22 B4，已接受）**：判据是 `(len, mtime_ns)`，不哈希 PDF 内容。若有人手工改 PDF
/// 且**同时**伪造出完全相同长度与 mtime（常规编辑/复制/另存都会改 mtime），则检测不到、会错误复用。
/// 这是需要刻意伪造的低风险场景；不引入整份 PDF 哈希是为了不给每次 Full 复用加一次 O(PDF) 读 + 哈希。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PdfIdentity {
    len: u64,
    mtime_ns: u128,
}

/// 取磁盘文件的产物身份；文件不存在/不是文件/时间戳不可读 ⇒ `None`（= 不能证明可复用）。
fn pdf_identity(path: &Path) -> Option<PdfIdentity> {
    let md = std::fs::metadata(path).ok()?;
    if !md.is_file() {
        return None;
    }
    let mtime_ns = md.modified().ok()?.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_nanos();
    Some(PdfIdentity { len: md.len(), mtime_ns })
}

/// PDF 身份记录的落点：`<tmp>/<stem>.tectonic.pdfid`（与页哈希缓存同目录；不进 `tmp/` 以外的任何地方）。
fn pdf_identity_cache_path(tmp_dir: &Path, stem: &str) -> PathBuf {
    tmp_dir.join(format!("{stem}.{}.pdfid", latteset_core::types::ENGINE_ID))
}

/// 身份记录的口径版本（与页哈希缓存一样带首行标记；不认识就当"没有记录"⇒ 保守转换）。
const PDF_IDENTITY_VERSION: &str = "v1";

fn format_pdf_identity(id: &PdfIdentity) -> String {
    format!("{PDF_IDENTITY_VERSION}\n{}\t{}\n", id.len, id.mtime_ns)
}

fn parse_pdf_identity(text: &str) -> Option<PdfIdentity> {
    let mut lines = text.lines();
    if lines.next()?.trim() != PDF_IDENTITY_VERSION {
        return None;
    }
    let line = lines.find(|l| !l.trim().is_empty())?;
    let (len, mtime) = line.trim().split_once('\t')?;
    Some(PdfIdentity { len: len.parse().ok()?, mtime_ns: mtime.parse().ok()? })
}

/// A 闸门的第三条件：磁盘 PDF 与上次记录的产物身份一致（未被手工改/替换/删除）。
fn pdf_unchanged_since_we_wrote_it(pdf: &Path, identity_cache: &Path) -> bool {
    let cached = std::fs::read_to_string(identity_cache)
        .ok()
        .and_then(|t| parse_pdf_identity(&t));
    cached.zip(pdf_identity(pdf)).is_some_and(|(recorded, on_disk)| recorded == on_disk)
}

/// 功能点 **A** 的判据（D-L4b：Quick/Full 共用）：页逐页相同 **且** 磁盘 PDF 仍是上次产出的那一份。
///
/// 反例全部挡住：`prev` 为 `None`（首轮/口径不符）或页数/页序不同（`prev == Some(hashes)`）、
/// 空表（"无法判定"，不是"零页"）、PDF 被手工改/替换/删除（身份不一致）。
fn should_reuse_pdf(hashes: &[u64], prev: Option<&[u64]>, pdf_identity_ok: bool) -> bool {
    !hashes.is_empty() && prev == Some(hashes) && pdf_identity_ok
}

/// `LATTESET_A_FULL=0|false|off|no` 可关掉"Full 也复用"（A/B 复核用；默认开 = 产品已批准行为）。
fn full_reuse_enabled() -> bool {
    match std::env::var("LATTESET_A_FULL") {
        Ok(v) => !matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "false" | "off" | "no"),
        Err(_) => true,
    }
}


/// 捕获表里某个输出的字节（页哈希要用**原始字节**，不能走文本解码）。
fn capture_bytes(shared: &SharedCapture, name: &str) -> Option<Vec<u8>> {
    shared.lock().unwrap_or_else(|e| e.into_inner()).files.get(name).cloned()
}

/// 捕获表里某个输出的文本（有损解码：`.blg`/`.log` 不保证合法 UTF-8）。
fn capture_text(shared: &SharedCapture, name: &str) -> String {
    let c = shared.lock().unwrap_or_else(|e| e.into_inner());
    c.files.get(name).map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default()
}

/// 跑一趟 TeX（上游 `tex_pass` 每次新建一个引擎实例，这里照抄）。
///
/// `halt_on_error(true)` 与显式 `build_date` 的理由见调用点注释（D-2 / D4）。
fn run_tex_pass(
    launcher: &mut CoreBridgeLauncher<'_>,
    input: &str,
    what: &str,
) -> Result<(), String> {
    let mut tex = TexEngine::default();
    tex.halt_on_error_mode(true);
    tex.synctex(true); // ≡ 子进程档的 `--synctex`
    tex.build_date(SystemTime::now());
    tex.process(launcher, crate::FORMAT_NAME, input)
        // `Ok(TexOutcome::Errors)` **不在此处失败**：与上游 `tex_pass` 同口径 —— 排版趟的错误由
        // 收尾时的 `.log` 解析成结构化 `ContentError` 交给用户，这里只拦引擎硬错。
        .map(|_outcome| ())
        .map_err(|e| format!("{what}失败：{e:#}"))
}

/// 哪些 `.aux` 要求跑 BibTeX（上游 `is_bibtex_needed`：**`.aux` 里出现 `\bibdata`**）。
///
/// 为什么按 aux 而不是扫源码：`\cite` 配 `thebibliography` 时**不需要** BibTeX，而
/// `\bibliography{refs}` 才会往 `.aux` 写 `\bibdata`。按源码里的 `\cite` 判会白跑一趟。
///
/// 与上游的一处**有意收窄**：上游 `bibtex_pass` 对该层里**所有** `.aux` 各跑一次（`\include`
/// 分章时章 aux 也在内），而章 aux 通常没有 `\bibdata` ⇒ BibTeX 只会吐一句
/// `I found no \bibdata command`。我们只对**含 `\bibdata` 的 aux** 跑：单文件情形与上游完全一致，
/// 分章 + 每章各带 `\bibliography` 的情形也照样覆盖，只是不再产生那句噪音。
fn aux_files_requesting_bibtex(shared: &SharedCapture, primary_aux: &str) -> Vec<String> {
    const BIBDATA: &[u8] = b"\\bibdata";
    let c = shared.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = Vec::new();
    // 主 aux 排在最前（上游口径：`tex_aux_path` 优先）
    if let Some(f) = c.files.get(primary_aux) {
        if f.windows(BIBDATA.len()).any(|s| s == BIBDATA) {
            out.push(primary_aux.to_owned());
        }
    }
    for (name, f) in c.files.iter() {
        if name.ends_with(".aux")
            && name != primary_aux
            && f.windows(BIBDATA.len()).any(|s| s == BIBDATA)
        {
            out.push(name.clone());
        }
    }
    out
}

/// 捕获表里是否**出现过**某个输出名（`\openout` 打开过它，哪怕内容为空）。
fn has_capture(shared: &SharedCapture, name: &str) -> bool {
    shared.lock().unwrap_or_else(|e| e.into_inner()).files.contains_key(name)
}

/// 是否**真的**请求 biber（上游 `check_biber_requirement`：biblatex 会写 `<主文件名>.run.xml`）。
///
/// ⚠ 判据**不能只看 `.run.xml` 是否存在**（阶段① review E2 实测）：biblatex 的
/// `backend=bibtex` 也写 `.run.xml`，但里面是 `<binary>bibtex</binary>`、`biber` **0 处**，
/// 参考文献照常生成 —— 只看文件存在就会把这类文档误报成"需要 biber"。
/// 上游 Tectonic 自己的判据也是扫 XML 里的 `<binary>` 文本
/// （`test_file/tectonic-src/src/driver.rs:2147-2167`），看不到 `biber` 就不跑 biber。
fn run_xml_requests_biber(shared: &SharedCapture, stem: &str) -> bool {
    let text = capture_text(shared, &format!("{stem}.run.xml"));
    run_xml_binary_requests_biber(&text)
}

/// 纯文本判定（见 [`run_xml_requests_biber`]）：扫每个 `<binary>` 元素的文本，等于 `biber` 才算。
///
/// 读不到 / 没有闭合标签 → **不算**（宁可漏报也不误报：误报会把参考文献已生成的文档说成不支持）。
fn run_xml_binary_requests_biber(text: &str) -> bool {
    const OPEN: &str = "<binary>";
    const CLOSE: &str = "</binary>";
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        rest = &rest[start + OPEN.len()..];
        let Some(end) = rest.find(CLOSE) else { return false };
        if rest[..end].trim() == "biber" {
            return true;
        }
        rest = &rest[end + CLOSE.len()..];
    }
    false
}

/// 三类外部工具的**能力边界提示**（D3①；ADR-0015 第 4 条：检出即如实说明，不静默降级）。
///
/// 判据是工具自己的中间产物：`<stem>.run.xml` 里真的请求了 `<binary>biber</binary>`（biblatex）、
/// `<stem>.idx`（makeindex）、`<stem>.glo`（glossaries）。
///
/// 三类工具的实际形态不同（阶段② T9 真机实测，2026-11）：
/// - **biber**：**外部程序**（不随本产品分发），而**内嵌引擎不运行外部工具**（没有上游子进程驱动的
///   `external_tool_pass`）⇒ biber 在 PATH 上也不会被跑、不产 `.bbl`，引用/参考文献可能停在未解析状态。
///   **不得沿用子进程形态的「引擎调用 biber、缺失/版本不匹配才失败」口径**（那是子进程的事实）；
///   出路是 biblatex 改 `backend=bibtex`（走内建 BibtexEngine）或先自行生成 `.bbl`。
/// - **makeindex / glossaries**：引擎**不会**自动运行它们（只跳过那一步）⇒ "编译成功但索引/术语表为空"。
///
/// 文案只给**本产品内真实可行的出路**（Tectonic 自带 bundle、不依赖 TeX Live；"换用完整 TeX Live"
/// 在产品里没有对应操作，等于没给出路）。
const BIBER_NOTICE: &str = "检测到该文档需要外部 biber（biblatex）：内嵌引擎不运行外部工具，\
     引用/参考文献可能停在未解析状态（不会生成 .bbl）。出路：① biblatex 写 \
     \\usepackage[backend=bibtex]{biblatex}（走内建 BibTeX，正常生成 .bbl，改动最小）；\
     ② 或改用传统 \\bibliography{...} + \\bibliographystyle{...}（同样走内建 BibTeX）；\
     ③ 留在 biber 上：先在本机命令行用 biber 生成 .bbl 再编译（引擎会读项目里的 .bbl；\
     本机 biber 的版本要与文档的 biblatex 匹配），或改用支持外部工具的其它构建流程；\
     ④ 先接受引用显示为空。";

const MAKEINDEX_NOTICE: &str = "检测到文档需要 makeindex（索引）：本引擎不会自动运行 makeindex，\
     生成的 PDF 里索引为空。出路：暂时去掉 \\makeindex/\\printindex（在正文里手工列出条目），或在命令行对 \
     tmp/<stem>.idx 跑一次 makeindex 后再编译；也可以接受索引为空。";

const GLOSSARIES_NOTICE: &str = "检测到文档需要 glossaries（术语表）：本引擎不会自动运行 \
     makeglossaries，术语表为空。出路：暂时去掉 \\makeglossaries/\\printglossaries，或在命令行跑一次 \
     makeglossaries 后再编译；也可以接受术语表为空。";

/// 编译**成功后**要报给用户的外部工具提示（无命中 = 空表；顺序即提示顺序）。
fn external_tool_notices(shared: &SharedCapture, stem: &str) -> Vec<&'static str> {
    let mut notices = Vec::new();
    if run_xml_requests_biber(shared, stem) {
        notices.push(BIBER_NOTICE);
    }
    if has_capture(shared, &format!("{stem}.idx")) {
        notices.push(MAKEINDEX_NOTICE);
    }
    if has_capture(shared, &format!("{stem}.glo")) {
        notices.push(GLOSSARIES_NOTICE);
    }
    notices
}

// ---------------------------------------------------------------- 超时证据（D3②）

/// 超时诊断的证据文本（阶段① `timeout_evidence_text` 的口径，按内嵌形态重写）。
///
/// 证据两路：① 捕获表里的 `<stem>.log`（有内容就有用，内嵌形态日志是**边写边在内存里**的，
/// 不像子进程形态要等收尾落盘）；② 引擎状态环尾部（流式输出的最后一段，含 `Note`/`Error`）。
/// 再用 `observed_pages`（本趟已上报的最大页）兜住"状态环的尾部窗口没记到 `[N]`"的情况 ——
/// 既然引擎确实排到过这一页，就把它补成一行 `[N]` 交回同一套解析（不新造判据）。
///
/// **不得**据此猜测因果：`diagnose_timeout` 只按"证据里有没有致命错误 / 有没有页输出"分档。
fn timeout_evidence_text(log: &str, status_tail: &str, observed_pages: Option<u32>) -> String {
    let tail = denoise_output_tail(status_tail);
    let mut text = if log.trim().is_empty() {
        tail
    } else if tail.is_empty() {
        log.to_owned()
    } else {
        let mut text = String::with_capacity(log.len() + tail.len() + 1);
        text.push_str(log);
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&tail);
        text
    };
    if let Some(observed) = observed_pages {
        let in_text = pages_typeset(&text);
        if in_text.map_or(true, |p| observed > p) {
            text.push_str(&format!("[{observed}]\n"));
        }
    }
    text
}

/// 输出尾部的轻量去噪：去 ANSI 序列与 `\r`（Tectonic 的进度行会用回车原地刷新），丢掉纯空白行。
/// **不动** `[N]` 页标记与错误文本 —— 它们正是证据本身。
fn denoise_output_tail(output: &str) -> String {
    let mut out = String::with_capacity(output.len());
    for line in output.lines() {
        let cleaned = strip_ansi(line).replace('\r', "");
        let trimmed = cleaned.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out
}

/// 去掉 CSI/OSC 转义序列（`ESC [ … m` 之类）。只处理尾部证据用的最小集合（阶段①同一实现）。
fn strip_ansi(line: &str) -> String {
    if !line.contains('\u{1b}') {
        return line.to_owned();
    }
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            // CSI：ESC [ … 终止于 0x40..=0x7e
            Some('[') => {
                chars.next();
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            // OSC：ESC ] … 终止于 BEL 或 ESC \
            Some(']') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// 超时错误条目（D3②）：**证据化诊断**，文案点明"内嵌形态无子树可杀、已置协作式取消"。
///
/// 证据：`.log` + 状态环尾部 + 已上报页（见 [`timeout_evidence_text`]）→ `core::log_parser::diagnose_timeout`
/// 给"慢 vs 卡住 vs 早就出错"的人话原因与一键提高超时的建议。`settled` = 取消后引擎是否已在
/// 下一个 I/O 回调处停下（决定文案用"已中止"还是"仍在收尾"——不虚报）。
fn timeout_entry(
    timeout: Duration,
    had_aux: bool,
    tex_files: Option<usize>,
    evidence_text: &str,
    settled: bool,
) -> ErrorEntry {
    let timeout_secs = u32::try_from(timeout.as_secs()).unwrap_or(u32::MAX);
    let pages = pages_typeset(evidence_text);
    let evidence = TimeoutEvidence { timeout_secs, first_build: !had_aux, tex_files };
    let diagnosis = diagnose_timeout(evidence_text, &evidence);

    let mut facts = vec![if had_aux { "增量编译".to_string() } else { "首次编译".to_string() }];
    if let Some(n) = tex_files {
        facts.push(format!("{n} 个 .tex 源文件"));
    }
    if let Some(p) = pages {
        facts.push(format!("已排版到第 {p} 页"));
    } else {
        facts.push("日志与输出均无页输出".to_string());
    }
    let stop = if settled {
        "引擎已在下一个 I/O 回调处中止"
    } else {
        "引擎仍在收尾（下一个 I/O 回调处中止；中止延迟上界 = 一次输出写/输入打开回调的间隔）"
    };
    ErrorEntry {
        message: format!(
            "内嵌引擎编译超过 {timeout_secs}s 上限，已请求中止（引擎随应用进程运行、无法强制结束）：\
             {stop}。证据（引擎输出）：{}",
            facts.join("，")
        ),
        file: None,
        line: None,
        kind: ErrorKind::Timeout,
        diagnosis: Some(diagnosis),
    }
}

// ---------------------------------------------------------------- 源码版模板提示（D7）

/// 源码版模板提示（阶段① `enrich_source_release_hints` 的语义，搬进本 crate 的失败路径）。
///
/// 编译报"缺 `.cls`"时，看看项目根里是不是只有 `.ins`/`.dtx`（源码版模板的发布形态）。
/// 判据是**看得见的事实**：根目录有哪些文件；"该编哪个文件"由 core 的
/// [`latteset_core::log_parser::source_release_hint`] 决定（同名优先、单一候选次之、拿不准就不给）。
fn enrich_with_candidates(mut errors: Vec<ErrorEntry>, candidates: &[String]) -> Vec<ErrorEntry> {
    for entry in &mut errors {
        let Some(diag) = entry.diagnosis.as_mut() else { continue };
        if diag.kind != DiagnosisKind::MissingClass {
            continue;
        }
        let Some(missing) = diag.missing_file.clone() else { continue };
        if let Some(hint) = source_release_hint(&missing, candidates) {
            diag.hint = Some(hint);
        }
    }
    errors
}

/// 见 [`enrich_with_candidates`]：只有确实出现"缺 `.cls`"时才读项目根（其余失败路径零成本）。
fn enrich_source_release_hints(errors: Vec<ErrorEntry>, project_root: &Path) -> Vec<ErrorEntry> {
    let needs_lookup = errors.iter().any(|e| {
        e.diagnosis.as_ref().is_some_and(|d| {
            d.kind == DiagnosisKind::MissingClass && d.missing_file.is_some()
        })
    });
    if !needs_lookup {
        return errors;
    }
    let candidates: Vec<String> = match std::fs::read_dir(project_root) {
        Ok(entries) => entries
            .flatten()
            .filter(|e| !e.file_type().map(|t| t.is_dir()).unwrap_or(true))
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .collect(),
        Err(e) => {
            debug!(root = %project_root.display(), "读项目根失败，跳过源码版模板识别：{e}");
            return errors;
        }
    };
    enrich_with_candidates(errors, &candidates)
}


// ---------------------------------------------------------------- bib 跳过（方案 §6.4 ④）

/// BibTeX 实际会读的命令。`.aux` 的其余内容（`\@writefile`、`\setcounter`、`\bibcite`…）它一概不看，
/// 所以签名只覆盖这三条 —— 覆盖多了会把"目录页码变了"误判成"要重跑 bibtex"。
const BIB_COMMANDS: &[&str] = &["\\citation", "\\bibdata", "\\bibstyle"];

/// bib 趟的**跳过判据**缓存：`tmp/<stem>.bibsig`。
///
/// 判据口径对齐 latexmk：`.bbl` 的输入是 ① 各 `.aux` 里的 BibTeX 命令（含 `\@input` 闭包里的
/// `\citation`）② 每个 `\bibdata` 指向的 `.bib` 的当前状态。两者都与产出 `.bbl` 那次一致 ⇒
/// 重跑 bibtex 只会得到同一份 `.bbl`，而它还会**强制多跑一趟排版**（上游 `RerunReason::Bibtex`）。
fn bib_signature_path(tmp_dir: &Path, stem: &str) -> PathBuf {
    tmp_dir.join(format!("{stem}.bibsig"))
}

/// 一次 bib 输入的状态。`commands` 覆盖 ①，`databases` 覆盖 ②。
#[derive(Debug, Clone, PartialEq, Eq)]
struct BibSignature {
    /// `\citation` / `\bibdata` / `\bibstyle` 三条命令的顺序化摘要（含 `\@input` 闭包）。
    commands: u64,
    /// `(相对路径, mtime_ns, size)`，按路径排序 ⇒ 顺序无关。
    databases: Vec<(String, u128, u64)>,
}

impl BibSignature {
    /// 一行一条，制表符分隔。`.bib` 名取**前 n-2 段**，所以名字里含制表符也不会错位。
    fn format(&self) -> String {
        let mut s = format!("v1\ncommands\t{:016x}\n", self.commands);
        for (path, mtime_ns, size) in &self.databases {
            s.push_str(&format!("db\t{path}\t{mtime_ns}\t{size}\n"));
        }
        s
    }

    /// 解析失败一律当"没有缓存"（⇒ 照旧跑 bibtex），不静默跳过。
    fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != "v1" {
            return None;
        }
        let mut commands = None;
        let mut databases = Vec::new();
        for line in lines {
            let f: Vec<&str> = line.split('\t').collect();
            match f.first() {
                Some(&"commands") if f.len() == 2 => {
                    commands = u64::from_str_radix(f[1], 16).ok();
                }
                Some(&"db") if f.len() >= 4 => {
                    let n = f.len();
                    databases.push((
                        f[1..n - 2].join("\t"),
                        f[n - 2].parse().ok()?,
                        f[n - 1].parse().ok()?,
                    ));
                }
                _ => return None,
            }
        }
        Some(Self { commands: commands?, databases })
    }
}

/// 收集一个 `.aux` **及其 `\@input` 闭包**里 BibTeX 会读的命令（按 BibTeX 的读取顺序）。
///
/// 为什么必须跟闭包：`\bibdata`/`\bibstyle` 在主 aux，而 `\citation` 在**章 aux**（`\include` 分章时
/// 每条 `\cite` 落在自己那章）。只看主 aux 会漏掉"新增一条引用"这件事 —— 那样 `.bbl` 少一条条目，
/// 而排版趟不会报错（只报 `Citation undefined` 警告），属于**静默错误输出**。
fn collect_bib_commands(
    shared: &SharedCapture,
    aux: &str,
    out: &mut Vec<String>,
    seen: &mut std::collections::BTreeSet<String>,
) {
    if !seen.insert(aux.to_owned()) {
        return; // `\@input` 成环时防死循环
    }
    let Some(bytes) = capture_bytes(shared, aux) else {
        return;
    };
    let text = String::from_utf8_lossy(&bytes);
    for line in text.lines() {
        let t = line.trim_end();
        if BIB_COMMANDS.iter().any(|c| t.starts_with(c)) {
            out.push(t.to_owned());
        } else if let Some(rest) = t.trim_start().strip_prefix("\\@input{") {
            if let Some(target) = rest.strip_suffix('}') {
                collect_bib_commands(shared, target, out, seen);
            }
        }
    }
}

/// 算出 bib 输入的当前签名；**任何一处判定不出来就返回 `None`**（⇒ 不跳过，宁可多跑一趟）。
///
/// 判不出来的情形都归到这里：`.bib` 不在项目磁盘上（由 bundle 提供、或路径带 kpathsea 展开）、
/// 或者路径指向的是目录。
fn bib_signature(
    shared: &SharedCapture,
    auxes: &[String],
    project_root: &Path,
) -> Option<BibSignature> {
    use std::hash::{Hash, Hasher};
    let mut commands = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for aux in auxes {
        collect_bib_commands(shared, aux, &mut commands, &mut seen);
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    commands.hash(&mut h);

    // `\bibdata{a,b}` → `a.bib`、`b.bib`，相对项目根解析（BibTeX 也从那里读）。
    let mut databases = Vec::new();
    for line in commands.iter().filter(|l| l.starts_with("\\bibdata")) {
        let braced = line.split_once('{')?.1.strip_suffix('}')?;
        for name in braced.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let rel = if name.to_ascii_lowercase().ends_with(".bib") {
                name.to_owned()
            } else {
                format!("{name}.bib")
            };
            let md = std::fs::metadata(project_root.join(&rel)).ok()?;
            if !md.is_file() {
                return None;
            }
            let mtime_ns = md
                .modified()
                .ok()?
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()?
                .as_nanos();
            databases.push((rel, mtime_ns, md.len()));
        }
    }
    databases.sort();
    databases.dedup();
    Some(BibSignature { commands: h.finish(), databases })
}

/// 跳过判定（纯函数）：三件事同时成立才跳 —— 缓存存在、签名一致、**每个** aux 的 `.bbl` 都在。
fn bib_pass_skippable(
    fresh: Option<&BibSignature>,
    cached: Option<&BibSignature>,
    all_bbls_present: bool,
) -> bool {
    all_bbls_present && fresh.is_some() && fresh == cached
}

/// 跑一趟 BibTeX，并把它自己的日志（`.blg`）作为**证据**：硬失败时附上尾部。
///
/// 口径（上游 `bibtex_pass_for_one_aux_file`）：`Spotless` 无事、`Warnings` 提示一行、
/// `Errors` **只警告不失败**（缺条目这类很常见，TeX 仍能出产物）；只有引擎硬错才失败。
fn run_bibtex_pass(
    launcher: &mut CoreBridgeLauncher<'_>,
    aux: &str,
    stem: &str,
    shared: &SharedCapture,
) -> Result<(), String> {
    let blg_name = format!("{stem}.blg");
    let mut engine = BibtexEngine::default();
    match engine.process(launcher, aux) {
        Ok(BibtexOutcome::Spotless) => {
            debug!(aux = %aux, "bib 趟：无告警");
            Ok(())
        }
        Ok(BibtexOutcome::Warnings) => {
            warn!(aux = %aux, blg = %capture_text(shared, &blg_name).trim(), "bib 趟有告警（继续）");
            Ok(())
        }
        Ok(BibtexOutcome::Errors) => {
            // 上游同口径：忽略并继续 —— 但把 `.blg` 摊开，不静默。
            warn!(
                aux = %aux,
                blg = %capture_text(shared, &blg_name).trim(),
                "bib 趟报 Errors（按上游口径忽略并继续；引用可能显示为未解析）"
            );
            Ok(())
        }
        Err(e) => {
            let blg = capture_text(shared, &blg_name);
            let tail: String = blg.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
            Err(format!(
                "参考文献处理失败（BibTeX 处理 {aux}）：{e:#}{}",
                if tail.trim().is_empty() { String::new() } else { format!("；.blg 尾部：{tail}") }
            ))
        }
    }
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
    // 根文件**绝对路径**：㊷ 的同步数据补丁要把它写进主输入记录（子进程档记的也是它）。
    root_file: PathBuf,
    pdf_dst: PathBuf,
    requested_kind: CompileKind,
    progress: Arc<dyn CompileProgress>,
    log_ring: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<String>>>,
    shared: SharedCapture,
    cancel: Arc<AtomicBool>,
    // 已上报过的最大页数（D3③）：趟边界写页进度，超时路径据此补页标记证据。
    pages_seen: Arc<AtomicU32>,
    // T14 基线打点：主文件读耗时（异步侧 `compile()` 里已计时；prep_ms = 它 + 本函数里的 bundle 打开）。
    root_read_ns: u128,
) -> CompileOutcome {
    // T18：后台 format 预热可能正在生成同一份缓存；先等它收尾，避免"预热 + 首编"各跑一趟 initex。
    wait_for_prewarm();
    let xdv_name = format!("{stem}.xdv");
    let pdf_name = format!("{stem}.pdf");
    let log_name = format!("{stem}.log");
    // 失败路径的源码版模板提示要读项目根；`project_root` 紧接着会被 `TectonicIo` 拿走。
    let project_root_for_hints = project_root.clone();
    // 强度分档（㉘）：Quick = 单趟直调引擎（引用/目录落后一趟）；Full = 跑到中间产物稳定。
    let wants_full = matches!(requested_kind, CompileKind::Full);

    // ---- 流式出图（roadmap ㉞）：编译期把"已完成页"变成部分 PDF，写 `tmp/<stem>.preview.pdf` ----
    // 上下文必须**在这里**组装：紧接着 `project_root`/`tmp_dir`/`cache_dir`/`cancel` 就被
    // `TectonicIo` 与 format 缓存路径拿走了。闸门（"还会再跑一趟" + 第 1 趟够长 + 页数有涨 +
    // 总数 ≤8）在 `preview` 模块里；这里只负责**在趟边界**调 `paint()`。
    // `LATTESET_PREVIEW_STREAM=0` 可关（A/B 复核用：对比开/关的编译墙钟）。
    let mut preview_painter = if crate::preview::stream_enabled() {
        Some(crate::preview::PartialPainter::new(crate::preview::PreviewContext {
            bundle_source: bundle_source.clone(),
            only_cached,
            cache_dir: cache_dir.clone(),
            project_root: project_root.clone(),
            tmp_dir: tmp_dir.clone(),
            stem: stem.clone(),
            xdv_name: xdv_name.clone(),
            shared: shared.clone(),
            progress: progress.clone(),
            cancel: cancel.clone(),
        }))
    } else {
        None
    };

    // T14 基线打点：prep_ms = bundle 打开（detect_bundle + digest）+ 主文件读（异步侧已计时）。
    // 只用于测量归因，不参与任何判定（语义不变）。
    let prep_started = Instant::now();

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
    // T14 基线打点：prep 到此结束（bundle 打开 + digest + 主文件读）。
    let prep_ms = (root_read_ns + prep_started.elapsed().as_nanos()) as f64 / 1e6;
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
    // 页哈希缓存落点（A 的判据）：**必须先算** —— `tmp_dir` 紧接着就被 `TectonicIo` 拿走了。
    // 缓存文件名带引擎段（`ENGINE_ID`，已知债 #25）；口径定义在 core，与子进程档共用。
    let pages_cache = latteset_core::xdv::pages_cache_path(&tmp_dir, &stem);
    // PDF 产物身份记录（D-L4b：Full 也复用时的"磁盘 PDF 未被手改"证据）：同样必须在 `tmp_dir`
    // 被 `TectonicIo` 拿走之前算好。
    let pdf_id_cache = pdf_identity_cache_path(&tmp_dir, &stem);
    // bib 判据缓存落点与 `.bib` 的解析根（同上：`project_root`/`tmp_dir` 紧接着被 `TectonicIo` 拿走）。
    let bib_sig = bib_signature_path(&tmp_dir, &stem);
    let bib_root = project_root.clone();
    // ㊷ 同步数据补丁的落点（同上：`tmp_dir` 紧接着被 `TectonicIo` 拿走）：见 `crate::synctex`。
    let mirror_dir = tmp_dir.clone();
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
    let mut status = ProgressStatus::new(progress.clone(), log_ring.clone());
    // 分阶段计时（P2 的复核证据：库形态的钱花在哪一趟）。命中 format 缓存时 format 趟为 0。
    let t_start = Instant::now();
    let mut phase_ms = (0u128, 0u128, 0u128);
    // 是否真的收敛（决定 `Success.kind`）：闭包内赋值，收尾处读。
    let mut converged = false;
    let mut pass_count = 0usize;
    // 页哈希与 A 的判定（闭包内赋值，收尾处用）：见 ⑥ 段。
    let mut page_hashes: Vec<u64> = Vec::new();
    let mut reuse_pdf = false;
    // T14 基线打点：编译中预览（流式出图）累计耗时与次数（只测量，不影响闸门）。
    let mut preview_ns: u128 = 0;
    let mut preview_paints: u32 = 0;
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
                    .map_err(|e| {
                        format!(
                            "首次编译初始化失败（生成排版格式缓存）：{e:#}。请重试；\
                             若持续失败，检查「设置 → 引擎」的缓存目录是否可写"
                        )
                    })?;
                // 上游 `driver.rs:1807-1819`：`Ok(Errors)` 也算失败 —— 先前忽略 `TexOutcome`
                // 会把"initex 没产出 format"当成功，接着排版趟必然报一堆 Undefined control sequence。
                if matches!(outcome, TexOutcome::Errors) {
                    let tail = log_tail(&log_ring);
                    return Err(format!(
                        "首次编译初始化失败：生成排版格式缓存时 LaTeX 报错{tail}。\
                         多为宏包集缺文件：请按错误列表处理，或在「设置 → 引擎 → 宏包集来源」\
                         切换到「在线完整宏包集」后重试"
                    ));
                }
                // 排版趟要换回用户文档作为主输入。
                format_pass.store(false, Ordering::Relaxed);
                // 收 dump 并落产品缓存（上游 `driver.rs:1825-1838`：遍历内存层里所有 `*.fmt`）。
                let dumped = take_dumped_format(&shared);
                let Some((dump_name, bytes)) = dumped else {
                    return Err(
                        "排版格式缓存没有产出（引擎未生成 .fmt）⇒ 已停止编译：继续只会报一串 \
                         Undefined control sequence"
                            .to_owned(),
                    );
                };
                let target = format_target.as_ref().ok_or_else(|| {
                    "需要生成排版格式缓存，但缓存目录不可用（未注入缓存目录或读取宏包集摘要失败）\
                     ⇒ 不会把格式文件写进项目目录——请检查「设置 → 引擎」的缓存目录"
                        .to_owned()
                })?;
                if let Some(parent) = target.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return Err(format!("创建排版格式缓存目录失败（{}）：{e}", parent.display()));
                    }
                }
                if let Err(e) = std::fs::write(target, &bytes) {
                    return Err(format!("写排版格式缓存失败（{}）：{e}", target.display()));
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
            // ⑤ 排版趟 + **重跑循环**（上游 `default_pass` 的主循环；上限 `MAX_TEX_PASSES`）。
            //
            // 为什么必须有这个循环：`latex → bibtex → latex` **还不等于**引用解析好了 ——
            // LaTeX 在 `\begin{document}` 读 `.aux`，而 `\bibcite`（引用编号）是**上一趟**才写进去的，
            // 所以 `\cite` 要第三趟才解析出来（实测：第二趟读到 `.bbl` 后仍报
            // `Citation 'knuth1984' undefined` + `Label(s) may have changed. Rerun`）。
            // 也就是说**只有 bib 趟而没有重跑循环，等于白跑**。
            //
            // 趟数按请求的强度分档（与 ㉘ 的产品语义对齐）：
            // - `Full`（首编 / 手动编译 / 空闲收敛）：跑到 rerun 相关中间产物**稳定**为止。
            // - `Quick`（编辑触发）：**在预算内**也跑到稳定（见 `quick_pass_allowed`）；收敛就按
            //   实际结果报 `Full` ⇒ 前端不亮「引用待更新」、也不再排一次空闲收敛。任一道闸门不过
            //   （趟数 / 第 1 趟太慢 / 累计超预算）则停在草稿态，行为与 ㉘ 今天完全一致。
            let aux_name = format!("{stem}.aux");
            let mut passes = 0usize;
            let mut bib_done = false;
            let mut stable = false;
            let mut unsupported_tool: Option<&'static str> = None;
            // D3③：已上报过的页数（只增不减；超时路径也用它兜页标记证据）。
            let mut reported_pages = 0u32;
            // 预算只算排版循环本身：format 趟（冷缓存要生成 `.fmt`）不占编辑触发档的额度。
            let loop_started = Instant::now();
            // 第 1 趟的墙钟（`quick_pass_allowed` 的第二个条件：一趟就慢的文档不再多跑）。
            let mut first_pass = Duration::ZERO;
            loop {
                let before = rerun_snapshot(&shared);
                let what = if passes == 0 { "排版趟" } else { "排版趟（重跑）" };
                run_tex_pass(&mut launcher, &name, what)?;
                passes += 1;
                // ⚠ **每趟都要更新**（原先只在 `passes == 1` 里赋值 ⇒ 第 2 趟及之后的时间被算进
                //    `convert_ms`，2 趟时那个字段虚高约一倍，让人误判"转换是大头"；2026-09-17 实测
                //    M0 夹具：2 趟报 convert=148–151、1 趟报 78–81，而两者的 XDV 完全同尺寸同内容）。
                //    口径：`typeset_ms` = **全部**排版趟；`convert_ms` = 只有 XDV→PDF 那一次。
                phase_ms.1 = t_start.elapsed().as_millis();
                if passes == 1 {
                    first_pass = loop_started.elapsed();
                    // 上游 `driver.rs:1900-1904`：TeX 没产出预期输出文件时要**明说**（多因文档为空），
                    // 否则错误会以"XDV→PDF 失败"的形式出现、指向错误的方向。
                    if capture_len(&shared, &xdv_name) == 0 {
                        return Err(format!(
                            "排版没有产出中间文件 {xdv_name}（空文档或引擎提前停下）；引擎请求的输入：{}",
                            summarize(&requests.lock().unwrap_or_else(|e| e.into_inner()).clone())
                        ));
                    }
                }

                // 编辑触发档的收敛预算：每趟算一次，bib 闸门与"还能不能再跑一趟"共用同一个判断
                // （Full 不设这道闸门，它的上限就是下面的 `MAX_TEX_PASSES`）。
                let quick_may_continue = quick_pass_allowed(passes, first_pass, loop_started.elapsed());

                // D3③ 页进度：内嵌形态没有子进程 stdout 的 `[N]` 可扫，但捕获表里的 XDV 就是
                // 权威页表 ⇒ 每趟收尾数一次完整页，**数值变大才发**（CompileProgress::pages 契约）。
                // 成本 = O(XDV 字节)，与一次页哈希同量级；这里只数页、不算哈希（哈希留给收尾的 A 闸门）。
                let pages_now = captured_pages(&shared, &xdv_name);
                if pages_now > reported_pages {
                    reported_pages = pages_now;
                    progress.pages(pages_now);
                    pages_seen.store(pages_now, Ordering::Relaxed);
                }

                // 外部工具类（biber / makeindex / glossaries）：内嵌形态不跑外部工具 ⇒ 登记后
                // **不得声称已收敛**（下面 kind 会退回 Quick）。检出信号取上游口径：
                // biber = `.run.xml` 里真的写 `<binary>biber</binary>`（`check_biber_requirement`），
                // 不是只看文件存在（backend=bibtex 也写 `.run.xml`）。
                if run_xml_requests_biber(&shared, &stem) {
                    unsupported_tool = Some("biber（biblatex）");
                }
                if has_capture(&shared, &format!("{stem}.idx")) {
                    unsupported_tool = Some("makeindex（`.idx`）");
                }
                if has_capture(&shared, &format!("{stem}.glo")) {
                    unsupported_tool = Some("glossaries（`.glo`）");
                }

                let mut force_rerun = false;
                if (wants_full || quick_may_continue) && !bib_done {
                    let aux_needing_bib = aux_files_requesting_bibtex(&shared, &aux_name);
                    if !aux_needing_bib.is_empty() {
                        let fresh = bib_signature(&shared, &aux_needing_bib, &bib_root);
                        let cached = std::fs::read_to_string(&bib_sig)
                            .ok()
                            .and_then(|t| BibSignature::parse(&t));
                        let all_bbls_present = aux_needing_bib.iter().all(|a| {
                            capture_len(&shared, &a.replace(".aux", ".bbl")) > 0
                        });
                        if bib_pass_skippable(fresh.as_ref(), cached.as_ref(), all_bbls_present) {
                            // 跳过的**不只是** bibtex：上游在 bib 之后无条件重跑一趟排版
                            // （`RerunReason::Bibtex`），没有新 `.bbl` 要读就不该多跑那一趟。
                            info!(
                                aux = ?aux_needing_bib,
                                "bib 趟跳过：引用集合 / `.bib` 状态与产出 `.bbl` 那次一致"
                            );
                            bib_done = true;
                        } else {
                            for aux in &aux_needing_bib {
                                run_bibtex_pass(&mut launcher, aux, &stem, &shared)?;
                            }
                            // 签名记的是**产出这份 `.bbl` 的输入**；`.bib` 在编译期间不会变，
                            // 所以用跑之前算的那份。判不出签名（`None`）时不写 ⇒ 下次照旧重跑。
                            if let Some(sig) = &fresh {
                                if let Err(e) = std::fs::write(&bib_sig, sig.format()) {
                                    debug!(path = %bib_sig.display(), "写 bib 判据缓存失败（下次会多跑一次 bibtex）：{e}");
                                }
                            }
                            debug!(aux = ?aux_needing_bib, passes, "bib 趟已完成");
                            bib_done = true;
                            // 上游 `Some(RerunReason::Bibtex)`：bib 之后**无条件**再跑一趟。
                            force_rerun = true;
                        }
                    }
                }

                if !force_rerun {
                    let now = rerun_snapshot(&shared);
                    if now == before {
                        stable = true;
                        break;
                    }
                    // 决策日志（roadmap ㉛）：不点名是哪个中间产物在变，"永远 2 趟"就无从查起。
                    let changed: Vec<&str> = before
                        .keys()
                        .chain(now.keys())
                        .filter(|k| before.get(*k) != now.get(*k))
                        .map(String::as_str)
                        .collect();
                    debug!(passes, changed = ?changed, force = force_rerun, "重跑：中间产物仍在变");
                }
                if passes >= MAX_TEX_PASSES {
                    warn!(
                        passes,
                        "重跑到上限仍未收敛：停在 {MAX_TEX_PASSES} 趟（引用/目录可能仍落后）"
                    );
                    break;
                }
                if !wants_full && !quick_may_continue {
                    // ㉛ 决策日志：为什么停在草稿态（否则"编辑触发档为什么还落后一趟"无从查起）。
                    debug!(
                        passes,
                        first_pass_ms = first_pass.as_millis() as u64,
                        elapsed_ms = loop_started.elapsed().as_millis() as u64,
                        first_pass_max_ms = QUICK_FIRST_PASS_MAX.as_millis() as u64,
                        max_passes = QUICK_MAX_PASSES,
                        budget_ms = QUICK_CONVERGENCE_BUDGET.as_millis() as u64,
                        "编辑触发档不再追收敛：停在草稿态（交给 ㉘ 的空闲收敛）"
                    );
                    break;
                }

                // ㉞ 流式出图：走到这里 = **确定还会再跑一趟**（两个 `break` 都过了），
                // 而这一趟刚收尾 ⇒ 引擎全局锁是空的。此刻出图既新鲜（前缀就是刚排完的那份）
                // 又不排队、不拖慢下一趟。放在循环末尾而不是开头，正是为了这个"确定"。
                if let Some(painter) = preview_painter.as_mut() {
                    let paint_started = Instant::now();
                    painter.paint(first_pass);
                    preview_ns += paint_started.elapsed().as_nanos();
                    preview_paints += 1;
                }
            }
            if let Some(tool) = unsupported_tool {
                warn!(stem = %stem, tool, "{}", crate::BIBER_PASS_MISSING_NOTE);
            }
            // 收敛支持位与 kind 的取值依据（不得虚报：有外部工具没跑、或跑满上限没稳定，
            // 都不能说"已收敛"）。**不再要求请求是 `Full`** —— 编辑触发档在预算内跑到稳定同样算
            // 收敛，这正是"把草稿从固定档变成判定结果"的落点。
            converged = unsupported_tool.is_none() && stable;
            pass_count = passes;
            info!(
                passes,
                stable,
                converged,
                requested = ?requested_kind,
                "库形态排版趟数"
            );

            // ⑥ 页哈希（A/B/C 三个功能点的共用判据）→ 转换趟（§3.4 第 7 步）。
            //
            // **必须与子进程档用同一个函数**（`latteset_core::xdv::page_hashes`）：若两套口径不同，
            // 换形态时前端会把整篇判成"变了"（全量重绘）。所以我们**不另写解析器**，直接把捕获表里
            // 的 XDV 字节喂给同一个函数 —— 与子进程档读 `tmp/<stem>.xdv` 拿到的是同一份数据。
            // 这也解释了为什么这里没用 `XdvParser`：流式解析是「编译中页事件」（P5）的入口，
            // 而 A/B/C 只要最终页表，用同一函数才能保证与子进程档**逐页可比**。
            page_hashes = {
                let bytes = capture_bytes(&shared, &xdv_name).unwrap_or_default();
                latteset_core::xdv::page_hashes(&bytes)
            };
            let prev_pages = std::fs::read_to_string(&pages_cache)
                .ok()
                .and_then(|t| latteset_core::xdv::parse_pages_cache(&t));
            // A（D-L4b）：页逐字节未变 **且** 磁盘 PDF 仍是上次产出的那一份 ⇒ 跳过转换与拷贝。
            // - Quick（编辑触发）：一直允许（"内容没变的编辑"的快速路径）；
            // - Full：产品已批准（2026-11-20）在同样条件下也跳过 —— 页哈希证明排版结果等价，
            //   磁盘身份证明 PDF 未被用户手改/替换/删除，两者同时成立才复用；
            // - `LATTESET_A_FULL=0` 可关（A/B 复核）。
            let allow_full_reuse = !wants_full || full_reuse_enabled();
            reuse_pdf = allow_full_reuse
                && should_reuse_pdf(
                    &page_hashes,
                    prev_pages.as_deref(),
                    pdf_unchanged_since_we_wrote_it(&pdf_dst, &pdf_id_cache),
                );
            if reuse_pdf {
                info!(
                    pages = page_hashes.len(),
                    requested_full = wants_full,
                    "页哈希与上次逐页相同，且磁盘 PDF 未被改动：跳过 XDV→PDF 转换与拷贝（复用现有产物）"
                );
            } else {
                let mut pdf_engine = XdvipdfmxEngine::default();
                pdf_engine
                    .process(&mut launcher, &xdv_name, &pdf_name)
                    .map_err(|e| format!("XDV→PDF 失败：{e:#}"))?;
            }
            phase_ms.2 = t_start.elapsed().as_millis();
            Ok(())
        })()
    };
    drop(driver);

    // ㊷ **同步数据的主输入记录修补**：引擎在库形态下把主文件记成 `texput`（根因在 C 侧，
    //    见 `crate::synctex` 模块文档），补成项目根下的真实绝对路径 —— 与子进程档同值。
    //    放这里 = 所有出口（含下面的拷贝失败 / IoError 早退）之前，且捕获表与 `tmp/` 镜像
    //    拿到同一份字节（自解析定位读的就是磁盘那份）。补丁是**纯数据**，不动排版与 `.log`。
    crate::synctex::patch_outputs(&shared, &stem, &mirror_dir, &root_file.to_string_lossy());

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
            // T14 基线打点：post_ms = PDF 原子替换 + 页哈希缓存写（成功路径收尾，不含 synctex 修补）。
            let post_started = Instant::now();
            // A（复用现有 PDF）时跳过拷贝：产物是上一次那一个，页哈希已证明它与本次排版逐页等价。
            if !reuse_pdf {
                let pdf = files.get(&pdf_name).map(Vec::as_slice).unwrap_or(&[]);
                if pdf.is_empty() {
                    return CompileOutcome::IoError {
                        message: format!(
                            "编译成功但引擎没有产出 {pdf_name}（引擎请求的输入：{}）",
                            summarize(&requests)
                        ),
                    };
                }
                // V-05：**原子替换**（写 `{stem}.pdf.tmp` → `rename` 覆盖）——与子进程档
                // `crates/latteset-infra/src/runner.rs:615-630` 同口径，失败时旧 PDF 保持完整。
                let pdf_tmp = pdf_dst.with_extension("pdf.tmp");
                if let Err(e) =
                    std::fs::write(&pdf_tmp, pdf).and_then(|()| std::fs::rename(&pdf_tmp, &pdf_dst))
                {
                    // V-05：失败时清理临时文件，**旧 PDF 不被截断**（与子进程档同口径）。
                    let _ = std::fs::remove_file(&pdf_tmp);
                    return CompileOutcome::IoError {
                        message: format!(
                            "PDF 无法写入项目根（{}）：{e}（旧 PDF 未被破坏）",
                            pdf_dst.display()
                        ),
                    };
                }
            }
            // 页哈希缓存（A 的判据，下一轮用）：失败只记 debug —— 它只是优化判据，
            // 不该让编译失败（最坏结果是下次多转换一次）。
            if let Some(body) = latteset_core::xdv::format_pages_cache(&page_hashes) {
                if let Err(e) = std::fs::write(&pages_cache, body) {
                    debug!(path = %pages_cache.display(), "写页哈希缓存失败（下次会多转换一次）：{e}");
                }
            }
            // D-L4b：记下**本次写出的** PDF 身份（复用路径不写：磁盘 PDF 没变，记录继续有效）。
            // 失败只记 debug：最坏结果是下一轮保守转换一次，不该让编译失败。
            if !reuse_pdf {
                match pdf_identity(&pdf_dst) {
                    Some(id) => {
                        if let Err(e) = std::fs::write(&pdf_id_cache, format_pdf_identity(&id)) {
                            debug!(path = %pdf_id_cache.display(), "写 PDF 身份记录失败（下次会多转换一次）：{e}");
                        }
                    }
                    None => debug!(path = %pdf_dst.display(), "取 PDF 身份失败（下次会多转换一次）"),
                }
            }
            let post_ms = post_started.elapsed().as_nanos() as f64 / 1e6;
            debug!(
                pages = page_hashes.len(),
                reused_pdf = reuse_pdf,
                log_bytes = log_text.len(),
                input_requests = requests.len(),
                "库形态编译完成（页哈希 = 与子进程档同一函数产出）"
            );
            // 分阶段耗时（命中缓存时 format_ms = 0）：P2/P4 复核用的一行证据。
            let (n_ram, n_mem, n_disk, n_bundle, n_miss) = request_counts(&requests);
            // LIB-2 的两段（我们 I/O 层能看到的）：bundle 读 + format 文件读。
            let (bundle_ns, bundle_n, format_ns) = {
                let c = shared.lock().unwrap_or_else(|e| e.into_inner());
                (c.bundle_read_ns, c.bundle_read_n, c.format_read_ns)
            };
            info!(
                format_ms = phase_ms.0,
                typeset_ms = phase_ms.1.saturating_sub(phase_ms.0),
                convert_ms = phase_ms.2.saturating_sub(phase_ms.1),
                engine_total_ms = phase_ms.2,
                format_cached,
                passes = pass_count,
                pages = page_hashes.len(),
                reused_pdf = reuse_pdf,
                // T14 基线打点（只测量，不参与判定）：
                // prep_ms = bundle 打开 + 主文件读；post_ms = PDF 原子替换 + 页哈希缓存写；
                // preview_ms = 编译中流式出图累计耗时（计入 typeset 窗口内）。
                prep_ms = prep_ms,
                post_ms = post_ms,
                preview_ms = preview_ns as f64 / 1e6,
                preview_paints = preview_paints,
                req_ram = n_ram,
                req_mem = n_mem,
                req_disk = n_disk,
                req_bundle = n_bundle,
                req_miss = n_miss,
                bundle_read_ms = bundle_ns as f64 / 1e6,
                bundle_read_n = bundle_n,
                format_read_ms = format_ns as f64 / 1e6,
                "库形态分阶段耗时"
            );
            // D3① 外部工具类能力边界提示（ADR-0015 第 4 条：检出即如实说明、不静默降级）。
            // 契约：`CompileProgress::notice` **只在成功路径**发（失败路径走 `errors`）。
            for msg in external_tool_notices(&shared, &stem) {
                debug!("编译成功但需要用户注意：外部工具能力边界提示");
                progress.notice(msg);
            }
            CompileOutcome::Success {
                pdf_path: pdf_dst,
                // V-04：**按实际做到的事报**（先前固定报 `Full` 是错的 —— 会让 ㉘ 的
                // "引用待更新"/`draft` 语义失效）：
                // - 中间产物已稳定 ⇒ `Full`（**含编辑触发档在预算内收敛的情形**：那正是我们要的
                //   "编辑后直接就是正确结果"，前端据此不亮提示、也不排空闲收敛）；
                // - 否则（预算用尽、或文档要外部工具而我们跑不了）⇒ `Quick`，
                //   由 ㉘ 的「引用待更新」+ 空闲收敛兜底。
                kind: if converged { CompileKind::Full } else { CompileKind::Quick },
                // 页级复用 A/B/C 的输入：与子进程档**同一个函数**、同一份 XDV 数据。
                // 空表仍然只表示"无法判定"（XDV 缺失/损坏），前端按保守全量刷新。
                page_hashes,
            }
        }
        Err(msg) => {
            // 判据 4：失败不静默——优先给结构化错误列表（`.log` 是权威），否则 IoError + 输入请求证据。
            // D7：源码版模板提示（`.ins`/`.dtx`）在失败路径保留阶段①语义，不静默丢。
            let errors = enrich_source_release_hints(entries_from_log(&log_text, true), &project_root_for_hints);
            if !errors.is_empty() {
                debug!(count = errors.len(), "库形态失败：已从 I/O 层捕获的 .log 解析出错误条目");
                CompileOutcome::ContentError { errors }
            } else {
                let ring = log_ring.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let tail: Vec<String> = ring.iter().rev().take(5).cloned().collect();
                CompileOutcome::IoError {
                    message: format!(
                        "{msg}；引擎请求的输入：{}；引擎日志尾部：{}",
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
        // T14 基线打点：主文件读耗时（prep_ms 的一半；bundle 打开在阻塞段里另计）。
        let root_read_started = Instant::now();
        let root_bytes = match self.fs.read_bytes(&req.root_file).await {
            Ok(b) => b,
            Err(e) => {
                return CompileOutcome::IoError {
                    message: format!("读取根文件失败（{}）：{e}", req.root_file.display()),
                };
            }
        };
        let root_read_ns = root_read_started.elapsed().as_nanos();

        // bundle 的**打开**放到阻塞段里做（`Box<dyn Bundle>` 不是 `Send`，见 `run_engines` 头注）。
        let shared: SharedCapture = new_capture();
        // 预热上一趟的中间产物（`tmp/`）：让重跑判据有"上一趟"可比较，重复编译收敛的文档只需 1 趟。
        seed_previous_intermediates(&shared, &tmp_dir);
        let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log_ring: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        // D3③：已上报过的最大页（超时证据用它兜住"状态环尾部窗口没截到页标记"的情形）。
        let pages_seen = Arc::new(AtomicU32::new(0));
        // D3②：超时取证要用的别名 —— 原值紧接着会被 move 进阻塞段。
        let log_name = format!("{stem}.log");
        let shared_for_timeout = shared.clone();
        let ring_for_timeout = log_ring.clone();
        let pages_for_timeout = pages_seen.clone();
        // 超时诊断的"首次编译"判据：**开始时**有没有上一趟的 `.aux`（必须在本次排版前看）。
        let had_aux = self.fs.exists(&tmp_dir.join(format!("{stem}.aux"))).await.unwrap_or(false);
        // V-06：协作式取消标志。置位后 `TectonicIo` 的每个入口立刻报错 ⇒ 引擎在**下一个
        // I/O 回调**处中止，而不是跑完当前趟（这是 LIB-9「下个回调报错」口径的落地）。
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let cancel_for_task = cancel_flag.clone();
        let progress = self.progress.clone();
        let bundle_source = self.bundle.clone();
        let only_cached = self.only_cached;
        let cache_dir = self.cache_dir.clone();
        let requested_kind = req.kind;
        let project_root = req.project_root.clone();
        let root_file = req.root_file.clone();
        let mut blocking = Box::pin(tokio::task::spawn_blocking(move || {
            run_engines(
                bundle_source,
                only_cached,
                cache_dir,
                project_root,
                tmp_dir,
                root_bytes,
                stem,
                name,
                root_file,
                pdf_dst,
                requested_kind,
                progress,
                log_ring,
                requests,
                shared,
                cancel_for_task,
                pages_seen,
                root_read_ns,
            )
        }));

        // 超时/取消：内嵌形态**无法树杀**，但**不是**"只能放弃等待"——置协作式取消标志后，
        // `TectonicIo` 会在下一个 I/O 回调处返回错误，引擎随即中止该趟（V-06 / LIB-9）。
        // 上界 = 一次输出写 / 输入打开回调的间隔（实测数字见 docs/dev/tectonic-library.md）。
        // `ENGINE_LOCK` 仍保证进程内不并发两个引擎。
        tokio::select! {
            joined = &mut blocking => match joined {
                Ok(outcome) => outcome,
                Err(e) => CompileOutcome::IoError {
                    message: format!("编译线程异常终止：{e}。请重试；若再次出现请附带此信息反馈"),
                },
            },
            _ = tokio::time::sleep(req.timeout) => {
                warn!(timeout = req.timeout.as_secs(), "内嵌引擎编译超时：置协作式取消标志（引擎在下一个 I/O 回调处中止）");
                cancel_flag.store(true, Ordering::Relaxed);
                // 给协作式取消一小段收尾时间：引擎在**下一个 I/O 回调**处中止，`.log` 与状态环
                // 那时才是中止后的完整证据（这段等待计入超时返回值，上界 = TIMEOUT_CANCEL_GRACE）。
                let settled = tokio::time::timeout(TIMEOUT_CANCEL_GRACE, &mut blocking).await.is_ok();
                let log_text = capture_text(&shared_for_timeout, &log_name);
                let status_tail = ring_for_timeout
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .join("\n");
                let observed = pages_for_timeout.load(Ordering::Relaxed);
                let evidence = timeout_evidence_text(
                    &log_text,
                    &status_tail,
                    (observed > 0).then_some(observed),
                );
                // "项目里有多少 .tex"只为诊断用（首编规模判据），只在超时这条慢路径上扫。
                let tex_files = latteset_core::project::collect_tex_files(
                    self.fs.as_ref(),
                    &req.project_root,
                )
                .await
                .ok()
                .map(|v| v.len());
                let entry = timeout_entry(req.timeout, had_aux, tex_files, &evidence, settled);
                debug!(
                    timeout_secs = req.timeout.as_secs(),
                    first_build = !had_aux,
                    tex_files = ?tex_files,
                    pages = ?pages_typeset(&evidence),
                    settled,
                    log_bytes = log_text.len(),
                    "内嵌编译超时：已采集诊断证据（.log + 状态尾部 + 已上报页）"
                );
                CompileOutcome::Timeout { entry }
            }
            _ = cancel.cancelled() => {
                warn!("内嵌引擎编译收到手动终止：置协作式取消标志（引擎在下一个 I/O 回调处中止）");
                cancel_flag.store(true, Ordering::Relaxed);
                CompileOutcome::Aborted
            }
        }
    }
}

// ===================== 首启 format 预热（T18） =====================
//
// 产品决定（2026-11-20）：`.fmt` **不随包**，改为应用首启**后台预热**生成 —— 把 ~1s 的冷缓存首编
// 等待藏到用户打开项目/打字期间。`prewarm_format` 只跑 **initex + dump**（等价 runner 里的 format
// 分支），**不跑用户文档**；成功才把 `.fmt` 原子落进产品缓存；失败静默（不影响启动与后续首编）。

/// 首启预热的判定结果（GUI 只记日志/上报，**不**据此让启动失败）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrewarmOutcome {
    /// 本次实际生成并落盘。
    Generated,
    /// 已有缓存（无需再跑 initex）。
    AlreadyCached,
    /// 没有可用的**本地/已缓存** bundle（网络 bundle 冷缓存）⇒ 启动期跳过（不联网）。
    SkippedNoBundle,
    /// 失败（静默；下次首编会照常自行生成）。`msg` 只进日志，不弹给用户。
    Failed(String),
}

/// 预热互斥状态（进程级一次性）：`Running` 期间别的预热/编译要等它收尾，避免同一份 format
/// 被"后台预热"与"用户首编"各生成一次（引擎锁只串行、不会去重）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GatePhase {
    Idle,
    Running,
    Done,
    Failed,
}

struct PrewarmGate {
    phase: Mutex<GatePhase>,
    cv: Condvar,
}

static PREWARM_GATE: OnceLock<Arc<PrewarmGate>> = OnceLock::new();

fn prewarm_gate() -> &'static Arc<PrewarmGate> {
    PREWARM_GATE.get_or_init(|| Arc::new(PrewarmGate {
        phase: Mutex::new(GatePhase::Idle),
        cv: Condvar::new(),
    }))
}

/// 编译入口等预热收尾：预热正在生成同一份 format 时先等它结束，再判定缓存命中
/// （否则首编会把"预热正在生成"看成"没有缓存"，白跑一趟 initex）。
///
/// 没有预热发生过（`OnceLock` 未初始化）⇒ 立即返回，零开销。
fn wait_for_prewarm() {
    let Some(gate) = PREWARM_GATE.get() else { return };
    let mut phase = gate.phase.lock().unwrap_or_else(|e| e.into_inner());
    if *phase != GatePhase::Running {
        return;
    }
    let deadline = Instant::now() + PREWARM_WAIT_TIMEOUT;
    while *phase == GatePhase::Running {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            debug!("等首启 format 预热超时：继续按「无缓存」处理（首编会自行生成）");
            break;
        };
        let (guard, _) = gate.cv.wait_timeout(phase, remaining).unwrap_or_else(|e| e.into_inner());
        phase = guard;
    }
}

/// 编译在入口最多等预热多久（预热是一次 initex 趟，正常 ~1s；超时就不再等、按无缓存继续）。
const PREWARM_WAIT_TIMEOUT: Duration = Duration::from_secs(60);

/// 首启 format 预热：只跑 initex + dump，成功才把 `.fmt` **原子**落进 `<cache>/formats/`。
///
/// 预热计划见 [`crate::bundle::prewarm_plan`]：**在线源先做零网络判定**（本地缓存标记可用且未过期
/// 才进 `open_bundle`，否则直接 `SkippedNoBundle`）——上游 `detect_bundle` 对 http 源在缓存冷时
/// 会联网取 digest，启动预热不得走那条路。**失败静默**：返回 [`PrewarmOutcome::Failed`] 只供日志，
/// 不写半成品（半成品会被首编当成"缓存命中"）、不影响启动与后续首编。
pub fn prewarm_format(bundle: &BundleSource, cache_dir: &Path) -> PrewarmOutcome {
    let only_cached = match crate::bundle::prewarm_plan(bundle, cache_dir) {
        crate::bundle::PrewarmPlan::Skip => return PrewarmOutcome::SkippedNoBundle,
        crate::bundle::PrewarmPlan::Prewarm { only_cached } => only_cached,
    };
    let open = match open_bundle(bundle, only_cached, Some(cache_dir.to_path_buf())) {
        Ok(Some(b)) => b,
        Ok(None) => return PrewarmOutcome::SkippedNoBundle,
        Err(msg) => {
            // 在线 bundle 的"只读缓存未命中"是**正常跳过**（启动期不联网）；本地 bundle 失败才是真失败。
            return if only_cached { PrewarmOutcome::SkippedNoBundle } else { PrewarmOutcome::Failed(msg) };
        }
    };
    let mut open = open;
    let digest = match bundle_digest(&mut *open) {
        Ok(d) => d,
        Err(msg) => return PrewarmOutcome::Failed(msg),
    };
    let format_cache_dir = cache_dir.join("formats");
    let target = match crate::io::format_cache_path(
        Some(&format_cache_dir),
        Some(&digest),
        crate::FORMAT_NAME,
    ) {
        Ok(p) => p,
        Err(msg) => return PrewarmOutcome::Failed(msg),
    };
    if target.is_file() {
        return PrewarmOutcome::AlreadyCached;
    }

    let gate = prewarm_gate();
    {
        let mut phase = gate.phase.lock().unwrap_or_else(|e| e.into_inner());
        while *phase == GatePhase::Running {
            phase = gate.cv.wait(phase).unwrap_or_else(|e| e.into_inner());
        }
        // 等锁期间可能已有另一路生成（或首编自己跑完了）。
        if target.is_file() {
            return PrewarmOutcome::AlreadyCached;
        }
        *phase = GatePhase::Running;
    }

    let started = Instant::now();
    let result = generate_format_prewarm(
        Some(open),
        digest,
        format_cache_dir,
        cache_dir.to_path_buf(),
        target,
    );
    let mut phase = gate.phase.lock().unwrap_or_else(|e| e.into_inner());
    let outcome = match result {
        Ok(()) => {
            *phase = GatePhase::Done;
            info!(
                prewarm_ms = started.elapsed().as_millis() as u64,
                "首启 format 预热完成（.fmt 已落产品缓存）"
            );
            PrewarmOutcome::Generated
        }
        Err(msg) => {
            *phase = GatePhase::Failed;
            debug!(
                prewarm_ms = started.elapsed().as_millis() as u64,
                %msg,
                "首启 format 预热失败（静默：首编会自行生成）"
            );
            PrewarmOutcome::Failed(msg)
        }
    };
    gate.cv.notify_all();
    outcome
}

/// 真正跑 initex 趟并落盘（与 `run_engines` 的 format 分支同序；改动要两边同步）。
fn generate_format_prewarm(
    bundle: Option<Box<dyn Bundle>>,
    digest: String,
    format_cache_dir: PathBuf,
    project_root: PathBuf,
    target: PathBuf,
) -> Result<(), String> {
    let shared = new_capture();
    let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut io = TectonicIo::new(
        project_root,
        None,
        bundle,
        Some(digest),
        Some(format_cache_dir),
        shared.clone(),
        requests,
        None,
    );
    // format 趟开关：主输入换成合成的 `\input tectonic-format-latex.tex`（上游 `enter_format_mode`）。
    io.set_format_pass(Arc::new(AtomicBool::new(true)));
    let mut driver = MinimalDriver::new(io);
    let progress: Arc<dyn CompileProgress> = Arc::new(NoProgress);
    let log_ring: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut status = ProgressStatus::new(progress, log_ring);
    let result = {
        let mut launcher = CoreBridgeLauncher::new(&mut driver, &mut status);
        (|| -> Result<(), String> {
            let mut init = TexEngine::default();
            init.halt_on_error_mode(true);
            init.initex_mode(true);
            init.build_date(SystemTime::now());
            let outcome = init
                .process(&mut launcher, crate::io::FORMAT_DUMP_PLACEHOLDER, "texput")
                .map_err(|e| format!("预热 initex 趟失败：{e:#}"))?;
            if matches!(outcome, TexOutcome::Errors) {
                return Err("预热失败：initex 趟报 Errors（format 没建起来）".to_owned());
            }
            let Some((_name, bytes)) = take_dumped_format(&shared) else {
                return Err("预热失败：initex 趟没有产出任何 `*.fmt`".to_owned());
            };
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("预热失败：建 format 缓存目录失败（{}）：{e}", parent.display())
                })?;
            }
            // 原子落盘：半成品不能被首编当成"缓存命中"。
            let tmp = target.with_extension("fmt.tmp");
            std::fs::write(&tmp, &bytes)
                .map_err(|e| format!("预热失败：写 format 临时文件失败（{}）：{e}", tmp.display()))?;
            if let Err(e) = std::fs::rename(&tmp, &target) {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("预热失败：落盘 format 失败（{}）：{e}", target.display()));
            }
            Ok(())
        })()
    };
    drop(driver);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个只带指定输出的捕获表（喂给下面几个纯判据函数）。
    fn capture_with(files: &[(&str, &str)]) -> SharedCapture {
        let c = new_capture();
        {
            let mut g = c.lock().unwrap_or_else(|e| e.into_inner());
            for (name, body) in files {
                g.files.insert((*name).to_owned(), body.as_bytes().to_vec());
            }
        }
        c
    }

    /// 同上，但装**二进制**内容（页哈希那条路要原始字节）。
    fn capture_with_bytes(files: &[(&str, Vec<u8>)]) -> SharedCapture {
        let c = new_capture();
        {
            let mut g = c.lock().unwrap_or_else(|e| e.into_inner());
            for (name, body) in files {
                g.files.insert((*name).to_owned(), body.clone());
            }
        }
        c
    }

    /// 检出信号是 **`.aux` 里的 `\bibdata`**（上游 `is_bibtex_needed`），不是扫源码找 `\cite`。
    ///
    /// 这条同时是 V-03 的回归守卫：先前按源码扫 `\cite` 判，既会漏（`\cite` 只在注释里）又会白跑。
    #[test]
    fn bibtex_is_requested_only_when_the_aux_has_bibdata() {
        // 用了 \bibliography ⇒ aux 里有 \bibdata ⇒ 要跑
        let with_bib = capture_with(&[(
            "main.aux",
            "\\relax\n\\citation{ref1}\n\\bibdata{refs}\n\\bibstyle{plain}\n",
        )]);
        assert_eq!(aux_files_requesting_bibtex(&with_bib, "main.aux"), vec!["main.aux".to_owned()]);

        // `\cite` + `thebibliography`：aux 只有 \citation/\bibcite，**没有** \bibdata ⇒ 不跑
        let thebib = capture_with(&[("main.aux", "\\relax\n\\citation{ref1}\n\\bibcite{ref1}{1}\n")]);
        assert!(
            aux_files_requesting_bibtex(&thebib, "main.aux").is_empty(),
            "thebibliography 不需要 BibTeX，扫源码的旧口径会在这里白跑一趟"
        );

        // 完全没有 .aux（空文档）：不跑，也不该 panic
        let none = capture_with(&[("main.log", "x")]);
        assert!(aux_files_requesting_bibtex(&none, "main.aux").is_empty());
    }

    /// `\include` 分章：含 `\bibdata` 的子 aux 也要跑，且主 aux 排在最前（上游口径）。
    #[test]
    fn sub_aux_files_with_bibdata_are_included_and_primary_comes_first() {
        let c = capture_with(&[
            ("main.aux", "\\relax\n\\@input{ch1.aux}\n\\bibdata{refs}\n"),
            ("ch1.aux", "\\relax\n\\citation{a}\n"),
            ("ch2.aux", "\\relax\n\\bibdata{ch2refs}\n"),
        ]);
        let got = aux_files_requesting_bibtex(&c, "main.aux");
        assert_eq!(got, vec!["main.aux".to_owned(), "ch2.aux".to_owned()]);
        assert!(!got.contains(&"ch1.aux".to_owned()), "无 \\bibdata 的章 aux 不该跑（避免噪音）");
    }

    /// biber 的检出信号是 `<stem>.run.xml` 里**真的**写 `<binary>biber</binary>`
    /// （上游 `check_biber_requirement`）；`backend=bibtex` 也写 `.run.xml`，但请求的是 bibtex。
    ///
    /// 阶段① review E2 的回归守卫：只看文件存在会把"参考文献已正常生成"的文档误报成需要 biber。
    #[test]
    fn biber_is_detected_by_binary_element_not_by_file_presence() {
        let with = capture_with(&[(
            "main.run.xml",
            "<requests><request><binary>biber</binary></request></requests>",
        )]);
        assert!(run_xml_requests_biber(&with, "main"));

        let bibtex_backend = capture_with(&[(
            "main.run.xml",
            "<requests><request><binary>bibtex</binary></request></requests>",
        )]);
        assert!(
            !run_xml_requests_biber(&bibtex_backend, "main"),
            "backend=bibtex 也写 run.xml，但它请求的是 bibtex —— 不许误报成 biber"
        );

        let no_run_xml = capture_with(&[("main.aux", "\\bibdata{refs}")]);
        assert!(!run_xml_requests_biber(&no_run_xml, "main"));
        // 读不到 / 没有闭合标签 → 不算（宁可漏报也不误报）
        assert!(!run_xml_binary_requests_biber("<binary>biber"));
        assert!(!run_xml_binary_requests_biber(""));
    }

    /// D3①：三类外部工具的提示只在**检出**时出现，顺序固定；`.idx`/`.glo` 按文件存在判。
    #[test]
    fn external_tool_notices_cover_the_three_nonexecutable_tools() {
        let all = capture_with(&[
            ("main.run.xml", "<request><binary>biber</binary></request>"),
            ("main.idx", ""),
            ("main.glo", ""),
        ]);
        let notices = external_tool_notices(&all, "main");
        assert_eq!(notices.len(), 3, "{notices:?}");
        assert!(notices[0].contains("biber"), "{}", notices[0]);
        assert!(notices[1].contains("makeindex"), "{}", notices[1]);
        assert!(notices[2].contains("glossaries"), "{}", notices[2]);

        // 普通文档（无外部工具签名）：零提示（不得无端吓用户）
        let plain = capture_with(&[("main.log", "This is XeTeX"), ("main.aux", "\\relax")]);
        assert!(external_tool_notices(&plain, "main").is_empty());
    }

    /// biber 声明文案必须与**内嵌形态的事实**一致：引擎不运行外部工具 ⇒ 不调用 biber、不产 `.bbl`；
    /// 同时点明"引用可能未解析"与可行动出路（不伪造能力、也不静默降级 —— ADR-0015 第 4 条）。
    ///
    /// 反向断言守住 T9 实测口径：**不得**出现子进程形态的「引擎调用外部 biber、缺失或版本不匹配才失败」。
    #[test]
    fn biber_notice_states_external_tool_and_nonresolution() {
        assert!(BIBER_NOTICE.contains("不运行外部工具"), "{}", BIBER_NOTICE);
        assert!(BIBER_NOTICE.contains("未解析"), "{}", BIBER_NOTICE);
        assert!(BIBER_NOTICE.contains("backend=bibtex"), "{}", BIBER_NOTICE);
        assert!(BIBER_NOTICE.contains("不会生成 .bbl"), "{}", BIBER_NOTICE);
        // T13 反向断言：内嵌形态不运行外部工具，不得沿用子进程口径。
        // 字面量拆开拼，免得本文件自己命中验收对这两句旧话的零命中门禁。
        let old_claim = ["会", "调用外部 biber"].concat();
        let engine_claim = ["引擎", "会调用"].concat();
        assert!(!BIBER_NOTICE.contains(&old_claim), "内嵌引擎不调用外部 biber：{}", BIBER_NOTICE);
        assert!(!BIBER_NOTICE.contains(&engine_claim), "{}", BIBER_NOTICE);
        // 警告文案（日志通道）同口径
        assert!(
            crate::BIBER_PASS_MISSING_NOTE.contains("可能未解析"),
            "{}",
            crate::BIBER_PASS_MISSING_NOTE
        );
        assert!(
            !crate::BIBER_PASS_MISSING_NOTE.contains("外部 biber 二进制"),
            "内嵌形态不需要外部 biber 二进制：{}",
            crate::BIBER_PASS_MISSING_NOTE
        );
        assert!(!crate::BIBER_PASS_MISSING_NOTE.contains(&old_claim));
    }

    /// D3②：超时证据文本 = `.log` + 去噪后的状态尾部；已上报页要能兜住尾部窗口漏掉的 `[N]`。
    #[test]
    fn timeout_evidence_merges_log_and_status_tail_and_observed_pages() {
        let text = timeout_evidence_text("log 里有 [12]\n", "[1mNote: 页 [13][0m\r\n\n", Some(14));
        assert!(text.contains("log 里有 [12]"), "日志要保留：{text}");
        assert!(text.contains("Note: 页 [13]"), "状态尾部要保留（ANSI/CR 已去）：{text}");
        assert!(!text.contains('\u{1b}'), "ANSI 必须去掉：{text:?}");
        assert!(!text.contains('\r'), "回车必须去掉：{text:?}");
        assert!(text.contains("[14]"), "已上报页要补成页标记（同一套解析）：{text}");
        assert_eq!(pages_typeset(&text), Some(14));

        // 尾部已经记到更大页码时不再补（不重复、不伪造前进）
        let text = timeout_evidence_text("", "[20]\n", Some(14));
        assert_eq!(pages_typeset(&text), Some(20));

        // 两个来源都空 ⇒ 空文本（调用方据此报"无页输出"）
        assert!(timeout_evidence_text("", "", None).is_empty());
    }

    /// D3②：超时条目必须带**证据化诊断**（不是只给一句"超时了"），并把协作式取消的性质说清。
    #[test]
    fn timeout_entry_carries_diagnosis_and_cooperative_cancel_wording() {
        let entry = timeout_entry(Duration::from_secs(120), true, Some(3), "[1] [2]\n", true);
        assert_eq!(entry.kind, ErrorKind::Timeout);
        assert!(entry.message.contains("内嵌引擎编译超过 120s"), "{}", entry.message);
        assert!(entry.message.contains("无法强制结束"), "{}", entry.message);
        assert!(entry.message.contains("证据（引擎输出）"), "{}", entry.message);
        assert!(entry.message.contains("已排版到第 2 页"), "{}", entry.message);
        assert!(entry.message.contains("增量编译"), "{}", entry.message);
        let diag = entry.diagnosis.expect("超时必须有诊断");
        assert_eq!(diag.kind, DiagnosisKind::CompileTimeout, "有页输出 ⇒ 在推进，只是慢");
        assert!(diag.suggested_timeout_secs.is_some(), "有页输出时该给一键提高超时的建议");

        // 没有任何页输出 ⇒ 不武断说卡住（core 的 CompileTimeoutStalled 口径）
        let stalled = timeout_entry(Duration::from_secs(30), false, None, "", false);
        assert_eq!(stalled.diagnosis.expect("诊断").kind, DiagnosisKind::CompileTimeoutStalled);
        assert!(stalled.message.contains("仍在收尾"), "{}", stalled.message);
    }

    /// D7：源码版模板提示（`.ins`/`.dtx`）在失败路径保留语义，且只动"缺 `.cls`"那几条。
    #[test]
    fn enrich_source_release_hints_keeps_stage_one_semantics() {
        let mut errors = entries_from_log(
            "! LaTeX Error: File `thuthesis.cls' not found.\n",
            true,
        );
        assert!(!errors.is_empty(), "夹具必须解析出缺 .cls 错误");
        let candidates = vec!["thuthesis.ins".to_owned(), "README.md".to_owned()];
        let enriched = enrich_with_candidates(errors.clone(), &candidates);
        let hint = enriched[0].diagnosis.as_ref().and_then(|d| d.hint.clone()).expect("要有源码版模板提示");
        assert!(hint.contains("thuthesis.ins"), "要点名该编哪个文件：{hint}");
        assert!(hint.contains("xelatex"), "要给可执行命令：{hint}");

        // 项目里没有 .ins/.dtx ⇒ 提示保持原样（降级为泛化建议，不编造命令）
        let untouched = enrich_with_candidates(errors.clone(), &["README.md".to_owned()]);
        assert_eq!(
            untouched[0].diagnosis.as_ref().and_then(|d| d.hint.clone()),
            errors[0].diagnosis.as_ref().and_then(|d| d.hint.clone()),
            "没有源码版模板文件时不得改写提示"
        );

        // 无诊断 / 非缺 .cls 的错误原样返回
        errors.clear();
        assert!(enrich_with_candidates(errors, &candidates).is_empty());
    }

    /// D3③：页进度的产端 —— 捕获表里的 XDV 完整页数（不是"零页"、也不是 `[N]` 文本）。
    #[test]
    fn captured_pages_counts_complete_xdv_pages() {
        let c = capture_with_bytes(&[("main.xdv", tiny_xdv(4))]);
        assert_eq!(captured_pages(&c, "main.xdv"), 4);
        // 没有 XDV ⇒ 0（"还数不出来"，不是"零页"：调用方只在变大时才发事件）
        assert_eq!(captured_pages(&c, "nope.xdv"), 0);
        // 半页（缺最后一页的 eop + post）：只算完整页
        let mut half = tiny_xdv(2);
        half.pop(); // 去掉 post(248)
        half.pop(); // 去掉最后一页的 eop(140)
        let c = capture_with_bytes(&[("main.xdv", half)]);
        assert_eq!(captured_pages(&c, "main.xdv"), 1);
    }

    /// **编辑触发档的收敛预算**（2026-09-15）：三道边界都要钉住 —— 预算内且第 1 趟够快才允许
    /// 再跑（编辑后直接收敛成正确结果），否则停在草稿态、交给空闲收敛。
    #[test]
    fn quick_budget_allows_only_within_bounds() {
        let ms = Duration::from_millis;
        let fast = ms(400); // 28 页夹具的第 1 趟
        let slow = ms(1900); // 125 页 / 462 KB 夹具的第 1 趟（实测）
        assert!(quick_pass_allowed(1, fast, ms(400)), "第 1 趟快、时间没到 ⇒ 允许再跑");
        assert!(quick_pass_allowed(2, fast, ms(1500)), "第 1 趟快、还没到预算 ⇒ 允许再跑");
        assert!(!quick_pass_allowed(3, fast, ms(100)), "跑满 QUICK_MAX_PASSES ⇒ 不允许");
        assert!(!quick_pass_allowed(1, slow, ms(1900)), "第 1 趟就慢 ⇒ 不再多跑（大文档退化点）");
        assert!(!quick_pass_allowed(1, fast, ms(2000)), "预算到点即停（判据是 < 预算）");
        assert!(!quick_pass_allowed(1, fast, ms(9000)), "整轮超预算 ⇒ 停");
        // 门槛之间的关系：第 1 趟门槛必须是预算的一半，否则"慢文档退化"就没有余量。
        assert_eq!(QUICK_FIRST_PASS_MAX * 2, QUICK_CONVERGENCE_BUDGET);
        // 编辑触发档必须比 Full 更早收手，否则就不是"预算"了。
        assert!(QUICK_MAX_PASSES < MAX_TEX_PASSES);
        // 同值不是巧合：编辑触发档只要能在"那 2 s 等待"内收敛，就严格优于先落后再补一趟。
        assert_eq!(QUICK_CONVERGENCE_BUDGET, ms(2000), "必须与前端空闲收敛的 DELAY_MS 同值");
    }

    /// 收敛支持位必须与实现一致：bib 已支持、biber 不支持（内嵌不运行外部工具）、收敛**有条件**支持、
    /// 页哈希已支持。
    #[test]
    fn support_flags_match_the_implementation() {
        assert!(crate::BIB_PASS_SUPPORTED, "bib 趟已实现 ⇒ 该位必须为 true");
        assert!(
            !crate::BIBER_PASS_SUPPORTED,
            "biber 是外部程序、内嵌引擎不运行外部工具 ⇒ 仍不支持"
        );
        assert!(
            crate::CONVERGENCE_SUPPORTED,
            "重跑循环已实现（跑到中间产物稳定）⇒ 该位为 true；编辑触发档在预算内也收敛，\
             只有预算用尽或含 biber/makeindex/glossaries 的文档才退回 Quick"
        );
        assert!(
            crate::PAGE_HASH_SUPPORTED,
            "页哈希已接线（捕获表的 XDV → core::xdv::page_hashes）⇒ 该位为 true"
        );
    }

    /// 最小 XDV：pre + n 页（bop + 44 B 计数器/prev + 1 B 页体 + eop）+ post。
    /// 每页页体不同 ⇒ 每页哈希应当不同。
    fn tiny_xdv(pages: usize) -> Vec<u8> {
        let mut v = vec![247u8, 7];
        v.extend_from_slice(&[0u8; 12]);
        v.push(0);
        for i in 0..pages {
            v.push(139); // bop
            v.extend_from_slice(&[0u8; 44]);
            v.push(i as u8);
            v.push(140); // eop
        }
        v.push(248); // post
        v
    }

    /// **页哈希的接线**：捕获表里的 XDV → `core::xdv::page_hashes`。
    ///
    /// 与子进程档用同一个函数是硬要求（两套口径会让"换形态"被判成整篇都变了），
    /// 所以这里断言的是"走的是 core 那条路 + 页数/页级差异正确"。
    #[test]
    fn page_hashes_come_from_the_captured_xdv_via_core() {
        let c = capture_with_bytes(&[("main.xdv", tiny_xdv(3))]);
        let bytes = capture_bytes(&c, "main.xdv").expect("捕获表里应当有 XDV");
        let hashes = latteset_core::xdv::page_hashes(&bytes);
        assert_eq!(hashes.len(), 3, "3 页就该给 3 个哈希");
        assert_ne!(hashes[0], hashes[1], "页体不同 ⇒ 哈希必须不同");
        assert_ne!(hashes[1], hashes[2]);

        // 缺失 / 空 ⇒ 空表 = **无法判定**（不是"零页"）
        assert!(latteset_core::xdv::page_hashes(&[]).is_empty());
        assert!(capture_bytes(&c, "nope.xdv").is_none(), "没写过的名字要返回 None");
    }

    /// **A 的判据**（D-L4b，Quick/Full 共用）：页表逐页相同 + 磁盘 PDF 身份一致，缺一不可。
    ///
    /// 反例清单（任务书要求自证）：页数变、页序变、PDF 被手工改/替换/删除、旧 PDF 缺失、空表。
    #[test]
    fn reuse_pdf_requires_unchanged_pages_and_an_untouched_pdf() {
        let hashes = vec![1u64, 2, 3];
        assert!(should_reuse_pdf(&hashes, Some(&hashes), true), "逐页相同 + PDF 未动 ⇒ 可复用（Quick/Full 同判据）");
        assert!(!should_reuse_pdf(&hashes, None, true), "首轮没有上一次的哈希 ⇒ 不复用");
        assert!(!should_reuse_pdf(&hashes, Some(&[9, 9, 9]), true), "页变了 ⇒ 不复用");
        assert!(
            !should_reuse_pdf(&hashes, Some(&[1, 2]), true),
            "页数变了 ⇒ 不复用（prev != cur）"
        );
        assert!(
            !should_reuse_pdf(&hashes, Some(&[3, 2, 1]), true),
            "页序变了 ⇒ 逐页哈希序列不同 ⇒ 不复用"
        );
        assert!(!should_reuse_pdf(&hashes, Some(&hashes), false), "PDF 被手改/替换/删除 ⇒ 必须转换");
        assert!(!should_reuse_pdf(&[], Some(&[]), true), "空表是'无法判定'，不得当成'零页相同'");
    }

    /// **PDF 身份记录**：往返可解析；磁盘 PDF 被手工改/删除后必须判为"不可复用"。
    ///
    /// 这正是 Full 复用新增的那道判据的可判定证据（页哈希相同 ≠ 磁盘 PDF 没被动过）。
    #[test]
    fn pdf_identity_detects_manual_edits_and_missing_files() {
        let dir = std::env::temp_dir().join(format!(
            "latteset-tectonic-pdfid-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let pdf = dir.join("main.pdf");
        let cache = pdf_identity_cache_path(&dir, "main");
        std::fs::write(&pdf, b"%PDF-1.5 original").expect("写 PDF 夹具");

        // ① 转换成功后记身份 ⇒ 下一轮可复用。
        let id = pdf_identity(&pdf).expect("取身份");
        std::fs::write(&cache, format_pdf_identity(&id)).expect("写身份记录");
        assert_eq!(parse_pdf_identity(&format_pdf_identity(&id)), Some(id), "身份记录必须可往返");
        assert!(pdf_unchanged_since_we_wrote_it(&pdf, &cache), "刚写出的 PDF ⇒ 身份一致");

        // ② 用户手工改 PDF（内容/大小变）⇒ 身份不一致 ⇒ 不复用。
        std::fs::write(&pdf, b"%PDF-1.5 hand edited, longer").expect("手改 PDF");
        assert!(!pdf_unchanged_since_we_wrote_it(&pdf, &cache), "PDF 被手工改 ⇒ 必须重新转换");

        // ③ 同长度覆盖也要能发现（mtime 变了；写后小睡确保时间戳粒度不会吃掉差异）。
        std::fs::write(&pdf, b"%PDF-1.5 hand edited, longer").expect("同长度覆盖");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&pdf, b"%PDF-1.5 hand edited, longer").expect("同长度覆盖 2");
        assert!(
            !pdf_unchanged_since_we_wrote_it(&pdf, &cache),
            "同长度但被改过（mtime 变）⇒ 不复用"
        );

        // ④ 旧 PDF 被删除 / 没有身份记录（老项目/清过 tmp）⇒ 保守转换。
        std::fs::remove_file(&pdf).expect("删 PDF");
        assert!(!pdf_unchanged_since_we_wrote_it(&pdf, &cache), "PDF 缺失 ⇒ 必须转换");
        std::fs::write(&pdf, b"%PDF-1.5 original").expect("重建 PDF");
        let _ = std::fs::remove_file(&cache);
        assert!(!pdf_unchanged_since_we_wrote_it(&pdf, &cache), "缺身份记录 ⇒ 保守转换一次");

        // ⑤ 损坏/老格式身份记录 ⇒ 当"没有记录"（不猜）。
        std::fs::write(&cache, "not-a-version\n").expect("写坏记录");
        assert!(!pdf_unchanged_since_we_wrote_it(&pdf, &cache));
        assert_eq!(parse_pdf_identity(""), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 重跑判据只覆盖会被回读的中间产物（不做无谓的大文件摘要）。
    #[test]
    fn rerun_snapshot_covers_read_back_intermediates_only() {
        let c = capture_with(&[
            ("main.aux", "\\relax\n\\bibcite{a}{1}\n"),
            ("main.toc", "\\contentsline{section}{A}{1}\n"),
            ("main.bbl", "\\begin{thebibliography}{1}\n"),
            ("main.pdf", "%PDF-1.5 很大但不会被回读"),
            ("main.xdv", "xdv"),
            ("main.log", "log"),
        ]);
        let snap = rerun_snapshot(&c);
        let names: Vec<&str> = snap.keys().map(String::as_str).collect();
        assert!(names.contains(&"main.aux"), "{names:?}");
        assert!(names.contains(&"main.toc"), "{names:?}");
        assert!(names.contains(&"main.bbl"), "{names:?}");
        for excluded in ["main.pdf", "main.xdv", "main.log"] {
            assert!(!names.contains(&excluded), "{excluded} 不该进重跑判据：{names:?}");
        }
    }

    /// 判据本身：内容变了 ⇒ 摘要变 ⇒ 触发重跑；内容不变 ⇒ 不重跑（收敛）。
    #[test]
    fn rerun_snapshot_changes_only_when_content_changes() {
        let c = capture_with(&[("main.aux", "\\relax\n\\citation{a}\n")]);
        let first = rerun_snapshot(&c);
        assert_eq!(rerun_snapshot(&c), first, "内容没动就不该判为变化（否则会无限重跑）");

        {
            let mut g = c.lock().unwrap_or_else(|e| e.into_inner());
            g.files.insert("main.aux".to_owned(), b"\\relax\n\\bibcite{a}{1}\n".to_vec());
        }
        assert_ne!(rerun_snapshot(&c), first, "aux 变了必须判为变化（否则引用永远解析不出来）");
    }

    // ---- bib 跳过（方案 §6.4 ④）----

    /// **递归预热**：`\include` 分章的子目录中间产物也要进捕获表，且键用正斜杠。
    ///
    /// 不递归的后果在 28 页 8 章档上实测是"每轮必多跑一趟排版"：那些 aux 每趟都被判成"新出现"。
    #[test]
    fn seeding_is_recursive_and_uses_forward_slash_keys() {
        let dir = BibTempDir::new("seed");
        std::fs::create_dir_all(dir.0.join("chapters")).expect("建子目录");
        std::fs::write(dir.0.join("main.aux"), b"\\bibdata{refs}\n").expect("写夹具");
        std::fs::write(dir.0.join("chapters/ch01.aux"), b"\\citation{a}\n").expect("写夹具");
        std::fs::write(dir.0.join("main.pdf"), b"%PDF").expect("写夹具");

        let c = capture_with(&[]);
        seed_previous_intermediates(&c, &dir.0);
        let keys: Vec<String> = c.lock().unwrap_or_else(|e| e.into_inner()).files.keys().cloned().collect();
        assert!(keys.contains(&"main.aux".to_owned()), "{keys:?}");
        assert!(
            keys.contains(&"chapters/ch01.aux".to_owned()),
            "子目录里的中间产物必须预热（键用正斜杠，与引擎写的名字一致）：{keys:?}"
        );
        assert!(!keys.iter().any(|k| k.ends_with(".pdf")), "`.pdf` 不该进捕获表：{keys:?}");
    }
    /// 临时项目目录（随用随建、Drop 时递归删）。
    struct BibTempDir(PathBuf);

    impl BibTempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("latteset-tectonic-bibsig-{tag}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("建临时目录");
            Self(dir)
        }
        fn write(&self, rel: &str, body: &str) {
            std::fs::write(self.0.join(rel), body).expect("写夹具");
        }
    }

    impl Drop for BibTempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 主 aux 只有 `\bibdata`/`\bibstyle`，`\citation` 在**章 aux**（`\include` 分章）。
    fn split_aux_capture() -> SharedCapture {
        capture_with(&[
            (
                "main.aux",
                "\\relax\n\\@input{chapters/ch01.aux}\n\\bibstyle{plain}\n\\bibdata{refs}\n",
            ),
            ("chapters/ch01.aux", "\\relax\n\\citation{ref1}\n\\citation{ref2}\n"),
        ])
    }

    /// **引用集合在章 aux 里**：签名必须跟 `\@input` 闭包，否则新增一条 `\cite` 会被判成"没变"，
    /// `.bbl` 少一条条目而排版趟只报 `Citation undefined` 警告 —— 静默的错误输出。
    #[test]
    fn bib_signature_follows_the_input_closure() {
        let dir = BibTempDir::new("closure");
        dir.write("refs.bib", "@book{ref1, title={A}}\n");
        let auxes = vec!["main.aux".to_owned()];

        let before = bib_signature(&split_aux_capture(), &auxes, &dir.0).expect("签名");
        // 章 aux 里多一条引用 ⇒ 签名必须变
        let after_capture = split_aux_capture();
        {
            let mut g = after_capture.lock().unwrap_or_else(|e| e.into_inner());
            g.files.insert(
                "chapters/ch01.aux".to_owned(),
                b"\\relax\n\\citation{ref1}\n\\citation{ref2}\n\\citation{ref3}\n".to_vec(),
            );
        }
        let after = bib_signature(&after_capture, &auxes, &dir.0).expect("签名");
        assert_ne!(before.commands, after.commands, "新增 \\cite 必须改变签名");

        // 与 bibtex 无关的 aux 内容变化**不该**改变签名（否则每次改正文都白跑 bibtex）
        let toc_capture = split_aux_capture();
        {
            let mut g = toc_capture.lock().unwrap_or_else(|e| e.into_inner());
            g.files.insert(
                "chapters/ch01.aux".to_owned(),
                b"\\relax\n\\@writefile{toc}{\\contentsline {section}{x}{9}}\n\\citation{ref1}\n\\citation{ref2}\n".to_vec(),
            );
        }
        assert_eq!(
            before.commands,
            bib_signature(&toc_capture, &auxes, &dir.0).expect("签名").commands,
            "目录页码变了不该触发 bibtex（那是排版趟的事）"
        );
    }

    /// `.bib` 的状态进了签名：内容变（size 变）⇒ 签名变；`.bib` 不在项目磁盘上 ⇒ **判不出来**。
    #[test]
    fn bib_signature_tracks_database_state_and_fails_open() {
        let dir = BibTempDir::new("dbstate");
        dir.write("refs.bib", "@book{ref1, title={A}}\n");
        let c = split_aux_capture();
        let auxes = vec!["main.aux".to_owned()];
        let before = bib_signature(&c, &auxes, &dir.0).expect("签名");
        assert_eq!(before.databases.len(), 1);
        assert_eq!(before.databases[0].0, "refs.bib");

        dir.write("refs.bib", "@book{ref1, title={A}}\n@book{ref2, title={B}}\n");
        let after = bib_signature(&c, &auxes, &dir.0).expect("签名");
        assert_ne!(before.databases, after.databases, "`.bib` 变了必须反映到签名里");

        // 读不到 `.bib`（bundle 提供 / 路径展开）⇒ None ⇒ 调用方不跳过
        let empty = BibTempDir::new("nodir");
        assert!(bib_signature(&c, &auxes, &empty.0).is_none(), "判不出来就不能给签名");
    }

    /// 跳过判定：**三件事同时成立**才跳；`.bbl` 缺失一律不跳（没有产物可复用）。
    #[test]
    fn bib_pass_skips_only_on_a_matching_signature_with_bbl_present() {
        let sig = BibSignature { commands: 7, databases: vec![("refs.bib".into(), 1, 2)] };
        let other = BibSignature { commands: 8, databases: vec![("refs.bib".into(), 1, 2)] };

        assert!(bib_pass_skippable(Some(&sig), Some(&sig), true));
        assert!(!bib_pass_skippable(Some(&sig), Some(&other), true), "签名不同必须重跑");
        assert!(!bib_pass_skippable(Some(&sig), None, true), "没有缓存必须重跑（首编）");
        assert!(!bib_pass_skippable(Some(&sig), Some(&sig), false), "`.bbl` 不在必须重跑");
        assert!(!bib_pass_skippable(None, Some(&sig), true), "判不出来必须重跑");
    }

    /// 缓存文本的往返：格式错一律当"没有缓存"（⇒ 重跑），不静默跳过。
    #[test]
    fn bib_signature_text_roundtrip_and_rejects_junk() {
        let sig = BibSignature {
            commands: 0xdead_beef,
            databases: vec![("refs.bib".into(), 1_700_000_000_123_456_789, 2369)],
        };
        assert_eq!(BibSignature::parse(&sig.format()).as_ref(), Some(&sig));

        for junk in ["", "v2\n", "v1\ncommands\tzz\n", "v1\ndb\trefs.bib\t1\n", "v1\nwhat\t1\n"] {
            assert!(BibSignature::parse(junk).is_none(), "{junk:?} 不该被当成有效缓存");
        }
        // 名字里带制表符也不会错位（名字取前 n-2 段）
        let weird = BibSignature { commands: 1, databases: vec![("a\tb.bib".into(), 5, 6)] };
        assert_eq!(BibSignature::parse(&weird.format()).as_ref(), Some(&weird));
    }

    // ---------------------------------------------------------------- 真编（`#[ignore]`）

    use latteset_core::project::DirEntry;
    use latteset_core::scheduler::NoProgress;

    /// 极简 `FileSystem`（真编用例专用，与 `examples/bench.rs` 同一套）：runner 只经 `read_bytes`
    /// 读根文件、`exists` 看上一趟的 `.aux`，其余输入走自持 I/O 层 ⇒ 直读 `std::fs` 即可。
    struct LocalFs;

    #[async_trait]
    impl FileSystem for LocalFs {
        async fn read_dir(&self, path: &Path) -> std::io::Result<Vec<DirEntry>> {
            let mut out = Vec::new();
            for e in std::fs::read_dir(path)? {
                let e = e?;
                out.push(DirEntry { path: e.path(), is_dir: e.file_type()?.is_dir() });
            }
            Ok(out)
        }
        async fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
            std::fs::read_to_string(path)
        }
        async fn read_bytes(&self, path: &Path) -> std::io::Result<Vec<u8>> {
            std::fs::read(path)
        }
        async fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
            std::fs::canonicalize(path)
        }
        async fn is_dir(&self, path: &Path) -> std::io::Result<bool> {
            Ok(path.is_dir())
        }
        async fn exists(&self, path: &Path) -> std::io::Result<bool> {
            Ok(path.exists())
        }
        async fn write(&self, path: &Path, contents: &str) -> std::io::Result<()> {
            std::fs::write(path, contents)
        }
        async fn create_dir(&self, path: &Path) -> std::io::Result<()> {
            std::fs::create_dir(path)
        }
        async fn remove_file(&self, path: &Path) -> std::io::Result<()> {
            std::fs::remove_file(path)
        }
        async fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
            std::fs::remove_dir_all(path)
        }
        async fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
            std::fs::rename(from, to)
        }
    }

    /// 记录型反馈通道（验证 D3①/③ 的产端真的被调用）。
    #[derive(Default)]
    struct RecordingProgress {
        pages: Mutex<Vec<u32>>,
        notices: Mutex<Vec<String>>,
        partial_pages: Mutex<Vec<u32>>,
    }

    impl CompileProgress for RecordingProgress {
        fn pages(&self, pages: u32) {
            self.pages.lock().unwrap_or_else(|e| e.into_inner()).push(pages);
        }
        fn notice(&self, msg: &str) {
            self.notices.lock().unwrap_or_else(|e| e.into_inner()).push(msg.to_owned());
        }
        fn partial_pdf(&self, _path: &Path, pages: u32) {
            self.partial_pages.lock().unwrap_or_else(|e| e.into_inner()).push(pages);
        }
    }

    /// **真编**（`#[ignore]`，本机夹具）：D3①/D3③ 的产端端到端验证 + 超时证据路径。
    ///
    /// 闸门 = PLAN [D6] 口径「编进来 + 有可用 bundle」：本机夹具 `test_file/tectonic-bundle`
    /// （424 文件 / 自带 SHA256SUM，被 gitignore）存在才跑；CI 无夹具时用例自行跳过。
    /// 缓存落在**工作区**（`test_file/tmp/tectonic-real-compile/cache`）—— 本会话写宿主
    /// `%TEMP%`/用户缓存可能被拒，且不该污染用户的真实 Tectonic 缓存。
    ///
    /// 跑法（必须带 vcpkg 环境）：
    /// `pwsh -NoProfile -File scripts/with-tectonic-lib.ps1 -Mode check` 之后
    /// `cargo test -p latteset-tectonic --lib -- --ignored real_compile`
    #[tokio::test]
    #[ignore = "真编：需要本机 test_file/tectonic-bundle 夹具（gitignore；CI 用 bundle 缓存夹具）"]
    async fn real_compile_with_local_bundle_fixture() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("仓库根");
        // Windows 的 `canonicalize()` 带 `\\?\` verbatim 前缀；拼 `file:///` URL 时必须剥掉
        // （上游会把 `?` 当 URL 的一部分 ⇒ "failed to parse local path"）。
        let repo_url = {
            let s = repo.to_string_lossy().replace('\\', "/");
            s.strip_prefix("//?/").map(str::to_owned).unwrap_or(s)
        };
        let bundle_dir = repo.join("test_file/tectonic-bundle");
        if !bundle_dir.join("SHA256SUM").is_file() {
            eprintln!("跳过真编：本机没有 {} 夹具（gitignore）", bundle_dir.display());
            return;
        }
        let work = repo.join("test_file/tmp/tectonic-real-compile");
        let _ = std::fs::remove_dir_all(&work);
        let proj = work.join("proj");
        std::fs::create_dir_all(&proj).expect("建项目目录");
        let runner = TectonicLibRunner::new(Arc::new(LocalFs), Arc::new(NoProgress))
            .with_bundle(BundleSource::Explicit(format!(
                "file:///{repo_url}/test_file/tectonic-bundle"
            )))
            .with_cache_dir(work.join("cache"));

        // ① 普通 1 页文档：成功 + 1 页 + 页进度产端真的被调用。
        std::fs::write(
            proj.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\nHello Latteset.\n\\end{document}\n",
        )
        .expect("写夹具");
        let progress = Arc::new(RecordingProgress::default());
        // 反馈通道要在构造 runner 时注入（真编用例专用；产品装配点由 T4/T5 负责）。
        let runner = TectonicLibRunner { progress: progress.clone(), ..runner };
        let outcome = runner
            .compile(
                CompileRequest {
                    root_file: proj.join("main.tex"),
                    project_root: proj.clone(),
                    timeout: Duration::from_secs(120),
                    kind: CompileKind::Full,
                },
                CancellationToken::new(),
            )
            .await;
        match outcome {
            CompileOutcome::Success { pdf_path, page_hashes, .. } => {
                assert!(pdf_path.is_file(), "PDF 必须落在项目根：{}", pdf_path.display());
                assert_eq!(page_hashes.len(), 1, "1 页文档要出 1 个页哈希");
                let pdf = std::fs::read(&pdf_path).expect("读 PDF");
                assert!(pdf.starts_with(b"%PDF"), "产物必须是 PDF");
            }
            other => panic!("真编应当成功，实际：{other:?}"),
        }
        assert_eq!(
            progress.pages.lock().unwrap_or_else(|e| e.into_inner()).last().copied(),
            Some(1),
            "D3③：趟边界应当上报页进度"
        );

        // ② 外部工具提示（D3①）的端到端接线：上一趟留下的 `.idx` 会被预热进捕获表
        //    （`RERUN_EXTENSIONS` 含 `.idx`），成功路径必须据此发"索引不会自动生成"的 notice。
        //    （本机最小 bundle 没有 makeidx.sty，所以用"上一趟的产物"造出同一判据，而不是再装宏包。）
        std::fs::write(proj.join("tmp/main.idx"), "\\indexentry{foo}{1}\n").expect("写 .idx 夹具");
        let outcome = runner
            .compile(
                CompileRequest {
                    root_file: proj.join("main.tex"),
                    project_root: proj.clone(),
                    timeout: Duration::from_secs(120),
                    kind: CompileKind::Full,
                },
                CancellationToken::new(),
            )
            .await;
        assert!(
            matches!(outcome, CompileOutcome::Success { .. }),
            "带 `.idx` 的文档仍应编译成功（索引只是不会自动生成），实际：{outcome:?}"
        );
        let notices = progress.notices.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert!(
            notices.iter().any(|n| n.contains("makeindex")),
            "D3①：检出 `.idx` 必须发 progress.notice，实际：{notices:?}"
        );

        // ③ 超时路径：极短超时 ⇒ Timeout，且证据化文案（协作式取消 + 诊断）在。
        let outcome = runner
            .compile(
                CompileRequest {
                    root_file: proj.join("main.tex"),
                    project_root: proj.clone(),
                    timeout: Duration::from_millis(1),
                    kind: CompileKind::Full,
                },
                CancellationToken::new(),
            )
            .await;
        match outcome {
            CompileOutcome::Timeout { entry } => {
                assert_eq!(entry.kind, ErrorKind::Timeout);
                assert!(entry.message.contains("内嵌引擎编译超过"), "{}", entry.message);
                assert!(entry.message.contains("无法强制结束"), "{}", entry.message);
                assert!(entry.diagnosis.is_some(), "超时必须带证据化诊断");
            }
            other => panic!("极短超时应当返回 Timeout，实际：{other:?}"),
        }

        let _ = std::fs::remove_dir_all(&work);
    }

    // ---------------------------------------------------------------- 首启 format 预热（T18）

    // 预热**计划**（本地主动 / 在线零网络判定）的纯函数用例在 `bundle::tests`
    // （`prewarm_plan_*`）：放那边才能与在线缓存的标记布局一起测，且与 feature 无关。

    /// T18 缺陷修复：**在线源 + 冷缓存**必须在**零网络、零写**下返回 `SkippedNoBundle`。
    ///
    /// 回归的是 `-Mode test`（4 包同建、`network-bundle` 特性被 server/src-tauri 打开）下的红：
    /// 上游 `detect_bundle` 对 http 源在缓存冷时会联网取 digest（~8s）并建 `data/` 目录。
    /// 本用例对 `Default` 与显式在线 URL 都断言：结果 Skip、耗时 ≈0、缓存目录**未被创建**；
    /// 不依赖网络可达性（无论特性开关/有网无网都应通过）。
    #[test]
    fn prewarm_online_cold_cache_skips_without_network_or_cache_writes() {
        let dir = std::env::temp_dir()
            .join(format!("latteset-tectonic-prewarm-online-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let cache = dir.join("cache");

        let started = Instant::now();
        let default_outcome = prewarm_format(&BundleSource::Default, &cache);
        let explicit_outcome = prewarm_format(
            &BundleSource::Explicit("https://example.invalid/default_bundle_v33.tar".into()),
            &cache,
        );
        let elapsed = started.elapsed();
        eprintln!("T18 修复：在线源冷缓存两次预判定耗时 {elapsed:?}（要求 ≈0ms、无网络）");

        assert!(
            matches!(default_outcome, PrewarmOutcome::SkippedNoBundle),
            "在线默认源冷缓存必须跳过（不得联网）：{default_outcome:?}"
        );
        assert!(
            matches!(explicit_outcome, PrewarmOutcome::SkippedNoBundle),
            "在线显式源冷缓存必须跳过（不得联网）：{explicit_outcome:?}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "在线冷缓存判定必须零网络（耗时 {elapsed:?}；联网取 digest 实测 ~8s）"
        );
        assert!(!cache.exists(), "零网络判定不得创建/污染缓存目录：{}", cache.display());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T18：预热失败必须**静默**且**不污染缓存**（半成品会被首编当成"缓存命中"）。
    #[test]
    fn prewarm_failure_is_silent_and_does_not_pollute_cache() {
        let dir = std::env::temp_dir()
            .join(format!("latteset-tectonic-prewarm-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let missing = dir.join("no-such-bundle.zip");
        let cache = dir.join("cache");
        match prewarm_format(&BundleSource::Shipped(missing), &cache) {
            PrewarmOutcome::Failed(msg) => {
                assert!(msg.contains("内置") || msg.contains("不存在"), "失败文案要可读：{msg}");
            }
            other => panic!("内置集缺失必须 Failed（静默失败）：{other:?}"),
        }
        let formats = cache.join("formats");
        let polluted = formats.is_dir()
            && std::fs::read_dir(&formats).map(|mut it| it.next().is_some()).unwrap_or(false);
        assert!(!polluted, "预热失败不得留下任何 .fmt（含半成品）");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T18：预热闸门 —— 编译侧在 `Running` 期间会被唤醒式等待，不会永久阻塞。
    #[test]
    fn prewarm_gate_wakes_waiting_compilers() {
        let gate = prewarm_gate();
        *gate.phase.lock().unwrap_or_else(|e| e.into_inner()) = GatePhase::Running;
        let started = Instant::now();
        let waiter = std::thread::spawn(|| {
            wait_for_prewarm();
            Instant::now()
        });
        std::thread::sleep(Duration::from_millis(80));
        *gate.phase.lock().unwrap_or_else(|e| e.into_inner()) = GatePhase::Done;
        gate.cv.notify_all();
        let woke = waiter.join().expect("预热等待线程");
        let waited = woke.duration_since(started);
        *gate.phase.lock().unwrap_or_else(|e| e.into_inner()) = GatePhase::Idle;
        assert!(waited >= Duration::from_millis(60), "编译侧必须真的等到 Running 结束：{waited:?}");
        assert!(waited < Duration::from_secs(5), "不应长期阻塞：{waited:?}");
    }
    /// T18 **真编**（`#[ignore]`）：预热生成 format 后，首编必须**命中缓存**（省掉 initex 趟），
    /// 且编译结果与未预热逐页一致（同一页哈希口径）。
    #[tokio::test]
    #[ignore = "真编：需要本机 test_file/tectonic-bundle 夹具（gitignore）"]
    async fn prewarm_then_first_compile_hits_format_cache() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("仓库根");
        let repo_url = {
            let s = repo.to_string_lossy().replace('\\', "/");
            s.strip_prefix("//?/").map(str::to_owned).unwrap_or(s)
        };
        let bundle_dir = repo.join("test_file/tectonic-bundle");
        if !bundle_dir.join("SHA256SUM").is_file() {
            eprintln!("跳过预热真编：本机没有 {} 夹具（gitignore）", bundle_dir.display());
            return;
        }
        let bundle = BundleSource::Explicit(format!("file:///{repo_url}/test_file/tectonic-bundle"));
        let work = repo.join("test_file/tmp/tectonic-prewarm-test");
        let _ = std::fs::remove_dir_all(&work);
        let proj = work.join("proj");
        std::fs::create_dir_all(&proj).expect("建项目目录");
        std::fs::write(
            proj.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\nPrewarm.\n\\end{document}\n",
        )
        .expect("写夹具");
        let make_runner = |cache: PathBuf| {
            TectonicLibRunner::new(Arc::new(LocalFs), Arc::new(NoProgress))
                .with_bundle(bundle.clone())
                .with_cache_dir(cache)
        };
        let make_req = || CompileRequest {
            root_file: proj.join("main.tex"),
            project_root: proj.clone(),
            timeout: Duration::from_secs(180),
            kind: CompileKind::Full,
        };

        // ① 冷缓存首编（自行生成 format）作为对照。
        let cold_started = Instant::now();
        let cold = make_runner(work.join("cache-cold"))
            .compile(make_req(), CancellationToken::new())
            .await;
        let cold_ms = cold_started.elapsed();
        let cold_hashes = match cold {
            CompileOutcome::Success { page_hashes, .. } => page_hashes,
            other => panic!("冷缓存首编应当成功：{other:?}"),
        };

        // ② 后台预热到**另一个空缓存**（产品场景：启动时缓存里还没有 .fmt）。
        let warm_cache = work.join("cache-warm");
        let prewarm_started = Instant::now();
        let outcome = prewarm_format(&bundle, &warm_cache);
        let prewarm_ms = prewarm_started.elapsed();
        assert!(
            matches!(outcome, PrewarmOutcome::Generated),
            "空缓存预热应当实际生成：{outcome:?}"
        );
        assert_eq!(
            prewarm_format(&bundle, &warm_cache),
            PrewarmOutcome::AlreadyCached,
            "二次预热必须命中已有缓存（不得重复生成）"
        );
        let fmt_count = std::fs::read_dir(warm_cache.join("formats"))
            .expect("formats 目录")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "fmt"))
            .count();
        assert_eq!(fmt_count, 1, "预热必须产出一份 .fmt");

        // ③ 预热后首编：命中缓存 ⇒ 省掉 initex 趟；结果与冷编逐页一致。
        let warm_started = Instant::now();
        let warm = make_runner(warm_cache)
            .compile(make_req(), CancellationToken::new())
            .await;
        let warm_ms = warm_started.elapsed();
        let warm_hashes = match warm {
            CompileOutcome::Success { page_hashes, .. } => page_hashes,
            other => panic!("预热后首编应当成功：{other:?}"),
        };
        assert_eq!(warm_hashes, cold_hashes, "预热与否必须逐页一致（同一页哈希口径）");
        assert!(
            warm_ms + Duration::from_millis(150) < cold_ms,
            "预热后首编应当省掉 initex 趟：cold={cold_ms:?} warm={warm_ms:?}（prewarm={prewarm_ms:?}）"
        );
        eprintln!(
            "T18 真编：prewarm={prewarm_ms:?} cold_first_compile={cold_ms:?} warm_first_compile={warm_ms:?} pages={}",
            cold_hashes.len()
        );
        let _ = std::fs::remove_dir_all(&work);
    }

    /// T19 阻断项真编（`#[ignore]`，**不经 `LATTESET_TECTONIC_BUNDLE`/Explicit**）：
    /// 用 [`BundleSource::Shipped`] + **canonicalize 后的 verbatim 路径**（release GUI 的真实形态：
    /// `\\?\E:\…\tectonic-bundle.zip`）直接编译；修复前这条路径在 `path_to_file_url` 处失败
    /// （`failed to parse local path`），修复后必须能正常出 PDF。
    #[tokio::test]
    #[ignore = "真编：需要随包 bundle（src-tauri/resources/tectonic-bundle.zip 或 test_file/tectonic-bundle）"]
    async fn shipped_bundle_compiles_through_verbatim_path() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("仓库根");
        let candidates = [
            repo.join("src-tauri/resources/tectonic-bundle.zip"),
            repo.join("src-tauri/target/release/tectonic-bundle.zip"),
            repo.join("test_file/tectonic-bundle"),
        ];
        let Some(shipped) = candidates.iter().find(|p| p.exists()) else {
            eprintln!("跳过 Shipped 真编：本机没有随包 bundle 产物（gitignore）");
            return;
        };
        // release 实况：资源解析给出的是 canonicalize 后的 `\\?\…` 形态（Windows）。
        let shipped = shipped.canonicalize().expect("canonicalize 随包 bundle");
        let work = repo.join("test_file/tmp/tectonic-shipped-test");
        let _ = std::fs::remove_dir_all(&work);
        let proj = work.join("proj");
        std::fs::create_dir_all(&proj).expect("建项目目录");
        std::fs::write(
            proj.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\nShipped bundle.\n\\end{document}\n",
        )
        .expect("写夹具");
        let runner = TectonicLibRunner::new(Arc::new(LocalFs), Arc::new(NoProgress))
            .with_bundle(BundleSource::Shipped(shipped.clone()))
            .with_cache_dir(work.join("cache"));
        let outcome = runner
            .compile(
                CompileRequest {
                    root_file: proj.join("main.tex"),
                    project_root: proj.clone(),
                    timeout: Duration::from_secs(180),
                    kind: CompileKind::Full,
                },
                CancellationToken::new(),
            )
            .await;
        match outcome {
            CompileOutcome::Success { pdf_path, page_hashes, .. } => {
                assert!(pdf_path.is_file(), "PDF 必须落在项目根：{}", pdf_path.display());
                let pdf = std::fs::read(&pdf_path).expect("读 PDF");
                assert!(pdf.starts_with(b"%PDF"), "产物必须是 PDF");
                assert_eq!(page_hashes.len(), 1, "1 页文档要出 1 个页哈希");
                eprintln!(
                    "T19 Shipped 真编：bundle={}（canonical={}）pages={}",
                    shipped.display(),
                    shipped.display(),
                    page_hashes.len()
                );
            }
            other => panic!(
                "Shipped 分支（verbatim 路径 {}）必须能编译成功：{other:?}",
                shipped.display()
            ),
        }
        let _ = std::fs::remove_dir_all(&work);
    }
}
