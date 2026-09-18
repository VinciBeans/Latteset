//! 测试工具（仅 #[cfg(test)]）：FakeFS / FakeRunner / 事件收集器。
//!
//! 信息局部性纪律的检验场：core 的模块只依赖 trait 接口，
//! 测试用假实现注入，验证模块不偷偷依赖 IO 或全局状态。

use crate::project::{DirEntry, FileSystem};
use crate::scheduler::CompileRunner;
use crate::types::{CompileKind, CompileOutcome, CompileRequest, PdfUpdated};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 内存文件系统：目录注册表 + 文件内容表。
#[derive(Default)]
pub struct FakeFS {
    dirs: HashSet<PathBuf>,
    files: HashMap<PathBuf, String>,
}

impl FakeFS {
    pub fn new() -> Self {
        Self::default()
    }

    /// 放一个文件（自动注册父目录链）。
    pub fn put_file(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        let path = path.into();
        self.files.insert(path.clone(), content.into());
        let mut parent = path.parent();
        while let Some(p) = parent {
            if p.as_os_str().is_empty() {
                break;
            }
            self.dirs.insert(p.to_path_buf());
            parent = p.parent();
        }
    }

    pub fn file(&self, path: &Path) -> Option<&str> {
        self.files.get(path).map(|s| s.as_str())
    }
}

/// 轮询等待条件成立（集成测试用；超时即 panic）。
pub async fn wait_until(mut cond: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !cond() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "等待条件超时（5s）"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

/// 同步执行一个 future（测试用单线程运行时）。
pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

#[async_trait]
impl FileSystem for FakeFS {
    async fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        let mut out: Vec<DirEntry> = Vec::new();
        for d in self.dirs.iter() {
            if d.parent() == Some(path) {
                out.push(DirEntry {
                    path: d.clone(),
                    is_dir: true,
                });
            }
        }
        for f in self.files.keys() {
            if f.parent() == Some(path) {
                out.push(DirEntry {
                    path: f.clone(),
                    is_dir: false,
                });
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        if out.is_empty() && !self.dirs.contains(path) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "目录不存在"));
        }
        Ok(out)
    }

    async fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "文件不存在"))
    }

    /// FakeFS 的"文件"是字符串，二进制读按其 UTF-8 字节返回（够测 XDV 解析的调用方）。
    async fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .get(path)
            .map(|s| s.as_bytes().to_vec())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "文件不存在"))
    }

    /// 内存路径已归一：原样返回，但目标必须存在（与真实 canonicalize 的"不存在即报错"一致）。
    async fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        if self.files.contains_key(path) || self.dirs.contains(path) {
            Ok(path.to_path_buf())
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "路径不存在"))
        }
    }

    async fn is_dir(&self, path: &Path) -> io::Result<bool> {
        Ok(self.dirs.contains(path))
    }

    async fn exists(&self, path: &Path) -> io::Result<bool> {
        Ok(self.files.contains_key(path) || self.dirs.contains(path))
    }

    async fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        let _ = (path, contents);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "FakeFS 只读：写入用例请用真实文件系统（infra 的 TokioFs）",
        ))
    }

    /// 与 [`Self::write`] 同一条纪律：FakeFS 不做写侧（建目录的真实用例走 TokioFs）。
    async fn create_dir(&self, path: &Path) -> io::Result<()> {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "FakeFS 只读：建目录用例请用真实文件系统（infra 的 TokioFs）",
        ))
    }
}

/// 可注入的编译结果队列（按调用顺序出队）。
pub type OutcomeQueue = std::collections::VecDeque<CompileOutcome>;

/// 可阻塞的假 runner：hold 时挂起直到 `release()`，用于测队列合并/终止。
/// 挂起期间收到取消信号 → 立即返回 `Aborted`（模拟真实 runner 的协作式取消）。
pub struct FakeRunner {
    results: Mutex<OutcomeQueue>,
    calls: Mutex<Vec<CompileRequest>>,
    hold: Option<Arc<tokio::sync::Notify>>,
}

impl FakeRunner {
    pub fn new() -> Self {
        Self {
            results: Mutex::new(OutcomeQueue::new()),
            calls: Mutex::new(Vec::new()),
            hold: None,
        }
    }

    /// 按调用顺序返回结果；队列空时默认 Success。
    pub fn with_results(results: Vec<CompileOutcome>) -> Self {
        Self {
            results: Mutex::new(results.into()),
            calls: Mutex::new(Vec::new()),
            hold: None,
        }
    }

    /// 每次编译都挂起，直到 `release()` 或取消。
    pub fn with_hold() -> Self {
        Self::with_hold_and_results(Vec::new())
    }

    /// 挂起 + 按调用顺序返回预设结果（放行后弹出）。
    pub fn with_hold_and_results(results: Vec<CompileOutcome>) -> Self {
        Self {
            results: Mutex::new(results.into()),
            calls: Mutex::new(Vec::new()),
            hold: Some(Arc::new(tokio::sync::Notify::new())),
        }
    }

    pub fn calls(&self) -> Vec<CompileRequest> {
        self.calls.lock().unwrap().clone()
    }

    pub fn release(&self) {
        if let Some(h) = &self.hold {
            h.notify_one();
        }
    }

    fn next_result(&self, req_kind: CompileKind) -> CompileOutcome {
        self.results
            .lock()
            .unwrap()
            .pop_front()
            // 默认结果按请求强度报（模拟"无升级"的正常执行）
            .unwrap_or(CompileOutcome::Success {
                pdf_path: PathBuf::from("out.pdf"),
                kind: req_kind,
            page_hashes: Vec::new() })
    }
}

impl Default for FakeRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CompileRunner for FakeRunner {
    async fn compile(&self, req: CompileRequest, cancel: tokio_util::sync::CancellationToken) -> CompileOutcome {
        let kind = req.kind;
        self.calls.lock().unwrap().push(req);
        match &self.hold {
            Some(h) => tokio::select! {
                _ = cancel.cancelled() => CompileOutcome::Aborted,
                _ = h.notified() => self.next_result(kind),
            },
            None => {
                if cancel.is_cancelled() {
                    CompileOutcome::Aborted
                } else {
                    self.next_result(kind)
                }
            }
        }
    }
}

/// 测试用收集器：把调度器输出的事件收进 Vec，供断言。
pub struct EventLog {
    statuses: Mutex<Vec<crate::types::CompileStatusDto>>,
    errors: Mutex<Vec<Vec<crate::types::ErrorEntry>>>,
    /// PDF 就绪事件（含变化页集合）——B/C 功能点的断言入口。
    pdf_events: Mutex<Vec<PdfUpdated>>,
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new()
    }
}

impl EventLog {
    pub fn new() -> Self {
        Self {
            statuses: Mutex::new(Vec::new()),
            errors: Mutex::new(Vec::new()),
            pdf_events: Mutex::new(Vec::new()),
        }
    }

    pub fn statuses(&self) -> Vec<crate::types::CompileStatusDto> {
        self.statuses.lock().unwrap().clone()
    }
    pub fn errors(&self) -> Vec<Vec<crate::types::ErrorEntry>> {
        self.errors.lock().unwrap().clone()
    }
    /// PDF 就绪事件全文（含 `changed_pages` / `pages`）。
    pub fn pdf_events(&self) -> Vec<PdfUpdated> {
        self.pdf_events.lock().unwrap().clone()
    }
    /// 只看路径的便捷视图（历史断言沿用；路径是 `PdfUpdated.path` 的字符串形态）。
    pub fn pdfs(&self) -> Vec<PathBuf> {
        self.pdf_events
            .lock()
            .unwrap()
            .iter()
            .map(|p| PathBuf::from(&p.path))
            .collect()
    }
}

/// 把 EventLog 包装成调度器的 Emitter。
pub fn event_log_emitter(log: Arc<EventLog>) -> crate::scheduler::Emitter {
    let log2 = log.clone();
    let log3 = log.clone();
    crate::scheduler::Emitter::new(
        Arc::new(move |s| log.statuses.lock().unwrap().push(s)),
        Arc::new(move |e| log2.errors.lock().unwrap().push(e)),
        Arc::new(move |p| log3.pdf_events.lock().unwrap().push(p)),
    )
}
