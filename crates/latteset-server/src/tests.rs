//! headless 服务的单元/集成测试（`cargo test -p latteset-server`）。
//!
//! 不需要 LaTeX 的部分（打开/读/写/树/大纲/错误形状）在默认测试里跑；
//! 真编译与 SyncTeX 相关用例标 `#[ignore]`（与 `latteset-infra` 的约定一致，需要本机 TeX Live）：
//! `cargo test -p latteset-server -- --ignored`。

use super::*;
use std::fs;

/// 临时项目目录（中文名 + .latteset 覆盖，覆盖路径与设置读写的真实形态）。
struct TempProject {
    dir: PathBuf,
}

impl TempProject {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("latteset-server-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("创建临时项目失败");
        Self { dir }
    }

    fn put(&self, rel: &str, content: &str) {
        let p = self.dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, content).unwrap();
    }

    /// 独立配置目录：隔离全局设置，避免动到真机 `%APPDATA%`。
    fn config_dir(&self) -> PathBuf {
        let d = self.dir.join(".config");
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn session(&self) -> Session {
        Session::new(self.config_dir())
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

const MAIN: &str = "\\documentclass{article}\n\\begin{document}\n\\section{一}\n内容\n\\end{document}\n";

#[tokio::test]
async fn open_project_detects_unique_root_and_reports_settings() {
    let p = TempProject::new("open");
    p.put("main.tex", MAIN);
    p.put("chapters/a.tex", "子文件\n");

    let mut s = p.session();
    let report = s.open_project(&p.dir).await.expect("打开项目");
    assert_eq!(report.root_file.as_deref(), Some(s.project().unwrap().root_file.clone().unwrap().to_string_lossy().as_ref()));
    assert_eq!(report.root_candidates.len(), 1, "唯一候选也回报（与 GUI 语义一致）");
    // 默认设置来自隔离的配置目录（不存在 → 默认值）
    assert_eq!(report.settings.compile.timeout_secs, 120);
    assert_eq!(report.settings.compile.engine, latteset_core::types::Engine::XeLaTeX);
}

#[tokio::test]
async fn open_project_without_root_reports_candidates() {
    let p = TempProject::new("multi");
    p.put("a.tex", "\\documentclass{article}\n\\begin{document}x\\end{document}\n");
    p.put("b.tex", "\\documentclass{article}\n\\begin{document}y\\end{document}\n");

    let mut s = p.session();
    let report = s.open_project(&p.dir).await.unwrap();
    assert!(report.root_file.is_none(), "多候选时不定根文件（由调用方选择/覆盖）");
    assert_eq!(report.root_candidates.len(), 2);
}

#[tokio::test]
async fn project_override_root_file_wins() {
    let p = TempProject::new("override");
    p.put("a.tex", "\\documentclass{article}\n\\begin{document}x\\end{document}\n");
    p.put("b.tex", "\\documentclass{article}\n\\begin{document}y\\end{document}\n");
    p.put(".latteset/settings.json", "{ \"root_file\": \"b.tex\" }");

    let mut s = p.session();
    let report = s.open_project(&p.dir).await.unwrap();
    assert!(report.root_file.as_deref().unwrap().ends_with("b.tex"), "项目覆盖应生效");
    assert!(report.root_candidates.is_empty(), "手动覆盖生效 → 不再探测候选（与 GUI 一致）");
}

#[tokio::test]
async fn read_write_and_tree_are_project_scoped() {
    let p = TempProject::new("rw");
    p.put("main.tex", MAIN);
    p.put("chapters/existing.tex", "已存在的子目录文件\n");
    let mut s = p.session();
    s.open_project(&p.dir).await.unwrap();

    // 写（覆盖 + 新建，父目录必须已存在——与 GUI 的 save_all 同契约）
    s.write(&p.dir.join("chapters/new.tex"), "新文件\n").await.unwrap();
    assert_eq!(s.read(&p.dir.join("chapters/new.tex")).await.unwrap(), "新文件\n");

    // 父目录不存在 → 明确拒绝（不代建目录），错误要说清怎么办
    let err = s.write(&p.dir.join("no-such-dir/x.tex"), "x").await.unwrap_err();
    assert!(err.to_string().contains("父目录不存在"), "{err}");

    // 树：默认只 .tex
    let tex = s.tree(false).await.unwrap();
    assert!(tex.iter().all(|f| f.ends_with(".tex")), "{tex:?}");
    assert_eq!(tex.len(), 3, "{tex:?}");
    // all=true 时至少包含同样多的文件
    let all = s.tree(true).await.unwrap();
    assert!(all.len() >= tex.len());

    // 越界读写被拒（D8）：先把文件真造出来，确保判的是"越界"而不是"不存在"
    let outside = p.dir.parent().unwrap().join(format!("evil-{}.tex", std::process::id()));
    fs::write(&outside, "越界内容\n").unwrap();
    let err = s.read(&outside).await.unwrap_err();
    assert!(matches!(err, ServerError::Invalid(_)), "越界读应报 Invalid：{err:?}");
    let err = s.write(&outside, "x").await.unwrap_err();
    assert!(matches!(err, ServerError::Invalid(_)), "越界写应报 Invalid：{err:?}");
    let _ = fs::remove_file(&outside);
}

#[tokio::test]
async fn read_reports_non_utf8_with_actionable_message() {
    let p = TempProject::new("gbk");
    p.put("main.tex", MAIN);
    // 造一个非法 UTF-8 文件（GBK "中文" = D6 D0 CE C4）
    fs::write(p.dir.join("gbk.tex"), [0xD6u8, 0xD0, 0xCE, 0xC4]).unwrap();
    let mut s = p.session();
    s.open_project(&p.dir).await.unwrap();

    let err = s.read(&p.dir.join("gbk.tex")).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("UTF-8"), "{msg}");
    assert!(msg.contains("GBK"), "{msg}");
}

#[tokio::test]
async fn outline_returns_document_structure() {
    let p = TempProject::new("outline");
    p.put(
        "main.tex",
        "\\documentclass{article}\n\\begin{document}\n\\section{第一章}\n\\subsection{小节}\n\\end{document}\n",
    );
    let mut s = p.session();
    s.open_project(&p.dir).await.unwrap();
    let nodes = s.outline().await.unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].title, "第一章");
    assert_eq!(nodes[0].children.len(), 1);
    assert_eq!(nodes[0].children[0].title, "小节");
}

#[tokio::test]
async fn outline_cache_never_pins_stale_content() {
    // ⑦a 的缓存不变量：缓存只复用"扫描结果"，内容每次都重取 —— 连续调用之间改了磁盘文件，
    // 第二次必须反映新内容（若把缓存做成"按路径命中就不再读盘"，这个用例会红）。
    let p = TempProject::new("outline-cache");
    p.put(
        "main.tex",
        "\\documentclass{article}\n\\begin{document}\n\\section{旧标题}\n\\include{chapters/a}\n\\end{document}\n",
    );
    p.put("chapters/a.tex", "\\section{子文件旧}\n");
    let mut s = p.session();
    s.open_project(&p.dir).await.unwrap();
    let first = s.outline().await.unwrap();
    // 结构项按层级建树：两个 \section 同级 → 平铺两项（include 不改变层级）
    let titles: Vec<&str> = first.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(titles, vec!["旧标题", "子文件旧"]);
    // 文件路径一律**归一化**（正斜杠），与前端存储键一致——含 root_file 本身
    assert!(!first[0].file.contains('\\'), "{}", first[0].file);
    // 第二次：内容未变（走缓存复用）→ 结果一致
    let second = s.outline().await.unwrap();
    assert_eq!(second, first);
    // 改磁盘（含把 \include 换成新文件）→ 第三次必须反映
    p.put("main.tex", "\\documentclass{article}\n\\begin{document}\n\\section{新标题}\n\\include{chapters/b}\n\\end{document}\n");
    p.put("chapters/b.tex", "\\section{子文件新}\n");
    let third = s.outline().await.unwrap();
    let titles3: Vec<&str> = third.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(titles3, vec!["新标题", "子文件新"]);
}

#[tokio::test]
async fn compile_without_project_is_rejected_clearly() {
    let p = TempProject::new("noproject");
    let mut s = p.session();
    let err = s.compile(false).await.unwrap_err();
    assert!(err.to_string().contains("尚未打开项目"), "{err}");
}

#[tokio::test]
async fn compile_without_root_file_is_rejected_clearly() {
    let p = TempProject::new("noroot");
    p.put("a.tex", "\\documentclass{article}\n\\begin{document}x\\end{document}\n");
    p.put("b.tex", "\\documentclass{article}\n\\begin{document}y\\end{document}\n");
    let mut s = p.session();
    s.open_project(&p.dir).await.unwrap();
    let err = s.compile(false).await.unwrap_err();
    assert!(err.to_string().contains("未确定根文件"), "{err}");
}

// ---- 默认配置目录（与 GUI 共享设置的那条约定）----

/// 一个测试同时覆盖两种情形：**必须合并**——`LATTESET_CONFIG_DIR` 是进程级环境变量，
/// 拆成两个测试会在并行执行时互相污染（实测踩过）。
#[test]
fn default_config_dir_matches_gui_and_honours_env_override() {
    // 1) 无覆盖：末段必须是应用 identifier（Windows = %APPDATA%\com.latteset.app，即 GUI 的 app_config_dir）
    unsafe { std::env::remove_var("LATTESET_CONFIG_DIR") };
    let dir = default_config_dir();
    assert_eq!(
        dir.file_name().map(|n| n.to_string_lossy().into_owned()).as_deref(),
        Some("com.latteset.app"),
        "{dir:?}"
    );

    // 2) 有覆盖（测试/CI 隔离用）：直接返回它；空串视为未设置
    unsafe { std::env::set_var("LATTESET_CONFIG_DIR", "/tmp/latteset-cfg-probe") };
    assert_eq!(default_config_dir(), PathBuf::from("/tmp/latteset-cfg-probe"));
    unsafe { std::env::set_var("LATTESET_CONFIG_DIR", "   ") };
    assert_eq!(
        default_config_dir().file_name().map(|n| n.to_string_lossy().into_owned()).as_deref(),
        Some("com.latteset.app"),
        "空白串应视为未设置"
    );
    unsafe { std::env::remove_var("LATTESET_CONFIG_DIR") };
}

// ---- 真实编译（需要 latexmk/xelatex）----

fn latexmk_available() -> bool {
    std::process::Command::new("latexmk")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tokio::test]
#[ignore]
async fn compile_waits_and_returns_success_report() {
    if !latexmk_available() {
        eprintln!("跳过：系统未安装 latexmk");
        return;
    }
    let p = TempProject::new("compile");
    p.put("main.tex", MAIN);
    let mut s = p.session();
    s.open_project(&p.dir).await.unwrap();

    let report = s.compile(false).await.expect("编译应成功");
    assert_eq!(report.status, "success");
    assert_eq!(report.kind, "full");
    assert!(report.elapsed_ms > 0, "必须带耗时");
    let pdf = report.pdf_path.expect("应有 PDF 路径");
    assert!(PathBuf::from(&pdf).exists(), "PDF 应落在项目根：{pdf}");
    // 会话内可查询（status/errors 的数据源）
    assert_eq!(s.last_report().unwrap().status, "success");
    assert!(s.last_report().unwrap().errors.is_empty());
}

#[tokio::test]
#[ignore]
async fn compile_failure_returns_structured_errors_with_diagnosis() {
    if !latexmk_available() {
        eprintln!("跳过：系统未安装 latexmk");
        return;
    }
    let p = TempProject::new("compile-fail");
    p.put(
        "main.tex",
        "\\documentclass{article}\n\\begin{document}\n\\usepackage{nosuchpackagexyz}\n\\end{document}\n",
    );
    let mut s = p.session();
    s.open_project(&p.dir).await.unwrap();

    let report = s.compile(false).await.unwrap();
    assert_eq!(report.status, "failed");
    assert_eq!(report.failure.as_deref(), Some("content_error"));
    assert!(!report.errors.is_empty(), "应带回结构化错误");
    let diagnosed = report.errors.iter().filter(|e| e.diagnosis.is_some()).count();
    assert!(diagnosed > 0, "错误里应带「原因 + 怎么改」诊断：{:?}", report.errors);
}

#[tokio::test]
#[ignore]
async fn quick_compile_reports_kind_and_upgrade() {
    if !latexmk_available() {
        eprintln!("跳过：系统未安装 latexmk");
        return;
    }
    let p = TempProject::new("compile-quick");
    p.put("main.tex", MAIN);
    let mut s = p.session();
    s.open_project(&p.dir).await.unwrap();

    // 无产物 → Quick 请求被 runner 升级为 Full，报告要如实说明
    let first = s.compile(true).await.unwrap();
    assert_eq!(first.status, "success");
    assert_eq!(first.kind, "full");
    assert!(first.upgraded_from_quick, "首编应标注自动升级");

    // 有产物 → 真的单趟
    let second = s.compile(true).await.unwrap();
    assert_eq!(second.status, "success");
    assert_eq!(second.kind, "quick");
    assert!(!second.upgraded_from_quick);
}

#[tokio::test]
#[ignore]
async fn synctex_roundtrip_through_session() {
    if !latexmk_available() {
        eprintln!("跳过：系统未安装 latexmk");
        return;
    }
    let p = TempProject::new("synctex");
    p.put(
        "main.tex",
        "\\documentclass{article}\n\\begin{document}\n第一章内容\n\\newpage\n第二章内容\n\\end{document}\n",
    );
    let mut s = p.session();
    s.open_project(&p.dir).await.unwrap();
    assert_eq!(s.compile(false).await.unwrap().status, "success");

    let fwd = s
        .synctex_forward(&p.dir.join("main.tex"), 3, 1)
        .await
        .expect("正向定位应成功");
    assert!(fwd.page >= 1);
    let inv = s.synctex_inverse(fwd.page, fwd.x, fwd.y).await.unwrap();
    let src = inv.source.expect("反向定位应回到源码");
    assert!(src.file.to_string_lossy().ends_with("main.tex"), "{src:?}");
    assert!((src.line as i64 - 3).abs() <= 3, "行号应接近 3：{}", src.line);
}
