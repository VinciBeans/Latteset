//! 编译执行器（modules.md §2.6 / 设计决策 D2；位于基础设施层，ADR-0010）。
//!
//! **驱动外部 `tectonic`（子进程形态）** —— ADR-0015 之后引擎只剩 Tectonic，latexmk 驱动路径与
//! 其它引擎的命令构造已删除。超时检测、进程树杀、PDF 拷贝全在这里；调度器无时钟、无进程概念。
//!
//! **流式输出**（roadmap「阶段 2」）：进程的 stdout/stderr 接管道并**逐行读**——页标记
//! `[N]`（Tectonic 的 `-p`）增量汇总成"已排版 N 页"事件；输出文本节流解析成"编译中的错误"事件。
//! 终态仍以 `.log` 为准（`parse_log` 是权威），流式期间报的是**非权威中间态**；
//! 且流式错误**只报致命错误、不报警告**（理由见 [`entries_from_log`]）。

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use latteset_core::log_parser::{
    diagnose, diagnose_timeout, pages_typeset, parse_log, source_release_hint, DiagnosisKind,
    MessageKind, PageMarkerScanner, TimeoutEvidence,
};
use latteset_core::project::{collect_tex_files, FileSystem};
use latteset_core::scheduler::{CompileProgress, CompileRunner};
use latteset_core::types::{
    CompileKind, CompileOutcome, CompileRequest, ErrorEntry, ErrorKind, ENGINE_ID,
};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::tectonic::{self, BundlePolicy};

/// 编译中间目录（design.md：统一收纳 tmp/，与 core 忽略规则一致）。
const OUT_DIR: &str = "tmp";

/// 流式错误解析的节流间隔：既不让解析吃 CPU，也不让错误"迟到"太多。
const LIVE_ERROR_INTERVAL: Duration = Duration::from_millis(500);
/// 尾随 `.log` 的轮询间隔。
///
/// 注意：Tectonic 的 `.log` **收尾才落盘**，编译期间这条轮询多半读不到东西（见 [`LiveFeedback`]）；
/// 它保留为**补充/兜底**（某些引擎或平台会把日志边写边落盘），终态解析不依赖它。
const LOG_POLL_INTERVAL: Duration = Duration::from_millis(200);
/// 超时路径的**收尾读任务**等待上限：进程已死 → 管道很快 EOF；上限只防"孙子进程仍持有写端"。
const PUMP_DRAIN_TIMEOUT: Duration = Duration::from_millis(600);
/// 超时证据取捕获输出的**尾部**（字符数）：只要最后几屏——够判"有没有页输出/有没有致命错误"。
/// 全量缓冲最多 2 MiB（[`LIVE_ERROR_BUFFER`]），全量重扫没有意义。
const TIMEOUT_OUTPUT_TAIL: usize = 8 << 10;
/// 流式错误解析保留的输出上限（超出后只保留尾部——页标记是单调的，历史文本对进度无用）。
const LIVE_ERROR_BUFFER: usize = 1 << 21; // 2 MiB

/// 编译器实现（`CompileRunner` 的唯一实现）：驱动外部 `tectonic` 可执行文件。
///
/// 历史：ADR-0015 之前它按 [`CompileKind`] 驱动两条命令（`Full` 用 latexmk、`Quick` 直调引擎），
/// 名字叫 `LatexmkRunner`。引擎统一后只剩 Tectonic 一条路径，故改名并删除那条分支。
pub struct SubprocessRunner {
    pub fs: Arc<dyn FileSystem>,
    /// 编译进行中的反馈通道（默认 [`latteset_core::scheduler::NoProgress`]）。
    pub progress: Arc<dyn CompileProgress>,
}

impl SubprocessRunner {
    /// 构造：`progress` 传 `Arc::new(NoProgress)` 即关闭流式反馈（headless/测试）。
    pub fn new(fs: Arc<dyn FileSystem>, progress: Arc<dyn CompileProgress>) -> Self {
        Self { fs, progress }
    }
}

/// 编译进行中的反馈状态机（**唯一**写者：外部把新文本 `feed` 进来）。
///
/// 两个来源都喂它：① 引擎 stdout/stderr 管道（**主来源**）；② `tmp/<stem>.log` 的尾随读取
/// （**补充**，见 [`tail_log`]）。
///
/// 实测口径（Tectonic，2026-11 review E4 复核）：页进度走 `-p` 打在 **stdout 的 `[N]`**；
/// `.log` 是**收尾才落盘**的（`--keep-logs`）——编译期间 `tmp/<stem>.log` 往往一个字节都没有，
/// 超时/取消树杀时更是如此。所以：
/// - "边编译边报"的实时通道是**管道**，不是日志；
/// - 超时/取消诊断的证据也必须取**已捕获的 stdout/stderr**（见 [`SubprocessRunner::timeout_entry`]），
///   只读 `.log` 会拿到空证据；
/// - `.log` 仍是**终态**解析的权威来源（进程正常退出后它才完整）。
///
/// 同一份内容可能从两个来源各来一次 → 页码单调（重复无害），错误按指纹去重。
struct LiveFeedback {
    progress: Arc<dyn CompileProgress>,
    scanner: PageMarkerScanner,
    buffer: String,
    /// 流式通道观测到的最大页码（**事实**：引擎打过 `[N]`）。
    ///
    /// 单独留一份是因为超时快照可能取不到完整尾部：树杀与管道 EOF 之间有竞态，
    /// 最后的 `[N]` 可能落在尾部窗口之外。页证据不该因此丢失（review E4 的原始症状）。
    max_page: Option<u32>,
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
            max_page: None,
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
            // `PageMarkerScanner` 只升不降：直接记最大页
            self.max_page = Some(pages);
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

    /// 超时/取消路径的证据快照：已捕获输出的尾部 + 流式观测到的最大页码。
    ///
    /// 两者一起取是**为了不依赖竞态**：树杀到管道 EOF 之间，最后一段 `[N]` 可能还没进缓冲，
    /// 但 `max_page` 是先前 `feed` 时就记下的（引擎确实打过这个页标记）——页证据不会丢。
    fn evidence_snapshot(&self, max_chars: usize) -> (String, Option<u32>) {
        (self.tail(max_chars), self.max_page)
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

/// 尾随 `tmp/<stem>.log`（**补充来源**，不是主力：Tectonic 的日志收尾才落盘）。
///
/// 用 [`FileSystem::read_appended`] 只读新增字节；每 [`LOG_POLL_INTERVAL`] 一次。
/// `stop` 由调用方在编译收尾时置位（终态解析以整份 `.log` 为准）。
/// 编译期间读不到文件是**正常**的（日志还没落盘），此时实时反馈完全依赖管道；
/// 收尾那次补读仍然有价值：日志落盘后能补上管道缓冲里丢掉的中段内容。
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

fn root_stem(root_file: &Path) -> String {
    root_file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".to_string())
}

/// Tectonic 的输入参数：相对项目根的完整路径（**不能只用 stem**——嵌套根文件如 `css/thesis.tex`
/// 只取 stem 会变成 `thesis.tex`，在项目根下不存在）。统一正斜杠：Windows 反斜杠在 TeX 的
/// 路径解析里是转义字符。Tectonic 的 jobname 取输入文件名 basename，故产物仍是 `tmp/<stem>.pdf`，
/// 与 `pdf_dst` 的 stem 计算一致。
fn input_path(root_file: &Path, project_root: &Path) -> String {
    root_file
        .strip_prefix(project_root)
        .unwrap_or(root_file)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Tectonic 命令（`docs/archive/research/tectonic-test-plan.md` §1.2 / INT-20）。
///
/// 参数按测试方案定为**必带**（`-C` 例外，见下）：
/// - `-C`（仅当 `bundle == CachedOnly`）：只用缓存资源（离线，不联网下载）。**不能无条件加**——
///   全新机器 + 空缓存时它会让首编必失败（`docs/archive/tectonic-library-plan.md` §6 P0 / E-1）。
///   判定依据与证据见 [`crate::tectonic`]；
/// - `-k`：保留中间产物（`.aux`/`.toc`/`.bbl`）—— Quick 的前置条件（`tmp/<stem>.aux`）与 bib 判据都要它；
/// - `--keep-logs`：保留 `.log`（Tectonic 的日志本来只在内存、收尾才落盘）；
/// - `--synctex`：产出 `.synctex.gz`（我们的正反向仍走外部 `synctex.exe`，需要本机 TeX Live）；
/// - `-p`：页进度走 **stdout 的 `[N]`** —— 这是 Tectonic 下唯一的实时页通道；
/// - `-r 1`（仅 Quick）：**两趟**（`-r N` = 再跑 N 趟）。⚠ **单趟（`-r 0`）会丢整段目录**：
///   Tectonic 的 `-o tmp` **只是输出目录、不作为输入目录**，所以它**读不回**上一趟留在 `tmp/` 的
///   `.toc`/`.aux`（库形态档靠 `seed_previous_intermediates` 预热），单趟档每趟都从零重建目录。
///   实测（release `tectonic.exe` 0.17.0、同夹具同命令形态、`-k` 保留中间产物）：
///   `-r 0` = **26 页 / 12,624 字**（第 2 页只剩页码标记，正文从第 3 页起 = 没有目录），
///   `-r 1` = **28 页 / 13,570 字**（目录占第 2–3 页，与收敛档逐项一致）。
///   Quick 的语义是"目录/交叉引用**页码**落后一趟"（roadmap ㉘）；单趟档丢的是整段目录，不是页码。
///   两趟 1057 ms（单趟 657、收敛 1495），Quick 仍明显快于 Full；
/// - `-o tmp`：产物落在 `tmp/<stem>.pdf`（`finish_success` 再原子拷贝到项目根）。
fn tectonic_command(req: &CompileRequest, quick: bool, bundle: BundlePolicy) -> tokio::process::Command {
    let mut c = crate::proc::command(ENGINE_ID);
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
    c.arg(input_path(&req.root_file, &req.project_root));
    c
}

#[async_trait]
impl CompileRunner for SubprocessRunner {
    async fn compile(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome {
        let stem = root_stem(&req.root_file);
        let tmp_dir = req.project_root.join(OUT_DIR);
        let pdf_dst = req.project_root.join(format!("{stem}.pdf"));

        // 输出目录由**产品自建**：Tectonic **不会**建 `tmp/` 本身——目录不存在时它直接
        // `error: output directory "tmp" does not exist`（exit 1）且**连 .log 都不落盘**
        // （U-14 实测，`test_file/u14/REPORT-u14.md`）⇒ 用户看到的是"编译失败且无法读取日志"
        // 这种没有信息量的报错。它自己会建**子目录**（`tmp/chapters/`、`tmp/a/b/`，含中文），
        // 所以这里只需要建 `tmp/` 这一层（幂等；真建不出来时后面的编译自会以更具体的方式失败）。
        if let Err(e) = tokio::fs::create_dir_all(&tmp_dir).await {
            debug!(dir = %tmp_dir.display(), "创建输出目录失败（继续尝试编译）：{e}");
        }

        // 强度决策（roadmap ㉘）：编辑触发的 Quick 跑「两趟」，其余跑完整收敛。
        // 但 **Quick 需要已有构建产物**才有意义（依赖上一趟的 .aux/.toc）；首次编译无 aux 时
        // 单趟会产出「引用全是 ??」的 PDF——故此处自动升级为 Full。
        // 探测失败（权限/竞态）按「无产物」保守处理：升级为 Full 只是更慢，不会出错。
        let had_aux = self.fs.exists(&tmp_dir.join(format!("{stem}.aux"))).await.unwrap_or(false);
        let mut kind = req.kind;
        if kind == CompileKind::Quick && !had_aux {
            debug!(stem, "无构建产物，Quick 升级为 Full（首编）");
            kind = CompileKind::Full;
        }

        // bundle 获取策略（P0/t14）：**只有拿到缓存正向证据才加 `-C`**——冷缓存加它等于
        // "拒绝联网取 bundle"，首编必失败。判定依据与证据见 crate::tectonic。
        let probe = tectonic::probe();
        debug!(
            policy = ?probe.policy,
            cache_ready = probe.cache_ready,
            cache_dir = ?probe.cache_dir,
            forced = ?probe.forced,
            "Tectonic bundle 策略"
        );
        let bundle = probe.policy;

        // 命令构造（modules.md §2.6 算法）：cwd = 项目根，相对 input/include 才能解析
        let mut cmd = tectonic_command(&req, kind == CompileKind::Quick, bundle);
        debug!(
            engine = ENGINE_ID,
            kind = kind.as_str(),
            "启动编译（Tectonic 子进程；Full = 引擎自带收敛）"
        );
        cmd.current_dir(&req.project_root)
            // 流式输出（roadmap「阶段 2」）：接管道逐行读——页进度与"编译中的错误"都从这里来。
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // 进程孤儿防护：若编译 future 被丢弃（应用退出/任务取消），随 future 一并杀掉子进程。
            // 避免子进程残留、持续写 tmp/ 或占用 PDF 锁。（树杀仍由 kill_tree 负责，这里是兜底。）
            .kill_on_drop(true);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            // Tectonic 未装时的文案必须**指对方向**：不是"TeX Live 未安装"——它自带 bundle、
            // 本来就不依赖 TL；它也不会命中 bundle/缓存失败那条路径（那是 spawn 成功之后的失败）。
            Err(e) => {
                return CompileOutcome::IoError {
                    message: format!("无法启动 tectonic（Tectonic 需单独安装、且要在 PATH 里）：{e}"),
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
                // 证据通道（review E4）：Tectonic 的 `.log` 是**收尾才落盘**的，被树杀时
                // `tmp/<stem>.log` 常常是零字节；而进程是 piped 的，`-p` 的页标记与 stderr
                // 都已经在内存缓冲里。所以这里**先等读任务把管道读干净**再取快照——
                // 顺序反了会漏掉最后一段输出（树杀 → EOF 之间还在管道里）。
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
                while let Some(pump) = pumps.pop() {
                    let _ = tokio::time::timeout(PUMP_DRAIN_TIMEOUT, pump).await;
                }
                let (output_tail, observed_pages) =
                    feedback.lock().await.evidence_snapshot(TIMEOUT_OUTPUT_TAIL);
                // roadmap ㉕：超时不静默重试，改为**现场采集证据**给出可操作诊断
                let entry = self
                    .timeout_entry(&req, &stem, &tmp_dir, had_aux, &output_tail, observed_pages)
                    .await;
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
                    // 成功收尾留在 select 分支内：分支里不能直接 `return`（会跳过下面的
                    // stop/join 收尾，泄漏读任务），所有收尾动作都在这里完成。
                    match self.finish_success(&tmp_dir, &stem, kind, &pdf_dst).await {
                        Ok(outcome) => {
                            // 成功路径的"外部工具"提示：biber / makeindex / glossaries 缺失不会让
                            // Tectonic 失败（它只跳过那一步），用户看到的会是"编译成功但引用/索引不对"。
                            for msg in external_tool_notices(self.fs.as_ref(), &tmp_dir, &stem).await {
                                warn!(stem, "{msg}");
                                self.progress.notice(msg);
                            }
                            outcome
                        }
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
                    message: format!("等待 tectonic 失败：{e}"),
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
        if matches!(outcome, CompileOutcome::IoError { .. } | CompileOutcome::ContentError { .. }) {
            let tail = feedback.lock().await.tail(4 << 10);
            if let Some(message) = tectonic::bundle_failure_message(bundle, &tail) {
                warn!(policy = ?bundle, "Tectonic 取 bundle 失败：已换成可执行文案");
                return CompileOutcome::IoError { message };
            }
        }
        outcome
    }
}

impl SubprocessRunner {
    /// 成功收尾：`tmp/<stem>.pdf` **原子拷贝**到项目根。
    ///
    /// 原子化：先写 `{stem}.pdf.tmp` 再 rename ⇒ 拷贝失败时旧 PDF 保留（不被截断/损坏）。
    ///
    /// **页哈希恒为空**：子进程形态的 Tectonic 走默认 PDF 档，**永不落 `.xdv`**（即使 `-k`，
    /// XDV→PDF 的转换也在内存文件表里完成）⇒ 页级差异不可判定，`PdfUpdated.pages == 0`，
    /// 前端全量刷新（这是该形态的既有行为）。页哈希与 A/B/C 只在**库内嵌形态**里有
    /// （XDV 在内存捕获表里，见 `latteset-tectonic`）。
    ///
    /// 返回 `Err(message)` → 调用方包装成 `IoError`。
    async fn finish_success(
        &self,
        tmp_dir: &Path,
        stem: &str,
        kind: CompileKind,
        pdf_dst: &Path,
    ) -> Result<CompileOutcome, String> {
        let pdf_src = tmp_dir.join(format!("{stem}.pdf"));
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
        Ok(CompileOutcome::Success {
            pdf_path: pdf_dst.to_path_buf(),
            kind,
            page_hashes: Vec::new(),
        })
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
    /// ③ 已排版到第几页（推进证据，见 [`pages_typeset`]）。
    /// 三项都是"看得见的事实"，诊断文案不据此猜测因果（宁可说"可能"，也不断言"卡住"）。
    /// 证据文本另交给 [`diagnose_timeout`]：里面若已有致命错误，超时就只是症状（提高超时救不了）。
    ///
    /// **证据来源**（review E4）：`.log` **不是**可靠来源——Tectonic 的日志收尾才落盘
    /// （`--keep-logs`），超时树杀时它常常是零字节或残缺。所以这里吃两路：
    /// `tmp/<stem>.log`（有内容就优先）＋ `output_tail`（进程 piped 的 stdout/stderr 尾部），
    /// 再加 `observed_pages`（流式通道已经确认过的 `[N]`，防止尾部窗口漏掉它）。
    /// 合并后 `[N]` 页标记、致命错误都能被看见，不再每次都落到"日志无页输出/可能卡住"。
    async fn timeout_entry(
        &self,
        req: &CompileRequest,
        stem: &str,
        tmp_dir: &Path,
        had_aux: bool,
        output_tail: &str,
        observed_pages: Option<u32>,
    ) -> ErrorEntry {
        let log_path = tmp_dir.join(format!("{stem}.log"));
        // 树杀刚发出，日志可能只落盘了一部分；读得到多少算多少（best-effort）
        let log = self.fs.read_to_string_lossy(&log_path).await.unwrap_or_default();
        let evidence_text = timeout_evidence_text(&log, output_tail, observed_pages);
        let pages = pages_typeset(&evidence_text);
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
        // 诊断看两样东西：本次运行的证据文本（里面可能已有致命错误 → 超时只是症状）与上面的证据
        let diagnosis = diagnose_timeout(&evidence_text, &evidence);
        debug!(
            timeout_secs,
            first_build = evidence.first_build,
            tex_files = ?tex_files,
            pages = ?pages,
            log_bytes = log.len(),
            output_chars = output_tail.chars().count(),
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
            facts.push(format!("已排版到第 {p} 页"));
        } else {
            facts.push("日志与输出均无页输出".to_string());
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

/// 超时诊断的证据文本：`.log` 优先，捕获的进程输出补在后面（并去噪），
/// 再用`observed_pages` 兜住"尾部窗口没截到 `[N]`"的情况。
///
/// 为什么必须合并（review E4 实测）：文档排完第 1 页后进入长循环，1 s 超时；
/// 流式 `[1]` 页标记早已从 stdout 到了内存，但 `tmp/` 里**连 `.log` 都没有**（收尾才落盘）⇒
/// 只读 `.log` 时诊断永远说"日志里还没出现任何页面输出"。合并后同一次超时能正确说
/// "已排版到第 1 页，是在推进只是比上限慢"。
///
/// `observed_pages` 是流式通道**已经上报过**的最大页码：树杀 → 管道 EOF 之间有竞态，
/// 最后一段 `[N]` 可能还没进缓冲就被取了尾部。既然引擎确实打过这个标记，就把它补成一行
/// `[N]` 交回给同一套解析（不新造判据，也不改变语义）。
fn timeout_evidence_text(log: &str, output_tail: &str, observed_pages: Option<u32>) -> String {
    let tail = denoise_output_tail(output_tail);
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

/// 进程输出尾部的轻量去噪：去 ANSI 序列与 `\r`（Tectonic 的进度行会用回车原地刷新），
/// 丢掉纯空白行。**不动** `[N]` 页标记与错误文本——它们正是证据本身。
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

/// 去掉 CSI/OSC 转义序列（`ESC [ … m` 之类）。只处理尾部证据用的最小集合。
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

/// 需要外部工具的三类产物 → 一句可行动提示（顺序即提示顺序）。
///
/// 判据是**工具自己的中间产物**：`<stem>.run.xml` 里真的请求了 `<binary>biber</binary>`（biblatex + biber）、
/// `<stem>.idx`（makeindex）、`<stem>.glo`（glossaries）。
///
/// 三类工具的**实际形态不同**（2026-11 复验实测，别混为一谈）：
/// - **biber**：Tectonic 0.17 的**子进程形态会调用外部 biber**（直跑日志 `note: Running external tool biber ...`，
///   上游 `driver.rs` 的 `external_tool_pass`）。所以缺 biber / 版本与 bundle 内 biblatex 不匹配时，
///   这一步会**失败**（编译中断或参考文献为空），不是"静默跳过"；
/// - **makeindex / glossaries**：Tectonic **不会**自动跑它们（只跳过那一步）⇒ "编译成功但索引/术语表为空"。
///
/// 所以提示分两段口径：biber 说"引擎会调用外部 biber、需要本机可用且版本匹配"；索引/术语表说"不会自动运行"。
/// 都必须在**成功**路径主动提示（ADR-0015 第 4 条：检出即如实说明、不静默降级、不自动切回别的工作流）。
///
/// ⚠ 判据**不能只看 `.run.xml` 是否存在**（2026-11 review E2 实测）：biblatex 的
/// `backend=bibtex` 也写 `.run.xml`，但里面是 `<binary>bibtex</binary>`、`biber` **0 处**，
/// 参考文献照常生成——只看文件存在就把这类文档误报成"不支持 biber"。上游 Tectonic 自己的判据
/// 也是扫 XML 里的 `<binary>` 文本（`test_file/tectonic-src/src/driver.rs:2147-2167`），看不到
/// `biber` 就不跑 biber。
///
/// 文案只给**本产品内真实可行的出路**：Tectonic 自带 bundle、不依赖 TeX Live，建议
/// "换用完整 TeX Live 工具链"在产品里没有对应操作，等于没给出路。
const EXTERNAL_TOOL_NOTICES: [(&str, &str); 3] = [
    (
        "run.xml",
        "检测到 biber 的构建请求（<stem>.run.xml 里 <binary>biber</binary>）：引擎会调用**外部** biber\
         （不随本产品分发）——本机没有它、或它的版本与文档用的 biblatex 不匹配时，这一步会失败，\
         参考文献为空或编译中断（版本差异见输出里的 `Found biblatex control file version …, expected version …`）。\
         出路：① 参考文献改用内建 bibtex——biblatex 写 \\usepackage[backend=bibtex]{biblatex}，\
         或改用传统的 \\bibliography{...} + \\bibliographystyle{...}（引擎会跑内建 bibtex，改动最小）；\
         ② 留在 biber 上就把本机 biber 升级/对齐到 bundle 内的 biblatex 版本；③ 先接受引用显示为空。",
    ),
    (
        "idx",
        "检测到 makeindex 的索引文件（<stem>.idx）：本引擎不会自动运行 makeindex，生成的 PDF 里索引为空。\
         出路：暂时去掉 \\makeindex/\\printindex（在正文里手工列出条目），或在命令行对 \
         <stem>.idx 跑一次 makeindex 后再编译；也可以接受索引为空。",
    ),
    (
        "glo",
        "检测到 glossaries 的术语表文件（<stem>.glo）：本引擎不会自动运行 makeglossaries，术语表为空。\
         出路：暂时去掉 \\makeglossaries/\\printglossaries，或在命令行跑一次 makeglossaries 后再编译；\
         也可以接受术语表为空。",
    ),
];

/// 编译成功后检查"需要外部工具"的产物，返回要提示的消息（无命中 = 空表）。
async fn external_tool_notices(fs: &dyn FileSystem, tmp_dir: &Path, stem: &str) -> Vec<&'static str> {
    let mut notices = Vec::new();
    for (ext, msg) in EXTERNAL_TOOL_NOTICES {
        let path = tmp_dir.join(format!("{stem}.{ext}"));
        let hit = if ext == "run.xml" {
            // `.run.xml` 的存在只说明 logreq 写了请求文件，**请求哪个工具**要看内容
            run_xml_requests_biber(fs, &path).await
        } else {
            fs.exists(&path).await.unwrap_or(false)
        };
        if hit {
            notices.push(msg);
        }
    }
    notices
}

/// `<stem>.run.xml`（logreq 请求文件）是否**真的**请求了 biber。
///
/// 判据与上游 Tectonic 等价（`driver.rs:2147-2167`）：扫每个 `<binary>` 元素的文本，
/// 等于 `biber` 才算 biber 请求；`bibtex` 就继续看下一个。
/// 读不到 / 没有闭合标签 → **不算**（宁可漏报也不误报：误报会把参考文献已生成的文档说成不支持）。
async fn run_xml_requests_biber(fs: &dyn FileSystem, path: &Path) -> bool {
    match fs.read_to_string_lossy(path).await {
        Ok(text) => run_xml_binary_requests_biber(&text),
        Err(e) => {
            debug!(path = %path.display(), "读 run.xml 失败，按无 biber 请求处理：{e}");
            false
        }
    }
}

/// 纯文本判定（见 [`run_xml_requests_biber`]）：`<binary>biber</binary>`。
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

/// 进程树杀（modules.md §2.6）：
/// Windows：`taskkill /T /F /PID`（Child::kill 只杀直接子进程，子进程会存活）。
/// Unix：先试进程组（-pid），失败再试直接 kill（best-effort，与 LaTeX Workshop 相同的已知局限）。
fn kill_tree(pid: u32) {
    // 阻塞的进程/进程组调用移到阻塞线程池，避免卡住 async 执行器（超时/取消路径的热路径）。
    // 结果 best-effort，丢弃 JoinHandle（仅作后台树杀，不等待）。
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            let _ = crate::proc::command_std("taskkill")
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
    //! 两层用例：
    //! - **不需要外部程序**（默认就跑）：命令构造（argv / `-C` 分档）、路径与 stem、成功收尾的
    //!   原子拷贝、外部工具提示。ADR-0015 之后的命令断言只针对 Tectonic；
    //!   依赖 latexmk / xelatex / xdvipdfmx 的断言随那些路径一起删除了。
    //! - **真实编译**（`#[ignore]`，需要 PATH 上有 `tectonic`）：成功 / 内容错误 / 超时树杀 /
    //!   取消 / Quick 升级 Full / 中文路径 / 流式页进度。
    //!   跑法：`cargo test -p latteset-infra -- --ignored`（没装 tectonic 时用例自行跳过）。

    use super::*;
    use crate::synctex::SyncTexCli;
    use latteset_core::scheduler::NoProgress;
    use latteset_core::synctex::{SourcePosition, SyncTexPosition, SyncTexProvider};
    use std::path::PathBuf;
    use std::time::Duration;

    struct TempProject {
        dir: std::path::PathBuf,
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

    fn tectonic_available() -> bool {
        std::process::Command::new("tectonic")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// 集成用例的超时（默认 60s）。
    ///
    /// 可用 `LATTESET_TEST_TIMEOUT_SECS` 放长：**冷缓存的 Tectonic 首编**要联网取 bundle
    /// （实测约 65 MB，几十秒到几分钟），60s 的夹具直接撞超时。
    fn test_timeout() -> Duration {
        Duration::from_secs(
            std::env::var("LATTESET_TEST_TIMEOUT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(60),
        )
    }

    fn sample_req() -> CompileRequest {
        CompileRequest {
            root_file: PathBuf::from("/proj/css/thesis.tex"),
            project_root: PathBuf::from("/proj"),
            timeout: Duration::from_secs(120),
            kind: CompileKind::Quick,
        }
    }

    fn req(project: &TempProject) -> CompileRequest {
        CompileRequest {
            root_file: project.dir.join("main.tex"),
            project_root: project.dir.clone(),
            timeout: test_timeout(),
            kind: CompileKind::Full,
        }
    }

    /// 同一项目上的 Quick 请求（编辑触发的强度）。
    fn quick_req(project: &TempProject) -> CompileRequest {
        CompileRequest { kind: CompileKind::Quick, ..req(project) }
    }

    fn argv(cmd: &tokio::process::Command) -> Vec<String> {
        cmd.as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    // ---- 命令构造（不需要装 tectonic，任何机器都能跑）----

    /// Tectonic argv 的唯一契约：Quick 两趟（`-r 1`）、Full 用引擎默认趟数（无 `-r`）、
    /// `-C` 只在缓存就绪档出现（冷缓存加它 = 拒绝联网取 bundle，首编必失败）。
    #[test]
    fn tectonic_command_argv_by_kind_and_bundle_policy() {
        let req = sample_req();
        let warm_quick = vec![
            "-C", "-k", "--keep-logs", "--synctex", "-p", "-o", "tmp", "-r", "1", "css/thesis.tex",
        ];
        let warm_full =
            vec!["-C", "-k", "--keep-logs", "--synctex", "-p", "-o", "tmp", "css/thesis.tex"];

        let quick = tectonic_command(&req, true, BundlePolicy::CachedOnly);
        assert_eq!(quick.as_std().get_program().to_string_lossy(), "tectonic");
        assert_eq!(
            argv(&quick),
            warm_quick,
            "Quick（缓存就绪）必须是「离线缓存 + 保留中间产物 + 页进度 + **两趟**」——\
             单趟（`-r 0`）会丢掉整段目录，见 `tectonic_command` 的注释"
        );

        let full = tectonic_command(&req, false, BundlePolicy::CachedOnly);
        assert_eq!(full.as_std().get_program().to_string_lossy(), "tectonic");
        assert_eq!(argv(&full), warm_full, "Full 不传 `-r`（用引擎默认的收敛趟数）");
        assert!(!argv(&full).iter().any(|a| a == "-r"), "Full 不能带 `-r`");

        // 首跑档（缓存没证据）：**不含 `-C`**，其余参数逐一相同。
        // 反例自证：若 `-C` 变回无条件，下面断言必红。
        for (quick_flag, warm) in [(true, warm_quick.clone()), (false, warm_full.clone())] {
            let cold = argv(&tectonic_command(&req, quick_flag, BundlePolicy::AllowFetch));
            let expected: Vec<String> = warm.iter().skip(1).map(|s| (*s).to_owned()).collect();
            assert!(
                !cold.iter().any(|a| a == "-C"),
                "首跑档不得带 `-C`（带了就永远拿不到 bundle）：{cold:?}"
            );
            assert_eq!(cold, expected, "首跑档与就绪档只差一个 `-C`");
        }

        // D4：Tectonic 把 SOURCE_DATE_EPOCH 当**引擎时间源**（设 0 ⇒ 正文 `\today` 印成
        // 1970-01-01）⇒ 绝不设它。XeLaTeX 的"固定 epoch 换 PDF /ID 稳定"那条路径已随引擎删除。
        let has_epoch = |c: &tokio::process::Command| {
            c.as_std()
                .get_envs()
                .any(|(k, v)| k.to_string_lossy() == "SOURCE_DATE_EPOCH" && v.is_some())
        };
        assert!(!has_epoch(&quick), "Tectonic 不能设 SOURCE_DATE_EPOCH");
        assert!(!has_epoch(&full), "Tectonic 不能设 SOURCE_DATE_EPOCH");
    }

    #[test]
    fn root_stem_variants() {
        assert_eq!(root_stem(Path::new(r"C:\proj\main.tex")), "main");
        assert_eq!(root_stem(Path::new("css/thesis.tex")), "thesis");
        assert_eq!(root_stem(Path::new("main")), "main"); // 无扩展名回退
    }

    #[test]
    fn input_path_keeps_nested_relative_path() {
        // H2 回归：嵌套根文件必须用相对项目的完整路径，而非仅 stem（否则在项目根下找不到文件）
        let root = Path::new(r"C:\proj");
        assert_eq!(input_path(Path::new(r"C:\proj\css\thesis.tex"), root), "css/thesis.tex");
        assert_eq!(input_path(Path::new(r"C:\proj\main.tex"), root), "main.tex");
        // 不在项目根下 → 回退到完整路径（统一正斜杠）
        assert_eq!(input_path(Path::new(r"D:\other\a.tex"), root), "D:/other/a.tex");
    }

    #[test]
    fn chinese_path_input_and_stem() {
        // 传给引擎的输入必须是相对项目的正斜杠路径，中文原样保留（不得被转义/丢字）
        let root = Path::new(r"E:\项目\中文测试工程");
        assert_eq!(
            input_path(Path::new(r"E:\项目\中文测试工程\中文主文件.tex"), root),
            "中文主文件.tex"
        );
        assert_eq!(
            input_path(Path::new(r"E:\项目\中文测试工程\章节\第一章.tex"), root),
            "章节/第一章.tex"
        );
        // 产物名沿用 basename 的 stem，中文 stem 需与 pdf_dst 计算一致
        assert_eq!(root_stem(Path::new(r"E:\项目\中文测试工程\中文主文件.tex")), "中文主文件");
    }

    // ---- 成功收尾（纯文件操作）----

    fn runner() -> SubprocessRunner {
        SubprocessRunner::new(Arc::new(crate::fs::TokioFs), Arc::new(NoProgress))
    }

    /// 收尾 = `tmp/<stem>.pdf` 原子拷贝到项目根；页哈希**恒空**（子进程形态没有 XDV）。
    #[tokio::test]
    async fn finish_success_copies_pdf_and_reports_no_page_hashes() {
        let project = TempProject::new("finish-ok");
        let tmp_dir = project.dir.join("tmp");
        std::fs::create_dir_all(&tmp_dir).unwrap();
        std::fs::write(tmp_dir.join("main.pdf"), b"%PDF-1.7 new").unwrap();
        std::fs::write(project.dir.join("main.pdf"), b"old bytes").unwrap();

        let pdf_dst = project.dir.join("main.pdf");
        let out = runner()
            .finish_success(&tmp_dir, "main", CompileKind::Full, &pdf_dst)
            .await
            .expect("拷贝应成功");
        let CompileOutcome::Success { pdf_path, kind, page_hashes } = out else {
            panic!("预期成功：{out:?}");
        };
        assert_eq!(pdf_path, pdf_dst);
        assert_eq!(kind, CompileKind::Full, "实际强度要如实回报");
        assert!(
            page_hashes.is_empty(),
            "子进程形态的 Tectonic 不落 .xdv ⇒ 页哈希必须为空（前端按 pages=0 全量刷新）"
        );
        assert_eq!(std::fs::read(&pdf_dst).unwrap(), b"%PDF-1.7 new", "项目根应是本次产物");
        assert!(!project.dir.join("main.pdf.tmp").exists(), "临时文件应已被 rename 掉");
    }

    /// 拷贝失败时**旧 PDF 必须原样保留**（原子替换的意义：不能让用户手里只剩半份文件）。
    #[tokio::test]
    async fn finish_success_keeps_old_pdf_when_source_missing() {
        let project = TempProject::new("finish-fail");
        let tmp_dir = project.dir.join("tmp");
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let pdf_dst = project.dir.join("main.pdf");
        std::fs::write(&pdf_dst, b"old bytes").unwrap();

        let err = runner()
            .finish_success(&tmp_dir, "main", CompileKind::Quick, &pdf_dst)
            .await
            .expect_err("源 PDF 不存在 ⇒ 必须报错（不能静默成功）");
        assert!(err.contains("PDF 拷贝失败"), "{err}");
        assert_eq!(std::fs::read(&pdf_dst).unwrap(), b"old bytes", "旧 PDF 不得被截断");
    }

    /// logreq 请求文件的真实形态（`test_file/review/probe-biBtex/tmp/main.run.xml` 摘录）：
    /// biblatex 会同时写 internal 段与 external 段，**biber 与 bibtex 的区别只在 `<binary>` 的文本**。
    fn run_xml_with_binary(binary: &str) -> String {
        format!(
            r#"<?xml version="1.0" standalone="yes"?>
<!DOCTYPE requests [
  <!ELEMENT requests (internal | external)*>
  <!ELEMENT internal (generic, (provides | requires)*)>
  <!ELEMENT external (generic, cmdline?, input?, output?, (provides | requires)*)>
  <!ELEMENT cmdline (binary, (option | infile | outfile)*)>
  <!ELEMENT generic (#PCDATA)>
  <!ELEMENT binary (#PCDATA)>
]>
<requests version="1.0">
  <internal package="biblatex" priority="9" active="1">
    <generic>latex</generic>
  </internal>
  <external package="biblatex" priority="5" active="0">
    <generic>{binary}</generic>
    <cmdline>
      <binary>{binary}</binary>
      <option>-min-crossrefs 2</option>
      <infile>main</infile>
    </cmdline>
  </external>
</requests>
"#
        )
    }

    /// E2 回归：`backend=bibtex` 的 `.run.xml` **不得**报 biber；真 biber 请求恰好报一条。
    ///
    /// 误报的实际代价（review E2 端到端）：`backend=bibtex` 的文档已经生成了 `main.bbl`（参考文献齐全），
    /// 用户却看到"本引擎不支持 biber、参考文献不会生成"——既误导又推着用户去改本来正确的设置。
    #[tokio::test]
    async fn biber_notice_requires_a_biber_request_in_run_xml() {
        let project = TempProject::new("biber-request");
        let tmp_dir = project.dir.join("tmp");
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let fs = crate::fs::TokioFs;
        let run_xml = tmp_dir.join("main.run.xml");

        // ① `backend=bibtex` 的真实产物：`.run.xml` 在，但 `<binary>` 是 bibtex
        std::fs::write(&run_xml, run_xml_with_binary("bibtex")).unwrap();
        let notices = external_tool_notices(&fs, &tmp_dir, "main").await;
        assert!(
            notices.iter().all(|n| !n.contains("biber")),
            "backend=bibtex 被误报成 biber：{notices:?}"
        );

        // ② 真 biber 夹具（真实 run.xml 的形态：`<binary>biber</binary>`）→ 恰好一条
        std::fs::write(&run_xml, run_xml_with_binary("biber")).unwrap();
        let notices = external_tool_notices(&fs, &tmp_dir, "main").await;
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(notices[0].contains("biber"), "{}", notices[0]);
        assert!(
            notices[0].contains("backend=bibtex"),
            "必须给出本产品可行的出路（改成 backend=bibtex）：{}",
            notices[0]
        );
        // 事实口径（2026-11 验收）：Tectonic 0.17 **会调用外部 biber**（`note: Running external tool biber`），
        // 所以不能说"本产品不自带/不支持/不会自动跑"——真实原因是缺 biber 或版本与 biblatex 不匹配。
        assert!(
            notices[0].contains("外部"),
            "文案必须说清 biber 是外部可执行文件、由引擎调用：{}",
            notices[0]
        );
        assert!(
            notices[0].contains("版本"),
            "文案必须说清要版本匹配：{}",
            notices[0]
        );
        for wrong in ["不支持", "不会自动跑", "不自带"] {
            assert!(
                !notices[0].contains(wrong),
                "biber 会被引擎调用，不能说「{wrong}」：{}",
                notices[0]
            );
        }

        // ③ 判定函数本身：只见 `<binary>bibtex</binary>` → false；`biber` → true
        assert!(!run_xml_binary_requests_biber(&run_xml_with_binary("bibtex")));
        assert!(run_xml_binary_requests_biber(&run_xml_with_binary("biber")));
        assert!(
            !run_xml_binary_requests_biber("<requests><internal package=\"biblatex\"/></requests>"),
            "没有 external/cmdline 段（例如只有 latex 内部步骤）不算 biber 请求"
        );
    }

    /// 成功路径的外部工具提示：三类产物各一条，顺序固定；别的 stem 不算。
    #[tokio::test]
    async fn external_tool_notices_detect_biber_index_and_glossary() {
        let project = TempProject::new("external-tools");
        let tmp_dir = project.dir.join("tmp");
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let fs = crate::fs::TokioFs;

        assert!(
            external_tool_notices(&fs, &tmp_dir, "main").await.is_empty(),
            "干净项目不该有提示"
        );

        std::fs::write(tmp_dir.join("main.run.xml"), run_xml_with_binary("biber")).unwrap();
        std::fs::write(tmp_dir.join("main.idx"), "x").unwrap();
        std::fs::write(tmp_dir.join("main.glo"), "x").unwrap();
        std::fs::write(tmp_dir.join("other.run.xml"), run_xml_with_binary("biber")).unwrap(); // 别的 stem 不算

        let notices = external_tool_notices(&fs, &tmp_dir, "main").await;
        assert_eq!(notices.len(), 3, "{notices:?}");
        assert!(notices[0].contains("biber"), "{}", notices[0]);
        assert!(notices[1].contains("makeindex"), "{}", notices[1]);
        assert!(notices[2].contains("makeglossaries"), "{}", notices[2]);
        // 口径区分（2026-11 验收）：biber 由引擎调用（外部可执行），索引/术语表则不自动运行
        assert!(
            notices[1].contains("不会自动运行") && notices[2].contains("不会自动运行"),
            "makeindex/glossaries 确实不会被自动跑，文案要如实这么写：{:?}",
            &notices[1..]
        );
        for n in &notices {
            assert!(
                !n.contains("TeX Live"),
                "Tectonic 自带 bundle：别把「换完整 TeX Live 工具链」当出路（产品里没法操作）：{n}"
            );
            assert!(
                n.contains("（<stem>") || n.contains("<stem>.idx") || n.contains("<stem>.glo"),
                "要点出是哪个文件：{n}"
            );
        }
    }

    // ---- 超时诊断的证据通道（review E4）----

    /// `.log` 零字节时（Tectonic 的日志收尾才落盘，被树杀时这就是常态），证据必须来自
    /// **已捕获的 stdout/stderr**：有 `[1]` 页标记 ⇒ "在推进，只是比上限慢"，
    /// 而不是"日志里还没出现任何页面输出/可能卡住"。
    #[tokio::test]
    async fn timeout_evidence_uses_captured_output_when_log_is_empty() {
        let project = TempProject::new("timeout-output");
        project.put("main.tex", "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n");
        let tmp_dir = project.dir.join("tmp");
        std::fs::create_dir_all(&tmp_dir).unwrap();
        assert!(!tmp_dir.join("main.log").exists(), "前提：超时杀进程时 .log 还没落盘");

        let tail = "note: Running TeX ...\n[1]\nnote: running xdvipdfmx ...\n";
        let entry = runner()
            .timeout_entry(&req(&project), "main", &tmp_dir, false, tail, Some(1))
            .await;

        let diag = entry.diagnosis.expect("超时条目必须带诊断");
        assert_eq!(
            diag.kind,
            DiagnosisKind::CompileTimeout,
            "有页证据就该判定为「慢」而不是「可能卡住」：{}",
            diag.cause
        );
        assert_eq!(diag.suggested_timeout_secs, Some(900), "首编跳档到 900s");
        assert!(entry.message.contains("第 1 页"), "message 要带上证据：{}", entry.message);
        assert!(!entry.message.contains("无页输出"), "{}", entry.message);
    }

    /// `.log` 有内容时**合并**（不是二选一）：页证据取两者最大值；日志里的致命错误仍然优先
    /// （那时超时只是症状，提高超时救不了 ⇒ 不给一键提高按钮）。
    #[tokio::test]
    async fn timeout_evidence_merges_log_and_captured_output() {
        let project = TempProject::new("timeout-merge");
        project.put("main.tex", "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n");
        let tmp_dir = project.dir.join("tmp");
        std::fs::create_dir_all(&tmp_dir).unwrap();

        std::fs::write(tmp_dir.join("main.log"), "No file main.aux.\n[3]\n").unwrap();
        let entry = runner()
            .timeout_entry(&req(&project), "main", &tmp_dir, true, "[1]\n", None)
            .await;
        assert!(
            entry.message.contains("第 3 页"),
            "日志里的第 3 页不能被输出的第 1 页盖掉：{}",
            entry.message
        );
        assert!(entry.message.contains("增量编译"), "{}", entry.message);

        // 日志自带致命错误 → 超时只是症状
        std::fs::write(
            tmp_dir.join("main.log"),
            "! I can't write on file `body/introduction.aux'.\nl.12 x\n",
        )
        .unwrap();
        let entry = runner()
            .timeout_entry(&req(&project), "main", &tmp_dir, true, "[2]\n", None)
            .await;
        let diag = entry.diagnosis.expect("必须带诊断");
        assert_eq!(
            diag.suggested_timeout_secs, None,
            "已有致命错误时提高超时救不了，不能给一键按钮：{}",
            diag.cause
        );
        assert!(diag.cause.contains("早已失败"), "{}", diag.cause);
    }

    /// 输出尾部去噪：去掉 ANSI 序列 / `\r` / 空行，但**保留** `[N]` 页标记与错误文本（它们是证据）。
    #[test]
    fn output_tail_denoise_keeps_evidence() {
        let raw = "\u{1b}[2Knote: running\n\r[1]\r\n\n\u{1b}[32m! Undefined control sequence.\u{1b}[0m\n";
        let cleaned = denoise_output_tail(raw);
        assert!(!cleaned.contains('\u{1b}'), "{cleaned:?}");
        assert!(!cleaned.contains('\r'), "{cleaned:?}");
        assert!(cleaned.contains("[1]"), "{cleaned:?}");
        assert!(cleaned.contains("! Undefined control sequence."), "{cleaned:?}");
        assert!(!cleaned.contains("\n\n"), "空行应被丢掉：{cleaned:?}");
    }

    /// 反馈状态机把流式页标记记成 `max_page`：超时证据不依赖尾部窗口大小。
    #[test]
    fn feedback_records_observed_pages_for_timeout_evidence() {
        let mut fb = LiveFeedback::new(Arc::new(NoProgress));
        assert_eq!(fb.evidence_snapshot(1 << 10).1, None, "还没输出就没有页证据");
        fb.feed("note: Running TeX ...\n[1]\n[2]\n", false);
        let (tail, pages) = fb.evidence_snapshot(1 << 10);
        assert!(tail.contains("[2]"), "{tail:?}");
        assert_eq!(pages, Some(2));
        // 尾部窗口小到截掉页标记，页证据仍在（这正是超时路径要的兜底）
        let (tiny, pages) = fb.evidence_snapshot(2);
        assert!(!tiny.contains('['), "前提：窗口确实截掉了标记：{tiny:?}");
        assert_eq!(pages, Some(2));
    }

    /// 证据文本合并规则：`.log` 优先，空日志才完全依赖输出；流式观测到的页码作兜底。
    #[test]
    fn timeout_evidence_text_prefers_log_when_present() {
        assert_eq!(timeout_evidence_text("", "[1]\n", None), "[1]\n");
        assert_eq!(timeout_evidence_text("[3]\n", "", None), "[3]\n");
        let merged = timeout_evidence_text("log-line\n", "[1]\n", None);
        assert!(merged.starts_with("log-line"), "{merged:?}");
        assert!(merged.contains("[1]"), "{merged:?}");
        let merged = timeout_evidence_text("log-without-newline", "[1]\n", None);
        assert!(merged.contains("log-without-newline\n[1]"), "{merged:?}");

        // 兜底：尾部没截到 `[N]`（树杀竞态）但流式通道确认过页码 → 补一行标记
        let rescued = timeout_evidence_text("", "note: running\n", Some(2));
        assert_eq!(pages_typeset(&rescued), Some(2), "{rescued:?}");
        // 文本里已经有更大的页 → 不重复补
        let kept = timeout_evidence_text("[9]\n", "", Some(2));
        assert_eq!(pages_typeset(&kept), Some(9), "{kept:?}");
    }

    // ---- 真实编译（#[ignore]：需要 PATH 上有 tectonic）----

    /// Tectonic 端到端：走**完整 runner 路径**跑一次中文文档，断言"出 PDF + **无页哈希**"——
    /// Tectonic 的 PDF 档永不落 `.xdv`，页级复用 A/B/C 在该形态下不可用（下游 `pages=0` 全量刷新）。
    #[tokio::test]
    #[ignore]
    async fn tectonic_compiles_cjk_pdf_without_page_hashes() {
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("tectonic-e2e");
        project.put(
            "main.tex",
            "\\documentclass[12pt]{ctexart}\n\\begin{document}\n你好，世界。Tectonic 端到端用例。\n\\end{document}\n",
        );
        let out = runner().compile(req(&project), CancellationToken::new()).await;
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

    /// 成功路径：编译 → PDF 拷贝到项目根 + tmp/ 中间文件 + SyncTeX 双向可用。
    #[tokio::test]
    #[ignore]
    async fn compile_success_and_synctex() {
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("success");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nHello Latteset\n\\end{document}\n",
        );

        let outcome = runner().compile(req(&project), CancellationToken::new()).await;
        let CompileOutcome::Success { pdf_path, .. } = outcome else {
            panic!("预期成功，得到：{outcome:?}");
        };
        assert_eq!(pdf_path, project.dir.join("main.pdf"));
        assert!(pdf_path.exists(), "PDF 应拷贝到项目根");
        assert!(project.dir.join("tmp/main.log").exists(), "中间文件应收纳在 tmp/（--keep-logs）");
        assert!(
            project.dir.join("tmp/main.synctex.gz").exists(),
            "--synctex 应产出 synctex 数据"
        );

        // SyncTeX CLI 输出契约实测（ADR-0008 风险落地）——它仍走外部 synctex.exe
        if std::process::Command::new("synctex").arg("--version").output().is_err() {
            eprintln!("跳过 SyncTeX 断言：未安装 synctex");
            return;
        }
        let cli = SyncTexCli;
        let pos = cli
            .forward(
                &SourcePosition { file: project.dir.join("main.tex"), line: 3, column: 1 },
                &pdf_path,
            )
            .await
            .expect("正向定位应成功");
        assert!(pos.page >= 1);

        let back = cli
            .inverse(&SyncTexPosition { page: pos.page, x: pos.x, y: pos.y }, &pdf_path)
            .await
            .expect("反向定位应成功");
        assert!(back.line >= 1);
    }

    /// Quick 路径（roadmap ㉘）：已有构建产物时跑 Tectonic 两趟，`Success{kind}` 必须报 Quick。
    ///
    /// 断言「真的跑了 Quick 档」而非「请求被接受」：先 Full 建产物（含 .aux/.toc），
    /// 再改正文源码走 Quick → 必须报 `kind: Quick`，且 PDF/SyncTeX 齐全
    /// （后续编辑期的双向定位依赖它）。
    #[tokio::test]
    #[ignore]
    async fn quick_path_reports_quick_after_full() {
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("quick");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\tableofcontents\n\\section{One}\\label{s:one}\nSee \\ref{s:one}.\n\\end{document}\n",
        );
        let cancel = || CancellationToken::new();

        // 1) 首编 = Full（建 tmp/main.aux，Quick 的前置条件）
        let first = runner().compile(req(&project), cancel()).await;
        assert!(
            matches!(first, CompileOutcome::Success { kind: CompileKind::Full, .. }),
            "首编应为 Full，实得：{first:?}"
        );
        assert!(project.dir.join("tmp/main.aux").exists(), "Full 应产出 .aux");

        // 2) 编辑正文 → Quick
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\tableofcontents\n\\section{One}\\label{s:one}\nSee \\ref{s:one}.\nEdited.\n\\end{document}\n",
        );
        let second = runner().compile(quick_req(&project), cancel()).await;
        assert!(
            matches!(second, CompileOutcome::Success { kind: CompileKind::Quick, .. }),
            "有产物时编辑触发应跑 Quick 档并如实回报，实得：{second:?}"
        );
        assert!(project.dir.join("main.pdf").exists(), "Quick 也应产出 PDF");
        assert!(
            project.dir.join("tmp/main.synctex.gz").exists(),
            "Quick 也应产出 synctex（定位不能因强度降级而失效）"
        );
    }

    /// Quick 前置条件缺失（无 tmp/<stem>.aux）→ runner 自动升级 Full，避免引用全是 `??`。
    #[tokio::test]
    #[ignore]
    async fn quick_upgrades_to_full_without_artifacts() {
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("quick-upgrade");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\tableofcontents\n\\section{One}\\label{s:one}\nSee \\ref{s:one}.\n\\end{document}\n",
        );
        let mut quick = req(&project);
        quick.kind = CompileKind::Quick;
        let outcome = runner().compile(quick, CancellationToken::new()).await;

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
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("error");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\n\\undefinedcommandhere\n\\end{document}\n",
        );

        let outcome = runner().compile(req(&project), CancellationToken::new()).await;
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
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("timeout");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nBig\n\\end{document}\n",
        );

        let mut request = req(&project);
        request.timeout = Duration::from_millis(1); // 必超时
        let outcome = runner().compile(request, CancellationToken::new()).await;

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
            }
            other => panic!("预期超时，得到：{other:?}"),
        }
    }

    /// E2 端到端回归（review 夹具 `probe-biBtex`）：`biblatex[backend=bibtex]` **编译成功且
    /// 参考文献已生成**时，不得出现任何 biber 提示。
    ///
    /// 这正是旧判据（只看 `.run.xml` 存在）的误报现场：`.run.xml` 确实存在（logreq 为
    /// biblatex 写的请求文件），但里面是 `<binary>bibtex</binary>`。
    #[tokio::test]
    #[ignore]
    async fn bibtex_backend_success_emits_no_biber_notice() {
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("bibtex-notice");
        project.put(
            "refs.bib",
            "@book{knuth, author={Donald Knuth}, title={The TeXbook}, year={1984}, publisher={AW}}\n",
        );
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\usepackage[backend=bibtex]{biblatex}\n\
             \\addbibresource{refs.bib}\n\\begin{document}\nCite: \\cite{knuth}.\n\
             \\printbibliography\n\\end{document}\n",
        );
        let progress = std::sync::Arc::new(RecordingProgress::new());
        let runner =
            SubprocessRunner::new(std::sync::Arc::new(crate::fs::TokioFs), progress.clone());
        let outcome = runner.compile(req(&project), CancellationToken::new()).await;

        assert!(
            matches!(outcome, CompileOutcome::Success { .. }),
            "backend=bibtex 应编译成功：{outcome:?}"
        );
        let bbl = std::fs::metadata(project.dir.join("tmp/main.bbl"))
            .map(|m| m.len())
            .unwrap_or(0);
        assert!(bbl > 0, "夹具前提：bibtex 后端已生成参考文献（.bbl）");
        assert!(
            project.dir.join("tmp/main.run.xml").exists(),
            "夹具前提：`.run.xml` 存在——旧判据的误报来源"
        );

        let notices = progress.notices.lock().unwrap().clone();
        assert!(
            notices.iter().all(|n| !n.contains("biber")),
            "backend=bibtex 被误报为「不支持 biber」：{notices:?}"
        );
    }

    /// E4 端到端回归（review 夹具 `timeout-pages`）：第 1 页之后是长 TeX 循环 ⇒ 超时被杀时
    /// `tmp/*.log` 还没落盘，诊断必须用上 stdout 里已经出现的 `[1]` 页证据。
    #[tokio::test]
    #[ignore]
    async fn timeout_diagnosis_uses_streamed_page_evidence() {
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("timeout-pages");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nPage one.\n\\newpage\n\
             \\newcount\\n \\n=0\n\\loop\\ifnum\\n<80000000 \\advance\\n by 1 \\repeat\n\
             \\newpage\nslow done\n\\end{document}\n",
        );
        let progress = std::sync::Arc::new(RecordingProgress::new());
        let runner =
            SubprocessRunner::new(std::sync::Arc::new(crate::fs::TokioFs), progress.clone());
        let mut request = req(&project);
        // 够 Tectonic 排完第 1 页（慢机器上仍留余量），又远小于那条循环的耗时
        request.timeout = Duration::from_secs(8);
        let outcome = runner.compile(request, CancellationToken::new()).await;

        let CompileOutcome::Timeout { entry } = outcome else {
            panic!("预期超时，得到：{outcome:?}");
        };
        let streamed = progress.pages.lock().unwrap().clone();
        assert!(!streamed.is_empty(), "夹具前提：stdout 已报出页进度");
        assert!(
            !project.dir.join("tmp/main.log").exists(),
            "夹具前提：超时被树杀时 .log 还没落盘（这正是旧证据通道失效的原因）"
        );

        let diag = entry.diagnosis.expect("超时必须带诊断");
        assert_eq!(
            diag.kind,
            DiagnosisKind::CompileTimeout,
            "诊断必须用上「已排版 N 页」这条内存证据，而不是说日志里没有页输出：{}",
            diag.cause
        );
        assert_ne!(
            diag.suggested_timeout_secs, None,
            "在推进只是慢 ⇒ 应给一键提高超时的建议：{}",
            diag.cause
        );
        assert!(
            entry.message.contains("第 1 页") || entry.message.contains("第 2 页"),
            "证据事实要写进 message：{}",
            entry.message
        );
    }

    /// 取消路径：提前取消 → Aborted。
    #[tokio::test]
    #[ignore]
    async fn compile_cancel_aborts() {
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("cancel");
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nBig\n\\end{document}\n",
        );

        let cancel = CancellationToken::new();
        cancel.cancel(); // 立即取消
        let outcome = runner().compile(req(&project), cancel).await;
        assert_eq!(outcome, CompileOutcome::Aborted);
    }

    /// 流式反馈记录器（阶段 2）：记下每次回调的**时刻**，好断言"编译结束前就到了"。
    struct RecordingProgress {
        start: Instant,
        pages: std::sync::Mutex<Vec<(u32, Duration)>>,
        errors: std::sync::Mutex<Vec<(usize, Duration)>>,
        /// 能力边界提示（`CompileProgress::notice`）：E2 回归要断言"backend=bibtex 0 条"。
        notices: std::sync::Mutex<Vec<String>>,
    }

    impl RecordingProgress {
        fn new() -> Self {
            Self {
                start: Instant::now(),
                pages: std::sync::Mutex::new(Vec::new()),
                errors: std::sync::Mutex::new(Vec::new()),
                notices: std::sync::Mutex::new(Vec::new()),
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
        fn notice(&self, msg: &str) {
            self.notices.lock().unwrap().push(msg.to_owned());
        }
    }

    /// 流式页进度（阶段 2）：真实编译期间就该报上"已排版 N 页"，而不是等编译结束。
    ///
    /// Tectonic 的页进度通道是 `-p` 打在 **stdout 的 `[N]`**；终态不依赖它。
    #[tokio::test]
    #[ignore]
    async fn reports_streaming_pages_before_compile_finishes() {
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("stream-pages");
        // 三页文档：页标记必须出现多次，且最后一次为 3
        project.put(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nFirst\n\\newpage\nSecond\n\\newpage\nThird\n\\end{document}\n",
        );
        let progress = std::sync::Arc::new(RecordingProgress::new());
        let runner = SubprocessRunner::new(std::sync::Arc::new(crate::fs::TokioFs), progress.clone());
        let started = Instant::now();
        let outcome = runner.compile(req(&project), CancellationToken::new()).await;
        let total = started.elapsed();
        assert!(matches!(outcome, CompileOutcome::Success { .. }), "预期成功：{outcome:?}");

        let events = progress.pages.lock().unwrap().clone();
        let seen: Vec<u32> = events.iter().map(|(p, _)| *p).collect();
        assert!(!events.is_empty(), "流式页进度一次都没上报（stdout 管道没接上？）");
        assert_eq!(seen.iter().max().copied(), Some(3), "最大页码应为 3：{seen:?}");
        // 引擎可能跑多趟（每趟页码重启）：`PageMarkerScanner` 只升不降，故前端展示的 max 不会回跳。
        assert!(seen.windows(2).all(|w| w[1] >= w[0]), "页码必须单调递增：{seen:?}");
        // 关键断言：进度**在编译结束前**就到了（不是收尾时才补发）
        let (_, first_at) = events[0];
        let lead = total.saturating_sub(first_at);
        assert!(
            lead >= Duration::from_millis(100),
            "首个页进度距编译结束只有 {lead:?}（总时长 {total:?}）——像是收尾才发的"
        );
    }

    /// 中文路径全链路：中文目录 + 中文文件名 + 中文子目录。
    #[tokio::test]
    #[ignore]
    async fn compile_chinese_paths_with_synctex() {
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        // 目录名含中文（temp_dir()/latteset-it-中文目录-<pid>）
        let project = TempProject::new("中文目录");
        project.put(
            "中文主文件.tex",
            "\\documentclass[UTF8]{ctexart}\n\\begin{document}\n\\include{章节/第一章}\n\\end{document}\n",
        );
        project.put("章节/第一章.tex", "\\section{子文件章节}\n中文正文。\n");

        let request = CompileRequest {
            root_file: project.dir.join("中文主文件.tex"),
            project_root: project.dir.clone(),
            timeout: test_timeout(),
            kind: CompileKind::Full,
        };
        let outcome = runner().compile(request, CancellationToken::new()).await;

        let pdf_path = match outcome {
            CompileOutcome::Success { pdf_path, .. } => pdf_path,
            other => panic!("中文路径应编译成功，实得：{other:?}"),
        };
        assert_eq!(pdf_path, project.dir.join("中文主文件.pdf"));
        assert!(pdf_path.exists(), "PDF 应拷贝到项目根（中文名）");
        assert!(project.dir.join("tmp/中文主文件.log").exists(), "中文名的 .log 应落在 tmp/");
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
                &SourcePosition { file: project.dir.join("章节/第一章.tex"), line: 1, column: 1 },
                &pdf_path,
            )
            .await
            .expect("中文路径正向定位应成功");
        assert!(pos.page >= 1);

        // 反向：PDF → 源码，回传的**中文文件名必须完好**（这是跳回源码的判据）
        let back = cli
            .inverse(&SyncTexPosition { page: pos.page, x: pos.x, y: pos.y }, &pdf_path)
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
        if !tectonic_available() {
            eprintln!("跳过：未安装 tectonic（或不在 PATH 上）");
            return;
        }
        let project = TempProject::new("中文错误");
        project.put(
            "中文主文件.tex",
            "\\documentclass{article}\n\\begin{document}\n\\undefinedcommandhere\n\\end{document}\n",
        );

        // 必须编译中文根文件本身：req() 造的是 main.tex，项目里并不存在，
        // 引擎会因缺少输入而无 .log 可解析（2026-09 修正过这条用例）。
        let request = CompileRequest {
            root_file: project.dir.join("中文主文件.tex"),
            project_root: project.dir.clone(),
            timeout: test_timeout(),
            kind: CompileKind::Full,
        };
        let outcome = runner().compile(request, CancellationToken::new()).await;

        match outcome {
            CompileOutcome::ContentError { errors } => {
                assert!(!errors.is_empty(), "应有解析出的错误条目");
                let with_line = errors.iter().find(|e| e.line.is_some()).expect("错误应带行号");
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
