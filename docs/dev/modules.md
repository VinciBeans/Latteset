# Latteset 模块契约（函数级）

> **这是什么、给谁看**：开发/维护者文档——把每个大模块拆到**小模块 → 函数**，给出签名、算法、信息局部性与大模块之间的通信契约。改实现之前先读这里。
> **配套**：产品语义（延迟预算、失败语义、MVP 边界）见 [design.md](../design.md)，分层/模块总览与 IPC 契约见 [design.md](../design.md) §4，内嵌 Tectonic（唯一形态）的构建/装配契约见 [tectonic-library.md](./tectonic-library.md)（本文件 §2.7 是它的 runner 侧契约），测试与验证命令见 [testing.md](./testing.md)，未决与后置事项见 [roadmap.md](../roadmap.md)，历史决策与被否决的备选见 [adr/](../adr/)。
> 文中 roadmap 编号（㉜/㉞/㊻/⑦a/#19/#26 等）的含义一律见 [roadmap.md](../roadmap.md)。

## 0. 设计原则与信息局部性总则

**两条硬原则**（本文件所有设计的判据）：

1. **最小外部依赖**：函数只接收它真正需要的参数；模块只依赖它真正需要的模块；一切外部信息经显式参数或接口传入，禁止隐式获取。
2. **最小全局状态**：core 内**零全局可变状态**；进程内唯一的"全局"是 Tauri managed state 里的**不可变句柄**（sender、trait 实现），所有可变状态收容在单一写者（scheduler task / settings 存储）内。

**信息局部性三级规则**：

| 层级 | 允许存放的信息 | 禁止 |
|---|---|---|
| 函数内 | 解析中间态、正则匹配结果、计时器、临时缓冲 | 任何跨调用存活的信息 |
| 小模块内 | 队列条目、运行句柄、监视过滤器、模块自己的缓存 | 项目状态、设置全量 |
| 大模块内 | 项目状态、设置快照、调度状态 | — |
| 大模块间 | 只经 DTO / 命令 / 事件（不可变值传递） | 共享可变引用、全局单例 |

**跨大模块的通信只允许两种**：同步命令调用（传 DTO、返回 DTO）与单向事件（不可变载荷）。模块间不得共享锁、不得互相持有内部对象。

## 1. 全局状态清单（进程内仅此一份）

| 状态 | 类型 | 写者 | 位置 |
|---|---|---|---|
| 调度命令通道 | `mpsc::UnboundedSender<SchedulerCommand>` | 组合层（watch/commands） | Tauri state |
| 设置存储 | `Arc<RwLock<Settings>>` | settings 存储模块 | Tauri state |
| 项目状态 | `Arc<RwLock<ProjectState>>` | open_project / 探测 | Tauri state |
| FS / Runner / SyncTex 实现 | `Arc<dyn Trait>`（不可变） | 启动时构造一次 | Tauri state |

core 内的调度器、探测、解析、合并全部是**无全局态**的纯逻辑或 task 内状态。

> 上表是 **GUI 进程**的清单。headless 进程（§8.1）没有 Tauri state、没有调度器：唯一的状态是一个 `Session`（项目 + 最近一次编译结果），由 CLI/MCP 二进制自己持有。

## 2. 编译子系统（scheduler 大模块）

### 2.1 拆分

```
scheduler（core）
├── queue.rs        —— 合并队列（纯逻辑，无 IO）
├── policy.rs       —— 失败语义决策表（纯函数）
├── runner.rs       —— CompileRunner trait + CompileRequest/CompileOutcome（core 定义）
└── scheduler.rs    —— actor 主循环（唯一写者，状态全在 task 内）

latteset-infra
└── synctex.rs / storage.rs / watch.rs / fs.rs / proc.rs / snippet.rs —— 无编译 runner（子进程 runner 已删除）

latteset-tectonic（唯一引擎形态，非可选依赖）
├── runner.rs       —— TectonicLibRunner：CompileRunner 实现（内嵌引擎，见 §2.7）
├── status.rs       —— 引擎状态后端 → 中间态（页进度 / 错误 / notice）
└── io.rs / bundle.rs / synctex.rs —— I/O 层、bundle 检测、SyncTeX 主输入补丁

src-tauri
└── runner.rs       —— EmbeddedRunner：薄壳，每趟读设置重建 TectonicLibRunner（§2.6）
```

### 2.2 queue.rs — 合并队列

```rust
/// 待编译条目：最多一个，总是最新（覆盖语义）。
pub struct Queue { pending: Option<CompileRequest> }

impl Queue {
    pub fn new() -> Self;
    /// 合并语义：新请求覆盖旧请求（ADR-0001）。
    pub fn push(&mut self, req: CompileRequest) { self.pending = Some(req); }
    pub fn take(&mut self) -> Option<CompileRequest>;
    pub fn clear(&mut self);
    pub fn is_empty(&self) -> bool;
}
```

**算法**：无。唯一规则是 `push` 覆盖——"最多一个、总是最新"由类型本身保证，无法构造出多条目状态。

**信息局部性**：`pending` 只存在于 Queue 内；Queue 只存在于 scheduler task 内。函数参数只有请求本身，不接收任何上下文。

### 2.3 policy.rs — 失败语义决策表（纯函数）

```rust
pub enum Decide {
    StartPending,            // 执行队列中的最新请求（跳过错过的旧版本）
    FinishOk,                // 成功且无等待：无事可做
    Fail(FailureKind),       // 展示失败
}

/// 输入只有：编译结果 + 是否有等待条目。不读任何外部信息（无跨调用状态）。
pub fn decide(outcome: &CompileOutcome, has_pending: bool) -> Decide;
```

**算法**（来自 design.md 失败语义表，逐一对应；见 `scheduler/policy.rs`）：

| outcome | has_pending | 决策 |
|---|---|---|
| Success | — | 有 pending → `StartPending`；无 → `FinishOk` |
| Timeout | true | `StartPending`（新内容优先，跳过报错） |
| Timeout | false | `Fail(Timeout)`（**不重试**，roadmap ㉕） |
| ContentError | true | `StartPending`（不重试） |
| ContentError | false | `Fail(ContentError)` |
| Aborted | true | `StartPending`（abort 后的新请求是新意图） |
| Aborted | false | `Fail(Aborted)` |
| IoError | — | 视同 ContentError（不重试） |

**信息局部性**：`decide` 只读 `outcome` 与 `has_pending`，不碰队列、不碰设置、不碰时间——因此**没有跨调用状态**，同一请求不会跑第二遍（超时即失败，见 §2.3 表 Timeout 行）。

### 2.4 runner.rs — CompileRunner 接口（core）

```rust
pub struct CompileRequest {
    pub root_file: PathBuf,        // 根文件绝对路径
    pub project_root: PathBuf,     // 工作目录
    pub timeout: Duration,         // 请求构造时从设置快照拷贝（之后设置变化不影响运行中任务）
    pub kind: CompileKind,         // 强度（roadmap ㉘）：编辑触发 = Quick；首编/手动/空闲收敛 = Full
}

pub enum CompileOutcome {
    Success { pdf_path: PathBuf, kind: CompileKind, page_hashes: Vec<u64> },  // kind = **实际执行**的强度（Quick 可能被升级为 Full）
    Timeout { entry: ErrorEntry }, // 超时强制终止（runner 已树杀），条目是现场采集的证据
    ContentError { errors: Vec<ErrorEntry> },   // 进程非零退出，.log 已解析
    Aborted,                       // 收到取消信号（runner 已树杀）
    IoError { message: String },   // 拷贝/读日志等 IO 失败
}

#[async_trait]
pub trait CompileRunner: Send + Sync {
    /// cancel 是取消令牌：scheduler 调用 cancel 后 runner 必须尽快树杀并返回 Aborted。
    async fn compile(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome;
}
```

**设计决策 D2（超时归属 runner）**：超时检测、进程树杀、PDF 拷贝全部在 runner 内，scheduler 无时钟、无进程概念。备选"调度器注入时钟管超时"被否决：scheduler 被迫依赖 tokio 时钟，单测要注入时间源，复杂度不成比例。收益：scheduler 单测只需喂 `CompileOutcome` 假结果，超时路径由 runner 的集成测试覆盖。

**编译进行中的反馈**（阶段 2 · 流式输出）：

```rust
pub trait CompileProgress: Send + Sync {
    fn pages(&self, _pages: u32) {}             // 只在**数值变大**时调用
    fn errors(&self, _errors: &[ErrorEntry]) {} // 只报致命错误；节流 + 指纹去重
    fn notice(&self, _msg: &str) {}             // 能力边界提示（ADR-0015 决策 4）：biber 不自带 / makeindex、glossaries 不自动跑
}

pub struct NoProgress;                          // 全空实现（headless / 测试）
```

内嵌 runner 的 `progress` 是可选注入依赖，**不进 `Emitter`**：`Emitter` 只在**任务完成**那一刻被调用（队列合并/失败语义都在那时成立），而中间态属于"运行中的引擎反馈"，与调度语义正交。

契约两条，改坏即出错：

- **非权威**：终态一律以 `CompileOutcome` 为准，UI 只在运行中采纳中间态；
- **中间态可能晚于终态抵达**（收尾补发 / 事件调度顺序）：故中间态**必须走独立事件**（`compile-progress` / `compile-errors` / `compile-preview`），前端配 `phase === "running"` 守卫——复用终态事件名会让晚到的中间态顶掉权威列表（真机实测：超时诊断 1 条被 30 条中间态覆盖后才发现的）。

### 2.5 scheduler.rs — actor 主循环（core）

```rust
pub enum SchedulerCommand {
    Compile(CompileRequest),   // 组合层已把文件事件翻译成请求（D3）
    Abort,                     // 手动终止：取消运行中 + 清空队列
    JobFinished(CompileOutcome), // 内部：运行任务完成（actor 私有，外部不发送）
}

struct RunningJob {            // actor 私有
    request: CompileRequest,
    handle: JoinHandle<CompileOutcome>,
    aborted: bool,             // 已收到 Abort：收尾时无论 outcome 为何，一律按 Aborted 呈现
}

pub struct Scheduler {
    rx: mpsc::UnboundedReceiver<SchedulerCommand>,
    runner: Arc<dyn CompileRunner>,
    emitter: Emitter,          // 注入的事件发射（status / errors / pdf 三通道）
    queue: Queue,
    running: Option<RunningJob>,
    cancel: Option<CancellationToken>,
}

impl Scheduler {
    /// 返回（外部句柄, 调度器本体）：由接线方 spawn `scheduler.run()`。
    pub fn create(runner: Arc<dyn CompileRunner>, emitter: Emitter) -> (SchedulerHandle, Scheduler);
    pub async fn run(self);
    async fn handle(&mut self, cmd: SchedulerCommand);
    async fn on_finished(&mut self, outcome: CompileOutcome);
}
```

**主循环算法**：

```
handle(Compile(req)):
  Idle        → start(req)                    // 发 Running 事件
  Running     → queue.push(req); emit(Queued) // 合并，最多一个等待
handle(Abort):
  running.aborted = true; 取消当前任务 → 清队列   // 终止语义：停 + 清队
on_finished(outcome):
  if running.aborted → outcome := Aborted     // runner 忽略 cancel 也不误报成功/错误/PDF
  先广播错误：ContentError{errors} / Timeout{entry} / IoError → emit(errors)
  Success{pdf_path, ..} → emit(pdf)
  d = decide(&outcome, !queue.is_empty())
  match d:
    StartPending → emit(Running); 启动队列最新
    FinishOk     → emit(Success)
    Fail(k)      → emit(Failed{kind:k})
```

**信息局部性**：`queue`、`running`、`cancel` 全部是 task 私有字段——**调度状态没有任何一份在 task 之外**。外部只有 `UnboundedSender`，连读都读不到。emit 闭包由 src-tauri 注入（发 tauri 事件），scheduler 不知道 tauri 存在。

**事件输出契约**（emit 的两个载荷，即前端 compile-status / errors-updated）：

```rust
pub struct CompileStatusDto { pub phase: CompilePhase, pub kind: Option<FailureKind>, pub draft: bool }
pub enum CompilePhase { Queued, Running, Success, Failed }
pub enum FailureKind { Timeout, ContentError, Aborted }
// draft（roadmap ㉘）：本次产出是否为**草稿**（Quick 单趟，引用/目录可能落后一趟）。
//   仅 Success 阶段按**实际执行**强度报（`CompileOutcome::Success{kind}`）；其余阶段按**请求**强度报（保守）
//   ——故首编被 runner 升级为 Full 时不误报草稿。
// 时序：Queued(入队时) → Running(启动时) → Success / Failed
// 不重发：同一请求只跑一遍（超时即失败，不重试）
```

### 2.6 编译 runner：内嵌唯一（原 SubprocessRunner 已删除）

> 阶段②（ADR-0015 决策 2）删除子进程路径：`crates/latteset-infra/src/runner.rs`（1844 行）与 `crates/latteset-infra/src/tectonic/`（bundle 获取 / `-C` 缓存判定）**整文件删除**，`infra` 不再有 `runner` / `tectonic` 模块（`proc.rs` 保留，synctex CLI 仍用它）。产品只剩**内嵌 Tectonic** 一种 runner：`crates/latteset-tectonic` 的 `TectonicLibRunner`（逐层契约见 §2.7），GUI 侧外加一层约 30 行薄壳 `src-tauri/src/runner.rs` 的 `EmbeddedRunner`。

**随子进程路径一起消失的语义**（改回去即把旧路径请回来）：

| 子进程时代 | 现状 |
|---|---|
| `tectonic [-C] -k --keep-logs --synctex -p -o tmp [-r 1]` 命令构造 / `-C` 条件化 / `SOURCE_DATE_EPOCH` argv 反向断言 | 删除；内嵌形态没有命令行 |
| 三条读任务（stdout / stderr / `tmp/.log` 尾随）→ `LiveFeedback` / `PageMarkerScanner` | 删除；页进度改由捕获的 XDV 完整页数给出，错误改由引擎状态后端上报（§2.6.1） |
| `taskkill /T /F` 树杀、`kill_tree`、Unix 进程组 kill | 删除；取消改**协作式**（X-3，见 §2.7「失败面」） |
| 超时证据 = `.log` + stdout 尾部 + 流式已上报页 | 语义**保留**、数据源改为捕获的 `.log` + 状态环尾部 + 已上报页（§2.6.1） |
| 成功路径 `external_tool_notices`（biber / makeindex / glossaries） | 语义**保留**、搬到 `latteset-tectonic` runner（D3①，§2.7） |
| `finish_success`（PDF 原子拷贝 + `page_hashes: Vec::new()`） | PDF 原子替换语义由内嵌 runner 继承；**页哈希不再是空表**（§2.7） |
| `.ins`/`.dtx` 源码版模板提示（`enrich_source_release_hints`） | 语义**保留**、搬进内嵌 runner 失败路径（D7） |

**薄壳装配（GUI）**：`src-tauri/src/runner.rs` 的 `EmbeddedRunner` 只持 `fs / progress / settings / app_cache_dir`；每趟 `compile` 读一次 `settings.tectonic` 的 bundle / cache 后重建 `TectonicLibRunner` ⇒ **资源设置改完即生效、不需重启**（D1 = R3 选 A）。bundle 来源优先级 = **显式设置 > 环境变量 > 随包内置集 > 在线兜底**；缓存目录 = **显式设置 > 环境变量 > 宿主默认**（与 headless 同序，§2.7）。公式预览共用 `commands::build_compile_runner`，避免三处口径漂移；首启预热的 bundle/cache 与 runner **同源**（settings > env > 随包 > 在线；cache 同序），保证预热生成的 `.fmt` 恰好是首编要用的那一份。

> **起进程仍走 `latteset_infra::proc::command` / `command_std`**（Windows 上 `CREATE_NO_WINDOW`，防 release 档弹黑框）——现只剩 SyncTeX CLI 使用它；编译本身不再 spawn 进程。

#### 2.6.1 内嵌形态的状态面（原「流式反馈」）

| 中间态 | 产端 | 语义 |
|---|---|---|
| `compile-progress`（页进度） | 每趟排版收尾，从捕获的 `<stem>.xdv` 数完整页（`core::xdv::complete_pages`） | **数值变大才发**；粒度 = **趟边界**；与终态页哈希同一份权威数据 |
| `compile-errors` | 引擎状态后端 `MessageKind::Error` → `CompileProgress::errors`（`crates/latteset-tectonic/src/status.rs`） | **非权威**中间态：只有消息文本，无文件/行号/诊断；终态清单仍由 `.log` 解析 |
| `compile-notice` | runner 成功路径 | 能力边界告知（内嵌不跑外部工具，见 §2.7） |
| `compile-preview` | 趟边界合成部分 PDF | `pages` 只增不减；路径恒为 `tmp/<stem>.preview.pdf` |

- **页进度不是 stdout `[N]`**：那路子进程通道已删除。`CompileProgress::pages` 的 trait 注释仍写着"来自 `[N]` 标记"（服务过子进程时代）——**以本表为准**。
- **错误提前量 `[未测]`**：原 `live_errors` 用例（断言首个实时错误距编译结束 ≥100 ms）依赖 latexmk 时代"`.log` 按页 flush"，已随引擎路径删除；内嵌形态的中间错误只在引擎报 `Error` 状态时出现，真实提前量没有真机读数。
- **超时取证**：`timeout_evidence_text(log, status_tail, observed_pages)` —— 捕获的 `.log`（内嵌形态**边写边在内存里**）+ 状态环尾部（含 `Note`/`Error`）+ 已上报最大页（不足时补一行 `[N]` 交回同一套解析），不新造判据；诊断文案来源从"日志"改为"**引擎输出**"。手动终止（`Aborted`）按设计不产证据条目。
- **只报致命错误（终态）**：`entries_from_log(text, warnings = false)` 丢掉 `Overfull`/`Underfull`/`LaTeX Warning`（实测：一次失败的长文档 30 条消息里 27 条是 `Overfull`，会把致命条目挤出可视区）；完整清单（含警告）由终态给出。中间态与终态**刻意分事件**，晚到的中间态不得顶掉权威列表。
- **信息局部性**：runner 无内部状态（一次调用所需信息全在 `CompileRequest` + 当前设置快照里；`fs` / `progress` 都是注入依赖）。

### 2.7 TectonicLibRunner — 内嵌引擎（唯一形态，crates/latteset-tectonic）

> 契约依据：`docs/archive/tectonic-library-plan.md` §3.2（落点裁决）/ §3.4（路径 B 九步）/ §4.2 X-5（本 crate 是外部依赖与 C 链的**第二个落点**）；决策记录 `docs/adr/0012-tectonic-library-form-engine.md` 与 `docs/adr/0015-tectonic-only-embedded-engine.md` 决策 2。**构建入口、装配点与环境前置见 [tectonic-library.md](./tectonic-library.md)**。

| 项 | 内容 |
|---|---|
| 位置 | **独立 workspace 成员** `crates/latteset-tectonic/`（`Cargo.toml` + `build.rs` + `src/{lib,io,status,bundle,runner,synctex}.rs` + `examples/`） |
| 依赖关系 | 依赖闭包拉 C/C++ 链（freetype2/graphite2/harfbuzz/ICU/fontconfig/libpng 外部探测，失败即 build script panic）。**`latteset-infra` 不依赖它**（ADR-0010/0012 硬约束）；`src-tauri` 与 `crates/latteset-server` 以**非可选依赖** + `features = ["network-bundle"]` 挂载 ⇒ **所有构建都需要 vcpkg**。根 `Cargo.toml` 的 `default-members` 已删除——它不再是"发布构建才带"的可选变体 |
| 构建前置 | `build.rs` 在 Windows 补发静态 ICU 需要的 MSVC 隐式系统库（`advapi32` 等 8 个）。**不加它 `cargo check` 过、`cargo test`/example 链接必失败**（`icuuc.lib(wintz.ao)` 未解析 `__imp_RegCloseKey`）；出发点是 vcpkg 自己的 `lib/pkgconfig/icu-uc.pc` 的 `baselibs` |
| 装配 | **两个入口各一处**，都只构造 `TectonicLibRunner`（没有形态判定、没有 feature）：GUI 是 `src-tauri/src/runner.rs` 的薄壳 `EmbeddedRunner`（**每趟编译读一次设置** ⇒ 资源改动即生效）；headless 是 `crates/latteset-server/src/lib.rs` 的 `build_runner`（装配期读 `Session.settings.tectonic`，D4）。**缓存目录优先序两处一致：显式设置 > 环境变量 > 宿主默认**（`pick_cache` 纯函数 + 单测）；**bundle 走 `resolve_source`**：显式 > env > 随包内置集 > 在线兜底（headless 无随包档） |
| **构建入口（唯一落点）** | `scripts/with-tectonic-lib.ps1`（`-Mode check|tauri|test|cli`；npm：`lib:check` / `tauri dev|build` / `lib:test` / `lib:cli`）：校验 vcpkg 检出与 triplet 依赖（缺项给可执行修复命令，而不是让 build script panic）→ 设 5 个环境变量 + 给 `RUSTFLAGS` 补 `-Ctarget-feature=+crt-static` → 跑命令。**别再在别处抄这套环境**；CI 的 `build-windows` 档也走它。**打包链路**：`-Mode tauri build` 先运行 `scripts/make-bundle.mjs`（确定性 zip + `--verify` 自校验 + zip ≤120 MiB 门禁）与 `scripts/make-notices.mjs --check`（许可资源离线一致性），再 `npx tauri build`；CI 另记安装包体积（≤200 MB） |
| 随包资源（③段 ADR-0016 候选 A） | **产品精简宏包集** `src-tauri/resources/tectonic-bundle.zip`（**517 文件 / 52.02 MiB 解包 / 30.43 MiB zip**，digest `9e8c63ca…`；单个 `.zip` ⇒ 上游 `ZipBundle`，不是 500+ 文件）；**许可资源** `src-tauri/resources/licenses/`（308 文件 / 48.95 MiB：NOTICE、261 份许可文本、Fandol 源码、TL patch、可复核清单），都经 `tauri.conf.json` `bundle.resources` 映射到安装目录。`.fmt` **不随包**（首启预热生成） |
| 预热（首启 format） | `runner::prewarm_format(&BundleSource, &Path) -> PrewarmOutcome`（`Generated / AlreadyCached / SkippedNoBundle / Failed`）：只跑 initex+dump、**不跑用户文档**、原子落盘（`.fmt.tmp` → rename）、失败静默**不留半成品**；`src-tauri` setup 末尾 `spawn_blocking` 派发，结果只记日志、不影响启动。**与首编互斥**：进程级 `PrewarmGate`（`Idle/Running/Done/Failed` + `Condvar`）+ 编译入口 `wait_for_prewarm()`（预热中则首编等它结束再判缓存命中；超时 60 s 按"无缓存"继续）。**在线源零网络判定**：`bundle::prewarm_plan` 在 `open_bundle` 之前纯决策——本地源（随包/目录/`file://`）⇒ `Prewarm{only_cached:false}`；在线源只读检查 `<cache>/hashes/<sanitize(url)>`（64hex digest + 同名 `.lock` ≤7 天）+ `data/<digest>/` 非空，齐全才 `Prewarm{only_cached:true}`，否则 `Skip` ⇒ 启动**不联网、不建目录** |
| 运行期配置（2 个环境变量，均被设置覆盖） | `LATTESET_TECTONIC_BUNDLE`（bundle 源：`file:///…` 或**相对路径**；`none`/`off` = 不要 bundle；未设 = 走**随包内置集**，release 有资源目录时）、`LATTESET_TECTONIC_CACHE`（产品缓存目录，覆盖宿主默认值）。`LATTESET_TECTONIC_LIB` **已删除**（启动时检测到会打一条 warn，`latteset_infra::warn_removed_env_overrides`，幂等）。设置面见 §6「引擎页」 |
| **bundle 来源优先级 + 不静默回退** | `latteset_tectonic::resolve_source`：**显式设置 > 环境变量 > 随包内置集（`Shipped`）> 在线兜底**（headless 无资源目录 ⇒ 同序但无"内置集"一档）。release 构建"应有内置集却缺失"传 `Some(缺失路径)` ⇒ 显式报错 `SHIPPED_BUNDLE_HINT`（修复安装 / 显式切在线），**不偷偷联网**；缺包时诊断给可行动出路（切在线完整集 / 补本地 bundle），**不自动换源**（ADR-0016 决策 2）。在线完整集 = 用户显式选择（`https://…`） |
| **缓存 GC** | `latteset_tectonic::gc_cache`：启动空闲时按"**当前 bundle digest**（由 `current_bundle_digest`/同一 bundle 解析器给出：随包/显式/在线；在线只读缓存、零网络）+ 最近 N 份"清理 `formats/` / `data/` / `hashes/`；`N` = `LATTESET_CACHE_KEEP`（默认 3，1..50）；**解析不出当前 digest ⇒ 整体不动**、无 format 时不动、未知条目不误删；缓存目录与 runner/预热共用 `resolve_cache_dir`（设置 > env > 宿主，T22 B3；单测覆盖） |
| 引擎调用 | `TectonicIo`（`IoProvider`）→ `MinimalDriver` → `CoreBridgeLauncher` → `TexEngine`（format 趟 → 排版趟）→ `XdvipdfmxEngine::process`。**没有**"向 `ProcessingSession` 注入 `IoProvider`"这条写法（0.17 没有该入口，方案 §3.4 理由 2） |
| 输入四层 | 内存直喂（主文件）→ **本次产物层**（`IoCapture`；LaTeX 在 `\end{document}` 用**原语** `\@@input\jobname.aux` 回读刚写的 aux，不过 `\IfFileExists` ⇒ 少这层就 abort `failed to open input file "<stem>.aux"`；上游顺序见 `driver.rs` 的 `bridgestate_ioprovider_cascade`：primary → **mem** → fs → … → bundle）→ 项目磁盘（`\input` 子文件，`input_open_name_with_abspath` 供 SyncTeX 用真实源码路径）→ bundle（宏包/字体/cmap） |
| 输出 | 全部先落 `IoCapture`（`Arc<Mutex<..>>`，运行中可读，供 P5 探针用），并镜像到 `tmp/`；**format 趟的输出不镜像**（dump 名是占位，上游在 format 趟后清空内存层）；PDF 收尾**原子替换**到项目根（`{stem}.pdf.tmp` → rename，与子进程档同口径） |
| format | 键 = **bundle digest**（`{digest}-latex-33.fmt`，与上游 `FormatCache` 同名，换 bundle 必换文件、不会复用旧 format）；落点 = `<cache_dir>/formats/`（**不是**缓存根）。未命中则跑 initex 趟：主输入换成合成的 `\input tectonic-format-latex.tex`（上游 `enter_format_mode`）、`halt_on_error_mode(true)`、`Ok(TexOutcome::Errors)` 也算失败、dump 出的 `*.fmt` 由 runner 收集后按 **format 的 stem** 落盘（上游 `make_format_pass`）。算不出落点时**显式报错**，不落项目目录（方案 §5.2 硬约束） |
| 时间源（D4） | 库内**显式** `build_date(SystemTime::now())`（format 趟与排版趟各一次）；`TexEngine::default()` 的 build_date 是 `UNIX_EPOCH`（`engine_xetex/src/lib.rs:92-96`），不写会把 `\today` 静默印成 1970-01-01。**禁止** `build_date_from_env`（进程级全局，会污染同进程其它步骤，D-3） |
| 页哈希 | **恒可用**（内嵌唯一）：XDV 本来就在捕获表里 ⇒ **不必读盘**，直接把那份字节喂 [`latteset_core::xdv::page_hashes`]。页哈希因此吃 **A/B/C**：**A 闸门（T17 L4b 起扩到 Full）** = 页哈希逐页相同 **且 磁盘 PDF 身份未变** ⇒ 跳过 `XdvipdfmxEngine` 转换与拷贝（Quick/Full 都适用）；PDF 身份记录在 `tmp/<stem>.tectonic.pdfid`（`PdfIdentity{len, mtime_ns}`，版本行 `v1`；缺失/不认识/损坏 ⇒ 保守转换一次），转换成功后写记录、复用路径不重写；反例（页数/页序变、PDF 被手工改含同长度覆盖、PDF 被删、空页表）全部回到转换路径（单测钉住）；A/B 复核开关 `LATTESET_A_FULL=0` 只关 Full 复用、默认开。**B/C** 由 `scheduler/actor.rs` 的 `changed_pages` 事件驱动。判据缓存 = `tmp/<stem>.tectonic.pages`（`core::xdv::pages_cache_path`；口径与路径定义在 **core**，唯一一份）。**空表只表示"无法判定"**（XDV 缺失/损坏）⇒ 调用方保守全量刷新；`pdf-updated.pages = 0` 这条分支**仍可达**，不能当死代码删（R8）。**"两形态共用一份页哈希口径"的叙述已失效**（只剩一个消费者），但"口径只在 core"这条不变量不变（R9） |
| bib 趟 / 收敛 | **bibtex 已支持**：跑上游 `default_pass` 的**重跑循环** —— 排版趟 → 若 `.aux` 含 `\bibdata` 则内建 `BibtexEngine` 处理该 aux（`.bib` 走项目磁盘、`plain.bst` 走 bundle）→ 比较 rerun 相关中间产物（`.aux`/`.toc`/`.bbl`…，见 `RERUN_EXTENSIONS`）与上一趟是否相同，不同就再跑 → 稳定即 `kind = Full`。趟数上限：`Full` 请求 **6 趟**（上游同值），编辑触发的 `Quick` 请求另有**三重闸门**（见下一行）。实测：普通文档冷 2 趟 / 已收敛 1 趟；带 `\cite` 的冷 3 趟 + 1 次 bibtex（`latex; bibtex; latex; latex`，引用解析成 `[1]`）。**biber 不跑**（内嵌形态不调用任何外部进程；`.run.xml` 请求 `<binary>biber</binary>` ⇒ `warn!` + `compile-notice` 且 `kind` 退回草稿态，见 `BIBER_PASS_MISSING_NOTE`）。**makeindex / glossaries 也不跑**（检出 `.idx`/`.glo` 即 notice）。T9 实测：本机 PATH 上有真 biber 2.21 也不被调用、不产 `.bbl`；`backend=bibtex` 由内建 BibtexEngine 正常出 `.bbl` |
| **编辑触发档的收敛预算** | 把「草稿」从**固定档**改成**判定结果**：`Quick` 请求同样走重跑循环（含 bib 趟），**预算内**收敛就按实际结果报 `Full` ⇒ 前端 `draft=false`：不亮「引用待更新」，`useIdleConvergence`（只看 `draft`）也不会再排一次 Full。三重闸门 = `passes < 3` ∧ **第 1 趟 < 1000 ms** ∧ 循环累计 < 2000 ms（末值与前端空闲收敛的 `DELAY_MS` **同值**：能在"那 2 s 等待"内收敛就严格更优）。任一条不满足即停在草稿态，**行为与 ㉘ 原样一致**，决策日志点名是哪条（`编辑触发档不再追收敛：停在草稿态 …`）。⚠ 第 1 趟闸门是实测逼出来的：只看总预算时，125 页 / 462 KB 夹具（一趟 1.7 s）会**白跑第 2 趟**、共 **3846 ms** 而中间产物仍在变 ⇒ 加闸门后回到 1 趟 **1847 ms**。release 实测（热 format、本地 bundle）：proj-26（28 页带 bib）已收敛 **1 趟 475–487 ms**、真实改动 **2 趟 1115 ms**，均报 `Full`；large（125 页）已收敛 **1 趟 1878 ms** 报 `Full`、真实改动 **1 趟 1847 ms** 报 `Quick`（退化档）。⚠ 阈值按**发布档**标定：debug 构建同一趟慢 ≈4.6× ⇒ `tauri dev` 下闸门更早触发（退化成旧行为，不是回归）。**T19 六档实测**：`edit`（A 命中）六档全部报 `full`（1 趟）；`edit2`（真实改动）除 `large` 如实报 `quick` 外均报 `full`（multifile 2 趟） |
| **bib 输入未变时跳过** | bibtex 的输出只取决于 ① 各 `.aux` 里的 BibTeX 命令（`\citation`/`\bibdata`/`\bibstyle`，含 `\@input` **闭包** —— `\include` 分章时 `\citation` 在章 aux 里）② 每个 `\bibdata` 指向的 `.bib` 的 `(mtime, size)`。两者与产出当前 `.bbl` 那次一致且 `.bbl` 在 ⇒ **跳过 bibtex，并去掉上游那次无条件重跑**（`RerunReason::Bibtex`）。判据缓存 = `tmp/<stem>.bibsig`（一行一条，`v1` 头；解析失败/`.bib` 读不到一律**当没有缓存** ⇒ 照旧跑，不静默跳过）。⚠ 只按 `.bib` 的 mtime 判是不够的：新增一条 `\cite` 时 `.bib` 没动，跳过去会让 `.bbl` 少一条条目而排版趟只报 `Citation undefined` 警告 |
| `tmp/` 是合法输入（上一趟的中间产物） | `disk_path` 的顺序是 **项目源文件 → 上一趟的 `tmp/` 副本 → bundle**，但第二档**只放行 `RERUN_EXTENSIONS` 那几类**（`.pdf`/`.xdv` 明确排除，否则 `\includegraphics{main.pdf}` 会读到我们自己的产物）。依据：latexmk 正是靠它让 Quick 档的目录/引用**不倒退**——少了这一条实测两个症状：Full 出来的参考文献表在下一次 Quick 里消失；每次编译中间产物都从零开始 ⇒ 永远要 2 趟。编译开始时还会把这些副本**预热进内存层**（`seed_previous_intermediates`），重跑判据才有"上一趟"可比较。⚠ 预热**必须递归**且键用**正斜杠**：`\include` 分章时 `chapters/chNN.aux` 在子目录里，只扫 `tmp/` 顶层会让它们每趟都算"新出现" ⇒ 多文件档永远收敛不了（实测 28 页 8 章档：不递归每轮必跑 2 趟 `engine_total_ms≈1104`；递归后 1 趟 `≈646`） |
| 失败面 | 先 `parse_log` + `diagnose` 出结构化 `ContentError`；`.log` 为空时给 `IoError`，附「I/O 层输入请求摘要（缺哪些文件）+ 引擎状态尾部」；Cancel/超时走**协作式**：置位后每个 I/O 入口立刻报错，引擎在下一个回调处中止（内嵌形态没有进程可树杀） |
| 分阶段计时 | 每次成功编译 `info!` 一行「库形态分阶段耗时」：`format_ms` / `typeset_ms` / `convert_ms` / `engine_total_ms` + 输入请求分类计数（`req_ram/mem/disk/bundle/miss`），另有 T14 新增的**归因打点** `prep_ms`（bundle 打开 + 主文件读）/ `post_ms`（PDF 原子替换 + 页哈希缓存写）/ `preview_ms`（编译中流式出图累计，计入 typeset 窗口，不是前端重载）——只加日志、不改语义。**口径**：`typeset_ms` = **全部**排版趟，`convert_ms` = **仅**那一次 XDV→PDF 转换。⚠ 修正前 `phase_ms.1` 只在第 1 趟末打点 ⇒ `convert_ms` 把**第 2 趟及之后**的时间也算了进去（2 趟时虚高约一倍：`bench/tiny` 片段 2 趟报 148–151 ms、1 趟报 78–81 ms，而两次的 XDV 同尺寸同内容 ⇒ 载荷可证不是转换变慢）；旧记录里"`convert_ms=1` 而 `reused_pdf=true`"那种读数正是 A 闸门跳过转换的那一轮。凡引用口径修正前的 `convert_ms`，先按下表折回"每页 ≈69 ms + 固定段"再比 |
| **流式出图**（roadmap ㉞） | 编译期把**已完成页**缝成部分 PDF（`tmp/<stem>.preview.pdf`，原子替换）交预览。**只能在趟边界出图**：`tectonic_bridge_core::CoreBridgeLauncher::with_global_lock` 用 `static ENGINE_LOCK` 把**整个 C 引擎调用**串起来（上游理由：C 侧 `setjmp/longjmp` 跨 FFI 是 UB）⇒ 同进程内排版趟与转换**不可能并行**（探测：`crates/latteset-tectonic/examples/engine-lock-probe.rs`，4 线程并发墙钟 533 ms == 单线程跑 4 次 541 ms）。实现：重跑循环**末尾**（两个 `break` 之后 = 确定还会再跑一趟）同步调 `PartialPainter::paint`，闸门 = 第 1 趟 ≥500 ms ∧ 页数有涨 ∧ 总次数 ≤8 ∧ 未取消。`bench/large` 冷编实测：1.72 s 出 121 页（转换 130 ms）、4.2 s 出 125 页（175 ms）、权威 PDF 7.4 s ⇒ 提前 ≈5.5 s 可见，代价 +326 ms（+4.4%）。⚠ 中间态**不得**喂页哈希缓存 / A 闸门 / `pdf-updated`。**内嵌唯一后该能力恒可用**（`compile-preview` 已真机验证：中间态严格早于权威 `pdf-updated`、`pages` 单调、权威事件未被污染；前端「编译中预览」换字节不换身份、滚动零漂移、中止后 106 ms 退回权威 PDF，契约见 §9.4） |
| ADR-0010 例外 | `IoProvider` 是**同步** trait（`io_base/src/lib.rs:433-533`）而 core 的 `FileSystem` 是 async（`project/fs.rs:15-16`）⇒ 本层的磁盘读写只能在 `spawn_blocking` 里用 `std::fs`（只碰 `project_root` 与 `tmp/`）。这是 X-5 登记的"第二落点"，**与方案 §4.1 写的"代理到 core FileSystem"有出入**（技术上前者不可达），已记入 t21 的交付说明 |
| 复核入口 | **主入口（唯一）**：`npm run lib:check` → `npm run lib:cli` → `latteset-cli --project <dir> compile` → `node scripts/validate-pdf.mjs --pdf <out.pdf> --expect-pages N --expect-text <子串>`；单测 `cargo test -p latteset-tectonic`（需 vcpkg env，或直接 `npm run lib:test`）；性能 `node scripts/bench-embedded.mjs`；随包/断网/缺包用 `scripts/make-bundle.mjs` + `validate-pdf`（见 [testing.md](./testing.md) §③）；许可 `node scripts/make-notices.mjs --check`；另有 `node scripts/tectonic-lib-xdvscan.mjs`。`scripts/bench-tectonic-lib.mjs` 原用于**库形态 vs 子进程 A/B**，对照组已删除，只作历史入口 |
| 当前状态（T22 复核，2026-11-21） | **内嵌唯一形态 + 随包精简集全绿**：`cargo test -p latteset-tectonic` **65 passed / 3 ignored**（3 个真编在 `--ignored` 下全过：真编 / 预热后首编 / 随包 `Shipped` verbatim 真编）；`npm run lib:test` **399 passed / 8 ignored**；随包 zip 断网首编 exit 0（4.4 s、Fandol 四款、0 U+FFFD）、二次 1.2 s；缺包 → 显式切在线后 exit 0；U-14 + SyncTeX 正向在随包集内跑绿；启动 release GUI 直启进程→Ready **126 ms**、后台预热 **992 ms**、首编 `format_ms=0`。⚠ debug 构建下 C 引擎 `-O0`，同夹具数字会大 6–8×，**比性能必须 release**。仍未支持（按 ADR-0015 决策 4 照单接受，如实提示）：biber / makeindex / glossaries |


## 3. 项目子系统（project 大模块）

### 3.1 拆分

```
project（core）
├── model.rs       —— Project / RootCandidate / RootResolution 类型
├── scan.rs        —— 文件收集（IO 经 FileSystem trait）
├── root_detect.rs —— 根文件探测（纯逻辑）
└── outline.rs     —— 文档大纲解析 + load 编排（§3.5）
```

### 3.2 FileSystem trait（core 定义，latteset-infra 实现）

```rust
/// core 唯一的 IO 抽象（"文件与进程的唯一入口"）。
#[async_trait]
pub trait FileSystem: Send + Sync {
    async fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>>;   // 非递归，带 is_dir
    async fn read_to_string(&self, path: &Path) -> io::Result<String>;
    async fn read_to_string_lossy(&self, path: &Path) -> io::Result<String>; // 默认退化为严格读取
    async fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;     // 对外形态（剥 Windows \\?\ 前缀）
    async fn is_dir(&self, path: &Path) -> io::Result<bool>;
    async fn write(&self, path: &Path, contents: &str) -> io::Result<()>;
    async fn exists(&self, path: &Path) -> io::Result<bool>;              // 产物探测（㉘：Quick 前置条件）
}
```

**设计决策 D4**：探测、日志解析、设置存储、命令面的文件读写全部经此 trait，core 因此无任何文件依赖；实现集中在 latteset-infra（tokio::fs，ADR-0010）。

### 3.2.1 paths.rs — 路径策略（D8）

core 提供 `resolve_project_root` / `resolve_in_project`（已存在目标）/ `resolve_creatable_in_project`（新建目标：解析父目录后拼接）与 `PathError{NotFound, RootUnavailable, Outside, NotADirectory}`；命令面只把失败原因翻译成 `{ code, message }`，不再自己 canonicalize + 前缀比对。

否决"宽松回调注入"（每个函数手写闭包签名，接口面发散）。

### 3.3 scan.rs — 文件收集

```rust
/// 递归收集项目内全部 .tex（排除 tmp/ 与隐藏目录），不跟随符号链接（防环）。
pub async fn collect_tex_files(fs: &dyn FileSystem, root: &Path) -> io::Result<Vec<PathBuf>>;
/// **编译触发规则**（单一事实来源：watch 的粗过滤 + compose 的最终判定都用它）
/// 触发 = 项目内**任何带扩展名**的非忽略文件（roadmap ㉜）；排除 tmp/、隐藏项与自己的产物
pub fn is_compile_trigger(path: &Path, root: &Path, root_file: Option<&Path>) -> bool;
/// 文件树忽略规则（只藏 tmp/ 与隐藏项，树展示所有扩展名）
pub fn is_tree_excluded(path: &Path, root: &Path) -> bool;
```

**算法**：DFS 递归；每个目录先 `read_dir`，逐项判定（`is_hidden_or_tmp` 跳过 `tmp/` 与隐藏项），目录递归、`.tex` 收集。**函数内**持有递归栈与缓冲；**模块内**无缓存（每次调用全量扫描——文件树重建与探测都调它，目录量大时再优化增量）。

**触发规则的三条边界**（`is_compile_trigger`，roadmap ㉜）：
1. **不再按扩展名白名单收窄**（过去只认 `.tex`）：`.bib`/图片/`.cls`/`.sty` 都是引擎的输入。实测（[G1 报告](../archive/research/g1-read-interception-feasibility.md) §5.1，latexmk 时代）：改被 `\bibliography` 引用的 `refs.bib` 时全部 tmp 产物 mtime 纹丝不动 ⇒ 用户"改了没反应"。
2. **必须带扩展名**：目录不是输入，而 notify 在 Windows 上把目录增删报成 `Create(Any)/Remove(Any)`（实测：建一个目录走的就是 `Any`）⇒ 靠事件类型认不出，用"带扩展名"兜住（跨平台成立，且不碰文件系统）。代价是非输入文件（`notes.md`）也会触发一次 —— 精确失效是 **㉝**。
3. **排除自己的产物**：根 `<stem>.pdf`（与原子替换的 `<stem>.pdf.tmp`）由每次编译写在项目根，不排除就会**自激**。只看项目根这一层、大小写不敏感；`figures/plot.pdf` 这类输入不受影响。

### 3.4 root_detect.rs — 根文件探测

```rust
pub struct RootCandidate { pub path: PathBuf }

pub enum RootResolution {
    Unique(PathBuf),
    Multiple(Vec<PathBuf>),   // 前端弹窗选择
    None,                     // 前端提示手动指定
}

/// 提取 \input{...} / \include{...} 引用（去扩展名、去花括号）
pub fn extract_includes(content: &str) -> Vec<String>;
/// 提取 \documentclass 声明（含可选参数形式 \documentclass[..]{..}）
pub fn extract_documentclass(content: &str) -> Option<String>;
/// 候选 = 含 \documentclass 且不被任何文件引用
pub fn find_candidates(files: &[PathBuf], read: impl Fn(&Path) -> Option<String>) -> Vec<RootCandidate>;
pub fn resolve(candidates: Vec<RootCandidate>) -> RootResolution;
```

**算法（设计决策 D5：正则启发式）**：

```
extract_includes:  正则 \\(input|include)\{([^}]+)\}，规范化：去 .tex 后缀 → 相对路径
extract_documentclass: 正则 \\documentclass(\[[^\]]*\])?\{([^}]+)\}
find_candidates:   读每个文件内容 → 收集引用集合（相对路径规范化）→
                   候选 = 含 documentclass 的文件 − 被引用的文件
resolve:           1 个 → Unique；>1 → Multiple（按路径排序，稳定顺序）；0 → None
```

否决"完整 TeX 词法解析"：v1 无此成本收益；**已知局限**（写进注释与测试）：注释里的 `\input` 会误报、`\includeonly` 未处理、编码（UTF-8 优先）。手动覆盖（settings.root_file）永远是逃生门。

**信息局部性**：`read` 闭包按需读内容、用完即弃——**任何文件内容不跨函数存活**；候选集合只在 find_candidates 内。探测是纯函数组合：`resolve(find_candidates(files, read))`，无状态。

### 3.5 outline.rs — 文档大纲（源结构树）

```rust
pub struct OutlineItem { /* title / level / file / line / file_base */ }
pub struct OutlineContext<'a> { /* root、root_file、打开标签的实时缓冲、文件树兜底列表 */ }
pub enum ScanEvent { Item(OutlineItem), Include(Vec<String>) }        // 单文件扫描事件（顺序 = 文档顺序）
pub struct OutlineCache { /* root + 每文件指纹/事件 + 打开缓冲 + 上次 scans/reused */ }
pub struct OutlineInput<'a> { /* root、root_file、changed_buffers、open_paths、fallback_files */ }
pub fn strip_tex_comment(line: &str) -> String;                                    // 行级剥离（语义见 §9.5）
pub fn resolve_include(raw: &str, from_file: &str, project_root: &str) -> Vec<String>;
pub fn build_tree(flat: &[OutlineItem]) -> Vec<OutlineNode>;
pub fn load(ctx: &OutlineContext<'_>, fs: &dyn FileSystem) -> Vec<OutlineNode>;    // 无缓存（一次性调用方/测试）
pub fn load_cached(input: &OutlineInput<'_>, cache: &mut OutlineCache, fs: &dyn FileSystem) -> Vec<OutlineNode>;
```

**语义**（GUI `get_outline`、headless `Session::outline` 共用同一实现，前端不留副本）：

- 跟随根文件的 `\include`/`\input` 图逐文件解析 `\part/\chapter/\section/\subsection/\subsubsection/\paragraph/\subparagraph`（含 `\section*` 与 `[short]`），按**文档顺序**建嵌套树；
- **缓冲优先**：打开标签的实时缓冲命中就用缓冲（未落盘也反映），否则读盘；读失败跳过；`visited` 防环；
- 无根文件时用前端文件树提供的兜底 .tex 列表，后端不自行扫描；
- `\input` 相对**项目根**解析优先、回退当前文件目录；`\begin{verbatim}` 环境内的行跳过；
- **读集安全是 D8 的词法版**（core 无 canonicalize）：与 `read_file` 命令的差异仅剩「符号链接目标解析」与 8.3 短名（Windows 大小写不敏感 FS 下行为一致）；
- **入口路径一律先归一**（反斜杠 → 正斜杠 + `normalize_path`）：否则同一文件在 `root_file`（后端给的 Windows 反斜杠路径）与 include 候选（正斜杠）两种拼写下是两个 key，`visited` 去重失效 → 大纲出现重复项、缓存存两份。

**增量缓存（`load_cached`，roadmap ⑦a）**——三条不变量：

1. **内容每次重取，只复用"扫描结果"**：文件内容要么取实时缓冲、要么读盘（外部改动不会被缓存钉住），仅在**内容指纹相同**时复用上次的 `ScanEvent` 序列（省掉逐行 `strip_tex_comment` + 正则）；
2. **缓冲缓存按项目根作废**：`OutlineCache.root` 与本次 `root` 不一致即整体清空；`open_paths` 里没有的缓冲被淘汰（标签关闭 → 该文件回到读盘）；
3. **只保留本轮访问过的文件**：删除 / 取消引用的文件从缓存移除（缓存不随时间膨胀）。

**刷新与交互**：`events.ts` 在项目打开、编译成功、`files-changed(structural)` 时调 `outline.refresh()`；前端只提交**变化过的**缓冲 + 当前标签集合（`lastSent` 差分，项目根变化时清空重发），并**合并进行中的刷新**（结构事件风暴实测一次连发 11 次 → 现在最多「当前 + 补一次」）；点击项 → `editor.openFile(file, line)` 揭示源码 + `useSyncTex.forward(file, line, 0)` 高亮 PDF 对应页（SyncTeX 不可用则只跳源码）。`OutlinePane` 用 `file:line` 作 key（稳定标识）。

**实测（11 文件 / 436KB 工程，真机）**：一次编译成功后的刷新 = 只重扫被改的 1 个文件（`scans=1 reused=10`），空闲收敛那次 = `scans=0 reused=11`；前端两次分别只推了 1 个缓冲与 0 个缓冲。成本从「全量重扫 + 全量提交」的 **55.5ms** 降到 **8.4–9.7ms**（详见 [archive/research/p1-large-doc-editor-analysis.md](../archive/research/p1-large-doc-editor-analysis.md) §3.3）。

**已知成本**：仍只有「编译成功」与「结构变化」两个触发点——**编译失败期间大纲不更新**（旧行为保留；缓存已让"按保存触发"变得便宜，见 §12.1 #17）。

## 4. 日志解析（log_parser 大模块，core）

```
log_parser
├── model.rs   —— LogMessage / MessageKind
└── scan.rs    —— 逐行扫描状态机
```

```rust
pub struct LogMessage {
    pub kind: MessageKind,          // Error | Warning
    pub message: String,            // 聚合后的可读消息
    pub file: Option<String>,       // 栈顶文件（若可确定）
    pub line: Option<u32>,          // "l.<n>" 行
    pub raw: String,                // 原始行（快照测试与调试用）
}

/// 输入只有 .log 全文，输出只有结构化消息。无状态、无 IO。
pub fn parse_log(text: &str) -> Vec<LogMessage>;
```

**算法（行级状态机）**：

```
逐行扫描，维护函数内的两个局部变量：
  file_stack: Vec<PathBuf>   —— "(" 开括号行 push（TeX 文件切换标记），")" 行 pop
  current: Option<LogMessage>—— 正在聚合的消息（跨 2-4 行）

分类规则（按行首匹配，顺序即优先级）：
  1. 以 "!" 开头                → 新 Error：file = stack 顶，后续缩进行聚合进 message
  2. 匹配 /^l\.(\d+)/           → 给 current 补 line（仅 Error 有效）
  3. /^\(([^)]*\.tex)/          → file_stack.push
  4. /^\)/                      → file_stack.pop（含 ")(" 连续切换的边界处理）
  5. /Warning|警告/             → 新 Warning（file/line 同规则尽力而为）
  6. latexmk 时代的 "Run #" / 目录回显行 → 丢弃（历史驱动产物；Tectonic 不写这些行）
  7. 其他                       → 若 current 正在聚合且行非空 → 追加；否则丢弃
```

**信息局部性**：`file_stack` 与 `current` 是 **parse_log 的函数内局部变量**，一次调用即生即灭——这是"函数内信息"的范例。模块无缓存、无全局。

**测试**：insta 快照——真实 `.log`（成功/内容错误/超时/多文件）固化为 4 个用例（语料来自 latexmk 时代，仍覆盖同一批 TeX 报错形态）；另加 Tectonic 日志布局用例 `tectonic_log_layout_keeps_file_stack`（打开标记在行中间 / 关闭+打开同一行）；含注释误报、`\include` 嵌套的探测用例另立。

**编码容错**：`.log` 可能含非法 UTF-8——GBK 源文件经 **pdflatex** 会把原始字节回显进日志（xelatex 自行替换）。`decode_log(bytes)` 严格 UTF-8 优先、失败则 lossy；`FileSystem::read_to_string_lossy`（默认实现退化为严格读取，`TokioFs` 覆盖为 lossy）是 runner 读 `.log` 的路径。严格读取会让「编译失败」退化为「拿不到任何错误信息」。**未覆盖**：`read_file` 仍是严格 UTF-8，编辑 GBK 源文件不在 v1 范围。⚠ 两个引擎已删除，但**结论仍成立**：任何往日志里回显原始字节的引擎都要走 lossy 解码（Tectonic 的日志编码行为 `[未测]`）。

### 4.1 diagnosis.rs — 把原始报错翻译成「原因 + 怎么改」

```rust
pub enum DiagnosisKind {   // 21 类（22 − 2 个引擎类 + 1 个 UnsupportedTool）
    MissingPackage, MissingClass, MissingFile, MissingFont, MissingFontset,
    UnsupportedTool, UndefinedControlSequence, UnclosedGroup,
    MissingMathMode, ExtraBrace, MissingBeginDocument, DoubleScript, MisplacedAlignment,
    EmergencyStop, OptionClash, NonUtf8Source,
    CompileTimeout, CompileTimeoutStalled, AuxWriteFailed,   // 超时与写盘，见 §4.2
    PackageError, LatexError,                                // 兜底：至少点出宏包名 / 原文
}
pub struct Diagnosis { pub kind: DiagnosisKind, pub cause: String, pub hint: Option<String>,
                       pub suggested_timeout_secs: Option<u32> }   // 机器可读的操作参数
pub fn diagnose(msg: &LogMessage) -> Option<Diagnosis>;
```

**契约**：

- **匹配不到返回 `None`**，前端降级为「原文 + 行号」——宁可不说，也不瞎说；
- 语料是 `real_error_corpus.rs` 的 **20 例真实 `.log` 片段**（模板编译矩阵的失败样本 + 故意写错的最小文档），期望值**手写**在 `diagnosis_tests.rs`，避免实现自证；断言含 DoD 覆盖率 ≥80% 与「干净日志不误报」；
- `ErrorEntry.diagnosis: Option<Diagnosis>` 进 DTO（specta 导出）；`ErrorList.vue` 有条目诊断时渲染两行（首行原因、次行 `→` 建议），无诊断退回原文首行、原始 `.log` 消息降为 `title`；头部「已诊断 N」；去重键按**诊断原因优先**；
- 两个实现陷阱：① 宏包报错的**续行前缀必须先剥**（`(fontspec)` 之类正好卡在两段关键词之间，不剥则永不匹配）；② `parse_log` 的 `l.<n> \cmd` 位置标记行**必须并入消息**，否则说不出未定义命令名（前端仍只渲染首行，观感不变）。

### 4.2 超时：证据采集与一键提超时

- 超时**进错误列表**：`CompileOutcome::Timeout { entry }`，条目由 runner 现场采集证据构造（是否首编 / `.tex` 源文件数 / **引擎输出**已排版到第几页；证据 = `.log` + 已捕获输出尾部 + 流式最大页，见 §2.6.1）；手动终止（`Aborted`）不产证据条目；
- 两类判据：有页输出 → `compile_timeout`（在推进，只是比上限慢）；无任何页输出 → `compile_timeout_stalled`（可能仍在做前置处理，也可能真卡住——**不武断说"卡住"**）；
- **强判据优先**：证据文本里已有致命错误 → 直接报那条错误且**不给**一键按钮（提高超时救不了）；
- `suggested_timeout_secs` 阶梯 `300 → 900 → 1800`，**首编直接跳 900**（首编最慢，让用户在注定不够的 300s 上再等一次代价过高）；`ErrorList.vue` 据此渲染「提高到 Ns 并重试」，点击后 `update_settings` → `compile_now`（顺序固定：先改设置再重跑）。

## 5. SyncTeX（synctex 大模块）

> **默认实现是进程内自解析**（[ADR-0013](../adr/0013-synctex-in-process-parser.md)）：`SyncTexProvider` **不依赖系统 `synctex` 二进制**（那个二进制由 TeX Live 分发，与"零预装可用"的前提冲突）。CLI 实现保留为 `LATTESET_SYNCTEX=cli`（A/B 复核用）。验收 = `node scripts/synctex-selfcheck.mjs`：三组真实工程上与 CLI **逐点对拍**，往返跳到位 **12/12、7/10、12/12**。已知偏差：beamer 类页面的前向坐标中位差 132 pt（样本行在记录里没有对应行）、`column` 恒 -1。
>
> ⚠ **2026-11 验收修正（口径收窄）**：**正向与 CLI 一致**；**反向在"页尾命令行 / 跨文件页"上有已知精度缺陷**——任意点点击会落回页尾命令的行号，跨文件页可能指错文件（`LATTESET_SYNCTEX=cli` 在同数据上正确；`xelatex` 产物同样复现 ⇒ 预存缺陷、非引擎切换引入）。上面"往返 12/12、7/10、12/12"是**用正向给出的坐标回查**的口径，**不等于任意点反向**，不得再当作"自解析不劣于 CLI"的证据。逐条机理/夹具/判据见 §12.1 #29。
>
> **重试语义（roadmap ㊱）**：退避**只给瞬时错误**（`Io` / `Busy`），确定性失败（`Unavailable` = 同步数据不存在、`Parse` = 没有这段映射）**立刻返回**；并且"整份同步数据用不了"时 `resolve_inverse` 会**跳出 5 个 y 候选的回落循环**。对比全退避的旧行为（一次失败查询 **668 ms**、无同步数据时 inverse **3.13 s** = 5 候选 × 625 ms），现为 **14 ms** 与 **11 ms**；瞬时竞争（编译中同步文件被重写）仍按 100/200/300 ms 退避且实测能救回来。
>
> **内嵌形态的「主输入名」缺口已修（roadmap ㊷）**：内嵌档编译出的 `.synctex.gz` 里 `Input:1` 原本是引擎的默认 jobname **`texput`**（根因在 C 侧：`tt_run_engine()` 先 `open_log_file()` 把 jobname 定成 `texput`，之后 `start_input()` 里"jobname 为空才用主输入名"的分支永不命中；而 `tt_xetex_set_string_variable` 是空实现 ⇒ Rust 侧设不了），于是自解析按文件找 tag 时**主文件永远落空**——单文件项目双向定位全失效（被 `\include` 的章节本来就正常）。修法是**纯数据补丁**：收尾处把主输入记录补成真实绝对路径（`crates/latteset-tectonic/src/synctex.rs`，捕获表与 `tmp/` 镜像两处都改），**不改引擎、不动文档结构**。实测（补丁当时）：两夹具的 `forward` 与历史子进程档逐字段相同、PDF/`.log`/错误归属逐字节不变（契约与判据见 [roadmap.md](../roadmap.md)）。
>
> ⚠ **残留（与本补丁无关、属既有引擎差异）**：**目录区域**的反向命中归到生成物 `main.toc`（前端按 ㉒ 显示"来自自动生成的文件"）。诊断方法（gunzip 看 `Input:` 行）与对照证据见 [troubleshooting.md](../troubleshooting.md)。

```
synctex（core）              synctex（latteset-infra）
├── model.rs                 └── synctex.rs —— SyncTexProvider 的两个实现
├── provider.rs                  · SyncTexSelf（默认，自解析：gzip 解压 + 几何查询）
├── parse.rs —— .synctex 解析    · SyncTexCli（LATTESET_SYNCTEX=cli，含竞争重试）
└── classify.rs —— 反向命中目标分类（roadmap ㉒，纯逻辑）
```

```rust
// core
pub struct SourcePosition { pub file: PathBuf, pub line: u32, pub column: i32 }   // column=-1 表示未知（synctex 1.21+ 实测）
pub struct SyncTexPosition { pub page: u32, pub x: f32, pub y: f32 }

#[async_trait]
pub trait SyncTexProvider: Send + Sync {
    /// 源码 → PDF：synctex view -i "line:col:file" -o "<pdf>" -x
    async fn forward(&self, src: &SourcePosition, pdf: &Path) -> Result<SyncTexPosition>;
    /// PDF → 源码：synctex edit -o "page:x:y:<pdf>"
    async fn inverse(&self, pos: &SyncTexPosition, pdf: &Path) -> Result<SourcePosition>;
}

// core/classify.rs：命中的东西是"用户源码"还是"生成产物/项目外文件"
pub enum InverseTarget { Source(PathBuf), Generated(PathBuf), OutsideProject(PathBuf) }
pub fn classify_inverse_target(root: &Path, hit: &Path) -> InverseTarget;

// latteset-infra synctex.rs：spawn synctex 二进制（tokio::process），解析 stdout
pub fn parse_forward_output(text: &str) -> Result<SyncTexPosition>;  // 纯函数，可单测
pub fn parse_inverse_output(text: &str) -> Result<SourcePosition>;   // 纯函数，可单测
// 失败按 100/200/300ms 退避重试（`with_retry`，退避时长可注入 → 单测传全零）
```

**算法**：CLI 输出契约（Windows 实测，完整输出形态见 ADR-0008）：`view` 可能返回多个 `Output:` 块 → `parse_forward_output` 取**首个完整块**；`edit` 输出 `Input:`（正斜杠 + `./` 路径）与 `Column:-1` → `parse_inverse_output` 处理，`Input` 路径必须经 `project.resolvePath`（内部 `normalizePath` 折叠 `.`/`..`/连续斜杠）归一化——否则反向命中会重复打开同一个标签。两者均以真实 Windows 输出固化单测（`provider.rs`）。pdf 路径指向 `tmp/<root>.synctex.gz` 对应 PDF 的**项目根副本**（synctex 按文件名关联），必须传 `-d <tmp>` 参数。

**取首个完整块是确定的选择**：beamer 下曾疑似命中容器框（`H` 达 30–255pt 的导航条/整页框），把规则换成「最小 H」「首个 H≤40 的块」后**往返结果完全一致**（三组样本逐一相同）→ beamer 的 2–4 行偏移属 beamer 记录粒度，与取块策略无关（精度基线见 [design.md](../design.md) §预览）。

**命令层策略（roadmap ㉒，`commands::synctex_inverse`）**：反向命中非源码时按 y 偏移 `[0, -40, +40, -80, +80]` 就近探测，取**第一个项目内源码**（首个命中即偏移最小者）→ 返回 `InverseResultDto { source: Option<…>, note: Option<String> }`；全部落空则 `source: None` + 一句人话提示（生成文件/项目外文件/同步数据缺失）。探测只在未拿到源码时发生，正常点击延迟不变。

**信息局部性**：provider 无状态；一次调用 = 一次 spawn + 一次解析（失败时若干次重试）。前端高亮 overlay 与滚动恢复是预览模块自己的事，不流入此模块。

## 6. 设置（settings 大模块）

```
settings（core）              latteset-infra
├── model.rs                  ├── storage.rs —— 读写盘、原子写、自写盘 hash 过滤（is_self_write）
├── merge.rs                  └── (watch.rs handle_settings_change —— 监视 → 重载 → 广播)
└── validate.rs
```

```rust
// core：纯逻辑
pub struct Settings {
    pub schema_version: u32,          // 当前 = 3（ADR-0015：v2 = 引擎统一，v3 = 形态收成内嵌唯一）；缺键按当前版本兜底
    pub compile: CompileSettings,
    pub tectonic: TectonicSettings,   // 全局；见下「引擎页的设置面」
    pub ui: UiSettings,               // 全局；#[serde(default)] ⇒ 旧 settings.json 没有 ui 也能读
    pub notices: Vec<Notice>,         // 全局一次性提示；空表不序列化
    pub root_file: Option<PathBuf>,   // 项目级手动覆盖（探测结果的逃生门）
}
/// 一次性提示：`{id, level, message}`，`level = info | warning`（snake_case）。
/// dismiss 按 `id` 从列表移除（SettingsPatch.dismiss_notice）。
pub struct Notice { pub id: String, pub level: NoticeLevel, pub message: String }
pub struct CompileSettings { pub mode: CompileMode, pub debounce_ms: u64, pub timeout_secs: u64, pub new_file_wizard: bool }
pub struct TectonicSettings { pub bundle: Option<String>, pub cache_dir: Option<PathBuf> }
/// 形态位（`lib_form`）在 v3 随子进程形态一起删除；只剩两项资源。
/// 界面（roadmap ⑩）。`UiTheme = light | dark | system`，**默认 light** —— 深色是新能力，
/// 默认值必须让既有用户升级后**外观不变**（默认 system 会在升级那刻静默换掉系统偏好深色用户的配色）。
pub struct UiSettings { pub theme: UiTheme }

pub fn default_settings() -> Settings;
/// 项目覆盖全局，逐键合并（项目缺失字段继承全局）
pub fn merge(global: Settings, project: Settings) -> Merged;
/// 范围校验：timeout 5..=1800（㉕ 上限由 600 放宽）、debounce 100..=2000（引擎已不可选，不再有引擎枚举校验）
pub fn validate(s: &Settings) -> Result<(), Vec<String>>;
/// 局部更新：只改 patch 里的键，其余不动
pub fn apply_patch(base: &mut Settings, patch: SettingsPatch) -> Result<()>;

// src-tauri storage.rs（自写盘过滤也在此；watch.rs 负责监视→重载→广播）
pub fn global_path(&self) -> &Path;
pub async fn load_global(&self, fs: &dyn FileSystem) -> Settings;   // 缺失/损坏/越界 → default + 落盘
pub async fn save_global(&self, s: &Settings);   // 原子写：临时文件 + rename
pub async fn load_overrides(&self, fs: &dyn FileSystem, project_root: &Path) -> ProjectOverrides; // 缺失 → 空（全继承全局）
pub async fn save_overrides(&self, project_root: &Path, o: &ProjectOverrides);
pub fn is_self_write(&self, path: &Path, content: &str) -> bool;  // 自写盘 hash 过滤（D6，消费一次）
```

**v1 → v2 → v3 迁移（ADR-0015，`storage.rs`）**：`migrate_global_value` 返回 `Migration { notices: Vec<Notice>, backup_from: u32 }`（一次迁移只落一次盘，v3 有两条提示；`backup_from` 取**最早一级**的源版本）。

| 级 | 判据 | 动作 | 备份 | notice |
|---|---|---|---|---|
| v1 → v2 | `schema_version < 2` **或**还存在 `compile.engine` | 版本写 2、删 `compile.engine`（旧值静默忽略、**不报错**） | `settings.json.v1.bak`（已有不覆盖） | `engine-unified-v2` |
| v2 → v3 | `schema_version < 3` **或**还存在 `tectonic` 的旧形态位键 | 版本写 3、删 `tectonic.lib_form` | `settings.json.v2.bak`（已有不覆盖） | `form-embedded-v3`（info，**恒加**）+ `subprocess-form-removed-v3`（warning，**仅旧值 `lib_form == false`**） |

- **幂等**：已是当前版本且无旧键 ⇒ `None`、一个字节都不改。
- **只读 / 写盘失败**：内存按迁移后值生效、收回 notice、只 `warn!`（同一条路径，不另写一份）。
- **损坏**：解析失败 → 先 `.corrupt.bak` 再回退默认（损坏文件无从得知旧值，接受）。
- **BOM**：剥 BOM 再解析。
- **单一入口**：`load_global`（启动）与 `watch` 热更新**共用** `storage.parse_global`——两处各写一份会漂移（一处迁移、一处不迁移）。项目覆盖不用迁（`ProjectOverrides` 从来没有 `tectonic` 段）；项目文件里的旧 `compile.engine` 仍按 v1 口径被忽略且不重写项目文件（只打一条 `info!`）。
- **已删环境变量**：`LATTESET_TEX_ENGINES`（①段删）与 `LATTESET_TECTONIC_LIB`（②段删）都在 `SettingsStorage::new` / 启动时经 `warn_removed_env_overrides` 检测，各打一条"已删除，忽略"的 warn（`Once` 幂等、不刷屏）。

**引擎页的设置面（⑫② + ③段收口）**：**宏包集来源**与**缓存目录**两项资源（`settings.json` 的 `tectonic` 键），改完**下一趟编译即生效**（GUI 的 `EmbeddedRunner` 每趟编译读一次全局设置；headless 装配时读同一份）。三条界面上才成立的契约：

- **bundle 的存储形态 ≠ 展示形态**。上游 `detect_bundle` 先做 `Url::parse`，绝对 Windows 路径会被当成 scheme `e` 吃掉（方案 §5.6 **LB-1**）⇒ **磁盘上必须存** `file:///E:/…` 或相对路径。但用户眼里它就是一条目录路径，所以：**入口宽容**（`TectonicSettings::normalize_bundle` 把 `E:\…` / `E:/…` / 带引号的「复制为路径」统一补成 `file:///…`；`apply_patch` 里归一化在前、校验在后），**出口好看**（`SettingsPanel` 的 `bundleDisplay` 剥掉 `file://` 前缀再显示/回填）。`cache_dir` 不走这套——它是普通路径，没有 URL 形态。
- **宏包集来源 = 三档显式可选（③段 ADR-0016 候选 A）**：① **内置精简集**（默认；随包 `tectonic-bundle.zip`，517 文件 / 30.43 MiB，离线可用）；② **在线完整宏包集**（`tectonic.bundle = https://…`；首次按需取包、之后复用缓存）；③ **本地目录 bundle**（`file:///…` 或相对路径，需自带 `SHA256SUM`）。设置页展示**当前来源**；错误面板对缺文件类诊断给「切到在线完整宏包集并重试」一键（写在线地址 + 重编），**绝不自动换源**（决策 2 不静默回退）。
- **引擎页没有形态位**：没有下拉、没有形态 chip、"选了但没编进来"这类禁用态随 `tectonic-lib` feature 一起删除。页签标题仍是「引擎（Tectonic）」，页内两行 = 宏包集来源 / 缓存目录（`cache_dir` 空 = 宿主默认；`LATTESET_CACHE_KEEP` 控制 GC 保留份数，默认 3）。
- **没有引擎闸门**：`ENGINE_ID` 恒为 `"tectonic"`，状态栏引擎名是前端常量 `Tectonic`（ADR-0015 ②后不再有 `engine_form` 命令；形态位不存在，也就没有"前端不得自己推形态"那条约束了——不要把它加回来）。

**设置面板的分页（标签栏）**：面板从"一长条滚动"改为四个标签，按**用户要解决的问题**分页，而不是按字段所属模块：

| 标签 | 内容 | 分页理由 |
|---|---|---|
| 外观 | 主题（浅色/深色/跟随系统） | 与文档无关的个人偏好 |
| 编译 | 编译模式、防抖、超时、**新建文件向导**（㊺） | 什么时候编、编多久，以及"新建第一份文档时领不领路" |
| 引擎（Tectonic） | **宏包集 / 缓存目录**两项资源 | 引擎固定为内嵌 ⇒ 这一页只回答"用哪份宏包集、format 缓存放哪" |
| 项目 | 根文件覆盖 | 只影响当前项目 |

三条界面契约（改面板时必须一起维持；旧的"琥珀点"契约随引擎选择删除）：

1. **分页是 `v-if` 而不是隐藏** ⇒ 任一瞬间 DOM 里只有当前页的字段（也避免了跨页重复 `id`）；测试断言据此写（见 `SettingsPanel.spec.ts` 的"每页只渲染自己那组字段"）。
2. **两级控件形态要分得开**：标签栏用**下划线式**（翻页），面板内选值用**胶囊式** `.seg-btn`（选一个值）。
3. **记住上次看的那页**：`activeTab` 是**模块级** `ref`（面板 `v-if` 挂载，组件内 ref 每次重开都会回到第一页）；它不进设置、不落盘。

**算法（merge）**：字段级 Option 语义——全局 `settings.json` 与项目 `.latteset/settings.json` 同 schema（含 `schema_version`）；项目文件只写它覆盖的键，其余继承。

**热更新（设计决策 D6）**：watch 识别 `.latteset/settings.json` 变化 → 重载 → 广播 `settings-changed`。**自写盘过滤**：`update_settings` 写盘时记录 `(path, content_hash)`；watch 事件到达时比对 hash，相同则跳过（防"自己写 → 自己重载 → 重复广播"）。hash 存在 `storage.last_write` 内，不跨模块。

**源文件没有对应的自写盘过滤，由前端兜**（`editor.ts` 的 `onFilesChanged`）：Windows notify 对**同一次**写盘会投递**多条**事件（实测：一次编辑 → 2 条 `files-changed`、**同一毫秒**、都早于 `saveAll` 的 promise 续体）。因此脏分支的时间窗口判据**不能"消费一次"**——一消费，第二条就撞上"仍脏"并误报 `外部修改`，而该提示的动作是 `acceptExternal`（**放弃本地**）⇒ 误报 + 一点击 = 丢输入。现行规则：窗口内（<2s）忽略；窗口外**再比一次磁盘内容**，与缓冲一致就不算冲突，不一致才报。**干净分支读盘后的复检同样只看内容**：那次 await 期间 buffer 可能已被上一条事件的重载写好（`EditorPane` 的 `setValue` 会把它标脏），磁盘与缓冲一致 ⇒ 没有可丢的输入。重载侧则在 `setValue` 的调用栈内用开关屏蔽内容变更事件（`EditorPane.vue` 的 `applyingReload`），否则重载被当成用户编辑：tab 变脏、自动保存回写、第二条事件误报冲突（真机实测 2/2，见 [troubleshooting.md](../troubleshooting.md)）。

**覆盖清洗（`sanitize_overrides`，core `settings/validate.rs`）**：读回项目覆盖时**逐字段**校验——越界/非法的 `root_file` 置 `None`（回退全局探测），合法的 compile 覆盖保留。整包丢弃会连带丢掉同一文件里合法的 compile 覆盖，不可取。

**信息局部性**：core 的合并/校验是纯函数；全局设置快照是 §1 清单里唯一的 `RwLock` 共享态——写者只有 storage 模块，读方（组合层构造 CompileRequest 时）只取一次性快照拷贝，不持有引用。

## 7. 监视与触发组合（latteset-infra）

```
watch.rs          —— notify 8.2 接线：事件规范化 + 过滤
compose.rs        —— 组合层：文件事件 → 编译请求（D3 的关键翻译，位于 core，由本层调用）
```

```rust
// watch.rs：notify 事件流 → 规范化 → 分类
//   任意非忽略路径（tmp/、隐藏项除外）→ compose::compile_request_for_change(path) → scheduler
//     ↑ roadmap ㉜：**不再只认 `.tex`**（`.bib`/图片/`.cls`/`.sty` 都是输入）；
//       "算不算输入"的最终判定在 core（`is_compile_trigger`）—— 那里才拿得到根文件名、
//       也才排除得了本次编译自己的产物（根 `<stem>.pdf`，不排除会自激）。
//   settings.json（全局/项目）→ 热更新：is_self_write 过滤 → 重载 → 合并项目覆盖 → 广播 settings-changed
// 每个被接受的事件同时旁路广播 files-changed{paths, structural}
//   structural=true 仅限增/删/重命名（前端据此重建文件树）；内容修改为 false（跳过）
// 结果经 WatchSink 回调送达 src-tauri，本 crate 不认识 Tauri（ADR-0010）

// compose.rs（core，纯函数）：快照进、请求出
pub struct ComposeContext<'a> { pub project: &'a ProjectState, pub settings: &'a Settings }

pub fn compile_request_for_change(ctx: ComposeContext<'_>, changed: &Path) -> Option<CompileRequest>;
//   触发条件全部收敛在此：已确定根文件 + changed 在项目根内 + is_compile_trigger 为真；否则 None（不编译）
//     —— 不触发的**原因**逐条 debug 打出（㉛）：无根文件 / 项目外 / 被排除（tmp、隐藏项、无扩展名、自己的产物）
//   强度 = Quick（编辑期两趟出图；目录/引用页码可能落后一趟，由首编与空闲收敛兜底）
pub fn compile_request_manual(ctx: ComposeContext<'_>) -> Option<CompileRequest>;
//   手动「编译」与前端空闲收敛共用；强度 = Full（Tectonic 自带收敛 + bibtex 趟）
//   两者都只从 ctx 取**一次性快照拷贝**（timeout / root_file）：请求发出后与触发源解耦
```

**设计决策 D3（翻译层）**：scheduler 只认识 `CompileRequest`，不认识文件、项目、设置。文件事件 → 请求的翻译在组合层完成。否决"watch 直连 scheduler 传路径"：scheduler 被迫依赖项目状态与设置，违背最小外部依赖。手动编译 `compile_now` 走同一组合函数——**所有触发源收敛到同一个入口**。

**触发面：任何带扩展名的非忽略文件**（roadmap ㉜ 已落地，判定在 core `is_compile_trigger`，见 §3.3）——它修掉的正是两条实测偏差：

- **改 `.bib`/图片等真输入 → 完全不触发**：实测 `\bibliography` 引用的 `refs.bib` 改动后，`tmp/` 全部产物 mtime 纹丝不动（用户以为改了没用）；
- **改未被引用的 `.tex` → 白白触发一次全量编译**：实测 `main.aux` 被重写；一次白编译 = 小档 ~1s / 真实论文档 >120s。**精确失效仍未做**，记为 roadmap ㉝。

判据**原本已经在磁盘上**（零新增依赖）：`tmp/<stem>.fls`（引擎**实际打开**的文件；latexmk 默认开 `-recorder`）与 `tmp/<stem>.fdb_latexmk`（latexmk 依赖图，含 mtime/size/md5 与 bibtex 等子步骤）——**`.bib` 只在后者**。⚠ **ADR-0015 之后这两份产物都不再生成**（内嵌形态不经 latexmk、也不写 `-recorder`）⇒ ㉝ 的现成判据随之失效，要重做需另找通道（**未接线、未定方案**）。历史工具 `node scripts/fls-report.mjs` 只对 latexmk 时代的产物有效；结论与保守原则见 [G1 报告](../archive/research/g1-read-interception-feasibility.md)。

**信息局部性**：组合层每次构造请求都取**快照拷贝**，不持有任何引用；请求发出后与触发源完全解耦。

## 8. 命令面实现（src-tauri commands.rs）

全部命令：DTO 进出、无业务逻辑；路径类参数一律**项目根内校验**（core `project::paths`：canonicalize + 根内前缀检查，防任意路径读写——自建命令没有 Tauri 权限模型兜底，这是安全底线）。文件与进程调用一律经 core trait，命令面不出现 `tokio::fs` / `std::fs` / `std::process`（ADR-0010）。

| 命令 | 实现要点（算法） |
|---|---|
| open_project(folder) | 校验目录 → 读**纯全局**设置（`load_global`，不是上一个项目的合并结果）→ 加载项目覆盖 → 探测根文件（有 root_file 覆盖则按 D8 校验后采用）→ 更新项目状态 → 返回 ProjectInfo（**含 `root_candidates`**）；探测为 Multiple → 前端弹窗后 update_settings 补 root_file |
| get_project | 只读返回当前 ProjectInfo：root_file 变化后前端重新同步用；`root_file` 已指定 → 候选为空，否则重新探测（与 open_project 同语义） |
| list_dir(path) | 递归 `collect_tex_files` 变体（返回全树 DirEntryInfo，含目录；前端防抖重建用） |
| read_file | `FileSystem::read_to_string` + 路径校验（core `project::paths`） |
| save_all | 写盘（`save_content`）——不触发编译逻辑，watch 自然驱动；唯一保存路径 |
| create_dir | 新建**单层**目录（roadmap ㊺ §6.13.2）：`resolve_creatable_in_project`（D8；父目录必须已存在）+ `FileSystem::create_dir`；已存在 ⇒ `Invalid`（"已存在同名文件或文件夹：<项目内相对路径>"，**不静默成功**）|
| delete_path | 删除文件或**整个目录**（roadmap ㊼）：三道闸 —— `resolve_in_project` 必须**已存在且在项目根内**、**不能是项目根**；目录走 `remove_dir_all`（递归）。二次确认与编辑器收口（关标签、根文件被删则重探）在前端 |
| rename_path(from, to) | 改名（roadmap ㊽）：`from` 必须已存在且在根内、不能是项目根；`to` 必须与 `from` **同目录**、落在根内、且**不存在**（不覆盖 ⇒ `Invalid`）；返回**改后的绝对路径**（前端据此 `editor.remapPaths` 并让根文件跟进）|
| new_file_skeleton(lang, kind, title) | 新建 `.tex` 的**最小骨架**（roadmap ㊺ §6.13.1-A）：转发 core `newfile::document_skeleton`，**无副作用、不读项目状态**（纯函数 ⇒ 前端可在向导里逐次改选项做实时预览）。「语言 × 类型」这张表只此一份，前端只传选择值（`chinese/english` × `article/report/book/beamer`） |
| compile_now | compose.compile_request_manual：只看 root_file（忽略活动文件路径），构造请求入队 |
| abort_compile | scheduler.send(Abort) |
| synctex_forward / inverse | 调 provider（失败按 100/200/300ms 退避重试）；`inverse` 输出 `InverseResultDto{source,note}`：命中生成产物/项目外文件时就近回落，落空则只给提示（㉒） |
| get_settings / update_settings | 读快照 / apply_patch → 校验 → 写盘（记录 hash，供 watch 自写盘过滤）→ 广播 settings-changed。**root_file 分支**：`Some(rel)` 先按 D8 解析为项目内绝对路径（失败即拒绝、**不落盘**）；`null` → 回到自动探测（复用 `detect_root`）；随后**同步内存 `ProjectState.root_file`**——漏掉这一步的症状是「选了根文件仍报未确定根文件，必须重开项目」 |
| open_licenses(kind) | 打开随安装包分发的许可材料（③段 T21 的 About 入口）：`kind ∈ notices \| sources`（`notices` = `THIRD-PARTY-NOTICES.md`；`sources` = GPL 字体 Corresponding Source 目录）。**只读、不接受路径**：目标由 `resource_dir()/licenses/` + 固定文件名算出（debug 构建回退 `src-tauri/resources/licenses/`）；`.md` 无关联程序时退回"在文件管理器中定位"，缺失/未知 kind 给可行动错误，不静默失败。真机点击 `[未测-环境限制]`（WebView2 不渲染），数据面由 bindings 13/13 + vitest 覆盖 |

> **已删除的命令**：`list_engines`（引擎清单）、`probe::find_in_path` / `EngineInfo` 类型随 ADR-0015 ①段一起删除；`lib_form_available`（是否编入库形态）、`engine_form` 命令与 `EngineForm` / `EngineFormDto` 类型随**②段（内嵌唯一）**删除——没有形态可选，就没有"形态可用性"与"形态位"这两个命令。`LATTESET_TEX_ENGINES` 与 `LATTESET_TECTONIC_LIB` 均已删除——启动时检测到会各打一条 warn「已删除，忽略」（`latteset_infra::warn_removed_env_overrides`，幂等）。当前命令面共 **19 条**（+③段 `open_licenses`；`src/bindings.ts` 与 Rust 侧一致，T19 bindings 核对 13/13 PASS、命令 19 条）。

### 8.1 无 GUI 交互层（crates/latteset-server，roadmap ⑥）

与命令面**并列的第二个入口**：CLI 与 MCP 都不经 Tauri，直接复用 core + infra（ADR-0010 的红利）。使用者是 harness / Agent，形态是「一条命令跑一次、拿一个 JSON」。用法、工具表、实测证据与偏差见 [cli-mcp-plan.md](../archive/cli-mcp-plan.md)（本文件只写实现契约）。

```
crates/latteset-server
├── lib.rs              Session：项目会话（打开 / 编译 / 大纲 / 文件 / SyncTeX / 设置 / **公式预览**）
├── mcp.rs              MCP（stdio JSON-RPC：initialize / tools/list / tools/call / ping）
└── bin/{latteset-cli,latteset-mcp}.rs
```

| 部件 | 实现要点 |
|---|---|
| `Session::open_project` | 与 `open_project` 命令同路径：项目覆盖 + `detect_root` + `ProjectState`；根文件未定时**报候选而不是猜** |
| `Session::compile(quick)` | 直接构造 `TectonicLibRunner`（内嵌 Tectonic 是唯一形态）跑一趟 → `CompileReport{status, failure, errors, pdf_path, elapsed_ms, engine, root_file, kind, upgraded_from_quick}`；**`engine` 字段保留、值恒为 `"tectonic"`**（既有 JSON 契约不破）。**不走 scheduler**（无合并队列/防抖/watch）；超时与取消走**协作式取消**（内嵌形态没有进程可树杀）。`upgraded_from_quick` = 「请求 Quick 但上报 Full」的**唯一判据**：首编无产物的请求会被预算判成 full、或预算内收敛按实际结果报 full（§2.7） |
| 会话内快照 | `status` / `errors` 返回**本进程最近一次**结果（`Session` 字段持有）。*不是* GUI 状态的镜像：CLI 是独立进程，读不到 GUI 内存（偏差见 docs/archive/cli-mcp-plan.md §4） |
| `outline` / `tree` / `read` / `write` | 调 core：`outline::load`（与 GUI 同源）、`project::scan`、`FileSystem`。`write` **不代建父目录**（与 GUI `save_all` 同契约），错误消息直接说清 |
| SyncTeX | 调 `core::synctex::{pdf_path_for_root, resolve_inverse}`——与 GUI 命令面同一份实现（㉒ 的生成产物回落一起继承），不会漂移 |
| `settings` | 读同一份全局设置（默认 Tauri `app_config_dir`，`LATTESET_CONFIG_DIR` / `--config-dir` 可覆盖）→ **CLI/MCP 与 GUI 共享设置**，Agent 看到的 mode/timeout/tectonic/**notices** 就是 GUI 会用的（引擎字段已不存在） |
| MCP 协议面 | `mcp.rs` 手写最小 JSON-RPC 子集：11 个 tool，`initialize` 回显受支持版本，未知方法回 `-32601`，**工具内错误按 MCP 约定回 `isError: true` + `structuredContent{code,message}`**（Agent 读得到"为什么失败"，不是裸协议错误） |
| 退出码 | `0` 成功（`compile` 仅当 `status == "success"`）/ `1` 编译未通过 / `2` 用法·路径类 / `3` 内部错误；JSON 内同时有 `{ok, error:{code,message}}` |
| 日志 | 走 stderr（stdout 留给 JSON），**级别走 `RUST_LOG`（默认 `info`）**，与 `examples/`、GUI 同口径。⚠ 内嵌引擎的冷编译诊断全靠 `RUST_LOG=info,latteset_tectonic=debug`（趟数决策、"钱花在哪一趟"、页进度、流式出图落帧），所以两个二进制都用 `EnvFilter` 而不是 `with_max_level(INFO)`（`tracing-subscriber` 走工作区依赖并开 `env-filter`，仍是同一个 crate、不新增依赖） |
| 依赖约束 | CLI 解析手写（不用 clap）⇒ 新增第三方依赖为零；但引擎是**非可选内嵌依赖**，构建需要 vcpkg（不再"离线可构建"） |

**并发契约**：headless 与 GUI **不得同时编译同一项目**（两路内嵌引擎抢同一个 `tmp/`）。一期约定 headless 独占，不做项目锁；GUI 那条路目前只有 settings.json 有自写盘过滤，`.tex` 没有。

#### 8.1.1 验证入口

| 要验的东西 | 入口 |
|---|---|
| Session 逻辑（打开 / 读写 / 树 / 大纲 / 非 UTF-8 / 编译拒绝 / 配置目录 / D4 资源优先级） | `cargo test -p latteset-server`（lib **12 passed / 4 ignored** + cli **8 passed**，默认不跑真编译；T19 实测） |
| 真实编译（multifile 三轮：成功 → 失败诊断 → 修好；闸门 = **编进来 + 有可用 bundle**，不再探测 PATH 上的 `tectonic`） | `cargo test -p latteset-server -- --ignored`（lib **4 个真编全过** + `mcp_stdio` 1 个真编，T19 实测） |
| MCP 协议链路（**真实二进制 + stdio 管道**：握手 / tools.list / 工具往返 / 错误语义 / 切换项目） | `cargo test -p latteset-server --test mcp_stdio`（**5 passed + 1 个 `#[ignore]`** 编译用例） |
| 端到端人工闭环（open → 改 → 编 → 验） | 内嵌形态已由 T9/T19 用 `latteset-cli` 覆盖：U-14 中文/子目录、Quick/Full、内容错误归属、随包断网首编、biber/bibtex 对照、`validate-pdf.mjs`（证据 `test_file/phase2/REPORT-gates.md` §1、`test_file/phase3/REPORT-gates.md` §6） |

## 9. 前端模块（函数级）

### 9.1 services（唯一碰 IPC 的层）

**另有四份纯函数工具**（roadmap ㊿ 收敛，改前是各处手写一份；都是纯函数、可单测）：

| 文件 | 职责 | 为什么独立出来 |
|---|---|---|
| `paths.ts` | `toSlashes` / `normalizePath` / `isUnder` / `samePath` / `basename` / `relativize` / `remapUnder` | 分隔符归一与"在项目内"判定原先手写 **10+ 处**，漏一处就是**静默错**（标签指向不存在的文件、改名后编辑被当外部修改、根文件覆盖写成绝对路径）。`relativize` 对项目根**外**的路径返回**空串**（返回绝对路径会被后端拒、用户看不懂原因）——调用方必须处理空串 |
| `errors.ts` | `errorText(e)` | 原先三份不等价实现 ⇒ 同一类失败在不同位置显示成 `[object Object]` 或空串 |
| `fuzzy.ts` | 文件树搜索的模糊匹配（子串优先、子序列兜底） | 纯函数 + 6 例单测（roadmap ㊾） |
| `draftPatch.ts` | 草案层：LaTeX → 近似排版文本 | 纯函数（实时预览 v1） |

`src/bindings.ts` 是**生成物**（调试构建启动时由 specta 覆盖，见 `src-tauri/src/lib.rs`），**禁止手改**。

```ts
// ipc.ts —— specta 生成 + 薄封装（类型全自动，本文件不手写任何 DTO）
export const ipc = { openProject, listDir, readFile, saveAll,
                     compileNow, abortCompile, synctexForward, synctexInverse,
                     getSettings, updateSettings, engineForm, libFormAvailable }  // 均为 Promise<T>

// events.ts —— 订阅一次，分发到各 store（内部经 useXxxStore() 取实例）；返回取消函数
export function subscribeEvents(): () => void
// 映射表（单向：事件 → store 动作）：
//   compile-status   → compileStore.setStatus
//   compile-progress → compileStore.setProgress（中间态：仅 running 时采纳，只增不减）
//   compile-errors   → compileStore.setLiveErrors（中间态：仅 running 时采纳）
//   errors-updated   → compileStore.setErrors（**权威终态**，无条件写入）
//   compile-notice   → noticesStore.push（**运行时**能力边界提示：非持久、不落盘；
//                       状态栏 chip，用户点击清空。不进 errors——编译仍可能成功）
//   pdf-updated    → previewStore.onPdfUpdated（含 changed_pages/pages：可能**不重载**）
//   compile-preview → previewStore.onCompilePreview（中间态：仅 running 时采纳；roadmap ㉞ 编译中预览）
//   files-changed  → editorStore.onFilesChanged(paths)（过滤+重载判定）
//                     projectStore.refreshTreeDebounced(structural)（300ms 防抖；仅结构变化重建）
//   settings-changed → settingsStore.setSettings
```

> **两套提示不要混**：`Settings.notices`（持久提示，后端落盘、状态栏 chip、点击发 `dismiss_notice`）与 `compile-notice`（运行时提示，`noticesStore` 内存态、点击清空）。两者都**不进错误列表**——成功态会清空 `errors`，把提示塞进去等于让它消失。

### 9.2 stores 与依赖方向

```
依赖单向，禁止反向：
settingsStore ← projectStore（读设置）← editorStore（读设置/项目）
settingsStore ← compileStore
events.ts 是唯一"写"多个 store 的地方（订阅分发）
useAutoSave 依赖 editorStore.dirty + settingsStore（读）
```

| store | 状态（模块内） | 动作 |
|---|---|---|
| projectStore | project、rootFile、fileTree | openProject、refreshTree、refreshTreeDebounced、resolvePath、relativizePath（绝对 → 项目内相对路径，`update_settings` 唯一可接受的形态）、syncProject（经 `get_project` 重新同步）、**rescanRoot**（无根文档时的静默重探测，㊺ §6.13.1-B） |
| editorStore | openTabs[]、activePath、dirtyPaths:Set、lastSaved:Map<path,time>、buffers、externalConflict | openFile、closeTab、markDirty、markSaved、saveAll、onFilesChanged、acceptExternal |
| compileStore | phase、kind、draft、errors[]、pages | setStatus、setProgress、setLiveErrors、setErrors |
| previewStore | pdfPath、reloadKey、highlight、syncNote、changedPages、pagesTotal、skippedReloads、**livePreview**（编译中的部分 PDF）、**docIdentity / sourcePath / displayPath**（派生：身份 / 字节来源 / 可显示） | onPdfUpdated（**页哈希全同则不递增 reloadKey**；但屏幕上是部分 PDF 时必须重载一次）、onCompilePreview（帧版本号 = `pages`，只增不减 ⇒ 旧帧丢弃）、clearLivePreview（编译终态收口；返回"刚才是否在预览"）、setHighlight、setSyncNote |
| settingsStore | settings | setSettings、updateSettings |

**前端自保存过滤算法（editorStore.onFilesChanged）**：入参 paths 中，`lastSaved` 里存在且时间近（< 2s）的路径判定为"自己刚保存"→ 忽略；其余 → 已打开且不脏 → 重载内容；已打开且脏 → **磁盘与缓冲一致则什么都不做，不一致才**保留 + 状态栏提示；未打开 → 忽略（文件树自会刷新）。`lastSaved` 是 editorStore 模块内状态，不进任何函数参数。

**并发与生命周期不变量**（异步读盘/渲染下的守卫，去掉即复发）：

- `editorStore.openFile`：去重判断放在 `await readFile` **之后复检**——并发的树节点双击不会开出两个标签；
- `editorStore.onFilesChanged`：读盘后**复检 `dirty`**，但结论只由**内容**给——磁盘与缓冲一致（含"上一条事件的重载刚写好 buffer"这种情况）就不算冲突；不一致才保留本地内容 + 冲突标记，不覆盖用户最新输入；
- `compileStore.setStatus("success")` 清空 `errors`/`hasError`——无 `running` 前置时旧错误不残留；
- **中间态只在运行中采纳**：`setProgress` / `setLiveErrors` 都以 `phase === "running"` 为前置（前者只增不减，后者不碰 `hasError`）——收尾期补发的中间态可能**晚于终态**抵达，缺了守卫就会顶掉权威列表（真机实测：超时诊断 1 条被 30 条中间态覆盖）；
- `EditorPane` 的 Monaco 事件订阅逐个收集并随卸载 dispose；`PreviewPane` 带 `unmounted` 守卫（在途 load 不回写插桩与标题、catch 不误报）、卸载时 `cancelAllRenders()`、`onCanvasClick` 包 try/catch（卸载期间点击不产生未处理 rejection）。

### 9.3 composables

```ts
// useAutoSave：防抖保存（连续模式）
// 输入事件：Monaco onDidChangeModelContent（仅当前活动文件）
// 状态：timer（composable 内）——信息局部性：计时器不出 composable
// 算法：每次变更 → 重置 500ms 计时器 → 到点 → saveAll(dirtyPaths) → 成功才清 dirty
//   清脏规则：只对「缓冲区仍等于已保存内容」的路径清脏；保存期间又变化的路径保持 dirty 并重排保存
//   （否则关闭时的 flush() 会因 dirty 为空丢掉最新输入）
// 触发时机：仅 mode === "continuous" 自动写盘；on_save 模式不自动写盘，
//   由 Ctrl+S / 点「编译」/ 关标签触发 flush()（写盘后经 watch 触发编译）
// 取消：组件卸载时 clearTimeout（防泄漏）
// 无根文档兜底（㊺ §6.13.1-B）：保存成功后若 project.root_file 仍为空 → projectStore.rescanRoot()
//   （复用 open_project 那套探测 ⇒ 口径一致；探到即撤状态栏标签并补一次首编；全程静默，失败只记 console）

// projectStore.rescanRoot：无根文档时的静默重探测（㊺ §6.13.1-B）
// 触达点两处：useAutoSave 保存成功后、FileTree 新建文件落盘后（同一份口径，不各写一遍）
// 算法：root_file 已有 → 直接返回；否则 openProject(root) 重跑 core 探测 → 探到 ⇒ compile_now 补一次首编
// 为什么补编译：watch 那条触发链在这个变化**之前**只能看到 root_file=None ⇒ 它已经把这个变化丢掉了，
//   不补的话用户得再敲一个字或手动点「编译」才会出 PDF（真机日志：不触发编译：尚未确定根文件 → 探到 → 手动编译）

// useIdleConvergence：空闲收敛（roadmap ㉘）
// 进入条件：编译成功且是草稿（draft=true，屏幕上的 PDF 目录/引用页码可能落后一趟）
// 算法：2000ms 无编辑 → compile_now（Full）把页码追上；成功且为 Full 后提示才清除
// 取消：任何编辑或新编译都取消待收敛——收敛的 Full 会占住调度器（大项目 ≈4s），
//   把紧随其后的编辑态 Quick 堵在队列里，反而拖慢「编辑→出图」
```

### 9.4 组件数据流

| 组件 | 输入（props/事件） | 输出（emit） | 模块内信息 |
|---|---|---|---|
| EditorPane | model(路径)、内容、语言 | 变更事件 → useAutoSave | Monaco 实例、worker、IME 组合状态；**无活动文件 → 显示"还没有打开文件"占位提示**（覆盖 Monaco）+ `readOnly`，打开文件才可编辑 |
| PreviewPane | pdfPath、highlight、syncNote、**livePreview（编译中的部分 PDF）** | 点击坐标 → useSyncTex | pdf.js 文档句柄、滚动位置缓存、canvas 代次与页高（见下方渲染契约，含"编译中预览 = 换字节不换身份"） |
| FileTree | 树数据、激活路径 | 打开文件/目录展开、右键菜单（新建/改名）、悬停垃圾桶（删除）、搜索 | 展开状态（只在前端本地）+ `expandPath`/`renamingPath`/`forceExpand`（三个**一次性请求**，递归透传：建完展开、行内改名、搜索强制展开）；**新建（文件 / 文件夹）**（㊺）：标题栏**一个** `＋` → 行内输入 + 类型小切换（📄 默认 / 📁，点它不会收掉输入行）→ `save_all` / `create_dir`；**删除**（㊼）：悬停垃圾桶 → `ConfirmDialog` 二次确认（目录报"里面还有 N 个文件"）→ `delete_path` → `closeTabsUnder` + 刷新；**改名**（㊽）：右键 → 行内编辑 → `rename_path` → `remapPaths` + 刷新；**搜索**（㊾）：`services/fuzzy.ts` 过滤树（保留祖先与整棵子树）；**新建向导**只在「无根文件 ∧ 无候选 ∧ `.tex` ∧ 开关为开」时拦一次 |
| FileTreeItem | node、expandPath、renamingPath、forceExpand | `menu`（右键）、`remove`（垃圾桶）、`rename`/`renameCancel` | 展开状态（组件内，信息局部性）；改名态是行内输入框（**只选中主名**，不含扩展名）；垃圾桶默认 `opacity:0`（**仍占位**，避免悬停时名字省略号跳宽）|
| ConfirmDialog | title、lines[]、confirmLabel、busy、error | `confirm` / `cancel` | 无状态（通用二次确认：先说要动什么、再说后果；`Esc`/点蒙层 = 取消）|
| NewFileWizard | fileName、initialTitle、busy、error | `confirm({lang,kind,title})` / `cancel` | 语言 / 类型 / 标题三项本地状态 + **骨架实时预览**（每次改动调 `new_file_skeleton` 拿 core 的产物 ⇒ 预览与落盘不可能不一致）。Esc / 点蒙层 / 取消 = **不产生任何文件** |
| RootFilePicker | root、candidates（探测候选）、fallbackFiles（零候选时全部 .tex）、busy、error | `select`（**项目内相对路径**）、`close` | 无（纯展示；相对路径由 `relativizePath` 计算） |
| ErrorList | errors[] | 点击条目 → openFile+定位 | 无；**诊断展示**（roadmap ④）：条目带 `diagnosis` 时渲染两行（原因 + 建议），无诊断降级为原文首行；头部「已诊断 N」。**去重/截断**：同源（文件 + 首行消息相同）聚合为一条并显示 `×N`；不同源最多展示 `MAX_DISPLAY=30` 组，超出提示隐藏数量（错误雪崩时不刷屏） |
| StatusBar | compileStore/editorStore/projectStore 只读投影 + 「未确定根文件」可点击入口（emit `pick-root`）+ 草稿期「引用待更新」+ 编译中「已排版 N 页」 | `pick-root` → App 打开根文件选择器 | 无（`queued/running/failed` 不改「引用待更新」标记——屏幕上的 PDF 仍是旧的，失败不产出新 PDF；「已排版 N 页」只在 `running && pages > 0` 时显示） |

**布局（App.vue）**：左栏文件树与大纲**上下分布**（`SplitPane direction="horizontal"`，比例 0.55）；底部面板默认折叠成约 30px 细条（头部「报告 · 展开」，点击展开/收起；**只在编译失败时**多显示「● N 个错误」——就绪/排版中… 归状态栏 phase chip，同源不重复渲染），展开后错误列表占满底部宽度。分割器自研（不引入 vue-code-layout，多面板布局后置），`.split-pane.vertical` 显式写规则，不依赖默认 flex 行为。

**窗口顶部的两条带子**：目前最上方是**系统标题栏**（Windows 原生，颜色由 `apply_window_theme` 的 DWM 沉浸式深色跟随主题），它下面才是我们自己的工具栏（品牌/打开项目/编译/终止/设置 + 项目路径）。两条带子并存是当前的结构现状；**把三个系统按钮搬进我们的顶栏（合成一条）已立项为 roadmap ㉟**（`decorations: false` + 自绘顶栏，含技术清单与验收判据）。

**PreviewPane 渲染契约**（滚动/缩放正确性的前提，改这里必须连改本清单）：

- **分页 DOM 虚拟化**：只挂载视口窗口内的页（`mountStart..mountEnd`，前后各 `PAGE_WINDOW=6`），顶部/底部占位撑住总高度；`renderNearViewport` / `updateCurrentPage` 只遍历窗口内页 → 复杂度 O(视口)，不是 O(总页数)。滚动驱动 + 窗口变化 watcher + 容器 `ResizeObserver`（不用 IntersectionObserver）。
- **canvas 代次**：`structuralEpoch` **仅在缩放 / 换文档时递增**（重建 canvas DOM）；同文件内容重载**复用 DOM**（`doRenderPage` 每次 `canvas.width=` 即重置 2D context），`pageH1` 保留 → 滚动恢复精确。
- **编译中预览 = 换字节不换身份**（roadmap ㉞）：`load()` 把"**身份**"（`preview.docIdentity`，决定 DOM 复用/页高重置）与"**字节来源**"（`preview.sourcePath`，编译中 = `tmp/<stem>.preview.pdf`）**分开**——部分 PDF 是另一个文件，但它属于同一篇文档。只让来源跟着帧走，换帧就不会走"换文档"分支。
  - **复用必须关掉**（`canReuse` 加 `!isPreviewLoad && !lastLoadWasPreview`）：部分 PDF 的页字节来自**中间趟**，而 `changed_pages` 是相对**上一份权威 PDF** 算的 ⇒ 混用会把中间趟的旧页留在屏幕上。故"预览过"要影响**下一轮**（权威产物落地那轮）也只能全量重绘。
  - **收口**：`compile-status` 终态（成功/失败/中止）与权威 `pdf-updated` 都会 `clearLivePreview()`；失败/中止时不会有 `pdf-updated`，全靠前者把屏幕拉回权威视图（否则中间态会留着冒充结果）。首编无权威产物时身份由部分 PDF 兜底，权威到达那一次走一次"换文档"（与今天首次出 PDF 同款）。
  - 真机读数（`bench/large` 125 页冷编，debug）：3 次加载 85/79/78 ms；**14 个页 DOM 节点跨三次加载全部存活**（无重建）、滚动 11839 帧 `min=max`（零漂移）、**预览帧 0 long task**；中止后 **106 ms** 退回权威 PDF。
  - 可见性：工具栏「编译中预览 · N 页」只在**真的收到帧**时出现（子进程档与短编译不出现 ⇒ 与今天行为一致），它同时就是这条能力的"能力位"。
- **只重绘变化页**（[incremental-edit-x-dvi.md](../archive/research/incremental-edit-x-dvi.md) 功能点 C）：`load()` 不再无条件 `renderedScale.clear()`——**同文件 + 页数一致 + 有变化页集合**时只让变化页失效，其余页沿用旧 canvas（`renderPage` 的 `renderedScale.get(n) === scale` 闸门自然跳过重绘）。安全依据：XDV 页哈希相同 ⇒ 页字节等价 ⇒ 页尺寸与内容都不变。其余情形（换文档 / `pages == 0` 无法判定 / 页数变了）仍全部重绘。实测：74 页文档改末章 → `pagesRendered 7→0`、`pagesReused 7`、render 102ms→7ms。
- **页高是布局的唯一驱动**：`.page-wrap` 高度绑定 `pageH1[n]×scale`（`pageWrapHeight(n)`），不依赖 canvas 尺寸。两类复发路径：① 释放/重建把 canvas 置 0×0 时页-wrap 塌缩到只剩边距 → `scrollHeight` 变短（滚动条拖不到真实末尾），且 `renderNearViewport` 用 `getBoundingClientRect`（0 高）把近页误判为远页而释放（PDF 消失）；② `pageH1`/`prefixH1` 是普通数组（非响应式），页高变化必须靠 `layoutRev` 自增让相关 computed 重算。`.page-wrap canvas { display:block }` 消除内联 canvas 在强制高度下的基线缝隙。
- **跳转先预热页高**：`renderPage` 返回渲染链 promise（`await` 真正完成 + `setHeight` 记录页高），跳转前预热目标页及之前页高并 `nextTick`，用瞬间定位（`behavior:"auto"`）——平滑滚动会在中途布局变化下跳不到位。页码输入与 SyncTeX 正向共用 `goToPage(n)`（展开窗口 + 预热 + 渲染 + 居中）。
- 插桩：`window.__previewLastReload` + 控制台 `[preview] reload#N`（真机验收清单第 6 项读它）；耗时基线见 [design.md](../design.md) §预览。**`pagesReused`** 与 `pagesRendered` 并列——"复用了多少页"是功能点 C 的判据。
- **草案层（实时预览 v1）**：编辑缓冲变更 → **30ms 去抖** → 用"上次成功编译后建立的 **PDF 文本行索引**"（`buildLineIndex`：逐页 `getTextContent()` → `linesOfPage()` 按基线聚行）定位**改动前那一行**（`firstChangedLine()` 找首处 diff + `findAnchor()` 匹配）→ 在该页 overlay canvas（`.draft-layer`，`pointer-events:none`）上**白底重绘新行**并画草稿下划线；`pdf-updated`（重载或跳过重载）时清草案并刷新 diff 基线 `compiledText`。
  **失败一律静默**（无 diff / 找不到锚点 / 无可画内容 / 索引未就绪 ⇒ 不画、不报错、不阻塞输入）；改动行会先经 **`latexToDraftText()` 近似成"排版结果的样子"**（去数学定界符与 `\begin/\end/label`、`\frac`→`(a)/(b)`、`\sqrt`→`√`、常见命令→Unicode、上下标尽量映射；**纯标记行返回空 ⇒ 不画**）——直接把源码画上去会把公式显示成 `$E = mc^2$` 这种**代码状态**（真机反馈的缺陷）；纯逻辑在 `src/services/draftPatch.ts`（单测 `src/services/__tests__/draftPatch.spec.ts`），插桩 `window.__lattesetDraftMs`（改动→草案可见毫秒）与 `__lattesetDraftDbg`（含 bail 原因，如 `bail:no-drawable-text`）。
  **真机实测**（28 页中文夹具、索引 594 行）：改动→草案可见 **31–34 ms**；索引构建 **164–173 ms**（编译成功后后台做）；草案存活窗口 = 到下次 `pdf-updated`（约 1.5–3 s）。**已知限制（v1）**：只画**第一处**改动的行；新增行（旧行为空）无锚点；跨行粘贴/重排不模拟；未做字体匹配（用系统字体栈，字形与最终排版不同）。口径与成本实测见 [realtime-preview-cost.md](../archive/research/realtime-preview-cost.md) §6。

### 9.5 编辑器语言特性（latexSuggest.ts / texParse.ts）

```ts
// latexSuggest.ts —— monaco 语言注册后调 registerLatexProvider()
// registerCompletionItemProvider：InsertAsSnippet，triggerCharacter 只有 '\'（不含 '{'——带参数处罗列全部片段是噪音）
// registerFoldingRangeProvider：按 \begin{env}…\end{env}（含嵌套）生成 FoldingRange
// 片段覆盖：文档骨架 / 环境（begin-end 名同步）/ 章节 / 数学 / 格式 / 文件操作，Tab 展开

// texParse.ts —— LaTeX 行级剥离，纯函数、不依赖 Monaco（可单测）
stripTexComment(line): string;                       // 折叠与大纲共用
shouldStripLeadBackslash(line, startColumn): boolean; // 补全专用启发式
```

**不变量（改回去即复发）**：

- **片段体写单反斜杠**：Monaco `SnippetParser` 只把 `\$` / `\}` / `\\` 当转义，其余 `\x` 保留字面 `\`，故 TS 里 `"\\begin"` 插入即得 `\begin`。**例外**：`\` 紧贴 `${占位符}` 前（`\newcommand{\${1:cmd}}`）会被当成 `\$` 转义 → 写 `\\${1:cmd}`；LaTeX 行换行 `\\` 在片段里写 `\\\\`。
- **补全项的 `range` 必须等于当前词范围**（`getWordUntilPosition`），否则 Monaco **过滤掉该建议**、列表空白。Monaco 的 word 不含 `\`（`\sec` → word=`sec`、startColumn 指向 `s`），所以做法是「词首前一字符是 `\` 时剥掉片段自带的前导 `\`」，而不是把 range 前延——后者触发上面那条过滤。
- **`FoldingRange.start/end` 是 1-based**（Monaco 行 API 均为 1-based）；传 0-based 会让折叠锚点整体上移一行（箭头落到 `\begin` 前一行、折叠后露出 `\end`）。
- **`stripTexComment` 语义**：`\verb` 跨度内与转义 `\%` 都不算注释；判 `%` 是否注释看其前**连续反斜杠的奇偶**（奇数 = 字面 `\%`，偶数含 0 = 注释截断）——单字符 lookbehind 会把 `\\%`（换行紧接注释）误判为字面。
- 剥离语义必须与 §3.5 大纲一致：注释里的 `\begin{}`/`\end{}` 不生成伪折叠，`正文 % \section{隐藏}` 不生成伪大纲项。
- `multiCursorModifier: 'alt'`（**必须**：设为 ctrlCmd 会与 SyncTeX 的 Ctrl+点击冲突）。
- `shouldStripLeadBackslash` 依赖「Monaco word 不含 `\`」；若改 `wordPattern` 需同步（`__tests__/texParse.spec.ts` 锁定）。

## 10. 通信契约总表（冻结）

**事件（tauri emit → TS 类型，specta 生成）**：

```ts
compile-status: { phase: 'queued'|'running'|'success'|'failed',
                  kind?: 'timeout'|'content_error'|'aborted', draft: boolean }
                                                   // draft=true：屏幕上的 PDF 是草稿（引用/目录可能落后一趟）
errors-updated: ErrorEntry[]                       // 失败时携带；编译启动时清空由前端 setStatus('running') 触发
                                                   // ErrorEntry = { message, file, line, kind, diagnosis }
                                                   // diagnosis = { kind, cause, hint, suggested_timeout_secs } | null
compile-progress: { pages: number }                // **中间态**（阶段 2）：编译中已排版页数，只升不降；仅 running 时采纳
                                                   // 内嵌形态的来源 = 每趟排版收尾从捕获的 `<stem>.xdv` 数完整页（趟边界，不是 stdout `[N]`）
compile-errors:  ErrorEntry[]                      // **中间态**（阶段 2）：编译中解析到的致命错误（不含 Overfull 等警告）；仅 running 时采纳
                                                   // 中间态与终态**刻意分开**：收尾补发可能晚于终态抵达，共用事件名会顶掉权威列表（§2.6.1）
compile-preview: { path: string, pages: number }   // **中间态**（roadmap ㉞ 流式出图）：编译中把**已完成页**缝成的部分 PDF
                                                   // path 恒为 tmp/<stem>.preview.pdf（同一路径被**原子替换**成新内容）；pages 只增不减 ⇒ 拿它丢弃晚到的旧帧
                                                   // 内嵌唯一后**恒可用**（XDV 在捕获表里才能合成部分 PDF；不再有"另一形态不发"的口径）。真机时间线：
                                                   //   status(running) → preview 121 页 → preview 125 页 → pdf-updated(125 页) → status(success)
                                                   // ⚠ 它不是权威产物，也**不是**"当前文档"：路径与权威 PDF 不同，按"换文档"处理会重建 DOM、丢滚动位置
compile-notice:  { message: string }               // **运行时**能力边界提示（ADR-0015 决策 4）：内嵌形态不跑外部工具（biber / makeindex / glossaries）
                                                   // 检出即发（编译仍可能成功）；状态栏 chip，点击清空，**不进 errors**。与 Settings.notices 分开
pdf-updated:    { path: string, changed_pages: number[], pages: number }
                                                   // 页级差异（incremental-edit-x-dvi.md 的 B/C）：
                                                   //   pages>0 且 changed_pages 空 → 逐页未变 → 前端**跳过重载**；
                                                   //   pages>0 且非空 → 只重绘这些页（其余复用 canvas）；
                                                   //   pages==0 → 无法判定（XDV 缺失/损坏）→ 全量刷新
                                                   // 来源：内嵌 runner 用捕获的 XDV 算页哈希（tmp/<stem>.tectonic.pages 缓存）——内嵌唯一后恒可用；
                                                   //   pages==0 分支仍可达（XDV 缺失/损坏），不能当死代码删
files-changed:  { paths: string[], structural: boolean }   // structural=true 仅增/删/重命名（文件树重建）；内容修改为 false（跳过）
settings-changed: Settings
```

**命令（invoke）**：见 §8 表（**19 条**，含 `open_licenses`），输入输出全部是 DTO；许可/第三方声明的 About 入口走**命令**（`open_licenses`），**不新增事件**。

**跨模块禁止**：共享可变引用、模块间互调私有函数、事件载荷携带非 DTO 对象、scheduler 感知项目/设置。

## 11. 设计决策记录（跨模块分叉点）

| # | 决策 | 否决的备选 | 理由 |
|---|---|---|---|
| D1 | 调度器 = actor（单 task + mpsc），状态收容 task 内 | 共享锁状态机 | 零全局可变状态；单写者免锁；合并队列天然适合串行处理 |
| D2 | 超时/取消在 runner 内（CancellationToken） | 调度器注入时钟管超时 | scheduler 无时钟无进程概念；单测喂假 outcome 即可 |
| D3 | 组合层翻译"文件事件→CompileRequest"，scheduler 不感知项目 | watch 直连 scheduler 传路径 | 最小外部依赖；所有触发源收敛单一入口 |
| D4 | core IO 经 FileSystem trait（read_dir / read_to_string / read_to_string_lossy / canonicalize / is_dir / write / exists） | 每函数手写回调注入 | 接口面最小、单一事实来源 |
| D5 | 根文件探测正则启发式 | 完整 TeX 词法解析 | 成本收益不成比例；局限已记录，root_file 覆盖是逃生门 |
| D6 | 设置热更新 + 自写盘 content_hash 过滤 | 重启生效 / 不设防 | 防"自己写→自己重载"循环，过滤状态收在 storage.last_write |
| D7 | 前端 store 单向依赖 + events.ts 统一分发 | store 互引 / 事件总线库 | 依赖图可推理；避免引入总线抽象 |
| D8 | 自建命令路径校验（项目根内 canonicalize 前缀） | 信任前端传入路径 | 自建命令无 Tauri 权限模型兜底，必须自守 |

## 12. 当前行为基线、已知债与验证入口

### 12.1 已知债与未验证

| # | 项 | 现状 |
|---|---|---|
| 1 | `LatexmkRunner` 命名偏窄（**已结清并随②段整体删除**） | ①段改名为 `SubprocessRunner`；②段（ADR-0015 决策 2）把整条子进程路径删除 ⇒ 该名字与文件都不再存在。现存唯一 runner = `TectonicLibRunner`（§2.7）+ GUI 薄壳 `EmbeddedRunner`（§2.6）。架构图 SVG 的同步见 [design.md](../design.md) §5「图待重生成」 |
| 2 | 编辑非 UTF-8 源文件 | **不支持编辑**（设计取舍）：`read_file` 严格 UTF-8，遇 `InvalidData` 返回中文提示 + 状态栏红色提示条（㉓ 已做"至少说清楚"）；不做 lossy 打开，否则保存会把 U+FFFD 写回磁盘 = 静默损坏用户文件。`.log` 侧仍是 lossy（见 §4） |
| 3 | bib/biber 场景的编辑期两趟 | 内嵌形态**不跑外部 biber**（T9 实测：PATH 上有真 biber 也不被调用、不产 `.bbl`），所以"编辑期两趟"对 biblatex 文档不再有正解：检出 `.run.xml` 即 notice、停在草稿态。`backend=bibtex` 由内建 BibtexEngine 正常出 `.bbl`（T9 实测）。makeindex / glossaries 同样不跑 |
| 4 | 大纲全量重扫（**已解决**，⑦a） | 现为增量：只重扫内容变化的文件 + 前端只提交变化过的缓冲。实测（11 文件/436KB）每次编译成功后的刷新 `scans=1 reused=10`、空闲收敛 `scans=0 reused=11`，成本 55.5ms → 8.4–9.7ms；契约见 §3.5，数据见 [分析文档](../archive/research/p1-large-doc-editor-analysis.md) §3.3 |
| 5 | 延迟预算口径（**已收口**，③段 T19 提出 / T22 复核） | 分类改按**实测页数**（≤8 页 = 小文档 / >8 页 = 大文档，产品负责人 2026-11-21 已采纳）：`multifile`（74 页）归大文档 ⇒ cold **2.94 s EXCELLENT（距 3 s 仅 64 ms，临界）** / e2e 2.01 s EXCELLENT；`graphics`（8 页）归小文档 ⇒ cold 1.50 s PASS / e2e 0.90 s EXCELLENT；`thesis`（28 页）EXCELLENT。**唯一硬缺口 = `large`（125 页）cold 6.41 s 超 5 s**（引擎排版上限量级）：**保留 5 s 线、不放宽阈值**，登记为已知债 #30。数字用 T22 环境税清零版（[design.md](../design.md) §2.8）；T19 绝对值含 ~244 ms/进程 fontconfig 税，跨版本绝对比较作废。旧 latexmk/xelatex 数字只作历史（[design.md](../design.md) §2.7/§2.9）。 |
| 6 | 真实论文模板（hithesis）（**latexmk 时代记录**） | 阻塞点未定位：模板自带 latexmkrc × `-outdir=tmp` 约定下 `\include{子目录/...}` 写不出中间文件、报错后 latexmk 挂住；**调高超时也编不过**（roadmap ㉖，最小复现见 [troubleshooting.md](../troubleshooting.md)）。⚠ latexmk 路径已删除 ⇒ 该阻塞机制**不再是产品路径**；Tectonic 下同一模板的行为 `[未测]`（U-14 只证明 Tectonic 会自建 `tmp/` 子目录） |
| 7 | beamer 往返偏差 2–4 行 | 已定性、不修：换用「最小 H」「首个 H≤40」取块规则后往返结果**逐一相同** → 属 beamer/主题的 synctex 记录粒度；`\only<n>` 覆盖层内容在非本层页面上的反向映射天然不确定（基线数字见 [design.md](../design.md) §预览） |
| 8 | 非 Windows 平台 | 未验证：进程组 kill 与平台相关路径处理均为 v1 后置 |
| 9 | `cargo test -p latteset`（src-tauri） | 本机因 WebView2 限制无法运行，见 [troubleshooting.md](../troubleshooting.md) |
| 10 | 文档欠账（roadmap ㉔） | `crates/latteset-infra/...` 的引用无残留；headless 文档已转正为 [cli.md](../cli.md)（历史编号对照见 [archive/cli-mcp-plan.md](../archive/cli-mcp-plan.md)） |
| 11 | headless 与 GUI 并发编译同项目 | **未加锁**：约定 headless 独占（§8.1 并发契约）。要让两者并存需给 watch 加一次性 `IgnorePaths` 通道或项目锁，均未做 |
| 12 | MCP 状态订阅（notifications） | 未做（P1-5）：一期用「`compile` 同步返回 + `status`/`errors` 快照」覆盖，订阅式推送等真实需求 |
| 13 | headless `file_write` 不代建目录 | **设计取舍**（与 GUI `save_all` 同契约）：父目录必须已存在，错误消息明说。Agent 新建子目录需先建目录 |
| 14 | headless 根文件探测每次重扫 | 未做缓存：多数项目 <100ms；大项目再评估（缓存一致性成本 > 收益） |
| 15 | 折叠提供者全量扫描（编辑器侧） | `latexSuggest.ts` 的 `provideFoldingRanges` 忽略区间、`getValue().split()` 扫全文；**实测** 2.2ms@462KB、3.7ms@1.36MB，折叠模型失效后重算一次（防抖 ≥200ms）。60Hz 帧预算内，144Hz + 多 MB 才明显（[分析](../archive/research/p1-large-doc-editor-analysis.md) §3.2，roadmap ⑦b 缓做） |
| 16 | 编辑器侧"大文档"（**已覆盖**，⑦c） | 三档夹具（9 / 21 / 41 文件 × 2.2–5.5 MB，后两档同总量双倍文件数）：五项门槛全过——每击键净 **0.077–0.097 ms（绝对值恒定，与文件数/总量无关）**、打开全部标签 252/507/675 ms、大纲往返 14.1/28/27.9 ms、关标签后 model 25→4 且缓冲归零（**无泄漏**）。**结论：不需要优化**（[p1c 报告](../archive/research/p1c-multifile-large-project.md)）。遗留：多文件项目的**编译期**表现、`Ctrl+F`/大范围替换、可信堆读数（dev 下 `performance.memory` 抖动到出负值） |
| 17 | 大纲只在**编译成功**时刷新 | 编译一直失败时大纲停在旧结构（旧行为保留）。⑦a 的缓存已让"按保存触发刷新"变便宜（8–10ms/次），但有失败编译时的刷新时机/节流策略需要单独定（未做） |
| 18 | `latteset-mcp.exe` 被常驻进程占用 | 接了 DSH 的 `mcp-latteset` 之后，该进程会**锁住二进制**：`cargo build -p latteset-server` 报「failed to remove file … 拒绝访问」(os error 5)。绕行：只跑 lib（`--lib`）或用独立 `CARGO_TARGET_DIR`（见 [troubleshooting.md](../troubleshooting.md)） |

| 19 | 编译期页进度的 **XDV 版**（**已接线**，D3③） | `tmp/<stem>.xdv` 在内嵌唯一后**恒存在**（捕获表里就是权威页表）⇒ 页进度已从它数完整页（`captured_pages`，每趟收尾、数值变大才发）。**追加式（增量）页索引仍不接线**（没有消费方；上游阶段 3 技术要件，`scripts/xdv-inc.mjs` 的 4 种喂入序列对拍、1122 点前缀一致性已验证；见 [阶段 3 报告](../archive/research/stage3-incremental-output-parsing.md)、[G2 报告](../archive/research/g2-byte-offset-resync.md)）。工具 `scripts/xdv-report.mjs` 仍可用（内嵌形态的 XDV 就在 `tmp/`） |

| 20 | 引擎输出与编译期进度（**内嵌形态已重做**，阶段②） | 内嵌形态没有 stdout 管道：页进度由捕获的 XDV 完整页数给出（每趟收尾、只增）；编译中错误由引擎状态后端 `MessageKind::Error` 上报（非权威、无文件行号）；`.log` **边写边在内存里**（不再"收尾才落盘"）。⚠ **错误提前量 `[未测]`**：`live_errors` 用例已随引擎路径删除；"编译还没结束就出现致命错误"没有真机读数（见 §2.6.1）。**超时取证**：证据 = 捕获的 `.log` + 状态环尾部 + 已上报最大页，诊断文案"引擎输出显示…"。契约见 §2.4 / §2.6.1，事件见 §10 |
| 21 | **外部工具类能力缺口（内嵌形态）** | **内嵌形态不跑任何外部工具**（T9 实测）：**biber** 不调用本机二进制、不产 `.bbl` ⇒ 检出 `.run.xml` 的 `<binary>biber</binary>` 即 `warn!` + `compile-notice`（ADR-0015 决策 4，成功路径也要说），`kind` 退回草稿态；`backend=bibtex` 由**内建 `BibtexEngine`** 正常出 `.bbl`。**makeindex / glossaries** 不自动跑（索引/术语表为空）。**bibtex 趟、多趟收敛、页哈希**都在内嵌形态可用。**文案纪律**：`diagnosis.rs` 的 `unsupported_tool` 与 `runner.rs` 的 `BIBER_NOTICE` 必须写"引擎依赖外部 biber、内嵌形态不会替你运行"，**不得**写"引擎会调用 biber"或"引擎不支持 biber"（T9 实测口径；三工具文案有单测覆盖） |
| 22 | 内嵌形态的**性能数字只在 release 下有效** | `dev` profile 下 C 引擎（xetex/xdvipdfmx）按 `-O0` 编译，同一夹具比 release 慢 **6–8×**（历史实测 3485 ms vs 543 ms）⇒ 比性能必须 `--release`，否则结论会反过来。**另**：构建需要 vcpkg 原生链 + 5 个环境变量 + `RUSTFLAGS`，完整清单与唯一入口见 [tectonic-library.md](./tectonic-library.md) §2 |
| 23 | **页哈希缓存的口径只有一份（core）** | `PAGES_CACHE_VERSION` / `pages_cache_path` / `parse_pages_cache` / `format_pages_cache` 都定义在 `latteset_core::xdv`：内嵌 runner 在 `latteset-tectonic`、按 ADR-0012 **不依赖 infra** ⇒ 各写一份必然漂移，而漂移的代价是"永远首轮"（A 面每轮白跑一次转换 + 视窗全量重绘）。**内嵌唯一后只有一个消费者**（"与 infra 共用一份"的旧叙述随子进程删除而失效），但"口径只在 core"这条不变量不变；文件名固定 `<stem>.tectonic.pages`（`pages_cache_path` 内部写死 `ENGINE_ID`） |
| 24 | **常驻档（同进程连续编译）在重文档上是负收益** | 实测（方案 §6.4，release）：进程地板只有 **9.3 ms**，轻档（1 页）净收益 −26 ms，**重档（28 页多文件 Full）第 3 轮起退化、稳态比"每次起进程"慢 31%**（1450 vs 1109 ms）。**退化不是"活变多"**：`passes` 恒 2、`req_*` 每轮完全相同，但 `typeset` +27% / `convert` +39% / **`bundle_read_ms` +75%（读次数恒 2009）** ⇒ 同进程内累计的内存压力把**所有**操作一起拖慢（峰值工作集 245 MB）。**上游从未承接过"一进程跑 N 次引擎"**（CLI 一进程一次）。⇒ **P4 常驻按回退点处理：不投入**；该投的是 bib 跳过判据（≈470 ms/次）与 bundle/format 常驻化（≈30–55 ms/次） |
| 25 | 复核 `--example bench` / `bench-tectonic-lib.mjs` 的口径 | `crates/latteset-tectonic/examples/bench.rs` = **同进程连续编译 N 次**（常驻档的唯一入口，CLI 是一次性语义）；脚本是**每样本起一个进程**的 harness，常驻档要让模板自带 `--runs N` 循环（注释已写明）。`{root}` 替换的是**目录路径本身**，模板要自己写 `--root {root}` |
| 26 | 内嵌形态（路径 B）的反向定位（**已修**，roadmap ㊷） | 「`Input:` 141 条只有 29 条非空、章节源码一个都没登记」的读数曾被误判为章节没登记；实际 `Input:` 记录是文件**首次被打开时**才登记，多文件工程里有相当一部分落在 `Content:` **之后**，`core::synctex::parse` 改成全文扫描后章节即正常（实测 `multifile` 20 章的 `forward` 与历史子进程档逐字段一致）。**主输入那半是真缺陷**：主输入走 `input_open_primary`（内存直喂）⇒ 引擎 C 侧先把 jobname 定成 `texput`，`Input:1` 因此是 `texput`。修法 = 收尾把主输入记录补成真实绝对路径（`latteset-tectonic/src/synctex.rs`），两夹具对拍 + 零变化 A/B 见 [roadmap §6.10.1](../roadmap.md) |
| 27 | SyncTeX 自解析的两条已知偏差 | 见 [ADR-0013](../adr/0013-synctex-in-process-parser.md)：① beamer 类页面上前向坐标与 CLI 中位差 132 pt（样本行在 `.synctex` 里没有对应记录，两侧的节点选择规则不同；**往返指标不受影响**）；② `column` 恒 -1（格式里没有列号）。漂移风险（格式官方未承诺公开）由 `scripts/synctex-selfcheck.mjs` 当探测器。**另有第三条、独立登记**：反向在跨文件页/页尾命令行的归属缺陷（2026-11 验收），见 #29 |
| 28 | 引擎的**资源**设置是全局的（无项目级覆盖） | 设置面收口为两项资源（`tectonic.bundle` / `cache_dir`；形态位已随 v3 删除），GUI 的 `EmbeddedRunner` **每趟编译读一次全局设置** ⇒ 改完即生效、不需重启。**故意不做项目级覆盖**：项目覆盖要「资源随 CompileRequest 下发」，而 `CompileRequest` 不带资源、`ProjectOverrides` 也从来没有 `tectonic` 段 ⇒ 放了就是「看着能覆盖、实际不生效」的假开关。要做是一次独立改动 |
| 29 | SyncTeX **反向**定位在"页尾命令行 / 跨文件页"上不准确（**预存缺陷，2026-11 验收登记**） | **现象**：`synctex_inverse` 给出页尾命令行（`\include` / `\newpage`）的行号；跨文件页甚至指回根文件而不是子文件（第 4 页正文属 `sub/chapter.tex`，却报 `主文件.tex` 的 `\include` 行），同页 y 从 120 扫到 500 结果恒定、落不到正文行。**机理**：`core::synctex::parse` 的 `inverse_point` 只考虑**有宽高**（`has_extent`）的记录（`parse.rs:338`），正文行内层 glyph 记录（如 `h19,6`）没有 height/depth 被跳过，只剩外层**行盒**——其行号恰是页尾命令行；跨文件页再按 tag 回退到根文件。**不是引擎切换引入**：用 TeX Live `xelatex -synctex=1` 的产物复现同样现象；`LATTESET_SYNCTEX=cli`（系统 `synctex.exe` 1.5）在同一份数据上正确。**正向不受影响**（文件/行 → 页/坐标逐条一致）。**复现夹具**：`test_file/acc/fixtures/中文验收工程/`（Tectonic 数据）与 `test_file/acc/fixtures/对比工程/`（xelatex 数据）；原始输出 `test_file/acc/a3-inverse-p*.json`、`xelatex-run.log`（**工作区未提交**）。**判据**：① 第 4 页必须回 `sub/chapter.tex` 的正文行（不是根文件页尾行）；② 同一页 y∈[120,500] 应能落到不同正文行；③ 修完后 `synctex-report.mjs` 的往返指标不得变差。归属判定见 `test_file/acc/REPORT-acceptance.md` §2.3.3 |
| 22 | 错误条目的**文件归属**会错一章（`parse_log` 文件栈）（**部分修复**，2026-11） | 原现场：错误写在 `ch_05.tex:164`，列表报 `./ch_04.tex:164`。机制：日志把"关闭上一个文件 + 打开下一个文件"写在**同一行**（`[64]) (./ch_05.tex`），旧实现只按行首 `)` 弹栈、不看同一行后面又打开的 `(` ⇒ `ch_05` 没入栈、`ch_04` 被弹出。**修法**（`log_parser/scan.rs`）：弹栈后**在行内剩余部分补 push** + 行尾未闭合的打开标记（`RE_OPEN_TAIL`，如 `L3 programming layer <…> (article.cls`）也入栈；放在分类链尾 ⇒ 不改变既有分支与消息聚合，**原有 insta 快照不变**。回归用例 `tectonic_log_layout_keeps_file_stack`（真实 Tectonic 日志）。**覆盖面（部分修复，不是全修）**：嵌套 `\include{章节/第一章}` 内报错、`.sty` 内报错的归属已正确；**仍漏**深层无扩展名的 `\input{子/更深}`（行内第二次 `(`、**无扩展名**、行尾）——会报成父文件 `main.tex`，其后父文件的报错也连带错位（review E3；残留判据 `scan.rs:25-28` / `:65-78` / `:92-95`）。补修要点：把"行中、无扩展名"的打开标记也纳入文件栈（未做） |
| 23 | 触发面收紧到"引擎真正读了什么"（**未接线**，roadmap ㉝） | 实测（[G1 研究](../archive/research/g1-read-interception-feasibility.md) §5.1）：改**被引用**的 `.bib` → 所有 tmp 产物 mtime 不变（该触发不触发，roadmap ㉜）；改**未被引用**的 `.tex` → 白编译一次（`main.aux` 被重写，roadmap ㉝）。⚠ 原判据 `.fls` / `.fdb_latexmk` **是 latexmk 的产物**，ADR-0015 之后两者都不再生成 ⇒ 这条判据**失效**（`scripts/fls-report.mjs` 同）；㉝ 要重做需另找通道（**未定方案**） |
| 24 | 字节级 I/O 拦截（上游 G1 原义）**不做** | 三条现成通道都只到文件级；字节偏移需改引擎 / 文件系统驱动 / API hook（与 ADR-0003 不签名分发冲突），且**没有消费方**——上游唯一用途 seen 水位要"引擎进程活着且状态可回退"（G4 fork，Windows 无 `fork()`）。见 [G1 研究](../archive/research/g1-read-interception-feasibility.md) §4 |
| 25 | Quick 路径的引擎闸门（**历史，已随引擎删除**） | latexmk 时代：Quick 无条件给引擎加 `-no-pdf`、收尾无条件调 `xdvipdfmx`，只有 XeTeX 成立（pdflatex 报 `unrecognized option '-no-pdf'`；lualatex 静默忽略且中文不产 DVI/XDV）⇒ 曾误报失败、甚至用旧 `.xdv` 覆盖新 PDF。当时修法是 `Engine::writes_xdv()` 闸门 + 页哈希缓存按引擎分文件。**ADR-0015 之后这三个引擎与 `xdvipdfmx` 全部删除**；内嵌形态恒有 XDV ⇒ 等价约束只剩"`page_hashes` 空表 = 无法判定（XDV 缺失/损坏）⇒ 全量刷新"，见 §2.7。历史数据见 [现代引擎实测](../archive/research/modern-engines-zh.md) §3.1 |
| 21 | 编译期预览（**已解决**，roadmap ㉞） | PDF 由 `xdvipdfmx` 在排版结束后产出（需完整 XDV + postamble）→ 编译期无图可显示。**可行性已验证**：页前缀 + 从零合成 postamble → `xdvipdfmx` 接受（页数正确、首页渲染一致）（[阶段 2 报告](../archive/research/stage2-streaming-feasibility.md) §3）。**（见 [DVI 报告](../archive/research/dvi-preview-feasibility.md) §10）**：① 合成器已成可复现入口 `scripts/xdv-partial.mjs`（构造 9–12 ms，整份重建自检同长）；② **进程内**转换 **92–131 ms**（31→125 页，页数全对），比外部 `xdvipdfmx` 的 497–564 ms 快 4–5×；③ 长编译窗口实测 **7601 ms**（125 页冷 Full，release）；④ 频繁重载体感：5 次背靠背重载 **0 long task**、重载期间 1797 帧滚动位置零偏移。⇒ 已**落地**（见 §2.7「流式出图」：趟边界出图 + `compile-preview` + 前端「编译中预览」）；同进程引擎**全局锁**决定只能在趟边界出图；**内嵌唯一后该能力恒可用**。触发预算与独立事件语义见 [roadmap.md](../roadmap.md) |
| 26 | **页哈希把 `bop` 的 prev 指针算进了内容** → 长度类改动打脏下游页哈希（B/C 精度） | 机制（字节级对拍）：DVI/XDV 每页头是 45 B `bop` = 1 opcode + 10×i32 计数 + **i32 prev（前一页 `bop` 的绝对字节偏移）**。样本：同一文档两次编译仅日期串差一位数字 ⇒ 页 1 **长 10 B**（哈希变，应当）；**页 2 逐字节相同**（它的 prev 指向页 1 起点，没动）；**页 3–26 长度全同、唯一差异在页内偏移 43–44**（prev 跟着移动）⇒ 25/26 页被判"变化"。而 `xdv.rs:211` 是 `hash_page(&bytes[bop..p])`——**含 bop**。影响：B（空表才跳过重载）要求全篇无一处变长，故只对"输出零变化"的编辑成立（如改注释），该场景仍工作；**C（只重绘变化页）**在"编辑页变长"时退化为**该页之后第 2 页起全量重绘**，长度不变型编辑与末页编辑仍精确。**收益/风险已量化（42 用例 / Σ2142 页次，口径与数据见 [量化报告](../archive/research/page-hash-prev-quant.md)）**：口径变化页数 = `N − m`、**假阳性页数 = `N − m − 1`**（m = 最早变长页；适用集 17/18 成立，唯一反例是真重排）⇒ raw 口径 **Σ假阳性 375 页 / 15 用例**（微 FP/口径变化页 **90.36%**、微 FP/N **17.51%**、raw 精确率 **9.64%**；首章单例最大 **66/74 = 89.19%**），**假阴性 0/42**（三口径），V1（`[bop,bop+41) ++ [bop+45..p)`：丢 4 B prev、保留 opcode+10×i32 计数）**假阳性 0 / 精确率 100%（本矩阵）**；**归一化对 A/B 零收益**（B/A 命中 raw=V1=V2=**12/42**，本矩阵 **0 例**「raw 非空而 V1 空」），收益全在 C：视窗 7 页口径省 **54.9–87.4 ms/次**（12/15 例；对照 V1 只失效 1 页 = 9.1–14.6 ms），全量口径上界 3428–5464 ms（推算，锚点 9.14/14.57 ms/页）。缓存迁移为**一次性**首轮失真（白跑 `xdvipdfmx` 0.65–0.94 s + 视窗全量重绘 64–102 ms，写回自愈、**不漏判**），推荐「文件头口径标记」（可叠加文件名带口径）。另：**A 的判据必须建在页级哈希（含页数）或「首 BOP→EOF」上**——整篇 XDV 逐字节的 A 命中只有 10/42（少的 2 例仅因 `pre` 注释墙钟分钟位差 1 B @43），页级为 12/42。**现行口径**：`hash_page` 为 **V1**（丢 4 B `prev`、保留 opcode + 10×i32 计数器，见 `crates/latteset-core/src/xdv.rs` 的 `BOP_LEN`/`BOP_PREV_LEN`）+ 缓存首行口径标记 `v1`（`PAGES_CACHE_VERSION` 同在 `crates/latteset-core/src/xdv.rs`；读侧 fail-closed）。验证：单测 `prev_only_change_is_not_a_page_change`、`page_growth_only_marks_the_grown_page`（**先在旧口径下跑红**：前者报"第 2 页变了"、后者报 {1,3,4}）、`pages_cache_requires_scope_marker_and_rejects_legacy`；`#[ignore]` 真机用例 `page_growth_edit_marks_few_pages_on_real_project`（20 页真实项目插 1 个汉字 ⇒ 只报 ≤3 页；旧口径会报第 3 页起全部） |

**ADR-0015 ②段之后新增的已知债 / 未验证**（号码续排，不与上表重复）：

- **流式错误的提前量 `[未测]`**：内嵌形态的中间错误来自引擎状态后端（`MessageKind::Error`），没有真机读数；`live_errors` 用例（latexmk 时代）已删除。页进度有真编用例。
- **六档性能基线（③段已测，T22 受控重测定稿）**：T22 环境税清零版三轮独立运行、同档同口径极差除 tiny 两条热路径（绝对 21–24 ms）外 ≤7.2%；数字见 [design.md](../design.md) §2.8。**唯一硬缺口 = `large` cold 6.41 s（见 #30）**；T14→T19 的 **+200–240 ms/进程差已归因：fontconfig 宿主字体扫描环境税（2026-11-21）**——旧 fontconfig 缓存失效时每个 CLI 进程重扫 `C:/Windows/Fonts`，用最小 fontconfig 配置 + 可写缓存即清零（tiny 热路径 365→73 ms），**与产品代码无关；T19 绝对值跨版本绝对比较作废，相对收益（L4b `convert_ms=0` / L5 `preview_ms` 减半）仍有效**（同进程稳态无回归：tiny Full 153→73 ms）。
- **#30 `large` cold 已知硬缺口（引擎排版上限量级）**：125 页冷编 **6.41 s**（T22 清零版；含税 6.63 s）超 5 s 线，T18 预热后 GUI 首编仍 ≈5.4 s；**不放宽阈值**（产品负责人决定）。回归对照用 §2.8 的 T22 六档表；后续优化（排版趟数/跳过转换类）再回看。
- **NSIS 安装版冷/温启动 `[未测-环境限制]`**：受限会话静默安装挂起（UAC/权限不可交互）；替代证据 = `tauri build` exit 0 + 体积门禁（NSIS 88.46 MiB / MSI 109.63 MiB ≤200 MB）+ release 直启分段（进程→Ready 126 ms）。
- **About 许可入口真机点击 `[未测-环境限制]`**：`open_licenses` 的数据面由 bindings 13/13 + vitest 覆盖，WebView2 在本会话不渲染，未做真机点击。
- **A 闸门 `.pdfid` 判据的已知限制（T22 B4，低风险、不修）**：`tmp/<stem>.tectonic.pdfid` 只记 `{len, mtime_ns}`，**不哈希 PDF 内容**。正常编辑/编译都会改变二者（同长度覆盖也会变 mtime）；只有刻意伪造"内容变、长度与 mtime 都不变"才能绕过 A 面 ⇒ 且第一条件（页哈希逐页相同）仍需同时成立，故记为低风险。改为内容哈希会让每次编译多读整份 PDF（首编 PDF 数 MB），收益不成比例。
- **缓存 GC / 预热的并发对抗样例 `[未测]`**：GC 单测覆盖保留当前/删旧/不误删、解析不出当前 digest 时整体不动，但 GC 与预热/编译的并发压测未构造。
- **本机 DSH 会话的 `cl D8050` / `LNK1104` `[环境归属，非代码债]`**：根因是**工作区根目录带 DSH 沙箱遗留的可继承 Low 完整性标签** ⇒ 由工作区内 exe（cargo build-script）起的 `cl.exe` 是 Low IL、写不了 Medium 的 `%TEMP%`，`/Z7` 临时文件与 `lib.exe` 的 `lnk{…}.tmp` 因此失败。**不是 vcpkg 缺包、不是 MSVC 版本、不是 pwsh 管道**。绕法 = 把 `TMP`/`TEMP` 指到工作区内目录；彻底修 = `icacls "<repo>" /setintegritylevel (OI|CI)Medium`。CI/普通终端无此问题。复现与 env 结论见 `test_file/build-env/REPORT-build-env.md`，排障条目见 [troubleshooting.md](../troubleshooting.md)。
- **构建代价三项 `[未测]`**：干净机器冷/热墙钟、峰值内存；CI 内嵌档冷构建记录 +15–20 min（缓存缓解）。**安装包体积已测**（T19 终版 NSIS 88.46 MiB / MSI 109.63 MiB）。
- **真机窗口/可编辑 `[未测-环境限制]`**：T9/T19 会话因同一 Low 标签导致 WebView2 起不来；替代证据（启动分段 / MCP bridge / watch 线程 / bindings 导出 / headless 全链路真编）见 `test_file/phase2/REPORT-gates.md` §2.1 与 `test_file/phase3/REPORT-gates.md` §8。
- **在线完整 bundle 只测了单向**：显式切在线后编过（首编含联网取包 **344 s**，一次性成本）；**在线 → 随包/本地 的反向切换未单独重测**（同一 `resolve_source` 优先级有单测覆盖）。真实超时真编、makeindex/glossaries 真编仍未跑。
- **架构图待重生成**：`docs/diagrams/*.png|svg` 还是 latexmk/TeXLive 版本（本机缺 mermaid + 无头浏览器，脚本跑不通）；mermaid 源码已按内嵌唯一更新，**图待重渲染**。

### 12.2 跨模块不变量（改回去即复发）

- **引擎口径（[ADR-0015](../adr/0015-tectonic-only-embedded-engine.md)：唯一引擎 = Tectonic，②段已收成内嵌唯一）**：产品**只有 Tectonic 一个引擎、只有内嵌一种形态**；`latteset_core::types::ENGINE_ID = "tectonic"` 是日志字段 / 页哈希缓存名 / 可执行文件名的**唯一字面量副本**，别处不许再写 `"tectonic"`。子进程路径、`tectonic.lib_form`、`LATTESET_TECTONIC_LIB`、`engine_form` / `lib_form_available` 命令与 UI 形态 chip **都已删除**；**能力面（页哈希 / 编译中预览 / 内存产物 / format 缓存）对所有用户一致可用**，不再有"能力随形态不同 ⇒ 需显式显示不可用"这条分支。别把"两形态"叙述加回来。
- **Quick 的语义（只有一种，别按旧两档写）**：`Quick` = 编辑触发的**请求强度**，内嵌 runner 同样走重跑循环（含 bib 趟），**预算内收敛就按实际结果报 `Full`**（三重闸门 = `passes < 3` ∧ 第 1 趟 < 1000 ms ∧ 累计 < 2000 ms，§2.7）；超出预算或检出外部工具才停在草稿态。`Success{kind}` 必须报**实际**值——报错会让前端误提示「引用待更新」或多跑一次空收敛。
- **设置的读入口**：`open_project`、`update_settings`、**以及 watch 的设置热更新**都必须先读**纯全局**设置（`load_global`）再合并项目覆盖；后两者还必须同步内存 `ProjectState.root_file`——否则出现跨项目设置污染、「选了根文件仍报未确定根文件，必须重开项目」，以及**外部清掉覆盖永远不生效**（拿有效值当基数会粘住旧值）。
- **覆盖清洗逐字段**：`sanitize_overrides` 不能退回「整包丢弃」（会连带丢掉同一文件里合法的 compile 覆盖）。
- **自写盘过滤**：`update_settings` 写盘记 hash、watch 消费一次——去掉即「自己写 → 自己重载 → 重复广播」。
- **外部写文件是非原子的**：设置热更新必须容忍"截断 → 写入 → 关闭"的中间态（3×150ms 短重试），否则第一次读空文件、第二次撞共享冲突 → 用户看到「改了没反应」。
- **路径校验只有一个入口**：命令面路径一律经 core `project::paths`（D8）；core 内部（如 outline 读盘）用词法前缀版，差异仅剩符号链接目标与 8.3 短名。
- **`.log` 必须容错解码**：严格 UTF-8 读取会把「编译失败」退化为「拿不到任何错误信息」（历史场景：GBK 源 + pdflatex；现役内嵌路径仍保留 lossy 解码，非 UTF-8 日志不得让错误列表变空）。
- **中间态必须走独立事件 + 前端 `phase` 守卫**（§2.4 / §2.6.1）：`compile-errors` / `compile-preview` 是非权威中间态，可能晚于终态抵达；复用 `errors-updated` 会让晚到的中间态顶掉权威列表（实测：超时诊断 1 条被 30 条中间态覆盖）。内嵌形态只在**引擎报 Error**时上报中间错误，收尾后不得再改写终态。
- **任何路径都不得给 Tectonic 设 `SOURCE_DATE_EPOCH`**：Tectonic 把它当**引擎时间源**，设 0 会让正文 `\today` 印成 1970-01-01。内嵌路径**禁止** `build_date_from_env`、必须显式 `build_date(SystemTime::now())`。同一条理由也意味着**"PDF 逐字节相等"不能再当 Tectonic 档的断言**（元数据/加密层不可控）。历史数据（xelatex 时代 `/ID` 只差 64 字节）保留在 [design.md](../design.md) §2.11。
- **生成产物永不当作源码打开**（㉒）：反向定位命中 `tmp/*.toc` 之类时先就近回落真实源码、落空则只给提示——直接打开会出现一屏用户没写过的内容。
- **失败必须可见**：`openFile` 的 rejection 与 SyncTeX 的正反向失败都要落到 UI（状态栏提示条 / 预览工具条同步提示），不能只有 `console.error`——否则一律表现为"点了没反应"。
- **路径与 SyncTeX 策略只有一份实现**：headless（§8.1）必须调 `core::synctex::{pdf_path_for_root, resolve_inverse}` 与 `core::project::paths`，不得自己拼 `tmp/` 路径或另写回落逻辑——两套实现迟早漂移（GUI 命令面与 CLI 的 PDF 路径必须永远一致）。
- **CLI 的 stdout 只有 JSON**：日志一律走 stderr——否则 Agent 侧 `compile | jq` 这类管道立刻坏掉。
- **`compile` 的退出码是"编译是否通过"**：`0` 仅当 `status == "success"`，失败即 `1`（未通过 ≠ 命令出错）。改坏这个语义会让 Agent 的「改 → 编 → 验」闭环静默失效。
- **大纲增量的三条不变量**（§3.5，改回去即复发）：① 内容必须每次重取（缓冲优先，否则读盘），缓存只复用**扫描结果**——按路径命中就不再读盘会让外部改动永远不生效；② 缓冲缓存按**项目根**作废、并按 `open_paths` 淘汰——漏了会让"关掉标签"或"切回旧项目"读到磁盘旧内容（脏缓冲丢失）；③ 入口路径必须先归一——否则同一文件两种拼写 = 两个 key，`visited` 去重失效、大纲出现重复项。
- **前端"只发改动缓冲"必须与后端的缓冲缓存配套**：`lastSent` 差分（值比较）只决定"这次推不推"，真正的内容真相仍在后端缓存里；因此**项目根变化时前端必须清 `lastSent` 全量重发**（后端此刻已清空缓存），否则未变化的脏缓冲不会被重发。
- **大纲刷新要合并**：`refresh()` 进行中只记一个"待补一次"意图（结构事件风暴实测一次连发 11 次）——并发刷新会让差分失效（每次都赶在上一次写回 `lastSent` 之前发出），且后端缓冲缓存的写入顺序不再确定。
- **新建 `.tex` 必须让根探测认得出来**（㊺）：向导骨架**一定**含 `\documentclass`（单测钉住）——根探测要求文件里有它，所以"空文件 + 建完就编"这条路本身不成立；落盘后必须重跑一次探测（`projectStore.rescanRoot`，与 `open_project` 同一口径），探到即补一次 `compile_now`——漏掉补编译的症状是「状态栏已显示就绪，预览却一直停在『PDF 在这里等你』」（watch 那条链在探到根文件**之前**就丢掉了这次变化）。
- **新加的设置键必须可缺省**（㊺ `compile.new_file_wizard`）：`#[serde(default = ...)]` —— 旧 `settings.json` 没有这个键也要能读，否则升级那一刻整个配置文件解析失败、用户设置全丢（与 `ui` / `tectonic` 同一条理由）。
- **引擎清单与形态位都不存在了**（ADR-0015 ①②）：`list_engines` / `EngineInfo` / `services/engines.ts` / `LATTESET_TEX_ENGINES` / `probe::find_in_path` 与 `lib_form_available` / `engine_form` / `EngineForm*` / `LATTESET_TECTONIC_LIB` 已全部删除；状态栏引擎名是前端常量 `Tectonic`（`StatusBar.vue`），设置页只剩 bundle / cache 两项资源。旧文档/脚本里设着这些环境变量的，启动时会各收到一条「已删除，忽略」warn。**别把任何形态判定/可用性命令加回来。**
- **前端路径与错误只有一份口径**（㊿）：分隔符归一 / "在项目内"判定 / 项目内相对路径一律走 `services/paths.ts`，**不得再手写 `replace(/\\/g,"/")` 与前缀比较**（历史上有 10+ 处，且两处"相对路径"实现语义不同 —— 一处项目外返回绝对路径，会把绝对路径当相对路径发出去）；rejection 取人话一律走 `services/errors.ts`。大小写口径同样只许一份：`.tex` 判定一律 `/\.tex$/i`（Windows 上 `.TEX` 是同一个文件）。
- **页哈希口径只有一个（V1）且缓存自带标记**（已知债 #26）：`hash_page` 必须丢掉 `bop` 尾部的 4 B `prev`（保留 opcode + 10×i32 计数器）——改回"整页哈希"即复发"前面某页变长 ⇒ 第 3 页起全部判变化"（实测 25/26 页、Σ375 页假阳性）；`tmp/<stem>.tectonic.pages` 首行必须写 `PAGES_CACHE_VERSION`，读侧不匹配一律当 `None`。**且 A/B 的判据必须建在页级哈希（含页数）上，不得建在整篇 XDV 字节指纹上**——`pre` 注释在 TeX Live 的 `xelatex` 下是本地墙钟（跨分钟必失效）。**内嵌唯一后页哈希恒可用**；空表只表示"无法判定"（XDV 缺失/损坏）⇒ 全量刷新，别把"空表"当"逐页未变"。
- **A 闸门（PDF 复用）的判据必须两条同时成立**（T17 L4b，③段不变量）：① 页哈希逐页相同（含页数）；② **磁盘 PDF 身份未变**——`tmp/<stem>.tectonic.pdfid` 记 `{len, mtime_ns}` + 版本行 `v1`，转换成功后写、复用路径不重写；**缺失/版本不认识/损坏/页数页序变/PDF 被手工改（含同长度覆盖）/PDF 被删/空页表 ⇒ 一律回到转换路径**（保守一次，不得跳过）。只加一条、或把"身份记录"当成可选优化去掉，都会复发"手工改 PDF 后仍跳过转换/输出旧 PDF"。
- **首启预热不得改变编译结果、不得联网/写脏缓存**：`prewarm_format` 只生成 `.fmt`（不跑用户文档），失败静默、半成品不落盘；预热与首编经 `PrewarmGate` 互斥（首编 `wait_for_prewarm()`，超时 60 s 按无缓存继续）；**预热前后页哈希逐页相等**（真编用例断言）。在线源预热必须走 `bundle::prewarm_plan` 的**零网络零写**判定（缓存标记齐全才预热），否则冷缓存启动会联网取 digest（实测 ~8 s）。
- **bundle 来源与"不静默回退"**（ADR-0016 决策 2）：优先级 = 显式设置 > 环境变量 > 随包内置集 > 在线兜底；内置集缺失/损坏 ⇒ 显式错误 + 修复安装/切在线指引，**不得偷偷联网**；缺包时诊断给"切在线完整集 / 补本地 bundle"，**不得自动换源**。随包 zip 的 Windows verbatim 路径（`\\?\…`）必须在拼 `file://` 前剥前缀（`path_to_file_url`），否则 `Url::to_file_path()` 失败、默认离线路径整体不可用。
- **缓存 GC 不得误删在用的 digest**：`gc_cache` 只按"**当前 bundle digest**（`current_bundle_digest`，随包/显式/在线各自的 digest；在线只读缓存、零网络）+ 最近 N（`LATTESET_CACHE_KEEP`）"清理；**解析不出当前 digest 时整体不动**、无 format 时不动、未知条目不删；GC 与 runner/预热**共用 `resolve_cache_dir`**（设置 > env > 宿主）；GC 只在启动空闲时执行。

前端的三处异步守卫与 PreviewPane 渲染契约见 §9.2 / §9.4，SyncTeX 相关约束见 §5。

### 12.3 验证入口

每条只给命令与一句话用途；**用例清单、门槛与口径见 [testing.md](./testing.md)**。

| 要验的东西 | 入口 |
|---|---|
| 编译性能与预算回归（**现行入口**） | `node scripts/bench-embedded.mjs`（先 `npm run lib:cli` + `node scripts/gen-bench-projects.mjs`；一条命令产出六档 × `cold-format`/`cold-project`/`noop`/`noop2`/`edit`/`edit2` + 跨项目 format 命中 + 进程地板的 JSON 与 Markdown；每档 3 次取中位、保留 min/max 与原始值；超预算 / 复现差 >10% / 档失败退出码 1）。**预算分类已按实测页数采纳**（≤8 页 small / >8 页 large；见 [design.md](../design.md) §2.6）；**T22 环境税清零版**六档数字与 `large` 硬缺口见 [design.md](../design.md) §2.8，原始数据 `test_file/phase3/BASELINE.md` §0.1（工作区证据）。**测量纪律**：需同时固定 fontconfig 最小配置 + 可写缓存，否则每进程多付 ~244 ms 税收、跨版本比较失效 |
| 编译性能与预算回归（**历史基线复现**） | `node scripts/bench.mjs`（fixture 同上；超预算退出码 1）。⚠ **脚本仍直接调 latexmk/xelatex**，产品已不再走该路径 ⇒ 只能当历史基线复现，不是当前实现的测量入口（见 [testing.md](./testing.md) 的「脚本失效说明」） |
| 随包精简集 / 断网 / 缺包补全（③段） | `node scripts/make-bundle.mjs --max-zip-mb=120` + `--verify` → 断网首编（坏代理 + 空缓存）→ `validate-pdf.mjs`；缺包夹具 → 诊断 + 显式切在线后重编。生成/覆盖：`node scripts/gen-bench-bundle.mjs --phase=all`；验收命令与 T19 数字见 [testing.md](./testing.md) §③ |
| 许可资源 / 安装包门禁（③段 T21） | `node scripts/make-notices.mjs --check`（离线确定性：NOTICE/manifest/LICENSES/sources 逐字节一致）；`npm run tauri:build` 打包后看 NSIS/MSI 体积（**≤200 MB 门禁**，T19 终版 NSIS 88.46 MiB / MSI 109.63 MiB） |
| 首启预热 / 启动分段 | release `latteset.exe` 直启（`RUST_LOG=info`，隔离 cache）→ 读 `startup 分段` 与 `首启 format 预热完成 prewarm_ms`；真编用例 `cargo test -p latteset-tectonic -- --ignored prewarm_then_first_compile_hits_format_cache`（断言预热后首编 `format_ms=0` 且页哈希一致） |
| 构建确定性（逐字节） | `node scripts/check-determinism.mjs`（三档 × Full/Quick 各两次；`--without-epoch` 可复现非确定性）。⚠ 同上：脚本仍驱动 latexmk/xelatex 并注入 `SOURCE_DATE_EPOCH`，与产品现行口径（Tectonic 不设该变量）**不一致**，结论只作历史 |
| SyncTeX 往返精度 / 自解析与 CLI 对拍 | `node scripts/synctex-report.mjs`（三组样本；基线见 [design.md](../design.md) §预览）、`node scripts/synctex-selfcheck.mjs`。⚠ 两者都只覆盖**往返**口径（用正向坐标回查），**不含任意点反向**——后者有已知缺陷，见 #29；`synctex-report.mjs` 内部仍调 `latexmk -xelatex`（历史；Tectonic 下未改） |
| core 逻辑（调度 / 解析 / 诊断 / 大纲） | `cargo test -p latteset-core`（**269 passed / 0 ignored**，T19 实测） |
| 子模块用例（公式扫描 ㊸ / 新建骨架 ㊺ / 片段装配 ㊸） | `cargo test -p latteset-core --lib <math\|newfile\|snippet>`（引擎清单 ㊻ 的用例随 `list_engines` 删除） |
| infra 用例（文件系统 / 存储迁移 / 监视 / synctex 提供者） | `cargo test -p latteset-infra`（**40 passed / 0 ignored**，T19 实测；子进程 runner 的 32 个用例随整文件删除，新增 v2→v3 迁移 6 例） |
| 内嵌 Tectonic 引擎（含真编） | `npm run lib:test`（四个 crate：**399 passed / 8 ignored**，T22 复核）；`cargo test -p latteset-tectonic` = **65 passed / 3 ignored**，`-- --ignored` 跑 3 个真编（真编 / 预热后首编 / 随包 `Shipped` verbatim，17.07 s 全过） |
| headless 服务层（CLI/MCP 共用） | `cargo test -p latteset-server`（lib 12 passed / 4 ignored + cli 8 passed）；`-- --ignored` 跑 4 个真编 + 1 个 MCP 真编，闸门 = **编进来 + 有可用 bundle**（不再探测 PATH 上的 `tectonic`）；`--test mcp_stdio` 跑真实二进制的 MCP 管道链路（5 passed + 1 ignored），见 §8.1.1 |
| 公式预览命令面（㊸） | `npm run lib:cli` 后 `latteset-cli --project <夹具> math '<公式>'`；落点 `<项目>/tmp/snippet/<键>/`，权威 `main.pdf` 与 `tmp/main.*` 逐字节不变 |
| 前端 store / composable / services（含路径口径 ㊿） | `npm run test`（**17 files / 161 tests passed**，T19 实测） |
| 类型检查与构建 | `npm run build`（`vue-tsc --noEmit` + `vite build`） |
| CI 发布前链路（③段 T22 B2） | `.github/workflows/ci.yml` 的 `build-windows` **无条件先** `Build release CLI`（`./scripts/with-tectonic-lib.ps1 -Mode cli`）——它是下面两处的共同前置：① 代表集 bundle 生成、② `Offline first compile`（直接执行 `src-tauri/target/release/latteset-cli.exe`）。**不得**再把它塞回某个缓存分支：代表集缓存命中而 rust-cache 无 release 产物时，② 必红（T22 review 修复） |
| 绑定生成物（DTO/命令/事件改过后必跑） | `cargo test -p latteset -- --ignored export_bindings`（本机受 WebView2 限制不可用）或**启动一次 debug 构建**自动导出；T19 核对 **13/13 PASS**（命令 **19 条** / 事件 9 个与 Rust 一致） |
| 流式反馈真机时间线 | `node scripts/gen-stream-fixture.mjs test_file/projects/_stream-lab [--error]` → `VITE_LATTESET_PROJECT=<夹具> npm run tauri dev` → 用 tauri server 记录事件时间线（页进度可用；**错误提前量 `[未测]`**） |
| 编辑器侧大文档性能（每击键 / 折叠 / 大纲往返 / 多文件） | `node scripts/gen-large-project.mjs` → `node scripts/editor-report.mjs --tier multi20 --json out.json`（WS 直驱真机、零第三方依赖、超门槛退出码 1） |
| DVI/XDV 产物（页索引 / 页级差分 / 截断可读性） | `node scripts/xdv-report.mjs <tmp/*.xdv>`（`--diff=<other.xdv>`、`--truncate-at=N`、`--watch=ms`）。内嵌形态的 `tmp/<stem>.xdv` 恒存在 ⇒ 对现役产物有效 |
| 追加式（增量）页索引 / 前缀一致性 / 增量 vs 全量成本 | `node scripts/xdv-inc.mjs <tmp/*.xdv> --selftest`（`--cost=<chunk>`、`--prefix=4096`、`--watch=ms`；仍未接线，无消费方） |
| 全仓 Rust（唯一入口） | `npm run lib:test`（需 vcpkg；T22 复核 **399 passed / 8 ignored**，8 个真编在各自 `--ignored` 下全过） |
| 引擎到底读了什么（依赖集合 / 为什么找不到文件）（**latexmk 时代工具**） | `node scripts/fls-report.mjs <tmp/main.fls> --fdb <tmp/main.fdb_latexmk>`；失败的查找尝试用 `KPATHSEA_DEBUG=32`。⚠ `.fls`/`.fdb_latexmk` 都是 latexmk 的产物，内嵌形态不生成 ⇒ 这两条只对历史产物有效 |
| 只有真实窗口能验的部分 | [troubleshooting.md](../troubleshooting.md) §真机验收清单（tauri server MCP 驱动） |
| 产品级实测数字 / 已完成项与证据 | [design.md](../design.md) ｜ [roadmap.md](../roadmap.md) |

### 12.4 与其它文档的关系

- **本文件是函数级契约的唯一落点**（含分层里各模块的签名、算法、通信契约与设计分叉点 §11）；产品语义（延迟预算、失败语义、MVP 边界、分层与 IPC 总览）以 [design.md](../design.md) 为准，本文只写实现契约与已知债，两者不重复展开。
- 运行命令与用例清单见 [testing.md](./testing.md)；内嵌 Tectonic（唯一形态）的构建/装配契约见 [tectonic-library.md](./tectonic-library.md)；CLI + MCP 的用法、工具表与限制见 [cli.md](../cli.md)；历史决策与被否决的备选见 [adr/](../adr/) 与 §11。
