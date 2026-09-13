# 比 XeLaTeX 更现代、又能排中文的引擎：盘点与实测

> 问题来源：「有没有支持中文、相比于 XeLaTeX 更加现代化的 TeX 引擎」。
> 姊妹篇：[no-fork-alternatives.md](./no-fork-alternatives.md)（§3.1/§3.2 的成本分解与 fmt 实测）、[incremental-edit-x-dvi.md](./incremental-edit-x-dvi.md)（页级复用 A/B/C）、[g1-read-interception-feasibility.md](./g1-read-interception-feasibility.md)（拦截每一次 I/O）。
> **结论一句话**：**有——LuaLaTeX（本机 = LuaHBTeX 1.24.0 + `luatexja`）是唯一"现代化且能排中文"的 LaTeX 引擎**：HarfBuzz 整形、100+ 个引擎内 Lua 回调、格式可 dump。但**对 Latteset 现在切过去是净亏**：同一份 28 页中文论文单趟 **6.47 s vs XeLaTeX 1.87 s（3.5×）**；更硬的是**我们的 Quick 路径在 LuaLaTeX 下实测是坏的**（§3.1 两种失败模式），而页级复用依赖的 XDV 在中文 LuaLaTeX 下**根本不存在**（`luatexja` 明确不支持 DVI 输出）。**它真正值钱的地方不在速度，而在"引擎内回调"**——那恰好是上游阶段 6/7 需要、我们现在只能在进程外做的事（§3.3）。

## 0. TL;DR

| 问题 | 答案 |
|---|---|
| 有比 XeLaTeX 现代、又能排中文的引擎吗 | ✅ **LuaLaTeX** = LuaHBTeX 1.24.0（TeX Live 2026）。中文走 `ctex → luatexja`；本机两条夹具（28 页 / 74 页）都编出来了，**页数与 XeLaTeX 一致** |
| 还有别的候选吗 | **Tectonic**（Rust、XeTeX 衍生、自动取包、可复现）——**本机未安装，全部结论未测**；**LuaJITTeX**（TL 自带，JIT，但无 HarfBuzz）；Typst / ConTeXt 不是 LaTeX 替代品 |
| 快吗 | ❌ **3.2–3.5× 慢**（§2 表）：28 页 6468 vs 1870 ms；74 页 7111 vs 2189 ms |
| 首次使用贵吗 | ❌ 冷启动要建字体缓存：**25.8 s / 23.6 MB / 35 个文件**，且缓存目录必须可写 |
| 我们的 Quick（编辑触发单趟）能用吗 | ❌ **不能用**（§3.1 实测）：干净项目 → `xdvipdfmx` 找不到 `.xdv` **报编译失败**（引擎其实成功了）；有遗留 XDV 时 → **转出上一次 XeLaTeX 的 PDF 并覆盖 LuaLaTeX 的产出，还报成功** |
| 页级复用（B/C/A）还能做吗 | ❌ 中文 LuaLaTeX **产不出 DVI/XDV**（`luatexja`：`DVI output is not supported in LuaTeX-ja`）→ 现有页哈希机制无输入。**但** LuaTeX 提供引擎内逐页回调，可做等价物（§3.2 实测到逐页指纹） |
| 那它有什么 XeTeX 给不了的 | ✅ **进程内拦截每一次 I/O**（`open_read_file` / `find_read_file`，实测拦到 4 次真实读取）、逐页/逐行/逐字回调、`finish_synctex`、`start_run`/`stop_run`（§3.3 全清单） |
| fmt（[no-fork §3.2](./no-fork-alternatives.md) 在 XeTeX 上被硬否决）在 Lua 侧呢 | 🟡 **dump 确实成功**（XeTeX 是彻底禁止），但**普通 ctex 文档的运行时挂接我没做通**（那正是 `mylatexformat` 的角色，本机未装）→ **收益未验证**（§3.4） |
| 建议 | 保持 XeLaTeX 为默认；**先把设置面板里 LuaLaTeX 的说明与 Quick 路径的缺陷对齐**（要么修 Quick，要么明确降级为"完整编译"）——见 §4 |

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

### 3.1 Quick 路径：实测坏了（两种失败模式）

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
3. 形态提醒：`pre_output_filter` 只在**输出例程**触发（本次 1 页文档触发了 5 次，因为多个 `\shipout` 盒子），要拿"文档第 N 页"仍需结合页号回调。

### 3.4 fmt：Lua 侧 dump 成功，但运行时挂接没做通

[no-fork §3.2](./no-fork-alternatives.md) 实测 XeLaTeX **彻底不能 dump**（连最简导言区都报 `Can't \dump a format with native fonts or font-mappings`）。Lua 侧不一样：

| 尝试 | 结果 |
|---|---|
| `lualatex -ini … \dump`，导言区 = `\documentclass{article}` | ✅ **dump 成功**：`fmtmin.fmt` 3.7 MB；且用 `-fmt=fmtmin.fmt` 跑只含 `\begin{document}…` 的正文 → **exit 0，出 1 页 PDF** |
| 同上，导言区 = `\documentclass[UTF8]{ctexart}` | ✅ dump 成功：`fmtctex.fmt` **11.3 MB**（`Beginning to dump on file …`） |
| 同上，导言区 = `thesis` 的完整导言区（ctexbook + amsmath + tikz + 标题） | ✅ dump 成功：`zhfmt2.fmt` **12.5 MB** |
| 用 ctex 格式跑"只有正文"的 `\begin{document}…` | ❌ `! LaTeX Error: Missing \begin{document}.`（换 `article` 格式则正常）→ **普通文档要用预编译导言区，需要 `mylatexformat` 那套类状态机**（本机 `kpsewhich mylatexformat.sty` 为空 = 未安装），我自写的 shim 也会破坏 ctex 的运行时状态 |
| （附带）dump 过程会打印一条**非致命** Lua 错误：`lua-uni-algos/lua-uni-stage-tables.lua:141: attempt to perform arithmetic on a string value` | 不加 `-halt-on-error` 时能继续并成功；加了就直接中止 → 说明这条错误在 TL2026 上是**已知噪音级**问题，但它会让"用 `-halt-on-error` 构建格式"的做法失败 |

⇒ 结论：**"LuaLaTeX 能 fmt" 在机制上成立（引擎不禁止），但在本机没验证出可用收益**。而且即便挂通，它省下的导言区时间（~1.5 s 量级）也补不回 LuaLaTeX 相对 XeLaTeX 的 4.6 s 差距。

## 4. 结论与建议

1. **回答原问题**：是，**LuaLaTeX（LuaHBTeX + luatexja）就是"支持中文且比 XeLaTeX 现代"的那个引擎**；Tectonic 是另一条更激进的路（换生态、自动取包，本机未测）。
2. **但不建议现在切换默认引擎**，理由是三条实测硬约束（不是口味问题）：
   - **慢 3.2–3.5×**（28 页 1.87 → 6.47 s），直接摧毁"编辑即预览"的体验预算；
   - **Quick 路径在 LuaLaTeX 下是坏的**（§3.1：干净项目误报失败 / 有残留时产出错引擎 PDF），而 Quick 是 ㉘ 的支柱；
   - **页级复用（B/C/A）在中文下失去输入**（`luatexja` 不支持 DVI），等于把我们刚做完的三个优化点清零。
   另外还有两个产品级成本：**首跑 26 s 建字体缓存**、**要求可写的 `TEXMFVAR`**。
3. **真正值得吸收的是"引擎内回调"这条能力**（§3.3）：进程内拦截每一次 I/O、逐页/逐行钩子、结构化错误捕获、SyncTeX 收尾——这些正是上游阶段 6/7 与 G1 报告里我们只能"在进程外绕"的东西。若将来要认真做实时渲染，**"换 LuaLaTeX 拿回调"比"打补丁改 XeTeX"更现实**（无需自维护引擎分支），代价就是上面那三条。
4. **近期该修的产品缺陷**（本次实测发现，已记入 [modules.md](../modules.md) §12 已知债）：
   - Quick 路径的 `-no-pdf` 与 `xdvipdfmx` 收尾**只对 XeLaTeX 成立**；对 LuaLaTeX/pdfLaTeX 应跳过转换（或直接降级为 Full），否则用户切引擎后编辑期全是错误；
   - `tmp/<stem>.xdv` + `tmp/<stem>.pages` **需要按引擎区分**（或至少在引擎变化时失效），否则跨引擎残留会互相污染（§3.1②）；
   - 设置面板 `LuaLaTeX` 的提示（"Lua 脚本、最新特性"）应写明这些代价。

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
```

## 6. 未验证与局限

1. **Tectonic 完全未测**（本机未安装）：它的"现代性"（自动取包 / 可复现 / 增量缓存）全是**外部资料判断**，没有本机数字；它对中文的实际表现（bundle 里的 Fandago 字体、系统字体、`ctex` 可用性）也未知。
2. **LuaJITTeX 未测**（已装未跑）：它没有 HarfBuzz，中文整形质量与速度都是空白。
3. **fmt 的收益数字没拿到**：dump 成功、`article` 格式运行成功，但 **ctex 文档的运行时挂接失败**（需要 `mylatexformat` 或等价实现，本机未装）→ "LuaLaTeX 上 fmt 到底省多少"仍是空白。
4. **回调只做了"能注册、能触发"这一层**：逐页指纹只验证了回调会带页节点表来（`nodes=`/`fnv=`），**没有验证把它接成 B/C 的页级复用是否真的正确**（需要与"PDF 实际变化页"对齐）；"零侵入拦截 I/O"的正确 reader 实现也**没做通**（§3.3 坑 2）。
5. **只测了合成夹具**：`thesis`/`multifile` 每页内容偏少；真实学位论文（图表/公式密集）里 LuaLaTeX 的相对劣势可能变大也可能变小，**未测**（hithesis 档仍被 ㉖ 阻塞）。
6. **只测了单趟**：多趟收敛（latexmk）只测了 `thesis` 的 Full 一次（18.6 s），没有系统性对比 XeLaTeX Full。
7. **慢的原因没有归因**：3.5× 里有多少是 HarfBuzz 整形、多少是 `luatexja` 的字距调整、多少是 PDF 后端的开销——**没有 profile**。
8. **跨引擎 PDF 等价性只到"页数一致"**：没有做像素级/文字流级对拍（`pdftotext` 在 io-off/io-on 之间做过一次文本一致校验，但那不是跨引擎对照）。
