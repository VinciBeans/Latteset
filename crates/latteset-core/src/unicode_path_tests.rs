//! 中文 / 非 ASCII 路径兼容性回归（roadmap P0-①，2026-09）。
//!
//! 背景：Windows 中文用户的常见形态是「中文目录 + 中文文件名 + 中文子目录」，而 TeX 工具链
//! 的日志与编码约定与系统代码页并不同源（见 docs/research/tex-ide-pain-points.md P7）。
//! 本模块把**实测结论**锁成回归：路径归一化、根文件探测、大纲遍历、读集安全（D8 词法前缀）、
//! 以及 .log 里的中文文件名能否带出行号。
//!
//! 实测证据（2026-09，TeX Live 2026 / Windows）：
//! - `latexmk -xelatex -outdir=tmp -synctex=1 -interaction=nonstopmode 中文主文件.tex` 在
//!   CP65001 与 CP936 下均 exit 0，`tmp/` 产出 `.log/.synctex.gz`，PDF 正常；
//! - `synctex view/edit` 对含中文的**绝对路径**返回正确结果，且 stdout 恒为 **UTF-8**
//!   （字节实测 `E4 B8 AD` = 「中」），与控制台代码页无关 —— 故 `from_utf8_lossy` 用法正确；
//! - `.log` 恒为合法 UTF-8：源文件是 GBK 时，xelatex 自己把非法字节替换为 U+FFFD 并写入日志，
//!   日志中的中文文件名保持 UTF-8，故严格 `read_to_string` 不会失败。

use crate::log_parser::{parse_log, MessageKind};
use crate::outline::{join_path, load as outline_load, normalize_path, resolve_include, OutlineContext};
use crate::project::{collect_tex_files, find_candidates, resolve, RootResolution};
use crate::testutil::{block_on, FakeFS};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 中文项目根（正斜杠形式，与前端 `normalizePath` 的输出一致）。
const ROOT: &str = "E:/项目/中文测试工程";

#[test]
fn normalize_path_handles_chinese_segments() {
    // `.`/`..` 折叠不得因多字节字符而错位或 panic
    assert_eq!(
        normalize_path("E:/项目/中文测试工程/./章节/第一章.tex"),
        "E:/项目/中文测试工程/章节/第一章.tex"
    );
    assert_eq!(
        normalize_path("E:/项目/中文测试工程/章节/../章节/./第一章.tex"),
        "E:/项目/中文测试工程/章节/第一章.tex"
    );
    // 越界 `..` 在绝对路径下丢弃（与前端一致）
    assert_eq!(normalize_path("E:/项目/../../秘密.tex"), "E:/秘密.tex");
    // 盘符段必须保留，不能被当普通段吃掉
    assert_eq!(
        normalize_path("E:/项目/中文测试工程/中文主文件.tex"),
        "E:/项目/中文测试工程/中文主文件.tex"
    );
}

#[test]
fn join_path_and_resolve_include_with_chinese() {
    assert_eq!(
        join_path("E:/项目/中文测试工程", "./章节/第一章"),
        "E:/项目/中文测试工程/章节/第一章"
    );

    // \include{章节/第一章} → 根相对优先，无扩展名补 .tex
    let cands = resolve_include(
        "章节/第一章",
        "E:/项目/中文测试工程/中文主文件.tex",
        ROOT,
    );
    assert!(
        cands.iter().any(|c| c == "E:/项目/中文测试工程/章节/第一章.tex"),
        "中文 include 应解析出根相对候选，实得 {cands:?}"
    );
}

#[test]
fn root_detect_picks_chinese_root_file() {
    // 中文工程：唯一含 \documentclass 的顶层 .tex 应被识别为根文件，
    // 被 include 的中文子文件不得被误判为根。
    let mut fs = FakeFS::new();
    fs.put_file(
        format!("{ROOT}/中文主文件.tex"),
        "\\documentclass[UTF8]{ctexart}\n\\include{章节/第一章}\n",
    );
    fs.put_file(format!("{ROOT}/章节/第一章.tex"), "\\section{子文件章节}\n");
    fs.put_file(format!("{ROOT}/参考文献.bib"), "@article{a, title={中文}}\n");

    let root = Path::new(ROOT);
    let files = block_on(collect_tex_files(&fs, root)).expect("扫描中文工程");
    let names: Vec<String> = files
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec![
            format!("{ROOT}/中文主文件.tex"),
            format!("{ROOT}/章节/第一章.tex"),
        ],
        "中文工程应收集到两个 .tex（排序稳定）"
    );

    let candidates = find_candidates(&files, root, |p| fs.file(p).map(str::to_owned));
    match resolve(candidates) {
        RootResolution::Unique(p) => assert_eq!(p, PathBuf::from(format!("{ROOT}/中文主文件.tex"))),
        other => panic!("应探测出唯一中文根文件，实得 {other:?}"),
    }
}

#[test]
fn outline_walks_chinese_include_graph() {
    // 覆盖 D8 词法前缀守卫：中文路径不得被误判为「项目外」而跳过读盘。
    let mut fs = FakeFS::new();
    let main = format!("{ROOT}/中文主文件.tex");
    let chapter = format!("{ROOT}/章节/第一章.tex");
    fs.put_file(
        &main,
        "\\documentclass{ctexart}\n\\begin{document}\n\\include{章节/第一章}\n\\end{document}\n",
    );
    fs.put_file(&chapter, "\\section{子文件章节}\n\\subsection{小节}\n");

    let root = Path::new(ROOT);
    let buffers: HashMap<String, String> = HashMap::new();
    let ctx = OutlineContext {
        root,
        root_file: Some(Path::new(&main)),
        buffers: &buffers,
        fallback_files: None,
    };
    let nodes = block_on(outline_load(&ctx, &fs));

    assert_eq!(nodes.len(), 1, "应只有一个顶层节点，实得 {nodes:?}");
    let top = &nodes[0];
    assert_eq!(top.title, "子文件章节");
    assert_eq!(
        top.file, chapter,
        "大纲节点 file 应为中文绝对路径（归一化后）"
    );
    assert_eq!(top.file_base, "第一章.tex");
    assert_eq!(top.children.len(), 1, "子节应挂到章节下：{top:?}");
    assert_eq!(top.children[0].title, "小节");
}

#[test]
fn outline_guard_still_rejects_paths_outside_chinese_root() {
    // 前缀守卫在中文路径下仍必须拦得住「项目外」——不能因为编码处理而放宽。
    let mut fs = FakeFS::new();
    let main = format!("{ROOT}/中文主文件.tex");
    fs.put_file(
        &main,
        "\\documentclass{ctexart}\n\\include{E:/项目/别处/外部文件}\n",
    );
    fs.put_file("E:/项目/别处/外部文件.tex", "\\section{不应被读到}\n");

    let buffers: HashMap<String, String> = HashMap::new();
    let ctx = OutlineContext {
        root: Path::new(ROOT),
        root_file: Some(Path::new(&main)),
        buffers: &buffers,
        fallback_files: None,
    };
    let nodes = block_on(outline_load(&ctx, &fs));
    assert!(
        nodes.is_empty(),
        "项目外的中文路径必须被 D8 守卫拒绝，实得 {nodes:?}"
    );
}

#[test]
fn parse_log_keeps_chinese_file_name_and_line() {
    // 快照来源：实测 `tmp/gbk测试.log` 的真实片段（TeX Live 2026 / xelatex）。
    // 中文文件名在日志中是 UTF-8，解析器须能带出文件名 + 行号，错误列表才能正确跳转。
    let log = "Latexmk: Run number 1 of rule 'xelatex'\n\
               (./中文主文件.tex\n\
               ! Undefined control sequence.\n\
               l.4 \\undefinedcommandhere\n\
               )\n";
    let msgs = parse_log(log);
    assert_eq!(msgs.len(), 1, "应解析出一条错误：{msgs:?}");
    assert_eq!(msgs[0].kind, MessageKind::Error);
    assert_eq!(msgs[0].line, Some(4));
    assert_eq!(
        msgs[0].file.as_deref(),
        Some("./中文主文件.tex"),
        "中文文件名必须完整保留（不得被截断/替换）"
    );
}

#[test]
fn parse_log_handles_chinese_path_in_nested_include() {
    let log = "(./中文主文件.tex\n\
               (./章节/第一章.tex\n\
               ! LaTeX Error: Something broke.\n\
               l.2 \\foo\n\
               )\n\
               )\n";
    let msgs = parse_log(log);
    assert_eq!(msgs.len(), 1);
    assert_eq!(
        msgs[0].file.as_deref(),
        Some("./章节/第一章.tex"),
        "嵌套中文路径应作为栈顶文件带出"
    );
    assert_eq!(msgs[0].line, Some(2));
}
