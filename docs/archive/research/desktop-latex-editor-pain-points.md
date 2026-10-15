# 桌面 LaTeX 编辑器核心痛点调研

> 调研对象：TeXstudio、TeXmaker、TeXworks、WinEdt、TeXShop、Texifier（原 Texpad）、Kile
> 数据采集时间：2026-09（数值以 GitHub / StackExchange API 返回时点为准）
> 调研方法：GitHub Search API（issue 计数 + 高互动 issue 原文）、StackExchange API（票数 / 浏览量 / 正文）、HN Algolia API、PullPush（Reddit 归档）、厂商官网、评测博客
> 证据标注：**事实** = 有 URL + 原文摘述；**[推断]** = 分析判断，无直接来源

---

## 0. 证据强度说明（先看这一段）

| 证据级别 | 含义 | 本次使用 |
|---|---|---|
| A 级 | 官方 API 返回的结构化数字 / issue 原文正文 | GitHub、StackExchange、HN、官网 |
| B 级 | 搜索引擎返回的页面摘要（未能抓取全文） | Reddit 帖子（`www.reddit.com` 在本环境被解析为内网 IP，无法直连；改用 PullPush 归档 API 取到部分评论正文） |
| C 级 | 厂商营销/推广博客 | 1 篇（dev.to 上的 TeX64 推广文），已明确标注 |

**未能取到的数据（诚实声明）**：
- KDE Bugzilla 的 Kile 精确 bug 总数：`bugs.kde.org` 的 REST 接口不返回总数，未取得。
- TeXmaker / TeXShop / WinEdt 没有公开的 issue 追踪库（TeXmaker 无 GitHub issue 区；TeXShop 用邮件列表；WinEdt 闭源），**只能**用 TeX Stack Exchange 标签数做代理指标。
- Reddit 原帖的票数/评论数：`www.reddit.com` 不可直连，PullPush 只返回了部分评论（含 `ups` 字段），**无法**给出完整帖子的投票数。
- Overleaf 的实时用户数只有 2021 年的官方博客数字，无更新数据。

---

## 1. 量化基线：各编辑器用户规模与问题密度

### 1.1 代码仓与问题库（事实，GitHub API，2026-09）

| 编辑器 | Stars | issue 总数 | open issue | 数据来源 |
|---|---|---|---|---|
| TeXstudio | 3,619 | **3,160** | 431 | [repo API](https://api.github.com/repos/texstudio-org/texstudio) · [issue 搜索](https://api.github.com/search/issues?q=repo%3Atexstudio-org%2Ftexstudio+is%3Aissue) |
| TeXworks | 778 | **1,012** | 227 | [repo API](https://api.github.com/repos/TeXworks/texworks) |

### 1.2 TeXstudio issue 按关键词分布（事实，GitHub Search API）

| 关键词 | issue 数 | 占比（/3,160） |
|---|---|---|
| `crash` | **293** | 9.3% |
| `completion`（自动补全/cwl） | **263** | 8.3% |
| `synctex` | **148** | 4.7% |
| `slow` | **76** | 2.4% |
| `freeze` | **31** | 1.0% |

> 说明：关键词命中不等于"该词即为痛点"，但 293 个 crash、263 个 completion、148 个 synctex 足以说明这三个方向的用户摩擦量级。

### 1.3 单版本下载量（事实，GitHub Releases API 的 `download_count`）

| 产物 | 下载量 |
|---|---|
| TeXstudio 4.9.7（最新稳定版）`win-qt6-signed.exe` | **61,807** |
| TeXstudio 4.9.7 `osx-m1.zip` | **64,281** |
| TeXstudio 4.9.7 `osx.zip` | 9,853 |
| TeXstudio 4.9.7 `x86_64.AppImage` | 1,569 |
| TeXworks 0.6.10 `win10-setup.exe` | **34,589** |
| TeXworks 0.6.11 `win10-setup.exe`（2026-02 发布，未满周期） | 13,954 |

来源：[TeXstudio 4.9.7 release](https://api.github.com/repos/texstudio-org/texstudio/releases/latest) · [TeXworks releases](https://api.github.com/repos/TeXworks/texworks/releases)

### 1.4 TeX Stack Exchange 标签题数（事实，StackExchange API）

| 标签 | 题目数 |
|---|---|
| `overleaf` | 2,348 |
| **`texstudio`** | **2,075** |
| **`texmaker`** | **1,558** |
| `texshop` | 727 |
| `texworks` | 516 |
| `winedt` | 395 |
| `kile` | 263 |
| `latex-workshop`（VS Code） | 87 |
| `forward-inverse-search` | 327 |

来源：[tags info API](https://api.stackexchange.com/2.3/tags/texstudio;texmaker;texworks;winedt;texshop;kile;overleaf;latex-workshop/info?site=tex.stackexchange)

### 1.5 竞品规模对照（用于判断迁移压力）

- **Overleaf：900 万用户，日活 40 万**（官方博客，2021-12）— [overleaf.com/blog/nine-years-nine-million-users](https://www.overleaf.com/blog/nine-years-nine-million-users)
- **VS Code LaTeX Workshop：5,596,003 次安装** — [Marketplace](https://marketplace.visualstudio.com/items?itemName=James-Yu.latex-workshop)
- WinEdt 定价：学生 **$50** / 教育 **$80** / 商业政府 **$150**，**"ALL SALES ARE FINAL"**（不可退款）— [winedt.com/registration.html](https://www.winedt.com/registration.html)
- Texifier 定价：单席 **$39.99** — [texifier.com/purchase](https://www.texifier.com/purchase)

**[推断]** LaTeX Workshop 的 559 万安装量与 TeXstudio 单版本约 7.8 万下载量（Win+macOS 合计）相差两个数量级。这不代表 TeXstudio 用户少（TeX Live 发行版内置、包管理器分发不计入 GitHub 下载），但说明"VS Code 插件 + 本地 TeX 发行版"已成为桌面写作的主流替代路径。

---

## 2. 前 10 类核心痛点（按出现频率 / 严重度排序）

### 痛点 1：编译错误只给"进程失败"或 .log 原文，定位不到出错行 —— 严重度最高

**具体表现**
- 报错面板只显示 `Process exited with error(s).`，无行号、无解释，log 面板甚至是空的。
- 用户的直观对比是 Overleaf 能指出行号并给出修复建议，而 TeXstudio 给不出。
- log 解析器把非标准格式的行误判为错误（`!` 开头但下一行不是 `l.NNN` 的情况）。

**证据（A 级）**
- TeX SE 提问《TeXStudio error messages just say "Process exited with error(s)" without any other information》，**2,325 次浏览**，原文：
  > "Whenever it detects an error, it simply says 'Process exited with error(s).' That's it--no explanation, no line number where the error occurred, nothing." … "the pictures above are to illustrate just how useless the TexStudio message is, while the Overleaf message is helpful is pointing out the error and makes a suggestion to correct it." … "Edit: The log tab is also blank :("
  — [tex.stackexchange.com/questions/574762](https://tex.stackexchange.com/questions/574762/texstudio-error-messages-just-say-process-exited-with-errors-without-any-oth)
- TeXstudio issue #50《Incorrect parsing of Latex error messages》：
  > "It may be helpful to have the log parser in Texstudio ignore lines that start with ! and are not followed directly by a line that starts with 'l...'"
  — [github.com/texstudio-org/texstudio/issues/50](https://github.com/texstudio-org/texstudio/issues/50)
- 评测文（C 级，推广性质，但对比陈述可作旁证）：
  > "**Smart error handling** — Overleaf at least shows you which line errored. Bare TeX errors are cryptic."
  — [dev.to/boson_jp/…](https://dev.to/boson_jp/i-finally-ditched-overleaf-for-a-local-latex-editor-heres-what-actually-works-2cgg)
- 相关规模：TSE `errors` 标签 **5,516 题** — [tags API](https://api.stackexchange.com/2.3/tags/forward-inverse-search;synctex;errors;compilation;pdftex/info?site=tex.stackexchange)

**性质：架构性。** 错误定位依赖正则解析 `.log`，而 TeX 引擎的错误格式本身就是多行、无结构、随宏包变化的。要真正解决必须改造"引擎 → 结构化诊断"链路，而不是修 log parser 的某个正则。

---

### 痛点 2：大型项目 / 长文档性能崩塌 —— 高频且致命，"无真正增量"

**具体表现**
- 打开多个大文件要等 5–10 分钟，甚至触发"possible endless loop"紧急暂停。
- 编辑中等规模文件就出现输入延迟、卡顿。
- `Ctrl+F` 首次搜索要等近 1 分钟。
- 启动就要 5–9% CPU 长时间占用。
- 150 页论文首次编译 **超过 10 分钟**。
- TeXworks 在 **308 KB / 4,300 行 / 56 页** 的文档上编辑就明显迟钝。

**证据（A 级）**
- TeXstudio issue #4410《problems loading many large files》（2026-04，13 条评论）：
  > "I have a very large project, which consists of about 20 files which altogether give almost 7000 pages in latex. … Now when I load it I get (after 5-10 minutes) a texstudio emergency 'texstudio has been paused due to a possible endless loop' … even when it works, texstudio takes a very long time to open many large files at once. It seems to me like it is trying to do too much processing."
  — [issues/4410](https://github.com/texstudio-org/texstudio/issues/4410)
- TeXstudio issue #3470《Texstudio extremely slow to start》：
  > "Texstudio is extremely slow to start on my laptop. it uses 5-9% of CPU not just showing 0 or a constant value."
  — [issues/3470](https://github.com/texstudio-org/texstudio/issues/3470)
- TeX SE《TeXstudio is exceptionally slow when using find》：
  > "TeXstudio is unresponsive on startup, when opening new files and when using the find function (ctrl+f). Sometimes it can get upwards of 1 minute after hitting ctrl+f for it to respond."
  — [tex.stackexchange.com/questions/706640](https://tex.stackexchange.com/questions/706640/texstudio-is-exceptionally-slow-when-using-find)
- TeX SE《Slow compiling of large documents in TeXstudio》（150 页、50 页图表，0 回答）：
  > "On its first compilation it can take well over 10 minutes. … I have missed more than one deadline because of these issues."
  — [tex.stackexchange.com/questions/434030](https://tex.stackexchange.com/questions/434030/slow-compiling-of-large-documents-in-texstudio)
- TeXworks issue #963《TeXworks editor lags on even moderately large files》：
  > "I just have a moderately large LaTeX source --- 308 KB, 4300 lines, 56-page PDF when compiled --- but the TeXworks text editor is already noticeably sluggish."
  — [TeXworks/issues/963](https://github.com/TeXworks/texworks/issues/963)
- TeX SE《TeXStudio very slow》/《TexStudio Slow Typing》— [507050](https://tex.stackexchange.com/questions/507050/texstudio-very-slow) · [514578](https://tex.stackexchange.com/questions/514578/texstudio-slow-typing)
- Reddit（B 级，搜索摘要）：《Texstudio Slow Compile》：
  > "Compiling simple documents takes forever, around one minute I'd say and right now it's mostly an empty ..."
  — [reddit.com/r/LaTeX/comments/1jss4b5](https://www.reddit.com/r/LaTeX/comments/1jss4b5/texstudio_slow_compile/)

**性质：架构性。** 两个层面都属架构：(a) 编辑器侧——语法高亮/补全/结构解析是全文档甚至全项目同步计算；(b) 编译侧——**没有任何桌面编辑器提供真正的增量编译**，每次编辑仍重跑整份文档的引擎（TeX 引擎本身无增量能力，`latexmk` 只优化"跑几遍"）。**[推断]** 这正好是 TeXstudio 类 Qt 单体架构最难补救的一环。

---

### 痛点 3：SyncTeX 双向定位错位 / 失效 —— 跨编辑器、长期存在

**具体表现**
- 正向搜索（源码 → PDF）跳到标题页、上一节或错的章节，大文档"会跳过好几张幻灯片"。
- 反向搜索（PDF → 源码）"总是跳到下一个 section/chapter"。
- `-synctex=1` 已开启、`.synctex.gz` 已生成，但功能仍不工作。
- SyncTeX 文件生成失败（残留 `XXX.synctex(busy)`，`XXX.synctex.gz` 未生成），导致"Go to source"/"Go to PDF"整体失效。

**证据（A 级）**
- TeXstudio issue #4642《Go to PDF with beamer jumps to wrong position》（open）：
  > "When I click 'Go to PDF' while cursor is on 'Content 1', the viewer jumps to the title page instead. This is a small example, but with bigger PDFs, it gets worse and jumps over multiple slides, making editing very hard."
  — [issues/4642](https://github.com/texstudio-org/texstudio/issues/4642)
- TeXstudio issue #3825《SyncTeX: Can't rename XXX.synctex(busy) to XXX.synctex.gz》（open）：
  > "A file called XXX.synctex(busy) remains, and XXX.synctex.gz is not created. As a result, some of texstudio's interactive features (like 'Go to source' and 'Go to PDF') do not (obviously) work." … "Happens ALMOST every time I try to compile."
  — [issues/3825](https://github.com/texstudio-org/texstudio/issues/3825)
- TeXstudio issue #205《Forward and backward search not working in Windows 10》：
  > "The option -synctex=1 is given and the synctex file is generated. I tried reinstall and deleting personal profile but the problem persists."
  — [issues/205](https://github.com/texstudio-org/texstudio/issues/205)
- TeX SE《Inverse search jumps to wrong position》：
  > "When Im doing the inverse search to find the corresponding code from the pdf, Texstudion always jumps to the wrong position. It is always the 'next' section or chapter, but not to the actual one Im looking for."
  — [tex.stackexchange.com/questions/400449](https://tex.stackexchange.com/questions/400449/inverse-search-jumps-to-wrong-position)
- 规模：TeXstudio `synctex` 关键词 **148 个 issue**；TSE `forward-inverse-search` 标签 **327 题**。

**性质：半架构性 + 长期未解决。** `-synctex=1` / 文件句柄竞争（`synctex(busy)`）是引擎与编辑器的协作契约问题，属长期结构缺陷；而 beamer 跳转错位更像 SyncTeX 坐标语义 + 预览器映射的架构问题。[推断] 纯版本 bug 只能解释其中一部分（如 #3825 与 Windows 文件锁相关）。

---

### 痛点 4：内置 PDF 预览器质量差（模糊 / 卡顿 / 崩溃 / 字体丢失）

**具体表现**
- 打开内置 PDF 预览直接崩溃（TeXstudio 4.8.5/4.8.6，Win10）；切到 Arthur 渲染后端不崩，但**中文以外的数字和字母全部消失**。
- 滚动"painfully slow"。
- TeXworks 0.8.3 渲染出白色竖线、奇怪边框；内置查看器"fuzzy"（与 Skim 对比）。
- macOS 上整个程序偶发灰屏/黑屏（重绘丢失）。

**证据（A 级）**
- TeXstudio issue #4023《texstudio crashes when use internal view-pdf》（47 条评论）：
  > "After I build a .tex file, I want to view it in the internal pdf-viewer(F7), then texstudio crashes. But the PDFfile has no problem because I can view it by external PDF-reader, such as Acrobat." … "If change the Render Backend of internal pdf-viewer from Splash to Arthur, then no crash, but the PDF in the viewer only show Chinese character, any number and letter disappear."
  — [issues/4023](https://github.com/texstudio-org/texstudio/issues/4023)
- TeXstudio issue #876《The whole program blacks out on MacOS》：
  > "The screen of the whole program becomes gray (or black, depending on whether darkmode is enabled) as if repaint is missed. This happens randomly, and I cannot find a pattern."
  — [issues/876](https://github.com/texstudio-org/texstudio/issues/876)
- TeXworks issue #1037《Ugly rendering of the output visualization of TeXworks》（20 条评论）：
  > "bad rendering with recent version of TeXworks 0.8.3. … A non clear output with the white vertical line, with the strange borders."
  — [TeXworks/issues/1037](https://github.com/TeXworks/texworks/issues/1037)
- TeXworks issue #892《PDF in the build-in viewer looks fuzzy》：
  > "Build-in viewer has a fuzzy pdf rendering … build-in PDF viewer shows this fuzzy looks compared to Skim"
  — [TeXworks/issues/892](https://github.com/TeXworks/texworks/issues/892)
- Reddit（B 级，搜索摘要）：《TeXstudio crashes every time I use the internal PDF viewer》— [1lvjmrf](https://www.reddit.com/r/LaTeX/comments/1lvjmrf/texstudio_crashes_every_time_i_use_the_internal/)；《painful slow scrolling in texstudio's internal pdf viewer》— [bovlrm](https://www.reddit.com/r/LaTeX/comments/bovlrm/painful_slow_scrolling_in_texstudios_internal_pdf/)

**性质：混合。** 具体崩溃/字体丢失多为版本 bug（换渲染后端即变化）；但"内置预览器体验不如外部 PDF 阅读器"是**架构性**——编辑器自研 PDF 渲染很难追上成熟阅读器（SumatraPDF/Skim/Adobe）。

---

### 痛点 5：崩溃 / 卡死 / 无响应（稳定性）

**具体表现**
- 修改设置点 Apply 就 SIGSEGV，反复崩溃后 "Forced kill after recovering failed"。
- 启动即弹 "TeXstudio has CRASHED due to a unknown at ..."，4.3.1 版本可复现，回退 4.3.0 正常。
- 词典对话框空输入点击即永久冻结（O(n²) 遍历）。

**证据（A 级）**
- TeXstudio issue #1100《Aborted (core dumped) - Forced kill after recovering failed after: SIGSEGV》：
  > "Clean installation, add `-shell-escape` parameter to LaTeX builder and click apply: … crashed with signal SIGSEGV … Forced kill after recovering failed after: SIGSEGV"
  — [issues/1100](https://github.com/texstudio-org/texstudio/issues/1100)
- TeXstudio issue #2569《TeXstudio has CRASHED due to a unknown at ...》：
  > "TeXStudio 'crashes' on startup with a popup … I do not have this problem with 4.3.0 and went back to this version for now."
  — [issues/2569](https://github.com/texstudio-org/texstudio/issues/2569)
- TeXstudio issue #4629《Thesaurus dialog: freeze when clicking "Starts With ..."》：
  > "Clicking 'Starts With ...' or 'Contains ...' with an empty input field freezes the whole application (UI thread blocked, needs to be killed)."
  — [issues/4629](https://github.com/texstudio-org/texstudio/issues/4629)
- TeXworks issue #1026《Randomly closes with error after compilation ends》（open）：
  > "about half ot the times TeXworks closes with error after compilation ends (PDF is produced)"
  — [TeXworks/issues/1026](https://github.com/TeXworks/texworks/issues/1026)
- 规模：TeXstudio `crash` **293 个 issue**、`freeze` **31 个**。

**性质：主要是版本 bug。** 例外是 [推断] Qt 版本升级（5→6）带来的渲染/主题/窗口回归反复出现，属"技术栈老化"的结构性成本。

---

### 痛点 6：配置与"可发现性"门槛 —— 大量高票问题都在问"功能在哪 / 怎么开"

**具体表现**
- 拼写检查报 `No dictionary Available`，要手动装/指向词典目录。
- 想用 `minted` 必须知道怎么加 `-shell-escape`。
- 显示行号、字数统计这类基础功能都要到设置里找。
- 深色主题要手工配每一项颜色。
- 模板在升级后"几乎全消失"。
- 用户自述"我把 TeXstudio 搞坏了，没耐心修"。

**证据（A 级，TSE 票数 + 浏览量）**
| 问题 | 票数 | 浏览量 |
|---|---|---|
| [How can I set a dark theme in TeXstudio?](https://tex.stackexchange.com/questions/108315/how-can-i-set-a-dark-theme-in-texstudio) | **206** | **230,010** |
| [Dictionary for TeXstudio: "No dictionary Available"](https://tex.stackexchange.com/questions/87650/dictionary-for-texstudio-no-dictionary-available) | **115** | **190,639** |
| [How to invoke latex with the -shell-escape flag in TeXStudio](https://tex.stackexchange.com/questions/99475/how-to-invoke-latex-with-the-shell-escape-flag-in-texstudio-former-texmakerx) | **104** | **131,938** |
| [texstudio: show line numbers in editor](https://tex.stackexchange.com/questions/183801/texstudio-show-line-numbers-in-editor) | **84** | **100,321** |
| [Is there a direct way to get word count in TeXstudio?](https://tex.stackexchange.com/questions/215692/is-there-a-direct-way-to-get-word-count-in-texstudio) | **82** | **93,625** |

- TeXstudio issue #253《Dictionary not found》：
  > "txs uses hunspell for checking. The dictionaries (de/en/fr) can be found in /usr/share/texstudio which needs to be set (if not already) in ..."
  — [issues/253](https://github.com/texstudio-org/texstudio/issues/253)
- TeXstudio issue #2975《Reintroduce templates》：
  > "With TeXstudio 4.5.2 … almost all the templates (File → New From Template...) have disappeared: in my case, only the 'Article (French) for LuaLaTeX' is still here."
  — [issues/2975](https://github.com/texstudio-org/texstudio/issues/2975)
- Reddit（A 级，PullPush 正文）r/LaTeX：
  > "I can't use texstudios cus I broke it somehow and I have no patience in fixing it."
  — [/r/LaTeX/comments/1kfon9z](https://www.reddit.com/r/LaTeX/comments/1kfon9z/switching_latex_compiler/)
- 规模：TSE `texstudio` 标签 2,075 题，其中相当一部分是"如何配置/在哪设置"类。

**性质：架构性（UX 架构）+ 文档债。** TeXstudio 维护者自己在 issue #2608《Overhauling user manual》里承认：
> "The user manual seems to be more and more outdated."
> — [issues/2608](https://github.com/texstudio-org/texstudio/issues/2608)（51 条评论）

**[推断]** 这类问题在 issue 数上不如 crash 显眼，但在"用户实际时间损耗"上可能是第一位的——23 万次浏览的"怎么开深色主题"意味着大量用户在重复踩同一个坑。

---

### 痛点 7：自动补全 / cwl 补全生态不可靠、不准确

**具体表现**
- 数学模式输入补全变慢，关闭 `$` 后还残留浮动框。
- 自定义宏包/文档类无法被补全识别（cwl 文件加载路径问题）。
- 语法高亮"不认关键字"：`\sunsection{}`（`\subsection{}` 的错拼）被高亮成和 `\subsection{}` 一模一样。
- AppImage 下无法放置系统级 cwl 补全目录。

**证据（A 级）**
- TeXstudio issue #744《Slow autofill in math mode》：
  > "When I switched from 2.12.10 to 2.12.16, the autofill in math mode (between dollar signs) became very slow, and when I finish typing the closing dollar sign there is a residual floating box about math mode … I had to switch back to version 2.12.10."
  — [issues/744](https://github.com/texstudio-org/texstudio/issues/744)
- TeXstudio issue #4622《Linux AppImage lacks a system-wide completion directory》：
  > "we have our own document class and a fitting cwl file for it. … This is seemingly impossible with the official AppImages, because all the system wide share paths are actually within the AppImage FS"
  — [issues/4622](https://github.com/texstudio-org/texstudio/issues/4622)
- TeXstudio issue #1020《Trying to achieve a special case of auto-completion…》：
  > "Due to my limited knowledge of cwl files I am unsure if my cwl syntax is wrong or if this might indicate a bug in the parsing and interpretation"
  — [issues/1020](https://github.com/texstudio-org/texstudio/issues/1020)
- TeXworks issue #776《Syntax highlighting is not keyword-compliant》（被标 **wontfix**）：
  > "Enter \sunsection{} (mistype of \subsection{}) … Should be either not highlighted, or highlighted differently … Many TeX editors (and even non-TeX like GVim or Medit) can do this. TeXwork is unfortunately not."
  — [TeXworks/issues/776](https://github.com/TeXworks/texworks/issues/776)
- TeX SE《Methods for autocompletion in TexStudio don't work》— [361259](https://tex.stackexchange.com/questions/361259/methods-for-autocompletion-in-texstudio-dont-work)；《TeXstudio: Don't autocomplete "content..." in block environments》（23 票）— [274178](https://tex.stackexchange.com/questions/274178/texstudio-dont-autocomplete-content-in-block-environments)
- 规模：TeXstudio `completion` 关键词 **263 个 issue**（8.3%）。

**性质：架构性。** cwl 是"静态补全词典"模式，无法理解自定义宏包、无法跨文件推导命令签名；且补全计算与编辑器主线程耦合（所以"变慢"）。#776 被 wontfix 说明语法高亮引擎的能力上限是被明确接受的。

---

### 痛点 8：深色主题 / UI 一致性 / 跨平台外观 —— 高票、长期未解决

**具体表现**
- 深色主题下按钮、复选框、标签不可读；页面周围有一圈无法消除的浅灰边框。
- macOS 深色模式下大量 GUI 元素在版本升级后突然不可读。
- TeXworks 到 2021 年仍**没有原生深色模式**（issue 至今 open）。
- 大文件行号 + 当前行高亮 + 折行粘贴会崩溃。

**证据（A 级）**
- TeX SE《How can I set a dark theme in TeXstudio?》**206 票 / 230,010 次浏览**（见痛点 6 表）。
- TeXstudio issue #45《Dark theme not consistent》（54 条评论）：
  > "the diffuclty to set up a consistent dark theme in the editor. … there is a big fat light gray frame around the page that i cannot get rid of"
  — [issues/45](https://github.com/texstudio-org/texstudio/issues/45)
- TeXstudio issue #848（10 个 👍）：
  > "Using the macOS dark theme after the upgrade from 2.12.16 to 2.12.18 a whole bunch of gui elements are not really well readable anymore."
  — [issues/848](https://github.com/texstudio-org/texstudio/issues/848)
- TeXworks issue #932《Implement dark mode》（2021-04 开，至今 **open**）：
  > "TeXworks is shipped with only light theme. … I'd like the option to choose between light and dark theme natively."
  — [TeXworks/issues/932](https://github.com/TeXworks/texworks/issues/932)
- TeXworks issue（从 issues 列表摘要）：
  > "Crash when pasting wrapped text with line numbers and current-line highlighting enabled"
  — [TeXworks/issues](https://github.com/TeXworks/texworks/issues)

**性质：架构性（长期未解决）。** Qt 原生控件 + 自绘编辑器混用导致主题无法统一；TeXworks 的 dark mode 请求开了 4 年多仍未落地。

---

### 痛点 9：跨平台原生体验差（macOS 尤其明显）

**具体表现**
- macOS 上最小窗口高度过大，鼠标够不到 resize 把手；默认样式下无法缩到半屏。
- 有用户直接评价"TeXstudio 感觉像一个套了薄 macOS 外壳的 Windows 应用"。
- Kile 强依赖 KDE 平台，在 Windows / 非 KDE 桌面上问题多。
- TeXworks 在 macOS Big Sur 上"完全打不开"。

**证据**
- TeXstudio issue #3637《minimum window height is too big on mac since 4.8.0》（44 条评论）：
  > "With the default screen resolution 1440*900, I cannot put the mouse pointer low enough to reach the resize handle. … Workaround: set Preferences/General/Style to Fusion."
  — [issues/3637](https://github.com/texstudio-org/texstudio/issues/3637)
- 评测（C 级，推广文，仅作旁证）：
  > "TeXShop is great but stripped-down. VS Code + LaTeX Workshop is powerful but took me three hours to configure properly (and the SyncTeX setup broke twice). TeXstudio works but feels like a Windows app wearing a thin macOS disguise."
  — [dev.to](https://dev.to/boson_jp/i-finally-ditched-overleaf-for-a-local-latex-editor-heres-what-actually-works-2cgg)
- TeXworks issue #905《TexWorks won't even start under OS X Big Sur》：
  > "I installed the latest version of TexWorks. I started on of my tex files as usual. Nothing happened... Texworks didn't react."
  — [TeXworks/issues/905](https://github.com/TeXworks/texworks/issues/905)
- Kile（B 级，搜索摘要）：《Kile won't initialize (again)》（discuss.kde.org）：
  > "I have the most common problem with Kile: the plain test doesn't work. I have a fresh EndeavourOS install, KDE Plasma on Wayland up-to-date"
  — [discuss.kde.org/t/kile-wont-initialize-again/48129](https://discuss.kde.org/t/kile-wont-initialize-again/48129)
- Kile（B 级）：《Kile LaTeX Editor works on windows?》：
  > "I saw some negative feedbacks that it is made for only KDE platforms and it might face some issues on windows 11."
  — [reddit.com/r/kde/comments/1ppmlfr](https://www.reddit.com/r/kde/comments/1ppmlfr/kile_latex_editor_works_on_windows/)
- TeXifier（B 级，搜索摘要）：《Thoughts on Texifier ?》：
  > "It often has file encoding conflicts, even if you've been typing everything from the start on the MacBook"
  — [reddit.com/r/LaTeX/comments/1mho92y](https://www.reddit.com/r/LaTeX/comments/1mho92y/thoughts_on_texifier/)

**性质：架构性（技术栈选择决定）。** Qt 跨平台外观、KDE/KParts 依赖、Cocoa 适配债，都不是单个版本能修的。

---

### 痛点 10：多文件工程管理 / 构建系统整合薄弱

**具体表现**
- 大项目一次打开全部文件会卡死（见痛点 2），项目级设置（根文件、构建规则）不透明。
- TeXworks 默认工具列表无法应对复杂构建，用户最常做的配置修改就是"加一个 make 工具"——该请求 **2017 年提出，至今 open**。
- 自定义文档类/宏包的补全需要手工放 cwl 到用户目录，无系统级/项目级机制。

**证据（A 级）**
- TeXworks issue #784《add "make" as an additional processing tool》（**open since 2017-04**，11 条评论）：
  > "Some documents have far more complicated build rules than what the tools listed by default can handle, and so relying on calling GNU Make to sort it all out is quite common. **Adding make is the single most common configuration change that I have to make to new installations of TeXworks.**"
  — [TeXworks/issues/784](https://github.com/TeXworks/texworks/issues/784)
- TeXstudio issue #4410（同痛点 2）与 #4622（同痛点 7）说明项目级/系统级配置机制的缺失。
- TeXstudio issue #54《Feature request: Macros saved in files inside the project》：
  > "the issue is that I have to program these macros on both my laptop and desktop. And everytime I format my computer, I have to reprogram all the macros"
  — [issues/54](https://github.com/texstudio-org/texstudio/issues/54)

**性质：架构性。** 桌面编辑器的"项目"概念普遍弱于 IDE：没有统一的构建配置、依赖图、项目级设置继承。

---

## 3. 架构性 / 长期未解决 vs 版本 bug 分类

| 痛点 | 架构性 / 长期 | 版本 bug | 判断依据 |
|---|---|---|---|
| 1 编译错误定位差 | ✅ 架构性 | 部分（log parser 正则） | 依赖正则解析 `.log`；#50 是解析器，但根因是引擎无结构化输出 |
| 2 大项目性能 / 无增量 | ✅ 架构性 | 部分 | #4410/#3470/#963 跨版本跨平台复现；无真正增量是引擎层限制 |
| 3 SyncTeX 错位 | ✅ 半架构 | 部分（#3825 文件锁） | #4642/#205/#400449 长期存在；TSE 327 题 |
| 4 PDF 预览器质量 | 混合 | ✅ 多数为版本 bug | #4023 换后端即变；#1037/#892 随版本变化 |
| 5 崩溃 / 卡死 | 部分（Qt 升级回归） | ✅ 主要为版本 bug | #2569 回退 4.3.0 即好；#4629 是纯逻辑 bug |
| 6 配置 / 可发现性 | ✅ 架构性 + 文档债 | — | 230k 浏览的深色主题问题、#2608 官方承认手册过时 |
| 7 自动补全 / cwl | ✅ 架构性 | 部分 | cwl 静态词典模型；#776 被 wontfix |
| 8 深色主题 / UI 一致性 | ✅ 架构性、长期未解决 | — | #45（2018 开）、#848、TeXworks #932（2021 开至今） |
| 9 跨平台原生体验 | ✅ 架构性 | 部分 | #3637、#905；技术栈决定 |
| 10 多文件工程 / 构建整合 | ✅ 架构性 | — | TeXworks #784 open 9 年；TeXstudio #4410 |

**结论（事实级）**：10 类痛点中，**至少 7 类属架构性/长期未解决**（1、2、3、6、7、8、9、10）。这与"桌面编辑器生态十年没有结构性变化"的观察一致——TeXstudio 最新稳定版仍是 4.9.7（2026-08），TeXworks 是 0.6.11（2026-02），版本号本身就说明演进节奏。

---

## 4. 桌面编辑器 vs 在线编辑器：特有优势与劣势

### 4.1 桌面编辑器的优势（有来源）

| 优势 | 证据 |
|---|---|
| **离线可用 / 不受服务宕机影响** | r/LaTeX 用户原话："Less swanky, sure, but I could work anywhere without internet and didn't rely on servers somewhere being up in an emergency, it's just nice peace of mind!" — [PullPush 归档](https://www.reddit.com/r/LaTeX/comments/1km90a3/overleaf_down/) |
| **数据/合规可控** | r/LaTeX《Overleaf vs local setup for company documents?》："I looked into Overleaf, but I'd prefer a local setup (for privacy/compliance reasons…)" — [1ppqzci](https://www.reddit.com/r/LaTeX/comments/1ppqzci/overleaf_vs_local_setup_for_company_documents/) |
| **无编译超时** | Overleaf 官方文档存在"compile timeout"概念并需专门排障：— [docs.overleaf.com](https://docs.overleaf.com/troubleshooting-and-support/fixing-and-preventing-compile-timeouts)；Reddit 有《Overleaf's new compilation timeout is a joke》— [1ndn0r3](https://www.reddit.com/r/LaTeX/comments/1ndn0r3/overleafs_new_compilation_timeout_is_a_joke/) |
| **本地工具链 / Git / 任意编辑器共存** | HN Show HN："the editor can read and write your project folder through the File System Access API, so you can use git, any local TeX install, local AI Agents, or any other editor on the same files" — [HN 49441375](https://news.ycombinator.com/item?id=49441375) |
| **本地方便安装任意宏包/字体/自定义类** | TeXstudio #4622 反映用户对自定义类补全的需求；WinEdt 官网强调 "WinEdt is a desktop application and none of a user's data is accessible to the WinEdt Team." — [winedt.com/registration.html](https://www.winedt.com/registration.html) |

### 4.2 桌面编辑器的劣势（有来源）

| 劣势 | 证据 |
|---|---|
| **实时协作缺失** | 评测原文："Real-time collaborative editing doesn't exist here. If you and your advisor are doing live back-and-forth edits simultaneously, Overleaf still wins." — [dev.to](https://dev.to/boson_jp/i-finally-ditched-overleaf-for-a-local-latex-editor-heres-what-actually-works-2cgg) |
| **首次配置成本高** | 同一篇："VS Code + LaTeX Workshop is powerful but took me three hours to configure properly (and the SyncTeX setup broke twice)." |
| **错误诊断落后于 Overleaf** | TeX SE 574762 的对比图（2,325 浏览） |
| **需要自己维护 TeX 发行版** | 同一篇："You still need MacTeX or TeX Live. … Installing MacTeX takes about 20 minutes the first time (it's a ~4GB download)." |
| **跨平台体验割裂** | TeXstudio #3637（macOS 窗口）、Kile（KDE 依赖）、TeXworks #905（macOS 打不开） |

**[推断]** 桌面编辑器真正的护城河不是"功能更多"，而是**离线 + 数据主权 + 无超时**这三件事；而它持续丢用户的地方是**诊断质量、大项目性能、开箱即用程度**——这三项恰好都是架构层面的欠债。

---

## 5. 量化证据汇总（可直接引用）

| 指标 | 数值 | 来源 |
|---|---|---|
| TeXstudio 总 issue | 3,160 | GitHub Search API |
| TeXstudio open issue | 431 | repo API |
| TeXstudio stars | 3,619 | repo API |
| TeXstudio `crash` issue | 293 | Search API |
| TeXstudio `completion` issue | 263 | Search API |
| TeXstudio `synctex` issue | 148 | Search API |
| TeXstudio `slow` issue | 76 | Search API |
| TeXstudio `freeze` issue | 31 | Search API |
| TeXstudio 4.9.7 Win 安装包下载 | 61,807 | Releases API |
| TeXstudio 4.9.7 macOS(arm) 下载 | 64,281 | Releases API |
| TeXworks 总 issue / open | 1,012 / 227 | repo API |
| TeXworks stars | 778 | repo API |
| TeXworks 0.6.10 Win 安装包下载 | 34,589 | Releases API |
| TSE `texstudio` 题数 | 2,075 | tags API |
| TSE `texmaker` 题数 | 1,558 | tags API |
| TSE `texworks` 题数 | 516 | tags API |
| TSE `winedt` 题数 | 395 | tags API |
| TSE `texshop` 题数 | 727 | tags API |
| TSE `kile` 题数 | 263 | tags API |
| TSE `forward-inverse-search` 题数 | 327 | tags API |
| TSE 深色主题问题票数 / 浏览 | 206 / 230,010 | SE API |
| TSE 拼写词典问题票数 / 浏览 | 115 / 190,639 | SE API |
| TSE shell-escape 问题票数 / 浏览 | 104 / 131,938 | SE API |
| TSE 编辑器总览题票数 / 浏览 | 951 / 753,853 | SE API |
| Overleaf 用户 / 日活 | 900 万 / 40 万 | Overleaf 官方博客（2021） |
| LaTeX Workshop 安装量 | 5,596,003 | VS Code Marketplace |
| WinEdt 学生/教育/商业价 | $50 / $80 / $150 | winedt.com |
| Texifier 单席价 | $39.99 | texifier.com |

---

## 6. 用户真正愿意为之付费 / 换工具的三大理由

> 下面是基于上述证据的**判断**，每条都标注了支撑证据与推断成分。

### 理由 1：**"编译不能超时 + 数据不出本机"** —— 离线与合规是硬需求
- 事实支撑：Overleaf 有专门的编译超时排障文档；r/LaTeX 有《Overleaf's new compilation timeout is a joke》；企业用户明确为 privacy/compliance 选择本地；r/LaTeX 用户把"不依赖服务器在线"称为 "peace of mind"。
- [推断] 这是**唯一一条在线编辑器结构性无法满足**的需求，因此付费意愿最刚性。目标客群：写博士论文/书籍/企业合规文档的人，以及网络受限环境（差旅、涉密、实验室离线机）。

### 理由 2：**"大项目不再卡死 + 编辑到预览的延迟足够低"** —— 性能是可感知的生产力
- 事实支撑：7,000 页项目加载 5–10 分钟并触发 endless loop 保护；150 页论文首次编译 >10 分钟；`Ctrl+F` 等 1 分钟；308 KB / 4,300 行文件编辑就迟钝。这些不是个别抱怨，而是跨编辑器、跨平台、跨版本反复出现。
- [推断] 用户愿意为"打字不掉帧 + 大项目秒开 + 每次编辑只重排受影响部分"付费。这也是当前桌面编辑器**最大的可攻占缺口**（无真正增量是引擎层限制，但"编辑器侧不全量重算 + 只重排变更页预览"是编辑器可以做的）。

### 理由 3：**"报错直接告诉我改哪一行" + "一次性买断，不要订阅"** —— 诊断质量与定价模式
- 事实支撑：错误信息问题 2,325 浏览、"log tab is also blank"；用户明确对比 Overleaf 能给行号；WinEdt 卖"终身个人许可"（$50 学生，且 30 多年只要求升级两次）；Texifier 单席 $39.99；HN 上有开发者因为 "git sync … sits behind a paywall" 而自己造编辑器，并说 "I didn't want to pay subscriptions for things that should simply just work"。
- [推断] 组合拳是：**把"引擎原始日志 → 可操作诊断"做成一等公民**（指出行号、给出修复建议、点击跳转），叠加**买断/低价永久许可**而非订阅。促销性质的 dev.to 文章专门把"AI 读编译日志+源码定位错误并给出 diff"当作核心卖点，也侧面印证这个方向的吸引力（注意该文为厂商推广，证据力有限）。

---

## 7. 本次调研没找到 / 无法验证的内容

1. **Kile 的 Bugzilla bug 总数**：`bugs.kde.org` REST 接口不提供总数，未取得。只能用 TSE `kile` 标签 263 题做代理。
2. **TeXmaker / TeXShop / WinEdt 的官方 issue 计数**：三者均无公开 GitHub issue 区（TeXmaker 无、TeXShop 用邮件列表、WinEdt 闭源），无法与 TeXstudio 3,160 直接对比。
3. **Reddit 原帖的完整票数/评论数**：`www.reddit.com` 在本环境被解析为非公网 IP，无法直连；PullPush 归档只返回部分评论，未取得完整帖子指标。文中标注为 B 级的 Reddit 证据均来自搜索摘要或归档片段。
4. **TeXstudio 历史版本下载总量 / 真实用户数**：GitHub 只给单 asset 的 `download_count`，Linux 包管理器（apt/AUR/Flatpak）与 TeX Live 内置分发的安装量无法统计。
5. **Overleaf 的当前（2026）用户规模**：官方博客最新公开数字停在 2021 年的 900 万，未找到更新来源。
6. **HN 上关于桌面 LaTeX 编辑器的专门讨论串**：HN Algolia 检索到的是"在线 LaTeX 编辑器"为主（Overleaf 246 分 / 127 评论、SwiftLaTeX 741 分 / 114 评论、TeXbrain 118 分 / 28 评论），未找到针对 TeXstudio/TeXworks 的独立高分讨论串。
