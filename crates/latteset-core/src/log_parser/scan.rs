//! 逐行扫描状态机（modules.md §4 算法）。
//!
//! 信息局部性范例：`file_stack` 与聚合中的 `current` 都是本函数的局部变量，
//! 一次调用即生即灭——模块无缓存、无全局。

use super::model::{LogMessage, MessageKind};
use regex::Regex;

/// 行首 `l.<n>`（TeX 错误位置标记）。
static RE_LINE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^l\.(\d+)").expect("line regex")
});

/// 文件打开标记里的「已知源文件扩展名」（本仓解析关心的类别）。
const SOURCE_EXTENSIONS: &str =
    r"tex|cls|sty|def|clo|cfg|fd|ltx|dtx|ins";

/// 带已知扩展名的文件打开标记：从候选片段开头取到扩展名结束（容忍后面的杂项，
/// 如 `(./main.tex:12: …`）。
static RE_OPEN_WITH_EXT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(&format!(r"^(.*?\.(?:{SOURCE_EXTENSIONS}))"))
        .expect("open with ext regex")
});

/// 日志行里的文件栈标记（按出现顺序生效）。
enum FileMarker {
    /// 打开一个文件（名字可能已补上默认扩展名）。
    Open(String),
    /// 关闭当前文件。
    Close,
}

/// 从一行日志里按顺序提取文件栈标记（打开/关闭）。
///
/// 为什么不再用「行首 `(`」+「行尾扩展名」两条互斥正则（2026-11 review E3 实测）：
/// Tectonic 的真实布局是**一行里多个标记、行首还带一个空格**，而且 `\input{子/更深}`
/// 打印出来的名字**没有扩展名**：
///
/// ```text
///  (章节/第一章.tex (子/更深          ← 两个打开，都还没闭合
///  (章节/第一章.tex (子/更深.tex)     ← 打开、打开、关闭，全在同一行
/// ```
///
/// 旧规则两种都不命中 ⇒ 文件栈停在根文件，错误条目点击跳到 `main.tex` 的 `\begin{document}`。
///
/// 判定（保守优先，宁可漏一个也不把 TeX 的括号闲聊当文件名）：
/// 1. 候选片段带**已知源文件扩展名** → 算打开（与旧 `RE_OPEN` 的前缀语义一致）；
/// 2. 候选片段在**行尾且未闭合**，并且看起来是**多级路径**（含 `/`、无 TeX 括号闲聊字符）
///    → 算打开；没有扩展名时按 TeX 的默认补 `.tex`（`\input{子/更深}` 打印的就是这个名字）。
///    要求含分隔符是因为无扩展名的单名（`(Font`、`(biblatex`）与括号闲聊无法区分。
/// 未接受的括号对是**纯文本**（`(fontspec)`、`(XeTeX/luaotfload)`），整对跳过：
/// 不产生标记、也不消耗后面的 `)`（否则会把错误续行的括号当成关文件，把文件栈弹穿）。
fn file_markers(line: &str) -> Vec<FileMarker> {
    let bytes = line.as_bytes();
    let mut markers = Vec::new();
    let mut i = 0;
    while i < line.len() {
        match bytes[i] {
            b'(' => {
                let start = i + 1;
                let mut end = line.len();
                let mut partner_close = false;
                for (offset, b) in bytes[start..].iter().enumerate() {
                    if *b == b'(' || *b == b')' {
                        end = start + offset;
                        partner_close = *b == b')';
                        break;
                    }
                }
                let candidate = line[start..end].trim();
                match open_marker_name(candidate, end == line.len()) {
                    // 接受：打开；若同一行紧跟 `)`（`(article.cls)`）就成对消费
                    Some(name) => {
                        markers.push(FileMarker::Open(name));
                        if partner_close {
                            markers.push(FileMarker::Close);
                            i = end + 1;
                        } else {
                            i = end;
                        }
                    }
                    // 拒绝：括号里不是文件名 → 连它的配对 `)` 一起当文本跳过
                    None if partner_close => i = end + 1,
                    None => i = end,
                }
            }
            b')' => {
                markers.push(FileMarker::Close);
                i += 1;
            }
            _ => i += 1,
        }
    }
    markers
}

/// 括号里的候选片段是否是一个「打开文件」标记；是则返回要入栈的文件名。
///
/// `at_line_end` = 这个片段后面没有别的括号（未闭合的打开）。
fn open_marker_name(candidate: &str, at_line_end: bool) -> Option<String> {
    if candidate.is_empty() {
        return None;
    }
    // 1) 已知扩展名（`article.cls`、`./main.tex:12: …`）
    if let Some(cap) = RE_OPEN_WITH_EXT.captures(candidate) {
        let name = cap[1].trim();
        if !name.is_empty() {
            return Some(name.to_owned());
        }
    }
    // 2) 行尾未闭合 + 多级路径（无扩展名 / 带空格 / 多级目录）
    if !at_line_end || !looks_like_nested_path(candidate) {
        return None;
    }
    // 没扩展名 ⇒ 按 TeX `\input` 的默认补 `.tex`：日志打的是用户写的形式（`子/更深`），
    // 而源码文件叫 `子/更深.tex`——不补的话错误条目点击打不开（前端按原样解析路径）。
    let has_ext = std::path::Path::new(candidate)
        .extension()
        .is_some_and(|e| !e.is_empty());
    Some(if has_ext { candidate.to_owned() } else { format!("{candidate}.tex") })
}

/// 「多级路径」判定（无扩展名打开标记的兜底判据）。
///
/// 必须含 `/`：无扩展名的**单名**（`(Font`、`(biblatex`、`(3`）与 TeX 日志里的括号闲聊
/// 长得一模一样，只有多级路径才是可靠信号。反斜杠不收：`\end`、`\read` 这类命令名会让
/// `(cannot \read from terminal …)` 之类的闲聊混进来。
fn looks_like_nested_path(token: &str) -> bool {
    if token.is_empty() || token.len() > 1024 {
        return false;
    }
    if token
        .chars()
        .any(|c| matches!(c, '<' | '>' | '{' | '}' | '"' | '\'' | '`' | '=' | '\\' | '|' | '*' | '?' | ';'))
    {
        return false;
    }
    token.contains('/')
}

/// 解析 .log 全文。输入只有文本，输出只有结构化消息；无状态、无 IO。
pub fn parse_log(text: &str) -> Vec<LogMessage> {
    let mut messages: Vec<LogMessage> = Vec::new();
    // 函数内局部状态（信息局部性三级规则的"函数内"层）
    let mut file_stack: Vec<String> = Vec::new();
    let mut current: Option<LogMessage> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        let raw = line.to_string();

        if trimmed.starts_with('!') {
            // 新错误：先落盘聚合中的消息
            if let Some(m) = current.take() {
                messages.push(m);
            }
            current = Some(LogMessage {
                kind: MessageKind::Error,
                message: trimmed.to_string(),
                file: file_stack.last().cloned(),
                line: None,
                raw,
            });
        } else if let Some(cap) = RE_LINE.captures(trimmed) {
            // 位置标记：补 Error 的 line，并把**出错的源码行**并入消息。
            // 并入是为了诊断（roadmap ④）：`l.5 \usepackage{nope}` 里的命令名只有这一行有，
            // 丢掉它就无法给出"命令 X 未定义"。原始需求见 modules.md §4（只补行号），
            // 此处扩展为"行号 + 该行原文"，前端仍只展示首行，无观感变化。
            if let Some(m) = current.as_mut() {
                if m.kind == MessageKind::Error {
                    m.line = cap[1].parse().ok();
                    m.message.push('\n');
                    m.message.push_str(trimmed);
                }
            }
        } else if apply_file_markers(line, &mut file_stack) {
            // 本行的括号都是文件栈标记：按出现顺序 pop/push（同一行可能先关后开、开多个）。
            // 放在 warning/noise 之前 = 保持旧 `RE_OPEN` 的分类优先级：真打开标记优先于文本分类。
            // 没有标记的行照常落到后面的分支（错误续行不受影响）。
        } else if is_warning_line(trimmed) {
            if let Some(m) = current.take() {
                messages.push(m);
            }
            current = Some(LogMessage {
                kind: MessageKind::Warning,
                message: trimmed.to_string(),
                file: file_stack.last().cloned(),
                line: None,
                raw,
            });
        } else if is_noise(trimmed) {
            // 噪音行：聚合中也不追加
        } else if let Some(m) = current.as_mut() {
            // 错误/警告的续行：非空即追加（保留可读性）
            m.message.push('\n');
            m.message.push_str(trimmed);
        }
    }

    if let Some(m) = current.take() {
        messages.push(m);
    }
    messages
}

/// 把本行的文件栈标记按顺序应用到栈上；本行没有任何标记时返回 `false`（交给后续分类）。
fn apply_file_markers(line: &str, file_stack: &mut Vec<String>) -> bool {
    let markers = file_markers(line);
    if markers.is_empty() {
        return false;
    }
    for marker in markers {
        match marker {
            FileMarker::Open(name) => file_stack.push(name),
            FileMarker::Close => {
                file_stack.pop();
            }
        }
    }
    true
}

/// 警告判定：LaTeX/Package Warning 行，或 Overfull/Underfull 盒子警告。
fn is_warning_line(trimmed: &str) -> bool {
    trimmed.contains("Warning")
        || trimmed.starts_with("Overfull")
        || trimmed.starts_with("Underfull")
}

/// 噪音判定：latexmk 元信息、TeX 回显行、交互提示。
fn is_noise(trimmed: &str) -> bool {
    trimmed == "?"
        || trimmed.starts_with("Latexmk:")
        || trimmed.starts_with("===")
        || trimmed.starts_with("Run number")
        || trimmed.starts_with("This is ")
        || trimmed.starts_with("entering extended mode")
        || trimmed.starts_with("restricted \\write18")
        || trimmed.starts_with("Document Class:")
        || trimmed.starts_with("Type X to quit")
        || trimmed.starts_with("or enter new name")
        || trimmed.starts_with("Enter file name:")
        || trimmed.starts_with("*** ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_log_yields_nothing() {
        assert!(parse_log("").is_empty());
        assert!(parse_log("\n\n").is_empty());
    }

    #[test]
    fn pure_latexmk_noise_yields_nothing() {
        let log = "Latexmk: Run number 1 of rule 'xelatex'\n\
                   ===========Latexmk: All targets are up-to-date===========\n";
        assert!(parse_log(log).is_empty());
    }

    #[test]
    fn error_with_line_number() {
        let log = "! LaTeX Error: File `nope.sty' not found.\n\
                   l.5 \\usepackage{nope}\n\
                   ?\n";
        let msgs = parse_log(log);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].kind, MessageKind::Error);
        assert_eq!(msgs[0].line, Some(5));
        assert!(msgs[0].message.contains("File `nope.sty' not found."));
        // "?" 噪音不进入聚合
        assert!(!msgs[0].message.contains('?'));
    }

    #[test]
    fn error_associated_with_current_file_from_stack() {
        let log = "(./main.tex\n\
                   ! LaTeX Error: Something broke.\n\
                   l.12 \\section{X}\n\
                   )\n";
        let msgs = parse_log(log);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].kind, MessageKind::Error);
        assert_eq!(msgs[0].file.as_deref(), Some("./main.tex"));
        assert_eq!(msgs[0].line, Some(12));
    }

    #[test]
    fn file_stack_push_pop_nested() {
        let log = "(./main.tex\n\
                   (/usr/share/texlive/texmf-dist/tex/latex/base/article.cls\n\
                   )\n\
                   ! LaTeX Error: In main.\n\
                   l.3 \\begin{document}\n\
                   )\n";
        let msgs = parse_log(log);
        assert_eq!(msgs.len(), 1);
        // cls 已 pop，栈顶回到 main.tex
        assert_eq!(msgs[0].file.as_deref(), Some("./main.tex"));
    }

    /// Tectonic 的日志把「关闭上一个文件」与「打开下一个文件」写在同一行
    /// （`) (l3backend-xetex.def`），而且部分打开标记写在行中间（`… (article.cls`）。
    /// 任一处理反了，文件栈就整体错位，错误条目的 `file` 变成 None（点击跳转失效）——
    /// 2026-11 用真实 Tectonic 日志复现。
    #[test]
    fn tectonic_log_layout_keeps_file_stack() {
        let log = "(中文主文件.tex\n\
                   LaTeX2e <2022-06-01>\n\
                   L3 programming layer <2022-02-24> (article.cls\n\
                   Document Class: article 2021/10/04 v1.4n Standard LaTeX document class\n\
                   (size10.clo\n\
                   )\n\
                   ) (l3backend-xetex.def\n\
                   )\n\
                   ! Undefined control sequence.\n\
                   l.3 \\undefinedcommandhere\n";
        let msgs = parse_log(log);
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs[0].file.as_deref(),
            Some("中文主文件.tex"),
            "错误必须关联到根文件，而不是 None/宏包文件"
        );
        assert_eq!(msgs[0].line, Some(3));
    }

    /// review E3 夹具 1（真实 Tectonic `.log`，`test_file/review/probe-nested-input`）：
    /// `\input{子/更深}`（显式无扩展名）后，打开标记写在行中间、行首还有一个空格，
    /// 且两个打开**都未闭合**（` (章节/第一章.tex (子/更深`）。
    ///
    /// 旧规则（行首 `(` + 行尾带扩展名）两个都不命中 ⇒ 错误归到 `main.tex:2`（`\begin{document}`），
    /// 点击跳错文件。`子/更深` 是日志里用户写的形式，真实源码是 `子/更深.tex`，故补默认扩展名。
    #[test]
    fn nested_input_without_extension_attributes_error_to_child() {
        let log = "(main.tex\n\
                   LaTeX2e <2021-11-15> patch level 1\n\
                   L3 programming layer <2022-02-24> (article.cls\n\
                   (size10.clo\n\
                   )\n\
                   ) (l3backend-xetex.def\n\
                   )\n\
                   \\openout2 = `章节/第一章.aux'.\n\
                   \n\
                    (章节/第一章.tex (子/更深\n\
                   ! Undefined control sequence.\n\
                   l.2 \\undefinedxd\n";
        let msgs = parse_log(log);
        assert_eq!(msgs.len(), 1, "{msgs:?}");
        assert_eq!(
            msgs[0].file.as_deref(),
            Some("子/更深.tex"),
            "错误必须归到最深的那层 \\input 子文件，而不是 main.tex"
        );
        assert_eq!(msgs[0].line, Some(2));
    }

    /// review E3 夹具 2（真实 Tectonic `.log`，`test_file/review/probe-nested-input2`）：
    /// `\include` 嵌套行 ` (章节/第一章.tex (子/更深.tex)` —— 打开、打开、关闭全在同一行，
    /// 行首带空格且行尾是 `)`。修好后栈顶应回到 `章节/第一章.tex`（错误实际发生在父文件）。
    #[test]
    fn include_nested_open_close_on_same_line_keeps_parent_file() {
        let log = "(main.tex\n\
                   LaTeX2e <2021-11-15> patch level 1\n\
                   L3 programming layer <2022-02-24> (article.cls\n\
                   (size10.clo\n\
                   )\n\
                   ) (l3backend-xetex.def\n\
                   )\n\
                   \\openout2 = `章节/第一章.aux'.\n\
                   \n\
                    (章节/第一章.tex (子/更深.tex)\n\
                   ! Undefined control sequence.\n\
                   l.3 \\undefinedinchapter\n";
        let msgs = parse_log(log);
        assert_eq!(msgs.len(), 1, "{msgs:?}");
        assert_eq!(
            msgs[0].file.as_deref(),
            Some("章节/第一章.tex"),
            "同一行内成对闭合的子文件必须被弹出，错误留在父文件"
        );
        assert_eq!(msgs[0].line, Some(3));
    }

    /// TeX 日志里的括号闲聊（`(fontspec)`、`(XeTeX/luaotfload)`）**不得**当成文件栈标记：
    /// 它们多是错误续行，误判会一边把名字塞进文件栈、一边丢掉续行里的关键信息（字体名等）。
    #[test]
    fn parenthetical_chatter_is_not_a_file_marker() {
        let log = "(./main.tex\n\
                   ! Package fontspec Error: \n\
                   (fontspec)                The font \"SourceHanSerifSC-Regular\" cannot be\n\
                   (fontspec)                found (XeTeX/luaotfload).\n\
                   *** (job aborted, no legal \\end found)\n";
        let msgs = parse_log(log);
        assert_eq!(msgs.len(), 1, "{msgs:?}");
        assert_eq!(msgs[0].file.as_deref(), Some("./main.tex"));
        assert!(
            msgs[0].message.contains("SourceHanSerifSC-Regular"),
            "续行必须并入消息：{}",
            msgs[0].message
        );
        assert!(!msgs[0].message.contains("job aborted"));
    }

    #[test]
    fn warning_classification() {
        let log = "LaTeX Warning: Citation `foo' undefined on page 1.\n\
                   Overfull \\hbox (12.3pt too wide) in paragraph at lines 4--5\n\
                   Package fancyhdr Warning: \\(\\headheight\\) is too small.\n";
        let msgs = parse_log(log);
        assert_eq!(msgs.len(), 3);
        for m in &msgs {
            assert_eq!(m.kind, MessageKind::Warning);
        }
    }

    #[test]
    fn error_continuation_lines_aggregated() {
        let log = "! Undefined control sequence.\n\
                   l.10 \\foo\n\
                   bar baz\n";
        let msgs = parse_log(log);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].message.contains("Undefined control sequence"));
        assert!(msgs[0].message.contains("bar baz"));
    }

    #[test]
    fn multiple_errors_all_reported() {
        let log = "! First error.\n\
                   l.1 a\n\
                   ! Second error.\n\
                   l.2 b\n";
        let msgs = parse_log(log);
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].message.contains("First error"));
        assert!(msgs[1].message.contains("Second error"));
        assert_eq!(msgs[1].line, Some(2));
    }

    /// 真实感样例：内容错误（快照测试，modules.md §4）。
    #[test]
    fn snapshot_content_error_log() {
        let log = r#"Latexmk: Run number 1 of rule 'xelatex'
This is XeTeX, Version 3.141592653-2.6-0.999996 (TeX Live 2024) (preloaded format=xelatex)
 restricted \write18 enabled.
entering extended mode
(/usr/local/texlive/2024/texmf-dist/tex/latex/base/article.cls
Document Class: article 2023/05/17 v1.4n Standard LaTeX document class
(/usr/local/texlive/2024/texmf-dist/tex/latex/base/size10.clo))
(./main.tex
LaTeX Warning: Citation `foo' undefined on page 1.
! LaTeX Error: File `nope.sty' not found.

Type X to quit or <RETURN> to proceed,
or enter new name. (Extension to read file)
Enter file name: 
l.5 \usepackage{nope}

?
! Emergency stop.
<*> main.tex

*** (job aborted, no legal \end found)
"#;
        insta::assert_debug_snapshot!(parse_log(log));
    }

    /// 真实场景：\input 的子文件含 \documentclass（2026-08 实测：错误雪崩 + 溢出）。
    #[test]
    fn snapshot_error_avalanche_log() {
        let log = r#"Latexmk: Run number 1 of rule 'xelatex'
(./main.tex
(./chapters/intro.tex
! LaTeX Error: Can be used only in preamble.
l.1 \documentclass

? 
! LaTeX Error: Can be used only in preamble.
l.5 \usepackage{amsmath}

? 
! TeX capacity exceeded, sorry [text input levels=15].
<argument> ...efore}\@@input "chapters/intro.tex"
l.17 \input{chapters/intro}
"#;
        insta::assert_debug_snapshot!(parse_log(log));
    }

    /// 真实感样例：仅警告（快照测试）。
    #[test]
    fn snapshot_warning_only_log() {
        let log = r#"Latexmk: Run number 1 of rule 'xelatex'
(./main.tex
LaTeX Warning: Citation `bar' undefined on page 1.
Overfull \hbox (12.3pt too wide) in paragraph at lines 4--5
) 
(./chapters/intro.tex
Package fancyhdr Warning: \headheight is too small.
)
"#;
        insta::assert_debug_snapshot!(parse_log(log));
    }
}
