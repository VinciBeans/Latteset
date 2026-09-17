//! 数学公式扫描（roadmap ㊸ 的纯逻辑层）：给定**缓冲全文 + 光标字节偏移**，回答"光标在哪个公式里"。
//!
//! 为什么要它：悬停/光标停在公式上时要单独编译**这一个公式**（TeXStudio 式内联浮层）。前端只负责
//! "偏移 → 请求"，识别必须在这一层 —— 可单测、GUI 与 headless（CLI/MCP）共用同一份口径。
//!
//! ## 识别范围（与 `src/latexSyntax.ts` 的高亮状态同源，但这里要**边界**）
//!
//! | 形态 | 定界 | `kind` | 备注 |
//! |---|---|---|---|
//! | 行内 | `$…$` | [`MathKind::Inline`] | `$$…$$` 是**行间**，先判双记号 |
//! | 行内 | `\(…\)` | [`MathKind::Inline`] | |
//! | 行间 | `$$…$$`、`\[…\]` | [`MathKind::Display`] | |
//! | 环境 | `\begin{align}…\end{align}`（见 [`DISPLAY_ENVS`]） | [`MathKind::Env`] | **光标落在环境任意一行都返回整段环境**（产品决定，2026-09-17）：`align` 第 3 行要看到对齐后的整体，而不是孤立一行 |
//!
//! 内部环境（`split`/`cases`/`aligned`/`matrix`…）**不单独成式**：光标在 `equation` 里的 `split`
//! 中，返回的是外层 `equation`（整段）—— 单独排 `split` 根本排不出来。
//!
//! ## 刻意不猜的边界（都按"扫不到就是没有"处理，不伪造）
//!
//! - `\$` 转义不是定界符；`\\`（换行）之后的 `$` **是**定界符（所以按 TeX 的记号规则走：
//!   `\` + 字母 = 控制字（跳过字母，含尾部 `*`）；`\` + 非字母 = 转义符号（跳 2 字节））；
//! - `%` 到行尾是注释：里面的 `$` 一律不算（**公式内**的 `%` 同样如此，与 TeX 一致）；
//! - `\verb|…|` / `\verb*|…|` 里的内容一律不算；
//! - 未闭合的区域（`$x`、`\begin{align}` 还没有 `\end`、`\[` 还没有 `\]`）⇒ **返回 `None`**：
//!   打字中途很常见，但"给半个公式排个版"只会给出误导性的图；
//! - 空公式（`$$`、`$ $`、`\begin{equation}\end{equation}`）⇒ `None`（没有东西可排）；
//! - `$…$` 里出现 `$` 时只有**花括号深度为 0** 才收尾 ⇒ `$\text{b $c$} d$` 整体是外层公式。
//!
//! 偏移一律是**字节**偏移（Rust 字符串切片），不是字符下标 —— 中文文档里两者不同，单测专门盖了这一条。

use std::ops::Range;

/// 公式形态（决定前端怎么摆：行内贴光标、行间居中）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathKind {
    /// 行内：`$…$`、`\(…\)`。
    Inline,
    /// 行间：`$$…$$`、`\[…\]`。
    Display,
    /// 行间环境：`\begin{equation}…\end{equation}` 等（见 [`DISPLAY_ENVS`]）。
    Env,
}

/// 会**单独成式**的顶层数学环境（内部环境如 `split`/`cases`/`aligned` 不在此列）。
pub const DISPLAY_ENVS: &[&str] = &[
    "equation",
    "equation*",
    "align",
    "align*",
    "alignat",
    "alignat*",
    "gather",
    "gather*",
    "multline",
    "multline*",
    "flalign",
    "flalign*",
    "math",
    "displaymath",
    "eqnarray",
    "eqnarray*",
];

/// 光标所在的公式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathExpr {
    /// 在缓冲里的**字节**范围（半开区间，含定界符/环境标记）。
    pub range: Range<usize>,
    /// 公式正文（**含定界符**，可以直接喂给 [`crate::snippet::build_snippet_document`]）。
    pub text: String,
    /// 形态。
    pub kind: MathKind,
    /// 环境名（仅 [`MathKind::Env`]）。
    pub env: Option<String>,
}

/// 扫描器的内部状态：进了哪个区域。
enum Open {
    /// `$…$`
    InlineDollar { start: usize },
    /// `\(…\)`
    InlineParen { start: usize },
    /// `$$…$$`
    DisplayDollar { start: usize },
    /// `\[…\]`
    DisplayBracket { start: usize },
    /// `\begin{name}…`（`begin`/`end` 在进入时算好，别在每个字节上重算）
    Env {
        start: usize,
        name: String,
        begin: String,
        end: String,
    },
}

impl Open {
    fn start(&self) -> usize {
        match self {
            Open::InlineDollar { start }
            | Open::InlineParen { start }
            | Open::DisplayDollar { start }
            | Open::DisplayBracket { start }
            | Open::Env { start, .. } => *start,
        }
    }
}

/// 光标（字节偏移）落在哪个公式里；不在任何公式里（或公式不值得排）⇒ `None`。
///
/// `offset` 超出 `source` 长度时按"在末尾"处理（编辑器可能给到 EOF 之后一位）。
pub fn math_at(source: &str, offset: usize) -> Option<MathExpr> {
    let bytes = source.as_bytes();
    let offset = offset.min(bytes.len());
    let mut i = 0usize;
    let mut open: Option<Open> = None;
    // `$…$` 里的花括号深度（只有深度 0 的 `$` 收尾，见模块文档）。
    let mut brace = 0i32;

    while i < bytes.len() {
        let c = bytes[i];

        if let Some(state) = &open {
            // ---- 收尾判定（`\)`/`\]`/`$$` 都是"先看结尾"） ----
            let closed_end = match state {
                Open::InlineDollar { .. } => (c == b'$' && brace == 0).then_some(i + 1),
                Open::InlineParen { .. } => (c == b'\\' && bytes.get(i + 1) == Some(&b')'))
                    .then_some(i + 2),
                Open::DisplayDollar { .. } => (c == b'$' && bytes.get(i + 1) == Some(&b'$'))
                    .then_some(i + 2),
                Open::DisplayBracket { .. } => (c == b'\\' && bytes.get(i + 1) == Some(&b']'))
                    .then_some(i + 2),
                Open::Env { end, .. } => bytes[i..]
                    .starts_with(end.as_bytes())
                    .then_some(i + end.len()),
            };
            if let Some(end) = closed_end {
                // 命中判定必须是**闭区间左端 + 开区间右端**：只看 `offset < end` 会让公式**之前**的
                // 光标也算命中（单测 `non_math_environment_is_not_a_formula` 抓到的就是这个）。
                if state.start() <= offset && offset < end {
                    if let Some(expr) = finish(source, state, end) {
                        return Some(expr);
                    }
                    // 空公式不算：继续往后扫，别把"没有东西可排"当成命中
                }
                open = None;
                brace = 0;
                i = end;
                continue;
            }
            // ---- 公式内部：只处理会改变状态的东西 ----
            if c == b'%' {
                i = skip_comment(bytes, i);
                continue;
            }
            if matches!(state, Open::InlineDollar { .. }) {
                if c == b'{' {
                    brace += 1;
                } else if c == b'}' {
                    brace = (brace - 1).max(0);
                }
            }
            if c == b'\\' {
                i = skip_control(bytes, i);
                continue;
            }
            i += 1;
            continue;
        }

        // ---- 正文：找开头 ----
        if c == b'%' {
            i = skip_comment(bytes, i);
            continue;
        }
        if c == b'\\' {
            match bytes.get(i + 1) {
                Some(b'(') => {
                    open = Some(Open::InlineParen { start: i });
                    i += 2;
                }
                Some(b'[') => {
                    open = Some(Open::DisplayBracket { start: i });
                    i += 2;
                }
                Some(b'v') if bytes[i..].starts_with(b"\\verb") => {
                    i = skip_verb(bytes, i);
                }
                Some(b'b') if bytes[i..].starts_with(b"\\begin{") => {
                    match read_env_name(bytes, i) {
                        Some((name, after)) if DISPLAY_ENVS.contains(&name.as_str()) => {
                            let begin = format!("\\begin{{{name}}}");
                            let end = format!("\\end{{{name}}}");
                            open = Some(Open::Env { start: i, name, begin, end });
                            i = after;
                        }
                        Some((_, after)) => i = after,
                        None => i += 1,
                    }
                }
                _ => i = skip_control(bytes, i),
            }
            continue;
        }
        if c == b'$' {
            if bytes.get(i + 1) == Some(&b'$') {
                open = Some(Open::DisplayDollar { start: i });
                i += 2;
            } else {
                open = Some(Open::InlineDollar { start: i });
                brace = 0;
                i += 1;
            }
            continue;
        }
        i += 1;
    }

    // 走到文件末尾仍未收尾 ⇒ 不猜（见模块文档）
    None
}

/// 收尾善后：定形态与正文范围、判空；空公式返回 `None`（"没有东西可排"）。
///
/// 形态与定界符长度**从状态直接取**，不再回头猜字符串（`head`/`ends_with` 那种猜法在
/// `align*` 与嵌套场景里容易错）。
fn finish(source: &str, state: &Open, end: usize) -> Option<MathExpr> {
    let start = state.start();
    let (kind, env, body_range) = match state {
        Open::InlineDollar { .. } => (MathKind::Inline, None, (start + 1)..(end - 1)),
        Open::InlineParen { .. } => (MathKind::Inline, None, (start + 2)..(end - 2)),
        Open::DisplayDollar { .. } => (MathKind::Display, None, (start + 2)..(end - 2)),
        Open::DisplayBracket { .. } => (MathKind::Display, None, (start + 2)..(end - 2)),
        Open::Env { name, begin, end: end_marker, .. } => (
            MathKind::Env,
            Some(name.clone()),
            (start + begin.len())..(end - end_marker.len()),
        ),
    };
    if source[body_range].trim().is_empty() {
        return None;
    }
    Some(MathExpr {
        range: start..end,
        text: source[start..end].to_owned(),
        kind,
        env,
    })
}

/// `%` 之后跳到行尾（不含换行符本身）。
fn skip_comment(bytes: &[u8], at: usize) -> usize {
    let mut i = at;
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i
}

/// `\verb` / `\verb*`：跳到配对的定界符之后。定界符缺失或跨行 ⇒ 停在行尾（保守，别吞掉后文）。
fn skip_verb(bytes: &[u8], at: usize) -> usize {
    let mut i = at + "\\verb".len();
    if bytes.get(i) == Some(&b'*') {
        i += 1;
    }
    let Some(&delim) = bytes.get(i) else {
        return bytes.len();
    };
    i += 1;
    while i < bytes.len() {
        if bytes[i] == delim {
            return i + 1;
        }
        if bytes[i] == b'\n' {
            return i;
        }
        i += 1;
    }
    bytes.len()
}

/// 读 `\begin{name}` ⇒ `(name, 花括号之后的偏移)`。
fn read_env_name(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    let open = at + "\\begin{".len();
    let close = bytes[open..].iter().position(|b| *b == b'}')? + open;
    let name = std::str::from_utf8(&bytes[open..close]).ok()?.to_owned();
    Some((name, close + 1))
}

/// TeX 记号规则：`\` + 字母 = 控制字（跳过字母与尾部 `*`）；`\` + 非字母 = 转义符号（跳 2 字节）。
///
/// 这条规则正是 `\\$x$` 与 `\$` 的分水岭：前者 `\\` 是换行 ⇒ 后面的 `$` **是**定界符；
/// 后者是转义美元 ⇒ 不是定界符。
fn skip_control(bytes: &[u8], at: usize) -> usize {
    let mut i = at + 1;
    if i >= bytes.len() {
        return i;
    }
    if bytes[i].is_ascii_alphabetic() {
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'*') {
            i += 1;
        }
        i
    } else {
        i + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `|` 标出光标：取它的**字节**位置，并从文本里去掉。
    fn at_marked(marked: &str) -> (String, usize) {
        let offset = marked.find('|').expect("用例要标出光标");
        let source = marked.replacen('|', "", 1);
        (source, offset)
    }

    fn expr(marked: &str) -> Option<MathExpr> {
        let (source, offset) = at_marked(marked);
        math_at(&source, offset)
    }

    /// 行内 `$…$`：光标在正文、在定界符上都算命中；在公式外不算。
    #[test]
    fn inline_dollar_hits_inside_and_on_delimiters() {
        let e = expr("前 $|E=mc^2$ 后").expect("光标在 $ 上应命中");
        assert_eq!(e.kind, MathKind::Inline);
        assert_eq!(e.text, "$E=mc^2$");
        assert_eq!(&"前 $E=mc^2$ 后"[e.range.clone()], "$E=mc^2$");

        let (s, _) = at_marked("前 $E=mc^2$| 后");
        let inside = s.find("mc").expect("有内容") + 1;
        assert_eq!(math_at(&s, inside).expect("正文里应命中").text, "$E=mc^2$");
        assert!(math_at(&s, 0).is_none(), "公式之前不算");
        assert!(math_at(&s, s.len()).is_none(), "公式之后不算");
    }

    /// `$$…$$` 是**行间**；单个 `$` 是行内（先判双记号）。
    #[test]
    fn double_dollar_is_display() {
        let e = expr("$$ a|+b $$").expect("应命中");
        assert_eq!(e.kind, MathKind::Display);
        assert_eq!(e.text, "$$ a+b $$");
    }

    /// `\(…\)` 行内、`\[…\]` 行间。
    #[test]
    fn paren_and_bracket_delimiters() {
        assert_eq!(expr(r"\(|x\)").expect("应命中").kind, MathKind::Inline);
        let e = expr(r"\[|x\]").expect("应命中");
        assert_eq!(e.kind, MathKind::Display);
        assert_eq!(e.text, r"\[x\]");
    }

    /// 环境：光标落在**任意一行**都返回整段环境（产品决定：`align` 第 3 行要看到整体）。
    #[test]
    fn environment_returns_the_whole_block_from_any_line() {
        let src = "前文\n\\begin{align}\n  a &= b \\\\\n  c &= d\n\\end{align}\n";
        let third_line = src.find("c &=").expect("第三行") + 1;
        let e = math_at(src, third_line).expect("环境里应命中");
        assert_eq!(e.kind, MathKind::Env);
        assert_eq!(e.env.as_deref(), Some("align"));
        assert_eq!(e.text, "\\begin{align}\n  a &= b \\\\\n  c &= d\n\\end{align}");
        assert!(math_at(src, 0).is_none(), "环境之前（前缀里）不算");
        assert!(math_at(src, src.len() - 1).is_none(), "环境之后不算");
    }

    /// 带星号的环境名要整体认（`align*` 不能退化成 `align`）。
    #[test]
    fn starred_environment_name_is_kept() {
        let e = expr("\\begin{align*}|x\\end{align*}").expect("应命中");
        assert_eq!(e.env.as_deref(), Some("align*"));
        assert_eq!(e.kind, MathKind::Env);
    }

    /// 内部环境（`split`）不单独成式：返回外层 `equation`。
    #[test]
    fn inner_environment_resolves_to_the_outer_one() {
        let src = "\\begin{equation}\n\\begin{split}\n a &= b\n\\end{split}\n\\end{equation}";
        let inside = src.find("a &=").expect("内层正文") + 1;
        let e = math_at(src, inside).expect("应命中");
        assert_eq!(e.env.as_deref(), Some("equation"));
        assert!(e.text.ends_with("\\end{equation}"), "{}", e.text);
    }

    /// `\$` 不是定界符；`\\` 之后的 `$` 才是（TeX 记号规则的分水岭）。
    #[test]
    fn escaped_dollar_vs_linebreak() {
        let (s, off) = at_marked(r"价格 \$|100 而已");
        assert!(math_at(&s, off).is_none(), "转义美元不开公式");

        let e = expr("换行 \\\\$|x$").expect("`\\\\` 之后的 $ 应开公式");
        assert_eq!(e.text, "$x$");
    }

    /// 注释里的 `$` 不算；注释行之后另起一行的公式照常命中。
    #[test]
    fn comment_is_ignored() {
        let (s, off) = at_marked("% 这里 $| 不算\n正文");
        assert!(math_at(&s, off).is_none());

        let src = "% 注释 $x$\n$y$";
        let second = src.find("$y$").expect("第二行");
        assert_eq!(math_at(src, second).expect("应命中").text, "$y$");
    }

    /// `\verb|$|` 里的 `$` 不算。
    #[test]
    fn verbatim_is_ignored() {
        let (s, off) = at_marked(r"\verb|$| 之后");
        assert!(math_at(&s, off).is_none());
        let s2 = "\\verb*+$+ tail";
        assert!(math_at(s2, 8).is_none(), "`\\verb*` 也要跳过：{s2}");
    }

    /// `$…$` 里的嵌套 `$`（`\text{… $…$}`）只有花括号深度 0 才收尾 ⇒ 整体是外层公式。
    #[test]
    fn nested_dollar_inside_braces_stays_in_the_outer_formula() {
        let src = r"$a + \text{b $c$} d$";
        let e = math_at(src, 3).expect("应命中");
        assert_eq!(e.text, src, "整个外层公式（含嵌套）");
    }

    /// 未闭合 ⇒ `None`（打字中途不猜）。
    #[test]
    fn unterminated_regions_are_none() {
        assert!(math_at("$x", 1).is_none(), "行内未闭合");
        assert!(math_at("\\[x", 2).is_none(), "行间未闭合");
        assert!(math_at("\\begin{align}\na &= b\n", 15).is_none(), "环境未闭合");
    }

    /// 空公式 ⇒ `None`（没有东西可排）。
    #[test]
    fn empty_formulas_are_none() {
        assert!(math_at("$$", 0).is_none(), "空行间公式");
        assert!(math_at("$ $", 1).is_none(), "只有空白的行内公式");
        assert!(
            math_at("\\begin{equation}\n\\end{equation}", 20).is_none(),
            "空环境"
        );
        assert!(math_at("\\[\\]", 1).is_none(), "空 \\[\\]");
    }

    /// 中文缓冲里偏移是**字节**：光标在公式中间时不能被多字节字符带偏。
    #[test]
    fn byte_offsets_with_cjk_text() {
        let src = "中文 $x^2$ 中文";
        let dollar = src.find('$').expect("有公式");
        let close = dollar + "$x^2$".len() - 1;
        assert_eq!(math_at(src, dollar).expect("左定界符").text, "$x^2$");
        assert_eq!(math_at(src, dollar + 1).expect("正文首字节").text, "$x^2$");
        assert_eq!(math_at(src, close).expect("右定界符").text, "$x^2$");
        assert!(math_at(src, close + 1).is_none(), "公式之后一位不算");
        assert!(math_at(src, 0).is_none(), "第一个汉字不算");
        assert!(math_at(src, src.len()).is_none());
    }

    /// 环境名不在白名单里的（`\begin{itemize}`）不算公式；里面的行内公式照常命中。
    #[test]
    fn non_math_environment_is_not_a_formula() {
        let src = "\\begin{itemize}\n\\item $x$\n\\end{itemize}";
        assert!(math_at(src, 5).is_none(), "itemize 不是公式");
        let item = src.find("$x$").expect("里面有行内公式");
        assert_eq!(math_at(src, item).expect("行内公式仍应命中").text, "$x$");
    }

    /// 公式内的 `%` 按注释处理（与 TeX 一致）：注释里的 `$` 不收尾。
    #[test]
    fn percent_inside_math_swallows_the_line() {
        let src = "$a % 注释里的 $ 不收尾\n+ b$";
        let e = math_at(src, 1).expect("应命中");
        assert_eq!(e.text, src);
    }

    /// 公式跨行（行内也能跨行 —— TeX 允许，编辑器里常见于手写换行）。
    #[test]
    fn multiline_inline_formula() {
        let src = "文字 $a +\nb$ 文字";
        let e = math_at(src, src.find('$').expect("有公式") + 1).expect("应命中");
        assert_eq!(e.text, "$a +\nb$");
        assert_eq!(e.kind, MathKind::Inline);
    }
}
