//! 自持输出层：`IoProvider` 适配器（方案 §3.3 的 `TectonicIo` / §3.4 第 1 步）。
//!
//! 三层输入解析顺序（方案 §5.4）：**内存直喂**（主文件、以及我们生成的中间产物）→
//! **项目磁盘**（`\input` 的子文件、图片等真实文件）→ **bundle**（宏包/字体/cmap）。
//! 输出一律先落到本层的捕获表（`IoCapture`），再由 runner 决定写哪些到 `tmp/`。
//!
//! **ADR-0010 例外（X-5）**：`IoProvider` 是**同步** trait（`tectonic_io_base` 的
//! `crates/io_base/src/lib.rs:433-533`），而 core 的 `FileSystem` 是 **async**
//! （`crates/latteset-core/src/project/fs.rs:15-16`）⇒ 在本层里无法调用 `FileSystem`。
//! 因此本 crate 直接使用 `std::fs`（只在自己的 `spawn_blocking` 线程里、只碰
//! `project_root` 与 `tmp/`），这正是方案 §4.2 X-5 登记的"第二个落点"。
//!
//! **两处 2026-09-15 的修正（t24，来自 t23 复审的 V-02 / V-06）**：
//! - **format 缓存名以 bundle digest 为键**（[`format_file_name`]）：先前是 `{stem}.fmt` 裸名，
//!   **换 bundle 会复用旧 format**（正确性风险）；现在口径与上游 `FormatCache::path_for_format`
//!   （`src/io/format_cache.rs:42-61`）一致，取不到 digest 时**显式报错**、不退回裸名。
//! - **协作式取消**（[`CANCEL_MESSAGE`]）：库形态没有进程可树杀，但取消置位后**每个 I/O 入口**
//!   立刻返回错误 ⇒ 引擎在**下一个回调**处中止，而不是跑完当前趟（LIB-9 的"下个回调报错"口径）。

use std::collections::HashMap;
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tectonic_bundles::Bundle;
use tectonic_io_base::{InputHandle, InputOrigin, IoProvider, OpenResult, OutputHandle};
use tectonic_status_base::StatusBackend;

use crate::bundle::FORMAT_SERIAL;

/// 块粒度（方案 §3.4 第 8 步 / t5 §4.2）：输出写句柄按这个粒度通知观察者。
///
/// 上游 C 侧 `DVI_BUF_SIZE = 16384` 才 `dvi_swap`（`xetex-shipout.c:11/67-77`），
/// `ttstub_output_flush` 的 9 个调用点参数全是 `rust_stdout` ⇒ XDV 句柄**从不** flush。
/// 所以逐页事件只能靠我们**自己的写句柄**在每块数据到达时累计（t5 §4.2 实测：
/// 16 KB 时已完成 4 页、243 KB 时 26 页）。
pub const OBSERVE_CHUNK: usize = 16 * 1024;

/// 取消消息（V-06 / LIB-9）：置位后 I/O 入口立刻用它报错，引擎在**下一个回调**处中止。
pub const CANCEL_MESSAGE: &str =
    "Latteset：库形态编译已取消 —— 在 I/O 回调处协作式中止（库形态没有进程可树杀）";

/// format 趟传给 `TexEngine::process` 的 **format 名占位串**（上游同值：`src/driver.rs:1802`）。
///
/// 上游注释点名了它是**占位**：dump 出来的字节由 driver 自己收走、再按 format 的 **stem**
/// 写进 format 缓存（`driver.rs:1836` 的 "intentionally pass 'stem', not 'name'"）。
///
/// **实测补充（2026-09-15，本机）**：引擎真正 dump 出的文件名取自 `input_file_name`
/// （我们传 `"texput"` ⇒ 文件叫 `texput.fmt`），**不是**这个占位串 ⇒ 收集时必须按 `*.fmt`
/// 后缀匹配，不能按名字匹配（`runner.rs` 的 `take_dumped_format` 就是这么做的）。
pub const FORMAT_DUMP_PLACEHOLDER: &str = "UNUSED.fmt";

/// format 趟的**合成主输入**（上游 `BridgeState::enter_format_mode`，`src/driver.rs:263-267`：
/// `format!("\\input {format_file_name}")`）。
///
/// 为什么不能拿用户文档当 initex 趟的主输入：initex 趟的产物是 **format**，它的输入必须是
/// bundle 里的 format 源（`tectonic-format-latex.tex` → `\input xelatex.ini`）。喂用户文档
/// 会得到一个没有 LaTeX 宏的"format"（实测症状：排版趟报 `! Undefined control sequence.
/// \documentclass`，且缓存目录里没有 `.fmt`）。
pub fn format_primary_source(format_name: &str) -> String {
    let stem = format_name.split('.').next().unwrap_or(format_name);
    format!("\\input tectonic-format-{stem}.tex")
}

/// 本层捕获到的所有输出（名字 → 字节）与状态。
///
/// 用 `Arc<Mutex<..>>` 共享而不是由 driver 独占：runner 在引擎跑完后要取回这些字节，
/// 观察者（P5 的逐块解析）也要能在引擎**运行中**读同一份缓冲。
#[derive(Debug, Default)]
pub struct IoCapture {
    /// 输出文件：名字与引擎请求的完全一致（`main.pdf` / `main.log` / `main.synctex.gz` …）。
    pub files: HashMap<String, Vec<u8>>,
    /// 已写字节总数（按输出名累计）：观察者与证据都用它。
    pub written: HashMap<String, usize>,
    /// 逐块通知（`name`, 累计字节）：P5 的 `XdvParser::parse(chunk)` 消费它。
    pub chunks: Vec<(String, usize)>,
}

impl IoCapture {
    pub fn bytes(&self, name: &str) -> Option<&[u8]> {
        self.files.get(name).map(|v| v.as_slice())
    }
}

pub type SharedCapture = Arc<Mutex<IoCapture>>;

pub fn new_capture() -> SharedCapture {
    Arc::new(Mutex::new(IoCapture::default()))
}

/// format 缓存文件名（上游口径，V-02）。
///
/// 上游 `FormatCache::path_for_format` 是 `format!("{}-{}-{}.fmt", bundle_digest, name, FORMAT_SERIAL)`
/// （`test_file/tectonic-src/src/io/format_cache.rs:55-58`），本机真实缓存里也能看到同形态的
/// `6ffe0558…-latex-33.fmt` ⇒ 我们照抄该口径：**digest 是缓存键**，换 bundle 自然换文件名、
/// 不会复用旧 format。
///
/// 注：t23 复审给的简写是 `{digest}-{FORMAT_SERIAL}.fmt`，少了中间的 format 名；**以上游源码与
/// 真实缓存文件为准**（本节两条出处都可核）。
pub fn format_file_name(bundle_digest: &str, format_name: &str) -> String {
    let stem = format_name.split('.').next().unwrap_or(format_name);
    format!("{bundle_digest}-{stem}-{FORMAT_SERIAL}.fmt")
}

/// format 缓存**路径**（两条硬约束的唯一落点；[`TectonicIo::format_path`] 与 runner 的
/// "收集 dump 并落盘"共用它，避免两处各拼一次名字）。
///
/// ① 未注入缓存目录 ⇒ 报错（方案 §5.2 硬约束：format 不得落项目目录）；
/// ② 取不到 bundle digest ⇒ **报错**，不得退回裸名 —— 裸名会让"换 bundle"复用旧 format
///    （正确性风险，t23 的 V-02）。
pub fn format_cache_path(
    cache_dir: Option<&Path>,
    bundle_digest: Option<&str>,
    format_name: &str,
) -> Result<PathBuf, String> {
    let dir = cache_dir.ok_or_else(|| {
        "库形态未注入 format 缓存目录（方案 §5.2 硬约束：format 必须落产品缓存目录，\
         不得落项目目录）"
            .to_owned()
    })?;
    let digest = bundle_digest.ok_or_else(|| {
        "拿不到 bundle digest（`Bundle::get_digest()` 未取到）⇒ 拒绝用裸名 format 缓存：\
         裸名会在换 bundle 时复用旧 format（V-02）"
            .to_owned()
    })?;
    Ok(dir.join(format_file_name(digest, format_name)))
}

/// 写句柄：写进共享捕获表；`mirror`（若给定）同时**追加**写到磁盘上的同一路径。
struct CaptureWriter {
    shared: SharedCapture,
    name: String,
    mirror: Option<std::fs::File>,
}

impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        {
            let mut c = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            c.files.entry(self.name.clone()).or_default().extend_from_slice(buf);
            let total = c.files.get(&self.name).map_or(0, Vec::len);
            c.written.insert(self.name.clone(), total);
            // 每累积到一块就记一条（观察者据此在**运行中**解析 XDV）
            let last = c.chunks.iter().rev().find(|(n, _)| n == &self.name).map(|(_, l)| *l);
            if last.map_or(true, |l| total >= l + OBSERVE_CHUNK) {
                c.chunks.push((self.name.clone(), total));
            }
        }
        if let Some(f) = self.mirror.as_mut() {
            f.write_all(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(f) = self.mirror.as_mut() {
            f.flush()?;
        }
        Ok(())
    }
}

/// `IoProvider` 适配器（自持输出层）。
pub struct TectonicIo {
    /// 项目根：磁盘输入的解析基准，也是只允许写 `tmp/` 的边界。
    project_root: PathBuf,
    /// 输出镜像目录（`tmp/`）：`Some` ⇒ 输出同时落盘，产物落点与子进程档一致（方案 §5.4）。
    mirror_dir: Option<PathBuf>,
    /// 内存直喂输入（主文件 `.tex`、以及我们自己生成的中间文件）。
    injected: HashMap<String, Vec<u8>>,
    /// 支持文件层（宏包/字体/cmap）。`None` = 没有可用 bundle（失败要显式报，不许静默）。
    bundle: Option<Box<dyn Bundle>>,
    /// **bundle digest**（`Bundle::get_digest()`）：format 缓存键（V-02）。`None` = 无 bundle。
    bundle_digest: Option<String>,
    /// format 缓存目录（产品缓存目录下的 `formats/`）。**必须显式注入**：
    /// 上游 `format_cache_path` 默认落**项目目录**（t3 §5.1 / HL-11）⇒ 不注入就会把
    /// 24 MB 的 `.fmt` 扔进用户项目。`None` ⇒ 不落盘（只在内存里用一次）。
    format_cache_dir: Option<PathBuf>,
    shared: SharedCapture,
    /// 本层的输入请求流水（`ram:` / `disk:` / `bundle:` / `missing:` / `nobundle:` / `format:`）：
    /// 失败时用它给出"缺什么"的可读证据。与 runner 共享同一份（引擎借用本层后无法再取回）。
    requests: Arc<Mutex<Vec<String>>>,
    /// **协作式取消标志**（V-06）：置位后每个 I/O 入口立刻返回 [`CANCEL_MESSAGE`]。
    cancel: Option<Arc<AtomicBool>>,
    /// **format 趟开关**（上游 `enter_format_mode`/`leave_format_mode`，`src/driver.rs:263-272`）：
    /// 置位期间主输入换成 [`format_primary_source`]（合成的 `\input tectonic-format-latex.tex`），
    /// 且**输出不镜像到 `tmp/`**（format 趟是准备步骤，上游在它结束后清空整个内存层）。
    ///
    /// 为什么用共享 `Arc<AtomicBool>` 而不是 `&mut self` 方法：driver 一旦持有本层，runner 就再也
    /// 拿不到 `&mut`（`CoreBridgeLauncher::new(&mut driver, …)`），而 format 趟与排版趟**必须走
    /// 同一个 driver**（bundle 在 IoProvider 里、`Box<dyn Bundle>` 不可共享）⇒ 只能靠共享标志
    /// 在两次 `process` 之间切换。
    format_pass: Option<Arc<AtomicBool>>,
}

impl TectonicIo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_root: impl Into<PathBuf>,
        mirror_dir: Option<PathBuf>,
        bundle: Option<Box<dyn Bundle>>,
        bundle_digest: Option<String>,
        format_cache_dir: Option<PathBuf>,
        shared: SharedCapture,
        requests: Arc<Mutex<Vec<String>>>,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Self {
        Self {
            project_root: project_root.into(),
            mirror_dir,
            injected: HashMap::new(),
            bundle,
            bundle_digest,
            format_cache_dir,
            shared,
            requests,
            cancel,
            format_pass: None,
        }
    }

    /// 挂上 format 趟开关（见字段说明）。runner 在 format 趟前把标志置 `true`、趟后置 `false`。
    pub fn set_format_pass(&mut self, flag: Arc<AtomicBool>) {
        self.format_pass = Some(flag);
    }

    /// 当前是否处于 format 趟。
    pub fn in_format_pass(&self) -> bool {
        self.format_pass.as_ref().is_some_and(|f| f.load(Ordering::Relaxed))
    }

    fn note(&mut self, line: String) {
        self.requests.lock().unwrap_or_else(|e| e.into_inner()).push(line);
    }

    /// 取消是否已置位（V-06）。
    pub fn is_cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
    }

    /// 取消时的错误值（`OpenResult::Err` / `write_format` 共用）。
    fn cancel_error(&self) -> tectonic_errors::Error {
        tectonic_errors::Error::msg(CANCEL_MESSAGE)
    }

    /// format 是否已在产品缓存目录里（决定要不要先跑 initex 趟生成，方案 §3.4 第 3 步）。
    ///
    /// 取不到路径（未注入缓存目录 / 拿不到 digest）一律当"没有"：那只是"这次不复用"，
    /// 真正要写缓存时 `write_format` 会显式报错，不静默。
    pub fn format_present(&self) -> bool {
        self.format_path(crate::FORMAT_NAME).map(|p| p.is_file()).unwrap_or(false)
    }

    /// 内存直喂（方案 §4.1 的"可直喂内存输入"；t2 §3.1）。
    pub fn inject(&mut self, name: &str, data: Vec<u8>) {
        self.injected.insert(name.to_owned(), data);
    }

    pub fn has_injected(&self, name: &str) -> bool {
        self.injected.contains_key(name)
    }

    fn disk_path(&self, name: &str) -> Option<PathBuf> {
        // 绝对路径直接拒绝：本层只认项目根内的相对名字（防止引擎写出项目外，
        // 也与子进程档 cwd = 项目根的相对输入口径一致）。
        let rel = Path::new(name);
        if rel.is_absolute() {
            return None;
        }
        let candidate = self.project_root.join(rel);
        candidate.is_file().then_some(candidate)
    }

    fn open_disk_input(&mut self, name: &str) -> Option<InputHandle> {
        let path = self.disk_path(name)?;
        let data = std::fs::read(&path).ok()?;
        self.note(format!("disk:{name}"));
        Some(InputHandle::new(name, Cursor::new(data), InputOrigin::Filesystem))
    }

    /// format 缓存路径（V-02）：`<cache>/formats/{digest}-{format_name}-{FORMAT_SERIAL}.fmt`。
    ///
    /// 无条件用**我们钉定的** [`crate::FORMAT_NAME`] 而不是调用方给的名字：上游在 dump 之后
    /// **有意**按 format 的 stem 而不是 dump 名写缓存（`driver.rs:1836` 的注释
    /// "intentionally pass 'stem', not 'name'"），否则会写出 `{digest}-UNUSED-33.fmt`。
    pub fn format_path(&self, _name: &str) -> Result<PathBuf, String> {
        format_cache_path(
            self.format_cache_dir.as_deref(),
            self.bundle_digest.as_deref(),
            crate::FORMAT_NAME,
        )
    }

    /// 取消检查的统一入口：返回 `Some(err)` 表示本次 I/O 应当立刻失败。
    fn abort_if_cancelled(&self) -> Option<tectonic_errors::Error> {
        self.is_cancelled().then(|| self.cancel_error())
    }
}

impl IoProvider for TectonicIo {
    fn output_open_name(&mut self, name: &str) -> OpenResult<OutputHandle> {
        if let Some(e) = self.abort_if_cancelled() {
            return OpenResult::Err(e);
        }
        // format 趟的输出（dump 名是 `UNUSED.fmt` 这个占位）**不镜像**：它不是项目产物，
        // 上游在 format 趟结束后把整个内存层清空（`driver.rs:1840-1841`）。字节仍留在捕获表里，
        // 由 runner 收走并写进产品缓存（`driver.rs:1825-1838`）。
        let mirror = match (&self.mirror_dir, Path::new(name).is_absolute(), self.in_format_pass()) {
            (Some(dir), false, false) => {
                let path = dir.join(name);
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::File::create(&path).ok()
            }
            _ => None,
        };
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .files
            .entry(name.to_owned())
            .or_default();
        OpenResult::Ok(OutputHandle::new(
            name,
            CaptureWriter { shared: self.shared.clone(), name: name.to_owned(), mirror },
        ))
    }

    /// xdvipdfmx 硬性要求：`_dpx_ensure_output_handle`（`pdf_io/dpx-error.c:48-57`）
    /// 会调 `ttstub_output_open_stdout()`，拿到无效句柄就直接死（t5 实测）。
    fn output_open_stdout(&mut self) -> OpenResult<OutputHandle> {
        if let Some(e) = self.abort_if_cancelled() {
            return OpenResult::Err(e);
        }
        const NAME: &str = "<stdout>";
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .files
            .entry(NAME.to_owned())
            .or_default();
        OpenResult::Ok(OutputHandle::new(
            NAME,
            CaptureWriter { shared: self.shared.clone(), name: NAME.to_owned(), mirror: None },
        ))
    }

    fn input_open_name(
        &mut self,
        name: &str,
        status: &mut dyn StatusBackend,
    ) -> OpenResult<InputHandle> {
        if let Some(e) = self.abort_if_cancelled() {
            return OpenResult::Err(e);
        }
        // ① 内存直喂
        if let Some(data) = self.injected.get(name) {
            let data = data.clone();
            self.note(format!("ram:{name}"));
            return OpenResult::Ok(InputHandle::new(name, Cursor::new(data), InputOrigin::Other));
        }
        // ② **本次编译的产物层**（上游 `BridgeState` 的 mem 层）。顺序依据：上游的
        //    `bridgestate_ioprovider_cascade`（`src/driver.rs`）是
        //    primary_input → **mem** → filesystem → … → bundle。
        //
        //    少了这一层的实测症状：CJK 夹具在页 1 shipout 之后 abort ——
        //    `failed to open input file "zh-min.aux"`。原因：LaTeX 在 `\end{document}` 用
        //    **TeX 原语** `\@@input\jobname.aux` 回读刚写出的 `.aux`（`latex.ltx`；
        //    `atveryend.sty` 也有一处 `\input\jobname.aux`），那是**不过 `\IfFileExists` 的硬输入**；
        //    而我们的产物在捕获表/`tmp/`、不在项目根 ⇒ 只查"项目磁盘 + bundle"必然找不到。
        if let Some(data) = {
            let c = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            c.files.get(name).cloned()
        } {
            self.note(format!("mem:{name}"));
            return OpenResult::Ok(InputHandle::new(name, Cursor::new(data), InputOrigin::Other));
        }
        // ③ 项目磁盘
        if let Some(h) = self.open_disk_input(name) {
            return OpenResult::Ok(h);
        }
        // ④ bundle（宏包/字体/cmap）
        match self.bundle.as_mut() {
            Some(b) => match b.input_open_name(name, status) {
                OpenResult::Ok(h) => {
                    self.note(format!("bundle:{name}"));
                    OpenResult::Ok(h)
                }
                OpenResult::NotAvailable => {
                    self.note(format!("missing:{name}"));
                    OpenResult::NotAvailable
                }
                e => e,
            },
            None => {
                self.note(format!("nobundle:{name}"));
                OpenResult::NotAvailable
            }
        }
    }

    /// 带绝对路径的输入（SyncTeX 需要真实源码路径，上游 `io_base` 文档 `:458-483`）。
    fn input_open_name_with_abspath(
        &mut self,
        name: &str,
        status: &mut dyn StatusBackend,
    ) -> OpenResult<(InputHandle, Option<PathBuf>)> {
        if let Some(e) = self.abort_if_cancelled() {
            return OpenResult::Err(e);
        }
        if let Some(abs) = self.disk_path(name) {
            return match self.input_open_name(name, status) {
                OpenResult::Ok(h) => OpenResult::Ok((h, Some(abs))),
                OpenResult::Err(e) => OpenResult::Err(e),
                OpenResult::NotAvailable => OpenResult::NotAvailable,
            };
        }
        match self.input_open_name(name, status) {
            OpenResult::Ok(h) => OpenResult::Ok((h, None)),
            OpenResult::Err(e) => OpenResult::Err(e),
            OpenResult::NotAvailable => OpenResult::NotAvailable,
        }
    }

    /// 主输入（`TexEngine::process` 的 `input_file_name`）：内存直喂优先（方案 §4.1 HL-1）。
    ///
    /// **format 趟例外**：置位期间返回合成的 [`format_primary_source`]，与上游
    /// `BufferedPrimaryIo` 同形（空名字 + `InputOrigin::Other`，`io_base/src/stdstreams.rs:140-147`）。
    fn input_open_primary(&mut self, status: &mut dyn StatusBackend) -> OpenResult<InputHandle> {
        if let Some(e) = self.abort_if_cancelled() {
            return OpenResult::Err(e);
        }
        if self.in_format_pass() {
            let text = format_primary_source(crate::FORMAT_NAME);
            self.note(format!("ram:{text}（format 趟合成主输入）"));
            return OpenResult::Ok(InputHandle::new(
                "",
                Cursor::new(text.into_bytes()),
                InputOrigin::Other,
            ));
        }
        let primary = self
            .injected
            .keys()
            .find(|k| k.ends_with(".tex"))
            .cloned();
        match primary {
            Some(name) => self.input_open_name(&name, status),
            None => OpenResult::NotAvailable,
        }
    }

    /// format 命中即复用（t3 §2.1 第 7 行）——**只查我们自己的产品缓存目录**，名字带 digest。
    fn input_open_format(
        &mut self,
        name: &str,
        _status: &mut dyn StatusBackend,
    ) -> OpenResult<InputHandle> {
        if let Some(e) = self.abort_if_cancelled() {
            return OpenResult::Err(e);
        }
        // 读不到路径（未注入缓存目录 / 无 digest）只是"这次不复用"，不是错误。
        let Some(path) = self.format_path(name).ok() else {
            return OpenResult::NotAvailable;
        };
        match std::fs::read(&path) {
            Ok(data) => {
                self.note(format!("format:{}", path.display()));
                OpenResult::Ok(InputHandle::new(name, Cursor::new(data), InputOrigin::Other))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => OpenResult::NotAvailable,
            Err(e) => OpenResult::Err(e.into()),
        }
    }

    /// format 落**产品缓存目录**（HL-11：绝不能落项目目录），文件名带 **bundle digest**（V-02）。
    ///
    /// 名字用我们钉定的 format（见 [`TectonicIo::format_path`]）。**注意**：正常流程里写缓存的是
    /// runner —— 它按上游口径在 format 趟结束后从捕获表收 `*.fmt` 再落盘
    /// （`driver.rs:1825-1838`），不依赖桥是否回调本方法；本方法是 `IoProvider` 契约的实现，
    /// 保证"若被回调，落点也一定正确"。
    fn write_format(
        &mut self,
        name: &str,
        data: &[u8],
        _status: &mut dyn StatusBackend,
    ) -> tectonic_errors::Result<()> {
        if let Some(e) = self.abort_if_cancelled() {
            return Err(e);
        }
        // 用 `Error::msg` 而不是 `anyhow!` 宏：`tectonic_errors` 0.3.0 不再 re-export
        // `anyhow` 这个模块名（`pub use anyhow;` 在该版本里没有），但 `Error` 类型本身
        // 就是 `anyhow::Error` 的 re-export（`crates/errors/src/lib.rs:25`）⇒ `msg` 可用。
        let path = self.format_path(name).map_err(tectonic_errors::Error::msg)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data)?;
        self.note(format!("write_format:{}", path.display()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn io_with(digest: Option<&str>, cache: Option<PathBuf>) -> TectonicIo {
        TectonicIo::new(
            std::env::temp_dir(),
            None,
            None,
            digest.map(str::to_owned),
            cache,
            new_capture(),
            Arc::new(Mutex::new(Vec::new())),
            None,
        )
    }

    /// 上游口径（`src/io/format_cache.rs:55-58`）：`{digest}-{name}-{FORMAT_SERIAL}.fmt`。
    #[test]
    fn format_file_name_follows_upstream_convention() {
        let name = format_file_name("6ffe0558deadbeef", "latex");
        assert_eq!(name, "6ffe0558deadbeef-latex-33.fmt", "FORMAT_SERIAL 必须是钉定的 33：{name}");
        // 传入 `latex.fmt` 也要归一化到同一名字（上游按 format 名而非文件名拼）
        assert_eq!(format_file_name("6ffe0558deadbeef", "latex.fmt"), name);
    }

    /// V-02：缓存路径**以 bundle digest 为键** —— 换 bundle 必须换文件名（否则复用旧 format）。
    #[test]
    fn format_path_is_digest_scoped() {
        let cache = std::env::temp_dir().join("latteset-tectonic-formats");
        let a = io_with(Some("digest-a"), Some(cache.clone()));
        let b = io_with(Some("digest-b"), Some(cache.clone()));
        let pa = a.format_path("latex").expect("digest-a 应当能算出路径");
        let pb = b.format_path("latex").expect("digest-b 应当能算出路径");
        assert_ne!(pa, pb, "换 bundle 必须换缓存文件名，否则会复用旧 format（V-02）");
        assert_eq!(pa.file_name().unwrap().to_string_lossy(), "digest-a-latex-33.fmt");
        assert_eq!(pa.parent(), Some(cache.as_path()));
    }

    /// V-02：**取不到 digest ⇒ 显式报错**，不得退回裸名（`{stem}.fmt`）。
    #[test]
    fn format_path_refuses_bare_name_without_digest() {
        let cache = std::env::temp_dir().join("latteset-tectonic-formats-nodigest");
        let io = io_with(None, Some(cache));
        let err = io.format_path("latex").expect_err("无 digest 必须报错");
        assert!(err.contains("digest"), "{err}");
        assert!(!io.format_present(), "无 digest 时不能声称缓存命中");
    }

    /// 方案 §5.2 硬约束：未注入缓存目录时**不能**算出一个落在项目目录的路径。
    #[test]
    fn format_path_requires_cache_dir() {
        let io = io_with(Some("digest-a"), None);
        let err = io.format_path("latex").expect_err("未注入缓存目录必须报错");
        assert!(err.contains("format 缓存目录"), "{err}");
    }

    /// 带镜像目录的 io（`tmp/` 落的那些副本）。
    fn io_with_mirror(mirror: PathBuf) -> TectonicIo {
        TectonicIo::new(
            std::env::temp_dir(),
            Some(mirror),
            None,
            Some("digest-a".to_owned()),
            Some(std::env::temp_dir()),
            new_capture(),
            Arc::new(Mutex::new(Vec::new())),
            None,
        )
    }

    /// 合成主输入的字面量必须与上游 `enter_format_mode` 一致
    /// （`src/driver.rs:265` 的 `format!("\\input {format_file_name}")`，文件名 = `tectonic-format-{stem}.tex`）。
    #[test]
    fn format_primary_source_matches_upstream_shape() {
        assert_eq!(format_primary_source("latex"), "\\input tectonic-format-latex.tex");
        // format 名带扩展名时要归一化到 stem（上游 `driver.rs:1785-1791` 先取 stem）
        assert_eq!(format_primary_source("latex.fmt"), "\\input tectonic-format-latex.tex");
    }

    /// **format 趟的主输入是合成的 format 源，不是注入的用户文档**。
    ///
    /// 这条是真缺陷的回归守卫：先前 initex 趟拿用户文档当主输入 ⇒ dump 出一个没有 LaTeX 宏的
    /// "format"，缓存目录里没有 `.fmt`，排版趟才炸成 `! Undefined control sequence. \documentclass`。
    #[test]
    fn format_pass_primary_is_the_synthetic_source_not_the_document() {
        let mut io = io_with(Some("digest-a"), Some(std::env::temp_dir()));
        io.inject("main.tex", b"\\documentclass{ctexart}".to_vec());
        let mut status = tectonic_status_base::NoopStatusBackend::default();

        // 未置位（排版趟）：主输入 = 注入的用户文档
        let (h, _) = match io.input_open_primary(&mut status) {
            OpenResult::Ok(h) => (h, ()),
            other => panic!("排版趟应当能拿到主输入：{:?}", matches!(other, OpenResult::NotAvailable)),
        };
        assert_eq!(h.name(), "main.tex", "排版趟的主输入必须是用户文档");

        // 置位（format 趟）：主输入 = 合成的 `\input tectonic-format-latex.tex`
        let flag = Arc::new(AtomicBool::new(true));
        io.set_format_pass(flag.clone());
        assert!(io.in_format_pass());
        let h = match io.input_open_primary(&mut status) {
            OpenResult::Ok(h) => h,
            _ => panic!("format 趟必须能拿到合成主输入"),
        };
        assert_eq!(h.name(), "", "上游 BufferedPrimaryIo 用的是空名字（stdstreams.rs:142-146）");
        let mut buf = String::new();
        use std::io::Read as _;
        let mut h = h;
        h.read_to_string(&mut buf).expect("读合成主输入");
        assert_eq!(buf, "\\input tectonic-format-latex.tex");

        // 放回（排版趟）：又回到用户文档
        flag.store(false, Ordering::Relaxed);
        let h = match io.input_open_primary(&mut status) {
            OpenResult::Ok(h) => h,
            _ => panic!("置位放回后应当又是用户文档"),
        };
        assert_eq!(h.name(), "main.tex");
    }

    /// format 趟的输出（dump 名 `UNUSED.fmt` 是占位）**不得镜像进 `tmp/`**：它不是项目产物，
    /// 上游在 format 趟结束后清空整个内存层（`driver.rs:1840-1841`）。
    #[test]
    fn format_pass_output_is_not_mirrored_to_tmp() {
        let mirror = std::env::temp_dir().join("latteset-tectonic-io-mirror-test");
        let _ = std::fs::remove_dir_all(&mirror);
        std::fs::create_dir_all(&mirror).expect("建镜像目录");

        let flag = Arc::new(AtomicBool::new(true));
        let mut io = io_with_mirror(mirror.clone());
        io.set_format_pass(flag.clone());
        assert!(matches!(io.output_open_name(FORMAT_DUMP_PLACEHOLDER), OpenResult::Ok(_)));
        assert!(
            !mirror.join(FORMAT_DUMP_PLACEHOLDER).exists(),
            "format 趟不得把 dump 镜像到 tmp/（占位名不是项目产物）"
        );

        // 排版趟：正常产物照旧镜像
        flag.store(false, Ordering::Relaxed);
        assert!(matches!(io.output_open_name("main.log"), OpenResult::Ok(_)));
        assert!(mirror.join("main.log").exists(), "排版趟的产物仍要镜像到 tmp/");
        let _ = std::fs::remove_dir_all(&mirror);
    }

    /// V-06 / LIB-9：取消置位后**每个 I/O 入口**都立刻报错（引擎在下一个回调处中止）。
    #[test]
    fn cancelled_io_aborts_at_every_entry_point() {        let flag = Arc::new(AtomicBool::new(false));
        let mut io = io_with(Some("digest-a"), Some(std::env::temp_dir()));
        io.cancel = Some(flag.clone());
        assert!(!io.is_cancelled(), "未置位时不应中止");

        flag.store(true, Ordering::Relaxed);
        let mut status = tectonic_status_base::NoopStatusBackend::default();
        assert!(io.is_cancelled());
        assert!(matches!(io.output_open_name("main.pdf"), OpenResult::Err(_)), "输出入口必须中止");
        assert!(matches!(io.output_open_stdout(), OpenResult::Err(_)), "stdout 入口必须中止");
        assert!(matches!(io.input_open_name("main.tex", &mut status), OpenResult::Err(_)));
        assert!(matches!(io.input_open_primary(&mut status), OpenResult::Err(_)));
        assert!(matches!(io.input_open_format("latex", &mut status), OpenResult::Err(_)));
        assert!(io.write_format("latex", b"x", &mut status).is_err(), "写 format 必须中止");
        assert!(
            matches!(io.input_open_name_with_abspath("main.tex", &mut status), OpenResult::Err(_)),
            "带绝对路径的输入入口必须中止"
        );
    }
}
