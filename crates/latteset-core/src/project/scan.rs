//! .tex 文件收集与忽略规则（modules.md §3.3）。
//!
//! 忽略规则是**单一事实来源**：编译监视（watch）与文件收集共用同一函数。

use super::fs::FileSystem;
use std::path::{Path, PathBuf};

/// 相对项目根的组件中是否有 `tmp/` 或隐藏项（.git 等）。
/// 项目根**之前**的隐藏父目录不算（如 `~/.projects/foo` 是合法项目根）。
fn is_hidden_or_tmp(path: &Path, root: &Path) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    for comp in rel.components() {
        let name = comp.as_os_str().to_str().unwrap_or("");
        if name == "tmp" || (name.starts_with('.') && name.len() > 1) {
            return true;
        }
    }
    false
}

/// 编译**触发**规则：项目内**任何非忽略文件**的变化都算输入（roadmap ㉜），
/// 但**排除本次编译自己写在项目根的产物**。
///
/// 为什么不再只认 `.tex`：`.bib`/图片/`.cls`/`.sty` 都是 latexmk 的输入。实测（G1 报告 §5.1）
/// 改被 `\bibliography` 引用的 `refs.bib` 时**全部 tmp 产物 mtime 纹丝不动** —— 用户"改了没反应"。
///
/// 为什么必须排除自己的产物：根 `<stem>.pdf` 是每次编译写在**项目根**的
/// （modules.md §12.3 的白名单只有 `tmp/**` 与项目根 `<stem>.pdf`）。不排除就会**自激**：
/// 编译 → 写 PDF → watch 看到变化 → 再触发编译。原子替换用的 `<stem>.pdf.tmp` 同理。
///
/// `root_file = None`（尚未确定根文件）时判不出产物名 ⇒ 只按"非忽略"处理；
/// 那种情况下 [`crate::compose::compile_request_for_change`] 本来也不会触发（拿不到根文件）。
pub fn is_compile_trigger(path: &Path, root: &Path, root_file: Option<&Path>) -> bool {
    if is_hidden_or_tmp(path, root) {
        return false;
    }
    // 必须带扩展名：**目录不是 latexmk 的输入**，而 notify 在 Windows 上把目录的增删报成
    // `Create(Any)/Remove(Any)`（实测：建一个 `verify-dir-probe/` 走的就是 `Any`）⇒ 靠事件类型
    // 认不出来。LaTeX 的真实输入都带扩展名（`.tex/.bib/.sty/.cls/.png/.pdf/.eps/.csv`…），
    // 而无扩展名的文件（`README`/`Makefile`）不可能是输入。跨平台都成立，且不碰文件系统。
    if path.extension().is_none() {
        return false;
    }
    !is_own_output(path, root, root_file)
}

/// 本项目**自己**写在项目根的产物（只此一类，见 [`is_compile_trigger`]）。
///
/// 只看项目根**这一层**：`figures/main.pdf`、`plot.pdf` 这类输入不受影响（哪怕与根文件同名）。
/// 文件名比对大小写不敏感（Windows/macOS 语义；误判的后果是"少触发一次"，比自激轻）。
fn is_own_output(path: &Path, root: &Path, root_file: Option<&Path>) -> bool {
    if path.parent() != Some(root) {
        return false;
    }
    let Some(stem) = root_file
        .and_then(|f| f.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
    else {
        return false;
    };
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.eq_ignore_ascii_case(&format!("{stem}.pdf")) || name.eq_ignore_ascii_case(&format!("{stem}.pdf.tmp"))
}

/// 文件树展示忽略规则：只藏 tmp/ 与隐藏项（树需要展示所有扩展名）。
pub fn is_tree_excluded(path: &Path, root: &Path) -> bool {
    is_hidden_or_tmp(path, root)
}

/// 是否 .tex 文件（扩展名大小写不敏感）。
///
/// 与触发规则**无关**（触发已不按扩展名过滤，见 [`is_compile_trigger`]）：本函数面向
/// "用户指定的根文件覆盖"这类人机边界（`MAIN.TEX` 应被接受）。
pub fn is_tex_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some(ext) if ext.eq_ignore_ascii_case("tex")
    )
}

#[cfg(test)]
mod tex_file_tests {
    use super::*;

    #[test]
    fn tex_extension_is_case_insensitive() {
        assert!(is_tex_file(Path::new("proj/main.tex")));
        assert!(is_tex_file(Path::new("proj/MAIN.TEX")));
        assert!(!is_tex_file(Path::new("proj/main.tex.bak")));
        assert!(!is_tex_file(Path::new("proj/notes.md")));
        assert!(!is_tex_file(Path::new("proj/main")));
    }
}

/// 递归收集项目内全部 .tex（排除 tmp/ 与隐藏目录），不跟随符号链接（防环）。
///
/// 每次调用全量扫描、无缓存（modules.md §3.3：目录量大时再优化增量）。
pub async fn collect_tex_files(fs: &dyn FileSystem, root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs.read_dir(&dir).await? {
            let path = entry.path;
            if is_hidden_or_tmp(&path, root) {
                continue;
            }
            if entry.is_dir {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("tex") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::FakeFS;
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new("proj")
    }

    #[test]
    fn tmp_dir_ignored() {
        let rf = Path::new("proj/main.tex");
        assert!(!is_compile_trigger(Path::new("proj/tmp/main.aux"), root(), Some(rf)));
        assert!(!is_compile_trigger(Path::new("proj/tmp/main.tex"), root(), Some(rf)));
        assert!(is_hidden_or_tmp(Path::new("proj/tmp/x"), root()));
    }

    #[test]
    fn hidden_entries_ignored() {
        let rf = Path::new("proj/main.tex");
        assert!(!is_compile_trigger(Path::new("proj/.git/config"), root(), Some(rf)));
        assert!(!is_compile_trigger(Path::new("proj/.latteset/settings.json"), root(), Some(rf)));
        assert!(is_hidden_or_tmp(Path::new("proj/.git"), root()));
    }

    /// **roadmap ㉜**：`.bib` / 图片 / `.cls` / `.sty` 都是输入 ⇒ 一律触发（过去只认 `.tex`）。
    #[test]
    fn any_project_file_triggers_compile() {
        let rf = Path::new("proj/main.tex");
        for p in [
            "proj/main.tex",
            "proj/refs.bib",
            "proj/figures/plot.png",
            "proj/figures/plot.pdf", // 与根文件名不同 ⇒ 是输入
            "proj/thesis.cls",
            "proj/custom.sty",
            "proj/data.csv",
            "proj/notes.md", // 非输入也触发：交给 latexmk 自己判定（精确失效见 ㉝）
        ] {
            assert!(is_compile_trigger(Path::new(p), root(), Some(rf)), "{p} 应触发");
        }
    }

    /// **防自激**：本次编译自己写在项目根的产物不得再触发编译（否则 编译→写 PDF→再编译 死循环）。
    #[test]
    fn own_root_artifacts_do_not_trigger() {
        let rf = Path::new("proj/main.tex");
        assert!(!is_compile_trigger(Path::new("proj/main.pdf"), root(), Some(rf)));
        assert!(!is_compile_trigger(Path::new("proj/main.pdf.tmp"), root(), Some(rf)));
        assert!(!is_compile_trigger(Path::new("proj/MAIN.PDF"), root(), Some(rf)), "大小写不敏感");
        // 同名但在子目录 ⇒ 是输入（`\includegraphics{figures/main.pdf}`）
        assert!(is_compile_trigger(Path::new("proj/figures/main.pdf"), root(), Some(rf)));
        // 别的 PDF 落在项目根 ⇒ 不是我们的产物
        assert!(is_compile_trigger(Path::new("proj/other.pdf"), root(), Some(rf)));
        // 根文件叫 main.tex 时，别的 stem 的 PDF 不算产物
        assert!(is_compile_trigger(Path::new("proj/slides.pdf"), root(), Some(rf)));
    }

    /// **目录不触发**：notify 在 Windows 上把目录增删报成 `Any`，认不出类型 ⇒ 用"必须带扩展名"兜住。
    #[test]
    fn directories_and_extensionless_files_do_not_trigger() {
        let rf = Path::new("proj/main.tex");
        assert!(!is_compile_trigger(Path::new("proj/verify-dir-probe"), root(), Some(rf)));
        assert!(!is_compile_trigger(Path::new("proj/figures"), root(), Some(rf)));
        assert!(!is_compile_trigger(Path::new("proj/README"), root(), Some(rf)));
        assert!(!is_compile_trigger(Path::new("proj/Makefile"), root(), Some(rf)));
    }

    /// 根文件未定 ⇒ 判不出产物名，只按"非忽略"处理（该场景上游不会触发编译）。
    #[test]
    fn without_root_file_nothing_is_an_artifact() {
        assert!(is_compile_trigger(Path::new("proj/main.pdf"), root(), None));
    }

    #[test]
    fn non_tex_is_shown_in_tree_but_artifacts_are_not_triggers() {
        assert!(!is_tree_excluded(Path::new("proj/notes.md"), root()));
        assert!(is_compile_trigger(Path::new("proj/notes.md"), root(), Some(Path::new("proj/main.tex"))));
    }

    #[test]
    fn hidden_parent_before_root_not_ignored() {
        // 项目根本身在隐藏目录里是合法场景（~/.projects/foo）
        let root = Path::new("/home/u/.projects/foo");
        assert!(!is_hidden_or_tmp(&root.join("main.tex"), root));
        assert!(is_hidden_or_tmp(&root.join(".git"), root));
    }

    #[test]
    fn collect_tex_files_dfs_with_filters() {
        let mut fs = FakeFS::new();
        fs.put_file("proj/main.tex", "\\documentclass{article}");
        fs.put_file("proj/chapters/intro.tex", "intro");
        fs.put_file("proj/chapters/deep/extra.tex", "extra");
        fs.put_file("proj/tmp/main.aux", "aux"); // 排除
        fs.put_file("proj/tmp/main.tex", "tmp tex"); // 排除
        fs.put_file("proj/.git/HEAD", "ref"); // 排除
        fs.put_file("proj/notes.md", "md"); // 非 tex 排除

        let files = crate::testutil::block_on(collect_tex_files(&fs, Path::new("proj")));
        let files: Vec<String> = files
            .unwrap()
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            files,
            vec![
                "proj/chapters/deep/extra.tex",
                "proj/chapters/intro.tex",
                "proj/main.tex",
            ]
        );
    }

    #[test]
    fn collect_tex_files_missing_root_errors() {
        let fs = FakeFS::new();
        let result = crate::testutil::block_on(collect_tex_files(&fs, Path::new("nope")));
        assert!(result.is_err());
    }
}
