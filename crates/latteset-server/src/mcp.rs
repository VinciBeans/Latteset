//! MCP（Model Context Protocol）stdio 传输层：JSON-RPC 2.0 + `tools/list` / `tools/call`。
//!
//! **为什么自己实现而不是引 SDK**：本服务只需要 MCP 的最小可用子集（initialize / tools / ping），
//! 而协议本体就是 JSON-RPC 消息 + 一张工具表。自实现没有额外依赖、行为完全可读（与仓库里
//! "外部依赖尽量收敛"的取向一致）；将来若要 notifications/订阅再换 SDK 也不影响工具面。
//!
//! 传输约定（MCP stdio）：**一行一条 JSON-RPC 消息**，stdout 只允许协议消息——
//! 任何日志都必须走 stderr（本模块不做日志，调用方负责）。
//!
//! 支持的方法：
//! - `initialize` → 回 `protocolVersion`（回显客户端版本，取交集）+ `capabilities.tools` + `serverInfo`
//! - `notifications/initialized` → 通知，无响应
//! - `tools/list` → 工具表（name / description / inputSchema）
//! - `tools/call` → 执行工具，回 `content: [{type:"text", text:<JSON>}]`
//! - `ping` → `{}`
//!
//! 未知方法回 JSON-RPC `-32601`；工具内错误按 MCP 约定回 `isError: true` 的 content（而非协议错误），
//! 这样 Agent 能读到 `{code, message}` 而不是一个裸的错误码。

use crate::{ServerError, Session};
use serde_json::{json, Value};
use std::path::PathBuf;

/// 服务端支持的协议版本（客户端请求版本不在表内时回退到最新支持的）。
pub const SUPPORTED_PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
pub const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

/// 工具定义（name/description/inputSchema + 调用入口）。
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// 工具表：与 [cli-mcp-plan.md] §2 的第一/第二梯队一致（第三梯队的订阅类一期不做，用快照代替）。
///
/// [cli-mcp-plan.md]: ../../docs/cli-mcp-plan.md
pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "project_open",
            description: "打开 TeX 项目（目录）并返回根文件、候选与生效设置。多数工具需要先调用它；不传 project 时用进程当前目录。",
            input_schema: json!({
                "type": "object",
                "properties": { "project": { "type": "string", "description": "项目目录（绝对路径；省略则用当前目录）" } },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "compile",
            description: "编译一次并等待结果（等价 GUI 的「编译」）。返回 {status, errors, pdf_path, elapsed_ms, kind}。status=success 才算通过；quick=true 时只跑单趟（快，但目录/引用可能落后一趟）。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string", "description": "项目目录（省略则用当前目录；会先自动打开）" },
                    "quick": { "type": "boolean", "description": "true = 单趟直调引擎（默认 false = 完整 latexmk 收敛）" }
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "compile_get_status",
            description: "本会话最近一次编译的状态（status/elapsed_ms/kind）；未编译过时返回 none。",
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        Tool {
            name: "compile_get_errors",
            description: "本会话最近一次编译的结构化错误列表 [{message,file,line,kind,diagnosis}]（含「原因 + 怎么改」诊断）。",
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        Tool {
            name: "outline_get",
            description: "文档大纲（章/节/小节 → 文件:行号），用于理解文档结构。",
            input_schema: json!({
                "type": "object",
                "properties": { "project": { "type": "string" } },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "project_tree",
            description: "项目文件列表。默认只列 .tex（排除 tmp/ 与隐藏项）；all=true 列全部文件。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project": { "type": "string" },
                    "all": { "type": "boolean", "description": "true = 包含非 .tex 文件" }
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "file_read",
            description: "读项目内文件（UTF-8；非 UTF-8 会返回明确错误）。",
            input_schema: json!({
                "type": "object",
                "properties": { "path": { "type": "string", "description": "项目内绝对路径或相对路径" } },
                "required": ["path"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "file_write",
            description: "写项目内文件（覆盖；文件不存在则创建，父目录必须在项目内）。返回写入的绝对路径。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synctex_forward",
            description: "源码位置 → PDF 页码/坐标（{page,x,y}）。需要先编译过。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string" }, "line": { "type": "integer" }, "column": { "type": "integer" }
                },
                "required": ["file", "line"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "synctex_inverse",
            description: "PDF 页码/坐标 → 源码位置（{source, note}）。命中目录/参考文献等生成内容时会就近回落到最近源码，并在 note 里说明。",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "page": { "type": "integer" }, "x": { "type": "number" }, "y": { "type": "number" }
                },
                "required": ["page"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "settings_get",
            description: "生效设置（mode/engine/debounce/timeout/root_file）——确认 AI 的假设与实际一致。",
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
    ]
}

/// 处理一条 JSON-RPC 消息；返回 `None` 表示这是通知（无需响应）。
pub async fn handle_message(session: &mut Session, msg: Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or_default();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    // 通知（无 id）：只做状态迁移，不回包
    if id.is_none() {
        if method == "notifications/initialized" || method == "initialized" {
            return None;
        }
        return None;
    }
    let id = id.unwrap();

    let response = match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or("");
            let version = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested) {
                requested
            } else {
                DEFAULT_PROTOCOL_VERSION
            };
            ok(id, json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "latteset", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Latteset headless：驱动 LaTeX 项目「读 → 改 → 编译验证 → 修」。先 project_open（或直接给 compile 传 project），再 compile（默认完整编译、返回结构化错误与诊断），改完再 compile。"
            }))
        }
        "ping" => ok(id, json!({})),
        "tools/list" => {
            let list: Vec<Value> = tools()
                .iter()
                .map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.input_schema }))
                .collect();
            ok(id, json!({ "tools": list }))
        }
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(session, &name, &args).await {
                Ok(value) => ok(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": pretty(&value) }],
                        "structuredContent": value,
                        "isError": false
                    }),
                ),
                Err(e) => ok(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": pretty(&serde_json::to_value(&e).unwrap_or(Value::Null)) }],
                        "structuredContent": serde_json::to_value(&e).unwrap_or(Value::Null),
                        "isError": true
                    }),
                ),
            }
        }
        other => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("不支持的方法：{other}") }
        }),
    };
    Some(response)
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// JSON 尽量单行紧凑（MCP content 是给人看也给模型看的；结构化字段同时给了 `structuredContent`）。
fn pretty(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "{}".into())
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn arg_path(args: &Value, key: &str, need_project: Option<&Session>) -> Option<PathBuf> {
    let raw = arg_str(args, key)?;
    let p = PathBuf::from(&raw);
    if p.is_absolute() {
        return Some(p);
    }
    // 相对路径：相对项目根（更符合"项目内路径"的直觉），无项目时相对 cwd
    let base = match need_project.and_then(|s| s.project()) {
        Some(proj) => proj.root.clone(),
        None => std::env::current_dir().ok()?,
    };
    Some(base.join(p))
}

/// 执行一个工具。`project` 参数存在时先切到该项目——让每条工具调用都能自洽
/// （同一个 server 可以服务多个项目；未传 `project` 时沿用当前项目）。
async fn call_tool(session: &mut Session, name: &str, args: &Value) -> Result<Value, ServerError> {
    // 需要项目的工具：显式传 `project` → 切过去；没传且尚未打开 → 用当前目录
    let needs_project = matches!(
        name,
        "compile" | "outline_get" | "project_tree" | "file_read" | "file_write" | "synctex_forward"
            | "synctex_inverse" | "settings_get" | "compile_get_status" | "compile_get_errors"
    );
    match arg_str(args, "project") {
        Some(raw) => {
            let want = crate::absolutize(std::path::Path::new(&raw));
            // 只在目标与当前不同才重开（避免每条调用都重扫项目）
            if session.project().map(|p| p.root != want).unwrap_or(true) {
                session.open_project(&want).await?;
            }
        }
        None => {
            if (needs_project || name == "project_open") && session.project().is_none() {
                crate::ensure_open(session, None).await?;
            }
        }
    }

    match name {
        "project_open" => Ok(serde_json::to_value(session.project_report().await?).unwrap_or(Value::Null)),
        "compile" => {
            let quick = args.get("quick").and_then(|v| v.as_bool()).unwrap_or(false);
            let report = session.compile(quick).await?;
            Ok(serde_json::to_value(report).unwrap_or(Value::Null))
        }
        "compile_get_status" => Ok(match session.last_report() {
            Some(r) => json!({
                "status": r.status,
                "failure": r.failure,
                "elapsed_ms": r.elapsed_ms,
                "kind": r.kind,
                "pdf_path": r.pdf_path,
                "root_file": r.root_file,
            }),
            None => json!({ "status": "none", "note": "本会话还没有编译过" }),
        }),
        "compile_get_errors" => Ok(match session.last_report() {
            Some(r) => json!({ "status": r.status, "errors": r.errors }),
            None => json!({ "status": "none", "errors": [] }),
        }),
        "outline_get" => {
            let nodes = session.outline().await?;
            Ok(serde_json::to_value(nodes).unwrap_or(Value::Null))
        }
        "project_tree" => {
            let all = args.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
            let files = session.tree(all).await?;
            Ok(json!({ "count": files.len(), "files": files }))
        }
        "file_read" => {
            let path = arg_path(args, "path", Some(session))
                .ok_or_else(|| ServerError::Invalid("缺少参数 path".into()))?;
            let content = session.read(&path).await?;
            Ok(json!({ "path": path.to_string_lossy(), "bytes": content.len(), "content": content }))
        }
        "file_write" => {
            let path = arg_path(args, "path", Some(session))
                .ok_or_else(|| ServerError::Invalid("缺少参数 path".into()))?;
            let content = arg_str(args, "content")
                .ok_or_else(|| ServerError::Invalid("缺少参数 content".into()))?;
            let written = session.write(&path, &content).await?;
            Ok(json!({ "path": written.to_string_lossy(), "bytes": content.len() }))
        }
        "synctex_forward" => {
            let file = arg_path(args, "file", Some(session))
                .ok_or_else(|| ServerError::Invalid("缺少参数 file".into()))?;
            let line = args.get("line").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let column = args.get("column").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let pos = session.synctex_forward(&file, line, column).await?;
            Ok(json!({ "page": pos.page, "x": pos.x, "y": pos.y }))
        }
        "synctex_inverse" => {
            let page = args.get("page").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let x = args.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let y = args.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let resolved = session.synctex_inverse(page, x, y).await?;
            Ok(json!({
                "source": resolved.source.map(|s| json!({
                    "file": s.file.to_string_lossy(), "line": s.line, "column": s.column
                })),
                "note": resolved.note,
            }))
        }
        "settings_get" => Ok(serde_json::to_value(session.settings()).unwrap_or(Value::Null)),
        other => Err(ServerError::Invalid(format!("未知工具：{other}"))),
    }
}
