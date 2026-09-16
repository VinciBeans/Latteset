//! synctex CLI 实现（ADR-0008：走系统二进制 + 接口抽象）。
//!
//! 风险记录：CLI 输出契约需 Windows 实测（modules.md §12 后置项）。
//!
//! **竞争重试（roadmap ⑤，2026-09）**：编译进行中时 `tmp/<stem>.synctex.gz` 正被重写
//! （引擎先写 `main.synctex(busy)` 再改名），此时 `synctex view/edit` 会失败或返回空块——
//! 表现为"点了没反应"。故失败后按短退避重试几次；调用方只在"尚未编译过"时才该报错。

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use latteset_core::synctex::{
    parse_forward_output, parse_inverse_output, SourcePosition, SyncTexDoc, SyncTexError,
    SyncTexPosition, SyncTexProvider,
};

/// 选实现的运行期开关（A/B 复核用）：默认**自解析**；设 `LATTESET_SYNCTEX=cli` 走系统二进制。
pub const SYNCTEX_IMPL_ENV: &str = "LATTESET_SYNCTEX";

/// 按 [`SYNCTEX_IMPL_ENV`] 造一个 provider。**默认自解析**：
///
/// 这是 ⑫ 里程碑的业务前提 —— "干净 Windows 机器零预装可用"。ADR-0008 原来的选择（走系统
/// `synctex` CLI）依赖 **TeX Live**；只装 Tectonic 时那个二进制不存在，正反向定位全废。
/// 而 Tectonic 只**生成** `.synctex.gz`、没有查询子命令 ⇒ 查询只能自己解（`core::synctex::parse`）。
///
/// 保留 CLI 实现是给**复核**用的：`docs/modules.md` 的同步精度基线（`scripts/synctex-report.mjs`）
/// 是在 CLI 上量的，自解析必须能与它逐点对拍。两套实现并存是**过渡**，不是长期形态。
pub fn default_provider() -> Arc<dyn SyncTexProvider> {
    match std::env::var(SYNCTEX_IMPL_ENV).ok().as_deref() {
        Some("cli") => Arc::new(SyncTexCli),
        _ => Arc::new(SyncTexSelf),
    }
}

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
            let out = crate::proc::command("synctex")
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
            let out = crate::proc::command("synctex")
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

/// **自解析**实现（不依赖系统 `synctex` 二进制）：读 `tmp/<stem>.synctex.gz` → 解压 →
/// `core::synctex::parse` → 几何查询。
///
/// 与 CLI 版的口径差异（**已知且刻意**，见 `core::synctex::parse` 的模块文档）：
/// - `forward` 取"该行第一条**有尺寸的 hbox**"（CLI 取它自己的迭代器第一个块）；
/// - `inverse` 取"包含该点的**最小**盒子"，点落在空白处时**邻近回落**（CLI 也会回落，但规则不同）；
/// - `column` 恒为 `-1` —— `.synctex` 的盒子记录里没有列号（CLI 实测也基本恒为 -1）。
///
/// 这三条不是"差不多就行"：验收标准是 `scripts/synctex-report.mjs` 的三组样本上**不劣于** CLI
/// （正反向成功率与往返跳到位率），见 `docs/modules.md` §5。
pub struct SyncTexSelf;

impl SyncTexSelf {
    /// `.synctex.gz` 路径：与 CLI 版同一个约定（PDF 在项目根、同步数据在 `tmp/`）。
    fn doc_path(pdf: &Path) -> PathBuf {
        let stem = pdf
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "main".into());
        Self::synctex_dir(pdf).join(format!("{stem}.synctex.gz"))
    }

    /// synctex 数据目录（与 CLI 版共用：`<pdf 父目录>/tmp`）。
    fn synctex_dir(pdf: &Path) -> PathBuf {
        SyncTexCli::synctex_dir(pdf)
    }

    /// 读 + 解压 + 解析。**同步**（在 `spawn_blocking` 外没有 async IO 需求，且文件很小）。
    fn load(pdf: &Path) -> Result<SyncTexDoc, SyncTexError> {
        let path = Self::doc_path(pdf);
        let gz = std::fs::read(&path).map_err(|e| {
            SyncTexError::Io(format!(
                "读同步数据失败（{}）：{e}（还没编译过？或编译中正被重写）",
                path.display()
            ))
        })?;
        // 先按 gzip 解；不是 gzip 就当作未压缩的 `.synctex` 文本（`-synctex=-1` 的形态）。
        let text = match gunzip(&gz) {
            Ok(t) => t,
            Err(_) => String::from_utf8_lossy(&gz).into_owned(),
        };
        let doc = SyncTexDoc::parse(&text);
        if doc.is_empty() {
            return Err(SyncTexError::Parse(format!(
                "同步数据里解不出任何页面（{}）：文件损坏，或这次编译没开 --synctex",
                path.display()
            )));
        }
        Ok(doc)
    }
}

/// gzip 解压（纯 Rust 后端）。
fn gunzip(bytes: &[u8]) -> Result<String, std::io::Error> {
    use std::io::Read;
    let mut out = String::new();
    flate2::read::GzDecoder::new(bytes).read_to_string(&mut out)?;
    Ok(out)
}

#[async_trait]
impl SyncTexProvider for SyncTexSelf {
    async fn forward(
        &self,
        src: &SourcePosition,
        pdf: &Path,
    ) -> Result<SyncTexPosition, SyncTexError> {
        with_retry(&RETRY_BACKOFF, || async {
            let doc = Self::load(pdf)?;
            let tag = doc.tag_for_file(&src.file).ok_or_else(|| {
                SyncTexError::Parse(format!(
                    "同步数据里没有这个源文件（{}）—— 库形态产出的文件可能只有空 Input（见 \
                     core::synctex::parse 的模块文档），此时后台定位不可用",
                    src.file.display()
                ))
            })?;
            doc.forward_line(tag, src.line).ok_or_else(|| {
                SyncTexError::Parse(format!("第 {} 行在同步数据里没有对应位置", src.line))
            })
        })
        .await
    }

    async fn inverse(
        &self,
        pos: &SyncTexPosition,
        pdf: &Path,
    ) -> Result<SourcePosition, SyncTexError> {
        with_retry(&RETRY_BACKOFF, || async {
            let doc = Self::load(pdf)?;
            doc.inverse_point(pos.page, pos.x, pos.y).ok_or_else(|| {
                SyncTexError::Parse(format!(
                    "第 {} 页 ({}, {}) 处没有可用的源码映射",
                    pos.page, pos.x, pos.y
                ))
            })
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
