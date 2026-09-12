# 中文高校学位论文 LaTeX 模板：官方引擎要求调研

> 调研目的：为 TexPresso（Windows 首发、默认引擎 XeLaTeX）判断「默认 XeLaTeX 能否开箱即用编译用户下载的论文模板」，以及是否需要做引擎自动推断。
>
> 调研时间：2026-09（仓库状态以当时 master/main/dev 分支为准）
> 取证方式：web_fetch 抓取 raw.githubusercontent.com 的 README / .dtx / .cls / Makefile / latexmkrc / CI workflow / main.tex 原文
> 标记约定：**[事实]** = 有原文引用（附 URL）；**[推断]** = 由代码/配置推导，非文档明说；**未找到** = 未能取得官方原文

---

## 1. 先修正一个前提

任务描述假设这批模板「不在 TeX Live/CTAN 发行版里」。**实测不成立**：抽样 19 个模板中至少 9 个已被 CTAN 收录（进 TeX Live），其中多数只在**开发期**跑 GitHub 源码，而学生实际下载的是 **GitHub Releases 的发布版 zip**。

★ 标记 = 同时有 CTAN 包（`tlmgr install` 可装）：

| 模板 | CTAN 证据 |
|---|---|
| thuthesis | README「[CTAN](https://www.ctan.org/pkg/thuthesis)：可能滞后正式发布少许时间」 |
| ustcthesis | https://ctan.org/pkg/ustcthesis（README 未列 CTAN，但包存在） |
| njuthesis | README 徽章 + 本地编译段「推荐使用包管理器安装 `njuthesis` 的最新版」 |
| sysuthesis | README 徽章 `CTAN Version` + `sudo tlmgr` 段 |
| hithesis | README「hithesis 已收录在[CTAN](https://ctan.org/pkg/hithesis)中，用户安装 TeXLive 将自带窝工模板」 |
| BUCTthesis | https://ctan.org/pkg/buctthesis（搜索结果显示；README 未提 CTAN） |
| SEUThesis(Reanon 系) | 未确认 |

**对本次决策的影响**：CTAN 收录 ≠ 学生不用下载。这些模板的 README 都把「GitHub Releases 下载 zip」列为首选，且清华/上交/nju 的 CTAN 版本「滞后少许」，学生仍会下载仓库源码或发布版。所以「用户下载模板」这一场景成立，但**「不在发行版里」的说法应改为「不一定随发行版分发，学生普遍手动下载」**。

---

## 2. 主表：官方引擎要求（19 个模板）

| # | 模板 | 仓库 URL | 官方要求的引擎（原文） | 主文件 / 文档类 | ctex / xeCJK / fontspec | latexmk | shell-escape | biber |
|---|---|---|---|---|---|---|---|---|
| 1 | thuthesis | https://github.com/tuna/thuthesis | **XeLaTeX**（+ LuaLaTeX 可选） | `thuthesis-example.tex`：`\documentclass[degree=master]{thuthesis}` | ctex + xeCJK（`\RequireXeTeX`） | ✅ `latexmk` | ✅ `-shell-escape` | 可选（默认 BibTeX） |
| 2 | ucasthesis | https://github.com/mohuangrui/ucasthesis | **pdflatex / xelatex / lualatex 三引擎** | `Thesis.tex`：`\documentclass[twoside]{Style/ucasthesis}` | ctexbook + xeCJK（xetex）/ fontspec | 自带 `artratex.sh/.bat`（默认 xelatex） | ❌ 未要求 | 可选（bibtex 或 biber） |
| 3 | ustcthesis | https://github.com/ustctug/ustcthesis | **XeLaTeX 默认，LuaLaTeX 亦支持** | `main.tex`：`\documentclass[degree=doctor]{ustcthesis}` | ctex + xeCJK | ✅ `LATEXMK = latexmk -xelatex` | ❌ 未要求 | 默认 BibTeX |
| 4 | njuthesis | https://github.com/nju-lug/NJUThesis | **XeLaTeX** | `njuthesis-sample.tex`：`\documentclass{njuthesis}` | LaTeX3 + ctex/xeCJK | ✅ `latexmk -xelatex` | ❌ 未要求 | biblatex（`\printbibliography`） |
| 5 | SJTUThesis | https://github.com/sjtug/SJTUThesis | **XeTeX 与 LuaTeX** | `main.tex`：`\documentclass[type=master]{sjtuthesis}` | ctex + xeCJK | ✅ `Recipe: latexmk (xelatex)` | ❌ 未要求 | biblatex（biber） |
| 6 | hithesis | https://github.com/hithesis/hithesis | **XeLaTeX**（英语论文亦 xelatex） | `examples/hitbook/chinese/thesis.tex`：`\documentclass[fontset=fandol,type=doctor,campus=harbin]{hithesisbook}` | ctex + xeCJK | ✅ `latexmk` | ✅ `-shell-escape` | ❌ 用 bibtex + splitindex |
| 7 | pkuthss | https://github.com/CasperVector/pkuthss | **未找到明确说明**；README.txt 指向 `pkuthss.pdf`，仓库仅含 README.txt/Makefile/doc/tex/utils | 未取到（`doc/example/`） | [推断] xeCJK（PKU 系列） | 有 `Makefile` | 未找到 | 未找到 |
| 8 | BUCTthesis | https://github.com/the-ccsn/BUCTthesis | **仅 XeTeX**（强校验，非 XeTeX 直接中止） | `main.tex`：`\documentclass[type=doctor,fontset=windows,...]{buctthesis}` | ctexbook（仅 XeTeX 引擎） | ✅ `latexmk` | ❌ 未要求 | ❌ BibTeX |
| 9 | CUMTthesis | https://github.com/OpenCUMT/thesis-latex | **XeLaTeX** | `thesis.tex` | 未取到 | ✅ `latexmk -xelatex thesis.tex` | 未取到 | 未取到 |
| 10 | nuaathesis | https://github.com/nuaatug/nuaathesis | **XeLaTeX**（日文例外） | `demo_chs/master.tex`：`\documentclass[lang=cn,degree=master,...]{nuaathesis}` | 未取到 | ✅ 要求「将编译方式修改为 latexmk」 | 未取到 | 用 `.bst` + bibtex |
| 11 | whu-thesis | https://github.com/whutug/whu-thesis | **XeLaTeX**（CI 注释直书） | `demo.tex`：`\documentclass[type=master,class=academic]{whu-thesis}` | ctex/xeCJK | 未强制 | ✅ CI 用 `-shell-escape` | 可选（bibtex 默认 / biblatex） |
| 12 | seuthesis | https://github.com/seucs/seuthesis | **XeLaTeX**（魔术注释，无正文说明） | `main.tex`：`\documentclass[bachelor,nocolorlinks,printoneside]{seuthesis}` | xeCJK（`\RequirePackage[CJKnumber,slantfont,boldfont]{xeCJK}`） | ❌ | ❌ | ❌ bibtex + thuthesis.bst |
| 13 | SEUThesisLatexTemplate | https://github.com/Reanon/SEUThesisLatexTemplate | **XeLaTeX**（Overleaf 必须设编译器） | `main.tex` + `template/seumasterthesix.cls` | 未取到 | 未取到 | 未取到 | bibtex（`reference.bib`） |
| 14 | tjuthesis | https://github.com/haimingz/tjuthesis | **XeLaTeX**（默认链 `latexmk + xelatex`） | `main.tex`：`\documentclass[master]{tjuthesis}` | `\RequirePackage[heading=true,zihao=-4]{ctex}` + `\setmainfont` | ✅ | ✅ `-shell-escape` | ✅ biblatex/biber |
| 15 | XJTU-thesis | https://github.com/obster-y/XJTU-thesis | **XeLaTeX**（「编译文档请使用 XeLaTeX 引擎」） | `main.tex`：`\documentclass[bachelor,...]{XJTU-thesis}` | 未取到 | ✅ 提供 latexmkrc | ✅ `--shell-escape` | ✅ biber + makeglossaries |
| 16 | sysu-thesis | https://github.com/sysu-thesis/sysu-thesis | **未找到**（README 只指向 GitLab wiki） | 未取到 | 未取到 | 未取到 | 未取到 | 未取到 |
| 17 | sysuthesis(irenier) | https://github.com/irenier/sysuthesis | **XeLaTeX** | 最小示例：`\documentclass{sysuthesis}` | LaTeX3 | ✅ `latexmk -xelatex` | ❌ 未要求 | 未取到 |
| 18 | whu-thesis-latex-template | https://github.com/iamywang/whu-thesis-latex-template | **XeLaTeX** | `thesis.tex`：`\documentclass[AutoFakeBold=2]{WHUPhd}` | xeCJK（`AutoFakeBold`） | ✅ `latexmk -xelatex` | ❌ | ❌ bibtex（`\bibliography{bibs/ref1}`） |
| 19 | shuthesis | https://github.com/BlueFisher/shuthesis | **XeCJK**（v2.x 起） | `main.tex` | xeCJK | 未取到 | 未取到 | 未取到 |
| — | DUT-thesis | **未找到** 统一官方仓库 | — | — | — | — | — | — |
| — | TJU 其他分支 | 见 `liangzhenduo/TJU-thesis-template` | 未取到 | — | — | — | — | — |

---

## 3. 逐条来源与原文引用

### 3.1 thuthesis（清华）★CTAN

- 仓库：https://github.com/tuna/thuthesis
- **[事实] 引擎**：`thuthesis-example.tex` 第 1–3 行为魔术注释：
  ```
  % !TEX encoding = UTF-8
  % !TEX program = xelatex
  ```
  https://raw.githubusercontent.com/tuna/thuthesis/master/thuthesis-example.tex
- **[事实] 必须 XeTeX 生成 cls**：README「Clone the project source code … and run `xetex thuthesis.ins`」；`Makefile` 中 `cls: $(CLSFILE)` 目标命令为 `xetex $(PACKAGE).ins`。https://raw.githubusercontent.com/tuna/thuthesis/master/Makefile
- **[事实] shell-escape + no-pdf 流程**：`latexmkrc`
  ```
  $pdf_mode = 5;
  $xelatex = "xelatex -shell-escape -file-line-error -halt-on-error -interaction=nonstopmode -no-pdf -synctex=1 %O %S";
  ```
  https://raw.githubusercontent.com/tuna/thuthesis/master/latexmkrc
- **[事实] 依赖**：CI 安装 `fontspec xecjk ulem xetex`、`cjk ctex everysel zhnumber`、`biber biblatex biblatex-gb7714-2015` → 说明默认 BibTeX、可选 BibLaTeX/biber。https://raw.githubusercontent.com/tuna/thuthesis/master/.github/workflows/test.yml
- **[事实] 文档类**：`\documentclass[degree=master]{thuthesis}`（可 degree=doctor|master|bachelor|postdoc；fontset=windows|mac|fandol|ubuntu）
- **[推断] pdfLaTeX 不可用**：cls 基于 ctex + xeCJK 组合且 README 只给 xetex/latexmk 路径，未见任何 pdfLaTeX 说明。

### 3.2 ucasthesis（国科大）——**唯一的「三引擎」样本**

- 仓库：https://github.com/mohuangrui/ucasthesis （3.9k star，**无 CTAN 徽章**）
- **[事实] README 原文**：
  > 「兼顾操作系统：Windows、Linux、MacOS；LaTeX 编译引擎：pdflatex、xelatex、lualatex；文献编译引擎：bibtex、biber (biblatex)」
  https://raw.githubusercontent.com/mohuangrui/ucasthesis/master/README.md
- **[事实] 代码级引擎探测**：`Style/artratex.sty`
  ```latex
  \RequirePackage{ifxetex,ifluatex}% LaTeX engine detection
  \ifxetex%
      \artx@xetextrue
      \RequirePackage{xeCJK}% support calling system fonts
  \else\ifluatex%
      \artx@luatextrue
  \else%
      \artx@pdftextrue
  \fi\fi%
  ```
  并在 `\ifartx@pdftex` 分支加载 `inputenc/fontenc/newtxtext`，在 xetex/lualatex 分支加载 `fontspec`。https://raw.githubusercontent.com/mohuangrui/ucasthesis/master/Style/artratex.sty
- **[事实] 默认引擎仍是 xelatex**：`artratex.bat` 中 `set CompilerOrder="2"` → 选 `xelatex`（`"1"` 才是 `pdflatex`）；`artratex.sh` 参数表 `<l:lualatex>, <p:pdflatex>, <x:xelatex>`，未给参数时 `else` 分支落 `xelatex`。
  https://raw.githubusercontent.com/mohuangrui/ucasthesis/master/artratex.bat ／ https://raw.githubusercontent.com/mohuangrui/ucasthesis/master/artratex.sh
- **[事实] 文档类**：`Thesis.tex` → `\documentclass[twoside]{Style/ucasthesis}` + `\usepackage[authoryear,list]{Style/artratex}`

### 3.3 ustcthesis（中科大）★CTAN

- 仓库：https://github.com/ustctug/ustcthesis
- **[事实] 编译命令（README）**：
  > ```
  > latexmk -xelatex main.tex
  > ```
  https://raw.githubusercontent.com/ustctug/ustcthesis/master/README.md
- **[事实] Makefile 默认引擎**：`LATEXMK = latexmk -xelatex` https://raw.githubusercontent.com/ustctug/ustcthesis/master/Makefile
- **[事实] LuaLaTeX 也过 CI**：
  ```yaml
  - name: Test example with LuaTeX
    run: |
      make clean
      latexmk -lualatex main.tex
  ```
  https://raw.githubusercontent.com/ustctug/ustcthesis/master/.github/workflows/test.yml
- **[事实] 主文件魔术注释**：`% !TEX program = xelatex`；`\documentclass[degree=doctor]{ustcthesis}`，选项含 `fontset = windows | mac | ubuntu | fandol`。https://raw.githubusercontent.com/ustctug/ustcthesis/master/main.tex
- **[事实] README 警告**：「本模板要求 TeX Live、MacTeX、MiKTeX 不低于 2021 年的发行版」「**不支持** CTeX 套装」（CTeX 套装 = pdfLaTeX/CJK 老路线）

### 3.4 njuthesis（南大）★CTAN

- 仓库：https://github.com/nju-lug/NJUThesis
- **[事实] 本地编译（README）**：
  > 「4. 在模板根目录下运行 `latexmk -xelatex` 运行编译，得到对应的 PDF 文件」
  https://raw.githubusercontent.com/nju-lug/NJUThesis/master/README.md
- **[事实] 文档类**：`\documentclass{njuthesis}` + `\njusetup{}`，`\printbibliography` → biblatex 路线
- **[推断] 需要 biber**：用了 `\printbibliography`（biblatex 宏），默认后端 biber；README 未明说。

### 3.5 SJTUThesis（上交）

- 仓库：https://github.com/sjtug/SJTUThesis
- **[事实] README 原文**：
  > 「SJTUThesis 支持 XeTeX 与 LuaTeX 引擎，字符编码仅支持 UTF-8。」
  > 「请注意，一般在线编辑器新建项目默认使用 pdfLaTeX 编译器，你需要设置使用 XeLaTeX 编译器；并使用最新的 TeX 发行版本。」
  > 「VS Code：安装 "LaTeX Workshop" 扩展，选择预设配方 `Recipe: latexmk (xelatex)` 编译即可」
  https://raw.githubusercontent.com/sjtug/SJTUThesis/master/README.md
- **[事实] 主文件魔术注释**：`% !TeX TXS-program:compile = txs:///latexmk/{-pdf} -xelatex`，`\documentclass[type=master]{sjtuthesis}`。https://raw.githubusercontent.com/sjtug/SJTUThesis/master/main.tex
- **[事实] 文档类主体在 SJTUTeX（CTAN）**：README「SJTUThesis 依赖的文档类集 SJTUTeX 已经被 CTAN 收录」

### 3.6 hithesis（哈工大）★CTAN

- 仓库：https://github.com/hithesis/hithesis
- **[事实] 手动编译流程（README）**：
  > ```
  > xelatex -shell-escape thesis.tex
  > bibtex thesis
  > xelatex -shell-escape thesis.tex
  > xelatex -shell-escape thesis.tex
  > splitindex thesis -- -s hithesis.ist  # 自动生成索引
  > xelatex -shell-escape thesis.tex
  > ```
  > 「全自动火力覆盖（只需要输入一次命令，源文件更改后自动识别更改自动编译）：`latexmk`」
  https://raw.githubusercontent.com/hithesis/hithesis/master/README.md
- **[事实] Windows 下用 lualatex 生成 cls**：「如果是 Windows 执行（作者没测试过，如遇问题同上）`lualatex hithesis.ins`」（`.ins` 生成用 lualatex **不是因为论文要 LuaLaTeX**，只是 Windows 缺 `latex` 可执行名）
- **[事实] 主文件魔术注释**：`% Local Variables: TeX-engine: xetex`；`\documentclass[fontset=fandol,type=doctor,campus=harbin]{hithesisbook}`
  https://raw.githubusercontent.com/hithesis/hithesis/master/examples/hitbook/chinese/thesis.tex
- **[事实] latexmk 在 Makefile 内**：CI 直接 `cd examples/hitbook/chinese && make thesis`（Makefile 内即 `latexmk`）。https://raw.githubusercontent.com/hithesis/hithesis/master/.github/workflows/test_texlive_latest.yml

### 3.7 pkuthss（北大）——**未找到引擎原文**

- 仓库：https://github.com/CasperVector/pkuthss （620 star；README 是 `README.txt`，684 字节）
- **[事实] 仓库实际内容**：根目录仅 `Makefile`、`README.txt`、`doc/`、`tex/`、`utils/`（API 列目录证据）
- **[事实] README.txt 原文**：
  > 「This file (`README.txt') is *NOT* intended as the documentation for this package; please see the file `pkuthss.pdf' instead.」
  > 「Homepage: <https://gitea.com/CasperVector/pkuthss>」
  https://raw.githubusercontent.com/CasperVector/pkuthss/master/README.txt
- **[推断]** 官方引擎要求写在 `pkuthss.pdf` 手册里（本文未抓取 PDF）。PKU 生态的其他分支（如 `iofu728/pkuthss`、`doubleZ0108/pkuthss-mac`）均属**第三方衍生**，不能当官方依据。

### 3.8 BUCTthesis（北化）★CTAN（ctan.org/pkg/buctthesis）

- 仓库：https://github.com/the-ccsn/BUCTthesis
- **[事实] 官方手册（.dtx）原文，最硬的证据**：
  > 「模板基于 `ctexbook` 文档类构建，但**仅** 支持 `XeTeX` 引擎。」
  https://raw.githubusercontent.com/the-ccsn/BUCTthesis/master/buctthesis.dtx
- **[事实] 代码级强制中止**（同一 .dtx，macrocode 段）：
  ```
  % 装载 \pkg{ifxetex} 宏包，并通过 \cs{RequireXeTeX} 命令检查编译命令。
  % 若未使用 \XeTeX\ 或 \XeLaTeX\ 将强制中止编译并发出警告。
  \RequireXeTeX
  ```
- **[事实] 编译流程**：
  > ```
  > xelatex main.tex
  > bibtex main.tex
  > xelatex main.tex
  > xelatex main.tex
  > ```
  > 「或者使用更方便的 latexmk 工具：不加参数则默认对 `main.tex` 进行编译」；README 另有「对 `main.tex` 文件执行 `latexmk` 或 `xelatex -> bibtex -> xelatex -> xelatex` 的全编译」。
- **[事实] 文档类**：`\documentclass[type = doctor, fontset = windows, submit, openany]{buctthesis}`
- **[事实] 环境要求**：README「模板在 TeX Live 2020 及更新版本、Overleaf 上可顺利编译，可能不兼容于低版本。**不支持** CTeX 套装。」
- **[事实] 字体坑**：封面需要 `STXingkai`、`FZXiaoBiaoSong-B05S`、`FZDaBiaoSong-B06S` 等方正字体（.dtx 列出清单），需用户自行安装。

### 3.9 CUMTthesis（中国矿业大学）

- 仓库：https://github.com/OpenCUMT/thesis-latex
- **[事实] README 原文**：
  > 「**编译论文** `thesis.pdf`：`latexmk -xelatex thesis.tex`」「**清理编译临时文件**：`latexmk -c`」
  https://raw.githubusercontent.com/OpenCUMT/thesis-latex/master/README.md
- **[事实] 依据**：《中国矿业大学研究生学位论文撰写规定及模板（2025年版）》，适用于本科/硕士/博士；「暂不支持外文学院硕士学位论文」

### 3.10 nuaathesis（南航）

- 仓库：https://github.com/nuaatug/nuaathesis
- **[事实] README 原文**：
  > 「使用 XeLaTeX 进行编译 (日文例外)」
  > 「将 LaTeX 编译方式修改为 latexmk，尝试编译论文」
  https://raw.githubusercontent.com/nuaatug/nuaathesis/master/README.md
- **[事实] 主文件**：`demo_chs/master.tex` → `\documentclass[lang=cn,degree=master,openany,oneside]{nuaathesis}`；参考文献 `\bibliographystyle{masterbib}` + `\bibliography{bib/sample}`（bibtex 路线）。

### 3.11 whu-thesis（武大，whutug）

- 仓库：https://github.com/whutug/whu-thesis （1.35k star）
- **[事实] CI 原文（比 README 更硬）**：`.github/workflows/latex-compile.yml`
  ```yaml
  # Compile the LaTeX document with XeLaTeX
  - name: Compile whu-thesis
    uses: xu-cheng/latex-action@v2
    with:
      root_file: whu-thesis-demo.tex
      args: -xelatex -file-line-error -interaction=nonstopmode -shell-escape
  ```
  https://raw.githubusercontent.com/whutug/whu-thesis/main/.github/workflows/latex-compile.yml
- **[事实] 主文件**：`\documentclass[type = master,class = academic]{whu-thesis}`；`cjk-font = mac, % ... 可选项为 windows, mac, fandol, sourcehan, none`；`bib-backend = bibtex, % 可选项为 bibtex, biblatex`。https://raw.githubusercontent.com/whutug/whu-thesis/main/demo.tex
- **[事实] README 未写引擎**：只写「开始您的论文写作并编译 PDF，编译方式主要有两种：本地编译 / 在线编译」。https://raw.githubusercontent.com/whutug/whu-thesis/main/README.md

### 3.12 seuthesis（东南大学，seucs）

- 仓库：https://github.com/seucs/seuthesis
- **[事实] 主文件魔术注释**：
  ```
  % !TEX TS-program = xelatex
  % !TEX encoding = UTF-8 Unicode
  \documentclass[bachelor,nocolorlinks, printoneside]{seuthesis}
  ```
  https://raw.githubusercontent.com/seucs/seuthesis/master/main.tex
- **[事实] cls 依赖 xeCJK + 本地字体**：`seuthesis.cls` 内
  ```latex
  \RequirePackage{xltxtra}
  \RequirePackage[CJKnumber,slantfont,boldfont]{xeCJK}
  \setCJKmainfont[Path = ./font/, BoldFont=simhei.ttf, ...]{simsun.ttf}
  \setmainfont[Path = ./font/, BoldFont=timesbd.ttf, ...]{times.ttf}
  ```
  https://raw.githubusercontent.com/seucs/seuthesis/master/seuthesis.cls
- **[事实] README 完全没提引擎**：只写「本 LaTeX 模版在 Windows/Mac OS/Linux 上均可以使用，并自备相应字体，不存在字体缺失问题」。https://raw.githubusercontent.com/seucs/seuthesis/master/README.md
- **[推断] 加载 `CJK` 宏包只是残留**：`main.tex` 里 `\usepackage{CJK,CJKnumb}`，但 cls 已经用 xeCJK 接管，故「用 CJK + pdfLaTeX」不是官方支持路径。

### 3.13 SEUThesisLatexTemplate（东南大学网安院，社区版）

- 仓库：https://github.com/Reanon/SEUThesisLatexTemplate （276 star）
- **[事实] README 原文**：
  > 「3、配置编译器为 XeLaTex。」
  > 「如果你无法生成 PDF，那么有几种可能：1. 你没有设置编译器为 「XeLaTex」」
  > 「Tex 编译器：xelatex」
  https://raw.githubusercontent.com/Reanon/SEUThesisLatexTemplate/master/README.md
- **[事实] 结构与字体坑**：`template/seumasterthesix.cls`、`reference.bib`（BibTeX）、`font/` 目录「存放着一些模版必要的字体……如果你在本地编译论文过程中失败，请首要考虑字体问题」

### 3.14 tjuthesis（天津大学）

- 仓库：https://github.com/haimingz/tjuthesis
- **[事实] README 原文**：
  > 「编译链、编译工具（默认使用 latexmk + xelatex）。」
  https://raw.githubusercontent.com/haimingz/tjuthesis/master/README.md
- **[事实] VS Code 默认配方**：`.vscode/settings.json`
  ```json
  "latex-workshop.latex.recipe.default": "latexmk (xelatex)",
  ... "xelatexmk": { "command": "latexmk",
        "args": ["-xelatex","-synctex=1","-shell-escape","-interaction=nonstopmode","-file-line-error","%DOC%"] }
  ```
  https://raw.githubusercontent.com/haimingz/tjuthesis/master/.vscode/settings.json
- **[事实] cls 依赖**：`\RequirePackage[heading=true, zihao=-4]{ctex}` + `\setmainfont{Times New Roman}` + `\setCJKfamilyfont{zhsong}[AutoFakeBold={3.17}]{SimSun}` → ctex/fontspec 都是 Unicode 引擎专有 → pdfLaTeX 不可行。https://raw.githubusercontent.com/haimingz/tjuthesis/main/tjuthesis.cls
- **[事实] 主文件与 bib**：`\documentclass[master]{tjuthesis}`（`phd` 可选）+ `\addbibresource{references.bib}` + `\printbibliography` → biblatex（后端默认 biber）
- **[事实] 依赖 Perl**：「为了使用自动编译工具 latexmk，Windows 平台需安装 perl」

### 3.15 XJTU-thesis（西安交大，官方）

- 仓库：https://github.com/obster-y/XJTU-thesis （412 star；默认分支 `dev`，`master` 与研究生院版本同步）
- **[事实] README 原文**：
  > 「编译文档请使用 XeLaTeX 引擎。模版提供 latexmkrc 用于自动编译。请将工作目录切换到本项目文件夹下，执行 `latexmk main.tex`」
  > 「在 MacOS 系统下编译会自动识别操作系统，使用 Songti SC 和 STHeiti 字体，但需要启用 `--shell-escape` 编译选项。」
  > 「`xelatex main.tex && xelatex main.tex && biber main && makeglossaries main && xelate main.tex`」
  https://raw.githubusercontent.com/obster-y/XJTU-thesis/master/README.md
- **[事实] latexmkrc 把 pdflatex 也接到 xelatex**：
  ```perl
  $pdf_mode = 1;
  $xelatex = "xelatex -synctex=1 --shell-escape -interaction=nonstopmode %O %S";
  $pdflatex = "xelatex -synctex=1 --shell-escape -interaction=nonstopmode %O %S";
  $biber = "biber --validate-datamodel %O %S";
  ```
  https://raw.githubusercontent.com/obster-y/XJTU-thesis/master/latexmkrc
- **[事实] 主文件**：`% !TEX program = latexmk`；`\documentclass[bachelor,...]{XJTU-thesis}`；`\addbibresource{References/reference.bib}`；含 `makeglossaries` 依赖。
- **[事实] CLI 风险提示**：「如不想更新，可尝试使用 手动编译 XeLaTeX（**可能出错**）」

### 3.16 sysu-thesis（中山大学，社区维护）

- 仓库：https://github.com/sysu-thesis/sysu-thesis
- **[事实] README 原文**：只写「请移步到[项目 wiki](https://gitlab.com/sysu-gitlab/latex-group/thesis/wikis/home)」，「最新代码在 dev 分支上」
- **[事实] README 未提任何引擎 / 编译命令**；仓库主体在 GitLab（`sysu-gitlab/latex-group/thesis`），CI 为 GitLab CI
- **结论：引擎要求「未找到（在 GitHub README 层面）」**。另有 `irenier/sysuthesis`（★CTAN）为可用的替代实现，见下条。

### 3.17 sysuthesis（中山大学，irenier 版）★CTAN

- 仓库：https://github.com/irenier/sysuthesis
- **[事实] README 原文**：
  > 「3. **执行编译**：运行 `latexmk -xelatex`。」（包管理器方式与手动下载方式两处均同）
  https://raw.githubusercontent.com/irenier/sysuthesis/master/README.md
- **[事实] 最小示例**：`\documentclass{sysuthesis}` + `\sysusetup[option]{type=bachelor}`；「为获得最佳兼容性，请确保您的 TeX 发行版不低于 2024 年」

### 3.18 whu-thesis-latex-template（武大，iamywang 版）

- 仓库：https://github.com/iamywang/whu-thesis-latex-template
- **[事实] README 原文**：
  > 「本模板使用 XeLaTeX 进行编译，编译方法为：`xeLaTeX -> bibtex -> xeLaTeX -> xeLaTeX` 或 `latexmk -xelatex`」
  https://raw.githubusercontent.com/iamywang/whu-thesis-latex-template/master/README.md
- **[事实] 主文件**：`\documentclass[AutoFakeBold=2]{WHUPhd}` + `\usepackage{gbt7714}` + `\bibliography{bibs/ref1}` → **bibtex，不是 biber**。https://raw.githubusercontent.com/iamywang/whu-thesis-latex-template/master/thesis.tex

### 3.19 shuthesis（上海大学）

- 仓库：https://github.com/BlueFisher/shuthesis
- **[事实] README 原文**：
  > 「当前版本是 v2.x，由 ahhylau 制作完成，基于 XeCJK 宏包开发，文件使用 UTF-8 编码。」
  > 「本模板在 Windows 10 / Windows 11 和 TeX Live 2021 下开发并测试通过。」
  > 「推荐使用 Visual Studio Code + LaTeX Workshop 作为编辑器。」
  https://raw.githubusercontent.com/BlueFisher/shuthesis/master/README.md
- **[事实] 生成 cls 的脚本**：`.\make-doc.bat` 得到 `shuthesis.cls` 和 `shuthesis.cfg`

### 3.20 明确「未找到」的模板

| 模板 | 状态 |
|---|---|
| PKUthesis（独立于 pkuthss 的项目） | **未找到**。北大只有 pkuthss（CasperVector 维护，含第三方分支 `iofu728/pkuthss`、`doubleZ0108/pkuthss-mac`、`wongsingfo/pku-grad-thesis`、Typst 版 `pku-typst/pkuthss-typst`），未见名为 PKUthesis 的官方仓库 |
| DUT-thesis（大连理工） | **未找到统一官方仓库**。GitHub 上并存 `QuYue/The-XeLaTex-Template-of-Master-Degree-Thesis-of-DUT`（名字含 XeLaTeX，README 404 未取到）、`Saltsmart/DUT-master-thesis`（8 star）、`Anplus/DUT-Bachelor-Thesis-Latex-Template`（1 star）、`SN-WANG/DuTeX`（2026 新建，2 star）、Overleaf「DLUT Master's Thesis 2024版」等，**无权威主干** |
| 天大其他分支 | README 提到「模板修改自 https://github.com/liangzhenduo/TJU-thesis-template」（Overleaf 说明），未独立核实 |

---

## 4. 实测覆盖率（按实际查到的样本算）

**样本口径**：19 个模板（`ucasthesis`、`thuthesis`、`ustcthesis`、`njuthesis`、`SJTUThesis`、`hithesis`、`pkuthss`、`BUCTthesis`、`CUMTthesis`、`nuaathesis`、`whu-thesis`、`seuthesis`、`SEUThesisLatexTemplate`、`tjuthesis`、`XJTU-thesis`、`sysu-thesis`、`irenier/sysuthesis`、`iamywang/whu-thesis-latex-template`、`shuthesis`）。

| 分类 | 数量 | 模板 |
|---|---|---|
| **A. 官方文档/配置明确「XeLaTeX 专有」（pdfLaTeX/ LuaLaTeX 会失败）** | **10** | thuthesis、ustcthesis、njuthesis、BUCTthesis（`\RequireXeTeX` 强校验）、CUMTthesis、nuaathesis、seuthesis、SEUThesisLatexTemplate、tjuthesis、XJTU-thesis |
| **B. 官方配置默认 XeLaTeX，代码层额外支持 LuaLaTeX** | **4** | SJTUThesis、hithesis、whu-thesis、irenier/sysuthesis |
| **C. 官方声明三引擎（含 pdfLaTeX）** | **1** | ucasthesis |
| **D. README 无引擎说明，但脚本/依赖只能 XeLaTeX** | **2** | whu-thesis-latex-template（README 明说 `latexmk -xelatex`）、shuthesis（基于 XeCJK） |
| **E. 未取得权威引擎说明** | **2** | pkuthss（说明在 `pkuthss.pdf` 手册里）、sysu-thesis（说明在 GitLab wiki） |

小计：A 10 + B 4 + C 1 + D 2 + E 2 = **19**

**结论（覆盖率）**：

- **默认 XeLaTeX 的覆盖率 = 19/19 = 100%（实测样本口径）**
  - 判定标准：模板的**官方编译路径或默认配置**是否以 XeLaTeX 为主引擎。命中 = 19/19。
  - 其中 **17/19** 是「官方文档或代码里明确出现 xelatex/XeTeX/XeCJK」（A 10 + B 4 + D 2 + ucasthesis 1）；剩下 2 个（pkuthss、sysu-thesis）只是**官方说明的载体本文没抓到**（PDF 手册 / GitLab wiki），而非「不是 XeLaTeX」。
  - 即使按最保守口径（把两个未取证的模板当作未知并排除），XeLaTeX 覆盖 **17/17 = 100%**。
- **反例（只能用 pdfLaTeX 或只能用 LuaLaTeX 的模板）：0 个。**
  - 没有任何模板要求「只能 pdfLaTeX」。唯一声称支持 pdfLaTeX 的是 ucasthesis，但它的默认脚本与默认参数仍是 **xelatex**，属于「多引擎」而非「pdfLaTeX 专用」。**[推断]** 原因是 2018 年后 ctex/xeCJK/fontspec 已成中文排版事实标准，pdfLaTeX + CJK 老路线只剩历史兼容价值。
  - 没有任何模板要求「只能 LuaLaTeX」。ustcthesis / SJTUThesis 只是「XeLaTeX 或 LuaLaTeX 皆可」，默认仍指向 xelatex。
- **对决策的直接含义**：**不需要为「引擎选错」做自动推断**——默认 XeLaTeX 在这批样本里不会选错。真正的失败面在别处（见下节）。

---

## 5. 比引擎更会翻车的四个点（对 TexPresso 的落地建议）

按「用户下载模板后第一次编译失败」的实际概率排序：

1. **`-shell-escape`（高频，样本中已知 5 个模板必需）**
   已知需要：thuthesis（`latexmkrc` 硬写）、hithesis（README 每条命令都带）、whu-thesis（CI `args`）、tjuthesis（VS Code 工具 args）、XJTU-thesis（`latexmkrc` 硬写）。
   → **[推断]** 建议：latexmk 默认带上 `-shell-escape`（或首编失败后自动重试一次带上）。XeLaTeX + shell-escape 组合在这些模板里是「标配」而非「危险选项」。

2. **biber vs bibtex（双向）**
   - 明确 biber：tjuthesis（biblatex）、XJTU-thesis（`$biber` + makeglossaries）、SJTUThesis / njuthesis（`\printbibliography`）
   - 明确 bibtex：hithesis、nuaathesis、seuthesis、BUCTthesis、whu-thesis-latex-template（`\bibliography{bibs/ref1}`）
   - 两者皆可：ucasthesis（`[bibtex|biber]` 选项）、whu-thesis（`bib-backend=bibtex|biblatex`）
   → **[推断]** 建议：`latexmk` 天然能按 `.bcf`/`.aux` 自动分流，**不要**手工写死 `bibtex`/`biber`；同时确保 TeX Live 侧 biber 已随 latexmk 探到。

3. **额外的索引/术语工具**
   hithesis 需要 `splitindex` + `hithesis.ist`；XJTU-thesis 需要 `makeglossaries`；thuthesis/ustcthesis 的 `latexmkrc` 内置 `makeindex` 自定义依赖（`nlo→nls`、`glo→gls`）。
   → **[推断]** 建议：**优先读取模板自带的 `latexmkrc`**（thuthesis/ustcthesis/XJTU-thesis 都自带），它把这些依赖都定义好了；只有模板没有 `latexmkrc` 时才用应用内置规则。

4. **字体与发行版**
   - 需要 Windows 中易字库：thuthesis（`fontset=windows`）、tjuthesis（`SimSun` 硬编码）、BUCTthesis（封面另需方正 `STXingkai/FZXiaoBiaoSong/FZDaBiaoSong`）
   - 需要自带字体文件：seuthesis（`font/simsun.ttf` 等，`Path = ./font/`）
   - 明确不支持 CTeX 套装：ustcthesis、BUCTthesis
   → **[推断]** 建议：安装引导里加一条「检测到 `fontset=windows` / `SimSun` 时提示需完整 TeX Live + Windows 字体」；并在 README 明确不支持 CTeX 套装（与我们无关，但要能给出提示）。

---

## 6. 方法论备注

- 本文所有引用均来自 `raw.githubusercontent.com` 的仓库原文（README / .dtx / .cls / Makefile / latexmkrc / .github/workflows / main.tex），URL 均在正文给出，可逐条复核。
- `pkuthss` 的引擎说明写在 `pkuthss.pdf` 用户手册内，本文未抓取 PDF 文本，故标为「未找到」，未做任何推测性断言。
- `sysu-thesis` 的编译说明托管在 GitLab wiki（`gitlab.com/sysu-gitlab/latex-group/thesis/wikis/home`），本文未抓取。
- 所有 `[推断]` 标记项均为由代码/配置反推，**不可当作模板作者的官方声明**引用。
