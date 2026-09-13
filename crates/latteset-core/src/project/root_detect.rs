//! 根文件探测（modules.md §3.4 / ADR-0009：正则启发式，不做完整 TeX 解析）。
//!
//! 算法：候选 = 含 `\documentclass` 且不被任何文件 `\input`/`\include` 引用的 .tex。
//!
//! 已知局限（ADR-0009，故意接受）：注释里的 `\input` 会误报；
//! `\includeonly` 未处理；编码假定 UTF-8。
//! 逃生门：项目 settings.json 的 `root_file` 手动覆盖。

use super::fs::FileSystem;
use super::model::{RootCandidate, RootResolution};
use super::scan::collect_tex_files;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// 根文件探测的 **IO 编排**：收集 `.tex` → 读内容 → [`find_candidates`] → [`resolve`]。
///
/// 探测需要文件内容（`\documentclass` 声明 / `\input` 引用），而读盘一律经 [`FileSystem`] trait
/// （设计决策 D4）——所以这个"编排"函数放在 core 里，由命令面与 watch 共用一份实现
/// （此前只有命令面有它，watch 想同步 root_file 就得复制一遍）。
/// 读单个文件失败（编码/权限）就跳过该文件：探测是尽力而为，不因一个坏文件整体失败。
pub async fn detect_root(fs: &dyn FileSystem, root: &Path) -> std::io::Result<RootResolution> {
    let files = collect_tex_files(fs, root).await?;
    let mut contents: HashMap<PathBuf, String> = HashMap::with_capacity(files.len());
    for f in &files {
        if let Ok(text) = fs.read_to_string(f).await {
            contents.insert(f.clone(), text);
        }
    }
    Ok(resolve(find_candidates(&files, root, |p| contents.get(p).cloned())))
}

/// 提取 `\input{...}` / `\include{...}` 引用（modules.md §3.4）。
fn extract_includes(content: &str) -> Vec<String> {
    static RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"\\(input|include)\{([^}]+)\}").expect("include regex")
    });
    RE.captures_iter(content)
        .map(|c| normalize_ref(&c[2]))
        .collect()
}

/// 提取 `\documentclass[...]{...}` 声明（含可选参数形式）。
/// TeX 允许 `\documentclass` 与 `{` 之间有空白（`\documentclass {article}` 合法），故用 `\s*` 放宽。
fn extract_documentclass(content: &str) -> Option<String> {
    static RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"\\documentclass\s*(\[[^\]]*\])?\s*\{([^}]+)\}").expect("documentclass regex")
    });
    RE.captures(content).map(|c| c[2].to_string())
}

/// 候选 = 含 documentclass 且未被任何文件引用（modules.md §5.4）。
///
/// `read` 闭包按需读内容、用完即弃——任何文件内容不跨函数存活。
pub fn find_candidates(
    files: &[PathBuf],
    root: &Path,
    mut read: impl FnMut(&Path) -> Option<String>,
) -> Vec<RootCandidate> {
    let mut referenced: HashSet<String> = HashSet::new();
    let mut with_documentclass: Vec<PathBuf> = Vec::new();
    for f in files {
        let Some(content) = read(f) else { continue };
        if extract_documentclass(&content).is_some() {
            with_documentclass.push(f.clone());
        }
        for r in extract_includes(&content) {
            referenced.insert(r);
        }
    }
    with_documentclass
        .into_iter()
        .filter(|f| {
            let rel = rel_without_ext(f, root);
            !referenced.contains(&rel)
        })
        .map(|path| RootCandidate { path })
        .collect()
}

/// 探测结果收敛（modules.md §5.4）：1 → Unique；>1 → Multiple（稳定排序）；0 → None。
pub fn resolve(mut candidates: Vec<RootCandidate>) -> RootResolution {    match candidates.len() {
        0 => RootResolution::None,
        1 => RootResolution::Unique(candidates.pop().unwrap().path),
        _ => {
            candidates.sort_by(|a, b| a.path.cmp(&b.path));
            RootResolution::Multiple(candidates.into_iter().map(|c| c.path).collect())
        }
    }
}

/// 引用规范化：去空白、去 .tex 后缀。
fn normalize_ref(s: &str) -> String {
    s.trim().trim_end_matches(".tex").to_string()
}

/// 相对项目根的路径，去扩展名、统一 `/` 分隔（与引用写法可比）。
fn rel_without_ext(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_includes_basic() {
        let content = r"\input{chapters/intro}
\include{chapters/methods.tex}
正文 \input{ appendix }";
        assert_eq!(
            extract_includes(content),
            vec!["chapters/intro", "chapters/methods", "appendix"]
        );
    }

    #[test]
    fn extract_includes_comment_false_positive_is_documented_limitation() {
        // ADR-0009 已知局限：注释中的引用会误报。此测试固化现状，防止"意外"变化。
        let content = "% 参考: \\input{old_stuff} 已废弃";
        assert_eq!(extract_includes(content), vec!["old_stuff"]);
    }

    #[test]
    fn extract_documentclass_variants() {
        assert_eq!(
            extract_documentclass("\\documentclass{article}").as_deref(),
            Some("article")
        );
        assert_eq!(
            extract_documentclass("\\documentclass[12pt,a4paper]{ctexart}").as_deref(),
            Some("ctexart")
        );
        assert_eq!(extract_documentclass("\\usepackage{foo}"), None);
        assert_eq!(extract_documentclass("no class here"), None);
    }

    #[test]
    fn extract_documentclass_allows_space_before_brace() {
        // TeX 允许 \documentclass 与 { 之间有空白（原正则不匹配 → 根探测返回 None）
        assert_eq!(
            extract_documentclass("\\documentclass {article}").as_deref(),
            Some("article")
        );
        assert_eq!(
            extract_documentclass("\\documentclass [12pt] {ctexart}").as_deref(),
            Some("ctexart")
        );
        // 仍不匹配非 documentclass
        assert_eq!(extract_documentclass("\\usepackage {foo}"), None);
    }

    fn fs_with(files: &[(&str, &str)]) -> crate::testutil::FakeFS {
        let mut fs = crate::testutil::FakeFS::new();
        for (path, content) in files {
            fs.put_file(*path, *content);
        }
        fs
    }

    fn detect(fs: &crate::testutil::FakeFS, root: &Path) -> RootResolution {
        let files = crate::testutil::block_on(crate::project::collect_tex_files(fs, root)).unwrap();
        resolve(find_candidates(&files, root, |p| fs.file(p).map(str::to_owned)))
    }

    #[test]
    fn unique_root_when_single_documentclass() {
        let fs = fs_with(&[
            ("proj/main.tex", "\\documentclass{article}\\input{chapters/intro}"),
            ("proj/chapters/intro.tex", "content"),
        ]);
        assert_eq!(
            detect(&fs, Path::new("proj")),
            RootResolution::Unique(PathBuf::from("proj/main.tex"))
        );
    }

    #[test]
    fn multiple_candidates_sorted() {
        let fs = fs_with(&[
            ("proj/a.tex", "\\documentclass{article}"),
            ("proj/b.tex", "\\documentclass{book}"),
        ]);
        assert_eq!(
            detect(&fs, Path::new("proj")),
            RootResolution::Multiple(vec![
                PathBuf::from("proj/a.tex"),
                PathBuf::from("proj/b.tex")
            ])
        );
    }

    #[test]
    fn no_candidates() {
        let fs = fs_with(&[("proj/readme.tex", "no class here")]);
        assert_eq!(detect(&fs, Path::new("proj")), RootResolution::None);
    }

    #[test]
    fn referenced_file_with_documentclass_is_not_candidate() {
        // 子文件即使含 documentclass（少见但合法），被引用即出局
        let fs = fs_with(&[
            ("proj/main.tex", "\\documentclass{article}\\input{sub}"),
            ("proj/sub.tex", "\\documentclass{article} sub content"),
        ]);
        assert_eq!(
            detect(&fs, Path::new("proj")),
            RootResolution::Unique(PathBuf::from("proj/main.tex"))
        );
    }

    #[test]
    fn backslash_reference_not_normalized() {
        // TeX 惯例用 `/` 分隔路径；反斜杠引用不归一化——固化现状（ADR-0009 局限）
        let content = "\\input{chapters\\intro}"; // TeX 源码字面：\input{chapters\intro}
        assert_eq!(extract_includes(content), vec!["chapters\\intro"]);
    }

    #[test]
    fn deep_nested_files_found() {
        let fs = fs_with(&[
            ("proj/main.tex", "\\documentclass{report}\\include{chapters/a/ch1}"),
            ("proj/chapters/a/ch1.tex", "chapter"),
        ]);
        assert_eq!(
            detect(&fs, Path::new("proj")),
            RootResolution::Unique(PathBuf::from("proj/main.tex"))
        );
    }
}
