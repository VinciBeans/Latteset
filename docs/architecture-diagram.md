# Latteset 架构图（Mermaid）

> 本文件是 [architecture.md](./architecture.md) 的配套**图示**：把「分层与依赖 / 编译触发链路 / 调度器语义 / 编辑 ↔ 预览双向定位」四张图以 Mermaid 源码固化为事实来源。
> 图形随代码同步维护：**模块或链路一变，本文件与 architecture.md 一起改**（渲染与校验方式见文末 §5）。

## 1. 分层与依赖总览

依赖方向严格单向：`视图 → 状态 → 服务 → IPC 契约 → src-tauri（接线） → latteset-infra（基础设施） → latteset-core（领域）`。
接口在 core、实现在 infra、注入在 src-tauri：**core 不依赖 infra**（也不依赖 Tauri、不做 IO，ADR-0006）；**src-tauri 不碰 OS API**（文件与进程一律经 core trait，ADR-0010）。
**第二条入口（roadmap ⑥）**：`harness / Agent → latteset-server（CLI / MCP） → latteset-infra → latteset-core`——同样不碰 Tauri、不碰 OS API，与 GUI 并列而非上下级。

<!-- mermaid: 01-layers -->
```mermaid
flowchart TB
  subgraph FE["Webview · Vue 3 + TypeScript（src/）"]
    direction TB
    VIEW["视图层 components<br/>App.vue 应用壳（三栏 + 自研 SplitPane）<br/>EditorPane（Monaco）· PreviewPane（pdf.js）<br/>FileTree · TabBar · OutlinePane · ErrorList · StatusBar · SettingsPanel"]
    STORE["状态层 stores（Pinia ×6）<br/>projectStore · editorStore · compileStore<br/>previewStore · outlineStore · settingsStore"]
    SVC["服务层 services + composables<br/>ipc.ts（唯一 IPC 入口）· events.ts（事件 → store 分发）<br/>useAutoSave（防抖保存）· useSyncTex（双向定位）"]
    WORKERS["Web Worker<br/>Monaco / pdf.js（public/pdfjs）"]
    VIEW --> STORE
    STORE --> SVC
    VIEW --> WORKERS
  end

  CONTRACT["IPC 契约 · tauri-specta 生成 src/bindings.ts<br/>命令面 11 个（invoke）· 事件面 5 个（emit）<br/>DTO：CompileStatus / ErrorEntry / ProjectInfo / Settings …"]

  subgraph SHELL["src-tauri · 接线与契约薄壳（Rust，不碰 OS API）"]
    direction TB
    CMDS["commands.rs<br/>invoke 处理器 · 路径策略调用 · 错误契约 { code, message }"]
    EVENTS["events.rs<br/>事件契约（specta）· TauriSink · Emitter 适配"]
    STATE["lib.rs 装配<br/>AppState：构造实现并注入 core trait 位"]
    CMDS --- STATE
  end

  AGENT["harness / Agent（DeepSeek Harness 等）<br/>不经 GUI，直接驱动「读 → 改 → 编译验证 → 修」"]

  subgraph SERVER["crates/latteset-server · 无 GUI 交互层（headless，无 Tauri）"]
    direction TB
    BINS["bin/latteset-cli · bin/latteset-mcp<br/>一条命令一个 JSON（stdout 只有 JSON）／MCP over stdio"]
    SESS["lib.rs · Session<br/>打开项目 · 编译并同步返回结果 · 大纲 · 文件读写 · SyncTeX · 设置"]
    MCPM["mcp.rs<br/>最小 JSON-RPC：initialize / tools/list / tools/call<br/>11 个 tool，错误回 isError"]
    BINS --> SESS
    BINS --> MCPM
    MCPM --> SESS
  end

  subgraph INFRA["crates/latteset-infra · 基础设施（外部依赖与文件系统唯一落点，无 Tauri）"]
    direction TB
    IFS["fs<br/>TokioFs：FileSystem 实现<br/>canonicalize / is_dir / write · verbatim 前缀剥离"]
    IRUN["runner<br/>LatexmkRunner：CompileRunner 实现<br/>超时树杀 · PDF 原子拷贝 · .log 容错解码"]
    ISYN["synctex<br/>SyncTexCli：SyncTexProvider 实现"]
    ISTO["storage<br/>SettingsStorage<br/>原子写 · 自写盘 hash 过滤"]
    IWATCH["watch<br/>notify 8.2 · 事件分类<br/>结果经 WatchSink 回调"]
  end

  subgraph CORE["crates/latteset-core · 纯领域核心（无 Tauri、无 IO、可单测）"]
    direction TB
    COMPOSE["compose<br/>文件事件 → CompileRequest 快照"]
    SCHED["scheduler<br/>actor + 合并队列 + 失败策略<br/>只有事件输入与指令输出，不 spawn 进程"]
    PROJECT["project<br/>项目状态 · 根文件探测 · 忽略规则<br/>paths：D8 路径策略（canonicalize + 根内校验）"]
    LOGP["log_parser<br/>.log → ErrorEntry（insta 快照）"]
    SYNM["synctex<br/>Provider 接口 + 输出解析"]
    SETTINGS["settings · outline · types<br/>合并校验 · 结构树 · 跨边界 DTO"]
  end

  subgraph EXT["外部依赖与文件系统（内容真相源）"]
    direction TB
    TEXLIVE["latexmk + xelatex / pdflatex / lualatex"]
    SXCLI["synctex CLI（TeX Live 自带）"]
    PROJ["项目目录<br/>*.tex 源文件 · tmp/ 编译中间产物<br/>.latteset/settings.json 项目设置"]
    PDFOUT["项目根 &lt;stem&gt;.pdf（编译产物）"]
    GSET["全局 settings.json（app_config_dir）"]
  end

  SVC -->|"invoke 命令（结果 ok / CmdError）"| CONTRACT
  CONTRACT -.->|"emit 事件（单向：事件 → store 动作）"| SVC
  CONTRACT --> CMDS
  AGENT --> BINS
  SESS -->|"领域策略：路径（D8）/ 根文件探测 / 大纲"| PROJECT
  SESS --> SETTINGS
  SESS -->|"经 core trait 调用实现"| IFS
  SESS --> IRUN
  SESS --> ISYN
  SESS --> ISTO
  EVENTS -.->|"实现 WatchSink（监视结果 → tauri 事件）"| IWATCH
  CMDS -->|"领域策略：路径（D8）/ 设置 / 大纲"| PROJECT
  CMDS --> SETTINGS
  CMDS -->|"经 core trait 调用实现"| IFS
  CMDS --> ISTO
  CMDS --> IWATCH
  CMDS --> ISYN
  IWATCH -->|"变化的文件路径"| COMPOSE
  COMPOSE --> SCHED
  IFS -.->|"实现 FileSystem"| PROJECT
  IRUN -.->|"实现 CompileRunner（core 反向调用）"| SCHED
  ISYN -.->|"实现 SyncTexProvider"| SYNM
  ISTO -.->|"settings 合并 / 校验"| SETTINGS
  IRUN -->|"失败时解析 .log"| LOGP
  IRUN --> TEXLIVE
  ISYN --> SXCLI
  IFS --> PROJ
  IFS --> GSET
  ISTO --> GSET
  IRUN --> PDFOUT
  TEXLIVE --> PROJ

  %% 仅排版用：不可见连线，把「外部依赖」层固定在「领域核心」之下
  SETTINGS ~~~ TEXLIVE
  PROJECT ~~~ PROJ
  SYNM ~~~ SXCLI
  SCHED ~~~ GSET
  LOGP ~~~ PDFOUT

  classDef fe fill:#e9e8ff,stroke:#4a4cd8,color:#241f3a
  classDef ipc fill:#fff3d6,stroke:#c98a12,color:#3a2c05
  classDef shell fill:#e4f6ec,stroke:#2f8f6b,color:#123024
  classDef headless fill:#dff0fb,stroke:#2b7fb8,color:#0d2536
  classDef infra fill:#ffe6e0,stroke:#c0563a,color:#3a1a12
  classDef core fill:#f3e8ff,stroke:#7c3aed,color:#2b1a45
  classDef ext fill:#f0f0f4,stroke:#8a8aa0,color:#2b2438
  class VIEW,STORE,SVC,WORKERS fe
  class CONTRACT ipc
  class CMDS,EVENTS,STATE shell
  class BINS,SESS,MCPM headless
  class AGENT ipc
  class IFS,IRUN,ISYN,ISTO,IWATCH infra
  class COMPOSE,SCHED,PROJECT,LOGP,SYNM,SETTINGS core
  class TEXLIVE,SXCLI,PROJ,PDFOUT,GSET ext
```

![Latteset 分层与依赖总览](./diagrams/01-layers.png)

**读图要点**

- **只有服务层碰 IPC**：视图与 store 不直接 invoke；命令与事件类型全部由 Rust 侧 specta 生成（`src/bindings.ts`，调试构建启动时刷新）。事件面与命令面共用一个边界：箭头向下为 invoke，虚线向上为 emit。
- **四个 Rust crate，两个"唯一落点"**：core 是唯一的**策略与接口**落点（队列语义、探测规则、D8 路径策略、trait 定义）；latteset-infra 是唯一的**外部依赖与文件系统**落点（tokio::fs、tokio::process、notify、设置落盘）。src-tauri 只做接线与契约（DTO、事件形态、装配注入）；latteset-server（⑥）是**与 src-tauri 并列的第二入口**——headless，不装配 scheduler/watch，不经 IPC 契约，直接调 core 策略 + infra 实现。
- **两条入口不互相认识**：GUI 走「前端 → IPC → src-tauri → infra → core」，headless 走「harness/Agent → server → infra → core」。二者**共用同一份**路径策略与 SyncTeX 实现（`core::synctex` / `core::project::paths`），但**不并发编译同一个项目**（会抢 `tmp/`，约定 headless 独占）。
- **core 零 IO**：进程、文件、监视、CLI 全在 infra；core 经 trait（`FileSystem` / `CompileRunner` / `SyncTexProvider`）被注入，单测用 fake 实现。虚线 `实现 Xxx` 表示「core 声明接口、infra 提供实现、运行期由 core 反向调用」。
- **infra 不认识 Tauri**：监视结果经 `WatchSink` 回调，事件形态（compile-status / files-changed …）在 src-tauri 的 events.rs 定型。
- 图中 `~~~` 是不可见连线，只用于固定分层顺序，不表示依赖。

## 2. 编译触发与调度链路

<!-- mermaid: 02-compile-chain -->
```mermaid
sequenceDiagram
  autonumber
  actor U as 用户
  participant EP as EditorPane（Monaco）
  participant AS as useAutoSave
  participant IPC as 服务层 ipc.ts
  participant CMD as commands.rs
  participant FS as 项目文件系统
  participant W as watch.rs（notify 线程）
  participant CO as compose.rs
  participant SC as scheduler（actor）
  participant RN as runner.rs
  participant LM as latexmk
  participant LP as log_parser
  participant EV as events.rs
  participant ST as stores（Pinia）
  participant PV as PreviewPane（pdf.js）

  alt 连续模式（默认）
    U->>EP: 输入字符
    EP->>AS: onChange 触发 schedule()
    AS->>AS: 防抖 debounce_ms（默认 500ms）
  else 保存触发模式
    U->>AS: Ctrl+S / 点「编译」/ 关标签 → flush()
  else 手动编译
    U->>IPC: compile_now（不校验是否有变化）
  end

  AS->>IPC: saveAll 全部脏文件
  IPC->>CMD: invoke save_all
  CMD->>FS: 写盘 save_content
  FS-->>W: notify 事件（内容变化）
  W->>W: 过滤：Access 事件跳过 / 忽略 tmp、隐藏项、无扩展名；其余（含 .bib/图片）触发编译
  W->>CO: 变化路径 + 项目与设置快照
  CO->>SC: CompileRequest（root_file / engine / timeout）
  SC->>SC: 合并队列：待编译最多 1 条，新请求覆盖旧请求
  SC->>RN: CompileRunner compile(req, CancellationToken)
  RN->>LM: spawn latexmk -xelatex -outdir=tmp -synctex=1（cwd = 项目根）
  Note over RN,LM: timeout_secs 超时 → taskkill /T /F 树杀；abort_compile 走同一终止路径
  LM-->>RN: 退出码 / 超时 / 取消
  RN->>LP: 失败时读 tmp 下的编译日志（容错解码）
  LP-->>RN: ErrorEntry 列表（同源去重、错误雪崩截断）
  RN-->>SC: CompileOutcome（Success / ContentError / Timeout / Aborted / IoError）
  SC->>EV: 状态 · 错误 · PDF 路径（Emitter 三通道）
  EV-->>ST: compile-status · errors-updated · pdf-updated
  ST-->>PV: pdf-updated → 重载 PDF + 恢复页码与滚动位置
  W-->>ST: files-changed（旁路：文件树刷新 / 外部修改判定）
```

![编译触发与调度链路](./diagrams/02-compile-chain.png)

**读图要点**

- 防抖只在**前端保存时机**层（500ms 是产品语义）；后端不做时间防抖——合并队列（最多一条等待）本身吸收事件风暴（ADR-0001）。
- latexmk 的「增量」是优化跑几遍（引用 / bib 多次 pass），**不跳过未改动子文件**；每次编辑仍对整份文档重跑一遍引擎（ADR-0005）。
- 手动终止语义 = 停当前 + 清队列；终止后即使 runner 返回其他结果，对外仍呈现为 Aborted。
- 外部工具改文件同样进这条链路（notify 不依赖前端）。

## 3. 调度器状态与失败语义

<!-- mermaid: 03-scheduler-semantics -->
```mermaid
stateDiagram-v2
  direction LR
  state "空闲（无请求）" as IDLE
  state "合并槽（等待，最多 1 条）" as QUEUED
  state "编译中（latexmk 子进程）" as RUNNING
  state "重试（同一请求一次）" as RETRY
  state "成功（PDF 就绪）" as OK
  state "失败：内容错误" as FAILC
  state "失败：超时" as FAILT
  state "已终止（手动 abort）" as ABORT

  [*] --> IDLE
  IDLE --> QUEUED: 新 CompileRequest
  QUEUED --> QUEUED: 新请求覆盖旧请求
  QUEUED --> RUNNING: 取出最新请求
  RUNNING --> OK: 退出码 0，PDF 原子拷贝到项目根
  RUNNING --> FAILC: 退出码非 0，解析 .log
  RUNNING --> FAILT: 日志不可读，IoError 视同内容错误
  RUNNING --> ABORT: abort_compile / CancellationToken
  RUNNING --> RETRY: 超过 timeout_secs，树杀后无等待条目且 attempt = 0
  RETRY --> RUNNING: 重试同一请求
  RUNNING --> FAILT: 超时且已有一次重试（attempt = 1）
  OK --> QUEUED: 队列有等待条目
  FAILC --> QUEUED: 队列有等待条目
  ABORT --> QUEUED: 队列有等待条目
  FAILT --> QUEUED: 队列有等待条目
  OK --> IDLE: 无等待
  FAILC --> IDLE: 无等待
  ABORT --> IDLE: 无等待
  FAILT --> IDLE: 无等待
```

![调度器状态与失败语义](./diagrams/03-scheduler-semantics.png)

**读图要点**：决策是纯函数（`scheduler/policy.rs`，输入只有 `attempt` + `outcome` + 是否有等待条目）；状态收容在 actor task 内，外部只有 `SchedulerHandle`，因此无共享可变状态、无锁。

## 4. 编辑 ↔ 预览双向定位（SyncTeX）

<!-- mermaid: 04-synctex -->
```mermaid
flowchart LR
  subgraph FWD["正向：源码 → PDF"]
    direction LR
    A1["EditorPane<br/>Ctrl+点击某行"] --> A2["useSyncTex.forward"]
    A2 --> A3["ipc.synctexForward"]
    A3 --> A4["commands.rs<br/>项目内路径校验"]
    A4 --> A5["infra synctex.rs<br/>synctex view"]
    A5 --> A6["tmp/&lt;stem&gt;.synctex.gz"]
    A6 --> A7["{ page, x, y }"]
    A7 --> A8["previewStore.setHighlight"]
    A8 --> A9["PreviewPane<br/>高亮叠加层"]
  end

  subgraph BWD["反向：PDF → 源码"]
    direction LR
    B1["PreviewPane<br/>点击 PDF"] --> B2["useSyncTex.inverse"]
    B2 --> B3["ipc.synctexInverse"]
    B3 --> B4["commands.rs<br/>项目内路径校验"]
    B4 --> B5["infra synctex.rs<br/>synctex edit"]
    B5 --> B6["{ file, line, column }"]
    B6 --> B7["editorStore.openFile<br/>打开并定位到行"]
  end

  subgraph REFRESH["PDF 刷新（编译成功）"]
    direction LR
    C1["pdf-updated 事件"] --> C2["previewStore<br/>记录页码 + 滚动位置"]
    C2 --> C3["pdf.js 重新加载文档"]
    C3 --> C4["恢复位置（重载 + 恢复）"]
  end

  classDef fe fill:#e9e8ff,stroke:#4a4cd8,color:#241f3a
  classDef rust fill:#e4f6ec,stroke:#2f8f6b,color:#123024
  class A1,A2,A3,A7,A8,A9,B1,B2,B3,B6,B7,C1,C2,C3,C4 fe
  class A4,A5,A6,B4,B5 rust
```

![SyncTeX 双向定位与 PDF 刷新](./diagrams/04-synctex.png)

**读图要点**：`SyncTexProvider` 是 core 中的接口，进程调用在 latteset-infra 的 `synctex` 模块（ADR-0008/0010）；CLI 指向 `tmp/<根名>.synctex.gz`。pdf.js 无增量渲染 API，故刷新用「重载 + 恢复位置」而非局部更新。

## 5. 渲染与校验

- 图源即上文 Mermaid 代码块，可直接粘贴到 GitHub、VS Code（Markdown Preview Mermaid 插件）或 [Mermaid Live Editor](https://mermaid.live)。
- 本仓库的渲染产物在 [diagrams/](./diagrams/)（每图一份 PNG + SVG，正文内嵌 PNG，SVG 可缩放查看），文件名由图源代码块前的 `<!-- mermaid: 名称 -->` 决定。
- 校验门槛：**mermaid 12.0.0** 离线渲染（`mermaid.parse` + 无头 Edge 出 SVG），四图全部通过、无语法错误。
- 重新生成（需要 node ≥ 18 与 Edge / Chrome）：

```bash
npm i mermaid@12                                  # 本仓库未把 mermaid 列为依赖，按需临时安装
node scripts/render-diagrams.mjs docs/architecture-diagram.md docs/diagrams --png
```
