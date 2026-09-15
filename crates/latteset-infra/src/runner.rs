//! Latexmk 执行器（modules.md §2.6 / 设计决策 D2；位于基础设施层，ADR-0010）。
//!
//! 超时检测、进程树杀、PDF 拷贝全在这里；调度器无时钟、无进程概念。
//!
//! **流式输出**（roadmap「阶段 2」）：引擎的 stdout/stderr 接管道并**逐行读**——页标记
//! `[N]` 增量汇总成"已排版 N 页"事件，输出文本节流解析成"编译中的错误"事件。
//! 终态仍以 `.log` 为准（`parse_log` 是权威），流式期间报的是**非权威中间态**；
//! 且流式错误**只报致命错误、不报警告**（理由见 [`entries_from_log`]）。

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use latteset_core::log_parser::{
    diagnose, diagnose_timeout, pages_typeset, parse_log, source_release_hint, DiagnosisKind,
    MessageKind, PageMarkerScanner, TimeoutEvidence,
};
use latteset_core::project::{collect_tex_files, FileSystem};
use latteset_core::scheduler::{CompileProgress, CompileRunner};
use latteset_core::types::{CompileKind, CompileOutcome, CompileRequest, Engine, ErrorEntry, ErrorKind};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::tectonic::{self, BundlePolicy};

/// 编译中间目录（design.md：统一收纳 tmp/，与 core 忽略规则一致）。
const OUT_DIR: &str = "tmp";

/// 流式错误解析的节流间隔：既不让解析吃 CPU，也不让错误"迟到"太多。
const LIVE_ERROR_INTERVAL: Duration = Duration::from_millis(500);
/// 尾随 `.log` 的轮询间隔（引擎按页 flush，故这个量级足够跟手）。
const LOG_POLL_INTERVAL: Duration = Duration::from_millis(200);
/// 流式错误解析保留的输出上限（超出后只保留尾部——页标记是单调的，历史文本对进度无用）。
const LIVE_ERROR_BUFFER: usize = 1 << 21; // 2 MiB

/// 编译器实现（`CompileRunner` 的唯一实现）。
///
/// 名字是历史遗留：㉘ 之后它按 [`CompileKind`] 驱动**两条**命令——`Full` 用 latexmk、
/// `Quick` 直调引擎单趟。改名会牵动架构图 SVG 与三处文档，暂留此名。
pub struct LatexmkRunner {
    pub fs: Arc<dyn FileSystem>,
    /// 编译进行中的反馈通道（默认 [`latteset_core::scheduler::NoProgress`]）。
    pub progress: Arc<dyn CompileProgress>,
}

impl LatexmkRunner {
    /// 构造：`progress` 传 `Arc::new(NoProgress)` 即关闭流式反馈（headless/测试）。
    pub fn new(fs: Arc<dyn FileSystem>, progress: Arc<dyn CompileProgress>) -> Self {
        Self { fs, progress }
    }
}

/// 编译进行中的反馈状态机（**唯一**写者：外部把新文本 `feed` 进来）。
///
/// 两个来源都喂它：① 引擎 stdout/stderr 管道；② `tmp/<stem>.log` 的尾随读取。
/// 为什么两个都要（实测）：stdout 在非 TTY 下是 **4KB 块缓冲**——短文档的错误要等到进程结束
/// 才 flush；而 `.log` 是**按页 flush** 的，所以"边编译边报"以日志为主、管道为辅。
/// 同一份内容可能从两个来源各来一次 → 页码单调（重复无害），错误按指纹去重。
struct LiveFeedback {
    progress: Arc<dyn CompileProgress>,
    scanner: PageMarkerScanner,
    buffer: String,
    dirty: bool,
    last_parse: Instant,
    last_emitted: Option<u64>,
}

impl LiveFeedback {
    fn new(progress: Arc<dyn CompileProgress>) -> Self {
        Self {
            progress,
            scanner: PageMarkerScanner::new(),
            buffer: String::new(),
            dirty: false,
            last_parse: Instant::now() - LIVE_ERROR_INTERVAL,
            last_emitted: None,
        }
    }

    /// 把新到的文本喂进来（可以是逐行，也可以是一整段）。
    ///
    /// 解析时机：**见到错误头（行首 `!`）立即解析**（错误要第一时间可见），
    /// 其余情况按 [`LIVE_ERROR_INTERVAL`] 节流补扫（错误可能没有 `!` 头，或上下文行后到）。
    /// 只上报致命错误（`warnings = false`），理由见 [`entries_from_log`]。
    fn feed(&mut self, text: &str, force_parse: bool) {
        if text.is_empty() {
            return;
        }
        if let Some(pages) = self.scanner.push(text) {
            self.progress.pages(pages);
        }
        self.buffer.push_str(text);
        self.dirty = true;
        if self.buffer.len() > LIVE_ERROR_BUFFER {
            let cut = self.buffer.len() - LIVE_ERROR_BUFFER / 2;
            let cut = self
                .buffer
                .char_indices()
                .map(|(i, _)| i)
                .find(|i| *i >= cut)
                .unwrap_or(self.buffer.len());
            self.buffer.drain(..cut);
        }
        let has_error = force_parse || text.lines().any(|l| l.trim_start().starts_with('!'));
        if !has_error && (!self.dirty || self.last_parse.elapsed() < LIVE_ERROR_INTERVAL) {
            return;
        }
        self.last_parse = Instant::now();
        self.dirty = false;
        let entries = entries_from_log(&self.buffer, false);
        if entries.is_empty() {
            return;
        }
        let fp = fingerprint(&entries);
        if self.last_emitted == Some(fp) {
            return;
        }
        self.last_emitted = Some(fp);
        self.progress.errors(&entries);
    }

    /// 捕获输出的尾部（最多 `max_chars` 个字符，按字符边界切，不切坏 UTF-8）。
    ///
    /// 用途：Tectonic 的 bundle/网络失败只出现在 stderr，而 `.log` 里什么都没有——
    /// 收尾时用它判定失败类型（见 [`crate::tectonic::bundle_failure_message`]）。
    fn tail(&self, max_chars: usize) -> String {
        let total = self.buffer.chars().count();
        if total <= max_chars {
            return self.buffer.clone();
        }
        self.buffer.chars().skip(total - max_chars).collect()
    }
}

/// 流式读取一条管道（stdout/stderr）：逐行喂 [`LiveFeedback`]。
async fn pump_output<R>(reader: R, feedback: Arc<tokio::sync::Mutex<LiveFeedback>>)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        feedback.lock().await.feed(&line, false);
        feedback.lock().await.feed("\n", false);
    }
}

/// 尾随 `tmp/<stem>.log`（**按页 flush**，是流式反馈的主力来源）。
///
/// 用 [`FileSystem::read_appended`] 只读新增字节；每 [`LOG_POLL_INTERVAL`] 一次。
/// `stop` 由调用方在编译收尾时置位（终态解析以整份 `.log` 为准）。
async fn tail_log(
    fs: Arc<dyn FileSystem>,
    log_path: std::path::PathBuf,
    feedback: Arc<tokio::sync::Mutex<LiveFeedback>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    use std::sync::atomic::Ordering;
    let mut offset = 0u64;
    while !stop.load(Ordering::Relaxed) {
        tokio::time::sleep(LOG_POLL_INTERVAL).await;
        match fs.read_appended(&log_path, offset).await {
            Ok((text, next)) => {
                offset = next;
                feedback.lock().await.feed(&text, false);
            }
            // 日志还没出现（或这一趟还没建）→ 下一轮再试
            Err(_) => continue,
        }
    }
    // 收尾补一次：把最后一段（可能不足一个轮询周期）喂进去
    if let Ok((text, _)) = fs.read_appended(&log_path, offset).await {
        feedback.lock().await.feed(&text, true);
    }
}

/// 文本 → `ErrorEntry`（与终态路径**同一套**解析与诊断，只是不做 `.ins`/`.dtx` 的项目内查证）。
///
/// `warnings = false`（流式通道用）时**丢掉警告**，只留致命错误。实测依据：一次失败的
/// ctexbook 长文档（2026-09）日志里 30 条消息中有 **27 条是 `Overfull \hbox`**——它们不阻止
/// 编译完成，却足以把"写不出 `c01.aux`""Emergency stop"这类真正致命的条目挤出可视区。
/// 流式通道的目标只是"尽早看见会终结本次编译的错误"，完整清单（含警告）由终态给出。
fn entries_from_log(text: &str, warnings: bool) -> Vec<ErrorEntry> {
    parse_log(text)
        .into_iter()
        .filter(|m| warnings || m.kind == MessageKind::Error)
        .map(|m| {
            let diagnosis = diagnose(&m);
            ErrorEntry {
                message: m.message,
                file: m.file,
                line: m.line,
                kind: ErrorKind::ContentError,
                diagnosis,
            }
        })
        .collect()
}

/// 条目集合指纹（用于"内容没变就不发"）。
fn fingerprint(entries: &[ErrorEntry]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for e in entries {
        e.message.hash(&mut h);
        e.file.hash(&mut h);
        e.line.hash(&mut h);
    }
    entries.len().hash(&mut h);
    h.finish()
}

/// 页哈希缓存路径（与 XDV 同目录：`tmp/<stem>.<引擎>.pages`）。
///
/// 为什么放**磁盘**而不是 runner 内存（功能点 A 的设计取舍）：runner 保持无状态（`&self` 不可变、
/// 一次调用完全独立，见 modules.md §2.6）；而且这份基线能被 Quick 与 Full 两条路径共享——
/// 若各自记在内存里，两者交替时会把对方的产物当成"变了"，白白多转换一次。
///
/// 为什么**带引擎名**（2026-09，已知债 #25）：页哈希的口径与引擎绑定。同一个 `main.tex` 换引擎后，
/// 页内容必然不同；若共用一份缓存，跨引擎的"逐页相同"判断就是拿两套口径比大小。今天只有 XeLaTeX
/// 产哈希（`Engine::writes_xdv`），但按引擎分文件后，将来给 LuaLaTeX 接**引擎内逐页指纹**
/// 时两套哈希不会互相污染。
/// 页哈希缓存路径。
///
/// **口径在 core**（[`latteset_core::xdv::pages_cache_path`]）：库形态档的 runner 在
/// `latteset-tectonic`，按 ADR-0012 **不依赖 infra**，所以"带引擎名 + 首行口径标记"这套约定
/// 必须有一处共同的定义（2026-09 提到 core）——两处各写一份就会漂移，而漂移的代价是
/// "永远首轮"（A 面每轮白跑 0.65–0.94 s）。
fn pages_cache_path(tmp_dir: &Path, stem: &str, engine: Engine) -> PathBuf {
    latteset_core::xdv::pages_cache_path(tmp_dir, stem, engine)
}

fn root_stem(root_file: &Path) -> String {
    root_file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".to_string())
}

/// latexmk 输入参数：相对项目根的完整路径（不能用 stem——嵌套根文件如 `css/thesis.tex`
/// 只取 stem 会跑成 `latexmk thesis.tex`，在项目根下不存在）。统一用正斜杠，避免 Windows
/// 反斜杠在 latexmk 内被当作转义。latexmk jobname 取输入文件名 basename，故产物仍是
/// `tmp/<stem>.pdf`，与 `pdf_dst`/`pdf_src` 的 stem 计算保持一致。
fn latexmk_input(root_file: &Path, project_root: &Path) -> String {
    root_file
        .strip_prefix(project_root)
        .unwrap_or(root_file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 编译命令构造（modules.md §2.6 算法）：cwd = 项目根，相对 input/include 才能解析。
///
/// 抽成独立函数是为了**可单测**（参数与环境变量都能直接断言，不必真的起进程）。
///
/// `bundle` 是 Tectonic 的 bundle 获取策略（P0/t14，见 [`crate::tectonic`]）；非 Tectonic 引擎
/// 不看它。生产路径由 `compile()` 探测后传入，测试按需显式传两档。
fn compile_command(req: &CompileRequest, kind: CompileKind, bundle: BundlePolicy) -> tokio::process::Command {
    let mut cmd = match kind {
        // Quick：直调引擎一趟（实测比完整 latexmk 快 40% 中位，见 design.md §延迟预算实测附节）。
        // 与 latexmk 的差异只在外层机制与收敛趟；产物路径与 Full 一致（都在 tmp/<stem>.pdf）。
        //
        // `-no-pdf`（2026-09，功能点 A）：直调引擎**只产 XDV**，PDF 改为由我们在收尾时按需调
        // `xdvipdfmx` 转换——这样"页哈希与上次逐页相同"时就能整个跳过转换（省 0.65–0.94s/次）。
        // latexmk 内部走的也是这条序列（`xelatex -no-pdf` → `xdvipdfmx -E -o …`，见 design.md）。
        CompileKind::Quick => {
            if req.engine == latteset_core::types::Engine::Tectonic {
                tectonic_command(req, true, bundle)
            } else {
                let mut c = tokio::process::Command::new(req.engine.binary_name());
                // `-no-pdf`（2026-09，功能点 A）**只对 XeTeX 成立**：直调引擎只产 XDV，PDF 改由收尾时
                // 按需调 `xdvipdfmx` 转换（"页哈希逐页相同"就整个跳过转换，省 0.65–1.4s/次，见 §已知债 #25）。
                // 换别的引擎加这个参数只会坏事——实测（2026-09）：pdflatex 报 `unrecognized option
                // '-no-pdf'`（Quick 直接跑不起来），lualatex 静默忽略它（照样写 PDF，白等一趟）。
                // 非 XeTeX 引擎的 Quick 就是"引擎单趟直写 PDF"，收尾不做转换（见 finish_success）。
                if req.engine.writes_xdv() {
                    c.arg("-no-pdf");
                }
                c.arg("-interaction=nonstopmode")
                    .arg("-synctex=1")
                    .arg(format!("-output-directory={OUT_DIR}"))
                    .arg(latexmk_input(&req.root_file, &req.project_root));
                c
            }
        }
        CompileKind::Full => {
            if req.engine == latteset_core::types::Engine::Tectonic {
                tectonic_command(req, false, bundle)
            } else {
                let mut c = tokio::process::Command::new("latexmk");
                c.arg(req.engine.latexmk_flag())
                    .arg(format!("-outdir={OUT_DIR}"))
                    .arg("-synctex=1")
                    .arg("-interaction=nonstopmode")
                    .arg(latexmk_input(&req.root_file, &req.project_root));
                c
            }
        }
    };
    // 构建确定性（roadmap ㉚，2026-09 实测）：不设这个变量时，同一份源码连跑两次产出的 PDF
    // **不是**逐字节相同的——差异只在 trailer 的 `/ID`（64 字节），而 XeTeX 用它做**内容无关**的
    // 随机/时间种子。设成固定值后实测两份 PDF SHA-256 完全一致（见 scripts/check-determinism.mjs）。
    // 这是"输出 diff / 只重排变化页"这类优化的前提：字节不同就分不清"真变了"还是"ID 抖了"。
    // 副作用实测：**XeTeX 的 `\today` 不受它影响**（仍打印构建当天日期），PDF 里也不会因此多出
    // `/CreationDate`——所以固定成 0（reproducible-builds 惯例）只影响 ID，不污染文档内容。
    //
    // **按引擎分档（2026-09-14 裁决 D4，别改回无条件设置）**：Tectonic 把这个变量当**引擎时间源**
    // 喂给 `\today` —— 设 0 时**正文**会印成 `1970 年 1 月 1 日`（实测；而 XeTeX 只动 PDF `/ID`、
    // 正文零变化）。所以 **Tectonic 进程不设**它；不设时 Tectonic 的 XDV 仍逐字节稳定（264,736 B），
    // 页哈希判据不受影响。依据与代价见 docs/research/tectonic-test-plan.md §5.4 D4 与
    // docs/research/page-hash-prev-quant.md。
    if req.engine != latteset_core::types::Engine::Tectonic {
        cmd.env("SOURCE_DATE_EPOCH", EPOCH_FOR_REPRODUCIBLE_BUILD);
    }
    cmd
}

/// Tectonic 命令（路线①；`docs/research/tectonic-test-plan.md` §1.2 / INT-20）。
///
/// 参数按测试方案定为**必带**（`-C` 例外，见下）：
/// - `-C`（仅当 `bundle == CachedOnly`）：只用缓存资源（离线，不联网下载）。**不能无条件加**——
///   全新机器 + 空缓存时它会让首编必失败（`docs/tectonic-library-plan.md` §6 P0 / E-1）。
///   判定依据与证据见 [`crate::tectonic`]；
/// - `-k`：保留中间产物（`.aux`/`.toc`/`.bbl`）—— Quick 的前置条件（`tmp/<stem>.aux`）与 bib 判据都要它；
/// - `--keep-logs`：保留 `.log`（Tectonic 的日志本来只在内存、收尾才落盘）；
/// - `--synctex`：产出 `.synctex.gz`（我们的正反向仍走外部 `synctex.exe`，需要本机 TeX Live）；
/// - `-p`：页进度走 **stdout 的 `[N]`** —— 这是 Tectonic 下唯一的实时页通道；
/// - `-r 1`（仅 Quick）：**两趟**（`-r N` = 再跑 N 趟）。⚠ **单趟（`-r 0`）会丢整段目录**：
///   Tectonic 的 `-o tmp` **只是输出目录、不作为输入目录**，所以它**读不回**上一趟留在 `tmp/` 的
///   `.toc`/`.aux`（对照：latexmk 靠 `-output-directory` 读得回，库形态档靠
///   `seed_previous_intermediates` 预热），单趟档每趟都从零重建目录。
///   实测（release `tectonic.exe` 0.17.0、同夹具同命令形态、`-k` 保留中间产物）：
///   `-r 0` = **26 页 / 12,624 字**（第 2 页只剩页码标记，正文从第 3 页起 = 没有目录），
///   `-r 1` = **28 页 / 13,570 字**（目录占第 2–3 页，与收敛档逐项一致）。
///   Quick 的语义是"目录/交叉引用**页码**落后一趟"（roadmap ㉘）；单趟档丢的是整段目录，不是页码。
///   两趟 1057 ms（单趟 657、收敛 1495），Quick 仍明显快于 Full；
/// - `-o tmp`：产物目录与其它引擎一致（`tmp/<stem>.pdf`）。
fn tectonic_command(req: &CompileRequest, quick: bool, bundle: BundlePolicy) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(req.engine.binary_name());
    if bundle == BundlePolicy::CachedOnly {
        c.arg("-C");
    }
    c.arg("-k")
        .arg("--keep-logs")
        .arg("--synctex")
        .arg("-p")
        .arg("-o")
        .arg(OUT_DIR);
    if quick {
        c.arg("-r").arg("1");
    }
    c.arg(latexmk_input(&req.root_file, &req.project_root));
    c
}

/// 固定时间戳（0 = 1970-01-01，reproducible-builds 惯例）：只用来让引擎的 `/ID` 不随时钟抖动。
const EPOCH_FOR_REPRODUCIBLE_BUILD: &str = "0";

#[async_trait]
impl CompileRunner for LatexmkRunner {
    async fn compile(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome {
        let stem = root_stem(&req.root_file);
        let tmp_dir = req.project_root.join(OUT_DIR);
        let pdf_dst = req.project_root.join(format!("{stem}.pdf"));

        // 输出目录由**产品自建**（tectonic-test-plan P-G4，2026-09 实测）：
        // latexmk / `xelatex -output-directory` 会自己建 `tmp/`，但 **Tectonic 不会**——
        // 目录不存在时它直接 `error: output directory "tmp" does not exist`（exit 1），
        // 而且**连 .log 都不落盘** ⇒ 用户看到的是"编译失败且无法读取日志"这种没有信息量的报错。
        // 三个引擎都走这一步（幂等；真建不出来时后面的编译自会以更具体的方式失败）。
        if let Err(e) = tokio::fs::create_dir_all(&tmp_dir).await {
            debug!(dir = %tmp_dir.display(), "创建输出目录失败（继续尝试编译）：{e}");
        }

        // 强度决策（roadmap ㉘）：编辑触发的 Quick 走「直调引擎单趟」；其余走完整 latexmk。
        // 但 **Quick 需要已有构建产物**才有意义（单趟依赖上一趟的 .aux/.toc）；首次编译无 aux 时
        // 单趟会产出「引用全是 ??」的 PDF——故此处自动升级为 Full。
        // 探测失败（权限/竞态）按「无产物」保守处理：升级为 Full 只是更慢，不会出错。
        let had_aux = self.fs.exists(&tmp_dir.join(format!("{stem}.aux"))).await.unwrap_or(false);
        let mut kind = req.kind;
        if kind == CompileKind::Quick && !had_aux {
            debug!(stem, "无构建产物，Quick 升级为 Full（首编）");
            kind = CompileKind::Full;
        }

        // Tectonic 的 bundle 获取策略（P0/t14）：**只有拿到缓存正向证据才加 `-C`**——冷缓存加它
        // 等于"拒绝联网取 bundle"，首编必失败。判定依据与证据见 crate::tectonic。
        let bundle = if req.engine == latteset_core::types::Engine::Tectonic {
            let probe = tectonic::probe();
            debug!(
                policy = ?probe.policy,
                cache_ready = probe.cache_ready,
                cache_dir = ?probe.cache_dir,
                forced = ?probe.forced,
                "Tectonic bundle 策略"
            );
            probe.policy
        } else {
            BundlePolicy::CachedOnly
        };

        // 命令构造（modules.md §2.6 算法）：cwd = 项目根，相对 input/include 才能解析
        let mut cmd = compile_command(&req, kind, bundle);
        match kind {
            CompileKind::Quick => debug!(engine = req.engine.binary_name(), "Quick 编译（单趟直调引擎）"),
            CompileKind::Full if req.engine == latteset_core::types::Engine::Tectonic => {
                debug!(engine = "tectonic", "Full 编译（Tectonic 自带收敛，不经 latexmk）")
            }
            CompileKind::Full => debug!(engine = req.engine.latexmk_flag(), "Full 编译（完整 latexmk 收敛）"),
        }
        cmd.current_dir(&req.project_root)
            // 流式输出（roadmap「阶段 2」）：接管道逐行读——页进度与"编译中的错误"都从这里来。
            // 此前是 Stdio::null()：引擎输出被整个丢弃，编译期既没有进度也没有实时错误。
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // 进程孤儿防护：若编译 future 被丢弃（应用退出/任务取消），随 future 一并杀掉子进程。
            // 避免子进程残留、持续写 tmp/ 或占用 PDF 锁。（树杀仍由 kill_tree 负责，这里是兜底。）
            .kill_on_drop(true);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return CompileOutcome::IoError {
                    message: match (kind, req.engine) {
                        // INT-16：Tectonic 未装时的文案必须**指对方向**（不是"TeX Live 未安装"——
                        // 它自带 bundle、本来就不依赖 TL）；它也不会命中 bundle/缓存失败那条路径
                        // （那是 spawn 成功之后的失败，见 compile() 收尾的 tectonic 文案）。
                        (_, latteset_core::types::Engine::Tectonic) => format!(
                            "无法启动 tectonic（Tectonic 需单独安装、且要在 PATH 里）：{e}"
                        ),
                        (CompileKind::Quick, _) =>
                            format!("无法启动 {}（TeX Live 未安装？）：{e}", req.engine.binary_name()),
                        (CompileKind::Full, _) => format!("无法启动 latexmk（TeX Live 未安装？）：{e}"),
                    },
                }
            }
        };
        let pid = child.id();

        // 读任务：stdout/stderr 管道 + `tmp/<stem>.log` 尾随，三路都喂同一个反馈状态机。
        // 它们随管道关闭 / stop 置位自然结束；下面在拿到 outcome 后会短暂 join 一下，避免任务泄漏。
        let log_path = tmp_dir.join(format!("{stem}.log"));
        let feedback = Arc::new(tokio::sync::Mutex::new(LiveFeedback::new(self.progress.clone())));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut pumps = vec![tokio::spawn(tail_log(
            self.fs.clone(),
            log_path,
            feedback.clone(),
            stop.clone(),
        ))];
        if let Some(out) = child.stdout.take() {
            pumps.push(tokio::spawn(pump_output(out, feedback.clone())));
        }
        if let Some(err) = child.stderr.take() {
            pumps.push(tokio::spawn(pump_output(err, feedback.clone())));
        }

        let outcome = tokio::select! {
            _ = tokio::time::sleep(req.timeout) => {
                if let Some(pid) = pid {
                    warn!(pid, "编译超时（{}s），树杀进程", req.timeout.as_secs());
                    kill_tree(pid);
                }
                // roadmap ㉕：超时不静默重试，改为**现场采集证据**给出可操作诊断
                let entry = self.timeout_entry(&req, &stem, &tmp_dir, had_aux).await;
                CompileOutcome::Timeout { entry }
            }
            _ = cancel.cancelled() => {
                if let Some(pid) = pid {
                    warn!(pid, "收到手动终止，树杀进程");
                    kill_tree(pid);
                }
                CompileOutcome::Aborted
            }
            status = child.wait() => match status {
                Ok(s) if s.success() => {
                    // 成功收尾抽成方法：里面可能"跳过转换"提前返回，而 select 分支里**不能直接 return**
                    // （会跳过下面的 stop/join 收尾，泄漏读任务）。
                    match self.finish_success(&req, &tmp_dir, &stem, kind, &pdf_dst).await {
                        Ok(outcome) => outcome,
                        Err(message) => CompileOutcome::IoError { message },
                    }
                }
                Ok(_) => {
                    // 非零退出：.log 是权威（modules.md §4：不做流式输出）
                    // 用**容错解码**读取：GBK 源文件经 pdflatex 会把非法 UTF-8 字节写进日志，
                    // 严格读取会导致「编译失败却拿不到任何错误信息」（2026-09 实测，roadmap P0-①）。
                    let log_path = tmp_dir.join(format!("{stem}.log"));
                    match self.fs.read_to_string_lossy(&log_path).await {
                        Ok(text) => {
                            // 终态清单**含警告**（Overfull/Underfull/LaTeX Warning）——既有行为不变。
                            let errors = entries_from_log(&text, true);
                            // roadmap ㉗：缺 .cls 且项目里确有 .ins/.dtx 时，把泛化建议换成具体命令
                            let errors = self.enrich_source_release_hints(errors, &req.project_root).await;
                            let diagnosed = errors.iter().filter(|e| e.diagnosis.is_some()).count();
                            debug!(
                                count = errors.len(),
                                diagnosed,
                                log = %log_path.display(),
                                "编译失败：已从 .log 解析出错误条目"
                            );
                            CompileOutcome::ContentError { errors }
                        }
                        Err(e) => {
                            warn!(log = %log_path.display(), "编译失败且无法读取日志：{e}");
                            CompileOutcome::IoError {
                                message: format!("编译失败且无法读取日志（{}）：{e}", log_path.display()),
                            }
                        }
                    }
                }
                Err(e) => CompileOutcome::IoError {
                    message: format!("等待 latexmk 失败：{e}"),
                },
            },
        };
        // 收尾：置 stop 让尾随任务做最后一次补读，然后 join（进程已退出/被杀 → 管道关闭）。
        // 给一个短上限是为了极端情况（树杀没杀干净、孙子进程仍持有写端）不把编译 future 拖住。
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for p in pumps {
            let _ = tokio::time::timeout(Duration::from_millis(600), p).await;
        }

        // Tectonic 的 bundle/网络失败**只写 stderr**（连 `tmp/<stem>.log` 都不落），上面那条
        // ".log 是权威"的路径会报成「编译失败且无法读取日志」这种没有操作信息的兜底文案。
        // 收尾后拿捕获的输出尾部换成可执行文案（标记与文案见 crate::tectonic）。
        if req.engine == latteset_core::types::Engine::Tectonic
            && matches!(outcome, CompileOutcome::IoError { .. } | CompileOutcome::ContentError { .. })
        {
            let tail = feedback.lock().await.tail(4 << 10);
            if let Some(message) = tectonic::bundle_failure_message(bundle, &tail) {
                warn!(policy = ?bundle, "Tectonic 取 bundle 失败：已换成可执行文案");
                return CompileOutcome::IoError { message };
            }
        }
        outcome
    }
}

impl LatexmkRunner {
    /// 读 `tmp/<stem>.xdv` 算**页哈希**（顺序 = 页号）。
    ///
    /// **只对产 XDV 的引擎有意义**（[`Engine::writes_xdv`]，已知债 #25）——调用方必须先过那道闸，
    /// 否则会把上一次 XeLaTeX 留下的陈旧 XDV 当成本次产物。
    ///
    /// 为什么需要它（docs/research/incremental-edit-x-dvi.md）：XDV 的页是自包含的字节区间，
    /// 页哈希相同 ⇔ 该页的排版结果逐字节未变。于是"这次编译到底改了什么"可以被精确判定，
    /// 下游据此跳过无谓工作——B：不发 `pdf-updated`；C：只重绘变化页；A：跳过 `xdvipdfmx` 转换。
    ///
    /// 契约：**不做错误上报**（它是优化判据，读不到只是"这次退化为全量刷新"）；返回空表
    /// 表示**无法判定**，调用方不要把它当成"零页变化"。
    async fn page_hashes(&self, tmp_dir: &Path, stem: &str) -> Vec<u64> {
        let path = tmp_dir.join(format!("{stem}.xdv"));
        match self.fs.read_bytes(&path).await {
            Ok(bytes) => latteset_core::xdv::page_hashes(&bytes),
            Err(e) => {
                // Quick 与 Full 都会产出 XDV；走到这里通常是"引擎没跑完就失败"或工作目录被清空
                debug!(path = %path.display(), "读 XDV 失败，页级差异不可判定：{e}");
                Vec::new()
            }
        }
    }

    /// 成功收尾：页哈希 →（必要时）`xdvipdfmx` 转换 → 拷贝到项目根 → 缓存页哈希。
    ///
    /// 功能点 A（docs/research/incremental-edit-x-dvi.md §2.2）就在这里落地：Quick 路径改用
    /// `-no-pdf` 后不再自己转 PDF，于是"本次 XDV 页哈希与上次**逐页相同**"时可以：
    /// **跳过转换与拷贝，直接复用项目根已有的 PDF**（省 0.65–1.4s/次）。
    /// Full（latexmk）自己会转换，不受影响。
    ///
    /// **引擎闸门**（已知债 #25）：页哈希与 `xdvipdfmx` 转换**只对 [`Engine::writes_xdv`] 为真的
    /// 引擎成立**。非 XeTeX 引擎走"引擎自己写 PDF → 直接拷贝"这一条最朴素的路——不转换、不算哈希
    /// （`page_hashes` 为空 ⇒ 下游按"无法判定"保守全量刷新）。
    ///
    /// 返回 `Err(message)` → 调用方包装成 `IoError`。
    async fn finish_success(
        &self,
        req: &CompileRequest,
        tmp_dir: &Path,
        stem: &str,
        kind: CompileKind,
        pdf_dst: &Path,
    ) -> Result<CompileOutcome, String> {
        // 页级差异的判据（B/C/A 三个功能点共用）：读本次产出的 XDV 算页哈希。
        // 读不到 → 空表 = "无法判定"，一切优化路径都不走（保守）。
        //
        // **只对产 XDV 的引擎做**（已知债 #25）：非 XeTeX 引擎不写 `.xdv`，而 `tmp/` 里可能**留着**
        // 上一次 XeLaTeX 的陈旧 XDV——若不设这道闸，收尾会把陈旧 XDV 当成本次产物：轻则多转换一次，
        // 重则**用陈旧 XDV 的转换结果覆盖掉引擎刚写好的 PDF**（实测：LuaLaTeX 自己出 109,732 B，
        // 被覆盖成 70,193 B 的 XeLaTeX 产物，还报"成功"）。
        let page_hashes = if req.engine.writes_xdv() {
            self.page_hashes(tmp_dir, stem).await
        } else {
            Vec::new()
        };
        let cache_path = pages_cache_path(tmp_dir, stem, req.engine);
        let prev = self.read_pages_cache(&cache_path).await;
        let unchanged = !page_hashes.is_empty() && prev.as_deref() == Some(page_hashes.as_slice());
        let pdf_src = tmp_dir.join(format!("{stem}.pdf"));

        if kind == CompileKind::Quick && req.engine.writes_xdv() {
            // 复用条件：内容逐页未变 **且** 项目根确实已经有上一次的 PDF（用户可能手删过）
            let reusable = unchanged && tokio::fs::try_exists(pdf_dst).await.unwrap_or(false);
            if reusable {
                debug!(
                    pages = page_hashes.len(),
                    "页哈希与上次逐页相同：跳过 xdvipdfmx 转换与 PDF 拷贝（复用现有产物）"
                );
                self.write_pages_cache(&cache_path, &page_hashes).await;
                return Ok(CompileOutcome::Success {
                    pdf_path: pdf_dst.to_path_buf(),
                    kind,
                    page_hashes,
                });
            }
            self.convert_xdv(tmp_dir, stem, &req.project_root).await?;
        }

        // tmp/<stem>.pdf → 项目根（design.md 产物位置）
        // 原子拷贝：先写临时文件再 rename，失败时旧 PDF 保留（不被截断/损坏）。
        let pdf_tmp = pdf_dst.with_extension("pdf.tmp");
        let copy = async {
            tokio::fs::copy(&pdf_src, &pdf_tmp).await?;
            tokio::fs::rename(&pdf_tmp, pdf_dst).await
        }
        .await;
        if let Err(e) = copy {
            let _ = tokio::fs::remove_file(&pdf_tmp).await;
            return Err(format!(
                "PDF 拷贝失败（{} → {}）：{e}",
                pdf_src.display(),
                pdf_dst.display()
            ));
        }
        self.write_pages_cache(&cache_path, &page_hashes).await;
        Ok(CompileOutcome::Success {
            pdf_path: pdf_dst.to_path_buf(),
            kind,
            page_hashes,
        })
    }

    /// 调 `xdvipdfmx` 把 `tmp/<stem>.xdv` 转成 `tmp/<stem>.pdf`（Quick 路径专用）。
    ///
    /// 为什么需要它：Quick 用了 `-no-pdf` 就没人转 PDF 了——而"按需转换"正是跳过转换的前提。
    /// Full 路径由 latexmk 内部完成同一件事（`xdvipdfmx -E -o …`）。
    async fn convert_xdv(&self, tmp_dir: &Path, stem: &str, project_root: &Path) -> Result<(), String> {
        let xdv = tmp_dir.join(format!("{stem}.xdv"));
        let pdf = tmp_dir.join(format!("{stem}.pdf"));
        let out = tokio::process::Command::new("xdvipdfmx")
            .arg("-q")
            .arg("-o")
            .arg(&pdf)
            .arg(&xdv)
            .current_dir(project_root)
            // 与 latexmk 那条路径保持一致（㉚ 构建确定性）
            .env("SOURCE_DATE_EPOCH", EPOCH_FOR_REPRODUCIBLE_BUILD)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("无法启动 xdvipdfmx（TeX Live 未安装？）：{e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let tail: Vec<&str> = err.lines().filter(|l| !l.trim().is_empty()).take(5).collect();
            return Err(format!(
                "xdvipdfmx 转换失败（exit={:?}）：{}",
                out.status.code(),
                if tail.is_empty() { "（无 stderr 输出）".to_string() } else { tail.join(" / ") }
            ));
        }
        Ok(())
    }

    /// 读上一次的页哈希缓存（`tmp/<stem>.<引擎>.pages`）。
    ///
    /// 解析口径在 core（[`latteset_core::xdv::parse_pages_cache`]）：**首行必须是口径标记**；
    /// 老格式（首行直接是 16 进制哈希）与任何截断/损坏都当 `None` = **无法判定** →
    /// 不做任何"跳过"优化（保守，只多转换一轮）。
    async fn read_pages_cache(&self, path: &Path) -> Option<Vec<u64>> {
        let text = self.fs.read_to_string(path).await.ok()?;
        latteset_core::xdv::parse_pages_cache(&text)
    }

    /// 写页哈希缓存（失败只记 debug：它是优化判据，不该让编译失败——最坏结果是下次多转换一次）。
    ///
    /// 文本由 core 生成（[`latteset_core::xdv::format_pages_cache`]）：首行口径标记 `v1`，
    /// 其后每页一行 `{h:016x}`；空表**不写**（免得把"无法判定"当成"零页"带给下一轮）。
    async fn write_pages_cache(&self, path: &Path, hashes: &[u64]) {
        let Some(body) = latteset_core::xdv::format_pages_cache(hashes) else {
            return;
        };
        if let Err(e) = self.fs.write(path, &body).await {
            debug!(path = %path.display(), "写页哈希缓存失败（下次会多转换一次）：{e}");
        }
    }

    /// 源码版模板提示（roadmap ㉗）：编译报"缺 `.cls`"时，看看项目里是不是只有 `.ins`/`.dtx`。
    ///
    /// 判据是**看得见的事实**：项目根目录里存在哪些 `.ins`/`.dtx` 文件（源码版模板的发布形态）。
    /// 具体的"该编哪个文件"由 core 的 [`source_release_hint`] 决定（同名优先、单一候选次之、
    /// 拿不准就不改建议）。
    async fn enrich_source_release_hints(
        &self,
        mut errors: Vec<ErrorEntry>,
        project_root: &Path,
    ) -> Vec<ErrorEntry> {
        let needs_lookup = errors.iter().any(|e| {
            e.diagnosis
                .as_ref()
                .is_some_and(|d| d.kind == DiagnosisKind::MissingClass && d.missing_file.is_some())
        });
        if !needs_lookup {
            return errors;
        }
        // 只扫项目根（源码版模板的 .ins 就在根目录；扫全树代价大且没必要）
        let candidates: Vec<String> = match self.fs.read_dir(project_root).await {
            Ok(entries) => entries
                .into_iter()
                .filter(|e| !e.is_dir)
                .filter_map(|e| e.path.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect(),
            Err(e) => {
                debug!(root = %project_root.display(), "读项目根失败，跳过源码版模板识别：{e}");
                return errors;
            }
        };
        for entry in &mut errors {
            let Some(diag) = entry.diagnosis.as_mut() else { continue };
            if diag.kind != DiagnosisKind::MissingClass {
                continue;
            }
            let Some(missing) = diag.missing_file.clone() else { continue };
            if let Some(hint) = source_release_hint(&missing, &candidates) {
                debug!(missing = %missing, "识别为源码版模板，替换为具体编译命令");
                diag.hint = Some(hint);
            }
        }
        errors
    }

    /// 超时错误条目（roadmap ㉕）：采集**可观察证据** → 证据化诊断 → [`ErrorEntry`]。
    ///
    /// 证据三条：① 本次是否首编（开始时有无 `.aux`）；② 项目 `.tex` 源文件数；
    /// ③ `.log` 里已排版到第几页（推进证据，见 [`pages_typeset`]）。
    /// 三项都是"看得见的事实"，诊断文案不据此猜测因果（宁可说"可能"，也不断言"卡住"）。
    /// 日志另交给 [`diagnose_timeout`]：里面若已有致命错误，超时就只是症状（提高超时救不了）。
    async fn timeout_entry(
        &self,
        req: &CompileRequest,
        stem: &str,
        tmp_dir: &Path,
        had_aux: bool,
    ) -> ErrorEntry {
        let log_path = tmp_dir.join(format!("{stem}.log"));
        // 树杀刚发出，日志可能只落盘了一部分；读得到多少算多少（best-effort）
        let log = self.fs.read_to_string_lossy(&log_path).await.unwrap_or_default();
        let pages = pages_typeset(&log);
        let tex_files = collect_tex_files(self.fs.as_ref(), &req.project_root)
            .await
            .ok()
            .map(|v| v.len());
        let timeout_secs = u32::try_from(req.timeout.as_secs()).unwrap_or(u32::MAX);

        let evidence = TimeoutEvidence {
            timeout_secs,
            first_build: !had_aux,
            tex_files,
        };
        // 诊断看两样东西：本次运行的日志（里面可能已有致命错误 → 超时只是症状）与上面的证据
        let diagnosis = diagnose_timeout(&log, &evidence);
        debug!(
            timeout_secs,
            first_build = evidence.first_build,
            tex_files = ?tex_files,
            pages = ?pages,
            kind = ?diagnosis.kind,
            "编译超时，已采集诊断证据"
        );

        // message 保留"机器看到的事实"，诊断给"人话原因 + 怎么改"（前端首行显示诊断原因，
        // 这条 message 落到 title 里，供排查时对照）
        let mut facts = vec![if had_aux { "增量编译".to_string() } else { "首次编译".to_string() }];
        if let Some(n) = tex_files {
            facts.push(format!("{n} 个 .tex 源文件"));
        }
        if let Some(p) = pages {
            facts.push(format!("日志已排版到第 {p} 页"));
        } else {
            facts.push("日志无页输出".to_string());
        }
        ErrorEntry {
            message: format!(
                "编译超过 {timeout_secs}s 上限，已强制终止（进程树已杀）。证据：{}",
                facts.join("，")
            ),
            file: None,
            line: None,
            kind: ErrorKind::Timeout,
            diagnosis: Some(diagnosis),
        }
    }
}

/// 进程树杀（modules.md §2.6）：
/// Windows：`taskkill /T /F /PID`（Child::kill 只杀直接子进程，latexmk 的子进程会存活）。
/// Unix：先试进程组（-pid），失败再试直接 kill（best-effort，与 LaTeX Workshop 相同的已知局限）。
fn kill_tree(pid: u32) {
    // 阻塞的进程/进程组调用移到阻塞线程池，避免卡住 async 执行器（超时/取消路径的热路径）。
    // 结果 best-effort，丢弃 JoinHandle（仅作后台树杀，不等待）。
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/T", "/F", "/PID", &pid.to_string()])
                .status();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let group = std::process::Command::new("kill")
                .args(["-9", &format!("-{pid}")])
                .status();
            if group.map(|s| !s.success()).unwrap_or(true) {
                let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
            }
        }
    });
}
#[cfg(test)]
mod tests {
    //! 真实环境集成测试（`#[ignore]`：需要系统安装 latexmk + synctex）。
    //!
    //! 在有 TeX Live/MiKTeX 的机器上运行：`cargo test -p latteset-infra -- --ignored`
    //! 这是 Windows 验收清单的自动化抓手（modules.md §8），验证：
    //! - latexmk 真实编译全链路（命令构造/产物位置/PDF 拷贝）；
    //! - 内容错误 → .log 解析链路；
    //! - 超时树杀 / 取消终止；
    //! - SyncTeX CLI 输出契约（ADR-0008 最大风险点）。

    use super::*;
    use crate::synctex::SyncTexCli;
    use std::path::PathBuf;
    use std::time::Duration;
    use latteset_core::synctex::{SourcePosition, SyncTexProvider, SyncTexPosition};

    struct TempProject {
        dir: std::path::PathBuf,
    }

    // ---- 命令构造（不需要 latexmk，任何机器都能跑）----

    fn sample_req() -> CompileRequest {
        CompileRequest {
            root_file: PathBuf::from("/proj/css/thesis.tex"),
            project_root: PathBuf::from("/proj"),
            engine: latteset_core::types::Engine::XeLaTeX,
            timeout: Duration::from_secs(120),
            kind: CompileKind::Quick,
        }
    }

    fn argv(cmd: &tokio::process::Command) -> Vec<String> {
        cmd.as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// 非 Tectonic 引擎不看 bundle 档（该参数只喂给 `tectonic_command`），这里固定传离线档，
    /// 让既有 argv 断言保持原口径。Tectonic 的用例直接调 [`compile_command`] 并显式给两档。
    fn engine_command(req: &CompileRequest, kind: CompileKind) -> tokio::process::Command {
        compile_command(req, kind, BundlePolicy::CachedOnly)
    }

    #[test]
    fn quick_command_calls_engine_directly() {
        let cmd = engine_command(&sample_req(), CompileKind::Quick);
        assert_eq!(cmd.as_std().get_program().to_string_lossy(), "xelatex");
        assert_eq!(
            argv(&cmd),
            vec![
                // -no-pdf（功能点 A）：直调引擎只产 XDV，PDF 改由 finish_success 按需调 xdvipdfmx 转换
                "-no-pdf",
                "-interaction=nonstopmode",
                "-synctex=1",
                "-output-directory=tmp",
                // 嵌套根文件用**相对项目根的完整路径**（只取 stem 会在项目根下找不到文件）
                "css/thesis.tex",
            ]
        );
    }

    /// 已知债 #25：非 XeTeX 引擎**不能**加 `-no-pdf`——pdflatex 会报 `unrecognized option`（Quick
    /// 直接跑不起来），lualatex 会静默忽略（照样写 PDF，等于白等一趟转换）。
    #[test]
    fn quick_command_omits_no_pdf_for_engines_without_xdv() {
        for (engine, binary) in [
            (latteset_core::types::Engine::LuaLaTeX, "lualatex"),
            (latteset_core::types::Engine::PdfLaTeX, "pdflatex"),
        ] {
            let mut req = sample_req();
            req.engine = engine;
            let cmd = engine_command(&req, CompileKind::Quick);
            assert_eq!(cmd.as_std().get_program().to_string_lossy(), binary);
            assert_eq!(
                argv(&cmd),
                vec![
                    "-interaction=nonstopmode",
                    "-synctex=1",
                    "-output-directory=tmp",
                    "css/thesis.tex",
                ],
                "{binary} 的 Quick 必须是「引擎单趟直写 PDF」"
            );
        }
    }

    /// Tectonic（2026-09 引入）：自己的命令形态（**不经 latexmk、不走我们的 xdvipdfmx**），参数按
    /// 测试方案必带；**且不能给它设 `SOURCE_DATE_EPOCH`**（D4 裁决：它把该变量当引擎时间源，
    /// 设 0 会让正文 `\today` 印成 1970-01-01；XeTeX 只动 PDF `/ID`）。
    ///
    /// `-C` 是**条件**参数（P0/t14，LIB-8）：缓存有证据才加；冷缓存/首跑档不能加，否则
    /// Tectonic 拒绝联网取 bundle、首编必失败（`docs/tectonic-library-plan.md` §6 P0）。
    #[test]
    fn tectonic_command_and_epoch_scoping() {
        let mut req = sample_req();
        req.engine = latteset_core::types::Engine::Tectonic;

        // 资源就绪档（缓存有可用 bundle）：`-C` 在首位 —— 离线、不联网
        let quick = compile_command(&req, CompileKind::Quick, BundlePolicy::CachedOnly);
        assert_eq!(quick.as_std().get_program().to_string_lossy(), "tectonic");
        assert_eq!(
            argv(&quick),
            vec![
                "-C",
                "-k",
                "--keep-logs",
                "--synctex",
                "-p",
                "-o",
                "tmp",
                "-r",
                "1",
                "css/thesis.tex",
            ],
            "Tectonic Quick（缓存就绪）必须是「离线缓存 + 保留中间产物 + 页进度 + **两趟**」——\
             单趟（`-r 0`）会丢掉整段目录，见 `tectonic_command` 的注释"
        );
        // Full = 收敛（不传 -r，用引擎默认趟数）
        let full = compile_command(&req, CompileKind::Full, BundlePolicy::CachedOnly);
        assert_eq!(full.as_std().get_program().to_string_lossy(), "tectonic");
        assert_eq!(
            argv(&full),
            vec!["-C", "-k", "--keep-logs", "--synctex", "-p", "-o", "tmp", "css/thesis.tex"]
        );

        // 首跑档（缓存没证据）：**不含 `-C`**，其余参数逐一相同 —— 让 Tectonic 能按需联网取 bundle。
        // 反例自证：若 `-C` 变回无条件，下面两条断言必红。
        for (kind, cold) in [
            (CompileKind::Quick, compile_command(&req, CompileKind::Quick, BundlePolicy::AllowFetch)),
            (CompileKind::Full, compile_command(&req, CompileKind::Full, BundlePolicy::AllowFetch)),
        ] {
            let cold = argv(&cold);
            assert!(
                !cold.iter().any(|a| a == "-C"),
                "{kind:?} 首跑档不得带 `-C`（带了就永远拿不到 bundle）"
            );
            let warm = argv(&compile_command(&req, kind, BundlePolicy::CachedOnly));
            let mut expected = warm.clone();
            expected.retain(|a| a != "-C");
            assert_eq!(cold, expected, "{kind:?} 首跑档与就绪档只差一个 `-C`");
        }

        // 环境变量分档（D4）：Tectonic 两档都不设 epoch
        let has_epoch = |c: &tokio::process::Command| {
            c.as_std()
                .get_envs()
                .any(|(k, v)| k.to_string_lossy() == "SOURCE_DATE_EPOCH" && v.is_some())
        };
        assert!(!has_epoch(&quick), "Tectonic 不能设 SOURCE_DATE_EPOCH");
        assert!(!has_epoch(&full), "Tectonic 不能设 SOURCE_DATE_EPOCH");
        for engine in [
            latteset_core::types::Engine::XeLaTeX,
            latteset_core::types::Engine::LuaLaTeX,
            latteset_core::types::Engine::PdfLaTeX,
        ] {
            let mut r2 = sample_req();
            r2.engine = engine;
            assert!(
                has_epoch(&engine_command(&r2, CompileKind::Quick)),
                "{engine:?} 仍必须固定 epoch（㉚ 的可复现前提）"
            );
            assert!(has_epoch(&engine_command(&r2, CompileKind::Full)), "{engine:?} Full 同");
        }
    }

    /// Tectonic 端到端（`#[ignore]`，需要装好 `tectonic`）：走**完整 runner 路径**跑一次中文文档，
    /// 断言"出 PDF + **无页哈希**"——Tectonic 的 PDF 档永不落 `.xdv`，页级复用 A/B/C 在该引擎下不可用
    /// （下游按"无法判定"保守全量刷新）。命令构造（`-C -k --keep-logs --synctex -p`）由
    /// [`tectonic_command_and_epoch_scoping`] 断言；这条用例额外复核**本机缓存就绪时 `-C` 档能真跑通**
    /// （`-C` 是最坏情况：缓存里缺一个宏包就会失败）。
    #[tokio::test]
    #[ignore]
    async fn tectonic_compiles_cjk_pdf_without_page_hashes() {
        if std::process::Command::new("tectonic").arg("--version").output().is_err() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("tectonic-e2e");
        project.put(
            "main.tex",
            "\\documentclass[12pt]{ctexart}\n\\begin{document}\n你好，世界。Tectonic 端到端用例。\n\\end{document}\n",
        );
        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let mut r = req(&project); // Full（收敛）
        r.engine = latteset_core::types::Engine::Tectonic;
        // P0（t14）真机证据：走 runner 前先把判定依据打出来（`-- --ignored --nocapture` 可见）。
        // 冷缓存档：在外层设 `TECTONIC_CACHE_DIR=<空目录>` 跑同一条用例。
        let probe = crate::tectonic::probe();
        eprintln!(
            "Tectonic bundle 策略 = {:?}（cache_ready={}，cache_dir={:?}，forced={:?}）",
            probe.policy, probe.cache_ready, probe.cache_dir, probe.forced
        );
        let out = runner.compile(r, CancellationToken::new()).await;
        let CompileOutcome::Success { pdf_path, page_hashes, .. } = &out else {
            panic!("Tectonic 编译应成功，实际：{out:?}");
        };
        assert!(pdf_path.exists(), "项目根应有 PDF：{}", pdf_path.display());
        let size = std::fs::metadata(pdf_path).expect("读 PDF 元数据").len();
        assert!(size > 1000, "PDF 太小（{size} B），像是失败产物");
        assert!(
            page_hashes.is_empty(),
            "Tectonic 的 PDF 档没有 XDV ⇒ 页哈希必须为空（前端据此按'无法判定'全量刷新）"
        );
    }

    /// 已知债 #25：页哈希基线按引擎分文件——换引擎后不能拿旧口径的基线比大小。
    #[test]
    fn pages_cache_path_is_engine_scoped() {
        let tmp = Path::new("/proj/tmp");
        let xe = pages_cache_path(tmp, "main", latteset_core::types::Engine::XeLaTeX);
        let lua = pages_cache_path(tmp, "main", latteset_core::types::Engine::LuaLaTeX);
        assert_eq!(xe, PathBuf::from("/proj/tmp/main.xelatex.pages"));
        assert_eq!(lua, PathBuf::from("/proj/tmp/main.lualatex.pages"));
        assert_ne!(xe, lua, "两个引擎的页哈希基线必须互不覆盖");
    }

    /// 页哈希缓存的**口径标记**（2026-09，已知债 #26 的迁移方案 B）：老格式（首行直接是哈希）
    /// 必须 fail-closed 成 `None`（只退化一轮），**绝不能**被当成"逐页相同"或"零页变化"。
    #[tokio::test]
    async fn pages_cache_requires_scope_marker_and_rejects_legacy() {
        let project = TempProject::new("pages-cache-scope");
        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let path = project.dir.join("tmp/main.xelatex.pages");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        // ① 老格式：首行就是 16 进制哈希（2026-09 之前的 raw 缓存）⇒ 无法判定
        std::fs::write(&path, "0123456789abcdef\nfedcba9876543210\n").unwrap();
        assert_eq!(
            runner.read_pages_cache(&path).await,
            None,
            "老格式必须 fail-closed"
        );

        // ② 空 / 只有标记 / 坏行：都不 panic，一律"无法判定"
        std::fs::write(&path, "").unwrap();
        assert_eq!(runner.read_pages_cache(&path).await, None);
        std::fs::write(&path, "v1\n").unwrap();
        assert_eq!(
            runner.read_pages_cache(&path).await,
            None,
            "只有标记没有页 = 无法判定"
        );
        std::fs::write(&path, "v1\nnot-a-hex\n").unwrap();
        assert_eq!(
            runner.read_pages_cache(&path).await,
            None,
            "坏行 → 无法判定"
        );

        // ③ 新格式往返（首行标记 + 每页一行十六进制）
        runner.write_pages_cache(&path, &[1u64, 0xdead_beef]).await;
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("v1\n"), "首行必须是口径标记：{text:?}");
        assert_eq!(
            runner.read_pages_cache(&path).await,
            Some(vec![1, 0xdead_beef])
        );

        // ④ 空表不写盘（"无法判定"不能被固化成"零页变化"）
        let empty = project.dir.join("tmp/empty.pages");
        runner.write_pages_cache(&empty, &[]).await;
        assert!(!empty.exists(), "空表不应写缓存");
    }

    /// 已知债 #26 的**真机回归**（`#[ignore]`，需要 TeX Live）：在真实多页项目上，编辑第 1 页
    /// 插 1 个汉字只应报**少数几页**变化。旧口径（页哈希含 `bop` 尾部的 `prev`）会报"第 3 页起
    /// 全部变了"——实验室实测 25/26 页（`docs/research/page-hash-prev-quant.md` §3）。
    #[tokio::test]
    #[ignore]
    async fn page_growth_edit_marks_few_pages_on_real_project() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("prev-scope-real");
        // 20 个 `\newpage` 段落：每段自成页，插入 1 个汉字只应影响第 1 页
        let mut tex = String::from("\\documentclass[12pt]{ctexart}\n\\begin{document}\n");
        for i in 1..=20 {
            tex.push_str(&format!(
                "第 {i} 页的内容，用来占满一行并保证分页稳定。\\newpage\n"
            ));
        }
        tex.push_str("\\end{document}\n");
        project.put("main.tex", &tex);

        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        // 1) Full 建产物；2) Quick 立基线
        assert!(
            matches!(
                runner.compile(req(&project), CancellationToken::new()).await,
                CompileOutcome::Success { .. }
            ),
            "Full 应成功"
        );
        let CompileOutcome::Success {
            page_hashes: base, ..
        } = runner
            .compile(quick_req(&project), CancellationToken::new())
            .await
        else {
            panic!("基线 Quick 应成功");
        };
        assert!(base.len() >= 10, "夹具应有多页，实际 {} 页", base.len());

        // 3) 编辑：第 1 页正文插 1 个汉字（页长 +3B ⇒ 触发 prev 链整体平移）
        project.put("main.tex", &tex.replacen("第 1 页的内容", "第 1 页的内容啊", 1));
        let CompileOutcome::Success {
            page_hashes: after, ..
        } = runner
            .compile(quick_req(&project), CancellationToken::new())
            .await
        else {
            panic!("编辑后的 Quick 应成功");
        };

        let changed = latteset_core::xdv::changed_pages(Some(&base), &after);
        assert!(changed.contains(&1), "插入点所在页必须被判变化：{changed:?}");
        assert!(
            changed.len() <= 3,
            "只应报编辑处附近的少数页，实际 {} 页（共 {} 页）：{changed:?}；\
             旧口径含 bop.prev 时会报第 3 页起全部",
            changed.len(),
            after.len()
        );
    }

    #[test]
    fn full_command_uses_latexmk_with_engine_flag() {
        let mut req = sample_req();
        req.engine = latteset_core::types::Engine::LuaLaTeX;
        let cmd = engine_command(&req, CompileKind::Full);
        assert_eq!(cmd.as_std().get_program().to_string_lossy(), "latexmk");
        assert_eq!(
            argv(&cmd),
            vec![
                "-lualatex",
                "-outdir=tmp",
                "-synctex=1",
                "-interaction=nonstopmode",
                "css/thesis.tex",
            ]
        );
    }

    #[test]
    fn both_paths_pin_source_date_epoch_for_reproducible_pdf() {
        // roadmap ㉚：不固定这个变量时，同一份源码两次编译的 PDF 只差 trailer 的 /ID（实测 64 字节），
        // 会让"输出 diff / 只重排变化页"分不清"真变了"和"ID 抖了"。
        for kind in [CompileKind::Quick, CompileKind::Full] {
            let cmd = engine_command(&sample_req(), kind);
            let epoch = cmd
                .as_std()
                .get_envs()
                .find(|(k, _)| k.to_string_lossy() == "SOURCE_DATE_EPOCH")
                .map(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()));
            assert_eq!(epoch, Some(Some("0".to_string())), "{kind:?} 路径必须固定 SOURCE_DATE_EPOCH");
        }
    }

    impl TempProject {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("latteset-it-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("创建临时项目失败");
            Self { dir }
        }

        fn put(&self, rel: &str, content: &str) {
            let p = self.dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, content).unwrap();
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn latexmk_available() -> bool {
        std::process::Command::new("latexmk")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn lualatex_available() -> bool {
        std::process::Command::new("lualatex")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn req(project: &TempProject) -> CompileRequest {
        CompileRequest {
            root_file: project.dir.join("main.tex"),
            project_root: project.dir.clone(),
            engine: latteset_core::types::Engine::XeLaTeX,
            timeout: test_timeout(),
            kind: CompileKind::Full,
        }
    }

    /// 集成用例的超时（默认 60s，与历史口径一致）。
    ///
    /// 可用 `LATTESET_TEST_TIMEOUT_SECS` 放长：**冷缓存的 Tectonic 首编**要联网取 bundle
    /// （实测约 65 MB，几十秒到几分钟），60s 的夹具直接撞超时，量不出"取到 bundle 后能编过"。
    fn test_timeout() -> Duration {
        Duration::from_secs(
            std::env::var("LATTESET_TEST_TIMEOUT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(60),
        )
    }

    /// 同一项目上的 Quick 请求（编辑触发的强度）。
    fn quick_req(project: &TempProject) -> CompileRequest {
        CompileRequest {
            kind: CompileKind::Quick,
            ..req(project)
        }
    }

    /// 功能点 A（docs/research/incremental-edit-x-dvi.md §2.2）：页哈希与上次**逐页相同**时，
    /// Quick 路径跳过 `xdvipdfmx` 转换与 PDF 拷贝。
    ///
    /// 判据用 `tmp/main.pdf` 的 mtime：转换没跑 ⇒ 文件不会被重写。
    #[tokio::test]
    #[ignore]
    async fn skips_conversion_when_page_hashes_unchanged() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("skip-convert");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nStable content for skip test.\n\\end{document}\n",
        );
        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let tmp_pdf = project.dir.join("tmp/main.pdf");

        // 1) Full：建产物 + 写页哈希缓存
        let first = runner.compile(req(&project), CancellationToken::new()).await;
        assert!(matches!(first, CompileOutcome::Success { .. }), "预期成功：{first:?}");

        // 2) 第一次 Quick：与 Full 的页哈希可能不同（收敛趟差异）→ 允许转换；这一步确立 Quick 基线
        let second = runner.compile(quick_req(&project), CancellationToken::new()).await;
        assert!(matches!(second, CompileOutcome::Success { .. }), "预期成功：{second:?}");
        tokio::time::sleep(Duration::from_millis(1100)).await; // 避开文件系统 mtime 粒度
        let before = std::fs::metadata(&tmp_pdf).expect("tmp/main.pdf 应存在").modified().unwrap();

        // 3) 第二次 Quick：同源同强度 → 页哈希必然相同 → 必须跳过转换
        let third = runner.compile(quick_req(&project), CancellationToken::new()).await;
        let CompileOutcome::Success { page_hashes, .. } = &third else {
            panic!("预期成功：{third:?}");
        };
        assert!(!page_hashes.is_empty(), "应产出页哈希，否则'跳过'的判据不成立");
        let after = std::fs::metadata(&tmp_pdf).expect("tmp/main.pdf 应存在").modified().unwrap();
        assert_eq!(
            after, before,
            "页哈希未变时不应重写 tmp/main.pdf（说明 xdvipdfmx 被跳过了）"
        );
        // 项目根的 PDF 也必须还在（跳过转换 = 复用上一次的产物）
        assert!(
            project.dir.join("main.pdf").exists(),
            "跳过转换后项目根的 PDF 必须仍然存在"
        );
    }

    /// 已知债 #25 的回归：`engine=lualatex` 的 Quick 必须走"**引擎自己写 PDF → 直接拷贝**"，
    /// 既不调 `xdvipdfmx`、也不读 XDV（读了就会把上一次 XeLaTeX 留下的陈旧 XDV 当成本次产物）。
    ///
    /// 两种原bug现场都覆盖：
    /// ① 目录里**有**陈旧 XDV（先跑一次 XeLaTeX）——旧代码会拿它算页哈希并转出 PDF 覆盖 LuaLaTeX 的产出；
    /// ② 目录里**没有** XDV（把 `.xdv` 删掉再跑）——旧代码会 `xdvipdfmx: Could not open specified DVI` 报失败。
    ///
    /// 前置：LuaLaTeX 要求**可写的 `TEXMFVAR`**（luaotfload 字体缓存；首次会花 ~26s 建库）。
    /// 本机默认落在 `~/.texlive<年>/texmf-var`，沙箱/只读配置下不可写 → 跑本用例时用环境变量指到可写目录。
    #[tokio::test]
    #[ignore = "需要 lualatex + 可写 TEXMFVAR（字体缓存）；对应已知债 #25"]
    async fn quick_with_lualatex_keeps_its_own_pdf() {
        if !latexmk_available() || !lualatex_available() {
            eprintln!("跳过：系统未安装 latexmk / lualatex（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("lua-quick");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nLua engine isolation.\n\\end{document}\n",
        );
        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let lua_quick = || CompileRequest {
            engine: latteset_core::types::Engine::LuaLaTeX,
            kind: CompileKind::Quick,
            ..req(&project)
        };

        // 现场 ①：XeLaTeX 先编一次，留下陈旧 XDV 与 XeLaTeX 版产物
        let xe = runner.compile(req(&project), CancellationToken::new()).await;
        assert!(matches!(xe, CompileOutcome::Success { .. }), "XeLaTeX 基线应成功：{xe:?}");
        assert!(
            project.dir.join("tmp/main.xdv").exists(),
            "前置现场要求：XeLaTeX 必须留下 tmp/main.xdv"
        );

        let out = runner.compile(lua_quick(), CancellationToken::new()).await;
        let CompileOutcome::Success { page_hashes, .. } = &out else {
            panic!("LuaLaTeX 的 Quick 必须成功（引擎写了 PDF，不该因转换失败而报错）：{out:?}");
        };
        assert!(
            page_hashes.is_empty(),
            "没有 XDV 就不该有页哈希——读到陈旧 XDV 的哈希会让下游拿错口径判'未变'"
        );
        let root = std::fs::read(project.dir.join("main.pdf")).expect("项目根应有 PDF");
        let tmp = std::fs::read(project.dir.join("tmp/main.pdf")).expect("引擎应写出 tmp/main.pdf");
        assert_eq!(
            root, tmp,
            "项目根 PDF 必须就是引擎刚写出的 tmp/main.pdf（不能被陈旧 XDV 的转换结果覆盖）"
        );
        assert!(
            !project.dir.join("tmp/main.lualatex.pages").exists(),
            "无页哈希 → 不应写页哈希缓存（免得下次拿它当基线）"
        );

        // 现场 ②：把 XDV 彻底删掉（干净项目）→ Quick 仍必须成功
        std::fs::remove_file(project.dir.join("tmp/main.xdv")).unwrap();
        let clean = runner.compile(lua_quick(), CancellationToken::new()).await;
        assert!(
            matches!(clean, CompileOutcome::Success { .. }),
            "没有 XDV 的干净项目里，LuaLaTeX 的 Quick 也必须成功：{clean:?}"
        );
    }

    /// 成功路径：编译 → PDF 拷贝到项目根 + tmp/ 中间文件 + SyncTeX 双向可用。
    #[tokio::test]
    #[ignore]
    async fn compile_success_and_synctex() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("success");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nHello Latteset\n\\end{document}\n",
        );

        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let outcome = runner
            .compile(req(&project), tokio_util::sync::CancellationToken::new())
            .await;

        match outcome {
            CompileOutcome::Success { pdf_path, .. } => {
                assert_eq!(pdf_path, project.dir.join("main.pdf"));
                assert!(pdf_path.exists(), "PDF 应拷贝到项目根");
                assert!(project.dir.join("tmp/main.log").exists(), "中间文件应收纳在 tmp/");
                assert!(
                    project.dir.join("tmp/main.synctex.gz").exists(),
                    "-synctex=1 应产出 synctex 文件"
                );

                // SyncTeX CLI 输出契约实测（ADR-0008 风险落地）
                if std::process::Command::new("synctex").arg("--version").output().is_err() {
                    eprintln!("跳过 SyncTeX 断言：未安装 synctex");
                    return;
                }
                let cli = SyncTexCli;
                let pos = cli
                    .forward(
                        &SourcePosition {
                            file: project.dir.join("main.tex"),
                            line: 3,
                            column: 1,
                        },
                        &pdf_path,
                    )
                    .await
                    .expect("正向定位应成功");
                assert!(pos.page >= 1);

                let back = cli
                    .inverse(
                        &SyncTexPosition { page: pos.page, x: pos.x, y: pos.y },
                        &pdf_path,
                    )
                    .await
                    .expect("反向定位应成功");
                assert!(back.line >= 1);
            }
            other => panic!("预期成功，得到：{other:?}"),
        }
    }

    /// Quick 路径（roadmap ㉘）：已有构建产物时直调引擎单趟，命令不经 latexmk。
    ///
    /// 断言「真的跑了引擎」而非「请求被接受」：先 Full 建产物（含 .aux/.toc），
    /// 再改正文源码走 Quick → Success 必须报 `kind: Quick`，且 PDF/SyncTeX 齐全
    /// （后续编辑期的双向定位依赖它）。
    #[tokio::test]
    #[ignore]
    async fn quick_path_skips_latexmk_and_reports_quick() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("quick");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\tableofcontents\n\\section{One}\\label{s:one}\nSee \\ref{s:one}.\n\\end{document}\n",
        );
        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let cancel = || tokio_util::sync::CancellationToken::new();

        // 1) 首编 = Full（建 tmp/main.aux，Quick 的前置条件）
        let first = runner.compile(req(&project), cancel()).await;
        assert!(
            matches!(first, CompileOutcome::Success { kind: CompileKind::Full, .. }),
            "首编应为 Full，实得：{first:?}"
        );
        assert!(project.dir.join("tmp/main.aux").exists(), "Full 应产出 .aux");

        // 2) 编辑正文 → Quick（单趟直调 xelatex）
        // 记录 latexmk 依赖库 mtime：Quick 若误走 latexmk，这一趟必然刷新它。
        let fdb = project.dir.join("tmp/main.fdb_latexmk");
        let fdb_before = std::fs::metadata(&fdb).and_then(|m| m.modified()).ok();
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\tableofcontents\n\\section{One}\\label{s:one}\nSee \\ref{s:one}.\nEdited.\n\\end{document}\n",
        );
        let mut quick = req(&project);
        quick.kind = CompileKind::Quick;
        let second = runner.compile(quick, cancel()).await;
        assert!(
            matches!(second, CompileOutcome::Success { kind: CompileKind::Quick, .. }),
            "有产物时编辑触发应跑单趟并报 Quick，实得：{second:?}"
        );
        assert!(project.dir.join("main.pdf").exists(), "Quick 也应产出 PDF");
        assert!(
            project.dir.join("tmp/main.synctex.gz").exists(),
            "Quick 也应产出 synctex（定位不能因强度降级而失效）"
        );
        assert_eq!(
            std::fs::metadata(&fdb).and_then(|m| m.modified()).ok(),
            fdb_before,
            "Quick 不该动 latexmk 依赖库（.fdb_latexmk mtime 变化 = 误走 latexmk）"
        );
    }

    /// Quick 前置条件缺失（无 tmp/<stem>.aux）→ runner 自动升级 Full，避免引用全是 `??`。
    #[tokio::test]
    #[ignore]
    async fn quick_upgrades_to_full_without_artifacts() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("quick-upgrade");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\tableofcontents\n\\section{One}\\label{s:one}\nSee \\ref{s:one}.\n\\end{document}\n",
        );
        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let mut quick = req(&project);
        quick.kind = CompileKind::Quick;
        let outcome = runner.compile(quick, tokio_util::sync::CancellationToken::new()).await;

        match outcome {
            CompileOutcome::Success { kind, .. } => {
                assert_eq!(kind, CompileKind::Full, "无产物时必须升级为 Full");
                // 交叉引用必须已解析（单趟会留下 ??）
                let pdf = std::fs::read(project.dir.join("main.pdf")).expect("应产出 PDF");
                let log = std::fs::read_to_string(project.dir.join("tmp/main.log")).unwrap();
                assert!(
                    !log.contains("There were undefined references"),
                    "升级为 Full 后不应有未解析引用；PDF {} 字节",
                    pdf.len()
                );
            }
            other => panic!("预期成功，得到：{other:?}"),
        }
    }

    /// 内容错误路径：非零退出 → .log 解析 → ContentError。
    #[tokio::test]
    #[ignore]
    async fn compile_content_error() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("error");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\undefinedcommandhere\n\\end{document}\n",
        );

        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let outcome = runner
            .compile(req(&project), tokio_util::sync::CancellationToken::new())
            .await;

        match outcome {
            CompileOutcome::ContentError { errors } => {
                assert!(!errors.is_empty(), "应有解析出的错误条目");
                assert!(errors.iter().any(|e| e.line.is_some()), "错误应带行号：{errors:?}");
            }
            other => panic!("预期内容错误，得到：{other:?}"),
        }
    }

    /// 超时路径：1ms 超时必触发 → 树杀 → Timeout + **证据化条目**（roadmap ㉕）。
    #[tokio::test]
    #[ignore]
    async fn compile_timeout_kills_tree_and_reports_evidence() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("timeout");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nBig\n\\end{document}\n",
        );

        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let mut request = req(&project);
        request.timeout = Duration::from_millis(1); // 必超时
        let outcome = runner
            .compile(request, tokio_util::sync::CancellationToken::new())
            .await;

        match outcome {
            CompileOutcome::Timeout { entry } => {
                // 超时必须带「原因 + 怎么改」——这正是 ㉕ 要修的东西（此前超时不给任何条目）
                assert_eq!(entry.kind, ErrorKind::Timeout);
                let diag = entry.diagnosis.expect("超时条目必须带诊断");
                // 1ms 超时时项目还没有 .aux → 判定为首编 → 直接建议到 900s（避免"注定不够"的一轮）
                let suggested = diag.suggested_timeout_secs.expect("应给出一键重试的超时值");
                assert_eq!(suggested, 900, "首编应跳档到 900s：{diag:?}");
                assert!(
                    entry.message.contains("1s") || entry.message.contains("0s"),
                    "消息应含实际上限：{}",
                    entry.message
                );
                // 树没被杀干净的话这里会留下 latexmk 子进程（kill_tree 的既有断言在别的用例里）
            }
            other => panic!("预期超时，得到：{other:?}"),
        }
    }

    /// 取消路径：提前取消 → Aborted。
    #[tokio::test]
    #[ignore]
    async fn compile_cancel_aborts() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("cancel");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nBig\n\\end{document}\n",
        );

        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel(); // 立即取消
        let outcome = runner.compile(req(&project), cancel).await;
        assert_eq!(outcome, CompileOutcome::Aborted);
    }

    #[test]
    fn root_stem_variants() {
        assert_eq!(root_stem(Path::new(r"C:\proj\main.tex")), "main");
        assert_eq!(root_stem(Path::new("css/thesis.tex")), "thesis");
        assert_eq!(root_stem(Path::new("main")), "main"); // 无扩展名回退
    }

    /// 流式反馈记录器（阶段 2）：记下每次回调的**时刻**，好断言"编译结束前就到了"。
    struct RecordingProgress {
        start: Instant,
        pages: std::sync::Mutex<Vec<(u32, Duration)>>,
        errors: std::sync::Mutex<Vec<(usize, Duration)>>,
    }

    impl RecordingProgress {
        fn new() -> Self {
            Self {
                start: Instant::now(),
                pages: std::sync::Mutex::new(Vec::new()),
                errors: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl CompileProgress for RecordingProgress {
        fn pages(&self, pages: u32) {
            self.pages.lock().unwrap().push((pages, self.start.elapsed()));
        }
        fn errors(&self, errors: &[ErrorEntry]) {
            self.errors.lock().unwrap().push((errors.len(), self.start.elapsed()));
        }
    }

    /// 流式输出（阶段 2）：真实编译期间就该报上"已排版 N 页"，而不是等编译结束。
    #[tokio::test]
    #[ignore]
    async fn reports_streaming_pages_before_compile_finishes() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("stream-pages");
        // 三页文档：页标记必须出现多次，且最后一次为 3
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nFirst\n\\newpage\nSecond\n\\newpage\nThird\n\\end{document}\n",
        );
        let progress = std::sync::Arc::new(RecordingProgress::new());
        let runner = LatexmkRunner::new(std::sync::Arc::new(crate::fs::TokioFs), progress.clone());
        let started = Instant::now();
        let outcome = runner
            .compile(req(&project), tokio_util::sync::CancellationToken::new())
            .await;
        let total = started.elapsed();
        assert!(matches!(outcome, CompileOutcome::Success { .. }), "预期成功：{outcome:?}");

        let events = progress.pages.lock().unwrap().clone();
        let seen: Vec<u32> = events.iter().map(|(p, _)| *p).collect();
        assert!(!events.is_empty(), "流式页进度一次都没上报（stdout 管道没接上？）");
        assert_eq!(seen.iter().max().copied(), Some(3), "最大页码应为 3：{seen:?}");
        // 注意：latexmk 会跑**多趟**（每趟一个新进程），页码在多趟之间会重启 → 只要求**非递减**。
        // 前端展示的是 max（compileStore.setProgress 只接受更大的值），故重启不会造成回跳。
        assert!(seen.windows(2).all(|w| w[1] >= w[0]), "页码在单趟内必须单调递增：{seen:?}");
        // 关键断言：进度**在编译结束前**就到了（不是收尾时才补发）
        let (_, first_at) = events[0];
        let lead = total.saturating_sub(first_at);
        assert!(
            lead >= Duration::from_millis(100),
            "首个页进度距编译结束只有 {lead:?}（总时长 {total:?}）——像是收尾才发的"
        );
    }

    /// 流式输出（阶段 2）：编译**进行中**就要报出错误（长文档不必等到超时/结束）。
    #[tokio::test]
    #[ignore]
    async fn reports_live_errors_before_compile_finishes() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("stream-errors");
        // 错误出现在第 1 页，后面还有足够内容让编译继续跑一会儿（nonstopmode 不中断）
        let filler = "Filler line with some words. ".repeat(20);
        project.put(
            "main.tex",
            &format!(
                "\\documentclass{{article}}\n\\begin{{document}}\n\\undefinedcommandhere\n{filler}\n\\newpage\n{filler}\n\\end{{document}}\n"
            ),
        );
        let progress = std::sync::Arc::new(RecordingProgress::new());
        let runner = LatexmkRunner::new(std::sync::Arc::new(crate::fs::TokioFs), progress.clone());
        let started = Instant::now();
        let outcome = runner
            .compile(req(&project), tokio_util::sync::CancellationToken::new())
            .await;
        let total = started.elapsed();
        assert!(
            matches!(outcome, CompileOutcome::ContentError { .. }),
            "预期内容错误：{outcome:?}"
        );

        let live = progress.errors.lock().unwrap().clone();
        assert!(!live.is_empty(), "编译中一次都没有上报错误（流式解析没生效？）");
        let (_, first_at) = live[0];
        let lead = total.saturating_sub(first_at);
        assert!(
            lead >= Duration::from_millis(100),
            "首个流式错误距编译结束只有 {lead:?}（总时长 {total:?}）——像是收尾才发的"
        );
    }

    #[test]
    fn latexmk_input_keeps_nested_relative_path() {
        // H2 回归：嵌套根文件必须用相对项目的完整路径，而非仅 stem（否则 latexmk 在项目根下找不到文件）
        let root = Path::new(r"C:\proj");
        assert_eq!(
            latexmk_input(Path::new(r"C:\proj\css\thesis.tex"), root),
            "css/thesis.tex"
        );
        assert_eq!(
            latexmk_input(Path::new(r"C:\proj\main.tex"), root),
            "main.tex"
        );
        // 不在项目根下 → 回退到完整路径（统一正斜杠）
        assert_eq!(
            latexmk_input(Path::new(r"D:\other\a.tex"), root),
            "D:/other/a.tex"
        );
    }

    // ---------------------------------------------------------------- 中文/非 ASCII 路径（roadmap P0-①）

    #[test]
    fn chinese_path_input_and_stem() {
        // 传给 latexmk 的输入必须是相对项目的正斜杠路径，中文原样保留（不得被转义/丢字）
        let root = Path::new(r"E:\项目\中文测试工程");
        assert_eq!(
            latexmk_input(Path::new(r"E:\项目\中文测试工程\中文主文件.tex"), root),
            "中文主文件.tex"
        );
        assert_eq!(
            latexmk_input(Path::new(r"E:\项目\中文测试工程\章节\第一章.tex"), root),
            "章节/第一章.tex"
        );
        // 产物名沿用 basename 的 stem，中文 stem 需与 pdf_dst 计算一致
        assert_eq!(root_stem(Path::new(r"E:\项目\中文测试工程\中文主文件.tex")), "中文主文件");
    }

    /// 中文路径全链路（真实 latexmk + synctex）：中文目录 + 中文文件名 + 中文子目录。
    ///
    /// 实测基线（2026-09，TeX Live 2026 / Windows，CP65001 与 CP936 均验证）：
    /// 编译 exit 0、`tmp/` 产出 `.log`/`.synctex.gz`、PDF 正常，synctex 双向可用且中文源路径可回传。
    #[tokio::test]
    #[ignore]
    async fn compile_chinese_paths_with_synctex() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        // 目录名含中文（temp_dir()/latteset-it-中文目录-<pid>）
        let project = TempProject::new("中文目录");
        project.put(
            "中文主文件.tex",
            "\\documentclass[UTF8]{ctexart}\n\\begin{document}\n\\include{章节/第一章}\n\\end{document}\n",
        );
        project.put("章节/第一章.tex", "\\section{子文件章节}\n中文正文。\n");

        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        let request = CompileRequest {
            root_file: project.dir.join("中文主文件.tex"),
            project_root: project.dir.clone(),
            engine: latteset_core::types::Engine::XeLaTeX,
            timeout: Duration::from_secs(60),
            kind: CompileKind::Full,
        };
        let outcome = runner
            .compile(request, tokio_util::sync::CancellationToken::new())
            .await;

        let pdf_path = match outcome {
            CompileOutcome::Success { pdf_path, .. } => pdf_path,
            other => panic!("中文路径应编译成功，实得：{other:?}"),
        };
        assert_eq!(pdf_path, project.dir.join("中文主文件.pdf"));
        assert!(pdf_path.exists(), "PDF 应拷贝到项目根（中文名）");
        assert!(
            project.dir.join("tmp/中文主文件.log").exists(),
            "中文名的 .log 应落在 tmp/"
        );
        assert!(
            project.dir.join("tmp/中文主文件.synctex.gz").exists(),
            "中文名的 synctex 数据应产出"
        );

        if std::process::Command::new("synctex").arg("--version").output().is_err() {
            eprintln!("跳过 SyncTeX 断言：未安装 synctex");
            return;
        }
        let cli = SyncTexCli;
        // 正向：从中文子文件定位 → 页码有效
        let pos = cli
            .forward(
                &SourcePosition {
                    file: project.dir.join("章节/第一章.tex"),
                    line: 1,
                    column: 1,
                },
                &pdf_path,
            )
            .await
            .expect("中文路径正向定位应成功");
        assert!(pos.page >= 1);

        // 反向：PDF → 源码，回传的**中文文件名必须完好**（这是跳回源码的判据）
        let back = cli
            .inverse(
                &SyncTexPosition {
                    page: pos.page,
                    x: pos.x,
                    y: pos.y,
                },
                &pdf_path,
            )
            .await
            .expect("中文路径反向定位应成功");
        let back_name = back
            .file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        assert!(
            back_name == "第一章.tex" || back_name == "中文主文件.tex",
            "反向定位应回传中文源文件名，实得：{}",
            back.file.display()
        );
    }

    /// 中文路径的内容错误：`.log` 解析须带出中文文件名 + 行号（错误列表点击跳转的判据）。
    #[tokio::test]
    #[ignore]
    async fn compile_chinese_path_content_error_keeps_file_and_line() {
        if !latexmk_available() {
            eprintln!("跳过：系统未安装 latexmk（需要 TeX Live/MiKTeX）");
            return;
        }
        let project = TempProject::new("中文错误");
        project.put(
            "中文主文件.tex",
            "\\documentclass{article}\n\\begin{document}\n\\undefinedcommandhere\n\\end{document}\n",
        );

        let runner = LatexmkRunner::new(
            std::sync::Arc::new(crate::fs::TokioFs),
            std::sync::Arc::new(latteset_core::scheduler::NoProgress),
        );
        // 必须编译中文根文件本身：req() 造的是 main.tex，项目里并不存在，
        // latexmk 会因缺少输入而无 .log 可解析（原用例恒失败，2026-09 修正）。
        let request = CompileRequest {
            root_file: project.dir.join("中文主文件.tex"),
            project_root: project.dir.clone(),
            engine: latteset_core::types::Engine::XeLaTeX,
            timeout: Duration::from_secs(60),
            kind: CompileKind::Full,
        };
        let outcome = runner
            .compile(request, tokio_util::sync::CancellationToken::new())
            .await;

        match outcome {
            CompileOutcome::ContentError { errors } => {
                assert!(!errors.is_empty(), "应有解析出的错误条目");
                let with_line = errors
                    .iter()
                    .find(|e| e.line.is_some())
                    .expect("错误应带行号");
                assert_eq!(with_line.line, Some(3));
                assert!(
                    with_line
                        .file
                        .as_deref()
                        .map(|f| f.contains("中文主文件.tex"))
                        .unwrap_or(false),
                    "错误条目应带中文文件名，实得：{:?}",
                    with_line.file
                );
            }
            other => panic!("预期内容错误，得到：{other:?}"),
        }
    }
}
