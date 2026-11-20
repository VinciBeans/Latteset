# Latteset Roadmap（优先级、批次与已否决项）

本文是**产品路线与优先级的唯一排序源**。编号（①②…㊾ 等）在全仓文档与代码注释中被引用，**含义以本文为准**；已否决项也在此列明，免得重复提议。
- 已落地项及其证据在 §1 基线（每条一句话现状 + 证据链接）；待办与批次在 §2；逐项详报（设计、实测、判据）在 [archive/research/roadmap-item-specs.md](./archive/research/roadmap-item-specs.md)，调研证据状态表在 [archive/research/README.md](./archive/research/README.md)。

## 1. 已完成基线

> **读法**：本表是**实现当时**的记录，含当时的引擎/驱动口径。ADR-0015 之后引擎只剩 Tectonic、latexmk 路径已删除 ⇒ 涉及 latexmk / xelatex / xdvipdfmx 的行（⑲㉖㉘㉝…）只作历史依据，**现行引擎口径见 §2 开头**。

| 编号 | 项 | 现状（一句话） | 证据 |
|---|---|---|---|
| ① | 中文文件名/路径与编码 | 路径层面全链路可用；GBK 源 + pdflatex 的非法 UTF-8 日志不再退化成「拿不到任何错误信息」 | [troubleshooting.md](./troubleshooting.md) |
| ②-1 | 根文件候选可见可交互 | 候选清单经 `ProjectInfo.root_candidates` / `get_project` 给到状态栏入口；`update_settings` 会同步内存 `ProjectState.root_file` | [dev/modules.md](./dev/modules.md) |
| ③ | 性能基准与回归基建 | `scripts/{gen-bench-projects,bench}.mjs` 一条命令出 cold/noop/edit 报告、超预算退出码 1；瓶颈在小文档固定开销而非大文档 | [design.md](./design.md) |
| ④ | 错误诊断升级 | **22 类** `DiagnosisKind`（缺包/缺类/缺字体/字体集/引擎不匹配/未定义命令/组未闭合/连锁 Emergency stop…），匹配不到则降级原文 + 行号；覆盖 23 例真实语料 | [design.md](./design.md) |
| ⑤ | SyncTeX 可靠性加固（含 ㉒、⑳） | 三组样本 34 点：正向/反向/同文件 34/34、跳到位 ≤5 行 34/34；生成文件被当源码打开 / 失败静默 / 编译中竞争三处缺陷已修 | [design.md](./design.md) |
| ⑥ | CLI + MCP 交互接口 | `latteset-server` crate：headless `Session` + `latteset-cli` + `latteset-mcp`，零新增第三方依赖；闭环 open → read → write → compile 已跑通 | [cli.md](./cli.md) |
| ⑦a | 大纲重扫增量 | 按内容指纹复用未变化文件的扫描结果 + 前端只提交变化过的缓冲；11 文件/436KB 实测 55.5ms → 9.7ms | [archive/research/p1-large-doc-editor-analysis.md](./archive/research/p1-large-doc-editor-analysis.md) |
| ⑦c | 多文件大项目（编辑器侧） | 三档夹具（9/21/41 文件 × 2.2–5.5MB）五项门槛全过：每击键净开销 0.077–0.097ms 恒定、无 >50ms longtask、关标签无泄漏 ⇒ **不需要优化** | [archive/research/p1c-multifile-large-project.md](./archive/research/p1c-multifile-large-project.md) |
| ⑩ | 深色主题（Candy Desk 暗色） | token 两层（通道变量 + 角色 token，59 个）单点定义/覆盖 + Monaco 深浅两套 + `ui.theme = light/dark/system`（默认 light）+ 首帧不闪 | [design.md](./design.md) |
| ⑲ | 模板样本调研 | 19 个真实模板双引擎实测 **0/19** 因默认引擎选错、`\RequirePDFTeX` 0/7374 ⇒ ②-2 砍掉；另暴露缺包/缺字体、首编超时、latexmkrc 交互、`.ins`/`.dtx` 四类障碍 | [archive/research/template-corpus-survey.md](./archive/research/template-corpus-survey.md) |
| ㉑ | 外部改 `.latteset/settings.json` 同步 `root_file` | watch 热更新后同步内存覆盖状态，合并基数改为磁盘纯全局；外部非原子写导致的截断读取有 3×150ms 短重试 | [dev/modules.md](./dev/modules.md) |
| ㉒ | 生成产物不当源码打开 | 反向定位落到生成产物时先回落真实源码并提示"已回落"，落空只给工具条同步提示 | [glossary.md](./glossary.md) |
| ㉓ | 编辑非 UTF-8 源文件 | `InvalidData` 返回中文可操作提示（另存为 UTF-8）+ 状态栏提示条；**故意不做 lossy 打开**（保存会写回 U+FFFD） | [design.md](./design.md) |
| ㉔ | 文档路径引用迁到 `latteset-infra` | `cli-mcp-plan` 里的引用已改为 `crates/latteset-infra/src/{runner,storage}.rs`，无残留 | [archive/cli-mcp-plan.md](./archive/cli-mcp-plan.md) |
| ㉕ | 学位论文首编超时 | 上限 600→1800s；不再静默重试，超时进错误列表给证据化诊断 + `suggested_timeout_secs` 与一键「提高到 Ns 并重试」 | [design.md](./design.md) |
| ㉖ | 模板自带 latexmkrc 与构建约定的交互 | 真凶是模板 rc 的 `$preview_continuous_mode = 1`（= `-pvc`，编译完不退出）；产品用 `latexmk -e` 中和 + 预建 `tmp/<子目录>/`，真夹具从"挂到超时"变为 3s 返回真实错误 | [archive/research/roadmap-item-specs.md](./archive/research/roadmap-item-specs.md) §6.1 |
| ㉗ | `.ins`/`.dtx` 源码版模板 | 缺 `.cls` 且项目里有同名 `.ins` 时给具体命令 `xelatex X.ins`；同名优先 `.ins`，只有 `.dtx` 时不编造命令 | [dev/modules.md](./dev/modules.md) |
| ㉘ | 编辑期轻量出图 + 空闲收敛 | Quick 直调引擎单趟 / Full 完整 latexmk；实测省 40.2%（6/6 档达标），无 `.aux` 自动升级 Full，2s 空闲收敛补目录页码 | [design.md](./design.md) |
| ㉚ | 构建确定性验证 | `scripts/check-determinism.mjs`：不固定 `SOURCE_DATE_EPOCH` 时两次编译只差 trailer `/ID`，固定后 3 档 × 2 路径逐字节一致 | [design.md](./design.md) |
| ㉛ | 编译决策日志 | 触发侧打三类"不触发"原因，调度侧打 开始/入队/合并替换/直接执行/成功(draft)/失败/终止清队；core 加 `tracing` facade | [dev/modules.md](./dev/modules.md) |
| ㉜ | 非 `.tex` 输入变化也触发编译 | 触发规则改为「项目内任何带扩展名的非忽略文件」（core 单一落点）；自己写在项目根的 `<stem>.pdf` 与 `.pdf.tmp` 被拒，目录不触发 | [dev/modules.md](./dev/modules.md) |
| ㉞ | 流式出图（编译中可见逐页出现） | 四片全部落地：趟边界出图提前 ≈5.5s、代价 +4.4%，`compile-preview` 独立事件，前端"换字节不换身份"，中止后 106ms 退回权威 PDF | [archive/research/dvi-preview-feasibility.md](./archive/research/dvi-preview-feasibility.md) |
| ㊱ | SyncTeX 失败查询的重试语义 | 错误按语义分四类，确定性失败不再退避 ⇒ forward 确定性失败 668→14–15ms、无同步数据 645→10–11ms、inverse 3127→11ms；瞬时竞争仍退避成功 | [dev/modules.md](./dev/modules.md) §5 |
| ㊲ | 首屏让位 | **定量否决**：索引真实耗时 278ms（旧线性外推高 2.6×）且不挡首屏，最坏附加延迟 ≈40ms ⇒ 不改 | [archive/research/roadmap-item-specs.md](./archive/research/roadmap-item-specs.md) §6.9.1 |
| ㊳ | 保存→开编链路的往返成本 | **定量否决**：Leg A 6–8ms + Leg B 0.21–0.26ms ⇒ 端到端 ≈7ms，与「≥100ms 才值得做」的门槛差 14 倍 | [archive/research/roadmap-item-specs.md](./archive/research/roadmap-item-specs.md) §6.9.1 |
| ㊷ | 库形态「根文件」的 SyncTeX 定位 | 收尾把主输入名补成真实绝对路径（纯数据补丁，不改引擎）；两夹具 `forward` 7/7 逐字段一致，PDF/日志/错误归属零变化 | [dev/modules.md](./dev/modules.md) §5 |
| ㊸ | 悬停公式即时预览 | 三片全落地：`core::math::math_at` + headless `latteset-cli math`（171→79ms）+ 前端悬停浮层；主编译期间不发起、主编译墙钟增量 −1.0% | [archive/research/roadmap-item-specs.md](./archive/research/roadmap-item-specs.md) §6.11 |
| ㊻ | 引擎清单动态化 + 默认引擎切 Tectonic | 引擎顺序/名字/说明/本机可用性都由后端给，`LATTESET_TEX_ENGINES` 可筛选重排但不能伪造可用性；新装默认 Tectonic，老配置不变 | [ADR-0014](./adr/0014-tectonic-first.md) |
| ㊼㊽㊾ | 文件管理补齐：删 / 改名 / 搜索 | 文件树补齐 CRUD：删除二次确认 + 目录递归、改名行内编辑、搜索模糊匹配；后端拒绝删/改项目根 | [archive/research/roadmap-item-specs.md](./archive/research/roadmap-item-specs.md) §6.15 |
| 阶段 2 | 引擎子进程 + 流式输出 | 管道 `Stdio::piped()` + 尾随 `tmp/<stem>.log` → 状态栏「已排版 N 页」+ 错误列表在编译结束前报致命错误；中间态走独立事件 + `phase` 守卫 | [dev/modules.md](./dev/modules.md) §2.4 |
| 增量编辑 × DVI | 页哈希差分驱动的三处复用 | 逐页未变则跳过预览重载、只重绘变化页（render 102→7ms）、页哈希全同则跳过 PDF 转换；core 新增 `xdv` 模块 | [archive/research/incremental-edit-x-dvi.md](./archive/research/incremental-edit-x-dvi.md) |
| 无 fork 替代路径（G4 / 阶段 6·7） | 快照 API 盘点 + 导言区占比实测 71–89.5% + 5 条替代逐条判定 | [archive/research/no-fork-alternatives.md](./archive/research/no-fork-alternatives.md) |

> 同期完成的**非 roadmap 工程项**：`latteset-infra` 拆分（[ADR-0010](./adr/0010-infra-crate-boundary.md)）、大纲解析下沉 Rust、架构图与渲染脚本、MCP Bridge 升级、真机验收清单固化、`scripts/xdv-report.mjs`。

## 2. 待办与批次

**评分模型**：W 痛点权重 / D 差异化 / C 实现成本 / V 验证难度，四项 1–5 分；`P = (W + D) / (C + V)`；分档 P0 = `P ≥ 1.3 且 W ≥ 4`、P1 = `P ≥ 0.9`、P2 = `P ≥ 0.6`，`P < 0.6` 或定位冲突/已证伪 = 不做。
**引擎口径**（[ADR-0015](./adr/0015-tectonic-only-embedded-engine.md)）：产品**只有 Tectonic 一个引擎、只有内嵌一种形态**——②段已完成（子进程路径、`tectonic.lib_form`、`LATTESET_TECTONIC_LIB` 与 UI 形态位全部删除，设置迁移到 v3，构建/CI 统一走 vcpkg 入口；证据 `test_file/phase2/REPORT-gates.md`）。xelatex / lualatex / pdflatex 与 latexmk 驱动已从产品中删除。新功能的设计与验收基准 = **内嵌 Tectonic**。**宏包集分发方案待定**：[ADR-0016](./adr/0016-ship-bundle-offline.md) 原决定"随包**完整**宏包集"，但 2026-11-16 实测完整 bundle ≈ **2.88 GB**（`test_file/acc/REPORT-acceptance.md:61`）⇒ 随包不可行，**方案待③段前重定**（候选 A 随包精简集 / B 首次联网 + 可选本地 bundle / C 混合，见 ADR-0016「实测修正」；G-L1..G-L9 门禁对最终随包物照旧）。当前默认**首次联网取包**（`network-bundle` 已打开）、用户可配本地 bundle。兼容性边界（内嵌不跑外部 biber、makeindex / glossaries 不自动跑、模板 `latexmkrc` 约定失效）按 ADR-0015 第 4 条如实标注，**不静默降级**。
**快速通道**：成本 ≤2 且独立可交付的小项随时可插队（不占批次位次）。当前通道为空；㉝（精确失效，P2）仍可随时插队。

### 2.1 待办总表（档内按 P 降序）

| 编号 | 事项 | W | D | C | V | P | 档 | 依据与下一步 |
|---|---|---|---|---|---|---|---|---|
| ⑧ | 跨文件 LaTeX 语义操作 | 4 | 5 | 4 | 3 | **1.29** | P1 | VS Code 明确缺失该能力；与 ⑨ 合并为一个里程碑 |
| ⑪ | 预览滚动/缩放保持、只重排变更页 | 4 | 3 | 4 | 2 | **1.17** | P1 | 判定输入已就位（页哈希差分与其 SyncTeX 对拍）；先做零风险的"全同则不刷新" |
| ⑫ | Tectonic 内嵌引擎集成（原 TinyTeX 捆绑） | 5 | 4 | 5 | 4 | **1.00** | P2\* | 已完成（②段：内嵌成为唯一形态；准入判据零预装 / 能力面一致 / 启动 ≤60s 由 T9 出数，见 [dev/tectonic-library.md](./dev/tectonic-library.md)） |
| ⑨ | LSP（texlab）集成 | 4 | 3 | 4 | 3 | **1.00** | P1 | 补全/引用是长期痛点；先做 monaco-languageclient 技术验证 |
| ⑬ | AI 集成 | 3 | 4 | 4 | 3 | **1.00** | P2 | 竞争主轴；**仍无一手需求证据** |
| ⑭ | 拼写检查 | 3 | 2 | 3 | 2 | **1.00** | P2 | 桌面编辑器调研里"拼写/语法弱" |
| ㉙ | 未存盘 overlay（编辑期不写用户文件） | 2 | 3 | 3 | 2 | **1.00** | P2 | 与 [ADR-0007](./adr/0007-filesystem-as-source-of-truth.md) 冲突，且无实测痛点支撑 |
| ㊶ | `\include` 分章并行草稿（**依赖 ㊴**） | 3 | 4 | 5 | 4 | **0.78** | P2 | 只对用 `\include` 的文档有效；浮动体与跨章引用保真需先对拍 |
| ㊴ | 子进程编译通道（主编译与草稿并行） | 4 | 3 | 5 | 4 | **0.78** | P2 | 唯一的大杠杆：引擎锁是**进程内**的，跨进程不受它管；两进程并发实测 ≈+30%（真并行） |
| ㊵ | 项目打开并发 | 2 | 1 | 2 | 2 | **0.75** | P2 | `open_project` 顺序只读 await ⇒ `try_join!` 可省一次全树遍历；收益小、风险低 |
| ㉝ | 精确失效：用依赖集合过滤 watch 事件 | 2 | 1 | 2 | 2 | **0.75** | P2 | 改未被引用的 `.tex` 会白编译一次；⚠ 原拟判据（`.fdb_latexmk` 源依赖集合 + `.fls`）**是 latexmk 的产物，ADR-0015 后不再生成** ⇒ 判据失效，要重做需另找通道（未定，见 [dev/modules.md](./dev/modules.md) §12.1「触发面收紧」条） |
| ⑮ | 多窗口与多项目 | 3 | 2 | 4 | 3 | **0.71** | P2 | 后置项 |
| ㊹ | 项目导言区固化为 format | 3 | 2 | 5 | 2 | **0.71** | P2 | 主写法被引擎拒绝（`Can't \dump a format with native fonts`），剩一条变体未测；成本集中在 format 生命周期 |
| ⑱ | 代码签名证书 | 2 | 2 | 2 | 4 | **0.67** | P2 | 分发信任，但外部依赖重 |
| ⑯ | 外部 PDF 查看器 | 2 | 2 | 3 | 3 | **0.67** | P2 | 后置项 |
| ⑰ | 自动更新 | 2 | 2 | 3 | 3 | **0.67** | P2 | 后置项，外部依赖重 |
| ㉟ | 窗口按钮融入应用界面（无边框 + 自绘顶栏） | 2 | 2 | 4 | 3 | **0.57** | 指定\*\* | 低于 0.6 的自动门槛（模型口径 = 不做档）⇒ **产品负责人点名入表**，不进批次 |
| ㊿ | 代码库与文档重整的**留档项**（大文件拆分、前端大组件拆分、缺单测等） | — | — | — | — | — | 留档 | 各条"为什么缓做"见主源 §6.16；动它们之前先读该节 |

\* ⑦c 的 P0 身份来自"**先测量**"这一步；⑫ 的 `*` 指它不适合塞进批次，应按独立里程碑推进。⑦b（折叠提供者增量）不在表内：实测 2.2–3.7ms/次、未越 60Hz 帧预算。\*\* ㉟ 的 `**` 指低于自动门槛但产品负责人点名。

### 2.2 批次与出口条件

| 批次 | 内容 | 出口条件 | 状态 |
|---|---|---|---|
| **Batch 1 收口** | 快速通道 ㉓㉚㉗㉑㉛ | 小项各自独立可交付 | 已达成 |
| **Batch 2 攻核心承诺** | ⑥ CLI + MCP | Agent 能驱动「改 → 编 → 验」闭环 | 已达成（真机走通 + 回归固化） |
| **Batch 3 硬骨头** | ⑦a → ⑦c → ⑩ → **⑧⑨ 语义层（合并）** → ⑪㉖ | 交互期无 >50ms 长任务；跨文件重命名可用；主题一致；带 rc 的模板编译路径有结论 | 进行中（⑦a / ⑦c / ⑩ / ㉖ 已收口，余 ⑧⑨ 与 ⑪） |
| **独立里程碑** | ⑫ Tectonic 内嵌引擎集成 | 零预装可用、能力面（页哈希 / 编译中预览 / 内存产物 / format 缓存）对所有用户一致可用；构建统一 vcpkg 入口 | 已完成（①段内嵌可用 → ②段收成唯一形态；证据 `test_file/phase2/REPORT-gates.md`） |
| **Batch 4 流式出图** | ㉞ | 长编译期间预览可见地逐页出现、页数与滚动位置正确；内嵌唯一后该能力恒可用（趟间可见） | 已完成 |
| **指定项（未排期）** | ㉟ 窗口按钮融入应用界面 | 无边框窗口下系统行为一条不丢：拖拽/8 向缩放/双击最大化/右键系统菜单/Alt+F4/Win+方向键吸附/DPI 切换/贴靠布局 | 未开始 |
| **Batch 5 并行与"白等"** | 快速通道（空，㊱㊲㊳ 均已收口）→ **㊴ 子进程编译通道**（→ ㊶）→ ㊵ | ㊴：主编译墙钟增量 ≤10%、低核机器（≤4 核）默认不开、缓存零互踩、进程杀干净；㊶：分章草稿页码/编号与全篇一致 | 余 ㊴㊵㊶ 未开始 |
| **库形态 SyncTeX 修复** | ㊷ | 库形态 `.synctex.gz` 的 `Input:1` 是真实路径；单文件与分章两夹具两形态 `forward`/`inverse` 逐字段一致；排版结果/日志/错误行归属零变化 | 已完成 |
| **候选（未排期）** | ㊸ 悬停公式即时预览 / ㊹ 导言区 format / ㊺ 软件内新建目录 | ㊸：公式扫描器单测通过、首次出图 p50 ≤400ms、主编译墙钟增量 ≤5%、权威产物与关功能时逐字节相同 | ㊸㊺ 主体完成（余新建目录与右键入口）；㊹ 待测一条变体 |
| **文件管理补齐** | ㊼㊽㊾ 删 / 改名 / 搜索 | 删除必须确认、取消零副作用；改名只改本目录；收口编辑器状态与根文件重探；搜索命中保留祖先 | 已完成 |
| **Tectonic 唯一引擎重构**（[ADR-0015](./adr/0015-tectonic-only-embedded-engine.md) / [ADR-0016](./adr/0016-ship-bundle-offline.md)） | ① 摘除 XeLaTeX/LuaLaTeX/pdfLaTeX + latexmk（**先保留两形态**）→ ② 收成内嵌唯一（构建/CI/设置/UI/状态栏/CLI/MCP/文档）→ ③ release 基线、性能优化与**按 ADR-0016「实测修正」重定 bundle 分发方案**（完整 bundle ≈2.88 GB，随包不可行） | ① 非 Tectonic 引擎分支 `grep` 为零、按语义改写的集成用例全绿；② 所有构建统一走 vcpkg 入口（缺项给修复命令）、旧 `settings.json` 迁移有一次提示、U-14（子目录 `\include`）跑绿、CLI/MCP 引擎开关收口；③ 六档 cold/noop/edit 基线进 `design.md`、编辑→出图不劣于 1 页暖编 372–642ms、**bundle 分发方案定稿**（候选 A/B/C 择一，给出安装包体积上限与离线验收判据）；发包前 G-L1..G-L9 绿（[审计](./dev/tectonic-licensing.md)） | **① 已完成**；**② 已完成（待提交/推送）**：门禁全 PASS（core 268 / infra 40 / tectonic 44+1 / server 24+4 / mcp 6+1 / vitest 151 / bindings 13/13），headless 真编 5 子项 PASS，启动 5.78s（热）/ 12.96s（强制重编），真机窗口因会话环境限制 `[未测]`——证据 `test_file/phase2/REPORT-gates.md`；**③ 未开始** |

## 3. 外部方案评估结论

对上游 [let-def/TeXpresso](https://github.com/let-def/texpresso)（改造 XeTeX 的实时预览器）的源码通读与 8 阶段复刻路线评估结论：**不采用该复刻路线**（它要求"拥有目标程序的内部"）——整条评估见 [archive/texpresso-live-rendering-roadmap.md](./archive/texpresso-live-rendering-roadmap.md) 与主源 §5。

**吸收的是与引擎无关的思想与便宜的那一半**（逐条现状见主源 §5.4）：空闲收敛（编辑期跑轻的、停手后跑全的）→ ㉘ 已落地；把决策过程打出来 → ㉛ 已落地；确定性验证（连跑两次逐字节比）→ ㉚ 已落地；未存盘 overlay → ㉙（P2，与 ADR-0007 冲突）；编译期页完成度进度 → 需求已由内嵌形态的「捕获 XDV 完整页数」落地（每趟收尾、只增）；页级变更判定 → ⑪ 的候选判定输入；引擎 stdout 管道 → 随子进程路径删除（内嵌形态无 stdout）。

**四条前提的结论**（G1 拦截每一次 I/O / G2 字节偏移重同步 / G3 部分输入 / G4 fork 快照）：G1 ➖ 不需要（"读了什么"已由 `.fls` / `.fdb_latexmk` 免费提供，字节偏移那半既拿不到也没有消费方）；G2 ✅ 成立且便宜（自研页索引，截断到任意位置解出的页与完整文件逐字节相同，4.41MB/186 页 2.7ms）；G3 ➖ 不需要；G4 ➖ 不需要（Windows 无 `fork()`，且实测编辑期单趟里导言区占 71–89.5%，fork 只能省小头）。**但整体仍停手**：G2 只给"知道第几页在哪、哪页变了"，不给"把页画出来"——渲染那一半比 PDF 链路慢 57–68×。

## 4. 已否决（不做）

| 项 | 理由 |
|---|---|
| **TeXpresso 式 fork 快照 / seen 水位 / VFS 事务回滚**（其阶段 7/6/5） | 需 G4（Windows 无 fork）与 G1（拦截 I/O）；VFS 与 [ADR-0007](./adr/0007-filesystem-as-source-of-truth.md) 冲突 |
| **DVI/XDV 作为预览格式** | 产物→屏幕比 PDF 链路慢 **57–68×**（121 页 44.06s vs 0.65s + 115ms）；XDV 体积不定（0.15×–29× PDF）；specials 是 `pdf:` 方言。见 [archive/research/dvi-preview-feasibility.md](./archive/research/dvi-preview-feasibility.md) |
| **自研 DVI/XDV 渲染栈** | 需字形级渲染 + 系统字体（含 `.ttc`）+ specials；上游用 MuPDF（AGPL，与本项目 MIT 冲突）。索引那一半已吸收 |
| **自建/改造 TeX 引擎**（含 fork TeX Live 的 xetex） | 许可允许但代价三层（从源码构建并长期跟进 TeX Live、分发性质改变、包与字体自己跟），且最值钱的 fork 快照在 Windows 上仍不可达；真要走应走 Tectonic 并另立 ADR |
| **实时协作** | P=0.40；在线方案护城河，本地方案投入产出不成立。定位「个人 / 离线优先」，协作交给 Git |
| **增量编译具体策略** | [ADR-0005](./adr/0005-latexmk-first-incremental-next.md) 已实测证伪：latexmk 增量＝整份单遍重排，属引擎上限 |
| **引擎语言自适应规则** | 19 个真实模板双引擎实测 **0 例**因默认引擎选错、`\RequirePDFTeX` 0/7374 ⇒ 解决的是不存在的问题；改用 ④ 的「选错时明确告诉用户怎么改」替代。见 [archive/research/template-corpus-survey.md](./archive/research/template-corpus-survey.md) |
| **推断 bib 工具链** | 实测 latexmk 自己就会跑 `bibtex`/`biber`，无需推断也无需用户配置 |
| **自研 PDF 渲染引擎** | pdf.js 已是够用底座；LaTeX Workshop 的教训是"预览是二等公民"，不是"必须自研渲染器" |

## 5. 后置 / 未决

多窗口与多项目 ｜ 外部查看器 ｜ 自动更新 ｜ 代码签名证书 ｜ 冲突对话框（v1 以状态栏「外部修改」点击重载替代）｜ 多面板布局 ｜ LSP 具体集成（monaco-languageclient 需专项研究）

## 6. 逐项详报索引

- [archive/research/roadmap-item-specs.md](./archive/research/roadmap-item-specs.md)：§6.1–§6.15 逐项详报（设计、实测数据、判据与风险）。
- [archive/research/README.md](./archive/research/README.md)：调研文档状态表（路径 / 状态 / 一句话结论 / 被谁引用）；[archive/research/tex-ide-roadmap-priority.md](./archive/research/tex-ide-roadmap-priority.md)：主源全文（含评分模型 §2、§6.16 代码库与文档重整留档、§10 不确定性与待补数据）。
