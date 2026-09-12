//! synctex CLI 实现（ADR-0008：走系统二进制 + 接口抽象）。
//!
//! 风险记录：CLI 输出契约需 Windows 实测（modules.md §12 后置项）。

use async_trait::async_trait;
use std::path::Path;
use texpresso_core::synctex::{
    parse_forward_output, parse_inverse_output, SourcePosition, SyncTexError, SyncTexPosition,
    SyncTexProvider,
};

pub struct SyncTexCli;

impl SyncTexCli {
    /// synctex 数据目录：PDF 在项目根（拷贝版），.synctex.gz 在 tmp/（latexmk -outdir）。
    /// `-d` 参数告诉 synctex 去哪里找数据文件（2026-08 实测必需）。
    fn synctex_dir(pdf: &Path) -> std::path::PathBuf {
        pdf.parent().unwrap_or(Path::new(".")).join("tmp")
    }
}

#[async_trait]
impl SyncTexProvider for SyncTexCli {
    async fn forward(&self, src: &SourcePosition, pdf: &Path) -> Result<SyncTexPosition, SyncTexError> {
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
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        parse_forward_output(&String::from_utf8_lossy(&out.stdout))
    }

    async fn inverse(&self, pos: &SyncTexPosition, pdf: &Path) -> Result<SourcePosition, SyncTexError> {
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
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        parse_inverse_output(&String::from_utf8_lossy(&out.stdout))
    }
}
