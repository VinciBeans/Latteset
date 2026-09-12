# TeXPresso 模块详细设计（函数级）

> 上层设计见 [architecture.md](./architecture.md)，产品语义见 [design.md](./design.md)。
> 本文件把 architecture.md 的每个大模块拆到**小模块 → 函数**，给出每个函数的签名、算法与信息局部性，以及大模块之间的通信契约。

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

## 2. 编译子系统（scheduler 大模块）

### 2.1 拆分

```
scheduler（core）
├── queue.rs        —— 合并队列（纯逻辑，无 IO）
├── policy.rs       —— 失败语义决策表（纯函数）
├── runner.rs       —— CompileRunner trait + CompileRequest/CompileOutcome（core 定义）
└── scheduler.rs    —— actor 主循环（唯一写者，状态全在 task 内）

texpresso-infra
└── runner.rs       —— LatexmkRunner：CompileRunner 实现（tokio 进程、超时、树杀、PDF 拷贝）
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
pub struct RunningJob {
    pub request: CompileRequest,
    pub attempt: u8,          // 0 = 首次，1 = 已重试一次
}

pub enum Decide {
    StartPending,            // 执行队列中的最新请求（跳过错过的旧版本）
    Retry,                   // 超时且无等待：同一请求重试一次
    FinishOk,                // 成功且无等待：无事可做
    Fail(FailureKind),       // 展示失败（不重试）
}

/// 输入只有：重试计数 + 编译结果 + 是否有等待条目。不读任何外部信息。
pub fn decide(attempt: u8, outcome: &CompileOutcome, has_pending: bool) -> Decide;
```

**算法**（来自 design.md 失败语义表，逐一对应；见 `scheduler/policy.rs`）：

| outcome | has_pending | attempt | 决策 |
|---|---|---|---|
| Success | — | — | 有 pending → `StartPending`；无 → `FinishOk` |
| Timeout | true | — | `StartPending`（跳过重试） |
| Timeout | false | 0 | `Retry` |
| Timeout | false | 1 | `Fail(Timeout)` |
| ContentError | true | — | `StartPending`（不重试） |
| ContentError | false | — | `Fail(ContentError)` |
| Aborted | true | — | `StartPending`（abort 后的新请求是新意图） |
| Aborted | false | — | `Fail(Aborted)` |
| IoError | — | — | 视同 ContentError（不重试） |

**信息局部性**：`decide` 只读 `running` 与 `outcome`，不碰队列、不碰设置、不碰时间。重试计数是唯一跨调用信息，收在 `RunningJob.attempt`。

### 2.4 runner.rs — CompileRunner 接口（core）

```rust
pub struct CompileRequest {
    pub root_file: PathBuf,        // 根文件绝对路径
    pub project_root: PathBuf,     // 工作目录
    pub engine: Engine,            // 请求构造时从设置快照拷贝（之后设置变化不影响运行中任务）
    pub timeout: Duration,         // 同上
}

pub enum CompileOutcome {
    Success { pdf_path: PathBuf },
    Timeout,                       // 超时强制终止（runner 已树杀）
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

### 2.5 scheduler.rs — actor 主循环（core）

```rust
pub enum SchedulerCommand {
    Compile(CompileRequest),   // 组合层已把文件事件翻译成请求（D3）
    Abort,                     // 手动终止：取消运行中 + 清空队列
    JobFinished(CompileOutcome), // 内部：运行任务完成（actor 私有，外部不发送）
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
  running.aborted = true; 取消当前任务 → 清队列   // 终止语义：停 + 清队；
                                                 // abort 后即使 runner 忽略 cancel 也按 Aborted 呈现
on_finished(outcome):
  d = decide(running.attempt, outcome, !queue.is_empty())
  match d:
    StartPending → emit(Running); 启动队列最新
    Retry        → emit(Running); attempt+1 启动同请求
    FinishOk     → 无事
    Fail(k)      → emit(Failed{kind:k}); 带 errors 时 emit(errors)
```

**信息局部性**：`queue`、`running`、`cancel` 全部是 task 私有字段——**调度状态没有任何一份在 task 之外**。外部只有 `UnboundedSender`，连读都读不到。emit 闭包由 src-tauri 注入（发 tauri 事件），scheduler 不知道 tauri 存在。

**事件输出契约**（emit 的两个载荷，即前端 compile-status / errors-updated）：

```rust
pub struct CompileStatusDto { pub phase: CompilePhase, pub kind: Option<FailureKind> }
pub enum CompilePhase { Queued, Running, Success, Failed }
pub enum FailureKind { Timeout, ContentError, Aborted }
// 时序：Queued(入队时) → Running(启动时) → Success / Failed
// 重试不重发 Queued，只重发 Running（attempt 对外不可见，前端不感知）
```

### 2.6 LatexmkRunner 实现（texpresso-infra）

```rust
pub struct LatexmkRunner { fs: Arc<dyn FileSystem> }

impl CompileRunner for LatexmkRunner {
    async fn compile(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome {
        // 1. 构造命令（算法见下）
        // 2. tokio::process::Command::new("latexmk").current_dir(&req.project_root)
        //    .args([...]).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).spawn()
        // 3. tokio::select! {
        //       _ = tokio::time::sleep(req.timeout)   => { kill_tree(pid); return Timeout }
        //       _ = cancel.cancelled()                => { kill_tree(pid); return Aborted }
        //       status = child.wait()                 => {
        //           if status.success() {
        //               let src = tmp/<root>.pdf; let dst = project_root/<root>.pdf;
        //               copy(src → dst.tmp) → rename(dst.tmp, dst) 成功 → Success{ pdf_path: dst }
        //               （原子化：失败 → IoError，旧 PDF 保留不被截断）
        //           } else {
        //               let log = fs.read_to_string(tmp/<root>.log)?;
        //               ContentError { errors: log_parser::parse_log(&log).errors }
        //           }
        //       }
        // 4. kill_tree(pid)：Windows → taskkill /T /F /PID <pid>（树杀，ADR 已定）；
        //    Unix → 进程组 kill（v1 后置，Windows 首发）
    }
}
```

**命令构造算法**（engine → 参数映射）：

```
latexmk -xelatex -outdir=tmp -synctex=1 -interaction=nonstopmode <root_file 相对项目根路径>
XeLaTeX → -xelatex；PdfLaTeX → -pdf；LuaLaTeX → -lualatex
cwd = project_root（相对 input/include 才能解析）；输入用完整相对路径（嵌套根文件如 css/thesis.tex 也能编译）
产物：tmp/<root>.pdf（原子拷贝到项目根）；tmp/<root>.synctex.gz（SyncTeX CLI 用）；tmp/<root>.log（解析用）
```

**信息局部性**：runner 无内部状态（`&self` 不可变），一次调用完全独立；每次调用所需信息全部在 `CompileRequest` 里。

## 3. 项目子系统（project 大模块）

### 3.1 拆分

```
project（core）
├── model.rs       —— Project / RootCandidate / RootResolution 类型
├── scan.rs        —— 文件收集（IO 经 FileSystem trait）
└── root_detect.rs —— 根文件探测（纯逻辑）
```

### 3.2 FileSystem trait（core 定义，texpresso-infra 实现）

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
}
```

**设计决策 D4**：探测、日志解析、设置存储、命令面的文件读写全部经此 trait，core 因此无任何文件依赖；实现集中在 texpresso-infra（tokio::fs，ADR-0010）。

**### 3.2.1 paths.rs — 路径策略（D8）**：core 提供 `resolve_project_root` / `resolve_in_project`（已存在目标）/ `resolve_creatable_in_project`（新建目标：解析父目录后拼接）与 `PathError{NotFound, RootUnavailable, Outside, NotADirectory}`；命令面只把失败原因翻译成 `{ code, message }`，不再自己 canonicalize + 前缀比对。

否决"宽松回调注入"（每个函数手写闭包签名，接口面发散）。

### 3.3 scan.rs — 文件收集

```rust
/// 递归收集项目内全部 .tex（排除 tmp/ 与隐藏目录），不跟随符号链接（防环）。
pub async fn collect_tex_files(fs: &dyn FileSystem, root: &Path) -> io::Result<Vec<PathBuf>>;
/// 忽略规则（与 watch 共用同一函数，单一事实来源）
pub fn is_ignored(path: &Path, root: &Path) -> bool;   // tmp/ 前缀、.git 等隐藏目录、非 .tex
/// 文件树忽略规则（只藏 tmp/ 与隐藏项，树展示所有扩展名；与 is_ignored 不同）
pub fn is_tree_excluded(path: &Path, root: &Path) -> bool;
```

**算法**：DFS 递归；每个目录先 `read_dir`，逐项判定 `is_ignored`，目录递归、.tex 收集。**函数内**持有递归栈与缓冲；**模块内**无缓存（每次调用全量扫描——文件树重建与探测都调它，目录量大时再优化增量）。

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
  6. latexmk 的 "Run #" / 目录回显行 → 丢弃
  7. 其他                       → 若 current 正在聚合且行非空 → 追加；否则丢弃
```

**信息局部性**：`file_stack` 与 `current` 是 **parse_log 的函数内局部变量**，一次调用即生即灭——这是"函数内信息"的范例。模块无缓存、无全局。

**测试**：insta 快照——真实 latexmk 日志（成功/内容错误/超时/多文件）固化为 4 个用例；含注释误报、`\include` 嵌套的探测用例另立。

## 5. SyncTeX（synctex 大模块）

```
synctex（core）              synctex（texpresso-infra）
├── model.rs                 └── synctex.rs —— SyncTexProvider 实现
└── provider.rs
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

// texpresso-infra synctex.rs：spawn synctex 二进制（tokio::process），解析 stdout
pub fn parse_forward_output(text: &str) -> Result<SyncTexPosition>;  // 纯函数，可单测
pub fn parse_inverse_output(text: &str) -> Result<SourcePosition>;   // 纯函数，可单测
```

**算法**：CLI 输出契约**已 Windows 实测定稿**（2026-08-25，见 ADR-0008）：`view` 可能返回多个 `Output:` 块 → `parse_forward_output` 取**首个完整块**；`edit` 输出 `Input:`（正斜杠 + `./` 路径）与 `Column:-1` → `parse_inverse_output` 处理，`Input` 路径需经 `project.resolvePath` 归一化。两者均以真实 Windows 输出固化单测（`provider.rs`）。pdf 路径指向 `tmp/<root>.synctex.gz` 对应 PDF 的**项目根副本**（synctex 按文件名关联），必须传 `-d <tmp>` 参数。

**信息局部性**：provider 无状态；一次调用 = 一次 spawn + 一次解析。前端高亮 overlay 与滚动恢复是预览模块自己的事，不流入此模块。

## 6. 设置（settings 大模块）

```
settings（core）              texpresso-infra
├── model.rs                  ├── storage.rs —— 读写盘、原子写、自写盘 hash 过滤（is_self_write）
├── merge.rs                  └── (watch.rs handle_settings_change —— 监视 → 重载 → 广播)
└── validate.rs
```

```rust
// core：纯逻辑
pub struct Settings {
    pub compile: CompileSettings,
    pub root_file: Option<PathBuf>,   // 项目级手动覆盖（探测结果的逃生门）
}
pub struct CompileSettings { pub mode: CompileMode, pub debounce_ms: u64, pub timeout_secs: u64, pub engine: Engine }

pub fn default_settings() -> Settings;
/// 项目覆盖全局，逐键合并（项目缺失字段继承全局）
pub fn merge(global: Settings, project: Settings) -> Settings;
/// 范围校验：timeout 5..=600、debounce 100..=2000、engine ∈ {xelatex,pdflatex,lualatex}
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

**算法（merge）**：字段级 Option 语义——全局 `settings.json` 与项目 `.texpresso/settings.json` 同 schema（含 `schema_version`）；项目文件只写它覆盖的键，其余继承。

**热更新（设计决策 D6）**：watch 识别 `.texpresso/settings.json` 变化 → 重载 → 广播 `settings-changed`。**自写盘过滤**：`update_settings` 写盘时记录 `(path, content_hash)`；watch 事件到达时比对 hash，相同则跳过（防"自己写 → 自己重载 → 重复广播"）。hash 存在 `storage.last_write` 内，不跨模块。

**信息局部性**：core 的合并/校验是纯函数；全局设置快照是 §1 清单里唯一的 `RwLock` 共享态——写者只有 storage 模块，读方（组合层构造 CompileRequest 时）只取一次性快照拷贝，不持有引用。

## 7. 监视与触发组合（texpresso-infra）

```
watch.rs          —— notify 8.2 接线：事件规范化 + 过滤
compose.rs        —— 组合层：文件事件 → 编译请求（D3 的关键翻译，位于 core，由本层调用）
```

```rust
// watch.rs：notify 事件流 → 项目内路径
// 过滤规则（与 project::is_ignored 共用）：
//   .tex（排除 tmp/）     → compose.on_tex_changed(path)
//   settings.json（全局或项目）→ 热更新（is_self_write 过滤后重载 + 广播 settings-changed）
//   其余                   → 丢弃
// 每个被接受的事件同时旁路广播 files-changed{paths}（前端文件树防抖重建）

// compose.rs：持有 project state + settings 快照，构造请求
pub fn on_tex_changed(&self, path: PathBuf) {
    let root = self.project.root_file();        // 读项目状态快照
    if root.is_none() { return; }               // 无根文件不编译（探测中/未设置）
    let req = CompileRequest {
        root_file: root,
        project_root: project.root,
        engine: settings.compile.engine,        // 一次性拷贝
        timeout: settings.compile.timeout,
    };
    self.scheduler.send(Compile(req));          // 唯一入口
}
```

**设计决策 D3（翻译层）**：scheduler 只认识 `CompileRequest`，不认识文件、项目、设置。文件事件 → 请求的翻译在组合层完成。否决"watch 直连 scheduler 传路径"：scheduler 被迫依赖项目状态与设置，违背最小外部依赖。手动编译 `compile_now` 走同一函数——**所有触发源收敛到同一个入口**。

**信息局部性**：组合层每次构造请求都取**快照拷贝**，不持有任何引用；请求发出后与触发源完全解耦。

## 8. 命令面实现（src-tauri commands.rs）

全部命令：DTO 进出、无业务逻辑；路径类参数一律**项目根内校验**（core `project::paths`：canonicalize + 根内前缀检查，防任意路径读写——自建命令没有 Tauri 权限模型兜底，这是安全底线）。文件与进程调用一律经 core trait，命令面不出现 `tokio::fs` / `std::fs` / `std::process`（ADR-0010）。

| 命令 | 实现要点（算法） |
|---|---|
| open_project(folder) | 校验目录 → 加载项目设置 → 探测根文件（有 root_file 覆盖则跳过探测）→ 更新项目状态 → 返回 ProjectInfo（**含 `root_candidates`**）；探测为 Multiple → 前端弹窗后 update_settings 补 root_file |
| get_project | 只读返回当前 ProjectInfo（roadmap P0-②-1）：root_file 变化后前端重新同步用；`root_file` 已指定 → 候选为空，否则重新探测（与 open_project 同语义） |
| list_dir(path) | 递归 `collect_tex_files` 变体（返回全树 DirEntryInfo，含目录；前端防抖重建用） |
| read_file | `FileSystem::read_to_string` + 路径校验（core `project::paths`） |
| save_all | 写盘（`save_content`）——不触发编译逻辑，watch 自然驱动；唯一保存路径 |
| compile_now | compose.compile_request_manual：只看 root_file（忽略活动文件路径），构造请求入队 |
| abort_compile | scheduler.send(Abort) |
| synctex_forward / inverse | 调 provider，错误 → { code, message } |
| get_settings / update_settings | 读快照 / apply_patch → 校验 → 写盘（记录 hash）→ 广播 settings-changed。**root_file 分支**（roadmap P0-②-1）：`Some(rel)` 先按 D8 解析为项目内绝对路径（失败即拒绝、**不落盘**）；`null` → 回到自动探测（复用 `detect_root`）；随后**同步内存 `ProjectState.root_file`**——此前只更新覆盖与有效设置，症状是「选了根文件仍报未确定根文件，必须重开项目」 |

## 9. 前端模块（函数级）

### 9.1 services（唯一碰 IPC 的层）

```ts
// ipc.ts —— specta 生成 + 薄封装（类型全自动，本文件不手写任何 DTO）
export const ipc = { openProject, listDir, readFile, saveAll,
                     compileNow, abortCompile, synctexForward, synctexInverse,
                     getSettings, updateSettings }  // 均为 Promise<T>

// events.ts —— 订阅一次，分发到各 store（内部经 useXxxStore() 取实例）；返回取消函数
export function subscribeEvents(): () => void
// 映射表（单向：事件 → store 动作）：
//   compile-status → compileStore.setStatus
//   errors-updated → compileStore.setErrors
//   pdf-updated    → previewStore.reload
//   files-changed  → editorStore.onFilesChanged(paths)（过滤+重载判定）
//                     projectStore.refreshTreeDebounced(structural)（300ms 防抖；仅结构变化重建）
//   settings-changed → settingsStore.setSettings
```

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
| projectStore | project、rootFile、fileTree | openProject、refreshTree、refreshTreeDebounced、resolvePath |
| editorStore | openTabs[]、activePath、dirtyPaths:Set、lastSaved:Map<path,time>、buffers、externalConflict | openFile、closeTab、markDirty、markSaved、saveAll、onFilesChanged、acceptExternal |
| compileStore | phase、kind、errors[] | setStatus、setErrors |
| previewStore | pdfPath、reloadKey、highlight | onPdfUpdated、setHighlight |
| settingsStore | settings | setSettings、updateSettings |

**前端自保存过滤算法（editorStore.onFilesChanged）**：入参 paths 中，`lastSaved` 里存在且时间近（< 2s）的路径判定为"自己刚保存"→ 忽略；其余 → 已打开且不脏 → 重载内容；已打开且脏 → 保留 + 状态栏提示；未打开 → 忽略（文件树自会刷新）。`lastSaved` 是 editorStore 模块内状态，不进任何函数参数。

### 9.3 composables

```ts
// useAutoSave：防抖保存算法
// 输入事件：Monaco onDidChangeModelContent（仅当前活动文件）
// 状态：timer（函数内/组件内）——信息局部性：计时器不出 composable
// 算法：每次变更 → 重置 500ms 计时器 → 到点 → saveAll(dirtyPaths) → 成功才清 dirty
// 取消：组件卸载时 clearTimeout（防泄漏）
```

### 9.4 组件数据流

| 组件 | 输入（props/事件） | 输出（emit） | 模块内信息 |
|---|---|---|---|
| EditorPane | model(路径)、内容、语言 | 变更事件 → useAutoSave | Monaco 实例、worker、IME 组合状态；**无活动文件 → 显示"还没有打开文件"占位提示**（覆盖 Monaco）+ `readOnly`，打开文件才可编辑 |
| PreviewPane | pdfPath、highlight | 点击坐标 → useSyncTex | pdf.js 文档句柄、滚动位置缓存 |
| FileTree | 树数据、激活路径 | 打开文件/目录展开 | 展开状态（只在前端本地） |
| RootFilePicker | root、candidates（探测候选）、fallbackFiles（零候选时全部 .tex）、busy、error | `select`（**项目内相对路径**）、`close` | 无（纯展示；相对路径由 `relativizePath` 计算） |
| ErrorList | errors[] | 点击条目 → openFile+定位 | 无；**诊断展示**（roadmap ④）：条目带 `diagnosis` 时渲染两行（原因 + 建议），无诊断降级为原文首行；头部「已诊断 N」 |
| StatusBar | compileStore/editorStore/projectStore 只读投影 + 「未确定根文件」可点击入口（emit `pick-root`） | `pick-root` → App 打开根文件选择器 | 无 |

## 10. 通信契约总表（冻结）

**事件（tauri emit → TS 类型，specta 生成）**：

```ts
compile-status: { phase: 'queued'|'running'|'success'|'failed', kind?: 'timeout'|'content_error'|'aborted' }
errors-updated: ErrorEntry[]                       // 失败时携带；编译启动时清空由前端 setStatus('running') 触发
                                                   // ErrorEntry = { message, file, line, kind, diagnosis }
                                                   // diagnosis（roadmap ④）= { kind, cause, hint } | null
pdf-updated:    { path: string }
files-changed:  { paths: string[], structural: boolean }   // structural=true 仅增/删/重命名（文件树重建）；内容修改为 false（跳过）
settings-changed: Settings
```

**命令（invoke）**：见 §8 表，输入输出全部是 DTO。

**跨模块禁止**：共享可变引用、模块间互调私有函数、事件载荷携带非 DTO 对象、scheduler 感知项目/设置。

## 11. 设计决策记录（本轮分叉点）

| # | 决策 | 否决的备选 | 理由 |
|---|---|---|---|
| D1 | 调度器 = actor（单 task + mpsc），状态收容 task 内 | 共享锁状态机 | 零全局可变状态；单写者免锁；合并队列天然适合串行处理 |
| D2 | 超时/取消在 runner 内（CancellationToken） | 调度器注入时钟管超时 | scheduler 无时钟无进程概念；单测喂假 outcome 即可 |
| D3 | 组合层翻译"文件事件→CompileRequest"，scheduler 不感知项目 | watch 直连 scheduler 传路径 | 最小外部依赖；所有触发源收敛单一入口 |
| D4 | core IO 经 FileSystem trait（read_dir/read_to_string 两方法） | 每函数手写回调注入 | 接口面最小、单一事实来源 |
| D5 | 根文件探测正则启发式 | 完整 TeX 词法解析 | 成本收益不成比例；局限已记录，root_file 覆盖是逃生门 |
| D6 | 设置热更新 + 自写盘 content_hash 过滤 | 重启生效 / 不设防 | 防"自己写→自己重载"循环，过滤状态收在 storage.last_write |
| D7 | 前端 store 单向依赖 + events.ts 统一分发 | store 互引 / 事件总线库 | 依赖图可推理；避免引入总线抽象 |
| D8 | 自建命令路径校验（项目根内 canonicalize 前缀） | 信任前端传入路径 | 自建命令无 Tauri 权限模型兜底，必须自守 |

## 12. 与上层文档的关系

- architecture.md §2/§3/§5 的模块表在本文件展开为函数级；**本文件冻结后，architecture.md 的模块表不再单独细化**
- 新增后置项：~~`parse_forward_output`/`parse_inverse_output` 输出契约~~（**已 Windows 实测定稿** 2026-08-25：`view` 可能多块、取**首个完整块**；`edit` 用 `Input:`/`Column:-1`，`Input` 路径为**正斜杠+`./`**需归一化——见 ADR-0008）；~~文件树增量刷新~~（**已实现** 2026-08-25：`files-changed` 载荷加 `structural`，仅**结构变化（增/删/重命名）**才防抖重建，内容修改如自动保存跳过——见 §7 watch.rs + §9.2 project.ts；后端 `is_structural_event`）；~~错误列表去重/截断~~（**已实现** 2026-08：前端按「文件 + 首行消息」聚合同源错误并展示 ×N 计数，最多展示 30 组，超出的提示隐藏数量）
- **pdf.js 重载耗时实测（2026-08）**：`fetch≈7ms / parse≈103ms / render≈320ms / total≈400–450ms`（`multifile`，见 design.md）。**render 是瓶颈（~75%）**——`PreviewPane.load()` 每次强制 `canvasEpoch++`（整页 canvas DOM 重建）+ 视口页全量重绘 + 二次 `renderNearViewport`。**A/B 优化已实现（2026-08）**：
  - **A. 分页 DOM 虚拟化**：只挂载视口窗口内页（`mountStart..mountEnd`，前后各 `PAGE_WINDOW=6`），顶部/底部占位撑住总高度（滚动条稳定）；`renderNearViewport`/`updateCurrentPage` 只遍历窗口内页 → **由 O(总页数) 降为 O(视口)**。移除 `IntersectionObserver`，改 scroll 驱动 + 窗口变化 watcher + 容器 `ResizeObserver`。
  - **B. 选择性 canvas 重建**：`canvasEpoch` → `structuralEpoch`，**仅缩放/换文档才 `++`（重建 DOM）**；同文件内容重载**复用 DOM**（`doRenderPage` 每次 `canvas.width=` 即重置 2D context；每页串行链已 cancel+await，黑屏/反转机制在代码层排除）。`pageH1`（scale=1 页高）同文件重载**保留** → 滚动恢复精确。
  - **验证**：`vue-tsc --noEmit` + `vite build` 通过；`npm run tauri dev` 真实窗口自动打开项目、点「编译」按钮生效（前端已渲染、click handler 触发 `compileNow`）、`main.pdf` 重新生成、前端未崩溃。⚠️ 首测**像素级「黑屏/文字反转」视觉确认与 `__previewLastReload` 时序读取受限**（截图工具捕获到错误窗口内容；e2e 手动二进制路径按 troubleshooting 记录前端不渲染）——故当时未声明新的耗时数字，A/B 收益为代码层面（避免整页 canvas DOM 重建 + O(N) 遍历）。
  - **真实窗口复测（2026-08-25，tauri server MCP 驱动）**：`npm run tauri dev` + `VITE_TEXPRESSO_PROJECT=…/test_file/projects/multifile` 自动打开项目 → 点「编译」→ `main.pdf`（3 页 / 108KB）重载。**像素级视觉确认通过**：标题页/目录/正文渲染正常，无黑屏/文字反转（此前受限点已解决）。**插桩修正**：原 `render`＝setup 时间、`pagesRendered` 恒为 0，无法反映渲染瓶颈；改为 `load()` 等本次挂载窗口渲染链全部落盘后再取 `tDone` → `render` 为真实 canvas 绘制耗时。
  - **实测（同文件复用路径，multifile 3 页）**：`fetch≈9–10ms / parse≈30ms / render≈59ms / total≈98–100ms / pagesRendered=2`（视口内 2 页绘制、远页释放）；首次换文档 `fetch≈6ms / parse≈29ms / render≈77ms / total≈112ms / pagesRendered=3`。**render 占总耗时 ~59%，仍为 PDF 重载开销主因**（与先前结论一致）；3 页小文档总耗时 ~100ms，远低于延迟预算。注：先前 `render≈320ms / total≈400–450ms` 是更大多页 `multifile`（12 页）的数值，与本次 3 页文档不可直接对比。插桩保留：`window.__previewLastReload` + 控制台 `[preview] reload#N` 日志。
  - **受控 A/B 对比（2026-08-25，同一 31 页大文档 `benchmark`，两版代码同一插桩）**：重构前（0be1977^，全量挂载）vs 重构后（虚拟化 + 同文件复用）——**DOM 节点 ~4.4× 减少（31 → 7 canvas）**；同文件复用路径 `render 49 → 21–28ms`、`total 89 → 62–69ms`、`pagesRendered 9 → 2`（重构后更激进离屏释放，仅保留视口内 ~2 页）。**结论**：虚拟化带来的 DOM 减量是受控、无歧义的核心收益；render/total 下降含「渲染页数变少（9→2）」因素，但同文档下总耗时仍明显下降（62–69 vs 89ms）。`test_file/projects/benchmark/` 为基准工程（未提交）。注：历史 `render≈320ms/total≈400–450ms`（12 页、旧插桩）为另一文档/插桩，仅为背景。
- **增量编译基准结论（2026-08）**：见 design.md「延迟预算实测与结论」——latexmk 增量=整份文档单遍重排（引擎特性），确认**暂不过 latexmk**。
- **SyncTeX 双向真机验证与定稿（2026-08-25，tauri server MCP 驱动真实窗口）**：① **正向**（源码 Ctrl+点击 → PDF 高亮 overlay）在 Monaco 上以带 `ctrlKey` 的 mousedown+mouseup 触发，`synctex view` 返回 Page/x/y → `setHighlight` → `.highlight` 盒显示（视觉确认，无黑屏/反转）；② **反向**（PDF 点击 → 源码跳转）点击 canvas 触发 `onCanvasClick → inverse → openFile` 并揭示行；③ **契约定稿**（ADR-0008）：`view` 多块取首个完整块、`edit` 输出 `Input:`+`Column:-1`，均以真实 Windows 输出固化单测（provider.rs）。④ **发现并修复 bug**：反向返回的源路径为 **正斜杠 + `./`**（如 `E:/…/test_file/projects/multifile/./main.tex`），`project.resolvePath` 对绝对路径不归一 → 会**重复打开 main.tex 标签**；改为 `normalizePath`（剥 `.`/合并 `..`/折叠斜杠，浏览器手写不依赖 node:path）后反向命中正文**复用单个已开标签**（实测 `tabs:["main.tex"]`、揭示到行）。⑤ 注意：点击 PDF **目录区**会映射到生成文件 `main.toc`（synctex 特性），正文区才映射回 `main.tex`。
- **已实现（2026-08-25）**：① **修复 SyncTeX 正向定位到「未加载页」无法一次跳转到位**——`renderPage` 改返回渲染链 promise（`await` 真正等渲染完成 + `setHeight` 记录页高），跳转前预热目标页及之前页高、加 `nextTick`，并改**瞬间定位**（`behavior:"auto"`，去掉 smooth 中途布局变化导致跳不到位）；② **页码跳转**——预览工具条页码指示器改为**可输入**（`1 / 15` → 输入框 `/ 15`，回车/失焦触发 `goToPage(n)`）。`goToPage(n)`：展开窗口 + 预热页高 + 渲染 + 居中滚动（页码输入与 SyncTeX 正向共用）。
- **已修复（2026-08-26）「调整放大倍率后 PDF 消失 + 滑动条向下拖无效」**：根因是 `.page-wrap` 高度由 canvas 尺寸驱动——`releasePage`/缩放重建（`structuralEpoch++`）把 canvas 置 0×0，窗口内已释放的页-wrap 塌缩到 ~0 高（仅剩 18px 边距）；而 `topSpacerH`/`bottomSpacerH` 只对**窗口之外**的页用真实高度（`pageH1×scale`）。于是窗口内页**高度流失** → `scrollHeight` 变短（滚动条拖不到真实末尾），且 `renderNearViewport` 用 `getBoundingClientRect`（0 高）判断 `near`，把本应渲染的近页误判为远页而 `releasePage`（PDF 消失）。**快速多次缩小**触发：每次缩放 `structuralEpoch++` 重建全部 canvas（0 尺寸）且保留 `pageH1`，正是暴露「页-wrap 高度流失」最彻底的路径（按用户提示复现，无需滚动即触发）。
  - **修复**：① `.page-wrap` 绑定 `:style` 高度 = `pageH1[n]×scale`（`pageWrapHeight(n)`）——布局永远由页高驱动、不依赖 canvas 尺寸；释放/重建也保留真实页面高度。② 新增 `layoutRev` ref，`setHeight` 每次记高后 `++`；所有依赖页高的 computed（`topSpacerH`/`bottomSpacerH`/`pageWrapHeight`）引用它以获响应式（`pageH1`/`prefixH1` 是普通数组非响应式，否则 warmHeights/逐页渲染填入页高时不重算布局）。③ `.page-wrap canvas { display:block }` 杜绝内联 canvas 在强制高度下的基线缝隙。
  - **验证**（tauri server MCP 真实窗口 + `multifile` 15 页）：50% 快速缩小 5 次 → 8 页挂载全部保留真实高度、`scrollHeight`=真实全文（15×页高+间距）、滚到底 `scrollTop==maxScroll`（第 15 页挂载+渲染、`currentPage=15`）；175% 快速放大 5 次 → 页高正确（`pageH1×1.75`）、滚到底仍 `currentPage=15`、近页绘制非空白；「适应宽度」正常。`vue-tsc --noEmit` + `vite build` 通过。
- **已修复（2026-08，全仓 code review High 级，见 docs/code-review.md）**：① **open_project 跨项目设置污染**——`open_project` 改为从磁盘读**纯全局** `load_global`（此前读 `state.settings`，可能是上个项目合并后的 `effective`），导致打开第二个项目继承第一个项目的覆盖值（`root_file`/`mode`）；现与 `update_settings`（读纯全局）一致（`commands.rs`）。② **嵌套 `root_file` 编译失败**——`runner` 用 `latexmk_input`（相对项目根的完整路径）替代仅取 stem 的 `{stem}.tex`；嵌套根文件（如 `css/thesis.tex`）可编译，产物仍按 jobname basename 落 `tmp/<stem>.pdf` 并拷贝到项目根（`runner.rs`）。③ **root_file 覆盖路径越界**——`open_project` 解析覆盖值后 `canonicalize` + `starts_with(项目根)` + `.tex` 校验；`validate_overrides` 增「`root_file` 形式校验」（拒空/`..` 组件）。④ **useAutoSave 陈旧保存数据丢失**——`run()` 保存成功后仅对「缓冲区仍等于已保存内容」的路径清脏；保存期间变化的路径保持 dirty 并重排保存，避免关闭时 `flush()` 因 `dirty` 为空丢失最新输入（`useAutoSave.ts`）。`cargo test -p texpresso-core`（94 pass）+ `cargo check -p texpresso` + `vue-tsc --noEmit` 通过。
- **已实现（2026-08，code review §3 「on_save 模式实际未实现」）**：`onEditorChange` 现仅在 `mode === "continuous"` 时调用 `autoSave.schedule()`；**on_save 模式不自动写盘**，改由 **Ctrl+S / 点「编译」/ 关标签** 触发 `flush()`（写盘后经 watch 触发编译）。`manualCompile` 先 `flush()` 落盘再 `compile_now`（合并队列吸收重复）。连续模式编辑仍走防抖自动保存。
- **已修复（2026-08，code review 前端 Medium）**：① `editor.openFile` 去重竞态——去重判断移到 `await readFile` 后复检（并发的树节点双击不再重复开标签）；② `editor.onFilesChanged` check-then-await 竞态——读取后复检 `dirty`，脏则保留本地 + 冲突标记（不覆盖最新输入）；③ `compile.setStatus("success")` 清空 `errors`/`hasError`（无 running 前置时旧错误不残留）；④ `EditorPane` 3 个 Monaco 事件订阅显式收集并随卸载 dispose；⑤ `PreviewPane` 增加 `unmounted` 守卫（在途 load 不再写 `__previewLastReload`/标题、catch 不误报）、卸载时 `cancelAllRenders()`、`onCanvasClick` 包 try/catch（卸载期间点击不产生未处理 rejection）。新增 `compile.spec.ts` 与 `editor` 并发去重回归测试。
- **已实现（2026-08-26）「文档大纲（源结构树）」**：原底部面板右侧的「大纲（后置）」占位替换为真实大纲。**数据源**：跟随根文件（`root_file`）的 `\include`/`\input` 图，逐文件解析 `\part/\chapter/\section/\subsection/\subsubsection/\paragraph/\subparagraph`（含 `*`/`[short]`），按**文档顺序**生成嵌套标题树（先当前文件目录、再项目根解析包含目标，防环 visited）。**实现**：`src/stores/outline.ts`（`LEVEL` 层级表、`SECTION_RE`/`INCLUDE_RE`、`resolveInclude`、`parseFile`、`buildTree` 栈式嵌套）+ `src/components/OutlinePane.vue`（拍平后按 depth 缩进渲染，level 色标、file:line、当前文件高亮）。**交互**：点击项 → `editor.openFile(file, line)` 揭示源码 + `useSyncTex.forward(file,line,0)` 高亮/居中 PDF 对应页（SyncTeX 不可用则仅跳源码）。**刷新**：项目打开（App.vue）+ 编译成功 + 结构变化（files-changed structural，events.ts）。**取内容**：打开标签用 `editor.buffers` 实时缓冲（未落盘也反映），否则读盘。**验证**（tauri MCP 真窗口，multifile）：大纲 15 项按 `include` 图正确嵌套（章→节、部→章）；点击「表格」→ 编辑器切到 `tables.tex` Ln 3、点击「附录」→ 跳到 `appendix.tex` Ln 3 且 PDF 视口跳第 14 页、`.highlight` 盒可见（computed `display:block` 恰好 1 个）。`vue-tsc --noEmit` + `vite build` 通过。
- **已调整（2026-08-26）「大纲位置迁移 + 错误栏默认折叠」**：① 大纲从底部面板右侧移到**左侧栏**，与文件树**上下分布**（`SplitPane direction="horizontal"`：文件树在上 primary、大纲在下 secondary，宽栏比例 0.55），不再与错误列表左右并排；② 底部面板右侧腾出后，展开时错误列表**全宽**显示；③ `bottomCollapsed` 默认 `true`（错误栏默认折叠成 30px 细条，头部显示「报告 · 状态 · 展开」），点击头展开/收起。验证（tauri MCP 真窗口）：左栏 `split-pane horizontal`（文件树在上、大纲在下）、底部默认折叠（31px、内容隐藏）、点击展开后错误列表占满底部宽度（1707px）。`vue-tsc --noEmit` + `vite build` 通过。
- **已修复（2026-08，近期 code review 优先级 1-4，见 docs/code-review-recent.md）**：① 大纲 `refresh()` 并发守卫（loadSeq supersede 防陈旧覆盖）+ `events.ts` 的 `void refresh()` 改 `.catch()`；② `storage.load_overrides` 由「整包丢弃」改为**逐字段清洗**（`sanitize_overrides`：越界/非法 `root_file` 置 None 回退全局，保留合法覆盖，避免连带丢弃合法 compile 覆盖）；③ 大纲细节——`resolveInclude` 根相对优先（TeX `\input` 相对项目根）、跳过 `verbatim` 环境、`OutlinePane` 稳定 key（`file:line`）；④ `SplitPane` 补显式 `.split-pane.vertical` 规则（防依赖默认 flex 行为的脆弱性）。
- **后置/优化点（2026-08-26，见 docs/code-review-recent.md §4 M2，**暂未实现**）**：大纲**每次编译成功都会全量重扫**——`events.ts` 在 `phase==="success"` 时调 `outline.refresh()`，递归读根文件 include 图下所有 `.tex` 并逐行正则扫描。大项目/高频编译下 IO 与解析成本显着，与延迟预算主题相悖。**现状**：已由 `refresh()` 的 `loadSeq` supersede 守卫消除并发重扫互相覆盖（M1）；全量重扫本身受编译频率约束（结构可能随编译变化，必要）。**优化方向（待做）**：a) 仅当「结构命令所在文件」（或即根文件/含 `\section` 等命令的文件）变化时才刷新；b) 复用上次解析的文件内容（缓存文件 mtime/内容 hash，未变的文件不重复读盘+扫描）；c) 对刷新做低频节流/去抖（合并编译成功 + files-changed structural 同窗触发）。若采纳，需记录「上次各文件内容/mtime」（解析已下沉 Rust 2026-09-03，缓存点应在 `texpresso_core::outline` 的 `load()`）并在 `events.ts` 把触发源细化为具体变更文件。
- **已实现（2026-08-26）「编辑器 v1.1 增强：折叠 / 多光标 / 代码片段」**：原 design.md §编辑器 v1.1 的规划项落地（纯前端，Monaco 原生 + provider，无需 Rust）。**实现**：新增 `src/latexSuggest.ts`——① 代码片段补全：`registrationCompletionItemProvider`（`InsertAsSnippet`，trigger `\`/`{`），覆盖文档骨架/环境（begin/end 名同步）/章节/数学/格式/文件操作等 ~60 条，`provideCompletionItems` 按当前词前缀过滤并回填 `range`；② 环境块折叠：`registerFoldingRangeProvider('latex')`，按 `\begin{env}`…`\end{env}`（含嵌套）生成 `FoldingRange`（0-based 行号）。`main.ts` 注册语言后调 `registerLatexProvider()`。`EditorPane.vue` 编辑器选项：`folding:true`、`tabCompletion:'on'`、`snippetSuggestions:'inline'`、`quickSuggestions`、`showSnippets`、`multiCursorModifier:'alt'`（**避免**设为 ctrlCmd 与 SyncTeX Ctrl+点击冲突）、`multiCursorPaste:'spread'`。**验证**（tauri MCP 真窗口 + pc-control 真实键盘）：① 折叠——math.tex 环境块点击折叠箭头后 expanded→collapsed（块折叠）；② 片段——键入 `\sec` 触发建议（section/subsection/subsubsection 含中文注释），Tab 展开 `\section{}` 且光标落在占位符；③ 多光标——Ctrl+Alt+Down 加第二光标后键入 `X` 同时出现在两行。`vue-tsc --noEmit` + `vite build` 通过。
  - **折叠 off-by-one 修复（2026-08-27）**：Monaco `FoldingRange.start/end` 为 **1-based** 行号（`monaco.d.ts`：*"The one-based start line…"*），初版传 0-based `i`致折叠锚点**整体上移一行**（箭头落在 `\begin` 前一行、折叠后露出 `\end{...}`）。改为 `start+1`/`end+1`（1-based）后：箭头对应当行 `\begin{env}`、折叠后 `\begin{env}` 为头、`\end` 被隐藏。真机复测 3 个环境块箭头均对齐 `\begin{}` 行。
- **代码 review 重审确认与优化（2026-08-27，B1-B6）**：`src/latexSuggest.ts`。
  - **B1（重点，虚警）**：Monaco `SnippetParser._parseEscaped` 只把 `\$`/`\}`/`\\` 当转义，**其余 `\x` 一律保留字面 `\`**（`\b`→`\`+`b`）。故片段体用单反斜杠（TS `\\begin`→JS `\begin`）插入即得真实 `\begin{...}`，**无需写 `\\\\`**；与真机 `\section{}` 现象一致。已在代码注释标明该约定。
  - **B2（已修）**：`triggerCharacters` 去掉 `{`，仅留 `\`（避免带参数处列出全部片段造成噪音）；真机 `\sec` 仍正常触发建议。
  - **B3（已修）**：折叠前剥离行内注释（`%` 之后）且跳过含 `\verb` 的行，避免注释/字面量里的 `\begin{}`/`\end{}` 生成伪折叠区；math.tex 含 `\verb|\begin{document}|` 行不再产生伪 `document` 折叠（折叠数仍为 3 个真实环境）。
  - **B4（接受）**：畸形/未闭合环境（begin A, begin B, end A）按就近配对、其余项忽略——对非良构文档属合理降级，不处理。
  - **B5（接受）**：`sortText: s.label` 字母序对已前缀过滤的结果已足够，暂不加权重。
  - **B6（确认安全）**：`main.ts` 无条件 `registerLatexProvider()`——其前有 latex 语言注册守卫，安全。
  - **B7（已修复，用户报告 `\sec`→`\\section`）**：Monaco `getWordUntilPosition` 的单词不含 `\`（`\sec`→word=`sec`，startColumn=2）。**修法**：range 保持 = 当前词范围（`word.startColumn..word.endColumn`，否则 Monaco 会因 range 与当前词不一致而**过滤掉该建议**致列表空白——一次误用「range 前延含 `\`」的方案即触发此问题，见下）；改为**当词首前一字符是 `\` 时，剥掉片段自带的前导 `\`**（`\section`→`section`），替换后保留的 `\`+`section{}`=`\section{}`（单反斜杠）。验证（独立 scratch Monaco 编辑器，Monaco API 驱动，无 IME 干扰）：`\sec` 触发建议显示 `section`（含中文注释），接受后插入 `\section{}`（首字符 92、光标落占位符）。**教训**：Monaco 建议项的 `range` 必须与当前词 `getWordUntilPosition` 对齐，否则不显示；`\\section` 根因是残留前导 `\` 与片段自带 `\` 拼接，故用「剥前导 `\`」而非「扩 range」。
- **同类问题排查（2026-08-27）【补强 1 处，其余已隔离/一致】**：针对近期「`\\section` 双反斜杠 / `\sec` 无建议 / main.tex 被覆盖」做同类扫描——① **Monaco 补全 range**：全仓仅 `latexSuggest.ts` 一个 completion provider，range 已修（=当前词范围），无其他 provider 存在同类 range 不匹配风险；② **片段/字符串反斜杠转义**：仅该文件用 `InsertAsSnippet`，无他处会过度转义/拼接；③ **1-based/0-based 偏移**：Monaco 行 API（`setPosition`/`revealLineInCenter`/`getLineContent`）皆 1-based，outline 行 = `i+1`、error 行 1-based、preview 页 1-based 一致；仅 `FoldingRange` 为特殊情况（1-based，已修正）；④ **模型覆盖 / 数据丢失**：`EditorPane` 的 `model.setValue` 有「内容不同才 set」守卫，`editor.onFilesChanged` 对 dirty 复检，buffer 为事实来源——无应用代码路径会任意覆盖真实文件（此前 main.tex 被覆盖系测试钩子 `model.setValue` 误操作，非应用 bug）。**已补强**：outline `parseFile` 原先只跳行首 `%` 注释，未像折叠 B3 那样剥离**行内** `%` 注释——`正文 % \section{隐藏}` 会生成伪大纲项；改为先 `split("%")[0]` 剥注释再匹配（verbatim 状态检测在剥注释后进行），与折叠一致。验证：multifile 大纲仍 15 项、结构正确（无回归）。
  - **片段反斜杠转义同类排查（2026-08-27）【修复 4 处】**：继续深究「其他关键字导致 `\` 错误」——Monaco `SnippetParser` 只把 `\$`/`\}`/`\\` 当转义，故两类片段体写法会出错：① **`\` 紧贴 `${占位符}` 前**（`\newcommand{\\${1:cmd}}`→`\$` 被转义成 `$`，占位符变**字面量** `${1:cmd}` 文本、且丢失 `\`）——`setlength`/`newcommand`/`renewcommand` 三处，改为 `\\\\${1:...}`（JS 值 `\\${1:...}`→Monaco `\`+占位符，保留字面 `\`）；② **LaTeX 行换行 `\\`**（`tabular` 的 `\\\\`→Monaco 只产出**单个** `\`，应为 `\\`），改为 `\\\\\\\\`（JS `\\\\`→Monaco `\\`）。**验证**（scratch Monaco 编辑器）：`\newc`→`\newcommand{\cmd}{}`、`\setl`→`\setlength{\parindent}{0pt}`（占位符 `\cmd`/`\parindent` 均为真实 tabstop 含 `\`，不再是字面量 `${1:...}`）；`tabular` 行换行现为 `\\`（codes 92×2）。其余片段体（`\begin`/`\ref`/`\textwidth` 等单 `\`）经 B1 规则逐一核对无误。
- **代码 review C1-C4（2026-08-27）【全部处理】**：新增 `src/texParse.ts`（LaTeX 行级剥离子程序，纯函数、不依赖 Monaco，可单测）。
  - **C1（外形）**：`latexSuggest.ts` 的 `latexCompletionProvider` JSDoc 内嵌第二个 `/**`，重排为**单块**整洁 JSDoc（B1/B2/B7 合并为多段，无嵌套）。
  - **C2（折叠，修）**：折叠原先按**整行**跳过含 `\verb` 的行——一行兼有 `\verb|y|` 与真 `\begin`/`\end` 会被漏折叠；改为 `stripTexComment` **只剥离 `\verb` 跨度**（`\verb*?<delim>…<delim>`），不再整行跳过。真机：math.tex 含 `\verb|\begin{document}|` 的行不产生伪折叠、3 个真实环境箭头仍对齐。
  - **C3（大纲，修）**：大纲原先 `split("%")[0]` 会在**转义 `\%`** 或 **`\verb` 内 `%`** 处截断（`\section` 跟在其后会被误切）；改为 `stripTexComment`（`(?<!\\)%` 只认未转义 `%`，先剥离 `\verb` 跨度），`\%\section{X}` 与 `\verb|%|\section{X}` 均正确保留 `\section`。
  - **C4（补测试）**：`shouldStripLeadBackslash(line,startColumn)` 抽出为可测单元，`__tests__/texParse.spec.ts`（6 例）锁定「词首前一字符是 `\` 判定」（依赖 Monaco word 不含 `\`，若改 wordPattern 需同步）。验证：vitest 34/34 通过（新增 6）+ vue-tsc + vite build 通过；大纲仍 15 项、折叠 3 环境正常。
  - **C5（边缘，修）**：`stripTexComment` 原用 `split(/(?<!\\)%/)[0]`——单字符否定 lookbehind 只判 `%` 前一字符，正确处理 `\%`（字面），但 `\\%`（LaTeX `\\` 换行紧接 `%` 注释）会把 `%` 误当字面保留。改为**扫描数 `%` 前连续反斜杠**：奇数→转义字面 `%`（`\%`、`\\\%`）、偶数（含 0）→真注释截断（`\\%`、`%`）。JS lookbehind 不支持变长，故用扫描。`__tests__/texParse.spec.ts` 增 `a\\%b→a\\`（C5）例，7/7 通过；vitest 35/35 + vue-tsc + vite build 通过。
  - **P3（可选项，已处理）**：① `\verb` 正则加 `line.includes("\\verb")` 短路（跳过多数无 `\verb` 行的正则，微优化）；② `shouldStripLeadBackslash` 的 `charCodeAt(2)===92` 改 `line[startColumn-2]==="\\"`（更直观）；③ 未闭合 `\verb` 不剥离（畸形 LaTeX 边界，接受）；④ 注释明确 `shouldStripLeadBackslash` 是**补全专用**启发式（放公用 texParse 仅为单测锁定，业务归 completion）。
- **outline 下沉 Rust（2026-09-03）「大纲解析等价重构（cli-mcp-plan.md §4 P1-4）」**：解析逻辑从前端 `src/stores/outline.ts` + `src/texParse.ts` 移入 `crates/texpresso-core/src/outline.rs`（纯解析函数 + `load` 编排 + `OutlineContext`{root/root_file/实时缓冲/兜底文件列表}），DTO `OutlineNode`（title/level/file/line/fileBase/children，serde `fileBase` 重命名经 specta 生效）进 `types.rs`；新增命令 `get_outline(buffers, files)`（specta 注册，`npm run tauri dev` 启动时重新导出 bindings.ts）。**前端**：`stores/outline.ts` 只留触发/传参/呈现（`items/isEmpty/refresh/goTo` API 不变；`OutlineNode` 从 bindings 再导出，`OutlinePane.vue` / `events.ts` / `App.vue` **零改动**）；`texParse.ts` 保留（折叠/片段补全仍用，注释已更新）。**等效性保证**：① Rust `regex` 不支持反向引用 → `\verb` 剥离开手写扫描器复刻 JS 正则回溯语义（先 `\verb*` 贪婪吃 `*`、失败回退把 `*` 当定界符、无闭合则逐字符推进找下一个 `\verb`、匹配后从末尾继续）；② `normalize_path`/`dir_of`/`join_path`/`resolve_include` 逐行对齐前端 `project.ts`（含**不**转换反斜杠、`C:` 盘符段不被 `..` 弹出、绝对路径越界 `..` 丢弃、扩展名 `\.[A-Za-z0-9]+$` 判定）；③ 遍历语义逐条保留：缓冲优先（未落盘也反映）→ 读盘、读失败跳过、visited 防环、根文件走 include 图、无根文件用前端文件树兜底列表、`\begin{verbatim}` 内跳过、行级剥离后匹配。**差异点（边界已判定可接受）**：读集安全为**词法前缀校验**（D8 词法版，core 无 canonicalize）——与 `read_file` 的 canonicalize 语义差仅剩「符号链接目标解析」与 8.3 短名（Windows 大小写不敏感 FS 下实际行为一致）；`\s`/`trim` 的 Unicode 空白集合有微差（U+0085 等，.tex 无影响）；无根文件时文件兜底列表仍由前端树提供（保持 300ms 防抖的旧语义，后端不自行扫描）。**验证**：core 新增 12 例单测（剥离/归一化/解析/建树/load 全链路，含 texParse.spec 对拍、`..` 越界包含跳过、cycle、兜底顺序、缓冲优先、无兜底自动扫描排除 tmp/）；`cargo test -p texpresso-core` **109 通过**、`cargo check -p texpresso`、`vue-tsc --noEmit`、`vitest 35/35`、`npm run build` 通过；**真实目录对拍**（`test_file/projects/multifile`：旧前端逻辑 Node 忠实移植 vs Rust `load()`，临时探针已清理）：**15 项逐字段一致**（depth/title/level/file/line/fileBase；含 `main.tex` 的 `\part` 项「反斜杠路径」这一旧行为怪癖——目录段取自未经归一化的根文件路径）。真机窗口 UI 复测（tauri server MCP）待有该工具集的会话补做。
- **中文文件名/路径兼容性实测 + 日志编码修复（2026-09，roadmap P0-①）**：完整实测记录见 [troubleshooting.md](./troubleshooting.md)「中文文件名/路径兼容性实测」。**结论**：路径层面的中文全链路可用（`latexmk` 编译、`tmp/` 中文名产物、App 真机打开/监视/编译并产出 `中文主文件.pdf`、`synctex view/edit` 中文绝对路径双向、`.log` 中文文件名解析、`\\?\` 剥离、中文配置目录读写）；**唯一真实缺陷在日志编码**。**修复**：新增 `texpresso_core::log_parser::decode_log(bytes)`（严格 UTF-8 优先、失败则 lossy）——GBK 源文件经 **`pdflatex`** 会把非法字节回显进 `.log`（实测 73 个，xelatex 则会自行替换），严格 `read_to_string` 读取失败会让「编译失败」退化为「拿不到任何错误信息」；`project::FileSystem` trait 增 `read_to_string_lossy`（**默认实现退化为严格读取**，`TokioFs` 覆盖为 lossy 解码），`runner` 读 `.log` 改走该方法。**验证**：core 新增 `unicode_path_tests`（7 例：归一化/`\include` 解析/根文件探测/大纲 include 图/中文前缀守卫/D8 越界拦截/`.log` 中文文件名带行号）与 `log_parser::decode_tests`（3 例，含「非法字节不破坏 ASCII 骨架 + `parse_log` 仍给行号」）——`cargo test -p texpresso-core` **119 通过**；**App 端到端**（`test_file/projects/中文GBK工程`，pdflatex + GBK 子文件）实测 dev stdout：`编译失败：已从 .log 解析出错误条目 count=9`（修复前该路径为 `编译失败且无法读取日志`，错误列表为空）；src-tauri 侧新增中文用例（`fs_impl`/`runner`/`storage`/`commands`，含 2 个需 latexmk 的 `#[ignore]` 集成用例）经 `cargo check -p texpresso --tests` 编译校验（本机 `cargo test -p texpresso` 因 WebView2 限制无法运行，见 troubleshooting）；`npm run build`（vue-tsc + vite build）通过。**未覆盖**：编辑 GBK 源文件（`read_file` 仍严格 UTF-8，本次未改）。**2026-09 补做（tauri server MCP）**：此前列为"未覆盖"的两项 GUI 目视项已用**截图 + DOM + 真实点击**结清——① pdf.js 经 asset 协议渲染中文路径 PDF 正常（截图见标题/中文目录/正文/公式，控制台 `[preview] reload#1 中文主文件.pdf pages=3 bytes=40648 fetch=9ms parse=32ms render=54ms total=94ms pagesRendered=3`，**fetch 无 404/编码错误**）；② 反向 SyncTeX 点 PDF 正文 → 编辑器切回 `中文主文件.tex` 且光标停在 **Ln 10**（点击句对应的源码行），并复现了「点目录区映射到 `.toc`」这一已记录特性。详见 [troubleshooting.md](./troubleshooting.md)「GUI 目视验证」。
- **基础设施层拆出 texpresso-infra（2026-09，ADR-0010）**：把 `fs_impl` / `runner` / `sync_cli` / `storage` / `watch` 从 src-tauri 迁入新 crate `crates/texpresso-infra`（与 core 同级），成为**外部依赖与文件系统的唯一落点**。**边界收紧**：① `FileSystem` trait 增 `canonicalize` / `is_dir` / `write`，命令面不再出现 `tokio::fs` / `std::fs` / `std::process`（D8 路径策略下沉为 core `project::paths`：`resolve_project_root` / `resolve_in_project` / `resolve_creatable_in_project` + `PathError`）；② watch 去掉 tauri 依赖——监视结果经新增的 `WatchSink` trait 回调，事件形态在 src-tauri `events.rs` 定型（`TauriSink`），async 任务改用注入的 `tokio::runtime::Handle`；③ src-tauri 只剩 commands / events / lib 装配，依赖面收窄（notify / tokio-util / async-trait / serde_json 全部移出）。**验证**：`cargo test -p texpresso-core` 127 + `cargo test -p texpresso-infra` 17 通过；`cargo test -p texpresso-infra -- --ignored` **6 个真实 latexmk/synctex 集成用例全通过**（成功 / 内容错误 / 超时树杀 / 取消 / 中文路径正向+反向 / 中文路径内容错误）——这些用例此前困在 src-tauri 无法本机运行（见 troubleshooting.md）；`npm run build`（vue-tsc + vite）通过；真机 `npm run tauri dev`（`VITE_TEXPRESSO_PROJECT=test_file/projects/multifile`）实测 dev stdout：`watch 线程启动` → `打开项目` → `触发编译` → `构造编译请求 root=…main.tex engine=XeLaTeX`，且 `tmp/main.pdf` 与项目根 `main.pdf` 时间戳推进（latexmk 真实重跑 + PDF 原子拷贝），日志无 panic/ERROR。**顺带修正两处既有测试缺陷**：① `load_global_out_of_range_falls_back_to_default` 用子串 `"timeout_secs": 1` 断言，会被默认值 120 误命中（改读回解析断言）；② `compile_chinese_path_content_error_keeps_file_and_line` 编译的是不存在的 `main.tex`（改编译 `中文主文件.tex`）。
- **根文件候选可见可交互（2026-09，roadmap P0-②-1）**：修掉「打开项目没反应」并补齐一处阻断缺陷。**根因**：`RootResolution::Multiple(_)` 的候选列表在命令层被直接丢弃（`Multiple(_) | None => None`），`ProjectInfo` 只有 `{root, root_file}`，前端拿到 `root_file: null` 后仅有一句 `console.warn`。**改动**：① core `RootResolution` 增 `candidates()` / `unique()`（把「解析结果 → 契约字段」的映射收敛到一处，含单测），`ProjectInfo` 增 `root_candidates: Vec<PathBuf>`（语义：未用手动覆盖时 `Unique`→单元素、`Multiple`→全部、`None`→空；覆盖生效→空）；② 命令面抽 `detect_root()` 供 `open_project` 与 `update_settings`（清除覆盖时回到自动探测）复用，新增只读命令 `get_project`；③ **修阻断缺陷**：`update_settings` 的 root_file 分支此前只写覆盖与有效设置，**不同步内存 `ProjectState.root_file`**（`project.write()` 全仓仅出现在 `open_project`），症状是「在设置/选择器里选了根文件 → 编译仍报『未确定根文件』，必须重开项目」；现在 `Some(rel)` 按 D8 解析为绝对路径（**失败即拒绝、不落盘**，避免存下必然失败的覆盖）后写入内存，`null` 走 `detect_root` 重探测；④ **前端**：新增 `RootFilePicker.vue`（多候选列候选、零候选退回列全部 `.tex`，展示相对路径、emit **项目内相对路径**）、`project.relativizePath()`（绝对→相对，供 `update_settings` 唯一可接受的形态）、`project.syncProject()`（选后经 `get_project` 重新同步）、`StatusBar` 增「未确定根文件 · 点击选择」入口（弹窗关掉后仍可重开）；`App.vue` 用弹窗取代 `console.warn`（含 `VITE_TEXPRESSO_PROJECT` 自动打开路径）。**验证**：core 130 通过（新增 `RootResolution` 3 例）；vitest **52 通过**（新增 `relativizePath` 8 例 + `RootFilePicker` 9 例，含「中文路径」「项目外过滤」「busy 禁用」「相对路径载荷」）；`vue-tsc --noEmit` + `npm run build` 通过。**真机端到端**（夹具 `test_file/projects/多候选工程/`：`main.tex` + `附录.tex` 均含 `\documentclass`）：dev stdout 依次为 `打开项目：…（根文件 None，候选 2 个）` → 用户点选后 `root_file 已同步（内存）：Some("…\main.tex")` → `设置自写盘，跳过重载`（自写盘过滤生效）→ 后续编辑 `构造编译请求: root=…\main.tex` → 项目根产出 `main.pdf`。**2026-09 补做（tauri server MCP 驱动真实窗口）**：`driver_session` 连接 → `webview_screenshot` 确认弹窗渲染（标题「选择根文件」+ 文案「检测到 2 个可能的根文件（都含 `\documentclass`）…」+ 两项候选，状态栏显示「未确定根文件 · 点击选择」）→ `webview_dom_snapshot` 核对无障碍树（`dialog 选择根文件` / `list` 两项 / title 为绝对路径 / `contentinfo` 含取消按钮）→ `webview_find_element` 取几何（2 个 `.file-row`）→ `webview_interact` 点 `main.tex` → 弹窗关闭、`main.tex` 打开、`.needs-root` 匹配数归 0（状态栏提示消失）→ 点「编译」→ `webview_wait_for .page-wrap canvas` → 截图确认 PDF 渲染（1/1 页，`\input` 的子文件内容也在）。**未修（已记录）**：外部直接编辑 `.texpresso/settings.json` 时 watch 只更新覆盖与有效设置、**不同步** `ProjectState.root_file`（同一缺陷的另一触发路径；需 infra `WatchState` 持有 `FileSystem` 才能复用 `detect_root`，留待后续）。
- **错误诊断升级（2026-09，roadmap ④）**：把 `.log` 原始报错翻译成「原因 + 怎么改」，取代"只有原文 + 行号"的及格线。**为什么值得做**：调研显示"错误信息不可读"是全行业最高频痛点（TeXstudio 只显示 `Process exited with error(s)`）；而 ⑲ 调研又证明**引擎自动推断没必要**——正确替代是「不猜，但选错时明确告诉用户怎么改」。**实现**：新增 `crates/texpresso-core/src/log_parser/diagnosis.rs`——纯函数 `diagnose(&LogMessage) -> Option<Diagnosis>`，19 类 `DiagnosisKind`（缺包/缺类/缺文件/缺字体/ctex 字体集/引擎不匹配/需不支持的引擎/未定义命令/组未闭合/数学模式/多右括号/缺 begin document/重复上下标/表格外 &/Emergency stop/选项冲突/非 UTF-8 源/宏包兜底/LaTeX 兜底）；**匹配不到就返回 `None`**（前端降级为原文，宁可不说也不瞎说）。两个关键实现点：① **续行前缀必须先剥**——fontspec 报错的续行带 `(fontspec)` 前缀，正好卡在 `cannot be` 与 `found` 之间，不剥则永远匹配不到（⑲ 样本暴露）；② `parse_log` 的位置标记行（`l.N \cmd`）**此前被丢弃**，导致取不到未定义命令名——改为在补行号的同时把该行原文并入消息（`l.5 \usepackage{nope}` → 诊断能说出 `\usepackage`，前端仍只渲染首行，观感不变）。**契约**：`ErrorEntry` 增 `diagnosis: Option<Diagnosis{kind,cause,hint}>`（specta 导出到 bindings）。**前端** `ErrorList.vue`：有条目诊断时渲染两行（首行「原因」、次行「→ 建议」），无诊断退回原文首行，原始 `.log` 消息降到 `title`；头部增「已诊断 N」徽标；去重键由"消息首行"改为"诊断原因优先"。**语料与测试**：`scripts/gen-log-error-corpus.ps1` 生成 `real_error_corpus.rs`（**23 例真实日志片段**：14 例收割自 ⑲ 模板编译矩阵的失败样本 + 9 例来自故意写错的最小文档，含 GBK 非 UTF-8 源）；期望值**手写**在 `diagnosis_tests.rs`（避免实现自证），断言类别/原因关键词/行号/是否有建议 + DoD 覆盖率 ≥80% + 干净日志不误报。**验证**：`cargo test -p texpresso-core` **134 通过**、`texpresso-infra` 17 通过、vitest 52、`npm run build` 通过；**真机（tauri server MCP）**：夹具 `test_file/projects/诊断测试工程/`（`\undefinedcommandhere` + `$ x_a_b $`）→ 错误列表出现「命令 \undefinedcommandhere 未定义」+「多为拼写错误；若拼写无误，通常是缺少提供该命令的宏包」与「同一个位置重复使用了上下标」+「用花括号分组，例如 x_{a_b}」，头部「已诊断 2」；`webview_interact` 点第二条 → 状态栏 `.cursor-pos` 由 `Ln 1` 变 **`Ln 4, Col 1`**（点击跳转生效）；换成 `\usepackage{nosuchpackagexyz}` 复验 → 「缺少宏包文件 nosuchpackagexyz.sty」+「…否则执行 `tlmgr install nosuchpackagexyz` 安装」，紧随的 `Emergency stop` 条目给出「这通常是「前面某条错误」的连锁反应 / 先修列表里第一条错误」。**顺带修掉一个验证中发现的缺陷**：`EmergencyStop` 的 cause 里写了 markdown `**`，UI 不渲染 markdown，会露出字面星号（已改中文引号）。
