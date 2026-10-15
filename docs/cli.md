# CLI / MCP 使用文档

Latteset 的 headless 入口：**与 GUI 并列的第二入口**，`latteset-cli` + `latteset-mcp` 两个二进制，让脚本与 AI Agent 不经窗口驱动「读 → 改 → 编译验证 → 修」闭环。读者是写脚本的人和接 harness 的人；架构契约见 [dev/modules.md](./dev/modules.md) §8.1。

## 构建

```bash
cargo build -p latteset-server
# 产物：src-tauri/target/debug/latteset-cli.exe、latteset-mcp.exe（工作区产物，不进安装包）
```

`latteset-server` 是独立 crate（`Session` + 两个 bin），与 GUI 共用 `latteset-core` / `latteset-infra`，因此**零新增第三方依赖**、离线可构建。

## CLI

**一条命令一个 JSON**：stdout 只有 JSON 对象，日志走 stderr（级别用 `RUST_LOG`，默认 `info`）。每个进程只跑一条命令——一次性语义，不 spawn watcher、不跑调度器。

```bash
latteset-cli --project <目录> open                    # → 根文件 / 候选 / 生效设置
latteset-cli --project <目录> compile                 # 默认完整 latexmk；--quick 单趟直调引擎
latteset-cli --project <目录> compile | jq .compile.errors
latteset-cli --project <目录> write chapters/intro.tex -   # 内容从 stdin 读（躲开 shell 的 $ 展开）
latteset-cli --project <目录> forward chapters/intro.tex 3 # 源码 → PDF 页码/坐标
latteset-cli --project <目录> inverse 5 --x 100 --y 600    # PDF → 源码
latteset-cli help                                     # 内建用法（与下表同源）
```

全局参数：`--project <目录>`（省略则用当前目录）、`--config-dir <目录>`。

| CLI 子命令 | 参数 | 说明 |
|---|---|---|
| `open` | — | 打开项目 → 根文件 / 候选 / 生效设置 |
| `compile` | `[--quick]` | **编译并等待结果**：`{status, failure, errors, pdf_path, elapsed_ms, engine, kind, upgraded_from_quick}` |
| `status` | — | 本会话最近一次编译状态 |
| `errors` | — | 最近一次编译的结构化错误（含「原因 + 怎么改」诊断） |
| `outline` | — | 文档大纲（章/节 → `文件:行号`） |
| `tree` | `[--all]` | 文件列表（默认只 `.tex`，排除 `tmp/` 与隐藏项） |
| `read` | `<路径>` | 读文件（非 UTF-8 → 明确错误） |
| `write` | `<路径> <内容\|->` | 写文件（父目录必须已存在，与 GUI 同契约；`-` = 从 stdin 读） |
| `math` | `<公式\|->` | 用项目导言区编译**这一个公式**（片段文档）并给单页预览产物 |
| `forward` | `<文件> <行> [--column N]` | 源码 → PDF 页码/坐标 |
| `inverse` | `<页> [--x N] [--y N]` | PDF → 源码（命中生成内容时就近回落 + `note` 说明） |
| `settings` | — | 生效设置（确认 mode / engine / timeout 与假设一致） |

**退出码**：`0` 成功（`compile` 仅当 `status == "success"`）；`1` 编译未通过；`2` 用法/路径类错误；`3` 内部错误。JSON 里同时有 `{ok, error:{code,message}}`，两种判断方式都可用——所以 `compile` 的 `1` 是「文档没编过」，**不是**命令本身出错。

**配置目录**：默认与 GUI 相同（Windows `%APPDATA%\com.latteset.app`），CLI/MCP 与 GUI **共享同一份设置**；`--config-dir` 或 `LATTESET_CONFIG_DIR` 可覆盖（测试/CI 隔离用）。

## MCP

stdio server，harness 侧当本地 server 拉起即可：

```bash
latteset-mcp --project <目录>
```

工具表与 CLI 子命令一一对应（`project_open` / `compile` / `compile_get_status` / `compile_get_errors` / `outline_get` / `project_tree` / `file_read` / `file_write` / `synctex_forward` / `synctex_inverse` / `settings_get`）；`math` 目前只有 CLI 侧。

协议面：`initialize` 回显受支持的 `protocolVersion` + `capabilities.tools` + `instructions`，另有 `notifications/initialized`、`ping`、`tools/list`；未知方法回 JSON-RPC `-32601`，**工具内错误按 MCP 约定回 `isError: true` 的 content**（Agent 能读到 `{code,message}`，而不是一个裸协议错误）。日志（含每一次工具调用）走 **stderr**，stdout 只有协议报文——排查时别把两者混进同一个管道。

### DSH 接线

DSH 的 MCP server 由 profile 的 patch 层声明（本地文件、不入库：`~/.dsh/profiles/web/cordis.patch.yml`，`patchReload: live` = 改完即时生效）：

```yaml
- insert:
    - id: mcp-latteset
      name: '@deepseek-ai/dsh-mcp-client'
      config:
        serverName: latteset
        transport: stdio
        command: 'E:/Works/tex-presso/src-tauri/target/debug/latteset-mcp.exe'
        args: []
        cwd: 'E:/Works/tex-presso'
        env: {}
```

三个要点：

- **`command` 用可执行文件绝对路径**：MCP SDK 以 `shell: false` spawn，`.cmd` 壳（npm 风格）在 Windows 起不来；Rust 二进制直接可用，不需要 node。
- **`args` 留空 = 不预打开项目**：每次工具调用带 `project=<目录>`（相对路径按 `cwd` 解析），或先调 `project_open`。留空是刻意的——避免「当前项目」隐式粘在某个夹具上；`--project` 若要用也只影响"未显式传参"的调用。
- **`cwd` 决定相对路径基准**：设成项目仓库根，工具调用里就能写 `test_file/projects/multifile` 这类相对路径。

## 已知限制

- **同一项目不要同时用 GUI 和 CLI 编译**：两路 latexmk 会抢同一个 `tmp/`（先到先写、后到覆盖）。约定 **headless 独占**；GUI 那条路没有自写盘过滤（`.tex` 目前只有 settings.json 有过滤），想并行需要给 watch 加一次性忽略通道（未做）。
- **没有合并队列 / 超时重试 / 手动终止**：headless 是"一条命令跑一次拿结果"（AI 场景正是如此），超时树杀仍在 runner 内生效。
- **`file_write` 不代建目录**：父目录必须已存在（与 GUI `save_all` 同契约）。Agent 要新建子目录时先建目录（或用 GUI）。
- **不暴露预览渲染**：AI 不看像素——`compile` 返回 `pdf_path`，AI 自己读 PDF。
- **状态订阅（MCP notifications）未做**：用"`compile` 同步返回 + `status`/`errors` 快照"覆盖；有真实订阅需求再做 Emitter fanout。
- **根文件探测每次重扫**：多数项目 <100ms；大项目再考虑缓存（缓存一致性成本 > 收益）。
- **`latteset-mcp.exe` 常驻时不可被替换**：它作为 stdio server 一直活着，Windows 锁定已加载的可执行文件 → 重新构建 `latteset-server` 报 `os error 5`。处置见 [troubleshooting.md](./troubleshooting.md)。

## 证据与详情

实测（本机 TeX Live 2026）：`test_file/projects/multifile` 上闭环走通 `open` → `tree`（6 个 .tex）→ `outline`（章/节带 `文件:行号`）→ `read` → `write`（经 stdin 写入含 `$E=mc^2$` 的段落）→ `compile` 成功（`elapsed_ms=912`，`pdf_path=…\main.pdf`）；写坏后 `compile` 回 `status: failed` + 退出码 `1` 且带 `diagnosis.cause`/`hint`，改对后 `success`；`forward chapters/intro.tex 3` → `page 4 (70.87, 47.48)`，`inverse 5 --x 100 --y 600` → `chapters/intro.tex:24`；同一份源码连编两次 `main.pdf` SHA-256 一致（与 `SOURCE_DATE_EPOCH` 的确定性口径一致）；DSH 会话内不经 shell 调用 `project_open` → `compile`（`status=success`、`kind=full`、2789ms）→ `outline_get` → `synctex_forward chapters/intro.tex:3` → `page 4 (70.87, 47.48)`；协议面用「真实二进制 + stdin 管道」复核 `initialize` 回 `protocolVersion=2024-11-05`，`tools/list` **每项都有 `inputSchema`**（SDK 客户端会校验这一点）。回归固化在 `cargo test -p latteset-server`（`-- --ignored` 跑真编译、`--test mcp_stdio` 跑真实二进制的管道链路），见 [dev/testing.md](./dev/testing.md)。

原计划、落地时的取舍与编号对照见 [archive/cli-mcp-plan.md](./archive/cli-mcp-plan.md)。
