# 模板语料调研（roadmap ⑲）—— 默认 XeLaTeX 的开箱即用率

> 状态：**已完成**，2026-09
> 目的：用**真实模板数据**回答「默认引擎 XeLaTeX 能不能开箱即用」，据此决定 roadmap **②-2 引擎推断**该不该做、做多大。
> 配套：[cn-thesis-template-engines.md](./cn-thesis-template-engines.md)（19 个高校模板的官方引擎要求，逐条原文 URL）
> 脚本：`scripts/tl-compile-matrix.ps1`（可复现）；临时产物在 `test_file/research/`（已 gitignore）
> 标注：**实测** = 本轮本机跑出的可复现结果；**[推断]** = 分析判断；**未验证** = 明确没做

## 0. 结论（三行）

1. **默认 XeLaTeX 选错引擎的案例 = 0/19**（实测）＋ `\RequirePDFTeX` = **0/7374**（静态扫描）＋ 文档调研 **19/19** 要求或兼容 XeLaTeX。
2. **→ ②-2 引擎推断应当砍掉**：它没有可救的案例，属于解决一个不存在的问题。
3. **但调研暴露了 4 类真实失败模式**，它们才是"首次打开即用"的真障碍（见 §4）。

## 1. 方法与两套证据

| 证据 | 语料 | 规模 | 性质 |
|---|---|---|---|
| **A. 静态扫描** | 本机 TeX Live 2026 `texmf-dist/doc/**/*.tex` | 扫描 12,955 份，含 `\documentclass` 的 **7,374** 份 | 全量、可复现、**但语料偏向英文宏包文档** |
| **B. 双引擎编译矩阵** | 真实用户模板（10 中文硕博 + 8 期刊/会议 + 1 日文控制组），取 TeX Live 自带样例 | **19 个 × 2 引擎 = 38 次编译** | 决定性；两种引擎各一份干净副本 |
| **C. 文档调研** | GitHub 上中文高校模板的 README/.dtx/.cls/CI 原文 | 19 个模板 | 官方口径，见[配套报告](./cn-thesis-template-engines.md) |

**A 的语料偏差必须说明**：doc 目录以宏包文档为主（`article` 占 51%），`ctex` 占比被稀释到 0.9%——**它不能代表用户打开的模板**，只能回答"TeX Live 生态里有多少东西依赖 XeLaTeX"。因此结论以 **B** 为准，A 作为旁证。

**B 的方法学局限（诚实标注）**：取的是 TeX Live 的 `doc/**/样例.tex`，**不总是面向用户的模板主文件**。已发现 1 例取样错误：`bithesis-doc.tex` 是 DTX 式**宏包文档**（报 `\DoNotIndex` 未定义），不是学生用的模板——它的失败是取样假象，不计入模板结论。

## 2. 证据 A：静态扫描（7,374 份够 `\documentclass` 的样例）

| 信号 | 命中 | 占比 |
|---|---|---|
| **显式要求 pdfLaTeX（`\RequirePDFTeX`）** | **0** | **0.0%** |
| 需要 XeLaTeX/LuaLaTeX（ctex/xeCJK/fontspec/unicode-math/RequireXe\|Lua） | 709 | 9.6% |
| ├ `fontspec` | 613 | 8.3% |
| ├ `unicode-math` | 112 | 1.5% |
| ├ `ctex` 或 `ctex*` 文档类 | 69 | 0.9% |
| ├ `xeCJK` | 22 | 0.3% |
| ├ `\RequireXeTeX` / `\RequireLuaTeX` | 5 / 2 | 0.1% / 0.03% |
| 无任何引擎绑定信号（引擎无关） | 6,665 | **90.4%** |
| `inputenc` + `fontenc`（pdfLaTeX 习惯写法，XeLaTeX 下 `inputenc` 仅告警） | 990 | 13.4% |
| `iftex` 多引擎分支 | 150 | 2.0% |
| `minted` | 84 | 1.1% |
| `shell-escape` / `\write18` 字样 | 168 | 2.3% |
| `biblatex`（其中 `backend=biber` 277） | 374 | 5.1% |
| `subfiles` | 2 | 0.05% |

**读法**：**没有任何一份样例声明自己只能跑 pdfLaTeX**；需要"非 pdfLaTeX 引擎"的是少数且集中在 fontspec（→ XeLaTeX/LuaLaTeX，正好是我们的默认）。

## 3. 证据 B：双引擎编译矩阵（19 个真实模板）

**总计：XeLaTeX 通过 6/19，pdfLaTeX 通过 5/19**——但这个总数**不是**开箱即用率，因为多数失败与引擎无关。按下表分解才有意义。

### 3.1 按"失败是否与引擎有关"分类

| 类别 | 数量 | 样本 | 证据（.log 首条错误原文） |
|---|---|---|---|
| ✅ **pdflatex 明确拒绝、XeLaTeX 是正确选择** | **4** | shtthesis、bjfuthesis、cquthesis、fduthesis | `Class shtthesis Error: shtthesis only works with LuaLaTeX or XeLaTeX.` ／ `Class bjfuthesis Error: XeLaTeX is required to compile this document.` ／ `Fatal Package fontspec Error: The fontspec package requires either XeTeX or…` ／ `Package unicode-math Error: Cannot be run with pdftex!` |
| ⚪ 两种引擎都通过（引擎无关） | 5 | mcmthesis、aastex701、elsarticle、nature、spie | — |
| ⚪ 两种引擎都失败，**与引擎无关** | 8 | llncs、IEEEtran、acmart、xtuthesis、seuthesis、revtex4-2、bithesis、hithesis、hitszthesis | 缺文件/缺宏包/超时，见 §3.2 |
| 🔴 **需 platex/upTeX（超出"三引擎推断"范围）** | 1 | jsarticle（日文） | `! LaTeX Error: This file needs format 'pLaTeX2e'` |
| 🔴 **"默认 XeLaTeX 选错"的案例** | **0** | — | — |

### 3.2 与引擎无关的失败，根因逐条核实

| 样本 | 根因（已核实） | 归属 |
|---|---|---|
| fduthesis（xe 侧） | `fontspec Error: The font "SourceHanSerifSC-Regular" cannot be found` | **缺中文字体** |
| xtuthesis | `File 'slashbox.sty' not found`（slashbox 已从 TeX Live 移除） | **缺宏包** |
| revtex4-2 | `Undefined control sequence: \AtBeginShipout`（缺 atbegshi） | **缺宏包** |
| acmart | `File 'acm-jdslogo' not found` | **缺图片资源** |
| llncs / IEEEtran | `File '../authorarchive.sty' not found`（样例用相对上级路径） | 取样/路径假象 |
| seuthesis | `File 'seuthesis.cls' not found`（`sample.tex` 是片段，依赖同仓库 cls 编译产物） | 取样假象 |
| bithesis | `\DoNotIndex` 未定义（DTX 文档，非用户模板） | 取样假象 |
| **hithesis / hitszthesis** | **超时（90s）**，但已经产出 PDF：hithesis 596 KB、hitszthesis 5 个变体 PDF（最大 600 KB） | **真·大文档：完整学位论文首编 >90s** |
| cquthesis（xe 侧） | `Package siunitx Error: Invalid number '1.654 x 2.34 x 3.430'` | 模板内容/宏包版本问题 |
| bjfuthesis（xe 侧） | 未取到 `!` 行（latexmk 层面失败） | **未验证** |

## 4. 调研暴露的 4 类真实障碍（比引擎更值得做）

### 4.1 缺宏包 / 缺字体 → 可操作提示（并入 ④ 错误诊断）

实测样本：`slashbox.sty`（已不在发行版）、`atbegshi`、`SourceHanSerifSC-Regular`（思源宋体未安装）、`acm-jdslogo`。
用户当前的体验是：一堆 LaTeX 原始报错 + 一行行号。**"缺 X，请 `tlmgr install Y` 或改字体"是错误诊断里最高价值的模式之一**，且现在有了真实样本可以固化测试用例。

### 4.2 完整学位论文首编 >90s → **默认超时 120s 存在风险**

hithesis 与 hitszthesis 在 90s 超时前**都已产出 PDF**（不是死循环），说明是正常的大文档编译。TexPresso 默认 `timeout_secs = 120`，学位论文首编（多遍 latexmk + splitindex + bibtex）很容易顶到。
→ 建议：把这类模板纳入基准（配合 ③），并复核默认值与超时提示文案。

### 4.3 模板自带 `latexmkrc` 与我们的构建约定存在未验证的交互

实测：`latexmk` **会自动读取项目自带的 `latexmkrc`**（hithesis / hitszthesis 的 stdout 明确出现 `Rc files read: latexmkrc`）——所以"支持模板自带构建配置"**本身不需要我们写代码**。

**但**这两份 rc 里做了这些事：

```perl
$pdf_mode = 1;
$pdflatex = "xelatex -file-line-error --shell-escape -src-specials -synctex=1 -interaction=nonstopmode %O %S;cp %D %R.pdf";
$makeindex = 'internal splitindex';
```

即：**模板自己把 `$pdflatex` 覆写成 xelatex、内建 `--shell-escape`、还自带 `cp` 把产物拷到源目录旁边**。而 TexPresso 固定传 `-outdir=tmp` 并**从 `tmp/<stem>.pdf` 拷贝**到项目根。

→ **未验证**：命令行 `-xelatex`/`-outdir=tmp` 与 rc 内 `$pdf_mode`/`$pdflatex`/`cp` 的**优先级与实际叠加行为**（本轮两个样本都超时，没跑到终态）。这是"模板兼容"里最可能翻车的一处，值得单列验证。

### 4.4 GitHub 源码版模板需要 `.ins`/`.dtx` 处理

配套报告指出：thuthesis 用 `xetex thuthesis.ins`、BUCTthesis/hithesis 用 `xelatex *.ins` 生成 `.cls`；学生若下的是仓库源码而非 Release zip，会先卡在"没有 cls"。**未验证**（本轮测的是 TeX Live 已安装版，cls 已就位）。

## 5. 对 roadmap 的影响

| 项 | 处置 |
|---|---|
| **⑲ 模板样本调研** | ✅ **完成**（本文） |
| **②-2 引擎推断** | ❌ **砍掉**（移入"不做"）：0/19 选错案例 + 0/7374 声明需要 pdfLaTeX；三种引擎推断覆盖不到真正需要别的引擎的场景（如日文 platex） |
| **④ 错误诊断** | ⬆ 扩充：缺宏包/缺字体提示**有实测样本可固化**（§4.1） |
| **② 的其余部分** | "字体探测"仍成立但优先级低于 ④（§4.1 已把它变成错误诊断的一个模式） |
| **新增** | 学位论文首编超时（§4.2）、模板 latexmkrc 交互（§4.3）、`.ins`/`.dtx` 源码版（§4.4） |
| **③ 性能基准** | 基准工程应包含**完整学位论文**档（§4.2 的证据） |

## 6. 未验证 / 待补

1. **bjfuthesis 在 XeLaTeX 下的失败原因**未取到 `!` 行，未定级。
2. **§4.3 的命令行 vs latexmkrc 优先级**未实测（两个样本都超时）。
3. **§4.4 的 `.ins` 流程**未实测（测的是已安装版）。
4. **样本代表性**：19 个仍偏小，且取自 TeX Live `doc/`；若要把"开箱即用率"当指标，应改为**脚本自动生成/下载真实模板集**并纳入 ③ 的基准。
5. **shell-escape**：配套报告称 5 个模板硬写需要，**本轮矩阵没有样本因它失败**（修正归类 bug 后无一例命中 `needs-shell-escape`）——两者不矛盾（模板可在 rc 里内建），但"我们是否需要主动加 `--shell-escape`"仍未验证。
