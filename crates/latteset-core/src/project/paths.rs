//! 项目内路径解析（设计决策 D8 的领域侧实现）。
//!
//! 上层（src-tauri 命令面 / 基础设施层）不再自己 canonicalize + 前缀比对：
//! 策略在这里，IO 一律经 [`FileSystem`]（core 不做 IO，见 ADR-0006/0010）。

use super::fs::FileSystem;
use std::path::{Path, PathBuf};

/// 路径解析失败原因（命令面据此映射为对外错误契约 `{ code, message }`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    /// 目标不存在或不可访问。
    NotFound,
    /// 项目根不可访问（目录被删/权限变更/未打开项目）。
    RootUnavailable,
    /// 解析后落在项目根之外。
    Outside,
    /// 目标存在但不是目录。
    NotADirectory,
}

/// 项目根：解析为规范化绝对路径并要求可读目录（打开项目用）。
pub async fn resolve_project_root(
    fs: &dyn FileSystem,
    folder: &Path,
) -> Result<PathBuf, PathError> {
    let canonical = fs.canonicalize(folder).await.map_err(|_| PathError::NotFound)?;
    match fs.is_dir(&canonical).await {
        Ok(true) => Ok(canonical),
        Ok(false) => Err(PathError::NotADirectory),
        Err(_) => Err(PathError::NotFound),
    }
}

/// 已存在的目标：解析后必须落在项目根内（读文件 / 列目录）。
pub async fn resolve_in_project(
    fs: &dyn FileSystem,
    root: &Path,
    path: &Path,
) -> Result<PathBuf, PathError> {
    let canonical_root = fs.canonicalize(root).await.map_err(|_| PathError::RootUnavailable)?;
    let canonical = fs.canonicalize(path).await.map_err(|_| PathError::NotFound)?;
    if canonical.starts_with(&canonical_root) {
        Ok(canonical)
    } else {
        Err(PathError::Outside)
    }
}

/// 可能尚不存在的目标（新建/覆盖写文件）：对父目录解析后拼接文件名。
///
/// 对含 `..` 的词法路径，父目录 canonicalize 后再拼接可给出真实落点，
/// 因此 `../outside.tex` 会被判为项目外（仅 `starts_with` 词法比对做不到）。
pub async fn resolve_creatable_in_project(
    fs: &dyn FileSystem,
    root: &Path,
    path: &Path,
) -> Result<PathBuf, PathError> {
    let file_name = path.file_name().ok_or(PathError::NotFound)?;
    let parent = path.parent().ok_or(PathError::NotFound)?;
    let canonical_root = fs.canonicalize(root).await.map_err(|_| PathError::RootUnavailable)?;
    let canonical_parent = fs.canonicalize(parent).await.map_err(|_| PathError::NotFound)?;
    let target = canonical_parent.join(file_name);
    if target.starts_with(&canonical_root) {
        Ok(target)
    } else {
        Err(PathError::Outside)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{block_on, FakeFS};

    /// FakeFS 的 canonicalize 不做 `..` 折叠（与真实 FS 的差异由实现方负责），
    /// 故这里只锁定"根内/根外"的判定与错误分类。
    #[test]
    fn missing_target_is_not_found() {
        let mut fs = FakeFS::new();
        fs.put_file("proj/main.tex", "x");
        let err = block_on(resolve_in_project(&fs, Path::new("proj"), Path::new("proj/none.tex")))
            .unwrap_err();
        assert_eq!(err, PathError::NotFound);
    }

    #[test]
    fn inside_root_resolves() {
        let mut fs = FakeFS::new();
        fs.put_file("proj/chapters/a.tex", "x");
        let p = block_on(resolve_in_project(&fs, Path::new("proj"), Path::new("proj/chapters/a.tex")))
            .unwrap();
        assert_eq!(p, PathBuf::from("proj/chapters/a.tex"));
    }

    #[test]
    fn outside_root_rejected() {
        let mut fs = FakeFS::new();
        fs.put_file("proj/main.tex", "x");
        fs.put_file("other/evil.tex", "x");
        let err = block_on(resolve_in_project(&fs, Path::new("proj"), Path::new("other/evil.tex")))
            .unwrap_err();
        assert_eq!(err, PathError::Outside);
    }

    #[test]
    fn missing_root_is_root_unavailable() {
        let fs = FakeFS::new();
        let err = block_on(resolve_in_project(&fs, Path::new("nope"), Path::new("nope/a.tex")))
            .unwrap_err();
        assert_eq!(err, PathError::RootUnavailable);
    }

    #[test]
    fn creatable_uses_parent_and_keeps_file_name() {
        let mut fs = FakeFS::new();
        fs.put_file("proj/main.tex", "x");
        let p = block_on(resolve_creatable_in_project(
            &fs,
            Path::new("proj"),
            Path::new("proj/new.tex"),
        ))
        .unwrap();
        assert_eq!(p, PathBuf::from("proj/new.tex"));
    }

    #[test]
    fn creatable_outside_root_rejected() {
        let mut fs = FakeFS::new();
        fs.put_file("proj/main.tex", "x");
        fs.put_file("other/evil.tex", "x");
        let err = block_on(resolve_creatable_in_project(
            &fs,
            Path::new("proj"),
            Path::new("other/evil.tex"),
        ))
        .unwrap_err();
        assert_eq!(err, PathError::Outside);
    }

    #[test]
    fn project_root_must_be_directory() {
        let mut fs = FakeFS::new();
        fs.put_file("proj/main.tex", "x");
        assert_eq!(
            block_on(resolve_project_root(&fs, Path::new("proj"))).unwrap(),
            PathBuf::from("proj")
        );
        // 文件不是目录
        assert_eq!(
            block_on(resolve_project_root(&fs, Path::new("proj/main.tex"))).unwrap_err(),
            PathError::NotADirectory
        );
        // 不存在
        assert_eq!(
            block_on(resolve_project_root(&fs, Path::new("missing"))).unwrap_err(),
            PathError::NotFound
        );
    }
}
