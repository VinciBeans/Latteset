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
//!    重跑只会得到同一份 `.bbl`，却会连带多跑一趟排版。**biber 仍不支持**（biblatex 要外部 `biber`）：
//!    检出 `<stem>.run.xml` 时显式 `warn!`，见 [`crate::BIBER_PASS_MISSING_NOTE`]。
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use async_trait::async_trait;
use latteset_core::log_parser::{MessageKind, diagnose, parse_log};
use latteset_core::project::FileSystem;
use latteset_core::scheduler::{CompileProgress, CompileRunner};
use latteset_core::types::{CompileKind, CompileOutcome, CompileRequest, Engine, ErrorEntry, ErrorKind};
use tectonic_bridge_core::{CoreBridgeLauncher, MinimalDriver};
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

/// 功能点 **A** 的判据：页逐页相同 **且** 项目根确实已有 PDF **且** 本次是 Quick。
///
/// 闸门与子进程档一致（`latteset-infra/src/runner.rs` 的 `finish_success`）：只对 Quick 跳过
/// 转换与拷贝 —— Full 的语义就是"完整刷新一遍"。`prev` 为 `None`（首轮/口径不符）或页数不同
/// 一律不复用（`prev == Some(hashes)` 同时挡住这两种）。
fn should_reuse_pdf(wants_full: bool, hashes: &[u64], prev: Option<&[u64]>, pdf_exists: bool) -> bool {
    !wants_full && !hashes.is_empty() && prev == Some(hashes) && pdf_exists
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

/// 是否请求 biber（上游 `check_biber_requirement`：biblatex 会写 `<主文件名>.run.xml`）。
fn biber_requested(shared: &SharedCapture, stem: &str) -> bool {
    let name = format!("{stem}.run.xml");
    shared.lock().unwrap_or_else(|e| e.into_inner()).files.contains_key(&name)
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
                "bib 趟失败（BibTeX 处理 `{aux}`）：{e:#}{}",
                if tail.trim().is_empty() { String::new() } else { format!("；`.blg` 尾部：{tail}") }
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
) -> CompileOutcome {
    let xdv_name = format!("{stem}.xdv");
    let pdf_name = format!("{stem}.pdf");
    let log_name = format!("{stem}.log");
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
    // 页哈希缓存落点（A 的判据）：**必须先算** —— `tmp_dir` 紧接着就被 `TectonicIo` 拿走了。
    // 引擎取 `Tectonic`（缓存文件名带引擎名，已知债 #25）；口径定义在 core，与子进程档共用。
    let pages_cache = latteset_core::xdv::pages_cache_path(&tmp_dir, &stem, Engine::Tectonic);
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
    let mut status = ProgressStatus::new(progress, log_ring.clone());
    // 分阶段计时（P2 的复核证据：库形态的钱花在哪一趟）。命中 format 缓存时 format 趟为 0。
    let t_start = Instant::now();
    let mut phase_ms = (0u128, 0u128, 0u128);
    // 是否真的收敛（决定 `Success.kind`）：闭包内赋值，收尾处读。
    let mut converged = false;
    let mut pass_count = 0usize;
    // 页哈希与 A 的判定（闭包内赋值，收尾处用）：见 ⑥ 段。
    let mut page_hashes: Vec<u64> = Vec::new();
    let mut reuse_pdf = false;
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
            // 预算只算排版循环本身：format 趟（冷缓存要生成 `.fmt`）不占编辑触发档的额度。
            let loop_started = Instant::now();
            // 第 1 趟的墙钟（`quick_pass_allowed` 的第二个条件：一趟就慢的文档不再多跑）。
            let mut first_pass = Duration::ZERO;
            loop {
                let before = rerun_snapshot(&shared);
                let what = if passes == 0 { "排版趟" } else { "排版趟（重跑）" };
                run_tex_pass(&mut launcher, &name, what)?;
                passes += 1;
                if passes == 1 {
                    phase_ms.1 = t_start.elapsed().as_millis();
                    first_pass = loop_started.elapsed();
                    // 上游 `driver.rs:1900-1904`：TeX 没产出预期输出文件时要**明说**（多因文档为空），
                    // 否则错误会以"XDV→PDF 失败"的形式出现、指向错误的方向。
                    if capture_len(&shared, &xdv_name) == 0 {
                        return Err(format!(
                            "排版趟没有产出 `{xdv_name}`（空文档或引擎提前停下）；I/O 层输入请求：{}",
                            summarize(&requests.lock().unwrap_or_else(|e| e.into_inner()).clone())
                        ));
                    }
                }

                // 编辑触发档的收敛预算：每趟算一次，bib 闸门与"还能不能再跑一趟"共用同一个判断
                // （Full 不设这道闸门，它的上限就是下面的 `MAX_TEX_PASSES`）。
                let quick_may_continue = quick_pass_allowed(passes, first_pass, loop_started.elapsed());

                // 外部工具类（biber / makeindex / glossaries）：库形态不跑外部工具 ⇒ 登记后
                // **不得声称已收敛**（下面 kind 会退回 Quick）。检出信号取上游口径：
                // biber = `<stem>.run.xml`（`check_biber_requirement`）。
                if biber_requested(&shared, &stem) {
                    unsupported_tool = Some("biber（biblatex）");
                }
                if rerun_snapshot(&shared).keys().any(|k| k.ends_with(".idx")) {
                    unsupported_tool = Some("makeindex（`.idx`）");
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
                    painter.paint(first_pass);
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
            // A：页逐字节未变 **且** 项目根确实已有 PDF（用户可能手删过）⇒ 跳过转换与拷贝。
            // 闸门与子进程档一致：只对 **Quick**（编辑触发）做 —— Full 的语义就是"完整刷新一遍"。
            reuse_pdf = should_reuse_pdf(
                wants_full,
                &page_hashes,
                prev_pages.as_deref(),
                pdf_dst.is_file(),
            );
            if reuse_pdf {
                info!(
                    pages = page_hashes.len(),
                    "页哈希与上次逐页相同：跳过 XDV→PDF 转换与拷贝（复用现有产物）"
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
            // A（复用现有 PDF）时跳过拷贝：产物是上一次那一个，页哈希已证明它与本次排版逐页等价。
            if !reuse_pdf {
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
                if let Err(e) =
                    std::fs::write(&pdf_tmp, pdf).and_then(|()| std::fs::rename(&pdf_tmp, &pdf_dst))
                {
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
            }
            // 页哈希缓存（A 的判据，下一轮用）：失败只记 debug —— 它只是优化判据，
            // 不该让编译失败（最坏结果是下次多转换一次）。
            if let Some(body) = latteset_core::xdv::format_pages_cache(&page_hashes) {
                if let Err(e) = std::fs::write(&pages_cache, body) {
                    debug!(path = %pages_cache.display(), "写页哈希缓存失败（下次会多转换一次）：{e}");
                }
            }
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
        // 预热上一趟的中间产物（`tmp/`）：让重跑判据有"上一趟"可比较，重复编译收敛的文档只需 1 趟。
        seed_previous_intermediates(&shared, &tmp_dir);
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
        let requested_kind = req.kind;
        let project_root = req.project_root.clone();
        let root_file = req.root_file.clone();
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
                root_file,
                pdf_dst,
                requested_kind,
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

    /// biber 的检出信号是 `<stem>.run.xml`（上游 `check_biber_requirement`）。
    #[test]
    fn biber_is_detected_by_run_xml() {
        let with = capture_with(&[("main.run.xml", "<requests/>")]);
        assert!(biber_requested(&with, "main"));
        let without = capture_with(&[("main.aux", "\\bibdata{refs}")]);
        assert!(!biber_requested(&without, "main"));
    }

    /// 声明文案必须点明"可能未解析"，不能只说"少跑一步"（如实收窄，不静默）。
    #[test]
    fn biber_note_states_the_possible_nonresolution() {
        assert!(crate::BIBER_PASS_MISSING_NOTE.contains("可能未解析"), "{}", crate::BIBER_PASS_MISSING_NOTE);
        assert!(crate::BIBER_PASS_MISSING_NOTE.contains("biber"), "{}", crate::BIBER_PASS_MISSING_NOTE);
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

    /// 收敛支持位必须与实现一致：bib 已支持、biber 不支持、收敛**有条件**支持、页哈希已支持。
    #[test]
    fn support_flags_match_the_implementation() {
        assert!(crate::BIB_PASS_SUPPORTED, "bib 趟已实现 ⇒ 该位必须为 true");
        assert!(!crate::BIBER_PASS_SUPPORTED, "biber 要外部二进制 ⇒ 仍不支持");
        assert!(
            crate::CONVERGENCE_SUPPORTED,
            "重跑循环已实现（跑到中间产物稳定）⇒ 该位为 true；编辑触发档在预算内也收敛，\
             只有预算用尽或含 biber/makeindex 的文档才退回 Quick"
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

    /// **A 的判据**（功能点 A）：四个条件缺一不可。
    #[test]
    fn reuse_pdf_requires_quick_unchanged_and_an_existing_pdf() {
        let hashes = vec![1u64, 2, 3];
        assert!(should_reuse_pdf(false, &hashes, Some(&hashes), true), "Quick + 逐页相同 + 有 PDF ⇒ 可复用");
        assert!(!should_reuse_pdf(true, &hashes, Some(&hashes), true), "Full 不做跳过（语义是完整刷新）");
        assert!(!should_reuse_pdf(false, &hashes, None, true), "首轮没有上一次的哈希 ⇒ 不复用");
        assert!(!should_reuse_pdf(false, &hashes, Some(&[9, 9, 9]), true), "页变了 ⇒ 不复用");
        assert!(
            !should_reuse_pdf(false, &hashes, Some(&[1, 2]), true),
            "页数变了 ⇒ 不复用（prev != cur）"
        );
        assert!(!should_reuse_pdf(false, &hashes, Some(&hashes), false), "项目根没有 PDF ⇒ 必须转换");
        assert!(!should_reuse_pdf(false, &[], Some(&[]), true), "空表是'无法判定'，不得当成'零页相同'");
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
}

