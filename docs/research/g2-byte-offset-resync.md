# G2「字节偏移重同步」深挖：在不改引擎的前提下能不能拿到，值多少

> 状态：**实测快照（2026-09）**，本机 Windows / TeX Live 2026。工具：新增 `scripts/xdv-report.mjs`（页索引 + 页级差分 + 截断/监视）。
> 上游依据：[texpresso-live-rendering-roadmap.md](../texpresso-live-rendering-roadmap.md) §1 前提表（G2）、§3 阶段 3（增量输出解析）、§6.2（`incdvi_update` / `incdvi_render_page`）。
> 姊妹篇：[dvi-preview-feasibility.md](./dvi-preview-feasibility.md)（换显示格式的结论：不采用）——**本文只回答 G2 这一半：页索引/字节偏移重同步**。
> **结论一句话**：G2 成立，而且**不需要改引擎、不需要额外调用**——`tmp/<stem>.xdv` 本来就在我们每次编译后落盘（实测 4.40MB/186 页），页包自包含、指令长度可算、**半成品可读**（截断到任意位置得到的页与完整文件逐字节相同），全量解析 4.41MB 只要 **2.7ms（1.6 GB/s）**。但它只是"索引"这一半：拿到的是**页边界与页字节**，不是屏幕上的图；渲染那一半的成本见姊妹篇（比 PDF 链路慢 57–68×）。可用价值收束为三条：**编译期页完成度进度**、**页级变更检测**（可作 ⑪ 的判定输入）、以及为任何将来的"页级"能力提供索引。

## 0. TL;DR

| # | 判据 | 实测结果 |
|---|---|---|
| 1 | 指令长度可算（不解释内容即可走完一页） | ✅ 权威表来自 Tectonic 的 **MIT** 实现 `tectonic_xdv`（`SetGlyphs=253` / `SetTextAndGlyphs=254` / `DefineNativeFont=252` / `Noop=138`），并用最小样张**逐字节复核**（详见 §2） |
| 2 | 页索引正确性 | ✅ 与 `pdfinfo` **逐档一致**：thesis 28/28、graphics 8/8、large 121/121、multi 186/186 |
| 3 | **半成品可读**（G2 的核心） | ✅ 截断到 10/33/50/75/90/99.9% → 分别解出 17/61/92/139/166/185 页，且**每页哈希与完整文件逐页相同**（17/17 … 185/185）；末页不完整即丢弃 = 上游 `incdvi` 的回滚规则 |
| 4 | 页包自包含 | ✅ 单页字节切片（只加文件前导）独立解析成 1 页且哈希相同（第 1/94/186 页） |
| 5 | 成本 | ✅ 全量 186 页/4.41MB = **2.7ms（1.6 GB/s）**；按 0.5MB 落盘粒度增量 ≈**0.3ms/次**（对比 dvisvgm 每次预扫 0.7–1.0s） |
| 6 | 编译期可用性 | ✅ 186 页编译 1937ms：**页 1 @896ms（46%）**，之后每 **~110ms** 一批 **~20 页**（flush 粒度 ≈0.5MB） |
| 7 | 页号映射 | ✅ **DVI 页号 = PDF 页号（1:1 同序）**：xdvipdfmx 日志按 1…186 输出；SyncTeX 交叉验证 `chapters/c05.tex:1 → PDF 79`，正是页哈希差分命中的第 79 页 |
| 8 | 页级差分价值 | 🟡 精确但受重排支配：**等长替换 → 1 页变**；**插入一段 → 其后 108 页变**；**只改注释 → 2 页变**（单趟编译 + TOC 收敛，字节确实变了） |
| 9 | 与 SyncTeX 比 | 🟡 互补而非替代：synctex.gz 179KB（解压 1.14MB、gunzip 3.4ms、61 个 `Input:` 源文件记录），给的是**源码↔坐标**；XDV 页哈希给的是**页字节是否变**（本次未做页级对拍） |
| 10 | 为什么 `dvisvgm` 读不了半成品 | ❌ 它**要求 postamble**（`DVI error: missing fill bytes at end of file`）——这是**工具的选择**，不是格式的限制；页包本身自包含（判据 3/4） |

## 1. G2 到底断言了什么

上游文档把它写成一条**前提**（§1 前提表）：

> 增量解析输出 ｜ 输出格式支持**字节偏移重同步**（DVI 靠 BOP/EOP 标记）

阶段 3（§3）给了这套机制的实现要点，可以逐条当作验收判据：

| 上游要求 | 含义 | 本次核对 |
|---|---|---|
| 两阶段结构（`incdvi.c:86`） | 维护 `offset`，**只算指令长度、不解释内容** | ✅ §3.1/§3.4 |
| BOP/EOP → 页索引数组 | 平面数组 `{bop_offset, eop_offset}` 交替，`page_count = page_len/2` | ✅ §3.1 |
| 缓冲区变短 → 丢弃不完整页、`offset` 退回上一页边界 | 回滚/半成品友好 | ✅ §3.2（这条被文档标为"一开始就要写进去"） |
| 页尺寸直接从 BOP 寄存器读出 | 不渲染也知道页面大小 | ✅（BOP 10 个计数可读；页尺寸需 post 的 max 值或首条 down/right，本次只用了计数） |
| 解析成本 ∝ **新增字节**，而非总输出 | "每帧常数级" | ✅ §3.4（1.6 GB/s；0.3ms/0.5MB） |

**一个关键区分**：上游读的是**流**（他们改造了 XeTeX，用 socketpair 把输出推给驱动）。G2 断言的是**格式性质**，与传输方式无关——所以"读文件"这条路必须独立验证。本文就是这份验证。

## 2. 权威依据：XDV 指令表（并逐字节复核）

TeX Live 不提供 xetex 源码，`dvitype` 只认经典 DVI（版本 2，实测对 XDV 直接报 `Bad DVI file: ID byte is 7!`）。可用依据是 **Tectonic 的 `tectonic_xdv`**（MIT，与 XeTeX 同源的 XDV/SPX 解码器）：[docs.rs](https://docs.rs/tectonic_xdv) / [源码](https://raw.githubusercontent.com/tectonic-typesetting/tectonic/master/crates/xdv/src/lib.rs)。其指令表（关键项）：

```
SetChar0..127=0..127   Set1..4=128..131   SetRule=132   PutRule=137   Noop=138
BOP=139   EOP=140   Push=141   Pop=142
Right1..4=143..146  RightByW=147  SetW1..4=148..151  RightByX=152  SetX1..4=153..156
Down1..4=157..160   DownByY=161   SetY1..4=162..165  DownByZ=166   SetZ1..4=167..170
FntNum0..63=171..234   Fnt1..4=235..238   Special1..4=239..242   FntDef1..4=243..246
Pre=247   Post=248   PostPost=249
DefineNativeFont=252   SetGlyphs=253   SetTextAndGlyphs=254
```

- `set_glyphs`：`width(i32) + n(u16) + n×x(i32) + n×y(i32) + n×gid(u16)`
- `define_native_font`：`fontNum(i32) + size(i32,16.16) + flags(u16) + nameLen(u8) + name + faceIndex(u32) + [color|extend|slant|embolden 各 4 字节，按 flags 位]`
- ⚠️ 该表**缺 `put1..4`（133..136）**（上游 parser 会把它们判为非法指令）；我们的实现补上了，以防某些引擎路径发出原子 `put`。

**用最小样张复核**（`\documentclass{article}\begin{document}Hi\end{document}` → `xelatex -no-pdf`，XDV 共 420 字节，1 页，`scripts/xdv-report.mjs --opcodes`）：

- `pre` @0：version=7、num=25400000、den=473628672（=7227×65536 ✓）、comment="XeTeX output 2026.09.13:1506"
- `bop` @44（44 字节载荷：10 个计数 + `prev=0xFFFFFFFF`）
- `define_native_font`：fontNum=40、**size=0x000A0000 → 10.0pt（16.16 定点 ✓）**、flags=0、nameLen=73、name=`c:/texlive/2026/texmf-dist/fonts/opentype/public/lm/lmroman10-regular.otf`（实测长度正好 73 ✓）、faceIndex=0
- 紧接 `fnt_num`(40) → 选中刚定义的字体 ✓
- `set_glyphs`：width=0x000A47AE、n=2、offsets x=0,y=0 与 x=0,y=0x00078000(=7.5pt)、**gid = 62 和 66**，随后正好是 `pop`
- 页尾 `eop`(140) 后是 `post`(248)、`post_post`(249) 与 `0xDF` 填充

→ **"Hi" 两个字形 = gid 62/66，页内只有 1 次 `define_native_font`、字号 10pt**：字段布局与 Tectonic 的表完全吻合，长度计算因此可信。

## 3. 五条判据的实测

所有命令见 §8；夹具：③ 的 `bench/*`（既有 `tmp/*.xdv`）+ 本次临时造的 `_g2-lab`（单文件大档 451KB/121 页；10 章多文件工程 436KB/**186 页**，已删）。

### 3.1 页索引正确性（vs `pdfinfo`）

| 文件 | 解析器页数 | `pdfinfo` 页数 |
|---|---|---|
| `bench/thesis/tmp/main.xdv`（0.25MB） | **28** | 28 |
| `bench/graphics/tmp/main.xdv`（0.08MB） | **8** | 8 |
| `_g2-lab/large.xdv`（4.36MB） | **121** | 121 |
| `_g2-lab/multi/tmp/main.xdv`（4.41MB） | **186** | 186 |

### 3.2 半成品可读（G2 的核心判据）

把完整 XDV 截到不同位置，用同一解析器解析（`--truncate-at`）：

| 截断位置 | 解出的完整页数 | 与完整文件逐页哈希一致 | 末页行为 |
|---|---|---|---|
| 10.0%（0.44MB） | 17 | **17/17** | 第 18 页"尾部被截断→丢弃" |
| 33.0%（1.45MB） | 61 | **61/61** | 同上 |
| 50.0%（2.20MB） | 92 | **92/92** | 同上 |
| 75.0%（3.31MB） | 139 | **139/139** | 同上 |
| 90.0%（3.97MB） | 166 | **166/166** | 同上 |
| 99.9%（4.40MB） | 185 | **185/185** | 同上 |

对照：**`dvisvgm` 对同一份截断文件直接失败**（`DVI error: missing fill bytes at end of file`，`--help` 无任何容错开关）。

### 3.3 页包自包含

把单页字节切出来、只补上文件前导（`pre`）后独立解析：

| 页 | 切片大小 | 解析结果 |
|---|---|---|
| 第 1 页 | 20232B | 1 页，哈希相同 ✅ |
| 第 94 页（中位） | 28026B | 1 页，哈希相同 ✅ |
| 第 186 页（末页） | 11995B | 1 页，哈希相同 ✅ |

→ 页与页之间**没有跨页状态依赖**（字体定义在首次使用的页内出现），所以"只读前 N 页"在格式上成立。

### 3.4 成本（决定它能不能每帧跑）

| 指标 | 实测 |
|---|---|
| 全量解析 4.41MB / 186 页 | **2.72ms**（1.6 GB/s；另一次 4.36MB/121 页 = 8.0ms，含首次 JIT 与页哈希） |
| 按 0.5MB 落盘粒度**增量追加** | ≈**0.3ms/次** |
| 对照：`dvisvgm` 预扫同一文件 | 0.7–1.0s（单页转换的固定成本，见姊妹篇 §4.3） |

### 3.5 编译期可用性（"边编边看"的上界）

`_g2-lab/multi`（186 页）实测：

| 里程碑 | 可用时刻 | 占编译时长 |
|---|---|---|
| 页 1 | **896ms** | **46%** |
| 19 页（10%） | 1010ms | 52% |
| 47 页（25%） | 1228ms | 63% |
| 93 页（50%） | 1444ms | 75% |
| 140 页（75%） | 1778ms | 92% |
| 168 页（90%） | 1889ms | 98% |
| 186 页（100%） | ≈1937ms（编译结束） | 100% |

**落盘是突发的**：10 次 flush，每次 ~0.5MB / ~20 页，间隔 ~110ms（第一次 flush 只带 5 页）。所以"页可用"的粒度是 **~110ms / ~20 页**，而不是逐页平滑。
**含义**：页索引能把"首屏可用的信息"提前到编译的 46%，但**能否提前看到图取决于渲染器**——把 5 页变成 SVG 要 1–5s（姊妹篇 §4.3），所以这条提前量单独兑现不了。

### 3.6 页号映射：DVI 页号 = PDF 页号

- `xdvipdfmx` 转换日志按 `[1][2]…[186]` 顺序输出 ✓
- 交叉验证（用我们自己的 SyncTeX 链路）：`texpresso-cli forward chapters/c05.tex 1` → **PDF page 79**，而"等长替换 c05"实验里页哈希差分命中的**正是第 79 页** ✓✓

→ 页哈希的差分结果可以直接当作"PDF 第 N 页要不要重画"的判据。

## 4. 页级差分能驱动"只重排变更页"吗（roadmap ⑪ 的机制候选）

三个场景（同一 186 页工程，单趟 `xelatex -no-pdf`）：

| 场景 | 变化页数 | 说明 |
|---|---|---|
| ① 等长词替换（`typesetting`→`typesetPing`，不重排） | **1 页**（第 79 页） | 页级增量最理想的情形 |
| ② 章首插入一段（重排其后内容） | **108 页**（79…186） | 重排把编辑点之后全部作废——页级增量收益有限 |
| ③ 只追加一行注释 | **2 页**（第 3、4 页） | 单趟编译 + `\tableofcontents` 收敛：③相对②改的是 TOC 页的页码，**字节确实变了**（差分没有误报） |

**读法**：页哈希差分是**精确**的（字节级，无漏报；③ 说明它连"看起来无关的改动"引发的真实变化也能抓到），但 **LaTeX 的重排特性决定了它多数时候会命中大量页**。因此它适合做 ⑪ 的"**哪些页需要重绘**"的判定输入，而不适合当作"只重绘一页"的承诺。

## 5. 与 SyncTeX 的对比（同一工程）

| 维度 | XDV 页索引 | SyncTeX（`tmp/main.synctex.gz`） |
|---|---|---|
| 体积 | 4.41MB（原始） | **179KB**（解压 1.14MB） |
| 读取成本 | 2.7ms（全量）/ 0.3ms（增量） | gunzip **3.4ms** + 解析 |
| 内容 | **页边界 + 页字节** | **源码文件表（61 个 `Input:` 记录）+ 盒级映射** |
| 能给"哪些页变了" | ✅ 直接（页字节哈希） | 🟡 需按页上下文推导（v1 里页级信息**不是**显式 `Page:` 记录） |
| 能给"源码↔页" | ❌ | ✅（我们已经在用：正向/反向定位） |
| 何时产生 | 每次编译（Quick/Full 都留 `tmp/*.xdv`） | 每次编译（`-synctex=1`，我们已开） |

→ 两者**互补**：要"页是否变"用 XDV 页哈希（便宜、直接），要"从源码找到页/从页找到源码"用 SyncTeX。**本次未做两者对拍**（见 §9）。

## 6. 与 §5 评估、DVI 报告的关系

| 上游前提 | §5.2 原判定 | 本文结论 |
|---|---|---|
| G1 拦截每一次 I/O | ❌ 需改造引擎 | **不需要**：文件本身渐进落盘（§3.5） |
| **G2 字节偏移重同步** | ❌ 当时判"我们产物是 PDF 且不流式" | ✅ **成立且便宜**（本文 §3）；补充：产物其实一直是 **XDV**（`tmp/*.xdv` 每次都在），"不流式"只对 PDF 成立 |
| G3 部分输入状态 | ❌ | 不需要（只读已产出的页） |
| G4 fork 快照 | ❌ Windows 无 fork | 不需要 |

**但整体结论不变**：G2 只解决"**知道第几页在哪、哪页变了**"，不解决"**把页画出来**"。渲染那一半的成本（页级 190–360ms，单页独立进程 0.77–1.03s）由姊妹篇实测，**比 PDF 链路慢 57–68×**。所以：**G2 值得吸收，DVI 渲染不值得做**。

## 7. 建议（按性价比排序）

1. **把 `scripts/xdv-report.mjs` 作为常驻诊断工具**（页索引/页哈希/差分/截断/监视）——本次全部结论都由它产出，后续回归也用它。
2. **编译期"已排版 N 页"进度**（不渲染任何东西）：`tmp/<stem>.xdv` 每次编译都在（实测 4.40MB），按落盘粒度（~110ms）增量追加解析（~0.3ms/次），状态栏显示页数；长编译（真实学位论文 10s+）体验收益最直接。**C1**。
3. **⑪ 用页哈希差分做"需要重绘的页"判定**：每次编译后解析 `tmp/*.xdv`（~3ms），与上一轮逐页比对，给出变化页集合；注意重排场景（§4）——它保证不漏，但可能覆盖大部分页。
4. **仍不做 DVI 渲染**（姊妹篇结论）；若将来真要"边编边出图"，G2 这一半已经现成，缺的是渲染栈。

## 8. 复现方法

```bash
# 页索引摘要（与 pdfinfo 对拍）
node scripts/xdv-report.mjs test_file/projects/bench/thesis/tmp/main.xdv
pdfinfo test_file/projects/bench/thesis/tmp/main.pdf | grep Pages

# 截断（半成品）与逐页哈希一致性
node scripts/xdv-report.mjs <file.xdv> --truncate-at=2200000
node scripts/xdv-report.mjs <a.xdv> --diff <b.xdv>          # 页级差分

# 编译期可用性时间线（另开一个进程跑编译）
node scripts/xdv-report.mjs <file.xdv> --watch=150 --budget=90000

# opcode/字体/specials 普查
node scripts/xdv-report.mjs <file.xdv> --opcodes
```

最小样张（复核指令表用）：

```bash
printf '\\documentclass{article}\n\\begin{document}\nHi\n\\end{document}\n' > min.tex
xelatex -no-pdf -interaction=nonstopmode min.tex   # 注意：stdout 重定向别用 min.log（会与 TeX 自己的日志抢名）
node scripts/xdv-report.mjs min.xdv --pages --opcodes
```

## 9. 未验证与局限

1. **只验证了"长度"，没有验证"值"**：BOP 计数、`set_glyphs` 的 x/y/width、`down/right` 的相对/绝对语义本次只做到"能跳过"，**没有**核对单位与坐标（要做页几何/缩略图才需要）。
2. **SPX（Tectonic 的语义化 XDV）未测**：我们不用它，但同一解析器改 1 个 ID 字节即可支持（未做）。
3. **未做 SyncTeX 页级对拍**：§5 的"互补"是基于结构与成本的判断；要坐实"页哈希差分 vs synctex 派生页变化"的一致性，需要按 ⑪ 的真实用例做一轮对拍。
4. **未在真实学位论文上验证**（hithesis 系被 ㉖ 阻塞）：真实模板的 XDV 会有更多 specials/图片，页包自包含性结论**预期**仍成立（不依赖 specials 内容），但未实测。
5. **解析器是 JS 原型**：产品化要移进 Rust（`texpresso-core`，纯逻辑 + `FileSystem` 注入）并补边界/fuzz 测试（截断点随机、恶意长度字段、`set_glyphs` 计数异常）。本文的 1.6 GB/s 是 Node 的数字，Rust 只会更快。
6. **"增量追加"尚未实现**：本文的成本下界（0.3ms/0.5MB）是按吞吐推算的；真正的追加式解析器需要维护"已扫描 offset + 跨 flush 的半页状态"（上游 `incdvi` 的做法），未实现。
7. **页哈希是字节哈希**：内容相同但位置不同的页会被判为"变了"（这正是重排场景的诚实反映）；若 ⑪ 需要"视觉无变化"的判定，得在渲染层再做比较。
