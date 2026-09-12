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

/// 文件打开标记 `(path.ext`（.tex/.sty/.cls/...），捕获到扩展名结束。
static RE_OPEN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\(+([^()\r\n]*?\.(?:tex|cls|sty|def|clo|cfg|fd|ltx|dtx|ins))")
        .expect("open regex")
});

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
        } else if let Some(cap) = RE_OPEN.captures(line) {
            file_stack.push(cap[1].to_string());
        } else if trimmed.starts_with(')') {
            let closes = trimmed.chars().take_while(|c| *c == ')').count();
            for _ in 0..closes {
                file_stack.pop();
            }
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
