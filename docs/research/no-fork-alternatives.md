# 没有 `fork()` 时，能不能实现上游阶段 6/7 的功能

> **状态（2026-09 回写）**：**已否决 —— 降级为依赖记录**。`fork` 换来的"从检查点续跑"在 Windows **没有等价机制**（WSL 本机不可用）⇒ 上游阶段 6/7 不做；真正沉淀下来的是本文的成本结论（编辑期单趟里**导言区占 71–89.5%** ⇒ 砍固定开销才是大头）与 §3.1 的 fmt 化实测否决（中文文档禁 fmt）。引用见 [roadmap §5.5/§9](./tex-ide-roadmap-priority.md)。
> 上游依据：[texpresso-live-rendering-roadmap.md](../texpresso-live-rendering-roadmap.md) §1（前提 G4）、§1.3（跨平台提醒）、§阶段 6（seen 水位 + trace）、§阶段 7（fork 快照 + fence）。
> 姊妹篇：[g1-read-interception-feasibility.md](./g1-read-interception-feasibility.md)（读拦截）、[g2-byte-offset-resync.md](./g2-byte-offset-resync.md)（页索引）、[stage3-incremental-output-parsing.md](./stage3-incremental-output-parsing.md)（追加式解析）、[incremental-edit-x-dvi.md](./incremental-edit-x-dvi.md)（已落地的页级复用）。
> **结论一句话**：`fork` 换来的"**从检查点续跑**"在 Windows 上**没有等价机制**（`PssCaptureSnapshot` 只读、CRIU 是 Linux 专属、Cygwin fork 要求 Cygwin 目标引擎），唯一真等价的是 **WSL**——而本机 WSL 不可用（服务拒绝访问）。但本轮实测给出了一个更要紧的事实：**编辑期单趟编译的成本里，导言区占 71%（28 页）～ 89.5%（74 页）**——也就是说 fork 能省的"重排"只占小头，**砍固定开销才是不需要 fork 的大头**。

## 0. TL;DR

| 问题 | 答案 |
|---|---|
| `fork` 在做什么 | 复制**活进程的全部内存状态**（宏、字体表、断行状态、页构建器），于是新进程从"父进程执行到 `trace_len` 时"的状态继续——**不需要重放**（上游 §7.2） |
| Windows 有等价 API 吗 | ❌ 没有。"进程快照"能拿到（`PssCaptureSnapshot`），但它是**只读**的（给调试器/转储用），**不能恢复执行**；CRIU 只在 Linux；Cygwin 的 `fork` 需要**把引擎编成 Cygwin 目标**（TeX Live 的 Windows 版是原生 MinGW） |
| 唯一真等价的路径 | **WSL**（真 `fork`，上游文档自己也这么建议）——代价是引擎搬进 Linux 侧：路径映射、**Windows 字体不可直接使用**、跨文件系统 IO 变慢、用户需预装 WSL。本机 `wsl -l -v` 返回 `E_ACCESS_DENIED`，**未验证** |
| 那有没有"不用 fork 也能拿到同样收益"的路 | ✅ 有几条，但要分清**收益是什么**：fork 省的是"**重排已排版部分**"，而实测显示这部分**只占小头** |
| **编辑期单趟的成本结构**（本轮实测） | `bench/thesis`（28 页）**导言区占 71%**（964/1358ms）；`bench/multifile`（74 页）**占 89.5%**（1347/1505ms）→ 固定开销（引擎启动 + 导言区 + 字体加载）是主项 |
| 推论 | **fork 就算能用也只省小头**；要加速编辑期编译，该做的是**砍固定开销**（导言区 fmt 化 / 字体预热），而这**不需要 fork** |
| 但"砍固定开销"也被关死了吗 | ❌ **基本关死**（§3.1 成本分解）：**引擎启动 946 ms**（进程级，fmt 省不到）+ **ctex/字体 400 ms**（native font，**XeTeX 禁止 dump**）＝ 61%，fmt 一分钱省不到；唯一可 fmt 化的"宏包加载"只有 **150 ms**。而且 `xelatex -ini … \dump` 直接报 `! Can't \dump a format with native fonts or font-mappings.` |
| "把字体挪出 fmt"能绕过吗 | ❌ **不能**（§3.2 实测）：**XeLaTeX 的基座 format 自身**就带 native font，连 `\documentclass{article}` 都 dump 不了。反过来在 **pdflatex** 上 fmt 确实可行且产出逐字节等价，收益与导言区重量成正比（**2.6% @极简 → 12.8% @重导言区**）——但那与我们的主场景（中文 + XeLaTeX）无关 |
| 已落地的不需要 fork 的收益 | 页级复用三件（B/C/A，2026-09）：跳过预览重载、只重绘变化页（render 102→7ms）、跳过 PDF 转换 |

## 1. `fork` 到底换来什么（精确口径）

上游的做法（§阶段 7）：建 `socketpair` → `fork()` → 子进程**完整继承内存状态** → 子进程把新 fd `dup2` 到通道继续执行，父进程 `waitpid` 挂起。配套的三件事：

| 机制 | 位置 | 作用 |
|---|---|---|
| 快照触发 | `engine_tex.c:474 need_snapshot` | 距上一检查点超过 **500ms 逻辑时间**就插一个 |
| **fence 调度** | `engine_tex.c:1066 compute_fences` | 从回滚点往回按指数间隔（50/100/200…ms）最多选 **16** 个候选；程序读到 fence 处**截断这次读**并强制快照 → **越靠近编辑点快照越密** |
| 进程数治理 | `engine_tex.c:270 decimate_processes` | 上限 **32**，超了保留时间上分散的 |

收益的本质是一句话（上游 §7.2 的"等价性"）：

> 子进程的程序状态 = 父进程执行到 `trace_len` 时的状态；而主进程为它记录的"已读历史"也正好是 `trace_len`——**两者天然对齐，不需要重放**。

所以 `fork` 提供的是「**位置无关的续跑**」：编辑点之后的工作全部保留，只从编辑点重排到结尾。

**要替代它，就得替代"保存并恢复进程内存"这件事**——这是硬前提，绕不过去。

## 2. Windows 上"进程快照 + 恢复执行"的可用性盘点

| 路径 | 能否"快照 + 恢复执行" | 判定 |
|---|---|---|
| **`fork()`** | — | Windows 内核不提供 ✗ |
| **`PssCaptureSnapshot`**（Win8.1+，进程快照 API） | ❌ **只读**：它能捕获进程内存/句柄用于调试与转储遍历，**没有"从快照恢复并继续执行"的语义** | ✗ 不可用（这是本次核对的关键一条） |
| **CRIU**（Checkpoint/Restore In Userspace） | ✅ 在 Linux 上真能做 | ✗ 平台专属 |
| **Windows 休眠/进程冻结**（`NtSuspendProcess`、Job 对象） | ❌ 只能**暂停**同一进程，不能"回到 500ms 前的状态" | ✗ 语义不符 |
| **Cygwin / MSYS2 的 `fork` 模拟** | ✅ 能（复制内存 + 重建句柄） | ✗ 要求**引擎本身是 Cygwin 目标**；TeX Live 的 Windows 版是原生 MinGW 构建 |
| **应用级状态序列化**（上游所说"显式序列化程序状态"） | ✅ 理论可行 | ✗ 需要**改引擎**（保存宏表/字体表/断行状态/页构建器/token 栈），上游自己评价"成本高一个数量级，通常不如放弃阶段 7" |
| **WSL** | ✅ 真 `fork` | 🟡 **唯一真等价**，但见下 |

**WSL 的代价**（不是"跑起来就行"）：

- **字体**：cx 中文字体（SimSun/msyh）在 Linux 侧不存在 → 需要把字体复制进 WSL 或改用 Linux 字体 → **同一份文档在两侧的排版结果不同** ✗ 这对"所见即所得"是硬伤；
- **路径**：`/mnt/e/...` 跨文件系统 IO 比原生慢（实测经验值通常 2–5×，本机未测）；
- **分发**：要求用户预装 WSL + 在 Linux 侧装 TeX Live（又是几个 GB）；
- 我们现在的 `SyncTeX`/产物路径/监视全建立在 Windows 路径语义上 → 全链路要分叉。

> 本机实测：`wsl -l -v` → `Wsl/EnumrateDistros/Service/E_ACCESS_DENIED`（服务拒绝访问）。**未验证**，也不建议为它分叉整条链路。

## 3. 关键实测：编辑期单趟的成本结构

这一节是本文最有价值的部分——它改变了"该优化什么"的判断。方法：把源文件的**导言区**（到 `\begin{document}` 为止）单独抽成一个文档，与完整文档各跑 3 次单趟（`xelatex -no-pdf`，预热后取中位）：

| 夹具 | 完整单趟 | 仅导言区 | **导言区占比** |
|---|---|---|---|
| `bench/thesis`（8 章 / 28 页 / ctexbook + amsmath + tikz + bibtex） | 1358 ms（1358/1035/1441） | 964 ms（963/964/1435） | **71%** |
| `bench/multifile`（20 章 / 74 页 / 中文） | 1505 ms（1375/2024/1505） | 1347 ms（1347/1520/1108） | **89.5%** |

读法：

- **74 页中文的正文排版只花了 ~158ms**（1505−1347），每页约 2ms——合成夹具每页内容少，所以这个数字偏乐观；
- 但**固定开销的量级已经明确**：引擎启动 + 导言区 + 字体加载 ≈ **1–1.4 s**，与文档大小几乎无关；
- 这与 ③（性能基准）的结论**互相印证**：当时就发现"瓶颈在小文档固定开销而非大文档"，只是这次量化得更极端。

**对 fork 的推论**（重要）：

> fork 快照能省的，是"**重排编辑点之后的部分**"——而在我们的成本结构里，重排只占 10–29%。
> 也就是说：**即便 fork 能用，它也只能省掉小头**；真正的大头（导言区/启动）它一分钱都省不到。

⚠️ 反向的诚实提醒：真实学位论文的正文远比合成夹具"重"（图表、公式、长表格、双栏），那时正文占比会上升，fork 的相对价值会变大。**这一点本机未测**（hithesis 档被 ㉖ 阻塞）。

### 3.1 "砍固定开销"能不能砍？——成本分解（2026-09 追加实测）

既然 §3 说大头是固定开销，下一步就是把它**拆开**看哪一块能省。方法：逐级加内容的空文档 + 完整文档（`bench/multifile` 的导言区），各跑 3 次取中位：

| 档 | 内容 | 中位耗时 | 增量 |
|---|---|---|---|
| 1 | `\documentclass{article}` + 空文档 | **946 ms** | **引擎启动 + LaTeX 内核 = 946 ms** |
| 2 | `\documentclass{ctexbook}` + 空文档 | 1346 ms | **ctex + 中文字体 = 400 ms** |
| 3 | 完整导言区（+ amsmath/amssymb/tikz）+ 空文档 | 1496 ms | **其余宏包 = 150 ms** |
| 4 | 完整文档（74 页正文） | 2207 ms | **正文排版 = 711 ms** |

**结论（两重否决）**：

1. **fmt 路线不可行**：`xelatex -ini … \dump` 直接报
   `! Can't \dump a format with native fonts or font-mappings.`——而 `fontspec`（以及用它设置中文字体的 ctex/xeCJK）**必然**产生 native font / font-mapping，所以**中文文档无法 fmt 化**（这不是配置问题，是引擎硬限制）。
   > 顺带修正：本文上一版把这条写成 [推断]（"format 可能保存不了字体，收益打折"）——实测比推断更严格：**不是打折，是直接禁止**。
2. **即便 fmt 能用，收益也只有 ~7%**：可 fmt 化的只有"宏包加载"那 **150 ms**；占大头的**引擎启动 946 ms**（进程级，fmt 省不到）与 **ctex/字体 400 ms**（native font，禁止 dump）一共 **1346 ms（61%）** 完全不在它的能力范围内。

⇒ "砍固定开销"这条路在 **stock 引擎 + Windows** 的约束下**基本被关死**。剩下的只能是产品层面的三件事：**减少编译次数**（合并队列/防抖/页级复用，已有）、**让单次编译不可见**（编译不阻塞 UI、只重绘变化页，已有）、以及**给用户的产品级建议**（精简导言区、少用重宏包——上面那 150ms 与用户文档的重宏包直接相关，但那是用户的取舍，不是我们能替他们做的优化）。

### 3.2 "把字体挪出 fmt"能绕过吗——实测（2026-09 追加）

上一版把"用 `mylatexformat` 把字体设置挪到 `\endofdump` 之后"列为"仍未测"。用手工 dump（机制等价）实测了这个想法：

**① XeLaTeX：连最简导言区都 dump 不了**——不是"含字体才不行"

`\documentclass{article}` + 空文档的 fmt 生成**同样**报
`! Can't \dump a format with native fonts or font-mappings.`（日志里 `Beginning to dump on file …` 之后就中止）。
根因：**XeLaTeX 的基座 format 自身**就用 native font 设默认字体（`\font\TU/lmr/m/n/10=[lmroman10-regular]:mapping=tex-text;`），
所以"把**用户导言区里**的字体挪走"根本不解决问题——**用 `xelatex` 作基座就无法 dump 任何 format**。
这与社区结论一致（[dpctex#15](https://github.com/davidcarlisle/dpctex/issues/15) 的答复是 "You really, really don't want to do this"、[TeX.SE: Precompile header with xelatex](https://tex.stackexchange.com/questions/49295/precompile-header-with-xelatex)）。

**② pdflatex：可行，收益与导言区重量成正比**（`bench/large` 125 页英文，各 4 次取中位）

| 导言区 | 完整编译 | fmt 编译 | 节省 | 产出对拍 |
|---|---|---|---|---|
| 极简（仅 `\documentclass{article}`） | 1370 ms | 1335 ms | **35 ms（2.6%）** | 125 页 / **172,953 B**，两版字节数一致 |
| 重（+ amsmath / amssymb / tikz / graphicx / booktabs / hyperref） | 1796 ms | 1567 ms | **229 ms（12.8%）** | 125 页 / **189,058 B**，两版字节数一致 |

⇒ fmt 机制**确实工作**（产出逐字节等价），但：
- 它省的只是"**导言区里的宏包加载**"，收益与导言区重量成正比（2.6% → 12.8%）——与 §3.1 的成本分解吻合；
- **对我们的主场景（中文 + XeLaTeX）收益为 0**：引擎层面就用不了（①），且中文必然依赖 native 字体（ctex/xeCJK）；
- 引擎启动（946 ms）与字体加载（400 ms）依旧不在它的能力范围内。

**仍未测**：pdflatex + CJK 宏包的中文路线（我们已不支持该路径）。
**已补测（2026-09，见 [现代引擎实测](./modern-engines-zh.md) §3.4）**：LuaLaTeX 侧**不受这条引擎硬限制**——`lualatex -ini … \dump` 对 `article` / `ctexart` / `thesis` 完整导言区**都 dump 成功**（3.7 / 11.3 / 12.5 MB，代价是一条非致命的 `lua-uni-algos` Lua 错误）。但**普通 ctex 文档的运行时挂接没做通**（那是 `mylatexformat` 的角色，本机未装；自写 shim 会破坏 ctex 的运行时状态）→ **收益仍未验证**；而且即便挂通，也补不回 LuaLaTeX 相对 XeLaTeX 的 3.2–3.5× 差距（同报告 §2）。

## 4. 不需要 `fork` 的替代路径（逐条判定）

| # | 路径 | 换来什么 | 状态 |
|---|---|---|---|
| a | **页级复用**（页哈希差分 → 跳过预览重载 / 只重绘变化页 / 跳过 PDF 转换） | 省的是**下游**（预览重载、canvas 重绘、`xdvipdfmx` 转换 0.65–0.94s） | ✅ **已落地**（2026-09，B/C/A；见 [报告](./incremental-edit-x-dvi.md) §2.3） |
| b | **Quick 单趟 + 空闲收敛**（㉘） | 编辑期整份重排但走轻路径（省 40% 中位），停手后补一次 Full | ✅ 已落地 |
| c | **导言区 fmt 化**（把导言区 dump 成 `.fmt`，编译时跳过） | 期望直击 §3 的"大头" | ❌ **实测否决（双重）**：① **XeTeX 根本不允许** dump 含 native font / font-mapping 的 format（`! Can't \dump a format with native fonts or font-mappings.`）——而 fontspec/ctex 必然产生它，**中文文档这条路走不通**；② 即便可行，成本分解（§3.1）显示**可 fmt 化的部分只有 ~150ms**（其余是引擎启动与字体，fmt 都省不到） |
| d | **编辑期只编当前章**（`\includeonly{ch}`） | 编辑期只排一章（页多时收益大） | 🟡 **产品取舍**：页码/章节号/目录全部不对（§[增量编辑 × DVI](./incremental-edit-x-dvi.md) §3 实测）→ 只能做"编辑期临时预览"，最终产物仍要全量编译 |
| e | **局部重编译 + 页级拼接** | 期望"只重排改动章、拼回旧 PDF" | ❌ **已证伪**（同上 §3：74→33 页、章节号变"第一章"、页码从 5 起） |
| f | **引擎常驻**（进程不退，反复喂新内容） | 省掉引擎启动 + 导言区（即 §3 的大头） | ❌ **TeX 引擎不提供这种接口**：没有"重跑一份新文档而不退出"的入口；`\dump` 是一次性的；唯一"部分常驻"的形式就是 fmt（c） |

## 5. 结论

1. **`fork` 本身不可替代**：要"从检查点续跑"，在 Windows 上只有 WSL 这一条真等价路径（且它带来字体/路径/分发三处硬代价）。这不是工程取舍，是内核能力缺口。
2. **但 `fork` 的收益里，我们真正需要的部分已经被覆盖**：
   - "编辑 → 看到新图"的跟手感 → 由 ㉘（Quick + 收敛）与 B/C/A（页级复用）承担；
   - 上游 fork 省的是"重排已排版部分"，而实测显示**重排只占编辑期单趟的 10–29%**（合成夹具）。
3. ~~下一块该啃的骨头是"砍固定开销"（导言区 fmt 化）~~ ❌ **§3.1 的实测把这条路关死了**：引擎启动（946 ms，进程级）与 ctex/字体（400 ms，XeTeX 禁止 dump native font）合起来占 **61%**，fmt 一分钱省不到；唯一可 fmt 化的"宏包加载"只有 **150 ms**。⇒ 固定开销**不可压缩**，剩下能做的只有"**减少编译次数**"与"**让单次编译不可见**"（都已在做：合并队列、防抖、页级复用、非阻塞编译），以及给用户的产品级建议。
4. 与 ④/⑤（seen 水位、追加式解析器）的关系不变：它们**依然没有消费方**——因为消费方（上游渲染闭环）我们本来就没有；本轮也没有改变这一点。

## 6. 复现方法

```powershell
# 1) WSL 可用性（本机返回 E_ACCESS_DENIED）
wsl -l -v

# 2) 导言区占比（把导言区抽出来单独编译，与完整文档各跑 3 次取中位）
cd test_file/projects/bench/thesis
$raw = Get-Content main.tex -Raw
$pre = ($raw -split '\\begin\{document\}')[0]
Set-Content _pre.tex -Value ($pre + "`n\begin{document}`n\end{document}`n") -NoNewline -Encoding utf8
$env:SOURCE_DATE_EPOCH="0"
Measure-Command { xelatex -no-pdf -interaction=nonstopmode -output-directory=tmp main.tex }   # 完整单趟
Measure-Command { xelatex -no-pdf -interaction=nonstopmode -output-directory=tmp _pre.tex }   # 仅导言区
Remove-Item _pre.tex

# 3) fmt 路线的前置检查（本机为空 = 未安装）
kpsewhich mylatexformat.sty

# 3b) 直接试 fmt（本机复现出的引擎硬限制）
cd test_file/projects/bench/small-article
$pre = ((Get-Content main.tex -Raw) -split '\\begin\{document\}',2)[0]
Set-Content _p.tex -Value $pre -NoNewline -Encoding utf8
xelatex -ini -jobname=labfmt "&xelatex _p.tex\dump"      # → ! Can't \dump a format with native fonts or font-mappings.
Remove-Item _p.tex,labfmt* -Force

# 4) 成本分解（§3.1）：逐级加内容的空文档 + 完整文档，各跑 3 次取中位
cd test_file/projects/bench/multifile
$pre = ((Get-Content main.tex -Raw) -split '\\begin\{document\}',2)[0]
Set-Content _l1.tex -Value "\documentclass{article}`n\begin{document}`n\end{document}`n" -NoNewline -Encoding utf8
Set-Content _l2.tex -Value "\documentclass[UTF8]{ctexbook}`n\begin{document}`n\end{document}`n" -NoNewline -Encoding utf8
Set-Content _l3.tex -Value ($pre + "`n\begin{document}`n\end{document}`n") -NoNewline -Encoding utf8
foreach ($f in @("_l1.tex","_l2.tex","_l3.tex","main.tex")) {
  Measure-Command { xelatex -no-pdf -interaction=nonstopmode -output-directory=tmp $f }
}
```

## 7. 未验证与局限

1. **WSL 路径完全未验证**（本机服务拒绝访问）：字体、IO、分发三处代价都是**定性判断**，没有数字。
2. **`PssCaptureSnapshot` 的"只读"结论来自 API 语义**（它面向 dump/调试），本文**没有写代码调用验证**。
3. ~~**fmt 化的收益没有实测**~~ ✅ **已实测并否决**（§3.1 / §3.2）：`xelatex -ini … \dump` 报
   `! Can't \dump a format with native fonts or font-mappings.`——而且**连最简导言区也如此**（根因在 XeLaTeX 基座自身，§3.2①）。**"把字体挪出 fmt"的绕过也已实测：无效**（§3.2）。pdflatex 侧则确实可行、产出逐字节等价，收益 2.6%（极简）→ 12.8%（重导言区）——但与中文 + XeLaTeX 的主场景无关。
   **已补测**：LuaLaTeX **不**受这条限制（dump 成功），但运行时挂接未做通、收益未验证——见 [现代引擎实测](./modern-engines-zh.md) §3.4。
4. **成本结构只测了合成夹具**：两档都是"页多、每页内容少"的中文文档；真实学位论文（图表/公式密集）的正文占比会明显更高，fork 的相对价值随之上升——**未测**（hithesis 被 ㉖ 阻塞）。
5. **只测了 `-no-pdf` 单趟**：没测多趟（latexmk 收敛）与 `xdvipdfmx` 的占比（后者已在 DVI 报告里测过：0.65–0.94s/次）。
6. **没有验证"字体在 fmt 里能不能用"**：这决定 c 路线的实际收益，是下一步最该补的实测。
