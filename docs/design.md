# TeXPresso 设计文档

> 项目状态：**已实现并迭代中**（Windows 首发 MVP：项目/编辑/编译调度/错误去重/连续 PDF 预览 + SyncTeX/设置页均可用）。
> 术语见根目录 [CONTEXT.md](../CONTEXT.md)，决策记录见 [ADR 目录](./adr/)，分层/接口/技术栈见 [architecture.md](./architecture.md)，函数级设计与实现契约见 [modules.md](./modules.md)。

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
| **超时**（单次 >120s，可配置，上限 1800s） | 队列有等待 → 直接执行等待条目；无等待 → **立即失败，不重试**：错误列表给一条证据化诊断（为什么慢 / 疑似卡住 + 怎么办）+ **一键「提高到 Ns 并重试」**（判据见 [modules.md](./modules.md) §4.2） |
| **内容错误**（源码问题，进程非零退出） | 不重试；队列有等待 → 直接执行最新版本；无 → 展示错误信息 |
| **手动终止**（用户点停止） | 终止运行中编译 + **清空队列**中的等待条目 |

**超时为什么不重试**：同一份源码在同一个上限下重跑几乎必然再次超时（TeX 运行确定性强），代价却是**再等一个完整超时窗口**——默认 120s 下用户要等 240s 才看到任何提示。所以「重试」不进调度器，而是变成用户可见、可调参的动作：立刻失败 + 证据化诊断 + 一键提高上限。诊断的证据与判据见 [modules.md](./modules.md) §4.2。

### 错误列表

- 编译中清空 → **编译还没结束就能看见致命错误**（阶段 2 流式反馈，2026-09；只报致命错误，`Overfull` 这类警告留到终态）→ 失败展示最新完整清单（含警告）→ 点击跳转源码行
- 依据 .log 解析（结构化错误提取）；实时那一路尾随 `tmp/<stem>.log`（引擎按页 flush），管道 stdout 只作补充（非 TTY 下 4KB 块缓冲）
- **诊断**：把原始报错翻译成「原因 + 怎么改」——22 类（缺包/缺类/缺文件/缺字体/字体集/引擎不匹配/不支持的引擎/语法与结构类/超时两类/写不出中间文件/宏包与 LaTeX 兜底），匹配不到则退回原文 + 行号（**不猜**）。这是对"错误信息不可读"这一行业最高频痛点的正面回应。**不做引擎自动推断**：19 个真实模板双引擎实测中 **0 例**因默认 XeLaTeX 选错，正确替代是"不猜，但选错时明确告诉用户怎么改"（见 [research/template-corpus-survey.md](./research/template-corpus-survey.md)）。契约与语料见 [modules.md](./modules.md) §4.1。

### 延迟预算（产品约束，验收标准）

- **小文档**（一次完整编译 ≤2s，典型：期刊论文、笔记）：1s 优秀 / 2s 及格
- **大文档**（一次完整编译 >5s，典型：毕业论文、书稿）：3s 优秀 / 5s 及格

### 延迟预算实测与结论（2026-08，本机 TeX Live 2026，xelatex）

| 文档 | 冷编译 | 增量(编辑一个文件) |
|---|---|---|
| 小（article，15 行） | 1.53s | 1.50s |
| `multifile`（原 `000test`，**单文件前身**，重测前） | 4.06s | 2.02s |
| 重多文件（ctexbook+hyperref+toc+公式，20 章） | 3.74s | 2.29s |

> 注：上表 `multifile` 行测的是该工程**单文件版**（重构为多文件 + 跨文件引用工程之前），仅供量级参考；现行 `test_file/projects/multifile/` 是 ctexbook + `\include` 组织的 15 页多文件工程，数值见下方基准表。

**结构性结论（关键）**：latexmk 的"增量" = 对**整份文档**重跑一次 xelatex 单遍；它优化的是"跑几遍"（引用/`\bib` 多次 pass、`\ref` 未变就少跑），**不是跳过未改动文件**。编辑任何子文件都触发整份重排——这是 **xelatex 引擎特性**，自研驱动无法突破。→ **确认暂不过 latexmk**（见 ADR-0005）。

对照预算：中小文档增量 1.5–2.3s，落在"大文档 3s 优秀"线内/附近；小文档接近及格线；**真正大文档（数百页/重图/bib）单遍必然超预算，属引擎上限**（后续文档预算说明需如实标注）。

### 基准脚本与实测基线（roadmap ③：性能基准与回归基建）

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
2. **三档小文档全部超预算**，且按本文的分类口径（"一次完整编译 ≤2s"）本机**已几乎不存在"小文档"**——连 6 行的 `tiny` 冷编译都要 2.3s。→ **预算口径待复核**：是放宽小文档预算，还是改为"相对基线/回归容忍度"（例如以 `tiny` 的 1.6s 为地板、其他档按倍数判定）。数据已就位（见 [modules.md](./modules.md) §12.1）。
3. **真实学位论文远超产品默认超时**：`thesis-real-hithesis` 冷编译 **>240s 未收敛**（另一次观察 >600s），而默认 `compile.timeout_secs = 120` → **必超时失败**（合成 `thesis` 档 9.4s 是"论文规模"的下界，真实模板才是上界）。
   - **该模板的阻塞点不是慢，是报错**：用产品完全相同的命令重跑，它在 ~4 分钟后报 `! I can't write on file 'body/introduction.aux'`（`\include{body/...}` 需要 `tmp/body/` 存在，而它不存在）→ `Emergency stop`，随后 **latexmk/perl 进程挂住（10 分钟零 CPU）**。所以阻塞点是**模板自带 latexmkrc × 本产品 `-outdir=tmp` 约定**（roadmap ㉖）——调高超时也编不过；">240s 未收敛"的记录需在 ㉖ 修好后重测（见 [modules.md](./modules.md) §12.1）。
   - **超时上限按真实论文规模放宽**：`compile.timeout_secs` 范围 **5..=1800**（上限 600 是"调到顶也编不过"的死路）。

> 复现性：两轮独立运行的关键档差异 <5%（tiny edit 1617→1625ms、multifile edit 2461→2463ms），脚本可作回归基线。

#### 附：latexmk 开销拆解与「编辑期单趟」收益（roadmap ㉘）

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

**落地形态**（实现契约见 [modules.md](./modules.md) §2.4 / §2.6 / §9.3）：

- 编辑触发走 `CompileKind::Quick`（直调 `xelatex -interaction=nonstopmode -synctex=1 -output-directory=tmp`，不经 latexmk）；首编、手动「编译」、空闲收敛走 `CompileKind::Full`（完整 latexmk）。
- **无构建产物自动升级**：`tmp/<stem>.aux` 不存在时单趟会让引用全成 `??`，runner 探测后自动升级为 Full，并把**实际**执行的强度回传（升级趟不误报草稿）。
- **空闲收敛**：成功且为草稿后停手 **2000ms**（任何编辑或新编译都取消）补一次 Full，把目录/交叉引用页码追上；期间状态栏显示琥珀色「引用待更新」，只由下一次成功且为 Full 的编译清除（失败与运行中都不清——屏幕上的 PDF 没变）。
- **编译中反馈**（阶段 2，2026-09）：状态栏显示「已排版 N 页」（来自引擎 `[N]` 标记，只升不降——多趟重启不回跳）；真机实测 400KB / 162 页 ctexbook 首编 `1.7s 起 running → 3.4–4.8s 页数 2→162 → 8.2s success`。预览仍只在编译成功后刷新（编译期无中间 PDF，见 [阶段 2 报告](./research/stage2-streaming-feasibility.md) 与 [G2 报告](./research/g2-byte-offset-resync.md)）。
- **真机证据**（tauri server MCP，`multifile`）：一个 cycle 内 就绪 → 排版中 → 就绪 +「引用待更新」→ **Δ≈2.0s** 后再次排版中（收敛的 Full）→ 就绪、提示消失；`tmp/main.fdb_latexmk` 由收敛那一趟更新（晚于 Quick 趟）而 Quick 趟不动它——确证收敛真的跑了 latexmk、Quick 真的没跑。
- **未验证**：bib/biber 场景下编辑期单趟的可用性（现有 fixture 编辑期不触发 bibtex；收敛兜底应能覆盖，但没有实测结论）。

#### 预览重载实测（`multifile`、31 页 `benchmark`）

**口径**：编辑 → PDF 刷新 ≈ 防抖 500ms + 编译 + pdf.js 重载；`fetch/parse/render/total` 由 `window.__previewLastReload` 插桩读出，其中 `render` = 本次挂载窗口渲染链全部落盘后的**真实 canvas 绘制耗时**（不是 setup 时间）。

| 场景 | fetch | parse | render | total | 备注 |
|---|---|---|---|---|---|
| `multifile` 同文件重载（3 页 / 108KB） | 9–10ms | 30ms | 59ms | **98–100ms** | 复用 canvas，`pagesRendered=2` |
| `multifile` 首次换文档 | 6ms | 29ms | 77ms | **112ms** | `pagesRendered=3` |
| 31 页 `benchmark` 同文件 | — | — | 21–28ms | **62–69ms** | 对比全量挂载版：render 49ms / total 89ms / `pagesRendered` 9→2 |

- **render 是 PDF 重载开销主因**（占总耗时 ~59%）：视口重绘 + 二次 `renderNearViewport`（分页虚拟化前还含整页 canvas DOM 重建）。分页 DOM 虚拟化把 DOM 节点从 31 降到 ~7 个 canvas（**~4.4× 减少**），同文件内容重载复用 canvas、仅缩放/换文档才重建——契约见 [modules.md](./modules.md) §9.4。31 页那行的 render/total 下降含「渲染页数变少（9→2）」因素，DOM 减量才是无歧义收益。
- 3 页文档总耗时 ~100ms 远低于延迟预算；更早一次 12 页 `multifile` 的 `render≈320ms / total≈400–450ms` 是旧插桩数值，与上表不可直接比较。
- `test_file/projects/benchmark/` 为基准工程（未提交）。

### 构建确定性（roadmap ㉚：为"输出 diff / 只重排变化页"打地基）

**问题**：TeX 的 PDF 写入器会在 trailer 放一个 `/ID`，其取值与**时钟**有关（PDF 里本来不写 `/CreationDate`）。不处理时"同一份源码编译两次"字节不同，任何基于字节比较的增量渲染都分不清"真改了"还是"ID 抖了"。

**实测（`node scripts/check-determinism.mjs`，三档样本 × 两条编译路径，各连跑两次、间隔 5s）**：

| 场景 | 不固定 `SOURCE_DATE_EPOCH` | 固定 `SOURCE_DATE_EPOCH=0`（现在的默认） |
|---|---|---|
| `multifile` Full（latexmk） | ✗ 不一致（长度 95445 vs 95443） | ✓ 逐字节一致 |
| `multifile` Quick（直调 xelatex） | ✗ 不一致（首差 @92143，共 67 字节） | ✓ 逐字节一致 |
| `beamer工程` / `bench/thesis` × 两条路径 | ✗ 不一致 | ✓ 逐字节一致 |

**结论与副作用（都已实测）**：runner 给两条编译路径都注入 `SOURCE_DATE_EPOCH=0` 后，六个组合全部逐字节一致；该变量**不影响 XeTeX 的 `\today`**（`\typeout{TODAYMARK=\today}` 仍打印构建当天日期），也**不会**让 PDF 多出 `/CreationDate`——所以固定 epoch 只掐掉 `/ID` 的时钟抖动，不污染用户文档的日期语义。

**边界**：文档**内容**里含日期（`\date{\today}`）时，跨天编译字节当然会变——那是内容变了，不是构建不确定性。

## 预览

- **内嵌 PDF 面板**（pdf.js）：编译成功后自动刷新、保持滚动位置（v1 必须）
- **SyncTeX 双向**（v1）：源码 Ctrl+点击 → PDF 高亮；PDF 点击 → 跳回源码
  - **只跳真实源码**：`synctex edit` 在生成内容上会返回生成它的中间文件（实测点目录区得 `tmp/main.toc:15`）。产品行为：**不打开生成文件**，先在附近小范围探测（y ±40/80pt）**回落到最近的真实源码**并提示"已回落"；仍无源码则忽略跳转、只提示（"此处来自自动生成的文件 main.toc…"）。项目外文件（`article.cls` 等）同理给提示。命令层策略与纯函数分类见 [modules.md](./modules.md) §5。
  - **同步提示可见**：反向/正向失败、以及"没有可跳转源码"都在预览工具条给一句提示（5s 自动消失）——否则用户看到的是"点了没反应"（错误只在 `console.error` 里）。
  - **竞争重试**：编译进行中 `.synctex.gz` 正被重写（引擎先写 `main.synctex(busy)` 再改名），失败按 100/200/300ms 退避重试。
  - **实测精度（`node scripts/synctex-report.mjs`，三组样本 34 个样本点）**：正向 **34/34**、反向 **34/34**、往返同文件 **34/34**；往返行号差 ≤3 行 **31/34（91.2%）**、≤5 行 **34/34**。分档：`large`（125 页）差恒为 **0**、`multifile` **0–1 行**、`beamer` **2–4 行**（帧边界 `\begin{frame}`/`\label` 处最松）。**取块启发式已排除**：first / 最小 H / "文本行"三种取块规则给出**完全相同**的往返结果 → 保留"取第一个完整块"，beamer 的 2–4 行偏移是 beamer 记录粒度的固有现象。
- 外部 PDF 查看器支持：后续版本

## 工具链

- **系统 TeX Live 优先**；缺失时检测并引导安装
- TinyTeX 内嵌兜底：后续实现
- **默认引擎 xelatex**；后续按系统语言自适应（中文 → xelatex，其他 → pdflatex），引擎可配置

## 编辑器

- **v1（已实现）**：语法高亮（自研 Monarch）、查找替换、错误跳转、**中文 IME 兼容验证**（Windows 首发，webview 组合输入是已知坑，实操要点见 troubleshooting）
  - **非 UTF-8 源文件（roadmap ㉓）**：打开时**明确拒绝并给中文提示**（"用记事本/VS Code 另存为 UTF-8…"），状态栏红色提示条显示，不开标签页。**不做 lossy 打开**：编辑器保存会把替换字符写回磁盘，等于静默损坏用户文件。此前是英文 IO 错误 + 无人接的 rejection（表现为"点了文件没反应"）。
- **语言特性（已实现）**：环境块折叠（`\begin{env}`/`\end{env}`，含嵌套）、多光标（Alt+点击/Alt+方向，Ctrl+D 加选；修饰符用 alt 以免与 SyncTeX 的 Ctrl+点击冲突）、代码片段（LaTeX CompletionItemProvider，覆盖文档骨架/环境/章节/数学/格式/文件操作，Tab 展开）。解析与折叠语义见 [modules.md](./modules.md) §9.5
- **v1.1（规划）**：texlab LSP、拼写检查

## MVP 边界

**MVP 包含**：项目打开 + 文件树 + 标签页 + 编辑（高亮/查找替换）+ 双模式编译 + 调度全套（队列/超时/手动终止/错误分类）+ 错误列表跳转 + 内嵌预览（含 SyncTeX）。

**MVP 后未做（规划中/后置）**：LSP（v1.1）、外部查看器、TinyTeX 兜底、自动更新、代码签名证书。已落地项见 [roadmap §1 基线](./research/tex-ide-roadmap-priority.md)。

**已否决**：引擎语言自适应（见文末「后置/未决清单」）｜ 增量编译（latexmk 的"增量" = 整份文档重跑单遍，属引擎上限，见 ADR-0005）。

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

多窗口与多项目 ｜ 外部查看器 ｜ TinyTeX 捆绑 ｜ 自动更新 ｜ 代码签名证书 ｜ 冲突对话框（v1 以状态栏「外部修改」点击重载替代，独立对话框后置）｜ 多面板布局 ｜ LSP 具体集成（v1.1 规划，monaco-languageclient 需专项研究）

**已否决**：引擎语言自适应规则（19 个真实模板双引擎实测中 **0 例**因默认 XeLaTeX 选错、`\RequirePDFTeX` 为 0/7374、文档口径 19/19 兼容 XeLaTeX——"引擎自动推断"解决的是不存在的问题；唯一两种引擎都不行的是日文 `jsarticle`，需 platex，不在三引擎范围内。见 [research/template-corpus-survey.md](./research/template-corpus-survey.md)）｜ **DVI/XDV 作为预览格式**（2026-09 实测：产物→屏幕比"xdvipdfmx + pdf.js"慢 **57–68×**——121 页 44.06s vs 0.65s+115ms；XDV 体积不定（0.15×–29× PDF）；未写完的 DVI 无 postamble 直接解析失败。唯一值得吸收的是"编译期页完成度进度信号"，见 [research/dvi-preview-feasibility.md](./research/dvi-preview-feasibility.md)）｜ 增量编译具体策略（latexmk 增量 = 整份单遍重排，属引擎上限，见 ADR-0005）
