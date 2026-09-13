//! 反向定位结果的**目标分类**（roadmap ㉒，纯逻辑）。
//!
//! 背景（2026-09 实测）：`synctex edit` 在**生成内容**上会返回生成它的那个中间文件——
//! 目录区返回 `tmp/main.toc`、参考文献区返回 `tmp/main.bbl` 之类。这些文件不是用户源码，
//! 把它们当"跳转目标"打开会出现一屏用户没写过的内容（还会污染标签页）。
//!
//! 实测样本（`test_file/projects/multifile`，第 3 页目录区）：
//! - y=700 → `main.tex:37`（`\tableofcontents` 之后那行，源码）；
//! - y=650/600 → `tmp/main.toc:15`（生成文件）。
//! 所以"同一屏里既有源码又有生成文件"是常态，**回落探测**（见 src-tauri 命令层）才有意义。
//!
//! 本模块只做**分类**（不开文件、不猜行号），策略由调用方决定：
//! - [`InverseTarget::Source`] → 正常跳转；
//! - [`InverseTarget::Generated`] → 先尝试附近回落，再决定忽略并提示；
//! - [`InverseTarget::OutsideProject`] → 系统/宏包文件（如 `article.cls`），无法在编辑器里打开。

use std::path::{Path, PathBuf};

/// 反向定位命中的目标类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InverseTarget {
    /// 项目内的用户源码（可安全打开）。
    Source(PathBuf),
    /// 生成产物 / 中间文件（`tmp/` 下的、或非 `.tex` 的生成扩展名）。
    Generated(PathBuf),
    /// 项目之外的文件（系统宏包、TeX Live 文档等）——编辑器打不开。
    OutsideProject(PathBuf),
}

/// 生成产物的扩展名（LaTeX 一次编译会在 `tmp/` 里留下十几个这类文件）。
const GENERATED_EXT: &[&str] = &[
    "toc", "aux", "bbl", "blg", "nav", "snm", "out", "lof", "lot", "idx", "ind", "glo", "gls",
    "bcf", "run", "vrb", "xdv", "fls", "fdb_latexmk", "synctex", "synctex.gz",
];

fn ext_lower(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// 分类一个反向定位结果（`root` = 项目根）。
///
/// 判据（按顺序）：
/// 1. 不在项目根内 → [`InverseTarget::OutsideProject`]；
/// 2. 在 `tmp/` 下（本产品约定：中间文件统一收纳在那里）→ [`InverseTarget::Generated`]；
/// 3. 生成产物扩展名（`.toc`/`.aux`/`.bbl`…）→ [`InverseTarget::Generated`]；
/// 4. 非 `.tex` → [`InverseTarget::Generated`]（例如 `.sty` 在项目内的私有宏包：不是"用户要编辑的源"，
///    但也不该报"生成"；这里归为 Generated 是为了**不自动打开**，提示文案会带上真实文件名，
///    用户看得见是哪个文件）；
/// 5. 其余 → [`InverseTarget::Source`]。
///
/// 注意：**不做 canonicalize**（core 不碰文件系统）——路径比较是词法级的，`..` 会在比较前
/// 规整掉（[`normalize_lexically`]）。这与 `project::is_ignored` 的口径一致。
pub fn classify_inverse_target(root: &Path, hit: &Path) -> InverseTarget {
    let hit_n = normalize_lexically(hit);
    let root_n = normalize_lexically(root);
    if !hit_n.starts_with(&root_n) {
        return InverseTarget::OutsideProject(hit_n);
    }
    let rel = hit_n.strip_prefix(&root_n).unwrap_or(&hit_n);
    // tmp/ 是本产品写中间文件的固定位置（design.md）
    if rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).as_deref()
        == Some("tmp")
    {
        return InverseTarget::Generated(hit_n);
    }
    let ext = ext_lower(&hit_n);
    if ext == "tex" {
        return InverseTarget::Source(hit_n);
    }
    if GENERATED_EXT.contains(&ext.as_str()) {
        return InverseTarget::Generated(hit_n);
    }
    // 非 .tex 且不是已知生成扩展名（私有 .sty/.cls/图片等）：不自动打开
    InverseTarget::Generated(hit_n)
}

/// 词法规整：去掉 `.` 与多余的 `/`、尽量消掉 `..`（不碰文件系统）。
///
/// synctex 返回的路径形态不统一（实测 `E:/…/multifile/./main.tex`），规整后才好比较与显示。
pub fn normalize_lexically(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // 根/前缀之后才能弹出；否则保留（相对路径开头的 ..）
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "E:/proj";

    #[test]
    fn project_tex_is_source() {
        let t = classify_inverse_target(Path::new(ROOT), Path::new("E:/proj/./chapters/intro.tex"));
        assert_eq!(t, InverseTarget::Source(PathBuf::from("E:/proj/chapters/intro.tex")));
    }

    #[test]
    fn tmp_file_is_generated() {
        // 实测样本：点目录区命中 tmp/main.toc
        let t = classify_inverse_target(Path::new(ROOT), Path::new("E:/proj/tmp/main.toc"));
        assert_eq!(t, InverseTarget::Generated(PathBuf::from("E:/proj/tmp/main.toc")));
        // tmp/ 下的 .tex 也算生成物（中间产物，不是用户源码）
        let t = classify_inverse_target(Path::new(ROOT), Path::new("E:/proj/tmp/main.tex"));
        assert!(matches!(t, InverseTarget::Generated(_)));
    }

    #[test]
    fn generated_extension_outside_tmp_is_still_generated() {
        for ext in ["toc", "aux", "bbl", "nav", "out", "snm"] {
            let p = format!("E:/proj/main.{ext}");
            assert!(
                matches!(classify_inverse_target(Path::new(ROOT), Path::new(&p)), InverseTarget::Generated(_)),
                "{p} 应判为生成产物"
            );
        }
    }

    #[test]
    fn system_package_is_outside_project() {
        let t = classify_inverse_target(
            Path::new(ROOT),
            Path::new("C:/texlive/2026/texmf-dist/tex/latex/base/article.cls"),
        );
        assert!(matches!(t, InverseTarget::OutsideProject(_)));
    }

    #[test]
    fn parent_dir_escaping_root_is_outside() {
        let t = classify_inverse_target(Path::new(ROOT), Path::new("E:/proj/../other/main.tex"));
        assert!(matches!(t, InverseTarget::OutsideProject(_)), "{t:?}");
    }

    #[test]
    fn project_local_non_tex_is_not_opened_as_source() {
        // 项目内私有 .sty：不自动打开（提示里会带文件名），但也不能当"生成产物"去回落
        let t = classify_inverse_target(Path::new(ROOT), Path::new("E:/proj/mystyle.sty"));
        assert!(matches!(t, InverseTarget::Generated(_)));
    }

    /// 分隔符语义**跟随编译目标平台**（`std::path::Path`）：Windows 上 `\` 是分隔符，
    /// Unix 上只是普通字符。所以跨平台那部分用 `std::path::MAIN_SEPARATOR` 拼路径——
    /// "项目内源码"这个结论两端都该成立；反斜杠形态是 Windows 专属结论，只在该平台断言。
    /// 背景：曾把 `E:\proj\…` 的 Windows 结果写成跨平台契约，Linux CI 直接红。
    #[test]
    fn windows_separators_and_case() {
        let s = std::path::MAIN_SEPARATOR;
        let root = format!("E:{s}proj");
        let hit = format!("E:{s}proj{s}chapters{s}intro.tex");
        assert_eq!(
            classify_inverse_target(Path::new(&root), Path::new(&hit)),
            InverseTarget::Source(PathBuf::from(&hit))
        );

        // 词法比较大小写敏感（盘符不 case-fold）：判成项目外是安全侧，不做猜测
        let cased = format!("e:{s}proj{s}chapters{s}intro.tex");
        assert!(matches!(
            classify_inverse_target(Path::new(&root), Path::new(&cased)),
            InverseTarget::Source(_) | InverseTarget::OutsideProject(_)
        ));

        // Windows 专属：反斜杠形态必须规整成项目内源码
        #[cfg(windows)]
        assert_eq!(
            classify_inverse_target(Path::new(r"E:\proj"), Path::new(r"E:\proj\chapters\intro.tex")),
            InverseTarget::Source(PathBuf::from(r"E:\proj\chapters\intro.tex"))
        );
        // 非 Windows：`\` 不是分隔符，整串只是一个文件名 → 词法上在根之外，绝不能误判为可打开的源码
        #[cfg(not(windows))]
        assert!(matches!(
            classify_inverse_target(Path::new(r"E:\proj"), Path::new(r"E:\proj\chapters\intro.tex")),
            InverseTarget::OutsideProject(_)
        ));
    }
}
