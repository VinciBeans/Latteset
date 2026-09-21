# Tectonic 库形态实测证据（§6.1–§6.5 收口）

> 本文是 [tectonic-library-plan.md](../tectonic-library-plan.md) **§6.1–§6.5 的原文搬移**（2026-09 拆分：正文一字未改，只加本页头）。
> **落盘依据**：`.gitignore:124` 与 library-plan §6 P8 的「证据落点约定」要求结论文档放 **`docs/research/`**（即本文件）；`test_file/research-tectonic-lib/**` 只作开发期原始证据，不作可复核入口。
> **编号不变**（§6.1–§6.5）；文中出现的章节号（§3.2 / §5.4 / §7 / §12 …）指回 [tectonic-library-plan.md](../tectonic-library-plan.md)。

---

### §6.1 已落地状态（库形态能真编出 PDF）

**库形态已能真编出 PDF**（中文夹具，1 页，文本层可抽回「你好，世界」）。

**构建前置有唯一落点**：`scripts/with-tectonic-lib.ps1`（npm 侧 `lib:check` / `lib:dev` / `lib:build` / `lib:test`）。
它一次做完三件事：① 校验 vcpkg 检出 / triplet 依赖 / 构建工具链，缺项给**可执行的修复命令**（此前是
build script 以 pkg-config 或 panic 的形式失败，文案不指向环境）；② 设好那 5 个环境变量
（`TECTONIC_DEP_BACKEND` / `VCPKG_ROOT` / 两个 triplet / `VCPKGRS_TRIPLET`，并给 `RUSTFLAGS` 补
`-Ctarget-feature=+crt-static`）；③ 跑对应命令。**别再在别处抄这套环境**。

```powershell
npm run lib:check                    # 只校验前置
npm run lib:cli                      # release 构建 headless CLI（带库形态）
$env:LATTESET_TECTONIC_LIB='1'                     # 运行期开关（装配点见 §3.2）
$env:LATTESET_TECTONIC_BUNDLE='file:///E:/.../bundle'   # 目录 bundle，须自带 SHA256SUM（LB-2）
$env:LATTESET_TECTONIC_CACHE='E:\...\cache'        # 产品缓存：<cache>/formats 与 <cache>/bundles
& src-tauri\target\release\latteset-cli.exe --project <dir> compile
node scripts/validate-pdf.mjs --pdf <dir>\<stem>.pdf --expect-pages 1 --expect-text 你好
```

**t10 期间实测出并修掉的 4 个真缺陷**（都属"未经编译器/未经真编就发现不了"的一类）：

| # | 缺陷 | 症状 | 修法（上游依据） |
|---|---|---|---|
| 1 | **initex 趟喂了用户文档当主输入** | 缓存目录里永远没有 `.fmt`；排版趟报 `! Undefined control sequence. \documentclass` | 主输入换成合成的 `\input tectonic-format-<stem>.tex`（`driver.rs` 的 `enter_format_mode`） |
| 2 | **忽略 `TexOutcome`** | initex 报 `Errors` 时我们当成功继续 | `Ok(TexOutcome::Errors)` 也失败（`driver.rs` 的 `make_format_pass`） |
| 3 | **dump 出的 `.fmt` 没人收** | 同上，format 永不落缓存 | format 趟后从捕获表收 `*.fmt`，按 **format 的 stem**（不是 dump 名）写 `<cache>/formats/{digest}-latex-33.fmt`（`driver.rs:1825-1838`） |
| 4 | **输入层没有"本次产物"层** | 页 1 已 shipout 后 abort：`failed to open input file "zh-min.aux"` | 输入解析顺序补一层 mem（上游 `bridgestate_ioprovider_cascade` 是 primary → **mem** → fs → … → bundle）；LaTeX 在 `\end{document}` 用**原语** `\@@input\jobname.aux` 回读刚写的 aux，不过 `\IfFileExists` |

另有一条**不是缺陷**但会误导复核的事实：本机 `test_file/projects/bench/_e1/en-tiny.tex` **没有 `\end{document}`**，用它做库形态用例会得到 `! Emergency stop / no legal \end found`（单趟计时夹具，本来就是这么设计的）。

**能力面现状**：bib 趟与多趟收敛、页哈希与 A/B/C、bib 输入未变时跳过 —— 都已落地（§6.2/§6.3/§6.4 ④-a）。
**仍不支持的**：外部工具类（biber / makeindex / glossaries —— 库形态不跑外部进程；检出即 `warn!` 且 `kind` 退回 `Quick`）。

### §6.2 bib 趟与收敛（2026-09-15 补齐，V-03/V-04 收口）

**结论先行**：`latex → bibtex → latex` **不等于引用解析好了** —— 实测第二趟读到 `.bbl` 之后 LaTeX 仍报
`Citation 'knuth1984' undefined` + `Label(s) may have changed. Rerun to get cross-references right.`，
因为 `\bibcite`（编号）是上一趟才写进 `.aux` 的。⇒ **只做 bib 趟而不做重跑循环，等于白跑**。
所以这里把上游 `default_pass` 的重跑循环一并落地，并按 ㉘ 的产品语义分档：

| 请求 | 行为 | `Success.kind` |
|---|---|---|
| `Quick`（编辑触发） | **1 趟**，不跑 bibtex（"落后一趟"正是它的语义，交 ㉘ 兜底） | `Quick` |
| `Full`（首编/手动/空闲收敛） | 排版趟 →（aux 有 `\bibdata` 则 bibtex）→ 比较 rerun 相关中间产物是否变化 → 跑到稳定，**上限 6 趟**（上游同值） | `Full`，**除非**文档要 biber/makeindex（那退回 `Quick`，不得虚报） |

**实测（release，同目录 bundle、热 format）**：

| 夹具 | 库形态 | 子进程 `tectonic.exe`（收敛档） |
|---|---|---|
| `zh-min.tex` 冷（2 趟） | **616 ms**（排版 269 / 转换 347） | 571 ms |
| `zh-min.tex` 已收敛（**1 趟**） | **358 ms**（排版 266 / 转换 92） | — |
| `cite.tex`（`\cite`+`.bib`）冷（**3 趟** + 1 次 bibtex） | **954 ms** | 869 ms |
| `cite.tex` 热（2 趟） | **679 ms** | — |
| `cite.tex` Quick（1 趟） | **405 ms** | — |

⇒ 与子进程**同速**；**已收敛的文档反而更快**（跳过无用趟是自持循环的直接收益）。
引用真的解析了：PDF 文本层含 `[1]` / `Knuth` / `The TeXbook`，`.aux` 里有 `\bibcite{knuth1984}{1}`，
`.blg` 显示 `plain.bst`（bundle）与 `refs.bib`（项目磁盘）都读到了。

**过程中揪出的两个真 bug**（都不是编译器能发现的）：

1. **输出层"打开即截断"缺失**：`IoCapture` 原来只 `entry().or_default()` 而不清空，于是第二趟的
   `.aux` **追加**在第一趟后面 ⇒ 每趟变长 ⇒ 重跑永不稳定（跑满 6 趟）并报 `multiply-defined labels`；
   `.log` 也被历趟拼在一起、让错误清单失真。上游 mem 层的 `\openout` 语义是**新建缓冲**。
2. **缺"上一趟中间产物"这条输入**：上游 latexmk 之所以能在单趟下保住目录/引用，是因为它读得到
   `tmp/<stem>.aux`/`.bbl`。补上后（且只放行 [`crate::RERUN_EXTENSIONS`] 那几类，**不含** `.pdf`/`.xdv`）
   实测消掉两个症状：**Full 出来的参考文献表会在下一次 Quick 里消失**、以及**每次编译都从零开始 ⇒ 永远 2 趟**。
   编译开始时还会把这些副本预热进内存层，重跑判据才有"上一趟"可比较（否则重复编译永远 2 趟）。

### §6.3 页哈希（任务 3 收口；A/B/C 在库形态档成立）

**做法**：不另写解析器 —— XDV 本来就在捕获表里（`main.xdv`），**连盘都不用读**，直接把那份字节喂
`latteset_core::xdv::page_hashes`，与子进程档**同一个函数、同一份数据**。这是硬要求：两套口径会让
"换形态"被判成整篇都变了（全量重绘）。⇒ **`XdvParser` 不用于这条路**（流式解析是 P5「编译中页事件」的
入口，A/B/C 只要最终页表，用同一函数才能与子进程档逐页可比）。

**顺带修的一处口径重复**：缓存版本标记 / 路径 / 读写都从 `latteset-infra` **提到 `latteset_core::xdv`**
（`PAGES_CACHE_VERSION`、`pages_cache_path`、`parse_pages_cache`、`format_pages_cache`）。理由：两个 runner
都要用它，而库形态档在 `latteset-tectonic`、按 ADR-0012 **不依赖 infra** ⇒ 两处各写一份会漂移，代价是
"永远首轮"（A 面每轮白跑 0.65–0.94 s）。infra 侧只留薄封装，既有测试全绿。

**实测（release，28 页多文件中文夹具 `ctexbook` + 8 章 `\include` + toc + `\bibliography`）**：

| 场景 | 结果 |
|---|---|
| 首次 Full | **3 趟**（tex→bibtex→tex→tex）、`pages=28`、1709 ms（排版 530 / 转换 1177） |
| 再 Full（内容未变） | 2 趟、1122 ms、逐页哈希与上一轮**完全一致** |
| **Quick（内容未变）** | **A 触发**：`convert_ms=0`、`reused_pdf=true`、**475 ms**（跳过转换与拷贝） |
| **改中间一章的一个词** | 页数 28→28，**只有第 16 页变**（1/28）—— 债 #26 的 V1 口径端到端成立 |

**边界**：空表仍只表示**无法判定**（XDV 缺失/损坏）⇒ 前端保守全量刷新（真机见 §6.3.1 的 V7）；A **只对 Quick**
（与子进程档同闸门，Full 的语义就是完整刷新一遍）。两者的判据不同：A 看 `.pages` 缓存（runner 侧），
"无法判定"看 `pages == 0`（前端侧）。

### §6.3.1 GUI 真机验证（2026-09-15）

§6.3 的数字来自 release 档的 bench harness（`latteset-cli` 是一次性语义）；本节是**真机 GUI** 下的同一批
契约，夹具相同（`test_file/tectonic-lib-run/proj-26`：8 章 `\include` + toc + `\bibliography`），用
tauri server MCP 驱动：`npm run tauri dev -- --features tectonic-lib`（+ `VITE_LATTESET_PROJECT` 自动打开项目）。
⚠ dev profile 下 C 引擎是 `-O0`，**耗时只作同环境对照，不作性能结论**。

| # | 场景 | 操作 | 实测 |
|---|---|---|---|
| V1 | 冷启动 Full | 清 `tmp/` + 根 PDF → 点「编译」 | `库形态排版趟数 passes=3 stable=true converged=true`；`pages=28 reused_pdf=false`；`changed=28`；项目根出 **82,586 B** PDF |
| V2 | 产物与判据缓存 | — | `tmp/main.tectonic.pages` = `v1` + 28 行；`validate-pdf.mjs` → pdf.js `numPages=28`、文本层 13,570 字符、0 个 U+FFFD |
| V3 | **A**（Quick + 内容未变） | Monaco 在 `ch08.tex` 末尾加一行**纯注释**（输出不变） | `触发编译（编辑触发 = Quick 单趟）`、`passes=1`；**`页哈希与上次逐页相同：跳过 XDV→PDF 转换与拷贝 pages=28`**；`convert_ms=1 reused_pdf=true`（对照冷 Full 的 `convert_ms=6399`）；`changed=0` ⇒ 控制台 3 次 `[preview] 跳过重载：28 页逐页未变`，**没有第二次 reload** |
| V4a | **B/C**（改一章、跨页重排） | 同一行再追加 6 个字 | `changed=3`；预览 `reload#2 … render=4ms pagesRendered=0 pagesReused=7`（对照首轮 `reload#1 render=66ms pagesRendered=7`） |
| V4b | **B/C**（改一个词） | 等长逐字替换 `追加`→`替换`→`变更`（断行不变） | **`changed=1`**，逐页比对确认**恰好第 16 页**（与 §6.3 的 CLI 结论一致）；两次替换均复现 |
| V5 | A 的判据（缓存）缺失时的退化 | 删 `tmp/main.tectonic.pages` → 编辑 | 该轮 **A 不触发**（`reused_pdf=false convert_ms=1928`）并**写回缓存**；下一轮恢复 `convert_ms=1 reused_pdf=true` ⇒ "只退化一轮"端到端成立 |
| V6 | 换引擎不串档 | 面板切 XeLaTeX → 点「编译」 | `Full 编译（完整 latexmk 收敛） engine="-xelatex"`、`changed=28`、**新生成 `tmp/main.xelatex.pages`**，`main.tectonic.pages` 原样保留；两份缓存行数与首行完全相同（`v1` + 28 行）⇒ 口径可比，**28/28 页哈希不同**（两个引擎的 XDV 不同，不是误报）；PDF 82,586 → 74,506 B |
| V7 | **「空表 ⇒ 无法判定 ⇒ 全量刷新」**（§2.7 页哈希行的第 4 条契约） | 面板切到 Tectonic 的**子进程**档（`lib_form=false`）→ 点「编译」 | 日志 **`pages=0 changed=0`**（该档 PDF 不落 XDV ⇒ 页哈希为空），前端 `reload#10` —— **是重载、不是"跳过重载"** ⇒ 保守全量刷新成立 |

**V7 只在子进程 Tectonic 档可达**：库形态档的 XDV 总在捕获表里，页哈希不会为空；断言"PDF 档没有 XDV ⇒
页哈希必须为空"的用例见 `latteset-infra/src/runner.rs`。

**`changed` 反映的是页字节，不是编辑规模**：等长替换（V4b）动 1 页；追加 6 个汉字让正文重排跨过页边界，
就动 3 页（V4a）。

**形态由引擎闸门决定**：`TectonicSettings::use_library_form(engine, env_forced)`——`lib_form` 只在
`engine == Tectonic` 时生效。缺少这半条闸门时，`engine=xelatex` + `lib_form=true` 会静默跑库形态，而状态栏
报的是 XeLaTeX（判据被踩反：不产 `tmp/<stem>.xelatex.pages`、`changed` 恒为 0，因为 Tectonic 复现出逐页
相同的页面）。`LATTESET_TECTONIC_LIB=1` 是**显式覆盖**，不受该闸门约束（复核/CI 的逃生门），状态栏的形态
chip 会注明它来自环境变量。

**形态段只在选中 Tectonic 引擎时渲染**（见 [modules.md](../modules.md) §6）⇒ 存过 `lib_form` 却切到别的
引擎时它是不可见的。引擎那一栏为此点名提示「已保存 Tectonic 形态设置（库内嵌 / 宏包集 / 缓存目录）；
仅在选择 Tectonic 引擎时生效」。

### §6.4 P4 常驻档实测（J1 判决：**不成立**，2026-09-15）

复核入口：`scripts/bench-tectonic-lib.mjs --mode resident` 驱动
`crates/latteset-tectonic/examples/bench.rs`（新增：**同一个进程里连续编译 N 次** ——
这正是 CLI 做不到的事，`latteset-cli` 是一次性语义）。所有数字 = release、热 format、同目录 bundle、中位。

**① 常驻的净效应（只差"是否同进程"，其它全同）**

| 夹具 | 常驻（稳态） | 每次起进程 | 净效应 |
|---|---|---|---|
| `zh-min`（1 页 CJK，Full） | **345 ms** | 368 ms | **−26 ms（−7%）**，无退化 |
| `proj-26`（28 页多文件，Quick） | **470 ms** | 486 ms | **−16 ms（−3%）**，无退化 |
| `proj-26`（28 页多文件，**Full**） | **≈1450 ms**（第 4 轮起） | **1109 ms** | **+341 ms（+31%）**，第 3 轮起退化 |

**② 退化不是"活变多"**（逐轮证据，已排除）：

- `passes` 恒为 2；`req_ram/mem/disk/bundle/miss` 每轮**完全相同**（2 / 104 / 169 / 1413 / 596）；
- 但 `typeset_ms` 464→590（+27%）、`convert_ms` 627→870（+39%）、
  **`bundle_read_ms` 23.5→41（+75%，而 `bundle_read_n` 恒为 2009）**；
- `format_read_ms` 基本平（15.2→13.6）⇒ 不是单纯的"文件缓存被冲掉"；
- 进程峰值工作集：重夹具 **245 MB** vs 轻夹具 **203 MB**。
⇒ 同进程内累计的内存压力把**所有**操作（含我们 I/O 层的文件读）一起拖慢。
**上游从未承接过"一个进程跑 N 次引擎"**（CLI 一进程一次），这是自持循环特有的暴露面。

**③ 进程地板本身很小**：`latteset-cli help` **9.3 ms**、`tectonic --version` **9.2 ms**
⇒ "省掉每次起进程"的天花板就是 ~9–26 ms，**远低于 P4 门槛要求的 ≥100 ms**。

**④ 真正的固定成本（都不是进程，本文 LIB-2 的分段数字）**

| 段 | 数字 | 说明 |
|---|---|---|
| format 文件读 | **7.3–15 ms/趟**（24.45 MB） | 归属我们 I/O 层；引擎侧对 dump 的**解析**在 C 里，未测 |
| bundle 读 | **2009 次/编译 → 23–41 ms** | `req_bundle=1413` + miss 596；常驻化它是比进程更值钱的杠杆 |
| **Full 的无条件 bib 重跑** | **≈470 ms（−42%）可省** | `.aux` 含 `\bibdata` ⇒ 跑 bibtex ⇒ **强制**重跑一趟（上游同口径）。按 latexmk 的"`.bib` 比 `.bbl` 新才跑 bibtex"判据，**内容未变的文档可以只跑 1 趟** |

**④-a bib 跳过的落地实测（2026-09-15）**：判据 = 各 `.aux` 的 BibTeX 命令（含 `\@input` 闭包）+ 每个
`.bib` 的 `(mtime, size)` 与产出当前 `.bbl` 那次一致；缓存 `tmp/<stem>.bibsig`。契约见
[modules.md](../modules.md) §2.7「bib 输入未变时跳过」。

| 档 | 冷（首编，跑 bibtex） | 热（内容未变，跳过） |
|---|---|---|
| `proj-cite`（单文件 `\cite` + `.bib`，release、热 format） | 3 趟 / **941 ms** | **1 趟 / 400 ms**（−541 ms，−57%） |
| `proj-26`（28 页多文件，同上） | 3 趟 / 1630 ms | **1 趟 / 646 ms**（−984 ms，−60%） |

三条反例（都必须**不**跳过）：改 `.bib` ⇒ 跑 + 2 趟；新增一条 `\cite`（`.bib` 未动）⇒ 跑 + 2 趟；
删 `tmp/<stem>.bbl` ⇒ 跑 + 3 趟，且该轮写回缓存后下一轮恢复跳过。冷/热两条路的 **PDF 逐字节相同**
（82,586 B / 28 页 / 13,570 字）；改正文后 bib 仍跳过、PDF 随源变化，改回后 PDF 逐字节还原。

> **量这条时先修掉了一个前置缺陷**：`proj-26` 上单独上 bib 跳过几乎量不出收益（1104 vs 1110 ms），
> 因为 `seed_previous_intermediates` 用的是**非递归** `read_dir` —— `\include` 分章时
> `chapters/chNN.aux` 在子目录里，从未被预热，于是每趟都被判成"新出现"，**多文件档无论如何都跑 2 趟**。
> 递归 + 键用正斜杠之后，多文件档 1104 → **646 ms**（2 趟 → 1 趟），bib 跳过那 ≈470 ms 的账才算兑现。

**⑤ LIB-1 判决**（判据：库形态同档中位 < 子进程地板 × 0.8 **且** 省 ≥ 100 ms）

| 档 | 库·常驻 | 子进程 | ×0.8 | 判定 |
|---|---|---|---|---|
| `zh-min` Full | 345 | 553 | 442 | **通过**（345<442，省 208 ms）—— 但请注意归因：赢的是**跳过一趟**（自持重跑循环 + `tmp/` 预热），**不是常驻** |
| `proj-26` Full | 1450 | 1345 | 1076 | **不通过**（连每次起进程的 1109 也略高于 1076） |

**⇒ P4 闸门判决：J1「常驻省地板」不成立**，按方案 §6 P4 的**回退点**处理：**不投入常驻**，
保留 P2/P3（内存产物与 I/O 接管）。后续该投的是 ④ 里那两条**有数量级差别**的杠杆
（bib 跳过判据 → 省 ~470 ms/次；bundle/format 常驻化 → 省 ~30–55 ms/次），
而不是进程常驻（~9–26 ms，且重文档为负）。

**性能口径（2026-09-15 实测，release，同一目录 bundle 与同一夹具）**：

| 夹具 | 库形态（本 crate） | 子进程 `tectonic.exe -C -r 0` |
|---|---|---|
| `zh-min.tex`（CJK 1 页）冷缓存 | **1943 ms**（format 生成 1494 / 排版 335 / 转换 114） | 1857 ms（首次） |
| 同上，热缓存 | **372–642 ms**（排版 279–509 / 转换 91–131） | 395–458 ms |
| 极小英文文档，热缓存 | **221–237 ms**（排版 86–108 / 转换 112–136） | 171–254 ms |

⇒ 库形态与子进程**同速**（与 t7 的"无可证实改善"一致）。**常驻档的结论以本节 ① 的同口径对照为准**（不投入常驻）；t7 早先报的常驻复用收益用的是另一个对照面，两者不可互相印证。

> ⚠ **测量纪律（测量时踩过）**：`dev` profile 下 C 引擎（xetex/xdvipdfmx）按 `-O0` 编译，同一夹具的库形态数字会比 release 大 **6–8×**（实测 debug 3485 ms vs release 543 ms / 同一文档）。**凡与 `tectonic.exe` 比性能，必须先 `--release`**，否则结论完全相反。

---

### §6.5 编辑触发档的收敛预算（2026-09-15：草稿从固定档变成判定结果）

**动机**：草稿档（㉘）的前提是"单趟一定落后"。库形态里这个前提不成立 —— 重跑循环与中间产物指纹本来就在，而"这一趟落没落后"是**可判定**的（`.aux`/`.toc`/`.bbl` 摘要比对）⇒ 让编辑触发的请求在预算内自己跑到稳定，收敛就按**实际**结果报 `Full`：前端 `draft=false`，不亮「引用待更新」，`useIdleConvergence`（只看 `draft`）也不会再排一次 Full。

**预算（三重闸门）**：`passes < 3` ∧ **第 1 趟 < 1000 ms** ∧ 循环累计 < 2000 ms（末值与前端 `DELAY_MS` 同值 —— 能在那 2 s 等待内收敛就严格更优）。任一条不满足即停在草稿态、行为与 ㉘ 原样一致；决策日志点名是哪条。**含 bib 趟**：预算内该跑就跑（否则"加了一条 `\cite`"要等到空闲收敛才解析出来）。

**实测**（release、热 format、本地 bundle、`cargo run --release -p latteset-tectonic --example bench -- --quick`）：

| 夹具 | 场景 | 趟数 | converged | kind | ms |
|---|---|---|---|---|---|
| `proj-26`（28 页带 bib） | 已收敛后的编辑触发 ×3 | 1 / 1 / 1 | true | **Full** | 487 / 475 / 477 |
| `proj-26` | 真实内容改动（`.toc` 变） | 2 | true | **Full** | 1115 |
| `proj-26` | 紧接着（已收敛） | 1 | true | **Full** | 468 |
| `large`（125 页 / 462 KB） | 已收敛 | 1 | true | **Full** | 1878 |
| `large` | 真实改动 —— **只看总预算**（中间实现，反例） | 2 | false | Quick | **3846** |
| `large` | 真实改动 —— **第 1 趟闸门**（最终实现） | 1 | false | Quick | 1847 |

**为什么加"第 1 趟 <1 s"**：预算是**跑完一趟之后**才检查的。125 页夹具一趟 1.7 s < 2 s ⇒ 中间实现放行了第 2 趟，两次共 **3846 ms** 而中间产物仍在变 ⇒ 既白付一趟、又仍报 `Quick`（比不加这道闸门更差）。加上后回到 1 趟 **1847 ms**，与旧行为一致。

**旧/新对照（28 页带 bib、真实改动）**：旧 = 编辑触发 1 趟（~475 ms、落后）+ 提示 + 停手 2 s + 收敛 Full 2 趟 ⇒ **≈3.6 s 才拿到正确结果**；新 = **1115 ms 直接就是正确结果**，全程无落后态。

**GUI 真机**（`npm run lib:dev`，debug）：注释改动（输出不变）→ 1 趟收敛、`draft=false`、状态栏无「引用待更新」、成功后 6 s 内**没有**第二次编译；目录改动（`.toc` 变）→ 第 1 趟 **2174 ms** 超闸门 ⇒ 停在草稿态、亮提示、2 s 后空闲收敛补 Full。**debug 一趟比 release 慢 ≈4.6×**（与 §6.4 的第 742 行同源）⇒ 阈值按**发布档**标定，`tauri dev` 下会更早退化成旧行为，**不是回归**。

**仍未测**：① 其它机器 / CI 上"一趟耗时"的分布（阈值现在是常数，要不要按机器自适应没有结论）；② 3 趟档（`cite` 冷启那类一起手就要三趟的文档）在真机 GUI 下的表现 —— 只在 bench 里验到 2 趟。

---
