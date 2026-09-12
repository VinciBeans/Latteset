# TeXPresso Roadmap（重整理版）

> 状态：**可执行排序（第 2 版 + ⑲ 调研修订，2026-09）**；第 1 版见 git 历史
> 依据：[tex-ide-pain-points.md](./tex-ide-pain-points.md)（总览）+ [desktop](./desktop-latex-editor-pain-points.md) / [online](./online-latex-editor-pain-points.md) / [vscode](./vscode-latex-workshop-pain-points.md) 三份专项 + [P0-② 分析](./p0-2-first-open-analysis.md) + [**模板语料调研**](./template-corpus-survey.md) / [中文高校模板引擎要求](./cn-thesis-template-engines.md)
> 现状基线：[design.md](../design.md) §后置/未决清单、[modules.md](../modules.md) §12、[cli-mcp-plan.md](../cli-mcp-plan.md)、[troubleshooting.md](../troubleshooting.md)
> **ID 稳定约定**：①…⑱ 沿用第 1 版编号（`modules.md` / `troubleshooting.md` 里的「roadmap P0-①」「P0-②-1」等引用因此不失效）；新增项从 ⑲ 起续编。已完成项**保留 ID**、移入 §1 现状基线，不再占用待办位次。

## 0. 变更记录

### 0.1 ⑲ 模板样本调研的修订（2026-09，本次）

| # | 变更 | 依据 |
|---|---|---|
| 1 | **⑲ 完成**，移入 §1 基线 | [template-corpus-survey.md](./template-corpus-survey.md)：19 个真实模板 × 双引擎实测 + 7,374 份样例静态扫描 |
| 2 | **②-2 引擎推断 → 砍掉**（移入 §6 不做） | **0/19 案例**因默认 XeLaTeX 选错；`\RequirePDFTeX` = **0/7374**；文档调研 19/19 要求或兼容 XeLaTeX。它解决的是一个不存在的问题 |
| 3 | **新增 ㉕ 学位论文首编超时**（P0） | 实测 hithesis / hitszthesis 在 90s 超时前已产出 PDF（596 KB / 5 个变体）→ **默认 `timeout_secs=120` 存在风险** |
| 4 | **新增 ㉖ 模板自带 latexmkrc 与构建约定的交互**（P1） | latexmk 会自动读模板 rc（实测 `Rc files read: latexmkrc`），但那些 rc 覆写 `$pdflatex`、内建 `--shell-escape`、自带 `cp`，与我们固定的 `-outdir=tmp` + `tmp/<stem>.pdf` 拷贝约定**优先级未验证** |
| 5 | **新增 ㉗ `.ins`/`.dtx` 源码版模板**（快速通道） | GitHub 源码版需先 `*.ins` 生成 cls；实测中 `bithesis-doc.tex` 是 DTX 文档、无法直接编译，佐证这类"看起来是主文件其实不是"的坑 |
| 6 | **④ 扩充 + ③ 扩充** | ④ 增「缺宏包/缺字体提示」（有 `slashbox.sty`/`atbegshi`/`SourceHanSerifSC-Regular` 实测样本）；③ 的基准须含**完整学位论文**档 |

### 0.3 ④ 错误诊断升级完成（2026-09）

| # | 变更 | 依据 |
|---|---|---|
| 1 | **④ 完成**，移入 §1 基线 | `.log` → 「原因 + 怎么改」19 类；23 例真实语料 + 手写期望表；真机 MCP 验证（两行式展示 + 点条目跳 `Ln 4` + 缺宏包点名并给 `tlmgr install` 建议）。见 [modules.md](../modules.md) §12 |
| 2 | ④ 的完成方式印证了 ⑲ 的结论 | 砍掉"引擎自动推断"后，改做「**选错时明确告诉用户怎么改**」（`EngineMismatch` / `UnsupportedEngine` 两类），把 ⑲ 的 4 个实测模板纳入回归语料 |

### 0.2 相对第 1 版的重整理

| # | 变更 | 原因 |
|---|---|---|
| 1 | **① 与 ②-1 移入基线**（§1） | 已完成并真机验证；继续留在 P0 详述里会让文档失真 |
| 2 | **② 正式拆解**：②-1 完成、②-2 引擎推断（后被砍）、**②-3 缺包诊断并入 ④** | [P0-② 分析](./p0-2-first-open-analysis.md) 实测证明「本项不能整体做」 |
| 3 | **V（验证难度）重估**：⑤ 由 3 → **2**（**已实跑**：MCP 点 PDF → 断言「编辑器文件 + 行号」）；④⑪ 由 3 → **2**（同类确定性断言，**尚未实跑**，属推断） | tauri server MCP 已可用；这类"点一下 → 断言 UI 状态"的检查不再依赖肉眼 |
| 4 | **⑥ 的 C 由 3 → 2** | [ADR-0010](../adr/0010-infra-crate-boundary.md) 拆出 `texpresso-infra` 后，headless 复用（runner/storage/watch）不再需要绕开 Tauri 依赖 |
| 5 | **③ 范围修正**：基准工程**脚本生成**而非入库 | `test_file/projects/` 已在 `.gitignore`，第 1 版写的「落库」与既有约定冲突 |
| 6 | **新增「快速通道」小项 ⑳–㉔**（§4） | 全部来自近期实测发现的**真实缺口**（非推测），单项成本 ≤2 |
| 7 | **新增文档欠账小节**（§8） | 发现 `cli-mcp-plan.md` 仍引用 ADR-0010 前的 `src-tauri/runner.rs`、`src-tauri/storage.rs` |

## 1. 现状基线（已完成，不再出现在待办中）

| ID | 项 | 完成 | 证据 |
|---|---|---|---|
| ① | 中文文件名/路径与编码实测复核 | 2026-09 | 路径层面全链路可用；**修掉真实缺陷**：GBK 源 + pdflatex 的 `.log` 含非法 UTF-8 → 严格读取失败会让「编译失败」退化成「拿不到任何错误信息」（`decode_log` + `read_to_string_lossy`）。MCP 补验：中文路径 PDF 渲染正常、反向 SyncTeX 跳到 `中文主文件.tex` Ln 10。见 [troubleshooting.md](../troubleshooting.md) |
| ②-1 | 根文件候选可见可交互 | 2026-09 | `ProjectInfo.root_candidates` + `get_project` + `RootFilePicker.vue` + 状态栏入口；**并修掉阻断缺陷**：`update_settings` 不同步内存 `ProjectState.root_file`（选完仍报「未确定根文件」）。MCP 全链路验证（弹窗 → 点选 → 编译出 PDF）。见 [modules.md](../modules.md) §12 |
| ⑲ | 模板样本调研（默认 XeLaTeX 开箱即用率） | 2026-09 | **0/19** 案例因默认引擎选错；`\RequirePDFTeX` **0/7374**；文档调研 19/19 兼容 XeLaTeX。**结论：②-2 砍掉**。同时暴露 4 类真实障碍（缺包/缺字体、首编超时、latexmkrc 交互、`.ins`/`.dtx`）。见 [template-corpus-survey.md](./template-corpus-survey.md) |
| ④ | 错误诊断升级（含 ②-3 缺包诊断 + 缺字体提示） | 2026-09 | `.log` → 「原因 + 怎么改」：19 类 `DiagnosisKind`（缺包/缺类/缺字体/字体集/引擎不匹配/未定义命令/组未闭合/数学模式/连锁 Emergency stop…），匹配不到返回 `None` 降级原文。**23 例真实语料**（14 例收割自 ⑲ 矩阵 + 9 例探针）+ 手写期望表 + DoD 覆盖率 ≥80%；真机验证：错误列表两行式展示、点条目跳到 `Ln 4`、缺宏包点名 `nosuchpackagexyz.sty` 并给 `tlmgr install` 建议。见 [modules.md](../modules.md) §12 |
| ③ | 性能基准与回归基建 | 2026-09 | `scripts/gen-bench-projects.mjs`（六档纯文本 fixture，确定性生成）+ `scripts/bench.mjs`（cold/noop/edit ×3 取中位数 → 预算判定 → 超预算退出码 1；`stdio:'ignore'` 故沙箱免提权）。**结论：瓶颈在小文档固定开销而非大文档**（tiny 6 行 edit 1625ms vs large 300 页 4104ms；noop 本身 ~0.5s）；**三档小文档全超 2s 及格线**→ 预算口径待复核；**真实论文档 cold >240s 未收敛**（默认超时 120s）。真机 MCP 取预览 115ms，端到端 3078ms 与估算偏差 15ms。见 [design.md](../design.md) §基准脚本实测 |

> 同期完成的**非 roadmap 工程项**（记录以免重复提议）：`texpresso-infra` 拆分（ADR-0010）、大纲解析下沉 Rust、架构图与离线渲染脚本、MCP Bridge 0.12→0.13 升级、MCP 驱动的真机验证方式。

## 2. 评分模型

四项 1–5 分，**分数越高＝该维度越强/越贵**：

| 维度 | 含义 | 打分依据 |
|---|---|---|
| **W** 痛点权重 | 该痛点在市场中的频率 × 严重度 | 调研的量化信号（issue 数、SE 票数/浏览量、来源数） |
| **D** 差异化 | 相对竞品能否形成壁垒 | 5＝竞品结构性做不到；3＝大家都有但可做得更好；1＝无意义 |
| **C** 实现成本 | 工程投入 | 5＝数月/高风险；4＝数周；3＝1–2 周；2＝几天；1＝小时级 |
| **V** 验证难度 | 能否证明"真的做好了" | 5＝需干净机器/多人/长期；3＝需真机交互且含主观判断；**2＝MCP 驱动可确定性断言**；1＝单测即可 |

**优先级分**：`P = (W + D) / (C + V)`
**分档**：P0 = `P ≥ 1.3 且 W ≥ 4`；P1 = `P ≥ 0.9`；P2 = `P ≥ 0.6`；`P < 0.6` 或定位冲突/已证伪 = 不做。

## 3. 待办总表（**按 P0 / P1 / P2 分档，档内按 P 分降序**）

| ID | 事项 | W | D | C | V | P | 档 | 主要依据 |
|---|---|---|---|---|---|---|---|---|
| ㉕ | **学位论文首编超时**（复核默认 120s + 超时提示） | 4 | 3 | 2 | 2 | **1.75** | P0 | ③ 实测：真实模板 `thesis-real-hithesis` cold **>240s 未收敛**（另次 >600s），默认 `timeout_secs=120` **必超时**；合成 thesis 档 9.4s 只是下界 |
| ⑤ | SyncTeX 可靠性加固 | 4 | 4 | 3 | 2 | **1.60** | P0 | P4：148 issue + 327 题；V 3→2（点 PDF 断言行号已跑通） |
| ⑥ | CLI + MCP 接口 | 3 | 5 | 2 | 2 | **2.00** | P1* | C 3→2（infra 拆分后 headless 复用直接）；W=3 故不入 P0，人工列 P1 首位 |
| ⑦ | 大文档编辑器侧性能 | 5 | 4 | 4 | 3 | **1.29** | P1 | desktop 报告"最大可攻占缺口" |
| ⑧ | 跨文件 LaTeX 语义操作 | 4 | 5 | 4 | 3 | **1.29** | P1 | P10：VS Code 明确缺失 |
| ⑩ | 深色主题（Candy Desk 暗色） | 3 | 2 | 2 | 2 | **1.25** | P1 | 配置类第一痛点：206 票 / 23 万浏览 |
| ㉖ | **模板自带 latexmkrc 与构建约定的交互** | 3 | 4 | 3 | 3 | **1.17** | P1 | ⑲：latexmk 自动读 rc（实测），但 rc 覆写 `$pdf_mode`/`$pdflatex`、内建 `--shell-escape`、自带 `cp`，与我们的 `-outdir=tmp` + 拷贝约定**优先级未验证** |
| ⑪ | 预览滚动/缩放保持、只重排变更页 | 4 | 3 | 4 | 2 | **1.17** | P1 | P11：官方承认"丢失滚动位置"；V 3→2 |
| ⑨ | LSP（texlab）集成 | 4 | 3 | 4 | 3 | **1.00** | P1 | P8：补全/引用是长期痛点 |
| ⑫ | TinyTeX 捆绑与环境引导 | 5 | 4 | 5 | 4 | **1.00** | P2* | C/V 全表最高；建议独立里程碑 |
| ⑬ | AI 集成 | 3 | 4 | 4 | 3 | **1.00** | P2 | 2026 竞争主轴；**仍无一手需求证据** |
| ⑭ | 拼写检查 | 3 | 2 | 3 | 2 | **1.00** | P2 | P8：拼写/语法弱 |
| ⑮ | 多窗口与多项目 | 3 | 2 | 4 | 3 | **0.71** | P2 | design.md 后置项 |
| ⑯ | 外部 PDF 查看器 | 2 | 2 | 3 | 3 | **0.67** | P2 | design.md 后置项 |
| ⑰ | 自动更新 | 2 | 2 | 3 | 3 | **0.67** | P2 | design.md 后置项 |
| ⑱ | 代码签名证书 | 2 | 2 | 2 | 4 | **0.67** | P2 | 分发信任，但外部依赖重 |

\* ⑥ 分数（2.00）高于 P0 里的 ④（1.80），但 **W=3** 不满足 P0 的 `W ≥ 4` 门槛——它的价值在"杠杆"（让 Agent 能驱动、顺带成为其他项的自动化验证手段），不在用户痛点权重，故列 P1 首位并注明理由，**不硬塞进 P0**。

## 4. 快速通道（小项，成本 ≤2，随时可插）

全部来自 2026-09 实测/验证中**真实发现**的缺口，不解决它们会持续制造小的体验摩擦或文档漂移。

| ID | 事项 | W | D | C | V | P | 来源 |
|---|---|---|---|---|---|---|---|
| ㉓ | **编辑 GBK/非 UTF-8 源文件**：至少给出**中文**错误提示（当前是英文 IO 错误） | 3 | 3 | 2 | 2 | **1.50** | P0-① 遗留：`read_file` 严格 UTF-8，中文用户遗留文件常见 |
| ㉗ | **`.ins`/`.dtx` 源码版模板**：识别并提示"需先编译 `*.ins` 生成 `.cls`" | 2 | 3 | 2 | 3 | **1.25** | ⑲：GitHub 源码版（thuthesis/BUCTthesis/hithesis）需 `xelatex *.ins`；实测 `bithesis-doc.tex` 是 DTX 无法直编，佐证这类"看着像主文件其实不是"的坑 |
| ㉒ | **点 PDF 目录区会打开生成的 `.toc` 文件**：改为忽略或回落到最近 `.tex` 记录 | 3 | 2 | 2 | 2 | **1.25** | 本轮 MCP 验证实测复现（modules.md §12 有记，属已知特性但体验突兀） |
| ㉑ | **外部改 `.texpresso/settings.json` 时同步 `ProjectState.root_file`** | 3 | 2 | 3 | 2 | **1.00** | ②-1 同源缺陷的另一触发路径（`git pull` 到别人的根文件设置会不生效） |
| ㉔ | **文档欠账**：`cli-mcp-plan.md` 的路径引用迁移到 `texpresso-infra` | 1 | 1 | 1 | 1 | **1.00** | ADR-0010 后 §1.3 仍写 `src-tauri/runner.rs` / `src-tauri/storage.rs` |
| ⑳ | **正向 SyncTeX 高亮**：补目视验证并常态化 | 2 | 1 | 1 | 3 | **0.75** | 唯一的验证债：`webview_interact` 不支持修饰键点击，需 pc-control |

> 说明：⑳ 分数最低但仍列出——它是**验证债**而非功能项，不做则 P0-①「SyncTeX 双向」的"正向"始终缺一次目视确认。

## 5. 分档详述（未完成项）

### 5.1 ②-2 引擎推断 —— ❌ **已砍掉**（见 §6 不做）

⑲ 调研的结论：**19 个真实模板里 0 个因默认 XeLaTeX 选错**，`\RequirePDFTeX` 为 0/7374，文档口径 19/19 兼容 XeLaTeX。三种引擎推断覆盖不到真正需要别的引擎的场景（日文模板需 platex）。**该项不解决任何已观测到的问题**，故移入不做清单，其预算转向 ㉕/㉖ 与 ④ 扩充。

### 5.2 ④ 错误诊断升级 —— ✅ **已完成**（见 §1 基线）

19 类 `DiagnosisKind` + `ErrorEntry.diagnosis` 契约 + `ErrorList` 两行式展示；语料与期望表见 `crates/texpresso-core/src/log_parser/{real_error_corpus,diagnosis_tests}.rs`。**它同时是 ②-2 的正确替代**：不猜引擎，但猜错时明确说"该文档需要 XeLaTeX，到设置里切换"。

### 5.3 ③ 性能基准与回归基建（**须含完整学位论文档**）

- **做什么**：**脚本生成**基准工程（小/中/大/多文件/重图 + **完整学位论文**六档，避免入库大文件）+ 一条命令产出「冷编译 / 增量编译 / PDF 重载（fetch·parse·render）/ 端到端延迟 / **首编总耗时**」报告，对照 [design.md](../design.md) §延迟预算。
- **为什么加学位论文档**：⑲ 实测 hithesis/hitszthesis 首编 >90s（已产出 596 KB PDF），这是 ㉕ 判定的依据。
- **顺带**：把本轮用手做的 MCP 真机验收步骤固化成清单（无需代码），作为回归手段。
- **DoD**：一条命令出报告；超预算标红（CI 或本地检查）。
- **风险**：大文档样本需自己合成（不可分发他人论文）。

### 5.3 ㉕ 学位论文首编超时（**实测驱动，③ 已完成，可直接动手**）

- **问题**：③ 实测——**真实模板 `thesis-real-hithesis` 冷编译 >240s 未收敛**（另一次观察 >600s），而默认 `compile.timeout_secs = 120` → **必然超时失败**。合成 `thesis` 档 9.4s 只是"学位论文规模"的下界，真实模板才是上界（⑲ 已见其单趟 27s、多趟 + splitindex 累计极高）。
- **做什么**：① 复核默认超时值（或按文档规模给建议值——现在已有六档数据可依据）；② 超时提示要能区分「真超时（死循环）」与「文档较大、需要更长超时」，并给"提高超时后重试"的一键操作；③ 顺带核对 `noop`/触发链路 ~0.5s 的地板是否可接受（③ 测得）。
- **DoD**：真实模板在调高超时后能编译成功；超时提示可操作。
- **依赖**：③（**已完成**）。

### 5.5 ⑤ SyncTeX 可靠性加固

- **做什么**：多文件/`\include` 嵌套、beamer、未加载页、大文档坐标偏移；`synctex(busy)` 竞争重试；失败时降级提示。**并把 ㉒（目录区映射到 `.toc`）一并考虑**——两者同属"点击 PDF 的映射语义"。
- **DoD**：multifile + beamer + 大文档三组样本的正反向成功率与"跳到位"比例记录在案。
- **验证**：MCP 点 PDF → 断言「编辑器文件 + 行号」（本轮已验证该断言可行）。

### 5.6 ⑥ CLI + MCP 接口

- **现状**：计划与清单见 [cli-mcp-plan.md](../cli-mcp-plan.md)（P0/P1 优化点与落地顺序已定）；**ADR-0010 之后 headless 复用更直接**（core + infra 都不依赖 Tauri）。
- **杠杆价值**：既是差异化能力（Agent 可直接驱动「读 → 改 → 编译验证 → 修」），也是其他项的**自动化验证手段**。
- **注意**：`compile_now` 目前是 fire-and-forget，`compile --wait` 需要包装（见 cli-mcp-plan §4 P0-1/2）。

### 5.7 P1 其余项（原样保留，理由未变）

⑦ 大文档编辑器侧性能（依赖 ③）｜⑧ 跨文件语义操作（**建议与 ⑨ 合并成"语义层"里程碑**，避免两套索引）｜⑩ 深色主题（Monaco + Monarch + pdf.js 反色需一起调）｜⑪ 预览滚动/缩放保持｜⑨ LSP（先做 monaco-languageclient 技术验证再排期）。

### 5.8 P2 / 后置

⑫ TinyTeX（C/V 最高，独立里程碑）｜⑬ AI（**无一手需求证据，先观察**）｜⑭ 拼写检查｜⑮ 多窗口多项目｜⑯ 外部查看器｜⑰ 自动更新｜⑱ 代码签名。

## 6. 明确不做（否决项）

| 项 | 理由 |
|---|---|
| **实时协作** | P=0.40；在线方案护城河，本地方案投入产出不成立。定位「个人 / 离线优先」，协作交给 Git |
| **增量编译** | [ADR-0005](../adr/0005-latexmk-first-incremental-next.md) 已实测证伪：latexmk 增量＝整份单遍重排，属 xelatex 引擎上限 |
| **自研 PDF 渲染引擎** | pdf.js 已是够用底座；LaTeX Workshop 的教训是"预览是二等公民"，不是"必须自研渲染器" |
| **自研完整 LaTeX AST/解析器** | 复用 texlab/现有解析；⑧⑨ 的语义层建在可维护索引上，不重写 TeX 解析 |
| **推断 bib 工具链**（原 ② 的一部分） | 实测 latexmk 自己就会跑 `bibtex` 与 `biber`（`biblatex/biber in use` → exit 0），无需推断也无需用户配置 |
| **②-2 引擎推断**（2026-09 由 ⑲ 实测砍掉） | ⑲ 调研：**0/19** 个真实模板因默认 XeLaTeX 选错；`\RequirePDFTeX` **0/7374**；文档口径 19/19 兼容 XeLaTeX。唯一"两种引擎都不行"的是日文 `jsarticle`（需 platex）——**不在三引擎推断范围内**。该项解决的是一个不存在的问题；原预算转入 ㉕/㉖ 与 ④ 扩充。见 [template-corpus-survey.md](./template-corpus-survey.md) |

## 7. 批次与出口条件

| 批次 | 内容 | 出口条件 |
|---|---|---|
| **Batch 1 收口与地基** | ㉕ 首编超时复核（③ 已完成，直接可用六档数据）→ 快速通道 ㉑–㉔、㉗ | 学位论文默认超时行为有实测记录且可操作；小项闭环 |
| **Batch 2 攻痛点** | ⑤ SyncTeX 加固 → ㉖ latexmkrc 交互 → ⑦ 大文档编辑器侧性能 | SyncTeX 三组样本有记录；带 latexmkrc 的模板编译路径有结论；大文档打字不掉帧 |
| **Batch 3 基建与硬骨头** | ⑥ CLI+MCP → ⑦ 大文档编辑器侧性能 → ⑧⑨ 语义层（合并） → ⑩ 深色主题 / ⑪ 预览状态保持 | Agent 能驱动「改→编→验」；大文档打字不掉帧；跨文件重命名可用；主题一致 |
| **独立里程碑** | ⑫ TinyTeX 捆绑 | 干净 Windows 机器零预装可用 |
| **验证债（可随时插）** | ⑳ 正向 SyncTeX 目视 | 补齐 P0-① 的"正向"确认 |

## 8. 与现有文档的对应（含文档欠账）

| 本文件项 | 现有文档位置 | 状态 |
|---|---|---|
| ① 中文路径 | troubleshooting.md「中文文件名/路径兼容性实测」 | ✅ 已完成 |
| ②-1 根文件候选 | modules.md §8 命令表 / §9.4 组件表 / §10 契约 / §12 | ✅ 已完成 |
| ⑲ 模板语料调研 | [template-corpus-survey.md](./template-corpus-survey.md) + [cn-thesis-template-engines.md](./cn-thesis-template-engines.md) + `scripts/tl-compile-matrix.ps1` | ✅ 已完成 |
| ④ 错误诊断 | design.md §错误列表（已补诊断说明）+ modules.md §12 + `log_parser/diagnosis.rs` + `scripts/gen-log-error-corpus.ps1` | ✅ 已完成（19 类，23 例真实语料） |
| ③ 性能基准 | design.md §「基准脚本与 2026-09 一轮实测」+ `scripts/{gen-bench-projects,bench}.mjs` + troubleshooting「真机验收清单」 | ✅ 已完成（六档 + 可选真实档；一条命令出报告） |
| ②-2 引擎推断 | design.md §后置清单「引擎语言自适应规则」 | ❌ **已砍掉**（⑲ 实测无反例）；design.md 该后置项应同步标注"经实测否决" |
| ③ 性能基准 | design.md §延迟预算（benchmark 未提交，且 `test_file/projects/` 已 gitignore） | 部分存在，**改为脚本生成**，并加学位论文档 |
| ⑤ SyncTeX | ADR-0008 + modules.md §5 | 已实现，需加固 |
| ⑥ CLI+MCP | [cli-mcp-plan.md](../cli-mcp-plan.md) | 计划任务，未实现 |
| ⑦ 大文档性能 | modules.md §12（大纲重扫优化点） | 优化方向已记录 |
| ⑫ TinyTeX / ⑮⑯⑰⑱ | design.md §后置/未决清单 | 后置 |
| **㉔ 文档欠账** | [cli-mcp-plan.md](../cli-mcp-plan.md) §1.3 仍写 `src-tauri/runner.rs`、`src-tauri/storage.rs` | **过期**（ADR-0010 已迁 `texpresso-infra`） |

## 9. 不确定性与待补数据

1. **W 打分是二手推断**：调研全部来自公开社区数据，**没有一手用户访谈/问卷**。若要做真实取舍，建议先补 5–10 个目标用户（研究生 / Git 型作者）的访谈。
2. **成本 C 未经工程估算**：C 是分析者基于当前代码结构的判断 `[推断]`，落地前应由实现者复核（尤其 ⑦⑧⑨）。
3. **付费意愿未验证**：只找到"换工具"意愿，没有付费证据——路线若依赖商业化需单独验证。
4. ~~**②-2 的收益未知，直到 ⑲ 出数**~~ —— **⑲ 已出数：反例为 0，②-2 砍掉**（详见 [template-corpus-survey.md](./template-corpus-survey.md) §0/§3）。
5. **AI 集成的权重可能被低估**：2025–2026 竞品集中投入，但缺乏用户需求的一手证据，故仍列 P2；市场信号增强需重排。
6. **V 的重估依赖"MCP 会话在场"**：MCP 驱动断言需要应用在 debug 下运行 + 会话连接，**不是 CI 可跑的自动化**；V=2 表示"半脚本化、确定性断言"，不等于"进 CI"。
7. **⑲ 的样本代表性有限**：19 个模板取自 TeX Live `doc/`，且已发现 1 例取样错误（`bithesis-doc.tex` 是 DTX 文档而非用户模板）。要把"开箱即用率"当长期指标，应改为脚本维护一份**真实模板集**并纳入 ③ 的基准。
8. **㉖（latexmkrc 交互）与 ㉗（`.ins`/`.dtx`）均未实测终态**：前者的两个样本都超时、后者测的是已安装版，结论来自静态阅读，需实现前先验证。
