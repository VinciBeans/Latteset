# CLI + MCP 交互接口 · 计划任务（仅记录）

> 状态：**计划任务，仅记录，待实现**（2026-08 记录）。本文档是后续实现的唯一事实来源，对应 [design.md](./design.md) 后置/未决清单里的「CLI + MCP 交互接口」条目。
> 目标：让 harness/Agent（如 DeepSeek Harness）**不经 GUI 直接调用** TexPresso 的编译/预览/SyncTeX 能力，形成「读 → 改 → 编译验证 → 修」闭环。
> 标注约定：**已核实**=读过代码/文档确认；**分析结论**=设计推演，未实测；未标注规划项一律视为未实现。

## 1. 现状盘点（已核实，2026-08）

### 1.1 命令面（`src-tauri/src/commands.rs`，tauri command，DTO 进出无业务逻辑）

| 命令 | 签名/返回 | 说明 |
|---|---|---|
| `open_project` | `folder: String` → `ProjectInfo{root, root_file}` | 打开项目 + 根文件探测/覆盖校验 + 设置合并；已返回根文件，CLI 直接可用 |
| `list_dir` | `path` → `Vec<DirEntryInfo>` | 递归文件树（排除 tmp/ 与隐藏项） |
| `read_file` | `path` → `String` | 路径校验在项目根内（D8） |
| `save_all` | `Vec<FileContent{path, content}>` → `()` | 批量写盘 |
| `compile_now` | `()` → `()` | **fire-and-forget**：结果只走事件，无返回 |
| `abort_compile` | `()` → `()` | 终止运行中编译 + 清空队列 |
| `synctex_forward` | `file, line, column` → `SyncTexTarget{page,x,y}` | 源码 → PDF 定位 |
| `synctex_inverse` | `page, x, y` → `SourcePositionDto{file,line,column}` | PDF → 源码定位 |
| `get_settings` | `()` → `Settings` | 生效设置（含 mode/debounce/timeout/engine） |
| `update_settings` | `SettingsPatch` → `Settings` | 局部更新（root_file 走项目覆盖，其余走全局） |

### 1.2 事件面（`src-tauri/src/events.rs`，Emitter 三回调 → tauri 事件）

| 事件 | 载荷 |
|---|---|
| `compile-status` | `CompileStatusDto{phase: queued\|running\|success\|failed, kind}` |
| `errors-updated` | `Vec<ErrorEntry{message, file, line, kind}>`（结构化错误，AI 可直接消费） |
| `pdf-updated` | `PdfUpdated{path}` |
| `files-changed` | `FilesChanged{paths, structural}`（structural=增删改名，内容修改为 false） |
| `settings-changed` | `Settings` |

### 1.3 可复用资产（核心资产已与 Tauri 解耦）

- `texpresso-core`：scheduler（合并队列/失败语义/abort）、log_parser（`.log` → `ErrorEntry`）、project（文件收集/根文件探测/树排除）、settings（全局+覆盖合并）、synctex provider（CLI 封装）、**outline（2026-09-03：文档结构树解析 + `load` 编排，见 `crates/texpresso-core/src/outline.rs`；GUI 走 `get_outline` 命令且已改调）**。
- `src-tauri/runner.rs`：`LatexmkRunner`（独立 `CompileRunner` trait 实现：tokio 进程、超时树杀、PDF 拷贝），headless 可直接实例化复用。
- `src-tauri/storage.rs`：`SettingsStorage` 按路径构造（全局设置目录路径由调用方传入），headless 传相同路径即可；`is_self_write` 过滤当前**仅用于 settings.json**（watch.rs），`.tex` 无自写盘过滤。

## 2. 工具清单（建议暴露，按 AI 价值排序）

### 第一梯队：AI「改 → 编 → 验」闭环核心（必须做）

| CLI 子命令 | MCP tool | 说明 | 复用现状 |
|---|---|---|---|
| `compile --wait` | `compile` | 触发编译并**返回结果**（status/errors/pdf 路径/耗时） | 需包装（现状 fire-and-forget，见 §4 P0-1/2） |
| `errors` | `compile_get_errors` | 结构化错误 `{file, line, message, kind}` | log_parser + errors-updated 已有 |
| `status` | `compile_get_status` | 当前编译状态 | CompileStatusDto 已有 |
| `abort` | `compile_abort` | 终止编译 | abort_compile 已有 |
| `open-project` | `project_open` | 打开项目 + 根文件探测 + 生效设置 | open_project 已有 |

### 第二梯队：AI 上下文感知（高价值）

| CLI 子命令 | MCP tool | 说明 | 复用现状 |
|---|---|---|---|
| `outline` | `outline_get` | 文档结构树（章→节→行），AI 理解文档语义结构 | **已下沉 Rust**（2026-09-03：`texpresso-core::outline` + `get_outline` 命令，前端已改调——见 §4 P1-4；CLI 命令本体待实现） |
| `list-files` | `project_tree` | 项目 `.tex` 文件树 | list_dir 已有 |
| `read` / `write` | `file_read` / `file_write` | 读写文件 | read_file / save_all 已有 |
| `synctex-forward` | `synctex_forward` | 源码位置 → PDF 页码 | 已有 |
| `synctex-inverse` | `synctex_inverse` | PDF 页码 → 源码位置 | 已有 |

### 第三梯队：状态订阅（MCP 专属价值；CLI 侧用 `--wait`/快照代替）

- `pdf_updated` / `compile_status` / `errors` 通知（MCP notifications）。
- `files_changed` 通知（含 structural 载荷）。
- 一期可不做（用 §4 P0-1 快照轮询覆盖），有真实订阅需求再做 fanout（§4 P1-5）。

### 暂不暴露 / 不做

- **预览渲染**（pdf.js canvas）：AI 不看像素，暴露 PDF 文件路径即可（AI 自己有 reader）。
- `settings` 修改：低价值；保留 `settings_get` 让 AI 确认 mode/engine/debounce 即可。
- 项目级锁 / watch `IgnorePaths`：**形态定案前不动**（见 §4 注意事项）。

## 3. 关键设计建议（分析结论）

1. **命令式一次性语义**：AI 场景要 `compile --wait`（跑一次、拿结果），不需要合并队列/防抖/watch 那套 GUI 体验逻辑；直接复用 `LatexmkRunner` 跑一次完整编译（超时树杀仍在 runner 内），不走 scheduler 状态机——简单、结果确定。注：因此无「超时重试」语义（GUI scheduler 有），AI 场景可接受。
2. **形态**：workspace 新增 `crates/texpresso-server`：服务层（无 tauri 依赖）+ `cli` 二进制 + `mcp` 二进制；MCP 用 **stdio 传输优先**（harness 本地拉起最简单）。headless setup 复用 fs/runner/synctex/storage 的构造（src-tauri `lib.rs` setup 参数化，不 spawn watch）。
3. **outline 下沉**：~~把 `src/stores/outline.ts` 的 `parseFile/buildTree` 移植到 Rust（服务层）~~（**已完成** 2026-09-03：`texpresso-core::outline`，GUI 前端已改调 `get_outline` 命令，无双实现；`src/texParse.ts` 仍供折叠/补全使用）。

## 4. 优化点：小改动换 CLI/MCP 强化，GUI 零/极小影响（分析结论）

> 共同前提：现有 GUI 路径（事件流 / watch / scheduler actor / 前端命令调用）**一行不改**即可获得下述收益，全部为「纯新增或可选参数」形态。

### P0（建议先做）

| # | 改动点 | GUI 影响 | CLI/MCP 收益 |
|---|---|---|---|
| 1 | `CompileOutcome` 补 `elapsed: Duration`（现无耗时字段）；`actor.rs on_finished` 顺手写 `Arc<RwLock<Option<CompileSnapshot>>>`（status/errors/pdf 信息在回调处已齐）；新增 `status`/`errors` 查询命令 | 零（GUI 走事件流不读快照；前端 DTO 忽略新字段） | `compile --wait` 一次返回 `{status, errors, pdf_path, elapsed}`；status/errors 变同步查询；AI 可感知编译耗时（与延迟预算挂钩） |
| 2 | headless `run_once`：直接实例化 `LatexmkRunner` 跑一次；`lib.rs` setup 参数化（headless 不 spawn watch/不初始化 tauri 依赖），传相同全局设置路径 | 零（GUI 继续走 scheduler 路径） | 单命令单次完整编译+结果；进程启动快、无 watcher 开销 |
| 3 | `pdf_path_for_root`（`commands.rs` 私有）与 SyncTeX 请求构造下沉 core，GUI 命令改调 | 零（行为等效重构） | CLI 的 PDF 路径/SyncTeX 逻辑与 GUI 永远一致，无双实现漂移 |

### P1（按需跟进）

| # | 改动点 | GUI 影响 | CLI/MCP 收益 |
|---|---|---|---|
| 4 | outline 移植 Rust（core）+ 单测，前端改调 | **已完成**（2026-09-03）：core `outline` 模块 + `get_outline` 命令，前端 stores/outline.ts 改调；OutlinePane/events/App 零改动；core 单测 109 项含对拍用例 | `outline` 直接复用（CLI 命令本体仍待实现） |
| 5 | Emitter 三回调改多订阅者 fanout（tauri 注册为第一路） | 零（第一路照发） | MCP notifications（编译/PDF/错误推送）；**建议后置**，一期用快照轮询 |

### 注意事项（形态定案前不动代码）

- **CLI/GUI 并发同项目**：CLI 独立进程写 `.tex` 后显式编译，若 GUI 同时开着（watch 活跃），两路 latexmk 抢同一 `tmp/` 可能互相覆盖/竞争。方案二选一：约定 headless 独占；或 watch 加 `IgnorePaths` 一次性忽略通道（`.tex` 目前无自写盘过滤，仅 settings.json 有）。取决于 MCP server 是否内嵌 GUI 进程，**形态定了再实现**。
- **根文件探测每次重扫**：CLI 每命令 `collect_tex_files` + 正则探测，多数项目 <100ms 可接受；真要大项目再考虑 `.texpresso/state.json` 缓存（缓存一致性成本大于收益，暂不做）。
- **run_once 与 scheduler 语义差异**：无合并队列/超时重试/手动终止（AI 场景 Accept；超时树杀在 runner 内仍生效）。

### 落地顺序

1. P0-1（elapsed + 快照）→ 2. P0-2（run_once + headless setup）→ 3. P0-3（路径逻辑下沉 core）——三步做完，`open`/`compile --wait`/`status`/`errors`/`synctex` 即低成本成立；
4. P1-4（outline）与 P1-5（fanout）按实际需求跟进。
