//! `StatusBackend` 捕获（方案 §3.3 的 `ProgressStatus`）。
//!
//! 上游 `StatusBackend::report(&mut self, kind, args: Arguments, err: Option<&Error>)`
//! （`tectonic_status_base` 的 `crates/status_base/src/lib.rs:88-107`）收的是
//! `std::fmt::Arguments`：它**不能存储**（借用 + 生命周期），必须**当场** `to_string()`
//! ——这是本模块存在的主要理由。
//!
//! 映射口径（与 infra 的流式口径一致，`crates/latteset-infra/src/runner.rs` 顶部注释）：
//! - `Note`：只进日志（页进度由写句柄/块观察者另算）；
//! - `Warning`：进日志，**不进**错误列表（终态 `.log` 才是权威清单）；
//! - `Error`：进日志 + 通过 [`latteset_core::scheduler::CompileProgress::errors`]
//!   以**非权威中间态**上报一条 `ErrorEntry`。

use std::sync::{Arc, Mutex};

use latteset_core::scheduler::CompileProgress;
use latteset_core::types::{ErrorEntry, ErrorKind};
use tectonic_errors::Error;
use tectonic_status_base::{MessageKind, StatusBackend};

/// 引擎状态 → 日志 + 流式错误。
pub struct ProgressStatus {
    sink: Arc<dyn CompileProgress>,
    log: Arc<Mutex<Vec<String>>>,
}

impl ProgressStatus {
    pub fn new(sink: Arc<dyn CompileProgress>, log: Arc<Mutex<Vec<String>>>) -> Self {
        Self { sink, log }
    }

    fn push(&self, line: String) {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).push(line);
    }
}

impl StatusBackend for ProgressStatus {
    fn report(&mut self, kind: MessageKind, args: std::fmt::Arguments, err: Option<&Error>) {
        // `Arguments` 不可存储：当场格式化（方案 §3.3）。
        let mut line = args.to_string();
        if let Some(e) = err {
            // `{:#}` 才是带 cause 链的 anyhow 口径（`{}` 只给最外层）。
            line.push_str(&format!("：{e:#}"));
        }
        self.push(format!("{kind:?}: {line}"));
        if kind == MessageKind::Error {
            self.sink.errors(&[ErrorEntry {
                message: line,
                file: None,
                line: None,
                kind: ErrorKind::ContentError,
                diagnosis: None,
            }]);
        }
    }

    /// `tectonic_status_base` 0.2.2 的必需项（没有默认实现）：引擎在出错收尾时把
    /// **自己的错误日志缓冲**交回来（`crates/status_base/src/lib.rs:125`）。
    /// 我们不需要另存一份（`.log` 走 I/O 层、状态走 `report`），但必须实现——否则编译不过；
    /// 这里把它记进日志环，供失败时的「引擎状态尾部」证据使用。
    fn dump_error_logs(&mut self, output: &[u8]) {
        if output.is_empty() {
            return;
        }
        self.push(format!("error_logs: {}", String::from_utf8_lossy(output)));
    }

    /// 抑制 `Note` 之外的处理与上游默认实现一致（这里只记录，不额外格式）。
    fn report_error(&mut self, err: &Error) {
        let line = format!("Error: {err:#}");
        self.push(line.clone());
        self.sink.errors(&[ErrorEntry {
            message: line,
            file: None,
            line: None,
            kind: ErrorKind::ContentError,
            diagnosis: None,
        }]);
    }
}
