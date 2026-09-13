//! 失败语义决策表（modules.md §2.3，纯函数）。
//!
//! 输入只有：编译结果 + 是否有等待条目。不读队列、不读设置、不读时间——**无跨调用状态**
//! （旧版有 `attempt` 重试计数，roadmap ㉕ 去掉重试后归零）。

use crate::types::{CompileOutcome, FailureKind};

/// 决策结果：由 actor 执行（modules.md §2.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Decide {
    /// 执行队列中的最新请求（跳过错过的旧版本）。
    StartPending,
    /// 成功且无等待：无事可做。
    FinishOk,
    /// 展示失败。
    Fail(FailureKind),
}

/// 决策表（design.md 失败语义三路，逐一对应）：
///
/// | outcome        | has_pending | 决策          |
/// |----------------|-------------|---------------|
/// | Success        | —           | 有→StartPending；无→FinishOk |
/// | Timeout        | true        | StartPending（新内容优先，跳过报错） |
/// | Timeout        | false       | Fail(Timeout)（**不重试**，见下） |
/// | ContentError   | true        | StartPending（不重试） |
/// | ContentError   | false       | Fail(ContentError) |
/// | Aborted        | true        | StartPending（abort 后的新请求是新意图） |
/// | Aborted        | false       | Fail(Aborted) |
/// | IoError        | —           | 视同 ContentError |
///
/// **超时为什么不自动重试**（roadmap ㉕，2026-09 改）：同一份源码在同一个上限下重跑
/// 一遍几乎必然再次超时（TeX 运行是确定性的），而代价是**再等一个完整超时窗口**——
/// 120s 默认值下用户要等 240s 才看到任何提示。现在改为立刻失败并给出证据化诊断
/// （["CompileOutcome::Timeout"] 里带 `ErrorEntry`），由用户点「提高超时并重试」一键完成
/// ——即"重试"从调度器静默行为变成用户可见、可调参的动作。
pub(crate) fn decide(outcome: &CompileOutcome, has_pending: bool) -> Decide {
    match outcome {
        CompileOutcome::Success { .. } => {
            if has_pending {
                Decide::StartPending
            } else {
                Decide::FinishOk
            }
        }
        CompileOutcome::Timeout { .. } => {
            if has_pending {
                Decide::StartPending
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
        page_hashes: Vec::new() }
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

    fn timeout() -> CompileOutcome {
        CompileOutcome::Timeout {
            entry: ErrorEntry {
                message: "编译在 120s 内未完成，已强制终止".into(),
                file: None,
                line: None,
                kind: ErrorKind::Timeout,
                diagnosis: None,
            },
        }
    }

    // ---- Success ----

    #[test]
    fn success_no_pending_finishes_ok() {
        assert_eq!(decide(&success(), false), Decide::FinishOk);
    }

    #[test]
    fn success_with_pending_starts_pending() {
        assert_eq!(decide(&success(), true), Decide::StartPending);
    }

    // ---- Timeout（roadmap ㉕：不自动重试） ----

    #[test]
    fn timeout_with_pending_starts_pending() {
        // 等待中的是新内容，直接跑它（不报旧内容的超时）
        assert_eq!(decide(&timeout(), true), Decide::StartPending);
    }

    #[test]
    fn timeout_without_pending_fails_immediately() {
        // 关键行为：无等待时**立刻**失败（旧版会静默重试一次 → 用户白等一个完整超时窗口）
        assert_eq!(decide(&timeout(), false), Decide::Fail(FailureKind::Timeout));
    }

    // ---- ContentError ----

    #[test]
    fn content_error_no_pending_fails_without_retry() {
        assert_eq!(
            decide(&content_error(), false),
            Decide::Fail(FailureKind::ContentError)
        );
    }

    #[test]
    fn content_error_with_pending_starts_pending() {
        assert_eq!(decide(&content_error(), true), Decide::StartPending);
    }

    // ---- Aborted ----

    #[test]
    fn aborted_no_pending_fails_aborted() {
        assert_eq!(
            decide(&CompileOutcome::Aborted, false),
            Decide::Fail(FailureKind::Aborted)
        );
    }

    #[test]
    fn aborted_with_pending_starts_pending() {
        assert_eq!(decide(&CompileOutcome::Aborted, true), Decide::StartPending);
    }

    // ---- IoError ----

    #[test]
    fn io_error_treated_as_content_error() {
        let io = CompileOutcome::IoError {
            message: "latexmk 无法启动".into(),
        };
        assert_eq!(
            decide(&io, false),
            Decide::Fail(FailureKind::ContentError)
        );
        assert_eq!(decide(&io, true), Decide::StartPending);
    }
}
