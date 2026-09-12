//! 失败语义决策表（modules.md §2.3，纯函数）。
//!
//! 输入只有：运行中请求 + 重试计数 + 编译结果 + 是否有等待条目。
//! 不读队列、不读设置、不读时间——信息局部性：重试计数是唯一跨调用信息，
//! 收在 `attempt` 参数里，由 actor 保管。

use crate::types::{CompileOutcome, FailureKind};

/// 决策结果：由 actor 执行（modules.md §2.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Decide {
    /// 执行队列中的最新请求（跳过错过的旧版本）。
    StartPending,
    /// 超时且无等待：同一请求重试一次。
    Retry,
    /// 成功且无等待：无事可做。
    FinishOk,
    /// 展示失败（不重试）。
    Fail(FailureKind),
}

/// 决策表（design.md 失败语义三路，逐一对应）：
///
/// | outcome        | has_pending | attempt | 决策          |
/// |----------------|-------------|---------|---------------|
/// | Success        | —           | —       | 有→StartPending；无→FinishOk |
/// | Timeout        | true        | —       | StartPending（跳过重试） |
/// | Timeout        | false       | 0       | Retry         |
/// | Timeout        | false       | 1       | Fail(Timeout) |
/// | ContentError   | true        | —       | StartPending（不重试） |
/// | ContentError   | false       | —       | Fail(ContentError) |
/// | Aborted        | true        | —       | StartPending（abort 后的新请求是新意图） |
/// | Aborted        | false       | —       | Fail(Aborted) |
/// | IoError        | —           | —       | 视同 ContentError |
pub(crate) fn decide(attempt: u8, outcome: &CompileOutcome, has_pending: bool) -> Decide {
    match outcome {
        CompileOutcome::Success { .. } => {
            if has_pending {
                Decide::StartPending
            } else {
                Decide::FinishOk
            }
        }
        CompileOutcome::Timeout => {
            if has_pending {
                Decide::StartPending
            } else if attempt == 0 {
                Decide::Retry
            } else {
                Decide::Fail(FailureKind::Timeout)
            }
        }
        CompileOutcome::ContentError { .. } | CompileOutcome::IoError { .. } => {
            if has_pending {
                Decide::StartPending
            } else {
                Decide::Fail(FailureKind::ContentError)
            }
        }
        CompileOutcome::Aborted => {
            if has_pending {
                Decide::StartPending
            } else {
                Decide::Fail(FailureKind::Aborted)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CompileKind, ErrorEntry, ErrorKind};
    use std::path::PathBuf;

    fn success() -> CompileOutcome {
        CompileOutcome::Success {
            pdf_path: PathBuf::from("proj/main.pdf"),
            kind: CompileKind::Full,
        }
    }

    fn content_error() -> CompileOutcome {
        CompileOutcome::ContentError {
            errors: vec![ErrorEntry {
                message: "boom".into(),
                file: None,
                line: None,
                kind: ErrorKind::ContentError,
                diagnosis: None,
            }],
        }
    }

    // ---- Success ----

    #[test]
    fn success_no_pending_finishes_ok() {
        assert_eq!(decide(0, &success(), false), Decide::FinishOk);
    }

    #[test]
    fn success_with_pending_starts_pending() {
        assert_eq!(decide(0, &success(), true), Decide::StartPending);
    }

    // ---- Timeout ----

    #[test]
    fn timeout_with_pending_skips_retry() {
        assert_eq!(decide(0, &CompileOutcome::Timeout, true), Decide::StartPending);
    }

    #[test]
    fn timeout_first_attempt_retries() {
        assert_eq!(decide(0, &CompileOutcome::Timeout, false), Decide::Retry);
    }

    #[test]
    fn timeout_second_attempt_fails() {
        assert_eq!(
            decide(1, &CompileOutcome::Timeout, false),
            Decide::Fail(FailureKind::Timeout)
        );
    }

    // ---- ContentError ----

    #[test]
    fn content_error_no_pending_fails_without_retry() {
        assert_eq!(
            decide(0, &content_error(), false),
            Decide::Fail(FailureKind::ContentError)
        );
    }

    #[test]
    fn content_error_with_pending_starts_pending() {
        assert_eq!(decide(0, &content_error(), true), Decide::StartPending);
    }

    // ---- Aborted ----

    #[test]
    fn aborted_no_pending_fails_aborted() {
        assert_eq!(
            decide(0, &CompileOutcome::Aborted, false),
            Decide::Fail(FailureKind::Aborted)
        );
    }

    #[test]
    fn aborted_with_pending_starts_pending() {
        assert_eq!(decide(0, &CompileOutcome::Aborted, true), Decide::StartPending);
    }

    // ---- IoError ----

    #[test]
    fn io_error_treated_as_content_error() {
        let io = CompileOutcome::IoError {
            message: "latexmk 无法启动".into(),
        };
        assert_eq!(
            decide(0, &io, false),
            Decide::Fail(FailureKind::ContentError)
        );
        assert_eq!(decide(0, &io, true), Decide::StartPending);
    }

    // ---- attempt 计数与请求身份 ----

    #[test]
    fn retry_keeps_request_identity_but_attempt_increments() {
        // 决策只依赖 attempt 数值；请求身份由 actor 保管（见 actor 集成测试）
        let d0 = decide(0, &CompileOutcome::Timeout, false);
        let d1 = decide(1, &CompileOutcome::Timeout, false);
        assert_eq!(d0, Decide::Retry);
        assert_eq!(d1, Decide::Fail(FailureKind::Timeout));
    }
}
