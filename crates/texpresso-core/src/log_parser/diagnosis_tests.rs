//! 错误诊断的**真实语料**回归（roadmap ④）。
//!
//! 语料在 [`super::real_error_corpus`]（脚本 `scripts/gen-log-error-corpus.ps1` 自动生成，
//! 23 例全部来自真实编译）；**期望值在这里手写**——若由实现反推期望值，测试就成了自证循环。
//!
//! 断言三件事：
//! 1. 语料与期望表**一一对应**（防止语料重生成后悄悄漂移）；
//! 2. 每例都能被 `parse_log` 解析出错误、并给出**预期类别 + 原因关键词 + 行号**；
//! 3. DoD 覆盖率：能给出「原因 + 建议」的比例 ≥ 80%。

use super::diagnosis::{diagnose, DiagnosisKind};
use super::parse_log;
use super::real_error_corpus::REAL_LOGS;

struct Expect {
    name: &'static str,
    kind: DiagnosisKind,
    /// 原因里必须出现的关键词（人话检查，避免只对枚举不对内容）。
    cause_contains: &'static str,
    /// 期望行号（`None` = 该错误形态本身不带 `l.N`）。
    line: Option<u32>,
    /// 是否必须给出可操作建议。
    want_hint: bool,
}

/// 期望表：与语料按 `name` 一一对应。
static EXPECT: &[Expect] = &[
    // ---- 缺文件 / 缺包 / 缺类（⑲ 实测：模板兼容的头号障碍）----
    // 注：LaTeX 的「文件找不到」会**先交互式要文件名**，在 nonstopmode 下紧接着 `! Emergency stop.`，
    // 于是 `l.<n>` 落在 Emergency stop 那条消息上、而非本条。这是引擎行为，不是解析缺陷——
    // 所以这四条期望行号为 None（诊断本身已点出缺少哪个文件，定位靠并排的 Emergency stop 条目）。
    Expect { name: "missing-package-slashbox", kind: DiagnosisKind::MissingPackage, cause_contains: "slashbox.sty", line: None, want_hint: true },
    Expect { name: "missing-package-relative", kind: DiagnosisKind::MissingPackage, cause_contains: "authorarchive.sty", line: None, want_hint: true },
    Expect { name: "missing-class-seuthesis", kind: DiagnosisKind::MissingClass, cause_contains: "seuthesis.cls", line: None, want_hint: true },
    Expect { name: "aastex-missing-logo", kind: DiagnosisKind::MissingFile, cause_contains: "acm-jdslogo", line: Some(194), want_hint: true },
    // ---- 字体（⑲ 实测：fduthesis 缺思源宋体 / hithesis 字体集取值错）----
    Expect { name: "missing-font-source-han", kind: DiagnosisKind::MissingFont, cause_contains: "SourceHanSerifSC-Regular", line: Some(857), want_hint: true },
    Expect { name: "missing-ctex-fontset", kind: DiagnosisKind::MissingFontset, cause_contains: "windowsnew", line: Some(1678), want_hint: true },
    // ---- 引擎（⑲ 核心结论：不自动推断，但选错时必须明确告诉用户怎么改）----
    Expect { name: "engine-fontspec-under-pdftex", kind: DiagnosisKind::EngineMismatch, cause_contains: "XeLaTeX", line: Some(101), want_hint: true },
    Expect { name: "engine-unicode-math-under-pdftex", kind: DiagnosisKind::EngineMismatch, cause_contains: "XeLaTeX", line: Some(40), want_hint: true },
    Expect { name: "engine-shtthesis-only", kind: DiagnosisKind::EngineMismatch, cause_contains: "XeLaTeX", line: Some(38), want_hint: true },
    Expect { name: "needs-platex-format", kind: DiagnosisKind::UnsupportedEngine, cause_contains: "pLaTeX", line: Some(14), want_hint: true },
    // ---- 编码（P0-① 实测：GBK 源 + pdflatex）----
    Expect { name: "non-utf8-source", kind: DiagnosisKind::NonUtf8Source, cause_contains: "UTF-8", line: Some(3), want_hint: true },
    // ---- 语法类（探针编译）----
    Expect { name: "undefined-cs-dtx", kind: DiagnosisKind::UndefinedControlSequence, cause_contains: "\\DoNotIndex", line: Some(1), want_hint: true },
    Expect { name: "unclosed-brace", kind: DiagnosisKind::UnclosedGroup, cause_contains: "\\textbf", line: None, want_hint: true },
    Expect { name: "runaway-argument", kind: DiagnosisKind::UnclosedGroup, cause_contains: "\\foo", line: None, want_hint: true },
    Expect { name: "missing-math-mode", kind: DiagnosisKind::MissingMathMode, cause_contains: "数学模式", line: Some(3), want_hint: true },
    Expect { name: "too-many-braces", kind: DiagnosisKind::ExtraBrace, cause_contains: "右花括号", line: Some(3), want_hint: true },
    Expect { name: "end-without-begin", kind: DiagnosisKind::MissingBeginDocument, cause_contains: "\\begin{document}", line: Some(2), want_hint: true },
    Expect { name: "double-subscript", kind: DiagnosisKind::DoubleScript, cause_contains: "上下标", line: Some(3), want_hint: true },
    Expect { name: "misplaced-alignment", kind: DiagnosisKind::MisplacedAlignment, cause_contains: "&", line: Some(5), want_hint: true },
    // ---- 连锁反应 / 宏包与选项 ----
    Expect { name: "emergency-stop", kind: DiagnosisKind::EmergencyStop, cause_contains: "连锁", line: None, want_hint: true },
    Expect { name: "file-ended-scanning", kind: DiagnosisKind::EmergencyStop, cause_contains: "连锁", line: None, want_hint: true },
    Expect { name: "option-clash-natbib", kind: DiagnosisKind::OptionClash, cause_contains: "natbib", line: Some(184), want_hint: true },
    Expect { name: "siunitx-invalid-number", kind: DiagnosisKind::PackageError, cause_contains: "siunitx", line: Some(430), want_hint: false },
];

#[test]
fn corpus_and_expectations_match_one_to_one() {
    assert!(
        REAL_LOGS.len() >= 20,
        "DoD 要求 ≥20 例真实语料，当前 {}",
        REAL_LOGS.len()
    );
    for c in REAL_LOGS {
        assert!(
            EXPECT.iter().any(|e| e.name == c.name),
            "语料 `{}` 没有对应期望（语料与期望表漂移了？）",
            c.name
        );
    }
    for e in EXPECT {
        assert!(
            REAL_LOGS.iter().any(|c| c.name == e.name),
            "期望 `{}` 没有对应语料",
            e.name
        );
    }
}

#[test]
fn every_real_error_is_diagnosed_as_expected() {
    let mut failures: Vec<String> = Vec::new();
    for e in EXPECT {
        let case = REAL_LOGS
            .iter()
            .find(|c| c.name == e.name)
            .unwrap_or_else(|| panic!("语料缺失：{}", e.name));

        // 端到端：真实日志文本 → parse_log → diagnose（不绕过解析器）
        let messages = parse_log(case.log);
        let err = messages
            .iter()
            .find(|m| matches!(m.kind, super::MessageKind::Error))
            .unwrap_or_else(|| panic!("`{}` 没解析出错误条目：{:?}", e.name, messages));

        let Some(diag) = diagnose(err) else {
            failures.push(format!("{}: 未给出诊断", e.name));
            continue;
        };
        if diag.kind != e.kind {
            failures.push(format!("{}: 类别 {:?} ≠ {:?}", e.name, diag.kind, e.kind));
        }
        if !diag.cause.contains(e.cause_contains) {
            failures.push(format!(
                "{}: 原因 `{}` 不含 `{}`",
                e.name, diag.cause, e.cause_contains
            ));
        }
        if e.want_hint && diag.hint.is_none() {
            failures.push(format!("{}: 缺可操作建议", e.name));
        }
        if err.line != e.line {
            failures.push(format!("{}: 行号 {:?} ≠ {:?}", e.name, err.line, e.line));
        }
    }
    assert!(failures.is_empty(), "诊断不符合预期：\n  {}", failures.join("\n  "));
}

#[test]
fn dod_coverage_is_at_least_80_percent() {
    // DoD（roadmap ④）：真实错误样本集里 ≥80% 能给出「原因 + 怎么改」
    let total = REAL_LOGS.len();
    let mut with_cause = 0;
    let mut with_hint = 0;
    for c in REAL_LOGS {
        let err = parse_log(c.log)
            .into_iter()
            .find(|m| matches!(m.kind, super::MessageKind::Error));
        if let Some(m) = err {
            if let Some(diag) = diagnose(&m) {
                with_cause += 1;
                if diag.hint.is_some() {
                    with_hint += 1;
                }
            }
        }
    }
    assert!(
        with_cause * 100 / total >= 80,
        "给出原因的覆盖率 {with_cause}/{total} 低于 80%"
    );
    // 建议允许有例外（如 PackageError 只点出宏包名），但不应过半缺失
    assert!(
        with_hint * 100 / total >= 80,
        "给出建议的覆盖率 {with_hint}/{total} 低于 80%"
    );
}

#[test]
fn clean_log_yields_no_errors_and_no_diagnosis() {
    // 反例守卫：正常成功的日志不得被诊断成"有问题"
    let clean = "This is XeTeX, Version 3.141592653-2.6-0.999996\n\
                 entering extended mode\n\
                 (./main.tex\n\
                 (./article.cls\n\
                 Document Class: article 2023/05/17 v1.4n Standard LaTeX document class\n\
                 ))\n\
                 [1] (./main.aux) )\n\
                 Output written on main.pdf (1 page).\n";
    let messages = parse_log(clean);
    let errors: Vec<_> = messages
        .iter()
        .filter(|m| matches!(m.kind, super::MessageKind::Error))
        .collect();
    assert!(errors.is_empty(), "干净日志不应解析出错误：{errors:?}");
}

// ---- 超时诊断（roadmap ㉕）：证据在手才开口 ----

use super::diagnosis::{diagnose_timeout, suggest_timeout_secs, TimeoutEvidence};

fn ev(first_build: bool) -> TimeoutEvidence {
    TimeoutEvidence {
        timeout_secs: 120,
        first_build,
        tex_files: Some(11),
    }
}

#[test]
fn timeout_with_progress_says_slow_not_stuck() {
    // 日志已排版到第 37 页 = 在推进 → 只能下"慢"的结论，绝不能暗示卡住
    let log = "[35] [36] [37]\n";
    let d = diagnose_timeout(log, &ev(true));
    assert_eq!(d.kind, DiagnosisKind::CompileTimeout);
    assert!(d.cause.contains("第 37 页"), "原因须含推进证据：{}", d.cause);
    assert!(d.cause.contains("在推进"), "原因须点明不是卡住：{}", d.cause);
    assert!(d.cause.contains("首次编译"), "原因须点明首编：{}", d.cause);
    assert!(d.cause.contains("11 个 .tex"), "原因须含规模证据：{}", d.cause);
    assert!(!d.cause.contains("卡住"), "有推进证据时不得说卡住：{}", d.cause);
    assert_eq!(d.suggested_timeout_secs, Some(900), "首编应跳档到 900s");
}

#[test]
fn timeout_without_progress_offers_both_interpretations() {
    // 无页输出：大文档首编可能整段时间都在加载字体 → 不能说"卡住"，要给两种可能
    let d = diagnose_timeout("", &ev(true));
    assert_eq!(d.kind, DiagnosisKind::CompileTimeoutStalled);
    assert!(d.cause.contains("可能"), "须保留两种可能：{}", d.cause);
    assert!(d.cause.contains("加载字体") || d.cause.contains("前置处理"), "{}", d.cause);
    let hint = d.hint.clone().unwrap_or_default();
    assert!(hint.contains("死循环"), "无推进证据时先让用户排除死循环：{hint}");
    assert!(hint.contains("900"), "建议值须出现在建议文案里：{hint}");
    assert_eq!(d.suggested_timeout_secs, Some(900));
}

#[test]
fn timeout_suggestion_ladder_avoids_repeating_the_same_wall() {
    // 非首编走阶梯：120→300→900→1800（不重复同一个"注定不够"的值）
    assert_eq!(suggest_timeout_secs(120, false), 300);
    assert_eq!(suggest_timeout_secs(300, false), 900);
    assert_eq!(suggest_timeout_secs(900, false), 1800);
    assert_eq!(suggest_timeout_secs(1800, false), 1800, "已是上限则不再提高");
    // 首编直接跳到 900：首编最慢，让用户在注定不够的 300s 上再等一次代价太高
    assert_eq!(suggest_timeout_secs(120, true), 900);
    assert_eq!(suggest_timeout_secs(900, true), 1800);
}

#[test]
fn timeout_diagnosis_never_exceeds_settings_ceiling() {
    // 建议值必须落在设置允许的范围内，否则前端一键操作会被后端校验拒绝
    for cur in [5, 120, 300, 600, 900, 1800] {
        for first in [true, false] {
            let v = suggest_timeout_secs(cur, first);
            assert!((5..=1800).contains(&v), "建议值越界：{v}（cur={cur} first={first}）");
            assert!(v >= cur, "建议值不应低于当前值：{v} < {cur}");
        }
    }
}

#[test]
fn pages_typeset_feeds_timeout_diagnosis() {
    // 端到端：真实日志片段 → 页码 → 诊断（折行标记必须被认出来）
    let log = "\\openout2 = `chapters/math.aux'.\n[3] [4] [5] [6\n\n\n\n\n] 第二章\n";
    assert_eq!(super::pages_typeset(log), Some(6));
    let mut e = ev(false);
    e.timeout_secs = 60;
    let d = diagnose_timeout(log, &e);
    assert_eq!(d.kind, DiagnosisKind::CompileTimeout);
    assert!(d.cause.contains("第 6 页"), "{}", d.cause);
    assert_eq!(d.suggested_timeout_secs, Some(300), "非首编走阶梯");
}

/// 真实日志（2026-09-12 20:15 冷跑 `thesis-real-hithesis` 原样摘录）：
/// 模板 `\include{body/introduction}` + 我们的 `-outdir=tmp` → `tmp/body/` 不存在 →
/// 写不出 `body/introduction.aux` → Emergency stop。**进程随后没退出**（latexmk 挂在文件错误上
/// 零 CPU 干等），所以产品看到的是"超时"而不是"内容错误"——这正是超时诊断必须先认日志错误的原因。
const REAL_HITHESIS_TIMEOUT_LOG: &str = "\
! I can't write on file `body/introduction.aux'.
\\@include ...mmediate \\openout \\@partaux \"#1.aux\" 
                                                  \\immediate \\write \\@partau...
l.106 \\include{body/introduction}
                                 
(Press Enter to retry, or Control-Z to exit; default file extension is `.tex')
Please type another output file name
! Emergency stop.
";

#[test]
fn timeout_prefers_fatal_error_found_in_log() {
    let d = diagnose_timeout(REAL_HITHESIS_TIMEOUT_LOG, &ev(true));
    // 报出真正要修的错误（写不出中间文件），而不是含糊的"慢/卡住"
    assert_eq!(d.kind, DiagnosisKind::AuxWriteFailed);
    assert!(d.cause.contains("body/introduction.aux"), "{}", d.cause);
    assert!(d.cause.contains("早已失败"), "须说明超时只是症状：{}", d.cause);
    let hint = d.hint.clone().unwrap_or_default();
    assert!(hint.contains("\\input"), "须给出可操作替代：{hint}");
    assert!(hint.contains("救不了"), "须说明提高超时无效：{hint}");
    // 关键：不给"一键提高超时"——那救不了这个错误
    assert_eq!(
        d.suggested_timeout_secs, None,
        "日志里已有致命错误时不得建议提高超时"
    );
}

#[test]
fn aux_write_failure_is_diagnosed_directly() {
    // 同一段日志走普通（非超时）路径也要能诊断——多数情况下 TeX 会正常退出并报内容错误
    let msgs = parse_log(REAL_HITHESIS_TIMEOUT_LOG);
    let fatal = msgs
        .iter()
        .find(|m| matches!(m.kind, super::MessageKind::Error))
        .expect("应解析出错误消息");
    let d = diagnose(fatal).expect("应能诊断");
    assert_eq!(d.kind, DiagnosisKind::AuxWriteFailed);
    assert!(d.cause.contains("body/introduction.aux"));
}
