# 实时渲染实现路线图（参考 TeXpresso 架构）

> 本文档是一份可直接参考的实施路线图，用于在**自己的项目**中复刻 TeXpresso 式的"边改边出图"能力。
>
> 分析对象：[TeXpresso](https://github.com/let-def/TeXpresso) —— 一个 LaTeX 实时预览器。
> 本文基于对其源码的实际通读（`src/frontend/`、`src/dvi/`、`src/engine/`）。

---

## 目录

- [0. 核心结论](#0-核心结论)
- [1. 可行性判定（开工前必做）](#1-可行性判定开工前必做)
- [2. 目标架构速览](#2-目标架构速览)
- [3. 分阶段实施路线](#3-分阶段实施路线)
- [4. 阶段验收清单](#4-阶段验收清单)
- [5. 调试方法论：把决策过程打出来](#5-调试方法论把决策过程打出来)
- [6. 附录：TeXpresso 源码索引](#6-附录TeXpresso-源码索引)

---

## 0. 核心结论

TeXpresso 的"实时"**不是靠把重编译做快**，而是靠**把一次编辑变成一次可撤销、可从中间恢复的事务**。

它有六个正交机制，难度差异极大。按依赖顺序分阶段做，每阶段都能独立验证、独立产出价值：

| # | 机制 | 难度 | 累计收益 |
|---|---|---|---|
| 1 | 端到端骨架 | ★ | 能显示 |
| 2 | 引擎子进程 + 流式输出 + 时间片 | ★★ | **体感收益的一半** |
| 3 | 增量输出解析（字节级） | ★★ | 长文档不再变慢 |
| 4 | 脏矩形显示 | ★★ | 滚动/缩放不卡 |
| 5 | VFS 三层模型 + 写前日志 | ★★★ | 可回滚 |
| 6 | seen 水位 + trace + 逻辑时钟 | ★★★★ | **打字不重算前半篇** |
| 7 | fork 快照 + fence 调度 | ★★★★★ | 从中间状态续跑 |
| 8 | 多趟收敛 | ★★★ | 交叉引用最终一致 |

**两个刻意的"重新评估"检查点**：阶段 4 之后（已有实时观感）、阶段 6 之后（已有效益核心）。阶段 7 的复杂度经常超过它带来的额外收益。

---

## 1. 可行性判定（开工前必做）

TeXpresso 的每个技巧都依赖目标程序的特定性质。逐条核对，**任何一条不成立，对应机制直接放弃**：

| 机制 | 前提条件 | 满足 |
|---|---|---|
| **fork 快照** | 目标程序**单线程**、**确定性**、无 GPU/驱动状态、无活跃外部连接、fork 后不加载新动态库 | ☐ |
| **回滚判定（seen 水位）** | 能**拦截目标程序的每一次 I/O**，且能拿到字节偏移 | ☐ |
| **增量解析输出** | 输出格式支持**字节偏移重同步**（DVI 靠 BOP/EOP 标记） | ☐ |
| **编辑批合并** | 程序能接受"部分输入状态"并继续（而非必须完整文件） | ☐ |

### 1.1 先量化收益

测量一次**完整重编译**的耗时：

- **< 200ms** → 用"防抖 + 全量重跑 + 输出 diff"就够了。后续所有机制都是**负收益的复杂度**，停手。
- **数百毫秒 ~ 数秒** → 值得做到阶段 4。
- **> 3 秒**（LaTeX 全量编译的典型值）→ 值得做到阶段 6，甚至阶段 7。

### 1.2 再验证确定性

**连跑两次，逐字节对比输出。** 这一步必须通过，否则第 7 阶段的快照机制会在某个时刻给出**静默错误**的结果——比慢更糟。

不确定性的常见来源：时间戳、PID、随机数、哈希遍历顺序、并发调度、locale。

### 1.3 跨平台提醒

`fork()` 快照**只在 Unix 有效**：

- Windows：要么跑 WSL，要么改用"显式序列化程序状态"——成本高一个数量级，通常不如放弃阶段 7。
- macOS：`fork` 后无法加载系统字体。TeXpresso 的 workaround 是把首次快照**推迟到输出开始之后**（`src/frontend/engine_tex.c:501`），寄望字体已全部加载。彻底解法需要"无 fork 的快照"，作者在注释里明确说明这是未解决的妥协。

---

## 2. 目标架构速览

```
编辑器 --stdin--> [driver: 前端] --socketpair--> [被改造的目标程序]
                       |                                |
                       |<-------- 输出字节流 ------------|
                       |
              增量输出解析（页索引 / 结构索引）
                       |
              display list --> GPU 纹理 --> 屏幕
```

三层增量**正交叠加**，任何一层单独都不足以做到"边打字边出图"：

| 层次 | 机制 | 效果 |
|---|---|---|
| **文件级** | 写前 undo log + `seen` 水位回退 | 编辑点之后的工作全部保留 |
| **进程级** | `fork()` 活进程快照 + fence 调度 | 从最近的中间状态继续，而非从头 |
| **字节级** | 增量结构扫描 + 索引 + 纹理脏矩形 | 只解析新增字节，只重绘新暴露像素 |

再叠加两个调度层优化：**编辑批合并** 与 **时间片推进**。

---

## 3. 分阶段实施路线

### 阶段 1：端到端骨架（无任何增量）

**目标**：改文件 → 重跑 → 屏幕上出现新图。

**做法**：

- 单进程，走 `system()` / `subprocess.run()` 调用编译器，输出落临时文件
- 解析输出、光栅化、显示

看似"什么都没做"，但这一步确立了**输出格式的解析能力**，是后面一切的地基。
TeXpresso 对应的是 `src/dvi/` 整个库（`dvi_interp.c` 解释器 + `dvi_context.c` 状态机）。

**验收**：能正确显示一个静态文件的所有页。

---

### 阶段 2：引擎子进程 + 流式输出

**目标**：输出一产生就显示，而不是等编译结束。**这一步单独就能拿到大约一半的体感收益。**

**做法**：

- 用 `socketpair` / 管道把子进程的 stdout 换成受控通道
- 子进程每写一段输出，主进程立刻追加到内存缓冲并解析
- 主进程每帧只处理有限的几条消息，然后回到 UI 事件循环

**参考实现**：

- `src/engine/main/fork.c:30` —— `socketpair` + `execvp`
  - 注意其中的 `dup2(STDERR_FILENO, STDOUT_FILENO)`：避免目标程序的普通输出污染协议通道
- `src/frontend/main.c:176` —— `advance_engine()`：**每帧最多 10 步或 5ms**，然后立刻返回事件循环

**最值得学的一段：时间片设计**

```
while (need_work) {
  if (!do_one_unit()) break;
  if (--steps == 0) { steps = 10; if (elapsed() > 5ms) break; }
}
return still_has_work;   // 返回值决定主循环是否阻塞在事件等待上
```

`src/frontend/main.c:1373` 用返回值决定是否跳过 `SDL_WaitEvent`：
**有活就忙但不阻塞事件处理，没活就休眠。**

**验收**：编译一个耗时 10 秒的文档，第一页应在数百毫秒内出现，且这 10 秒内 UI 不卡。

---

### 阶段 3：增量输出解析（字节级）

**目标**：新增输出只解析新增的字节。

**设计要点**（照抄 `src/frontend/incdvi.c:86` 的**两阶段结构**）：

1. 维护一个 `offset`（已扫描到的字节位置）
2. 只做**指令长度计算**，不解释指令内容
3. 遇到结构标记（DVI 的 BOP/EOP）就记录**字节偏移**进页索引数组
4. **缓冲区变短时**（回滚导致截断）：丢弃最后不完整页，`offset` 退回上一页边界

```c
if (d->offset > len) {
  while (d->page_len > 0 && d->pages[d->page_len-1] >= len) d->page_len -= 1;
  if (d->page_len == 0) d->offset = 0;
  else { d->page_len -= 1; d->offset = d->pages[d->page_len]; }
}
```

> **第 4 条是"回滚友好"的关键，一开始就要写进去，后补会很痛。**

**性能护栏**：让解析成本与**新增字节数**成正比，而不是与**总输出大小**成正比。

- `incdvi_update` 每次"追加事件"调用一次，而不是每字节调用一次
- 页索引是平面数组 `{bop_offset, eop_offset}` 交替，`page_count = page_len / 2`
- 页尺寸直接从 BOP 寄存器读出（`incdvi_page_dim`），**不需要先渲染就能知道页面大小**

**验收**：长文档编译过程中，每帧的解析耗时应该是**常数级**而非随文档增长。

---

### 阶段 4：脏矩形显示

**目标**：滚动 / 缩放不重绘整页。

**参考实现**：`src/frontend/renderer.c:546` 的 `update_texture`

标准做法：

1. 求**旧视口与新视口的交集**
2. 只把新暴露的 4 个矩形（tl / tr / bl / br）重新光栅化
3. 只上传这些矩形到 GPU 纹理

配合渲染库的裁剪参数（MuPDF 的 `fz_run_display_list` 接受 `bounds`）裁剪图元。

**验收**：小幅滚动时每帧耗时应该只有几十微秒。
（TeXpresso 内置了 `stopclock` 打点，见 `renderer.c:487`，可直接照抄。）

> **检查点 A**：如果到这里体感已经够好，可以收工。下面是数量级更难的复杂度。

---

### 阶段 5：VFS + 事务回滚

**目标**：让"编辑"成为一个可撤销的事务。

#### 5a. 三层内容模型

参考 `src/frontend/state.h:42` 的 `fileentry_t`。每个文件维护三份内容 + 优先级 `saved > edit_data > fs_data`：

| 字段 | 含义 |
|---|---|
| `edit_data` | 编辑器里**尚未存盘**的内容（overlay） |
| `fs_data` | 磁盘内容 |
| `saved.data` | **目标程序实际观察到 / 写出的**内容 ← 回滚的基准 |

优先级规则的含义：

> 一旦程序读过某文件，就以"它读到的那份"为准；否则用编辑器未存盘的内容。

对应用户项目时，`edit_data` 就是你的编辑器/IDE 缓冲区 overlay——**这是"未存盘也能实时预览"的来源**。

#### 5b. 写前日志（undo log）

参考 `src/frontend/state.c`：

- `log_fileentry` / `log_filecell` / `log_overwrite` —— 在改状态**前**把旧值压栈
- `log_snapshot()` —— 返回栈深度作为 `mark_t`
- `log_rollback(mark)` —— 逐条弹栈还原

**必须学的一个优化**（`src/frontend/state.c:97`）：

```c
if (entry->saved.snap != log->snap) { /* ...记录旧值... */ entry->saved.snap = log->snap; }
```

用"世代号"保证同一快照内一个 entry 只记一次。

> 这让快照成本与**修改量**成正比，而非与**项目规模**成正比。**没有这一条，大项目会直接卡死。**

同理，`log_overwrite` 只存被覆盖的那段字节，不存整份文件。

#### 5c. 事务外壳

主循环每轮固定三段（`src/frontend/main.c:1299`、`1338`）：

```
begin_changes()   → 打 mark
  ...应用编辑器命令 / 扫描磁盘...
end_changes()     → 若回滚了 → 重跑
```

**验收**：故意让程序在写到一半时失败，确认 `log_rollback` 后状态与失败前**完全一致**。
（TeXpresso 用 `abort()` 断言这点，见 `state.c:197`。）

---

### 阶段 6：seen 水位 + trace（回滚判定）

**这是"实时"真正成立的地方**，也是最值得理解的一段逻辑。

#### 6.1 数据结构

| 结构 | 含义 |
|---|---|
| 全局 `trace[]` | 按时间顺序记录「文件 F 被读到位置 X，发生在时刻 T」 |
| 每文件 `seen` | 当前读水位 |
| 每进程 `trace_len` | 该进程"白得"的历史长度 |

`trace` 就是一份**目标程序执行过程中"读了什么"的完整历史**，等价于它的输入依赖序列。

#### 6.2 核心判定

`src/frontend/engine_tex.c:1369` —— `rollback_add_change()`，整个机制的心脏：

```c
if (e->seen < changed) return;       // 编辑点在读水位之后 → 全部工作有效，不回滚！
while (e->seen >= changed) {          // 否则回退到"读该文件之前"
  trace_len--;
  revert_trace(&self->trace[trace_len]);
}
offset = changed;                     // 新进程只从 changed 处重读
```

语义非常清晰：

> **如果改动落在程序尚未读到的位置之后，此前所有工作全部有效，直接继续往下跑。**

这正是"改文档末尾不影响前面已排版内容"的实现方式。

#### 6.3 必须同时实现的三个健壮性机制

**① 逻辑时钟**

TeXpresso 用 `xetex_tokens`（每读一个 token 递增，`xetex-xetex0.c:5171`）当时间戳。

- 关键：时间戳要**单调**，且在**查询发起时采样**（`texpresso_protocol.c:136`）
- **不要用真实 `clock_gettime`**：快照恢复后会产生乱序

**② 竞态处理** —— `engine_tex.c:1318` `process_pending_messages()`

主进程以为程序还没读到，但程序其实已读过、只是"已读"通知还在管道里。

必须先**排空管道里的待处理通知**，再决定是否回滚。

> **这个 bug 一定会遇到。** TeXpresso 的 CHANGELOG v0.1 里
> "Fix a crash due to a broken invariant when the contents being edited has been read by the TeX worker but the driver is not aware"
> 就是它。

**③ 卡死看门狗**

查询 **10ms 无应答**就判定 worker 卡死 → 杀掉 → 回退到上一个快照（`engine_tex.c:1336`）。

这让"目标程序死循环"不会冻住整个应用。

#### 6.4 编辑批合并

连续击键只触发**一次**端到端重算。参考 `src/frontend/main.c:776` `interpret_change()`：

只在很窄的条件下把编辑缓存起来延迟应用。TeXpresso 的条件是：

- 引擎仍在运行
- 页数 `<=` 当前显示页
- 缓冲区未溢出（64 条 / 4096 字节）

条件刻意收窄，是为了**不缓存"已经渲染过、当前正在显示"的那部分内容**——那类编辑必须立刻可见。

**验收**：

- 在文档末尾敲字 → 日志里应看到 `e->seen < changed` 直接返回（**零重算**）
- 在文档开头敲字 → 应看到 trace 回退

> **检查点 B**：到这里已经有了"打字不重算前半篇"的核心收益。阶段 7 的复杂度经常超过其额外收益。

---

### 阶段 7：fork 快照（最难，收益也最大）

**目标**：从最近的中间状态续跑，而不是从头重跑到编辑点。

**参考实现**：`src/engine/main/fork.c:58` —— `texpresso_fork_with_channel()`

#### 7.1 机制

1. 建 `socketpair`
2. `fork()` —— 子进程**完整继承全部内存状态**（已读入的宏、字体、页面构建器、断行状态……）
3. 子进程把新 socket `dup2` 到通道 fd，继续执行
4. 父进程通过 `SCM_RIGHTS` 把 fd 传给主进程，然后 `waitpid` 挂起
5. 主进程登记新进程：`p2->trace_len = p->trace_len`

#### 7.2 为什么这个等价性成立

> 子进程的程序状态 = 父进程执行到 `trace_len` 时的状态；
> 而主进程为它记录的"已读历史"也正好是 `trace_len`。
> 两者天然对齐，**不需要重放**。

这是整套设计最漂亮的地方。

#### 7.3 触发时机（`engine_tex.c:474` `need_snapshot`）

距上一检查点超过 **500ms 逻辑时间**就插一个。

#### 7.4 fence 调度（`engine_tex.c:1066` `compute_fences`）

- 从回滚点开始往回走，按**指数增长的间隔**（50、100、200… ms 逻辑时间）最多选 **16** 个候选检查点
- 当程序读到 fence 位置时，把这次读**截断**在 fence 处并返回 fork 请求，**强制在该精确位置产生快照**
- 效果：**越靠近当前编辑点，快照越密**——主动重塑了检查点分布

#### 7.5 进程数治理

上限 **32**，超了用 `decimate_processes()`（`engine_tex.c:270`）保留时间上分散的、杀掉冗余的。

避免进程数与内存的无界增长。

#### 7.6 必须提前排查的坑

- **线程**：程序若开了线程，`fork` 后只有调用线程存活，其他线程持有的锁会**永久死锁** → 快照必须单线程
- **`dlopen`**：`fork` 后不能再加载动态库（macOS 甚至不能加载系统字体）
- **孤儿进程**：子进程退出前父进程一直 `waitpid` 挂着 → 需要处理资源回收
- **fd 泄漏**：`fork.c:108-111` 的两个 `close` 缺一不可
- **父进程阻塞语义**：父进程在 `waitpid` 期间完全挂起，不能承担新工作——它只是一个"更早的快照"

---

### 阶段 8（可选）：多趟收敛

**仅当**目标程序有"需要多趟才能稳定的副作用"（LaTeX 的 `.aux` / `.toc`）时才需要。

**参考实现**：`src/frontend/engine_tex.c:1470` `engine_finish_convergence()`

1. 检测到有非系统输出文件被写过 → 标记 `aux_dirty`
2. **空闲 500ms 后**触发一趟"跑到底"
3. 比对：写出的 aux 与输入的 aux **逐字节相同** → 已收敛，只重建工作进程
4. 不同 → 把新 aux 当作输入，清空所有状态重跑
5. 最多 **5** 趟

**关键：空闲触发，而不是每次编辑都触发。** 编辑期间显示上一趟的结果，停手后静默收敛。

---

## 4. 阶段验收清单

```
阶段 0  可行性判定 + 量化收益            ← 决定要不要做
阶段 1  端到端骨架
阶段 2  子进程 + 流式输出 + 时间片        ← 性价比最高，先拿到这里
阶段 3  增量输出解析
阶段 4  脏矩形显示
        ==== 检查点 A：够用就停 ====
阶段 5  VFS 三层模型 + 写前日志
阶段 6  seen 水位 + trace + 逻辑时钟 + 竞态处理
        ==== 检查点 B：够用就停 ====
阶段 7  fork 快照 + fence 调度
阶段 8  多趟收敛
```

| 阶段 | 验收标准 |
|---|---|
| 1 | 静态文件所有页正确显示 |
| 2 | 10 秒文档，首页 < 数百 ms 出现，UI 全程不卡 |
| 3 | 每帧解析耗时为常数级，不随文档增长 |
| 4 | 小幅滚动每帧耗时 < 100μs |
| 5 | 写入中途失败后，`rollback` 能还原到完全一致的状态 |
| 6 | 末尾编辑零重算；开头编辑正确回退 trace |
| 7 | 检查点续跑而非从头跑；日志可见 snapshot 栈 |
| 8 | aux/toc 收敛且不阻塞编辑 |

---

## 5. 调试方法论：把决策过程打出来

调试这套机制时，**唯一有效的办法是把决策过程可视化**。TeXpresso 到处都留着这类日志，直接照抄：

```
rolling back to position %d / before rollback: %d bytes of output
[fence] placing fence %d at trace position %d, file %s, offset %d
[change] rewinded trace from %d to %d entries
Snapshots: - position %d, time %dms
```

**特别注意** `engine_tex.c:1004` 的 `Last trace entries:` 那段——打印最近 10 条 trace。

> 没有这些日志，你会在"为什么这次编辑重算了整篇"这类问题上耗掉大量时间。

**其他调试辅助**：

- 内置 stopclock 打点（`renderer.c:487`），随时能看到 render / upload 各花多少微秒
- 断言用 `abort()` + backtrace（`src/frontend/myabort.c`），让不变量违反立刻可见——这套回滚逻辑**很脆弱**，静默错渲染比崩溃危险得多
- 保留 `-stream` 这类"完全跳过磁盘、所有文件由宿主推送"的模式，便于构造确定性测试

---

## 6. 附录：TeXpresso 源码索引

### 6.1 关键文件

| 文件 | 职责 |
|---|---|
| `src/frontend/main.c` | 主事件循环、命令解释、时间片推进（`advance_engine`） |
| `src/frontend/engine_tex.c` | **核心**：trace、回滚判定、fork 调度、fence、收敛 |
| `src/frontend/state.c` | 写前日志（undo log）、快照/回滚原语 |
| `src/frontend/state.h` | `fileentry_t` 三层内容模型 |
| `src/frontend/fs.c` | 内存 VFS（哈希表） |
| `src/frontend/incdvi.c` | 增量 DVI 解析 + 页索引 |
| `src/frontend/renderer.c` | 脏矩形纹理更新、主题/反色、文本选择 |
| `src/frontend/sprotocol.c` | 查询-应答通道（驱动侧） |
| `src/dvi/dvi_interp.c` | DVI 指令解释器 |
| `src/dvi/dvi_context.c` | 跨页持久的上下文（字体表、资源管理器） |
| `src/engine/main/fork.c` | `fork` + `SCM_RIGHTS` fd 传递 |
| `src/engine/main/texpresso_protocol.c` | 被改造的目标程序侧的 I/O 协议 |
| `src/engine/main/main.c` | 目标程序入口、诊断输出 |

### 6.2 关键函数速查

| 函数 | 位置 | 作用 |
|---|---|---|
| `advance_engine` | `main.c:176` | 5ms 时间片推进 |
| `interpret_change` | `main.c:776` | 编辑批合并 |
| `record_seen` | `engine_tex.c:402` | 写 trace 条目 |
| `need_snapshot` | `engine_tex.c:474` | 何时插检查点 |
| `compute_fences` | `engine_tex.c:1066` | 指数稀疏选检查点 |
| `process_pending_messages` | `engine_tex.c:1318` | 竞态处理 + 卡死看门狗 |
| `rollback_add_change` | `engine_tex.c:1369` | **核心回滚判定** |
| `rollback_processes` | `engine_tex.c:986` | 杀进程 + 还原 trace + 重建输出 |
| `engine_finish_convergence` | `engine_tex.c:1470` | 多趟收敛 |
| `log_fileentry` | `state.c:97` | 世代号去重的写前日志 |
| `incdvi_update` | `incdvi.c:86` | 增量扫描新字节 |
| `incdvi_render_page` | `incdvi.c:193` | 按需重放字节区间 |
| `txp_read` | `texpresso_protocol.c:257` | 收到 `T_FORK` 后 fork 并重试 |
| `texpresso_fork_with_channel` | `fork.c:58` | fork + fd 传递 |

### 6.3 协议速查

**查询（目标程序 → 驱动）**：

```
Q_OPRD/Q_OPWR  Q_READ  Q_APND  Q_CLOS  Q_SIZE  Q_MTIM  Q_SEEN  Q_GPIC  Q_SPIC  Q_CHLD
```

**应答（驱动 → 目标程序）**：

```
A_OPEN  A_READ  A_PASS  A_DONE  A_SIZE  A_MTIM  A_GPIC  A_FORK
```

`A_FORK` 是快照机制的关键——驱动不回数据，而是要求"在此处 fork 一个快照，然后在子进程里重试这次读"。

### 6.4 延伸阅读

- 项目内的 `src/frontend/README.md`：模块划分说明
- `EDITOR-PROTOCOL.md`：编辑器 ↔ TeXpresso 的完整协议（VFS、`register`/`pause`/`resume`、`rerun` 收敛控制）
- `CHANGELOG.md`：多数条目的修复说明都能反推出这套机制的脆弱点
- 上游仓库：<https://github.com/let-def/TeXpresso>

---

## 7. 最后的提醒

这套架构的全部价值，建立在三个前提之上：

> **目标程序是确定性的、可拦截 I/O 的、读多写少的纯计算过程。**

TeX 完美符合。

如果你的目标程序有**网络请求、时间戳、随机数、并发**，那么阶段 6、7 不仅难做，而且会产出**静默错误的结果**——比慢更糟。

**这种情况下正确选择是做阶段 1–4，加上充分的缓存与防抖，然后停手。**
