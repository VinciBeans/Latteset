//! 文档大纲（源结构树）——2026-09-03 从前端 `src/stores/outline.ts` + `src/texParse.ts` 下沉
//! （cli-mcp-plan.md P1-4；modules.md §12「文档大纲」）。
//!
//! 目标：**与前端旧实现行为等价**（等价的判据 = 此处单测全部对拍旧实现用例），
//! 前端 `get_outline` 命令只做传参（实时缓冲 + 兜底文件列表）与结果呈现。
//!
//! 语义（均照搬旧实现，单测锁定）：
//! - 跟随根文件（root_file）的 `\include`/`\input` 图，逐文件解析
//!   `\part/\chapter/.../\subparagraph`（含 `\section*`/`[short]`），按**文档顺序**生成嵌套标题树；
//! - 缓冲优先：打开标签的实时缓冲命中则用缓冲（未落盘也反映），否则读盘；读失败跳过；
//! - 解析前先做**行级剥离**（等效 `src/texParse.ts::stripTexComment`）：剥 `\verb` 跨度
//!   + 真注释截断（`%` 前为奇数个连续反斜杠 = 转义字面 `%`，偶数 = 注释）；
//! - `\begin{verbatim}`…`\end{verbatim}` 环境内的行跳过（其中的结构命令是字面量）；
//! - 防环 visited；`\input` 相对**项目根**解析优先、回退当前文件目录；
//! - 读集安全（D8 词法版）：磁盘读取须落在项目根内（与前端 `read_file` 命令
//!   canonicalize 校验的行为差异仅剩「符号链接目标解析」，见 modules.md §12）。

use crate::project::FileSystem;
use crate::types::OutlineNode;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// ---------------------------------------------------------------- 正则（等效 SECTION_RE/INCLUDE_RE）

/// 结构命令（含 `*`、可选 `[short]`、`{title}`；需匹配到紧邻的 `{`，避免误命中 `\sectionmark` 等）。
fn section_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\\(part|chapter|section|subsection|subsubsection|paragraph|subparagraph)(\*)?\s*(?:\[([^\]]*)\])?\s*\{([^}]*)\}",
        )
        .expect("section_re 编译")
    })
}

/// 文件引用（`\include`/`\input`）：取第一个 `{...}` 参数。
fn include_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\(include|input)(\*)?\s*\{([^}]*)\}").expect("include_re 编译"))
}

fn begin_verbatim_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\begin\{verbatim\*?\}").expect("begin_verbatim 编译"))
}

fn end_verbatim_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\\end\{verbatim\*?\}").expect("end_verbatim 编译"))
}

/// 结构命令 → 层级（等效 LEVEL 表；未知命令回退 2，与旧实现 `?? 2` 一致）。
fn level_of(name: &str) -> u32 {
    match name {
        "part" => 0,
        "chapter" => 1,
        "section" => 2,
        "subsection" => 3,
        "subsubsection" => 4,
        "paragraph" => 5,
        "subparagraph" => 6,
        _ => 2,
    }
}

// ---------------------------------------------------------------- 行级剥离（等效 texParse.ts）

/// `\verb` 的定界符合法判定：非 ASCII 字母、非空白、非反斜杠（等效 JS `[^a-zA-Z\s\\]`）。
fn is_verb_delim(c: char) -> bool {
    !c.is_ascii_alphabetic() && !c.is_whitespace() && c != '\\'
}

/// s = `\verb`（未含 `*`）之后的字符串；`consume_star=true` 按 `\verb*` 解析（`*` 属命令）。
/// 成功返回「匹配结束处相对 s 的偏移（不含）」。复刻 JS `\\verb\*?([^a-zA-Z\s\\])(.*?)\1`：
/// 定界符取紧跟字符；内容**非贪婪**到下一个定界符（首个即闭合）。
fn verb_match(s: &str, consume_star: bool) -> Option<usize> {
    let mut it = s.char_indices();
    if consume_star {
        match it.next() {
            Some((0, '*')) => {}
            _ => return None,
        }
    }
    let (delim_start, d) = it.next()?;
    if !is_verb_delim(d) {
        return None;
    }
    let rest = &s[delim_start + d.len_utf8()..];
    let i = rest.find(d)?;
    Some(delim_start + d.len_utf8() + i + d.len_utf8())
}

/// 剥除非行内 `\verb*?<delim>…<delim>` 字面量（其内容含 `%`/结构命令不应被当作注释/命令）。
/// 复刻 JS `line.replace(/\\verb\*?([^a-zA-Z\s\\])(.*?)\1/g, "")` 的整体语义：
/// - 引擎在**每个位置**尝试匹配；先按 `\verb*`（贪婪吃 `*`）尝试，失败则回退把 `*` 当定界符；
/// - 单个 `\verb` 无闭合 → 该位置不匹配，前进 1 字符继续找下一个 `\verb`（重叠如 `\\verb` 也命中）；
/// - 成功匹配后从匹配末尾继续（相邻两个 `\verb` 各自剥离）。
fn strip_verb(line: &str) -> String {
    if !line.contains("\\verb") {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len());
    let mut cursor = 0usize;
    let mut search_from = 0usize;
    while let Some(rel) = line[search_from..].find("\\verb") {
        let p = search_from + rel;
        let after = &line[p + 5..];
        let a = verb_match(after, true).map(|e| p + 5 + e);
        let b = verb_match(after, false).map(|e| p + 5 + e);
        if let Some(end) = a.or(b) {
            out.push_str(&line[cursor..p]);
            cursor = end;
            search_from = end;
        } else {
            out.push_str(&line[cursor..p + 5]);
            cursor = p + 5;
            search_from = p + 1;
        }
    }
    out.push_str(&line[cursor..]);
    out
}

/// 剥离 LaTeX 行内 `\verb` 跨度与真注释（等效 `stripTexComment`）：
/// 先剥 `\verb` 跨度，再在「真注释 `%`」处截断——`%` 前连续反斜杠为**奇数** → 转义字面 `%`
/// （`\%`、`\\\%`）；**偶数（含 0）** → 真注释（`\\%` = `\\` 换行 + `%` 注释）。
pub fn strip_tex_comment(line: &str) -> String {
    let s = strip_verb(line);
    for (i, c) in s.char_indices() {
        if c != '%' {
            continue;
        }
        let mut bs = 0usize;
        for j in (0..i).rev() {
            if s.as_bytes()[j] == b'\\' {
                bs += 1;
            } else {
                break;
            }
        }
        if bs % 2 == 0 {
            return s[..i].to_string();
        }
    }
    s
}

// ---------------------------------------------------------------- 路径归一（等效 normalizePath）

fn is_drive_abs(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'/'
}

fn is_drive_head(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// 归一化文件路径：折叠连续斜杠、剥 `.`、合并 `..`。
/// 与前端 `project.ts::normalizePath` 行为一致（含**不**转换反斜杠、绝对路径下越界 `..` 丢弃、
/// `C:` 盘符段保留、根绝对路径补 `/`）。
pub fn normalize_path(p: &str) -> String {
    let abs = p.starts_with('/') || is_drive_abs(p);
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            match out.last().copied() {
                Some(prev) => {
                    if prev != ".." && !is_drive_head(prev) {
                        out.pop();
                    } else if !abs && prev != ".." {
                        out.push(seg);
                    }
                    // 绝对路径/上一段为 `..`/盘符段 → 丢弃该 `..`
                }
                None => {
                    if !abs {
                        out.push(seg);
                    }
                }
            }
            continue;
        }
        out.push(seg);
    }
    if abs {
        let head = out.first().copied().unwrap_or("");
        if is_drive_head(head) {
            if out.len() > 1 {
                format!("{}/{}", head, out[1..].join("/"))
            } else {
                format!("{head}/")
            }
        } else {
            format!("/{}", out.join("/"))
        }
    } else {
        out.join("/")
    }
}

/// 目录段：等效旧实现 `dirOf`（按 `/` 切分）。
fn dir_of(p: &str) -> String {
    match p.rfind('/') {
        Some(i) => p[..i].to_string(),
        None => p.to_string(),
    }
}

/// 拼接两段路径（b 剥一次前导 `./`；a 的 `\` 转 `/`）后归一化。
pub fn join_path(a: &str, b: &str) -> String {
    let a = a.replace('\\', "/");
    let b = b.replace('\\', "/");
    let b = b.strip_prefix("./").unwrap_or(&b);
    normalize_path(&format!("{a}/{b}"))
}

/// `\include`/`\input` 参数 → 候选绝对 .tex 路径（等效 `resolveInclude`）：
/// TeX `\input` 相对**项目根**解析，故根相对优先；再回退当前文件目录。
/// 无扩展名（最后一段无 `\.[A-Za-z0-9]+`）则补 `.tex`。
pub fn resolve_include(raw: &str, from_file: &str, project_root: &str) -> Vec<String> {
    let mut rel = raw.replace('\\', "/");
    if let Some(s) = rel.strip_prefix("./") {
        rel = s.to_string();
    }
    if !has_extension(&rel) {
        rel.push_str(".tex");
    }
    let root = project_root.replace('\\', "/");
    let mut cands: Vec<String> = Vec::new();
    let root_rel = join_path(&root, &rel);
    if !root_rel.is_empty() {
        cands.push(root_rel);
    }
    let file_rel = join_path(&dir_of(from_file), &rel);
    if !file_rel.is_empty() && !cands.contains(&file_rel) {
        cands.push(file_rel);
    }
    cands
}

fn has_extension(rel: &str) -> bool {
    let seg = rel.rsplit('/').next().unwrap_or("");
    match seg.rfind('.') {
        Some(i) => i + 1 < seg.len() && seg[i + 1..].chars().all(|c| c.is_ascii_alphanumeric()),
        None => false,
    }
}

// ---------------------------------------------------------------- 遍历与建树

/// 扁平大纲项（内部类型；DTO 为 [`OutlineNode`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    pub level: u32,
    pub title: String,
    pub file: String,
    pub line: u32,
}

/// 大纲构建输入（语义与旧实现 `refresh()` 一致）。
pub struct OutlineContext<'a> {
    /// 项目根（后端 `ProjectState.root`）。
    pub root: &'a Path,
    /// 根文件（存在则走 include 图；None 用兜底列表/全量扫描）。
    pub root_file: Option<&'a Path>,
    /// 打开标签的实时缓冲（键 = 归一化路径；**缓冲优先**，未落盘也反映）。
    pub buffers: &'a HashMap<String, String>,
    /// 无根文件时的兜底文件列表（前端文件树已排序，保持其语义；None 则自动扫描）。
    pub fallback_files: Option<&'a [PathBuf]>,
}

#[derive(Default)]
struct Walk {
    flat: Vec<OutlineItem>,
    visited: HashSet<String>,
}

/// 构建大纲（等效前端 `refresh` + `parseFile` + `buildTree` 全链路）。
pub async fn load(ctx: &OutlineContext<'_>, fs: &dyn FileSystem) -> Vec<OutlineNode> {
    // 项目根按正斜杠归一，作为磁盘读集的词法前缀（D8）
    let root_str = ctx.root.to_string_lossy();
    let root_prefix = format!("{}/", root_str.replace('\\', "/"));
    let mut walk = Walk::default();
    if let Some(root_file) = ctx.root_file {
        parse_file(&root_str, &root_file.to_string_lossy(), &root_prefix, ctx, &mut walk, fs).await;
    } else if let Some(files) = ctx.fallback_files {
        for p in files {
            parse_file(&root_str, &p.to_string_lossy(), &root_prefix, ctx, &mut walk, fs).await;
        }
    } else if let Ok(files) = crate::project::collect_tex_files(fs, ctx.root).await {
        // CLI/MCP 复用路径：与 open_project 同源的收集规则（tmp/ 与隐藏项排除）
        for p in files {
            parse_file(&root_str, &p.to_string_lossy(), &root_prefix, ctx, &mut walk, fs).await;
        }
    }
    build_tree(&walk.flat)
}

/// 递归解析单文件：按行扫描，结构命令入列，文件引用递归（文档顺序）。
/// 递归 async fn 需 boxing（E0733）。
#[allow(clippy::type_complexity)]
fn parse_file<'a>(
    root: &'a str,
    raw: &'a str,
    root_prefix: &'a str,
    ctx: &'a OutlineContext<'a>,
    walk: &'a mut Walk,
    fs: &'a dyn FileSystem,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        let key = normalize_path(raw);
        if !walk.visited.insert(key.clone()) {
            return; // 防环 / 重复包含
        }
        let content = match ctx.buffers.get(&key) {
            Some(c) => c.clone(),
            None => {
                // 读集安全（D8 词法版）：仅读项目根内路径；读失败（不存在/不可读）→ 跳过
                // （旧实现经 read_file 的 canonicalize 校验 + catch 跳过，语义一致）
                if !key.replace('\\', "/").starts_with(root_prefix) {
                    return;
                }
                match fs.read_to_string(Path::new(&key)).await {
                    Ok(c) => c,
                    Err(_) => return,
                }
            }
        };
        let mut in_verbatim = false;
        for (i, line) in content.split('\n').enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let trimmed = strip_tex_comment(line);
            let trimmed = trimmed.trim_start();
            if trimmed.is_empty() {
                continue; // 空行 / 行首注释
            }
            // 跳过 verbatim 环境：其中的 \section/\include 是字面量
            if begin_verbatim_re().is_match(trimmed) {
                in_verbatim = true;
                continue;
            }
            if end_verbatim_re().is_match(trimmed) {
                in_verbatim = false;
                continue;
            }
            if in_verbatim {
                continue;
            }
            if let Some(cap) = include_re().captures(trimmed) {
                let arg = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                for cand in resolve_include(arg, &key, root) {
                    parse_file(root, &cand, root_prefix, ctx, &mut *walk, fs).await;
                }
                continue;
            }
            if let Some(cap) = section_re().captures(trimmed) {
                let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                let title = cap.get(4).map(|m| m.as_str().trim()).unwrap_or("");
                walk.flat.push(OutlineItem {
                    level: level_of(name),
                    title: title.to_string(),
                    file: key.clone(),
                    line: (i + 1) as u32,
                });
            }
        }
    })
}

/// 扁平列表 → 按层级嵌套的树（等效 `buildTree`：栈式；空标题跳过；fileBase = 最后一段）。
pub fn build_tree(flat: &[OutlineItem]) -> Vec<OutlineNode> {
    struct Raw {
        node: OutlineNode,
        children: Vec<usize>,
    }
    let mut all: Vec<Option<Raw>> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for f in flat {
        if f.title.is_empty() {
            continue;
        }
        let file_base = f.file.rsplit('/').next().unwrap_or(&f.file).to_string();
        let idx = all.len();
        all.push(Some(Raw {
            node: OutlineNode {
                title: f.title.clone(),
                level: f.level,
                file: f.file.clone(),
                line: f.line,
                file_base,
                children: Vec::new(),
            },
            children: Vec::new(),
        }));
        while let Some(&top) = stack.last() {
            if all[top].as_ref().unwrap().node.level >= f.level {
                stack.pop();
            } else {
                break;
            }
        }
        match stack.last().copied() {
            Some(parent) => all[parent].as_mut().unwrap().children.push(idx),
            None => roots.push(idx),
        }
        stack.push(idx);
    }
    fn assemble(all: &mut [Option<Raw>], idx: usize) -> OutlineNode {
        let mut raw = all[idx].take().expect("大纲树节点只取一次");
        raw.node.children = raw
            .children
            .into_iter()
            .map(|c| assemble(all, c))
            .collect();
        raw.node
    }
    roots.into_iter().map(|r| assemble(&mut all, r)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{block_on, FakeFS};

    // ------------------------------------------------ strip_tex_comment（对拍 texParse.spec.ts 全部用例）

    #[test]
    fn strip_comment_basic() {
        assert_eq!(strip_tex_comment(r"\begin{eq} % 注释"), r"\begin{eq} ");
        assert_eq!(strip_tex_comment(r"\section{A}%x"), r"\section{A}");
        assert_eq!(strip_tex_comment(r" % \begin{Y}"), " ");
        assert_eq!(strip_tex_comment(r"\%\section{X}"), r"\%\section{X}");
        // C5：a\\%b（% 前 2 个反斜杠=偶数 → 注释）→ 截断到 a\\
        assert_eq!(strip_tex_comment("a\\\\%b"), "a\\\\");
    }

    #[test]
    fn strip_verb_spans() {
        // \verb|%| 是字面量，其 % 不应被当注释；\section{X} 在其后保留
        assert_eq!(strip_tex_comment(r"\verb|%|\section{X}"), r"\section{X}");
        // \verb|...| 内 `\begin{Y}` 是字面量，把跨度剥掉
        assert_eq!(
            strip_tex_comment(r"\begin{X} \verb|\begin{Y}| \end{X}"),
            r"\begin{X}  \end{X}"
        );
        // 非贪婪到首个闭合定界符
        assert_eq!(strip_tex_comment(r"\verb|a|b"), "b");
        // 相邻两个 \verb 各自剥离
        assert_eq!(strip_tex_comment(r"\verb|a|\verb|b|"), "");
        // 未闭合（且无后续合法 \verb）→ 原样保留
        assert_eq!(strip_tex_comment(r"\verb|a"), r"\verb|a");
        // 先 `\verb*` 贪婪尝试：\verb*|x| 整段剥离
        assert_eq!(strip_tex_comment(r"\verb*|x|"), "");
        // 回退语义：`*` 当定界符（\verb*a* = * 字面量内容 a）
        assert_eq!(strip_tex_comment(r"\verb*a*"), "");
        // `*` 后接字母 → 两种尝试都失败 → 原样
        assert_eq!(strip_tex_comment(r"\verb*ab"), r"\verb*ab");
        // `\\verb|x|` 命中的是第二个反斜杠（JS 逐位置匹配），剩一个 `\`
        assert_eq!(strip_tex_comment(r"\\verb|x|"), r"\");
        // 未闭合的内层 \verb 被外层跨度吞掉（与 JS replace 一致）
        assert_eq!(strip_tex_comment(r"x\verb|a \verb|b|"), "xb|");
    }

    // ------------------------------------------------ normalize_path（对拍 project.spec.ts 用例）

    #[test]
    fn normalize_path_cases() {
        assert_eq!(
            normalize_path("E:/Works/tex-presso/test_file/projects/multifile/./main.tex"),
            "E:/Works/tex-presso/test_file/projects/multifile/main.tex"
        );
        assert_eq!(
            normalize_path("E:/Works/tex-presso/test_file/projects/multifile/main.tex"),
            "E:/Works/tex-presso/test_file/projects/multifile/main.tex"
        );
        assert_eq!(
            normalize_path("/home/u/proj/a/../chapters/./b.tex"),
            "/home/u/proj/chapters/b.tex"
        );
        assert_eq!(normalize_path("./main.tex"), "main.tex");
        assert_eq!(normalize_path("/home/u/proj/main.tex"), "/home/u/proj/main.tex");
        // 相对路径的开头 `..` 保留；越界 `..` 在绝对路径下丢弃
        assert_eq!(normalize_path("a/../.."), "..");
        assert_eq!(normalize_path("/a/../.."), "/");
        assert_eq!(normalize_path("E:/proj/../x.tex"), "E:/x.tex");
        // 盘符段不因 `..` 弹出（等效 JS：prev 盘符不 pop、置空丢弃）
        assert_eq!(normalize_path("E:/proj/C:/x.tex"), "E:/proj/C:/x.tex");
        // 反斜杠不转换（旧实现语义）
        assert_eq!(normalize_path(r"E:\proj\main.tex"), r"E:\proj\main.tex");
    }

    // ------------------------------------------------ resolve_include

    #[test]
    fn resolve_include_priority_and_ext() {
        // 根相对优先；当前文件目录候选与此重合 → 去重
        assert_eq!(
            resolve_include("chapters/intro", "E:/proj/main.tex", "E:/proj"),
            vec!["E:/proj/chapters/intro.tex"]
        );
        // 子目录回退（根相对失败的写法）→ 两条候选
        assert_eq!(
            resolve_include("b/../a", "E:/proj/sub/c.tex", "E:/proj"),
            vec!["E:/proj/a.tex", "E:/proj/sub/a.tex"]
        );
        // 无扩展名补 .tex；已有扩展（含大写）不补；文件目录候选不重合
        assert_eq!(
            resolve_include("chapters/intro", "m.tex", "E:/p"),
            vec!["E:/p/chapters/intro.tex", "m.tex/chapters/intro.tex"]
        );
        assert_eq!(
            resolve_include("a.TEX", "m.tex", "E:/p"),
            vec!["E:/p/a.TEX", "m.tex/a.TEX"]
        );
        // 反斜杠参数与 `./` 前缀归一
        assert_eq!(
            resolve_include(r".\chapters\a", "m.tex", "E:/p"),
            vec!["E:/p/chapters/a.tex", "m.tex/chapters/a.tex"]
        );
        // 未扩展名的目录名带点：仅最后一段判定
        assert_eq!(
            resolve_include("dir.name/x", "m.tex", "E:/p"),
            vec!["E:/p/dir.name/x.tex", "m.tex/dir.name/x.tex"]
        );
        // 空参数 → 补 .tex 的退化处理（与旧实现一致）
        assert_eq!(resolve_include("", "m.tex", "E:/p"), vec!["E:/p/.tex", "m.tex/.tex"]);
    }

    // ------------------------------------------------ build_tree

    #[test]
    fn build_tree_nesting_and_pop() {
        let flat = vec![
            item("ch1", 1, 1, "第一章"),
            item("ch1", 2, 9, "第一节"),
            item("ch1", 3, 20, "一小节"),
            item("ch1", 2, 30, "第二节"),
            item("ch2", 0, 1, "部"),
            item("ch2", 1, 5, "章下"),
        ];
        let tree = build_tree(&flat);
        assert_eq!(tree.len(), 2);
        let root = &tree[1]; // 部
        assert_eq!(root.level, 0);
        assert_eq!(root.title, "部");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].title, "章下");
        // 第一节 → 一小节 → 第二节（same level pop）
        let ch = &tree[0];
        assert_eq!(ch.children.len(), 2);
        assert_eq!(ch.children[0].children[0].title, "一小节");
        // 空标题跳过 + fileBase
        let mut flat2 = flat.clone();
        flat2.push(item("ch1", 2, 99, ""));
        let tree2 = build_tree(&flat2);
        assert_eq!(tree2.len(), 2);
        assert_eq!(tree2[0].file_base, "ch1");
    }

    fn item(file: &str, level: u32, line: u32, title: &str) -> OutlineItem {
        OutlineItem {
            level,
            title: title.to_string(),
            file: file.to_string(),
            line,
        }
    }

    // ------------------------------------------------ load（FakeFS 全链路）

    /// 与 `test_file/projects/multifile/` 同构的固定样张（自包含，不依赖外部文件）：
    /// main.tex 组织 3 章 + part + 2 章，含 \verb 字面量、注释、verbatim 环境等边界。
    const MAIN: &str = r"\documentclass{ctexbook}
\begin{document}
% 注释 % \section{隐藏}
\include{chapters/intro}
\include{chapters/math}
\include{chapters/tables}
\part{总结与附录}
\include{sections/conclusion}
\appendix
\include{sections/appendix}
\end{document}
";
    const INTRO: &str = r"\chapter{引言}
\label{ch:intro}
\section{项目概述}
正文 % \section{行内注释伪项}
\section{结构说明}
";
    const MATH: &str = r"\chapter{数学}
\section{公式}
代码 \verb|\begin{document}| 与 \verb|%|。
\begin{verbatim}
\section{verbatim 内伪项}
\end{verbatim}
\section{matrix 与求和}
";

    fn multifile_fixture() -> FakeFS {
        let mut fs = FakeFS::new();
        fs.put_file("proj/main.tex", MAIN);
        fs.put_file("proj/chapters/intro.tex", INTRO);
        fs.put_file("proj/chapters/math.tex", MATH);
        fs.put_file("proj/chapters/tables.tex", "\\chapter{表格}\n\\section{编译引擎对比}\n\\section{列表与文本}\n");
        fs.put_file("proj/sections/conclusion.tex", "\\chapter{结论}\n\\section{展望}\n");
        fs.put_file("proj/sections/appendix.tex", "\\chapter{附录}\n\\section{宏包清单}\n\\section{引擎选择}\n");
        fs
    }

    fn ctx<'a>(root: &'a Path, root_file: Option<&'a Path>, buffers: &'a HashMap<String, String>) -> OutlineContext<'a> {
        OutlineContext {
            root,
            root_file,
            buffers,
            fallback_files: None,
        }
    }

    #[test]
    fn load_multifile_full_tree() {
        let fs = multifile_fixture();
        let no_buffers = HashMap::new();
        let tree = block_on(load(
            &ctx(Path::new("proj"), Some(Path::new("proj/main.tex")), &no_buffers),
            &fs,
        ));
        // 章×3 + 部 在根层：引言/数学/表格/部
        let titles: Vec<&str> = tree.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["引言", "数学", "表格", "总结与附录"]);
        // 部 → 结论 → 展望；附录 → 2 节
        let part = &tree[3];
        assert_eq!(part.children.len(), 2);
        assert_eq!(part.children[0].title, "结论");
        assert_eq!(part.children[0].children[0].title, "展望");
        assert_eq!(part.children[1].title, "附录");
        assert_eq!(part.children[1].children.len(), 2);
        // 数学：\verb 字面量与 verbatim 环境不产生伪项
        assert_eq!(tree[1].children[0].title, "公式");
        assert_eq!(tree[1].children[1].title, "matrix 与求和");
        // 行号 1-based；file 为归一化路径
        assert_eq!(tree[0].line, 1);
        assert_eq!(tree[0].file, "proj/chapters/intro.tex");
        // 注释里的 \section 不产生伪项（intro 只有 2 节）
        assert_eq!(tree[0].children.len(), 2);
        // 部在 main.tex 第 7 行
        assert_eq!(part.line, 7);
    }

    #[test]
    fn load_buffer_first_and_disk_fallback() {
        let mut fs = multifile_fixture();
        fs.put_file("proj/chapters/intro.tex", "\\chapter{磁盘旧标题}\n");
        let mut buffers = HashMap::new();
        buffers.insert("proj/chapters/intro.tex".to_string(), "\\chapter{缓冲新标题}\n".to_string());
        let tree = block_on(load(
            &ctx(Path::new("proj"), Some(Path::new("proj/main.tex")), &buffers),
            &fs,
        ));
        assert_eq!(tree[0].title, "缓冲新标题");
    }

    #[test]
    fn load_handles_cycle_and_missing_target() {
        let mut fs = FakeFS::new();
        fs.put_file("proj/a.tex", "\\section{A}\n\\input{b}\n");
        fs.put_file("proj/b.tex", "\\section{B}\n\\input{a}\n\\input{ghost}\n");
        let no_buffers = HashMap::new();
        let tree = block_on(load(
            &ctx(Path::new("proj"), Some(Path::new("proj/a.tex")), &no_buffers),
            &fs,
        ));
        let titles: Vec<&str> = tree.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["A", "B"]); // 防环不重复；缺失目标静默跳过
    }

    #[test]
    fn load_out_of_root_include_is_skipped() {
        let mut fs = FakeFS::new();
        fs.put_file("proj/main.tex", "\\input{../outside}\n\\section{OK}\n");
        fs.put_file("outside.tex", "\\section{外部}\n"); // 若读集不守边界会被包含
        let no_buffers = HashMap::new();
        let tree = block_on(load(
            &ctx(Path::new("proj"), Some(Path::new("proj/main.tex")), &no_buffers),
            &fs,
        ));
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].title, "OK");
    }

    #[test]
    fn load_fallback_files_no_root_file() {
        let mut fs = FakeFS::new();
        fs.put_file("proj/b.tex", "\\subsection{B1}\n");
        fs.put_file("proj/a.tex", "\\section{A1}\n\\input{b}\n");
        let files = vec![PathBuf::from("proj/a.tex"), PathBuf::from("proj/b.tex")]; // 前端已排序
        let no_buffers = HashMap::new();
        let ctx_ = OutlineContext {
            root: Path::new("proj"),
            root_file: None,
            buffers: &no_buffers,
            fallback_files: Some(&files),
        };
        let tree = block_on(load(&ctx_, &fs));
        // a.tex 先解析：A1 下递归 b（visited 后 fallback 不再二次解析）
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].title, "A1");
        assert_eq!(tree[0].children[0].title, "B1");
    }

    #[test]
    fn load_auto_scan_when_no_fallback() {
        // CLI/MCP 复用路径：fallback_files=None → collect_tex_files（tmp/ 与隐藏项排除）
        let mut fs = FakeFS::new();
        fs.put_file("proj/z.tex", "\\section{Z}\n");
        fs.put_file("proj/a.tex", "\\section{A}\n");
        fs.put_file("proj/tmp/x.tex", "\\section{tmp 不应出现}\n");
        let no_buffers = HashMap::new();
        let ctx_ = OutlineContext {
            root: Path::new("proj"),
            root_file: None,
            buffers: &no_buffers,
            fallback_files: None,
        };
        let tree = block_on(load(&ctx_, &fs));
        let titles: Vec<&str> = tree.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["A", "Z"]); // 收集排序（a < z）
    }
}
