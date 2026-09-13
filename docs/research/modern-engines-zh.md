# 比 XeLaTeX 更现代、又能排中文的引擎：盘点与实测

> 问题来源：「有没有支持中文、相比于 XeLaTeX 更加现代化的 TeX 引擎」。
> 姊妹篇：[no-fork-alternatives.md](./no-fork-alternatives.md)（§3.1/§3.2 的成本分解与 fmt 实测）、[incremental-edit-x-dvi.md](./incremental-edit-x-dvi.md)（页级复用 A/B/C）、[g1-read-interception-feasibility.md](./g1-read-interception-feasibility.md)（拦截每一次 I/O）。
> **结论一句话**：**有——LuaLaTeX（本机 = LuaHBTeX 1.24.0 + `luatexja`）是唯一"现代化且能排中文"的 LaTeX 引擎**：HarfBuzz 整形、100+ 个引擎内 Lua 回调、格式可 dump。但**对 Latteset 现在切过去是净亏**：同一份 28 页中文论文单趟 **6.47 s vs XeLaTeX 1.87 s（3.5×）**——而且 §7 把两边优化**拉齐之后结论不变**（英文文档里 LuaLaTeX 反而更快 ⇒ 3.5× 是**中文/字体层**的差距，不是引擎代差）。另外两条硬伤：**我们的 Quick 路径在 LuaLaTeX 下实测是坏的**（§3.1 两种失败模式，**已修**），而页级复用依赖的 XDV 在中文 LuaLaTeX 下**根本不存在**（`luatexja` 明确不支持 DVI 输出）。**它真正值钱的地方不在速度，而在"引擎内回调"**——那恰好是上游阶段 6/7 需要、我们现在只能在进程外做的事（§3.3）。

## 0. TL;DR

| 问题 | 答案 |
|---|---|
| 有比 XeLaTeX 现代、又能排中文的引擎吗 | ✅ **LuaLaTeX** = LuaHBTeX 1.24.0（TeX Live 2026）。中文走 `ctex → luatexja`；本机两条夹具（28 页 / 74 页）都编出来了，**页数与 XeLaTeX 一致** |
| 还有别的候选吗 | **Tectonic**（Rust、XeTeX 衍生、自动取包、可复现）——**本机未安装，全部结论未测**；~~LuaJITTeX~~ ❌ [已测并否决](#73-试过的-lualatex-针对性优化逐项结论)：TL2026 **没有 luajitlatex**（`fmtutil.cnf` 只有 plain 的 `luajittex`），手工建 JIT+LaTeX 格式也失败；Typst / ConTeXt 不是 LaTeX 替代品 |
| 快吗 | ❌ **3.2–3.5× 慢**（§2 表）：28 页 6468 vs 1870 ms；74 页 7111 vs 2189 ms |
| 首次使用贵吗 | ❌ 冷启动要建字体缓存：**25.8 s / 23.6 MB / 35 个文件**，且缓存目录必须可写 |
| 我们的 Quick（编辑触发单趟）能用吗 | ~~❌ 不能用~~ ✅ **已修（#25）**：原实测两种失败——干净项目 → `xdvipdfmx` 找不到 `.xdv` **报编译失败**（引擎其实成功了）；有遗留 XDV 时 → **转出上一次 XeLaTeX 的 PDF 并覆盖 LuaLaTeX 的产出，还报成功** |
| 页级复用（B/C/A）还能做吗 | ❌ 中文 LuaLaTeX **产不出 DVI/XDV**（`luatexja`：`DVI output is not supported in LuaTeX-ja`）→ 现有页哈希机制无输入。**但**可注入 `shipout/before` + `\ShipoutBox` 拿到"页号 → 页指纹"做等价物（§7.2 原型已通、引擎侧零开销；A 无对应物） |
| 那它有什么 XeTeX 给不了的 | ✅ **进程内拦截每一次 I/O**（`open_read_file` / `find_read_file`，实测拦到 4 次真实读取）、逐页/逐行/逐字回调、`finish_synctex`、`start_run`/`stop_run`（§3.3 全清单） |
| fmt（[no-fork §3.2](./no-fork-alternatives.md) 在 XeTeX 上被硬否决）在 Lua 侧呢 | 🟡 **dump 确实成功**（XeTeX 是彻底禁止），但**普通 ctex 文档的运行时挂接我没做通**（那正是 `mylatexformat` 的角色，本机未装）→ **收益未验证**。**上限已量化**：导言区占 LuaLaTeX 单趟 **44%**（上限 ~2.9 s）⇒ 它是 LuaLaTeX **唯一真正大的杠杆**（§3.4 / §7.3 #3） |
| 建议 | 保持 XeLaTeX 为默认；**先把设置面板里 LuaLaTeX 的说明与 Quick 路径的缺陷对齐**（要么修 Quick，要么明确降级为"完整编译"）——见 §4 |

| 拉齐之后呢（§7：给 LuaLaTeX 也做一轮针对性优化） | **排名不变，但原因被定位**：中文仍慢 3.2–3.5×；而**英文同结构文档 LuaLaTeX 反而更快**（1878 vs 2341 ms）⇒ 差距**全在 CJK 字体层**（XeTeX native font 编译在引擎内 vs `luatexja + luaotfload` 全在 Lua 层）。LuaLaTeX 唯一的大杠杆是**预编译导言区**（导言区占它单趟 44%，上限 ~2.9 s）；LuaJIT 在 TL2026 **没有受支持配置**、PDF 压缩调优**无收益**、注入式页指纹（恢复 B/C）引擎侧零开销但收益仅 ~100 ms |

## 1. 候选盘点

| 引擎 | 是什么 | 中文怎么走 | "更现代"体现在哪 | 本机状态 |
|---|---|---|---|---|
| **LuaLaTeX / LuaHBTeX** | LuaTeX 的 LaTeX 格式 + HarfBuzz 整形后端 | `ctex` → `luatexja`（+ `luatexja-fontspec`） | ① 整形用 **HarfBuzz**（XeTeX 用自研 ICU 路径）；② **Lua 脚本可直接介入排版**（100+ 回调，见 §3.3）；③ 格式**可以 dump**；④ PDF 由引擎后端直接写出（无独立转换进程） | ✅ 已装（1.24.0 / TL2026） |
| **Tectonic** | Rust 重写的 XeTeX 衍生引擎，单二进制 | 同 XeTeX（`xeCJK`/`ctex`）+ 包从 bundle 自动拉取 | ① 依赖自动解析（无需本地装宏包）；② 输出可复现；③ 内置 SyncTeX 与增量缓存 | ❌ **未安装 → 本报告不含它的任何实测** |
| **LuaJITTeX** | LuaTeX 的 LuaJIT 版（无 HarfBuzz） | 同 LuaLaTeX | 速度（Lua 侧 JIT） | ✅ 已装（未测） |
| **pdfLaTeX** | 老牌引擎 | 需 `CJK`/`zhmakeindex` 等老路线，**中文支持差** | 无 | ✅ 已装（我们已支持但非中文路线） |
| **upLaTeX / pLaTeX** | 日文社区引擎 | 日文 JIS 字体路线 | 与中文无关 | ✅ 已装（`uplatex`） |
| Typst / ConTeXt(LMTX) | 非 LaTeX 体系 | 各自字体机制 | 现代，但**不是 LaTeX 兼容替代**（用户模板不能直接编） | Typst 未装 |

> 判据：用户拿来的是 LaTeX 模板（[cn-thesis-template-engines.md](./cn-thesis-template-engines.md) 里那些高校模板），所以只有"仍是 LaTeX、且能排中文、且比 XeLaTeX 现代"才有替换意义 → **只剩 LuaLaTeX**。

## 2. 实测：中文编译速度（含两个前置坑）

夹具：`test_file/projects/bench/_zhcmp/`（本次新建，`bench/thesis`、`bench/multifile` 的副本 + 三档自建中文小样）。每档 3 次取中位，`SOURCE_DATE_EPOCH=0`；Windows / TeX Live 2026。

| 夹具（中文） | `xelatex -no-pdf`（我们的 Quick 形态） | `xelatex`（含转换） | `lualatex` | `lualatex -no-pdf` |
|---|---|---|---|---|
| `empty-en`（article 空文档） | 999 ms | 998 ms | 1255 ms | — |
| `empty-zh`（ctexart 空文档） | 1194 ms | 1430 ms | 2552 ms | — |
| `min-zh`（1 页：中文 + 公式 + 表格） | 1370 ms | 2794 ms | 4392 ms | — |
| `thesis`（ctexbook + amsmath + tikz，**27 页**） | **1870 ms** | 2992 ms | **6468 ms** | 6492 ms |
| `multifile`（20 章，**74 页**） | **2189 ms** | 3363 ms | **7111 ms** | 7045 ms |

读法：

- **LuaLaTeX ≈ 3.5× XeLaTeX 单趟 / 2.2× 完整 `xelatex`**。它不是"略慢"，是把 1.9 s 的编辑期延迟推到 6.5 s——对"实时预览"这个产品定位是致命的。
- 增幅随文档变大而略降（3.5× → 3.2×），说明慢的主要是**每页排版与字体处理**，不是固定开销。
- `-no-pdf` 被 LuaHBTeX **静默忽略**：仍写出 PDF、耗时不变（6468 vs 6492、7111 vs 7045，差异在噪声内）→ 这直接引出 §3.1 的缺陷。

**产出核对**（同一份 `thesis`）：XeLaTeX 27 页 / `lualatex` **27 页**；`multifile`：74 / **74 页**。页数一致（版式等价的第一道关），但**字节数不同**（XeLaTeX 70,203 B vs LuaLaTeX 109,732 B，字体嵌入方式不同，属正常）。

### 2.1 前置坑（都会变成用户可见的产品问题）

1. **luaotfload 需要可写的 `TEXMFVAR`**。本机在 DSH 沙箱下（工作区外只读）第一次跑就失败：
   ```
   luaotfload | load : FATAL ERROR
   luaotfload | load :   × Failed to load "fontloader" module "basics-gen".
   luaotfload | load :     × ".../luaotfload-init.lua:301: system : no writeable cache path, quiting"
   ```
   实测**只设 `TEXMFCACHE` 不够**，必须让 `TEXMFVAR` 指向可写目录（本机 `TEXMFVAR = C:/Users/Vinci/.texlive2026/texmf-var`）。真实含义：**用户的 TeX Live 若装在只读位置（Program Files / 只读漫游配置），LuaLaTeX 直接不可用**，而 XeLaTeX 不受影响。
   > 附带教训：这条 FATAL 若**不带 `-halt-on-error`**，`dvilualatex` 会陷入无限报错循环（本次实测把日志写到 **226 MB** 才被我掐掉）。所有非交互探针都必须带 `-halt-on-error`。
2. **首跑要建字体库**：全新缓存目录下第一次 `lualatex empty-zh.tex` = **25,846 ms**，产出 **23.6 MB / 35 个文件**（`luatex-cache/generic/{names,otl,luatexja}`）。之后才回到 ~1–2.5 s。这 26 s 会原样落在第一次点「编译」的用户身上。

## 3. 对既有机制的影响（这才是取舍的关键）

### 3.1 Quick 路径：实测坏了（两种失败模式）→ **已修（2026-09，见 [modules.md](../modules.md) §12 已知债 #25）**

> 修法：`Engine::writes_xdv()` 作为唯一闸门（`-no-pdf`、页哈希、`xdvipdfmx` 三处都过它）＋ 页哈希缓存按引擎分文件。
> 真机验证：engine=lualatex 编辑触发 → `draft=true` 编译成功、项目根得到 LuaLaTeX 自己的 65,813 B PDF（不再是陈旧 XDV 转出来的 40,6xx B）、不写 `.pages`；切回 XeLaTeX → 日志出现 `页哈希与上次逐页相同：跳过 xdvipdfmx 转换`（A 未回归）。
> 下面保留**修复前**的现场记录，作为这道闸门的存在理由。

代码上 Quick 固定给引擎加 `-no-pdf`（`crates/latteset-infra/src/runner.rs:238-246`），收尾时按需调 `xdvipdfmx`（同文件 `finish_success` / `convert_xdv`）。而 LuaHBTeX **忽略 `-no-pdf`**（§2），于是：

**① 干净项目（无遗留 `.xdv`）→ 报编译失败，其实引擎成功了**

```
latteset-cli --project <thesis 副本> --config-dir <空目录> compile --quick   # .latteset/settings.json: engine=lualatex
→ status: failure, kind: io
  "xdvipdfmx 转换失败（exit=Some(1)）：xdvipdfmx:fatal: Could not open specified DVI (or XDV) file: …\tmp\main.xdv
   / No output PDF file written."
```
即：LuaLaTeX 已经把 `tmp/main.pdf` 写好了，我们却因为"找不到 `.xdv`"把这次编译判成失败 → **编辑期每一次自动编译都会报错**。

**② 有遗留 XDV（用户从 XeLaTeX 切过来就会这样）→ 拿错引擎的 PDF，还报成功**

同一命令（此时目录里留着上一次 XeLaTeX 的 `tmp/main.xdv`，261,776 B）：

```
→ status: success, engine: lualatex, pdf_path: <项目根>/main.pdf
```
但 `tmp/main.pdf` 与项目根 `main.pdf` 都是 **70,193 B**，而同一份文档 LuaLaTeX 自己产出的是 **109,732 B**（27 页）——说明收尾时 `xdvipdfmx` 把**上一次 XeLaTeX 的陈旧 XDV** 转成 PDF，**覆盖掉了 LuaLaTeX 刚写出的 PDF**，再拷到项目根并报成功。项目根 PDF 与"整份 Full 编译（latexmk -lualatex）"的产物（112,543 B / 28 页）也完全不同。
> 结论：页哈希缓存（`tmp/<stem>.pages`）与 `tmp/<stem>.xdv` **没有按引擎区分**，跨引擎残留会互相污染。

**③ 完整编译（Full）不受影响**：`compile`（latexmk -lualatex）实测 `status: success`，18.6 s（多趟收敛）。即"只有编辑期 Quick 是坏的"——而 Quick 正是我们 ㉘ 的核心卖点。

### 3.2 页级复用（B/C/A）：中文下没有 DVI 可解析，但有引擎内替代

- **DVI 路线彻底不可用**（两条都试过）：
  - `dvilualatex`（plain luatex 基座）+ ctex 中文 → `luaotfload FATAL`；
  - `lualatex --output-format=dvi`（HarfBuzz 引擎 + DVI 后端）→ 缓存问题排除后明确报
    `! Package luatexja Error: DVI output is not supported in LuaTeX-ja.`
  ⇒ **中文 LuaLaTeX 只能出 PDF**，我们那套"读 XDV 算页哈希"的机制（`crates/latteset-core/src/xdv.rs`）在 Lua 路径上**没有输入**。
  > 顺带确认：`xdv.rs` 的 opcode 表与标准 DVI 的 0–249 完全一致，理论上能吃 DVI——问题不在解析器，而在**没有 DVI 可吃**。
- **替代方案（已实测可行）**：LuaTeX 有引擎内逐页回调。注册 `pre_output_filter` 后，Lua 侧每次输出例程都能拿到该页节点表并算指纹（本次探针输出 `page 1…5  nodes=84/3/111/4/5  fnv=…`）。这比"事后解析二进制中间文件"更直接：**不需要额外产物、不需要解析器**，代价是必须换成 Lua 引擎。
  > 注意 API 变更：本机 LuaHBTeX 1.24 **没有 `node.serialize`**（早期文档里的写法会 `attempt to call a nil value`）→ 指纹要用 `node.traverse` 自己遍历算。

### 3.3 引擎内回调：LuaLaTeX 真正的差异化（与 G1 直接相关）

TeX Live 2026 的 LaTeX 内核（`ltluatex.lua`）向文档暴露 **70+ 个回调类型**，其中与本项目直接相关的：

| 回调 | 对我们的意义 |
|---|---|
| `open_read_file` / `find_read_file` / `read_*_file` / `start_file` / `stop_file` | **G1「拦截每一次 I/O」的进程内实现**（[G1 报告](./g1-read-interception-feasibility.md) 里我们只能在进程外绕）；上游阶段 6 的 seen 水位/依赖图直接受益 |
| `process_input_buffer` / `input_level_string` | 输入缓冲级拦截（可以做"改了哪几行"的精确记账） |
| `pre_output_filter` / `buildpage_filter` / `contribute_filter` / `append_to_vlist_filter` / `page_objnum_provider` / `finish_pdfpage` / `finish_pdffile` | 逐页信息与指纹（§3.2 替代方案）、按页产出控制 |
| `pre_linebreak_filter` / `post_linebreak_filter` / `linebreak_filter` / `hyphenate` / `kerning` / `ligaturing` / `glyph_info` / `new_graf` | 行/字级介入（连中文断行引擎 `luatexja` 自己就是靠这些回调实现的） |
| `start_run` / `stop_run` / `wrapup_run` / `pre_dump` | 运行生命周期与格式构建钩子 |
| `finish_synctex` | SyncTeX 产物收尾钩子（我们现在只能等文件落盘再读） |
| `show_error_hook` / `show_warning_message` / `show_error_message` | **结构化错误捕获**（不必再只靠解析 `.log` 文本） |

**实测（被动拦截真的会发生）**：在中文文档导言区用 `luatexbase.add_to_callback("open_read_file", …)` 注册后，一次真实编译里拦到 **4 次文件读取**（`supp-pdf.mkii`、`epstopdf-base.sty`、`umsa.fd`、`umsb.fd`），函数拿到了完整路径。

两个必须知道的坑（都是本次实测踩出来的）：

1. **LaTeX 内核禁止直接 `callback.register()`**：`Module luatexbase Error: Attempt to use callback.register() directly` → 必须 `luatexbase.add_to_callback(name, fn, desc)`。
2. **"只记录不改变行为"没那么容易**：`open_read_file` 的返回值就是"读文件"本身（返回 `nil` = 打不开）。我第一版直接 `return nil` → `! I can't find file 'umsa.fd'`；第二版把整份内容当"一行"交回 → 后续某个文件读取被破坏、整趟编译 fatal。**要真正做到被动，必须按引擎的契约逐行喂回**（随 TeX Live 的 `luaprogtable-stringbuffer.lua` 是可参考的写法）。→ 对 G1 的含义：**钩子存在，但"零侵入拦截"仍需认真实现**。
3. 形态提醒：`pre_output_filter` 只在**输出例程**触发（本次 1 页文档触发了 5 次，因为多个 `\shipout` 盒子），**不能用它做"文档第 N 页"映射**——实测同一份 1 页文档触发 2 次且页号已变成 2（§7.2 有正确做法）。

### 3.4 fmt：Lua 侧 dump 成功，但运行时挂接没做通

[no-fork §3.2](./no-fork-alternatives.md) 实测 XeLaTeX **彻底不能 dump**（连最简导言区都报 `Can't \dump a format with native fonts or font-mappings`）。Lua 侧不一样：

| 尝试 | 结果 |
|---|---|
| `lualatex -ini … \dump`，导言区 = `\documentclass{article}` | ✅ **dump 成功**：`fmtmin.fmt` 3.7 MB；且用 `-fmt=fmtmin.fmt` 跑只含 `\begin{document}…` 的正文 → **exit 0，出 1 页 PDF** |
| 同上，导言区 = `\documentclass[UTF8]{ctexart}` | ✅ dump 成功：`fmtctex.fmt` **11.3 MB**（`Beginning to dump on file …`） |
| 同上，导言区 = `thesis` 的完整导言区（ctexbook + amsmath + tikz + 标题） | ✅ dump 成功：`zhfmt2.fmt` **12.5 MB** |
| 用 ctex 格式跑"只有正文"的 `\begin{document}…` | ❌ `! LaTeX Error: Missing \begin{document}.`（换 `article` 格式则正常）→ **普通文档要用预编译导言区，需要 `mylatexformat` 那套类状态机**（本机 `kpsewhich mylatexformat.sty` 为空 = 未安装），我自写的 shim 也会破坏 ctex 的运行时状态 |
| （附带）dump 过程会打印一条**非致命** Lua 错误：`lua-uni-algos/lua-uni-stage-tables.lua:141: attempt to perform arithmetic on a string value` | 不加 `-halt-on-error` 时能继续并成功；加了就直接中止 → 说明这条错误在 TL2026 上是**已知噪音级**问题，但它会让"用 `-halt-on-error` 构建格式"的做法失败 |

⇒ 结论：**"LuaLaTeX 能 fmt" 在机制上成立（引擎不禁止），但在本机没验证出可用收益**。而它的**上限**在本轮补测清楚了（§7.3）：LuaLaTeX 的导言区占单趟 **44%**（2869/6468 ms），所以 fmt 是它**唯一真正大的杠杆**——比下面试过的任何一项都大一个量级。

## 4. 结论与建议

1. **回答原问题**：是，**LuaLaTeX（LuaHBTeX + luatexja）就是"支持中文且比 XeLaTeX 现代"的那个引擎**；Tectonic 是另一条更激进的路（换生态、自动取包，本机未测）。
2. **但不建议现在切换默认引擎**，理由是三条实测硬约束（不是口味问题）——**而且这三条在 §7 把两边优化拉齐之后依然成立**：
   - **慢 3.2–3.5×**（28 页 1.87 → 6.47 s），直接摧毁"编辑即预览"的体验预算；
   - **Quick 路径在 LuaLaTeX 下曾整个是坏的**（§3.1：干净项目误报失败 / 有残留时产出错引擎 PDF；**已修**），而 Quick 是 ㉘ 的支柱；
   - **页级复用（B/C/A）在中文下失去输入**（`luatexja` 不支持 DVI），等于把我们刚做完的三个优化点清零；对等优化（§7.2）只能补回 B/C，A 结构上没有对应物。
   另外还有两个产品级成本：**首跑 26 s 建字体缓存**、**要求可写的 `TEXMFVAR`**。
   反过来也要记住 §7.4 的一句公道话：**英文文档上 LuaLaTeX 比 XeLaTeX 快**（1878 vs 2341 ms），3.2–3.5× 是**中文/字体层**的差距，不是"引擎代差"。
3. **真正值得吸收的是"引擎内回调"这条能力**（§3.3）：进程内拦截每一次 I/O、逐页/逐行钩子、结构化错误捕获、SyncTeX 收尾——这些正是上游阶段 6/7 与 G1 报告里我们只能"在进程外绕"的东西。若将来要认真做实时渲染，**"换 LuaLaTeX 拿回调"比"打补丁改 XeTeX"更现实**（无需自维护引擎分支），代价就是上面那三条。
4. **近期该修的产品缺陷**（本次实测发现）：~~Quick 路径的 `-no-pdf` 与 `xdvipdfmx` 收尾只对 XeLaTeX 成立~~ ✅ **已修（2026-09）**——`Engine::writes_xdv()` 闸门 + 页哈希缓存按引擎分文件，真机验证见 §3.1 顶部与 [modules.md](../modules.md) §12 #25；设置面板 LuaLaTeX 的提示也已写明速度与首次编译代价。

## 5. 复现方法

脚本都在工作区（`test_file/` 已 gitignore，未提交）：

```powershell
$lab = "test_file\projects\bench\_zhcmp"
# 0) 前置：给 luaotfload 一个可写的缓存目录（否则 FATAL: no writeable cache path）
$env:TEXMFVAR = "$lab\var"; $env:TEXMFCACHE = "$lab\var"; $env:SOURCE_DATE_EPOCH = "0"

# 1) 速度对照（自带 -halt-on-error 与超时保护；dvilualatex 会无限刷日志，别裸跑）
pwsh -NoProfile -File "$lab\run3.ps1" -N 3      # 小样三档
pwsh -NoProfile -File "$lab\run4.ps1" -N 3      # thesis / multifile（需先建 tmp\chapters）

# 2) 首跑建缓存成本（换一个全新 TEXMFVAR）
$env:TEXMFVAR = "$lab\var-cold"; Measure-Command { lualatex -interaction=nonstopmode -halt-on-error -output-directory=tmp empty-zh.tex }

# 3) DVI 路线（应报 luatexja 不支持）
lualatex --output-format=dvi -interaction=nonstopmode -halt-on-error -output-directory=tmp min-zh.tex

# 4) Quick 路径缺陷（用 headless CLI，最省事）
Set-Content "$lab\thesis\.latteset\settings.json" -Value '{"compile":{"engine":"lualatex"}}' -NoNewline -Encoding utf8
src-tauri\target\debug\latteset-cli.exe --project "$lab\thesis" --config-dir "$lab\cfg" compile --quick   # ①干净项目：xdvipdfmx 报找不到 .xdv
                                                                                                          # ②有残留 .xdv：产出被旧 XDV 覆盖

# 5) 回调探针
lualatex -interaction=nonstopmode -halt-on-error cb-probe.tex   # 枚举回调类型（cb.lua）
lualatex -interaction=nonstopmode -halt-on-error io-on.tex      # 逐页指纹（cb-io.lua）→ _cb.log

# 6) fmt（Lua 侧能 dump）
lualatex -ini -interaction=nonstopmode -jobname=fmtmin "&lualatex _min.tex\dump"   # 不带 -halt-on-error 才会成功

# 7) 公平对比（§7）：PDF 后端成本 / 注入钩子开销 / Full 对比
pwsh -NoProfile -File "$lab\fair.ps1" -N 3

# 8) 引擎内页指纹（§7.2，不改用户文档）
lualatex -interaction=nonstopmode -halt-on-error -jobname=shipmin `
  "\AddToHook{shipout/before}{\directlua{dofile('pageship.lua')}}\input{min-zh.tex}"   # → _ship.log：shipout page=1 nodes=979 fnv=…

# 9) Lua 层版本与速度（§7.3 #1）
luajittex --luaonly luaversion.lua    # → Lua 5.1 + LuaJIT 2.1.81742，且 jit.status()==false
lualatex  --luaonly luaversion.lua    # → Lua 5.3，无 jit
luajittex --luaonly bench-lua.lua     # 与 lualatex --luaonly 对照；jit.on() 版见 _jitonbench.lua 的拼法
```

## 6. 未验证与局限

1. **Tectonic 完全未测**（本机未安装）：它的"现代性"（自动取包 / 可复现 / 增量缓存）全是**外部资料判断**，没有本机数字；它对中文的实际表现（bundle 里的 Fandago 字体、系统字体、`ctex` 可用性）也未知。
2. ~~**LuaJITTeX 未测**~~ ✅ **已测并否决**（§7.3 #1）：TL2026 的 `fmtutil.cnf` 里**没有 luajitlatex**（只有 plain 的 `luajittex`/`luajithbtex` + `luatex.ini`），手工给 JIT 引擎建 LaTeX 格式也失败 ⇒ 这个版本上"JIT + LaTeX + 中文"**没有受支持的配置**（因此它是否更快仍是空白，但没有可用的落地形态）。
3. **fmt 的收益数字没拿到**：dump 成功、`article` 格式运行成功，但 **ctex 文档的运行时挂接失败**（需要 `mylatexformat` 或等价实现，本机未装）→ "LuaLaTeX 上 fmt 到底省多少"仍是空白。**上限已量化**：导言区占 LuaLaTeX 单趟 44%（2869/6468 ms，§7.4）。
4. **回调只做了"能注册、能触发"这一层**：逐页指纹验证到"页号准确 + 引擎侧零开销"（§7.2），但**没有验证把它接成 B/C 的页级复用是否真的正确**（需要与"PDF 实际变化页"对齐）；"零侵入拦截 I/O"的正确 reader 实现也**没做通**（§3.3 坑 2）。
5. **只测了合成夹具**：`thesis`/`multifile` 每页内容偏少；真实学位论文（图表/公式密集）里 LuaLaTeX 的相对劣势可能变大也可能变小，**未测**（hithesis 档仍被 ㉖ 阻塞）。
6. **Full 对比只有一组可用样本**：latexmk 收敛对比里，第二次运行两边都被判"已是最新"（~0.9 s）不可比，只有首跑那组（xe 9193 / lua 16686 ms）可用。
7. **慢的原因只归因到"层"没归因到"代码"**：已定位到 **CJK 字体层**（英文场景 LuaLaTeX 反而更快，§7.4），但层内——HarfBuzz 整形、`luatexja` 字距调整、`luaotfload` 字体加载各占多少——**没有 profile**。
8. **跨引擎 PDF 等价性只到"页数一致"**：没有做像素级/文字流级对拍（`pdftotext` 在 io-off/io-on 之间做过一次文本一致校验，但那不是跨引擎对照）。
9. **"注入式页指纹"只在原型层验证**：命令行扩展命令行（`"\AddToHook…\input{…}"`）对 **SyncTeX 顶层文件映射 / 错误行号归属 / 与 `-output-directory` 的组合**都**没有验证**——这正是它没落地的原因（§7.2）。

## 7. 公平对比：给 LuaLaTeX 也做一轮针对性优化（2026-09 追加）

§2 的速度表被质疑得对：**XeLaTeX 那一列享受了我们整套管线优化（Quick `-no-pdf` + 按需转换 + 页级复用），LuaLaTeX 那一列是裸引擎**。本节把两边拉齐，并把"能给的优化"逐项试一遍。

### 7.1 对等性台账

| 优化 | XeLaTeX（我们的管线） | LuaLaTeX（本轮拉齐后） |
|---|---|---|
| 编辑期单趟（Quick，不经 latexmk） | ✅ `xelatex -no-pdf` | ✅ `lualatex` 单趟（#25 修好后可用） |
| 不做 PDF 工作再转 | ✅ 转 PDF 交给 `xdvipdfmx`，**页未变时可整个跳过** | ⛔ **没有可跳过的独立步骤**——PDF 在单趟里直接写（英文场景实测只占 **~290 ms**，见 §7.3） |
| 页级复用 B（页未变 → 跳过预览重载） | ✅ XDV 页哈希 | ✅ **原型已打通**：注入 `shipout/before` + `\ShipoutBox` 指纹（§7.2） |
| 页级复用 C（只重绘变化页） | ✅ 页哈希 → 页号 | ✅ 同上（页号准确，实测 1 页文档恰好 1 次 shipout、页号=1） |
| 页级复用 A（跳过转换） | ✅ 省 0.65–1.4 s/次 | — 无对应物（不是被漏掉，是结构上没有这一步） |
| 首编后的多趟收敛 | 空闲收敛 Full | 同样（且 Quick 的相对收益更大，§7.3） |
| 预编译导言区（fmt） | ⛔ 引擎**禁止** dump（no-fork §3.2） | 🟡 **dump 可行**（§3.4），运行时挂接未做通 → 这是**唯一一个"XeLaTeX 永远做不到、LuaLaTeX 有机会"的大杠杆**（导言区占它单趟 44%） |

### 7.2 引擎内页指纹：不改用户文档也能拿到"页号 → 该页指纹"

LuaLaTeX 没有 XDV，所以 B/C 在它下面原本没有输入。原型（**未落产品代码**）证明了等价物存在：

```powershell
# 关键：用命令行的"扩展命令行"注入，用户的 .tex 一个字都不用改
lualatex -jobname=inj "\AddToHook{shipout/before}{\directlua{dofile('pageship.lua')}}\input{main.tex}"
```

- **注入是零侵入的**：同一份 `min-zh.tex` 注入前后产出的 PDF **字节数完全一致**（96,362 B），耗时差异在噪声内
  （min-zh 中位：无注入 4578 ms / 仅注入 4487 ms / 注入+逐页指纹钩子 **4396 ms**）→ **指纹钩子在引擎侧几乎不要钱**。
- **钩子要选对**：`pre_output_filter` 只在**输出例程**入口触发（1 页文档触发 2 次、页号已变成 2），**不能**当页号用；
  正确做法是 LaTeX 的 `shipout/before` 钩子 + 从 Lua 取 `\ShipoutBox`（`token.create("ShipoutBox").index`）与 `\c@page`。
  实测 1 页文档正是 1 次 shipout、`page=1`；27 页论文逐页递增。
- **指纹要自己递归**：本机 LuaHBTeX 1.24 既没有 `node.serialize`，`node.traverse` 也**不下钻盒子内部**——
  直接 traverse 顶层 vlist 只看到 1 个节点（指纹退化成常量）。要按 `n.head` 递归（改完后同一页 979 个节点）。
- **为什么不落地**：它的收益在下游（B：省一次预览重载；C：渲染 102→7 ms，见 ⑦c），量级约 **~100 ms**；
  代价是**编译命令行形态改变**（`-jobname` + 注入串 + `\input`），会牵动 SyncTeX 顶层文件映射、错误行号归属、
  `-output-directory` 交互等一串已验证的行为。**收益/风险不成比例**，故只作为对比公平性的证据保留。

### 7.3 试过的 LuaLaTeX 针对性优化（逐项结论）

| # | 想法 | 实测 | 结论 |
|---|---|---|---|
| 1 | **LuaJIT 变体**（`luajitlatex`） | ⛔ **此版本 TeX Live 上不可用**（根因见 §7.5：**连 LaTeX 内核自己的 Lua 代码都解析不了**）。TL2026 的 `fmtutil.cnf` 只有 plain 的 `luajittex`/`luajithbtex`（`luatex.ini`）；另外三件实测：① 该构建里 **JIT 默认是关的**（`jit.status()==false`，`jit.on()` 才 true）；② 它是 **Lua 5.1**、stock 引擎是 **Lua 5.3**（`utf8` 库缺失、`math.type` 缺失，连 `//` 都是语法错误）；③ 纯 Lua 层微基准（N=200 万，最好值）：numeric/string/table = **Lua 5.3 18–24/92–124/230–253 ms**、LuaJIT(JIT off) **11–14/57–66/88–131 ms**、LuaJIT(JIT on) **3/39–49/111–122 ms** ⇒ Lua 层本身**快 2–8×**（[推断] 若 CJK 字体层的耗时确在 Lua，JIT 本可吃掉一大块；受 §7.5 限制无法验证） |
| 2 | **PDF 压缩调优**（`\pdfvariable compresslevel=0 objcompresslevel=0`） | min-zh：PDF **281,486 B**（原 96,362 B，2.9×）而耗时 **3817 ms vs 3704 ms**（噪声内，甚至更慢） | ⛔ 无收益 |
| 3 | **预编译导言区**（fmt） | dump 成功（§3.4）；导言区占 LuaLaTeX 单趟 **44%**（2869/6468 ms）→ 上限约 **2.9 s** | 🟡 **唯一大杠杆**，但需 `mylatexformat` 等价实现（本机未装），未落地 |
| 4 | **引擎内页指纹**（恢复 B/C） | 原型打通、钩子零开销（§7.2） | 🟡 可行，收益 ~100 ms，风险不成比例，未落地 |

### 7.4 拉齐之后的对比

同一份夹具、同一台机器、各自最优形态（中文 3 次取中位；英文文档用于隔离"PDF 后端"这一项）：

| 场景 | XeLaTeX（我们的管线） | LuaLaTeX（对等优化后） | 倍率 |
|---|---|---|---|
| 中文 27 页单趟 | **1870 ms**（`-no-pdf`） | **6468 ms**（直接写 PDF） | 3.5× |
| 中文 74 页单趟 | **2189 ms** | **7111 ms** | 3.2× |
| 同上，含 PDF 产出（用户视角） | 2992 ms | 6468 ms | 2.2× |
| 仅导言区（固定开销） | 1565 ms（占单趟 **84%**） | 2869 ms（占单趟 **44%**） | 1.8× |
| **英文**同结构文档单趟 | 2341 ms | **1878 ms** | **0.80×（LuaLaTeX 更快）** |
| 英文：其中"产出 PDF"这一步 | xdvipdfmx 转换 ≈ **1125 ms** | 引擎内写 PDF ≈ **288 ms** | — |
| 首次使用 | ~0 | 建字体库 **25.8 s / 23.6 MB** | — |
| 完整编译（latexmk 收敛，首跑） | 9193 ms | 16686 ms | 1.8×（第二次跑两边都被判"已是最新"~0.9 s，不可比） |

**读法（这是本节最重要的一句）**：拉齐之后排名没有变，但**原因被定位了**——
- **不是"LuaLaTeX 引擎慢"**：英文文档里它**反而更快**（1878 vs 2341 ms），且它的 PDF 产出方式（引擎内写，~288 ms）比 XeLaTeX 的"先 XDV 再转换"（~1125 ms）更省。
- **差距全在 CJK 字体层**：XeTeX 的 native font 是**引擎内编译进去的**，`xeCJK` 只是薄薄一层；而 LuaLaTeX 走 `luatexja + luaotfload`，字体加载与字距调整都是 **Lua 层实现**——中文场景 3.2–3.5× 的差距就从这里来。
- 因此"给 LuaLaTeX 做针对性优化"的正确落点是**固定开销里的字体/导言区部分**（§7.3 第 3 项，上限 2.9 s，能把 3.5× 压到约 1.9×），而不是页级复用（~100 ms 量级）或压缩之类的边角。

### 7.5 LuaJIT 源码实测：能不能用它跑 LaTeX（2026-09 追加）

拿到 LuaJIT 源码（`E:\Works\luajit`，v2.1 分支）后把"不可用"从"TL 没提供格式"追到了**确切的一行**：

**① 正规形态建格式 → 内核 Lua 直接解析失败**

上一轮我手工建格式时把 `language.dat language.dat.lua` 当**命令行输入**传了进去——那是错的：在 fmtutil 的数据模型里它们是 **hyphen 字段**，命令行只该传 ini 文件。改正后复现（`language.dat` 由 kpathsea 按 `-progname` 解析）：

```powershell
luajithbtex -ini -interaction=nonstopmode -halt-on-error -jobname=ljlatex -progname=ljlatex lualatex.ini
# → error loading module ltluatex from file .../base/ltluatex.lua:
#      .../ltluatex.lua:335: unexpected symbol near '/'
# → !  ==> Fatal error occurred, no output PDF file produced!     （没有 dump 出任何格式）
```

`ltluatex.lua:335` 是 `for i=2, length//2 do`——**Lua 5.3 的整除运算**（同段还用了 `table.move`）。
Lua 是整块解析的，所以这一处足以让**整个 LaTeX 内核加载不了**，与是否走到那条分支无关。
> 这也解释了为什么 TL2026 的 `fmtutil.cnf` 只给 JIT 引擎配 plain 格式（`luatex.ini`）：**不是漏了，是 LaTeX 侧过不去**。

**② 用你给的源码构建 + 能力探针（VS18 / MSVC 14.51）**

```powershell
cmd /c "call `"C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat`" && cd /d <src>\src && msvcbuild.bat"
# → === Successfully built LuaJIT for Windows/x64 ===
```

| 探针 | 你源码构建的 LuaJIT | TL 自带的 luajittex | stock 引擎 |
|---|---|---|---|
| 版本 | `LuaJIT 2.1.1788856981`（2026 rolling） | `LuaJIT 2.1.81742` | Lua 5.3 |
| `_VERSION` | **Lua 5.1** | **Lua 5.1** | Lua 5.3 |
| `jit.status()` 默认 | **true（JIT 开）** | false（要手动开） | 无 jit |
| `table.move` | ✅（新版补了） | ✅ | ✅ |
| `5 & 3`（位运算） | ✅（新版补了） | ❌ | ✅ |
| **`4//2`（整除）** | ❌ **syntax error** | ❌ syntax error | ✅ |
| `utf8` / `math.type` / `string.pack` / `goto` | ❌ / ❌ / ❌ / ❌ | 同左 | ✅（goto 5.3 也有） |

⇒ **即使是最新的 LuaJIT 2.1，也解析不了 `ltluatex.lua`**——因为 LuaJIT 有意停留在 5.1 语法（缺的正是 `//` 这类 5.3 新增运算符）。
（LuaJIT 的 5.2 特性如 `goto` 需要**构建时**开 `-DLUAJIT_ENABLE_LUA52COMPAT`，两个现成构建都没开。）

**③ 真要跑起来得改多少（静态扫描 5.3 专属语法）**

| Lua 树 | .lua 文件 | 5.3 专属语法点 |
|---|---|---|
| LaTeX 内核 `base`（`ltluatex.lua`） | 1 | **1**（`//`；`table.move` 新版 JIT 已有） |
| `luaotfload` | 93 | **182**（goto/标签 127、`utf8.` 26、`//` 10、位运算 10、`string.pack` 5、`math.type` 4） |
| `luatexja` | 38 | **14**（`//` 9、位运算 5） |
| `lua-uni-algos` | 10 | **30**（`string.pack` 8、`utf8.` 7、`//` 7、goto 4、`math.type` 2、位运算 2） |
| **合计** | **142** | **227 处**（开 `LUA52COMPAT` 可消掉 127 处 goto，**残余仍约 79 处**；其中 `math.type` 是**语义差异**——5.1 没有整数子类型，polyfill 不了） |

**④ 结论：不做**（三条硬理由）
1. **要改的是用户机器上的 TeX Live 内核与宏包**（`ltluatex.lua` / `luaotfload` / `luatexja`）。我们不分发 TeX Live；用 `TEXINPUTS`/`LUAINPUTS` 覆盖等于给每个用户塞一份补丁副本，且 **TL 一升级补丁就失效**。
2. 与 ADR-0003（签名分发、不自维护引擎分支）冲突——这正是上游 TeXpresso 那条路线被否的原因之一。
3. **收益不确定**：Lua 层实测快 2–8×，但 §7.4 只把 3.5× 归因到"CJK 字体层"，层内 Lua 占多少**没有 profile**；而代价是 79+ 处语义级补丁的长期维护。
> 触发重估的条件（都在我们手里之外）：LuaJIT 上游支持 `//`，或 LaTeX 内核放弃 5.3 专属语法。
