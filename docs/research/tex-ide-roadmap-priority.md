# TeXPresso Roadmap（优先级、批次与出口条件）

> 作用：把调研结论变成可执行的优先级与批次，并记录已完成项的 ID 与证据。
> 依据：[tex-ide-pain-points.md](./tex-ide-pain-points.md)（调研总览）+ [desktop](./desktop-latex-editor-pain-points.md) / [online](./online-latex-editor-pain-points.md) / [vscode](./vscode-latex-workshop-pain-points.md) 三份专项 + [P0-② 分析](./p0-2-first-open-analysis.md) + [模板语料调研](./template-corpus-survey.md) / [中文高校模板引擎要求](./cn-thesis-template-engines.md) + [texpresso-live-rendering-roadmap.md](../texpresso-live-rendering-roadmap.md)（外部方案评估，见 §5）
> 现状实现：[design.md](../design.md)（产品语义与实测数字）、[modules.md](../modules.md)（实现契约、已知债与验证入口）、[cli-mcp-plan.md](../cli-mcp-plan.md)、[troubleshooting.md](../troubleshooting.md)（真机验收清单）
>
> **ID 稳定约定**：①…㉗ 沿用历史编号（`modules.md` / `troubleshooting.md` / `design.md` 里的「roadmap P0-①」「P0-②-1」「roadmap ④」等引用因此不失效）；新增项从 **㉘** 起续编。已完成项**保留 ID 并移入 §1 基线**，不再占用待办位次。

## 1. 已完成基线（保留 ID 与证据，不再出现在待办中）

| ID | 项 | 完成 | 证据 |
|---|---|---|---|
| ① | 中文文件名/路径与编码实测复核 | 2026-09 | 路径层面全链路可用；修掉真实缺陷（GBK 源 + pdflatex 的 `.log` 非法 UTF-8 → 「编译失败」退化为「拿不到任何错误信息」）。MCP 补验：中文路径 PDF 渲染正常、反向 SyncTeX 跳 `Ln 10`。见 [troubleshooting.md](../troubleshooting.md) |
| ②-1 | 根文件候选可见可交互 | 2026-09 | `ProjectInfo.root_candidates` + `get_project` + `RootFilePicker.vue` + 状态栏入口；**并修掉阻断缺陷**：`update_settings` 不同步内存 `ProjectState.root_file`。见 [modules.md](../modules.md) §12 |
| ④ | 错误诊断升级 | 2026-09 | 19 类 `DiagnosisKind`（缺包/缺类/缺字体/字体集/引擎不匹配/未定义命令/组未闭合/连锁 Emergency stop…），匹配不到返回 `None` 降级原文；**23 例真实语料** + 手写期望表 + DoD 覆盖率 ≥80%；真机验证两行式展示 + 点条目跳 `Ln 4` |
| ③ | 性能基准与回归基建 | 2026-09 | `scripts/{gen-bench-projects,bench}.mjs`：六档纯文本 fixture + 可选真实模板档；一条命令出 cold/noop/edit 报告，超预算退出码 1。**结论：瓶颈在小文档固定开销而非大文档**；三档小文档超 2s 及格线；真实论文档 cold >240s |
| ⑲ | 模板样本调研 | 2026-09 | **0/19** 案例因默认引擎选错；`\RequirePDFTeX` **0/7374**；文档调研 19/19 兼容 XeLaTeX → ②-2 砍掉。另暴露 4 类真实障碍（缺包/缺字体、首编超时、latexmkrc 交互、`.ins`/`.dtx`） |
| ㉕ | 学位论文首编超时 | 2026-09 | 上限 **600→1800s**；**不再静默重试**（旧行为要白等两个超时窗口）；超时**进错误列表**并带证据化诊断（首编/源文件数/日志页码 → 慢 vs 疑似卡住；日志已有致命错误则直接报那条）；`Diagnosis.suggested_timeout_secs` + ErrorList 一键「提高到 Ns 并重试」。真机 cycle 已验证。**真实模板 DoD 被 ㉖ 阻塞**（见 §6.1） |
| ㉘ | 编辑期轻量出图 + 空闲收敛 | 2026-09 | 实测省 **40.2%**（6/6 档达标）→ 落地：Quick 直调引擎 / Full 完整 latexmk；无产物自动升级 Full；`draft` 状态 + 状态栏「引用待更新」+ 2s 空闲收敛。真机 cycle 与 `fdb_latexmk` mtime 双重佐证。见 §6.3 |
| ⑤ | SyncTeX 可靠性加固（含 ㉒、⑳） | 2026-09 | `scripts/synctex-report.mjs` + beamer 夹具；三组样本 34 点：正向/反向/同文件 **34/34**、跳到位 ≤3 行 31/34（**≤5 行 34/34**）；修掉「生成文件被当源码打开 / 失败静默 / 编译中竞争」三处缺陷。见 design.md §预览 |
| ㉔ | 文档欠账：`cli-mcp-plan.md` 的路径引用迁到 `texpresso-infra` | 2026-09 | `src-tauri/runner.rs` / `src-tauri/storage.rs` 已改为 `crates/texpresso-infra/src/{runner,storage}.rs`（本次复核确认无残留；⑥ 完成后该文的现状盘点章节已改写为使用说明） |
| ㉚ | 构建确定性验证 | 2026-09 | 新增 `scripts/check-determinism.mjs`；**实测**：不固定 `SOURCE_DATE_EPOCH` 时同一份源码两次编译不一致（差异只在 trailer `/ID`：Quick 首差 @92143 共 67 字节），固定后 **3 档 × 2 路径全部逐字节一致**；顺带实测排除"会影响 `\today`"的顾虑。见 [design.md](../design.md) §构建确定性 |
| ㉛ | 编译决策日志 | 2026-09 | 触发侧打"不触发"的三类原因（无根文件 / 项目外 / 被忽略）；调度侧打 开始(root/kind/timeout)、入队等待、合并替换、直接执行最新、成功(draft)/失败(kind)、终止清队。core 加 `tracing` facade（非 IO，ADR-0006 仍成立） |
| ㉓ | 编辑非 UTF-8 源文件 | 2026-09 | `read_file` 遇 `InvalidData` 返回**中文可操作提示**（另存为 UTF-8），前端状态栏提示条承接（此前是英文 IO 错误 + 无人接的 rejection = "点了没反应"）；**故意不做 lossy 打开**（保存会写回 U+FFFD = 静默损坏文件）。真机用 `中文GBK工程/子目录/gbk.tex` 验证（截图） |
| ㉗ | `.ins`/`.dtx` 源码版模板 | 2026-09 | `Diagnosis.missing_file` + core `source_release_hint` + runner 读项目根：缺 `.cls` 且项目里有同名 `.ins` → 给具体命令 `xelatex X.ins`。**实测教训**：`.ins`/`.dtx` 同名时目录序会先选中 `.dtx`（文档源码，不是安装脚本）→ 显式优先 `.ins`；只有 `.dtx` 时不编造命令（指向模板 README）。真机两行式展示已验证 |
| ㉑ | 外部改 `.texpresso/settings.json` 同步 `root_file` | 2026-09 | watch 热更新后同步内存 `ProjectState.root_file`（设覆盖按 D8 解析、清覆盖回自动探测）+ **合并基数改为磁盘纯全局**（否则"清掉覆盖"永远不生效——验证中实测发现）+ 前端 `settingsChanged` 不一致时 `syncProject()`；`detect_root` 的 IO 编排下沉 core 供命令面与 watch 共用。**真机双向验证**：外部设 → 提示消失；外部清 → 提示回来。顺带修掉外部非原子写导致的"读到截断文件即丢弃"（3×150ms 短重试） |
| ⑥ | CLI + MCP 交互接口 | 2026-09 | 新 crate `texpresso-server`（headless `Session` + `texpresso-cli` + `texpresso-mcp`，**零新增第三方依赖**）。**实测闭环**：`open → read → write → compile(912ms 成功) → 改坏 → compile(status:failed + 诊断 + 退出码 1) → 修好 → compile 成功`；SyncTeX 正反向往返命中；同一源码两次编译 PDF 逐字节一致。回归固化：`cargo test -p texpresso-server`（9 用例）+ `--test mcp_stdio`（**真实二进制 + stdio 管道** 5 用例）+ 5 个 `#[ignore]` 真编译用例全绿。用法/工具表/偏差/限制见 [cli-mcp-plan.md](../cli-mcp-plan.md)，契约见 [modules.md](../modules.md) §8.1 |

> 同期完成的**非 roadmap 工程项**（记录以免重复提议）：`texpresso-infra` 拆分（ADR-0010）、大纲解析下沉 Rust、架构图与渲染脚本、MCP Bridge 0.12→0.13 升级、真机验收清单固化。

## 2. 评分模型

四项 1–5 分，**分数越高＝该维度越强/越贵**：

| 维度 | 含义 | 打分依据 |
|---|---|---|
| **W** 痛点权重 | 市场频率 × 严重度 | 调研量化信号（issue 数、SE 票数/浏览量、来源数） |
| **D** 差异化 | 相对竞品能否形成壁垒 | 5＝竞品结构性做不到；3＝大家都有但可做得更好；1＝无意义 |
| **C** 实现成本 | 工程投入 | 5＝数月/高风险；4＝数周；3＝1–2 周；2＝几天；1＝小时级 |
| **V** 验证难度 | 能否证明"真的做好了" | 5＝需干净机器/多人/长期；3＝需真机交互且含主观判断；**2＝MCP/脚本可确定性断言**；1＝单测即可 |

**优先级分**：`P = (W + D) / (C + V)`；**分档**：P0 = `P ≥ 1.3 且 W ≥ 4`；P1 = `P ≥ 0.9`；P2 = `P ≥ 0.6`；`P < 0.6` 或定位冲突/已证伪 = 不做。

## 3. 待办总表（按 P0 / P1 / P2 分档，档内按 P 分降序）

| ID | 事项 | W | D | C | V | P | 档 | 主要依据 |
|---|---|---|---|---|---|---|---|---|
| ⑦ | 大文档编辑器侧性能 | 5 | 4 | 4 | 3 | **1.29** | P1 | desktop 报告"最大可攻占缺口" |
| ⑧ | 跨文件 LaTeX 语义操作 | 4 | 5 | 4 | 3 | **1.29** | P1 | P10：VS Code 明确缺失 |
| ⑩ | 深色主题（Candy Desk 暗色） | 3 | 2 | 2 | 2 | **1.25** | P1 | 配置类第一痛点：206 票 / 23 万浏览 |
| ㉖ | **模板自带 latexmkrc 与构建约定的交互**（**已有硬证据，见 §6.1**） | 3 | 4 | 3 | 3 | **1.17** | P1 | hithesis（rc 覆写 `$pdflatex` + `--shell-escape` + 尾部 `;cp`）在 `-outdir=tmp` 下 `\include{body/...}` 写不出 `body/introduction.aux`、报错后 latexmk 挂住不退出；**而精简 rc 最小复现仍能恢复** → 具体触发条件待定位 |
| ⑪ | 预览滚动/缩放保持、只重排变更页 | 4 | 3 | 4 | 2 | **1.17** | P1 | P11：官方承认"丢失滚动位置"；V 3→2 |
| ⑨ | LSP（texlab）集成 | 4 | 3 | 4 | 3 | **1.00** | P1 | P8：补全/引用是长期痛点 |
| ⑫ | TinyTeX 捆绑与环境引导 | 5 | 4 | 5 | 4 | **1.00** | P2* | C/V 全表最高；建议独立里程碑 |
| ⑬ | AI 集成 | 3 | 4 | 4 | 3 | **1.00** | P2 | 2026 竞争主轴；**仍无一手需求证据** |
| ⑭ | 拼写检查 | 3 | 2 | 3 | 2 | **1.00** | P2 | P8：拼写/语法弱 |
| ㉙ | 未存盘 overlay（编辑期不写用户文件） | 2 | 3 | 3 | 2 | **1.00** | P2 | 吸收自 §5.4（`edit_data` overlay），但与 **ADR-0007「文件系统为真相源」冲突**，且无实测痛点支撑 |
| ⑮ | 多窗口与多项目 | 3 | 2 | 4 | 3 | **0.71** | P2 | design.md 后置项 |
| ⑯ | 外部 PDF 查看器 | 2 | 2 | 3 | 3 | **0.67** | P2 | design.md 后置项 |
| ⑰ | 自动更新 | 2 | 2 | 3 | 3 | **0.67** | P2 | design.md 后置项 |
| ⑱ | 代码签名证书 | 2 | 2 | 2 | 4 | **0.67** | P2 | 分发信任，但外部依赖重 |

> P0 档当前为空：㉕ 首编超时、⑤ SyncTeX、⑥ CLI+MCP 已完成并移入 §1 基线；下一优先项是下表首行的 ⑦。

\* 已完成项里 ⑥ 的分数（2.00）仍是全表最高，但 **W=3** 不满足 P0 的 `W ≥ 4` 门槛——它的价值在"杠杆"（Agent 可驱动 + 顺带成为其他项的自动化验证手段），不在用户痛点权重。**下一步的排序不变**：Batch 3 按 ⑦ → ⑧⑨ → ⑩⑪㉖ 推进。

## 4. 快速通道（小项，成本 ≤2，随时可插）

> **2026-09：本通道已清空**——以下五项全部完成（证据见 [§1 基线](#1-已完成基线保留-id-与证据不再出现在待办中)），当前没有待办小项。

| ID | 事项 | W | D | C | V | P | 状态 |
|---|---|---|---|---|---|---|---|
| ㉓ | **编辑 GBK/非 UTF-8 源文件**：至少给出**中文**错误提示 | 3 | 3 | 2 | 2 | **1.50** | ✅ 2026-09（含状态栏提示条；不做 lossy 打开） |
| ㉚ | **构建确定性验证**：连跑两次逐字节比 PDF；不行则先让它确定 | 2 | 2 | 1 | 1 | **2.00** | ✅ 2026-09（`SOURCE_DATE_EPOCH` 固定后 6/6 组合逐字节一致） |
| ㉗ | **`.ins`/`.dtx` 源码版模板**：识别并提示"需先编译 `*.ins` 生成 `.cls`" | 2 | 3 | 2 | 3 | **1.25** | ✅ 2026-09（给具体命令；`.ins` 优先于 `.dtx`） |
| ㉑ | **外部改 `.texpresso/settings.json` 时同步 `ProjectState.root_file`** | 3 | 2 | 3 | 2 | **1.00** | ✅ 2026-09（双向真机验证；顺带修掉合并基数与竞态） |
| ㉛ | **编译决策日志补全**：把"为什么这次触发/合并/跳过/失败"打出来 | 2 | 1 | 1 | 1 | **1.50** | ✅ 2026-09（触发侧 + 调度侧全链路） |

> 表头「P」按公式算，与档位无关——快速通道的取舍标准是**成本 ≤2 且独立可交付**。

## 5. 外部方案评估：TeXpresso 式「实时渲染」

**评估对象**：[texpresso-live-rendering-roadmap.md](../texpresso-live-rendering-roadmap.md) —— 对上游 [let-def/texpresso](https://github.com/let-def/texpresso)（一个把 XeTeX **改造成可 fork 的实时预览器**的项目）的源码通读与复刻路线（8 阶段）。

### 5.1 它要求什么前提

该文档 §1 自列了四条**必须同时成立**的前提（任一不成立，对应机制直接放弃）：

| # | 前提 | 它怎么满足 |
|---|---|---|
| G1 | 能**拦截目标程序的每一次 I/O**，且拿得到字节偏移 | 它自己改造了 XeTeX（`src/engine/`，`texpresso_protocol.c` 的 `Q_READ`/`A_READ` 协议） |
| G2 | 输出格式支持**字节偏移重同步** | 它解析 **DVI**，靠 BOP/EOP 标记做页索引（`incdvi.c`） |
| G3 | 目标程序能接受**部分输入状态**继续 | 进程内引擎，配合 VFS 三层内容模型（`edit_data` overlay） |
| G4 | `fork()` 快照：目标程序**单线程、确定性**、无 GPU/驱动状态 | 它就是被 fork 的那个进程 |

### 5.2 我们满足几条 —— **0 / 4**

| 前提 | TexPresso 的现状 | 判定 |
|---|---|---|
| G1 拦截 I/O | 我们跑的是**未改造的 stock latexmk + xelatex**（外部进程），无法拦截其每一次读；被改造的引擎意味着要自建并长期维护一个 XeTeX 分支 | ❌ |
| G2 字节偏移重同步 | 我们的产物是 **PDF**（xelatex → `xdvipdfmx`），且**在编译结束时才落盘**，不流式；pdf.js 也没有"增量更新已解析文档"的能力（它是整份 parse） | ❌ |
| G3 部分输入 | latexmk 只认文件系统；我们的模型是"编辑器自动保存 → 监视 → 编译" | ❌ |
| G4 fork 快照 | **Windows 无 `fork()`**——该文档 §1.3 自己写明：Windows 要么跑 WSL，要么放弃阶段 7；而本项目 **Windows 首发** | ❌ |

**该文档 §1.1 的"量化收益门"我们倒是满足了**（③ 实测全量重编译 1.6–13.5s，落在它所谓"值得做到阶段 6/7"的区间）——但 §5.2 的四条前提一条不成立。**按它自己的判定规则：停手。**

### 5.3 我们实际处在它的第几阶段

| 它的阶段 | 我们的状态 |
|---|---|
| 阶段 1 端到端骨架 | ✅ **已完成**（改文件 → 编译 → 解析 → 显示） |
| 阶段 2 子进程 + 流式输出 + 时间片 | 🟡 **部分**：子进程 ✅；**流式输出 ❌**（PDF 不流式）；时间片 ✗ 不需要（编译在独立进程，前端本来就不阻塞） |
| 阶段 3 增量输出解析 | ❌ 前提 G2 不成立 |
| 阶段 4 脏矩形显示 | 🟡 **已被粗粒度等价物覆盖**：我们的分页 DOM 虚拟化 + 同文件 canvas 复用（见 [modules.md](../modules.md) §9.4）解决的是同一个"滚动/缩放不重绘整页"问题；PDF 路线下无法做到矩形级增量 |
| 阶段 5 VFS + 事务回滚 | ❌ 与 **ADR-0007「文件系统为内容真相源」直接冲突**（overlay 会引入第二真相源）；且需引擎侧配合 |
| 阶段 6 seen 水位 + trace | ❌ 前提 G1 不成立 |
| 阶段 7 fork 快照 + fence | ❌ 前提 G4 不成立（Windows）+ G1 不成立 |
| 阶段 8 多趟收敛 | 🟡 **思想可吸收**（见 ㉘）：与引擎无关，只关乎"何时跑多趟" |

### 5.4 结论与吸收清单

**结论：不采用该路线。** 它对本项目不可行的根因不是"难"，而是**它要求拥有目标程序的内部**（改造引擎 + fork + 拦截 I/O），而我们选择的是"驱动 stock TeX Live"这条完全不同的路——这个选择本身由 Windows 首发（无 fork）与 ADR-0007（FS 为真相源）锚定，**不建议为一个预览优化去维护一个 XeTeX 分支**。

**吸收 3 条（与引擎无关的思想）+ 1 条明确降级**：

| 吸收项 | 来源 | 变成 |
|---|---|---|
| **空闲收敛**（编辑期跑轻的、停手后跑全的） | 阶段 8「空闲 500ms 后触发一趟跑到底」 | **㉘**（已落地：编辑期 Quick 单趟 + 2s 空闲收敛，见 [design.md](../design.md) §附） |
| **把决策过程打出来** | §5「没有这些日志，你会在'为什么这次编辑重算了整篇'上耗掉大量时间」 | **㉛**（快速通道） |
| **确定性验证**（连跑两次逐字节比） | §1.2「这一步必须通过，否则快照机制会静默给出错误结果」 | **㉚**（快速通道；对我们是"输出 diff 类优化的前提"） |
| 未存盘 overlay | 阶段 5a `edit_data` | **㉙**（P2；与 ADR-0007 冲突，无实测痛点支撑） |

**明确不做（本评估的 4 条否决项，见 §7）**：fork 快照、seen 水位/trace、增量输出解析（DVI 字节偏移）、VFS 事务回滚。

### 5.5 「那我们自己改造 XeLaTeX 行不行」——评估结论

**法律上可以**：XeTeX 由 Jonathan Kew 开发、按 **X11（MIT 类）许可**分发（[Wikipedia](https://en.wikipedia.org/wiki/XeTeX)），fork 与修改没有许可障碍。按 TeX 系惯例，改过的引擎要**改名**（pdfTeX / XeTeX / LuaTeX 本身都是这么来的），不能继续叫 xelatex。

**但改造拿不到最值钱的那一项**：

| 目标机制 | 改造引擎后是否可达 | 说明 |
|---|---|---|
| 阶段 7 fork 快照 | ❌ **仍不可达** | Windows 无 `fork()`。该文档 §1.3 自己写：显式序列化状态"成本高一个数量级，通常不如放弃阶段 7" |
| 阶段 6 seen 水位 | ✅ **可达**（改造的最大价值） | 需要拦截每一次读 + 字节偏移，正需 C 层改造 |
| 阶段 3 增量 DVI 解析 | ⚠️ 可达但代价大 | 要 DVI 输出 + 自研解析/渲染——等于重建预览栈（现为 PDF + pdf.js） |
| 阶段 5 VFS/overlay | ✅ **不需要改 C 也能做** | 见下 |

**代价（三层）**：① 从源码构建并长期跟进 TeX Live（Windows MSVC 构建需 ICU/HarfBuzz/FreeType/Graphite2/poppler 整套，[tlbuild](https://www.tug.org/texlive/doc/tlbuild.html)）；② **分发性质改变**——不再"用你已有的 TeX Live"，而要分发自建引擎（与 ADR-0003 不签名分发叠加，放大体积与信任成本）；③ 包与字体更新要自己跟。

**两条不改造引擎的替代路径（应先评估）**：

1. **LuaLaTeX + Lua 回调**：`find_read_file` / `open_read_file` 是**官方支持的扩展点**，社区已有"用 Lua 伪造文件系统输入"的实现（[Alan Xiang: Mimicking File System Input](https://www.alanshawn.com/tech/2020/07/07/luatex-mimic-input.html)、[TeX SE](https://tex.stackexchange.com/questions/684792/with-luatexbase-add_to_callback-how-can-i-conditionally-get-default-behavior-f)）。→ **㉙ 未存盘 overlay 用纯 Lua 即可，不必碰 C**；LuaTeX 也能 `--output-format=dvi`，但走 DVI 就要自研预览栈，不划算。
2. **Tectonic**：上游 TeXpresso 实际是**基于 fork 的 Tectonic**（[tectonic discussion #1029](https://github.com/tectonic-typesetting/tectonic/discussions/1029)："powered by a fork of Tectonic … snapshotting in XeTeX engine directly"）。Tectonic 是 Rust、可作为库嵌入——**若真要"引擎在自己手里"，从 Tectonic 入手远比改 TeX Live 的 C 版 xetex 现实**，且 Rust 与本项目后端同构。

**建议顺序（与 §9 决策原则一致）**：先做 ㉘（测收益）+ ㉚㉛（便宜）→ 若证明瓶颈在"每趟重算"，再考虑 LuaLaTeX 回调做 overlay（不改 C）→ **"拥有引擎"只应作为一次显式战略决策（另立 ADR），而不是一个功能项**；真要走，走 Tectonic 而不是 texlive 的 xetex。

## 6. 分档详述（P1/P2 未完成项）

### 6.1 ㉖ 模板自带 latexmkrc 与构建约定的交互（P1，证据已就位）

- **硬证据**：`thesis-real-hithesis`（TeX Live 自带样例）用产品完全相同的命令冷编译，~4 分钟后报 `! I can't write on file 'body/introduction.aux'` → `Emergency stop`，随后 `latexmk`/`perl` **挂住不退出**（实测 10 分钟零 CPU）——产品侧因此看到的是「超时」而不是「内容错误」。
- **已知机制**：`\include{子目录/文件}` 要写 `tmp/子目录/文件.aux`，而 xelatex **不创建输出目录的子目录**；latexmk 会在第一趟失败后建目录并自动重跑，所以合成的 `multifile` 档无事，hithesis 这一档没有恢复。
- **嫌疑**：hithesis 自带 latexmkrc（覆写 `$pdflatex`、内建 `--shell-escape`、尾部 `;cp`）与 `-outdir=tmp` 的相互作用。
- **尚未定位**：用精简 rc 复刻其关键行跑最小工程仍能正常恢复 → **没有可复现的 rc 最小组合**。最小复现与可用绕过（`\include` → `\input`）见 [troubleshooting.md](../troubleshooting.md)「`\include{子目录/文件}` + `-output-directory`」。
- **出口条件**：带 latexmkrc 的真实模板能编译成功，或产品给出明确可操作的诊断；③ 的 ">240s 未收敛" 记录在 ㉖ 修好后用 `node scripts/bench.mjs --with-real --no-warmup` 重测。

### 6.2 ⑥ CLI + MCP 接口 —— ✅ 已完成（2026-09）

已移入 [§1 基线](#1-已完成基线保留-id-与证据不再出现在待办中)。实现与用法、实测证据、与原计划的偏差、已知限制见 [cli-mcp-plan.md](../cli-mcp-plan.md)；实现契约见 [modules.md](../modules.md) §8.1。
三条对后续项的影响：**① 自动化验证手段**——⑧⑨ 语义层、㉖ latexmkrc 这类"改完要证明真的对了"的项，现在可以先用 CLI 跑闭环再上真机；**② `compile` 有了确定性退出码**（0 成功 / 1 编译未通过），脚本化断言不再依赖解析日志；**③ 与 GUI 的分工写清了**——同一项目不并发编译（headless 独占），没有引入项目锁。

### 6.3 P1 其余项（理由未变）

⑦ 大文档编辑器侧性能（量化手段已由 ③ 就位）｜⑧ 跨文件语义操作（**建议与 ⑨ 合并成"语义层"里程碑**）｜⑩ 深色主题（Monaco + Monarch + pdf.js 反色需一起调）｜⑪ 预览滚动/缩放保持｜㉖ latexmkrc 交互（见 §6.1）｜⑨ LSP（先做 monaco-languageclient 技术验证）。

### 6.4 P2 / 后置

⑫ TinyTeX（C/V 最高，独立里程碑）｜⑬ AI（无一手需求证据，先观察）｜⑭ 拼写检查｜**㉙ 未存盘 overlay**｜⑮ 多窗口多项目｜⑯ 外部查看器｜⑰ 自动更新｜⑱ 代码签名。

## 7. 明确不做（否决项）

| 项 | 理由 |
|---|---|
| **TeXpresso 式 fork 快照**（其阶段 7） | Windows 无 `fork()`（该文档 §1.3 自述）；且需维护被改造的 XeTeX 分支 |
| **seen 水位 / trace 回滚**（其阶段 6） | 需拦截引擎每一次 I/O 与字节偏移（前提 G1 不成立） |
| **增量输出解析（DVI 字节偏移）**（其阶段 3） | 我们产物是 PDF 且不流式；pdf.js 也不支持增量更新文档（前提 G2 不成立） |
| **VFS 三层 + 事务回滚**（其阶段 5） | 与 **ADR-0007「文件系统为内容真相源」**直接冲突；且需引擎侧配合 |
| **自建/改造 TeX 引擎**（含 fork TeX Live 的 xetex） | 见 §5.5：许可允许（X11）但代价是三层（自建构建+分发自建引擎+自己跟包），**且最值钱的 fork 快照在 Windows 上仍不可达**；不改造也能拿到 overlay（LuaLaTeX 回调）。真要走应走 Tectonic 并另立 ADR |
| **实时协作** | P=0.40；在线方案护城河，本地方案投入产出不成立。定位「个人 / 离线优先」，协作交给 Git |
| **增量编译** | [ADR-0005](../adr/0005-latexmk-first-incremental-next.md) 已实测证伪：latexmk 增量＝整份单遍重排，属 xelatex 引擎上限 |
| **②-2 引擎自动推断** | ⑲ 实测：**0/19** 模板因默认 XeLaTeX 选错、`\RequirePDFTeX` 0/7374。改用 ④ 的「选错时明确告诉用户怎么改」替代 |
| **推断 bib 工具链** | 实测 latexmk 自己就会跑 `bibtex`/`biber`，无需推断也无需用户配置 |
| **自研 PDF 渲染引擎** | pdf.js 已是够用底座；LaTeX Workshop 的教训是"预览是二等公民"，不是"必须自研渲染器" |
| **自研完整 LaTeX AST/解析器** | 复用 texlab/现有解析；⑧⑨ 的语义层建在可维护索引上 |

## 8. 批次与出口条件

| 批次 | 内容 | 出口条件 |
|---|---|---|
| **Batch 1 收口** | ~~快速通道 ㉚㉛㉑㉓㉗~~（✅ 2026-09 全部完成） | 小项闭环（每项独立可交付）——**已达成** |
| **Batch 2 攻核心承诺** | ~~⑥ CLI+MCP~~（✅ 2026-09 完成，移入 §1 基线） | Agent 能驱动「改 → 编 → 验」闭环——**已达成**（真机走通并固化为 `cargo test -p texpresso-server` 回归） |
| **Batch 3 硬骨头** | ⑦ 大文档编辑器侧性能 → ⑧⑨ 语义层（合并） → ⑩ 深色主题 / ⑪ 预览状态保持 / ㉖ latexmkrc（**证据已就位，优先级应上调**） | 大文档打字不掉帧；跨文件重命名可用；主题一致；带 rc 的模板编译路径有结论 |
| **独立里程碑** | ⑫ TinyTeX 捆绑 | 干净 Windows 机器零预装可用 |

## 9. 决策原则（从本例提炼，后续取舍按此）

1. **不拥有目标程序内部**：我们驱动 stock TeX Live。任何要求"改造引擎 / 拦截其 I/O / fork 其进程"的方案，一律先按 §5.2 的 4 条前提过筛。**若要推翻这条（例如自建引擎），必须是一次显式战略决策并另立 ADR**，不能作为功能项顺手带进来（§5.5 给了两条不必拥有引擎的替代路径）。
2. **先量化收益再上机制**：③ 已让"量化"变成一条命令；新增优化若拿不出 ≥20% 的实测改善，不进 Batch。
3. **证据不足就压后，不硬排**：②-2 因 ⑲ 出数被砍、㉙ 因与 ADR-0007 冲突且无痛点而降 P2——**"没有证据"本身就是不做的理由**。
4. **文档=事实来源**：每项完成后回写 design/modules/troubleshooting，且只记实测结论（被证伪的也回写）。
5. **阈值与成本打分标 `[推断]`**：可被推翻，但必须留下可溯源的依据。

## 10. 与现有文档的对应

状态以本文件 §1（已完成基线）/ §3–§4（待办）为准；下表只回答"实现与证据在哪读"。

| 本文件项 | 实现 / 证据位置 |
|---|---|
| ① 中文路径 | [troubleshooting.md](../troubleshooting.md)「中文文件名/路径兼容性实测」 |
| ②-1 根文件候选 | [modules.md](../modules.md) §8 命令表 / §9.2 stores / §9.4 组件表 / §10 契约 |
| ②-2 引擎推断（否决） | [design.md](../design.md) §后置/未决清单「已否决」 |
| ③ 性能基准 | design.md §基准脚本与实测基线 + `scripts/{gen-bench-projects,bench}.mjs` |
| ④ 错误诊断 | modules.md §4.1 + design.md §错误列表 + `log_parser/diagnosis.rs`（22 类）/ `real_error_corpus.rs`（23 例真实日志） |
| ㉕ 首编超时 | modules.md §4.2 + design.md §失败语义 + [CONTEXT.md](../../CONTEXT.md)「超时」+ ErrorList 一键重试 |
| ㉘ 编辑期单趟 + 空闲收敛 | design.md §附：latexmk 开销拆解 + modules.md §2.4 / §2.6 / §9.3 + `useIdleConvergence.ts` |
| ⑤ SyncTeX | [ADR-0008](../adr/0008-synctex-via-cli-with-interface.md) + modules.md §5 + design.md §预览 + `scripts/synctex-report.mjs` |
| ⑥ CLI+MCP（**已实现**） | [cli-mcp-plan.md](../cli-mcp-plan.md)（用法/工具表/实测/偏差/限制）+ modules.md §8.1（契约与验证入口）+ `crates/texpresso-server` |
| ⑦ 大文档编辑器侧性能 | modules.md §3.5「已知成本」+ §12.1 |
| ㉖ latexmkrc 交互 | 本文件 §6.1 + troubleshooting.md「`\include{子目录/文件}` + `-output-directory`」 |
| ㉚㉛ | [texpresso-live-rendering-roadmap.md](../texpresso-live-rendering-roadmap.md)（外部方案）+ 本文件 §5 |
| ⑫ TinyTeX / ⑮⑯⑰⑱ | design.md §后置/未决清单 |
| ㉔ 文档欠账（已修） | `crates/texpresso-infra/src/{runner,storage}.rs` 引用已就位（当时核对的是 cli-mcp-plan 的现状盘点章节，该章节已随 ⑥ 完成改写为使用说明） |

## 11. 不确定性与待补数据

1. **W 打分是二手推断**：调研全部来自公开社区数据，**没有一手用户访谈/问卷**。真实取舍前建议补 5–10 个目标用户访谈。
2. **成本 C 未经工程估算**：C 是分析者基于当前代码结构的判断 `[推断]`，落地前应由实现者复核（尤其 ⑦⑧⑨）。
3. **付费意愿未验证**：只找到"换工具"意愿，没有付费证据。
4. **V 的重估依赖"MCP 会话在场"**：MCP 驱动断言需应用在 debug 下运行 + 会话连接，**不是 CI 可跑的自动化**；V=2 表示"半脚本化、确定性断言"。
5. **预算口径待复核**：③ 实测"冷编译 ≤2s 的小文档"在本机几乎不存在（连 6 行文档都 2.3s）→ design.md 的 小/大 文档分类与 1s/2s、3s/5s 阈值需要重新定义（放宽？还是改为相对基线的回归容忍度？）。
6. **㉘ 的剩余开口项**：bib/biber 场景的编辑期样本（现有 fixture 不触发 bibtex）。收益（平均 40.2%，6/6 档达阈值）与代价（目录页码落后一趟）均已量化，见 [design.md](../design.md) §附。
7. ~~**① 遗留**：编辑 GBK 源文件仍只报英文 IO 错误~~ —— **㉓ 已处理**：改为中文可操作提示 + 状态栏提示条；仍**不支持编辑**（不做 lossy 打开，见 modules.md §12.1 #2），这是设计取舍不是欠账。
8. **③ 真实档数字需重测（㉕ 实测修正）**：`thesis-real-hithesis` 的 ">240s / >600s 未收敛" 至少部分是**"报错后 latexmk 挂住"**造成的（见 §6.1），不是纯慢。㉖ 修好后必须重跑 `node scripts/bench.mjs --with-real --no-warmup` 才能给真实论文档一个可信的耗时上界。
9. **㉖ 的触发条件未定位**：`\include{子目录}` + `-outdir=tmp` 的最小复现在**无 rc / 精简 rc** 下 latexmk 都能自愈（建目录 + 重跑）；hithesis 那一档不自愈且挂起，**具体是 rc 的哪一行造成的还没定论**（已排除"单纯覆写 `$pdflatex` + 尾部 `;cp`"这一组合）。
10. **beamer 的 2–4 行往返偏移未消除（已定性）**：三组样本里只有 beamer 有偏移，且换取块规则后不变 → 归因于 beamer/主题的 synctex 记录粒度。要进一步改善得离开"CLI + 记录匹配"这条路（例如自己在 `.synctex` 里做盒模型筛选），与 ⑤ 的成本/收益不成比例，**暂不做**。
11. **⑥ 留了三个未做项**（不阻塞闭环，按需再开）：MCP 状态订阅（notifications，一期用 `compile` 同步返回 + `status`/`errors` 快照代替）、headless 与 GUI 并发编译同一项目的锁（现约定 headless 独占）、headless `file_write` 不代建父目录（与 GUI `save_all` 同契约）。
12. **⑥ 的 `compile` 不再有"等待期状态"**：headless 是"跑一次拿结果"，`status` 只在**调用返回后**有意义（GUI 那种 `queued/running` 中间态不可见）。想让 Agent 看到进度需要先做第 11 条的订阅——目前没有证据表明需要。