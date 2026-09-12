# TeXPresso 设计文档

> 项目状态：**已实现**（Windows 首发 MVP 落地：项目/编辑/编译调度/错误去重/连续 PDF 预览+SyncTeX/设置页均可用；迭代中，实测与优化结论见下）。
> 术语见根目录 [CONTEXT.md](../CONTEXT.md)，决策记录见 [ADR 目录](./adr/)，分层/接口/技术栈见 [architecture.md](./architecture.md)，函数级设计见 [modules.md](./modules.md)。

## 产品定位

本地运行的 TeX **完整 IDE**（桌面 GUI 应用），核心设计因素是**实时编译**：文档随用户编辑自动持续编译，无需手动触发。

- 平台优先级：**Windows 首发 → Linux → macOS**
- UI 语言：中文 v1，文案抽离（i18n 结构预留），开源后补英文

## 技术栈

- **Tauri 2**：Rust 后端 + webview 前端（后端负责进程监督、编译调度，前端负责编辑与展示）
- **Vue 3 + TypeScript + Vite**（前端框架；无历史包袱，中文生态）
- **Monaco**（代码编辑器）、**pdf.js**（内嵌 PDF 预览）
- **texlab LSP**：规划项（v1.1，`\ref`/`\cite` 补全、hover、跳转定义）；检测系统安装优先，缺失则首次启动引导下载（存应用数据目录）

## 项目模型

- **文件夹即项目**：打开文件夹 = 打开项目
- **根文件**：自动探测（含 `\documentclass` 的顶层 .tex），可手动覆盖；`\input`/`\include` 指向的文件为子文件
- 多文件项目 v1 支持；**任何 .tex 文件变化都触发编译**（监视范围 = 项目内全部 .tex，排除 tmp/）
- **中间文件**（.aux/.log/.toc/.out）→ 项目内 `tmp/` 目录（latexmk `-outdir=tmp`，进 .gitignore）
- **PDF 产物** → 项目根目录、与根文件同级（编译成功后从 tmp/ 拷贝）
- **配置两层**：全局默认值（用户目录）+ 项目级 `.texpresso/settings.json`（可进 git）

## 编译子系统（核心）

### 触发

- 双模式可切换：**连续编译**（默认，输入停顿防抖 ~500ms 即触发，优先实现）+ **保存触发编译**
- 手动编译按钮（第三种触发）

### 调度：合并队列

- 运行中的编译**执行完毕、不中断**
- 待编译条目**最多一个，总是最新**（合并语义）
- 最大可见延迟 ≈ **2× 完整渲染时间**
- 已否决"杀旧启新"：反复中断会让渲染迟迟不落地（见 ADR-0001）

### 失败语义（三类）

| 情形 | 处理 |
|---|---|
| **超时**（单次 >120s，可配置） | 队列有等待 → 直接执行等待条目（跳过重试）；无等待 → 自动重试一次；两次均超时 → 展示错误信息 |
| **内容错误**（源码问题，进程非零退出） | 不重试；队列有等待 → 直接执行最新版本；无 → 展示错误信息 |
| **手动终止**（用户点停止） | 终止运行中编译 + **清空队列**中的等待条目 |

### 错误列表

- 编译中清空 → 失败展示最新错误 → 点击跳转源码行
- 依据 .log 解析（结构化错误提取）
- **诊断**（2026-09，roadmap ④）：把原始报错翻译成「原因 + 怎么改」——缺包/缺字体/引擎不匹配/语法类等 19 类，匹配不到则退回原文 + 行号（不猜）。这是本产品对"错误信息不可读"这一行业最高频痛点的正面回应；**替代了原「引擎自动推断」后置项**（⑲ 实测：默认 XeLaTeX 在 19 个真实模板里选错 0 次，无需推断，只需在用户选错时明确提示怎么改）。见 [modules.md](./modules.md) §12。

### 延迟预算（产品约束，验收标准）

- **小文档**（一次完整编译 ≤2s，典型：期刊论文、笔记）：1s 优秀 / 2s 及格
- **大文档**（一次完整编译 >5s，典型：毕业论文、书稿）：3s 优秀 / 5s 及格

### 延迟预算实测与结论（2026-08，本机 TeX Live 2026，xelatex）

| 文档 | 冷编译 | 增量(编辑一个文件) |
|---|---|---|
| 小（article，15 行） | 1.53s | 1.50s |
| `multifile`（原 `000test`，**单文件前身**，重测前） | 4.06s | 2.02s |
| 重多文件（ctexbook+hyperref+toc+公式，20 章） | 3.74s | 2.29s |

> 注：`test_file/projects/multifile/`（原 `000test/`）已重构为**多文件 + 跨文件引用**工程（ctexbook、`\include` 组织 chapters/sections，15 页），上表 `multifile` 行的数值为重构前的单文件版测量，仅供量级参考；多文件工程编译与引用解析已在 [modules.md](./modules.md) §12 / e2e 中验证。

**结构性结论（关键）**：latexmk 的"增量" = 对**整份文档**重跑一次 xelatex 单遍；它优化的是"跑几遍"（引用/`\bib` 多次 pass、`\ref` 未变就少跑），**不是跳过未改动文件**。编辑任何子文件都触发整份重排——这是 **xelatex 引擎特性**，自研驱动无法突破。→ **确认暂不过 latexmk**（见 ADR-0005）。

对照预算：中小文档增量 1.5–2.3s，落在"大文档 3s 优秀"线内/附近；小文档接近及格线；**真正大文档（数百页/重图/bib）单遍必然超预算，属引擎上限**（后续文档预算说明需如实标注）。

### 基准脚本与 2026-09 一轮实测（roadmap ③：性能基准与回归基建）

**工具**（fixture 不入库，入库的是脚本——`test_file/projects/` 已 gitignore）：

```bash
node scripts/gen-bench-projects.mjs        # 生成六档（纯文本无二进制资产，任意机器可复现）
node scripts/bench.mjs                     # 一条命令出报告；超预算退出码 1
node scripts/bench.mjs --tiers=thesis --samples=5
node scripts/bench.mjs --with-real         # 追加真实模板档（依赖本机 TeX Live 样例）
```

口径严格对齐产品：cwd=项目根、`latexmk -xelatex -outdir=tmp -synctex=1 -interaction=nonstopmode`，不加产品没传的参数；每档 3 次取中位数；先 warm-up 一次排掉文件系统冷缓存与字体缓存。**端到端 = debounce(500) + edit + preview(真机实测值)**。

**实测（2026-09，本机 TeX Live 2026，xelatex，Windows）**：

| 档 | 内容 | 冷编译 | 空跑(无改动) | 编辑触发 | 端到端(估) | 判定 |
|---|---|---:|---:|---:|---:|---|
| `tiny` | article 6 行 | 2307ms | 500ms | 1625ms | 2225ms | **FAIL**（小文档 2s 及格） |
| `small-article` | 期刊论文（图表 + bibtex） | 6476ms | 502ms | 2593ms | 3193ms | **FAIL** |
| `multifile` | ctexbook + `\include` 20 章 | 6465ms | 495ms | 2463ms | 3063ms | **FAIL** |
| `graphics` | 30 个 TikZ 图 + 长表 | 2631ms | 499ms | 1826ms | 2426ms | EXCELLENT（大文档预算） |
| `large` | 约 300 页纯文本 | 13532ms | 486ms | 4104ms | 4704ms | PASS（大文档 5s 及格） |
| `thesis` | ctexbook 学位论文（目录/公式/bibtex） | 9412ms | 507ms | 3143ms | 3743ms | PASS |
| `thesis-real-hithesis` | 真实模板样例（可选档，需 `--with-real`） | **>240s 未收敛** | — | — | — | 不计入判定 |

**真机预览（tauri server MCP，同一 `multifile` 档：46 页 / 78KB）**：`fetch 11ms / parse 39ms / render 64ms / total 115ms / pagesRendered=7`
→ **真实端到端 = 500 + 2463 + 115 = 3078ms**（脚本用 `--preview=100` 估算 3063ms，偏差 15ms，说明估算口径可用）；render 占预览耗时 **56%**，仍是主因。

**三条结论**：

1. **瓶颈在小文档的固定开销，不在大文档**。`tiny`（6 行）与 `large`（约 300 页）的编辑触发只差 2.5×（1625ms vs 4104ms），而内容量差两个数量级——说明 ≈1.5s 的固定开销（latexmk 启动 + xelatex 启动 + 字体加载）在主导小文档；`noop`（latexmk 判定最新、什么都不做）也要 **~0.5s**，这是任何触发的地板。
2. **三档小文档全部超预算**，且按 design.md 的分类口径（"一次完整编译 ≤2s"）本机**已几乎不存在"小文档"**——连 6 行的 `tiny` 冷编译都要 2.3s。→ **预算口径需复核**：是放宽小文档预算，还是改为"相对基线/回归容忍度"（例如以 `tiny` 的 1.6s 为地板、其他档按倍数判定）。本轮不做结论，但数据已就位。
3. **真实学位论文远超产品默认超时**：`thesis-real-hithesis` 冷编译 **>240s 未收敛**（另一次观察 >600s），而默认 `compile.timeout_secs = 120` → **必超时失败**。这是 roadmap ㉕ 的直接输入（合成 `thesis` 档 9.4s 是"论文规模"的下界，真实模板才是上界）。

> 复现性：两轮独立运行的关键档差异 <5%（tiny edit 1617→1625ms、multifile edit 2461→2463ms），脚本可作回归基线。

#### 附：latexmk 开销拆解与「编辑期单趟」收益（roadmap ㉘ 评估，2026-09）

`node scripts/bench-single-pass.mjs`（六档 × 3 次、A/B 交替消序）测「完整 latexmk」vs「单趟 xelatex」：

| 档 | 完整 latexmk | 单趟 xelatex | 节省 |
|---|---:|---:|---:|
| `tiny` | 1625ms | 1119ms | 31.1% |
| `small-article` | 2683ms | 1320ms | 50.8% |
| `multifile` | 2437ms | 1761ms | 27.8% |
| `graphics` | 1878ms | 1315ms | 30.0% |
| `large` | 5307ms | 2361ms | 55.5% |
| `thesis` | 3370ms | 1816ms | 46.1% |

**平均 40.2%，6/6 达标 → 值得做**（阈值预设 20%）。

**省在哪（已核实）**：捕获 latexmk 实际命令是 `xelatex -no-pdf … -output-directory="tmp" main.tex` **然后** `xdvipdfmx -E -o "tmp/main.pdf" "tmp/main.xdv"`——它把「LaTeX 趟」与「转换」拆成两个规则，而直接 `xelatex` 一趟内部完成转换。**两者工作量本质相同**，省下的是 **latexmk 外层机制（perl 启动 + `.fdb_latexmk` 依赖库读写）+ 其判断出的收敛趟**。这也解释了上文 `noop ≈ 0.5s` 的地板——纯 latexmk 开销。

**代价（已量化）**：单趟的目录/交叉引用页码**落后一趟**。实测 `multifile` 插 400 行后单趟运行，`.toc` 里「第四章」页码由 **29 → 41**，即该趟 PDF 显示的是旧值 29；再跑一趟 `.toc` 稳定（「单趟旧、两趟收敛」）。

**已实现（2026-09，roadmap ㉘）**：编辑触发走 `CompileKind::Quick`（直调 `xelatex -interaction=nonstopmode -synctex=1 -output-directory=tmp`，不经 latexmk）；首编 / 手动「编译」/ 空闲收敛走 `CompileKind::Full`（完整 latexmk）。**实测落点**：
- **无构建产物自动升级**：Quick 请求但 `tmp/<stem>.aux` 不存在 → runner 自动升级为 Full（否则单趟引用全是 `??`）。集成用例 `quick_upgrades_to_full_without_artifacts`（真实引擎）断言升级后 `.log` 无 `There were undefined references`。
- **状态事件报"实际执行强度"**：`CompileStatusDto.draft` 由 `CompileOutcome::Success { kind }` 决定，故首编被升级为 Full 时**不会**误报草稿（否则前端会多提示一次"引用待更新"并多跑一次无意义收敛）。
- **空闲收敛**：前端 `useIdleConvergence` 在「成功且是草稿」后 **2000ms** 无编辑（任何编辑/新编译都取消）时调 `compile_now` → Full。延时取 2s 而非沿用 500ms 防抖：调度器合并队列只留一个待办，正在跑的 Full 会把紧随其后的编辑态 Quick 堵在队列里（大项目 Full ≈4s），反而拖慢"编辑→出图"。
- **真机时间线（tauri server MCP 驱动，`multifile`）**：就绪 → 排版中(t=1980) → 就绪+**引用待更新**(t=6780) → 排版中(t=8786，Δ=**2006ms**，即空闲收敛的 Full) → 就绪、提示消失(t=12169)。同一 cycle 内 `tmp/main.fdb_latexmk` 由收敛那一趟更新（晚于 Quick 趟），证明收敛**真的跑了 latexmk**、不是空转跳过。手动「编译」日志为 `手动编译` + `Full 编译（完整 latexmk 收敛）`。
- **UI**：状态栏在草稿期间显示琥珀色「引用待更新」；`queued/running/failed` 不改该标记（屏幕上的 PDF 仍是旧的，失败不产出新 PDF）。
- **Quick 真的不经 latexmk**（集成用例断言）：跑完 Quick 后 `tmp/main.fdb_latexmk` 的 mtime **不变**。

**未验证**：bib/biber 场景下单趟的可用性（现有 fixture 编辑期未触发 bibtex；收敛兜底应能覆盖，但未见实测）。

**端到端延迟**（编辑 → PDF 刷新 ≈ 防抖 500ms + latexmk + pdf.js 重载）：`multifile` 重载实测 `fetch≈7ms / parse≈103ms / render≈320ms / total≈400–450ms` → **render 占 ~75% 是瓶颈**（`canvasEpoch` 整页 canvas 重建 + 视口重绘 + 二次 `renderNearViewport`）。**A/B 优化已落地**（分页 DOM 虚拟化 + 同文件重载复用 canvas、仅缩放/换文档才重建）——见 [modules.md](./modules.md) §12。

**A/B 优化后真实窗口复测（2026-08-25，tauri server MCP 驱动）**：`npm run tauri dev` + `VITE_TEXPRESSO_PROJECT=…/test_file/projects/multifile` 自动开项目 → 点「编译」→ `main.pdf`(3 页/108KB) 重载。**像素级视觉确认通过**（标题页/目录/正文正常，无黑屏/文字反转——此前受限点已解决）。**插桩修正**：把 `render` 从 setup 时间改为等挂载窗口渲染链落盘后的真实 canvas 绘制耗时（原 `pagesRendered` 恒 0）。**实测**：同文件复用 `fetch≈9–10ms / parse≈30ms / render≈59ms / total≈98–100ms / pagesRendered=2`；首次换文档 `render≈77ms / total≈112ms / pagesRendered=3`。**render 占总耗时 ~59%，仍为 PDF 重载开销主因**（一致结论）；3 页小文档总耗时 ~100ms 远低于延迟预算（先前 `render≈320ms/total≈400–450ms` 是 12 页文档数值，非同比）。

**受控 A/B 对比（2026-08-25，同一 31 页 `benchmark`）**：重构前全量挂载 31 canvas，重构后虚拟化只挂 ~7 canvas（**DOM 节点 ~4.4× 减少**）；同文件复用路径 `render 49→21–28ms / total 89→62–69ms / pagesRendered 9→2`。DOM 减量为无歧义收益；render/total 下降含「渲染页数变少」因素，但同文档总耗时仍明显下降。`test_file/projects/benchmark/` 为基准工程（未提交）。

## 预览

- **内嵌 PDF 面板**（pdf.js）：编译成功后自动刷新、保持滚动位置（v1 必须）
- **SyncTeX 双向**（v1）：源码 Ctrl+点击 → PDF 高亮；PDF 点击 → 跳回源码
- 外部 PDF 查看器支持：后续版本

## 工具链

- **系统 TeX Live 优先**；缺失时检测并引导安装
- TinyTeX 内嵌兜底：后续实现
- **默认引擎 xelatex**；后续按系统语言自适应（中文 → xelatex，其他 → pdflatex），引擎可配置

## 编辑器

- **v1（已实现）**：语法高亮（自研 Monarch）、查找替换、错误跳转、**中文 IME 兼容验证**（Windows 首发，webview 组合输入是已知坑，e2e 有实操要点见 troubleshooting）
- **v1.1（规划）**：texlab LSP、~~折叠~~（**已实现** 2026-08-26：`\begin{env}`/`\end{env}` 环境块折叠，见 modules.md §12）、~~多光标~~（**已实现**：Alt+点击/Alt+方向，Ctrl+D 加选；多光标修饰符设为 alt 避免与 SyncTeX Ctrl+点击冲突）、~~代码片段~~（**已实现**：LaTeX CompletionItemProvider，覆盖文档/环境/章节/数学/格式，Tab 展开）、拼写检查

## MVP 边界

**MVP 包含**：项目打开 + 文件树 + 标签页 + 编辑（高亮/查找替换）+ 双模式编译 + 调度全套（队列/超时/手动终止/错误分类）+ 错误列表跳转 + 内嵌预览（含 SyncTeX）。

**MVP 后（已完成/规划）**：~~增量编译~~（**已实测**：latexmk 增量=整份文档单遍重排，属引擎上限，**暂不过 latexmk**，见 ADR-0005）、~~设置页~~（已完成 v1）、~~错误列表去重/截断~~（已完成）、~~文件树增量刷新~~（已完成：内容修改跳过）、LSP（v1.1 规划）、折叠/多光标、外部查看器、TinyTeX 兜底、引擎语言自适应、自动更新。

## 分发

- **GitHub Releases + NSIS 安装包**；**不签名**（SmartScreen"未知发布者"警告，风险由用户自行承担）
- 仓库：**MVP 前私有，之后开源**
- 自动更新：内置（tauri-updater），**不强制**

## 测试与质量底线

- **调度器单元测试**：队列合并/超时/重试/手动终止语义（全项目最容易写崩的核心）
- **.log 解析器快照测试**：真实 latexmk 日志固化为用例
- **Windows 手动验收清单**：打开 → 编译 → 报错 → 修改 → 恢复 → 预览刷新全链路
- **前端单测**：vitest（project 路径归一化 / editor 自保存过滤与冲突 / useAutoSave 防抖），组件测后续补

## 已确认的实现细节（默认如此，可随时推翻）

- 监视范围：项目内所有 .tex（含子文件，排除 tmp/）
- 连续模式默认开启，防抖 ~500ms（由延迟预算倒推）
- latexmk 用 `-outdir=tmp`，成功后把 PDF 拷贝到项目根
- 错误列表时机：编译中清空 → 失败展示 → 点击跳转

## 后置/未决清单

多窗口与多项目 ｜ 外部查看器 ｜ TinyTeX 捆绑 ｜ ~~引擎语言自适应规则~~（**已实测否决** 2026-09：19 个真实模板双引擎实测中 **0 例**因默认 XeLaTeX 选错、`\RequirePDFTeX` 为 0/7374、文档口径 19/19 兼容 XeLaTeX——"引擎自动推断"解决的是不存在的问题；唯一两种引擎都不行的是日文 `jsarticle`（需 platex），不在三引擎范围内。见 [research/template-corpus-survey.md](./research/template-corpus-survey.md)）｜ 自动更新 ｜ 代码签名证书 ｜ ~~增量编译具体策略~~（**已实测**：latexmk 增量=整份单遍重排，属引擎上限，暂不过——见 ADR-0005）｜ LSP 具体集成（v1.1 规划，monaco-languageclient 需专项研究）｜ ~~冲突对话框~~（**v1 以状态栏「外部修改」点击重载替代**，独立对话框后置）｜ ~~多面板布局~~（**后置**）｜ ~~页码跳转~~（**已实现** 2026-08-25：预览工具条输入页码跳转；并修复 SyncTeX 正向定位未加载页无法一次跳到位——见 modules.md §12）｜ **CLI + MCP 交互接口（计划任务，2026-08 记录，仅记录待实现）**：目标让 harness/Agent（如 DeepSeek Harness）不经 GUI 直接调用 TexPresso 的编译/预览/SyncTeX 能力——详见 [cli-mcp-plan.md](./cli-mcp-plan.md)（现状盘点、工具清单、P0/P1 优化点与落地顺序）
