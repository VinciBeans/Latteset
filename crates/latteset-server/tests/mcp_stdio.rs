//! 用**真实二进制**跑一遍 MCP 协议（stdio 传输）：握手 → 工具表 → 工具调用 → 错误面。
//!
//! 为什么值得单独一个集成测试：MCP 的失败模式大多是"协议层看起来通了、工具层其实没通"
//! （字段名/大小写、content 结构、isError 语义）。这里直接 spawn `latteset-mcp`、按 stdio
//! 约定喂 JSON-RPC、逐条断言响应形状——等于把一个最小 MCP client 固化进仓库，
//! 以后换 SDK 或改工具表都能立刻发现回归。
//!
//! 需要 LaTeX 的 `compile` 用例标 `#[ignore]`（与仓库其它真实引擎用例一致）。

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("latteset-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".config")).unwrap();
        std::fs::write(
            dir.join("main.tex"),
            "\\documentclass{article}\n\\begin{document}\n\\section{一}\n内容\n\\end{document}\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("chapters")).unwrap();
        std::fs::write(dir.join("chapters/a.tex"), "子文件\n").unwrap();
        Self { dir }
    }

    fn project(&self) -> &Path {
        &self.dir
    }

    fn config(&self) -> PathBuf {
        self.dir.join(".config")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// 与 `latteset-mcp` 的一次 stdio 会话。
struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Client {
    fn start(project: &Path, config: &Path) -> Self {
        let exe = env!("CARGO_BIN_EXE_latteset-mcp");
        let mut child = Command::new(exe)
            .arg("--project")
            .arg(project)
            .arg("--config-dir")
            .arg(config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()) // 日志走 stderr，测试里不关心
            .spawn()
            .expect("启动 latteset-mcp 失败");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self { child, stdin, stdout, next_id: 0 }
    }

    fn send(&mut self, mut msg: Value) -> Value {
        self.next_id += 1;
        msg["id"] = json!(self.next_id);
        self.send_raw(&msg);
        self.read()
    }

    fn notify(&mut self, msg: Value) {
        self.send_raw(&msg);
    }

    fn send_raw(&mut self, msg: &Value) {
        let line = serde_json::to_string(msg).unwrap();
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("读响应失败");
        assert!(!line.trim().is_empty(), "服务端提前关闭了 stdout");
        serde_json::from_str(&line).expect("响应必须是合法 JSON")
    }

    fn call_tool(&mut self, name: &str, args: Value) -> Value {
        let resp = self.send(json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": { "name": name, "arguments": args }
        }));
        resp["result"].clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn handshake(c: &mut Client) -> Value {
    let init = c.send(json!({
        "jsonrpc": "2.0",
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "latteset-test", "version": "0" }
        }
    }));
    // 通知：无响应
    c.notify(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    init
}

#[test]
fn handshake_reports_server_info_and_tool_capability() {
    let f = Fixture::new("handshake");
    let mut c = Client::start(f.project(), &f.config());
    let init = handshake(&mut c);

    assert_eq!(init["jsonrpc"], "2.0");
    let result = &init["result"];
    assert_eq!(result["protocolVersion"], "2024-11-05", "客户端版本受支持时应回显");
    assert_eq!(result["serverInfo"]["name"], "latteset");
    assert!(result["capabilities"]["tools"].is_object(), "必须声明 tools 能力");
    assert!(result["instructions"].is_string(), "instructions 帮 Agent 理解调用顺序");
}

#[test]
fn tools_list_covers_the_read_edit_compile_loop() {
    let f = Fixture::new("tools");
    let mut c = Client::start(f.project(), &f.config());
    handshake(&mut c);

    let resp = c.send(json!({ "jsonrpc": "2.0", "method": "tools/list", "params": {} }));
    let tools = resp["result"]["tools"].as_array().expect("tools 应是数组").clone();
    let names: Vec<String> = tools
        .iter()
        .map(|t| t["name"].as_str().unwrap_or_default().to_string())
        .collect();
    for expected in [
        "project_open",
        "compile",
        "compile_get_errors",
        "compile_get_status",
        "outline_get",
        "project_tree",
        "file_read",
        "file_write",
        "synctex_forward",
        "synctex_inverse",
        "settings_get",
    ] {
        assert!(names.contains(&expected.to_string()), "缺工具 {expected}：{names:?}");
    }
    // 每个工具都要有可用 schema 与描述（Agent 只看这些）
    for t in &tools {
        assert!(t["description"].as_str().is_some_and(|d| d.len() > 10), "{t}");
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
    }
}

#[test]
fn tool_calls_round_trip_over_stdio() {
    let f = Fixture::new("calls");
    let mut c = Client::start(f.project(), &f.config());
    handshake(&mut c);

    // 1) 预打开的项目可直接用：tree
    let tree = c.call_tool("project_tree", json!({}));
    assert_eq!(tree["isError"], false, "{tree}");
    let files = tree["structuredContent"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "应看到 main.tex + chapters/a.tex：{files:?}");

    // 2) 文件读写：写 → 读 → 内容一致（含中文）
    let write = c.call_tool(
        "file_write",
        json!({ "path": "chapters/b.tex", "content": "第二章：中文内容\n" }),
    );
    assert_eq!(write["isError"], false, "{write}");
    let read = c.call_tool("file_read", json!({ "path": "chapters/b.tex" }));
    assert_eq!(read["structuredContent"]["content"], "第二章：中文内容\n");

    // 3) 大纲（结构化，AI 用它定位章节）
    let outline = c.call_tool("outline_get", json!({}));
    assert_eq!(outline["isError"], false, "{outline}");
    let nodes = outline["structuredContent"].as_array().unwrap();
    assert_eq!(nodes[0]["title"], "一", "{nodes:?}");

    // 4) 编译前：status 明确说"还没编译过"，而不是报错
    let status = c.call_tool("compile_get_status", json!({}));
    assert_eq!(status["structuredContent"]["status"], "none");

    // 5) settings：Agent 可以确认引擎/超时/模式
    let settings = c.call_tool("settings_get", json!({}));
    assert_eq!(settings["structuredContent"]["compile"]["engine"], "xelatex");

    // 6) 工具内错误按 isError 回（不是协议错误），并带 {code,message}
    let bad = c.call_tool("file_read", json!({ "path": "../../etc/passwd" }));
    assert_eq!(bad["isError"], true, "{bad}");
    let code = bad["structuredContent"]["code"].as_str().unwrap_or_default();
    assert!(code == "Invalid" || code == "NotFound", "{bad}");
}

#[test]
fn unknown_method_and_unknown_tool_are_reported_distinctly() {
    let f = Fixture::new("errors");
    let mut c = Client::start(f.project(), &f.config());
    handshake(&mut c);

    // 未知方法 → JSON-RPC 协议错误
    let resp = c.send(json!({ "jsonrpc": "2.0", "method": "no/suchMethod", "params": {} }));
    assert_eq!(resp["error"]["code"], -32601, "{resp}");

    // 未知工具 → isError（工具层错误，Agent 能读到 message）
    let bad = c.call_tool("no_such_tool", json!({}));
    assert_eq!(bad["isError"], true, "{bad}");
    assert!(bad["structuredContent"]["message"].as_str().unwrap_or_default().contains("未知工具"), "{bad}");

    // ping 必须回空结果（客户端保活）
    let pong = c.send(json!({ "jsonrpc": "2.0", "method": "ping", "params": {} }));
    assert_eq!(pong["result"], json!({}));
}

#[test]
fn project_can_be_switched_per_call() {
    let f = Fixture::new("switch");
    // 第二个项目必须放在夹具**之外**：放进夹具里会让根文件探测看到两个 \documentclass → Multiple
    let other = std::env::temp_dir().join(format!("latteset-mcp-other-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&other);
    std::fs::create_dir_all(other.join(".config")).unwrap();
    std::fs::write(
        other.join("paper.tex"),
        "\\documentclass{article}\n\\begin{document}p\\end{document}\n",
    )
    .unwrap();

    let mut c = Client::start(f.project(), &f.config());
    handshake(&mut c);

    // 预打开的项目：project_open 不带参数即回报当前项目
    let first = c.call_tool("project_open", json!({}));
    assert!(
        first["structuredContent"]["root_file"].as_str().unwrap_or_default().ends_with("main.tex"),
        "应回报预打开项目的根文件：{first}"
    );

    // 显式传 project → 切过去（Agent 常见用法：一个 server 管多个项目）
    let second = c.call_tool("project_open", json!({ "project": other.to_string_lossy() }));
    assert!(
        second["structuredContent"]["root_file"].as_str().unwrap_or_default().ends_with("paper.tex"),
        "{second}"
    );

    // 切过去之后的工具调用落在新项目上（而不是粘在旧项目）
    let tree = c.call_tool("project_tree", json!({ "all": false }));
    let files = tree["structuredContent"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "新项目只有 paper.tex：{files:?}");

    let _ = std::fs::remove_dir_all(&other);
}

/// 真实编译：需要 latexmk/xelatex（`cargo test -p latteset-server -- --ignored`）。
#[test]
#[ignore]
fn compile_tool_returns_errors_and_then_success() {
    let f = Fixture::new("compile");
    let mut c = Client::start(f.project(), &f.config());
    handshake(&mut c);

    // 先写坏：缺宏包 → 期望结构化错误 + 诊断
    let _ = c.call_tool("file_write", json!({
        "path": "main.tex",
        "content": "\\documentclass{article}\n\\begin{document}\n\\usepackage{nosuchpackagexyz}\n\\end{document}\n"
    }));
    let failed = c.call_tool("compile", json!({}));
    assert_eq!(failed["isError"], false, "工具本身成功执行（编译结果是数据）：{failed}");
    let report = &failed["structuredContent"];
    assert_eq!(report["status"], "failed", "{report}");
    let errors = report["errors"].as_array().unwrap();
    assert!(!errors.is_empty());
    assert!(
        errors.iter().any(|e| e["diagnosis"].is_object()),
        "错误应带诊断：{errors:?}"
    );

    // 再改对 → 期望 success + PDF 路径 + 耗时
    let _ = c.call_tool("file_write", json!({
        "path": "main.tex",
        "content": "\\documentclass{article}\n\\begin{document}\nHello\n\\end{document}\n"
    }));
    let ok = c.call_tool("compile", json!({}));
    let report = &ok["structuredContent"];
    assert_eq!(report["status"], "success", "{report}");
    assert!(report["elapsed_ms"].as_u64().unwrap_or(0) > 0);
    let pdf = report["pdf_path"].as_str().unwrap();
    assert!(Path::new(pdf).exists(), "PDF 应存在：{pdf}");

    // 编译后 status/errors 同步可查（会话内状态）
    let status = c.call_tool("compile_get_status", json!({}));
    assert_eq!(status["structuredContent"]["status"], "success");
    let errors = c.call_tool("compile_get_errors", json!({}));
    assert_eq!(errors["structuredContent"]["errors"].as_array().unwrap().len(), 0);
}
