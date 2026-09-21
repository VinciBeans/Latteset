# T3 分册：测量与性能回归方案（夹具 · 协议 · 控制组 · 阈值 · 对拍口径）

> 用途：供 **T5** 收录进 `docs/research/tectonic-test-plan.md` 的「3 测量协议与阈值 / 7 复现资产清单」两节，并与 T1（引擎层）、T2（集成层）、T4（分发/缓存/许可）的分册对齐。
> 本册只出**方案**，不改产品代码，也不在编写期跑长测量。
> 证据来源全部为**已有实测/已有产物**：`docs/research/modern-engines-zh.md` §2 / §7.4 / §7.7 / §8、`docs/design.md`（延迟预算与基准表）、`docs/troubleshooting.md`（fontconfig / LuaLaTeX 两条）、`docs/research/incremental-edit-x-dvi.md`、`docs/research/tectonic-integration-plan.md`、`docs/modules.md` §12、`scripts/bench.mjs`、`scripts/bench-single-pass.mjs`、`scripts/check-determinism.mjs`、`test_file/projects/bench/_zhcmp/*.ps1`、`test_file/projects/bench/_zhcmp/_sec8.md`。
> 标记约定：**[实测]** = 有已记录数字或本轮已复核的现场证据；**[推断]** = 由源码/日志机制推出、未实跑；**[未测]** = 无任何证据，必须在 T5 的「未验证清单」里保留。

---

## 0. TL;DR（给 T5 的 9 条）

1. **主判据只有一条**：**「中文文档从源码到能在屏幕上看的那份 PDF」的同批墙钟中位数比值**（Tectonic ÷ XeLaTeX）。绝对毫秒数跨会话可差 ±30%（§7.7 ⑤），只作记录。
2. **协议照抄 §7.7 的五条**：先预热 / 同批交错 / 逐次校验产出 / 带控制组 / 只信同批比值。本册把它补成可执行细节（§2），并补三条本轮新增的：**控制组自身也要预热**、**有效性校验要含 TeX 趟数与缺字数**、**必须先建 `tmp/chapters/` 输出子目录**。
3. **夹具要新增两档**：`multifile`（74 页，74 页档在 Tectonic 上**从未测过**，§8.3 只测了 min-zh/thesis）与 `tiny`（英文，1 页，作用是把"引擎固定开销"与"CJK 字体层"分开）；`thesis-real-hithesis`（真实学位论文模板）**当前受阻，排除在阈值判定外**（§1.3）。
4. **阈值形态**：比值阈值（≤1.15 警告 / ≤1.30 失败）+ 相对回归阈值（同档同比恶化 >20% 判回归）。**不给 Tectonic 承诺"更快"**——已实测是"打平"（0.99×），比它快 1.5× 的是"只排版不出 PDF"那档，不是同一件事。
5. **页级复用命中率**要按 A/B/C 分别定义分母（§4.2）：`H_B` 用"无可见变化的编辑"次数，`H_C` 用 `changed_pages.length / pages`，`H_A` 在 Tectonic 上**没有独立步骤**（PDF 档那一步是同进程内的 `note: Running xdvipdfmx`，没有可跳过的调用边界），判据换成"PDF 形态 vs XDV 形态的耗时差"。**⚠️ PDF 档拿不到 XDV ⇒ `pages == 0`「无法判定」⇒ H_B/H_C 在 PDF 档没有输入**，只能改档测（见第 6 条）。
6. **U1 已定论（T1 复核，2026-09；见 `engine.md` §0 与 §2-E2/E2.8–E2.10、G5 门禁）**：默认 PDF 形态的 Tectonic **不落 `.xdv`**（不是"落得晚"，是"不落"）。
   - 证据：PDF 档 `det1.log`/`det2.log` 全文**无任何 `main.xdv` 字样**；xdv 档 `run-xdv.log`/`detxdv1.log`/`detxdv2.log` 都有 ``note: Writing `main.xdv` (258.5234375 KiB)``。mtime：`main.xdv` 停在 23:44:01（最后一次 xdv 档，与 `d2.xdv` 同刻），随后 23:44:03 的 PDF+synctex 档只更新 `main.pdf/main.log/main.synctex.gz/main.blg`（若写过 xdv，`File::create` 必然把 mtime 推到 23:44:03）。
   - 机制（直接原因，T2 定位 + T1 复核，见 `engine.md` G5/E2.8）：**`driver.rs:1985` 在 xdvipdfmx 跑完后把 XDV 从内存文件表里删掉**——`self.bs.mem.files.borrow_mut().remove(&self.tex_xdv_path)`（PDF 档：`driver.rs:1980-1985`；xdv 档走另一条 `process_to_filesystem` 分支，:2013/:2017）。文件都不在表里，`write_files` 自然看不到它 ⇒ 这比"`access_pattern = WrittenThenRead` 被跳过"更贴，也解释了为什么 `-k` 无效（**不是策略跳过，是对象已被删除**）。
   - **⚠️ 机制与探针两处都更正过（本轮干净目录实测，2026-09-14）**：用**两个只跑过一次的空目录**实测两档——`_zhcmp/tect-kcheck-with-k/`（带 `-k`）与 `tect-kcheck-no-k/`（不带）。`-k` 确实保住了 `main.aux`(443 B)/`main.toc`(1623 B)/`main.bbl`(680 B)/`main.log`，但 **`.xdv` 仍然不落盘**：`with-k/run.log` 里 **`main.xdv` 字样出现 0 次**（`Writing \`main.pdf\`` 1 次、`Running xdvipdfmx` 1 次），`no-k` 目录则只有 `.log/.blg/.pdf`。⇒ **PDF 档不落 `.xdv` 与 `-k` 无关**；"`-k` 会短路跳过分支 ⇒ 假'是'"这条旧写法**作废**。**要拿到 XDV 只能改用 `--outfmt xdv`（或 `--pass tex`）档。**
   - 结论（与 T1 一致）：**"出 PDF + 页级复用"不可兼得**，除非改用 `--pass tex` / `--outfmt xdv` 档，或让 Full 之后显式作废旧 XDV。⇒ §8.4「XDV 就是我们的格式、A/B/C 无需改代码」只在非 PDF 档成立（详见 §4.2、§5.1–5.2、§7 U1/U7）。
7. **首跑成本要拆成两件**：bundle 下载（已实测 51–214 s）与 **format 构建（[未测]，现缓存里那个 `.fmt` 是 23.3 MB，未记过耗时）**。把两件混成一个阈值会让"离线 + 冷 format"这条路径永远测不准。
8. **基线固化**：原始样本留 `test_file/research/`（gitignored，同 `bench-report.json` 律），**精炼表必须进 `docs/`**（否则换机器/清仓即丢）——落 `docs/design.md` 的基准表加一列 + T5 的 test-plan 文档；harness 脚本进 `scripts/`（tracked），夹具副本生成进 `scripts/gen-bench-projects.mjs`。
9. **⚠️ bib 链路是好的，"日志判 bib 成败"才是错的（队长产物级实测 + 本轮独立复核 + T1 字节级反证，P0 判据改写）**：Tectonic 的 `main.bbl` = 680 B、含 9 条 `\bibitem`，且与两条 XeLaTeX 基线（`bench/thesis/tmp/main.bbl`、`_zhcmp/thesis/tmp/main.bbl`）**SHA-256 完全相同**；同次产出的 PDF 28 页、`pdftotext` 文末有 `参考文献` + `[1]`…`[9]` ⇒ **主 bibtex 调用成功、参考文献已排版、产出等价**。那份 537 B 的 `main.blg` **不是丢尾**：Tectonic 的 BibTeX 端口**不实现收尾统计段**（`tectonic.exe` 字节搜索 `You've used`/`wiz_defined`/`built_in function`/`histogram` **全 0 命中**，而对照串 `This is BibTeX`(@13662231)/`Capacity: max_strings`(@13662281) **命中**；源码树同 0 命中；`crates/engine_bibtex/src/lib.rs:548-567` 只写 banner/Capacity/aux 轨迹）⇒ **成功的 `.blg` 也必然止于 `Database file #1: refs.bib`**。⇒ 这是**跨引擎日志契约差异**（确定性），不是"`.blg` 不可信/数据损坏"（偶发）。**两个日志判据都禁用**：`.blg` 摘要行 ⇒ **假阴性**；error 计数 ⇒ **8 假阳性 + 1 假阴性**。**V6b 改为产物级判据**（§2.2）：① `-k` 档读 `.bbl` 的 `\bibitem` 条数 = 预期引用数；② 默认档读产出 PDF 文本的参考文献节；③ 才轮到 `--print`/`--keep-logs` 辅助信号。

---

## 1. 夹具清单与用途

### 1.1 主夹具（P0，全部已在磁盘上）

| 夹具 | 路径 | 规模（本轮复核） | 内容特征 | 覆盖 | 现状 |
|---|---|---|---|---|---|
| `tiny` | `test_file/projects/bench/tiny/main.tex` | **1 页**（`tmp/main.log`: `Output written on tmp/main.xdv (1 page, 912 bytes)`） | `\documentclass{article}` 6 行，**无 CJK、无宏包** | **固定开销地板**（进程启动 + format 加载 + latexmk/perl 开销）；用来把"引擎固定开销"从"CJK 字体层"里分离出来（英文同结构对照见 §7.4 `英文 2341 vs 1878`） | [实测] XeLaTeX：cold 2307 / noop 500 / edit 1625 ms |
| `min-zh` | `_zhcmp/min-zh.tex`（Tectonic 副本 `_zhcmp/tect/min-zh.tex`） | **1 页** | `ctexart` + `amsmath,amssymb,graphicx,booktabs,hyperref`；行内+行间公式、`booktabs` 三线表 | **中文 + 公式 + 表格**（无图、无 bib、无 `\include`） | [实测] §8.3 四档全有；Tectonic 产物 `tect/min-zh.pdf` 50,651 B |
| `thesis` | `test_file/projects/bench/thesis`（Tectonic 副本 `_zhcmp/tect-thesis`） | **28 页**（`bench/thesis/tmp/main.log`: `(28 pages, 265588 bytes)`；XDV 264,728 B / 28 页 §8.4） | `ctexbook` + `amsmath,amssymb,tikz`；8 章 `\include{chapters/chNN}`；每章 `figure`×1 + `equation`×1 + `\cite`×2；`\bibliography{refs}` | **中文 + 公式 + 图 + 多文件 `\include` + bibtex** —— **五个覆盖里唯一同时带 bibtex 的档** | [实测] §8.3；**bib 产出正常且与 XeLaTeX 等价**：`-k` 档 `main.bbl` 680 B / 9 条 `\bibitem`，与两条 XeLaTeX 基线 **SHA-256 相同**；PDF 文本含 `参考文献`+`[1]`…`[9]`（§2.2 V6b） |
| `small-article` | `test_file/projects/bench/small-article` | **3 页** | `article`（英文）+ `graphicx,tikz,booktabs`；`figure`×1 + `table`×1 + `\cite`×2 + `refs.bib` + `\tableofcontents` | **表/图 + bibtex 的小档**；作为"bibtex 不拖慢小文档"的对照 | [实测] XeLaTeX：cold 6476 / edit 2593 ms |
| `multifile` | `test_file/projects/bench/multifile` | **74 页**（`tmp/main.log`: `(74 pages, 650252 bytes)`） | `ctexbook` + `tikz`，20 章 `\include`，**无 bibtex**、章内基本纯文本（`ch03` 30 KB 是唯一的"长章"） | **多文件 `\include` 的规模化**；也是页级复用 C（只改末章）与"重排会命中多少页"的标准夹具 | [实测] XeLaTeX：cold 6465 / edit 2463 ms；**Tectonic 档 [未测]** |
| `graphics` | `test_file/projects/bench/graphics/main.tex` | — | 30 个 `tikzpicture` + 1 长表 | 图/图形重载对引擎的影响 | P1；XeLaTeX cold 2631 ms |
| `large` | `test_file/projects/bench/large/main.tex`（462 KB） | 125 页 XDV（`incremental-edit-x-dvi.md` §2.1） | ~300 页纯文本 | 大文档压力 / 页哈希差分的极端档 | P1（Tectonic 档 [未测]） |

### 1.2 覆盖矩阵（谁覆盖什么，别用一种夹具代替另一种）

| 覆盖面 | `tiny` | `min-zh` | `thesis` | `small-article` | `multifile` | 备注 |
|---|:-:|:-:|:-:|:-:|:-:|---|
| 中文正文（断行/标点挤压/字体回退） | ✗ | ✓ | ✓ | ✗(英文) | ✓ | `tiny`/`small-article` **不能**用来判中文 |
| 公式 | ✗ | ✓ | ✓ | ✗ | ✗ | |
| 表格 | ✗ | ✓ | ✗ | ✓ | ✗ | |
| 图（TikZ / figure 环境） | ✗ | ✗ | ✓ | ✓ | ✗ | `graphics` 是纯图重档（P1） |
| 多文件 `\include` | ✗ | ✗ | ✓(8 章) | ✗ | ✓(20 章/74 页) | |
| bibtex（多趟 + `.bbl`） | ✗ | ✗ | ✓ | ✓ | ✗ | 收敛趟数差异会污染"单趟对拍"，见 §5.2 |
| 多趟收敛（`\tableofcontents`/bibtex 引起） | ✗ | ✗ | ✓ | ✓ | ✓ | `multifile` **也带 `\tableofcontents`** ⇒ Tectonic 同样会自收敛；"单趟对拍"要用 **`--pass tex`** 强制（`-r 0` 只是 1 趟 TeX、仍跑 bibtex），见 §5.2 |

> **判据（T5 收录用）**：报告里每一行必须写清"这行是哪个夹具、多少页、几趟、**bib 是什么结果（V6b 的产物级判据）**"。缺任一项的行不得进入阈值判定。**依据**：§8.4 那行"28 页 vs 27 页 / 文本 15,227 vs 14,913"的差异来自**趟数不同**（Tectonic 3 趟 + 9 次 bibtex，XeLaTeX 档是单趟）——本轮已用 `.bbl` 的 **SHA-256 相同** + PDF 含 `参考文献` 证伪了"bib 失败导致不等价"这一中间猜测，**产出等价性成立**。

### 1.3 `thesis-real-hithesis`（真实学位论文模板）：**当前受阻，不得进判定**

| 项 | 内容 | 出处 |
|---|---|---|
| 夹具 | `test_file/projects/bench/thesis-real-hithesis`（hithesis 样例，自带 `latexmkrc`：`--shell-escape` + 自建 cp） | `test_file/projects/bench/bench-manifest.json`（`external: true`） |
| 阻塞点 | 用产品相同命令重跑，~4 分钟后报 `! I can't write on file 'body/introduction.aux'`（`\include{body/...}` 需要 `tmp/body/` 存在）→ `Emergency stop` → **latexmk/perl 进程挂住（10 分钟零 CPU）** | `docs/design.md` §基准脚本（>240 s 未收敛）、`docs/modules.md` §12.1 #6 |
| 结论 | 阻塞点是**模板自带 latexmkrc × 本产品 `-outdir=tmp` 约定**（roadmap ㉖），**调高超时也编不过** | `docs/modules.md` §12.1 #6 |
| 本方案处置 | ① 在 ㉖ 修好前**排除在阈值判定之外**（保留为"已知不可测档"记录）；② Tectonic 上要单独回答"bundle 里有没有 `hithesis.cls` / 能否按 bundle 解析"——**这是 [未测] 的 P0 前置**（Tectonic 不读本机 TL，见 `tectonic-integration-plan.md` §3 摩擦点 1） | — |
| 可测的那一半 | 表格/公式/图密集度**可以由 `thesis`（28 页，8 章各 1 图 1 式）代表下界**；真实模板是上界，上界在 ㉖/许可审计前**不承诺任何数字** | — |

---

## 2. 每档测量协议

### 2.1 参数（默认值 = 现有脚本已用过的形态）

| 参数 | 默认 | 依据 |
|---|---|---|
| 轮数 `Rounds` | **3** | §8.3 就是"同批交错 3 轮"；`bench.mjs` 也是 3 次取中位 |
| 预热次数 | **每 (夹具 × 引擎) 各 1 次，不计入** | §7.7 ①："每引擎每夹具先跑一次不计入（把宏包/字体/OS 缓存焐热）"；`tectonic-bench.ps1` / `recheck.ps1` 均已如此实现 |
| 交错方式 | **同批轮转**：外层轮次、内层夹具、再内层引擎；且**引擎顺序按轮次奇偶反转**（A/B/B/A） | §7.7 ②"别先跑完 A 再跑 B（机器状态会漂）"；`bench-single-pass.mjs:82` 已用 `i % 2 === 0 ? ["A","B"] : ["B","A"]` |
| 环境固定 | `SOURCE_DATE_EPOCH=0`（全部引擎）、Tectonic 加 `-C`（只有"首跑档"不加） | design.md ㉚（注入后六组合逐字节一致）；`-C` 避免联网抖动进入计时 |
| 输出目录 | **两边都显式指定**：Tectonic `-o <scratch-tect>`、XeLaTeX `-output-directory=<scratch-xe>` | Tectonic 默认 `-o` = **input 所在目录**（`tectonic --help`），不显式指定就与 XeLaTeX 的落点不同，I/O 口径不等价 |
| 超时保护 | 单次 **120 s** 硬超时（与产品默认 `compile.timeout_secs=120` 一致） | `crates/latteset-core/src/settings/model.rs:39`；设计值域 5..=1800 |
| 取数 | **中位数**（只对有效样本）；同时记录 min/max/极差 | `tectonic-bench.ps1` 与 `bench.mjs` 同律 |

### 2.2 逐次有效性校验（**必须见到产出才计入**）

一次运行只有在**全部**满足时才计为有效样本：

| # | 检查 | 命令/依据 | 为什么必须有 |
|---|---|---|---|
| V1 | 进程退出码 = 0 | `$p.ExitCode`（注意 .NET Core 需**第二次 `WaitForExit()`** 才落实，见 `_zhcmp/run4.ps1:31` 的注释） | — |
| V2 | **见到产出**：XeLaTeX/LuaLaTeX 日志含 `Output written`；Tectonic = exit 0 **且** 目标 PDF 存在 | `tectonic-bench.ps1:31` 已实现这一分支 | §7.7 ②：跑 4 s 的"成功"其实是 exit 1（`I can't write on file 'chapters/ch01.aux'` → Emergency stop） |
| V3 | 产出**页数** > 0 且与档案一致 | `pdfinfo <pdf> \| Pages`（本机 `C:\texlive\2026\bin\windows\pdfinfo.exe` 存在，[实测]） | 页数漂移是"编译成功但内容变了"的唯一低成本信号（§8.4 就用 27/28 页做等价性判据） |
| V4 | **中文夹具缺字数 = 0** | Tectonic：`--keep-logs` 后 grep `Missing character`（§8.4 实测 0 条）；XeLaTeX：同法查 `tmp/<stem>.log` | 否则"快"的一种实现就是**少排字**（§7.6 的教训：mylatexformat 让中文文档"编译成功、预览静默缺字"） |
| V5 | **TeX 趟数 + BibTeX 次数/结果已记录** | Tectonic：`--keep-logs`/stdout 里 `note: Running TeX` + `note: Rerunning TeX …` 的条数（T1 复核：**7 份运行日志全部 3 趟**；本轮 `tect-kcheck-with-k/run.log` 复核：`Running TeX` 1 + `Rerunning TeX` 2 = 3）；`Running BibTeX` 次数（**9 次** = `main.aux` + 8 个章级 aux）**且必须连带记 V6b 的结果**；XeLaTeX：单趟=1；latexmk Full：从 `tmp/<stem>.fdb_latexmk` 或日志 `Run number N of rule` 读 | 趟数不同的两个数**不可比**（§8.4 的 28/27 页就是这么来的）。⚠️ **"9 次"要拆成两个互不相关的结论**：**8 次是引擎行为差异**（Tectonic 逐 aux 都跑 bibtex，`driver.rs:1949-1959` 遍历主 aux + 其它所有 `.aux`；latexmk 只对 `main.aux` 跑一次——实证：xe 基线的 `_zhcmp/thesis/tmp/` 与 `bench/thesis/tmp/` 下**只有 `main.blg` 一份**），**第 9 次（主调用）是成功的**（V6b 的 `.bbl` 证据） |
| V6 | 无 `^!` 致命行（XeLaTeX 侧） | `Select-String -Pattern '^!'`（`tectonic-bench.ps1:32`） | 区分"exit 0 但有错误"与真正干净 |
| **V6b** | **bib 链路结果（判据必须落在产物，**不能**落在日志）** | 按优先级：① **`-k` 档读 `.bbl`**：存在 且 `\bibitem` 条数 = 预期引用数（本仓 `thesis` = **9**；`-k` 是本来就该传的，因为 `.aux`/`.toc` 也要）；② **默认档（无 `.bbl`）读产出 PDF 文本**：含参考文献节（`参考文献`/`References`）与 `[n]` 条目；③ 才轮到 `--print`/`--keep-logs` 的辅助信号。⛔ **明令禁用两个日志判据**：**(a) `.blg` 的 `You've used N entries` 摘要行**——**这行永远不会出现**（不是"成功时也可能缺"，而是该端口根本不实现收尾统计段：`tectonic.exe` 字节搜索 + 源码树 + `engine_bibtex/src/lib.rs:548-567` 三重证据）⇒ 用它判 = **假阴性**；**(b) `.blg` 的 error 行计数**——同一夹具上会得到 **8 个假阳性**（8 份章级 `.blg` 各一条 `(There were 2 error messages)`）+ **1 个假阴性**（成功的主调用那份 `main.blg` **0 条 error**） | **判定器本身是被测对象**：拿本夹具的 1 份 537 B + 8 份 306 B `.blg` **加上** 680 B 的 `.bbl` 当回归样本，要求判定器给出"主调用 **success** / 8 次章级 benign"。⚠️ 负例的性质是「**成功且日志形态不同**（无摘要段）⇒ 必须判 **unknown**，不得判 fail」——要防的是**跨引擎日志契约差异**（`engine.md` **G6**），**不是**"数据损坏容错"；写成后者会让 T5 得出错误结论。对应 `engine.md` **E6.1/E6.2/E6.2b** 与 §5 一票否决 |
| **V6c** | **章级 bibtex 空跑要单列（引擎行为差异，非缺陷）** | Tectonic 会为**每个** `.aux` 跑 bibtex；章级 aux 结构上就没有 `\bibdata`/`\bibstyle`（本轮用 **Tectonic 自己的 `-k` 产物**核实：`tect-kcheck-with-k/main.aux` 443 B 有 `\bibdata`×1/`\bibstyle`×1/`\bibcite`×9，而 `chapters/ch01.aux`(1013 B)、`ch03.aux`(1014 B) 三项**全为 0**）⇒ 8 次运行注定报 `I found no \bibdata command` | `run.log` 里这 8 次各对应一条 `warning: errors were issued by BibTeX, but were ignored`（**实测恰好 8 条**，主调用不产生该 warning）⇒ 这条 warning **不能**当作"bib 失败"判据；它只是引擎行为差异 |

> **Tectonic 特有的一条（判据已从 .blg 改到产物，见 V6b）**：BibTeX 的告警**默认被吞**（§8.4 的 `warning: errors were issued by BibTeX, but were ignored`）。本轮干净目录实测把这条告警的来源钉死了：**恰好 8 条，对应 8 次章级空跑；主调用不产生它**。⇒ "被忽略的错误"**不等于** bib 失败，**也不能**当作失败判据。
> **本仓现状（队长产物级实测 + 本轮独立复核，2026-09-14）**：bib 链路**成功且与 XeLaTeX 等价**——`tect-kcheck-with-k/main.bbl` 680 B / 9 条 `\bibitem` / 18 条 `\newblock`，与 `bench/thesis/tmp/main.bbl`、`_zhcmp/thesis/tmp/main.bbl` **三份 SHA-256 完全相同**；同次 `main.pdf` 28 页、`pdftotext` 文末 `参考文献` + `[1]`…`[9]`。而 `main.blg` 537 B **止于 `Database file #1: refs.bib`、0 条 error** ⇒ **是"该端口的 `.blg` 确定地不含收尾统计段"，不是丢尾、也不是运行中止**（同一次运行里 `.bbl` 是完整的；`tectonic.exe` 字节搜索 + 源码树 + `engine_bibtex/src/lib.rs:548-567` 三重证据）。⇒ **"产物不含参考文献 / 与 XeLaTeX 不等价"这一中间结论已撤回**；§8.4 的页数/文本长度差归因回到"**趟数不同**"，**R2 的 0.990 不需要因 bib 复测**。
> **⚠️ 两个轴仍然要分开（V6b 的边界）**：判据换成产物级之后，`output_equivalent` 在本仓 `thesis` 档是**通过**；但记录格式仍保持两栏 `timing_valid` / `output_equivalent(+原因)`，因为"计时有效但产出不等价"是真实存在的组合（例如缺字、页数漂移）。

### 2.3 无效样本怎么记录（不许静默丢弃）

- 每条无效样本写一行，字段固定：`夹具 | 引擎档 | 轮次 | 测得 ms | exit | 首条 ^! 行 | 原因枚举`。
- 原因枚举（固定集合，便于汇总）：`no-output` / `write-fail`（`I can't write on file …`）/ `emergency-stop` / `timeout` / `missing-glyphs` / `pagecount-drift` / `log-locked`（`I can't write on file 'min-zh.log'`，§7.7 ②）/ `crashed`。
- 汇总口径：**每 cell 有效样本 < 3 ⇒ 该 cell 不出结论**，标 `[数据不足]` 并把无效行原样贴出（`tectonic-bench.ps1` 末尾的"=== 无效样本 ===" 已经是这个形态，保留）。
- 另一个必须记录的现场：**极差**。同一 cell 3 轮的 `(max-min)/median > 50%` ⇒ 判"本批不可信"、整批重跑。依据：故障期 `min-zh` 从 1370 → 5723 ms（4.2×，§7.7 ①）；正常期两轮独立运行关键档差异 <5%（design.md §基准脚本复现性）。

### 2.4 夹具侧前置条件（P0，漏了就整批样本静默作废）

- `\include{chapters/chNN}` 的夹具，一旦传了 `-output-directory`/`-o`，**必须先建 `<outdir>/chapters/`**。`recheck.ps1:20-22`、`run4.ps1:15`、`tectonic-bench.ps1:20` 都专门建了它，原因写在注释里：不建 → `I can't write on file 'chapters/ch01.aux'` → Emergency stop（§7.7 ②）。
- 每档测量前后**清理产出**（`tmp/`、PDF、`.pages`、Tectonic 落在源目录的 `.aux/.toc/.blg`），保证下一轮从同一起点开始；`bench.mjs` 的 `cleanBuild()` 是现成实现。
- **编辑类样本必须还原源文件**（`bench.mjs:145-147` 的 `writeFileSync(original)`）。页哈希差分对"内容变没变"是字节级的，没还原的残留会让下一轮的 B/C 判据全错。

---

## 3. 控制组与机器状态检查（**先跑，不合格就不测**）

### 3.1 四组探针

| 组 | 命令 | 基线（正常） | 故障判据 | 依据 |
|---|---|---|---|---|
| **CPU** | `lualatex --luaonly _zhcmp/bench-lua.lua 2000000` | numeric **18–24 ms**、string 92–124、table 230–253（Lua 5.3） | 任一项 > 基线 ×1.5 | §7.7 ④、§7.3 #1 |
| **磁盘 I/O** | `lualatex --luaonly _zhcmp/io-control.lua <tex-tree> <work>`（400 个 `.sty`） | **第 2 遍（热）≥ 120–178 MB/s**；另有"写/读 32 MB"两项 | 热读 < 100 MB/s | §7.7 ①（400 文件热读 178 MB/s、首遍冷读仅 1.6 MB/s —— **冷读慢不是故障**） |
| **字体（fontconfig）** | `fc-list` 计时 + `fc-cache -v` | **连测 3 次，取第 2/3 次**：正常 ~0.44–0.7 s、3640 条字体 | 重复出现 **4.2–4.7 s**，且 `fc-cache -v` 报 `invalid cache file: C:/texlive/<年>/texmf-var/fonts/cache/<hash>-x64.cache-9` | `docs/troubleshooting.md`「XeLaTeX 每次编译都慢 4–5 秒：fontconfig 字体缓存失效」 |
| **进程启动地板** | `cmd /c exit`、`tectonic --version`、`xelatex --version`、`lualatex --version` | `cmd /c exit` ~19 ms、`lualatex --version` ~16 ms、`xelatex --version` 495–690 ms | `xelatex --version` >1.5 s（同组其他正常）⇒ 指向 XeTeX 基座 format / fontconfig | §7.7 ①（"只有 XeTeX 慢"是该故障的特征） |

### 3.2 **控制组自身必须先预热**（本轮新增的方法学结论，[实测]）

`fc-list` 的**第一次调用是冷的**：本轮现场三连测 = **3862 / 545 / 446 / 441 ms**（第 1 次 3.9 s，之后稳定 0.44–0.55 s）。3.9 s 落在"故障区间 4.2–4.7 s"的**边缘**，如果只用单次值判定，会把一台健康机器判成 fontconfig 故障、然后去做一次并不需要的 `fc-cache -f`。

> **规则**：控制组的每一条都**连测 3 次，判据用第 2/3 次**（第 1 次的值单独记录，用于说明"进程/缓存冷启动"）。这条和 §2.1 的"夹具预热"是两件事：**夹具预热焐的是宏包与字体缓存，控制组预热焐的是探针自己的进程与缓存文件**。

**本轮现场抽检（方案编写时，非测量批次）**：

| 探针 | 值 | 读法 |
|---|---|---|
| `fc-list` 计数 | **3640 条** | 与 troubleshooting 记录的 3,640 个系统字体一致 |
| `fc-list` 计时 | 3862（冷）/ 545 / 446 / 441 ms | **正常**（热态 ~0.45 s），非故障 |
| `fc-cache -v` | 579 行输出，**0 条** `invalid`，末尾 `C:\texlive\2026\bin\windows\fc-cache.exe: succeeded`；各目录 `skipping, existing cache is valid` | **缓存有效**，无需 `fc-cache -f` |
| fontconfig 缓存目录 | `C:\texlive\2026\texmf-var\fonts\cache`（cache 文件 mtime **2026-09-13 23:10**）、`%LOCALAPPDATA%\fontconfig\cache` 均存在 | mtime 与 §7.7 ① 记录的"断电后用 `fc-cache -f` 重建"时刻吻合 |
| `cmd /c exit` / `tectonic --version` / `lualatex --version` / `xelatex --version` | 133 / 32 / 123 / **819** ms | `xelatex --version` 偏高但仍在文献区间（495–690）之上一点；按 §3.2 规则**首测不作判据**，正式批次需三连测 |
| Tectonic 缓存 | `%LOCALAPPDATA%\TectonicProject\Tectonic\cache` = **426 文件 / 65,254,100 B（62.2 MB）**，与 §8.2 的"426 文件 / 62 MB"一致 | 缓存完整 |
| Tectonic format 缓存 | `cache\formats\` 仅 1 个文件：`6ffe0558…-latex-33.fmt`，**24,451,466 B（23.3 MB）**，mtime 2026-09-13 23:32:59（晚于 bundle 缓存 23:30:15） | **format 构建的产物在**；**构建耗时从未记录** → §4.3 M2 |

### 3.3 出现冷缓存 / 故障时的处置

| 现场 | 处置 | 依据 |
|---|---|---|
| `fc-list` 重复 4.2–4.7 s + `fc-cache -v` 报 `invalid cache file` | **先 `fc-cache -f` 修复再测**（TeX Live 自带 `fc-cache.exe`） | troubleshooting 同名条目：修好后极简文档 4799→790 ms、`min-zh` 5380–5765→1371 ms、`fc-list` 4.2→0.7 s |
| 沙箱内执行 `fc-cache` 得 `Permission denied`（缓存写在 `C:/texlive/<年>/texmf-var/fonts/cache`，在工作区之外） | **不在沙箱里测性能**：换提权会话跑一次 `fc-cache -f`，或由应用在正常权限下自建；否则"缓存永远建不起来、每次编译都慢" | troubleshooting 同条目的「DSH 沙箱注意」 |
| 同批里只有 XeLaTeX 慢、LuaLaTeX 与 Lua 微基准正常 | 判 **fontconfig 故障**（这是该故障的判别特征），不是"电脑变慢"；当批**所有绝对数字作废**，修好后同批复测 | §7.7 ①（修好后的复测：`thesis` xe-nopdf 1813 vs lua-pdf 5727 = 3.2×，与故障前的 3.5× 一致 ⇒ 结论稳健、数字不稳健） |
| LuaLaTeX 侧报 `no writeable cache path` | 判**环境不合格**（`TEXMFVAR` 必须指向可写目录，只设 `TEXMFCACHE` 不够），换目录再测 | troubleshooting「LuaLaTeX」条目 ① |
| LuaLaTeX 首跑 25.8 s | **不是故障**，是建字体库（23.6 MB / 35 文件）；写进预热说明，别当离群点剔 | troubleshooting 同条目 ②、§2.1 前置坑 2 |
| 有别的重负载在跑（预览/编辑器/agent） | 停掉再测；测量期**不要同时跑真机 GUI** | §7.7 ②（文件锁 `I can't write on file 'min-zh.log'` 就是并发写同一目录的现场） |

---

## 4. 阈值与判据

> 所有阈值都**只在同批有效样本上判**。每条给出「判据 / 依据 / 不可达时怎么办」。

### 4.1 Tectonic 出 PDF 相对 XeLaTeX 基线的可接受区间

**口径**：`R = median(Tectonic 默认形态: 出 PDF) ÷ median(xelatex: 出 PDF)`，同一夹具、同一批、同一轮换顺序。

| # | 夹具 | 判据 | 依据（已有实测） | 不可达时怎么办 | 优先级 |
|---|---|---|---|---|---|
| **R1** | `min-zh`（1 页） | **R ≤ 1.0 优秀 / ≤ 1.5 及格 / > 1.5 失败** | §8.3：Tectonic 1586 vs `xelatex` 2350 ms ⇒ **R = 0.675** | 失败 ⇒ 排查"PDF 后端是否被同步写盘放大"（`-o` 落点）与 format/字体缓存状态；若稳定 >1.5 ⇒ 该档判"Tectonic 在小文档上不划算"，写进 T5 的风险节 | P0 |
| **R2** | `thesis`（28 页，含 bibtex） | **R ≤ 1.0 优秀 / ≤ 1.15 及格 / > 1.30 失败** | §8.3：2515 vs 2539 ms ⇒ **R = 0.990**（打平）。±15% 余量用于吸收夹具漂移（页数曾在 27↔28 之间）与批间噪声。**方向性说明（对 Tectonic 保守，故"打平"更稳）**：Tectonic 那次计时里含 **3 趟 TeX + 9 次 bibtex 子进程**，而分母的裸 `xelatex` 单趟**不跑 bibtex** ⇒ Tectonic 多做了活还打平；产出等价性已由 `.bbl` 逐字节相同 + PDF 含参考文献节证实（§2.2 V6b） | >1.30 ⇒ 先核对趟数与 `Running BibTeX` 次数（§2.2 V5），再看 V6b 的产物判据；若都一致仍 >1.30 ⇒ 判"中文中档退化"，必须在 T5 标为 P0 风险 | P0 |
| **R3** | `multifile`（74 页，多文件） | **先测基线（本轮只记录、不判定）；随后沿用 R2 的 1.15/1.30** | **无 Tectonic 实测**（§8.3 未覆盖此档）；XeLaTeX 侧有 §2 的 2189 ms（`-no-pdf`）与 design.md 的 6465 ms（Full） | 首测出基线后**必须由 T5 复核阈值**（这是"未知档"，不许拿 R2 的数字直接当结论） | P0（新增测量） |
| **R4** | 任意中文档 vs **只排版** | **首选：同引擎同夹具的 `默认档 − xdv 档` = Tectonic 的 PDF 后端成本（干净口径）**；跨引擎的 `Tectonic ÷ (xelatex -no-pdf)` **≤ 2.0** 仅作诊断列 | §8.3：thesis 2515 vs 1681 ⇒ **1.50**；min-zh 1586 vs 1378 ⇒ 1.15。⚠️ 跨引擎这列**含趟数差**（Tectonic 默认 3 趟 + 9 次 BibTeX；`xelatex -no-pdf` 是 1 趟，`det1.log`/T1 复核），所以它不能直接当作"PDF 工作 = 800 ms"的证据——要那 800 ms 就用左边的同引擎差值 | 超限 ⇒ 先按第 2 列的干净口径定位是 PDF 后端还是趟数；本行**不判"不达标"** | P1 |
| **R5** | 任意中文档 vs **LuaLaTeX**（出 PDF） | **R₅ ≤ 1/1.5 = 0.67**（即 Tectonic 至少快 1.5×；现状 2.36×） | §8.3：2515 vs 5932 ms ⇒ **Tectonic 快 2.36×**；§7.4 中文单趟 3.2–3.5× 是 LuaLaTeX 侧的差距 | 掉到 1.5× 以下 ⇒ 说明"用 Tectonic 替 LuaLaTeX 的理由"在速度上不成立，T5 需重述 Tectonic 的价值主张（它原本的理由是"免装 TL + 可复现"） | P1 |

**相对回归阈值（比上表更重要）**：同一夹具、同一引擎，本次中位 vs **上一次固化基线**——**恶化 ≤10% 视为噪声内；>20% 判回归；10–20% 标"观察"并要求第二次复测**。
依据：design.md 复现性"两轮独立运行关键档差异 <5%（tiny edit 1617→1625、multifile edit 2461→2463）"；§7.7 ⑤"跨会话的绝对值可以差 ±30%" ⇒ **10% 是"同机器同会话"的噪声带，不是跨会话的**。

> **基线形态不许中途换**：R1–R5 的分母是**裸 `xelatex` 单趟出 PDF**（= §8.3 的形态）。若 T5 改用 `latexmk -xelatex` Full 当基线（design.md 的 `thesis` cold 9412 ms / edit 3370 ms），分母里多了 perl 启动 + 多趟收敛，**同一台机器上 R 会系统性变小**，两组数字**不可混用**；换基线必须整批重跑并重设阈值。

### 4.2 页级复用命中率（A/B/C 各自的分母）

**前提**（`incremental-edit-x-dvi.md` §2.3 成立条件）：必须是**同一强度、同一收敛状态**的两次编译；页哈希是**字节哈希**；命中率只反映"少做下游重复工作"，**不代表排版变快**。

| 功能点 | 命中率定义 | 判据 | 依据（已有实测） | Tectonic 侧注意 | 优先级 |
|---|---|---|---|---|---|
| **B 跳过预览重载** | `H_B = 命中"pages>0 且 changed_pages 为空"的编译次数 ÷ 无可见变化编辑的编译次数`（编辑形态固定：文件末尾追加 `% comment`） | **H_B = 100%（3/3）**；同时 `reloadKey` 不递增、`skippedReloads` +1 | `incremental-edit-x-dvi.md` §2.1 E1：125 页加一行注释 → **0 页变化**；§2.3 真机 `reloadKey 2→2、skippedReloads 1→2、changedPages []` | **⚠️ 本档只在"产 XDV"的形态下可测**（页哈希来源）。Tectonic **PDF 档不落 `.xdv`**（U1 已定论）⇒ 调度器拿到 `pages == 0`「无法判定」⇒ 前端按保守全量刷新，**H_B 在 PDF 档没有任何输入**。要测 H_B 必须走 `--outfmt xdv` 档；且**页哈希不可跨档混比**（PDF 档 vs xdv 档不是"同一强度、同一收敛状态"，`incremental-edit-x-dvi.md` §2.3 成立条件 1）。T1 已把这三条写成 `engine.md` **E2.11（P0）**：① PDF 档 `page_hashes` 为空；② **不得**把空页集发成"零页变化"信号；③ 拿 xdv 档哈希当基线去比 PDF 档必须判为**不同**（不能因为都是 28 页就认定逐页相同），依据 `xdv.rs:216-233` + `runner.rs:426-438` | P0 |
| **C 只重绘变化页** | `H_C = 1 − changed_pages.length / pages`（编辑形态固定：**末章等长替换**） | `multifile`（74 页）**H_C ≥ 0.90**（即 changed_pages ≤ 7 页）；并断言 `pagesReused` 上升、`render` 下降到 ~10 ms 量级 | §2.3 C 实测：74 页文档改最后一章 → `pagesRendered 7→0、pagesReused 0→7、render 102ms → 7ms、total 183→71ms` | 编辑形态必须固定：G2 实测"等长替换 1 页变 / 章首插入 108 页变 / 只加注释 2 页变"——**换个编辑位置就能把 H_C 从 0.99 打到 0** | P0 |
| **A 跳过 PDF 转换** | `H_A = 命中"跳过 xdvipdfmx 转换"的次数 ÷ 页哈希全同的次数` | XeLaTeX 侧：日志出现 `页哈希与上次逐页相同：跳过 xdvipdfmx 转换`，**H_A = 100%** | §2.3 A：连续 3 次"改注释 → Quick"全部命中；省的是 `xdvipdfmx` **0.65–0.94 s/次**（`incremental-edit-x-dvi.md` §2.2） | **Tectonic 没有可跳过的转换调用边界**：转换是同一进程内的 `note: Running xdvipdfmx`（`det1.log`）。⇒ A 在 Tectonic 上的判据改为**同引擎同夹具的 `默认档 − --outfmt xdv` 差值 = 它的 PDF 后端成本**（**不要**用 §8.3 的"2515 vs 1681 = 800 ms"当这个差值——那两档趟数不同，见 §4.1 R4 的注） | P1 |

### 4.3 首跑成本：bundle 下载与 format 构建必须分开计

| 指标 | 判据 | 依据 | 不可达时怎么办 | 优先级 |
|---|---|---|---|---|
| **M1 联网懒下载首跑**（空缓存） | `min-zh` **≤ 240 s**（实测 214 s，余量 12%）；`thesis` **≤ 90 s**（实测 51 s）；`hello` ≤ 210 s（实测 189 s）。同时记录**下载行数**（实测 104 / 73 行 `downloading …`） | §8.2 | 超限 ⇒ 判"懒下载形态不可接受"，切 T4 的另两条路线（预置缓存目录 / `-b <本地bundle>` + `-C`）；并把首跑成本显式做进 UI（进度 + 说明），记入 `docs/design.md` 的"首编"预算说明 | P0 |
| **M2 format 构建**（bundle 已热、`cache\formats\` 被清） | **阈值待定，先测基线**；预设上限 **≤ 60 s〔推断〕**（依据：现 `.fmt` = 23.3 MB，构建是"引擎初始化 + 宏包加载"，量级应远小于 214 s 的下载） | **从未记录过耗时**（本轮只复核到产物存在：`6ffe0558…-latex-33.fmt` / 24,451,466 B / mtime 2026-09-13 23:32:59）；源码侧机制见 `tectonic-src/src/io/format_cache.rs`（按 bundle 摘要缓存，§8.1） | 超限 ⇒ **预置 `formats\*.fmt` 必须与预置 bundle 同批**（文件名含 bundle 摘要，换 bundle 即失效，§8.1）；T5 需把它写成 T4 的缓存门禁项 | P0（测量） |
| **M3 `-C` 离线（bundle+format 都热）** | **必须成功**，且耗时 ≤ 该档热态中位 × 1.1；日志 **0 行** `downloading` | `-C/--only-cached`（`tectonic --help`、§8.2） | 失败 ⇒ 判"离线不可用"（T4 的 `-b` 本地 bundle 路线是唯一出路） | P0 |
| **M4 `-b <本地bundle> -C`** | 产物与默认 bundle **等价**（页数、缺字数 0），体积与耗时另行记录 | `--bundle` 可指 URL 或路径（`tectonic --help`）；**本机从未实测**（`tectonic-integration-plan.md` §7、T4 分册） | 未通过 ⇒ 分发形态退回"预置缓存目录" | P1（T4 主责，本册只登记测量点） |

### 4.4 失败 / 超时 / 波动预算

| 项 | 预算 | 依据 | 处置 |
|---|---|---|---|
| 单次编译硬超时 | **120 s**（与产品默认一致；上限 1800） | `settings/model.rs:39`、`docs/design.md` §失败语义 | 超时样本记 `timeout`、**不计入 perf 样本**；不重试（同一上限重跑几乎必然再超时，design.md 明文） |
| 每 cell 有效样本 | **≥ 3**（默认 3 轮全有效） | §8.3 的 3 轮中位 | < 3 ⇒ 该 cell 判 `[数据不足]`，先按 §3 排障 |
| 整批无效样本占比 | **> 1/3 ⇒ 整批作废** | §7.7 ②（"4 s 的成功其实是 exit 1"这一类占多数的批次无意义） | 先修夹具前置条件（§2.4）再重跑 |
| 批内离散 | `(max−min)/median ≤ 50%` | 故障期 4.2×（§7.7 ①）；正常期 <5%（design.md） | 超限 ⇒ 整批不可信，重跑；两次都超限 ⇒ 报"环境不稳定"，不出结论 |
| 首跑档（M1/M2） | 允许单次、允许长（≥120 s 也要跑完，**不要套 120 s 超时**） | 214 s 的实测值本身就超过产品默认超时 | 首跑档单独一批（不与热态交错，避免一次 214 s 把同批热样本的机器状态拖走） |
| `thesis-real-hithesis` | **不设预算** | 见 §1.3（阻塞点未修，>240 s 未收敛，且末态是挂住而非失败） | 排除在判定外 |

---

## 5. 对拍口径：与 XeLaTeX / LuaLaTeX 比什么

### 5.1 比什么（三层，别混）

| 层 | 定义 | 用途 | 判据归属 |
|---|---|---|---|
| **L1 用户视角（主判据）** | 源码 → **PDF 落地**的墙钟（单次进程调用；Tectonic 用默认形态，`xelatex` 单趟出 PDF） | 决定"换引擎值不值" | §4.1 的 R1/R2/R3/R5 |
| **L2 只排版** | 源码 → **排版结束、不含 PDF 后端**：`xelatex -no-pdf`（1 趟）；Tectonic **`--outfmt xdv`（仍 3 趟 + 9 次 BibTeX，只省掉 xdvipdfmx）**，真正对齐"单趟只排版"的是 **`--pass tex`**（T1 复核：`PassSetting::Tex` 分支只调 `tex_pass`，不跑 bibtex、不跑 xdvipdfmx） | 分离"排版成本"与"PDF 后端成本" | §4.1 R4（诊断） |
| **L3 端到端（产品口径）** | `debounce(500ms) + 编译 + 预览重载` | 与 `docs/design.md` §延迟预算 对齐 | 沿用 design.md 预算（小文档 1s 优秀/2s 及格；大文档 3s 优秀/5s 及格）；**预览重载取真机实测值**（`multifile` 115 ms、31 页 62–69 ms） |

### 5.2 单趟 vs 收敛（**最容易比错的一处**）

| 引擎 | 默认形态实际做了几趟 | 怎么强制"单趟" | 本方案的记法 |
|---|---|---|---|
| `xelatex`（裸调） | 1 趟 | 已经就是 1 趟 | 记 `passes=1` |
| `latexmk -xelatex` | 收敛（多趟） | 不用它 | 记 `passes=N`（从 `fdb_latexmk`/日志读） |
| `lualatex`（裸调） | 1 趟 | 已经就是 1 趟 | 记 `passes=1` |
| **Tectonic（默认）** | **内部自己收敛**：T1 复核 **7 份运行日志全部 3 趟 TeX + 9 次 BibTeX**（`Running TeX` + `Rerunning TeX` 计数；仅 xdvipdfmx 有 1/0 之分）；上限 `DEFAULT_MAX_TEX_PASSES = 6`（`tectonic-src/src/driver.rs:1344`） | **三个档位要分清（T1 复核，本册旧稿的"`-r 0` = 单趟"不完整）**：<br>① `-r 0` = 1 趟 TeX，**但仍会跑 bibtex**；<br>② `--outfmt xdv` = **不是 Quick**：照样 3 趟 + 9 次 bibtex，只是省掉 xdvipdfmx；<br>③ **真正的单趟 = `--pass tex`**（`driver.rs:1492-1503 / 1754-1758`，不跑 bibtex、不跑 xdvipdfmx） | **三个数都记**：`default`（用户视角，进 R1–R3/R5）、`--outfmt xdv`（进 R4 的同引擎差值）、`--pass tex`（与 `xelatex -no-pdf` 对拍，进诊断） |

> **§8.3 的口径说明（要写进 T5，否则会被误读）**：那批的 Tectonic 2515 ms **含 3 趟 + 9 次 BibTeX + 内部 xdvipdfmx**（趟数/BibTeX 次数由 T1 在 7 份运行日志上复核），而对照的 `xelatex` 2539 ms 是**1 趟 + xdvipdfmx**。也就是说 §8.3 的"打平"对 Tectonic 是**保守**的；但正因工作量不同，**这 2515/2539 只能当阈值依据，不能当"同工作量对拍"**。

> **单趟档不能用来判"产出等价"**（`docs/design.md` §附：latexmk 开销拆解）：带 `\tableofcontents` 的夹具单趟跑出来的目录页码是**落后一趟**的——实测 `multifile` 插 400 行后单趟运行，`.toc` 里「第四章」页码由 29 → 41，即该趟 PDF 显示旧值 29；再跑一趟才稳定。
> ⇒ **分工**：单趟数字（XeLaTeX 侧＝裸调；Tectonic 侧＝**`--pass tex`**，不是 `-r 0`）只进 §4.1 的 **R4（成本诊断）**；**页数、页哈希、页级复用、SyncTeX 往返、产出等价性一律只在"收敛档"上判**（XeLaTeX 侧对应 latexmk Full 或"再跑一趟后的产物"）。`thesis`/`multifile`/`small-article` 三个夹具都带目录 ⇒ Tectonic 默认档会自动收敛，不能假设它是单趟。

> **R4 差值的归因是干净的**（T1 复核旁证）：`默认档` 与 `--outfmt xdv` 在日志上的差别**只有** `note: Running xdvipdfmx …` 这一行（7 份日志统计：PDF 档 **4/4 有**、xdv 档 **0/3 有**），其余 3 趟 TeX + 9 次 BibTeX 完全相同 ⇒ `默认档 − --outfmt xdv` 就是 PDF 后端成本，可直接用。**两档的 bib 结果也相同**（都会跑那 9 次 BibTeX，主调用成功、8 次章级空跑），所以这个差值不夹带 bib 差异。

### 5.3 已有基线数值（可直接复用，逐条给出处）

**A. Tectonic 侧（§8.3，同批交错 3 轮、逐次校验产出）**

| 夹具 | Tectonic（默认，出 PDF） | 备注 |
|---|---|---|
| `min-zh`（1 页） | **1586 ms** | 产物 `_zhcmp/tect/min-zh.pdf` 50,651 B |
| `thesis`（28 页） | **2515 ms** | 与 XeLaTeX 打平（0.99×）、比 LuaLaTeX 快 2.36×。**该 2515 ms 含 3 趟 TeX + 9 次 bibtex 子进程**（分母的裸 `xelatex` 单趟不跑 bibtex）⇒ 比值**对 Tectonic 保守**；bib 产出正常且与 XeLaTeX 等价（`.bbl` SHA-256 相同，见 §2.2 V6b） |

**B. XeLaTeX / LuaLaTeX 侧**

| 档 | 值 | 出处 |
|---|---|---|
| `xelatex -no-pdf` | `min-zh` **1378** / `thesis` **1681** ms | §8.3 |
| `xelatex`（出 PDF） | `min-zh` **2350** / `thesis` **2539** ms | §8.3 |
| `lualatex`（出 PDF） | `min-zh` **3887** / `thesis` **5932** ms | §8.3 |
| 中文单趟（27 页 / 74 页） | xe **1870 / 2189** vs lua **6468 / 7111** ms（3.5× / 3.2×） | §7.4、§2 |
| 英文同结构单趟 | xe 2341 vs lua **1878**（LuaLaTeX 反而快） | §7.4 |
| 仅导言区（固定开销） | xe 1565（占单趟 84%）vs lua 2869（44%） | §7.4 |
| PDF 后端成本 | `xdvipdfmx` ≈ **1125 ms** vs LuaLaTeX 引擎内写 PDF ≈ **288 ms** | §7.4 |
| LuaLaTeX 首用 | **25.8 s / 23.6 MB / 35 文件** | §2.1、troubleshooting |
| 完整编译（latexmk 收敛，首跑） | xe **9193** vs lua **16686** ms（1.8×） | §7.4 |

**C. 产品侧口径（`docs/design.md`，XeLaTeX + latexmk）**

| 档 | 冷编译 | 空跑 | 编辑触发 | 端到端(估) | 判定 |
|---|---:|---:|---:|---:|---|
| `tiny` | 2307 | 500 | 1625 | 2225 | FAIL |
| `small-article` | 6476 | 502 | 2593 | 3193 | FAIL |
| `multifile` | 6465 | 495 | 2463 | 3063 | FAIL |
| `graphics` | 2631 | 499 | 1826 | 2426 | EXCELLENT |
| `large` | 13532 | 486 | 4104 | 4704 | PASS |
| `thesis` | 9412 | 507 | 3143 | 3743 | PASS |
| `thesis-real-hithesis` | **>240 s 未收敛** | — | — | — | 不计入 |

（原始 JSON：`test_file/research/bench-report.json`，2026-09-12；同样 gitignored。§延迟预算口径：小文档 1 s 优秀 / 2 s 及格；大文档 3 s 优秀 / 5 s 及格，`docs/design.md` §延迟预算。）

**D. 编辑期单趟 vs 完整 latexmk（`scripts/bench-single-pass.mjs`，A/B 交替 3 次）**

| 档 | 完整 latexmk | 单趟 xelatex | 节省 |
|---|---:|---:|---:|
| `tiny` | 1625 | 1119 | 31.1% |
| `small-article` | 2683 | 1320 | 50.8% |
| `multifile` | 2437 | 1761 | 27.8% |
| `graphics` | 1878 | 1315 | 30.0% |
| `large` | 5307 | 2361 | 55.5% |
| `thesis` | 3370 | 1816 | 46.1% |
| **平均** | — | — | **40.2%**（阈值预设 20%，`test_file/research/bench-single-pass.json`） |

**E. 机制侧数值（页级复用 / 确定性 / 预览）**

| 项 | 值 | 出处 |
|---|---|---|
| 无可见变化编辑 → 页变化数 | **0 / 125 页** | `incremental-edit-x-dvi.md` §2.1 |
| 跳过 `xdvipdfmx` 省 | **0.65–0.94 s/次** | 同上 §2.2 |
| 74 页改末章 | `render` **102 → 7 ms**、`total 183 → 71 ms`、`pagesReused 0→7` | 同上 §2.3 |
| 页哈希页数（Tectonic XDV） | **28 页**，页字节 min/avg/max = 1038/9432/16101，解析 5.66 ms（44.6 MB/s） | §8.4 |
| Tectonic 确定性 | `SOURCE_DATE_EPOCH=0` 两次：PDF **80,203 B** / XDV **264,728 B** 逐字节一致（SHA-256 相同） | §8.4 |
| 预览重载 | `multifile` fetch 11 / parse 39 / render 64 / total **115 ms**；31 页档 62–69 ms | design.md §预览重载实测 |
| SyncTeX 产物 | Tectonic `main.synctex.gz` **77,983 B** | §8.4（**往返等价性未验**，见 §7） |
| **bib 产出等价性（最硬的一条）** | Tectonic `-k` 档 `main.bbl` **680 B** 与两条 XeLaTeX 基线 `.bbl` **SHA-256 完全相同**（`4370D168…`）；PDF 文本含 `参考文献`+`[1]`…`[9]` | 队长产物级实测 + 本轮独立复核（2026-09-14，§2.2 V6b） |

> **复用纪律**：上表数值来自**别的会话/别的批次**（2026-09-12～13）。按 §7.7 ⑤，它们**只能用作阈值依据与复现目标**，不能与今天跑出的绝对值直接相减判回归。判回归必须在**同批**里放上基线的重测值。

---

## 6. 回归基线如何固化

### 6.1 落点（tracked vs gitignored 必须分清）

| 类别 | 落点 | 是否入库 | 理由 |
|---|---|---|---|
| **测量 harness（脚本）** | `scripts/bench-tectonic.ps1`（由现成 `test_file/projects/bench/_zhcmp/tectonic-bench.ps1` 提升而来）+ `scripts/bench-tectonic-report.mjs`（汇总/阈值判定/退出码） | **入库**（`scripts/` 未被 gitignore） | `test_file/projects/` 与 `test_file/research/` 已被 `.gitignore` 覆盖 ⇒ 脚本留在那里=换机器/清仓即丢（现有 `_zhcmp/*.ps1` 就处于这个状态） |
| **夹具生成** | 扩展 `scripts/gen-bench-projects.mjs`，让它顺带产出 Tectonic 用的夹具副本（`_zhcmp/tect`、`_zhcmp/tect-thesis`、`_zhcmp/thesis`、`_zhcmp/multifile`） | 入库 | 现在这几份副本是**手抄**的（`_zhcmp/min-zh.tex` 与 `_zhcmp/tect/min-zh.tex` 同为 1007 B，但没有任何生成器保证它们与主夹具同步），手抄迟早与主夹具漂移；夹具漂移会直接变成"阈值漂移" |
| **控制组探针** | `_zhcmp/bench-lua.lua`（CPU）、`_zhcmp/io-control.lua`（磁盘）**提升到 `scripts/`** 或由 harness 内联 | 入库 | 同上 |
| **原始样本 / JSON 报告** | `test_file/research/bench-tectonic.json`（含每次运行的 ms、exit、passes、页数、缺字数、无效样本、控制组值、机器指纹） | **不入库**（同 `bench-report.json` 律） | 体积与机器相关 |
| **精炼基线表（权威）** | ① `docs/design.md` 基准表**加一列 Tectonic**（同一张表里才可比）；② `docs/research/tectonic-test-plan.md`（T5）的「测量与阈值」节 | **入库** | 这是唯一能跨机器/跨清仓存活的基线 |
| **现场证据（日志/产物）** | `_zhcmp/logs/`、`_zhcmp/tect*/` | 不入库 | 已在 gitignore 覆盖面内 |

### 6.2 每次改动后跑什么（按改动面分层，避免每次全跑）

| 改动面 | 最小回归集 | 为什么是这些 |
|---|---|---|
| 引擎命令构造 / `Engine::Tectonic` 分支 / 收尾逻辑 | ① `scripts/bench-tectonic.ps1 -Rounds 3 -Tiers min-zh,thesis`（含控制组）② `node scripts/check-determinism.mjs`（三档 × Full/Quick ×2）③ `node scripts/xdv-report.mjs <tmp/*.xdv>` 页数对拍（必须解出 28 页；**U1 定论后，这条只在产 XDV 的档上成立，PDF 档要用 `pdfinfo` 的页数代替**） | ①②③ 分别覆盖"变慢了 / 变得不可复现 / 页索引不再认识产物"三类回归，且都是秒级到十几秒级 |
| 页级复用 A/B/C 与预览 | `multifile` 的 §4.2 三条命中率 + 真机 `window.__previewLastReload`（`skippedReloads`/`changedPages`/`pagesReused`/`render`） | 命中的是"下游重复工作"，脚本量不出体感，必须真机插桩（口径见 design.md §预览重载实测） |
| 默认引擎仍是 XeLaTeX（回归保护）—— **⚠ 默认已改 Tectonic（2026-09-17，ADR-0014 修订 1）**，本条改读为"默认引擎是 Tectonic" | `node scripts/bench.mjs`（超预算退出码 1） | 防止"为 Tectonic 改了命令构造，把默认路径弄坏" |
| 分发 / bundle / 缓存 | §4.3 的 M1–M4（首跑档单独一批）+ **V6b 的产物级判据（`.bbl` 的 `\bibitem` 条数 / PDF 参考文献节）** + T4 分册的门禁清单 | 首跑成本是分发形态的函数，不是产品代码的函数；**bundle 版本/来源一变，bib 的行为就可能变**（`--bundle` 可指任意 tar），所以 bundle 改动必须带 V6b |
| **日志/可观测性（本轮新增，独立一行）** | ① `-k` 档下断言 `.bbl` 的 `\bibitem` 条数与 PDF 参考文献节；② **判定器负例**：喂一份"成功且日志形态不同（无 `You've used` 摘要段）"的 `.blg`（本仓 `main.blg` 537 B：0 error、无摘要行），判定器**必须判为 unknown，不得判为 fail** | **防的是"跨引擎日志契约差异"（`engine.md` G6「引擎自述 ≠ 事实」），不是数据损坏容错**：Tectonic 的 `.blg` 是**完整文件**（端口不实现收尾统计段），任何"按 `.blg` 判 bib 成败"的工具/脚本都错（摘要行 ⇒ 100% 假阴性；error 计数 ⇒ 8 假阳性 + 1 假阴性）。`run.log`/`.log` 侧另有更强的一条自述失真：见附录 `.log` 自述行 |
| 提交前（任何改动） | `npm run build`、`cargo test -p latteset-core`、`cargo test -p latteset-infra -- --ignored`（含 streaming / live_errors 两条） | `docs/modules.md` §12.3 已固化的验证入口 |

### 6.3 基线固化形态（建议写进 T5 的「复现资产清单」）

```text
scripts/bench-tectonic.ps1            # 探针 + 控制组 + 交错 + 逐次校验 + 无效样本表（tracked）
scripts/bench-tectonic-report.mjs     # 读 JSON → 判定 R1..R5 / H_B,H_C / M1..M4 → 退出码（tracked）
scripts/gen-bench-projects.mjs        # 顺带产出 _zhcmp/* 的 Tectonic 夹具副本（tracked）
test_file/research/bench-tectonic.json  # 原始样本（gitignored）
docs/design.md                        # 精炼基线表（加 Tectonic 列）——唯一权威（tracked）
```

**机器指纹（每次写进 JSON）**：`platform/arch`、`node -v`、引擎版本三件套（`tectonic --version` / `xelatex --version` / `lualatex --version`）、TL 年份、CPU/内存型号、Tectonic 缓存文件数与字节数、`formats/*.fmt` 名字、fontconfig 缓存 mtime、控制组四组值。**依据**：§7.7 ① 那次故障复盘——没有"fontconfig 缓存 mtime + fc-list 计时"这两条，就分不清"电脑变慢"与"缓存坏了"。

---

## 7. 未验证清单（T5 直接收录；按优先级）

| # | 项 | 为什么关键 | 一条命令/最小做法 | 优先级 | 归属 |
|---|---|---|---|---|---|
| ~~**U1**~~ ✅ | ~~**PDF 形态能否同时落 XDV**（`-k/--keep-intermediates`）~~ **已定论：不能（PDF 档不落 `.xdv`，`-k` 也不落）** | 决定"出 PDF 能不能同时拿到页哈希"——即 **A/B/C 与默认 PDF 形态能否共存**。结论：**不可兼得**（除非改用 `--pass tex`/`--outfmt xdv` 档，或让 Full 之后显式作废旧 XDV）。证据与机制见 §0 第 6 条；T1 已写成 `engine.md` **E2.8/E2.9/E2.10（P0）+ G5 门禁** | 已闭环；自证方式 = 对比 `_zhcmp/tect-kcheck-with-k/`（有 `.aux/.toc/.bbl`、**无 `.xdv`**，`run.log` 里 `main.xdv` 0 次提及）与 `tect-kcheck-no-k/`（只有 `.log/.blg/.pdf`） | **P0（已闭环）** | T1 已确认；影响 T2 的 Quick/Full 映射与 A/B/C 判据 |
| **U2** | `multifile`（74 页）在 Tectonic 上的基线 | §8.3 **未测**；R3 阈值现在没有依据 | 跑 §2 协议一档（3 轮 + 1 预热；`multifile` 单次秒级，整档分钟级） | **P0** | T3 后续批次 |
| **U3** | Tectonic **`--pass tex`（真单趟）与 `-r 0`（1 趟但仍跑 bibtex）两档**数字 | 现在只有"默认档（3 趟 + 9 次 BibTeX）"的数字，单趟对拍缺一半；且 `-r 0` **不等于**单趟（T1 复核），旧写法会给出错误的"单趟" | 同 U2 批次里加两列：`--pass tex` 与 `-r 0`（注意 `--pass tex` 不产 PDF，只能与 `xelatex -no-pdf` 对拍） | P1 | T3 |
| **U4** | **冷 format 构建耗时**（M2） | 首跑成本被拆成两半，其中一半从没量过；影响 T4 的"预置 formats"门禁 | 移走 `cache\formats\*.fmt`（先备份）→ `SOURCE_DATE_EPOCH=0 tectonic -C --keep-logs main.tex` 计时 → 还原 | **P0** | T3 / T4 |
| **U5** | `-b <本地bundle> -C` 的离线等价性 | T4 推荐路线②的前提（`tectonic-integration-plan.md` §1） | 造一个中文常用 bundle → 离线编 `thesis` → 页数/缺字数/耗时 | **P0** | T4（本册只登记测量点） |
| **U6** | SyncTeX 往返等价性（Tectonic 产物 vs XeLaTeX 基线） | 现在只验到"有产物 77,983 B"（§8.4） | `latteset-cli --project … forward/inverse` 对拍，报页码/行号偏差 | **P0** | T1/T2 |
| **U7** | 页级复用命中率在 Tectonic 上的实测（`pages>0`、`changed_pages` 正确） | §8.4 只验了"`xdv-report.mjs` 能吃 Tectonic 的 XDV"，**没验**调度器端的 `pages/changed_pages` 真值；U1 定论后还多一层：**PDF 档 `pages==0` ⇒ 该形态下页级复用根本不成立**，判据要在 `--outfmt xdv` 档上建 | 集成后跑 §4.2 的 B/C 两组编辑（**必须用产 XDV 的档**），并记录 `pages == 0` 时的降级行为（全量刷新）是否为可接受的产品行为 | **P0** | T2（判据由本册给） |
| **U8** | 真实学位论文模板（hithesis 档） | 当前受阻（§1.3）；Tectonic 侧还要先答"bundle 里有没有 `hithesis.cls`" | ㉖ 修好后重测；Tectonic 侧可先 `tectonic -C --keep-logs` 试编并读缺包错误 | P1 | T2/T4 |
| **U9** | **首次 `fc-list` 冷启动**对判据的影响（本轮实测 3.9 s vs 热态 0.45 s） | 已在本册 §3.2 给出规则（三连测取后两次），但**只有本轮一次现场观测**，没有多机样本 | 在 2–3 台机器上重复三连测，确认冷启动量级 | P1 | T3 |
| **U10** | `xelatex --version` 819 ms（本轮）vs 文献 495–690 ms | 可能是冷启动，也可能是 fontconfig 边缘劣化的前兆；本轮只有 1 次观测 | 正式批次里三连测，并在控制组里固化 | P1 | T3 |
| ~~**U11**~~ ✅ | ~~biber/bibtex 全链路~~ **已定论：bib 链路好的，是被测的"日志判据"坏了** | 产物级证据：`.bbl` 680 B / 9 条 `\bibitem`，与两条 XeLaTeX 基线 **SHA-256 相同**；PDF 文本含 `参考文献`+`[1]`…`[9]`；`main.blg` 537 B **只含该端口实现的段**（banner/Capacity/aux 轨迹），**确定不含 `You've used` 摘要段**（非丢尾）⇒ 摘要行判据 100% 假阴性、error 计数 8 假阳性 + 1 假阴性 | 已闭环；判据按 §2.2 **V6b（产物级）** 执行，`V6c` 把 8 次章级空跑单列 | P1（仅剩观测性） | T3（判据）+ T1/T2（`engine.md` G6：别把"引擎自述/日志形态"当事实） |
| **U12** | **Tectonic 模式下 `.aux`/`.toc` 的存在性依赖 `-k`** | 干净实测：`with-k` 目录有 `main.aux`(443 B)、`main.toc`(1623 B)、`main.bbl`(680 B)、`chapters/ch0N.aux`；`no-k` 目录**只有** `.log/.blg/.pdf`。这直接影响既有不变量「无 `tmp/<stem>.aux` 时 Quick 自动升级 Full」（`docs/modules.md` §12.2）在 Tectonic 模式下该怎么映射——Tectonic 自己内部收敛 3 趟，"Quick/Full"的语义需要重新定义 | 集成后：分别在 `-k`/无 `-k` 下跑一次，记录 `.aux` 存在性与 `Success{kind}` 的实际取值 | P1 | T2 |

---

## 附：本轮只读核查的现场证据索引

| 证据 | 位置 | 关键内容 |
|---|---|---|
| PDF 档**不落** XDV（负证据，T1 复核） | `_zhcmp/tect-thesis/det1.log`、`det2.log`（PDF 档全文） | **无任何 `main.xdv` 字样**；两档都有 `note: Skipped writing 11 intermediate files (use --keep-intermediates to keep them)` |
| XDV 档会落 XDV（正证据） | `_zhcmp/tect-thesis/run-xdv.log`、`detxdv1.log`、`detxdv2.log` | 都有 ``note: Writing `main.xdv` (258.5234375 KiB)``；有 9 次 `Running BibTeX`、3 趟 TeX；**没有** `Running xdvipdfmx` |
| mtime 佐证（不是"落得晚"） | `_zhcmp/tect-thesis/` 目录时间戳 | `main.xdv` 23:44:01（= 最后一次 xdv 档 `d2.xdv` 时刻）与 `main.pdf`/`main.log`/`main.synctex.gz` 23:44:03 **不同**；若 PDF 档写过 xdv，`File::create` 必把 mtime 推到 23:44:03 |
| 驱动跳写中间产物的条件（**已被更直接的原因取代**） | `tectonic-src/src/driver.rs:1615-1623`（+ `:59-77` / `:487-508`） | `!keep_intermediates && (access_pattern != Written \|\| ALWAYS_INTERMEDIATE_EXTENSIONS…)` → skip；`ALWAYS_INTERMEDIATE_EXTENSIONS = [".snm", ".toc"]`（:1345）不含 `.xdv`。⚠️ 这条**不是** PDF 档不落 `.xdv` 的原因（`-k` 档实测也不落，见下两行）⇒ 直接原因见 `driver.rs:1985` 的 `files.remove(&self.tex_xdv_path)` |
| **两档产物集合（本轮干净实测，各自只跑一次）** | `_zhcmp/tect-kcheck-with-k/` vs `tect-kcheck-no-k/`（2026-09-14 16:14） | `with-k`：`main.aux` 443 / `main.toc` 1623 / `main.bbl` 680 / `main.log` 22909 / `main.blg` 537 / `main.pdf` 80203 + `chapters/ch0N.aux`(1013–1044) + 8×`ch0N.blg`；**无 `.xdv`**。`no-k`：**只有** `main.blg`/`main.log`/`main.pdf`（+8×章级 `.blg`）。`with-k/run.log` 计数：`Running TeX` 1、`Rerunning TeX` 2、`Running BibTeX` 9、`errors were issued by BibTeX` **8**、`Running xdvipdfmx` 1、`Writing \`main.pdf\`` 1、**`main.xdv` 提及 0 次** |
| **不落 `.xdv` 的直接原因（T2 定位，T1 收进 `engine.md` G5/E2.8）** | `tectonic-src/src/driver.rs:1985`（PDF 档）/ `:2013`+`:2017`（xdv 档分支） | xdvipdfmx 跑完后 `self.bs.mem.files.borrow_mut().remove(&self.tex_xdv_path)` ⇒ **XDV 被从内存文件表删除**，`write_files` 根本看不到它 ⇒ 与 `-k` 无关、与访问模式也无关 |
| **bib 产出等价性（产物级，最硬）** | `tect-kcheck-with-k/main.bbl`(680 B)、`bench/thesis/tmp/main.bbl`(680 B)、`_zhcmp/thesis/tmp/main.bbl`(680 B) | 三份 **SHA-256 完全相同**（`4370D1681F352FA1…`）；内容含 `\begin{thebibliography}`…`\end{thebibliography}`、`\bibitem`×9、`\newblock`×18；同次 `main.pdf` 28 页、`pdftotext` 文末 `参考文献` + `[1]`…`[9]` |
| **`.log` 自述 ≠ 事实（G6 的最强现场旁证，队长建议互引）** | `tect-kcheck-with-k/main.log`（22,909 B，PDF 档） | 末行写着 `Output written on main.xdv (28 pages, 264728 bytes).`，**而那个 `main.xdv` 在磁盘上并不存在**（PDF 档不落 xdv，见 §0 第 6 条）⇒ 比 mtime 更硬地证明"**引擎自述 ≠ 事实**"；`engine.md` **G6** 已互引 |
| **`.blg` 字节级证据：端口不实现收尾统计段（T1 反证 + 本轮独立复核）** | `test_file/tectonic-install/unpacked/tectonic.exe`（51,538,432 B）ASCII 搜索 + `test_file/tectonic-src` 源码树 grep | `You've used` **0 命中**、`wiz_defined` **0 命中**、`built_in function` 0、`histogram` 0；**对照组**同一次搜索命中：`This is BibTeX`@**13662231**、`Capacity: max_strings`@**13662281**、`I found no`@13671510 ⇒ 方法有效。源码树同样 0 命中；`crates/engine_bibtex/src/lib.rs:548-567` 只写 banner(`Version 0.99d`)/`Capacity: max_strings=…`/顶层 aux 行 ⇒ **537 B 的 `.blg` 是完整文件** |
| Tectonic 自己的章级 aux（V6c 的实证） | `tect-kcheck-with-k/{main.aux, chapters/ch01.aux, chapters/ch03.aux}` | `main.aux` 443 B：`\bibdata`×1 / `\bibstyle`×1 / `\bibcite`×9；`chapters/ch01.aux` 1013 B 与 `ch03.aux` 1014 B：**三项全为 0** ⇒ 8 次章级 bibtex 注定报 `I found no \bibdata command`（**不是** bundle 缺陷） |
| 趟数 / BibTeX 次数（T1 复核） | 7 份运行日志（`det1/det2/detxdv1/detxdv2/run1/run-xdv/sync`） | **全部 3 趟 TeX + 9 次 BibTeX**，仅 xdvipdfmx 有 1/0 之分（PDF 档 4/4 有该行、xdv 档 0/3 有）；`DEFAULT_MAX_TEX_PASSES = 6`（`driver.rs:1344`）；真单趟 = `--pass tex`（`driver.rs:1492-1503 / 1754-1758`） |
| **BibTeX 日志的三种误判（V6b 的反例素材）** | `tect-kcheck-with-k/main.blg`(537 B)、`chapters/ch01..08.blg`(各 306 B)、xelatex 基线 `bench/thesis/tmp/main.blg`(1226 B) | **成功的那份 `main.blg` 也没有 `You've used` 摘要行**（端口不写该段，**确定性缺失**而非截断）⇒ 摘要行判据 **假阴性**；数 `error messages` 行 ⇒ **8 假阳性**（章级各 1）+ **1 假阴性**（`main.blg` 0 条）；数 warn 行 ⇒ 同向错。**唯一不装反的判据是产物**：`.bbl` 的 `\bibitem` 条数 / PDF 的参考文献节 |
| **`.blg` 清单（引擎行为差异的实证）** | 递归 `*.blg`：`_zhcmp/tect-kcheck-{with-k,no-k}/`（8 章级 + 1 主）、`_zhcmp/tect-thesis/`（8 章级 + 1 主）、`_zhcmp/thesis/tmp/`（**仅主**）、`bench/thesis/tmp/`（**仅主**） | ⇒ 逐 aux 跑 bibtex 是 **Tectonic 独有**（`driver.rs:1949-1959` 遍历主 aux + 其它所有 `.aux`；latexmk 只跑 `main.aux`）⇒ 8 次注定报错的空跑 |
| CLI 面 | `tectonic --help`（本机 0.17.0） | `-o/--outdir`（默认 = input 所在目录）、`-b/--bundle`、`-C/--only-cached`、`--outfmt`、`--pass`（`--pass tex` = 真单趟）、`-r/--reruns`、`-k/--keep-intermediates`、`--keep-logs`、`--synctex`、`-p/--print`、`-c/--chatter`、`--hide`、`-Z` |
| 页数复核 | `bench/{tiny,small-article,multifile,thesis}/tmp/main.log` | 1 / 3 / 74 / 28 页 |
| 夹具内容统计 | `bench/thesis/chapters/ch01..08.tex`、`bench/small-article/main.tex` | 每章 `figure`×1 + `equation`×1 + `\cite`×2；small-article 1 图 1 表 2 cite + `refs.bib`（`article` 类、英文） |
| 控制组现场 | 本轮命令输出 | `fc-list` 3640 条；3862（冷）/545/446/441 ms；`fc-cache -v` 0 条 invalid、`succeeded`；`cmd /c exit` 133 ms、`tectonic --version` 32 ms、`lualatex --version` 123 ms、`xelatex --version` 819 ms |
| Tectonic 缓存 | `%LOCALAPPDATA%\TectonicProject\Tectonic\cache` | `bundles/` + `formats/`；426 文件 / 65,254,100 B；`formats/6ffe0558…-latex-33.fmt` 24,451,466 B |
| fontconfig 修复痕迹 | `C:\texlive\2026\texmf-var\fonts\cache` | 缓存文件 mtime 全为 2026-09-13 23:10（与 §7.7 ① 的 `fc-cache -f` 重建时刻一致） |
| 现成 harness | `test_file/projects/bench/_zhcmp/{tectonic-bench,recheck,run4}.ps1` | 协议实现：预热不计入、3 轮、逐次 `Output written` 校验、无效样本表、`tmp/chapters` 预建、`SOURCE_DATE_EPOCH=0`、双 `WaitForExit` 取退出码 |
