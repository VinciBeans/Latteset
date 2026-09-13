# G1「拦截每一次 I/O」深挖：当前项目里有没有廉价等价物

> 上游依据：[texpresso-live-rendering-roadmap.md](../texpresso-live-rendering-roadmap.md) §1 前提表（G1）、§阶段 6（seen 水位 + trace + `rollback_add_change`）。
> 姊妹篇：[g2-byte-offset-resync.md](./g2-byte-offset-resync.md)（输出侧的字节偏移重同步）、[dvi-preview-feasibility.md](./dvi-preview-feasibility.md)（渲染那一半）、[stage2-streaming-feasibility.md](./stage2-streaming-feasibility.md)（流式输出，已落地）。
> **结论一句话**：G1 的**原义**（拦到每一次读**且拿到字节偏移**）在当前项目里既**拿不到**、也**无处兑现**——它唯一的消费方是 seen 水位回滚，而那要求"引擎进程活着且状态可回退"（G4 fork 快照，Windows 无 `fork()`）。但 G1 的**信息内容**（读了什么）**已经免费躺在磁盘上**：`.fls`（引擎实际打开的文件，编译中增量可尾随）+ `.fdb_latexmk`（latexmk 依赖图，含 bibtex 步骤与 md5），两者互补、**零新增依赖**。它们能换来两件真事：**精确失效**（现在两个方向都是错的，已实测）与**缺失文件的查找轨迹**（kpathsea 调试位，时间开销不可测）。

## 0. TL;DR

| 问题 | 实测答案 |
|---|---|
| 上游的 G1 是什么 | 「能拦截目标程序的每一次 I/O，**且能拿到字节偏移**」——为阶段 6 的 `trace[]` / 每文件 `seen` 水位服务 |
| 字节偏移拿得到吗 | ❌ 三条现成通道都只给**文件级**；要字节级需改引擎 / API hook / 文件系统驱动（成本量级见 §4，均 [推断]） |
| 拿得到有用吗 | ❌ 上游用它判断"编辑点是否在已读水位之后 → 零重算"；外部进程模型下没有可回退的引擎状态，**信息无消费方**（除非先有 G4，而 Windows 拿不到） |
| "读了什么"拿得到吗 | ✅ **三份现成**：`.fls`（引擎实际打开，**实时可尾随**）、`.fdb_latexmk`（latexmk 依赖图 + mtime/size/**md5** + bibtex 步骤）、`.synctex.gz` 的 Input 表（96 条） |
| 我们已经在付这份成本吗 | ✅ 是：`-recorder` 是 latexmk 默认，每个项目 `tmp/<stem>.fls` 早就在写（19.3 KB / 349 行 @ 真实论文档） |
| 换来什么 | ① **精确失效**：实测现在"改无关 `.tex` 白编译一次"、"改被引用的 `.bib` 完全没反应"；② **缺失诊断**：`KPATHSEA_DEBUG=32` 能报"在哪些目录找过"（含失败尝试）；③ 依赖清单/UI |
| 值多少 | 缺陷修复（.bib/图片该触发不触发）**值得做**（C1）；精确失效优化 **P2**（C2/V2）；字节级拦截 **不做** |

## 1. G1 在上游的确切口径与用途

上游前提表把它写成一句话：**「能拦截目标程序的每一次 I/O，且能拿到字节偏移」**（[文档 §1](../texpresso-live-rendering-roadmap.md)）。它服务的机制只有一个——阶段 6 的 `seen 水位`：

```c
if (e->seen < changed) return;       // 编辑点在读水位之后 → 全部工作有效，不回滚
while (e->seen >= changed) {          // 否则回退到"读该文件之前"
  trace_len--; revert_trace(&self->trace[trace_len]);
}
offset = changed;                     // 新进程只从 changed 处重读
```

`trace[]` 记的是「文件 F 被读到**位置 X**，发生在时刻 T」。**"位置 X" 必须是文件内的字节偏移**，因为判定的语义是"改动落在程序尚未读到的位置之后"。这是"打字不重算前半篇"的实现。

两条隐含前提，决定了它在当前项目能不能落地：

1. **有进程可以回退**：`revert_trace` 回退的是**进程内的读取历史**，`offset = changed` 让**活着的进程**从该处继续——这要求引擎进程在编辑期间一直存在（上游是进程内引擎 + G4 fork 快照）。
2. **信息要被消费**：字节偏移本身不产生任何用户可见效果，它的全部价值都在上面那段判定里。

我们两条都不成立：引擎是**每次编译新起的子进程**（`latexmk`/`xelatex`），编译结束进程即消失（`runner.rs`），没有任何可回退的状态；Windows 也没有 `fork()`（上游自己承认只能走 WSL 或"显式序列化程序状态"，成本高一个数量级）。

> 顺带排除一条近似路线：不改引擎、只靠"局部重排版"逼近"不重算前半篇"——TeX 的排版单位是整份文档（页码、交叉引用、断行互相耦合），`\includeonly` 之类的原生切分不能保证与全量结果一致。**不存在廉价的近似**。

## 2. 现状：零成本的那一半已经在跑了

`-recorder` 是 **latexmk 的默认行为**（`$recorder = 1`），每个编译过的项目 `tmp/` 下都有一份 `<stem>.fls`——这不是"要引入的能力"，而是**已经在付的成本**：

```
$ ls test_file/projects/*/tmp/*.fls | wc -l      # 实测：仓库里 50+ 个项目早就在写
$ node scripts/fls-report.mjs test_file/projects/bench/thesis/tmp/main.fls \
      --fdb test_file/projects/bench/thesis/tmp/main.fdb_latexmk
=== .fls（引擎实际打开）
文件: …/tmp/main.fls   19.3 KB   349 行
INPUT 335（去重 156）  OUTPUT 12
项目内源文件输入（9）: chapters/ch01..08.tex  main.tex
中间产物（11）: tmp/chapters/ch01.aux … 
系统字体（1）: c:/Windows/Fonts/msyh.ttc
TeX Live 文件: 235（已折叠）
```

三条需要点名的性质（都是实测，不是格式文档的复述）：

- **覆盖到字体与中间产物**：`c:/Windows/Fonts/msyh.ttc`（19.7 MB）与 `tmp/*.aux`、`tmp/main.toc`、`tmp/main.bbl` 都在里面——所以它确实是"引擎打开过的文件"，而不是"`.tex` 依赖"。
- **没有失败尝试**：一份**缺包**的探针（`\usepackage{no-such-package-xyz}`）里，`.fls` 含 `no-such-package-xyz` 的行数是 **0**（它只记 open 成功的文件）。想拿"找过哪些路径"要另走 §3.4。
- **`.bib` 不在里面**：bibtex 是**独立进程**，它读的 `.bib` 不写进 xelatex 的 `.fls`（实测 `refs.bib` 出现 **0** 次，而同项目的 `used.tex` 出现 5 次）。这一半在 `.fdb_latexmk` 里（§3.2）。

## 3. 五条候选通道实测

### 3.1 `.fls` —— 引擎实际打开，且**编译中就能读**

用真实论文档（`bench/thesis`，强制 `-g` 重编）逐 250 ms 采样文件大小：

```
287ms … 1870ms   (文件不存在)
2136ms  1945 B   ← 首次出现
2400ms  2500 B
2652ms  5540 B
2905ms  9416 B
3158ms 11335 B
3411ms 13917 B
3664ms 18276 B
3916ms 19809 B   ← 长满
4180ms … 6249ms  19809 B（不再增长；总编译 6281ms）
```

**结论**：`.fls` 是**追加写**的（不是收尾一次性落盘），2.1 s 出现、3.9 s 长满，之后 2.3 s 的剩余时间花在 PDF 转换与收尾上。我们已经有 `FileSystem::read_appended(path, offset)` 这套尾随基础设施（[modules.md](../modules.md) §2.6.1 流式输出），**复用成本≈0**：想要"编译期依赖流"，加一条读任务即可。

一个**实现细节**（实测，容易踩）：**Quick 路径（直调 `xelatex`）不带 `-recorder`，不产 `.fls`**——Quick 编译后 `tmp/main.fls` 的 mtime 保持不变，直到下一次 Full 才更新。要用 `.fls` 做依赖，得给 Quick 的命令行补一个 `-recorder`（成本：一个参数）。

### 3.2 `.fdb_latexmk` —— latexmk 的依赖图（**这才是完整依赖**）

同一个项目的 `tmp/main.fdb_latexmk`（1.7 KB 小项目 / 19.7 KB 真实档）：

```
["bibtex tmp/main"] 1789288944.62291 "tmp/main.aux" "tmp/main.bbl" "tmp/main" 1789288948.63233 0
["xdvipdfmx"] 1789288947.54372 "tmp/main.xdv" "tmp/main.pdf" "tmp/main" 1789288948.63361 0
["xelatex"] 1789288945.24485 "main.tex" "tmp/main.xdv" "main" 1789288948.63302 0
  "./refs.bib" 1789288640 165 275096a9c7b7403c641b19be8bd9f48e ""
  "c:/texlive/2026/texmf-dist/bibtex/bst/base/plain.bst" 1787455757 20613 bd3fbfa9f64872b81ac57a0dd2
```

它给出的是**步骤图 + 每个依赖的 mtime / size / md5**，并且**含 bibtex/biber 这类子进程的输入**——`.fls` 与它的差集恰好是"引擎不读、但产物依赖"的文件：

```
只在 .fdb_latexmk、不在 .fls 的源依赖（1）: refs.bib
```

两个要记住的边界：

- **只有 Full 才会更新**（Quick 直调引擎，latexmk 根本没跑）；首编之前它不存在。
- 依赖集合里**含系统字体**（`msyh.ttc`，19.7 MB 且有 md5）——真拿它做"内容变了没"的判定，不能无脑算 md5，应先用 mtime/size 过滤（或直接把 TeX Live / 系统字体路径排除在触发集合外）。

### 3.3 `TEXMFLOG` —— 成功打开 + **秒级时间戳**

`TEXMFLOG=<file>` 让 kpathsea 把**它打开的每个文件**追加进该文件，每行前缀是 Unix 秒：

```
1789288482 c:/texlive/2026/texmf-config/ls-r
1789288482 c:/texlive/2026/texmf-dist/tex/latex/base/article.cls
1789288482 c:/texlive/2026/texmf-dist/fonts/opentype/public/lm/lmroman10-regular.otf
```

缺包探针下 **13 行里没有一行含缺失包名**——与 `.fls` 一样，**只记成功**。价值在于它是**唯一带时间戳**的通道，且覆盖 kpathsea 层（`ls-r`、`texmf.cnf`、`.fmt`）。秒级粒度对 1–20 s 的编译来说太粗，不构成"读的时机"。

### 3.4 `KPATHSEA_DEBUG` —— **唯一能拿到"失败的查找尝试"的通道**

kpathsea 的调试位把查找过程打到 **stderr**。逐位实测（缺包探针）：

| 值 | 行数 | 含缺失包名 | 含 `returning from generic search` |
|---|---|---|---|
| 1 | 30 | 0 | 0 |
| 2 | 378 | 90 | 0 |
| 4 | 23 | 0 | 0 |
| 8 | 103 | 0 | 0 |
| 16 | 51 | 0 | 0 |
| **32** | **147** | **21** | **17** |
| -1（全开） | 798 | 111 | — |

`KPATHSEA_DEBUG=32` 是"查找轨迹"的最小集合，147 行里能看到完整结论链：

```
kdebug:kpse_find_file: searching for no-such-package-xyz.sty of type tex (from texmf.cnf)
kdebug:start generic search(files=[no-such-package-xyz.sty], must_exist=1, find_all=0, path=.;C:/Users/…/tex/xelatex//;…)
kdebug:returning from generic search([no-such-package-xyz.sty]) =>
```

**开销实测**（`bench/thesis`，交替 5 轮单趟 `xelatex`，绕开 latexmk 的 perl/up-to-date 噪声）：

```
baseline(ms): 2522, 2064, 2032, 2620, 2174   中位 2174
debug32 (ms): 2029, 1686, 2303, 2015, 2801   中位 2029
差异 = -6.7%（落在噪声内，即"测不出开销"）
stderr 体积 = 1,010,462 字节/趟（≈1 MB）
```

**代价不在时间，在体积**。而且有一条**必须注意的耦合**：调试输出走 **stderr**，而 stderr 正是我们流式错误通道的输入之一（[modules.md](../modules.md) §2.6.1）——若常开，1 MB 的 `kdebug:` 文本会灌进流式解析缓冲（需要在 pump 里按行前缀过滤，否则白白扫 1 MB 并可能触发 2 MiB 截断逻辑）。

### 3.5 `.synctex.gz` 的 Input 表 —— 文件表完整，页级归属稀疏

`multifile` 项目：**Input 记录 96 条**（含项目内 5 个章节源文件 + 中间产物 + TeX Live 宏包 + `.fd` 字体定义）。但节点级的输入引用 `x<id>` **很稀疏**：全文件只有 **1 个** input id（87 = `chapters/math.tex`）出现 89 次，其余文件一次都没有。

所以 synctex 能给"这份文档引用了哪些文件"（Input 表），**给不了"每页读了哪些文件"**（节点归属只在跨文件盒子等少数位置记录）。页级归属要用我们**已有**的日志文件栈 + `[N]` 页标记（`core::log_parser::scan` + `PageMarkerScanner`）去近似。

### 3.6 覆盖度对照

| 想知道 | `.fls` | `.fdb_latexmk` | `TEXMFLOG` | `KPATHSEA_DEBUG=32` | synctex Input |
|---|---|---|---|---|---|
| 引擎打开了哪些文件 | ✅ | ✅ | ✅ | ✅ | ✅ |
| 含字体 / 中间产物 | ✅ | ✅ | ✅ | ✅ | 部分 |
| 含 bibtex 等子进程输入 | ❌ | ✅ | ❌ | ❌ | ❌ |
| **失败的查找尝试** | ❌ | ❌ | ❌ | ✅ | ❌ |
| 时间信息 | ❌（仅顺序） | mtime（依赖侧） | ✅ 秒级 | ❌（仅顺序） | ❌ |
| 内容指纹 | ❌ | ✅ md5 | ❌ | ❌ | ❌ |
| 编译中可读 | ✅（实测 §3.1） | 收尾时写 | 追加写 | 追加写（stderr） | ❌（gz） |
| 体积 | 19 KB | 20 KB | 小数百 B | **1 MB/趟** | 数 KB |
| 新增依赖 | 无（默认已开） | 无（默认已开） | 环境变量 | 环境变量 | 无（已开 `-synctex=1`） |
| **文件内字节偏移** | ❌ | ❌ | ❌ | ❌ | ❌ |

## 4. 「字节偏移」这一半：拿不到，也不需要

**拿不到**（四条候选路径，均为 [推断]——本文未实测它们）：

| 路径 | 做法 | 成本量级 | 与本项目的冲突 |
|---|---|---|---|
| 改引擎 | 上游做法：fork XeTeX，在 `open_input`/`getc` 处埋点 | 极高（要跟上游引擎演进） | 与"用 stock 工具链"的现状完全相反 |
| 文件系统驱动 | WinFsp/Dokan 挂只读镜像，记录每次 Read 的 offset/len | 高 | 要在用户机器装内核驱动，与 **ADR-0003**（Windows 首发**不签名**分发）直接冲突 |
| API hook | 注入进程钩 `CreateFileW`/`ReadFile` | 中高 | 需要注入机制、杀软误报、跟引擎版本走 |
| ETW | 内核文件 I/O 事件 | 中 | 需管理员权限 + 解析 `.etl`，且拿不到"文件语义" |

**也不需要**：即便拿到字节偏移，当前架构里**没有消费方**——没有活着的引擎进程、没有可回退状态、没有"从偏移 X 续跑"的能力（§1）。它是**为 fork 快照服务的元数据**，缺了 G4 就是死数据。

## 5. 廉价实现能兑现什么（实测价值）

### 5.1 精确失效：现在两个方向都是错的

`watch.rs:178` 只看扩展名：`if path.extension() == Some("tex") { 触发编译 }`。真机实测（`_g1-lab/watch-probe`：`main.tex` 引用 `used.tex`，另有 `unused.tex` 与 `refs.bib`）：

| 操作 | 实际结果（tmp 产物 mtime） | 期望 | 判定 |
|---|---|---|---|
| 改被引用的 `used.tex` | `out.aux` 重写，触发 Quick 编译 → 成功 | 触发 | ✅ 对 |
| 改**未被引用**的 `unused.tex` | `main.aux` **`16:41:36.918 → 16:41:57.933`**（白编译一次） | 不触发 | ❌ **多编译** |
| 改**被 `\bibliography` 引用**的 `refs.bib` | 所有 tmp 产物 mtime **纹丝不动** | 触发 | ❌ **该触发不触发** |

而 `.fls` + `.fdb_latexmk` 恰好同时给出两个方向的判据：`unused.tex` 在两者里都是 0 次命中（可跳过），`refs.bib` 出现在 `.fdb_latexmk` 的源依赖里（必须触发）。

代价量化：白编译一次 = 该文档一次完整编译（本机小档 ~1 s、`bench/thesis` 6.3 s、400 KB ctexbook 8.2 s、真实学位论文档 >120 s）。

### 5.2 缺失诊断：从"缺 X"到"在哪些路径找过"

现有诊断（`diagnosis.rs`）从 `.log` 拿到 `File 'x.sty' not found`，但说不出"为什么找不到"。`KPATHSEA_DEBUG=32` 的轨迹能补上搜索路径与结论。两个现实约束：**必须预先开启**（事后拿不到，除非重跑一次编译）；**输出走 stderr**（见 §3.4 的耦合）。

### 5.3 依赖清单

`.fls` 的"项目内源文件输入"直接就是"这份文档引用了哪些文件"，可用于 UI（文档依赖视图）或 ⑪ 之类的后续能力。

## 6. 建议（按性价比）

1. **先修缺陷，再谈优化**（C1，不必引入依赖图）：把 `.bib`/图片等**非 `.tex` 输入**纳入触发集合——最省的做法是"项目内非忽略文件变化即触发"，交给 latexmk 自己判定；纯 Quick 路径下这一条会多跑一次，但它修的是"用户改了参考文献毫无反应"。
2. **精确失效（P2，C2/V2）**：用 `.fdb_latexmk` 的**源依赖集合**（排除 `tmp/` 中间产物与 TeX Live/系统字体）过滤 `watch` 事件。必须在实现里守住四条保守原则：
   - `.fdb_latexmk` 不存在（首编前）或过旧 → **照常触发**；
   - 依赖集合里出现但**内容判定失败**（读不到/hash 不了）→ 触发；
   - 只对"**项目内**且被记录为依赖"的文件做跳过判定，其余一律触发；
   - Quick 不更新 fdb → 依赖集合滞后一轮（新增 `\input{新文件}` 后，下一轮 Full 才补上）——期间按"未知即触发"处理。
   工具已就绪：`node scripts/fls-report.mjs <tmp/main.fls> --fdb <tmp/main.fdb_latexmk>`。
3. **缺失诊断增强（P2，C2）**：缺文件时把 `KPATHSEA_DEBUG=32` 的轨迹落一份（过滤 `kdebug:` 行、只留 `searching for` / `returning from`），显示"在 N 个目录找过"。注意：**不能常开**（1 MB/趟 + stderr 耦合），更适合"诊断触发式重跑"或"只对失败编译开一次"。
4. **不做**：字节级 I/O 拦截（§4）；`.fls` 依赖流（除非将来真要"编译期实时依赖图"——§3.1 证明可行，但当前无消费方）。
5. 若要做 1/2，给 Quick 补 `-recorder`（一个参数），让 `.fls` 在两条路径下都存在。

## 7. 复现方法

```bash
# 依赖报告（.fls + .fdb_latexmk，含"只在 fdb、不在 fls"的差集）
node scripts/fls-report.mjs test_file/projects/bench/thesis/tmp/main.fls \
     --fdb test_file/projects/bench/thesis/tmp/main.fdb_latexmk

# .fls 是否编译中增量写入（探针脚本为一次性产物，逻辑见 §3.1）
#   Start-Process latexmk -g …；每 250ms 采样 (Get-Item tmp/main.fls).Length

# 失败的查找尝试（缺包探针）
$env:KPATHSEA_DEBUG = "32"; xelatex -interaction=nonstopmode -output-directory=tmp a-missing-pkg.tex 2> kpse.err

# 只记成功打开 + 秒级时间戳
$env:TEXMFLOG = "$PWD\texmf.log"; xelatex … 
```

真机部分（watch 触发行为）：`VITE_LATTESET_PROJECT=<含被引用/未引用文件的探针项目> npm run tauri dev`，改文件后用 `tmp/*` 的 **mtime 是否变化**判定是否触发（比翻 watch 日志干净）。

## 8. 未验证与局限

1. **字节级四条路径全是 [推断]**：本文只做成本量级判断，未实测 WinFsp/Dokan、Detours、ETW 任何一条。
2. **未测 `.fdb_latexmk` 在编辑期的写入时机**：只确认它"Full 之后是新的"；若要把它当实时依赖源需补测。
3. **未测 biber/biblatex 与 `makeindex`/`glossaries` 等更多子步骤**：本文只覆盖了 bibtex（`.bbl`/`.blg`/`.bst` 均已出现在 fdb）。
4. **未测多趟（latexmk 跑 2–3 遍 xelatex）时 `.fls` 的合并行为**：观察到 latexmk 先写 `tmp/xelatex<pid>.fls` 再 rename 成 `main.fls`（watch 日志里有 `xelatex8136.fls → main.fls` 的 `Name(From)/Name(To)`），但"多趟是否累积"未验证。
5. **精确失效的收益未在真实大文档上量化**：只测了小探针的"白编译一次"；真实收益 = 白编译频率 × 单次编译时长（未统计用户实际编辑模式）。
6. **`KPATHSEA_DEBUG` 的位名未考证**：只按位值实测（32 = 有轨迹、无时间开销），未查 kpathsea `debug.h` 里的符号名。
