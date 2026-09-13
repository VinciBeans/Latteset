# CLI + MCP 交互接口（roadmap ⑥）

> 状态：**已实现**（2026-09）。让 harness / Agent **不经 GUI** 直接调用 Latteset 的编译/大纲/SyncTeX 能力，形成「读 → 改 → 编译验证 → 修」闭环。
> 实现位置：`crates/latteset-server`（headless 服务层 + 两个二进制）。设计依据与落地偏差见 §4。
> 标注约定：**已核实**=读过代码/实测确认；**分析结论**=设计推演（未实测）。

## 1. 形态与用法

```
crates/latteset-server
├── lib.rs             —— Session：headless 会话（打开项目/编译/大纲/文件/SyncTeX）
├── mcp.rs             —— MCP（stdio JSON-RPC + tools/list + tools/call）
└── bin/
    ├── latteset-cli.rs —— 命令行（一条命令一个 JSON）
    └── latteset-mcp.rs —— MCP server（stdio）
```

构建：`cargo build -p latteset-server` → `src-tauri/target/debug/latteset-{cli,mcp}.exe`（**工作区产物，未打进安装包**）。

```bash
# CLI：一条命令一个 JSON（stdout 只有 JSON，日志走 stderr）
latteset-cli --project <目录> open
latteset-cli --project <目录> tree
latteset-cli --project <目录> outline
latteset-cli --project <目录> read chapters/intro.tex
latteset-cli --project <目录> write chapters/intro.tex -      # 内容从 stdin 读（避免 shell 转义吃掉 $）
latteset-cli --project <目录> compile                        # 默认完整 latexmk；--quick 单趟
latteset-cli --project <目录> compile | jq .compile.errors    # 结构化错误 + 诊断
latteset-cli --project <目录> forward chapters/intro.tex 3    # 源码 → PDF 页码/坐标
latteset-cli --project <目录> inverse 5 --x 100 --y 600       # PDF → 源码

# MCP：stdio server（harness 侧配置成本地 server 即可）
latteset-mcp --project <目录>
```

**退出码**：`0` 成功（`compile` 仅当 `status == "success"`）；`1` 编译未通过；`2` 用法/路径类错误；`3` 内部错误。
JSON 里同时有 `{ok, error:{code,message}}`，两种判断方式都可用。

**配置目录**：默认与 GUI 相同（Windows `%APPDATA%\com.latteset.app`），因此 CLI/MCP 与 GUI **共享同一份设置**；`--config-dir` 或环境变量 `LATTESET_CONFIG_DIR` 可覆盖（测试/CI 隔离用）。

## 2. 工具/命令清单（全部已实现）

| CLI 子命令 | MCP tool | 说明 |
|---|---|---|
| `open` | `project_open` | 打开项目 → 根文件 / 候选 / 生效设置 |
| `compile [--quick]` | `compile` | **编译并等待结果**：`{status, failure, errors, pdf_path, elapsed_ms, engine, kind, upgraded_from_quick}` |
| `status` | `compile_get_status` | 本会话最近一次编译状态 |
| `errors` | `compile_get_errors` | 最近一次编译的结构化错误（含「原因 + 怎么改」诊断） |
| `outline` | `outline_get` | 文档大纲（章/节 → `文件:行号`） |
| `tree [--all]` | `project_tree` | 文件列表（默认只 `.tex`，排除 `tmp/` 与隐藏项） |
| `read <路径>` | `file_read` | 读文件（非 UTF-8 → 明确错误） |
| `write <路径> <内容\|->` | `file_write` | 写文件（父目录必须已存在，与 GUI 同契约） |
| `forward <文件> <行>` | `synctex_forward` | 源码 → PDF 页码/坐标 |
| `inverse <页> [--x --y]` | `synctex_inverse` | PDF → 源码（命中生成内容时就近回落 + `note` 说明） |
| `settings` | `settings_get` | 生效设置（确认 mode/engine/timeout 与假设一致） |

MCP 侧还实现了 `initialize`（回显受支持的 `protocolVersion` + `capabilities.tools` + `instructions`）、`notifications/initialized`、`ping`、`tools/list`；
未知方法回 JSON-RPC `-32601`，**工具内错误按 MCP 约定回 `isError: true` 的 content**（Agent 能读到 `{code,message}`，而不是一个裸协议错误）。

## 3. 实测（2026-09，本机 TeX Live 2026）

- **闭环走通**（`test_file/projects/multifile`）：`open` → `tree`（6 个 .tex）→ `outline`（章/节带 `文件:行号`）→ `read` → `write`（经 stdin 写入含 `$E=mc^2$` 的段落）→ `compile` 成功（`elapsed_ms=912`，`pdf_path=…\main.pdf`）。
- **失败路径可操作**：写入一段缺 `$` 的公式 → `compile` 返回 `status: failed`、**退出码 1**，错误里带 `diagnosis.cause`「在数学模式之外用了上下标（_ 或 ^）…」与 `hint`「把该片段放进 `$...$`…」；改对后再 `compile` → `success`。
- **SyncTeX**：`forward chapters/intro.tex 3` → `page 4 (70.87, 47.48)`；`inverse 5 --x 100 --y 600` → `chapters/intro.tex:24`。
- **确定性**：同一份源码连编两次，`main.pdf` SHA-256 一致（与 roadmap ㉚ 的 `SOURCE_DATE_EPOCH` 一致）。
- **MCP 会话**：手工喂 `initialize` / `notifications/initialized` / `tools/call compile` / `tools/call compile_get_status`，四条响应形状符合协议；仓库内有 `tests/mcp_stdio.rs`（**真实二进制 + stdio 管道**，5 个用例 + 1 个 `#[ignore]` 真编译用例）把这条链路固化成回归。

## 4. 落地时的三处偏差（与 §4 原计划）

| 原计划 | 实际落地 | 为什么 |
|---|---|---|
| P0-1：`CompileOutcome` 补 `elapsed` + actor 写 `Arc<RwLock<CompileSnapshot>>` | **不加 core 字段、不动 actor**：`elapsed_ms` 在 server 调用处用 `Instant` 测；`status`/`errors` 的"最近一次"存在 `Session` 里 | 快照只有**同进程**看得见（CLI 是独立进程，读不到 GUI 的内存），GUI 本来就有事件流。加字段会牵动 core 全部构造点与测试，收益相同 |
| P0-2：`lib.rs` setup 参数化，GUI/CLI 共用装配 | 只共享**基础设施构造方式**（TokioFs / LatexmkRunner / SettingsStorage / SyncTexCli），不共用 `lib.rs` | GUI 要 spawn watch + scheduler + tauri 事件，headless 全都不需要；共享装配反而把 tauri 拖进 headless 依赖 |
| 用 clap 解析 CLI | **手写解析**（≈80 行 + 8 个单测） | 本 crate 保持**零新增依赖**：沙箱/离线环境也能构建（`clap` 需要联网取包，实测被沙箱拒绝） |

P0-3（路径与 SyncTeX 策略下沉 core）按计划做了：`core::synctex::{pdf_path_for_root, synctex_data_path, resolve_inverse}`
（含 ㉒ 的生成产物分类 + 就近回落 + 提示文案），GUI 命令面与 headless 调同一份 → 不会行为漂移。

## 5. 已知限制（形态已定，写清楚而不是埋着）

- **同一项目不要同时用 GUI 和 CLI 编译**：两路 latexmk 会抢同一个 `tmp/`（先到先写、后到覆盖）。约定 **headless 独占**；GUI 那条路无自写盘过滤（`.tex` 目前只有 settings.json 有过滤），想并行需要给 watch 加一次性忽略通道（未做）。
- **没有合并队列 / 超时重试 / 手动终止**：headless 是"一条命令跑一次拿结果"（AI 场景正是在此），超时树杀仍在 runner 内生效。
- **`file_write` 不代建目录**：父目录必须已存在（与 GUI `save_all` 同契约）。Agent 要新建子目录时先建目录（或用 GUI）。
- **不暴露预览渲染**：AI 不看像素——`compile` 返回 `pdf_path`，AI 自己读 PDF。
- **状态订阅（MCP notifications）未做**：一期用"`compile` 同步返回 + `status`/`errors` 快照"覆盖；有真实订阅需求再做 Emitter fanout（原 P1-5）。
- **根文件探测每次重扫**：多数项目 <100ms；大项目再考虑缓存（缓存一致性成本 > 收益）。

## 6. 原计划编号对照（本文档旧版的 P0/P1 条目）

> 旧版按「P0-1…P1-5」编号计划；源码注释与 roadmap 里仍可能引用这些编号（如 `outline.rs` 的 P1-4、⑥ 条目的 P0-1/2）。对照如下：

| 旧编号 | 内容 | 现状 |
|---|---|---|
| P0-1 | `CompileOutcome` 加 `elapsed` + actor 写编译快照 + `status`/`errors` 查询命令 | **改为会话内快照**（`elapsed` 在调用处测），命令面已提供 `status`/`errors`（见 §4） |
| P0-2 | headless `run_once` + `lib.rs` setup 参数化共用装配 | **部分**：`run_once` 已落地；装配不共用（见 §4） |
| P0-3 | `pdf_path_for_root` / SyncTeX 请求构造下沉 core，GUI 改调 | **已做**（`core::synctex::{pdf_path_for_root, synctex_data_path, resolve_inverse}`） |
| P1-4 | outline 解析下沉 Rust（前端改调 `get_outline`） | **早已落地**（更早的提交，见 [modules.md](./modules.md) §3.5） |
| P1-5 | Emitter 三回调改多订阅者 fanout（跑 MCP notifications） | **未做**（一期用快照；见 §5） |

## 7. harness 接线（DSH，已落地并实测）

DSH 的 MCP server 由 profile 的 patch 层声明（**本地文件、不入库**：`~/.dsh/profiles/web/cordis.patch.yml`，`patchReload: live` = 改完即时生效）。实际写入的行：

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

接线要点（都是踩过的）：

- **`command` 用可执行文件绝对路径**：MCP SDK 以 `shell: false` spawn，`.cmd` 壳（npm 风格）在 Windows 起不来；Rust 二进制直接可用，**不需要 node**。
- **`args` 留空 = 不预打开项目**：每次工具调用带 `project=<目录>`（相对路径按 `cwd` 解析），或先调 `project_open`。留空是刻意的——避免"当前项目"隐式粘在某个夹具上；`--project` 若要用也只影响"未显式传参"的调用。
- **`cwd` 决定相对路径基准**：设成项目仓库根，工具调用里就可以写 `test_file/projects/multifile` 这类相对路径。

**实测（DSH 会话内直接调用，不经 shell）**：`project_open`（multifile → `root_file=main.tex`、无候选歧义）→ `compile`（`status=success`、`kind=full`、2789ms、errors 空、`pdf_path` 就位）→ `outline_get`（章/节带 `文件:行号`）→ `synctex_forward chapters/intro.tex:3` → `page 4 (70.87, 47.48)`。协议面另外用「真实二进制 + stdin 管道」复核：`initialize` 回 `protocolVersion=2024-11-05`、`capabilities.tools`，`tools/list` 11 项且**每项都有 `inputSchema`**（SDK 客户端会校验这一点）。
