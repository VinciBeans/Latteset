# 比 XeLaTeX 更现代、又能排中文的引擎：盘点与实测

> 问题来源：「有没有支持中文、相比于 XeLaTeX 更加现代化的 TeX 引擎」。
> 姊妹篇：[no-fork-alternatives.md](./no-fork-alternatives.md)（§3.1/§3.2 的成本分解与 fmt 实测）、[incremental-edit-x-dvi.md](./incremental-edit-x-dvi.md)（页级复用 A/B/C）、[g1-read-interception-feasibility.md](./g1-read-interception-feasibility.md)（拦截每一次 I/O）。
> **结论一句话**：**有——LuaLaTeX（本机 = LuaHBTeX 1.24.0 + `luatexja`）是唯一"现代化且能排中文"的 LaTeX 引擎**：HarfBuzz 整形、100+ 个引擎内 Lua 回调、格式可 dump。但**对 Latteset 现在切过去是净亏**：同一份 28 页中文论文单趟 **6.47 s vs XeLaTeX 1.87 s（3.5×）**——而且 §7 把两边优化**拉齐之后结论不变**（英文文档里 LuaLaTeX 反而更快 ⇒ 3.5× 是**中文/字体层**的差距，不是引擎代差）。另外两条硬伤：**我们的 Quick 路径在 LuaLaTeX 下实测是坏的**（§3.1 两种失败模式，**已修**），而页级复用依赖的 XDV 在中文 LuaLaTeX 下**根本不存在**（`luatexja` 明确不支持 DVI 输出）。**它真正值钱的地方不在速度，而在"引擎内回调"**——那恰好是上游阶段 6/7 需要、我们现在只能在进程外做的事（§3.3）。

## 0. TL;DR

| 问题 | 答案 |
|---|---|
| 有比 XeLaTeX 现代、又能排中文的引擎吗 | ✅ **LuaLaTeX** = LuaHBTeX 1.24.0（TeX Live 2026）。中文走 `ctex → luatexja`；本机两条夹具（28 页 / 74 页）都编出来了，**页数与 XeLaTeX 一致** |
| 还有别的候选吗 | **Tectonic**（Rust、XeTeX 衍生、自带 bundle、可复现）——✅ **已装上并实测（§8）**：中文可用（0 缺字）、出 PDF 与 XeLaTeX 打平（28 页 2515 vs 2539 ms）、比 LuaLaTeX 快 2.4×，**且它的 XDV 能被我们现有页哈希工具直接解析（28 页）**；代价是自带 bundle + 默认联网按需取文件（首跑 51–214 s，可 `-b`/`-C` 离线化）。~~LuaJITTeX~~ ❌ [已测并否决](#73-试过的-lualatex-针对性优化逐项结论)：TL2026 **没有 luajitlatex**（`fmtutil.cnf` 只有 plain 的 `luajittex`），手工建 JIT+LaTeX 格式也失败；Typst / ConTeXt 不是 LaTeX 替代品 |
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

# 10) Tectonic（§8，2026-09 装好）
tectonic --version                                     # 0.17.0（官方 Windows MSVC 二进制，装在 ~/.cargo/bin）
tectonic --keep-logs min-zh.tex                        # 首次会联网按需下载（本机 214 s / 104 个文件）
tectonic --outfmt xdv --keep-logs main.tex             # → main.xdv
node scripts\xdv-report.mjs main.xdv                   # 我们的工具直接解析：28 页
tectonic --synctex --keep-logs main.tex                # → main.synctex.gz
pwsh -NoProfile -File "$lab\tectonic-bench.ps1" -Rounds 3   # 交错对比 tectonic / xe-nopdf / xe-pdf / lua-pdf
```

## 6. 未验证与局限

1. ~~**Tectonic 只做了源码级评估**~~ ✅ **已装上并实测**（§8）：中文可用（0 缺字）、出 PDF 与 XeLaTeX 打平（28 页 2515 vs 2539 ms）、比 LuaLaTeX 快 2.4×、`--outfmt xdv` 的产物能被我们现有工具解析（28 页）、`SOURCE_DATE_EPOCH` 下 PDF/XDV 逐字节可复现、SyncTeX 可用。
   **仍未测**：真实学位论文模板（图表/公式密集）上的表现、bibtex/biber 全链路、`-b <本地 bundle>` 的离线实测、与 Latteset 现有"打开项目 → 探测根文件 → SyncTeX"链路的适配成本、bundle 的分发体积与许可。
2. ~~**LuaJITTeX 未测**~~ ✅ **已测并否决**（§7.3 #1）：TL2026 的 `fmtutil.cnf` 里**没有 luajitlatex**（只有 plain 的 `luajittex`/`luajithbtex` + `luatex.ini`），手工给 JIT 引擎建 LaTeX 格式也失败 ⇒ 这个版本上"JIT + LaTeX + 中文"**没有受支持的配置**（因此它是否更快仍是空白，但没有可用的落地形态）。
3. **fmt 的收益数字没拿到，但"能不能用"已有答案**：dump 成功（`article` 格式运行成功），**中文文档运行时失败**——根因是 `luatexja`（§7.6 五组对照）。**上限已量化**：导言区占 LuaLaTeX 单趟 44%（2869/6468 ms，§7.4），但这条上限拿不到。
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
| 预编译导言区（fmt） | ⛔ 引擎**禁止** dump（no-fork §3.2） | ⛔ **实测对中文文档不可用**（§7.6）：装上 `mylatexformat` 后英文/fontspec 能过，**`luatexja` 必失败**（`! LaTeX Error: Missing \begin{document}`）——而中文在 LuaLaTeX 上必经 luatexja |

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
| 3 | **预编译导言区**（fmt） | dump 成功（§3.4）；导言区占 LuaLaTeX 单趟 **44%**（2869/6468 ms）→ 上限约 **2.9 s** | ⛔ **实测关死（§7.6）**：装上 `mylatexformat` 后，**中文文档运行时必失败**（`! LaTeX Error: Missing \begin{document}`）；根因是 **`luatexja`**（英文 / fontspec 都能过）⇒ 这条杠杆对目标用户不存在 |
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
- 因此"给 LuaLaTeX 做针对性优化"的正确落点**本应是**固定开销里的字体/导言区部分（上限 2.9 s，能把 3.5× 压到约 1.9×）——但**这条路本轮实测也关死了**（§7.6：预编译导言区在 luatexja 上必失败；§7.3 #1：JIT 在 TL2026 无 LaTeX 格式）。
  ⇒ **拉齐之后，LuaLaTeX 已经没有可用的大杠杆了**：3.2–3.5× 是当前 TeX Live + 当前 luatexja/luaotfload 的结构性结果，不是我们管线没优化。

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

### 7.6 mylatexformat 实测：预编译导言区对中文文档不可用（2026-09 追加）

§7.3 #3 说过"预编译导言区是 LuaLaTeX 唯一真正大的杠杆（上限 2.9 s）"。拿到 `mylatexformat`
（`E:\Works\mylatexformat`，v3.4 / 2011）后把它真正跑起来——**结论是这条路对中文文档不通**。

**① 装包（不碰用户的 TeX Live）**

包只给了 `.dtx`，按它自己的说明（"Unpacking (b): `etex mylatexformat.dtx`"）提取：

```powershell
etex -interaction=nonstopmode mylatexformat.dtx      # → mylatexformat.ltx（12.6 KB）+ .ins/.drv
# 注：pdflatex 跑 dtx（提取+文档）会先死在文档部分：Package etoolbox Error: \scr@load@hook undefined
```
把 `mylatexformat.ltx` 放进**工作区内**的 texmf 树，用 `TEXMFHOME` 指过去即可（`kpsewhich mylatexformat.ltx` 验证命中）。

**② 用法（建格式 + 运行时，**不改用户文档**）**

```powershell
# 建格式（README 的形态；本机 lualatex 对应 &lualatex）
lualatex -ini -interaction=nonstopmode -jobname=thfmt '&lualatex' mylatexformat.ltx main.tex
# 运行时：用 -fmt 指到格式文件即可，文档一个字都不用改
lualatex -fmt="<abs>\thfmt.fmt" -interaction=nonstopmode -halt-on-error -output-directory=tmpfmt main.tex
```

**③ 五组对照（全部本机实测）**

| 文档 | 引擎 | 建格式 | 运行时 `-fmt` |
|---|---|---|---|
| `article`（英文） | lualatex | ✅ 4.7 MB | ✅ **出 PDF 1 页**（`CUSTOMISED FORMAT: "enfmt"` → `Output written`） |
| `article + fontspec` | lualatex | ✅ 6.2 MB | ✅ 出 PDF |
| `article + luatexja` | lualatex | ✅ 10 MB | ❌ `! LaTeX Error: Missing \begin{document}.` |
| `article + [UTF8]{ctex}` 宏包 | lualatex | ✅ 25.1 MB | ❌ 同上 |
| `ctexart`（`min-zh`）/ `ctexbook`（thesis 27 页） | lualatex | ✅ 25.8 / 26.4 MB | ❌ 同上（用显式 `\endofdump` 标记重建格式也**一样失败**） |
| `article`（英文） | **xelatex** | ⚠️ 写出了 6.2 MB 文件，但 dump 报 `Can't \dump a format with native fonts or font-mappings` | —（格式不可用） |

⇒ **根因锁定在 `luatexja`**：不是 ctex、不是 fontspec、也不是文档类（`fontspec` 能过，`luatexja` 不能）。
而 **LuaLaTeX 上排中文必经 `luatexja`**（ctex 在 LuaTeX 下就是走它）——所以"预编译导言区"这条杠杆**对目标用户不存在**。
（顺带否掉 README 里"XeTeX 可用"那条 2011 年的说明：今天 XeTeX 的 native font 限制依旧。）

**④ 影响**

- §7.3 #3 的"上限 2.9 s"作废；§7.4 的公平对比**不会再变**：LuaLaTeX 侧已无可用的大杠杆（JIT 见 §7.5、fmt 见本节、页指纹只有 ~100 ms）。
- 仍未测的一个想法：~~**只 dump 非 CJK 宏包**（amsmath/tikz…）~~ ✅ **已测并否决（§7.6 ⑤）**：能编译但**静默丢掉全部中文**（398 条 Missing character），且上限只有 ~519 ms。
- 版本边界：mylatexformat v3.4（2011）× TeX Live 2026（ctex 2.x / luatexja 2025-09）——上游任一更新都可能改变结论。

**⑤ "只 dump 非 CJK 包"也试了：能编译，但会静默丢掉全部中文**（本节最初列为"未测"，现补上）

构造一个**不含 ctex/luatexja** 的格式（`article` + amsmath/amssymb/graphicx/booktabs/hyperref，4.7 MB），拿去编译中文文档：

```powershell
lualatex -ini -interaction=nonstopmode -jobname=partial '&lualatex' mylatexformat.ltx partial-pre.tex   # partial-pre.tex 以 \endofdump 结尾
lualatex -fmt="<abs>\partial.fmt" -interaction=nonstopmode -output-directory=tmpP min-zh.tex
```

| 结果 | 数值 |
|---|---|
| 编译退出码 / 产出 | **exit 0**，PDF 照出（74,349 B，1 页）——**不是报错** |
| 丢失字符 | **`Missing character` 警告 398 条** |
| PDF 实际文字 | `Engine comparison baseline (Chinese) … ��`（中文全没了，英文/数字还在） |
| 同一格式编译英文文档 | ✅ 完全正常（0 条 Missing character） |

原因是 mylatexformat 的机制（读它的源码确认：它**没有**去 patch `\documentclass`/`\usepackage`）：**运行时把用户文档的整个导言区扫掉丢弃**，只在 `\begin{document}` 处恢复读取 ⇒ 文档自己的 `\documentclass[UTF8]{ctexart}` 与 `ctex`→`luatexja` **根本不会被执行**。
所以"只 dump 非 CJK 包"不是"省一部分"，而是**把 CJK 层整个绕过去了**——而且是最坏的失败形态：**编译成功、预览静默缺字**。

**顺带量了这项的"上限"**（干净状态下同批实测，导言区两档）：`ctexbook` 空文档 **2585 ms** vs 论文完整导言区空文档 **3104 ms** ⇒ 非 CJK 宏包只值 **~519 ms**（约占 27 页全文 5727 ms 的 **9%**）。也就是说：即便这个玩法能用，收益也不到一成的十分之一量级；而它连"能用"都不成立。

### 7.7 测量方法学：本轮踩到的三类坑（含一次故障复盘）

本节所有"倍率"结论都建立在同一批交错测量上，不是跨会话比绝对值——这是被现实教育出来的：

**① 故障复盘：断电 → fontconfig 缓存失效 → XeLaTeX 每次编译多 4 秒**
某轮实测里 `min-zh`（1 页）的 xelatex 从 1370 ms 变成 **5723 ms**、极简文档要 4.8 s，而同期 `lualatex` 完全正常、Lua 层 CPU 微基准也在基线。定位链条：
- 进程启动：`cmd /c exit` 19 ms、`lualatex --version` **16 ms**、`xelatex --version` **495–690 ms** ⇒ 只有 XeTeX 慢；
- 文件读：400 个宏包文件热读 **178 MB/s**（首遍冷读只有 1.6 MB/s，但那是另一回事）⇒ 磁盘不背锅；
- 字体：`fc-list` **4.2–4.7 s**、`fc-cache -v` 报 `invalid cache file`、四处候选缓存目录**全都不存在** ⇒ **fontconfig 缓存失效，每次重新扫 3,640 个系统字体**。
`fc-cache -f` 重建后：极简文档 4799 → **790 ms**、`min-zh` 5380–5765 → **1371 ms**、`fc-list` 4.2 s → 0.7 s。**结论：这是断电的次生故障，不是"电脑变慢"**（详见 [troubleshooting.md](../troubleshooting.md) 同名条目）。
> 期间的所有绝对数字**作废**。修好后同批复测：`thesis` **xe-nopdf 1813 ms vs lua-pdf 5727 ms = 3.2×**、`min-zh` 1609 vs 3837 ms = 2.4×——与 §7.4 记录的 1870/6468（3.5×）一致 ⇒ **§7.4 的结论稳健**。

**② 静默作废的样本**（最危险的一类：跑得"快"其实是失败）
- 论文夹具 `\include{chapters/chNN}` + `-output-directory`：目标目录里没有 `chapters/` 子目录 → `I can't write on file 'chapters/ch01.aux'` → Emergency stop（4 s 的"成功"其实是 exit 1）；
- 文件锁/残留进程 → `I can't write on file 'min-zh.log'` → 同样静默早退。
⇒ **规则：每次运行都要验 `Output written` 才算有效样本**（本轮为此把测量脚本改成逐次校验 + 剔除无效样本并打印原因）。

**③ 协议（以后按这个来）**
1. **先预热**：每引擎每夹具先跑一次不计入（把宏包/字体/OS 缓存焐热）；
2. **同批交错**：A/B 两档交替跑 N 轮，别"先跑完 A 再跑 B"（机器状态会漂）；
3. **逐次校验**：没见到 `Output written` 的样本直接丢弃并记录原因；
4. **带控制组**：CPU（Lua 微基准，本轮基线 numeric 18–24 ms）、磁盘 I/O（400 文件热读 178 MB/s）、字体（`fc-list` ~0.7 s）——控制组异常就先修环境，别测；
5. **只信同批比值**：跨会话的绝对值可以差 ±30%（缓存/温度/电源都影响），倍率才有意义。

## 8. Tectonic 0.17：装上并实测（2026-09 追加）

§1 把 Tectonic 列为候选，长期处于"未安装、全部未测"。本轮**把它装好并测完**：结论是 **Tectonic 在中文上与 XeLaTeX 打平、
比 LuaLaTeX 快 2.4×，而且它产出的 XDV 能被我们现有的页哈希管线直接吃**——代价是它**自带 bundle、默认联网按需取文件**。

### 8.1 结构事实（源码级）

- **XeTeX 衍生引擎**：`crates/engine_xetex`（42 文件，36 个 `.c/.h`）——引擎 C 源码在库内。
- **自带 Rust 版 xdvipdfmx**：`crates/engine_xdvipdfmx` → 不需要外部转换进程（这一点与 XeLaTeX 的"引擎 + `xdvipdfmx` 两步"不同）。
- **XDV 是一等输出格式**：`OutputFormat::Xdv`（`src/driver.rs`），CLI 有 `--outfmt`（`src/bin/tectonic/compile.rs:43`）。
- 26 个子 crate；`crates/xdv` 是一个 **Rust 的 XDV 解析器**，事件式（`trait XdvEvents` 带 **`handle_begin_page`**）+ `checkpoint()/current_offset()/process_with_seeks()`（`crates/xdv/src/lib.rs:115,128`）——正是我们 G2/阶段 3 手搓的"增量 + 字节偏移"能力。
- `src/io/format_cache.rs`：**按 bundle 摘要缓存已编译 format**；`src/bin/tectonic/v2cli/commands/watch.rs`：自带 watch 子命令；`crates/engine_spx2html` + `--outfmt html`：`.spx` 语义中间产物（对 roadmap ⑧⑨ 有参考价值）。

### 8.2 安装记录（怎么装的、装在哪、缓存多大）

源码包（`E:\Works\tectonic-tectonic-0.17.0`）**跑不起来**：tarball 不含子模块（`crates/bridge_harfbuzz/harfbuzz` 与
`reference_sources` 都是空目录），而 Windows MSVC 的官方构建路线要 **vcpkg**（`[package.metadata.vcpkg]` + 他们的
`cargo-vcpkg` 分支，见 `.github/actions/vcpkg-deps/action.yml`）——即一整套 C 依赖编译。所以按"装好可用"的目标，
用了**官方 0.17.0 的 Windows MSVC 二进制**（源码仍留作结构分析，未改动）：

| 项 | 值 |
|---|---|
| 来源 | GitHub release `tectonic@0.17.0` → `tectonic-0.17.0-x86_64-pc-windows-msvc.zip`（20.1 MB） |
| 校验 | 下载后 SHA-256 = 官方 digest `f61ce51f…845f` ✅ 一致 |
| 安装位置 | `C:\Users\Vinci\.cargo\bin\tectonic.exe`（该目录已在用户 PATH 上，无需改环境变量） |
| 版本 | `Tectonic 0.17.0` |
| 缓存目录 | `%LOCALAPPDATA%\TectonicProject\Tectonic\cache\{bundles,formats}`（本轮跑完 426 文件 / **62 MB**） |

**首跑成本（联网按需取文件，这是它最大的产品差异）**：`hello.tex` 首次 **189 s**；`min-zh.tex` 首次 **214 s**（日志里
104 行 `downloading …`：`ctexart.cls`、`xeCJK.sty`、`pdftex.map`…）；`thesis` 首次 **51 s**（73 行下载）。
之后同一份文档走本地缓存，回到下面的正常耗时。**离线可用性**：`-C/--only-cached` 只吃缓存，`-b/--bundle <URL|path>`
可以指向**本地 bundle**（即"自带一个 bundle 离线分发"这条路是通的）。

### 8.3 实测：中文 + 速度（本机、同批交错 3 轮、逐次校验产出）

夹具与 §2/§7.4 相同（`min-zh` 1 页、`thesis` 27–28 页中文）。`xelatex`/`lualatex` 记的是"能出 PDF 的形态"：

| 夹具 | **tectonic** | `xelatex -no-pdf`（只排版） | `xelatex`（出 PDF） | `lualatex`（出 PDF） |
|---|---|---|---|---|
| `min-zh`（1 页） | **1586 ms** | 1378 ms | 2350 ms | 3887 ms |
| `thesis`（28 页） | **2515 ms** | 1681 ms | 2539 ms | 5932 ms |

读法：
- **出 PDF 这一档，Tectonic ≈ XeLaTeX**（2515 vs 2539 ms；小文档上 Tectonic 还快 1.5×）——因为它把 TeX 与 xdvipdfmx 合成一步、且**内部自己收敛**（日志可见 `Rerunning TeX because "main.aux" changed`）；
- 比"只排版不出 PDF"的 `xelatex -no-pdf` 慢 1.5×（2515 vs 1681 ms）——那 800 ms 正是它多做的 PDF 工作；
- **比 LuaLaTeX 快 2.4×**（2515 vs 5932 ms）。

### 8.4 与既有机制的对接（这几条比速度更重要）

| 检查 | 结果 |
|---|---|
| **中文正确性** | ✅ `--keep-logs` 里 **0 条** `Missing character`；`pdftotext` 出来就是中文正文（`引擎对比基准（中文）…`），28 页 |
| **`--outfmt xdv`** | ✅ 产出 `main.xdv`（264,736 B；XeLaTeX 同文档是 261,776 B，同一格式） |
| **我们的页哈希工具能否吃它** | ✅ **但只在 `--outfmt xdv` 档**：`node scripts/xdv-report.mjs main.xdv` → **页数 28**、页字节 min/avg/max = 1038/9432/16101、解析 5.66 ms（44.6 MB/s）。⚠️ **默认 PDF 档永不落 `.xdv`**（2026-09-14 两轮实测：默认与**加 `-k`** 都是「无 `.xdv`」，机制是转换后从内存文件表移除，`driver.rs:1985`）⇒ **PDF 与页级复用不可兼得**：走 PDF 档则 `page_hashes` 恒空（`pages == 0` = 无法判定 = 保守全量刷新），B/C 失效；若沿用旧 `.xdv` 还会把"陈旧页集"误判成"逐页未变"（比 `runner.rs:463-466` 的覆盖事故更隐蔽）。A（跳过转换）在 Tectonic 下**无对应物**（同进程完成；TL-less 机器也没有 `xdvipdfmx`） |
| **构建确定性（㉚）** | ✅ `SOURCE_DATE_EPOCH=0` 连跑两次：**PDF 与 XDV 都逐字节一致**（80,203 B / 264,728 B，SHA-256 相同）。⚠️ **但这个变量在 Tectonic 上会改正文**（`\today` → 1970）⇒ 见 §8.6 |
| **SyncTeX** | ✅ `--synctex` → `main.synctex.gz`（77,983 B）；⚠️ 但我们**自己的** forward/inverse 走外部 `synctex.exe`（TeX Live-only）⇒ 免装 TL 时该链路不可用 |
| 产出与 XeLaTeX 的差异 | 28 页 vs 27 页、文本长度 15,227 vs 14,913：两者**都含参考文献**（已核：Tectonic `.bbl` 680 B / 9 条 `\bibitem`，PDF 文末有 `参考文献` + `[1]`…`[9]`），差异来自**趟数与 bibtex 次数**（Tectonic 那档跑了 9 次 bibtex + 内部收敛，`xelatex` 基线是单趟）⇒ 该计时对 Tectonic **保守** |
| 已知噪音（2026-09-14 更正） | ① `Fontconfig error: Cannot load default config file: No such file: (null)` —— 中文渲染正常（0 缺字），**良性**；② **bibtex 其实成功了**：主调用产出完整 `.bbl`（9 条）与含参考文献的 PDF，Tectonic 那句 `errors were issued by BibTeX, but were ignored` 全部来自 **8 次章级良性调用**（无 `\bibdata` 的章级 aux 也跑 bibtex，属引擎行为差异）；③ **真正要记住的坑：`.blg` 尾部会在成功时也被截断**（缺 `You've used N entries` 摘要行，537 B 止于 `Database file #1: refs.bib`，且**零 error 行**）⇒ **任何按 `.blg` 判 bib 成败的工具都会误判**——判据必须落在**产物**（`.bbl` 条数 / PDF 文本）上 |

### 8.5 结论（相对我们的取舍）

1. **Tectonic 是"现代化 + 中文可用 + 速度不亏"的那一个**：中文出 PDF ≈ XeLaTeX、比 LuaLaTeX 快 2.4×，
   并且**它的 XDV 就是我们的格式**——页级复用、确定性、SyncTeX 三件事都不用重写。
2. **但它的形态与我们的现状冲突在"文件从哪来"**：它自带 bundle（默认联网按需下载，首跑 51–214 s），
   **不读用户的 TeX Live**。我们现在的价值主张之一是"用户已有 TeX Live 就能用"，且我们的 ㉒/㉖/㉗ 一堆逻辑
   （`.fls`、模板 `.cls` 探测、`kpsewhich` 回路）都建立在"本机 TL"上。
3. ⇒ **值得试的方向不是"换引擎"，而是"把 Tectonic 当作免装 TeX Live 的备选后端"**：`-b <本地 bundle>` +
   `-C` 可离线化；`--outfmt xdv` 让现有页级机制原样复用。要落地还需回答：bundle 分发体积/许可、与用户 TL 的宏包差异
   （本机 `tex/luatex`、`context` 等被排除；`.lua` 文件也在 ignore 列表里）、以及首跑下载能不能接受。
   **落地计划、摩擦点清单与明天的四件事见 [tectonic-integration-plan.md](./tectonic-integration-plan.md)。**
4. **仍未测**：真实学位论文模板（图表/公式密集）上的表现；bibtex/biber 全链路；`--bundle` 指的本地 bundle 实测；
   与 Latteset 现有"打开项目 → 探测根文件 → SyncTeX"链路的端到端适配成本。

### 8.6 `SOURCE_DATE_EPOCH` 的引擎语义差异与裁决 (b)（2026-09-14 追加）

**同一个环境变量在两个引擎上不是同一件事**——这是本节唯一重要的一句：

| | 固定 `SOURCE_DATE_EPOCH=0` 的效果 |
|---|---|
| XeLaTeX / pdfLaTeX / LuaLaTeX | **只动 PDF 的 `/ID` 与时间戳**，正文零变化 ⇒ `\today` 仍印当天（实测：同一轮设了 epoch 的 XeLaTeX 输出印 `2026 年 9 月 13 日`） |
| Tectonic 0.17 | **改正文** —— 该变量被 `build_date_from_env`（`compile.rs:206` → `driver.rs:995-1010`）当作**引擎时间源**喂给 `\today`：设 `0` 印 `1970 年 1 月 1 日`、设 `1700000000` 印 `2023 年 11 月 15 日`；不设则 fallback `SystemTime::now()` |

本仓的 `compile_command` 是**无条件**设置该变量（`crates/latteset-infra/src/runner.rs:275`，Quick/Full、与引擎无关）⇒ 照抄给 Tectonic 会让**每一份用 `\today` 的文档**（含学位论文标题页）印 1970。

**不固定 epoch 时到底丢什么**（实测：最小文档 + 28 页样本 / 264,736 B XDV）：

| 对照 | XDV | PDF |
|---|---|---|
| 不设 epoch，同日连跑两次 | **逐字节相同**（SHA-256 相等） | 不同：**56 / 80,509 B（0.07%）**，全部落在 `/ID` 与 Info 时间戳区，**长度相同、`pdftotext` 文本相同** |
| 不印 `\today` 的文档，epoch `0` vs `1700000000` | **逐字节相同**，正文相同 | 仅元数据不同 |
| 印 `\today` 的文档，epoch 变化（= 跨天） | 不同：页 1 因日期串**多一位数字长 10 B**；**页 2 逐字节相同**；**页 3..N 长度全同、唯一差异在页内偏移 43–44** | 同上 |

⇒ **页级复用（A/B/C）吃的是 XDV，而不固定 epoch 时 XDV 依然稳定**；固定该变量对 Tectonic 唯一"买"到的是 **PDF 的逐字节可复现**，代价却是把正文日期压成 1970。

**页哈希口径（`bop` 的 prev 指针）**：上表第三行的偏移 43–44 就是 DVI/XDV 每页头 45 B `bop` 的**最后一个 i32 = 前一页 `bop` 的绝对字节偏移**；页 1 变长 ⇒ 页 3 起每页的 prev 跟着移动 ⇒ 25/26 页被判"变化"（**并非每页内容都变了**）。本仓 `crates/latteset-core/src/xdv.rs:211` 的 `hash_page(&bytes[bop..p])` 是**原始口径**（含 prev），已登记为已知债 **#26**（[modules.md](../modules.md) §12.1）。触发频率差别：XeLaTeX 固定 epoch ⇒ 该债几乎不触发；Tectonic 选 (b) ⇒ **会印日期的文档每天触发一次**（而其内容本就该变）。

**裁决（用户，2026-09-14）：选 (b) —— Tectonic 档不固定 `SOURCE_DATE_EPOCH`。** 落地口径是**按子进程施加**：Tectonic 进程不设（保正文日期）；同一次编译里若还要调外部 `xdvipdfmx`（路线②）则**该进程设 `0`**，㉚ 的逐字节确定性由它承担。现实现是无条件设置（`runner.rs:275`），**接入 Tectonic 时必须改**。附带推论（**待实测**，计划文档 U-31）：路线② 因此有望同时拿到"日期正确 + PDF 逐字节可复现"。逐条计划与判据见 [tectonic-test-plan.md](./tectonic-test-plan.md) §5.4 / E3.7 / E3.8 / P-G19。
