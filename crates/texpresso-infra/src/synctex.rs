//! synctex CLI 实现（ADR-0008：走系统二进制 + 接口抽象）。
//!
//! 风险记录：CLI 输出契约需 Windows 实测（modules.md §12 后置项）。
//!
//! **竞争重试（roadmap ⑤，2026-09）**：编译进行中时 `tmp/<stem>.synctex.gz` 正被重写
//! （引擎先写 `main.synctex(busy)` 再改名），此时 `synctex view/edit` 会失败或返回空块——
//! 表现为"点了没反应"。故失败后按短退避重试几次；调用方只在"尚未编译过"时才该报错。

use async_trait::async_trait;
use std::path::Path;
use std::time::Duration;
use texpresso_core::synctex::{
    parse_forward_output, parse_inverse_output, SourcePosition, SyncTexError, SyncTexPosition,
    SyncTexProvider,
};

pub struct SyncTexCli;

/// 重试退避（首次失败后依次等待）——总等待 ~600ms，用户感知不到但足以跨过改名瞬间。
const RETRY_BACKOFF: [Duration; 3] = [
    Duration::from_millis(100),
    Duration::from_millis(200),
    Duration::from_millis(300),
];

impl SyncTexCli {
    /// synctex 数据目录：PDF 在项目根（拷贝版），.synctex.gz 在 tmp/（latexmk -outdir）。
    /// `-d` 参数告诉 synctex 去哪里找数据文件（2026-08 实测必需）。
    fn synctex_dir(pdf: &Path) -> std::path::PathBuf {
        pdf.parent().unwrap_or(Path::new(".")).join("tmp")
    }
}

/// 带退避的重试：仅当 `attempt` 返回 `Err` 时按 `backoff` 依次等待重试；全失败返回**最后一次**错误。
///
/// 单独抽出来是为了可测：退避时长由调用方注入，测试传全零即可瞬间跑完（不必等真时间）。
async fn with_retry<T, F, Fut>(backoff: &[Duration], mut attempt: F) -> Result<T, SyncTexError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, SyncTexError>>,
{
    let mut last = match attempt().await {
        Ok(v) => return Ok(v),
        Err(e) => e,
    };
    for wait in backoff {
        tokio::time::sleep(*wait).await;
        match attempt().await {
            Ok(v) => return Ok(v),
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[async_trait]
impl SyncTexProvider for SyncTexCli {
    async fn forward(&self, src: &SourcePosition, pdf: &Path) -> Result<SyncTexPosition, SyncTexError> {
        with_retry(&RETRY_BACKOFF, || async {
            let out = tokio::process::Command::new("synctex")
                .arg("view")
                .arg("-i")
                .arg(format!("{}:{}:{}", src.line, src.column, src.file.display()))
                .arg("-o")
                .arg(pdf)
                .arg("-d")
                .arg(Self::synctex_dir(pdf))
                .arg("-x")
                .output()
                .await
                .map_err(|e| SyncTexError::Io(format!("synctex 启动失败：{e}")))?;
            if !out.status.success() {
                return Err(SyncTexError::Io(format!(
                    "synctex view 退出码 {}：{}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
            parse_forward_output(&String::from_utf8_lossy(&out.stdout))
        })
        .await
    }

    async fn inverse(&self, pos: &SyncTexPosition, pdf: &Path) -> Result<SourcePosition, SyncTexError> {
        with_retry(&RETRY_BACKOFF, || async {
            let out = tokio::process::Command::new("synctex")
                .arg("edit")
                .arg("-o")
                .arg(format!("{}:{}:{}:{}", pos.page, pos.x, pos.y, pdf.display()))
                .arg("-d")
                .arg(Self::synctex_dir(pdf))
                .output()
                .await
                .map_err(|e| SyncTexError::Io(format!("synctex 启动失败：{e}")))?;
            if !out.status.success() {
                return Err(SyncTexError::Io(format!(
                    "synctex edit 退出码 {}：{}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
            parse_inverse_output(&String::from_utf8_lossy(&out.stdout))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn retry_stops_at_first_success() {
        let calls = AtomicU32::new(0);
        let out: Result<u32, SyncTexError> = with_retry(&[Duration::ZERO, Duration::ZERO], || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(7)
        })
        .await;
        assert_eq!(out.unwrap(), 7);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "首次成功不应重试");
    }

    #[tokio::test]
    async fn retry_recovers_from_transient_failure() {
        // 复刻 synctex(busy) 竞争：前两次失败、第三次成功
        let calls = AtomicU32::new(0);
        let out: Result<u32, SyncTexError> = with_retry(&[Duration::ZERO, Duration::ZERO, Duration::ZERO], || async {
            let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
            if n < 3 {
                Err(SyncTexError::Io(format!("第 {n} 次：文件被占用")))
            } else {
                Ok(n)
            }
        })
        .await;
        assert_eq!(out.unwrap(), 3);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_returns_last_error_after_exhausting() {
        let calls = AtomicU32::new(0);
        let out: Result<u32, SyncTexError> = with_retry(&[Duration::ZERO, Duration::ZERO], || async {
            let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
            Err(SyncTexError::Parse(format!("第 {n} 次失败")))
        })
        .await;
        let err = out.unwrap_err().to_string();
        assert!(err.contains("第 3 次失败"), "应返回最后一次错误：{err}");
        assert_eq!(calls.load(Ordering::SeqCst), 3, "1 次首试 + 2 次重试");
    }
}
