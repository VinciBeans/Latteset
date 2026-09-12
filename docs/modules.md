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
    pub engine: Engine,            // 请求构造时从设置快照拷贝（之后设置变化不影响运行中任务）
    pub timeout: Duration,         // 同上
    pub kind: CompileKind,         // 强度（roadmap ㉘）：编辑触发 = Quick；首编/手动/空闲收敛 = Full
}

pub enum CompileOutcome {
    Success { pdf_path: PathBuf, kind: CompileKind },  // kind = **实际执行**的强度（Quick 可能被升级为 Full）
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

**命令构造算法**（engine + 强度 → 参数映射，roadmap ㉘）：

```
Full（默认）：latexmk -xelatex -outdir=tmp -synctex=1 -interaction=nonstopmode <root_file 相对项目根路径>
Quick（编辑期）：xelatex -interaction=nonstopmode -synctex=1 -output-directory=tmp <root_file 相对项目根路径>
  —— 直调引擎单趟，不经 latexmk（实测省 40% 中位，见 design.md §延迟预算实测附节）
  —— 前置条件：tmp/<stem>.aux 存在（有上一趟产物）；否则 runner 自动升级为 Full
     （单趟在无 .aux/.toc 时会让引用全成 `??`），且 `Success{kind}` 报 Full
XeLaTeX → -xelatex / xelatex；PdfLaTeX → -pdf / pdflatex；LuaLaTeX → -lualatex / lualatex
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
├── root_detect.rs —— 根文件探测（纯逻辑）
└── outline.rs     —— 文档大纲解析 + load 编排（§3.5）
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
    async fn exists(&self, path: &Path) -> io::Result<bool>;              // 产物探测（㉘：Quick 前置条件）
}
```

**设计决策 D4**：探测、日志解析、设置存储、命令面的文件读写全部经此 trait，core 因此无任何文件依赖；实现集中在 texpresso-infra（tokio::fs，ADR-0010）。

### 3.2.1 paths.rs — 路径策略（D8）

core 提供 `resolve_project_root` / `resolve_in_project`（已存在目标）/ `resolve_creatable_in_project`（新建目标：解析父目录后拼接）与 `PathError{NotFound, RootUnavailable, Outside, NotADirectory}`；命令面只把失败原因翻译成 `{ code, message }`，不再自己 canonicalize + 前缀比对。

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

### 3.5 outline.rs — 文档大纲（源结构树）

```rust
pub struct OutlineItem { /* title / level / file / line / file_base */ }
pub struct OutlineContext<'a> { /* root、root_file、打开标签的实时缓冲、文件树兜底列表 */ }
pub fn strip_tex_comment(line: &str) -> String;                                    // 行级剥离（语义见 §9.5）
pub fn resolve_include(raw: &str, from_file: &str, project_root: &str) -> Vec<String>;
pub fn build_tree(flat: &[OutlineItem]) -> Vec<OutlineNode>;
pub fn load(ctx: &OutlineContext<'_>, fs: &dyn FileSystem) -> Vec<OutlineNode>;
```

**语义**（GUI `get_outline` 与后续 CLI/MCP 共用同一实现，前端不留副本）：

- 跟随根文件的 `\include`/`\input` 图逐文件解析 `\part/\chapter/\section/\subsection/\subsubsection/\paragraph/\subparagraph`（含 `\section*` 与 `[short]`），按**文档顺序**建嵌套树；
- **缓冲优先**：打开标签的实时缓冲命中就用缓冲（未落盘也反映），否则读盘；读失败跳过；`visited` 防环；
- 无根文件时用前端文件树提供的兜底 .tex 列表，后端不自行扫描；
- `\input` 相对**项目根**解析优先、回退当前文件目录；`\begin{verbatim}` 环境内的行跳过；
- **读集安全是 D8 的词法版**（core 无 canonicalize）：与 `read_file` 命令的差异仅剩「符号链接目标解析」与 8.3 短名（Windows 大小写不敏感 FS 下行为一致）。

**刷新与交互**：`events.ts` 在项目打开、编译成功、`files-changed(structural)` 时调 `outline.refresh()`；点击项 → `editor.openFile(file, line)` 揭示源码 + `useSyncTex.forward(file, line, 0)` 高亮 PDF 对应页（SyncTeX 不可用则只跳源码）。`OutlinePane` 用 `file:line` 作 key（稳定标识）。

**已知成本**：每次编译成功都全量重扫 include 图（结构可能随编译变化，必要）。并发重扫由前端 `loadSeq` supersede 守卫收敛、不互相覆盖；若大项目出现 IO 压力，优化方向是「只刷新结构命令所在文件 / 按 mtime+hash 复用上次解析 / 低频节流」，缓存点应在 `load()`（见 §12）。

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

**编码容错**：`.log` 可能含非法 UTF-8——GBK 源文件经 **pdflatex** 会把原始字节回显进日志（xelatex 自行替换）。`decode_log(bytes)` 严格 UTF-8 优先、失败则 lossy；`FileSystem::read_to_string_lossy`（默认实现退化为严格读取，`TokioFs` 覆盖为 lossy）是 runner 读 `.log` 的路径。严格读取会让「编译失败」退化为「拿不到任何错误信息」。**未覆盖**：`read_file` 仍是严格 UTF-8，编辑 GBK 源文件不在 v1 范围。

### 4.1 diagnosis.rs — 把原始报错翻译成「原因 + 怎么改」

```rust
pub enum DiagnosisKind {   // 22 类
    MissingPackage, MissingClass, MissingFile, MissingFont, MissingFontset,
    EngineMismatch, UnsupportedEngine, UndefinedControlSequence, UnclosedGroup,
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
- 语料是 `real_error_corpus.rs` 的 **23 例真实 `.log` 片段**（模板编译矩阵的失败样本 + 故意写错的最小文档），期望值**手写**在 `diagnosis_tests.rs`，避免实现自证；断言含 DoD 覆盖率 ≥80% 与「干净日志不误报」；
- `ErrorEntry.diagnosis: Option<Diagnosis>` 进 DTO（specta 导出）；`ErrorList.vue` 有条目诊断时渲染两行（首行原因、次行 `→` 建议），无诊断退回原文首行、原始 `.log` 消息降为 `title`；头部「已诊断 N」；去重键按**诊断原因优先**；
- 两个实现陷阱：① 宏包报错的**续行前缀必须先剥**（`(fontspec)` 之类正好卡在两段关键词之间，不剥则永不匹配）；② `parse_log` 的 `l.<n> \cmd` 位置标记行**必须并入消息**，否则说不出未定义命令名（前端仍只渲染首行，观感不变）。

### 4.2 超时：证据采集与一键提超时

- 超时**进错误列表**：`CompileOutcome::Timeout { entry }`，条目由 runner 现场采集证据构造（是否首编 / `.tex` 源文件数 / 日志已排版到第几页）；
- 两类判据：有页输出 → `compile_timeout`（在推进，只是比上限慢）；无任何页输出 → `compile_timeout_stalled`（可能仍在做前置处理，也可能真卡住——**不武断说"卡住"**）；
- **强判据优先**：日志里已有致命错误 → 直接报那条错误且**不给**一键按钮（提高超时救不了）；
- `suggested_timeout_secs` 阶梯 `300 → 900 → 1800`，**首编直接跳 900**（首编最慢，让用户在注定不够的 300s 上再等一次代价过高）；`ErrorList.vue` 据此渲染「提高到 Ns 并重试」，点击后 `update_settings` → `compile_now`（顺序固定：先改设置再重跑）。

## 5. SyncTeX（synctex 大模块）

```
synctex（core）              synctex（texpresso-infra）
├── model.rs                 └── synctex.rs —— SyncTexProvider 实现（含竞争重试）
├── provider.rs
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

// texpresso-infra synctex.rs：spawn synctex 二进制（tokio::process），解析 stdout
pub fn parse_forward_output(text: &str) -> Result<SyncTexPosition>;  // 纯函数，可单测
pub fn parse_inverse_output(text: &str) -> Result<SourcePosition>;   // 纯函数，可单测
// 失败按 100/200/300ms 退避重试（`with_retry`，退避时长可注入 → 单测传全零）
```

**算法**：CLI 输出契约（Windows 实测，完整输出形态见 ADR-0008）：`view` 可能返回多个 `Output:` 块 → `parse_forward_output` 取**首个完整块**；`edit` 输出 `Input:`（正斜杠 + `./` 路径）与 `Column:-1` → `parse_inverse_output` 处理，`Input` 路径必须经 `project.resolvePath`（内部 `normalizePath` 折叠 `.`/`..`/连续斜杠）归一化——否则反向命中会重复打开同一个标签。两者均以真实 Windows 输出固化单测（`provider.rs`）。pdf 路径指向 `tmp/<root>.synctex.gz` 对应 PDF 的**项目根副本**（synctex 按文件名关联），必须传 `-d <tmp>` 参数。

**取首个完整块是确定的选择**：beamer 下曾疑似命中容器框（`H` 达 30–255pt 的导航条/整页框），把规则换成「最小 H」「首个 H≤40 的块」后**往返结果完全一致**（三组样本逐一相同）→ beamer 的 2–4 行偏移属 beamer 记录粒度，与取块策略无关（精度基线见 [design.md](./design.md) §预览）。

**命令层策略（roadmap ㉒，`commands::synctex_inverse`）**：反向命中非源码时按 y 偏移 `[0, -40, +40, -80, +80]` 就近探测，取**第一个项目内源码**（首个命中即偏移最小者）→ 返回 `InverseResultDto { source: Option<…>, note: Option<String> }`；全部落空则 `source: None` + 一句人话提示（生成文件/项目外文件/同步数据缺失）。探测只在未拿到源码时发生，正常点击延迟不变。

**信息局部性**：provider 无状态；一次调用 = 一次 spawn + 一次解析（失败时若干次重试）。前端高亮 overlay 与滚动恢复是预览模块自己的事，不流入此模块。

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
/// 范围校验：timeout 5..=1800（㉕ 上限由 600 放宽）、debounce 100..=2000、engine ∈ {xelatex,pdflatex,lualatex}
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

**覆盖清洗（`sanitize_overrides`，core `settings/validate.rs`）**：读回项目覆盖时**逐字段**校验——越界/非法的 `root_file` 置 `None`（回退全局探测），合法的 compile 覆盖保留。整包丢弃会连带丢掉同一文件里合法的 compile 覆盖，不可取。

**信息局部性**：core 的合并/校验是纯函数；全局设置快照是 §1 清单里唯一的 `RwLock` 共享态——写者只有 storage 模块，读方（组合层构造 CompileRequest 时）只取一次性快照拷贝，不持有引用。

## 7. 监视与触发组合（texpresso-infra）

```
watch.rs          —— notify 8.2 接线：事件规范化 + 过滤
compose.rs        —— 组合层：文件事件 → 编译请求（D3 的关键翻译，位于 core，由本层调用）
```

```rust
// watch.rs：notify 事件流 → 规范化 → 分类（忽略规则与 project::is_ignored 共用）
//   .tex（排除 tmp/）        → compose::compile_request_for_change(path) → scheduler
//   settings.json（全局/项目）→ 热更新：is_self_write 过滤 → 重载 → 合并项目覆盖 → 广播 settings-changed
//   其余                      → 丢弃
// 每个被接受的事件同时旁路广播 files-changed{paths, structural}
//   structural=true 仅限增/删/重命名（前端据此重建文件树）；内容修改为 false（跳过）
// 结果经 WatchSink 回调送达 src-tauri，本 crate 不认识 Tauri（ADR-0010）

// compose.rs（core，纯函数）：快照进、请求出
pub struct ComposeContext<'a> { pub project: &'a ProjectState, pub settings: &'a Settings }

pub fn compile_request_for_change(ctx: ComposeContext<'_>, changed: &Path) -> Option<CompileRequest>;
//   触发条件全部收敛在此：已确定根文件 + changed 在项目根内 + 未被 is_ignored 排除；否则 None（不编译）
//   强度 = Quick（编辑期快速出图；引用/目录可能落后一趟，由首编与空闲收敛兜底）
pub fn compile_request_manual(ctx: ComposeContext<'_>) -> Option<CompileRequest>;
//   手动「编译」与前端空闲收敛共用；强度 = Full（多趟 + bibtex/biber/索引）
//   两者都只从 ctx 取**一次性快照拷贝**（engine / timeout / root_file）：请求发出后与触发源解耦
```

**设计决策 D3（翻译层）**：scheduler 只认识 `CompileRequest`，不认识文件、项目、设置。文件事件 → 请求的翻译在组合层完成。否决"watch 直连 scheduler 传路径"：scheduler 被迫依赖项目状态与设置，违背最小外部依赖。手动编译 `compile_now` 走同一组合函数——**所有触发源收敛到同一个入口**。

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
| compile_now | compose.compile_request_manual：只看 root_file（忽略活动文件路径），构造请求入队 |
| abort_compile | scheduler.send(Abort) |
| synctex_forward / inverse | 调 provider（失败按 100/200/300ms 退避重试）；`inverse` 输出 `InverseResultDto{source,note}`：命中生成产物/项目外文件时就近回落，落空则只给提示（㉒） |
| get_settings / update_settings | 读快照 / apply_patch → 校验 → 写盘（记录 hash，供 watch 自写盘过滤）→ 广播 settings-changed。**root_file 分支**：`Some(rel)` 先按 D8 解析为项目内绝对路径（失败即拒绝、**不落盘**）；`null` → 回到自动探测（复用 `detect_root`）；随后**同步内存 `ProjectState.root_file`**——漏掉这一步的症状是「选了根文件仍报未确定根文件，必须重开项目」 |

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
| projectStore | project、rootFile、fileTree | openProject、refreshTree、refreshTreeDebounced、resolvePath、relativizePath（绝对 → 项目内相对路径，`update_settings` 唯一可接受的形态）、syncProject（经 `get_project` 重新同步） |
| editorStore | openTabs[]、activePath、dirtyPaths:Set、lastSaved:Map<path,time>、buffers、externalConflict | openFile、closeTab、markDirty、markSaved、saveAll、onFilesChanged、acceptExternal |
| compileStore | phase、kind、draft、errors[] | setStatus、setErrors |
| previewStore | pdfPath、reloadKey、highlight、syncNote | onPdfUpdated、setHighlight、setSyncNote |
| settingsStore | settings | setSettings、updateSettings |

**前端自保存过滤算法（editorStore.onFilesChanged）**：入参 paths 中，`lastSaved` 里存在且时间近（< 2s）的路径判定为"自己刚保存"→ 忽略；其余 → 已打开且不脏 → 重载内容；已打开且脏 → 保留 + 状态栏提示；未打开 → 忽略（文件树自会刷新）。`lastSaved` 是 editorStore 模块内状态，不进任何函数参数。

**并发与生命周期不变量**（异步读盘/渲染下的守卫，去掉即复发）：

- `editorStore.openFile`：去重判断放在 `await readFile` **之后复检**——并发的树节点双击不会开出两个标签；
- `editorStore.onFilesChanged`：读盘后**复检 `dirty`**，脏则保留本地内容 + 冲突标记，不覆盖用户最新输入；
- `compileStore.setStatus("success")` 清空 `errors`/`hasError`——无 `running` 前置时旧错误不残留；
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
| PreviewPane | pdfPath、highlight、syncNote | 点击坐标 → useSyncTex | pdf.js 文档句柄、滚动位置缓存、canvas 代次与页高（见下方渲染契约） |
| FileTree | 树数据、激活路径 | 打开文件/目录展开 | 展开状态（只在前端本地） |
| RootFilePicker | root、candidates（探测候选）、fallbackFiles（零候选时全部 .tex）、busy、error | `select`（**项目内相对路径**）、`close` | 无（纯展示；相对路径由 `relativizePath` 计算） |
| ErrorList | errors[] | 点击条目 → openFile+定位 | 无；**诊断展示**（roadmap ④）：条目带 `diagnosis` 时渲染两行（原因 + 建议），无诊断降级为原文首行；头部「已诊断 N」。**去重/截断**：同源（文件 + 首行消息相同）聚合为一条并显示 `×N`；不同源最多展示 `MAX_DISPLAY=30` 组，超出提示隐藏数量（错误雪崩时不刷屏） |
| StatusBar | compileStore/editorStore/projectStore 只读投影 + 「未确定根文件」可点击入口（emit `pick-root`）+ 草稿期「引用待更新」 | `pick-root` → App 打开根文件选择器 | 无（`queued/running/failed` 不改「引用待更新」标记——屏幕上的 PDF 仍是旧的，失败不产出新 PDF） |

**布局（App.vue）**：左栏文件树与大纲**上下分布**（`SplitPane direction="horizontal"`，比例 0.55）；底部面板默认折叠成约 30px 细条（头部「报告 · 状态 · 展开」，点击展开/收起），展开后错误列表占满底部宽度。分割器自研（不引入 vue-code-layout，多面板布局后置），`.split-pane.vertical` 显式写规则，不依赖默认 flex 行为。

**PreviewPane 渲染契约**（滚动/缩放正确性的前提，改这里必须连改本清单）：

- **分页 DOM 虚拟化**：只挂载视口窗口内的页（`mountStart..mountEnd`，前后各 `PAGE_WINDOW=6`），顶部/底部占位撑住总高度；`renderNearViewport` / `updateCurrentPage` 只遍历窗口内页 → 复杂度 O(视口)，不是 O(总页数)。滚动驱动 + 窗口变化 watcher + 容器 `ResizeObserver`（不用 IntersectionObserver）。
- **canvas 代次**：`structuralEpoch` **仅在缩放 / 换文档时递增**（重建 canvas DOM）；同文件内容重载**复用 DOM**（`doRenderPage` 每次 `canvas.width=` 即重置 2D context），`pageH1` 保留 → 滚动恢复精确。
- **页高是布局的唯一驱动**：`.page-wrap` 高度绑定 `pageH1[n]×scale`（`pageWrapHeight(n)`），不依赖 canvas 尺寸。两类复发路径：① 释放/重建把 canvas 置 0×0 时页-wrap 塌缩到只剩边距 → `scrollHeight` 变短（滚动条拖不到真实末尾），且 `renderNearViewport` 用 `getBoundingClientRect`（0 高）把近页误判为远页而释放（PDF 消失）；② `pageH1`/`prefixH1` 是普通数组（非响应式），页高变化必须靠 `layoutRev` 自增让相关 computed 重算。`.page-wrap canvas { display:block }` 消除内联 canvas 在强制高度下的基线缝隙。
- **跳转先预热页高**：`renderPage` 返回渲染链 promise（`await` 真正完成 + `setHeight` 记录页高），跳转前预热目标页及之前页高并 `nextTick`，用瞬间定位（`behavior:"auto"`）——平滑滚动会在中途布局变化下跳不到位。页码输入与 SyncTeX 正向共用 `goToPage(n)`（展开窗口 + 预热 + 渲染 + 居中）。
- 插桩：`window.__previewLastReload` + 控制台 `[preview] reload#N`（真机验收清单第 6 项读它）；耗时基线见 [design.md](./design.md) §预览。

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
pdf-updated:    { path: string }
files-changed:  { paths: string[], structural: boolean }   // structural=true 仅增/删/重命名（文件树重建）；内容修改为 false（跳过）
settings-changed: Settings
```

**命令（invoke）**：见 §8 表，输入输出全部是 DTO。

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
| 1 | `LatexmkRunner` 命名偏窄 | 它同时驱动 latexmk（Full）与直调引擎（Quick）；改名牵动架构图 SVG 与三处文档，当前以注释说明 |
| 2 | 编辑 GBK/非 UTF-8 源文件 | 不支持：`read_file` 是严格 UTF-8（`.log` 侧已 lossy，见 §4） |
| 3 | bib/biber 场景的编辑期单趟 | 未实测：现有 fixture 编辑期不触发 bibtex；空闲收敛兜底应能覆盖，但没有实测结论 |
| 4 | 大纲全量重扫 | 每次编译成功都重扫 include 图；优化方向见 §3.5 |
| 5 | 延迟预算口径 | 本机三档小文档均超「小文档 2s 及格」（连 6 行的 `tiny` 冷编译也要 2.3s）→ 需复核是放宽口径还是改判「相对基线的回归容忍度」，见 [design.md](./design.md) §基准脚本 |
| 6 | 真实论文模板（hithesis） | 阻塞点未定位：模板自带 latexmkrc × `-outdir=tmp` 约定下 `\include{子目录/...}` 写不出中间文件、报错后 latexmk 挂住；**调高超时也编不过**（roadmap ㉖，最小复现见 [troubleshooting.md](./troubleshooting.md)） |
| 7 | 外部直接编辑 `.texpresso/settings.json` | 重算了有效设置，但不重算内存 `ProjectState.root_file`（roadmap ㉑：与 `update_settings` 同源缺陷的另一条触发路径） |
| 8 | 非 Windows 平台 | 未验证：进程组 kill 与平台相关路径处理均为 v1 后置 |
| 9 | `cargo test -p texpresso`（src-tauri） | 本机因 WebView2 限制无法运行，见 [troubleshooting.md](./troubleshooting.md) |

### 12.2 跨模块不变量（改回去即复发）

- **Quick 的前置条件**：无 `tmp/<stem>.aux` 时 runner 必须把 Quick 升级为 Full，且 `Success{kind}` 报**实际**强度——否则引用全成 `??`，或前端多提示一次「引用待更新」并多跑一次空收敛。
- **设置的读入口**：`open_project` 与 `update_settings` 都必须先读**纯全局**设置（`load_global`）再合并项目覆盖；`update_settings` 还必须同步内存 `ProjectState.root_file`——否则出现跨项目设置污染与「选了根文件仍报未确定根文件，必须重开项目」。
- **覆盖清洗逐字段**：`sanitize_overrides` 不能退回「整包丢弃」（会连带丢掉同一文件里合法的 compile 覆盖）。
- **自写盘过滤**：`update_settings` 写盘记 hash、watch 消费一次——去掉即「自己写 → 自己重载 → 重复广播」。
- **路径校验只有一个入口**：命令面路径一律经 core `project::paths`（D8）；core 内部（如 outline 读盘）用词法前缀版，差异仅剩符号链接目标与 8.3 短名。
- **`.log` 必须容错解码**：严格 UTF-8 读取会把「编译失败」退化为「拿不到任何错误信息」（GBK 源 + pdflatex）。

前端的三处异步守卫与 PreviewPane 渲染契约见 §9.2 / §9.4，SyncTeX 相关约束见 §5。

### 12.3 验证入口

| 要验的东西 | 入口 |
|---|---|
| 编译性能与预算回归 | `node scripts/bench.mjs`（fixture 由 `node scripts/gen-bench-projects.mjs` 生成；超预算退出码 1） |
| SyncTeX 往返精度 | `node scripts/synctex-report.mjs`（三组样本；基线见 design.md §预览） |
| core 逻辑（调度 / 解析 / 诊断 / 大纲） | `cargo test -p texpresso-core` |
| 真实 latexmk / synctex 集成 | `cargo test -p texpresso-infra -- --ignored` |
| 前端 store 与 composable | `npm run test` |
| 类型检查与构建 | `npm run build` |
| 只有真实窗口能验的部分 | [troubleshooting.md](./troubleshooting.md) §真机验收清单（tauri server MCP 驱动） |
| 产品级实测数字与结论 | [design.md](./design.md)（延迟预算、预览重载、编辑期单趟收益） |
| 已完成项及其证据 | [roadmap §1 基线](./research/tex-ide-roadmap-priority.md) |

### 12.4 与上层文档的关系

- architecture.md §2/§3/§5 的模块表在本文件展开为函数级；**本文件冻结后，architecture.md 的模块表不再单独细化**。
- 产品语义（延迟预算、失败语义、MVP 边界）以 [design.md](./design.md) 为准；本文件只写实现契约与已知债。
- 历史决策与被否决的备选见 [adr/](./adr/) 与 §11。
