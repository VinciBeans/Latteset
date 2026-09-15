# 用 DVI/XDV 代替 PDF 做预览？—— 可行性研究与实测

> 状态：**实测快照（2026-09）**，本机 TeX Live 2026 + Windows。结论一句话：**不采用**——把产物从 PDF 换成 DVI/XDV 后，"编译产物 → 屏幕"这一步会慢**两个数量级**（同一份 121 页文档：`xdvipdfmx` 0.65s vs `dvisvgm` 44.06s），而现有 PDF 链路（0.65s + pdf.js 115ms）已经很快；DVI 唯一真正的额外价值是**编译过程中逐页出现**，但工具链**拒绝读取未写完的 DVI**，要吃下这一点必须自研 DVI 读取器 + 渲染器（上游 TeXpresso 那条路，见 §6）。
> **⚠ 2026-09-15 重新评估：显示格式的结论不变，但"流式出图"这一半的结论已改** —— 见 [§10](#10-重新评估2026-09-15显示格式不变流式出图已经便宜到可落地)。一句话说：**不需要 DVI 显示**，只要「页前缀 + 合成 postamble → 部分 PDF」，而库形态把这一步搬进进程内后每次刷新只要 **~0.1 s**（旧评估用的外部进程是 0.65–0.94 s），旧文留下的两个开口（自研读取器、频繁重载体感）现在都已关闭或已量化。
> 上游依据：[texpresso-live-rendering-roadmap.md](../texpresso-live-rendering-roadmap.md)（上游方案通读）与 [roadmap §5](./tex-ide-roadmap-priority.md)（不采用该路线的评估）；本文只回答"**换格式**能不能更便宜地拿到同样收益"。
> 证据约定：**实测** = 本机跑出的可复现数字（命令见 §8、§10.5）；**[推断]** = 未实测的判断。

## 0. TL;DR

| # | 问题 | 实测/结论 |
|---|---|---|
| 1 | DVI/XDV 比 PDF 小吗？ | **不一定，且常常更大**：451KB 合成文档 121 页 → XDV **4.36MB** vs PDF **152KB**（29×）；反向的例子也有（CJK 短文 XDV 5.8KB vs PDF 39.7KB）。XDV 是**逐字形未压缩流**，PDF 是"压缩内容 + 字体子集"——两者不可比 |
| 2 | 跳过 `xdvipdfmx` 能省多少？ | **0.65s / 2.07s ≈ 31%**（451KB/121 页）。这是 DVI 路线唯一的"确定收益"，但它要求**有别的办法把 XDV 显示出来** |
| 3 | XDV 能直接显示吗？ | 需要一整套**字形级渲染器**：XDV 只有 font id + glyph id，字体靠**文件路径**引用（实测含 `C:/Windows/fonts/simsun.ttc`、`C:/texlive/.../lmroman12-regular.otf`），还带 hyperref/xcolor/pgf 的 **`pdf:` 方言 specials**（`pdf:dest (page.1) [@thispage /XYZ …]`、`color push gray 0`） |
| 4 | 买现成的（`dvisvgm`）行不行？ | **行，但太慢**：121 页 **44.06s**（单文件）/ **51.29s**（逐页 woff2）；单页独立进程 **0.77–1.03s**；批量 ≈ **190–360ms/页**。对比 `xdvipdfmx` **0.65s 全量** ≈ **5ms/页** |
| 5 | 编译中能"边编边看"吗？ | XDV **确实渐进写入**（实测：657ms 已有 122KB，1291ms 达 92%，1349ms 结束）——但 **`dvisvgm` 拒绝截断文件**（`DVI error: missing fill bytes at end of file`，无容错开关）。要利用它，必须自研**容忍缺 postamble** 的读取器 |
| 6 | 那还剩什么可做？ | 三条小项：① 用 BOP 链做**"已排版 N 页"进度信号**（不渲染 DVI，C1）；② 评估 draft 路径**跳过 xdvipdfmx**（省 31%，但需 ⑪ 的"编辑期用上一版 PDF"语义）；③ 真要"边编边出图"就走上游那条（引擎侧流式 + 自研解释器 + MuPDF 级渲染，且 MuPDF 是 **AGPL**，与本项目 MIT 冲突） |

## 1. 为什么会有这个想法（以及它想解决什么）

上游 [let-def/TeXpresso](https://github.com/let-def/TeXpresso) 的实时预览**建立在 DVI 上**：改造过的 XeTeX 把 DVI 流式吐出来，它自己增量解析（`src/dvi/` 解释器 + `incdvi.c` 页索引）、按页重放给 MuPDF 渲染。于是自然会问：**我们不走 fork/改引擎那条路，只把"预览格式"从 PDF 换成 DVI，能不能便宜地拿到"边编边看 + 只更新变化页"？**

本文把这个想法拆成三个可独立证伪的问题：

1. **成本**：DVI/XDV 从产物到屏幕要多少钱？（§4.3）
2. **体积/传输**：DVI/XDV 比 PDF 更适合传输吗？（§4.1）
3. **增量**：编译过程中能不能读到"已完成的新页"？（§4.5）

## 2. 格式与工具链事实

| 项 | 事实（本机实测/文档） |
|---|---|
| DVI vs XDV | `latex`/`lualatex --output-format=dvi` 产出**经典 DVI**（字体靠 TFM/编码解析）；**XeLaTeX 产出 XDV**（DVI 的扩展：`0xfc` 原生字体定义 + `0xfd` 批量字形输出）。两者页结构相同：`bop`(0x8B) + 10×4 字节计数 + **前页偏移指针**，页尾 `eop`(0x8C) |
| 我们的引擎矩阵 | 默认 **XeLaTeX → XDV → xdvipdfmx → PDF**；pdflatex **直接出 PDF**（不经 DVI）；LuaLaTeX 两者都能出（本次未走通，见 §7） |
| 谁产出 XDV | `xelatex -no-pdf`（本次全部实验都走它，与产品同一条引擎路径） |
| 可用工具（本机 TeX Live 2026） | `xdvipdfmx 20260113`、`dvisvgm 3.6`、`dvipng 1.18`、`dvipdfmx`、`pdfinfo`（poppler）；**没有** mutool/ImageMagick |
| XDV 里有什么（实测 `multifile`） | `color push gray 0` / `color pop`、`pdf:pagesize width 614.295pt …`、`pdf:docinfo<</Title()/Subject()/Creator(LaTeX with hyperref)…>>`、`pdf:dest (page.1) [@thispage /XYZ @xpos @ypos null]`、`pdf:majorversion 1`、`pdf:docview<<…>>` —— **这些是"PDF 驱动"的 specials**，由 hyperref/xcolor 针对 xdvipdfmx 发出，渲染器必须认识它们，否则丢链接/颜色/页面尺寸 |
| 字体引用方式（实测 CJK 文档） | XDV 内的原生字体定义带**完整文件路径**：`C:/Windows/fonts/simsun.ttc`、`C:/Windows/fonts/simhei.ttf`、`c:/texlive/2026/texmf-dist/fonts/opentype/public/lm/lmroman12-regular.otf` → 渲染器要能加载**系统字体（含 `.ttc` 集合）**与 TeX Live 字体 |

## 3. 测量环境与样本

- 环境：Windows / 24 线程 / TeX Live 2026（XeTeX 0.999998、xdvipdfmx 20260113、dvisvgm 3.6）。
- 样本（`test_file/projects/` 下的既有夹具，实验副本在临时目录 `_dvi-lab`，已删）：

| 样本 | 源 | 页数 | 特征 |
|---|---|---|---|
| `bench/large` | 451KB | **121**（pdfinfo） | 纯文本长文档，字形最多的极端档 |
| `multifile` | 6 文件 / 7KB | 46（design.md） | ctexbook + `color` + `hyperref` |
| `中文测试工程` | — | 3 | CJK（`.ttc` 系统字体 + OTF 混排） |
| `bench/thesis` / `bench/graphics` | 39KB / 14KB | — | 学位论文（bibtex）/ TikZ 图（pgf specials）：只用了既有 `tmp/*.xdv` 比体积 |

## 4. 实测

### 4.1 产出与体积：XDV 不是"更轻"的格式

| 样本 | XDV | PDF | XDV/PDF |
|---|---|---|---|
| `bench/large`（121 页文本） | **4464 KB** | **152 KB** | **29.4×** |
| `bench/thesis` | 259 KB | 71 KB | 3.6× |
| `bench/graphics`（TikZ） | 79 KB | 15 KB | 5.4× |
| `multifile`（hyperref+color） | 28 KB | 93 KB | **0.30×** |
| `中文测试工程`（CJK 3 页） | **5.8 KB** | **39.7 KB** | **0.15×** |

**读法**：XDV ≈ 每字形几个字节的**未压缩字形流**（4.46MB ÷ 约 60 万字形 ≈ 7 字节/字形），PDF ≈ 压缩后的内容流 + **字体子集**。所以：文本量大的文档 XDV 会大一个数量级；而 CJK 文档反而相反——PDF 必须嵌入 CJK 子集（即使只有几个字也要几万字节），XDV 只放 glyph id 与字体路径。
**含义**：把预览产物换成 XDV 会**把"字体嵌入"的成本从编译期挪到渲染期**（每页都要有字体），这也是 §4.4 里"逐页 SVG 各自带字体"的根因。

### 4.2 跳过 `xdvipdfmx` 的收益（DVI 路线唯一确定的收益）

| 步骤 | 实测（`bench/large`，121 页） |
|---|---|
| `xelatex -no-pdf` → XDV（4.36MB） | **1.42s** |
| `xdvipdfmx` → PDF（152KB） | **0.65s** |
| 合计（= `xelatex` 全量） | **2.07s** |
| 其中 `xdvipdfmx` 占比 | **31%** |

**含义**：如果能不生成 PDF 就显示，编辑期单趟能省约三成；但 §4.3 说明"显示 XDV"这条路本身要贵 68 倍。

### 4.3 从产物到屏幕：DVI 路线慢两个数量级（本次的核心数据）

| 链路 | 实测成本 | 备注 |
|---|---|---|
| **PDF 路线**：`xdvipdfmx` 全量 | **0.65s / 121 页 ≈ 5ms/页** | 之后 pdf.js：真机 46 页/78KB = **115ms**（fetch 11 + parse 39 + render 64，design.md 实测） |
| **SVG 路线**：`dvisvgm -p 1-121 -o full.svg` | **44.06s**（≈364ms/页） | 单文件、默认内嵌 SVG 字体，输出 18.8KB |
| **SVG 路线**：`dvisvgm -p 1-121 -f woff2` 逐页 | **51.29s**（121 文件 / 1.37MB） | 每页**各自重复字体载荷**（≈11KB/页） |
| **SVG 路线**：单页**独立进程** | **0.77–1.03s** | 固定成本主导（XDV 预扫 + FreeType 载字体），与取哪一页无关 |
| **SVG 路线**：20 页批量 | 3.8–4.6s（≈190–230ms/页） | 与 121 页的 364ms/页 差异来自页内字形密度 |

⇒ **XDV→屏幕 比 XDV→PDF 慢 68×**（44.06 / 0.65），比「XDV→PDF→pdf.js 首屏」完整链路（≈0.77s）慢 **57×**。

### 4.4 SVG 产物的体积与字体（若要"只转可见页"）

| 场景 | 默认（内嵌 SVG 字体） | `-f woff2` | `-n`（字形转路径） |
|---|---|---|---|
| `bench/large` 第 1 页 | 18.6 KB | 11.0 KB | **221.9 KB** |
| CJK 第 1 页 | 31.5 KB | 15.7 KB | **64.7 KB** |
| `multifile` 第 1 页 | 13.6 KB | 7.5 KB | — |

两个直接结论：

1. **字体载荷是页级成本的主体**：同一页 `-n`（每个字形展开成 `<path>`）体积是内嵌字体的 6–12 倍；逐页文件各带一份字体（121 页 → 1.37MB），而**单文件 121 页只要 18.8KB**（`<use>` + 一份字体）。
2. **woff2 模式保留 `<text>`**（实测 `multifile` 页 1：`<text>` 9 个，无 `<image>`/`<a>`）→ 文本可选中/可搜索，这是相对 pdf.js canvas 的一个**潜在优势**（但我们的预览目前也不支持选中，不构成理由）。

### 4.5 增量可用性：产物确实"边写边有"，但读不了

**XDV 渐进写入（实测，`bench/large` 全量 1349ms）**：

| 时刻 | 文件大小 | 占最终 |
|---|---|---|
| 142ms | 0 | 0% |
| **657ms** | 122 KB | 2.8% |
| 910ms | 1.57 MB | 36% |
| **1291ms** | 4.23 MB | **92%** |
| 1349ms（编译结束） | 4.46 MB | 100% |

**但工具链拒绝半成品**：把同一文件截到 90% 后 `dvisvgm --page=1-100 partial.xdv` →

```
DVI error: missing fill bytes at end of file at position 4134847
```

`dvisvgm --help` 里**没有任何**容忍截断/缺 postamble 的开关（已核对全文）。
**页计数可以近似**（BOP 链 + 前页指针 + `eop` 校验，纯字节解析，不渲染）：

| 文件 | 我的链式计数 | 真值（dvisvgm/pdfinfo） |
|---|---|---|
| 完整 `large.xdv` | 125 | **121**（+3.3%） |
| 截到 90% 的 `partial.xdv` | 113 | —（可作进度：90% 时约 110 页） |

⇒ **"读文件做增量显示"要想成立，必须自研一个容忍缺 postamble 的 DVI 读取器**（页包 BOP…EOP 是自包含的，理论上可读），而"渲染"这一半仍然存在（§4.3 的成本）。

### 4.6 结论性对照

| 维度 | PDF（现状） | DVI/XDV（提案） |
|---|---|---|
| 编译期成本 | XDV→PDF 0.65s（121 页） | 0（不用转） |
| 首屏成本 | 0.65s + pdf.js 115ms | dvisvgm 单页 0.77–1.03s（**且每页如此**） |
| 全量显示成本 | 0.77s（121 页） | 44–51s（121 页） |
| 体积 | 152KB（含字体子集） | 4.36MB（字体另算） |
| 文本/搜索/链接 | pdf.js 已实现 | 需自研或依赖 dvisvgm 的 `pdf:`/`color` specials 实现 |
| 增量更新 | 整份重载（⑪ 在做"只重排变更页"） | 页级增量**理论可行**，但工具链拒绝半成品、渲染成本高 |
| 引擎改动 | 无 | 无（读文件）或**有**（流式：改 XeTeX） |

## 5. 三个选项的评估

| 选项 | 做法 | 成本 | 收益 | 判定 |
|---|---|---|---|---|
| **A. 自研 DVI/XDV 渲染器** | 解释 opcode → 字形 → 自己画（字体解析、`.ttc`、数学字形、`pdf:`/`color`/`graphic` specials） | **数月**（上游是 `dvi_interp.c` + `dvi_context.c` + MuPDF 渲染栈） | 页级增量 + 省掉 xdvipdfmx 的 31% | ❌ 不做。上游那条路的成本我们已经评估过（roadmap §5.5），且 MuPDF 是 **AGPL** |
| **B. dvisvgm 转 SVG，浏览器显示** | `xelatex -no-pdf` → `dvisvgm` → `<svg>`（WebView2 原生渲染） | 中（新外部依赖 + 页缓存 + 字体载荷管理） | 页级增量、文本可选；**不用改引擎** | ❌ 成本比现状高 57–68×（§4.3），且需处理逐页字体重复（§4.4） |
| **C. 保持 PDF，吸收 DVI 的"信息"** | 不改显示格式，只用 XDV 的**页完成度**做进度；或评估 draft 跳过 xdvipdfmx | **C1** | 长编译有进度条；draft 省 31%（需 ⑪ 语义配合） | ✅ 建议只做这一档（§7） |

## 6. 与 roadmap §5 的关系（哪些前提变了）

roadmap §5.2 用上游方案的四条前提判定"停手"。换格式这件事**改变了其中一条**：

| 前提 | 上游做法 | 换格式后 |
|---|---|---|
| G1 拦截 I/O | 改造 XeTeX，把 DVI 流式吐给前端 | **不需要**——DVI 文件本身就是渐进写的（§4.5 实测）✅ |
| G2 字节偏移重同步 | DVI 的 BOP/EOP 做页索引 | 仍需自研读取器（工具链拒绝半成品）⚠️ |
| G3 部分输入状态 | VFS overlay | 不需要（我们只是显示已产出的页）✅ |
| G4 fork 快照 | Windows 不可用 | 不需要 ✅ |

**但结论不变**：剩下要自己做的部分（G2 的读取器 + 渲染器）正是成本大头，而**收益只有"页码级进度 + 省 0.65s"**。上游之所以值得做，是因为它连"每次按键都重新排版"都省掉了（fork + overlay）；我们只换显示格式，省不掉重排版。

## 7. 建议（可执行的下一步）

1. **不做 DVI/XDV 预览**（本文结论，写入 roadmap §5 吸收清单的"明确不做"）。
2. **做**：长编译的**页完成度进度信号**——纯读 XDV 字节（BOP 链，不渲染），给状态栏/编译中提示「已排版 ≈N 页」；C1，只依赖已经存在的 `tmp/*.xdv`（我们本来就生成它）。⚠️ 精度实测 ±3%（§4.5），且要在**编译中**读一个正在写的文件（需容忍 `EBUSY`/短读）。
3. **评估**：draft（编辑期 Quick）**跳过 xdvipdfmx** 的可行性——收益 31%（0.65s/2.07s，本例），前提是 ⑪ 能做到"编辑期先显示上一版 PDF，空闲收敛再出新的"；否则用户会看到"改了但预览不动"。**这是 ⑪ 的一条设计输入，不是 ⑦ 的**。
4. **不要**为了"边编边出图"去改引擎/自研渲染栈（与 roadmap §5.5 一致；若将来真要，走 Tectonic 并另立 ADR）。

## 8. 复现方法（命令清单）

```bash
# 0) 取一份大文档（本机用 bench/large，451KB/121 页）
cd test_file/projects/_dvi-lab && cp ../bench/large/main.tex large.tex

# 1) 产出与体积
xelatex -no-pdf -interaction=nonstopmode large.tex      # → large.xdv（本次 4.36MB，1.42s）
xdvipdfmx -q large.xdv                                  # → large.pdf（152KB，0.65s）
pdfinfo large.pdf | grep Pages                          # → 121

# 2) DVI→屏幕的成本（对比 1) 的 0.65s）
dvisvgm -p 1 -o p1.svg large.xdv                        # 单页 ≈0.8–1.0s
dvisvgm -p 1-20 -o t20.svg large.xdv                    # 20 页 ≈4.5s
dvisvgm -p 1-121 -o full.svg large.xdv                  # 121 页 44.06s
dvisvgm -p 1-121 -f woff2 -o "px-%p.svg" large.xdv       # 逐页 51.29s / 1.37MB

# 3) 增量可用性：编译中轮询 .xdv 大小（脚本见 §8 附注）
#    结论：657ms 起有内容、1291ms 达 92%；把文件截断后 dvisvgm 报
#    "DVI error: missing fill bytes at end of file"（无容错开关）

# 4) specials/字体普查（在 XDV 原始字节里找可见字符串）
python - <<'PY'   # 或 node；本机用 node 一行脚本
data=open('large.xdv','rb').read()
import re
for s in sorted({m.group().decode('latin1') for m in re.finditer(rb'[ -~]{12,}', data)}):
    if any(k in s for k in ('pdf:','color','graphic','.otf','.ttc','.ttf')): print(s)
PY
```

> 本次用的两个临时脚本（未入库，逻辑已在 §4.5/§4.6 复述）：`xdv-incremental-probe.mjs`（起 `xelatex -no-pdf` 并每 120ms 采样大小/BOP 数）与 `xdv-survey.mjs`（BOP 链页索引 + opcode 直方图 + specials/字体普查）。

## 9. 未验证与局限

1. **未做视觉对拍**：只统计了 SVG 里的元素（`<text>`/`<image>`/`<a>`/色值），没有逐页比对 dvisvgm 输出与 PDF 的渲染保真度（尤其 TikZ/pgf 的 `graphics`/`pdf:` specials、透明、旋转）。
2. **CJK 只测了 3 页短文**：真实中文论文（多字体、`.ttc` 集合、竖排/注音）未测；`.ttc` 在 dvisvgm 下的子字体选择未核。
3. **经典 DVI 未走通**：`lualatex --output-format=dvi` 在该 ctexbook 夹具上失败（`nullfont` 报错、超时已终止），`latex` 一次尝试因日志文件占用中断——两条都不是我们的默认引擎路径，故按"未测"记录，未据此下结论。
4. **"自研渲染器能多快"没有 PoC**：本文给的是**买现成**的成本（dvisvgm 364ms/页）；自研有做快的空间（上游是进程内、只重放变化区间的字节），但功能覆盖（字体/specials）与工期都没有实测，不能拿它当"会更便宜"的依据。
5. **SVG 在 WebView2 的滚动性能未测**：121 页 `<use>` 密集 SVG 的内存/滚动表现未知（若要走选项 B 必须先测这一项）。
6. **进度信号的实现细节未做**：编译中读取正在写的 `.xdv` 会有短读/占用（Windows 上尤其），需要与 runner 的写入时序配合（这属于建议 2 的落地工作）。

## 10. 重新评估（2026-09-15）：显示格式不变，"流式出图"已经便宜到可落地

**为什么重开**：本文原判据里最贵的一环是"把 XDV 变成屏幕上能看到的东西"。此后本仓落地了两件当年没有的东西：① **库形态 runner**（`TectonicLibRunner`，自持 `IoProvider` + 进程内 `XdvipdfmxEngine`，`crates/latteset-tectonic/`）；② **增量 XDV 解析器**（`latteset_core::xdv` + `XdvParser`，追加式解析已按 1122 个采样点 / 71,993 页比对、0 处不一致验证过，见 [g2](./g2-byte-offset-resync.md) / [阶段 3](./stage3-incremental-output-parsing.md)）。于是要重问的是：**"流式出图"是否仍然必须付出"DVI 渲染器"的代价？**

### 10.1 结论

| 问题 | 结论 |
|---|---|
| DVI/XDV **当显示格式**（自研渲染器 / dvisvgm）？ | **维持否决**。理由与 §4.3/§5 完全相同（慢 57–68×；渲染器要数月；MuPDF 是 AGPL），本次测量**没有**触碰这三条 |
| **流式输出**（边编边出图）？ | **结论已改：可行且便宜** —— 不需要 DVI 显示，只用「页前缀 + 合成 postamble → **部分 PDF**」；库形态下每次刷新的转换成本 **~0.1 s**（旧的 0.65–0.94 s 是外部进程形态） |
| 旧文留下的两个开口 | ① "必须自研容忍缺 postamble 的读取器" → **已有**（且现在还能给前缀**补出** postamble，连不容错的外部工具也能吃）；② "频繁重载的体感未测" → **已测**：0 long task、滚动位置零帧偏移（§10.4） |

### 10.2 成本侧实测（本轮重测，release / 本机）

`scripts/xdv-partial.mjs`（本轮入库）把 `[0..第 N 页 eop]` 缝成"看起来完整"的 XDV：`post` 头（改 `last_bop` 与页数 t）→ **字体定义**（`define_native_font` 252 + 经典 `fnt_def` 243–246，放在 post 头**之后**）→ `post_post`（指针 + 1 字节 id=7 + `0xDF` 补齐 4 字节对齐）。样本：`bench/large` 的 XDV（4.59 MB / 125–126 页）。

| 前缀比例 | 前缀页数 | 合成耗时 | 进程内 `XdvipdfmxEngine` | 外部 `xdvipdfmx` | 产出页数 |
|---|---|---|---|---|---|
| 25% | 31 | **9 ms** | **92 ms** | 497 ms | 31 ✓ |
| 50% | 63 | 9 ms | **105 ms** | 513 ms | 63 ✓ |
| 75% | 94 | 11 ms | **121 ms** | 531 ms | 94 ✓ |
| 100% | 125 | 11 ms | **131 ms** | 550 ms | 125 ✓ |

- 页数**全部精确**（`pdfinfo` 核对）；**整份重建**自检：产出 4,594,800 B（与原文件同长）、`xdvipdfmx` exit 0、PDF 161,363 B ≈ 原 161,365 B。
- 成本与前缀大小**近似无关**（固定成本主导：载字体/映射），与旧文的"0.65–0.94 s 与页数几乎无关"一致；**进程内比外部快 4–5×**，且无临时文件、无进程创建。
- 该路径当年只在一次性脚本里做通过（`s2-*.mjs` 未入库），**本轮才成为可复现入口**。

### 10.3 价值侧实测：长编译的窗口有多大

| 场景 | 实测 |
|---|---|
| 冷 Full（125 页 / 462 KB，库形态 **release**） | **7601 ms**：首趟排版 1856 ms → 之后还有 **5736 ms** 才出 PDF |
| Full（同一夹具，GUI **debug**） | 4892 ms（排版 4282 + 转换 610） |
| 对照：28 页带 bib 的编辑触发档（上一项改动后） | 0.5–1.1 s —— **这个量级开流式纯亏** |

⇒ 流式的收益窗口 = **"排版结束到 PDF 出现"的那几秒**，只在长编译（首编、大文档、重度收敛）里存在；阈值应按"预计编译时长"而不是页数来定。

### 10.4 体感侧实测（旧文的开口，本轮补上）

真机（GUI + 125 页预览）：连续发 5 次 `pdf-updated`（每次 `changed_pages` 非空 ⇒ 真的重载）：

- **long task：0 个**（`PerformanceObserver('longtask')` 全窗口无条目）；
- PDF 取回 **2–3 ms/次**（本地资源协议 + 缓存）；
- 重载**期间**的滚动位置：1797 个 rAF 帧（6 s，覆盖一次重载）**全部** `scrollTop = 32208`，零帧偏移 ⇒ 旧文担心的"频繁重载抖动/丢滚动位置"在本机不成立。

> 探针踩坑（自记）：第一版探针装了两个 `setInterval`，其中一个按"最大可滚动 div"选元素，PDF 载入后它选中的是 **Monaco**（`scrollHeight` 16.6 MB）而另一个选的是 `.preview-pane`，两者交错写入同一数组 ⇒ 看上去像"滚动在 0↔32208 之间 13 Hz 翻转"。**换 rAF 单探针后立刻稳定**。凡看到"高频翻转"，先怀疑探针本身。

### 10.5 复现命令

```bash
# 1) 合成前缀（自检：--pages all 应与原文件同长、可被 xdvipdfmx 接受）
node scripts/xdv-partial.mjs test_file/projects/bench/large/tmp/main.xdv --ratio 0.5 --out lab/p_0.5.xdv --json
"C:/texlive/2026/bin/windows/xdvipdfmx.exe" -q -o lab/p_0.5.pdf lab/p_0.5.xdv
pdfinfo lab/p_0.5.pdf | grep Pages          # 期望 63

# 2) 进程内（库形态同一引擎；骨架在 test_file/research-tectonic-lib/engine-spike）
cargo build --release --bin xdv2pdf        # 需 scripts/with-tectonic-lib.ps1 的环境
xdv2pdf --xdv lab/p_0.5.xdv --bundle file:///<本地 bundle> --cache <产品缓存目录> --out lab/p_0.5.inproc.pdf
```

### 10.6 还没解决的（落地前要定的，不是可行性问题）

1. **前缀来源**：库形态 = 内存捕获表（`io.rs` 的逐块钩子已预留、**当前无消费者**）；子进程 XeLaTeX 档 = 读正在写的 `tmp/<stem>.xdv`（需容忍短读/占用）；**Tectonic 子进程档不落 XDV ⇒ 该档没有这条能力**，必须如实登记而不是静默不生效。
2. **触发与预算**：每 N 页 / 每 T ms，且限制总次数；转换要放工作线程、可取消，别和排版抢 CPU（子进程档每次 ~0.55 s，阈值要更保守）。
3. **事件语义**：进度用的部分 PDF **不能**覆盖权威 `pdf-updated`（否则「引用待更新」与页哈希缓存都会被污染）；需要一条独立事件 + 前端"编译中预览"的表现（页数在涨、目录/引用必然是 `??`）。
4. **切页边界**：前缀必须切在完整页（`eop`）上 —— 解析器已保证"半页回滚"，但要在产品路径上再断言一次。
5. **未测**：① 库形态下**内存里**取前缀并转换的端到端时延（本轮是"从文件前缀"量的，差的是 I/O 层那几十微秒）；② 与排版并发时的实际争抢（本轮是串行量的）；③ 用户在"页数逐渐变多"的中间态下的接受度（属产品判断，需要真机走查）。
