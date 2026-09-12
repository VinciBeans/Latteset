//! tokio::fs 实现 core 的 FileSystem trait（modules.md §3.2 / D4、ADR-0010）。
//!
//! 基础设施层是**文件系统唯一落点**：上层（src-tauri 命令面）只知道
//! [texpresso_core::project::FileSystem] 这组方法，不出现 tokio::fs / std::fs。

use async_trait::async_trait;
use std::io;
use std::path::{Path, PathBuf};
use texpresso_core::project::{DirEntry, FileSystem};

/// Windows：`canonicalize` 返回 `\\?\` 前缀的 verbatim 路径，
/// 会破坏前端 `resolvePath` 的绝对路径判定（WSL 遗留：前端只认 `/`/盘符 开头）。
/// 对外暴露（root / root_file / 目录项）前统一剥掉该前缀；内部校验仍可 canonicalize。
pub fn strip_verbatim(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        // \\?\UNC\server\share\... → \\server\share\...
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    p.to_path_buf()
}

/// 真实文件系统实现（由基础设施层注入 core 的 trait 位）。
pub struct TokioFs;

#[async_trait]
impl FileSystem for TokioFs {
    async fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        let mut rd = tokio::fs::read_dir(path).await?;
        let mut out = Vec::new();
        while let Some(entry) = rd.next_entry().await? {
            let file_type = entry.file_type().await?;
            out.push(DirEntry {
                path: strip_verbatim(&entry.path()),
                is_dir: file_type.is_dir(),
            });
        }
        Ok(out)
    }

    async fn read_to_string(&self, path: &Path) -> io::Result<String> {
        tokio::fs::read_to_string(path).await
    }

    /// 容错解码覆写：诊断类文件（`.log`）不保证合法 UTF-8——GBK 源文件经 pdflatex
    /// 会把原始字节回显进日志，严格读取会让用户彻底拿不到错误信息（见
    /// `texpresso_core::log_parser::decode_log` 的实测说明）。
    async fn read_to_string_lossy(&self, path: &Path) -> io::Result<String> {
        let bytes = tokio::fs::read(path).await?;
        Ok(texpresso_core::log_parser::decode_log(&bytes))
    }

    /// canonicalize 后剥掉 Windows verbatim 前缀：core 的路径策略（D8）与前端
    /// resolvePath 都要求「对外可用形态」，两种形态混用会让 starts_with 判定永远失败。
    async fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        Ok(strip_verbatim(&tokio::fs::canonicalize(path).await?))
    }

    async fn is_dir(&self, path: &Path) -> io::Result<bool> {
        Ok(tokio::fs::metadata(path).await?.is_dir())
    }

    async fn exists(&self, path: &Path) -> io::Result<bool> {
        // 用 try_exists 而非 metadata：权限错误等应报错而不是伪装成"不存在"
        tokio::fs::try_exists(path).await
    }

    async fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        tokio::fs::write(path, contents).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_verbatim_drive_prefix() {
        // Windows canonicalize 返回 \\?\ 前缀；对外暴露前必须剥掉，否则前端 resolvePath 判定失败
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\C:\proj\main.tex")),
            Path::new(r"C:\proj\main.tex")
        );
    }

    #[test]
    fn strips_verbatim_unc_prefix() {
        // \\?\UNC\server\share\... → \\server\share\...
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\UNC\server\share\a.tex")),
            Path::new(r"\\server\share\a.tex")
        );
    }

    #[test]
    fn keeps_normal_paths_untouched() {
        assert_eq!(
            strip_verbatim(Path::new(r"C:\proj\main.tex")),
            Path::new(r"C:\proj\main.tex")
        );
        assert_eq!(
            strip_verbatim(Path::new("/proj/main.tex")),
            Path::new("/proj/main.tex")
        );
    }

    #[test]
    fn strips_verbatim_with_chinese_components() {
        // canonicalize 的中文路径同样带 \\?\ 前缀；剥离不得破坏中文（前端 resolvePath 依赖无前缀形态）
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\E:\项目\中文测试工程\中文主文件.tex")),
            Path::new(r"E:\项目\中文测试工程\中文主文件.tex")
        );
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\UNC\服务器\共享\中文目录\a.tex")),
            Path::new(r"\\服务器\共享\中文目录\a.tex")
        );
        // 无前缀的中文路径原样返回
        assert_eq!(
            strip_verbatim(Path::new(r"E:\项目\中文测试工程")),
            Path::new(r"E:\项目\中文测试工程")
        );
    }
}
