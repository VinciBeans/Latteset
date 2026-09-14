# T1 · 引擎层测试矩阵：Tectonic 0.17 行为 + 与现有 `xdv.rs` / `parse_log` / SyncTeX 链路的兼容性

> **本文边界**：只覆盖**引擎层**（进程行为、产物格式、日志/进度口径、失败面）。
> 集成/UI（`Engine::Tectonic` 分支、设置项、Quick/Full 映射、降级回退）→ **T2**；
> 测量与阈值 → **T3**；分发/缓存/许可 → **T4**。本文只出方案，不改产品代码。
>
> **验证标记**（每项都有）：
> - `✅实测` = 本轮或已有工件/日志/产物直接读出的实测结果；
> - `📖源` = 读 Tectonic 0.17 源码或本仓库源码得到的确定结论（未运行）；
> - `⚠️推断` = 未实测、按源码/行为外推，**必须由本矩阵的用例把它变成实测**。
>
> 记号：`<T>` = `tectonic.exe`（0.17.0，`C:\Users\Vinci\.cargo\bin\tectonic.exe`）；
> `<FIX>` = `test_file/projects/bench/_zhcmp`；`<P>` = 单用例独立探针目录（**不要**写进夹具目录，也不要写进应用 `tmp/`）；
> `<R>` = 本任务定义的仓库内原始证据（见 §3）。所有 `<P>` 必须预先建目录：`-o` 指向不存在的目录 Tectonic 直接报错退出（📖源 `src/bin/tectonic/compile.rs:163-168`）。

---

## 0. 结论先行：引擎层有 6 个必须先解决的门禁 + 1 个判据反转

> **判据反转（§0 最重要的一句）**：本矩阵**原先把 G3 写成"bibtex 失败被静默"，该结论已被产物级证据推翻并撤回**（2026-09-14，队长实测 + 本册复核，见 E6.1/§3）。真实结论是：**主 bibtex 调用成功**，而**不能从 `.blg` 判成败**——因为 Tectonic 的 BibTeX 端口**根本不写统计块**。凡"用引擎自述当判据"的规则都在这条上翻过车，故新增 **G6**。

| # | 门禁 | 等级 | 依据 |
|---|---|---|---|
| G1 | **编译中没有任何实时反馈**：Tectonic 把 `.log`/`.xdv`/`.pdf` 全部写在**内存层**，进程结束才落盘；而我们的实时反馈主来源是**尾随 `tmp/<stem>.log`**（按页 flush）。⇒ Tectonic 模式下「编译中错误 / 页进度」双双失效。**最硬的现场旁证（比 mtime 硬）**：`<R>/tect-kcheck-with-k/main.log`（22,909 B）的**末行**写着 `Output written on main.xdv (28 pages, 264728 bytes).`，而该 `.xdv` **在磁盘上并不存在**（PDF 档，`run.log` 全文 0 次提及 `main.xdv`）——**日志自述的产物不在磁盘上**。与 **G6** 是同一根问题的两个面（G1 = 落盘时机，G6 = 自述内容），**两条互引** | **P0** | 📖源 `src/driver.rs:1121-1205`（`FilesystemIo::new(..., writes_allowed=false, ...)`）、`src/driver.rs:1528`/`1506`（`write_files` 只在结束/报错时调用）；✅实测 `<R>/tect-kcheck-with-k/main.log` 尾行 vs 该目录清单（无 `.xdv`）；本仓 `crates/latteset-infra/src/runner.rs:138-161` |
| G2 | **forward/inverse 在"免装 TeX Live"下不可用**：Tectonic 只产出 `.synctex.gz`，**它自己没有 synctex 解析器/CLI**（26 个 crate 里无 synctex）；我们的 `SyncTexCli` 起的是**外部 `synctex.exe`**（TeX Live 提供） | **P0** | 📖源 `test_file/tectonic-src/crates/`（无 synctex crate）；本仓 `crates/latteset-infra/src/synctex.rs:60-68`；实测 `synctex.exe` 来自 `C:\texlive\2026\bin\windows\synctex.exe`（v1.5） |
| G3 | **bibtex 告警是"假警报"、且无真信号**：Tectonic 对**每个** `.aux` 各跑一次 bibtex（含 8 个无 `\bibdata` 的章级 aux），章级那 8 次必然报错 ⇒ 每次成功构建都打 **8 条**小写 `warning: errors were issued by BibTeX, but were ignored; …`；而主调用（`main.aux`）**成功且无任何信号**。⇒ 两条后果：① 我们的 `parse_log` 警告判定要求字面量 `Warning`（大写）⇒ 这 8 行**连警告都不算**（用户看不见）；② 反过来，若 T2 把它当告警实现，用户会在**每次成功构建**上收到 8 条假警报。**真正的 bib 失败信号也不存在**（见 G6） | **P0** | ✅实测（`<R>/tect-kcheck-with-k`：`run.log` 9 条 `Running BibTeX` + **8** 条 `warning:`；`main.bbl` **680 B / 9 条 `\bibitem`**；`main.pdf` 28 页）；📖源 `src/driver.rs:1934-1939`（吞错措辞）、`1949-1959`（逐 aux 跑）；本仓 `crates/latteset-core/src/log_parser/scan.rs:89-93` |
| G4 | **缺字（Missing character）同样不可见**：`Missing character: …` 行不满足任何警告/噪音规则 ⇒ 既不入 `parse_log` 的警告，也不入流式通道（流式只收 Error） | **P1**（但失败形态最坏：编译成功、预览静默缺字） | 本仓 `scan.rs:31-109`（无该规则）、`runner.rs:169-184`（`warnings=false`） |
| G5 | **默认 PDF 形态根本不落 `tmp/<stem>.xdv`**（不是"落得晚"，是**不落**）。**机制（比访问模式更直接）**：`xdvipdfmx_pass` 结束时把 XDV 从**内存文件表里删除**——`self.bs.mem.files.remove(&self.tex_xdv_path)`（`driver.rs:1985`）⇒ `write_files` 无论 `keep_intermediates` 真假都看不到它，**`-k` 不是变量**。⇒ ① §8.4「XDV 就是我们的格式、A/B/C 无需改代码」**只在 `--outfmt xdv` / `--pass tex` 档成立**；② 更危险：产品侧"Quick 与 Full 共享一份页哈希缓存"的设计（`runner.rs:200-211`）会让 Full 读到 **Quick 留下的陈旧 XDV**，而 `runner.rs:463-466` 记录的正是这个已实测事故（陈旧 XDV 的转换结果覆盖引擎刚写好的 PDF，109,732 B → 70,193 B） | **P0** | 📖源 `driver.rs:1985`（`remove`）+ `1615-1623`（跳过规则，仅在未 remove 时才轮到它）；✅实测 **PDF 档 + `-k` 两个夹具均无 `.xdv`**（T2，`<R>/integration.md §0.1.1`）；✅旁证（昨日）：PDF 档日志无 `main.xdv`、`main.xdv` mtime 停在 xdv 档那次（见 E2.8） |
| G6 | **引擎自述 ≠ 事实：Tectonic 的日志/`.blg` 形态与 TeX Live 不同，不能拿 TeX Live 的日志契约当判据**。旗舰例：**Tectonic 根本不可能输出 `You've used N entries`**——该字符串在源码树与**已安装二进制**里都不存在（字节级复核），因为 BibTeX 的收尾统计块没被这个 Rust 端口实现。⇒ ① **成功的 `.blg` 也永远没有摘要行**（`You've used` 判据 = 100% 假阴性）；② 那份 537 B 的 `main.blg` **是完整文件，不是"丢尾"**（本册与 T3 曾先后判为"中止"与"尾部丢失"，均作废）。同族现象：日志**无 `This is XeTeX` 抬头**（首行是 `**`）、告警措辞是**小写 `warning:`**、章级 `.blg` 只有 `(There were 2 error messages)` 这一种收尾。⇒ **判据必须落在产物上**（`.bbl` 的 `\bibitem` 数 / PDF 文本 / `main.log` 的 `Citation … undefined` 计数）。**可判定形态（门禁检查表）**：任何新增的日志级规则，必须逐条过这三问——① 该规则依赖的字面串**在本二进制里存在吗**（用对照串做字节搜索自证方法有效）；② 该规则在**成功**样本上会不会误报（Tectonic 侧必须用"成功但无摘要行的 `.blg`"当负例，要求判 `unknown` 而非 `fail`）；③ 该规则在**失败**样本上会不会漏报（需 T2 造一个真失败的 bib 夹具）。**三问任一不过 ⇒ 该规则不得进产品**。与 **G1** 互引 | **P0** | 📖源 `crates/engine_bibtex/src/lib.rs:548-567`（banner/Capacity/aux 轨迹都写了，统计块无对应代码）；✅**二进制字节级**：`tectonic.exe` 中 `You've used`/`wiz_defined` **未命中**，而同一次搜索的对照串 `This is BibTeX`(@13662231)、`Capacity: max_strings`(@13662281) **命中**（⇒ 方法有效、字符串确实不存在）；✅全源码树 grep 统计块关键词 **0 命中**；✅产物级：`<R>/tect-kcheck-with-k` 的 `.bbl` 完整（9 条 `\bibitem`）而 `.blg` 仍止于 `Database file #1: refs.bib`；✅grep `Database file #1` 在二进制里**未命中**，但那是因为二进制存的是 `Database file #`、编号运行时拼接（**不构成反例**，队长复核） |

**另有一个工具链缺陷（不是 Tectonic 的问题，但会毒化本矩阵的判据）**：
`scripts/xdv-report.mjs` 头部第 9 行的用法 `--diff <b.xdv>`（**空格分隔**）**静默不生效**——脚本只认 `--diff=<b.xdv>`（等号）。
本轮实测：空格形式输出的是普通摘要、`exit=0`、**没有任何差分**；等号形式才输出「页级差分：A=28 页 / B=27 页」。
⇒ 任何按文件头用法写的"页级差分"用例都会**永远绿**。见 **E2.4** 与 §3 的「工具-①②」。

---

## 1. 公共约定（所有用例共用）

| 约定 | 值 | 说明 |
|---|---|---|
| 被测可执行 | `<T>` = `tectonic.exe` 0.17.0 | 已装于 `~/.cargo/bin`，PATH 可用（§8.2） |
| 离线/联网 | 除 E7.2/E7.3 外，一律 `-C`（只用缓存）。**本矩阵不下载 bundle** | 首跑联网下载成本见 §8.2（hello 189 s / min-zh 214 s / thesis 51 s） |
| 输出目录 | `-o <P>`，`<P>` 必须先 `New-Item -ItemType Directory` | 📖源 compile.rs:163-168 |
| 文件搜索根 | = **主输入文件所在目录**（与 cwd 无关） | 📖源 driver.rs:1123-1127 ⇒ 可以传相对路径，也可以传绝对路径，`\include{chapters/chNN}` 都解析 |
| 日志 | `--keep-logs`；失败时同样产出 `.log`/`.blg` | 📖源 driver.rs:1506（错误路径 `write_files(None, status, true)`，`only_logs=true`）+ 1625-1629 |
| 确定性 | 所有确定性用例固定 `SOURCE_DATE_EPOCH=0`（与产品一致，`runner.rs:275-280`） | |
| 页哈希工具 | `node scripts/xdv-report.mjs <file.xdv>`（研究工具） / `latteset_core::xdv::page_hashes`（产品路径） | 二者**哈希算法不同**：JS 用 SHA-1 前 16 hex，Rust 用 `DefaultHasher`（见 E2.5） |
| 夹具 | `tect-thesis/`（main.tex + chapters/ch01..08 + refs.bib，28 页）、`tect/min-zh.tex`（1 页）、`tect/hello.tex` | 已有实测产物 |

---

## 2. 测试矩阵

### E1 · 中文正确性（字体回退 / fontset / 缺字）

| ID | 目的 | 命令 | 期望 | 判据（可判定） | 优先级 | 依赖 | 是否已验证 |
|---|---|---|---|---|---|---|---|
| E1.1 | 基线：中文能排、无缺字 | `$env:SOURCE_DATE_EPOCH=0; <T> -C --keep-logs --outfmt pdf -o <P> <FIX>/tect-thesis/main.tex` | exit 0；`<P>/main.pdf` 存在；`<P>/main.log` 存在 | `grep -c "Missing character" <P>/main.log` **== 0**；`pdftotext <P>/main.pdf -` 含中文正文（至少命中 `引擎对比基准` 或夹具章标题）；`pdfinfo` 页数 == 28 | **P0** | 缓存已装（§8.2） | ✅实测（§8.4：0 条 Missing、pdftotext 出中文、28 页） |
| E1.2 | 字体回退路径确认为 `ctex → fontset=windows + fontconfig`，**不依赖 bundle 内 CJK 字体** | ① `<T> -C --keep-logs -o <P> <FIX>/tect/min-zh.tex`；② 在 `main.log` 里找 `fontspec` / `xeCJK` / `TU/…` 字体声明行 | 排版成功、无缺字 | ① `Missing character` == 0；② 日志里出现系统字体名（非 bundle 内字体文件路径）| **P0** | E1.1 | ⚠️推断（§1 结论"中文不依赖 bundle 字体"来自本次集成方案，**本轮未复验**；必须由本用例坐实） |
| E1.3 | **fontset 显式档位**不改变正确性（回退健壮性） | 对 `<P>/min-zh.tex` 的副本各加一行 `\ctexset{fontset=windows}` / `=mac` / `=fandol` / `=ubuntu`，各编译一次 | 每档：缺字 == 0 或**给出可解释的差异** | 每档记录 `Missing character` 计数与页数；判据：**windows 档 == 0**；其它档若 >0，必须同时在 UI 侧有可见信号（G4 的修复目标）——否则记为「静默缺字」缺陷 | P1 | E1.2 | ⚠️推断（从未测过 `fontset` 档位） |
| E1.4 | **缺字必须可判定**（把"静默缺字"变成红灯） | 构造缺字夹具：`\setCJKmainfont{不存在的字体}` 或 `\usepackage{ctex} + 生僻字` | 编译"成功"但日志有 `Missing character` | 判据三段式：`grep -c "Missing character"` > 0 **且** `pdftotext` 抽检该字**缺失** **且** 页数不变 ⇒ 定义该夹具为"缺字样本"；随后断言**当前 UI 不可见**（G4 的修复目标，交 T2） | **P0** | G4 | ⚠️推断（`Missing character` 在本仓**无任何消费方**——全仓 grep 只命中研究报告） |
| E1.5 | 与 XeLaTeX 的**产出等价性口径**（必须先统一"趟数"） | 对同一夹具 `thesis`：① Tectonic 默认档（内部收敛）；② `latexmk -xelatex` 收敛档；③ `xelatex -no-pdf` 单趟（**仅作反例**） | ① 与 ② 页数差 ≤ 1 且文本流差异可逐条解释 | 判据：`pdfinfo` 页数、`pdftotext` 字符数、`diff` 后**每一条差异都能归因**到（包版本 / 收敛趟 / 目录页码），不得出现"整章丢失""空白页"；**③ 不得作为等价性基线**（趟数不同 ⇒ 结论无效） | **P0** | E1.1 | ⚠️部分：§8.4 已有 `28 页 vs 27 页` 的对照，但**该对照的 XeLaTeX 侧是单趟**，属口径不齐；见 §3 证据 E1-③ |

**E1 依据与证据**
- `✅` `<R>/tect-thesis/main.log`：28 个 `[N]`、`Missing character` **0**、`Output written on main.xdv (28 pages, 264728 bytes).`
- `✅` `<R>/tect-thesis/xepdf/main.log`（XeLaTeX 单趟，TL2026）：27 个 `[N]`、**17 条** `LaTeX Warning: Citation 'refN' … undefined on input line 55.` + `There were undefined references.`
- `✅` `<R>/tect-thesis/main.log` 的 LaTeX 版本是 **LaTeX2e <2021-11-15> / ctexbook 2021/12/12 v2.5.8**；XeLaTeX 侧是 **TeX Live 2026 / XeTeX 3.141592653-2.6-0.999998**。⇒ **同一份源码在不同包版本上排版**，页数/警告差异必须先归因到版本，再谈"引擎差异"。
- `✅` Tectonic 的 `main.log` **没有 `This is XeTeX …` 抬头**（grep 计数 0），首行是 `**`，第二行 `(main.tex`。⇒ 任何"从日志识别引擎/版本"的逻辑在 Tectonic 下会失效（T2 需注意）。

### E2 · `--outfmt xdv` 与现有 `xdv.rs` / `xdv-report.mjs` 的兼容

| ID | 目的 | 命令 | 期望 | 判据（可判定） | 优先级 | 依赖 | 是否已验证 |
|---|---|---|---|---|---|---|---|
| E2.1 | 产物是我们认的 XDV | `<T> -C --outfmt xdv --keep-logs -o <P> <FIX>/tect-thesis/main.tex`；然后 `node scripts/xdv-report.mjs <P>/main.xdv` | 工具输出 `pre: version=7 num=25400000 den=473628672 mag=1000`、页数 28、`errors` 空 | **`页数 == 28` 且 `errors == []` 且 pre 三字段与 XeLaTeX 侧一致** | **P0** | — | ✅实测（本轮跑同一文件：28 页、1028/9431/16101、5.6–6.7 ms、37–45 MB/s；§8.4 记录 264,736 B/28 页） |
| E2.2 | **Rust 路径与 JS 工具同解**（产品路径 ≠ 研究工具） | 同一份 `main.xdv` 分别过 `xdv-report.mjs --json` 与 `latteset_core::xdv::page_hashes`（写一个 `#[test]` 或 `cargo run -p` 小探针） | 页数相同；**逐页"是否变化"的判定相同** | 判据：`len(page_hashes) == pages.length` **且** 对任意两份 XDV，`changed_pages()` 结果 == JS `--diff=` 的 `changedPages` | **P0** | E2.1 | ⚠️推断（两套实现**哈希算法不同**：Rust `DefaultHasher` vs JS SHA-1；本项测的是**语义一致**，不是字节相同） |
| E2.3 | `--outfmt xdv` 与 `--outfmt pdf` 的**页数一致** | 同一夹具分别 `--outfmt xdv` 与 `--outfmt pdf`，各 `xdv-report` / `pdfinfo` | 页数相同 | `xdv-report.xdv 页数 == pdfinfo 页数`（偏差 0；不一致即为 P0 缺陷） | **P0** | E2.1 | ⚠️部分：本轮拿到 `main.log` 的 28 页（pdf 档）与 `xdv-report` 的 28 页（xdv 档），但**是两次独立运行**，需同批复验 |
| E2.4 | **页级差分口径**（并且必须用等号形式！） | `node scripts/xdv-report.mjs <P>/a.xdv --diff=<P>/b.xdv` | 输出 `页级差分：…` 行 | 判据：**stdout 含 `页级差分`**（不含即用例无效——空格形式会静默返回摘要，见 §0 尾）；再比 `changedCount` | **P0** | E2.1 | ✅实测（空格形式：无差分、exit 0；等号形式：`A=28/B=27，变化 28 页`） |
| E2.5 | 页哈希缓存**不跨引擎污染** | 在同一项目先跑 XeLaTeX 全量、再跑 Tectonic `--outfmt xdv`，检查 `tmp/` | 两个缓存文件并存 | 判据：`tmp/<stem>.xelatex.pages` 与 `tmp/<stem>.<tectonic 引擎名>.pages` 是两个文件（📖源 `runner.rs:209-211` 已按引擎名分文件）⇒ **Tectonic 的引擎名必须是唯一的 `binary_name()`**，不能复用 `xelatex` | **P0** | T2（引擎命名） | 📖源 + ⚠️推断（`Engine` 枚举当前只有 3 个变体，`binary_name()` 无 `tectonic`） |
| E2.6 | **页哈希稳定性**：同源两次 XDV 逐页一致 | 两次独立运行（`SOURCE_DATE_EPOCH=0`）→ `--diff=` | `内容变化的页：0 页（逐页字节完全一致）` | 判据：`changedCount == 0` **且** 两文件 SHA-256 相同 | **P0** | E3.1 | ✅实测（`d1.xdv` vs `d2.xdv`：0 页变化；三份 XDV SHA-256 全等 `738F96DD…71FEB`，264,728 B） |
| E2.7 | 半成品/截断容忍（编译中被读到） | `node scripts/xdv-report.mjs <P>/main.xdv --truncate-at=8192` | 前缀解出的页**不越界**、无「假完整页」 | 判据：所有 `eopOffset < 8192`；未见到 `eop` 的页不出现在结果里 | P1 | E2.1 | ⚠️推断（该缺陷 2026-09 已修，回归用例在 `xdv.rs` 单测；Tectonic 产物从未跑过 `--truncate-at`） |
| E2.8 | **默认 PDF 形态是否落 `.xdv`**（T2/T3 的输入判据） | ① `<T> -C --keep-logs -o <P> main.tex`；② **同参数再加 `-k`**（验证 `-k` 是不是变量）；③ 同目录换 `--outfmt xdv` 再跑一次 | ① **不产出** `<P>/main.xdv`；② **同样不产出**（`-k` 不是变量）；③ 产出 | 判据（四条）：① ① 与 ② 运行后 `<P>/main.xdv` **均不存在**；② 两次运行的日志里**都没有** ``Writing `main.xdv` `` 行、但**有** `Skipped writing N intermediate files`；③ ③ 运行后存在且 `xdv-report` 解出 28 页；④ 机制依据 = `driver.rs:1985` 的 `mem.files.remove(&tex_xdv_path)`（**不是**"不加 `-k` 就行"） | **P0** | — | ✅实测（**三路证据**）：(a) 队长/T2 实测 **PDF + `-k` 在 `hello` 与 28 页 `tect-thesis` 两个夹具上均无 `.xdv`**（`<R>/integration.md §0.1.1`）；(b) `<R>/tect-thesis/det1.log`/`det2.log`（PDF 档）**无任何 `main.xdv` 提及**，`run-xdv.log`/`detxdv1.log`/`detxdv2.log`（xdv 档）都有 ``Writing `main.xdv` ``；(c) mtime：`main.xdv` 停在 **23:44:01**（xdv 档那次），随后 23:44:03 的 PDF 运行未触碰它 |
| E2.9 | **陈旧 XDV 污染**（写进 T2 的必测项） | 同一 `<P>`：先 `--pass tex`（或 `--outfmt xdv`）造出 `main.xdv`，**再**跑 `--outfmt pdf`，检查最终 PDF | 最终 PDF 必须是 **PDF 档自己**的产出 | 判据：比较"跑过全流程"与"干净目录只跑 PDF 档"的 `main.pdf` SHA-256 —— **必须相等**；不等即命中 `runner.rs:463-466` 记录的覆盖缺陷 | **P0** | E2.8 + T2 | ✅**旁证（本轮新增）**：Tectonic 在 PDF 档**不读盘上的 `main.xdv`**（XDV 全程在内存层，已被 `remove`）⇒ 磁盘上有没有旧 XDV **不影响 Tectonic 自己的产出**：`tect-kcheck-with-k`（同目录里有 `.aux`/`.bbl`/`.toc` 等中间产物）与 `tect-kcheck-no-k`（干净）两次运行**不在同一目录**，但 `main.pdf` SHA-256 **完全相同**。⚠️ 本用例要钉的是**产品侧**（`runner.rs` 会去读 `tmp/<stem>.xdv` 算页哈希）的作废规则，这一点**尚未测** |
| E2.10 | **`--outfmt xdv` ≠ Quick**；真正的 Quick 等价物是 `--pass tex` | 对同一夹具分别跑：① 默认档、② `--outfmt xdv`、③ `--pass tex`、④ `-r 0`；统计 `Running TeX`/`Rerunning TeX` 次数（收敛趟与文档相关，**两个夹具各测**） | ① thesis = 3 趟；② thesis = **3 趟**、hello = **2 趟**（都只是不跑 xdvipdfmx）；③ = **1 趟**（不跑 bibtex、不跑 xdvipdfmx，**XDV 落盘、无 PDF**）；④ = 1 趟 + bibtex，**出 PDF、无 XDV** | 判据：`(Running TeX + Rerunning TeX)` 计数 == 期望趟数；且 ③ 下 `<P>/main.xdv` **存在**、`<P>/main.pdf` **不存在**；④ 下反之（`main.pdf` 存在、`main.xdv` **不存在**）**另加一条产物判据**：③ 的单趟 XDV 页数 **<** 收敛档（本夹具 26 vs 28）且两档页级差分**几乎全不同** ⇒ ③ **必须**由 Full 收敛兜底。**页哈希口径声明（INT-31b/E3.8）**：以下所有页级比较均为**原始口径**（`bop..eop`，含 `prev`），跨档比较**必须在同一口径下**，且不得把 `prev` 连锁读成"内容变了" | **P0** | E2.8 | ✅实测（① ② ④ 的 thesis 趟数：7 份日志全部 **3 趟 TeX + 9 次 BibTeX**，xdv 档 xdvipdfmx=0、pdf 档 =1）；✅ T2 复核（③ = 1 趟/XDV 落盘/无 PDF；`--outfmt xdv` 在 hello 上 **2 趟**；`-r 0` = 1 趟/出 PDF/无 XDV）；✅**本轮实测 ③**：`_t1-probe/passtex` 产出 `main.xdv`(242,960 B)+`main.synctex.gz`+`main.log`、**无 PDF**，单趟 **26 页**，与收敛档差分 **27/28 页不同**（原始口径，见上）；📖源 `--pass tex` 分支只调 `tex_pass`（driver.rs:1492-1503），不调 `xdvipdfmx_pass`（后者只在 `default_pass` 里，driver.rs:1754-1758） |
| E2.11 | **PDF 档的"无页哈希"语义**：`pages == 0` 是**无法判定**，不是"零页变化"；且**PDF 档与 xdv 档的页哈希不可跨档混比** | 走产品链路：PDF 档编译后读 `tmp/<stem>.<engine>.pages` 与 `ChangedPages` 事件 | `pages == 0` → 前端**保守全量刷新**；任何"跳过重载/跳过转换"都**不得**触发 | 判据：① `CompileOutcome::Success.page_hashes` 在 PDF 档为空；② `ChangedPages` 不发出"零页变化"信号；③ 若把 xdv 档产出的哈希当基线去比 PDF 档 ⇒ **必须判为不同**（不得因为都是 28 页就认为"逐页相同"） | **P0** | E2.8 + T2 | 📖源 本仓 `xdv.rs:216-233`（`cur` 为空 = 无法判定，返回空表；**不要伪造页号**）+ `runner.rs:426-438`（读不到即退化全量刷新）+ T2 已同步 |
| E2.12 | **路线② 可行性**：外部 `xdvipdfmx`（TL 自带）能否吃 Tectonic 的 XDV（含字体解析） | ① `node scripts/xdv-report.mjs <tectonic.xdv> --opcodes`（看 `define_native_font` 的**字体名形态**）；② `xdvipdfmx -q -o <P>/out.pdf <tectonic.xdv>`；③ `pdffonts` + `pdftotext` 与 Tectonic 自带 PDF 对拍 | 转换 exit 0、页数与字体组一致、文本等价 | 判据（四条）：① exit 0 且产出 PDF；② `pdfinfo` 页数 == **28**；③ 嵌入字体组与 Tectonic 自带 PDF **相同**（FandolSong/Kai + LMRoman，**无替换字体**）；④ `pdftotext` **去掉所有空白后逐字符相同**（否则判不等价） | **P0** | T2（D2 路线②） | ✅**已实测（本轮）**：exit **0**；28 页；字体组一致（子集前缀不同属正常）；**去空白后 13,559 字符完全相同**；字节不同（**79,070 vs 80,203 B**）但归因于转换器版本（`Producer: xdvipdfmx (0.1)` vs `(20260113)`）⇒ **引擎层可行**。**⚠️ 但它不是"免装 TL"路线**——见「E2 依据与证据」的字体解析链 |
| E2.13 | **Tectonic XDV 的字体是"裸名"，路线② 依赖 TL 的字体树**（E2.12 的限定条件） | `kpsewhich FandolSong-Regular.otf` / `FandolKai-Regular.otf`；再把 Tectonic XDV 的字体名与 XeLaTeX XDV 的做对照 | Tectonic 记为裸名；解析靠 TL | 判据：① Tectonic 的 `define_native_font` 名 == `FandolSong-Regular.otf` / `lmroman12-regular`（**无路径、lmroman 连扩展名都没有**）；② XeLaTeX 侧是**绝对路径**（`C:/Windows/fonts/simsun.ttc`、`c:/texlive/…/lmroman12-regular.otf`）；③ `kpsewhich FandolSong-Regular.otf` **命中且在 `c:/texlive/2026/texmf-dist/…`** ⇒ 外部转换成功靠的是 **TeX Live 的字体树 + kpathsea**，同时 `xdvipdfmx.exe` 本身也来自 TL | **P0** | E2.12 + T2/T4 | ✅实测（三类字体名 + `kpsewhich` 全命中 TL 路径；`lmroman12-regular` kpathsea 未直接命中但由 TL 的字体映射解析成功）。⇒ **路线② 同时需要 `xdvipdfmx.exe` 与 TL 字体树 ⇒ 不满足"免装 TeX Live"**（与 G2 的 synctex 是同一类问题） |

**E2 依据与证据**
- `✅` `xdv-report` on `<R>/tect-thesis/main.xdv`：28 页，页字节 min/avg/max = **1028/9431/16101**；XeLaTeX 单趟 `<R>/tect-thesis/xetmp/main.xdv`：27 页、**1062/9658/16101**。⇒ 页字节分布不同（趟数/包版本不同），**不是**格式不兼容。
- ✅ 与 T3 对齐的口径结论：**PDF 档与 xdv 档的页哈希不可跨档混比**（不满足"同强度、同收敛状态"）；两档唯一差别是 `xdvipdfmx` 那一步，因此"PDF 后端成本"的正确测法是同引擎 **`默认档 − --outfmt xdv`**，而不是拿 `tectonic` 默认档去比 `xelatex -no-pdf`（后者趟数不同，§8.3 的 ~800 ms 读法已按此撤回）。
- ✅ **E2.12/E2.13 的实证链（本轮新增，`<R>/_t1-probe/`）**：
  1. 字体名形态：Tectonic 的 `define_native_font` 记 **`FandolSong-Regular.otf` / `FandolSong-Bold.otf` / `FandolKai-Regular.otf` / `lmroman10-regular` / `lmroman12-regular`**（裸名，lmroman 无扩展名）；XeLaTeX 侧记 **`C:/Windows/fonts/simsun.ttc`、`c:/texlive/2026/texmf-dist/fonts/opentype/public/lm/lmroman12-regular.otf`**（绝对路径）。
  2. 外部转换：`xdvipdfmx -q -o out.pdf main.xdv` → **exit 0**，28 页，嵌入 `FandolSong-Regular/Bold`、`FandolKai-Regular`、`LMRoman10/12-Regular/Bold`（**与 Tectonic 自带 PDF 同一组，无替换字体**）。
  3. 等价性：两份 PDF 的 `pdftotext` 输出**去掉全部空白后逐字符相同**（各 13,559 字符）；字节数不同（79,070 vs 80,203 B）只因 `Producer` 版本不同（`xdvipdfmx (0.1)` vs `xdvipdfmx (20260113)`）。
  4. **但解析链依赖 TL**：`kpsewhich FandolSong-Regular.otf` ⇒ `c:/texlive/2026/texmf-dist/fonts/opentype/public/fandol/FandolSong-Regular.otf`（命中且**在 TL 里**）；`FandolKai-Regular.otf`、`simsun.ttc` 同样命中 TL/Windows。⇒ **没有 TL 字体树的机器上，外部 xdvipdfmx 无处解析这些裸名**；何况 `xdvipdfmx.exe` 本身就随 TL 分发。**结论：路线② = "需要 TL" 的路线，不是"免装 TL"的路线**。
- `✅` `<R>/tect-thesis/d1.xdv` == `d2.xdv` == `main.xdv`（SHA-256 `738F96DDE8699684E165D7839A4B5167C930505FC11DB5916ADD4C395C971FEB`）；`d1.pdf` == `d2.pdf` == `main.pdf`（`926683A0…7F9EE0`，80,203 B）。
- `📖源` `--outfmt` 取值 `aux|html|xdv|pdf|fmt`（driver.rs:134-146）；`--outfmt xdv` 时**不跑 xdvipdfmx**（`default_pass` 只在 `OutputFormat::Pdf` 时调 `xdvipdfmx_pass`，driver.rs:1754-1758）——`✅` 佐证：`<R>/tect-thesis/detxdv1.log` 里没有 `Running xdvipdfmx` 行。
- `📖源` `xdv.rs` 与 `xdv-report.mjs` 的指令长度表**逐条一致**，且都以"见到 EOP"为页完整唯一判据（`xdv.rs:207-210`、`xdv-report.mjs:204-209`）。
- `📖源`（**E2.8/E2.9 的机制**，三条链在一起才成立）：
  1. `write_files` 的跳过条件（driver.rs:1615-1623）：`!keep_intermediates && (access_pattern != Written || 扩展名 ∈ ALWAYS_INTERMEDIATE_EXTENSIONS)`。**决定项是前半句**——`.xdv` 不在 `ALWAYS_INTERMEDIATE_EXTENSIONS`（driver.rs:1345-1347，只有 `.snm`/`.toc`）里，所以只看访问模式。
  2. 访问模式演进（driver.rs:59-77、487-508、555-590）：引擎写文件 → `Written`；随后**被读** → `WrittenThenRead`；一旦进入 `ReadThenWritten`/`WrittenThenRead` **不再变化**。
  3. ⇒ **PDF 档**：TeX 写 `main.xdv`，内部 xdvipdfmx 再把它读走 ⇒ `WrittenThenRead` ⇒ **跳过**；**xdv 档 / `--pass tex`**：只写不读 ⇒ `Written` ⇒ **落盘**。
     （`FilesystemIo` 是 `writes_allowed=false`（见 G1），所以任何"读"都必须过 `BridgeState`，一定被记账。）
- `✅` 趟数与后端（**7 份日志全量统计**，`Running TeX`+`Rerunning TeX` / `Running BibTeX` / `Running xdvipdfmx`）：
  `det1=3/9/1`、`det2=3/9/1`、`sync=3/9/1`、`run1=3/9/1`（PDF 档）；`detxdv1=3/9/0`、`detxdv2=3/9/0`、`run-xdv=3/9/0`（xdv 档）。
  ⇒ ① **`--outfmt xdv` 不是 Quick**（照样 3 趟 + 9 次 BibTeX，只是省掉 xdvipdfmx）；② 本夹具上 Tectonic 默认档 = **3 趟**。

### E3 · 确定性（`SOURCE_DATE_EPOCH`）

| ID | 目的 | 命令 | 期望 | 判据（可判定） | 优先级 | 依赖 | 是否已验证 |
|---|---|---|---|---|---|---|---|
| E3.1 | PDF/XDV 逐字节可复现 | 连跑两次：`$env:SOURCE_DATE_EPOCH=0; <T> -C --outfmt pdf --synctex --keep-logs -o <P1> main.tex` → `<P2>` | 两个 PDF 与两个 XDV 逐字节一致 | `Get-FileHash -Algorithm SHA256` 两次 PDF 相等 **且** 两次 XDV 相等 | **P0** | — | ✅实测（§8.4：80,203 B / 264,728 B，SHA-256 相同；本轮独立复核 d1/d2 全等） |
| E3.2 | **确定性 + `--keep-logs` 也不引入抖动** | 同 E3.1，但 `--keep-logs`，比较 `main.log` | 日志允许**有差异**（含路径/时间），产物必须一致 | 判据：`.pdf`/`.xdv` SHA-256 相等；`.log` 差异必须逐条落在 `Input:` 绝对路径或下载顺序上 | P1 | E3.1 | ⚠️推断 |
| E3.3 | **确定性 + SyncTeX 的冲突**（已知不兼容） | `$env:SOURCE_DATE_EPOCH=0; <T> -C --outfmt pdf --synctex -Z deterministic-mode …` | 预期 `main.synctex.gz` **不可用**（Tectonic 自述：`-Z deterministic-mode` 会破坏 synctex，因为它依赖绝对路径） | 判据：分别生成 `-Z deterministic-mode` 开/关两份 synctex，解压后比 `Input:` 行 ⇒ 开档时相对/空路径 ⇒ **产品决策：不得同时要求"逐字节确定"与"SyncTeX 可用"**，写进 T2 | **P0** | E4.1 | 📖源 `tectonic -Zhelp` 原文：「deterministic mode breaks SyncTeX's auxiliary files as they include and rely on absolute file paths」 |
| E3.4 | 边界：**首次未收敛**（无 `.aux` 的干净目录） | 在空 `<P>` 里跑一次（不喂上次 `.aux`） | 仍收敛并出 PDF/XDV | 判据：`main.log` 页数 == 稳态页数；`main.aux` 存在；再跑第二次 `--diff=` **0 页变化** | **P0** | E3.1 | ⚠️推断（"首次"这一步正是产品 Quick/Full 升级判据的输入，`runner.rs:293-298`） |
| E3.5 | 边界：**bibtex 参与时**（本夹具必触发） | `<T> -C --outfmt pdf --keep-logs -o <P> main.tex` 两次 | bibtex 参与时仍可复现 | 判据：两次 PDF SHA-256 相等。**但注意**：本夹具的 bib 是**成功**的（E6.1），故该用例同时覆盖"bibtex 参与下的确定性"；`refs.bib` 换成会失败的样本时须重跑 | **P0** | E6.2 | ⚠️推断（未在"bib 成功 + 两次独立运行"下测过确定性；d1/d2 那次 **是** bib 参与的，已全等） |
| E3.6 | **确定性的代价：`SOURCE_DATE_EPOCH` 会改文档"印出来的日期"**（引擎行为差异，直接命中产品） | 同一夹具两次：① `SOURCE_DATE_EPOCH=0` ② **不设**；各跑 `-C --outfmt pdf -o <P> main.tex`，再 `pdftotext` 取日期行 | Tectonic：① → `1970 年 1 月 1 日`，② → 构建当天；**XeLaTeX：两者都是构建当天** | 判据（三条）：① 设 epoch 的 PDF **文本**含 `1970 年 1 月 1 日`；② 两份 PDF 去空白文本**唯一差异就是日期**（首个差异落在日期处）；③ **XeLaTeX 对照两份日期相同** ⇒ 差异是**引擎级**、不是夹具级 | **P0** | E3.1 + **T2/D4** | ✅**实测（本轮复核 T2 的发现）**：`_t2-probe/r2/witdate`（设 epoch）→ `1970 年 1 月 1 日`、**80,203 B**；`_t2-probe/r2/nodate`（不设）→ `2026 年 9 月 14 日`、**80,509 B**；去空白文本 13,559 vs 13,560 字符、**首个差异 @18 正是日期**；XeLaTeX 侧 `<R>/tect-thesis/xepdf/main.pdf` 印 `2026 年 9 月 13 日`（同样设了 epoch=0）⇒ **XeTeX 不受影响**，与 `runner.rs:273-274` 的既有注释一致 |
| E3.7 | **D4 三选项的引擎层代价**（给队长裁决用；本项是 E3.6 的量化） | ① **不设 epoch** 各连跑两次：`-C --outfmt pdf` 与 `-C --outfmt xdv`，分别比 SHA-256；② **仅 epoch 不同**的两次 `--pass tex --keep-logs`（其余参数全同）比**逐页字节**；③ 根因 = `bop` 末尾 4 B 的 `prev`（前一页 `bop` 绝对偏移） | (b) 的代价**只有两条**（PDF 元数据 + 跨天日期）；XDV 逐字节稳定 | 判据：① 不设 epoch 时两次 **XDV 的 SHA-256 相等**（⇒ 页哈希源稳定，(b) **不**摧毁 A/B/C），而两次 **PDF 的 SHA-256 不等**（差异仅在 `/ID`/Info 等元数据，字节数相同、`pdftotext` 相同）；② 仅 epoch 不同时，**页 1 变长 10 B（78 处差异）**、**页 2 逐字节相同**、**页 3–26 长度全同、差异只落在页内偏移 43–44**（= `prev` 的低字节，`prevB − prevA = +10`）⇒ 机制 = **`prev` 绝对偏移连锁**，不是"每页内容都变了"；③ 设 epoch 后两次 PDF SHA-256 相等 | **P0** | E3.1 + E3.6 + `docs/modules.md` 已知债 **#26** + **队长 D4** | ✅**实测（`<R>/_t1-probe/d4/`，本轮 + 队长复算）**：① `xdv-noep-1`/`xdv-noep-2`（不设 epoch）→ XDV **SHA-256 相同**（`63531F94…4BAC`，264,736 B）；`noep-1`/`noep-2` → PDF **不同**（`92240605…` vs `F766FACC…`，各 80,509 B、文本相同）。② 逐页对拍结果：页1 `1028→1038`（+10 B，78 处，落点 793…）、页2 **0 处**、页3–26 **各 1–2 处且只在偏移 43/44**，`prev` 从 1051→1061、1469→1479、…、224114→224124（**每页 +10**）⇒ **与已知债 #26 记录的样本逐项一致** |
| E3.8 | **页哈希口径必须声明**（E3.7 ② 的接口影响） | 读 `xdv.rs:211`；对同一份 XDV 分别算"原始（含 `bop` 的 45 B）"与"归一化（`bop+45..`）"两套哈希 | 现实现 = **原始口径** | 判据：产品必须**显式声明**采用哪种口径；**原始口径**下"任何一页变长 ⇒ 其后所有页的 `prev` 连锁变化 ⇒ 被判为变化"（本夹具 25/26 页），**归一化口径**可把该连锁收窄到"真正变化的那一页" | **P0** | E3.7 + **T2/T5**（B/C 精度） | 📖源 `xdv.rs:211` `hash_page(&bytes[bop..p])`——`bop` 是页起点，**含 45 B 页头与 `prev`**；与 `docs/modules.md` 已知债 **#26** 记载一致（候选修法：改 `bytes[bop+45..p]`，收益/风险未量化 ⇒ **未修**） |

| E3.9 | **"确定性由谁承担"：外部 `xdvipdfmx` 那一步是否可复现**（路线②/INT-20b 的前提） | 同一份 XDV → **同一个 `-o` 输出路径**，分两组各跑多次：① `SOURCE_DATE_EPOCH=0` ② 不设；逐次比 SHA-256。**另测 XDV 侧是否也受输出路径影响**（同源 + epoch=0，输出目录名长度差 30+ 字符） | ① 组**逐次相同**；② 组**逐次不同**；**XDV 与输出路径无关** | 判据：① `epoch=0` 组三次 SHA-256 **全等**（⇒ "㉚ 的逐字节确定性由转换步骤承担"**成立**）；② 不设组两次 **不等**（⇒ 转换步必须显式设 epoch）；③ **"同路径"纪律只对转换步成立**：转换产物**依赖 `-o` 路径**，**XDV 不依赖**（不同长度输出目录 → 逐字节相同）⇒ 别把这条纪律无差别套到 XDV 比较上 | **P0** | E3.7 + E2.12 + **T2/INT-20b·INT-36④** | ✅**实测（`<R>/_t1-probe/d4/conv-*` 与 `d4/x*`）**：`epoch=0` 连续 3 次 → **79,065 B / `6F7EFC14…` 三次全等**；不设 → **79,073 B** 但 `DF49B482…` vs `C4A86ED1…` **不等**；**XDV 侧**：`-o …/x` vs `-o …/xdv-with-a-much-longer-output-dir-name` → **264,728 B / `738F96DD…71FEB` 完全一致**。⚠️ **方法论坑（我踩过一次）**：首轮让两次跑写**不同目录**再比字节，size 在 79,059–79,071 间漂、看起来像"设了 epoch 也不确定"——实为**输出路径被写进转换产物**。⇒ **转换步**的确定性用例必须固定输出路径 |
| E3.10 | **D4 裁定后的分步口径**（回应 T2 的 INT-20b，供 T5 引用） | 读 `runner.rs:275`（现状无条件）与 `523-548`（`convert_xdv`） | 按**子进程**分别设 epoch，而不是"整个编译设/不设" | 判据（三条，逐进程声明）：① **Tectonic 进程不设** epoch（裁定为 (b)）⇒ 正文日期 = 当天，**XDV 逐字节稳定**，**PDF 不保证**；② **外部 `xdvipdfmx` 进程设 `0`**（仅路线②）⇒ 该步产物逐字节确定（E3.9 ① 已证）；③ 断言边界：**XDV 逐字节相等**可断言、**正文日期 = 当天**可断言、**PDF 逐字节相等不得断言**（实测差异 56 B/80,509 B ≈ 0.07%，仅在 `/ID`+Info，长度与文本相同） | **P0** | E3.7 + E3.9 + **T2/D4 裁决** | ✅实测支撑：① XDV 不设 epoch 两次全等（`63531F94…`）；② 转换步 `epoch=0` 三次全等（`6F7EFC14…`）；③ PDF 不设 epoch 两次不等（`92240605…` vs `F766FACC…`，各 80,509 B、文本相同）。📖源 现状 `runner.rs:275` 对 Tectonic 与转换步**都是无条件设** ⇒ 需按 INT-20b 拆开 |

**E3 依据与证据**：`✅` §8.4「构建确定性（㉚）：`SOURCE_DATE_EPOCH=0` 连跑两次，PDF 与 XDV 都逐字节一致」+ 本轮 SHA-256 复核。`📖源` 产品侧本来就固定 `SOURCE_DATE_EPOCH=0`（`runner.rs:275-280`）。
`📖源` **`build_date` 的来源（E3.6/E3.7 的根因）**：`sess_builder.build_date_from_env(deterministic_mode)`（`src/bin/tectonic/compile.rs:206`）→ `driver.rs:995-1010`：设了 `SOURCE_DATE_EPOCH` 就用它，**否则 `(false, None) => SystemTime::now()`**。该日期同时喂给 `TexEngine`（`driver.rs:1880`，驱动 `\today`）与 `XdvipdfmxEngine`（`driver.rs:1974`，驱动 PDF `/ID`）⇒ **影响的是"正文日期"与"PDF 元数据"，而 XDV 的类型排结果不含 build_date**（这解释了 ① 的 XDV 稳定）。
`⚠️` **确定性用例的两条硬纪律**（E3.9 实测暴露）：① **转换步必须固定输出路径**（`-o` 路径会进**转换产物**；XDV 侧不受影响）；② **必须固定 `SOURCE_DATE_EPOCH`**（跨天/跨进程都不行）。两条都属"不写就得到假阴性"的坑。
`✅` **`--synctex` 对 XDV 字节零影响**（本轮实测）：`--pass tex --synctex --keep-logs` 与 `--pass tex --keep-logs` 的 XDV **逐字节相同**（242,960 B / `08E7219F…`）；且带 `--synctex` 的 XDV 里**搜不到源码绝对路径**（`tect-thesis` 未命中）⇒ 页哈希与 SyncTeX **互不污染**，两者可同时开启。（我此前一度把"`--synctex` 会逐页加 special"当作两次运行不可比的理由，**该说法错误，已撤回**；当时的对照因`--synctex`无影响而本已有效，重跑仍得同一结果 25/26 页。）
`⚠️` **E3.6 的产品含义（必须交 T2/T5，别把㉚当成零成本）**：XeLaTeX 下固定 epoch 只动 PDF 的 `/ID`（**内容不变**，所以当年判定为"免费"）；**Tectonic 下它会把 `\today` 冻成 1970-01-01**，即**用户可见的正文改动**。而 `compile_command` 是**无条件**设这个环境变量的（`runner.rs:275`，Quick/Full 与引擎无关）⇒ 若 `Engine::Tectonic` 复用该命令构造而不加分支，**所有用 `\today` 的文档都会印 1970 年**。三条备选（接受 1970 / 不固定 epoch / 只在需要页哈希的档固定）各有权衡，已由 T2 挂 **D4** 待裁。
`⚠️` **E3.7 给 D4 的引擎层结论（已按队长复算更正，(b) 的代价比我初版写的轻）**：
- **(a) 接受 1970**：唯一保住"输出逐字节确定（PDF+XDV）"的选项；代价 = 正文日期。
- **(b) 不固定 epoch**：代价**只有两条** —— (i) **PDF 档放弃逐字节确定性**（差异仅在 `/ID`/Info 元数据；同日两次运行 PDF 不同而 **XDV 逐字节相同**）；(ii) **会印 `\today` 的文档在跨天时污染页哈希**（页 1 + 因 `prev` 连锁的页 3..N，页 2 不变）。**我初版写的"B/C 退化为每次都全量刷新"是错的，已撤回**——B/C 的输入是 XDV，而不设 epoch 时 XDV 稳定。
- **(c) 只在页哈希档固定**：Quick 与 Full 的日期政策不同 ⇒ 若某文档确实印日期，两档产物分叉（页 1 及 `prev` 连锁页）；对不印日期的文档无影响。
`⚠️` **与已知债 #26 的关系（重要框架）**：`prev` 连锁**不是 Tectonic 新问题**，而是 DVI/XDV 的固有性质，早已登记为 `docs/modules.md` 已知债 **#26**（连"日期串差一位数字 ⇒ 页 1 +10 B、页 2 相同、页 3–26 差异只在偏移 43–44"这个样本都同款）。差别在**触发频率**：XeLaTeX 侧因为固定 epoch，日期永不变 ⇒ 该债几乎不触发；**Tectonic 选 (b) 会把它从"理论问题"变成"每天发生一次"**（跨天首编多刷若干页）。归一化页哈希口径（E3.8）可把影响收窄到真正变化的那一页。

### E4 · SyncTeX（产物口径 + 与 CLI forward/inverse 的对拍）

| ID | 目的 | 命令 | 期望 | 判据（可判定） | 优先级 | 依赖 | 是否已验证 |
|---|---|---|---|---|---|---|---|
| E4.1 | 文件名与位置 | `<T> -C --synctex --keep-logs --outfmt pdf -o <P> main.tex` | `<P>/main.synctex.gz` | 判据：文件存在；大小 > 0；gzip 可解 | **P0** | — | ✅实测（77,983 B；解压 18,582 行 / 508,271 字符） |
| E4.2 | 内容口径（我们得吃得下） | 解压取头部 | `SyncTeX Version:1` / `Output:pdf` / `Magnification:1000` / `Unit:1` / `Content:` / `Postamble:`+`Count:` | 判据（逐条）：① 首行 == `SyncTeX Version:1`；② **`Input:1:` 是绝对 Windows 路径**；③ 子文件形如 `…\tect-thesis\chapters/ch01.tex`——**混用 `\` 与 `/`**；④ **存在 132 行空 `Input:N:`**（`Input:2..123` 等）；⑤ `!` 记录 58 条 | **P0** | E4.1 | ✅实测（全部四条已在本轮读出：非空 Input 共 9 条 = 1 主文件 + ch01..ch08；空 Input **132** 条；`!NNN` **58** 条） |
| E4.3 | **对拍方法（只列判据，不实现）** | 对同一夹具分别产出 Tectonic 与 XeLaTeX 的 synctex，然后各跑我们 CLI：`latteset-cli --project <proj> forward chapters/ch01.tex <行>` / `inverse <页> --x <X> --y <Y>` | 两端**命中同一源码位置**（行号允许 ±小偏差） | 判据（三条全中才算通过）：① 对 8 章各抽 3 个"章标题行"，`forward` 返回的 `page` 与 `pdfinfo` 实际页**一致**；② 用 ① 的 `(page,x,y)` 做 `inverse`，返回 `source` **落在同一子文件**且行号 `|Δ| ≤ 2`；③ 每个子文件至少 1 次命中（不得全部落回主文件或"已回落"提示） | **P0** | E4.2 + **G2 未解则本条不可执行** | ⚠️从未测（§3 摩擦点 #4 明说"只验到有产物"） |
| E4.4 | **`-d` 目录参数在当前实现里是硬编码 `tmp/`** | 读 `synctex.rs:29-31`；探针：把 synctex 产物放到 `-o` 指定目录后调用 CLI | 期望失败（现状） | 判据：`-d` 指向的目录**必须来自运行期 outdir**，而不是常量 `"tmp"`；用例：outdir=`<P>` 时 `forward` 成功率 == 100%（现状应为 0% 或报错） | **P0** | T2 | 📖源（`fn synctex_dir(pdf) = pdf.parent()/tmp`） |
| E4.5 | `--synctex` 在**非 PDF 档**下的产物与 `Output:` 字段 | `<T> -C --pass tex --synctex --keep-logs -o <P> main.tex`（该档**不产出 PDF**） | `.synctex.gz` 仍产出；`Output:` 字段**恒为 `pdf`** | 判据：① 文件存在且首行 `SyncTeX Version:1`；② **`Output:` == `pdf`，与是否真的产出 PDF 无关** ⇒ **该字段不可用作任何判据**（不要拿它决定 `synctex view -o` 传什么，也不要写进 INT-68/69 的分支） | P1 | E4.1 | ✅**已实测（本轮）**：`--pass tex --synctex` 档产出 `main.xdv`(242,960 B) + `main.synctex.gz`(69,500 B) + `main.log`、**无 `main.pdf`**，而头部仍写 `Output:pdf`；默认档（出 PDF）同为 `Output:pdf` ⇒ **常量**。另：该档 `Input:1:` 末段用 `/` 分隔（`…\tect-thesis/main.tex`），空 `Input:` 121 条、总 15,808 行 |

**E4 依据与证据**：`✅` 本轮解压 `<R>/tect-thesis/main.synctex.gz` 全文；`📖源` `crates/` 26 个 crate 中**没有 synctex**；`✅` 本机 `synctex.exe` 来自 `C:\texlive\2026\bin\windows`（v1.5）——即 G2。

### E5 · 日志与进度（`--keep-logs` / `note:` / `--chatter`）

| ID | 目的 | 命令 | 期望 | 判据（可判定） | 优先级 | 依赖 | 是否已验证 |
|---|---|---|---|---|---|---|---|
| E5.1 | `main.log` 能否被 `parse_log` / `pages_typeset` **直接吃** | 拿现成 `<R>/tect-thesis/main.log` 跑 `parse_log`（`latteset-cli compile` 的终态路径）与 `pages_typeset` | 能。返回页数 = 28 | 判据：`pages_typeset(log) == Some(28)`；`parse_log(log)` 不 panic；`len(messages) == 0`（该夹具无错无警） | **P0** | — | ✅实测（本轮统计：28 个 `[N]`、0 个 `!`、0 个 `l.N`、0 个 `Warning`）；📖源 `progress.rs:13-24` 的正则容忍 `max_print_line` 折行 |
| E5.2 | **日志只在结束后落盘** ⇒ 实时反馈主来源失效 | 编译期间（大夹具）每 200 ms 轮询 `<P>/main.log` 是否存在/增长 | 期望：**整个编译期间文件不存在**，结束瞬间一次出现完整内容 | 判据：`main.log` 首次 `Length > 0` 的时刻 `≥` 进程退出时刻 − 0.5 s；即"用 `.log` 尾随拿进度"在本模式下**命中 0 次** | **P0** | — | 📖源（`FilesystemIo writes_allowed=false` + `write_files` 只在 driver.rs:1506/1528 调用）+ ✅旁证（`<R>/tect-thesis/detxdv1.log` 里 `Writing 'main.xdv'` 出现在整个日志**倒数第 3 行**，即所有编译动作之后） |
| E5.3 | `note:` 到「编译中错误 / 页进度」的映射 | `<T> -C --keep-logs --outfmt pdf -o <P> main.tex`，抓 stdout+stderr | `note:` → stdout；`warning:`/`error:` → stderr；**无 `[N]`** | 判据：① 分开抓 stdout/stderr，`note:` 行只在 stdout、`warning:` 行只在 stderr；② stdout 里 `[N]` 计数 == **0**（不带 `-p`）；③ 带 `-p` 时 stdout 出现 `[N]`，计数 == 28 | **P0** | — | 📖源 `plain.rs:52-58` / `termcolor.rs:70-78`（note→stdout，warning/error→stderr）；📖源 `-p` → `GenuineStdoutIo` → 真 stdout（driver.rs:1168-1172、io_base/src/stdstreams.rs:29-34）；✅旁证 `<R>/tect-thesis/run1.log` 115 行**全是 `note:`/`warning:`**，无一个 `[N]` |
| E5.4 | `--chatter` 档位的实际可见面 | `--chatter minimal` / `--chatter default` 各跑一次 | `minimal` 只压 **Note**，Warning/Error 保留 | 判据：两次运行下 `warning:` 行数**相等且 > 0**（用 E6.1 的失败夹具）；`note:` 行数在 minimal 下 == 0 | **P0** | E6.1 | 📖源 `status_base/src/lib.rs:34-51`（`Minimal` 仅 `kind == Note` 被抑制）；✅实测取值集只有 `default`/`minimal`（`--chatter bogus` → `error: invalid value 'bogus' … unsupported or unknown chatter level`，**exit 2**） |
| E5.5 | **`-p` 是页进度唯一来源**，且**多趟会从 1 重数** | `<T> -C -p --keep-logs -o <P> main.tex`（28 页夹具），抓 stdout 里的 `[N]` 序列 | `[N]` 逐块到达（非一次性 dump），且序列**在每趟开头回到 `[1]`** | 判据（三条）：① 首个 `[N]` 显著早于进程退出，末 3 个 `[N]` 到达时间方差 > 0（流式，非 dump）；② 序列形如 `[1] [1] [1] …`（3 趟各从头数），**总计数 > 页数**（T2 实测 **110 个**）；③ ⇒ `PageMarkerScanner` 已有的"只升不降"语义（`progress.rs:64-73`）**恰好正确**，但**UI 显示的页数会停在当前趟的最大值**，跨趟不对累加——必须由 T2 明确"页进度 = 当前趟页数"还是"总进度" | **P0** | E2.10 + T2 | ✅ T2 实测（`-p` 下 28 页档 stdout 有 **110 个 `[N]`**，前三个是 `[1] [1] [1]`，见 `<R>/integration.md §0.1.1`）+ 📖源 `runner.rs:122-132`（管道逐行喂）与 `progress.rs:55-74`（只升不降）；⚠️"逐字节流式"的到达时间证据仍待补 |

**E5 依据与证据**：`✅` `<R>/tect-thesis/main.log` 22,861 B、28 个 `[N]`、0 错 0 警、尾部 `Output written on main.xdv (28 pages, 264728 bytes).`。`✅` `<R>/tect-thesis/run1.log` / `sync.log`：`note: Running TeX ...`、`note: Rerunning TeX because "main.aux" changed ...`、`note: Running xdvipdfmx ...`、`note: Writing 'main.pdf' (78.7060546875 KiB)`、`note: Skipped writing 11 intermediate files`、以及 8 条 `warning: errors were issued by BibTeX, but were ignored; use --print and/or --keep-logs for details.`。

### E6 · 多趟收敛与 bibtex/biber

| ID | 目的 | 命令 | 期望 | 判据（可判定） | 优先级 | 依赖 | 是否已验证 |
|---|---|---|---|---|---|---|---|
| E6.1 | **主 bibtex 调用成功**的证实（⚠️ **本项已按产物级证据反转，取代原"bibtex 失败被吞"**） | 干净目录（先删后建）跑一次：`<T> -C -k --keep-logs --outfmt pdf -o <P> <FIX>/tect-thesis/main.tex` | 主 bibtex **成功**：`<P>/main.bbl` 存在且条目数 == 预期引用数 | 判据（**必须落在产物上，不得用 `.blg`**）：① `<P>/main.bbl` 含 `\bibitem` 条数 **== 9**（`ref1..ref9`）；② `<P>/main.pdf` 的 `pdftotext` 含 `参考文献` 且含 `[1]`…`[9]`；③ 无 `-k` 时退化为 `<P>/main.log` 最终趟 `Citation … undefined` 计数 **== 0**（对照 XeLaTeX 单趟基线 **17** 条，判据有区分度） | **P0** | — | ✅实测（`<R>/tect-kcheck-with-k`）：`main.bbl` **680 B / 9 条 `\bibitem` / 18 条 `\newblock`**；`run.log` 有 `note: Writing \`main.bbl\` (680 B)`；`main.pdf` 80,203 B（与 no-`k` 档 **SHA-256 完全相同**）、28 页、`main.log` `LaTeX Warning` **0**。**更强的一条（队长/perf-method 复核，本册复算）**：该 `.bbl` 与两条 XeLaTeX 基线 `_zhcmp/thesis/tmp/main.bbl`、`bench/thesis/tmp/main.bbl` **三份 SHA-256 完全相同**（`4370D1681F352FA1E934809DBAFC6643B226F0CC6EF442F2E54D48ED69AA4FBF`，各 680 B / `\bibitem`×9 / `\newblock`×18）⇒ **bib 产出与 XeLaTeX 逐字节等价**，比"条数相等"强得多 |
| E6.2 | bundle 内 BibTeX 版本与容量（**降级为风险项，不再是已发生的失败**） | 读两份 `.blg` 首三行 | 0.99d `max_strings=35307 hash_size=35307` vs 本机 0.99e `max_strings=200000` | 判据：记录两侧版本与容量；**结论只能是"容量上限低约 5.7×，大 `.bib`/多条目时有耗尽风险"**——本夹具**未**触发（9 条目通过）。⇒ 若要证伪，需造一个 string 数 >35307 的 `.bib` 并观察容量错误文案 | P1 | E6.1 | ✅实测（两文件首行已读出）；⚠️ 风险未经触发验证 |
| E6.2b | **"bibtex 是否成功"的判定器本身要测**（本轮实测出三个方向的反例） | 对两侧 `.blg` + 产物跑候选规则：① 计 `error messages` 行；② 计 `warning` 行；③ 查摘要行 `You've used N entries`；④ **查产物**（`.bbl` 条目数 / PDF 文本 / `Citation … undefined` 计数） | 只有 ④ 可用 | 判据：① Tectonic 侧**恒错**（8 份章级空跑判失败 + 成功的主调用判通过）；② 无区分度；③ **Tectonic 侧恒假阴性**——该字符串在源码与二进制里都不存在（G6），**成功也没有**；④ 正确。⇒ **回归样本必须包含"成功但无摘要行的 `.blg`"这种负例，要求判定器输出 `unknown`，不得输出 `fail`** | **P0** | E6.1 + G6 + T2 | ✅实测（`<R>/tect-kcheck-with-k`：`.blg` 537 B 无摘要行、而 `.bbl` 680 B/9 条 `\bibitem`、PDF 28 页有 `参考文献`）；✅二进制字节级（`You've used` 未命中、对照串命中）；📖源 统计块关键词全树 0 命中 |
| E6.2c | **章级多跑是 Tectonic 独有，且是那 8 条警告的来源** | 比较 `<P>/chapters/*.blg` 数量：Tectonic vs `latexmk -xelatex` | Tectonic 9 份；latexmk 1 份 | 判据：Tectonic 侧 `Running BibTeX` 行数 == **aux 文件数**、`warning: errors were issued by BibTeX` 行数 == **无 `\bibdata` 的 aux 数**（此处 9 与 8）；latexmk 基线目录只有 `main.blg` | P1 | E6.1 | ✅实测（`<R>/tect-kcheck-with-k/run.log`：9 条 `Running BibTeX`、**8** 条 `warning:`；Tectonic `chapters/` 8 份 `.blg`；基线 `_zhcmp/thesis/tmp` 仅 `main.blg`）+ 📖源 `driver.rs:1949-1959` |
| E6.2d | **章级空跑的产物落点 + 主 `.bbl` 的命名唯一性**（T2 · INT-43 按文件名定位主 `.bbl` 的前提） | `<T> -C -k --keep-logs -o <P> <FIX>/tect-thesis/main.tex`，然后枚举 `<P>` 下所有 `.aux`/`.bbl` 的相对路径 | `<P>/main.bbl` **唯一一份**；章级只落 `.aux`（与 `.blg`），**不落 `.bbl`** | 判据（三条）：① `Get-ChildItem -Recurse -Filter *.bbl` **恰好 1 项**且相对路径 == `main.bbl`；② 章级 `.bbl` 的缺席有**显式解释**：日志里 8 条 ``note: Not writing `chapters/chNN.bbl`: it would be empty.``（章级 aux 无 `\bibdata`/`\bibstyle` ⇒ bibtex 产出空文件 ⇒ `write_files` 跳过）；③ 主产物 stem == **根文件 basename**（`tex_input_name = input_path.file_name()`，compile.rs:144-151），**与 cwd 无关**；`\include{chapters/chNN}` 的键名以**主输入文件所在目录**为根（`filesystem_root`，driver.rs:1123-1127）⇒ 章级产物在 `<outdir>/chapters/` 子目录 | P1 | E6.1 + T2 | ✅实测（**两个独立探针目录**：`<R>/tect-kcheck-with-k` 与 `<R>/tect-t2-probe/thesis/tprint` 均只有 `main.bbl`(680 B) + 9 份 `.aux`（1 主 + 8 章）；`run.log` 里 8 条 `Not writing \`chapters/chNN.bbl\`` + 1 条 `Writing \`main.bbl\` (680 B)`）⇒ **不存在同名冲突**（章级路径本就不同名，且永远为空不落盘） |
| E6.3 | **可见性处置**（交给 T2 落地，本条只定判据） | 对 E6.1 的产物走一次产品链路（headless `latteset-cli compile`） | 期望：**静默成功**（不产生 8 条假警告），但**真失败时要能报出来** | 判据（两条都要过）：① 本夹具（bib 成功）下 `compile_get_errors()` 里**没有**任何 `BibTeX` 相关条目（现状：✅ 已经是 0 条，属"碰巧正确"，因为它连警告都收不到）；② 构造一个**真的会失败**的 bib 夹具（如 `refs.bib` 容量爆掉 / 语法错），要求 `compile_get_errors()` 出现一条可归因条目——**这才是缺陷工单**。⇒ 注意方向与旧稿相反：不是"漏报成功"，而是"**该报的在两个极端都报不出来**" | **P0** | E6.1 + E6.2b + T2 | ✅实测（代码路径级）：流式通道只留 `Error`（`runner.rs:169-184`）、终态通道只在**非零退出**才走（`runner.rs:373-397`）、小写 `warning:` 行两条路都拿不到（`scan.rs:89-93`）⇒ 现状态**同时**是"不误报"与"不真报" |
| E6.4 | `-r/--reruns` 语义 | ① `<T> -C -r 0 …`；② `-r 2`；③ 不给 | ① 只跑一趟（引用/目录落后）；② 首趟 + 固定 2 趟；③ 自动收敛到不再需要 | 判据：趟数用**日志行计数**（`Running TeX` + `Rerunning TeX`）⇒ ① `1`、② `3`、③ `1..6`；且 ① 的 `main.log` 应含 `Citation … undefined` | **P0** | E6.1 | 📖源 `-r` 是"首趟之后再跑 N 趟"且一旦给定即**固定**（driver.rs:1704-1711 把重跑原因写成 `I was told to`）；✅ 已有 **③ = 3 趟** 的实测基线（7 份日志全部 3 趟）；⚠️ ①②待实测。**注意 `-r 0` 仍会跑首趟 + bibtex，"最干净的单趟"是 `--pass tex`（见 E2.10）** |
| E6.5 | 收敛上限与告警 | 构造需要 >6 趟的文档（如链式 `\ref`） | 打 `warning: TeX rerun seems needed, but stopping at 6 passes` | 判据：该 warning 行出现 ⇒ 结论"未收敛"必须能被产品识别（否则用户看到的是"成功"但交叉引用错） | P1 | E6.4 | 📖源 `DEFAULT_MAX_TEX_PASSES = 6`（driver.rs:1344）+ 告警文案（1736-1742） |
| E6.6 | biber 路径 | 造一个 `biblatex + biber` 夹具 | 预期：能跑（driver 有 `check_biber_requirement`）或明确失败 | 判据：exit code + 是否产出 `.bbl`；失败时错误信息必须落在日志或 stderr，且能被 `parse_log` 抓到一条 Error | P1 | — | ⚠️推断（§7 明确列为未验证项） |

**E6 依据与证据**（⚠️ 本节已按产物级证据**反转**，旧稿的"bibtex 失败"结论作废）：
`✅` **`<R>/tect-kcheck-with-k`（2026-09-14，干净目录，Tectonic 0.17.0，`-C -k --keep-logs --outfmt pdf`）**：`main.bbl` **680 B / `\bibitem{ref1..ref9}` 9 条**；`main.pdf` **80,203 B**；`main.log` 28 页 / `LaTeX Warning` **0** / `Missing character` **0**；`main.blg` 537 B、0 错误行、无 `You've used`；`run.log` 9 条 `Running BibTeX` + **8** 条 `warning:` + `note: Writing \`main.bbl\` (680 B)`。
`✅` **`<R>/tect-kcheck-no-k`（同源、不加 `-k`）**：`main.pdf` **SHA-256 与上者完全相同**（`926683A0…7F9EE0`）⇒ bibtex 在同一次运行里同样成功；该目录**没有** `main.bbl`（intermediate 不落盘），但 `main.log` 同样 0 条 `Citation … undefined`。
`✅` 对照 XeLaTeX 单趟基线 `<R>/tect-thesis/xepdf/main.log`：**17 条** `LaTeX Warning: Citation … undefined` ⇒ "0 条"这个判据确有区分度。
`✅` **9 份 `.blg` 逐个复核**：`main.blg` 537 B / 0.99d / `error messages` **0** / `no \bibdata` **0** / `You've used` **0**；`chapters/ch01..08.blg` 各 306 B / `error messages` **1** / `no \bibdata` **1** / `You've used` **0**。
`📖源` **`main.blg` 的 537 B 是"完整"的，不是被截断的**（这一点与队长初判不同，已按源码修正）：`crates/engine_bibtex/src/lib.rs:548-567` 逐个 `write_log_file` 了 banner / `Capacity:` / aux 轨迹 / `The style file:` / `Database file #1:`，而**统计块的任何字符串在全 crate 都是 0 命中**（`wiz_defined`、`built_in`、`function-call counts`、`in all, are`、`strings with`）⇒ 该端口**不实现** BibTeX 的收尾统计块。旁证：同一批章级 `.blg`（306 B）**有**自己的收尾行 `(There were 2 error messages)` ⇒ 端口会写收尾行，只是没有统计块那一段。
`✅` 章级 aux 确无 `\bibdata`/`\bibstyle` —— **证据用 `.blg` 自述**（结论与出处同文件）：`I found no \bibdata command---while reading file chapters/chNN.aux`；且该 306 B 文件**以收尾行结束**，可判定自身完整。
`✅` latexmk 基线目录 `<R>/_zhcmp/thesis/tmp` 下**只有 `main.blg` 一份**（1226 B，0.99e，含 `You've used 9 entries`），无 `chapters/*.blg` ⇒ 逐 aux 跑 bibtex 是 Tectonic 独有行为。

### E7 · 失败面（退出码 / 错误信息 / 我们的呈现）

**统一判据模板**：每条用例记录 `(exit code, stdout 关键行, stderr 关键行, 是否产出 .log, 是否产出 .pdf/.xdv)`，并与"产品应呈现什么"逐条对齐。

| ID | 目的 | 命令 | 期望 | 判据（可判定） | 优先级 | 依赖 | 是否已验证 |
|---|---|---|---|---|---|---|---|
| E7.0 | 基线：**退出码约定** | 成功 / 编译失败 / 参数错 各一次 | 成功 `0`；编译类失败 **`1`**；命令行参数错 **`2`** | 判据：三个 exit code 精确相等（0/1/2） | **P0** | — | 📖源 `main.rs:183-186`（编译错误 `process::exit(1)`）+ ✅实测（`--chatter bogus` → exit **2**）；⚠️"编译失败 == 1"待实测 |
| E7.1 | 缺包（`\usepackage{nope}`） | `<T> -C --keep-logs -o <P> <P>/bad.tex` | exit 1；`<P>/bad.log` 存在且含 `File 'nope.sty' not found` | 判据：① exit == 1；② `--keep-logs` 产出的 `.log` **含 `!` 开头的行**；③ 该 `.log` 喂 `parse_log` 能出 ≥1 条 `Error` 且 `line` 非空 | **P0** | — | ⚠️推断（错误路径确实会 `write_files(only_logs=true)`，driver.rs:1506；但"日志里一定有 `!`"未实测） |
| E7.2 | 离线 `-C` + 缓存缺文件 | 用一个**故意缺文件**的夹具（`\usepackage{tikz}` 而缓存里没有）跑 `-C` | exit **1**，**不得**偷偷联网 | 判据：① exit == 1；② 断开网络后重跑结果**完全相同**；③ stderr 里能定位到缺失文件名 | **P0** | — | 📖源 `only_cached → OpenResult::NotAvailable`（crates/bundles/src/cache.rs:370-372）；⚠️最终表现（TeX 报错 vs 明确 error 行）未实测 |
| E7.3 | 网络不可用（**不加 `-C`**） | 断网后跑一个缓存里没有的包 | exit 1，且**首跑下载失败信息可读** | 判据：失败信息里含 URL 或 `download` 语义；**不得**表现为"卡住不动"（配合超时用例计时） | **P0** | E7.2 | ⚠️推断 |
| E7.4 | `-b` 指错路径 | `<T> -b <不存在的路径> -C -o <P> main.tex` | exit 1，`error: '<path>' doesn't specify a valid bundle.` | 判据：① exit == 1；② stderr 含 `doesn't specify a valid bundle` | **P0** | — | 📖源 `compile.rs:193-199` |
| E7.5 | **本地 bundle 的合法形态** | 分别 `-b <目录>` / `-b x.zip` / `-b x.ttb` / `-b x.tar` | 前三个可用；**`.tar` 不被识别** | 判据：`-b x.tar` 必须**报 E7.4 的错**（若它静默回退到默认 bundle，则该用例判**失败**——会偷偷联网） | **P0** | T4（bundle 制作） | 📖源 `bundles/src/lib.rs:275-287`（只认 dir/`.zip`/`.ttb`，其余 `Ok(None)` ⇒ 上层报"invalid bundle"）；️⚠️ `.tar` 的最终表现待实测；⚠️ 本仓 `tectonic-integration-plan.md §5.2` 写的是"TeX Live tarball + GNU patch"，**注意那不是 `-b` 的输入格式** |
| E7.6 | `-o` 指向不存在的目录 | `-o <不存在>` | exit 1，`error: output directory "<path>" does not exist` | 判据：exit == 1 且 stderr 含该文案 | P1 | T2 | 📖源 compile.rs:163-168 |
| E7.7 | 超时被我们杀掉 | 用例：故意慢文档 + 产品 `timeout`；进程侧观测 | 我们的实现走"树杀 + 超时诊断"，**不自动重试** | 判据：① 杀掉后 `<P>/main.log` 状态（**很可能不存在**，因内存层未落盘）⇒ 超时诊断拿不到页证据；② 必须在 `<R>` 记录"Tectonic 模式下超时诊断的输入退化"并交 T2 处置 | **P0** | E5.2 + T2 | ⚠️推断（`runner.rs:347-356` 的 `timeout_entry` 依赖 `tmp/<stem>.log` 的页证据） |
| E7.8 | 用户取消 | 触发 `CancellationToken` | `CompileOutcome::Aborted`；不留残留进程 | 判据：取消后 3 s 内无 `tectonic` 子进程存活；`tmp/` 无半成品 XDV（内存层未落盘 ⇒ 预期"什么都没有"） | P1 | T2 | ⚠️推断 |

**E7 依据与证据**：`📖源` exit 码只有 0/1（编译）与 2（clap）；`✅` clap 的 2 已实测。

---

## 3. 直接引用的已有实测证据（`<R>`）

| 引用 | 文件 / 命令 | 关键数字 |
|---|---|---|
| E1-① | `test_file/projects/bench/_zhcmp/tect-thesis/main.log` | 22,861 B；`[N]`=28；`Missing character`=0；`!`=0；`Warning`=0；尾行 `Output written on main.xdv (28 pages, 264728 bytes).`；**无 `This is XeTeX` 抬头**；LaTeX2e <2021-11-15> / ctexbook v2.5.8 |
| E1-② | `…/tect-thesis/xepdf/main.log`（XeLaTeX TL2026 单趟） | 43,097 B；`[N]`=27；**17 条 `LaTeX Warning: Citation … undefined` + `There were undefined references.`** |
| E1-③ | `…/tect-thesis/main.xdv` vs `…/xetmp/main.xdv` | 28 页 vs 27 页；页字节 1028/9431/16101 vs 1062/9658/16101 —— **口径不齐（单趟 vs 收敛）**，只能当反例 |
| E2-① | `node scripts/xdv-report.mjs test_file/projects/bench/_zhcmp/tect-thesis/main.xdv` | 28 页；`pre: version=7 num=25400000 den=473628672 mag=1000`；解析 5.6–6.7 ms（37–45 MB/s） |
| E2-② | `…/d1.xdv --diff=…/d2.xdv` | `A=28 页 / B=28 页`，**0 页内容变化** |
| E2-③ | `Get-FileHash` | XDV `738F96DD…71FEB`（264,728 B，三份相同）；PDF `926683A0…7F9EE0`（80,203 B，三份相同） |
| E2-④ | **`<R>/_t1-probe/`（2026-09-14，本轮，路线② 探针）** | `tect-from-xdv.pdf` 由**外部** TL2026 `xdvipdfmx` 转出：**79,070 B / 28 页 / 字体组与自带 PDF 同 / 去空白文本逐字符相同（13,559）**；`xe-from-xdv.pdf`（对照，XeLaTeX XDV）70,208 B。字体解析靠 TL：`kpsewhich FandolSong-Regular.otf` → `c:/texlive/2026/texmf-dist/fonts/opentype/public/fandol/…` |
| E2-⑤ | **`<R>/_t1-probe/passtex/`（本轮，`-C --pass tex --synctex --keep-logs`）** | exit 0；产出 `main.xdv`(242,960 B) + `main.synctex.gz`(69,500 B) + `main.log`(23,955 B)，**无 `main.pdf`**；`Skipped writing 10 intermediate files`；**`pass tex` 的单趟 XDV = 26 页**（vs 收敛档 28 页），页级差分 **27/28 页不同**（第 1 页相同）⇒ 单趟远未收敛 |
| E4-② | `…/_t1-probe/passtex/main.synctex.gz`（15,808 行、空 `Input:` 121 条） | 头部 **`Output:pdf`**——与默认档相同，**尽管该档根本没有产出 PDF** ⇒ 该字段是常量、无判据价值；`Input:1:` 末段用 `/` 分隔 |
| E3-① | **`<R>/_t2-probe/r2/`（T2 实测，本轮复核）** | `witdate/main.pdf`（设 `SOURCE_DATE_EPOCH`）**80,203 B**、印 `1970 年 1 月 1 日`；`nodate/main.pdf`（不设）**80,509 B**、印 `2026 年 9 月 14 日`；去空白文本 13,559 vs 13,560 字符，**首个差异 @18 = 日期处**（其余全同） |
| E4-① | `…/tect-thesis/main.synctex.gz`（77,983 B，解压 18,582 行 / 508,271 字符） | `SyncTeX Version:1`；非空 `Input` 9 条（主文件绝对路径 + `…\chapters/ch0N.tex` 混用分隔符）；**空 `Input:` 132 条**；`Output:pdf`；`Magnification:1000`；`!` 记录 58 条 |
| E5-① | `…/tect-thesis/run1.log`（115 行） | 73 行 `downloading …`；8 条 `warning: errors were issued by BibTeX, but were ignored; use --print and/or --keep-logs for details.`（**这 8 条是章级空跑的假警报，见 G3/E6.2c**）；`note: Rerunning TeX because bibtex was run ...`；`note: Rerunning TeX because "main.aux" changed ...`；`note: Running xdvipdfmx ...`；`note: Writing 'main.pdf' (78.7060546875 KiB)`；`note: Skipped writing 11 intermediate files`；另有 1 行 `Fontconfig error: Cannot load default config file: No such file: (null)`（§8.4 判为良性） |
| E6-① | **`…/tect-kcheck-with-k/`（2026-09-14，队长实测，Tectonic 0.17.0，干净目录，`-C -k --keep-logs --outfmt pdf`）** | `main.bbl` **680 B / `\bibitem{ref1..ref9}` 9 条 / `\newblock` 18 条**，SHA-256 `4370D1681F352FA1E934809DBAFC6643B226F0CC6EF442F2E54D48ED69AA4FBF` —— **与两条 XeLaTeX 基线 `.bbl` 三份完全相同**；`main.pdf` **80,203 B**、28 页；`main.log`（22,909 B）`LaTeX Warning`=**0**、`Missing character`=0、**末行 `Output written on main.xdv (28 pages, 264728 bytes).` 而该 `.xdv` 不存在**（G1 的现场）；`main.blg` **537 B / 0 错误行 / 无 `You've used`**；`run.log` 9 条 `Running BibTeX` + **8** 条 `warning:` + `note: Writing \`main.bbl\` (680 B)` + `note: Writing \`main.aux\`/\`main.toc\`/章级 .aux/.blg`，**全文 0 次提及 `main.xdv`** |
| E6-② | `…/tect-kcheck-no-k/`（同源、**不加 `-k`**） | `main.pdf` SHA-256 **与 E6-① 完全相同**（`926683A0…7F9EE0`）；**无** `main.bbl`（intermediate 不落盘）；`main.log` `LaTeX Warning`=**0** |
| E6-③ | `…/tect-kcheck-with-k/main.blg` 全文（537 B） | `This is BibTeX, Version 0.99d`；`Capacity: max_strings=35307, hash_size=35307, hash_prime=30011`；…；**止于 `Database file #1: refs.bib`**。**注：这是"完整"文件**（Tectonic 端口不写统计块，见 G6），**不是**截断 |
| E6-④ | `…/tect-kcheck-with-k/chapters/ch01..08.blg`（各 306 B） | `I found no \bibdata command---while reading file chapters/chNN.aux` + `I found no \bibstyle command---…` + `(There were 2 error messages)`；**以收尾行结束 ⇒ 自身完整** |
| E6-⑤ | `test_file/projects/bench/_zhcmp/thesis/tmp/main.blg`（1,226 B，latexmk 基线） | `This is BibTeX, Version 0.99e (TeX Live 2026)`；`max_strings=200000`；`You've used 9 entries`；该目录**只有这一份 `.blg`** |
| E6-⑥ | `test_file/tectonic-src` 全树 grep + **`tectonic.exe` 字节搜索**（决定性） | 源码树：`You've used` / `wiz_defined` / `built_in function` / `histogram` **全 0 命中**；二进制：`You've used`、`wiz_defined` **未命中**。**复跑用这三条控制串（应命中）**：`This is BibTeX`@**13662231**、`Capacity: max_strings`@**13662281**、`I found no`@**13671510**。⚠️ **`Database file #1` 不是有效对照**（二进制存的是 `Database file #`@13679298，编号运行时拼接）——用它验"未命中"会**误判搜索方法失效** |
| E7-① | `tectonic --chatter bogus` | `error: invalid value 'bogus' for '--chatter <CHATTER_LEVEL>': unsupported or unknown chatter level`；**exit 2** |
| G5-① | `…/tect-thesis/det1.log` / `det2.log`（PDF 档）vs `run-xdv.log`/`detxdv1.log`/`detxdv2.log`（xdv 档） | 前者**全文无 `main.xdv` 字样**；后者均有 ``note: Writing `main.xdv` (258.5234375 KiB)``；两者都有 `Skipped writing 11 intermediate files (use --keep-intermediates to keep them)` |
| G5-② | `Get-ChildItem … | Sort LastWriteTime` | `d2.xdv`/`main.xdv` = **23:44:01**（同一刻，均为 264,728 B）；随后的 **23:44:03** PDF+synctex 运行只更新 `main.pdf`(80,203)/`main.log`(22,909)/`main.synctex.gz`(77,983)/`main.blg`(537) ⇒ PDF 档**没写过** `main.xdv` |
| G5-③ | 7 份运行日志的趟数统计 | 全部 **3 趟 TeX + 9 次 BibTeX**；`xdvipdfmx` 仅 PDF 档出现（4/4），xdv 档 0/3 |
| 工具-① | `node scripts/xdv-report.mjs a.xdv --diff b.xdv`（**空格**） | 输出普通摘要、**无 `页级差分`**、exit 0 ⇒ 静默失效 |
| 工具-② | `node scripts/xdv-report.mjs a.xdv --diff=b.xdv`（**等号**） | `页级差分：A=28 页 / B=27 页`；`内容变化的页：28 页 → 1..28` |
| 文档 | `docs/research/modern-engines-zh.md` §8.2/§8.3/§8.4 | 缓存 426 文件 / 62 MB；首跑 hello 189 s、min-zh 214 s（104 下载）、thesis 51 s（73 下载）；速度 min-zh 1586 ms、thesis 2515 ms vs `xelatex -no-pdf` 1378/1681、`xelatex` 2350/2539、`lualatex` 3887/5932 ms。**注**：§8.3 的读法「2515 vs 1681 差的 ~800 ms 是 PDF 后端成本」**不成立**（两档趟数不同，属口径不齐）——与 T3 已对齐：干净口径是同引擎 **`默认档 − --outfmt xdv`**（由 E2.10 的机制可知这两档**只差 xdvipdfmx 一步**，其余 3 趟 TeX + 9 次 BibTeX 完全相同），该差值才是真·PDF 后端成本 |
| 源码 | `test_file/tectonic-src`（0.17.0，只读） | 见各用例的 `📖源` 行号 |

---

## 4. 未验证清单（`⚠️` 汇总，交给 T5 的"未验证清单"章节）

**A. 必须在本矩阵内变成实测的（按优先级）**
1. **E2.8/E2.9/E2.10（G5）**：PDF 档不落 `.xdv` 已由**三路证据**坐实（队长/T2 的 `-k` 实测、日志无 `main.xdv`、mtime 未变），机制 = `driver.rs:1985` 的 `mem.files.remove(&tex_xdv_path)`。⚠️ **本项早期版本写的"探针禁止用 `-k`，否则得到假'是'"已作废**：`-k` 对"XDV 落不落盘"**不是变量**。**但 `-k` 本身仍必须测**——它是 `.aux`/`.toc`/`.bbl` 落盘的前提（`tect-kcheck-with-k` 有 `main.aux`(443)/`main.toc`(1623)/`main.bbl`(680)，`no-k` 一个都没有），而产品侧的 **"无 `tmp/<stem>.aux` ⇒ Quick 升级 Full"不变量（`runner.rs:293-298`）在 Tectonic 模式下要重新映射**（与 T2 的 **U12** 对应，归 T2）。仍需真跑的只剩 **E2.9 的产品侧作废规则**（Tectonic 引擎侧已实测不读盘上的 `.xdv`）。
2. `-C` 缺文件、断网、`-b .tar` 三种失败面的**真实退出码与错误文案**（E7.2/E7.3/E7.5）。
3. **E6.1/E6.2b 的 exit code 与产物级判据**：主 bibtex 成功已由 `.bbl`(9 条 `\bibitem`)/PDF 坐实，但两次运行**各自的 exit code** 仍未记录；且"真失败"的 bib 夹具（容量爆/语法错）还没造，E6.3 的第②条判据缺被测样本。
4. "编译失败 ⇒ exit 1" 的实测确认，以及失败时 `.log` 是否真的含 `!` 行（E7.0/E7.1）。
5. `-p/--print` 的 `[N]` 是否**逐字节流式**（决定 Tectonic 能否提供页进度，E5.5；`110 个 [N]` 与"每趟从 [1] 重数"已有实测）。
6. ~~`--outfmt xdv --synctex` 下 `Output:` 字段的实际值~~ ✅ **已实测（本轮）**：`Output:` **恒为 `pdf`**（`--pass tex --synctex` 档**没有产出 PDF** 也照写 `pdf`）⇒ **常量、无判据价值**，不要写进任何分支（见 E4.5）。
6b. **路线② 的部署面**（E2.12/E2.13 已测"能转、文本等价"，但**未测**）：在**没有 TeX Live** 的机器上，外部 `xdvipdfmx` 既不存在、其字体解析链（kpathsea/fontconfig → TL 字体树）也不存在 ⇒ **路线② 的"免装 TL"前提不成立**；若要保留该路线，需另测"把 bundle 字体目录喂给第三方转换器"的可行性（归 T2/D2 与 T4）。
7. `fontset` 非 windows 档的缺字行为（E1.3）。
8. `-r 0` / `-r 2` 的趟数计数（E6.4；`③ 不给 = 3 趟`已有基线；`-r 0` 的"1 趟/出 PDF/无 XDV"已由 T2 实测）。
9. 本地 bundle 的**合法形态**（目录 / zip / ttb）与 `.tar` 拒绝（E7.5）——与 T4 联合。
10. E2.2 的 Rust/JS 双实现语义一致性（两套哈希算法本就不同，这项**只能靠语义对拍**）。

**B. 本矩阵明确不覆盖（非目标）**
- bundle 制作与体积、许可审计 → **T4**；
- GUI 侧表现、设置项、Quick/Full 映射、降级回退 → **T2**；
- 阈值与性能回归基线 → **T3**；
- `--outfmt html`（`.spx`）、`-X/nextonic` V2 CLI、`-Z search-path`/`shell-escape` 等不稳定项 → **不在本轮范围**（V2 CLI 只有 `build/compile/dump/new/init/show/watch/bundle` 子命令，`-X build` **没有** `--synctex/--outfmt/-r`，因此**不要**走 V2 接口）。

**C. 与其它 part 的接口（需要 T2/T3/T4 回填）**
- 引擎名：`Engine::Tectonic` 的 `binary_name()` 取值（影响 `tmp/<stem>.<engine>.pages` 与日志文件名，E2.5）。
- **Quick/Full 的产物形态**（E2.8/E2.9/E2.10）：Quick 侧必须拿到 `tmp/<stem>.xdv`（用 `--pass tex` 或 `--outfmt xdv`）；Full 侧用 `--outfmt pdf` 时**不会**有 `tmp/<stem>.xdv` ⇒ 必须回答"Full 之后页哈希缓存的基线是什么 / 陈旧 XDV 怎么作废"。
- Tectonic 模式的 outdir 约定（影响 E4.4 的 `-d`）。
- 是否采用 `-p`（影响 E5.3/E5.5 与流式反馈的成本）。
- SyncTeX：**是否需要自带 `synctex.exe`（许可/体积）或自研解析**——这是 G2，超出本文范围但会决定 E4.3 能否执行。
- **`SOURCE_DATE_EPOCH` 的引擎分支**（E3.6/E3.7/E3.10）：**已按 D4 裁定为 (b)** —— **Tectonic 进程不设** epoch（正文日期 = 当天；**XDV 逐字节稳定**；**PDF 不保证**），**外部 `xdvipdfmx` 进程设 `0`**（路线②），即"确定性由转换步骤承担"（E3.9 已证：同路径 + epoch=0 ⇒ 转换产物三次全等）。现状 `runner.rs:275` 是**无条件设**、`523-548` 的转换步也设 ⇒ 需按 T2 的 **INT-20b** 拆成按子进程。**断言边界**：XDV 逐字节相等 ✅、正文日期=当天 ✅、**PDF 逐字节相等 ❌ 不得断言**（差异仅 `/ID`+Info，≈0.07%）。**用例纪律**：固定输出路径 + 固定 epoch（E3.9 的两条硬纪律）。
- **页哈希口径**（E3.8/INT-31b，与已知债 #26 同源）：`xdv.rs:211` 目前是**原始口径**（含 45 B `bop` 的 `prev`）⇒"任一处变长 ⇒ 其后各页 `prev` 连锁 ⇒ 被判变化"。**逐用例必须声明原始/归一化口径**；口径切换本身归已知债 #26、**不在本计划内**（队长裁决）。
- **路线② 的定位**（E2.12/E2.13）：外部 `xdvipdfmx` 能转且文本等价，但字体按**裸名**解析、依赖 **TL 字体树 + kpathsea**，且 `xdvipdfmx.exe` 本身随 TL ⇒ **不是"免装 TL"路线**；对应 T2 的 INT-35/INT-38/INT-38b 与 T4 的 bundle 字体喂给第三方转换器。

---

## 5. 建议的执行顺序（引擎层）

```
G5/E2.8·E2.10（一次性，最快出结论）→ G6/E6.1 产物级判据 → E2.9 陈旧 XDV（产品侧）
→ G1/E5.2 → G2/E4.3 → E7.* 失败面 → E2/E3 兼容与确定性 → E1 中文正确性 → E4.2/E4.4 口径
（先做能"一票否决"和能复现缺陷的；口径类用例最后做，因为它们依赖 G1–G6 的结论）
```

**一票否决（任一为真即不建议上线）**：
- E2.1 页数 ≠ 28（XDV 不兼容）→ A/B/C 三个功能点全部作废；
- **E2.8 的结论（PDF 档无 XDV）** ⇒ 若 T2 不能给出"Quick 用 `--pass tex`/`--outfmt xdv`、Full 用 `--outfmt pdf` 且**显式作废旧 XDV**"的方案，则页级复用与"出 PDF"不可兼得；
- E2.9 陈旧 XDV 覆盖新 PDF（命中即等同 `runner.rs:463-466` 的已实测事故重演）；
- E4.3 三条判据任一不中（SyncTeX 不等价）；其中 **G2（无 synctex 解析器）不解决则该条无法执行**；
- E1.1 `Missing character` > 0；
- **E6.2b 判定器装反**：任何**日志级**判据都不可用——① 计 `error messages` = 8 假阳性（章级空跑）+ 1 假阴性；② 计 `warning` 行 = 无区分度；③ **查摘要行 `You've used N entries` = 对 Tectonic 100% 假阴性**（该字符串在源码与二进制里都不存在，成功也永远没有）⇒ 必须用**产物级判据**（`.bbl` 的 `\bibitem` 条数 / PDF 参考文献节 / `main.log` 的 `Citation … undefined` 计数）。**判定器回归样本必须包含"成功但无摘要行的 `.blg`"，要求判 `unknown` 而**不是** `fail`**。

**⚠️ 本节已反转两次，引用时请看日期**：旧稿"bibtex 失败被静默"（2026-09-13，基于 `.blg` 缺摘要行）→ 中稿"主 bibtex 成功、摘要行是唯一正确判据"（2026-09-14 上午）→ **定稿"主 bibtex 成功、且任何日志判据都不可用"**（2026-09-14，产物级 + 源码级 + 二进制字节级三重证据，见 G6/E6.1/E6.2b）。
