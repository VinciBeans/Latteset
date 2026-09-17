//! 片段文档装配（**数学公式悬停预览**，roadmap ㊸）：把「项目导言区 + 片段」拼成一份**可独立编译**
//! 的小文档，用真实排版给出这一个公式（[`crate::math::math_at`] 认出范围、这里装成文档）的预览。
//!
//! **来历**：装配口径来自已回退的「片段预览 A」（`git show 0ac930c`）。A 那一刀（编辑时弹段落预览卡）
//! 按产品判断回退了，但**这个函数本身**被 ㊸ 原样复用 —— 两处要的东西一样（真排版、相对路径指回
//! 项目根、页脚页码不撑裁剪框），而 A 踩过的三个坑的修法就落在下面这几条口径里。**A 的功能点没有
//! 因此复活**：段落级预览仍是回退状态，这里服务的是"悬停一个公式"。
//!
//! **口径（v1，写死在这里，别在调用方各写一套）**：
//! - **导言区** = 根文件里 `\begin{document}` **之前**的全部内容，逐字照搬（含注释与空行）；
//! - **片段** = 调用方给的文本，原样插进文档体（不解析、不转义）；
//! - **相对路径**：临时文档不在项目根，所以两处注入把搜索路径指回项目根 —— 否则导言区里的
//!   `\input{...}` 与正文里的 `\includegraphics{...}` 会全部找不到：
//!   ① `\input@path` 在**最前面**（`\documentclass` 之前），因为导言区自己的 `\input` 更早发生；
//!   ② `\graphicspath` 在 **`\begin{document}` 之后**，且用 `\@ifundefined` 守卫 —— 它是
//!      `graphicx` 提供的，放在导言区之前会得到 `! Undefined control sequence`（实测踩到），
//!      而没加载 `graphicx` 的文档里它同样不存在；
//! - **明确不解决**（免得误以为它是"文档里那一页"）：正文里定义的宏、`\cite`/`\ref`（没有 aux），
//!   以及浮动体/分页上下文 —— 片段文档的页号、行宽之外的版式都与真文档无关。
//!
//! 路径转换：TeX 只认正斜杠，且 `\input@path` / `\graphicspath` 的每一项**必须以 `/` 结尾**
//! （少了斜杠 LaTeX 会把它当文件前缀拼错）。

use std::path::Path;

/// 装配失败的原因（都是**可预期**的输入问题，不是内部错误）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnippetError {
    /// 根文件里找不到 `\begin{document}`（没打开项目、根文件选错、或文件不是完整文档）。
    NoBeginDocument,
    /// 片段是空的（调用方不该为空白触发编译）。
    EmptySnippet,
}

impl std::fmt::Display for SnippetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoBeginDocument => write!(
                f,
                "根文件里找不到 `\\begin{{document}}`：片段预览需要一份完整文档的导言区"
            ),
            Self::EmptySnippet => write!(f, "片段是空的"),
        }
    }
}

impl std::error::Error for SnippetError {}

/// `\begin{document}` 在源码里的位置（返回 `\begin{document}` 的**起始字节**）。///
/// 容忍 `\begin {document}` 这种带空格的写法？**不容忍** —— LaTeX 允许，但真实项目里没见过，
/// 而"猜"会让边界更难解释；找不到就如实报 [`SnippetError::NoBeginDocument`]。
fn begin_document_at(source: &str) -> Option<usize> {
    source.find("\\begin{document}")
}

/// TeX 路径：正斜杠 + 结尾 `/`（`\input@path` / `\graphicspath` 的硬要求）。
fn tex_dir(path: &Path) -> String {
    let mut s = path.to_string_lossy().replace('\\', "/");
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

/// 片段键：`「导言区 + 片段」→ 12 位十六进制`（roadmap ㊸ 切片 2）。
///
/// 用途：`<项目>/tmp/snippet/<键>/` 的目录名 —— 同一公式（同一导言区）落**同一个**目录，才能吃到
/// runner 已有的 **A 闸门**（页哈希相同 ⇒ 跳过 XDV→PDF 转换）与热 aux；换公式或改导言区都会换目录，
/// 因此不会把别的公式的产物当缓存复用。用标准库 `DefaultHasher`（与 `core::xdv::page_hashes` 同源），
/// **不引人新依赖**；它只是缓存键，**不是**安全用途的哈希。
pub fn snippet_key(preamble: &str, body: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    preamble.hash(&mut h);
    body.hash(&mut h);
    format!("{:012x}", h.finish() & 0xffff_ffff_ffff)
}

/// 装配片段文档。`project_root` 用来指回相对路径（见模块头的口径）。
pub fn build_snippet_document(
    root_source: &str,
    project_root: &Path,
    snippet: &str,
) -> Result<String, SnippetError> {
    if snippet.trim().is_empty() {
        return Err(SnippetError::EmptySnippet);
    }
    let at = begin_document_at(root_source).ok_or(SnippetError::NoBeginDocument)?;
    let preamble = &root_source[..at];
    let dir = tex_dir(project_root);

    let mut out = String::with_capacity(preamble.len() + snippet.len() + 256);
    // ① `\input@path` 必须在 `\documentclass` **之前**：导言区自己的 `\input{...}` 更早发生
    //    （`\input@path` 是 LaTeX 内部的搜索路径，越早设越好）。
    out.push_str("\\makeatletter\n");
    out.push_str(&format!("\\def\\input@path{{{{{dir}}}}}\n"));
    out.push_str("\\makeatother\n");
    out.push_str(preamble);
    // 切片 2 的决定：**注入 `\nofiles`**（roadmap ㊸ §6.11.9）——新目录本来是 2 趟（第 1 趟写出 `.aux`
    //   ⇒ 重跑判据判"变了"），而公式预览不需要 aux/toc ⇒ 1 趟，实测省 30–43%。
    //   ⚠ **必须放在导言区**：放进文档体会报 `Command \nofiles not allowed in the document body`（实测踩到）。
    out.push_str("\\nofiles\n");
    out.push_str("\\begin{document}\n");
    // ② `\graphicspath` 只能在文档体开始处设，且**必须守卫**：它是 `graphicx` 提供的，
    //    放在导言区之前会 `! Undefined control sequence`（真机实测踩到），
    //    而没加载 `graphicx` 的文档里它同样不存在。
    out.push_str("\\makeatletter\n");
    out.push_str(&format!(
        "\\@ifundefined{{graphicspath}}{{}}{{\\graphicspath{{{{{dir}}}}}}}\n"
    ));
    out.push_str("\\makeatother\n");
    // 关掉页式：片段卡要按**文字包围盒**裁图，而页脚页码会把包围盒撑到整页高
    // （真机实测：一张两行的片段被裁成 336×560，正文挤在顶部一条里）。
    out.push_str("\\pagestyle{empty}\n");
    out.push_str(snippet);
    if !snippet.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\\end{document}\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const DOC: &str = "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\nhi\n\\end{document}\n";

    /// 切片 2：公式键 = 「导言区 + 公式」的短哈希，用作 `tmp/snippet/<键>/` 的目录名（必须文件系统安全）。
    #[test]
    fn 公式键稳定_可区分_且只含十六进制() {
        let k1 = snippet_key("\\documentclass{article}\n", "$E=mc^2$");
        let k2 = snippet_key("\\documentclass{article}\n", "$E=mc^2$");
        let k3 = snippet_key("\\documentclass{article}\n", "$a^2+b^2=c^2$");
        let k4 = snippet_key("\\documentclass{book}\n", "$E=mc^2$");
        assert_eq!(k1, k2, "同输入必须同键（缓存与 A 闸门都靠它）");
        assert_ne!(k1, k3, "公式不同 ⇒ 键不同");
        assert_ne!(k1, k4, "导言区不同 ⇒ 键不同");
        assert!(k1.chars().all(|c| c.is_ascii_hexdigit()), "键要能直接当目录名：{k1}");
        assert_eq!(k1.len(), 12, "短哈希长度固定：{k1}");
    }

    /// 切片 2：`\nofiles` 必须在**导言区**（放文档体会报 `Command \nofiles not allowed…`）。
    #[test]
    fn nofiles_注入在导言区() {
        let out = build_snippet_document(DOC, &PathBuf::from("E:/proj"), "hello").unwrap();
        let nofiles = out.find("\\nofiles").expect("必须注入 \\nofiles");
        let body = out.find("\\begin{document}").expect("有文档体");
        assert!(nofiles < body, "`\\nofiles` 要在 `\\begin{{document}}` 之前：{out}");
        assert!(out.contains("\\usepackage{amsmath}\n\\nofiles\n"), "紧跟在导言区之后：{out}");
    }

    #[test]
    fn 导言区逐字照搬_片段插进文档体() {
        let out = build_snippet_document(DOC, &PathBuf::from("E:/proj"), "hello").unwrap();
        assert!(out.contains("\\documentclass{article}\n\\usepackage{amsmath}\n"));
        // 片段在文档体里、且原文档体（hi）不出现；两者之间允许有路径注入（见下一个用例）
        let body = out.find("\\begin{document}\n").unwrap();
        let snip = out.find("hello").unwrap();
        assert!(snip > body, "片段要在 \\begin{{document}} 之后");
        assert!(out.ends_with("hello\n\\end{document}\n"), "{out}");
        assert!(!out.contains("\nhi\n"), "片段文档不得带上原文档体");
    }

    #[test]
    fn 相对路径分两处注入且图形路径有守卫() {
        let out = build_snippet_document(DOC, &PathBuf::from("E:\\proj\\sub"), "x").unwrap();
        // ① \input@path：在 \documentclass 之前
        assert!(out.contains("\\def\\input@path{{E:/proj/sub/}}"), "正斜杠 + 结尾斜杠：{out}");
        let inj = out.find("\\input@path").unwrap();
        let cls = out.find("\\documentclass").unwrap();
        assert!(inj < cls, "\\input@path 要先于 \\documentclass");
        // ② \graphicspath：在 \begin{document} 之后，且被 \@ifundefined 守卫
        //    （放前面会 `! Undefined control sequence` —— 真机实测踩到）
        let gfx = out.find("graphicspath").unwrap();
        let bod = out.find("\\begin{document}").unwrap();
        assert!(gfx > bod, "\\graphicspath 要在 \\begin{{document}} 之后");
        assert!(out.contains("\\@ifundefined{graphicspath}"), "必须守卫：没加载 graphicx 的文档里它不存在");
        // ③ 页式必须关掉：页脚页码会把"文字包围盒"撑到整页高（真机实测 336×560）
        assert!(out.contains("\\pagestyle{empty}"), "片段文档要关页式，否则裁剪框被页码撑高");
    }

    #[test]
    fn 没有_begin_document_就如实报错() {
        let e = build_snippet_document("\\documentclass{article}\n", Path::new("E:/p"), "x").unwrap_err();
        assert_eq!(e, SnippetError::NoBeginDocument);
    }

    #[test]
    fn 空片段拒绝() {
        let e = build_snippet_document(DOC, Path::new("E:/p"), "   \n").unwrap_err();
        assert_eq!(e, SnippetError::EmptySnippet);
    }

    #[test]
    fn 片段末尾缺换行也能收尾() {
        let out = build_snippet_document(DOC, Path::new("E:/p"), "abc").unwrap();
        assert!(out.ends_with("abc\n\\end{document}\n"));
    }
}
