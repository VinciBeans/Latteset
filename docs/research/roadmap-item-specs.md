# Roadmap §6 逐项详报（编号不变）

> 本文是 [tex-ide-roadmap-priority.md](./tex-ide-roadmap-priority.md) **§6 的原文搬移**（2026-09 拆分：正文一字未改，只加本页头与下表）。
> 排序来源仍是 roadmap §3（待办总表）与 §4（批次）；已完成基线见 roadmap §1。**编号不变**（§6.1–§6.15），roadmap 及其它文档里的 `§6.x` 指针一律指本文。

## §6.x 顺序表（按正文出现顺序）

| 出现序 | 编号 | 标题（正文原文） |
|---|---|---|
| 1 | §6.1 | ㉖ 模板自带 latexmkrc 与构建约定的交互（P1，硬证据已就位） |
| 2 | §6.2 | ⑦c 多文件大项目 —— ✅ 已完成（夹具 + 口径 + 数据，结论：**不需要优化**） |
| 3 | §6.3 | ⑧⑨ 语义层（建议合并为一个里程碑） |
| 4 | §6.4 | ⑩ 深色主题 ✅（已完成）/ ⑪ 预览状态保持 |
| 5 | §6.5 | ⑫ Tectonic 库形态集成（独立里程碑；原「TinyTeX 捆绑」2026-09 改判） |
| 6 | §6.6 | ㉞ 流式出图：编译期把"已完成页"变成部分 PDF（P1；**产品负责人指定为下一项**） |
| 7 | §6.7 | ㉟ 窗口按钮融入应用界面（无边框 + 自绘顶栏；**产品负责人点名，未排期**） |
| 8 | §6.8 | P2 其余（一行带过） |
| 9 | §6.9 | 编译期并行与"白等"：串行点清单、实测与判据（2026-09-16） |
| 10 | §6.10 | ㊷ 库形态下**根文件**的 SyncTeX 定位不可用（2026-09-17 发现 → 同日侦察完毕 → **同日落地**） |
| 11 | §6.11 | ㊸ 悬停公式即时预览（TeXStudio 式内联浮层；2026-09-17 产品负责人点名入表） |
| 12 | §6.12 | ㊹ 侦察：把项目导言区固化成 format（2026-09-17；**范围更正：本节结论只对非目标引擎成立**） |
| 13 | §6.13 | ㊺ 软件内新建文件/目录（2026-09-17 产品负责人点名入表） |
| 14 | §6.15 | 文件管理补齐：删 / 改 / 查（㊼㊽㊾，2026-09-21 产品负责人点名立项） |
| 15 | §6.14 | ㊻ 引擎清单动态化 + 默认引擎切 Tectonic（2026-09-17 产品负责人点名） |

> **编号说明**：§6.12 在正文里写成 `####` 级（原文如此，未改标题级别），本表按出现顺序收录；§6.13 / §6.15 / §6.14 是后续追加项，按追加顺序排在文末，故出现顺序与编号大小不一致 —— 本轮**只加表、不重排正文**。
> **旧行号引用**：凡引用 `tex-ide-roadmap-priority.md:NNN`（NNN ≥ 223，即原 §6 内的行号）者，内容现在本文：**本文行 = 原行 − 192**（例：原 `roadmap:244` → 本文 `:52`；核对锚点：原 §6.5 标题 = 原 282 → 本文 90）。

---

## 6. 待办详述

### 6.1 ㉖ 模板自带 latexmkrc 与构建约定的交互（P1，硬证据已就位）

- **硬证据**：`thesis-real-hithesis`（TeX Live 自带样例）用产品完全相同的命令冷编译，~4 分钟后报 `! I can't write on file 'body/introduction.aux'` → `Emergency stop`，随后 `latexmk`/`perl` **挂住不退出**（实测 10 分钟零 CPU）——产品侧因此看到的是「超时」而不是「内容错误」。
- **已知机制**：`\include{子目录/文件}` 要写 `tmp/子目录/文件.aux`，xelatex **不创建输出目录的子目录**；latexmk 一般会在第一趟失败后建目录并重跑（合成 `multifile` 档无事），hithesis 这一档没有恢复。
- **嫌疑**：该模板自带 latexmkrc（覆写 `$pdflatex`、内建 `--shell-escape`、尾部 `;cp`）与 `-outdir=tmp` 的相互作用。
- **✅ 触发条件已定位（2026-09-17）**：模板 rc 里写着 **`$preview_continuous_mode = 1;`** —— 那是 latexmk 的 **`-pvc`（连续预览模式）**：编译完（或报错后）**都不退出**，停在那里等文件变化 ⇒ 我们的 runner 一直等 ⇒ 产品侧看到的是**超时**而不是内容错误 ✓ 与当年"10 分钟零 CPU"完全吻合。
  **判决实验**（含同一份 rc 的临时目录 + 一个 3 行小文档，`latexmk -xelatex -outdir=tmp`）：文档**编译成功**（`main.pdf` 2723 B）却在 25 s 后**仍在运行**（CPU 累计 0.3 s），输出写着
  `Latexmk: I have not found a previewer that is already running. So I will start it for 'tmp/main.pdf'` → `Running 'start start "tmp/main.pdf"'` → `=== Watching for updated files. Use ctrl/C to stop ...`
  ⇒ ① 不退出 = pvc；② **还会顺手拉起外部 PDF 预览器**（`start` 一个 `tmp/main.pdf`）——这在用户机器上是可见的副作用。
- **修法（已定，待实现 + 真夹具验证）**：
  1. **命令行覆盖 rc 的 pvc**：`latexmk -e '$preview_continuous_mode = 0;'`（`-e` 在读完 rc **之后**执行 ⇒ 与我们已经用过的 `-e '$biber=0;…'` 同一机制；实测该机制在本机可用）；
  2. **预建输出子目录**：`\include{body/…}` 要写 `tmp/body/*.aux`，而 xelatex 不建子目录 ⇒ 按项目 include 图预建 `tmp/<子目录>/`（我们本来就有 include 图/扫描器），让那句 `I can't write on file 'body/…aux'` 根本不发生；
  3. 顺带：`;cp %D %R.pdf` 这类 rc 尾部命令在 Windows 上依赖 `cp`（Git 的 usr/bin 有才行）——**不改它**（属模板自由），但失败要能从日志里看出来。
- **✅ 修法已实现（2026-09-17，`crates/latteset-infra/src/runner.rs`）+ 两条机制验证通过**：
  1. `latexmk -e '$preview_continuous_mode=0;$pdf_update_method=0;'`（常量 `LATEXMK_RC_NEUTRALIZE`；**刻意不留空格** —— 带空格的参数要靠调用方正确加引号，不留空格就与引号无关）；
  2. `ensure_include_subdirs()`：起 latexmk 前扫根文件的 `\include{…}`/`\input{…}` 字面量（含反斜杠的宏名跳过），预建 `tmp/<子目录>/`。
  **验证**：① 探针（`test_file/e2e/latexmkrc-fix/probe`，同一个 hithesis rc + 3 行小文档、argv 与产品一致）⇒ latexmk **1453 ms 正常退出**、产物齐（`main.pdf`/`main.xdv`/`main.synctex.gz`）、末尾 `All targets … are up-to-date`（**不再**出现 `Watching for updated files` / `start start "tmp/main.pdf"`）；对照：不带该参数的同一探针 25 s 仍在跑 ✓。② 合成工程 `\include{sub/x}` 走我们的 CLI `Full`（latexmk）⇒ `status=success`、`tmp/sub/x.aux` 与项目根 `main.pdf` 都落位 ✓。单测同步更新（`full_command_uses_latexmk_with_engine_flag` 现在钉住这两个参数；infra **39 例全绿**）。
- **真夹具实测（2026-09-17，`test_file/research/tl-compile/hithesis_哈工大___xe`）**：
  - **挂住已消除（核心结论）**：同一个夹具、同一份文档，只差我们那两个参数 ——
    **不带** `-e` 中和：20 s 后**仍在跑**，输出停在 `=== Watching for updated files. Use ctrl/C to stop ...`（= pvc 挂住）；
    **带**产品 argv：**3 s 退出（exit 12）**并给出真实错误 ⇒ 产品从此看到的是**内容错误**而不是 120 s **超时** ✓
    ⇒ 出口条件的第二条（"**或**产品给出明确可操作的诊断"）成立 ✓。
  - **"模板能编译成功"在本机不成立，且与 ㉖ 无关**：该模板要 `ctexbook` 的 `windowsnew` 字体集（本机没有）、报 `Command \Bbbk already defined`、且 `hithesis.cls` 不在测试 bundle 里 ⇒ 属**环境/模板**问题（缺字体集、包冲突），不是构建约定问题。
  - ⚠ 因此本轮**没有**在真夹具上走到 `\include{body/…}`（它在到达 include 之前就失败了）⇒ 预建子目录这条只在**合成工程** `\include{sub/x}` 上证过（见上）；③ 的 `>240s 未收敛` 复测对 hithesis **不适用**（它在本机根本编不过），要换一个能编过、且带 rc 的模板再测。
- **✅ 回归夹具已入库（路 B，2026-09-17）**：`test_file/e2e/latexmkrc-fixture/` —— 自带 `latexmkrc`，**只复刻咬过我们的两件**：`$preview_continuous_mode = 1`（= `-pvc`）与 `$pdflatex = "… %O %S;cp %D %R.pdf"`（`%O` 带 `-outdir`、尾部 `;cp` 复刻 hithesis 写法）；正文用 `\include{body/intro}` 走子目录 include。
  **判据（两臂、秒级、确定性）**：① 用**产品 argv**（含 `-e` 中和）⇒ `exit=0`、2 s、`tmp/main.pdf` 与 `tmp/body/intro.aux` 都落位 ✓；② **不带**中和 ⇒ 15 s **仍在跑**（输出停在 `=== Watching for updated files. Use ctrl/C to stop ...`）✓ ⇒ 同一条夹具既钉"修好了"也钉"没修会挂"。
  ⚠ 本夹具里 `tmp/body/` 是 latexmk 自愈式补建的（所以它**验不了**我们的预建）⇒ 预建子目录改由单测 `include_subdirs_are_precreated_under_outdir` 单独钉（含"宏名不猜""顶层 include 不造目录"两条边界）；infra 单测 **40 例全绿**。
  ⚠ 一并记下 rc 尾部 `;cp %D %R.pdf` 在本机的行为：**没生效**（项目根 `main.pdf` 未产出；Windows 上没有 `cp`，属模板侧写法，我们不改）。
- **㉖ 收口（2026-09-17）**：出口条件按第二条（"**或**产品给出明确可操作的诊断"）达成 —— 真夹具从"挂到超时"变为"3 s 返回真实错误"；回归由上面的夹具 + 单测双钉。
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

### 6.4 ⑩ 深色主题 ✅（已完成）/ ⑪ 预览状态保持

- **⑩** ✅ **已完成（2026-09-15）**：Monaco + 自研 Monarch 语法 + 外壳三处一起调（Candy Desk 语系的暗色版，落成"墨紫罗兰纸面 + 抬亮的糖果色 + 硬阴影翻暗投影"）。配置类第一痛点（206 票 / 23 万浏览）。**落地要点**：token 两层（通道变量 + 角色 token，共 59 个）单点定义/单点覆盖；`ui.theme = light | dark | system` 走设置模型（默认 light ⇒ 升级外观不变）；首帧不闪；Monaco 因只吃具体色值而需要独立的深浅两套主题。**对比度实测与过程中修掉的"实底强调 + 白字 2.4:1"见 §1 的 ⑩ 行与 [design.md](../design.md) §界面主题。**
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

- **形态 A 已落地（2026-09，提交 `f69bad2`）**：`crates/latteset-tectonic/`（路径 B：自持 `IoProvider` + 自建 format 趟）已能**真编出中文 PDF**（1 页 / 7590 B，文本层可抽回），**release 实测与子进程同速**：冷缓存 1943 ms（format 1495 + 排版 335 + 转换 114）、热缓存 372–642 ms，对照 `tectonic.exe -r 0` 1857 / 395–458 ms；缓存隔离成立（format 落产品缓存目录、项目目录无 `.fmt`）。默认关闭（`tectonic-lib` 特性 + `LATTESET_TECTONIC_LIB=1`），**失败不回退**。方案与已落地状态见 [tectonic-lib-evidence.md](./tectonic-lib-evidence.md) §6.1（2026-09 自 [tectonic-library-plan.md](../tectonic-library-plan.md) 迁出；原文此处是指向 library-plan §6.1 的坏链 `../../`，本次一并修正），决策记录 [ADR-0012](../adr/0012-tectonic-library-form-engine.md)。**仍未做**：bib 趟 / 页哈希 / 多趟收敛 / 常驻复用（P4）。

**代价（先算清再动手）**：拖进 **C 依赖**（harfbuzz 需子模块源码；Windows 官方路线是 vcpkg + 他们的 `cargo-vcpkg` 分支）⇒ CI 明显变重；要重划 **ADR-0010** 的边界（C 链接进哪一层）；引擎与宿主**同进程** ⇒ 崩溃/取消语义要重设计（隔离从"杀子进程"变成"自己不能崩"）。官方 MSVC 二进制**只能**当子进程用。

**准入判据（已全部实测判决，2026-09）**：① **常驻收益仍无法判定** —— t7 实测库形态对击键路径**无可证实改善**（PDF 后端腿 CLI 279 ms vs 库内 307 ms），唯一实测收益是常驻复用 **−19%~−26%**（折算单次暖编 6–9%），故 P4（常驻）闸门保持关闭；② **"官方按页 flush"不成立，但"页事件"成立** —— C 侧 16 KB `dvi_swap`、9 处 flush 全是 `rust_stdout`，driver 先跑完 TeX 再转换 ⇒ 逐页 flush 记为**交上游的需求项**；改用**自持输出层 + `XdvParser::parse(chunk)`** 后 16 KB 即已完成 4 页、243 KB 26 页（滞后 <8 KiB）；③ **"内存 XDV 前缀"不成立、"整份"成立** —— 整份 XDV 同进程 → PDF 与 CLI **四维等价**（243,012 B → 74,389 B / 26 页），但截断前缀一律 `Something is wrong. Are you sure this is a DVI file?`（XDV 必须有 postamble）。⇒ 实时路径的形状 = **自持输出层 + 增量解析**，不是"喂前缀"。

**与"实时预览"的关系**：200 ms 交互**已由前端草案层独立解决**（提交 `564f39d`，实测改动→草案可见 31–34 ms），**不依赖本里程碑**；库形态买到的是"草案层更准（真字形坐标，而不是从 PDF 文本层反推）+ 纠偏更快（去 I/O、去进程启动）"。

判据与摩擦点见 [tectonic-test-plan.md](./tectonic-test-plan.md)、[tectonic-integration-plan.md](../archive/tectonic-integration-plan.md) §2（2026-09 归档）；成本实测见 [realtime-preview-cost.md](./realtime-preview-cost.md) §6。属"环境引导 + 实时能力"性质，**按独立里程碑推进（必要时另立 ADR），不塞进 Batch 3**。

### 6.6 ㉞ 流式出图：编译期把"已完成页"变成部分 PDF（P1；**产品负责人指定为下一项**）

**一句话**：不改显示格式（仍是 PDF + pdf.js），改成"**编译中就有图**"——每完成若干页就把**当前页前缀**缝成一份**部分 PDF** 交给预览，用户看到页数一页页长出来；编译结束由权威 PDF 收口。

**为什么现在能做而去年不能**：旧评估的成本锚点是**外部 `xdvipdfmx` 进程** 0.65–0.94 s/次（且当年只在未入库的一次性脚本里做通过）。库形态 runner 落地后，同一个转换在**进程内**完成、XDV 本来就在内存里，实测 **92–131 ms/次**（31/63/94/125 页前缀，页数全对）。数据与复现命令见 [DVI 报告 §10](./dvi-preview-feasibility.md)。

**落地现状（2026-09-16）**：

- **切片 1 已入库**（`d994691`）：core 合成器 `latteset_core::xdv::synthesize_postamble` + `complete_pages`（+6 单测，含"回指指针必须落在 `post` 上"的守门用例）；Rust 端与 JS 参考实现 `scripts/xdv-partial.mjs` 的字节/页数逐例一致。
- **切片 2 已落地**：`crates/latteset-tectonic/src/preview.rs` —— 在**趟边界**出图（重跑循环末尾、确定还会再跑一趟时才调）+ 三条闸门（第 1 趟 ≥500 ms / 页数有涨 / 总次数 ≤8）。`bench/large` 冷编实测：**1.72 s 出 121 页**（转换 130 ms）、**4.2 s 出 125 页**（175 ms）、权威 PDF **7.4 s** ⇒ **提前约 5.5 s 可见**，代价 **+326 ms / 总墙钟 +4.4%**（7482 → 7783 ms，A/B 同夹具）。
- ⚠ **设计修正（原方案被实测推翻）**：立项时写的"旁观线程按节拍看捕获表出图"**不成立** —— 引擎有**进程内全局锁**（`tectonic_bridge_core` 的 `static ENGINE_LOCK`，`with_global_lock` 在整个 C 引擎调用期间持有；上游理由是 C 侧 `setjmp/longjmp` 跨 FFI 是 UB），排版趟与转换**不可能并行**。探测（`crates/latteset-tectonic/examples/engine-lock-probe.rs`，24 逻辑核）：单次 130 ms；**4 线程并发墙钟 533 ms == 单线程跑 4 次 541 ms**（每次 132/265/398/530 = 排队序号整倍数）。实测那版旁观线程的帧落在终态 PDF 之前约 0.1 s（读者什么也看不到）。详见 [DVI 报告 §11](./dvi-preview-feasibility.md)。
- **切片 3 已完成**（2026-09-16）：独立事件 `compile-preview`（`{path, pages}`）—— core trait `CompileProgress::partial_pdf` → `TauriProgress` → specta 事件/bindings（`src/bindings.ts` 的 `events.compilePreview`）。**真机时间线**（GUI 库形态冷编 125 页，debug）：`status(running)` 14:14:20.5 → **`compile-preview` 121 页** +4.99 s → **125 页** +9.94 s → `pdf-updated`(125 页, changed=125) +14.92 s → `status(success)`；中间态严格早于权威事件、`pages` 单调、权威载荷**未被污染**。<br>⚠ 契约里的坑（已写进 `modules.md` §4）：部分 PDF 的**路径与权威 PDF 不同**，而 `PreviewPane.load()` 按 `path === lastDocPath` 决定是否重建 DOM ⇒ 前端不得把它当"当前文档"塞进同一个槽位（否则每次换帧都重建 DOM、丢滚动位置）。
- **切片 4 已完成**（2026-09-16）：前端「编译中预览」——`load()` 把**身份**（`docIdentity`，判 DOM 复用）与**字节来源**（`sourcePath`，编译中 = 部分 PDF）分开 ⇒ **换字节不换身份**；`canReuse` 关掉复用（部分 PDF 的页来自中间趟，与权威 PDF 的 `changed_pages` 不可混用）；`compile-status` 终态与权威 `pdf-updated` 都收口（首编在预览态里失败/中止也不会把中间态留在屏幕上）；工具栏加「编译中预览 · N 页」（只在真收到帧时出现）。**真机实测**（`bench/large` 125 页冷编，debug）：3 次加载 85/79/78 ms；**14 个页 DOM 节点跨"预览 121 → 预览 125 → 权威 125"全部存活**（无重建）；滚动 **11839 帧 min=max**（零漂移）；**预览帧 0 long task**（全程唯一 1 条 69 ms 出现在首次权威全量重绘，属既有行为）；**中止后 106 ms 退回权威 PDF**。
- **㉞ 四刀齐活**（切片 1–4 各自提交、各自真机验证）：剩余仅"趟内逐页"这一项能力边界（需另起进程，见上）。

**最窄切片（第一步只做这些）**：
1. ✅ **前缀来源 = 库形态的内存捕获表**（`crates/latteset-tectonic/src/io.rs` 的逐块钩子；已消费）；
2. ✅ **Rust 侧实现合成器**（把 `scripts/xdv-partial.mjs` 的规则搬进来 + core 单测）：`post` 头（改 `last_bop`/页数 t）→ 字体定义（`define_native_font` 252 + `fnt_def` 243–246）→ `post_post`（指针 + 1 字节 id=7 + `0xDF` 对齐）；
3. ✅ **独立事件**（`compile-preview`，**未**复用 `pdf-updated`——中间态不得污染「引用待更新」语义与页哈希基线；切片 3 真机验证）；
4. ✅ **阈值**（改成趟边界三条闸门：只在"还会再跑一趟"时出图 + 第 1 趟 ≥500 ms + 页数有涨 + 总次数 ≤8；实测总开销 +4.4%）。

**验收判据**（可确定性断言的部分优先）：
- ① 页数正确：部分 PDF 的页数 == 前缀完整页数（对照组：31/63/94/125 全对，`pdfinfo` 核对）；
- ② 时延：单次刷新（合成 + 转换）**≤ 200 ms**（release；本轮产品路径实测 **130 / 175 ms**，`open_ms=0`）；
- ③ 有界：整轮编译里的刷新次数与总开销有上限（占编译时长 ≤5%，**实测 +4.4%**），且**可取消**（与编译同一 cancellation token）；
- ④ 不影响权威路径：终态 `pdf-updated` 仍是唯一权威；页哈希缓存/A 闸门不被中间态写入（用现有页面哈希单测守）；
- ⑤ 退化可见：**Tectonic 子进程档**（PDF 档不落 XDV ⇒ 无此能力）与"少于阈值"的短编译**行为与今天完全一致**，能力位如实显示"不可用"而不是静默不生效；
- ⑥ 体感（真机）：长编译期间预览可见地逐页出现；**滚动位置零帧偏移**、**0 long task**（探针口径见 [DVI 报告 §10.4](./dvi-preview-feasibility.md)）。

**非目标**（写清免得滑坡）：
- **不做** DVI/XDV 当显示格式（自研渲染器 / dvisvgm）——§5.6 的三条理由不变；
- **不做**"编译中显示未收敛的完整文档"——部分 PDF 的目录/引用必然是 `??`，它只是**进度**，不是正确性表达；
- **不做** 引擎改造 / fork（§8 原则 1）。

**未测与风险**（立项时如实登记，落地时补）：
1. ~~与排版**并发**时的 CPU 争抢~~ → **已测并证否（2026-09-16）**：不是争抢，是引擎**进程内全局锁**，并发根本不可能（§11）；方案随之从"旁观线程"改成"趟边界"。
2. ~~**内存前缀**直接喂引擎的端到端时延~~ → **已测（2026-09-16）**：走产品路径（内存捕获表 → 合成 → 进程内转换）**130 / 175 ms**，与"从文件前缀"量级一致。
3. **"页数逐渐变多"中间态的用户接受度**（产品判断，需真机走查；也是"要不要显示进度条式骨架"的输入）；
4. 子进程 XeLaTeX 档读"正在写的 `tmp/<stem>.xdv`"的短读/占用行为（Windows 尤其）——该档不在最窄切片内。

**出口条件**（2026-09-16 达成情况）：长编译在真机上**可见逐页出现** ✅（`bench/large` 125 页冷编：121 页 → 125 页 → 权威 PDF；release 提前 ≈5.5 s、debug 提前 ≈10 s）；⑥ 两条 ✅（滚动 11839 帧 `min=max`、预览帧 **0 long task**）；**短编译**零行为变化 ✅（`passes=1` 时出图闸门一次都不跑，实测无帧、无提示）；**子进程档**：本轮切片 4 只改前端、未动任何 Rust 路径，该档不发 `compile-preview` ⇒ 派生量（`docIdentity = pdfPath`）与今天逐字相同，**未重新实跑**（如需 diff 式回归要另起一次非 lib 形态构建）；文档回写 ✅（modules §2.7/§4/§9.1/§9.2/§9.4 + DVI 报告 §11 已由"未接线"改为落地记录）。

### 6.7 ㉟ 窗口按钮融入应用界面（无边框 + 自绘顶栏；**产品负责人点名，未排期**）

**一句话**：把 Windows 那三个系统按钮（最小化 / 最大化 / 关闭）从"系统标题栏"搬进应用自己的顶栏，让最上方那条带子整体属于 Candy Desk —— 实现形态就是 `decorations: false` + 自绘顶栏。

**与 ⑩ 的关系（先看这条，免得重复劳动）**：⑩ 已经让标题栏**跟随主题**（Windows DWM 沉浸式深色，提交 `8eb260a`，深浅两个方向都实测过），所以**"颜色不搭"这条已经不成立**。本项要的是**结构**融入。三条现状差距（实测）：

| # | 现状 | 影响 |
|---|---|---|
| 1 | 系统标题栏固定约 30px 高、**不可复用** | 品牌、当前项目路径、编译强度 chip 只能挤在它下面那一条；纵向空间白扔一条 |
| 2 | 字体 / 悬停 / 圆角完全由系统决定 | 与 Candy Desk 不是一套观感（浅色尤其明显） |
| 3 | 无法把标签页或工具栏提到标题栏高度 | VS Code / Typora / Obsidian 都是这么做的；我们做不到"现代感"那一档 |

**评分与入表理由**：W2 D2 C4 V3 ⇒ P = (2+2)/(4+3) = **0.57**，**低于 0.6 的自动门槛**（按 §2 模型口径属"不做"档）。入表是因为**产品负责人点名**；因此标 `指定**`、**不进批次**。§8 原则 3「证据不足就压后」在这里被显式的产品决策覆盖 —— 记下来是为了将来能追溯"为什么它在表里但没排期"。

**技术清单与坑**（按"最容易看起来像、用起来差"排序）：

1. **Win11 贴靠布局（Snap Layouts）**：鼠标悬停最大化按钮时弹出的贴靠面板，**无边框窗口不会自动拥有** —— 要么在自绘按钮的 hit-test 里调系统菜单，要么配合 `WM_NCHITTEST`。**不做这条，用户会觉得"最大化变难用了"**（这是本项最容易翻车的一处）。**建议列为第一优先**。
2. **四边四角缩放热区**：用 `startResizeDragging(dir)` 铺 8 条边（或 `WM_NCHITTEST` 返回 `HT*`）；还要处理 DPI 100%/125% 下热区宽度、圆角遮挡、以及热区与内容点击的冲突（顶栏里的按钮不能被热区吃掉）。
3. **系统行为不能丢**：双击标题栏最大化/还原、**右键 → 系统菜单**、`Alt+Space`、`Alt+F4`、`Win+←/→` 吸附。
4. **保留系统阴影与圆角**（`shadow: true`）：否则窗口在深色桌面上会"糊"成一片，比现在更差。
5. **与 `tauri-plugin-window-state` 共存**：记住尺寸/位置仍要工作（无边框不影响，但要回归一遍最大化状态与"上次是最大化"的恢复）。
6. **多显示器 + DPI 变更**：拖到不同缩放的显示器后，按钮与热区位置要重新算对。
7. **可退化**：`decorations: true` 的原生路径**必须保留**——今天这套 DWM 方案就是它的外观保障，自绘出问题时要能一键退回。

**建议两步落地**：

- **㉟-a（最窄）**：无边框 + 拖拽区 + 三个按钮（hover/禁用态 + `aria-label`）+ 8 向缩放热区 + 双击最大化 + 右键系统菜单。**暂不承诺** Snap Layouts，但要在顶栏给等效入口（最大化按钮支持"拖到屏幕边缘吸附"以外的显式左右半屏）。
- **㉟-b**：Win11 Snap Layouts / 贴靠组（平台细节多，可行性待验证；先做技术验证再决定）。

**验收判据**（尽量客观，真机）：

1. 拖拽移动、四边四角缩放、双击最大化/还原、最大化↔还原后按钮图标状态正确；
2. `Alt+F4`、右键系统菜单、`Win+←/→` 吸附三条系统行为**不丢**；
3. 鼠标悬停最大化按钮出现**贴靠布局面板**（㉟-b 判据；㉟-a 阶段如实标注"暂缺"，不得含糊）；
4. DPI 100%↔125% 切换、跨显示器拖动后按钮与热区仍准确；
5. 键盘可达：Tab 到三个按钮、Enter/Space 触发、有可读 `aria-label`；
6. **零回归**：编译/预览/快捷键/文件树/设置面板照旧；MCP 自动化仍可用（自定义按钮在 webview 里，反而比原生按钮**更容易**自动化 —— 这是本项的附带收益，值得在验收里显式确认）；
7. 深浅两态截图：顶栏与内容同色系、没有第二条"异色带子"。

**非目标**：不做自绘阴影/圆角（交给 DWM，见坑 4）；不把标签页提到顶栏（那是更大的一项，可后续单独立项）；本轮只做 Windows（首发平台），macOS/Linux 的窗口装饰不动。

### 6.8 P2 其余（一行带过）

⑬ AI 集成（无一手需求证据，先观察）｜⑭ 拼写检查｜㉙ 未存盘 overlay（与 ADR-0007 冲突）｜⑮ 多窗口多项目｜⑯ 外部查看器｜⑰ 自动更新｜⑱ 代码签名证书。

### 6.9 编译期并行与"白等"：串行点清单、实测与判据（2026-09-16）

**为什么单列一节**：这条线不是"再加一个功能"，而是**把已经串起来的时间轴拆开**。判据只有两条：① 这段串行是**物理/语义**的，还是**实现顺序**的；② 拆开后的收益是否**实测**过（§8 原则 2）。

**结论速览**：真正的大杠杆只有一个 —— **㊴ 把编译搬进另一个进程**（引擎锁是**进程内**的）。快速通道那三项（㊱ syncTeX 失败重试 / ㊲ 首屏让位 / ㊳ 保存→开编往返）**已全部收口**：㊱ 修完（失败查询 668 → 14 ms），㊲㊳ **被实测否掉**（分别是"不挡首屏、只占 278 ms"与"往返仅 7 ms"）。另有**六条我原本怀疑、被实测否掉**的（§6.9.3），一并回写，免得后人重复投入。

#### 6.9.1 快速通道三项（C≤2、彼此独立）

**㊱ SyncTeX 失败查询的重试语义**（C=1 → ✅ **2026-09-17 落地**）

根因两处（都在本项查清）：

1. `crates/latteset-infra/src/synctex.rs::with_retry` 把**所有**错误都退避重试 —— 包括"同步数据里没有这个源文件""第 N 行没有对应位置"这类**确定性**失败：重试一万次也是同一个答案，而每次退避是 100/200/300 ms ⇒ 一次失败查询 **668 ms**；
2. `core::synctex::resolve_inverse` 会按 `FALLBACK_Y_OFFSETS` 试 **5 个 y 候选**，每个候选都白付一次 600 ms 退避 ⇒ 没有同步数据时 `inverse` = **5 × 625 ms ≈ 3.13 s**（与实测 3127–3156 ms 吻合 —— 这就是先前"inverse 是 forward 的 5 倍"的谜底）。

改法：把错误按**语义**分成四类，并在"整份同步数据用不了"时跳出候选循环：

| 变体 | 含义 | 退避重试？ | 继续试 y 候选？ |
|---|---|---|---|
| `Io` | 进程/设备层瞬时失败（CLI 起不来、退出码非 0） | ✅ | ✅（CLI 档的"没命中"也走这条，不能改） |
| `Busy` | 同步数据**正被重写**（读被占用 / gzip 半截 / 解不出页面） | ✅（㉒ 的竞争窗口） | ❌（数据级问题，换位置没用） |
| `Unavailable` | 同步数据**不存在**（引擎写 `.synctex` 是"写 `(busy)` 再改名"，**改名原子** ⇒ 读不到就是**从没编译过**） | ❌ | ❌ |
| `Parse` | **确定性**的"没有这段映射" | ❌ | ✅（这才是候选偏移的用途） |

**实测**（release、`bench/large`、**子进程档**产出的有效同步数据）：

| 场景 | 修复前 | 修复后 |
|---|---|---|
| forward 成功（有映射，第 100 行） | 16 ms | **15–21 ms**（不变） |
| inverse 成功（第 5 页） | 16–19 ms | **14–16 ms**（不变） |
| forward 确定性失败（源文件不在同步数据里） | **668 ms** | **14–15 ms** |
| forward 无同步数据 | **645–652 ms** | **10–11 ms** |
| inverse 无同步数据 | **3127–3156 ms** | **11 ms**（≈285×） |
| **瞬时竞争**（独占锁住 `.synctex.gz` 250 ms 期间发起查询） | — | **608 ms 后退避成功命中 page 8** ✓ |

⚠ **一处方法教训**：先前"成功路径 16 ms"的读数是在**库形态**的同步数据上量的，而那份数据 `forward` 其实**必然失败**（库形态的 Input 名是 `texput`，见 §6.10）—— 只量墙钟、没看载荷，把"快速失败"当成了"成功"。修好后失败变快，二者更难区分 ⇒ **凡断言"路径正常"，必须同时核对返回载荷**。

**复现命令**：`latteset-cli --project <夹具> forward <文件> <行>` / `inverse <页>`；瞬时竞争用 `[System.IO.File]::Open($gz,'Open','Read','None')` 独占锁住同步文件，另起进程查询（本机做法见 §1 ㊱ 的实测记录）。

**㊲ 首屏让位**（C=2 → ✅ **2026-09-17 定量后否决**）

**立项依据**（现在看是错的）：28 页实测索引 164–173 ms ⇒ **按页线性外推** 125 页 **0.7–0.8 s**，并假设它"排首屏渲染前面"（两者共用同一个 pdf.js worker）。

**实测否掉了两条**（dev、`bench/large` 125 页 / 5646 行、子进程档）：

| 量 | 读数 | 说明 |
|---|---|---|
| 索引自身耗时 | **278 ms** | 控制台现成插桩：`[draft] 文本行索引就绪：5646 行 / 125 页，278ms` ⇒ 旧外推**高 2.6×**（成本是**次线性**的） |
| 首屏可见 | 加载 `total=98–115 ms`、`render=55–74 ms`（8 页全渲染完） | **完成时索引仍在跑**（日志：索引比加载晚 **204 ms** 结束）⇒ 首屏没被挡 |
| 最坏情况：加载刚结束（索引仍在跑）就跳到第 60 页 | **52 ms** | 稳态跳页 **12–13 ms** ⇒ 索引的**最坏附加延迟 ≈40 ms** |

**为什么没被挡**：索引是 `void buildLineIndex(true)` 发起的（不 await），内部**逐页 `await`**、每页一次往返；加载侧的渲染请求与它**交错**进入 worker 队列，而不是排在 125 次文本抽取之后。

**结论：不改**。把索引挪到 idle 只会把这 278 ms 的占用从"加载刚结束"挪到"首次编辑时"（草案层要用它），换来的只是 ~40 ms 的最坏情况改善 —— 不划算，且会引入"编辑时索引还没就绪"的新竞态。

⚠ **本项的方法教训（比结论更值钱）**：立项用的是**线性外推**（28 页 × 4.5），实测是 **278 ms**。凡"每页成本"的估算都要**实测**再进批次 —— 这与 §8 原则 2（先量化收益再上机制）是同一件事，只是这次栽在"外推也是量化"上。

**㊳ 保存→开编链路的往返**（C=1 → **2026-09-17 定量否决**）

实测（dev、`bench/tiny`、库形态、真机；夹具已还原）：

| 段 | 含义 | 读数 |
|---|---|---|
| **Leg A** | 前端 `save_all`（IPC 往返 + 落盘） | **7 / 7 / 7 / 6 / 8 ms**（5 样本，页内 `Date.now()` 包住 `invoke`） |
| **Leg B** | FS 事件 → 忽略过滤 → 排编译 → runner 起跑 | **211 / 244 / 232 / 245 / 255 µs**（日志三元组 `watch 原始事件` → `触发编译` → `开始编译`） |
| 端到端 | 落盘 → 开编 | **≈7 ms** |

⇒ 与"≥100 ms 才值得做"的门槛**差 14 倍**：**不做**"自写盘快速通道"（那点收益连 IPC 抖动的量级都盖不住，却要引入"自写去重"的边界逻辑）。

**顺带纠正一处误解**：`debounce_ms`（当前 500 ms、可调 100–2000，`validate.rs:11`）是**自动保存**的防抖 —— `src/composables/useAutoSave.ts:16-17` 里 `setTimeout(run, debounce)`，**不是**编译侧的合并窗口。所以"编辑 → 开编"的主导项就是它，而它是有意设计（一次编辑常伴随多处写盘）；真要更快只能让用户把它调小，不需要新机制。

**复现方法**（将来重测用）：`npm run tauri dev` 起应用并打开 `bench/tiny` → 页内 `invoke("save_all", …)` 写一个**未被编辑器打开**的项目内 `.tex`（避免外部改动弹窗）→ 在 tauri dev 的 stdout 里取三元组时间戳；Leg A 用页内 `Date.now()` 包住 invoke（⚠ MCP 的 `webview_execute_js` 有执行超时 ⇒ 别在脚本里 `sleep`，一次调用写一次）。

#### 6.9.2 需要新基建的两项

**㊴ 子进程编译通道**（P2，C=5；唯一的大杠杆）

- **为什么只有这条路**：`tectonic_bridge_core` 的 `ENGINE_LOCK` 是**进程内**静态锁（源码依据 + 4 线程探测见 [DVI 报告](./dvi-preview-feasibility.md) §11）⇒ 同进程内排版与转换**不可能并行**；跨进程不受它约束。
- **实测**（2026-09-16、release、库形态、两个**不同**项目、共享同一份缓存目录）：两进程同时各做一次单趟编译 → **各 2248 / 2307 ms**（单独跑 1716–1737 ms，即 **+30%**）；串行基线 3500 ms ⇒ **真并行**。
- **前置三件套**（上一轮实测踩出来的，缺一条就会出事故）：
  1. **输出与 tmp 覆盖**：`CompileRequest` 没有 `tmp_dir` / `pdf_dst` 字段，草稿编译会**覆盖权威 `<stem>.pdf`** 并与主编译抢同一个 `tmp/`（现场事故已复现一次）；
  2. **缓存只读**：tectonic 缓存**没有进程间锁**（临时名用 `process::id()`，`.lock` 不是锁 —— [tectonic-library-plan.md](../tectonic-library-plan.md) §12）⇒ 草稿进程只读缓存，format 由主进程保证已生成；
  3. **生命周期**：编辑去抖 → 起进程；新编辑 / 主编译结束 → 树杀（`infra` 已有 `kill_tree`）。
- **附带收益**：同一套基建**解锁 ㉞ 唯一剩下的能力边界**（"趟内逐页出现"必须把转换搬出进程）。
- **验收判据**：主编译墙钟增量 ≤10%（真机 ≥8 核）；**低核机器（≤4 核）默认不开**；缓存零互踩（并发 N 次后 format 与页哈希仍正确）；进程杀干净（无残留）。

**㊵ 项目打开并发**（P2，C=2）

`commands.rs::open_project` 目前顺序 `await`：项目覆盖 → 全局设置 → 根文件探测 → 文件树/大纲扫描。全是只读 ⇒ `try_join!` 并发化可省一次全树遍历。收益小、风险低，随手做（可与 ㊱㊲ 同批交付，但**不必**占快速通道位次）。

**㊶ `\include` 分章并行草稿**（P2，C=5，**依赖 ㊴**）

- 20 章论文：每章一个进程并行 + 用上一轮 aux 保页码/编号 ⇒ 草稿 ≈ 单章时间。成本锚点（实测）：分章/截断类 **65 ms + 13.2 ms/页**（[DVI 报告](./dvi-preview-feasibility.md) §11）。
- 只对用 `\include` 的文档有效：本仓夹具 `thesis`(8 处) / `multifile`(20 处) / `thesis-real-hithesis`(8 处) **全部符合**。
- **未验风险**：浮动体（延迟输出）与跨章引用在"只看一章"时的保真度 —— 必须先做一次对拍（章节片段的页哈希 vs 全篇对应页）。

#### 6.9.3 已证伪 / 不立项（回写，免得重复投入）

| 项 | 结论 | 依据 |
|---|---|---|
| "把文本行索引挪到首屏之后"（㊲ 的原方案） | **不立项**：索引真实 **278 ms**（旧外推 0.7–0.8 s 高 2.6×）且**不挡首屏**（加载 98–115 ms 完成时索引仍在跑）；最坏附加延迟 ≈40 ms | ㊲ 实测（§6.9.1）；挪到 idle 只是把占用从"加载后"挪到"首次编辑时"，还多一个竞态 |
| "自写盘快速通道"（保存后跳过 watcher 直接排编译） | **不立项**：往返只有 **≈7 ms**（Leg A 6–8 ms + Leg B 0.21–0.26 ms），门槛是 ≥100 ms | ㊳ 实测（§6.9.1）；"编辑→开编"的主导项是**自动保存防抖** 500 ms —— 有意设计、用户可调 |
| "SyncTeX 每次点击都全量解析 ⇒ 该加缓存/索引" | **否**：解析只有 **~7 ms** | 正常查询 16–26 ms，扣掉 ~10 ms 进程启动（㊱ 的表） |
| "页哈希与 XDV→PDF 转换可以并行" | **不可行**：**A 闸门**要先用页哈希决定"要不要转换" | 两条 runner 都是"先哈希后转换"（`infra/runner.rs:591→616`、`tectonic/runner.rs:892→913`）—— 看着能并行，其实有依赖，写下来免得误改 |
| "页哈希遍历可以 rayon 并行" | **不值得**：125 页 / 4.6 MB 估 **10–30 ms**（未实测） | 与噪声同量级，先测再说 |
| "`save_all` 多文件并发写" | **不值得**：通常只有 1 个文件 | — |
| "TeX 趟内并行 / 同一项目跑两个主编译" | **不做**：趟间有依赖；`tmp/` 单写者 + 调度器单飞（队列只留最新一条） | §6.6 的趟数实测；`scheduler/actor.rs:151-183,215` |

#### 6.9.4 已经并行的部分（别重复投入）

LiveFeedback 三条读任务（stdout / stderr / `.log` 尾随）· watcher 线程与编译 actor 独立 · ㉞ 预览出图在趟间隙跑（受锁约束，不是没做）· pdf.js worker 与 UI 主线程分离 · tokio 的 bundle/设置 I/O（实测 `bundle_read_ms ≈ 4.5 ms`）· 以及既有的"省一次串行"手段：format 缓存（`format_ms=0`）、页哈希跳过转换、bib 跳过。

### 6.10 ㊷ 库形态下**根文件**的 SyncTeX 定位不可用（2026-09-17 发现 → 同日侦察完毕 → **同日落地**）

**一句话（侦察后修正过范围）**：库内嵌档编译出的 `.synctex.gz` 里，**主输入的 `Input:1` 名字是 `texput`**（引擎默认 jobname，不是真实文件名）⇒ **根文件自身的内容定位不了**；而被 `\include`/`\input` 的**章节源码是正常的**（实测章节 `forward` 与子进程档逐字段一致 ✓）。**单文件项目**（如 `bench/large`）全部内容都在根文件里 ⇒ 表现为**双向全失效** —— 这正是第一版结论过宽的原因（当时只用单文件夹具测）。

**怎么发现的**：做 ㊱ 时把"失败查询"的**载荷**打了出来（此前只看墙钟），看到 `forward` 报 `Parse`："同步数据里没有这个源文件（…\main.tex）" ✗。

**三形态/夹具对照**（release、同一台机器；`multifile` = `\include` 分章工程）：

| 夹具 | 形态 | `Input:1` | 根文件 forward | 章节 forward |
|---|---|---|---|---|
| `large`（单文件） | 子进程档 | `E:\…\main.tex` ✓ | ✓ `page 8` | — |
| `large`（单文件） | **库形态** | **`texput`** ✗ | ✗ 报"没有这个源文件" | — |
| `multifile`（分章） | 子进程档 | `E:\…\main.tex` ✓ | ✓ | ✓ `page 7` |
| `multifile`（分章） | **库形态** | **`texput`** ✗ | ✗ | **✓ `page 7`，与子进程档逐字段一致** |

**根因（已定位到 C 源码行）**：`xetex-ini.c` 的 `tt_run_engine()` 在 **2945 行**就调了 `open_log_file()`，而它内部 `if (job_name == 0) job_name = maketextstring("texput");`（`xetex-xetex0.c:10874`）⇒ **jobname 先被定成 `texput`**；等到 **3872 行** `start_input(input_file_name)` 时，那句"`job_name == 0` 就用主输入名"（`xetex-xetex0.c:11014`）**再也不会触发** ✗。子进程档之所以正常，是因为 `tectonic.exe` 从命令行设过 jobname。
**Rust 侧设不了**：`tt_xetex_set_string_variable` 在 0.5.3 是**空实现**（`xetex-engine-interface.c:47`注释 "Currently unused"）✗ ⇒ 库里没有"设 jobname"的入口（这也是不走 fork 的原因）。

**修法（已用"就地打补丁"预验证 ✓）**：把同步数据里的 **`Input:1:texput` 改写成根文件的真实路径**（补在 I/O 层／runner 收尾，纯函数 + 单测）。**决定性证据**：拿一份库形态产物**不重新编译**、只做这一处字符串替换 ⇒
- `forward main.tex 100` → `{"ok":true,"page":8,"x":133.76837158203125,"y":204.46743774414062}`
- `inverse 第 8 页` → `{"ok":true,"source":{"file":"…\\main.tex",…}}`
- 两者与**子进程档逐字段相同**（子进程档同一点：`page 8, x 133.76837158203125, y 204.46743774414062`）✓✓

**为什么这个修法是对的（而不是去改引擎）**：① 我们本来就**拥有**这份字节（它在我们的捕获表里、也要镜像到 `tmp/`）⇒ 打补丁是"I/O 层的职责"；② 我们的 `tag_for_file` 本来就**宽容**（相等或**后缀包含**都算命中，`core::synctex::parse`）⇒ 补相对名也行、补绝对路径也行；③ **不改文档结构、不改引擎** ⇒ 排版结果、日志、错误行归属全都不受影响（对比之下"套一层 `\input` 包装"会多一层文件栈，风险更大）。
**要点**：补丁要同时落到**捕获表**与 `tmp/` 的镜像件；`.synctex.gz` 要 gz 往返（本机 108 KB ⇒ 毫秒级）；若哪天出现**未压缩**的 `.synctex`（`-synctex=-1` 形态）也要一并处理。

**代价重估**：C **3 → 2**（一个纯函数 + 收尾接线 + 单测 + 两形态真机对拍）；V=2（可确定性断言：同步数据的 `Input:1` + `forward`/`inverse` 逐字段一致）。

**验收判据**（同 §4 的 ㊷ 批次行）：库形态产物的 `Input:1` 是真实路径；**单文件与分章两个夹具**上，两形态的 `forward`/`inverse` **逐字段一致**；`scripts/synctex-report.mjs` 三组样本的往返跳到位率不降。

**与 ㊱ 的关系**：㊱ 修的是"失败要**快**"，本项修的是"**不该**失败" —— 别把两者混为一谈。第一版把范围写成"双向定位整体不可用"是**用单文件夹具外推**的又一次教训（与 ㊲ 同款：小样本外推 → 进表前必须实测）。

#### 6.10.1 落地（2026-09-17）

**实现**：新增 `crates/latteset-tectonic/src/synctex.rs`（模块文档记根因、备选方案与"为什么落在本 crate"）：

| 件 | 落点 | 说明 |
|---|---|---|
| 纯函数 | `patch_primary_input(sync: &[u8], real_name) -> PatchOutcome` | 三态（`Patched`/`AlreadyCorrect`/`NotApplicable`）；只改 `Input:1:` 那一行，**逐字节搬运其余内容**，行尾（`\n`/`\r\n`/末行无换行）原样保留；gzip 进 gzip 出、未压缩文本原样进出；按**行首**匹配（顺带挡住 `Input:10:`）；幂等 |
| 落点 | `patch_outputs(capture, stem, mirror_dir, real_name) -> bool` | 捕获表 + `tmp/` 镜像**两处都改**（定位读的是磁盘那份）；`AlreadyCorrect` 记 `debug!`、`NotApplicable` 记 `warn!`（缺陷回来的唯一线索）；**写盘失败不致命**（只 `warn!`，不让编译失败） |
| 接线 | `runner.rs` 的 `drop(driver)` 之后、⑧ 收尾之前 | 位于所有出口（含"PDF 为空 / 拷贝失败"的早退）**之前**；`root_file` 作为新参数传进 `run_engines`（与子进程档记的是同一个绝对路径）；`tmp_dir` 先留一份 `mirror_dir` 再交给 `TectonicIo` |
| 依赖 | `flate2 = "1"` 写成本 crate 的直依赖 | **不是新依赖**：`Cargo.lock` 里本来就有（tectonic 传递，纯 Rust miniz_oxide）；`latteset-core` 依旧不碰压缩 |

单测 **8 条**（`cargo test -p latteset-tectonic --lib synctex` 全绿；该 crate 全部 **38** 条单测亦全绿）：只改主输入行、幂等、未压缩形态、CRLF/末行无换行、空 `Input:1:`、不伪造记录（`Input:10:` 不算）、坏 gzip/空字节不 panic、**捕获表与 `tmp/` 镜像拿到同一份字节**。

**真机验证**（release CLI `latteset-cli`（`--features tectonic-lib`）+ 本地 bundle；脚本见下）：

| 问 | 结果 |
|---|---|
| 库形态 `Input:1` 是真实路径？ | **两夹具都是**（`bench/large` 8 条 Input / `bench/multifile` 167 条，首条 = 项目根下 `main.tex` 绝对路径） |
| 两形态 `forward` 逐字段一致？ | **7/7 相同**：`large` `main.tex:100/1000/5000` → `page 8/28/117`（`x 133.76837158203125`、`y 204.46743774414062`、`x 535.9402465820312`）；`multifile` `main.tex:10` → `page 39`、`chapters/ch01.tex:5` → `page 7`、`ch03:50` → `page 12`、`ch20:3` → `page 73`。**修复前**同一问法直接报"同步数据里没有这个源文件"（已用"把产物里的名字改回 `texput`"复现 ✓） |
| 两形态 `inverse` 逐字段一致？ | **9/11 相同**（`large` 8/40 与 8@300,400；`multifile` 7、7@300,400、73）。余 2 项：`large` 第 1 页 与 `multifile` 第 3 页的**目录区域** —— 子进程档归到源码行 `\tableofcontents`（= `main.tex:3` / `:7`），库形态归到生成物 `main.toc`（前端按 ㉒ 显示"来自自动生成的文件"）。**判决性反证**：把补丁后的产物里主输入名改回 `texput` 再问同样两处，答案**一字不变**（而 `forward` 立刻退回失败）⇒ 该差异由两引擎对目录盒子的记录归属决定，与本补丁无关 |
| 排版/日志/错误归属零变化？ | **逐字节相同**。做法：`git worktree` 检出 HEAD 编一个基线 CLI（共享 target 目录，7.8 s），与补丁版**各自清空 `tmp/` 跑一轮**，比 `main.pdf` SHA256 / `tmp/main.log` SHA256 / `compile` JSON（去 `elapsed_ms`）/ 同步数据**去掉全部 `Input:` 行**后的 SHA256：三夹具（`large`、`multifile`、**有错文档** `诊断测试工程`）全部一致；有错文档的 `content_error line:3 / undefined_control_sequence` 逐字不差。**唯一差异就是 `Input:1` 那一行** ✓ |
| 精度不降？ | ① `node scripts/synctex-report.mjs`：CLI 侧复现文档基线（`bench/large` **12/12**、beamer **7/10**）；② 我们的自解析在**补丁后的库形态数据**上与系统 CLI 逐点比：前向**页号 6/6 一致**（坐标差是 ADR-0013 记录的既有口径差：Δy `-11.955/0/-49.356/0/-32.989`），往返**每侧用自己前向的坐标**问自己 —— CLI 侧行差 0–1、自解析侧 0–6 且**两形态逐点相同**（即补丁没动它）。⚠ 脚本 `synctex-report.mjs` 在本机 `multifile` 组是红的（样本文件拼写与同步记录不一致 ⇒ `SyncTeX Warning: No tag`），**与本次改动无关**：该脚本只用 `latexmk` + 系统 `synctex`，不经过我们的代码 |

**验证脚本（工作区产物，未提交）**：`test_file/e2e/synctex-42-verify.ps1`（两夹具 × 两形态逐字段对拍 + 已定性差异白名单）、`test_file/e2e/synctex-42-artifacts-ab.ps1`（零变化 A/B）、`test_file/e2e/synctex-42-selfcheck.ps1`（同产物上系统 CLI vs 自解析），证据 JSON 同名落 `test_file/e2e/*.json`。

**遗留（不在本项范围）**：① 目录区域的反向归属差异（上面那条，涉及 ㉒ 的"就近回落"策略，不是"主输入名"问题）；② `test_file/projects/multifile`（`ctexbook[11pt]`）在本地测试 bundle 下编不过（bundle 有 `bk10.clo`、缺 `bk11.clo`）—— 与本项无关，但**分章夹具要用 `test_file/projects/bench/multifile`**（`ctexbook` 10pt，文档里的 `bench/multifile` 指的就是它）。


### 6.11 ㊸ 悬停公式即时预览（TeXStudio 式内联浮层；2026-09-17 产品负责人点名入表）

> **范围（ADR-0014）**：v1 **只对 Tectonic 形态提供**（优先库内嵌）；非 Tectonic 引擎**不提供该预览**（如实显示不可用），不写引擎分支。§6.11.8 里那条 **xelatex 腿**（biber 8.6 s、地板 ≈420 ms）降级为**背景数据**，不作为适配目标；§6.11.9/§6.11.10 的两条结论同样限 Tectonic。

**一句话**：光标停在（或悬停在）一个**数学公式**上 → 弹出浮层，里面是**这一个公式的真实排版结果**。触发面按公式定而不是按"选中一段"定：`$…$`、`\(…\)`、`$$…$$`、`\[…\]`，以及 `equation`/`align`/`gather`/`multline` 等环境（含 `*` 变体、跨行）。

**为什么不是"再来一遍片段预览 A"**：A（已回退，`git show 0ac930c`）是**编辑触发的段落级**预览 —— 悬浮一张"改动那一段长什么样"的卡；本项是**指针触发的公式级**预览，触发时机、扫描对象（数学模式而不是段落）、缓存键（公式文本 + 导言区指纹）都不同。**但后端可以整段复用**：A 的独立小文档编译 + `tmp/snippet/` 隔离 + 三个真机坑的修法都留在历史里，这是本项成本能压到 C=2 的原因。

#### 6.11.1 竞品各做到哪一步（2026-09-17 核实）

| 编辑器 | 做法 | 短板（= 我们能做得更好的地方） |
|---|---|---|
| **TeXStudio** | **真排版**：光标在公式上即出内联浮层（可配范围：行内/行间公式、图、todo），另可 `Alt+P` / 右键 Preview 主动触发；原文改动后浮层会自动更新（[配置文档](https://texstudio-org.github.io/configuration.html)、[r/LaTeX](https://www.reddit.com/r/LaTeX/comments/3rszm7/automatic_preview_texstudio/)） | 历史坑集中：**显式输出目录会让内联预览失效**（[#552](https://github.com/texstudio-org/texstudio/issues/552)）、浮层只显示约 1 秒/与工具栏预览重复（[SF #2102](https://sourceforge.net/p/texstudio/bugs/2102/)、[SE 500114](https://tex.stackexchange.com/questions/500114/texstudio-tooltip-help-on-mouseover-only-displaying-for-about-1-second)）、**多行公式与多字符定界符的上下文识别弱**（[SE 108401](https://tex.stackexchange.com/questions/108401/texstudio-user-defined-hover-preview) 的答复承诺改进，4.9.7 变更日志才写"improved context detection for preview (multi-line math, cursor inside multi-char delimiter)"）、**math preview 生成曾直接崩**（同变更日志"fix: crash in math preview generation"） |
| **LaTeX Workshop**（VS Code） | 悬停在**数学环境的起始标签**上弹 **MathJax** 预览；另有独立的 math preview 面板（[wiki: Hover](https://github.com/james-yu/latex-workshop/wiki/Hover)） | **是近似渲染**：按 MathJax 排，**不读项目导言区里的宏**（`\newcommand`/`\DeclareMathOperator`/自定义环境）、字体度量与断行都与最终 PDF 不保真；且以**环境的起始标签**为锚（[SE 554795](https://tex.stackexchange.com/questions/554795/how-to-get-latex-workshop-showing-equations-previews-as-i-move-my-cursor-into-in) 问的正是"光标进到行内公式里能不能也出预览"） |

**⇒ 差异化点（D=3 的依据）**：两家都有，但**"用项目自己的导言区 + 真引擎排这一个公式"只有 TeXStudio 做**，而它把上面四个坑留给了用户；我们做同一件事并且把"输出目录无关 / 浮层可停留 / 多行与多字符定界符 / 不崩"当成立项判据。

#### 6.11.2 需求信号（量化，2026-09-17 取数）

| 来源 | 读数 |
|---|---|
| [SE 108401](https://tex.stackexchange.com/questions/108401/texstudio-user-defined-hover-preview)「Texstudio user-defined hover preview」 | 票 **6**、**3777** 浏览（2013，且站内自建了 2 条衍生链接） |
| [SE 338368](https://tex.stackexchange.com/questions/338368/texstudio-equation-preview-does-not-work-anymore)「Texstudio equation preview does not work anymore」 | 票 1、**3418** 浏览（2016，2025 年仍有活动） |
| [SE 338840](https://tex.stackexchange.com/questions/338840/sublime-text-3-equation-preview-broken-fix-or-disable)「Sublime text 3 Equation preview broken」 | 票 2、**6149** 浏览 |
| [SE 500114](https://tex.stackexchange.com/questions/500114/texstudio-tooltip-help-on-mouseover-only-displaying-for-about-1-second)「tooltip … only displaying for about 1 second」 | 票 3、**1252** 浏览 |
| [SE 534321](https://tex.stackexchange.com/questions/534321/showing-hyperrefs-figures-references-in-a-pop-up-when-hovering-over)「hovering 弹窗看图/引用」 | 票 4、**1691** 浏览（同类心智模型，对象是图/引用不是公式） |
| [SE 32314](https://tex.stackexchange.com/questions/32314/is-there-an-easy-way-to-add-hover-text-to-all-incidents-of-math-mode-where-the-h)「给所有数学片段加 hover 文本」 | 票 **35**、**10633** 浏览 —— **性质不同**：那是"在**成稿 PDF** 里加 tooltip"，不是编辑器预览，只作旁证 |
| 第三方实现 | [`latex-preview.nvim`](https://github.com/sonv/latex-preview.nvim)：Neovim 的 hover 式数学预览（含随打字实时更新、公式/定理/引用）⇒ 说明"悬停看公式"是被反复自建的能力 |

**W=3（不是 4）**：反复被要、且有第三方实现，但本仓痛点调研（[desktop-latex-editor-pain-points.md](./desktop-latex-editor-pain-points.md)）里排在前面的仍是"编译延迟 / 内置 PDF 预览质量 / 大项目卡死"；本项属**生产力增益**而非止血。

#### 6.11.3 候选路线与取舍

| 路线 | 做法 | 成本/证据 | 判定 |
|---|---|---|---|
| **① 独立小文档按需编译（主路）** | 把「项目导言区 + `\begin{document}` + 该公式 + `\end{document}`」装成一份小文档，在**独立项目根** `<项目>/tmp/snippet/` 编译，取第 1 页渲染 | **已实测三轮**。A（release、库形态、热 format）：最小片段文档第 1 趟 **79 ms**、真实片段 **204/200/198 ms**、同机全篇第 1 趟 **1716 ms**。**P0**（§6.11.8）：公式内容不构成成本、同一 snippet 目录重复悬停走 A 闸门 77/332 ms、冷启动的悬崖来自**外部工具**（biber 8.6 s）。**最小包专项**（§6.11.9）：注入 `\nofiles` 把趟数 2→1 后 158/174/422 ms（轻/最小数学集/ctexbook+tikz），**地板 158 ms**（typeset 79 + 转换 79）；"最小包"最多把重导言区拉到 174 ms，但要丢项目宏 | **采用**（后端与三个坑的修法见 `git show 0ac930c`）。**两处设计修正**：① snippet 文档**注入 `\nofiles`**（省 30–43%）② 目录**按公式保留**、不要照 A"每次清空重建"（才吃得到 A 闸门与热 aux） |
| ② 整篇 + `preview.sty`（`active,tightpage`）一次编译出"每公式一页" | 悬停只查表渲染，**零额外编译**；但每次公式变化要**重编全篇**（125 页 1.7 s 起，且与主编译争锁），还要**往用户文档注入宏包**（改文档结构 ⇒ 与 D7/ADR-0007 的取向冲突，且 `preview` 与部分宏包相互打架） | 未测（只作否决记录） | **否决**：把"单次 200 ms 的按需"换成"每次 1.7 s 的整篇"，方向反了 |
| ③ 复用权威 PDF 的页 + 裁剪 | 悬停时若权威 PDF 新鲜，直接裁出该公式那一块 | SyncTeX 只给"源码行 → **一个点**"，**不给包围盒**；范围信息本身要靠 ② 的 `preview.sty` 或自算盒 ⇒ 没有免费午餐 | **不做**（留作以后若 ② 的 tightpage 元数据已在手时的降级路径） |

#### 6.11.4 关键设计点（落地时必须照做）

1. **公式扫描器（core，纯函数，可单测）**：输入「缓冲全文 + 光标偏移」，输出「公式范围 + 类型（行内/行间/环境）+ 内容」。定界规则可直接照 `src/latexSyntax.ts` 的 `math` 状态（`$`、`\(`、`\[` 进入；`$`、`\)`、`\]` 退出），但必须补齐它的**边界**：`\$` 转义、`%` 注释里的 `$`、`\verb`、`$$…$$`、`\begin{equation}` 等环境（含 `*`）、`\left…\right` 里的 `$` 不出现但 `\text{…$…$}` 会出现、跨行公式。**放在 Rust 侧**（与大纲/根探测同层 ⇒ GUI 与 headless 都能用、能单测；前端只做"偏移 → 请求"）。
2. **触发与缓存**：Monaco `registerHoverProvider`（注册范式照 `src/latexSuggest.ts` + `src/main.ts` 的 `registerLatexProvider()`），并复用既有光标管线 `EditorPane.vue` 的 `onDidChangeCursorPosition`。**悬停去抖**（指针停留 ~120–200 ms 才发起）+ **两级复用**（P0 实测后定的口径）：
   - **一级**：同一 snippet 目录的**重复编译**由 runner 已有的 **A 闸门**接住（页哈希相同 + PDF 在 ⇒ 跳过 XDV→PDF 转换、只排 1 趟）⇒ 轻导言区 **77 ms**、ctexbook+tikz **332 ms**（§6.11.8，`reused_pdf=true` 实证）。**成本与公式内容无关**（同导言区下空跑/行内/行间/align 全在 ±15 ms 内）⇒ 缓存键里的"公式文本"只为"换公式时别误命中"服务，**真正省时的是热 aux + A 闸门**；
   - **二级**：前端按 公式文本 + 导言区哈希 + 引擎/形态 + format 指纹 缓存**渲染结果**，命中就连编译都不发起。
3. **隔离（硬要求，照 A 的结论）**：独立项目根 `<项目>/tmp/snippet/<公式键>/`（产物 `tmp/snippet/.../main.pdf`、中间产物在其 `tmp/` 下）；**不碰**权威 `<stem>.pdf`、**不碰**主编译的 `tmp/`；**不经调度器**（否则 `compile-status`/`pdf-updated`/页哈希基线/A 闸门/「引用待更新」语义会被中间产物污染）；`tmp/` 已在 watch 与文件树忽略清单里 ⇒ 不触发编译（A 真机日志确认过）。**目录按公式保留**（吃 A 闸门与热 aux，见 §6.11.9），**片段文档注入 `\nofiles`**（趟数 2→1，省 30–43%，且公式预览不需要 aux/toc）。
4. **与主编译的关系（两条形态约束，P0 后由一条变两条）**：
   - **引擎锁**：库形态的引擎是**进程内全局锁**（㉞ 的探测：4 线程并发 == 单线程），所以**主编译进行中不发起**悬停编译（A 当年就是"编译中直接跳过"）；子进程档（`LATTESET_TECTONIC_LIB=0`）无此约束。**㊴（子进程编译通道）落地后可放开这条** —— 那时悬停编译走自己的进程通道，真并行。
   - **外部工具**（P0 新发现，见 §6.11.8）：`latexmk` 会为片段跑 **biber/bibtex/makeindex** —— biblatex 模板的第一次悬停实测 **8.6 s**（热态 440 ms）；用 `-e '$biber=0;…'` 关掉能降到 2.65 s 但那份文档直接 **exit 12**（缺 `.bbl`）。⇒ 悬停通道必须**显式选择**：库形态天然不跑外部工具（只 `warn!`），子进程形态要么关（可能编不过）、要么不关（可能几十秒）。**默认取"库形态 + 不跑外部工具"，并对超阈值的项目退回按钮式**（见 §6.11.5 的分级目标）。
5. **诚实边界（沿用 A，别让用户当成"文档里的那一处"）**：版式从第 1 页第 1 行起、公式编号/`\tag` 与真文档可能不同、**无 aux ⇒ `\cite`/`\ref` 显示 `??`**、只认导言区里的宏、字体取决于导言区（`article` 无 CJK ⇒ 中文缺字，**这是忠实结果**）。
6. **失败可见（A 的教训 ③）**：公式排不出来时浮层里给一行原因；**绝不静默留白** —— A 当年正是"画布未挂载就渲染 + 静默 return"藏了一轮 bug。

#### 6.11.5 成本、优先级与验收

- **成本**：C=3（公式扫描器 + 悬停/去抖/缓存 + 隔离与接线 + 三个已知坑的复用 + 真机验收）；**若把 A 的后端整段取回**（`git show 0ac930c` 的 `core/snippet.rs` + `compile_snippet` + 卡片组件），可压 **C=2**。V=2（扫描器逐例单测 + 真机可观测钩子断言延迟/缓存/零污染）。W=3、D=3 ⇒ **P=1.2（P1）**；按 C=2 算 **P=1.5**，且满足"成本 ≤2 且独立可交付"⇒ **可走快速通道插队**。
- **切片计划**（照 ㉞ 的做法：每片自带可验证判据，片内不留半个功能）：
  1. ✅ **切片 1（core 纯逻辑，2026-09-17 落地）**：`core::math::math_at`（公式扫描，16 例单测）+ `core::snippet::build_snippet_document`（片段文档装配，5 例单测，口径取自已回退的 A）。**能力位：用户还看不见**（无命令、无 UI）；判据 = `cargo test -p latteset-core --lib math` 与 `--lib snippet` 全绿、`cargo test -p latteset-core` 250 例不回归。
  2. ✅ **切片 2（命令面 + 隔离 + 退让，2026-09-17 落地）**：`Session::compile_math` + `latteset-cli math <公式|->`；落点 `<项目>/tmp/snippet/<键>/`（`core::snippet::snippet_key` = 导言区+公式短哈希，**已存在即复用**以吃 A 闸门）；片段文档由 `core::snippet` 装配（注入 `\nofiles`、保留 `\documentclass`）；建目录/写文件落 `latteset-infra::snippet`（ADR-0010）；`mode=button` 当首次 > 3 s。**实测**（release、库形态）：轻导言区 **171 ms → 二次 79 ms**、ctexbook+tikz **439 → 333 ms**、换公式新目录 **167 ms**、坏公式 `ok=false` + 原因（不留白）、**权威产物逐字节不变** ✓。**能力位：headless 可用，前端 UI 待切片 3**。以下为定稿细节：
     - **命令面**：`compile_math(formula, line, column) -> MathPreviewDto`（含 `elapsed_ms` / `pdf_path` / `page` / `mode`），并给 headless 入口（`latteset-cli math <公式>` 或复用 `Session`）⇒ 判据能在 CLI/MCP 上跑；真机 UI 留切片 3。
     - **隔离**：落点 `<项目>/tmp/snippet/<公式键>/`（键 = 公式文本 + 导言区指纹的短哈希）；**不碰**权威 `<stem>.pdf` 与主编译的 `tmp/main.*`；**不经调度器**（否则 `compile-status`/`pdf-updated`/页哈希基线被污染）。
     - **片段文档**：与 `core::snippet` 同口径 + 两处切片 2 决定：① 注入 `\nofiles`，**必须放导言区**（放文档体会报 `Command \nofiles not allowed in the document body` —— 本轮踩到）；② **保留 `\documentclass` 行**（根文件探测要求，见 §6.12.1）。
     - **两级复用**：同一公式目录的二次悬停走 runner 已有的 **A 闸门**（`reused_pdf=true`）；前端再按「公式 + 导言区指纹 + 引擎形态」缓存渲染结果。
     - **分级退让（决定 1 的落法）**：记录**首次**片段编译墙钟，> 3 s ⇒ 该项目转"悬停出按钮"；**不额外做探测编译**（首次预览本身就是探针，异步出图、不阻塞 UI）。
     - **判据（本轮已实测，实现时照抄）**：新公式 **177 ms（轻）/ 430 ms（ctexbook+tikz）**、同一公式二次 **89 / 350 ms**（`reused_pdf=true`）；坏公式给原因、不留白；权威产物**零污染**（已实测：片段编译 success 后 `main.pdf` 与 `tmp/main.*` 的 SHA256 逐字节不变）；导言区指纹成本 **≈3.7 ms**（452 KB 根文件：读 0.79 + 抽导言区 2.4 + SHA256 0.55）⇒ 可按保存缓存、不必每次悬停重算。
  3. ✅ **切片 3（前端悬停浮层，2026-09-17 完成）**：
     - 后端 `commands::compile_math(text, offset)`（**不**走调度器、独立 project_root、30 s、Quick、`mode=button` 退让）+ specta 注册 + `src/bindings.ts` 再生；
     - 前端 `EditorPane` 自己接 `onMouseMove` + **160 ms 去抖**（不走 Monaco hover provider：浮层要放画布）+ 序号丢弃过期响应 + `window.__mathPreview` 钩子；`MathPreviewCard.vue`（Teleport 到 body、pdf.js 画第 1 页、**失败给一句话不留白**）；
     - ⚠ 口径修正（实现时发现）：Monaco 给的是 **UTF-16 码元**偏移、后端按 **UTF-8 字节**解释 ⇒ 在 `ipc.compileMath` 里统一换算（中文在前时两者不等，中文文档必然发生）；
     - **已真机验证**：应用内 `invoke("compile_math")` 真实执行成功（`hit=true`、公式识别正确含中文前缀、`ok=true`、`elapsedMs=433`、`key` 与 CLI 一致 ⇒ 键稳定）；派发合成 mousemove ⇒ 链路走通（钩子被写入、无公式时**不显示**浮层）✓；`npm run build`（vue-tsc）通过 ✓。
     - **卡片渲染已真机验证**（2026-09-17 用户实测截图）：`$E=mc^2+\sum_{k=1}^n\frac{\alpha_k}{\beta_k}$` 连项目导言区的宏与字体都排对了 ✓；同时暴露一处观感问题 —— **片段文档是一整页，直接显示整页会留下大片空白**；已修：离屏 2× 渲染 → 扫非白像素求**内容包围盒** → 裁剪 + 10 px 留白 → 最多 340 CSS px 宽（`MathPreviewCard.vue`）。
     - **主编译期间不发起**：✅ 已实现**前端守卫**（`compileStore.phase` 为 `queued`/`running` 时不发起悬停编译 —— 库形态引擎是进程内全局锁，此刻发起只会排队等锁、白占一次 30 s 任务）。⚠ 仍留一个毫秒级竞态窗口（守卫与主编译开跑之间）：后果只是那张卡迟到/不出现 + 一次被锁阻塞的任务，**不会拖慢主编译** ✓。
     - **主编译墙钟增量**：✅ 已实测（release、库形态、`bench/large`、`--quick`）：只跑主编译 ×3 = **1938/1748/1732 ms**（均值 1806）；主编译之间穿插公式预览 ×3 = **1771/1788/1807 ms**（均值 1789）⇒ **增量 −1.0%（噪声内）**；穿插的预览本身 **93–104 ms**（热目录走 A 闸门，与 §6.11.8 的"二次"档位一致 ✓）。
     - **观感**（用户反馈后调整）：上限 340 → **480 CSS px**、并给极小公式 **≥180 px** 的放大下限。
     - **应用内端到端断言已补（2026-09-17，真机）**：起 `npm run lib:dev`（lib 形态 + 自动打开 `bench/large`）→ `invoke("compile_now")` 起一次主编译（debug 下几十秒）→ **编译进行中**派发悬停 ⇒
       `window.__mathHoverSkipped` **0 → 1**（守卫分支确实执行）、`window.__mathPreview` 逐字段未变、无浮层 ⇒ **主编译期间不发起**得到端到端确认 ✓。
       （为什么要这个计数：首次断言时悬停位置上**没有公式**，光看"没有新产物"无法区分"守卫生效"与"本来就没公式"——加一个只读诊断计数才把它变成决定性证据；该计数与 `__mathPreview` 同款可观测约定。）
     - **切片 3 收口为 ✅**：卡片渲染（用户真机截图）+ 尺寸调整 + 主编译期间不发起（本断言）+ 主编译墙钟增量 −1.0%（实测）四项齐备。
- **验收判据**（先量后做的次序不能反）：
  1. ~~**P0 前置（先量，不写功能）**~~ ✅ **已完成（2026-09-17）**，数据与结论见 §6.11.8：**没有出现"导言区把片段拉向全篇"的悬崖**（库形态 240/545/760 ms 三档；全篇 1.9–2.2 s），**形态不需要改判**；但量化出两条新的硬约束与一处设计修正（外部工具 8.6 s 悬崖、按导言区分级的目标、snippet 目录按公式保留以吃到 A 闸门）。
  2. 公式扫描器：构造样例逐例断言范围（行内/行间/环境/多行/`\$`/注释里的 `$`/`\verb`/嵌套/跨行 `align`）。
  3. 首次出图（缓存未命中）**按导言区分级**（P0 + 最小包专项实测校准，§6.11.8/§6.11.9，**注入 `\nofiles` 后的口径**）：**轻导言区 ≤ 250 ms**（实测 158–175）、**含 ctex/tikz 的重导言区 ≤ 500 ms**（实测 415–429）；**同一公式的重复悬停 ≤ 400 ms**（实测 90/108 ms 轻中、348 ms 重，走 A 闸门）；**首次冷启动若 > 3 s（外部工具介入，实测 biber 8.6 s）⇒ 该项目退回"悬停出按钮、点击才编译"**，且首测结果要落盘复用（别每次都探一遍）。
  4. **主编译墙钟增量 ≤5%**（同夹具同编辑脚本，开/关各 N 次；口径与 ㊴ 的判据一致），且**主编译进行中不发起**悬停编译（日志断言）。
  5. **零污染**：权威 `main.pdf` 与 `tmp/main.*` 的 SHA256 与关闭该功能时**逐字节相同**（照 ㊷ 的 A/B 做法：HEAD 二进制 vs 改动版、清空 `tmp/` 各跑一轮）。
  6. 失败可见：坏公式 ⇒ 浮层含原因文本；**任何情况下不出现空白卡**。
  7. 真机验收走**可观测钩子**（`window.__mathPreview`，同 `window.__previewLastReload` / `window.__snippetPreview` 约定）：MCP 的合成按键驱动不了 Monaco，这条是硬约束。

#### 6.11.6 风险与未测（诚实标注）

1. ~~**导言区成本分布 `[未测]`**~~ ✅ **已量（§6.11.8）**：库形态下 24 B / 337 B / ctexbook+tikz 三档 = **240 / 545 / 760 ms**（热 format，含 145–409 ms 的转换），同机全篇 1.87–2.20 s ⇒ 片段稳定在 **1/3–1/8**，**没有"导言区一重就崩到全篇量级"这回事**。真正拉长的是另外两处：**外部工具**（biber ⇒ 冷 8.6 s，见下第 7 条）与 **xelatex 自身地板**（子进程形态热态也 ≈420 ms，与导言区无关）。
2. **悬停抖动**：鼠标扫过一行会路过多个公式 ⇒ 去抖 + 只在指针**静止**时触发。✅ **触发方式已定（产品负责人，2026-09-17）：悬停即编译**（不做"必须先点按钮"），并按 P0 数据保留**分级退让**：首次冷启动 > 3 s 的项目（外部工具介入）自动退回按钮式；阈值真机手感再校准。
3. **浮层 UX**：停留时长、位置（遮挡光标）、多屏/DPI、滚动时是否跟随 —— TeXStudio 的两个历史坑（约 1 秒消失、与工具栏预览重复）都是 UX 层的，必须真机试。
4. **多行公式的归属**：✅ **已定（产品负责人，2026-09-17）：按整个环境**。`align` 里光标在第 3 行 ⇒ 预览**整段环境**（对齐关系看得出来才是这个功能的意义）；"只当前行"不进 v1。已落到扫描器：`core::math::math_at` 对环境的任意一行都返回整段（单测 `environment_returns_the_whole_block_from_any_line`）。
5. **导言区口径（A 的结论可直接复用，已核源码）**：A 的口径 = 「根文件里 `\begin{document}` **之前**的全部内容逐字照搬（含注释与空行）」，并靠 `\input@path` 指回项目根 ⇒ **导言区自己的 `\input{preamble}` 是能工作的**（那正是它踩坑后专门修的一处）。仍在本项范围外的：项目根**之外**的共享导言区、**正文里**定义的宏（A 已明确不解决）、以及 `\begin {document}` 这类带空格的写法（A 选择不容忍、如实报错）。本项沿用同一口径，并把"正文宏不可用"如实显示给用户。
6. **编译中 + 悬停的组合**：引擎锁期间只显示"上一次的结果/无结果"还是排队到编译结束（㊴ 之前）—— 行为要显式定义并显示，别静默不响应。
7. **外部工具（P0 新发现，已量）**：biblatex 模板的片段第一次编译会**跑 biber ⇒ 8.6 s**（热态 440 ms）；`-e '$biber=0;$bibtex=0;$makeindex=0;'` 关掉降到 **2.65 s 但 exit 12**（文档缺 `.bbl`）⇒ "关掉"不是免费午餐。悬停通道必须显式选：**默认库形态（天然不跑外部工具）**，子进程形态要么不关（可能几十秒）、要么关（可能编不过）。
8. **bundle 覆盖与字体 = 可用性边界（P0 新发现）**：库形态用的是**项目配置的 bundle**，本地测试 bundle（424 文件）只含 `article/book/ctexart/ctexbook` + tikz/amsmath/amssymb/amsfonts/hyperref/booktabs ⇒ `amsthm`/`pgfplots`/`listings`/`geometry`/`ctex.sty`/`beamer`/各模板类**都编不了**；更隐蔽的是**字体缺口**：`$\mathbb{R}$`（缺 `msbm10.pfb`）与 `$\mathbf{x}$`（缺 `cmbx10`）**排得出但转换报 "Cannot proceed without .vf or physical font"** ⇒ 悬停预览**可用范围 = 该项目能编译的范围**，不可用时必须**如实说明**（不能静默空浮层）。⚠ 这是**测试 bundle** 的性质，不是设计缺陷；但它决定"验收要在真实 bundle 上再跑一遍"。

#### 6.11.7 与其它项的关系

- **㊴ 子进程编译通道**：落地后本项才能在主编译期间并行（4 条约束里唯一的外部依赖）；本项**不阻塞**㊴。
- **㉞ 编译中预览**：那是"整篇的中间态"，本项是"单个公式的即时态"，互不替代；两者抢的是同一个引擎锁，闸门要一起看。
- **既有草稿层（字符近似，31–34 ms）**：本项**不加也不动**它（A 当年也是加一层）。
- **㉒/⑤ 的 SyncTeX**：本项**不用** SyncTeX（那是"源码 ↔ 已排版页"的映射；公式预览是"另排一份"），所以 ㊷ 的修补与本项无关。

#### 6.11.8 P0 定量侦察结果（2026-09-17 完成；脚本 `test_file/e2e/math-preview-lab.ps1`，证据 `math-preview-lab.json`）

**问题**：片段文档到底要花多少时间？它随「导言区重量」怎么变？——A 的 200 ms 是在 **24 B 导言区**（`\documentclass{article}`）上量的，属于最乐观的一端，必须先把曲线量出来。

**方法与有效性**（两条腿，别混）：
1. **库形态 + 本地 bundle**（= 默认计划）：`latteset-cli --project <snippet> compile --quick`（与 A 同口径），片段文档按 A 的装配口径逐字生成（`\input@path` 在前、`\graphicspath` 守卫在文档体内、`\pagestyle{empty}`），每次**清空重建**，取 `elapsed_ms` + runner 的 `passes/typeset_ms/convert_ms/pages` 阶段行；每例 3 样本。
2. **系统 xelatex**（覆盖真模板）：`latexmk -xelatex -outdir=tmp -synctex=0 -interaction=nonstopmode`，片段与**整篇**各跑一遍（整篇前先删 `tmp/`+`*.pdf`，否则 latexmk 报 "Nothing to do" 给出假数字 —— 第一版就踩了这个坑）。⚠ 不能用 `Start-Process` 读退出码（重定向下拿到空值）、**不能加 `-timeout=`**（本机 latexmk 4.87 视其为非法选项 ⇒ 直接 exit 10 什么都不编）。

**① 库形态：成本由导言区与固定开销决定，公式内容免费**（release、热 format、库形态、本地 bundle）

| 导言区 | 字节 | 空跑 `x` | 行内 `$E=mc^2$` | 行间 `\[…\]` | `align` | passes | typeset | **convert** | 热 aux 重复（3 次） |
|---|---|---|---|---|---|---|---|---|---|
| `article`（24 B，`bench/tiny` 口径） | 24 | 239 | 240 | 240 | — | 2 | 78–80 | **145–149** | 240 / **88** / 85 |
| `ctexbook`+`tikz`（`bench/multifile`） | 83 | 745 | 760 | 764 | — | 2 | 330–344 | **397–409** | 752 / **342** / 348 |
| 合成重导言区（amsmath+amssymb+amsfonts+hyperref+tikz+booktabs+5 宏） | 337 | 554 | ✗ | 549 | 553 | 2 | 232–236 | **297–303** | — |

- **公式内容不构成成本**：同一导言区下"空跑 / 行内 / 行间 / align"全在噪声内（±15 ms）⇒ 悬停**任意**公式的代价相同；能省的只有"同一公式重复悬停"（见下）。
- **转换是固定成本，但不是大头**（口径修正后，见 §6.11.9）：1 页片段实测 **78–86 ms**（`typeset` 才是随导言区长大的那一段）。先前这里写的"145–409 ms、是 typeset 的 1.3–2×"是**读了旧字段口径**——`convert_ms` 当时把第 2 趟也算进去了（`phase_ms.1` 只在第 1 趟末打点），修好后 2 趟 = typeset 151 + convert 79 ✓。顺带结掉那条"A 的转换 1 ms 对不上"的疑点：那是 **A 闸门跳过转换**的那一轮（`reused_pdf=true`），旧记录 `tectonic-library-plan.md` V3 同款 ✓。
- **重复悬停走 A 闸门（已实现，免费用）**：同一 snippet 目录连编 3 次 ⇒ 第 2/3 次 `reused_pdf=true`、`convert_ms=0`、`passes=1`：**232 → 77 ms**（轻）、**744 → 332 ms**（重）。⚠ 前提是**目录保留**；A 当年每次清空重建 ⇒ 吃不到这一档。
- **整篇对照**（同形态 `--quick`）：`bench/tiny` 259 ms（1 页）、`bench/large` 1873 ms（121 页）、`bench/multifile` 2198 ms（74 页）⇒ 片段 : 全篇 = **1/7.8**（轻）～**1/2.9**（重），与 A 的 "≈1/8" 同量级。
- **两处失败是数据不是噪声**：`$\R^n$`（`\mathbb`）与 `$\norm{\vx}$`（`\mathbf`/`\lVert`）**排版成功但 XDV→PDF 失败** —— 本地 bundle 缺 `msbm10.pfb` / `cmbx10`（`Cannot proceed without .vf or physical font`）⇒ 见 §6.11.6 第 8 条。

**② 系统 xelatex：冷启动的悬崖是外部工具，不是导言区**（每例：片段冷 1 次 + 热 2 次 + 整篇强制 `-gg` 1 次）

| 工程 | 导言区 | 片段**冷** | 片段**热**（2 次） | 整篇（`-gg`） | 片段:整篇 | 退出码 / PDF |
|---|---|---|---|---|---|---|
| `bench/large`（article，125 页） | 24 B | 1396 ms | 422 / 405 | **8415 ms** | 0.166 | 0/0 ✓ |
| `elsarticle`（中重） | 2020 B | 1379 ms | 426 / 427 | 2344 ms | 0.588 | 0/0 ✓ |
| `shtthesis`（重；**biblatex**） | 6828 B | **8561 ms** | 441 / 431 | 16063 ms | 0.533 | 0/0 ✓；**关外部工具 2652 ms 但 exit 12** |
| `bithesis`（重） | 11156 B | 938 ms | 420 / 425 | 909 ms | — | **12/12 ✗**（模板自身在本机编不过：`! LaTeX Error: Command \, already defined.` 等，环境问题，与本项无关） |

- **热态 ≈420–440 ms 且与导言区无关** ⇒ 那是 **xelatex 自身地板**（起进程 + 读 format + 一趟 + 转换）。子进程形态每次悬停都要付它；库形态全含只要 240 ms（轻）⇒ **形态上库形态更优**，而 **㊴ 的价值是"能并行"，不是"更省时"**。
- **冷态由外部工具决定**：`shtthesis` 的 8.6 s 基本是 **biber**（产物里 `.bbl/.bcf/.blg/.run.xml` 都在 ⇒ 确实跑了）；同样的片段热态只要 440 ms。
- `bench/large` 的整篇 8415 ms ≈ `scripts/synctex-report.mjs` 在同夹具上的 8.3 s ⇒ 测量口径互相印证 ✓。

**结论（对 ㊸ 的形态影响）**：
1. **不否决、不改判形态**：主路（独立小文档按需编译）在库形态下三档导言区都是 **240–760 ms**，把"片段 : 全篇"稳定压在 **1/3–1/8**；没有出现"重导言区拉向全篇量级"。
2. **目标分级**（写进 §6.11.5 验收）：轻/中 ≤400 ms、含 ctex/tikz 的重导言区 ≤900 ms、同一公式重复悬停 ≤400 ms（实测 77/332）。
3. **两条硬约束**：外部工具（biber）必须显式决策；主编译期间不发起（引擎锁，㊴ 后可放开）。
4. **一处设计修正**：snippet 目录**按公式保留**（吃 A 闸门 + 热 aux），别照 A 每次清空。
5. **可用性边界如实显示**：bundle 缺包/缺字体（`amsthm`/`msbm10.pfb` 这类）会让预览失败 ⇒ 报原因、不空浮层；**正式验收要在真实 bundle 上重跑一遍**（本轮是 424 文件的测试 bundle）。

#### 6.11.9 「加载最小包」能不能降延迟？（2026-09-17 定量答复）

**一句话**：能，但**上限有限** —— 它只压得动"导言区那一段"，压不动 typeset/转换的固定成本，而且**有保真代价**（丢掉项目宏与宏包）。同一次测量里还捞到两个**更划算且零保真损失**的杠杆：`\nofiles`（趟数 2→1）与 A 闸门（重复悬停不转换）。

**先把延迟拆成三段**（只有第 ① 段里的"导言区"受"包"影响）：

| 段 | 1 页片段的实测 | 受什么影响 |
|---|---|---|
| ① typeset（引擎排版） | **79 ms**（空导言区）→ **342 ms**（ctexbook+tikz） | **导言区加载**（≈全部差额）+ 公式本身（≈0，见 §6.11.8） |
| ② 转换 XDV→PDF | **78–86 ms**（1 页；口径修正后） | 有固定段 + 每页约 69 ms；**最小包几乎不省**（M0 78 vs M2 86） |
| ③ 前端渲染（pdf.js 单页 + 卡片） | ≈7–9 ms/页（㉞ 实测 8 页 55–74 ms）+ IPC | 与包无关 ⇒ **不是瓶颈** |

**测法**（脚本 `test_file/e2e/math-preview-minpkg.ps1`，证据 `math-preview-minpkg.json`）：库形态、热 format、每次清空重建、`--quick`；三档导言区 × {inline, display, align} × 3 样本，另加"同目录二跑"与两个注定失败的用例。

| 导言区 | 冷（清空重建，2 趟） | 冷（片段文档注入 `\nofiles`，**1 趟**） | 同目录二跑（A 闸门） |
|---|---|---|---|
| **M0** `\documentclass{article}` | 227–233 ms | **158–161 ms** | **~90 ms** |
| **M1** M0 + `amsmath,amssymb,amsfonts`（"最小数学集"） | 257–261 ms | **173–175 ms** | ~108 ms |
| **M2** 项目自己的 `ctexbook`+`tikz` | 741–754 ms | **415–429 ms** | ~348 ms |

**三条结论**：
1. **"最小包"的收益上限 = 把 M2 拉到 M1**：同趟数下 741→257 ms（≈2.9×），注入 `\nofiles` 后 429→174 ms，但**地板是 158 ms**（M0 = typeset 79 + 转换 79）—— **零宏包也降不到 100 ms 以下**；而且最小集**自身要 +16 ms**（M1 173 vs M0 158）。另外"最小包"连公式类型都盖不全：M0 连 `align` 都排不了（`Environment align undefined`），M2 也一样（**项目导言区没加载 amsmath 时 `align` 同样失败**）。
2. **更划算的两条（零保真损失 ⇒ 直接进设计）**：
   - **片段文档注入 `\nofiles`**：新目录本来是 2 趟（第 1 趟写出 `.aux` ⇒ 重跑判据判"变了" ⇒ 再排一趟），而**公式预览根本不需要 aux/toc/`.out`**（没有 aux 时 `\ref` 本来就是 `??`）⇒ 1 趟：**M0 230→159、M1 261→174、M2 745→422（省 30–43%）**。
   - **snippet 目录按公式保留**（吃 A 闸门）：`convert` 归零 ⇒ **90 / 108 / 348 ms**。
3. **两级快路径（"先最小包试排、失败再退回项目导言区"）需要静态判定，不作为 v1 默认**：M1 下用项目宏的公式 —— `$\R^n$` 直接 **failed 111 ms**（还算便宜），但 `$\mathbb{R}^n$` 是**转换阶段**失败 **264 ms**（本地 bundle 缺 `msbm10.pfb`，见 §6.11.6 第 8 条）⇒ 最坏白付 ~264 ms，而且它会把我们与 LaTeX Workshop 的差异（**真项目导言区**）打掉。真要做得靠**静态判定**：扫公式里的 `\宏` 与环境名，与项目导言区的 `\newcommand`/`\DeclareMathOperator`/`\def` 及 `\usepackage` 清单比对，判定"这个公式是否只用到基础数学" ⇒ **列为后续可选优化**。

**顺带修掉一个度量口径 bug**（本次差点被它误导）：`convert_ms` 原先只在 `passes == 1` 时打点 `phase_ms.1` ⇒ **2 趟时把第二趟的时间算进了"转换"**（M0 夹具：2 趟报 148–151、1 趟报 78–81，而两次的 XDV **同尺寸同内容（444 B）** ⇒ 载荷证明不是转换变慢）。修法：**每趟结束都打点** ⇒ 现在 `typeset_ms` = 全部排版趟、`convert_ms` = 仅那一次转换（修后复核：2 趟 = typeset 151 + convert 79 ✓）。这同时解释了旧记录里两处看似矛盾的数：`tectonic-library-plan.md` V3 的 `convert_ms=1 reused_pdf=true` 正是 **A 闸门跳过转换**的那一轮（所以 A 当年报的"转换 1 ms"不是笔误），而 V5 的 `convert_ms=1928`（28 页）折合每页 ≈69 ms，与本轮 1 页 ≈79 ms 同量级 ✓。

**对设计的影响**（已并入 §6.11.4）：片段文档加 `\nofiles`；snippet 目录按公式保留；"最小包"记为**可选快路径**（前提是静态判定）；**降延迟的重心在编译侧，不在前端渲染**。

#### 6.11.10 「启发式推出能正确编译的最小包集」能不能成立？（2026-09-17 定量答复）

**一句话**：**能推"能编"的最小集，推不出"编得一样"的最小集** —— 前者可枚举可测（基础+amsmath 一档就够覆盖绝大多数公式），后者被实测否掉：**任何**导言区差异都会改变页字节，连与公式无关的宏包也一样。所以它只能当**可选加速层**，且验证判据必须是**渲染结果**而不是页哈希。

**测法**（脚本 `test_file/e2e/math-preview-minpkg-heuristic.ps1`，证据同名 `.json`）：同一批公式分别用三档导言区编译（库形态、`--quick`、片段文档注入 `\nofiles` ⇒ 1 趟），比 `tmp/main.tectonic.pages` 的页哈希：
- `H1` = `\documentclass{article}` + **从项目导言区抽出的宏**（`\newcommand` 等，文本级）；
- `H2` = H1 + `amsmath,amssymb,amsfonts`（"最小数学集"）；
- `FULL` = 合成重导言区（H2 + `hyperref` + `tikz` + `booktabs`）。

**① 可编译性：启发式在这一档是可靠的**（10 类公式 × 3 导言区）

| 公式类 | H1 | H2 | FULL | 结论 |
|---|---|---|---|---|
| 基础行内/行间（`$E=mc^2$`、`\[\frac ab\]`） | ✓ | ✓ | ✓ | 基础档就够 |
| `align` / `\text{}` / `cases` | ✗ | ✓ | ✓ | **必须 amsmath** |
| `\mathbb`、项目宏 `\R`、`\norm` | ✗ | ✗（转换/字体失败） | ✗（同样失败） | 本地 bundle 缺 `msbm10.pfb`/`cmbx10` ⇒ 三者都失败，**不是最小包的锅** |
| `\bm`、`\SI`（未装宏包） | ✗ | ✗ | ✗ | **未装就是未装**，快速失败（`Undefined control sequence`） |

⇒ "命令 → 提供它的宏包"这张表 + **从项目导言区抽宏**是靠得住的；未知命令一律回退（失败检测便宜，实测 ~110 ms）。**注意**：`FULL` 自己也可能排不出 `align`（项目导言区没加载 amsmath，见 §6.11.9 的 M2）⇒ 有时最小集反而**更能编**。

**② 保真性：页哈希证明"任何导言区差异都改变页字节"** —— 同一个 `$E=mc^2$`：

| 导言区 | 页哈希 | PDF 体积 |
|---|---|---|
| H2（`article`+amsmath,amssymb,amsfonts） | `6982ad21f723534c` | 3937 B |
| H2 + `tikz,booktabs`（与公式无关） | `2e9fd03277b3e71b` | 3987 B |
| H2 + `hyperref` | `7c25b239c1380efc` | 4077 B |

XDV 逐字节对比显示原因：重导言区里有 `pdf:pagesize width 614.295pt …`（hyperref 注入的 special），最小集是 `pdf:pagesize default`（XDV 360 B vs 1000 B）✓。**`tikz`/`booktabs` 与 `$E=mc^2$` 的排版毫无关系，页字节照样变** ⇒
- **不能用页哈希当"最小集是否等价"的判据**：它太严（语义无关差异也判不同 ⇒ 大量假阴性）；
- 也不能**不验证就用**：那就成了"近似渲染"（LaTeX Workshop 的 MathJax 路线），正是 §6.11.1 里我们的差异点所在；
- 唯一可用的判据是**渲染结果**（预览本来就要渲染 ⇒ 顺手做位图/几何比对），代价是首次要**两个都编 + 两次渲染**（重导言区 ≈600 ms）才能建立信任，之后把结论按「公式 + 导言区哈希」缓存 ✓。

**③ 收益与代价**：重导言区上，最小集首次 **174 ms** vs 项目导言区 **429 ms**（省 ~255 ms）、重复悬停 108 vs 348 ms（省 ~240 ms）；但"看一眼就走"的首次要付验证税 ~600 ms ⇒ **净收益只落在"同一公式反复悬停"**。⇒ 判定：**记为可选加速层（不进 v1）**，前提是 ① 静态分类保守（未知命令回退）② 验证走渲染结果 ③ 预览上明确标注"最小包渲染"以示区别。

**④ 同类但保真的替代（更值得做，已另立 ㊹）**：把**项目自己的导言区预编译成 format**（`mylatexformat` 在本机 TeX Live 里存在：`kpsewhich mylatexformat.ltx` → `c:/texlive/2026/texmf-dist/tex/latex/mylatexformat/mylatexformat.ltx`）。format 里装的就是项目导言区 ⇒ 每次编译**不再重复加载宏包**（typeset 342 → 基础量级），而输出与项目导言区**逐字节一致**，**不需要任何启发式与验证**。代价是 format 生命周期（键 = 导言区哈希 + bundle digest）与"哪些宏包不能进 format"的降级。⚠ 它的收益不止于本项：**主编译每趟也要重读导言区**（库形态每趟起一次引擎）⇒ 重导言区文档每趟白付 ~260 ms。

**对 ㊸ 的影响**：v1 仍走"项目导言区 + `\nofiles` + 目录按公式保留"（§6.11.4）；"启发式最小包"作为可选加速层记录在案；㊹ 若先落地，本项可直接白拿它的收益。

#### 6.12 ㊹ 侦察：把项目导言区固化成 format（2026-09-17；**范围更正：本节结论只对非目标引擎成立**）

> ⚠ **按 ADR-0014（以 Tectonic 为准）重判**：本节全部证据取自 **xelatex + mylatexformat**，而 ㊹ 的目标形态是 **Tectonic 库形态** —— 两者的 format 机制不同（库形态的 format 趟由 bundle 的 `tectonic-format-<name>.tex` 驱动，我们的 I/O 层**可以注入自己的源**、缓存按名字取），**尚未测**。因此下方的「技术前提未成立」**只对 xelatex/latexmk 路径成立**；㊹ 恢复为**待侦察（仅 Tectonic）**，C 回 4、P 回 0.83。xelatex 那份记录保留作反面参考（非目标引擎不必再投入）。

##### 6.12.1 Tectonic 路线实测（2026-09-17 当天补测；脚本 `test_file/e2e/format-recon-tectonic.ps1`）

**怎么测**（**一行 Rust 都没改**）：库形态的 format 趟读的是 bundle 里的 `tectonic-format-latex.tex`（19 B，内容 = `\input xelatex.ini`，见 `io.rs:478` 的 `format_primary_source`）⇒ **改写这个 19 B 的文件**就等于换掉"format 里装什么"；再删掉缓存里的 `<digest>-latex-33.fmt` 逼它重建即可。⚠ 两个环境前提：**沙箱不许写 `%LOCALAPPDATA%`** ⇒ 用 `LATTESET_TECTONIC_CACHE` 指向**工作区内的实验缓存目录**（该环境变量本就是为这个场景留的）；**根文件探测**要求文档里有 `\documentclass`（ADR-0009）⇒ 只有文档体的"裸 body"文档会被 CLI 拒（`未确定根文件（候选 0 个）`）—— 这本身是"format 装导言区"这条路必须记住的一条约束。

| 用例 | 结果 |
|---|---|
| ① 基线：标准 format + 逐字读导言区（缓存已清 ⇒ 含**冷建 format**） | ✅ success：`format_ms=985`、`typeset=94`、总 **1163 ms**、1 页 ⇒ 标准 format 冷建 ≈1 s、24.45 MB |
| ② 自定义 format（源 = 抑制 `latex.ltx` 的 `\dump` → `\input xelatex.ini` → 插 `article+amsmath+` 自定义宏 → `\dump`）+ 文档只留类行 | ❌ **引擎拒绝**：`生成 format 失败（initex 趟）：Error: !Can't \dump a format with native fonts or font-mappings`（`/dev/null` 那几条 missing 是同一次失败的伴随噪声） |
| ③ 同上但导言区换 `ctexbook+tikz+amsmath` | ❌ 同一条错误 |

**结论（Tectonic 路线，实测）**：
- **"抑制内核的 dump、之后再 dump"这个写法在 XeTeX 类引擎上根本不被允许** —— 引擎在**已经登记了 native 字体或字体映射**之后拒绝 `\dump`。这与 TeX Live 上观察到的现象能对上：同一写法在 `xelatex` 上"造得出但没有 `\documentclass`"（内核没跑完）、在 Tectonic 上直接明确报错 ⇒ **不是工具版本问题，是引擎级约束**。
- ⇒ **㊹ 的"主写法"在目标形态上也不成立**；剩下**一条未测的变体**：不要"越过内核原本的 dump 点"，而是**把导言区插在 `latex.ltx` 原本的 `\dump` 之前**（改写 `latex.ltx` 副本、dump 时机与标准流程**完全一致**）—— 若"越界"才是报错原因，这条就有戏；若"只要有任何字体映射就拒绝"，则彻底封死。**复活前先测这一条**（判据同 §4：用/不用 format 的 PDF 与页哈希逐字节相同）。
- **代价与优先级**：即使变体可行，它也要**改写上游内核文件并与 bundle 版本绑定** ⇒ C 记 **5**、P = (3+2)/(5+2) = **0.71 ⇒ P2**（不再是不做档，但也不排期）。
- **正面副产品**（对 ㊸ 有用）：库形态的 `format` 冷建成本实测 **≈1 s / 24.45 MB**，之后 `format_ms` 恒为 0（热缓存）⇒ "format 一次性成本"可从预算里划掉。
**（以下是原记录，读时带上上面的范围限定）**

**一句话（限非目标引擎）**

**一句话**：想法本身完全正确（不猜包、不验证、输出与项目导言区逐字节一致），但**现行工具链上造不出可加载的自定义 format** —— 两条路线都实测失败，其中一条是硬失败（加载即崩）。

**路线 A：`mylatexformat`（3.4，2011；本机 TeX Live 2026 存在）** —— ❌ **加载即崩**
- 造 format **成功**（`xetex -ini "&xelatex" mylatexformat.ltx main.tex`；⚠ 不是 `-initialize`（那是 MiKTeX 写法）；⚠ `mylatexformat.ltx` 必须传**裸文件名**让 kpathsea 找，给绝对路径会 "Please type another input file name" 而什么都 dump 不出来）：四个导言区（`article+amsmath`、`ctexbook+tikz`、`+hyperref`、`+biblatex`）**全部造出 `.fmt`**（705–1054 ms、5.8–7.6 MB）—— 连 `hyperref`/`biblatex` 都能进 ✓（说明"哪些包不能进 format"不是当下的主要障碍）。
- **但用它编译必崩**：`xelatex -fmt=<name> main.tex` 一律退出码 **`-1073741819`（0xC0000005 访问违例）**，`main.log` 0 字节、无产物；**最小 `article` 复现同样崩**（format 能加载，横幅已打出 `preloaded format=minfmt` + mylatexformat 的 `CUSTOMISED FORMAT: minfmt`，随即崩）⇒ 不是包相关、不是参数问题，而是"**加载一个已 dump 的 format 再 re-dump**"这条 2011 年的路线与现代 LaTeX 内核不兼容。

**路线 B：自写 `.ini`（单趟：抑制内核自带的 `\dump` → 插导言区 → 再 `\dump`）** —— ⚠ **未证实可行**
- 关键事实：`latex.ltx` 结尾是**无条件 `\dump`**（`c:/texlive/2026/texmf-dist/tex/latex/base/latex.ltx` 第 **22466** 行），上游**没有**给"在 dump 前插入用户导言区"留钩子 ⇒ 只能在 `\let\dump\relax … \let\dump\origdump`（实测：format 造得出来但**缺 `\documentclass`**，说明内核没跑完）或**改写上游 `latex.ltx`**（与 TeX Live 版本绑定）之间二选一。
- 第二次尝试（复制 `latex.ltx` 并把尾部 `\dump` 换成「`\usepackage{amsmath}` + `\newcommand` + `\dump`」，配 `xelatex.ini` 的前 16 行）第一次因漏 `-etex` 报 `! LaTeX requires e-TeX`（**已定位**：fmtutil 给 xelatex 的口令里有 `-etex`），补上后**仍未产出 `.fmt`**，卡点未定位（下一步：逐行核对 fmtutil 的 xelatex 配方 + 检查 patch 后文件的尾部 + 确认 ini 模式下 `\usepackage` 是否受限）。
- ⇒ **技术前提未成立**：不是"成本高"，而是"还没有一次成功产出可加载的自定义 format"。

**正面事实（一并记下）**：① format **构建成本很低**（0.7–1.1 s、5.8–7.6 MB）⇒ 一次性代价不是障碍，瓶颈全在"能不能造出来"；② **bundle 自带的标准 `latex` format 工作正常**（库形态 `format_cached=true`、`format_ms=0`）⇒ 问题只出在"自定义导言区 format"；③ 我们现有的 format 缓存接口本来就按名字取（`format_file_name(digest, name)` / `format_cache_path`）⇒ **一旦能造出来，装载侧几乎不用改**（要改的只是 `TectonicIo` 里把 `FORMAT_NAME` 常量换成注入名 + `format_present()` 的判定 + format 趟的源注入）。

**结论与处置**：㊹ 从"待侦察"改为 **"技术前提未成立（2026-09-17 侦察）"**，代价 C 4→**5**、优先级 P 0.83→**0.5（低于自动门槛 ⇒ 不做档）**；**不排期**，保留条目与下一步（把一次成功的单趟 ini+dump 做出来；或评估"改写 `latex.ltx` 尾部"的维护代价）。㊸ 的 v1 不依赖它 ✓（`\nofiles` + A 闸门 + 可选的最小包加速层，见 §6.11.9/§6.11.10）。

**侦察脚本（工作区产物，未提交）**：`test_file/e2e/format-recon.ps1` + `format-recon.json`（四导言区 × 造/用/字节对拍），最小复现与路线 B 的两次尝试记录在上面。

### 6.13 ㊺ 软件内新建文件/目录（2026-09-17 产品负责人点名入表）

**需求**：打开**空目录**、或临时需要加一个文件时，现在必须切到资源管理器去建 ⇒ 应当能**在软件内**建。

**现状（代码为准）**：命令面其实**已经能建新文件** —— `Session::write` 走 `resolve_creatable_in_project`（允许目标尚不存在，只校验落在项目根内，D8）；缺的是**前端入口**：`FileTree` 只读，没有"新建"。

**设计（三件，都是小改动）**：
1. **入口**：文件树工具栏 `+`（新建文件 / 新建目录）与节点右键菜单（在选中目录里建）；**名称就地输入**（树内行内编辑，不弹系统对话框）。
2. **落盘与打开**：`ipc.saveAll([{ path, content: "" }])` 或单文件 `write`（二者都要走 D8 校验）⇒ 成功后**刷新文件树**并**在编辑器打开**（新文件立刻可写）。
3. **空目录特例（关键）**：`open_project` 与 `compile` 在 **0 个候选根文件**时会拒（`未确定根文件（候选中 0 个）`）⇒ 新建第一个 `.tex` 之后必须**重跑根探测**（或把新建的文件直接作为根候选交给后续流程），否则"建了文件却编不了"——这正是本项最容易漏的一步。

**判据（可断言）**：
- 空目录：新建 `main.tex` ⇒ 文件树出现、编辑器打开、**直接 `compile` 成功**（根探测已重跑）；
- 建目录 `chapters/` 后在其中新建 `ch1.tex` ⇒ 落盘于 `chapters/ch1.tex`；
- 重名 / 非法字符（`a:b`、`..`）/ 项目外路径 ⇒ 明确提示，**不静默失败、不留空文件**；
- `Esc` 取消 ⇒ 不产生任何文件；
- 真机验收走文件树交互（MCP：`webview_find_element` + `webview_interact` + 截图），并核对树与编辑器状态。

**落地状态（2026-09-17）**：**新建文件**已实现 —— `src/components/FileTree.vue`（标题栏 `＋` + 行内输入 + Enter 落盘 / Esc 取消 + 错误提示留白不静默）、复用 `ipc.saveAll`（目标可不存在 ⇒ 后端无需改动）、成功后 `refreshTree()` + `editor.openFile()`，并在"项目还没有根文件"时重跑 `openProject`（空目录那条路）；`npm run build`（vue-tsc + vite build）通过 ✓。**真机验收（2026-09-17，空目录 `test_file/e2e/empty-project`）**：点 `＋` → 输入 `main.tex` → 回车 ⇒ **文件落盘 ✓、树里出现 ✓、编辑器把 `main.tex` 打开（标签已开）✓、无错误提示 ✓**。⚠ **但"建完就能编"这条当时没达成**：`get_project` 仍是 `rootFile=null / 候选 0` —— 因为**根探测要求文件里有 `\documentclass`**，而 `＋` 建的是**空文件**。**已修（2026-09-17，两条都做了）**：① 新建 `.tex` 时给**最小骨架**（新手向导，见 §6.13.1）；② 落盘后重跑根探测 + 补一次首编（`projectStore.rescanRoot`）。真机复验：空目录建 `main.tex` ⇒ 立刻认到根、PDF 自动出图 ✓。
**其余待办**：~~② 新建**目录**~~ / ~~③ 右键在选中目录里建~~ ✅ **两项已落地（2026-09-18，见 §6.13.2）**。

### 6.15 文件管理补齐：删 / 改 / 查（㊼㊽㊾，2026-09-21 产品负责人点名立项）

**需求原文**：文件管理"只有增（新建）"，要把 CRUD 补齐 ——
㊼ **删**：鼠标悬停在文件/文件夹行上时，行右侧出现垃圾桶按钮，点击触发**二次提示**，确认后删除；
㊽ **改**：右键菜单增加**改名**；
㊾ **查**：文件树上方加**搜索栏**，输入文字**模糊搜索**。

**三件共用的地基**（一次做完，避免三次动同一处）：

- `FileSystem` trait 新增 `remove_file` / `remove_dir_all` / `rename`（FakeFS 保持"只读"、两个示例实现补上）——
  与 `write`/`create_dir` 同一分工：**能不能删、能不能改由命令面的策略决定，trait 只做系统调用**。
- 命令面 `delete_path`（三道闸：**必须已存在**、**必须在项目根内**、**不能是项目根**；目录递归删）
  与 `rename_path`（同目录改名、**不覆盖**已存在的目标、返回改后的绝对路径）。
- 前端收口（**最容易漏，漏了就是静默坏掉**）：
  - **删**：`editor.closeTabsUnder(path)`（删目录要连子文件的标签一起关，否则点进去必然报错）；
  - **改**：`editor.remapPaths(from, to)`（标签 / 活动路径 / 脏集合 / 缓冲 / 自保存时刻 / 冲突标记
    **全部跟着搬** —— 漏一个的症状分别是"标签指向不存在的文件""保存写回旧路径""改名后的编辑被当成外部修改"）；
  - **两者都动到根文件时**：`afterTreeMutation` —— 有手动覆盖就改覆盖（删 ⇒ 清、改 ⇒ 换新名），
    没有覆盖就重开项目重探；然后**补一次编译**（watch 在改名/删除那一刻已经用旧根编过一次，必然失败）。

**㊼ 删**：垃圾桶按钮在行**悬停**时显形（`opacity: 0 → .7`，**仍占位** —— 用 `display:none` 会让文件名的
省略号宽度在悬停瞬间跳一下）。二次确认用新增的 `ConfirmDialog.vue`：**先说要删什么、再说后果**
（目录报"里面还有 N 个文件，会一起删除"，N 从已有的树里数、不额外读盘）。取消 = 零副作用。
> 取舍：**不做回收站**（真删除 + 明确确认）。需求写的是"垃圾桶按钮 + 二次提示"，而"能撤销"会削弱
> 确认弹窗的必要性；改成进系统回收站要引入 `trash` 这类依赖，属另一个决策。

**㊽ 改**：右键菜单第三项「✏️ 重命名」→ **行内输入框**（打开即聚焦并**只选中主名**，不含扩展名 ——
改名的常见意图是改名字不是改后缀）；`Enter` 提交、`Esc`/失焦取消（与新建行的取消语义一致）。
v1 **只支持同目录改名**（跨目录移动是另一个功能点），目标已存在时明确报错**不覆盖**。

**㊾ 查**：树上方一个输入框 + `src/services/fuzzy.ts`（纯函数、6 例单测）：**子串命中优先**（忽略大小写），
**子序列兜底**（`cit` → `chapters/intro.tex`，这才是"模糊"）。命中项与它们的**祖先目录**一起显示
（保留上下文），**目录命中时保留整棵子树**（搜 `chapters` 想看的是"里面有什么"）；搜索期间树强制展开
（`forceExpand`）。命中数实时显示，0 命中给一块空状态卡。刻意不做拼音/编辑距离：规模是"一个项目几百个
文件"，按命中位置排序就够，别为一个输入框引入匹配引擎。

**真机验收（2026-09-21，`test_file/e2e/crudlab` 临时工程，跑完即删）**：

| 点 | 场景 | 结果 |
|---|---|---|
| ㊾ | 输入 `cit` | ✅ 只剩 `chapters → intro.tex`（子序列命中），「命中 1 个文件」 |
| ㊾ | 输入 `chap` / `chapters` | ✅ 命中 2 个文件，目录命中**保留整棵子树**（intro.tex + notes.txt 都在） |
| ㊾ | 输入 `zzz` / 清空 | ✅ 「命中 0 个文件（换个词试试）」+ 空状态卡；清空 ⇒ 完整树回来 |
| ㊽ | 右键 `chapters/notes.txt` → 重命名 | ✅ 行内输入框打开、聚焦、**只选中 `notes`**（不含 `.txt`）；改成 `memo.txt` ⇒ 磁盘与树都变 |
| ㊽ | 改**根文件**名 `main.tex` → `thesis.tex` | ✅ 日志：改名 →（watch 用旧根编一次，失败）→ `打开项目…根文件 Some(thesis.tex)` → `手动编译` → `编译成功`；状态栏回到「就绪」、当前标签跟着变成 `thesis.tex` |
| ㊼ | 悬停行 | ✅ 规则 `.row:hover .trash{opacity:.7}` 在样式表里且作用域正确、按钮在行内（基础值 0）。⚠ **`webview_interact action=hover` 是合成事件，改不了浏览器真实的 `:hover` 状态**（`row.matches(':hover') === false`）⇒ 悬停显形按"规则 + 结构"验证，截图拍不到 |
| ㊼ | 点垃圾桶（文件）| ✅ 弹窗「文件：README.md / 删除后不可撤销（项目里没有回收站）」；**取消 ⇒ 什么也没删** |
| ㊼ | 点垃圾桶（目录 `chapters`，内含 2 个文件）| ✅ 弹窗「文件夹：chapters / 它里面还有 2 个文件，会一起删除（不可撤销）」；确认后**递归删掉** |
| ㊼ | 删**根文件** | ✅ 磁盘删除、树刷新、编辑器标签**被关掉**（`tabs: []`）、状态栏回到「未确定根文件 · 点击选择」、无崩溃 |
| 旁证 | 删掉被 `\input` 的章节文件 | ✅ 编译失败但给出**可行动诊断**：「找不到文件 chapters/intro.tex → 确认该文件确实在项目内，且路径/大小写与 `\input` 里写的一致」（④ 错误诊断的既有能力，不是新问题）|

**两处真机看出来的瑕疵（当场修）**：① 根文件改名后 `root_file` 仍指旧路径 ⇒ 之后每次编译都失败
（比对用错了新路径，改成用旧路径比）；② 确认弹窗里的 `**一起删除**` 是 Markdown 星号，界面上原样显示 ⇒ 去掉。


#### 6.13.2 新建目录 + 右键菜单（2026-09-18 产品负责人点名）

**需求原文**：「完成目录新建的功能点，在**新建弹窗**中和**右键菜单**均增加功能入口」。

**落地（三处入口，共用一条创建流程）**：

| 入口 | 位置 | 行为 |
|---|---|---|
| 工具栏 | 资源管理器标题栏右缘**一个** `＋`（`new-file-button`）| 展开行内输入；**建文件还是建文件夹，在输入框右边那对小图标里切换**（📄 默认 / 📁），占位文案跟着变 |
| 右键菜单 | 树节点右键（菜单逻辑在 `FileTree`，节点侧只 emit）| 「建在：<目录名>」+「新建文件」/「新建文件夹」；**目录 ⇒ 建在它里面；文件 ⇒ 建在它所在目录** |
| 向导弹窗 | 新建向导底部「或者先建个文件夹」（`wizard-folder`）| 关掉向导、回到文件夹命名流程（空项目里想先分目录的那条路）|

⚠ **工具栏为什么只剩一个 `＋`**（2026-09-21 产品负责人反馈"创建目录和创建文件并排两个 + 太奇怪"）：第一版做成两个并排图标，**先逼用户做一次无谓的选择**、还占掉半格标题栏。改成"一个入口 + 行内小切换"——默认文件，于是常见用法仍是"点一下、打名字、回车"（比两个按钮**少一次决策**）。右键菜单仍保留两项：菜单里罗列本来就自然，而且能指定落点。

**后端**：`FileSystem` trait 新增 `create_dir`（**单层**语义：父目录必须已存在——与 `write` 同契约；已存在 ⇒ `AlreadyExists`，**不静默成功**）→ infra `TokioFs` 用 `tokio::fs::create_dir`（**刻意不用 `create_dir_all`**，否则"已存在"就检测不出来了）→ 命令面 `create_dir`（D8：`resolve_creatable_in_project` 校验落点；提示文案用**项目内相对路径**，因为面板只有 256 px 宽）。

**两个细节**（都是真机跑出来的）：
1. **建完要展开目标目录**：在收起的目录里右键建文件，刷新后新文件藏在里面，看起来像"没建成"。做法是给 `FileTreeItem` 加一个 `expandPath` 属性（**一次性请求**、递归原样透传，命中即展开）——展开状态仍留在组件内（§9.4 的信息局部性不变）。
2. **右键那条路锁定目录**：名称里再带 `/` 是误操作 ⇒ 前端直接提示（工具栏那条路仍允许 `chapters/ch1.tex`，那是写明了的用法）。

**真机验收（2026-09-18，`test_file/e2e/mkdir-lab{,2}` 两个临时工程，跑完即删）**：

| 场景 | 结果 |
|---|---|
| 工具栏 `＋`（默认 📄）→ `note.tex` | ✅ 落盘为文件、编辑器打开 |
| 工具栏 `＋` → 点 📁 → 占位变「文件夹名，如 chapters」→ `素材` | ✅ 落盘为目录；**点切换不会收掉输入行**（`mousedown.prevent` 保住焦点），焦点仍在输入框 |
| 再点 `＋` | ✅ 回到默认 📄（不记上次选择——每次都从"最常见的那件事"开始） |
| 右键目录 → 新建文件夹 → `附录` | ✅ 落在**那个目录**里（`素材/附录`），且父目录自动展开 |
| 右键目录 → 新建文件 → `note.tex` | ✅ 落在 `素材/附录/note.tex`，编辑器打开、树展开到位 |
| 右键**文件** → 菜单标题 | ✅ 「建在：notes.txt 所在目录」（目标解析为项目根）|
| 重名 | ✅ 「已存在同名文件或文件夹：素材」（**相对路径、单行**；输入框保留且仍聚焦，改个名字就能重试）|
| 右键锁定目录时输入 `sub/ch1.tex` | ✅ 「在某个目录里新建时只需填名称（要建子目录请先建那个目录）」，**什么都没建** |
| `Esc` | ✅ 收掉输入行与错误提示，**零产物** |
| 向导「或者先建个文件夹」 | ✅ 关向导 → 文件夹输入行（占位「文件夹名，如 chapters」）→ `chapters` 落盘、树里出现、状态栏仍显示「未确定根文件」（目录不构成根文档，符合预期）|

**单测**：infra `create_dir` 单层语义（成功 / 已存在 `AlreadyExists` / 父目录不存在报错 / 同名文件占位也报 `AlreadyExists`，中文目录名可建）。


#### 6.13.1 新手向导 + 无根文档时的状态栏引导（2026-09-17 产品负责人追加要求）

**需求原文**：在空目录新建**第一个** `.tex` 时**弹窗引导**用户选择（文档语言、文档类型等；面向新手 ⇒ **只放常见选项**）；该向导**默认打开**、**可在设置里关闭**。另外：**没有根文档时**在**下方状态栏显示一个标签**，并且**此后每次保存都静默触发一次根文件探测**，直到探测到根文档为止。

**A. 新建向导（只在"空项目里的第一个 .tex"出现）**
- **触发条件**：项目里**还没有根文档** ∧ 本次是"新建文件" ∧ 扩展名是 `.tex` ∧ 设置里的向导开关为开。已有根文档时**不弹**（避免打扰日常使用）。
- **选项（面向新手，保持少）**：
  - **文档语言**：`中文（ctex）` / `English` ⇒ 决定 `\documentclass` 用 `ctexart`/`ctexbook` 还是 `article`/`book`，以及 `\usepackage[UTF8]{ctex}` 要不要加；
  - **文档类型**：`短文（article）` / `报告（report）` / `书（book）` / `幻灯片（beamer）`；
  - **标题**（可选，预填文件名）。
- **产物**：一份**最小骨架**写进新文件 —— 这同时解决 6.13 里那个"空文件探测不出根"的缺口（骨架里必有 `\documentclass`）⇒ 建完立刻被认成根、能直接编 ✓。
- **开关**：设置面新增 `compile.newFileWizard`（默认 `true`）⇒ 走现有 settings 模型（schema_version 递增 + merge/validate + 设置面板一个开关）；关闭后行为回到"建空文件"。
- **取舍**：只做"语言 + 类型 + 标题"，不做宏包多选、不做模板库（那些属 ⑫ 的账）。

**B. 状态栏"未确定根文档"标签 + 保存即静默重探测**
- **显示条件**：当前项目**没有根文档**（`rootFile == null`）⇒ 状态栏出现一个**可点**标签（点了打开 `RootFilePicker` 指定根文件，或提示"新建一个 .tex"）。
- **静默重探测**：在该状态下，**每次保存后**重跑一次根文件探测（复用 `open_project` 用的同一套 `core` 探测，不新增口径）；**探测到就撤标签**并正常编译。为避免噪声：重探测**不弹任何提示**，只在日志留一行 `debug!`。
- **为什么是"保存后"而不是"每次击键"**：探测要读盘（内容真相源），保存才是磁盘状态变化的时刻 —— 与 ADR-0007 一致。

**判据（可断言）**：
1. 空项目 ⇒ 新建 `main.tex` ⇒ **弹向导**；选"中文 + 文章" ⇒ 生成含 `\documentclass{ctexart}`（或等价的 ctex 装法）的骨架 ⇒ **文件树出现 + 编辑器打开 + 根探测立刻认到 + 直接 `compile` 成功**；
2. 设置里关掉向导 ⇒ 再建文件**不弹**、得到空文件（回到今天的行为）；
3. 项目**已有根文档**时新建 `.tex` ⇒ **不弹**；
4. 打开一个"有 .tex 但没有 `\documentclass`"的目录 ⇒ 状态栏**出现标签**；在编辑器里补上 `\documentclass` 并保存 ⇒ **标签消失**且能编译（每次保存都重探测，直到成功）；
5. 全程**无弹窗打扰**（除第一次向导）—— 这是本项与"处处弹提示"的分界。

**落地状态（2026-09-17）**：**骨架生成已落地**（`core::newfile::document_skeleton`，`DocLanguage` × `DocKind` = 8 种组合，6 例单测全绿 —— 含"任何组合都含 `\documentclass`/`\begin{document}`/`\end{document}`"这条**根探测不变量**；只预置 `amsmath`，因为 `amssymb` 的符号字体在缺字体的 bundle 下会让预览转换失败；中文一律走 `ctex*` 类；beamer 不给 `\maketitle`，直接给一页 frame）。**A + B 全部落地（2026-09-17）**，三件都在：

1. **骨架命令面**：`new_file_skeleton(lang, kind, title)`（`src-tauri/src/commands.rs`）—— 纯转发 core、无副作用、不读项目状态（前端因此能在向导里逐次改选项做**实时预览**）。"语言 × 类型"这张表**只此一份**（core），前端只传选择值。
2. **`NewFileWizard.vue`**（新组件）：文档语言（中文/English）、文档类型（短文/报告/书/幻灯片）、标题（可不填，预填文件名），外加一块**骨架预览**（调同一个命令现算 ⇒ 预览与落盘产物不可能不一致）。`Esc` / 点蒙层 / 取消 = **不产生任何文件**。
3. **接进 FileTree 新建流程**：触发条件是「**无根文件** ∧ **无候选** ∧ 扩展名 `.tex` ∧ 设置开关为开」（`needsWizard`）。设置项 `compile.new_file_wizard`（默认 `true`，`#[serde(default = ...)]` ⇒ 旧 `settings.json` 能读）+ 设置面板「编译」页一个 开/关 段 + 「恢复默认」一并回默认。
4. **状态栏标签 + 保存即静默重探测**（B）：标签本来就有（可点 → `RootFilePicker`）；本次补上缺的那半 —— 探测与"探到之后怎么办"收进 `projectStore.rescanRoot()`（**复用 `open_project` 那条路 ⇒ 与初次口径一致**），`useAutoSave`（保存后）与 `FileTree`（新建落盘后）共用同一份。

**两处对原设计的细化（都写进代码注释了）**：

- **"有候选"也不弹向导**：原文只说"还没有根文档（`root_file == null`）"。但模板项目常是"有好几个 `\documentclass` 而没定下用哪个"——那不是新手的空项目，弹"文档语言/类型"只会挡路（他们会走根文件选择器）。故触发条件收紧为**无候选**。
- **探到根文件后补一次首编**（原文 §6.13.1-B 写的是"探测到就撤标签**并正常编译**"）：watch 那条触发链在这个变化**之前**只能看到 `root_file=None` ⇒ 它已经把这个变化丢掉了；不补的话状态栏已显示"就绪"、预览却一直停在「PDF 在这里等你」，用户得再敲一个字或手动点「编译」。真机日志即为证据：`不触发编译：尚未确定根文件` → `打开项目…（根文件 Some(...)，候选 1 个）` → `手动编译` → `编译成功 pages=1`。
- **顺手修掉的一个首屏打扰**（服务判据 5）：打开**一个 `.tex` 都没有**的目录时不再弹根文件选择器（那个弹窗没有可选项，而"空目录"恰是新用户第一条路）；改由状态栏那条常驻标签兜底（点了仍能打开选择器），用户也可以直接用 `＋` 建第一份文档。有 `.tex` 但零/多候选时**照旧弹**（那时弹窗真有东西可挑）。

**真机验收（2026-09-17，五个空目录 `test_file/e2e/wizard-lab{,2,3,4,5}` 轮换，跑完即删；引擎 Tectonic 库内嵌）**：

| 判据 | 结果 | 证据 |
|---|---|---|
| 1 空项目 → 向导 → 骨架 → 认到根 → 能编 | ✅ | 中文+短文 → `\documentclass[UTF8]{ctexart}` 骨架 129 B，树/编辑器都到位，状态栏「未确定根文件」**消失**，PDF 自动出图（标题 + 日期 + 第一节）；另一轮 **English+书** → `\documentclass{book}` 122 B + `thesis.pdf` 7883 B |
| 2 关掉向导 → 不弹、得到空文件 | ✅ | 面板关掉（`settings.json` 写 `"new_file_wizard": false`）后建 `main.tex` ⇒ **无弹窗**、磁盘 0 B、编辑器打开 |
| 3 已有根文档时新建 `.tex` → 不弹 | ✅ | 有根文件时建 `second.tex` ⇒ 无弹窗、0 B，主文档 PDF 不受影响 |
| 4 有 `.tex` 无 `\documentclass` → 标签出现 → 补上并保存 → 标签消失且能编 | ✅ | 建空 `main.tex` ⇒ 标签出现；在编辑器里输入 `\documentclass{article}…` 并保存 ⇒ 磁盘 75 B、`main.pdf` 3011 B **自动产出**、标签消失、预览渲染出 "Hello Latteset" |
| 5 全程无弹窗打扰 | ✅ | 空目录不再弹根文件选择器；判据 2/3 无任何弹窗；只有"空项目里第一个 .tex"弹一次向导（可关） |

**一条环境事实（不是缺陷）**：本机实验室用的 **精简 bundle**（`test_file/tectonic-bundle`，424 个文件，离线复现用）**不含 `beamer.cls`** ⇒ 选「幻灯片」会得到 `! LaTeX Error: File 'beamer.cls' not found.`（错误列表照常给出、不是静默失败）。**同一份骨架用 TeX Live 2026 的 `xelatex` 编出 1 页（exit 0）** ⇒ 骨架本身是合法 beamer；上游标准 bundle 含 beamer。详见 [troubleshooting.md](../troubleshooting.md)。

**一处真机反馈（2026-09-17 产品负责人）**：`＋` 原本**紧贴「资源管理器」文字**，应贴在这一列的**右缘**。根因不是 `margin-left:auto` 写错，而是 `FileTree.vue` 里 `.new-file/.new-row/.new-input/.new-error` **四条规则写在了 `</style>` 之后** ⇒ 整段是死 CSS（SFC 编译期不报错、`npm run build` 照过），输入框当时用的是浏览器默认样式。修法：把这段挪回 `<style scoped>` 内。真机几何判据：`.panel-title` 横跨 x 0→256、按钮右缘 **242**（= 右内边距 14 px），`margin-left` 计算值 119 px（auto 生效）；建文件行输入框宽 240 px、`padding 3px 6px`、圆角 4 px。同类坑见 [troubleshooting.md](../troubleshooting.md)。

**英文骨架的占位文字**：首轮真机验收发现英文 book 的目录里冒出"第一节" ⇒ `document_skeleton` 的占位文字改为随语言（`第一节`/`Introduction`、`第一页`/`First frame`），单测钉住"英文骨架里不出现汉字"。

**待真机验收**：无（~~判据 4~~ ✅ 已验）。**其余待办**：见 §6.13 的"新建目录 / 右键在选中目录里建"。


**与 6.13 的关系**：6.13 的"新建文件"是入口，本条是它的**新手引导与无根兜底**；A 同时把 6.13 遗留的"空文件探测不出根"一并解决（骨架里有 `\documentclass`），B 则是"用户手打内容"那条路的兜底。

**为什么要进表（而不是"随手改一下"）**：① 它牵着**根探测**这条跨模块口径（0 候选的行为）；② 需要前端交互与 D8 校验两处配合；③ 空目录是"新用户第一条路"，属**首次体验**而不是小众便利。

### 6.14 ㊻ 引擎清单动态化 + 默认引擎切 Tectonic（2026-09-17 产品负责人点名）

**需求原文（三条，同一轮提出）**：① 设置面板的引擎下拉**弹层冲出面板**；② "引擎既然以 Tectonic 为优先，Tectonic 就应该在第一个"；③ "我不想写死这个选项栏，而是通过**环境变量或者其他方式**来动态加载 TeX 引擎"。经确认：③ 取 **A 口径**（后端探测可用性 + 环境变量覆盖，**不做**用户自定义引擎命令行）；② 连带把**默认引擎也改成 Tectonic**。

**① 溢出（已修，`f99ac11`）**：根因不是宽度写错 —— 原生 `<select>` 的弹层按**最长选项**撑宽，而选项文本原本是 `引擎名 — 整句说明`（Tectonic 那条约 100 字），CSS 管不到弹层。修法：说明移出 `<option>`，改在 select 下方用 `.field-hint`（与「主题」「编译模式」同一范式）。真机：选项最长 **8 字符**、select 自身 424 px。⚠ 验证口径：**截图拍不到原生弹层**（WebView2 的下拉是独立窗口），要用几何证明（见 troubleshooting）。

**② 顺序**：清单顺序 = **Tectonic 第一**（`core::engine::ENGINE_ORDER`）。

**③ 动态化（本次落地）**：

| 层 | 落点 | 契约 |
|---|---|---|
| core | `crates/latteset-core/src/engine.rs` | `ENGINE_ORDER` / `label` / `hint` / `required_binaries`（判据的唯一副本）/ `EngineInfo{id,label,hint,available,reason}` / `engine_list(has_binary, lib_form, env)` —— **纯函数**，可用性由闭包注入 ⇒ 6 例单测（含"env 覆盖不能伪造可用性"） |
| infra | `crates/latteset-infra/src/probe.rs` | `find_in_path`（**只查文件、不启动进程**：便宜、无副作用、不会在坏安装上卡住）+ `env_tex_engines()`（读 `LATTESET_TEX_ENGINES`，ADR-0010 的"外部依赖唯一落点"） |
| 命令面 | `commands::list_engines` | 把三者拼起来；不缓存（一次面板打开只是几趟目录查询） |
| 前端 | `src/services/engines.ts`（会话内缓存一次）+ `SettingsPanel` / `StatusBar` | **前端不再有引擎数组**；不可用的引擎**留着并禁用** + 显示原因（"本机没装 XeLaTeX"本身就是要讲给用户听的信息）；状态栏的显示名走同一份 |

**关键边界（写进 modules §12.2）**：
- **支持的引擎集合仍是 `Engine` 枚举** —— 编译命令按引擎分支（`-xelatex` / `-C` 分档 / 是否经 latexmk），环境变量只能**筛选与重排**，不能凭空造一个新引擎（那要扩 runner 的命令模板与产物契约，是另一个功能点）；
- 覆盖**不能把不可用的引擎变成可用**（可用性是探测出来的事实）；
- **清单里必须始终含有当前设置的那个引擎**：真机实测过反面 —— `LATTESET_TEX_ENGINES='pdf, xelatex'` 时若设置里是 `tectonic`，`<select>` 因为没有匹配项而**显示空白**、说明行也空。修法：前端补一条不可选的只读项并给出原因（"当前设置指定的引擎不在本次构建开放的清单里…"）。

**默认引擎切 Tectonic（ADR-0014 修订 1）**：`Settings::default().compile.engine` 改为 `Tectonic`，**形态位仍是 `lib_form=false`**（子进程档）。**只影响没有 `settings.json` 的新装** —— 老配置写着 `xelatex` 的升级后一字节不变（不静默换引擎）。随之更新的口径：`design.md` 的默认引擎、`modules.md` §12.2、`README.md`；`docs/research/` 下的带日期报告**保留当时的"默认 XeLaTeX"表述**（它们是证据不是现状）。已知代价：本机既没有 `tectonic.exe`、构建又没编入库形态时**首编会失败**（错误文案已可行动；面板上该引擎显示为不可用并给原因）。

**真机验收（2026-09-17，`bench/tiny` 夹具，引擎 Tectonic 库内嵌）**：

| 项 | 结果 |
|---|---|
| 清单来自后端 | ✅ 选项恰为 `Tectonic / XeLaTeX / LuaLaTeX / pdfLaTeX`（Tectonic 第一），`disabled=false`（本机五个可执行文件都在 PATH 上），说明行随选中项切换 |
| `LATTESET_TEX_ENGINES='pdf, xelatex'` | ✅ 选项恰为 `pdfLaTeX / XeLaTeX`（**顺序照写**，小写别名 `pdf` 也认）；设置里是 `tectonic` ⇒ 补出 `tectonic（不可用）` 只读项 + 原因，下拉**不再空白** |
| 状态栏显示名 | ✅ 走同一份清单（`Tectonic`），清单缺该引擎时退化成 id |
| 默认引擎 | ✅ 单测 + headless 用例（隔离配置目录 ⇒ 真走"没有 settings.json"这条路）断言 `Tectonic`；恢复默认按钮也已跟着改 |

**未在本机验的**：`不可用 ⇒ 禁用 + 原因` 的**真机**渲染 —— 本机 `latexmk/xelatex/lualatex/pdflatex/tectonic` 全在 PATH 上，且库形态也编了进来，四个引擎都可用；这条分支由 core 单测（假闭包）覆盖，真机只验到"后端给的可用性被如实渲染"。要在真机上看它，得在一台没装 TeX Live（或没装 tectonic）的机器上打开设置面板。

**为什么进表**：它同时是**默认值变更的必要条件**（默认改成一个"本机可能没有"的引擎，就必须让用户看得见哪个可用），也是"不把产品事实写死在前端"的第一处。
