# VS Code + LaTeX Workshop 生态调研报告

> 调研日期：2026-09（数据抓取时点见各条）
> 调研人：产品调研分析（受 Latteset 项目委托）
> 抓取方式：GitHub REST API v3、VS Code Marketplace、Stack Exchange API、Hacker News Algolia API、PullPush（Reddit 归档）、官方 wiki raw 页
> 说明：本文件是**调研产物**，不是设计文档；未改动 `docs/README.md` 索引，是否纳入文档体系由项目方决定。

---

## 0. 方法与可信度声明

### 0.1 实际可访问性（重要）

| 目标 | 直连结果 | 采用方案 |
|---|---|---|
| `github.com` 网页 | `fetch failed`（多次） | **GitHub REST API**（`api.github.com`）——本报告所有 issue 数据均由 API 返回原文核实 |
| `tex.stackexchange.com` | HTTP 403 + Cloudflare | **Stack Exchange API** |
| `reddit.com` / `old.reddit.com` | DNS 解析到非公网 IP，被沙箱拦截 | **PullPush API**（Reddit 归档快照） |
| `news.ycombinator.com` | `fetch failed` | **HN Algolia API** |
| `marketplace.visualstudio.com` | 可直连 | 直连 HTML |
| `raw.githubusercontent.com/wiki/...` | 可直连（部分页面偶发失败，重试成功） | 官方 wiki 原文 |

**因此**：本报告中所有 GitHub issue 编号、标题、日期、标签、正文摘述均来自 `api.github.com` 的实际返回；所有引语均来自实际抓取文本，未构造任何 URL。

### 0.2 两个必须先讲清的事实

**事实 A：仓库当前 open issues = 0。** 三个独立查询一致：

- `GET /repos/James-Yu/LaTeX-Workshop` → `"open_issues_count": 0`
- `GET /repos/James-Yu/LaTeX-Workshop/issues?state=open` → `[]`
- `GET /search/issues?q=repo:James-Yu/LaTeX-Workshop is:issue is:open` → `"total_count": 0`

**事实 B：但 issue 仍在被持续提交，只是被极快关闭。** 例如：

- [#4976](https://github.com/James-Yu/LaTeX-Workshop/issues/4976)「Incorrect parsing and passing of TEX magic options」创建 `2026-09-07T12:24`，关闭 `2026-09-08T05:08`；
- [#4977](https://github.com/James-Yu/LaTeX-Workshop/issues/4977)「Add top padding for pdf viewer」创建 `2026-09-07T22:11`，关闭 `2026-09-08T04:57`。

[推断] 维护者采用「快速分诊 / 即提即关」策略，因此**不能用「open issue 数」推断「没有问题」**，也不能用「有没有多年 open 的 issue」来回答「作者无解」——该信号已被维护流程抹平。本报告改用**维护者自己打的标签**与**官方 wiki 的明文免责声明**来识别架构性问题（见 §3）。

---

## 1. 量化用户规模 / 下载量数据（全部有来源）

| 指标 | 数值 | 来源 | 说明 |
|---|---|---|---|
| LaTeX Workshop 安装量 | **5,608,577** unique installs（同页另一次抓取读到 5,608,428） | [Marketplace](https://marketplace.visualstudio.com/items?itemName=James-Yu.latex-workshop) | 页面 `title` 明示为「unique installations, not including updates」；**不等于活跃用户** |
| LaTeX Workshop 评分 | **4.8 / 5，300 条评分** | 同上 | |
| LaTeX Workshop 版本 | **v10.18.0**（要求 VS Code ≥ 1.114.0，即 2026-04 之后） | 同上 | 版本门槛本身就是抱怨源之一 |
| GitHub stars / forks | **12,308 stars / 587 forks** | [API](https://api.github.com/repos/James-Yu/LaTeX-Workshop) | 创建于 2016-12-26 |
| GitHub open issues | **0**（见 §0.2） | 同上 | |
| TeXstudio stars / forks / open issues | **3,619 / 399 / 431** | [API](https://api.github.com/repos/texstudio-org/texstudio) | 对照：TeXstudio 反而有 431 个 open issue |
| Overleaf 用户数 | **1,000 万**（2022-06 官方博客「Ten million users」） | [Overleaf Blog](https://www.overleaf.com/blog/wow-ten-million-users) | 官方口径 |
| Overleaf 用户数（第三方估计） | 「over 19 million users」（2025 估计） | [GetLatka](https://getlatka.com/companies/overleaf.com) | **第三方估算，置信度低**，仅作量级参考 |
| 需求侧信号（SE 浏览量） | 「How to use LaTeX on VS Code」**299,276** views / 51 分 | [SE 462365](https://tex.stackexchange.com/questions/462365) | 浏览量是事实性指标 |
| | 「PDF Preview in Visual Studio Code」**89,448** views | [SE 527463](https://tex.stackexchange.com/questions/527463/pdf-preview-in-visual-studio-code) | |
| | 「latexmk: The script engine could not be found」**88,055** views | [SE 461954](https://tex.stackexchange.com/questions/461954) | |
| | 「custom recipes file location」**66,470** views / 36 分 | [SE 478865](https://tex.stackexchange.com/questions/478865) | |
| | 「Go to Source 不工作」**43,680** views / 37 分（最佳答案 85 分） | [SE 538797](https://tex.stackexchange.com/questions/538797) | |
| | 「Undefined citations in VS Code, works well in texstudio」**39,902** views | [SE 553023](https://tex.stackexchange.com/questions/553023) | |
| | 「How to use … with xelatex」**33,998** views | [SE 564758](https://tex.stackexchange.com/questions/564758) | |

**结论（有来源支撑）**：LaTeX Workshop 是 VS Code 生态中事实标准的 LaTeX 扩展，量级 ~560 万安装；同时，社区里浏览量最高的若干问题集中在**配置 / 编译 recipe / PDF 预览 / SyncTeX / 中文 xelatex** 这几类，而不是高级功能。

---

## 2. 用户抱怨最集中的前 10 类问题（按「出现频率 × 严重度」排序）

> 排序依据：官方 wiki 是否专门写章节、维护者是否打特殊标签、Stack Exchange 浏览量、GitHub issue 密度、社区讨论中反复出现的主题。
> 每条均标注 **【事实】**（有来源）或 **【推断】**。

### 1️⃣ 编译 recipe / 工具链配置复杂（最高频）

**【事实】**
- 官方 wiki 要求用户手写 `latex-workshop.latex.recipes` / `latex-workshop.latex.tools` 两套 JSON 结构，并理解 13 个 `%PLACEHOLDER%` 占位符（含 `_W32` 变体）。见 [wiki/Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile)。
- SE 上「自定义 recipe 放哪」**66,470 views**、[「latexmk: The script engine could not be found」](https://tex.stackexchange.com/questions/461954) **88,055 views**——高热度问题大量是纯配置问题。
- Reddit r/LaTeX：用户 `wpowell96`（14 分）称 LW「does have its learning curves for more advanced features like custom build recipes」；`theDekuMagic` 提醒「it does take some effort to learn how to use it」。来源：[r/LaTeX TeXstudio vs LaTeX Workshop](https://www.reddit.com/r/LaTeX/comments/15sup7o/texstudio_vs_latex_workshop_on_vscode/)。
- 用户 `GreatLich`（3 分）：「Having to (hunt for and) install extensions and configuring them **_is_ 'fighting the IDE' to me**」。来源：[r/LaTeX rkvq56/hpjisyh](https://www.reddit.com/r/LaTeX/comments/rkvq56/which_latex_ide_do_you_prefer/hpjisyh/)。
- **最新证据（2026-09）**：issue [#4976](https://github.com/James-Yu/LaTeX-Workshop/issues/4976) 报告 `%!TEX options = -lualatex -synctex=1 ...` 被整体加引号传给 latexmk，导致 `Latexmk: Bad options specified`——即「魔法注释 + 参数解析」这条配置路径至今脆弱。

**【推断】** 这是「通用编辑器 + 插件」模型的直接代价：编译链路不是扩展的原生概念，而是用户用 JSON 拼出来的。对 Latteset 而言，「开箱即用的编译配置」是明确的可差异化点。

---

### 2️⃣ SyncTeX 双向定位不可靠

**【事实】**
- 官方 wiki 明文：外部 PDF viewer 的 SyncTeX **「As of mid-2025, this function is [not working]」**，且整段标题写「**this function is not officially supported**」；`latex-workshop.view.pdf.viewer` 的 `"external"` 取值标注为 **Experimental**。来源：[wiki/View](https://github.com/James-Yu/LaTeX-Workshop/wiki/View)、[issue #4584](https://github.com/James-Yu/LaTeX-Workshop/issues/4584)（2025-04-28 提出，2025-04-29 关闭）。
- SE 最高票答案指出：「**The command `-synctex=1` must be present in your LaTeX Workshop recipe. The default latexmk recipe does not compile with this flag.**」并且「Show in Preview 按钮完全无反应，双击/ctrl+点击也无效」。来源：[SE 538797](https://tex.stackexchange.com/questions/538797/go-to-source-for-latex-on-vs-code-does-not-seem-to-work)（43,680 views / 37 分 / 最佳答案 85 分）。
- 另有答案指出：**删掉 `.synctex.gz` 辅助文件会让功能失效**。来源同上。
- 早期 issue [#1749「Synctex fails to work」](https://github.com/James-Yu/LaTeX-Workshop/issues/1749)（2019-10-13，仅来自搜索结果摘要，本次未逐条核实）。

**【推断】** SyncTeX 依赖 `-synctex=1`、`.synctex.gz` 文件生命周期、pdf.js 坐标映射三者对齐，而这三者分属「用户的 recipe」「构建产物」「VS Code webview」三个不同层，任何一层脱节都会静默失效。这是**架构层面的耦合缺陷**，不是单个 bug。

---

### 3️⃣ PDF 预览是「二等公民」（渲染缺陷 + 刷新跳页 + 丢滚动位置）

**【事实】**
- 官方 wiki 明文承认：若遇到「repeated viewer refreshes and/or **loss of PDF scrolling position**」，需调大 `latex-workshop.latex.watch.pdf.delay`；并警告可能「**potentially causing file corruption issues**」。来源：[wiki/View](https://github.com/James-Yu/LaTeX-Workshop/wiki/View)。
- 内置预览使用 **PDF.js**（wiki 原文），而维护者给一批渲染缺陷同时打了 `pdf.js issue` + `external issue` 两个标签（含义：「PDF.js 本身的问题」+「不是本扩展的问题」）：
  - [#4616](https://github.com/James-Yu/LaTeX-Workshop/issues/4616) 部分字体字符渲染错误（2025-06-20）
  - [#2640](https://github.com/James-Yu/LaTeX-Workshop/issues/2640) 暗色模式下 PDF 书签白字白底不可读（2021-04-27）
  - [#2624](https://github.com/James-Yu/LaTeX-Workshop/issues/2624) 侧栏大纲过暗（2021-04-18）
  - [#2518](https://github.com/James-Yu/LaTeX-Workshop/issues/2518) Visio 生成的 PDF 图像渲染错位（2021-02-08）
  - [#2004](https://github.com/James-Yu/LaTeX-Workshop/issues/2004) 滚动后 ctrl+滚轮缩放有 1–1.5 秒延迟（2020-03-25）
  - [#1758](https://github.com/James-Yu/LaTeX-Workshop/issues/1758) TikZ 透明/混合渲染错误（2019-10-21）
  - [#1577](https://github.com/James-Yu/LaTeX-Workshop/issues/1577) PDF 图片显示为空白（2019-08-04）
- [#2017「PDF preview always jump to the top after typesetting as opposed to returning to the previous scroll position」](https://github.com/James-Yu/LaTeX-Workshop/issues/2017)（2020-04-01，4 条评论）——刷新后跳回顶部。
- **架构性证据**：贡献者 [#3337](https://github.com/James-Yu/LaTeX-Workshop/issues/3337)「Feature Request: Consider using the customEditor api」原文：「I already implemented this in **PR #3069, which was promptly rejected and locked**. For the last half year I have been using **a custom fork** of this repo, just because **viewing pdf files works so much smoother when using the customEditor api properly**.」→ 改善 PDF 预览架构的提案被**明确拒绝并锁定**。
- 外部查看器路线被官方 wiki 标注「not officially supported」；HN 用户 `siproprio`：「**The only thing I miss about the vscode setup is support for a decent PDF viewer (not sumatra)**, and that magnifier loupe thing that other editors have.」来源：[HN 32606875](https://news.ycombinator.com/item?id=32606875)。
- 博主 danmackinlay：「Compared to a special-purpose editor (e.g. TeXShop) **the preview and build process is inferior but adequate**」；「**we are somewhat hamstrung because everything is constrained to a single window by VS Code**」；「**External PDF viewers are not officially supported** but more-or-less work for me… although definitely jankier」。来源：[danmackinlay.name](https://danmackinlay.name/notebook/vs_code_for_latex.html)。

**【推断】** PDF 预览在 VS Code 里必然是 webview/tab，无法获得原生窗口的滚动/缩放/多窗口语义；一旦底层用 pdf.js，渲染正确性就受上游牵制。这是**平台级短板**。

---

### 4️⃣ 多文件 / 子文件项目支持差

**【事实】**
- 官方 wiki 用 5 步启发式猜根文件（magic comment → self check → 扫描工作区 `.tex` → subfiles → `.fls`），并明确：
  - 「**we cannot compute file inclusions if user defined macros are used to include files**」
  - 「**If no root file is found, most of the features in LaTeX Workshop will not work.**」
  - 「**you have to open the directory (or one of its antecedents) containing the whole LaTeX project.**」
  - `auxDir`：「for compilation to succeed, it is sometimes required to reproduce in `auxdir` the directory hierarchy… **compilation may fail if LaTeX-Workshop cannot properly detect files inclusion**」
  - subfiles：需在根目录放 `.latexmkrc` 并写 `$do_cd = 1;`，因为 `-cd` 会破坏 `makeindex`（引 [#1895](https://github.com/James-Yu/LaTeX-Workshop/issues/1895) 与 [#1932](https://github.com/James-Yu/LaTeX-Workshop/issues/1932) 的讨论结论）。
  - 来源：[wiki/Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile)
- [#1272「Compiling projects using subfiles package」](https://github.com/James-Yu/LaTeX-Workshop/issues/1272)（2019-04-03，4 条评论）：打开子文件保存时**只编译子文件，不编译 magic comment 指定的主文件**。
- [#3467](https://github.com/James-Yu/LaTeX-Workshop/issues/3467)（2022-09-10）要求「Make the working directory from which build tools are invoked configurable」：LW 会把 cwd 切到 `.tex` 所在目录，导致用项目根相对路径的 `\includegraphics` 失效。该 issue 被维护者打上 **`come back later (hopefully)`**（描述：「The issue is hard to resolve for now. Hopefully the dev would come back later.」）并 **`state_reason: not_planned`** 关闭。后由 `latex-workshop.latex.build.fromFolder` 部分缓解。

**【推断】** 「根文件」是一个**启发式猜出来的全局变量**，而不是用户声明的项目属性；多文件语义（相对路径基准、cwd、auxDir 层级）散落在多个 setting 里靠约定拼合。这是缺少「项目」概念的直接后果（见 §5）。

---

### 5️⃣ 中文 / CJK 支持需手工配置，且存在真实缺陷

**【事实】**
- SE [692519](https://tex.stackexchange.com/questions/692519/unable-to-use-package-ctex-using-latexworkshop-in-visual-studio-code-on-my-mac)：用户写中文，明确「**pdflatex doesn't work for me. I have to use xelatex with the ctex package**」，报 `CTeX fontset 'mac' is unavailable`，并称「**I need to do something about changing the json file to use xelatex… which to be honest is a bit confusing to me**」。
- SE [564758](https://tex.stackexchange.com/questions/564758/how-to-use-visual-studio-code-latex-workshop-with-xelatex)（33,998 views）标题即「How to use VS Code LaTeX Workshop **with xelatex**」。
- SE [553023](https://tex.stackexchange.com/questions/553023/undefined-citations-when-use-latex-workshop-in-vs-code-while-it-works-well-in-t)（39,902 views）标题即「**Undefined citations when use latex-workshop in VS-code, while it works well in texstudio**」，正文关键句：「**Surprisingly, once it has been compiled once in Texstudio, these citations also work properly in VSCode!**」
- [#1245「Support Chinese path...again?」](https://github.com/James-Yu/LaTeX-Workshop/issues/1245)（2019-03-23，4 条评论）：中文路径下 xelatex 报 `! Emergency stop.`，路径被显示为乱码 `Ґ¾¿ʺ`。**注意：这是回归（作者称此前可用），不是永久不支持**。
- [#4317](https://github.com/James-Yu/LaTeX-Workshop/issues/4317)（子调研经 GitHub 取得）：`ctexart` 文档「**Compile [x] but Error [0]**」——编译失败但 Problems 面板零信息。
- [PR #4433](https://github.com/James-Yu/LaTeX-Workshop/pull/4433)（已合并）：格式化含中文的 `.tex` 把 `不等于语言` 变成 `不等于���言`，根因是 stdout 分块先 `chunk.toString()` 再拼接，「**This error frequently occurs with Chinese characters as they are more than 8 bits each**」。
- [#4713](https://github.com/James-Yu/LaTeX-Workshop/issues/4713)（子调研经 GitHub 取得）：`ctexart` + xelatex「Still 0 error, but **RED 'x' symbol**」。
- 中文社区教程同样要求把默认 latexmk 改为 xelatex（如[知乎《使用VSCode编写LaTeX》](https://zhuanlan.zhihu.com/p/38178015)）。

**【推断】** 对中文用户，VS Code + LW 不是开箱即用：默认走 pdflatex 路线，中文必须自行改 recipe 换 xelatex；且存在编码截断、错误不上报等真实缺陷。**这是 Latteset（中文友好）最直接的差异化空间。**

---

### 6️⃣ 编译慢 / 卡死 / 无限循环

**【事实】**
- [#1933「bad building performance compared to latexmk from command line」](https://github.com/James-Yu/LaTeX-Workshop/issues/1933)（2020-02-05，20 条评论）：同一份 `thesis.tex`，扩展内 recipe **20 秒**，终端直接跑 latexmk **5 秒**；「During the entire build process (invoked through the extension) **the editor feels very sluggish**」。该 issue 被打了 `need more info` + `external issue` + `can't reproduce` 三个标签。
- SE [695655](https://tex.stackexchange.com/questions/695655/compile-extremely-slow-on-vscode)：100 页文档平时 14–15 秒，出问题 **>2 分钟**（日志 `pdflatex 64.00s + 62.36s`，`Processing time = 127.48`），卡在 `bk10.clo`。⚠️ **根因在 TeX Live/latexmk 层，不应归因 VS Code**（用户自述唯一解法是重装 TeX Live）。
- [#4201](https://github.com/James-Yu/LaTeX-Workshop/issues/4201)（子调研经 GitHub 取得）：写论文时「compiles fine for about an hour then the extension will either **crash or become stuck in an infinite compilation loop**. Rebooting VScode is the only valid fix」。
- [#3980](https://github.com/James-Yu/LaTeX-Workshop/issues/3980)（子调研经 GitHub 取得）：44 页文档「Select environment」**约 8 秒**。
- [#3889](https://github.com/James-Yu/LaTeX-Workshop/issues/3889)（子调研经 GitHub 取得）：<70KB 文件方程预览 **>2 秒**。
- [#4590](https://github.com/James-Yu/LaTeX-Workshop/issues/4590)（子调研经 GitHub 取得）：错误日志「can sum up ~100k in a few seconds」。
- Reddit [1h5ifcx](https://www.reddit.com/r/LaTeX/comments/1h5ifcx/build_time_very_slow/)：2 页文件等 2 分钟（对照：正常大文档 2–5 秒）。

**【推断】** 慢有两类：① 用户/环境侧（TeX Live、包、字体）；② 扩展侧（大项目解析、日志风暴、intellisense 全量刷新）。二者在用户感知上无法区分，导致「VS Code 编译慢」成为笼统口碑。**报告时应避免把 ① 算作 VS Code 的锅。**

---

### 7️⃣ 缺少「项目」概念与跨文件操作

**【事实】**
- 官方 wiki：「If no root file is found, **most of the features in LaTeX Workshop will not work**」（[wiki/Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile)）——项目边界靠猜。
- Reddit 用户 `bill_klondike`：「**The only thing VS Code lacks is the ability to change labels across a project**, which TS [TeXstudio] handles well」；`yzqx` 仍用 TeXstudio 做「outline manipulation, **label manipulation** and table generation」。来源：[r/LaTeX 15sup7o](https://www.reddit.com/r/LaTeX/comments/15sup7o/texstudio_vs_latex_workshop_on_vscode/)。
- HN `diarrhea`：「The latter falls apart for what I use it for often (*.cls files etc.)」。来源：[HN 27029377](https://news.ycombinator.com/item?id=27029377)。

**【推断】** VS Code 的 workspace 是「文件夹」，不是 LaTeX「项目」：没有项目级引擎/输出目录/根文件/编码的声明式模型，也没有跨文件的 LaTeX 语义重构（改 label、改 citation key、重排结构）。这是**模型层面的缺失**。

---

### 8️⃣ 报错与日志定位差（错误不进 Problems 面板）

**【事实】**
- [#4317](https://github.com/James-Yu/LaTeX-Workshop/issues/4317)（子调研经 GitHub 取得）：「Compile [x] but Error [0]」——`ctexart` 文档编译失败但 Problems 面板没有任何信息。
- 官方 wiki 承认必须加 `-file-line-error` 才有定位，并提示 `--max-print-line=10000`；另注明 draft 模式下问题不显示（引 [#2893](https://github.com/James-Yu/LaTeX-Workshop/issues/2893) 的评论）。来源：[wiki/Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile)。
- 从 Overleaf 迁到本地 VS Code 的用户：「**In VSCode, if the compiler encounters an error, it just stops and you have to go on a hunt looking for your mistakes**」；Overleaf 会继续编译文档其余部分。来源：[r/LaTeX 1h0koc6](https://www.reddit.com/r/LaTeX/comments/1h0koc6/latex_workshop_vs_code_compilation/)。
- danmackinlay：「it was confusing when things went wrong (**Where WAS that syntax error?**)」。来源：[danmackinlay.name](https://danmackinlay.name/notebook/vs_code_for_latex.html)。

**【推断】** 报错体验依赖「log 解析器 + 用户 recipe 的 flag 组合」，而不是编译器原生结构化输出；这是 LaTeX 工具链本身的历史包袱被扩展继承下来。

---

### 9️⃣ Windows 路径 / 跨平台差异

**【事实】**
- 官方 wiki 专门为 Windows 提供 `_W32` 版占位符（`%DOC_W32%`、`%DIR_W32%`、`%OUTDIR_W32%`…），并注明「some Windows commands only work with the `\` path separator」。
- 官方 wiki：「Notice that **on Windows you might have to use `"Path"` instead of `"PATH"`** to override the PATH environment variable.」
- 官方 wiki：WSL 下若调用 Windows 侧 TeX，必须在 `command` 里加 `.exe`，否则「those executables will not respond to the commands」。
- [#1245](https://github.com/James-Yu/LaTeX-Workshop/issues/1245)：中文路径导致 xelatex `! Emergency stop.`。
- [#4976](https://github.com/James-Yu/LaTeX-Workshop/issues/4976)（2026-09）：Windows 11 + MiKTeX，magic options 被整体加引号 → `Latexmk: Bad options specified`；日志里还能看到 LW 主动改写 Windows 控制台代码页（`Initial Win CP for (console input, console output, system): (CP850, CP65001, CP1252)` → 「I changed them all to CP1252」）。
- 来源：[wiki/Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile)、上述 issue。

**【推断】** Windows 上的路径分隔符、控制台代码页、PATH 变量名大小写、WSL 互操作，都是「通用编辑器 shell 出去调用外部工具」时暴露的跨平台坑。对 Windows 首发的中文编辑器，这是必须系统解决的一类问题。

---

### 🔟 内存 / 资源占用（证据最弱，需谨慎）

**【事实】**
- Reddit [eqgxl4](https://www.reddit.com/r/LaTeX/comments/eqgxl4/my_short_and_terrible_experience_with_vs_code/)（score 14 / 26 comments 快照）：原帖「the minute I open up vs code, my computer slows down significantly… **Granted, I've got 4 gigs and a hdd**」；评论 `i_lack_discipline`：「VSCode is based on Electron… **it still is quite resource hungry for a text editor**」；`ruilvo`：「**VSCode can be a ram hogger**」。
- [#2736「Multiple extension instances taking up huge memory」](https://github.com/James-Yu/LaTeX-Workshop/issues/2736)（2021-06-03，1 条评论）：用户观察到多个 **java 进程**残留。**该 issue 被维护者打上 `external issue`（「Issue with something else than LaTeX-Wokshop」）**；日志显示这些 java 进程来自 `ltex-ls`（LTeX 语言服务器），**不是 LaTeX Workshop 自身**。

**【推断】** 「LaTeX Workshop 内存占用高」这一说法**证据不足**：现有证据指向 VS Code/Electron 本体，以及第三方扩展（LTeX）。**未能找到任何「LW 占用 XXX MB」的实测数字**——如实报告为「未找到」。

---

## 3. 「多年 open、作者无解」的架构性问题

### 3.1 先回答问题的前提

如前所述，**当前 open issues = 0**，且新 issue 在数小时内被关闭（#4976、#4977 均 2026-09-07 提、2026-09-08 关）。因此：

> **不存在「至今仍 open 多年」的 issue 可供引用。** 任何声称「某 issue 至今 open 多年」的说法，都与本次 API 实测不符。本节改用**维护者自己标注为「难以解决 / 非本扩展问题 / 待讨论」**的 issue，以及**官方 wiki 的明文免责声明**来识别架构性问题。

### 3.2 维护者自建标签所指向的架构性问题

维护者给 issue 打了几个语义明确的标签（[标签列表 API](https://api.github.com/repos/James-Yu/LaTeX-Workshop/labels)）：

| 标签 | 官方描述 | 含义 |
|---|---|---|
| `come back later (hopefully)` | 「The issue is hard to resolve for now. Hopefully the dev would come back later.」 | **维护者承认「暂时无解」** |
| `discussion needed & close for now` | 「Discuss before coming up with an implementation」 | 架构上要重做，先关掉 |
| `external issue` | 「Issue with something else than LaTeX-Wokshop」 | 不是本扩展能修的 |
| `pdf.js issue` | 「Issue related to PDF.js itself」 | 卡在 PDF.js 上游 |
| `vscode issue` | 「Issue related to VSCode itself」 | 卡在 VS Code 平台 |

**`come back later (hopefully)` 全仓库仅 2 个 issue：**

1. [#3467](https://github.com/James-Yu/LaTeX-Workshop/issues/3467)「Make the working directory from which build tools are invoked configurable」
   - 创建 2022-09-10，关闭 2022-09-11，`state_reason: not_planned`，1 个 👍，6 条评论。
   - 正文核心：LW 编译前把 cwd 切到 `.tex` 所在目录，导致项目根相对路径失效；用户希望有 `latex-workshop.latex.workingDir`。
   - **架构含义**：「编译工作目录」这个语义在 LW 里是隐式约定而非可声明属性。

2. [#3758](https://github.com/James-Yu/LaTeX-Workshop/issues/3758)「PDF.js reference preview」
   - 创建 2023-03-07，关闭 2023-03-08，1 条评论。
   - 正文核心：想让 PDF 内 `\ref` 链接能悬停预览而不跳走；作者指出多年前就有人在 [mozilla/pdf.js#5835](https://github.com/mozilla/pdf.js/issues/5835) 提过，只有 hack 方案。
   - **架构含义**：PDF 交互能力受 PDF.js 上游限制。

**`discussion needed & close for now` 的代表：**

3. [#3337](https://github.com/James-Yu/LaTeX-Workshop/issues/3337)「Feature Request: Consider using the customEditor api」
   - 创建 2022-06-18，当日关闭，11 条评论，作者是 CONTRIBUTOR。
   - 正文核心（原文）：「I already implemented this in **PR #3069, which was promptly rejected and locked**. For the last half year I have been using **a custom fork** of this repo, just because **viewing pdf files works so much smoother when using the customEditor api properly**.」
   - **架构含义**：**改善 PDF 预览架构的提案被明确拒绝并锁定**——这是「PDF 预览是二等公民」最硬的证据。

### 3.3 被归因到上游 / 平台的架构性问题（`external issue` + `pdf.js issue`）

| Issue | 标题 | 状态 |
|---|---|---|
| [#1758](https://github.com/James-Yu/LaTeX-Workshop/issues/1758) | TikZ 透明/混合渲染错误 | 关闭，`pdf.js issue` |
| [#1577](https://github.com/James-Yu/LaTeX-Workshop/issues/1577) | PDF 图片显示为空白 | 关闭，`pdf.js issue` |
| [#2004](https://github.com/James-Yu/LaTeX-Workshop/issues/2004) | 滚动后缩放延迟 1–1.5s | 关闭，`pdf.js issue` |
| [#2640](https://github.com/James-Yu/LaTeX-Workshop/issues/2640) | 暗色模式 PDF 书签不可读 | 关闭，`pdf.js issue` |
| [#2624](https://github.com/James-Yu/LaTeX-Workshop/issues/2624) | 侧栏大纲过暗 | 关闭，`pdf.js issue` |
| [#2518](https://github.com/James-Yu/LaTeX-Workshop/issues/2518) | Visio PDF 图像错位 | 关闭，`pdf.js issue` |
| [#4616](https://github.com/James-Yu/LaTeX-Workshop/issues/4616) | 部分字体字符渲染错误 | 关闭，`pdf.js issue` |
| [#2736](https://github.com/James-Yu/LaTeX-Workshop/issues/2736) | 多实例内存占用（实为 LTeX） | 关闭，`external issue` |
| [#1933](https://github.com/James-Yu/LaTeX-Workshop/issues/1933) | 编译比终端 latexmk 慢 4 倍 | 关闭，`external issue` + `can't reproduce` |

### 3.4 官方 wiki 自己写明的结构性限制（最可靠的「作者无解」证据）

以下均为**官方文档原文承认**，比 issue 状态更能说明「架构性」：

1. 「If no root file is found, **most of the features in LaTeX Workshop will not work**.」——项目边界靠猜。
2. 「**we cannot compute file inclusions if user defined macros are used to include files**.」——依赖分析是正则静态解析，不是真解析。
3. 「**External PDF viewers are not officially supported**」，且 SyncTeX **「As of mid-2025, this function is not working」**。
4. `view.pdf.viewer: "external"` = **Experimental**。
5. 「repeated viewer refreshes and/or **loss of PDF scrolling position**… potentially causing **file corruption issues**」。
6. subfiles 路径语义需用户手写 `.latexmkrc` 的 `$do_cd = 1;`（源自 #1895/#1932 的取舍）。
7. 「**compilation may fail if LaTeX-Workshop cannot properly detect files inclusion**」（auxDir 章节）。

---

## 4. 为什么从 TeXstudio / Overleaf 迁到 VS Code？迁移后遇到什么新问题？

### 4.1 TeXstudio → VS Code 的动机（有来源）

| 动机 | 证据 | 来源 |
|---|---|---|
| 一个编辑器统一所有工作 | 「all my note taking and programming can be done in one place」 | [r/LaTeX 15sup7o](https://www.reddit.com/r/LaTeX/comments/15sup7o/texstudio_vs_latex_workshop_on_vscode/) |
| 扩展生态 / 多光标 / 列选择 | 「the lack of extensions that VSCode has… column selection, multiple cursors」 | 同上 |
| 灵活性 + Git 连通性 | 「makes up for it IMO in flexibility and connectivity」 | 同上 |
| 与写代码的工作流复用 | 「writing code and the poster together」 | 同上 |
| 设置调好后 95% 用 VS Code | 「95% VS Code / 5% Texstudio once I got the settings right」 | 同上（jwi08w1） |
| 减少 CPU 密集程序 | 「The fewer CPU intensive programs for my workflow, the better.」 | [HN 25299134](https://news.ycombinator.com/item?id=25299134) |
| 摆脱「小众编辑器」 | 「any weird, tin-pot specialist editor maintained by someone」 | [danmackinlay.name](https://danmackinlay.name/notebook/vs_code_for_latex.html) |

**迁移后遇到的新问题（有来源）：**
- 配置负担：「configuring them _is_ 'fighting the IDE' to me」（GreatLich，[r/LaTeX hpjisyh](https://www.reddit.com/r/LaTeX/comments/rkvq56/which_latex_ide_do_you_prefer/hpjisyh/)）。
- 跨文件 label 操作缺失：「The only thing VS Code lacks is the ability to change labels across a project, which TS handles well」（bill_klondike，[15sup7o](https://www.reddit.com/r/LaTeX/comments/15sup7o/texstudio_vs_latex_workshop_on_vscode/)）。
- 引用失效（而 TeXstudio 正常）：「**once it has been compiled once in Texstudio, these citations also work properly in VSCode!**」（[SE 553023](https://tex.stackexchange.com/questions/553023/undefined-citations-when-use-latex-workshop-in-vs-code-while-it-works-well-in-t)）。
- 键位冲突：「an **aggressively intrusive set of keybindings that clash with so many other things**」（danmackinlay）。
- **有人迁回去**：GreatLich（7 分）「**I've got both VScode and Texstudio, but I keep coming back to Texstudio.**」[r/LaTeX nip46a/gz37tj3](https://www.reddit.com/r/LaTeX/comments/nip46a/what_latex_editor_would_you_suggest/gz37tj3/)；Reddit eqgxl4「**Guess I'm going back to Texmaker.**」；HN ykonstant「**I will always recommend TeXStudio over everything else.**」[HN 44354226](https://news.ycombinator.com/item?id=44354226)。

### 4.2 Overleaf → 本地 VS Code 的动机与新问题

**动机（有来源）**：离线/隐私/安全（[r/LaTeX 1ifumm8](https://www.reddit.com/r/LaTeX/comments/1ifumm8/switching_from_overleaf_to_vscode/)「for greater security」）；免费层无 git 集成/云盘同步/编译时长限制、需联网（[mark-wang.com](https://mark-wang.com/blog/2022/latex/)）。

**新问题（有来源）**：
- 「**one headache after another**」「Apart from the **heavy installation of Texlive**, I keep encountering errors after errors… in Overleaf… compiles the rest of the document easily. But in VS Code… I need to debug at like 3 4 places.」来源：[r/LaTeX 1h0koc6](https://www.reddit.com/r/LaTeX/comments/1h0koc6/latex_workshop_vs_code_compilation/)。
- 参考文献失效：「**it is impossible to have the pdf with a correct bibliography. Citations appear in key form and not to the required APA standards.**」来源：[r/LaTeX 1ifumm8](https://www.reddit.com/r/LaTeX/comments/1ifumm8/switching_from_overleaf_to_vscode/)。
- 环境成本：「**the last command downloads over 7 GB**」「**The installation step took about two hours for me**」。来源：[mark-wang.com](https://mark-wang.com/blog/2022/latex/)。
- **有人迁回/留在 Overleaf**：HN `lhoff`「**Overleaf does everything what i need and it works so frictionless that I haven't installed latex locally for years now.**」[HN 32604862](https://news.ycombinator.com/item?id=32604862)；HN `feet` 从 vscodium+LW 改为自建 Overleaf 实例，因为「**collaboration is a ton easier**」[HN 32605723](https://news.ycombinator.com/item?id=32605723)。

---

## 5. VS Code 方案的结构性短板

> 以下每条都附最有力出处，并标注类型。

| # | 结构性短板 | 最有力证据 | 类型 |
|---|---|---|---|
| 1 | **非 LaTeX 原生**，靠扩展拼装 | 官方 wiki 要求手写 recipes/tools JSON；`GreatLich`「configuring them _is_ 'fighting the IDE'」；`paulwintz`「This extension is not designed for LaTeX in particular, so you need to do some specific configuration」（[paulwintz.com](https://paulwintz.com/latex-in-vscode/)） | 维护者声明 + 用户报告 + 作者观点 |
| 2 | **配置门槛高**（recipe/tool/占位符/魔法注释） | [wiki/Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile)；SE 66,470 / 88,055 views；[#4976](https://github.com/James-Yu/LaTeX-Workshop/issues/4976)（2026-09 仍在踩） | 维护者声明 + 用户报告 |
| 3 | **PDF 预览是二等公民** | wiki 称外部 viewer「not officially supported」、external = Experimental、滚动位置会丢、可能损坏文件；贡献者 [PR #3069](https://github.com/James-Yu/LaTeX-Workshop/pull/3069) 被拒（[#3337](https://github.com/James-Yu/LaTeX-Workshop/issues/3337)）；7+ 个 `pdf.js issue` | 维护者声明 + 贡献者报告 |
| 4 | **没有「项目」概念** | wiki「If no root file is found, most of the features… will not work」；`bill_klondike`「change labels across a project」缺失 | 维护者声明 + 用户报告 |
| 5 | **依赖分析是正则静态解析** | wiki「we cannot compute file inclusions if user defined macros are used」 | 维护者声明 |
| 6 | **跨文件 LaTeX 语义操作缺失** | `yzqx` 仍用 TeXstudio 做 outline/label/table；`bill_klondike` 同上 | 用户报告 |
| 7 | **SyncTeX 三处耦合易断** | 默认 recipe 不带 `-synctex=1`；删 `.synctex.gz` 即失效；外部 viewer SyncTeX 2025 年中起 not working | 用户报告 + 维护者声明 |
| 8 | **单窗口约束** | danmackinlay「everything is constrained to a single window by VS Code」；HN `siproprio`「miss… a decent PDF viewer」 | 作者观点 + 用户报告 |
| 9 | **平台/上游依赖风险** | `pdf.js issue`、`external issue`、`vscode issue` 三类标签；升级 VS Code 后预览渲染变化（[#4616](https://github.com/James-Yu/LaTeX-Workshop/issues/4616) 与 VS Code 1.101.0 更新同期） | 维护者标签 + issue 正文 |

---

## 6. 事实 vs [推断] 一览

### 6.1 有来源支撑的事实
- Marketplace 安装量 ~560 万、评分 4.8/5（300 条）、v10.18.0；GitHub 12,308 stars / 587 forks；**当前 0 open issues**，且新 issue 数小时内被关闭。
- TeXstudio GitHub 3,619 stars / 431 open issues；Overleaf 官方称 1,000 万用户（2022）。
- 所有引用的 issue 编号、标题、创建/关闭日期、标签、正文摘述，均来自 `api.github.com` 实际返回。
- 官方 wiki 的所有「not officially supported / not working / Experimental / most features will not work / cannot compute file inclusions」表述，均为原文。
- 所有 Stack Exchange 浏览量/票数、HN 评论原文、Reddit 引语（PullPush 快照），均来自实际抓取。

### 6.2 标记为 [推断] 的结论
- 维护者采用「即提即关」分诊策略，因此不能用 open issue 数推断问题多少。
- 「PDF 预览 / SyncTeX 失效」是架构耦合问题，而非单点 bug。
- 「LaTeX Workshop 内存占用高」证据不足；现有证据指向 Electron 本体与 LTeX 扩展。
- SE 695655 的极端慢编译根因在 TeX Live/latexmk，不应归因 VS Code。
- 「中文支持」是 VS Code + LW 的明确薄弱环节（多源汇聚，无单一权威来源直下结论）。
- 「开发者不打算支持链式编译」的说法（Reddit 一条 score −9、被折叠的评论）**已过时**：现版本有完整的 `latex-workshop.latex.recipes` 链式 recipe，不应采信。

### 6.3 明确未找到 / 无法证实
1. **任何「至今仍 open 多年」的 issue**——因为仓库当前 0 open issues（三处 API 交叉确认）。
2. LaTeX Workshop 的**具体内存 MB 实测数值**。
3. TeXstudio 的**官方下载总量 / 活跃用户数**。
4. **Overleaf → 本地 VS Code 的回退率统计**。
5. **同一文档、同一机器上「VS Code vs 专用 IDE」的端到端编译耗时对照实验**（现有数据均为用户自述，环境不可控）。
6. HN 上**专门**讨论「VS Code for LaTeX 抱怨」的独立大帖（相关内容散落在 [Ask HN: What LaTeX editor do you use?](https://news.ycombinator.com/item?id=32604306) 等评论区）。
7. r/LaTeX 帖 `1pxyjts` 正文（PullPush 返回空），未采信。

---

## 7. VS Code 方案满足不了、必须专门的 LaTeX IDE 才能解决的三个问题

> 判据：该问题**不是「扩展没做好」，而是「在通用编辑器 + 插件模型下无法真正做好」**。

### ① 一等公民的「项目」模型

**问题**：VS Code 的 workspace 是通用文件夹，LaTeX 项目所需的核心语义——根文件、编译工作目录、输出/辅助目录层级、子文件路径基准——在 LW 里全靠**启发式猜测 + 魔法注释 + 多个 setting 拼合**。官方 wiki 明说找不到根文件就「most of the features… will not work」，且无法解析自定义宏引入的文件；工作目录不可声明（[#3467](https://github.com/James-Yu/LaTeX-Workshop/issues/3467) 被标 `come back later (hopefully)` + `not_planned`）；子文件路径语义要靠用户手写 `.latexmkrc` 的 `$do_cd = 1;` 兜底（源自 [#1895](https://github.com/James-Yu/LaTeX-Workshop/issues/1895)/[#1932](https://github.com/James-Yu/LaTeX-Workshop/issues/1932)）。

**为什么必须专门 IDE**：这需要把「项目」提升为编辑器的**第一类对象**（显式声明根文件/引擎/工作目录/输出目录/编码，并让编译、预览、SyncTeX、智能提示全部以同一项目模型为唯一真相源）。在 VS Code 里，这只能是另一个扩展再叠一层配置，无法改变 workspace 本身不是 LaTeX 项目的事实。

### ② PDF 预览 + SyncTeX + 编译的一体化实时状态管理

**问题**：内置预览是 pdf.js（渲染缺陷被维护者归因上游，7+ 个 `pdf.js issue`）；刷新会丢滚动位置甚至可能损坏文件；外部 viewer 路线官方标注「not officially supported」，其 SyncTeX「As of mid-2025… not working」；改善架构的 customEditor PR 被**明确拒绝并锁定**（[#3337](https://github.com/James-Yu/LaTeX-Workshop/issues/3337)）；SyncTeX 还要求用户自己在 recipe 里加 `-synctex=1` 并保住 `.synctex.gz`。

**为什么必须专门 IDE**：要做到「编译 → PDF 增量更新 → 保持滚动位置 → 双向 SyncTeX 精确跳转」四件事始终一致，需要**自己拥有 PDF 渲染层、滚动/缩放状态、SyncTeX 坐标映射与编译调度器**，并让它们共享同一份实时状态。VS Code 把 PDF 关在 webview/tab 里，扩展拿不到足够的窗口与渲染控制权——这不是努力问题，是权限与架构边界问题。

### ③ 跨文件 LaTeX 语义重构与 IDE 级 LaTeX 智能

**问题**：用户明确抱怨 VS Code「lacks the ability to change labels across a project」，至今仍要用 TeXstudio 做「outline manipulation, label manipulation and table generation」；LW 的依赖分析是正则静态解析，「cannot compute file inclusions if user defined macros are used」；报错经常不进 Problems 面板（`Compile [x] but Error [0]`）。

**为什么必须专门 IDE**：跨文件改 label/citation key、重排章节、表格式编辑、宏感知的语义分析，要求一个**理解 LaTeX 语义的 AST/索引层**，并把它作为编辑、重构、报错定位、补全的共同底座。目前 LW 的静态正则解析在宏面前就会失效，而 VS Code 的通用 LSP 框架也不提供 LaTeX 特有的项目级重构原语。

### 附：不算「结构性不可能」但现实未解决的差异化机会（[推断]）

- **中文/CJK 开箱即用**：默认走 pdflatex、需手改 JSON 切 xelatex，且存在编码截断（[PR #4433](https://github.com/James-Yu/LaTeX-Workshop/pull/4433)）、错误不上报（[#4317](https://github.com/James-Yu/LaTeX-Workshop/issues/4317)）等真实缺陷。这在技术上**可以被扩展解决**，只是现实中没解决——对 Windows 首发、中文友好的 Latteset 是直接卖点。
- **实时/连续编译**：LW 是 save/build 触发（`latex.autoBuild.run` = never/onSave/onFileChange），没有 Overleaf/Latteset 式连续排版。这同样属于「扩展可以做得更好」而非「VS Code 做不到」。

---

## 8. 调研局限（未取得 / 未核实，明确声明）

本次调研共执行 **35+ 次检索与抓取**（主调研 ~17 条 web_search + ~25 次 web_fetch；并行子调研另执行 20+ 次），覆盖 GitHub issue 区、官方 wiki、VS Code Marketplace、Reddit r/LaTeX、TeX Stack Exchange、Hacker News、对比文章。仍存在以下局限，**均按「未取得」如实声明，未做任何填充或推测性数字**：

1. **未取得：任何「至今仍 open 多年」的 issue。**
   原因：仓库当前 open issues = 0（三处 API 交叉确认），且新 issue 在数小时内被关闭（#4976、#4977 均为 2026-09-07 提、2026-09-08 关）。因此本报告改用维护者标签（`come back later (hopefully)` 等）与官方 wiki 免责声明识别架构性问题。**不存在可引用的「长期 open」编号。**

2. **未取得：LaTeX Workshop 的具体内存占用数值（MB）。**
   现有 Reddit 讨论只有定性描述，且指向 VS Code/Electron 本体；GitHub 内存类 issue（#2736）经核实实为第三方 LTeX 扩展的 java 进程，已被维护者标为 `external issue`。

3. **未取得：TeXstudio 的官方下载总量 / 活跃用户数。** 仅有 GitHub stars（3,619）可作对照。

4. **未取得：Overleaf → 本地 VS Code 的「回退率」统计数据。** 只有个案自述（Reddit / HN 上「I keep coming back to Texstudio」「I haven't installed latex locally for years」）。

5. **未取得：同一文档、同一机器上「VS Code + LW vs 专用 IDE」的端到端编译耗时对照实验。** 现有耗时数据（#1933 的 20s vs 5s、SE 695655 的 127.48s 等）均为用户自述，环境不可控，不能当作严谨基准。

6. **未取得：HN 上专门讨论「VS Code for LaTeX 抱怨」的独立大帖。** 相关观点散落在 [Ask HN: What LaTeX editor do you use?](https://news.ycombinator.com/item?id=32604306)（24 分）等评论区。

7. **未核实：issue #1749「Synctex fails to work」的正文细节。** 该编号仅来自 web_search 摘要，本次未逐条 fetch 核实，正文与结论未采信。

8. **未采信：Reddit r/LaTeX 帖 `1pxyjts` 正文**（PullPush 返回空）、以及一条 score −9 被折叠的评论所称「开发者不打算支持链式编译」（已被现版本 `latex-workshop.latex.recipes` 证伪）。

9. **方法性局限**：Reddit 的 `score` / `num_comments` 来自 PullPush **归档快照**，不等于实时票数（部分 2024 年底帖子快照显示 `score: 1, num_comments: 0`，明显为早期快照）；`tex.stackexchange.com` 与 `reddit.com` 直连被拦截，经 API/归档绕行，可能与实时页面存在细微差异。

10. **未取得的对照**：VS Code 官方未公布扩展的「活跃用户数」，Marketplace 只给 unique installs（~560 万），不能等同于活跃用户或 LaTeX 用户总量。

---

## 附：本报告引用的核心来源清单

**官方**
- [LaTeX Workshop Marketplace 页](https://marketplace.visualstudio.com/items?itemName=James-Yu.latex-workshop)
- [仓库 API](https://api.github.com/repos/James-Yu/LaTeX-Workshop) / [标签列表](https://api.github.com/repos/James-Yu/LaTeX-Workshop/labels)
- [wiki/Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile) · [wiki/View](https://github.com/James-Yu/LaTeX-Workshop/wiki/View)
- [TeXstudio 仓库 API](https://api.github.com/repos/texstudio-org/texstudio) · [Overleaf 十百万用户公告](https://www.overleaf.com/blog/wow-ten-million-users)

**GitHub issues（均经 API 核实）**
[#1933](https://github.com/James-Yu/LaTeX-Workshop/issues/1933) · [#2017](https://github.com/James-Yu/LaTeX-Workshop/issues/2017) · [#2736](https://github.com/James-Yu/LaTeX-Workshop/issues/2736) · [#3337](https://github.com/James-Yu/LaTeX-Workshop/issues/3337) · [#3467](https://github.com/James-Yu/LaTeX-Workshop/issues/3467) · [#3758](https://github.com/James-Yu/LaTeX-Workshop/issues/3758) · [#4584](https://github.com/James-Yu/LaTeX-Workshop/issues/4584) · [#1272](https://github.com/James-Yu/LaTeX-Workshop/issues/1272) · [#1245](https://github.com/James-Yu/LaTeX-Workshop/issues/1245) · [#4976](https://github.com/James-Yu/LaTeX-Workshop/issues/4976) · [#4977](https://github.com/James-Yu/LaTeX-Workshop/issues/4977) · [#1758](https://github.com/James-Yu/LaTeX-Workshop/issues/1758) · [#1577](https://github.com/James-Yu/LaTeX-Workshop/issues/1577) · [#2004](https://github.com/James-Yu/LaTeX-Workshop/issues/2004) · [#2640](https://github.com/James-Yu/LaTeX-Workshop/issues/2640) · [#2624](https://github.com/James-Yu/LaTeX-Workshop/issues/2624) · [#2518](https://github.com/James-Yu/LaTeX-Workshop/issues/2518) · [#4616](https://github.com/James-Yu/LaTeX-Workshop/issues/4616) · [#4934](https://github.com/James-Yu/LaTeX-Workshop/issues/4934)

**Stack Exchange**
[538797](https://tex.stackexchange.com/questions/538797/go-to-source-for-latex-on-vs-code-does-not-seem-to-work) · [527463](https://tex.stackexchange.com/questions/527463/pdf-preview-in-visual-studio-code) · [462365](https://tex.stackexchange.com/questions/462365) · [461954](https://tex.stackexchange.com/questions/461954) · [478865](https://tex.stackexchange.com/questions/478865) · [553023](https://tex.stackexchange.com/questions/553023/undefined-citations-when-use-latex-workshop-in-vs-code-while-it-works-well-in-t) · [564758](https://tex.stackexchange.com/questions/564758/how-to-use-visual-studio-code-latex-workshop-with-xelatex) · [692519](https://tex.stackexchange.com/questions/692519/unable-to-use-package-ctex-using-latexworkshop-in-visual-studio-code-on-my-mac) · [695655](https://tex.stackexchange.com/questions/695655/compile-extremely-slow-on-vscode)

**社区**
[r/LaTeX 15sup7o](https://www.reddit.com/r/LaTeX/comments/15sup7o/texstudio_vs_latex_workshop_on_vscode/) · [r/LaTeX 1h0koc6](https://www.reddit.com/r/LaTeX/comments/1h0koc6/latex_workshop_vs_code_compilation/) · [r/LaTeX 1ifumm8](https://www.reddit.com/r/LaTeX/comments/1ifumm8/switching_from_overleaf_to_vscode/) · [r/LaTeX eqgxl4](https://www.reddit.com/r/LaTeX/comments/eqgxl4/my_short_and_terrible_experience_with_vs_code/) · [r/LaTeX rkvq56](https://www.reddit.com/r/LaTeX/comments/rkvq56/which_latex_ide_do_you_prefer/) · [HN 32604306](https://news.ycombinator.com/item?id=32604306) · [danmackinlay](https://danmackinlay.name/notebook/vs_code_for_latex.html) · [paulwintz](https://paulwintz.com/latex-in-vscode/) · [mark-wang](https://mark-wang.com/blog/2022/latex/) · [知乎](https://zhuanlan.zhihu.com/p/38178015)
