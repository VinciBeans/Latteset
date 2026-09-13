//! 合并队列（modules.md §2.2）。
//!
//! 待编译条目最多一个、总是最新（覆盖语义，ADR-0001）。
//! "无法构造出多条目状态"由类型本身保证。

use crate::types::CompileRequest;

#[derive(Debug, Default)]
pub(crate) struct Queue {
    pending: Option<CompileRequest>,
}

impl Queue {
    pub fn new() -> Self {
        Self { pending: None }
    }

    /// 合并语义：新请求覆盖旧请求。
    pub fn push(&mut self, req: CompileRequest) {
        self.pending = Some(req);
    }

    /// 取出等待条目（无则 None）。
    pub fn take(&mut self) -> Option<CompileRequest> {
        self.pending.take()
    }

    /// 清空（手动终止语义：停运行 + 清队列）。
    pub fn clear(&mut self) {
        self.pending = None;
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Engine;
    use std::path::PathBuf;
    use std::time::Duration;

    fn req(name: &str) -> CompileRequest {
        CompileRequest {
            root_file: PathBuf::from(name),
            project_root: PathBuf::from("proj"),
            engine: Engine::XeLaTeX,
            timeout: Duration::from_secs(120),
            kind: crate::types::CompileKind::Full,
        }
    }

    #[test]
    fn new_queue_is_empty() {
        let mut q = Queue::new();
        assert!(q.is_empty());
        assert_eq!(q.take(), None);
    }

    #[test]
    fn push_overwrites_older_entry() {
        let mut q = Queue::new();
        q.push(req("a.tex"));
        q.push(req("b.tex")); // 合并：覆盖
        assert!(!q.is_empty());
        assert_eq!(q.take().unwrap().root_file, PathBuf::from("b.tex"));
        assert!(q.is_empty());
    }

    #[test]
    fn take_after_push_then_empty() {
        let mut q = Queue::new();
        q.push(req("a.tex"));
        assert_eq!(q.take().unwrap().root_file, PathBuf::from("a.tex"));
        assert_eq!(q.take(), None);
    }

    #[test]
    fn clear_empties_queue() {
        let mut q = Queue::new();
        q.push(req("a.tex"));
        q.clear();
        assert!(q.is_empty());
        assert_eq!(q.take(), None);
    }
}
