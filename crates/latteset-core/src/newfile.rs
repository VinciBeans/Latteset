//! 新建 `.tex` 的**最小骨架**（roadmap ㊺ §6.13.1-A 的纯逻辑层）。
//!
//! 为什么在 core：
//! 1. **可单测** —— "语言 × 类型"就是一张表，口径写死在这里，别让前端/命令面各拼一份；
//! 2. **两个入口共用** —— GUI 的新手向导与将来的 headless（"给 Agent 建一个中文学位论文骨架"）；
//! 3. 它同时是**"空文件探测不出根"的解药**：骨架里**必有 `\documentclass`** ⇒ 新建完立刻能被
//!    根探测认成根文档，"空目录建完第一个文件就能编"这条路才闭合（见 roadmap §6.13/§6.13.1）。
//!
//! 取舍（面向新手，刻意保持少）：
//! - 只给 `amsmath` 一个宏包 —— 它是绝大多数文档都要用的，且**不需要额外字体**（`amssymb` 的
//!   `\mathbb` 在缺字体的 bundle 下会转换失败，不适合放进默认骨架）；
//! - 中文一律走 **ctex 类**（`ctexart`/`ctexrep`/`ctexbook`/`ctexbeamer`）而不是"article + ctex 宏包"：
//!   少一个 `\usepackage`、少一处能写错的地方；
//! - beamer 不给 `\maketitle`（它要在 frame 里用 `\titlepage`），直接给一页可写的 frame。

use serde::{Deserialize, Serialize};
use specta::Type;

/// 文档语言（决定用 ctex 类还是标准类）。
///
/// 序列化名（`chinese` / `english`）就是前端 `NewFileWizard.vue` 传过来的字面量：这张表只此一份，
/// 前端不另抄一遍（modules.md §8 命令面「DTO 进出、无业务逻辑」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DocLanguage {
    /// 中文（ctex 类：`\documentclass[UTF8]{ctexart}` …）。
    Chinese,
    /// 英文（标准类：`article` / `report` / `book` / `beamer`）。
    English,
}

/// 文档类型（面向新手只放最常见的四种）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    /// 短文 / 论文（`article` / `ctexart`）。
    Article,
    /// 报告（`report` / `ctexrep`）。
    Report,
    /// 书（`book` / `ctexbook`）。
    Book,
    /// 幻灯片（`beamer` / `ctexbeamer`）。
    Beamer,
}

impl DocKind {
    /// 该类在给定语言下的**类名**（`ctex` 前缀只加在中文上）。
    pub fn class_name(self, lang: DocLanguage) -> &'static str {
        match (lang, self) {
            (DocLanguage::Chinese, DocKind::Article) => "ctexart",
            (DocLanguage::Chinese, DocKind::Report) => "ctexrep",
            (DocLanguage::Chinese, DocKind::Book) => "ctexbook",
            (DocLanguage::Chinese, DocKind::Beamer) => "ctexbeamer",
            (DocLanguage::English, DocKind::Article) => "article",
            (DocLanguage::English, DocKind::Report) => "report",
            (DocLanguage::English, DocKind::Book) => "book",
            (DocLanguage::English, DocKind::Beamer) => "beamer",
        }
    }
}

/// 生成最小骨架。`title` 为空时不写 `\title`（也别留空行噪声）。
///
/// **不变量**（单测钉住）：产物**一定**含 `\documentclass`、`\begin{document}`、`\end{document}`；
/// 且**占位文字随语言走**——给英文文档塞"第一节"是上一版真机验收时看出来的（英文书里冒出汉字）。
pub fn document_skeleton(lang: DocLanguage, kind: DocKind, title: &str) -> String {
    let class = kind.class_name(lang);
    let options = if lang == DocLanguage::Chinese { "[UTF8]" } else { "" };
    // 占位文字：只有语言决定它（与 `\documentclass` 的选择同源，不另开一个开关）
    let (section, frame) = match lang {
        DocLanguage::Chinese => ("第一节", "第一页"),
        DocLanguage::English => ("Introduction", "First frame"),
    };
    let title = title.trim();
    let mut out = String::new();
    out.push_str(&format!("\\documentclass{options}{{{class}}}\n"));
    out.push_str("\\usepackage{amsmath}\n");
    if !title.is_empty() {
        out.push_str(&format!("\\title{{{title}}}\n"));
    }
    out.push_str("\\begin{document}\n");
    match kind {
        DocKind::Beamer => {
            // beamer 不要 `\maketitle`（要 `\frame{\titlepage}`）—— 新手向导就给一页能直接写的 frame。
            let head = if title.is_empty() { frame } else { title };
            out.push_str(&format!("\\begin{{frame}}{{{head}}}\n\n\\end{{frame}}\n"));
        }
        _ => {
            if !title.is_empty() {
                out.push_str("\\maketitle\n\n");
            }
            out.push_str(&format!("\\section{{{section}}}\n\n"));
        }
    }
    out.push_str("\\end{document}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [DocKind; 4] = [DocKind::Article, DocKind::Report, DocKind::Book, DocKind::Beamer];

    /// **不变量**：任何组合都含 `\documentclass` / `\begin{document}` / `\end{document}`
    /// —— 前一条正是"新建完立刻能被根探测认出来"的前提（roadmap ㊺）。
    #[test]
    fn every_skeleton_has_the_root_detection_markers() {
        for lang in [DocLanguage::Chinese, DocLanguage::English] {
            for kind in KINDS {
                let s = document_skeleton(lang, kind, "标题");
                assert!(s.contains("\\documentclass"), "{lang:?}/{kind:?} 缺 documentclass：{s}");
                assert!(s.contains("\\begin{document}"), "{lang:?}/{kind:?} 缺 begin document");
                assert!(s.contains("\\end{document}"), "{lang:?}/{kind:?} 缺 end document");
                assert!(s.ends_with("\\end{document}\n"), "末行要收在 end document");
            }
        }
    }

    /// 中文走 ctex 类（带 `[UTF8]`），英文走标准类（不带选项）。
    #[test]
    fn language_picks_the_class() {
        assert!(document_skeleton(DocLanguage::Chinese, DocKind::Article, "").starts_with("\\documentclass[UTF8]{ctexart}"));
        assert!(document_skeleton(DocLanguage::Chinese, DocKind::Book, "").contains("{ctexbook}"));
        assert!(document_skeleton(DocLanguage::Chinese, DocKind::Beamer, "").contains("{ctexbeamer}"));
        assert!(document_skeleton(DocLanguage::English, DocKind::Article, "").starts_with("\\documentclass{article}"));
        assert!(document_skeleton(DocLanguage::English, DocKind::Report, "").contains("{report}"));
        assert!(!document_skeleton(DocLanguage::English, DocKind::Book, "").contains("ctex"));
    }

    /// 标题：有就写、没有就不写（不留空 `\title{}`）；beamer 不能有 `\maketitle`。
    #[test]
    fn title_is_optional_and_beamer_never_gets_maketitle() {
        let with = document_skeleton(DocLanguage::English, DocKind::Article, "  我的论文  ");
        assert!(with.contains("\\title{我的论文}\n"), "标题要去掉首尾空白：{with}");
        assert!(with.contains("\\maketitle"));
        let without = document_skeleton(DocLanguage::English, DocKind::Article, "   ");
        assert!(!without.contains("\\title"), "空白标题不该写 \\title：{without}");
        assert!(!without.contains("\\maketitle"), "没标题就不该 \\maketitle");
        let beamer = document_skeleton(DocLanguage::Chinese, DocKind::Beamer, "演示");
        assert!(!beamer.contains("\\maketitle"), "beamer 用 frame 而不是 \\maketitle：{beamer}");
        assert!(beamer.contains("\\begin{frame}{演示}"));
    }

    /// 只给 `amsmath` 一个宏包：`amssymb` 的符号字体在缺字体的 bundle 下会让预览转换失败。
    #[test]
    fn only_amsmath_is_preloaded() {
        let s = document_skeleton(DocLanguage::English, DocKind::Article, "x");
        assert!(s.contains("\\usepackage{amsmath}"));
        assert!(!s.contains("amssymb"), "默认骨架不引入需要额外字体的宏包");
    }

    /// 占位文字随语言：英文文档里不该出现汉字（真机验收时英文 book 的目录里冒出过"第一节"）。
    #[test]
    fn placeholder_text_follows_the_language() {
        let has_cjk = |s: &str| s.chars().any(|c| c as u32 >= 0x2E80);
        for kind in KINDS {
            let en = document_skeleton(DocLanguage::English, kind, "Title");
            assert!(!has_cjk(&en), "英文 {kind:?} 骨架里不该有汉字：{en}");
            let cn = document_skeleton(DocLanguage::Chinese, kind, "标题");
            assert!(has_cjk(&cn), "中文骨架该有中文占位：{cn}");
        }
        // 无标题时 beamer 的 frame 标题也是分语言的
        assert!(document_skeleton(DocLanguage::English, DocKind::Beamer, "").contains("\\begin{frame}{First frame}"));
        assert!(document_skeleton(DocLanguage::Chinese, DocKind::Beamer, "").contains("\\begin{frame}{第一页}"));
    }

    /// 与前端交换的**字面量**（`NewFileWizard.vue` 的选择值 → 命令参数 → 这里）：
    /// 钉住它们，改名就得同时改前端（编译期不会替我们抓这个）。
    #[test]
    fn wire_names_are_the_snake_case_literals() {
        for (lang, want) in [(DocLanguage::Chinese, "\"chinese\""), (DocLanguage::English, "\"english\"")] {
            assert_eq!(serde_json::to_string(&lang).unwrap(), want);
        }
        for (kind, want) in [
            (DocKind::Article, "\"article\""),
            (DocKind::Report, "\"report\""),
            (DocKind::Book, "\"book\""),
            (DocKind::Beamer, "\"beamer\""),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), want);
        }
    }
}
