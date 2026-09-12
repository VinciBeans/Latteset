//! 错误诊断（roadmap ④）：把 `.log` 里的原始报错翻译成「原因 + 怎么改」。
//!
//! 设计要点：
//! - **纯函数**：输入 [`LogMessage`]（`.log` 已聚合的消息），输出 [`Diagnosis`]；无 IO、无状态，全量可单测。
//! - **只在有把握时给建议**：匹配不到就返回 `None`，由前端降级为「原文 + 行号」——宁可不说，也不要瞎说。
//! - **规则全部由真实报错驱动**：语料见 [`crate::log_parser::real_error_corpus`]（23 例，来自 ⑲ 模板编译
//!   矩阵的真实失败 + 故意写错的最小文档），期望值在 `diagnosis_tests.rs` 手写，避免自证循环。
//!
//! 为什么值得做：调研里"错误信息不可读"是全行业最高频痛点（`errors` 标签 5,516 题、
//! TeXstudio 只显示 `Process exited with error(s)`）。而 ⑲ 调研又证明**引擎自动推断没有必要**
//! ——正确的替代是：**不猜，但猜错时明确告诉用户怎么改**（见 [`DiagnosisKind::EngineMismatch`]）。

use super::model::{LogMessage, MessageKind};
use serde::{Deserialize, Serialize};
use specta::Type;

/// 诊断类别（前端据此选图标/分组；测试据此断言）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosisKind {
    /// 缺宏包（`.sty`）。
    MissingPackage,
    /// 缺文档类（`.cls`）。
    MissingClass,
    /// 缺其他文件（图片、`\input` 的子文件等，无 `.sty`/`.cls` 扩展名）。
    MissingFile,
    /// 系统缺字体（fontspec）。
    MissingFont,
    /// ctex 字体集不存在（`fontset=` 取值错）。
    MissingFontset,
    /// 引擎不匹配：文档需要 XeLaTeX/LuaLaTeX，当前跑的是 pdfLaTeX（或反之）。
    EngineMismatch,
    /// 需要本产品不支持的引擎（如日文 pLaTeX/upLaTeX）。
    UnsupportedEngine,
    /// 未定义控制序列。
    UndefinedControlSequence,
    /// 组未闭合（`File ended while scanning use of ...`）。
    UnclosedGroup,
    /// 数学模式外的 `_`/`^`。
    MissingMathMode,
    /// 多余的 `}`。
    ExtraBrace,
    /// 缺少 `\begin{document}`。
    MissingBeginDocument,
    /// 重复上下标。
    DoubleScript,
    /// 表格/对齐环境之外的 `&`。
    MisplacedAlignment,
    /// `Emergency stop`（多为连锁反应）。
    EmergencyStop,
    /// 宏包选项冲突。
    OptionClash,
    /// 源文件含非法 UTF-8（常见于 GBK 旧文件）。
    NonUtf8Source,
    /// **编译超时但在推进**（日志里有页输出）：文档比上限大，不是卡住（roadmap ㉕）。
    CompileTimeout,
    /// **编译超时且无推进证据**（日志里没有任何页输出）：可能仍在做前置处理，也可能真卡住（roadmap ㉕）。
    CompileTimeoutStalled,
    /// **写不出中间文件**：`\include{子目录/文件}` 需要 `tmp/子目录/` 存在，而它不存在（roadmap ㉖ 的首个实测证据）。
    AuxWriteFailed,
    /// 其他宏包报错（兜底，至少点出宏包名）。
    PackageError,
    /// 其他 LaTeX 报错（兜底）。
    LatexError,
}

/// 一条诊断：原因 + 可操作建议。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Diagnosis {
    pub kind: DiagnosisKind,
    /// 人话原因（一句话）。
    pub cause: String,
    /// 怎么改（可操作步骤）。无法给出时为空。
    pub hint: Option<String>,
    /// **一键操作的机器可读参数**（roadmap ㉕）：目前只有超时诊断会给——
    /// "把 `compile.timeout_secs` 提到这个值再重试一次"。前端据此渲染按钮，
    /// 避免把"该调到多少"这条规则在前后端各写一遍。
    pub suggested_timeout_secs: Option<u32>,
    /// **缺失的文件名**（roadmap ㉗）：仅"缺文件/缺宏包/缺文档类"三类会给（如 `thuthesis.cls`）。
    /// 调用方据此去项目里找线索（例如同名的 `.ins`/`.dtx` → 源码版模板需先编译生成 `.cls`）。
    pub missing_file: Option<String>,
}

fn d(kind: DiagnosisKind, cause: impl Into<String>, hint: impl Into<String>) -> Diagnosis {
    Diagnosis {
        kind,
        cause: cause.into(),
        hint: Some(hint.into()),
        suggested_timeout_secs: None,
        missing_file: None,
    }
}

/// 带"缺失文件名"的诊断（缺宏包/缺文档类/缺文件三类用，roadmap ㉗）。
fn d_missing(
    kind: DiagnosisKind,
    cause: impl Into<String>,
    hint: impl Into<String>,
    missing_file: impl Into<String>,
) -> Diagnosis {
    Diagnosis {
        kind,
        cause: cause.into(),
        hint: Some(hint.into()),
        suggested_timeout_secs: None,
        missing_file: Some(missing_file.into()),
    }
}

/// 源码版模板提示（roadmap ㉗）：项目里确实有 `.ins`/`.dtx` 时，把"可能需先编 .ins"换成**具体命令**。
///
/// 为什么值得单独判：⑲ 实测三个真实模板（thuthesis / BUCTthesis / hithesis）的 GitHub **源码版**
/// 只带 `.ins`/`.dtx`，直接编译必然报 `File 'X.cls' not found`。泛泛的"可能需先编译 .ins"用户
/// 看不懂下一步做什么；点名文件 + 给出命令才是可执行的。
///
/// 优先级（实测教训：两者同名时会按目录序选中 `.dtx`，而 `.dtx` 是**文档源码**、不是安装脚本，
/// 直接 `xelatex X.dtx` 只会排版出文档而不是生成 `.cls`）：
/// 1. 同名 `.ins` → 给具体命令；
/// 2. 项目里唯一的 `.ins` → 给具体命令；
/// 3. 只有同名 `.dtx`（没有 `.ins`）→ 说明是源码版并指向模板 README（不瞎给命令）；
/// 4. 其余 → `None`（保留原来的泛化建议）。
pub fn source_release_hint(missing_file: &str, candidates: &[String]) -> Option<String> {
    let stem = std::path::Path::new(missing_file)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())?;
    let lower = |n: &String| n.to_lowercase();
    let same_stem = |n: &String, ext: &str| lower(n).starts_with(&stem) && lower(n).ends_with(ext);
    let ins_files: Vec<&String> = candidates.iter().filter(|n| lower(n).ends_with(".ins")).collect();

    let picked = candidates
        .iter()
        .find(|n| same_stem(n, ".ins"))
        .or_else(|| if ins_files.len() == 1 { ins_files.first().copied() } else { None });
    if let Some(picked) = picked {
        return Some(format!(
            "项目里有源码版模板文件 `{picked}`：先在项目目录执行 `xelatex {picked}` 生成缺失的 `{missing_file}`，再重新编译（这类模板只发布 .ins/.dtx，不直接带 .cls）"
        ));
    }

    // 只有 .dtx：它是文档源码，不能直接生成 .cls——给出方向但不编造命令
    if let Some(dtx) = candidates.iter().find(|n| same_stem(n, ".dtx")) {
        return Some(format!(
            "项目里有源码版模板源码 `{dtx}`（没有 .cls）：需要先跑模板自带的 docstrip 安装脚本生成 `{missing_file}`（通常是 `xelatex {stem}.ins`，但项目里没找到 .ins）——请按模板 README 的安装说明操作"
        ));
    }
    None
}

fn d_no_hint(kind: DiagnosisKind, cause: impl Into<String>) -> Diagnosis {
    Diagnosis {
        kind,
        cause: cause.into(),
        hint: None,
        suggested_timeout_secs: None,
        missing_file: None,
    }
}

/// 超时诊断的证据（全部来自**可观察事实**，不做猜测）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeoutEvidence {
    /// 本次生效的超时上限（秒）。
    pub timeout_secs: u32,
    /// 本次是否为**首次编译**（开始时无 `tmp/<stem>.aux`）——首编最慢（要建 aux/toc 并多趟收敛）。
    pub first_build: bool,
    /// 项目内 `.tex` 源文件数（扫描失败 = None）。
    pub tex_files: Option<usize>,
}

/// 建议的下一个超时值：阶梯 `300 → 900 → 1800`（与设置上限一致），首编直接跳到 900。
///
/// 为什么首编跳档：首编要建 `.aux`/`.toc` 并跑收敛趟，③⑦ 的实测都显示它显著慢于增量；
/// 让用户在"注定不够"的 300s 上再失败一次（真实论文档是分钟级等待）代价太高。
/// 这只是**起点不是预测**——不够时可以继续点（上限 1800s）。
pub fn suggest_timeout_secs(current: u32, first_build: bool) -> u32 {
    const LADDER: [u32; 3] = [300, 900, 1800];
    let next = LADDER
        .iter()
        .copied()
        .find(|&v| v > current)
        .unwrap_or(1800);
    if first_build {
        next.max(900)
    } else {
        next
    }
}

/// 超时诊断（roadmap ㉕）：把"编译超时了"翻译成"为什么 + 怎么办"。
///
/// **判据优先级**（从强到弱）：
/// 1. **日志里已经有致命错误** → 超时只是症状：进程报错后没退出（实测 `latexmk` 会挂在
///    文件错误上零 CPU 干等），真正要修的是那条错误，**提高超时救不了它**——此时不给
///    "一键提高超时"（`suggested_timeout_secs = None`），直接把那条错误的诊断摆出来；
/// 2. 日志里有页输出 → 在推进，只是比上限慢（[`DiagnosisKind::CompileTimeout`]）；
/// 3. 两者都无 → **不武断说"卡住"**（大文档首编可能整段时间都在加载字体/展开宏包），
///    给出两种可能并让用户先排除死循环（[`DiagnosisKind::CompileTimeoutStalled`]）。
pub fn diagnose_timeout(log: &str, ev: &TimeoutEvidence) -> Diagnosis {
    // 1) 日志自有致命错误优先（实测触发者：hithesis 的 `I can't write on file ...aux`）
    if let Some(fatal) = super::parse_log(log)
        .iter()
        .find(|m| m.kind == MessageKind::Error)
        .and_then(diagnose)
    {
        return Diagnosis {
            kind: fatal.kind,
            cause: format!(
                "本次编译在 {}s 内没跑完、已被终止；但日志显示它其实**早已失败**——真正要修的是这条：{}",
                ev.timeout_secs, fatal.cause
            ),
            hint: fatal.hint.map(|h| format!("{h}（注意：这种情况提高编译超时救不了）")),
            // 提高超时救不了已有致命错误的运行 → 不给一键按钮
            suggested_timeout_secs: None,
            // 保留原诊断的"缺失文件名"：调用方还要用它找源码版模板线索（㉗）
            missing_file: fatal.missing_file,
        };
    }

    // 2) / 3) 按推进证据判断"慢"还是"疑似卡住"
    let suggested = suggest_timeout_secs(ev.timeout_secs, ev.first_build);
    let scale = match ev.tex_files {
        Some(n) => format!("共 {n} 个 .tex 源文件"),
        None => "源文件规模未知".to_string(),
    };
    let first = if ev.first_build { "首次编译" } else { "增量编译" };

    match super::pages_typeset(log) {
        Some(pages) => Diagnosis {
            kind: DiagnosisKind::CompileTimeout,
            cause: format!(
                "编译在 {}s 内没有跑完，但日志显示已经排版到第 {pages} 页——是在推进，只是比上限慢（{first}，{scale}）",
                ev.timeout_secs
            ),
            hint: Some(format!(
                "把编译超时提高到 {suggested}s 后重试（下方按钮一键完成）；大文档首编常需数分钟，仍不够可继续调大（上限 1800s）"
            )),
            suggested_timeout_secs: Some(suggested),
            missing_file: None,
        },
        None => Diagnosis {
            kind: DiagnosisKind::CompileTimeoutStalled,
            cause: format!(
                "编译在 {}s 内没有跑完，且日志里还没出现任何页面输出（{first}，{scale}）——可能仍在做前置处理（加载字体、展开宏包），也可能真的卡住了",
                ev.timeout_secs
            ),
            hint: Some(format!(
                "先排除死循环（\\loop / \\foreach / \\whileloop）与等待终端输入（如 \\read）；若确认只是慢，把超时提高到 {suggested}s 后重试（下方按钮一键完成）"
            )),
            suggested_timeout_secs: Some(suggested),
            missing_file: None,
        },
    }
}

/// 从 `` `name' `` / `` `name `` 形态里取名字（TeX 报错的引号形式不统一）。
fn capture_quoted(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"`([^'`\s]+)'?").expect("quoted regex");
    re.captures(text).map(|c| c[1].to_string())
}

/// 从 `"name"` 形态里取名字（fontspec 的字体名用双引号，不是反引号）。
fn capture_double_quoted(text: &str) -> Option<String> {
    let re = regex::Regex::new(r#""([^"]+)""#).expect("double quoted regex");
    re.captures(text).map(|c| c[1].to_string())
}

/// `Option clash for package X` 里的 X（该句式**不带引号**，用 `capture_quoted` 取不到）。
fn capture_option_clash_package(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"Option clash for package\s+([A-Za-z0-9@_-]+)").expect("clash regex");
    re.captures(text).map(|c| c[1].to_string())
}

/// 取文件名（去掉 `../foo/bar.sty` 这类路径前缀，用于拼 `tlmgr install` 建议）。
fn base_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// 取 `l.<n> \cmd` 里的命令名（去掉可能的参数残留）。
fn capture_command_after_line_marker(message: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?m)^l\.\d+\s+\\([A-Za-z@]+)").expect("cmd regex");
    re.captures(message).map(|c| format!("\\{}", &c[1]))
}

/// 取 `Package X Error` / `Class X Error` 里的名字。
fn capture_package_name(message: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?:Package|Class)\s+([A-Za-z0-9@_-]+)\s+Error").expect("pkg regex");
    re.captures(message).map(|c| c[1].to_string())
}

/// 折叠空白用于短语匹配：**先剥掉续行的 `(包名)` 前缀**，再合并空白。
///
/// 为什么必须先剥前缀：TeX 的宏包报错续行形如
/// ```text
/// ! Package fontspec Error:
/// (fontspec)                The font "X" cannot be
/// (fontspec)                found; ...
/// ```
/// 前缀 `(fontspec)` 正好卡在 `cannot be` 与 `found;` 之间——不剥就永远匹配不到
/// "cannot be found"（这是 ⑲ 实测样本 `missing-font-source-han` 暴露的问题）。
fn normalize_for_match(text: &str) -> String {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^\s*\([A-Za-z0-9@_.-]+\)\s*").expect("prefix regex")
    });
    let stripped: Vec<String> = text.lines().map(|l| RE.replace(l, "").into_owned()).collect();
    stripped
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 诊断一条日志消息；无法判断时返回 `None`（前端降级为原文 + 行号）。
pub fn diagnose(m: &LogMessage) -> Option<Diagnosis> {
    // TeX 的报错常**跨行折断**且带续行前缀，逐字匹配会漏——所有"短语匹配"跑在规范化副本上；
    // 而"取 `l.N \cmd`"必须用原文（依赖行首锚点）。
    let normalized = normalize_for_match(&m.message);
    let text = normalized.as_str();

    // 1) 源文件编码（先判：它的报错文本里也含 "Unicode"/"byte"，避免被后面的规则抢走）
    if text.contains("Invalid UTF-8 byte") {
        return Some(d(
            DiagnosisKind::NonUtf8Source,
            "源文件含非法 UTF-8 字节——常见于 GBK 编码的旧文件",
            "把该 .tex 另存为 UTF-8；或改用 XeLaTeX 引擎（它对多字节输入更宽容）",
        ));
    }

    // 2) 引擎不匹配：⑲ 实测的 4 个模板（shtthesis/bjfuthesis/cquthesis/fduthesis）都落在这一类
    if text.contains("requires either XeTeX or")
        || text.contains("Cannot be run with pdftex")
        || text.contains("The fontspec package requires")
        || text.contains("only works with LuaLaTeX or XeLaTeX")
        || text.contains("only works with XeLaTeX")
        || text.contains("XeLaTeX is required")
    {
        return Some(d(
            DiagnosisKind::EngineMismatch,
            "该文档需要 XeLaTeX 或 LuaLaTeX，当前用的是 pdfLaTeX",
            "到「设置 → 引擎」把引擎切成 XeLaTeX 后重新编译",
        ));
    }

    // 3) 需要本产品不支持的引擎（日文 pLaTeX/upLaTeX）
    if text.contains("needs format `pLaTeX2e'") || text.contains("pLaTeX2e") {
        return Some(d(
            DiagnosisKind::UnsupportedEngine,
            "该文档需要 pLaTeX / upLaTeX（日文排版引擎）",
            "本产品当前只支持 XeLaTeX / LuaLaTeX / pdfLaTeX；日文文档请改用 upLaTeX 工具链编译",
        ));
    }

    // 4) ctex 字体集取值错（学位论文模板常见：fontset=windowsnew 之类）
    if text.contains("fontset") && text.contains("could not be found") {
        let name = capture_quoted(text).unwrap_or_else(|| "（未知）".into());
        return Some(d(
            DiagnosisKind::MissingFontset,
            format!("ctex 字体集 `{name}` 不存在"),
            "改用可用取值：fontset=windows（Windows 自带中易字体）或 fontset=fandol（TeX Live 自带，无需系统字体）",
        ));
    }

    // 5) 系统缺字体（fontspec）——字体名是**双引号**，不是反引号
    if text.contains("cannot be found") || text.contains("cannot be loaded") {
        let name = capture_double_quoted(text).unwrap_or_else(|| "（未知）".into());
        return Some(d(
            DiagnosisKind::MissingFont,
            format!("系统里找不到字体 \"{name}\""),
            "安装该字体，或改用 TeX Live 自带字体（ctex 的 fontset=fandol / 换成 SimSun 等已装字体）",
        ));
    }

    // 6) 缺文件：`.sty` / `.cls` / 其他
    if text.contains("not found") {
        let name = capture_quoted(text);
        if let Some(n) = name {
            let lower = n.to_ascii_lowercase();
            if lower.ends_with(".sty") {
                let pkg = base_name(&n).trim_end_matches(".sty").to_string();
                return Some(d_missing(
                    DiagnosisKind::MissingPackage,
                    format!("缺少宏包文件 {n}"),
                    format!(
                        "若模板自带该文件，确认「打开项目」选的是模板根目录；否则执行 `tlmgr install {pkg}` 安装"
                    ),
                    n,
                ));
            }
            if lower.ends_with(".cls") {
                let cls = base_name(&n).trim_end_matches(".cls").to_string();
                return Some(d_missing(
                    DiagnosisKind::MissingClass,
                    format!("缺少文档类文件 {n}"),
                    format!(
                        "学位论文模板常自带 .cls：确认项目根目录是否正确（或执行 `tlmgr install {cls}`）；若是 GitHub 源码版，可能需先编译 .ins 生成 .cls"
                    ),
                    n,
                ));
            }
            return Some(d_missing(
                DiagnosisKind::MissingFile,
                format!("找不到文件 {n}"),
                "确认该文件确实在项目内，且路径/大小写与 \\input、\\includegraphics 里写的一致",
                n,
            ));
        }
    }

    // 7) 宏包选项冲突（句式不带引号）
    if text.contains("Option clash for package") {
        let name = capture_option_clash_package(text).unwrap_or_else(|| "（未知）".into());
        return Some(d(
            DiagnosisKind::OptionClash,
            format!("宏包 {name} 被以不同选项加载了两次"),
            "统一两处的选项，或在文档最前面用 \\PassOptionsToPackage{选项}{宏包名} 提前声明",
        ));
    }

    // 8) 未定义控制序列
    if text.contains("Undefined control sequence") {
        let cmd = capture_command_after_line_marker(m.message.as_str());
        return Some(match cmd {
            Some(c) => d(
                DiagnosisKind::UndefinedControlSequence,
                format!("命令 {c} 未定义"),
                "多为拼写错误；若拼写无误，通常是缺少提供该命令的宏包（\\usepackage{...}）",
            ),
            None => d(
                DiagnosisKind::UndefinedControlSequence,
                "存在未定义的控制序列",
                "多为拼写错误；若拼写无误，通常是缺少提供该命令的宏包",
            ),
        });
    }

    // 9) 组未闭合
    if text.contains("File ended while scanning use of") {
        let cmd = capture_command_after_line_marker(m.message.as_str())
            .or_else(|| {
                let re = regex::Regex::new(r"scanning use of \\?([A-Za-z@]+)").ok()?;
                re.captures(text).map(|c| format!("\\{}", &c[1]))
            })
            .unwrap_or_else(|| "（某命令）".into());
        return Some(d(
            DiagnosisKind::UnclosedGroup,
            format!("{cmd} 的参数没有闭合（缺少右花括号 }}）"),
            "检查该命令的 { } 是否配对；宏定义或环境体里漏掉 } 也会报这条",
        ));
    }

    // 10) 数学模式
    if text.contains("Missing $ inserted") {
        return Some(d(
            DiagnosisKind::MissingMathMode,
            "在数学模式之外用了上下标（_ 或 ^）等数学符号",
            "把该片段放进 $...$ 或 \\(...\\)；表格/正文里的下标尤其容易漏",
        ));
    }

    // 11) 多余右花括号
    if text.contains("Too many }'s") {
        return Some(d(
            DiagnosisKind::ExtraBrace,
            "多了一个右花括号 }",
            "检查该行附近的 { } 配对（常见于宏定义结尾多打一个 }）",
        ));
    }

    // 12) 缺 \begin{document}
    if text.contains("Missing \\begin{document}") {
        return Some(d(
            DiagnosisKind::MissingBeginDocument,
            "缺少 \\begin{document}",
            "正文内容必须写在 \\begin{document} 与 \\end{document} 之间；也可能是把 \\documentclass 写漏了",
        ));
    }

    // 13) 重复上下标
    if text.contains("Double subscript") || text.contains("Double superscript") {
        return Some(d(
            DiagnosisKind::DoubleScript,
            "同一个位置重复使用了上下标",
            "用花括号分组，例如 x_{a_b} 或 x^{2}_{i}",
        ));
    }

    // 14) 表格外的 &
    if text.contains("Misplaced alignment tab character") {
        return Some(d(
            DiagnosisKind::MisplacedAlignment,
            "在表格/对齐环境之外出现了 &",
            "& 只能用在 tabular、array、align 等环境里；正文里的 & 要写成 \\&",
        ));
    }

    // 15) 写不出中间文件（roadmap ㉖ 的首个实测证据：2026-09 冷跑 hithesis 复现）
    // **必须排在 Emergency stop 之前**：这段日志里两条常常落在同一条聚合消息里，而
    // "写不出文件"才是根因、也才有可操作修法；Emergency stop 只是它的收尾。
    if text.contains("I can't write on file") {
        let file = capture_quoted(text).unwrap_or_else(|| "（未知文件）".into());
        return Some(d(
            DiagnosisKind::AuxWriteFailed,
            format!("写不出中间文件 `{file}`——`\\include{{子目录/文件}}` 要在中间文件目录里写同名 `.aux`，而那个子目录不存在"),
            "把主文件里的 `\\include{子目录/文件}` 改成 `\\input{子目录/文件}`（\\input 不写子文件 .aux，实测可绕开）；根因是模板自带的构建约定与本产品「中间文件统一收纳 tmp/」的冲突（roadmap ㉖）",
        ));
    }

    // 16) Emergency stop（连锁反应的收尾，通常不是根因）
    if text.contains("Emergency stop") {
        return Some(d(
            DiagnosisKind::EmergencyStop,
            "编译被紧急中止（Emergency stop）——这通常是「前面某条错误」的连锁反应",
            "先修列表里第一条错误再重新编译；只看这一条会找不到根因",
        ));
    }
    // 16) 兜底：至少点出是哪个宏包/文档类报的错
    if let Some(pkg) = capture_package_name(text) {
        let first = m.message.lines().next().unwrap_or("").trim();
        return Some(d_no_hint(
            DiagnosisKind::PackageError,
            format!("宏包 {pkg} 报错：{first}"),
        ));
    }
    if text.contains("LaTeX Error") || (m.kind == MessageKind::Error && text.starts_with('!')) {
        let first = m.message.lines().next().unwrap_or("").trim();
        return Some(d_no_hint(
            DiagnosisKind::LatexError,
            format!("LaTeX 报错：{first}"),
        ));
    }

    None
}
