# Latteset Roadmap（当前状态、优先级与批次）

> 作用：把调研结论变成**当前可执行的优先级与批次**，并保留已完成项的 ID 与证据指针。
> 依据：[调研总览](./tex-ide-pain-points.md) + [desktop](./desktop-latex-editor-pain-points.md) / [online](./online-latex-editor-pain-points.md) / [vscode](./vscode-latex-workshop-pain-points.md) 三份专项 + [P0-② 分析](./p0-2-first-open-analysis.md) + [模板语料调研](./template-corpus-survey.md) / [中文高校模板引擎要求](./cn-thesis-template-engines.md) + [上游实时渲染方案](../texpresso-live-rendering-roadmap.md)（评估见 §5）
> 现状实现：[design.md](../design.md)（产品语义与实测数字）、[modules.md](../modules.md)（实现契约、已知债与验证入口）、[cli-mcp-plan.md](../cli-mcp-plan.md)（headless 用法）、[troubleshooting.md](../troubleshooting.md)（操作坑与真机验收清单）
>
> **ID 稳定约定**：①…㉗ 沿用历史编号（`modules.md` / `troubleshooting.md` / `design.md` / 源码注释里的「roadmap P0-①」「P0-②-1」「roadmap ④」等引用因此不失效）；新增项从 **㉘** 起续编。已完成项**保留 ID 并移入 §1 基线**，不再占用待办位次。
> **证据约定**：**实测** = 可复现的本机结果（命令见各报告）；**[推断]** = 未实测的判断；未标注的规划项一律视为未实现。
>
> **当前下一步**：**⑧⑨ 语义层**（合并里程碑，见 §6.3）→ **⑩⑪㉖**；⑦c 已完成（夹具 + 口径 + 数据，结论是"不需要优化"，见 §1 与 [p1c 报告](./p1c-multifile-large-project.md)）。§3 是唯一的排序来源，§4 说明批次与出口条件。

## 1. 已完成基线（保留 ID 与证据指针）

| ID | 项 | 完成 | 结论与证据 |
|---|---|---|---|
| ① | 中文文件名/路径与编码实测复核 | 2026-09 | 路径层面全链路可用；修掉真实缺陷（GBK 源 + pdflatex 的 `.log` 非法 UTF-8 → 「编译失败」退化为「拿不到任何错误信息」）。MCP 补验：中文路径 PDF 渲染正常、反向 SyncTeX 跳 `Ln 10`。见 [troubleshooting.md](../troubleshooting.md) |
| ②-1 | 根文件候选可见可交互 | 2026-09 | `ProjectInfo.root_candidates` + `get_project` + `RootFilePicker.vue` + 状态栏入口；**并修掉阻断缺陷**：`update_settings` 不同步内存 `ProjectState.root_file`。见 [modules.md](../modules.md) §12 |
| ③ | 性能基准与回归基建 | 2026-09 | `scripts/{gen-bench-projects,bench}.mjs`：六档纯文本 fixture + 可选真实模板档；一条命令出 cold/noop/edit 报告，超预算退出码 1。**结论：瓶颈在小文档固定开销而非大文档**；三档小文档超 2s 及格线；真实论文档 cold >240s（口径待重定，见 §10） |
| ④ | 错误诊断升级 | 2026-09 | 19 类 `DiagnosisKind`（缺包/缺类/缺字体/字体集/引擎不匹配/未定义命令/组未闭合/连锁 Emergency stop…），匹配不到返回 `None` 降级原文；**23 例真实语料** + 手写期望表 + DoD 覆盖率 ≥80%；真机验证两行式展示 + 点条目跳 `Ln 4` |
| ⑤ | SyncTeX 可靠性加固（含 ㉒、⑳） | 2026-09 | `scripts/synctex-report.mjs` + beamer 夹具；三组样本 34 点：正向/反向/同文件 **34/34**、跳到位 ≤3 行 31/34（**≤5 行 34/34**）；修掉「生成文件被当源码打开 / 失败静默 / 编译中竞争」三处缺陷。见 design.md §预览 |
| ⑥ | CLI + MCP 交互接口 | 2026-09 | 新 crate `latteset-server`（headless `Session` + `latteset-cli` + `latteset-mcp`，**零新增第三方依赖**）。实测闭环 `open → read → write → compile(成功) → 改坏 → compile(failed + 诊断 + 退出码 1) → 修好 → 成功`；SyncTeX 正反向往返命中；两次编译 PDF 逐字节一致。回归：`cargo test -p latteset-server`（10）+ `--test mcp_stdio`（真实二进制 + stdio 管道 5）+ 真编译 `#[ignore]` 用例。用法见 [cli-mcp-plan.md](../cli-mcp-plan.md)，契约见 [modules.md](../modules.md) §8.1 |
| ⑦a | 大纲重扫增量（⑦ 的拆项之一） | 2026-09 | core `OutlineCache`（按内容指纹复用未变化文件的扫描结果，**内容每次仍重取**）+ 前端只提交**变化过**的缓冲（`lastSent` 差分 + `open_paths` 淘汰）+ 刷新合并。实测（11 文件/436KB）：全量重扫 + 全量提交 **55.5ms → 9.7ms**（编辑一个文件）/ **8.4ms**（无编辑）；真机端到端 `scans=1 reused=10` → `scans=0 reused=11`。顺带修掉**入口路径未归一**（同一文件两种拼写 → 大纲重复项）与**并发刷新重复发送**（结构事件风暴实测连发 11 次）。契约见 [modules.md](../modules.md) §3.5/§12.2，数据见 [p1 分析](./p1-large-doc-editor-analysis.md) §3.3 |
| ⑲ | 模板样本调研 | 2026-09 | **0/19** 案例因默认引擎选错；`\RequirePDFTeX` **0/7374**；文档调研 19/19 兼容 XeLaTeX → ②-2 砍掉。另暴露 4 类真实障碍（缺包/缺字体、首编超时、latexmkrc 交互、`.ins`/`.dtx`） |
| ㉑ | 外部改 `.latteset/settings.json` 同步 `root_file` | 2026-09 | watch 热更新后同步内存 `ProjectState.root_file`（设覆盖按 D8 解析、清覆盖回自动探测）+ **合并基数改为磁盘纯全局**（否则"清掉覆盖"永远不生效）+ 前端 `settingsChanged` 不一致时 `syncProject()`。真机双向验证。顺带修掉外部非原子写导致的"读到截断文件即丢弃"（3×150ms 短重试） |
| ㉓ | 编辑非 UTF-8 源文件 | 2026-09 | `read_file` 遇 `InvalidData` 返回**中文可操作提示**（另存为 UTF-8）+ 状态栏提示条；**故意不做 lossy 打开**（保存会写回 U+FFFD = 静默损坏文件）。真机验证（截图） |
| ㉔ | 文档欠账：`cli-mcp-plan.md` 的路径引用迁到 `latteset-infra` | 2026-09 | 已改为 `crates/latteset-infra/src/{runner,storage}.rs`，无残留（⑥ 完成后该文的现状盘点半节改写为使用说明） |
| ㉕ | 学位论文首编超时 | 2026-09 | 上限 **600→1800s**；**不再静默重试**（旧行为要白等两个超时窗口）；超时**进错误列表**并带证据化诊断（首编/源文件数/日志页码 → 慢 vs 疑似卡住；日志已有致命错误则直接报那条）+ `Diagnosis.suggested_timeout_secs` 与一键「提高到 Ns 并重试」。真机 cycle 已验证。**真实模板 DoD 被 ㉖ 阻塞**（§6.1） |
| ㉗ | `.ins`/`.dtx` 源码版模板 | 2026-09 | `Diagnosis.missing_file` + core `source_release_hint` + runner 读项目根：缺 `.cls` 且项目里有同名 `.ins` → 给具体命令 `xelatex X.ins`。**实测教训**：`.ins`/`.dtx` 同名时目录序会先选中 `.dtx` → 显式优先 `.ins`；只有 `.dtx` 时不编造命令。真机验证 |
| ㉘ | 编辑期轻量出图 + 空闲收敛 | 2026-09 | 实测省 **40.2%**（6/6 档达标）→ 落地：Quick 直调引擎 / Full 完整 latexmk；无产物自动升级 Full；`draft` 状态 + 状态栏「引用待更新」+ 2s 空闲收敛。真机 cycle 与 `fdb_latexmk` mtime 双重佐证。见 design.md §附 |
| ㉚ | 构建确定性验证 | 2026-09 | `scripts/check-determinism.mjs`；实测：不固定 `SOURCE_DATE_EPOCH` 时两次编译不一致（只差 trailer `/ID`：Quick 首差 @92143 共 67 字节），固定后 **3 档 × 2 路径逐字节一致**；顺带排除"会影响 `\today`"的顾虑。见 design.md §构建确定性 |
| ㉛ | 编译决策日志 | 2026-09 | 触发侧打"不触发"的三类原因（无根文件 / 项目外 / 被忽略）；调度侧打 开始(root/kind/timeout)、入队等待、合并替换、直接执行最新、成功(draft)/失败(kind)、终止清队。core 加 `tracing` facade（非 IO，ADR-0006 仍成立） |
| 阶段 2 | 引擎子进程 + 流式输出（上游实时渲染路线的吸收项，§5.3 / §5.8） | 2026-09 | 管道已接（`Stdio::piped()`）+ 尾随 `tmp/<stem>.log`（三条读任务 → 同一状态机）→ 状态栏「已排版 N 页」+ 错误列表**编译还没结束**就报致命错误。中间态走**独立事件**（`compile-progress` / `compile-errors`），前端按 `phase` 守卫——终态 `errors-updated` 不被晚到的中间态覆盖。真机实测（400KB / 162 页 ctexbook 首编）：`1.7s running → 3.4–4.8s 页数 2→162 → 8.2s success`；插错后 `41.5s` 收到实时错误、`42.3s` 出终态（37 条含警告）。**落地时才实测到的两条**：引擎输出在非 TTY 下是 4KB 块缓冲（按页 flush 的日志才是主力通道）、中间态可能晚于终态抵达。契约见 [modules.md](../modules.md) §2.4 / §2.6.1 |
| ⑦c | 多文件大项目（编辑器侧）：夹具 + 口径 + 数据 | 2026-09 | 三档夹具（9/21/41 文件 × 2.2–5.5 MB，后两档**同总量双倍文件数**以分离变量）+ 口径脚本 `scripts/editor-report.mjs`（WS 直驱真机、零第三方依赖、超门槛退出码 1）。**五项门槛全过**：每击键净开销 **0.077–0.097 ms 绝对值恒定**（与文件数/总量无关）、无 >50 ms longtask、折叠 0.73–1.45 ms/MB、大纲往返 5.07–6.41 ms/MB（门槛 20）、20×1 MB model 22–46 ms（门槛 100）；21 个标签全关后 model 25→4、缓冲 0 → **无泄漏**。**结论：不需要优化**。见 [p1c 报告](./p1c-multifile-large-project.md) |
| 增量编辑 × DVI（B/C/A 三件） | 2026-09 | **页哈希差分驱动的三处复用**（[报告](./incremental-edit-x-dvi.md) §2.3）：① B 逐页未变则**跳过预览重载**（`reloadKey` 不动；真机 `skippedReloads` 1→2）；② C 只重绘变化页（74 页改末章：`pagesRendered 7→0`、render **102→7ms**）；③ A 页哈希全同则**跳过 PDF 转换**（Quick 改 `xelatex -no-pdf` + 条件 `xdvipdfmx`，缓存 `tmp/<stem>.pages`）。core 新增 `xdv` 模块（页索引 + 页哈希，9 条单测）。**顺带**：XDV 页索引第一次进入产品路径（此前只是诊断工具） |
| 无 fork 的替代路径（G4 / 阶段 6·7） | [no-fork-alternatives.md](./no-fork-alternatives.md)（快照 API 盘点 + **导言区占比实测 71–89.5%** + 5 条替代逐条判定） |

> 同期完成的**非 roadmap 工程项**（记录以免重复提议）：`latteset-infra` 拆分（ADR-0010）、大纲解析下沉 Rust、架构图与渲染脚本、MCP Bridge 0.12→0.13 升级、真机验收清单固化、**为 DVI/G2 研究新增 `scripts/xdv-report.mjs`**。

## 2. 评分模型与通道

四项 1–5 分，**分数越高＝该维度越强/越贵**：

| 维度 | 含义 | 打分依据 |
|---|---|---|
| **W** 痛点权重 | 市场频率 × 严重度 | 调研量化信号（issue 数、SE 票数/浏览量、来源数） |
| **D** 差异化 | 相对竞品能否形成壁垒 | 5＝竞品结构性做不到；3＝大家都有但可做得更好；1＝无意义 |
| **C** 实现成本 | 工程投入 | 5＝数月/高风险；4＝数周；3＝1–2 周；2＝几天；1＝小时级 |
| **V** 验证难度 | 能否证明"真的做好了" | 5＝需干净机器/多人/长期；3＝需真机交互且含主观判断；2＝MCP/脚本可确定性断言；1＝单测即可 |

**优先级分**：`P = (W + D) / (C + V)`；**分档**：P0 = `P ≥ 1.3 且 W ≥ 4`；P1 = `P ≥ 0.9`；P2 = `P ≥ 0.6`；`P < 0.6` 或定位冲突/已证伪 = 不做。

**快速通道规则**：**成本 ≤2 且独立可交付**的小项随时可插队（不占批次位次）。当前通道：**㉜**（非 `.tex` 输入不触发编译，C1，G1 研究顺带发现）。

## 3. 待办总表（唯一排序来源；档内按 P 降序）

| ID | 事项 | W | D | C | V | P | 档 | 依据与下一步 |
|---|---|---|---|---|---|---|---|---|
| ㉜ | **非 `.tex` 输入变化不触发编译**（改了 `.bib`/图片没反应） | 3 | 2 | 1 | 1 | **2.50** | P1 | G1 研究实测（[报告](./g1-read-interception-feasibility.md) §5.1）：`watch.rs:178` 只认 `.tex` 扩展名；改**被 `\bibliography` 引用**的 `refs.bib` → 全部 tmp 产物 mtime 纹丝不动。修法 C1：把非 `.tex` 输入纳入触发集合（最省：项目内非忽略文件变化即触发，交给 latexmk 自己判定） |
| ⑧ | 跨文件 LaTeX 语义操作 | 4 | 5 | 4 | 3 | **1.29** | P1 | P10：VS Code 明确缺失（§6.3） |
| ⑩ | 深色主题（Candy Desk 暗色） | 3 | 2 | 2 | 2 | **1.25** | P1 | 配置类第一痛点：206 票 / 23 万浏览（§6.4） |
| ㉖ | **模板自带 latexmkrc 与构建约定的交互**（硬证据已就位） | 3 | 4 | 3 | 3 | **1.17** | P1 | hithesis：rc 覆写 `$pdflatex` + `--shell-escape` + 尾部 `;cp`，在 `-outdir=tmp` 下 `\include{body/...}` 写不出 aux、报错后 latexmk 挂住；精简 rc 最小复现仍能恢复 → 触发条件待定位（§6.1） |
| ⑪ | 预览滚动/缩放保持、只重排变更页 | 4 | 3 | 4 | 2 | **1.17** | P1 | 官方承认"丢失滚动位置"；判定输入**已就位**（页哈希差分，见 §6.4 与 [增量编辑 × DVI 专项](./incremental-edit-x-dvi.md)）；建议先做零风险的"全同则不刷新"（C1） |
| ⑨ | LSP（texlab）集成 | 4 | 3 | 4 | 3 | **1.00** | P1 | P8：补全/引用是长期痛点；建议与 ⑧ 合并（§6.3） |
| ⑫ | ~~TinyTeX 捆绑~~ → **Tectonic 库形态集成**（2026-09 改判；"零预装可用"已由子进程形态兑现） | 5 | 4 | 5 | 4 | **1.00** | P2\* | 分数沿用"环境引导"口径，待库形态准入判据出数后重评；建议独立里程碑（§6.5） |
| ⑬ | AI 集成 | 3 | 4 | 4 | 3 | **1.00** | P2 | 2026 竞争主轴；**仍无一手需求证据** |
| ⑭ | 拼写检查 | 3 | 2 | 3 | 2 | **1.00** | P2 | P8：拼写/语法弱 |
| ㉙ | 未存盘 overlay（编辑期不写用户文件） | 2 | 3 | 3 | 2 | **1.00** | P2 | 吸收自上游 `edit_data` overlay；与 **ADR-0007「FS 为真相源」冲突**，且无实测痛点支撑 |
| ⑮ | 多窗口与多项目 | 3 | 2 | 4 | 3 | **0.71** | P2 | design.md 后置项 |
| ㉝ | 精确失效：用依赖集合过滤 watch 事件 | 2 | 1 | 2 | 2 | **0.75** | P2 | 反向的浪费：改**未被引用**的 `.tex` 会白编译一次（实测 `main.aux` 被重写；一次白编译 = 小档 1s / 真实论文 >120s）。判据现成：`.fdb_latexmk` 源依赖集合 + `.fls`（`scripts/fls-report.mjs` 已解析），四条保守原则见 [G1 报告](./g1-read-interception-feasibility.md) §6.2 |
| ⑯ | 外部 PDF 查看器 | 2 | 2 | 3 | 3 | **0.67** | P2 | design.md 后置项 |
| ⑰ | 自动更新 | 2 | 2 | 3 | 3 | **0.67** | P2 | design.md 后置项 |
| ⑱ | 代码签名证书 | 2 | 2 | 2 | 4 | **0.67** | P2 | 分发信任，但外部依赖重 |

\* ⑦c 的 P0 身份来自"**先测量**"这一步（C1）；⑫ 的 `*` 指它不适合塞进批次，应按独立里程碑推进。⑦b（折叠提供者增量）**不在表内**：实测 2.2–3.7ms/次、未越 60Hz 帧预算，等 ⑦c 的数字再定是否排期（§6.2）。

## 4. 批次与出口条件

| 批次 | 内容 | 出口条件 | 状态 |
|---|---|---|---|
| **Batch 1 收口** | 快速通道 ㉓㉚㉗㉑㉛ | 小项各自独立可交付 | ✅ 2026-09 达成 |
| **Batch 2 攻核心承诺** | ⑥ CLI + MCP | Agent 能驱动「改 → 编 → 验」闭环 | ✅ 2026-09 达成（真机走通 + 回归固化） |
| **Batch 3 硬骨头** | ⑦a ✅ → ⑦c ✅ → **⑧⑨ 语义层（合并）** → ⑩⑪㉖ | 交互期无 >50ms 长任务（⑦c ✅：三档夹具五项门槛全过）；跨文件重命名可用（⑧⑨）；主题一致（⑩）；带 rc 的模板编译路径有结论（㉖） | 🟡 进行中（⑦a / ⑦c 已完成） |
| **独立里程碑** | ⑫ **Tectonic 库形态集成**（原 TinyTeX 捆绑，2026-09 改判） | 零预装可用**已由子进程形态兑现**（`Engine::Tectonic`）；本里程碑的出口 = 库内嵌三项准入判据出数并据此决定是否投入（缓冲直喂 / 页事件实时性 / 内存 XDV 前缀可转换） | ⬜ 未开始（形态 B 已落，提交 `ef89165`） |

## 5. 外部方案与产物格式评估（否决与吸收都在这里）

### 5.1 上游 TeXpresso 路线要求什么前提

评估对象：[上游实时渲染方案](../texpresso-live-rendering-roadmap.md)（把 XeTeX 改造成可 fork 的实时预览器）。该文档 §1 自列四条**必须同时成立**的前提：

| # | 前提 | 它怎么满足 |
|---|---|---|
| G1 | 能**拦截目标程序的每一次 I/O**，且拿得到字节偏移 | 自己改造 XeTeX（`src/engine/`，`texpresso_protocol.c` 的 `Q_READ`/`A_READ`） |
| G2 | 输出格式支持**字节偏移重同步** | 解析 **DVI**，靠 BOP/EOP 做页索引（`incdvi.c`） |
| G3 | 目标程序能接受**部分输入状态**继续 | 进程内引擎 + VFS 三层内容模型（`edit_data` overlay） |
| G4 | `fork()` 快照：目标程序单线程、确定性、无 GPU/驱动状态 | 它就是被 fork 的那个进程 |

### 5.2 我们满足几条（2026-09 复核后的现状）

| 前提 | 现状 | 判定 |
|---|---|---|
| G1 拦截 I/O | 跑的是 **stock latexmk + xelatex**（外部进程），不拦截。**2026-09 深挖**：其**信息内容**（读了什么）已免费拥有——`.fls`（latexmk 默认开 `-recorder`，引擎实际打开的文件，编译中可尾随）+ `.fdb_latexmk`（依赖图 + mtime/size/**md5** + bibtex 步骤），两者互补、零新增依赖；**字节偏移**那半既拿不到（需改引擎 / 文件系统驱动 / API hook），也**没有消费方**（上游唯一用途 seen 水位依赖 G4 的进程快照，Windows 无 `fork()`）。见 [G1 报告](./g1-read-interception-feasibility.md) | ➖ 不需要（"读了什么"已现成，可兑现为精确失效/缺失诊断） |
| G2 字节偏移重同步 | **成立**：产物是 **XDV**（`tmp/<stem>.xdv` 每次编译都在），页包自包含、指令长度可算；自研页索引实测：截断到任意位置解出的页与完整文件**逐字节相同**、4.41MB/186 页解析 **2.7ms**。见 [G2 报告](./g2-byte-offset-resync.md) | ✅ 成立（索引） |
| G3 部分输入 | latexmk 只认文件系统；我们的模型是"自动保存 → 监视 → 编译"；**不需要**（只读已产出的页） | ➖ 不需要 |
| G4 fork 快照 | **Windows 无 `fork()`**（该文档 §1.3 自述）；且**不需要**——但 2026-09 专项把这条收紧成两句：① **不可替代**（`PssCaptureSnapshot` 只读、CRIU 仅 Linux、Cygwin fork 要 Cygwin 目标引擎，唯一真等价是 WSL）；② **收益也没那么大**：实测编辑期单趟里导言区占 **71–89.5%**，fork 只能省"重排"那一小半。见 [无 fork 报告](./no-fork-alternatives.md) | ➖ 不需要（收益结构也不支持投入） |

**但整体结论仍是停手**：G2 只给"**知道第几页在哪、哪页变了**"，不给"**把页画出来**"——渲染那一半实测比 PDF 链路慢 **57–68×**（§5.6）。上游值得做是因为它连"每次按键重排版"都省掉了（fork + overlay）；我们只换显示格式，省不掉重排版。（顺带记录：该文档 §1.1 的"量化收益门"我们是**满足**的——③ 实测全量重编译 1.6–13.5s，落在它所谓"值得做到阶段 6/7"的区间；判定停手的原因不是收益不够，而是前提与代价。）

### 5.3 我们实际处在它的第几阶段

| 它的阶段 | 我们的状态 |
|---|---|
| 阶段 1 端到端骨架 | ✅ 已完成（改文件 → 编译 → 解析 → 显示） |
| 阶段 2 子进程 + 流式输出 + 时间片 | 🟢 **唯一真缺的那件已落地**：子进程 ✅ / 时间片 ➖ 不需要 / **流式输出 ✅（2026-09）**：管道已接（`Stdio::piped()`）+ 尾随 `tmp/<stem>.log`（引擎输出在非 TTY 下是 **4KB 块缓冲**，按页 flush 的日志才是主力通道）；状态栏「已排版 N 页」+ 错误列表**编译还没结束**就见致命错误。真机实测 400KB / 162 页 ctexbook 首编：`1.7s running → 3.4–4.8s 页数 2→162 → 8.2s success`；插错后 `41.5s` 收到实时错误、`42.3s` 出终态。契约见 [modules.md](../modules.md) §2.4 / §2.6.1。**未做**：部分 PDF 只到可行性（`xdvipdfmx` 接受合成 postamble，0.65–0.94s/次，未接线）。时间片结构性不需要：32s 编译期间打字 4.42ms/键、零 >50ms longtask |
| 阶段 3 增量输出解析 | 🟡 **技术要件已达成、但无消费方**（2026-09 深挖，[报告](./stage3-incremental-output-parsing.md)）：追加式页索引 + 回滚已实现并验证（4 种喂入序列对拍一致；141 轮实测**增量累计恒定在 6.6–8.1ms**、每轮全量线性涨到 260ms → 上游"每轮耗时常数级"的验收标准**达成**）；但编译期页进度已由阶段 2 的 `[N]` 标记满足、编译后索引一次全量只要 5.4ms、⑪ 未开工 → **原型入库不接线**。副产品：修复了全量解析器两个静默错误（截断前缀会解出内容被截断的假完整页），并把"半成品可读"从 6 个采样点加强到 **1122 个截断点 / 71993 页 / 0 处不一致**；页哈希差分与 SyncTeX 已完成对拍 |
| 阶段 4 脏矩形显示 | 🟡 已被粗粒度等价物覆盖：分页 DOM 虚拟化 + 同文件 canvas 复用（[modules.md](../modules.md) §9.4）；PDF 路线下无法矩形级增量 |
| 阶段 5 VFS + 事务回滚 | ❌ 与 **ADR-0007** 冲突（overlay 会引入第二真相源），且需引擎配合 |
| 阶段 6 seen 水位 + trace | ❌ **执行层不可得**：机制要"引擎进程活着 + 状态可回退"（G4 fork 快照，Windows 无 `fork()`）；信息层（读了什么）已由 `.fls` / `.fdb_latexmk` 免费提供，但只够**依赖级**用途（精确失效），兑不出"打字不重算前半篇"。见 [G1 报告](./g1-read-interception-feasibility.md) |
| 阶段 7 fork 快照 + fence | ❌ **不可达，但结论已收紧**（2026-09 专项，[报告](./no-fork-alternatives.md)）：Windows 上"进程快照 + 恢复执行"**没有等价机制**（`PssCaptureSnapshot` 只读、CRIU 是 Linux 专属、Cygwin fork 要求 Cygwin 目标引擎），唯一真等价是 **WSL**（本机不可用：服务拒绝访问）。**但实测显示 fork 能省的只占小头**：编辑期单趟的成本里**导言区占 71%（28 页）～89.5%（74 页）**——重排只占 10–29%，fork 一分钱也省不到固定开销。⇒ 该啃的是"砍固定开销"（导言区 fmt 化等），**不需要 fork** |
| 阶段 8 多趟收敛 | 🟡 思想已吸收（㉘：编辑期轻、停手后跑全） |

### 5.4 结论与吸收清单

**结论：不采用上游路线**（它要求"拥有目标程序的内部"）。**吸收的是与引擎无关的思想与便宜的那一半**：

| 吸收项 | 来源 | 变成 |
|---|---|---|
| 空闲收敛（编辑期跑轻的、停手后跑全的） | 阶段 8 | **㉘**（已落地：Quick 单趟 + 2s 空闲收敛） |
| 把决策过程打出来 | 该文档 §5 | **㉛**（已落地） |
| 确定性验证（连跑两次逐字节比） | 该文档 §1.2 | **㉚**（已落地；也是"输出 diff 类优化"的前提） |
| 未存盘 overlay | 阶段 5a `edit_data` | **㉙**（P2；与 ADR-0007 冲突，无实测痛点） |
| **编译期页完成度进度** | 阶段 3 的页索引（去掉渲染） | §5.7 / §10-B.8：读 `tmp/*.xdv` 出「已排版 N 页」，C1，未接线——**需求已由阶段 2 的 `[N]` 标记落地**（§1「阶段 2」行），XDV 版只剩页级差分/增量解析的价值 |
| **页级变更判定** | 同上（页哈希差分） | ⑪ 的候选判定输入（§6.4）：保证不漏，但重排时会覆盖大量页 |
| **引擎 stdout 管道 + 部分 PDF** | 阶段 2（子进程 + 流式输出） | §5.8：`Stdio::null()` → `piped()` 出**编译期进度 + 实时错误**（✅ **已落地 2026-09**，见 [modules.md](../modules.md) §2.6.1）；页前缀 + 合成 postamble 出**部分 PDF**（仅可行性，长文档再评估） |

**明确不做**：fork 快照、seen 水位/trace、VFS 事务回滚（§7）、**DVI/XDV 作为预览格式**（§5.6）。

### 5.5 「那我们自己改造 XeLaTeX 行不行」——评估结论

**法律上可以**：XeTeX 按 X11（MIT 类）许可分发，fork 与修改无许可障碍；按 TeX 惯例改过的引擎要**改名**。

**但改造拿不到最值钱的那一项**：

| 目标机制 | 改造引擎后是否可达 | 说明 |
|---|---|---|
| 阶段 7 fork 快照 | ❌ 仍不可达（2026-09 收紧：Windows 无等价机制，唯一真等价是 WSL；**且实测显示它只能省小头**——编辑期单趟里导言区占 71–89.5%）。见 [无 fork 报告](./no-fork-alternatives.md) | Windows 无 `fork()`；显式序列化状态"成本高一个数量级" |
| 阶段 6 seen 水位 | ✅ 可达（改造的最大价值） | 需拦截每一次读 + 字节偏移，正需 C 层改造 |
| 阶段 3 增量 DVI 解析 | ⚠️ 可达但代价大 | 等于重建预览栈（现为 PDF + pdf.js） |
| 阶段 5 VFS/overlay | ✅ 不需要改 C | 见下 |

**代价三层**：从源码构建并长期跟进 TeX Live（Windows MSVC 需 ICU/HarfBuzz/FreeType/Graphite2/poppler 整套）；**分发性质改变**（不再"用你已有的 TeX Live"，与 ADR-0003 不签名分发叠加放大体积与信任成本）；包与字体更新要自己跟。

**两条不改造引擎的替代路径**：① **LuaLaTeX + Lua 回调**（`find_read_file`/`open_read_file` 是官方扩展点）→ ㉙ 用纯 Lua 即可，不必碰 C；② **Tectonic**（Rust、可嵌入；上游实际基于 fork 的 Tectonic）→ 若真要"引擎在自己手里"，从这里入手远比改 TeX Live 的 C 版 xetex 现实。

**建议**：**"拥有引擎"只应作为一次显式战略决策（另立 ADR），不作为功能项顺手带进来**；真要走，走 Tectonic。

### 5.6 产物格式：DVI/XDV 代替 PDF？—— 否决

实测（121 页文档，同机同夹具）：

| 链路 | 成本 |
|---|---|
| `xdvipdfmx` 全量 → PDF（152KB） | **0.65s**（≈5ms/页） |
| 之后 pdf.js 首屏（46 页/78KB，真机） | 115ms |
| `dvisvgm` 121 页 → SVG | **44.06s**（≈364ms/页）；单页独立进程 0.77–1.03s |
| XDV 体积 | **不定**：0.15×（CJK 短文）～**29.4×**（121 页纯文本：4.46MB vs 152KB） |

⇒ 从产物到屏幕**慢 57–68×**；XDV 还是"逐字形未压缩流"，且 specials 是 hyperref/xcolor 针对 xdvipdfmx 发的 **`pdf:` 方言**、字体靠文件路径引用（含 Windows `.ttc`）。**不采用**。数据与三个选项的成本对照见 [dvi-preview-feasibility.md](./dvi-preview-feasibility.md)。

### 5.7 G2「字节偏移重同步」：索引吸收、渲染不做

- **成立且便宜**（自研页索引，只算指令长度）：页数与 `pdfinfo` 逐档一致（28/28、8/8、121/121、186/186）；**截断到 10/33/50/75/90/99.9% 解出的完整页与完整文件逐页哈希相同**；全量 4.41MB = **2.7ms**，按落盘粒度增量 ≈0.3ms/次。
- **2026-09 加强**（[阶段 3 报告](./stage3-incremental-output-parsing.md)）：**追加式页索引**已实现并验证（4 种喂入序列与全量逐字段对拍一致；141 轮实测增量累计**恒定** 6.6–8.1ms、每轮全量线性涨到 260ms → 上游阶段 3 的"每轮耗时常数级"达成）；**截断可读性从 6 个采样点加强到 1122 个 4KB 网格点 / 71993 页比对 / 0 处不一致**。期间发现并修复全量解析器**两个静默错误**（截断点落在指令中间或恰在页尾时，会把内容被截断的假完整页推进页索引）。**结论：原型入库、不接线**（编译期页进度已由 `[N]` 标记满足）。
- **编译期可用性**：186 页编译 1937ms → **页 1 @896ms（46%）**，之后每 ~110ms 一批 ~20 页（flush 粒度 0.5MB）。
- **页号映射**：DVI 页号 = PDF 页号（1:1 同序），SyncTeX 交叉验证 `c05:1 → PDF 79` = 页哈希差分命中的那页。**2026-09 又补两个样本**（[阶段 3 报告](./stage3-incremental-output-parsing.md) §7）：等长替换 → 差分命中 1 页 = 19、SyncTeX = 19；文首插入 → 差分 120/125 页（首个命中 = 5）、SyncTeX = 5 → **页哈希差分与 SyncTeX 自洽**（⑪ 用它做判定输入不会与跳转页号打架）。
- **页级差分**：等长替换 → 1 页变；插入一段 → 其后 **108** 页变；只加注释 → 2 页变（TOC 收敛，字节确实变了）。**精确但受重排支配**。
- ⚠️ 上一版文档里"DVI 无法解析半成品"的说法**已被修正**：那是 `dvisvgm` 要 postamble 的选择，不是格式限制。

⇒ **吸收两条**（编译期页进度、页级变更判定），**不吸收渲染**。工具：`scripts/xdv-report.mjs`；数据见 [g2-byte-offset-resync.md](./g2-byte-offset-resync.md)。

**2026-09「增量编辑 × DVI/XDV」五点判定**（[专项报告](./incremental-edit-x-dvi.md)）：在"每次仍整份重编译"的前提下，页级信息能让**下游**少做重复工作——① **页哈希差分 → 复用旧 PDF / 跳过预览重载 / 只重绘变化页**：✅ 成立且值得做（实测只加一行注释 → **125 页逐页字节完全一致、0 页变化**；建议顺序 B 跳过重载 C1 → A 跳过 `xdvipdfmx` 0.65–0.94s C2 → C 只重绘变化页 = ⑪）；② **局部重编译（`\includeonly`）+ 页级拼接**：❌ 实测证伪（74 → 33 页、第 3 章显示成"第一章"、页码 5 起、目录重写）；③ XDV 渲染仍不做；④⑤ 无消费方。

### 5.8 阶段 2（引擎子进程 + 流式输出 + 时间片）—— 2026-09 专项（**流式输出 ✅ 已落地**）

完整实测见 **[stage2-streaming-feasibility.md](./stage2-streaming-feasibility.md)**。要点：

- **子进程 ✅ 早有**（`infra::runner`，含超时树杀）；**唯一缺口是输出通道**：`runner.rs` 的 `Stdio::null()` 把引擎 stdout/stderr 全丢了 → 编译期既没有进度也没有实时错误。**2026-09 已改为 `piped()`**。
- **时间片 ➖ 不需要**：引擎是独立 OS 进程。实测 32s Full 编译期间打字 **4.42ms/键、p95 7.2ms、零 >50ms longtask**（空闲对照 4.33ms / p95 5.4ms）。
- **流式输出 ✅ 已落地**（落地记录见 [modules.md](../modules.md) §2.4 / §2.6.1）：三条读任务（stdout/stderr/`tmp/<stem>.log` 尾随）→ 同一个状态机 → `compile-progress` / `compile-errors` 两个**中间态**事件 → 状态栏「已排版 N 页」+ 错误列表实时致命错误。
  - **落地时才发现的两条新事实**：① 引擎输出**非 TTY 下是 4KB 块缓冲**，短文档要等进程结束才 flush → 实时通道必须以"按页 flush 的 `.log`"为主力（只接管道时，1471ms 的编译里首条错误出现在 1394ms，等于没有提前量）；② 中间态**可能晚于终态抵达**（runner 收尾仍会补发最后一批），故中间态必须走**独立事件** + 前端 `phase` 守卫——最初复用 `errors-updated` 时真机观察到"超时诊断 1 条被 30 条中间态覆盖"。
  - **实时错误只报致命错误**：`Overfull`/`Underfull`/`LaTeX Warning` 留到终态（实测一次失败的长文档日志 30 条里 27 条是 `Overfull`，会淹没真错误）；终态行为不变（仍含警告）。
  - **顺带定位一条既有缺陷**：错误条目的文件归属会错一章（TeX 日志把 `)` 与下一个 `(` 打在同一行，文件栈失配）——见 [troubleshooting.md](../troubleshooting.md) 与新记的 [modules.md](../modules.md) §12.1 #22（未修）。
- **"部分 PDF" 做通了**（这是验收里"输出一产生就显示"的唯一现实路径）：页前缀 + **从零合成 postamble** → `xdvipdfmx` 接受 → 页数 = 前缀页数（12/30/60/90/121 全对）、首页渲染与完整文件一致（仅字体编号不同）。成本：构建 0–4ms + 转换 **0.65–0.94s/次**（与页数几乎无关）。坑：字体定义必须同时收 `define_native_font`(252) **和** 经典 `fnt_def`(243–246)，`post_post` 回指指针必须改。**未接线**。
- **验收对照**：`UI 不卡` ✅ 已满足；`10 秒文档首屏数百毫秒` ❌ **达不到**（实测 ≈1–3s）——差的那部分正是"拥有渲染栈"（渲染那半已否决）。

## 6. 待办详述

### 6.1 ㉖ 模板自带 latexmkrc 与构建约定的交互（P1，硬证据已就位）

- **硬证据**：`thesis-real-hithesis`（TeX Live 自带样例）用产品完全相同的命令冷编译，~4 分钟后报 `! I can't write on file 'body/introduction.aux'` → `Emergency stop`，随后 `latexmk`/`perl` **挂住不退出**（实测 10 分钟零 CPU）——产品侧因此看到的是「超时」而不是「内容错误」。
- **已知机制**：`\include{子目录/文件}` 要写 `tmp/子目录/文件.aux`，xelatex **不创建输出目录的子目录**；latexmk 一般会在第一趟失败后建目录并重跑（合成 `multifile` 档无事），hithesis 这一档没有恢复。
- **嫌疑**：该模板自带 latexmkrc（覆写 `$pdflatex`、内建 `--shell-escape`、尾部 `;cp`）与 `-outdir=tmp` 的相互作用。
- **尚未定位**：用精简 rc 复刻关键行跑最小工程仍能恢复 → **没有可复现的 rc 最小组合**；最小复现与绕过（`\include` → `\input`）见 [troubleshooting.md](../troubleshooting.md)。
- **出口条件**：带 latexmkrc 的真实模板能编译成功，**或**产品给出明确可操作的诊断；③ 的 ">240s 未收敛" 记录在修好后用 `node scripts/bench.mjs --with-real --no-warmup` 重测。

### 6.2 ⑦c 多文件大项目 —— ✅ 已完成（夹具 + 口径 + 数据，结论：**不需要优化**）

背景：⑦（大文档编辑器侧性能）经真机实测拆成四份，只有 ⑦c 还是未知数——[p1 分析](./p1-large-doc-editor-analysis.md) 已证伪"竞品的架构病我们有"（每击键净开销 0.02–0.05ms 且不随规模增长），并把大纲重扫做成增量（⑦a ✅）。

| 拆项 | 实测 | 状态 |
|---|---|---|
| ⑦a 大纲重扫增量 | 55.5ms → 9.7ms / 8.4ms（11 文件/436KB） | ✅ 已完成（§1） |
| **⑦c 多文件大项目** | 三档夹具（9/21/41 文件 × 2.2–5.5MB）：每击键净 **0.077–0.097ms 绝对值恒定**、打开全部标签 252/507–536/675ms、大纲往返 14.1/28/27.9ms、无 longtask、关标签后无泄漏 | ✅ **已完成**（§1；[p1c 报告](./p1c-multifile-large-project.md)） |
| ⑦b 折叠提供者增量 | 2.2ms@462KB → 3.7ms@1.36MB；⑦c 复测 0.2–0.3ms@282KB | 缓做：未越 60Hz 帧预算（多文件也不改变该判断） |
| ⑦d 每击键省一次 `getValue` | 收益 ~0.05ms | **明确不做**（分数好看、用户无感） |

**DoD 已固化**为 `scripts/editor-report.mjs`（WS 直驱真机、零第三方依赖、超门槛退出码 1）+ `scripts/gen-large-project.mjs`（三档夹具）。门槛与结果：`每击键净开销 ≤0.5ms`（实测 0.077–0.097）、`折叠 ≤2ms/MB`（0.73–1.45）、`大纲往返 ≤20ms/MB`（5.07–6.41）、`无 >50ms longtask`（0 条）、`20 个 1MB model ≤100ms`（22–46）——**五项全过**。两条留给既有条目的观察：冷路径大纲全量 57–182ms 归入 ③ 的"打开耗时"口径；`>20 文件时编译期 watch 事件`未测（属 ③/流式输出的账）。

**方法论教训（回写，避免重犯）**：① A/B 必须**顺序受控 + 预热丢弃**（首轮差 0.47ms，交替 3 轮后 0.02–0.05ms，单轮会高估 5–10 倍；⑦c 用 5 轮弃首轮）；② 不能用 `slice()` 代理 `getValue()`（V8 SlicedString 是 O(1) 视图，实测 0.000ms）；③ MCP 的 `webview_keyboard type` **打不进 Monaco 0.56**（EditContext + `readonly` 的 IME textarea）——用"动态 import 已加载模块 + `editor.trigger('keyboard','type')`"；④ ⑦c 新增七条口径坑（Vite 预打包 URL、活动文件必须是**大文件**、`.length` ≠ 字节数、堆读数不可信、`editor.dispose()` 不 dispose model、`String.raw` 模板不能含反引号、门槛该用**绝对毫秒**而不是 ms/MB）全部记在 [p1c 报告](./p1c-multifile-large-project.md) §6。

### 6.3 ⑧⑨ 语义层（建议合并为一个里程碑）

- **⑧ 跨文件语义操作**（重命名 `\label`/宏/环境、跳转定义、引用补全）：P10 证据显示 VS Code 明确缺失，③ 的夹具与 ⑥ 的 CLI 已能用来自动化验证。
- **⑨ LSP（texlab）集成**：先做 monaco-languageclient 技术验证（v1.1 规划，需专项研究）；若走 LSP，⑧ 的一部分能力可以直接来自 texlab，避免自研索引。
- **共用底座**：⑦a 落地的 `core::outline::OutlineCache` 就是"include 图 + 每文件扫描结果"的雏形——语义层应建在同一份索引上（一份投入两处收益）。

### 6.4 ⑩ 深色主题 / ⑪ 预览状态保持

- **⑩**：Monaco + 自研 Monarch 语法 + pdf.js 反色需要一起调（Candy Desk 语系的暗色版）；配置类第一痛点（206 票 / 23 万浏览）。
- **⑪**：pdf.js 无增量更新文档的能力，现方案是"重载 + 恢复滚动/缩放"（design.md §预览）。**判定输入已就位**（2026-09）：§5.7 的**页哈希差分**（XDV 页字节哈希）已与 SyncTeX 对拍一致，且实测"不影响排版的编辑 → **0 页变化**"——于是可以先做零风险的"页哈希全同 → 不刷新预览"（C1），再把"变化页集合"接进 `PreviewPane` 做"只重绘变化页"（C2–3）。它能保证不漏，但 LaTeX 重排时会命中大量页（插入一段 → 其后 108 页变），故 DoD 按"**视觉无跳动**"而不是"只重绘一页"来定。**建议**：先量化真实编辑的命中率分布再投入；夹具与口径已由 ⑦c 备好。见 [增量编辑 × DVI 专项](./incremental-edit-x-dvi.md)。

### 6.5 ⑫ Tectonic 库形态集成（独立里程碑；原「TinyTeX 捆绑」2026-09 改判）

**为什么改判**：原计划靠"捆绑 TinyTeX"解"干净 Windows 机器零预装可用"。现在有更省的路——**Tectonic** 本身就是"自带 bundle 的现代引擎"，我们能以两种形态接它，而**第一种已经落地**：

- **形态 B（子进程，已落，提交 `ef89165`）**：`Engine::Tectonic` 已进产品，设置里可选。同批实测（28 页中文夹具）**单趟出 PDF 0.88–1.07 s**（对照现状 xelatex 两段 ≈1.96 s），且**免装 TeX Live 已经成立** ⇒ "零预装可用"这个核心价值**不再需要捆绑 TinyTeX**。
- **形态 A（库内嵌，本里程碑）**：把 `tectonic` crate 作为库嵌进产品。相对子进程**额外**拿到：
  1. **`IoProvider` 接管 IO** ⇒ 编辑器**未保存的缓冲直接喂引擎**（无保存、无 watcher、无去抖）；
  2. **常驻 format 与进程** ⇒ 省掉每次运行地板（子进程形态实测地板 283 ms）里的一部分；
  3. **`XdvEvents` 事件**：`handle_begin_page`（页事件在排版**进行中**到达）+ `handle_text_and_glyphs(font, text, width, glyphs, **x[]**, **y[]**)`（**逐字形 + 绝对坐标**）——这是"按字实时"在**观测面**上的最后一块拼图；
  4. 三个 pass（`TexEngine` / `BibtexEngine` / `XdvipdfmxEngine`）可**自己编排**；产物（XDV/`.synctex.gz`）全程在内存；
  5. **结构化状态与错误**（`StatusBackend` / `Result`），不再解析 `.log`（G6 那类"引擎自述≠事实"的坑在库内基本消失）。

**代价（先算清再动手）**：拖进 **C 依赖**（harfbuzz 需子模块源码；Windows 官方路线是 vcpkg + 他们的 `cargo-vcpkg` 分支）⇒ CI 明显变重；要重划 **ADR-0010** 的边界（C 链接进哪一层）；引擎与宿主**同进程** ⇒ 崩溃/取消语义要重设计（隔离从"杀子进程"变成"自己不能崩"）。官方 MSVC 二进制**只能**当子进程用。

**准入判据（不满足就不开工）**：① 实测"format/进程常驻后每次击键的 pass 成本"确实显著低于子进程形态；② 引擎是否**按页 flush** 到我们拿得到的 XDV 句柄（决定页事件是"真实时"还是"收尾一次性"——与 U-8/E5.5 同源，至今 `[推断]`）；③ 内存里的 XDV 前缀能否直接喂 `XdvipdfmxEngine::process`（阶段 2 在外部 `xdvipdfmx` 上已验证"前缀 + 合成 postamble"可行）。②③ 任一为否，库形态的实时价值就要打折——但**只取第 1 条（缓冲直喂）也仍然值钱**。

**与"实时预览"的关系**：200 ms 交互**已由前端草案层独立解决**（提交 `564f39d`，实测改动→草案可见 31–34 ms），**不依赖本里程碑**；库形态买到的是"草案层更准（真字形坐标，而不是从 PDF 文本层反推）+ 纠偏更快（去 I/O、去进程启动）"。

判据与摩擦点见 [tectonic-test-plan.md](./tectonic-test-plan.md)、[tectonic-integration-plan.md](./tectonic-integration-plan.md) §2；成本实测见 [realtime-preview-cost.md](./realtime-preview-cost.md) §6。属"环境引导 + 实时能力"性质，**按独立里程碑推进（必要时另立 ADR），不塞进 Batch 3**。

### 6.6 P2 其余（一行带过）

⑬ AI 集成（无一手需求证据，先观察）｜⑭ 拼写检查｜㉙ 未存盘 overlay（与 ADR-0007 冲突）｜⑮ 多窗口多项目｜⑯ 外部查看器｜⑰ 自动更新｜⑱ 代码签名证书。

## 7. 明确不做（否决项）

| 项 | 理由 |
|---|---|
| **TeXpresso 式 fork 快照 / seen 水位 / VFS 事务回滚**（其阶段 7/6/5） | 需 G4（Windows 无 fork）与 G1（拦截 I/O）；VFS 与 **ADR-0007** 冲突。见 §5.2/§5.4 |
| **DVI/XDV 作为预览格式** | 产物→屏幕慢 57–68×；XDV 体积不定；specials 是 `pdf:` 方言。见 §5.6 |
| **自研 DVI/XDV 渲染栈** | 需字形级渲染 + 系统字体（含 `.ttc`）+ specials；上游用 MuPDF（**AGPL**，与本项目 MIT 冲突）。索引那一半已吸收（§5.7） |
| **自建/改造 TeX 引擎**（含 fork TeX Live 的 xetex） | 许可允许但代价三层，且最值钱的 fork 快照在 Windows 上仍不可达；真要走应走 Tectonic 并另立 ADR。见 §5.5 |
| **实时协作** | P=0.40；在线方案护城河，本地方案投入产出不成立。定位「个人 / 离线优先」，协作交给 Git |
| **增量编译** | [ADR-0005](../adr/0005-latexmk-first-incremental-next.md) 已实测证伪：latexmk 增量＝整份单遍重排，属 xelatex 引擎上限 |
| **②-2 引擎自动推断** | ⑲ 实测：**0/19** 模板因默认 XeLaTeX 选错、`\RequirePDFTeX` 0/7374；改用 ④ 的「选错时明确告诉用户怎么改」替代 |
| **推断 bib 工具链** | 实测 latexmk 自己就会跑 `bibtex`/`biber`，无需推断也无需用户配置 |
| **自研 PDF 渲染引擎** | pdf.js 已是够用底座；LaTeX Workshop 的教训是"预览是二等公民"，不是"必须自研渲染器" |

## 8. 决策原则（后续取舍按此）

1. **不拥有目标程序内部**：我们驱动 stock TeX Live。任何要求"改造引擎 / 拦截其 I/O / fork 其进程"的方案先按 §5.1 四条前提过筛；要推翻须是一次显式战略决策并另立 ADR。
2. **先量化收益再上机制**：③、⑦（p1 分析）、DVI/G2 三份报告都证明"先测再投入"能省掉整块工作；新增优化拿不出实测改善就不进批次。
3. **证据不足就压后，不硬排**：②-2 因 ⑲ 出数被砍、㉙ 因与 ADR-0007 冲突且无痛点降 P2、⑦b 因未越帧预算缓做——**"没有证据"本身就是不做的理由**。
4. **文档=事实来源**：每项完成后回写 design/modules/troubleshooting，且只记实测结论；**被证伪的也回写**（本项目已有三例：`getValue` 代理、`$` 全量重词法、DVI 半成品不可读）。
5. **阈值与成本打分标 `[推断]`**：可被推翻，但必须留下可溯源的依据。

## 9. 与现有文档的对应

状态以本文件 §1（已完成）/ §3–§4（待办与批次）为准；下表只回答"实现与证据在哪读"。

| 本文件项 | 实现 / 证据位置 |
|---|---|
| ① 中文路径 | [troubleshooting.md](../troubleshooting.md)「中文文件名/路径兼容性实测」 |
| ②-1 根文件候选 | [modules.md](../modules.md) §8 命令表 / §9.2 stores / §9.4 组件表 / §10 契约 |
| ②-2 引擎推断（否决） | [design.md](../design.md) §后置/未决清单「已否决」 |
| ③ 性能基准 | design.md §基准脚本与实测基线 + `scripts/{gen-bench-projects,bench}.mjs` |
| ④ 错误诊断 | modules.md §4.1 + design.md §错误列表 + `log_parser/diagnosis.rs` / `real_error_corpus.rs` |
| ⑤ SyncTeX | [ADR-0008](../adr/0008-synctex-via-cli-with-interface.md) + modules.md §5 + design.md §预览 + `scripts/synctex-report.mjs` |
| ⑥ CLI + MCP（**已完成**） | [cli-mcp-plan.md](../cli-mcp-plan.md) + modules.md §8.1 + `crates/latteset-server` |
| ⑦a 大纲增量（**已完成**） | modules.md §3.5（缓存三不变量）/ §12.2 + `core::outline::{load_cached, OutlineCache}` + `src/stores/outline.ts` |
| ⑦c / ⑦b（**已完成 / 缓做**） | [p1-large-doc-editor-analysis.md](./p1-large-doc-editor-analysis.md)（拆分依据）+ [p1c-multifile-large-project.md](./p1c-multifile-large-project.md)（夹具/口径/数据）+ `scripts/{gen-large-project,editor-report}.mjs` |
| ㉖ latexmkrc 交互 | 本文件 §6.1 + troubleshooting.md「`\include{子目录/文件}` + `-output-directory`」 |
| ⑪ 预览状态保持 | design.md §预览 + 本文件 §5.7（页哈希差分候选）+ [incremental-edit-x-dvi.md](./incremental-edit-x-dvi.md)（判定输入已就位：差分 ⟷ SyncTeX 对拍 + "0 页变化"实测） |
| DVI/XDV 预览（**否决**） | [dvi-preview-feasibility.md](./dvi-preview-feasibility.md) |
| G2 字节偏移重同步（**索引吸收**） | [g2-byte-offset-resync.md](./g2-byte-offset-resync.md) + `scripts/xdv-report.mjs` |
| G1 拦截每一次读（**降级为依赖记录**） | [g1-read-interception-feasibility.md](./g1-read-interception-feasibility.md) + `scripts/fls-report.mjs`（`.fls` / `.fdb_latexmk` 依赖报告） |
| 上游阶段 2（子进程/流式/时间片） | [stage2-streaming-feasibility.md](./stage2-streaming-feasibility.md)（进度三通道 + 部分 PDF 实测 + UI 不卡对照） |
| ⑫ TinyTeX / ⑮⑯⑰⑱ | design.md §后置/未决清单 |

## 10. 不确定性与待补数据

**A. 会影响排期的未知**

1. **㉖ 的触发条件未定位**：`\include{子目录}` + `-outdir=tmp` 的最小复现在**无 rc / 精简 rc** 下 latexmk 都能自愈；hithesis 那一档不自愈且挂起，**是 rc 的哪一行造成的还没定论**（已排除"单纯覆写 `$pdflatex` + 尾部 `;cp`"）。
2. ~~**⑦c 的多文件大项目全未测**~~ **已补（2026-09，⑦c 完成）**：≥20 文件、打开全部标签的内存/耗时、树刷新、include 图解析规模均已测，五项门槛全过、结论"编辑器侧不需要优化"（[p1c 报告](./p1c-multifile-large-project.md)）；剩下的是**编译期**表现与可信堆读数（见 §10-B.21）。
3. **⑥ 留了三个未做项**：MCP 状态订阅（notifications）、headless 与 GUI 并发编译同项目的锁（现约定 headless 独占）、headless `file_write` 不代建父目录（与 GUI 同契约）。
4. **⑦a 留了一个开口**：大纲只在**编译成功**时刷新，编译一直失败时大纲停在旧结构；缓存已让"按保存触发"变便宜（8–10ms/次），但时机/节流策略未定（modules.md §12.1 #17）。

**B. 需要补测的数字**

5. **③ 的真实档数字需重测**：`thesis-real-hithesis` 的 ">240s / >600s 未收敛" 至少部分是**"报错后 latexmk 挂住"**造成的，不是纯慢；㉖ 修好后必须重跑 `node scripts/bench.mjs --with-real --no-warmup`。
6. **③ 的预算口径待复核**：实测"冷编译 ≤2s 的小文档"在本机几乎不存在（连 6 行文档都 2.3s）→ design.md 的小/大文档分类与 1s/2s、3s/5s 阈值需要重新定义（放宽？还是改为相对基线的回归容忍度？）。
7. **㉘ 的剩余开口**：bib/biber 场景的编辑期样本（现有 fixture 不触发 bibtex）。
8. **G2 的"增量追加"未实现**：本文的成本下界（0.3ms/0.5MB）是按吞吐推算的；真正的追加式解析器要维护"已扫描 offset + 跨 flush 的半页状态"（上游 `incdvi` 的做法）。页完成度进度若要落地，这条必须先做。
8′. **阶段 2 的两条开口（2026-09 专项；① 已闭环）**：① ~~runner 的 stdout 管道未接（`Stdio::null()`）~~ → **✅ 已落地（2026-09，见 §1 基线「阶段 2」行）**：页进度 + 编译中致命错误都已在真机验证；② "渐进部分 PDF"只做到**可行性**（能造出来），**端到端体感未评估**（滚动位置稳定性、pdf.js 频繁重载抖动、额外 `xdvipdfmx` 对编译时长的影响都未测），且只对 >5s 的长编译有意义。
9. ~~**⑪ 的页哈希差分未与 SyncTeX 对拍**~~ **已补（2026-09，阶段 3 深挖）**：两个新样本完全自洽（等长替换 19=19；文首插入首个命中页 5=5）——即页哈希差分可以直接当 ⑪ 的"哪些页要重绘"判定输入。见 [阶段 3 报告](./stage3-incremental-output-parsing.md) §7。**仍未做**：插入导致**页数变化**（其后页号整体平移）时的对拍。
10. **未在真实学位论文上验证 DVI/G2**：hithesis 系被 ㉖ 阻塞；真实模板的 specials/图片更多，页包自包含性**预期**仍成立但未实测。

**C. 证据性质与口径**

11. **W 打分是二手推断**：调研全部来自公开社区数据，**没有一手用户访谈/问卷**；真实取舍前建议补 5–10 个目标用户访谈。
12. **成本 C 未经工程估算**：C 是分析者基于当前代码结构的判断 `[推断]`，落地前应由实现者复核（⑦a 实测 C=2 属实；⑦c/⑧⑨ 未验）。
13. **V 的重估依赖"MCP 会话在场"**：MCP 驱动断言需应用在 debug 下运行 + 会话连接，**不是 CI 可跑的自动化**；V=2 表示"半脚本化、确定性断言"。
14. **付费意愿未验证**：只找到"换工具"意愿，没有付费证据。

**D. 已定性、不再追的旧条目**（保留以免重复提议）

15. **beamer 的 2–4 行往返偏移**：三组样本里只有 beamer 有偏移，换取块规则后不变 → 归因于 beamer/主题的 synctex 记录粒度，**不修**（基线见 design.md §预览）。
16. **① 的遗留（编辑 GBK 源文件只报英文错误）**：已由 ㉓ 处理（中文可操作提示 + 状态栏提示条）；**仍不支持编辑**非 UTF-8 源文件，这是设计取舍不是欠账。
17. **`cargo test -p TeXpresso`（src-tauri）在本机因 WebView2 限制无法运行**：见 [troubleshooting.md](../troubleshooting.md)（另有新增坑：接了常驻 `latteset-mcp` 后该二进制被锁，需换 `CARGO_TARGET_DIR` 或用 `--lib`）。
18. **`parse_log` 的文件栈错位（2026-09 落地阶段 2 时新发现，未修）**：TeX 日志把"关闭上一个文件 + 打开下一个文件"打在**同一行**（`[64]) (./ch_05.tex`），而扫描器只认行首 `(` / `)` → 文件归属错一章（实测：错误在 `ch_05.tex:164`，列表报 `./ch_04.tex:164`），错误列表的文件名与点击跳转目标都错。终态与流式共用同一解析器，两条路径都受影响；修法要按字符顺序做括号配对。见 [modules.md](../modules.md) §12.1 #22 与 [troubleshooting.md](../troubleshooting.md)。
19. **精确失效的收益未在真实大文档上量化**（[G1 报告](./g1-read-interception-feasibility.md) §8.5）：只测到小探针"改无关文件 → 白编译一次"；真实收益 = 白编译频率 × 单次编译时长，两个因子都未统计。
20. **`.fls` / `.fdb_latexmk` 当依赖源的边界未全测**（[G1 报告](./g1-read-interception-feasibility.md) §8）：多趟 xelatex 的 `.fls` 合并行为、biber/makeindex 等更多子步骤、编辑期写入时机，均未验证。
21. **⑦c 的三条遗留**（[p1c 报告](./p1c-multifile-large-project.md) §8）：多文件大项目的**编译期**表现（watch 事件淹没）未测；`Ctrl+F`/大范围替换未测；堆内存没有可信读数（dev 下 `performance.memory` 抖动到出现负差值），且只测了 dev 模式、生产构建未测。
22. **"无变化编辑"的频率未量化**（[增量编辑 × DVI](./incremental-edit-x-dvi.md) §7）：页哈希差分能精确判出"这次编译什么都没变"（实测只加注释 → 125 页 0 变化），但它决定的是"能省多少"——真实使用中这类编辑的占比未统计（需在编译后钩子上挂 `--diff` 记录一轮）。
23. ~~**导言区 fmt 化的收益未实测**~~ ✅ **已实测并否决（2026-09）**（[无 fork 报告](./no-fork-alternatives.md) §3.1）：① `xelatex -ini … \dump` 直接报 `! Can't \dump a format with native fonts or font-mappings.`——fontspec/ctex 必然引入 native font，**中文文档禁止 fmt 化**（引擎硬限制）；② 成本分解：**引擎启动 946ms** + **ctex/字体 400ms**（fmt 都省不到）＝ **61%**，可 fmt 化的"宏包加载"只有 **150ms**。⇒ **编辑期固定开销不可压缩**；能做的是"减少编译次数 / 让单次编译不可见"（都已在做）。
