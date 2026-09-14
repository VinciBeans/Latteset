# T2 集成层测试矩阵：Engine::Tectonic 功能面、降级与回退（分册）

> 分册归属：团队 `tectonic-test-plan` / 成员 `app-integration` / 任务 `t2`。
> 本册只覆盖「把 Tectonic 接进 Latteset 之后」要验的**集成面**（引擎自身行为归 T1、测量口径归 T3、分发与许可归 T4），最终由 T5 合并进 `docs/research/tectonic-test-plan.md`。
> 只出方案，不改产品代码。

## 0. 读法、字段与结论先行

### 0.1 现状基线（本册的出发点）

| # | 事实 | 证据（本次只读核查） |
|---|---|---|
| B1 | **产品代码里目前零 Tectonic 引用**：`Engine` 枚举只有 xelatex/pdflatex/lualatex，全仓 `grep -ri tectonic src crates src-tauri` 无命中 | `crates/latteset-core/src/types.rs:17-24`；本次 grep 结果 0 |
| B2 | ⇒ 本册**全部条目都是"接入后待验"**。凡未经实测的判断一律标 `[推断]`（含源码级推断——推断依据写在同行的「证据·落点」里） | — |
| B3 | 引擎相关映射只有**三处**：`latexmk_flag()` / `binary_name()` / `writes_xdv()`；声明与单测都锁在这三个函数上 | `types.rs:26-59`、`types.rs:300-322` |
| B4 | 引擎是**设置项**（全局 `compile.engine` + 项目覆盖 `CompileOverrides.engine` + `SettingsPatch.engine`），默认 `xelatex`；校验器**不校验** engine | `settings/model.rs:16-91`、`settings/validate.rs:14-37` |
| B5 | 前端引擎选项**写死三个** `<option>`，即点即存（走 `update_settings`） | `src/components/SettingsPanel.vue:21,75-78,149-150` |
| B6 | 命令构造：Full = `latexmk <flag> -outdir=tmp -synctex=1 -interaction=nonstopmode <rel>`；Quick = `<binary> [-no-pdf] -interaction=nonstopmode -synctex=1 -output-directory=tmp <rel>`；两条路径都固定 `SOURCE_DATE_EPOCH=0` | `crates/latteset-infra/src/runner.rs:235-280` |
| B7 | Quick 前置条件 = `tmp/<stem>.aux` 存在，否则**自动升级 Full** 并回传实际强度 | `runner.rs:293-298`、`modules.md §12.2` |
| B8 | 页哈希缓存**按引擎分文件** `tmp/<stem>.<binary_name()>.pages` | `runner.rs:209-211` |
| B9 | 功能点 A：Quick ∧ `writes_xdv()` ∧ 页哈希逐页全同 ∧ 项目根已有 PDF → 跳过 `xdvipdfmx` 与拷贝（日志行「页哈希与上次逐页相同：跳过 xdvipdfmx 转换与 PDF 拷贝」） | `runner.rs:452-517` |
| B10 | 流式反馈 = 三条读任务（子进程 stdout、stderr、`tmp/<stem>.log` 尾随）→ 同一 `LiveFeedback` | `runner.rs:329-345`、`modules.md §2.6.1` |
| B11 | SyncTeX 走**系统 `synctex` 二进制** + `-d <pdf 父目录>/tmp`；失败按 100/200/300ms 退避重试 | `crates/latteset-infra/src/synctex.rs:29-105` |
| B12 | 生成产物分类：`tmp/` 前缀或生成扩展名（含 `xdv`/`fls`/`synctex.gz`）→ 不自动打开 | `crates/latteset-core/src/synctex/classify.rs:31-75` |
| B13 | `.fls` 来自 **latexmk 默认 `-recorder`**（Quick 直调引擎**不产**），依赖信息**至今未接线** | `docs/research/g1-read-interception-feasibility.md §3.1`、`modules.md §12.1 #23` |
| B14 | 模板/缺文件文案里**写死了 TeX Live 工具**：``tlmgr install {cls}``、``tlmgr install {pkg}``、``xelatex {X.ins}``、`改用 TeX Live 自带字体` | `crates/latteset-core/src/log_parser/diagnosis.rs:383,394,358,368,110-143` |
| B15 | 启动失败文案写死 TeX Live：`无法启动 {binary}（TeX Live 未安装？）`、`无法启动 latexmk（TeX Live 未安装？）`、`无法启动 xdvipdfmx（TeX Live 未安装？）` | `runner.rs:321,322,538` |
| B16 | 失败可见性：`IoError` 被当作 `ContentError` 并**进错误列表**（一条 `ErrorEntry`） | `crates/latteset-core/src/scheduler/policy.rs:53-59`、`scheduler/actor.rs:250` |
| B17 | 全局设置**解析失败会把整份重置为默认并落盘覆盖**（未知枚举值就是"解析失败"） | `crates/latteset-infra/src/storage.rs:41-67`（`save_global(default)`）；项目覆盖同理整份忽略 `storage.rs:91-106` |
| B18 | 应用**不显示当前引擎**：状态栏只有 阶段/页数/引用待更新/根文件/冲突/打开错误 | `src/components/StatusBar.vue:48-99` |
| B19 | 没有「关闭项目」命令；"重开" = 再 `open_project`（切项目）或重启应用 | `src-tauri/src/commands.rs:118-176`（全仓无 close 命令） |

**Tectonic CLI 侧的前置事实**（本次读 `test_file/tectonic-src`（0.17.0 源码副本）取得，未新增实测；实测数字引用 `docs/research/modern-engines-zh.md §8`）：

| # | 事实 | 证据 |
|---|---|---|
| T-a | `-o/--outdir` 目录**必须已存在**，否则直接报错退出（`output directory "…" does not exist`）；默认 outdir = 输入文件所在目录 | `src/bin/tectonic/compile.rs:163-171` |
| T-b | `--outfmt {pdf,html,xdv,aux,fmt}`，默认 `pdf` | `compile.rs:41-43`、`src/driver.rs:120-143` |
| T-c | `-r/--reruns N` = 首次之后**恰好**重跑 N 次；不传 = 自动收敛（上限 6 趟）+ bibtex/biber | `compile.rs:53-55`、`driver.rs:1704-1745`、`driver.rs:1344` |
| T-d | `--pass {default,tex,bibtex_first}`；`PassSetting::Tex` **不跑 xdvipdfmx** ⇒ `--pass tex` **不出 PDF**（只产 XDV）。**实测（本轮）**：1 趟 TeX、0 次 xdvipdfmx、产出 `hello.aux/log/xdv`、无 `.pdf` | `driver.rs:1492-1503`、`driver.rs:1752-1758`；本轮探针 p3 |
| T-e | `--keep-logs` 才写 `.log`/`.blg`；不加则日志不落盘 | `driver.rs:1625-1629` |
| T-f | `-k/--keep-intermediates` 才把中间产物写到磁盘。**实测（本轮）**：带 `-k` 时 `.aux`/`.toc`/`.bbl`/`.blg` 落盘；不带时 1 页文档只落 `.log`+`.pdf` 并打 `note: Skipped writing 1 intermediate files`。**但 `.xdv` 例外：PDF 档下无论是否 `-k` 都不落盘**（见 T-p） | `driver.rs:1615-1623`；本轮探针 p1/p2 + thesis |
| T-g | 输出命名 = **输入文件的 file_name stem**：`<stem>.pdf`/`.xdv`/`.log`，写在 outdir | `driver.rs:1209-1227,1644` |
| T-h | 文件**在整个 session 结束时一次性落盘**（不是边跑边写） | `driver.rs:1506-1528`（`write_files` 只在末段调用） |
| T-i | `note:` → **stdout**；`warning:`/`error:` → stderr（非 TTY 走 PlainStatusBackend） | `crates/status_base/src/plain.rs:44-67`、`src/bin/tectonic/main.rs:71-78` |
| T-j | `-p/--print` 时引擎 chatter（含页标记 `[N]` 与 `! …` 错误）走真 stdout | `crates/io_base/src/stdstreams.rs:29-34`、`driver.rs:1168-1172` |
| T-k | filesystem_root = **主输入文件的父目录** ⇒ 嵌套根文件（`css/thesis.tex`）下项目根的 `\input` 不在搜索根内；`-Z search-path=<path>` 可补（`--untrusted` 下被忽略） | `driver.rs:1123-1141`、`src/unstable_opts.rs:22-23,127-128` |
| T-l | `--makefile-rules <dest>` 写 `<out>: <input> \ <引擎从文件系统读过的文件>…` | `driver.rs:1540-1573` |
| T-m | 失败 `exit 1`；bundle 无效 → `error: `…` doesn't specify a valid bundle.` | `main.rs:183-186`、`compile.rs:196-198` |
| T-n | 首跑下载**逐文件** `note: downloading <name>`，**没有百分比**；实测首跑 189s / 214s / 51s | `crates/bundles/src/ttb_net.rs:171`、`itar.rs:288`、`modern-engines-zh.md §8.2` |
| T-o | 实测：`--outfmt xdv` 产物是 `xdv.rs` 认的格式（28 页）；`--synctex` 有 `.synctex.gz`；`SOURCE_DATE_EPOCH=0` 下 PDF/XDV 逐字节一致 | `modern-engines-zh.md §8.4` |
| T-p | **PDF 档下 `.xdv` 永不落盘**（不是"落得晚"，是根本不写）：`xdvipdfmx_pass` 结束时把 XDV 从内存文件表**删除**（`mem.files.remove(tex_xdv_path)`），因此 `write_files` 无论 `-k` 与否都写不到它 | `driver.rs:1985`（`remove`）、`driver.rs:1966-1987`；T1 E2.8 两路证据 + **本轮探针 p2**（带 `-k` 仍无 `hello.xdv`）与 thesis（有 `main.toc`、无 `main.xdv`） |
| T-q | `[N]` 页标记**只在 `-p/--print` 时**到 stdout，且**每趟从头重数**。**实测（本轮）**：28 页文档 `-p` 下 stdout 有 110 个标记，前三个是 `[1] [1] [1]`（3 趟）；`main.log` 里是 28 个 | `driver.rs:1168-1172`、`crates/io_base/src/stdstreams.rs:29-34`；本轮探针 thesis；T1 E5.3 |
| T-r | `-k` 同时决定 `.aux` 与 `.toc` 是否落盘 ⇒ 产品的「Quick 前置条件（`tmp/<stem>.aux`）」与「收敛/落后一趟判据（解析 `tmp/<stem>.toc`）」**都依赖 `-k`**；`.toc` 在 `ALWAYS_INTERMEDIATE_EXTENSIONS` 里（不带 `-k` 恒被跳过） | `driver.rs:1615-1623`、`driver.rs:1345-1347`；本轮探针 thesis（`-k` 时 `main.toc` 1623 B 存在） |
| T-s | **bib 的成功与失败都没有可靠信号**（T1 G3 修订版，本册独立复现）：① 主 bibtex 调用是**成功**的（`.bbl` 680 B / `\bibitem` **9 条** == `.bib` 条目数；PDF 文末有参考文献）；② 那 8 条小写 `warning: errors were issued by BibTeX, but were ignored` **全部来自章级空跑**（Tectonic 对**每个** aux 都跑一次 bibtex；本册实测 `Running BibTeX`=9、小写警告=8、且只有 `main.aux` 带 `\bibdata`）⇒ **每次成功构建都会打 8 条假警报**；③ `.blg` **不实现** `You've used N entries` 统计块（该字符串在源码与 `tectonic.exe` 里都不存在）⇒ 不能用 `.blg` 判成败 | `driver.rs:1912-1947`、`driver.rs:1949-1959`；本册实测（`_t2-probe/thesis/tprint`：`.bbl` 9 条、`.blg` 无摘要行）+ T1 G3/G6 |

### 0.1.1 本次新增实测（T2 探针，2026-09；工件在 `test_file/projects/_t2-probe/`，随 `test_file/**` gitignore）

`tectonic 0.17.0`，一律 `-C`（只用缓存、不联网）、`SOURCE_DATE_EPOCH=0`；夹具 `hello.tex`（1 页 article）与 `tect-thesis/main.tex`（28 页 ctexbook + bibtex）。

| # | 命令（`-o` 目录已先建） | exit | 产出（outdir） | TeX 趟 / BibTeX / xdvipdfmx | 结论 |
|---|---|---|---|---|---|
| p1 | `-C --keep-logs -o t1 hello.tex` | 0 | `hello.log` `hello.pdf` | 2 / 0 / 1 | 默认档**不落 `.aux`**，打 `Skipped writing 1 intermediate files` ⇒ 不带 `-k` 时产品的 aux 探测必然为假 |
| p2 | `… -k -o t2 hello.tex` | 0 | `hello.aux` `hello.log` `hello.pdf` | 2 / 0 / 1 | **带 `-k` 仍无 `.xdv`**（T-p）⇒「`-k` 换 XDV」不成立 |
| p3 | `… -k --pass tex -o t3 hello.tex` | 0 | `hello.aux` `hello.log` `hello.xdv` | 1 / 0 / 0 | 真单趟，但**无 PDF** ⇒ 不能当产品 Quick |
| p4 | `… -k -r 0 -o t4 hello.tex` | 0 | `hello.aux` `hello.log` `hello.pdf` | 1 / 0 / 1 | **`-r 0` = 1 趟 + 出 PDF** ⇒ 产品 Quick 的可用形态（无 `.xdv` ⇒ 无页哈希） |
| p5 | `… -k --outfmt xdv -o t5 hello.tex` | 0 | `hello.aux` `hello.log` `hello.xdv` | 2 / 0 / 0 | `--outfmt xdv` **不是单趟**（照样自动收敛）且无 PDF |
| th | `-C --keep-logs -k -p --synctex -o tprint main.tex`（28 页） | 0 | `main.aux` `main.bbl` `main.blg` `main.log` `main.pdf` `main.synctex.gz` `main.toc`（**无 `main.xdv`**） | 3 / 9 / 1 | 3.56s；`-p` 下 stdout 110 个 `[N]`（每趟从 `[1]` 重启）；stderr 有 `Fontconfig error` 与 **8 条小写 bibtex 假警报**（成功构建也有，见 T-s/INT-43）；`.bbl` 680 B / 9 条 `\bibitem`、`.blg` 无 `You've used` 摘要行 |

附带独立复现：**`-o` 指向不存在的目录 → `error: output directory "…" does not exist`，exit 1**（本轮两次误用都撞上）——佐证 INT-21。

### 0.2 字段定义（T5 合并时按此 7 列，最后一列可裁）

| 字段 | 含义 |
|---|---|
| **ID** | `INT-nn`，本册内稳定、可被其它分册与最终文档交叉引用；编号按章留空档（1x 引擎与设置 / 2x Quick-Full / 3x 页级复用 / 4x 状态进度错误 / 5x 降级 / 6x 项目链路 / 7x 分发前置 / 8x 回退） |
| **目的** | 这条要证明什么（一句话，能反推"失败长什么样"） |
| **操作** | 可执行步骤：优先给**能跑的入口**（单测 / `#[ignore]` 真机用例 / headless CLI / 真机 GUI 步骤） |
| **期望** | 正确的行为（用户可见层面） |
| **判据** | 可判定、二值的断言（含反例自证：怎样算失败） |
| **优先级** | **P0** = 接入即必须过（不过就是用户可见错误或静默退化）；**P1** = 可延后但必须记录 |
| **依赖** | 前置（其它 ID、T1/T3/T4 的产出、产品决策） |
| **证据·落点** | 现有落点（file:line）或已有实测数字；**未实测的标 `[推断]`** |

> 全部条目均为**设计**，不是实测结果；本册不含任何"已验证"声明。

---

## 1. 引擎选择与设置（`Engine::Tectonic` 的接入面）

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-10 | 枚举与序列化正确 | `cargo test -p latteset-core engine` 扩三条：serde 往返 `"tectonic"`、`binary_name()=="tectonic"`、`writes_xdv()` 的语义被**明确拆开** | 新增变体后原有三引擎行为逐条不变；Tectonic 的 `latexmk_flag()` 不再被使用（不能返回一个假的 `-X` 参数） | 单测：`serde_json::to_string(Engine::Tectonic)=="\"tectonic\""`；`Engine::Tectonic` 不实现/不使用 `latexmk_flag()`（编译期保证：Tectonic 分支不走 latexmk）；原有 `engine_latexmk_flags` / `only_xelatex_writes_xdv` 三条断言仍绿 | P0 | 无（纯单测） | `types.rs:17-59,300-322`；`[推断]` "`writes_xdv` 需重定义"——Tectonic 产 XDV 但**是否落盘取决于 `-k`/`--outfmt`**（T-b/T-f） |
| INT-11 | **XDV 落盘在 Tectonic 上不可能**（Rev.2 实测推翻原设计）⇒ `writes_xdv()` 必须为 `false`，但 `-k` 仍必须带（为了 `.aux`/`.toc`） | 单测：`Engine::Tectonic.writes_xdv() == false`；同时断言 argv 含 `-k` | 页哈希路径**永不**对 Tectonic 生效（避免读陈旧 XDV）；Quick 的前置探测与收敛判据仍有输入 | 断言 `writes_xdv()==false` **且** argv 含 `-k`；反例自证：去掉 `-k` 后 `tmp/<stem>.aux` 不存在（探针 p1 已实测） | P0 | INT-30, INT-31 | T-p（`driver.rs:1985`）+ 探针 p1/p2/p4；B8、B9 |
| INT-12 | 设置三层都能选 Tectonic 并真生效 | ① 面板选 Tectonic（全局）；② 项目 `.latteset/settings.json` 写 `{"compile":{"engine":"tectonic"}}`；③ `update_settings {engine:"tectonic"}` | 三条路径都让下一次编译用 Tectonic（`CompileRequest.engine` 实际变化） | 单测：`SettingsStorage::effective` 合并后 `compile.engine==Tectonic`；真机：切换后编译日志出现 Tectonic 专属分支（INT-05）；headless：`latteset-cli --config-dir <带 tectonic 的配置> compile` 的 JSON `status` 语义与 GUI 一致 | P0 | 无 | `settings/model.rs:16-91`、`settings/merge.rs`、`src-tauri/src/commands.rs:475-560` |
| INT-13 | 前端能选、且旧值不炸 | `SettingsPanel.vue` 加第 4 个 option；用一份**含 `"engine":"tectonic"` 的 settings.json** 启动**当前构建**（尚无该变体） | 面板显示 4 个引擎；未知/未来的引擎值**不得**导致整份设置被重置 | 单测：`serde_json::from_str::<Settings>(含未知 engine)` 失败时，`load_global` 必须**逐字段容忍**（保留 `timeout_secs` 等其它字段），不得 `save_global(default)` 覆盖写盘；反例自证：用现状代码跑该用例应当**失败**（写出 timeout 120 覆盖用户的 300） | P0 | 无（是"加枚举值"即触发的既有缺陷，不修则降级/手改必踩） | B17 `storage.rs:41-67,91-106`（当前整份重置 → `[推断]` 现状 FAIL） |
| INT-14 | 项目覆盖的逐字段清洗不因新枚举值退化 | 项目覆盖里放 `{"compile":{"engine":"tectonic", "timeout_secs":9999}}` | 非法字段被清、合法字段（engine/root_file）保留 | 单测：`sanitize_overrides` + `load_overrides` 后 `engine==Some(Tectonic)`、`timeout_secs==None`、`root_file` 不受影响 | P1 | INT-13 | `settings/validate.rs:55-110`、`modules.md §12.2` |
| INT-15 | 运行中切引擎不改已构造的请求 | 真机：编辑触发编译（Tectonic）**运行中**把引擎切成 XeLaTeX | 运行中那次仍用 Tectonic 跑完（快照语义）；下一次触发用 XeLaTeX | 断言日志两次 `Quick/Full 编译（…engine…）` 的引擎名按时间线正确；页哈希缓存分别落在 `tmp/<stem>.tectonic.pages` 与 `tmp/<stem>.xelatex.pages` | P0 | 无 | `types.rs:249-262`（请求携带快照）、`compose.rs:150-191`（构造后改设置不影响请求）、B8 |
| INT-16 | 引擎未安装时的文案必须指对方向 | 把 `engine=tectonic` 指向不存在的可执行（用临时 PATH 或未安装机器） | 错误列表出现**可读中文**：「找不到 tectonic（Tectonic 未安装或路径不对）…」，并给出下一步；不得说"TeX Live 未安装？" | 真机：错误列表 1 条且 `message` 含 "tectonic"、**不含** "TeX Live"；同判据对 latexmk/xdvipdfmx 三条既有文案不回归 | P0 | 无 | B15 `runner.rs:321,322,538`、B16 `actor.rs:250` |
| INT-17 | **必须有可见指示：当前引擎 + 编译强度**（D3 已裁决） | 真机：切到 Tectonic → 编译一次 Quick，再编译一次 Full；再切回 XeLaTeX 重复 | 状态栏**同处**显示引擎名，并体现本次强度（Quick/单趟 vs Full/完整） | ① 不打开设置面板也能在状态栏看到引擎名（切到 Tectonic 后文案含 `Tectonic`）；② 强度可见：Quick 成功后引擎名与「引用待更新」chip **同时**可见，Full 成功后 chip 消失；③ 切回 XeLaTeX 后文案随之变化。**反例**：现状 `StatusBar.vue` 无引擎字段 ⇒ `[推断]` 现状 FAIL | P0 | D3（已裁决） | B18 `StatusBar.vue:48-99`、`useIdleConvergence.ts:20-48`（draft 语义） |
| INT-18 | headless 与 GUI 不漂移 | `latteset-cli --config-dir … compile` / `latteset-mcp` 在 Tectonic 配置下跑同一夹具 | 同一 runner 分支、同一产物路径、同一退出码语义（0=通过、1=未通过） | CLI 退出码 0 且 `pdf_path` 指向项目根 `<stem>.pdf`；`compile_get_errors` 与 GUI 错误列表条数一致 | P1 | INT-12 | `modules.md §8.1,§12.2`（CLI 退出码语义冻结） |
| INT-19 | 恢复默认仍回到 XeLaTeX | 面板「恢复默认」 | 默认引擎仍是 `xelatex`（Tectonic 是**显式选择**，不得成为默认） | 单测 `Settings::default().compile.engine==XeLaTeX`；真机：恢复默认后面板选中 XeLaTeX | P1 | 无 | `settings/model.rs:32-45`、`SettingsPanel.vue:114` |

---

## 2. Quick/Full 映射：`-r` 与「引用待更新」

**映射结论（Rev.2：按 T1 的 E2.10 与本册探针 p1–p5 修订）**：

| 档 | 建议命令（`cwd=项目根`） | 语义（实测趟数） | 与 XeLaTeX 的对应 |
|---|---|---|---|
| **Full** | `tectonic -C -o tmp --synctex --keep-logs -k -p <rel>` | default pass：自动收敛（≤6 趟）+ bibtex/biber + 内置 xdvipdfmx 出 PDF（实测 hello 2 趟；28 页档 3 趟 + 9 次 BibTeX） | ≈ `latexmk` 收敛 |
| **Quick** | `tectonic -C -o tmp --synctex --keep-logs -k -p -r 0 <rel>` | 首次之后**不重跑** ⇒ 单趟排版 + 出 PDF（文档需要 bibtex 时**仍会跑一次 bibtex**，只是不再重排版） | ≈ `xelatex -no-pdf` + 这一次的内部转换 |

**两条硬结论（它们改掉了本册 §3 的全部判据）**：

1. **`--outfmt xdv` 不是"只排版"档**：它照样自动收敛（p5：hello 2 趟；thesis 3 趟 + 9 次 BibTeX），只省掉 xdvipdfmx，而且**不出 PDF**。
2. **PDF 档拿不到 `.xdv`**（T-p：转换后从内存文件表删除，`-k` 无效）⇒ 路线① 下**页哈希恒为空** ⇒ `pdf-updated.pages == 0`（"无法判定"）⇒ B/C 不生效，**A 明确标「该引擎不支持」**（不是"未触发"、更不是"静默跳过"）。恢复 B/C 只能走**独立分支路线②**（`--pass tex` / `--outfmt xdv` + 转换方案，P1），见 INT-35/INT-37/INT-38/INT-39 与 §9 反例清单。

> **路线①（P0，已裁决 = 本册默认）**：`--pass tex` **不得**当 Quick——它**不跑 xdvipdfmx ⇒ 没有 PDF**（T-d / p3）。`--pass tex` 与 `--outfmt xdv` 是**路线②（P1 独立分支）**的原料，**不得**出现在路线① 的命令里（INT-37）。`-p/--print` 是**页进度的唯一来源**（T-q），两条路线都必须带；代价是引擎原始输出混进 stdout，其流式性由 INT-40 验。
>
> **env 口径（D4=(b) 已裁决）**：`SOURCE_DATE_EPOCH` **按子进程施加**——**Tectonic 进程不设**（它把该变量当正文时间源，设了就把 `\today` 印成 1970）；同一次编译里若还要调**外部 `xdvipdfmx`**（路线②），**该进程设 `0`**（㉚ 的逐字节确定性由它承担）。现实现是无条件设置（`runner.rs:275`）⇒ 必须改成分步传 env，**P0 前提而非可选优化**（INT-20b）。

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-20 | 命令构造正确且可单测 | 扩 `compile_command` 单测：Tectonic 的 Full/Quick argv 逐项断言 | argv 与上表一致；不含 `latexmk`、不含 `-no-pdf`、不含 `xdvipdfmx`；Tectonic 分支**不得**进入 `writes_xdv()` 的页哈希/转换路径 | 断言 `argv == ["tectonic","-C","-o","tmp","--synctex","--keep-logs","-k","-p", …]`；Quick 追加 `-r 0`；`Engine::Tectonic.writes_xdv() == false`（T-p：PDF 档无 XDV）；**env 按子进程断言**（见 INT-20b）——**不得**断言 `SOURCE_DATE_EPOCH` 出现在 Tectonic 进程上 | P0 | T1 E2.10（已给）、INT-20b | `runner.rs:235-280,826-840`；T-p/T-q；探针 p4 |
| INT-20b | **`SOURCE_DATE_EPOCH` 必须按子进程施加**（D4=(b) 落地要求，**P0 前提**）＋**确定性比较的两条硬纪律** | 单测断言每个子进程的 env：Tectonic 主进程**不含**该变量；路线② 的外部转换进程（`xdvipdfmx`）**含 `0`**。真机做字节比较时**必须同一输出路径** | Tectonic 的 `\today` = 当天；㉚ 的逐字节确定性由**转换步骤**承担，而该步骤**不是天生确定的**（必须显式设 epoch） | 断言：① Tectonic 命令 env **无** `SOURCE_DATE_EPOCH`（现实现 `runner.rs:275` **无条件**设置 ⇒ **该断言现在应当 FAIL**，落地时改成分步传 env）；② 路线② 的外部转换步骤 env 含 `SOURCE_DATE_EPOCH=0`；③ 同一编译里两个子进程的 env 互不影响（环境捕获断言，不必真跑引擎）；④ **比较纪律**（**针对转换产物**）：转换步的字节级比较必须**固定输出路径 + 固定 epoch**（T1 E3.9：同一路径 + epoch=0 连跑 **3 次 SHA-256 全等**（79,065 B）；**不设 epoch** 两次**不等**（79,073 B）⇒ 转换步非天生确定；**换输出目录**则字节在 79,059–79,071 间漂 ⇒ **假阴性**；另证 TL2026 的 `xdvipdfmx` **认** `SOURCE_DATE_EPOCH`）。**注意与 XDV 的区别**：`-o` 路径**只影响转换产物**，XDV 侧**不依赖**输出路径（T1 新实测：输出到两个长度差 30+ 字符的目录 → XDV 逐字节相同 264,728 B / `738F96DD…71FEB`）⇒ XDV 的纪律见 INT-36④（只需固定 epoch） | P0 | D4=(b)（已裁决）、T1 E3.9/E3.10、INT-36/36b/35 | `runner.rs:275`（现状无条件）、`runner.rs:523-548`（`convert_xdv` 的 env 位置）、T1 E3.9/E3.10 |
| INT-21 | `tmp/` 自己创建（Tectonic 不做这件事） | 冷项目（无 `tmp/`）在 Tectonic 模式下编译 | 编译成功、`tmp/` 被创建 | 真机 + `#[ignore]` 集成用例：断言 exit 0 且 `tmp/<stem>.pdf` 存在；反例自证：**去掉 `create_dir_all`** 后必现 `error: output directory "tmp" does not exist`（**已实测 3 次**：探针首轮 + thesis 首次 + T1 E7.6）⇒ 该测试抓得住漏建（手法同已知债 #25 "先测它在旧行为下会失败"） | P0 | 无 | T-a；`runner.rs` 全文件**无** `create_dir` 调用（现状依赖 latexmk 建目录）；§0.1.1 实测 |
| INT-22 | 首编必须升级为 Full（不能静默退化） | 干净项目第一次编辑触发（Quick 请求） | 升级 Full、`draft=false` | 日志出现「无构建产物，Quick 升级为 Full」；`compile-status` 的 `draft=false`；`tmp/<stem>.aux` 确实被产出（前提：命令带 `-k`——**已实测**：不带 `-k` 时 1 页文档 outdir 只有 `.log`+`.pdf` 并打 `Skipped writing 1 intermediate files`；带 `-k` 才有 `.aux`） | P0 | INT-11, INT-20 | `runner.rs:293-298`、`modules.md §12.2`；T-f、探针 p1/p2 |
| INT-23 | 第二次编辑触发**真的**是 Quick（不许恒升级） | ① Tectonic Full 成功一次（带 `-k`）；② 改一个字触发 | 第二次是单趟、`draft=true` | 断言日志「Quick 编译（单趟直调引擎）」+ `draft=true`；**反例判据**：若出现"无构建产物，Quick 升级为 Full"即 FAIL（**已实测**：不带 `-k` 时 `.aux` 不落盘 ⇒ Quick 必然恒被升级，纯性能静默退化） | P0 | INT-11, INT-22 | B7 + T-f（探针 p1/p2 已证 `.aux` 依赖 `-k`） |
| INT-24 | Full 的收敛语义与 XeLaTeX 基线一致 | 同一夹具（`bench/multifile` 或 T3 指定），Tectonic Full（带 `-k`）vs `latexmk` **收敛**档 | 目录/交叉引用页码收敛到同一值（页数允许差 1） | 解析 `tmp/<stem>.toc` 的目标页码逐条相等（**前提 `-k`**：`.toc` 在 `ALWAYS_INTERMEDIATE_EXTENSIONS` 里，不带 `-k` 恒被跳过；带 `-k` 实测落盘 1623 B）；stdout 含 ≥1 条 `Rerunning TeX because`。**基线口径警告**（T1 E1.5）：XeLaTeX 侧必须是收敛档，且两边包版本不同（Tectonic `LaTeX2e <2021-11-15>` / bundle BibTeX 0.99d vs 本机 TL2026），差异必须逐条归因 | P0 | T3 夹具与对拍脚本；T1 E1.5 | `design.md §附`（单趟 vs 收敛的 `.toc` 29→41）、T-c、T-r |
| INT-25 | Quick 的"落后一趟"可复现 | 插 400 行后 Tectonic Quick（`-r 0 -k`），然后 Full | Quick 后 `.toc` 保持**上一轮**页码；Full 后追上 | `.toc` 目标页码（需 `-k`，T-r）：Quick 后 == Full 前，Full 后 == 新值；UI 期间显示「引用待更新」 | P0 | INT-23 | `design.md §延迟预算实测附节`、`types.rs:61-76`、T-r |
| INT-26 | `draft` 只在**真单趟**时报 | 三种情形各跑一次：Quick 未升级 / Quick 升级 / 失败 | `draft=true` 仅第一种；失败与运行中**不清除**已有提示 | 单测（store 已有三条）+ 真机事件序列：`success{draft:true}` → 收敛 `success{draft:false}`；`failed` 后 `draft` 保持 true | P0 | 无 | `src/stores/compile.ts:15-46`、`__tests__/compile.spec.ts:45-70` |
| INT-27 | 空闲收敛仍走 Full | 草稿成功后停手 2s | 触发一次 Full，成功后「引用待更新」消失 | 真机时间线：`success(draft)` → Δ≈2s → `running` → `success(draft:false)`；判定用 `fdb_latexmk` 等价物不适用（Tectonic 不产）→ 改用 stdout 的 `Rerunning TeX`/`-r` 缺失证据 | P0 | INT-25 | `useIdleConvergence.ts:20-48`、`design.md §附`（Δ≈2.0s 实测） |
| INT-28 | Tectonic 的"自动收敛"不得污染 Quick 语义 | 同一源连跑：Quick（`-r 0`）两次，观察 `.toc` 与趟数 | Quick 永远不自己收敛（页码保持落后），收敛只由 Full/空闲收敛提供 | **已实测**：`-r 0` = 1 趟 TeX（探针 p4）⇒ 两次 Quick 后 `.toc`（需 `-k`）不变、日志无 `Rerunning TeX because`。**两点限定**：① `-r 0` 仍会跑一次 bibtex（T1 E6.4）；② **`--outfmt xdv` 不是单趟**（p5：照样自动收敛），不得拿它当 Quick | P0 | INT-20 | T-c、T1 E2.10/E6.4、探针 p4/p5 |
| INT-29 | bib/biber 场景下 Quick 的可用性 | 夹具 `small-article`（bibtex）：编辑正文 → Quick；再 Full | Quick 后引文/页码可能落后**但不崩**；Full 后全部正确 | **夹具有效性前置门**（T1 G3 修订版）：用**产物**证明 bib 真的成功——`tmp/<stem>.bbl` 存在且 `\bibitem` 条数 == `.bib` 条目数（本册实测 9/9）；**不得**用 `.blg` 的 `You've used N entries`（Tectonic 的 BibTeX 端口无该统计块 ⇒ 100% 假阴性，本册实测该行不存在）。通过后：断言 Quick 后 PDF 可渲染、无 `??` 泛滥（与 XeLaTeX 同档对比）；Full 后引用编号与页码与 XeLaTeX 基线一致 | P1 | T1/T3（bib 夹具与基线）、INT-43 | `modules.md §12.1 #3`；T1 G3/G6；本册实测（`.bbl` 9 条 `\bibitem`、`.blg` 无摘要行） |

---

## 3. 页级复用 A/B/C：**可用性门禁与降级**判据（Rev.2）

> **Rev.2 变更缘由**（T1 G5 + 本册探针，两条独立证据）：原设计假定"用 `-k` 让 PDF 档也落 XDV"，**该假定已被推翻**——`xdvipdfmx_pass` 结束时把 XDV 从内存文件表删除（`driver.rs:1985`），`-k` 无效（探针 p2：带 `-k` 仍无 `hello.xdv`；28 页档有 `main.toc` 无 `main.xdv`）。⇒ **产品形态（要出 PDF）下页哈希恒为空**：`pages == 0`（无法判定）⇒ 前端每次全量刷新；B/C 不生效、A 无对应步骤。故本节判据改为"**验门禁与透明降级**"，把"生效判据"降为条件性门禁（INT-35）。
> 同时修正 `modern-engines-zh.md §8.4` 的口径：那里"XDV 就是我们的格式 / A/B/C 无需改代码"只对 **`--outfmt xdv` 档**与**页哈希解析代码**成立，对产品默认档**不成立**（请 T5 在最终文档里加这一句限定，不必改研究报告本身）。
>
> **Rev.4（D2 裁决落地）**：本节分**两条路线**——**路线①（P0，默认）** = `--outfmt pdf -k -p`（Quick 加 `-r 0`），接受 B/C 失效与 A 不支持（INT-30..34/36）；**路线②（P1，独立分支）** = 另立"有 XDV 的路线"以恢复 B/C，必须自带**触发条件、前置探测、成功/失败判据、与① 的互斥与切换**，且转换器只在用户本机装了 TL 时才存在（INT-35/37/38/39）。**两条路线不得混测**：页哈希不可跨档比较（INT-34 的不变量保留，按档隔离基线）。

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-30 | 确认 PDF 档**没有** XDV（B/C 无输入；这是**预期**，不是缺陷） | 路线① 的 Full（`-k -p`）成功后列出 `tmp/` | `tmp/<stem>.xdv` 不存在；也不生成 `tmp/<stem>.tectonic.pages`；**A 能力被显式标为「该引擎不支持」** | ① 断言 `tmp/<stem>.xdv` 不存在（实测：p2 与 28 页档）；② 断言无 `.tectonic.pages`（`write_pages_cache` 对空表提前返回）；③ **A 的缺失有显式登记**（能力表/文档/提示文案写"该引擎不支持"，而不是留空、不触发或静默跳过）；**反例判据**：若 XDV 存在 → 说明命令混进了 `--outfmt xdv`/`--pass tex`，此时**必定没有新 PDF** | P0 | D2（已裁决：路线① 为 P0） | T-p、探针 p2/thesis、`runner.rs:571-580` |
| INT-31 | **陈旧 XDV 不得被当作本次产物**（正确性，不是性能） | ① 先用 `--outfmt xdv` 在项目 `tmp/` 留下 `<stem>.xdv`；② 改文档；③ 跑 Tectonic Full（pdf 档） | 前端按"无法判定"全量刷新；**不得**出现 `pages>0 && changed_pages == []`；全程不调用 `xdvipdfmx` | ① 断言 `pdf-updated.pages == 0`（且 `changed_pages` 空——两者同时成立才是"无法判定"）；② 断言日志不含「跳过 xdvipdfmx 转换与 PDF 拷贝」且不出现 `xdvipdfmx` 调用（TL-less 机器上那必然 `无法启动 xdvipdfmx`）；**反例自证**：若 `writes_xdv()` 对 Tectonic 返回 true，第 ③ 步会读出陈旧 XDV 的哈希 → 与上一轮相同时报"逐页未变"→ 前端**跳过重载** → 屏幕上是旧 PDF 而文件已改（比 `runner.rs:463-466` 记录的"覆盖 PDF"更隐蔽） | P0 | INT-30 | T-p、B9（`runner.rs:463-466` 的既有事故）、`types.rs:138-153`、`actor.rs:461-480` |
| INT-31b | **页哈希口径必须逐用例声明**（原始 vs 归一化）——纪律项，**口径选择权不在本计划** | 每个涉及页哈希/页级差分的用例，在文案里显式写"原始口径"或"归一化口径" | 读者能判断期望值属于哪一种，不会把"prev 连锁"当成 bug | 判据：① 用例**必须声明口径**；② **原始口径 = 含 prev 的 `bop..eop`**，**归一化口径 = `bop+45..eop`**；③ 本仓现实现是**原始**口径（`crates/latteset-core/src/xdv.rs:211`），已登记为已知债 **#26**（`docs/modules.md`）；④ 队长已裁决：**归一化口径的选择权归本仓 #26，不在 Tectonic 计划内** ⇒ 本册只声明、不改判定 | P0 | 队长裁决（归一化归 #26）、P-G19 / 纪律 6（T5 编号） | `crates/latteset-core/src/xdv.rs:211`（原始口径）、`docs/modules.md` #26 |
| INT-32 | B 的**降级**判据（不是生效判据） | 大夹具：改一行注释 → Tectonic Full | 每次全量刷新（正确性优先）；不静默跳过重载 | 断言 `pages == 0` ⇒ 前端 `reloadKey` 每次递增；**禁止**在 Tectonic 模式下断言 `skippedReloads` 增长（那条判据属 XeLaTeX 路径） | P0 | INT-30 | `types.rs:145`（`pages==0` = 无法判定 → 全量刷新） |
| INT-33 | C 的**降级**判据 | 74 页夹具改**末章** → Tectonic Full | 全量重绘（不复用 canvas 位图） | 断言 `pdf-updated` 不携带页级差异（`pages == 0`）；前端 `pagesReused` 不增。收益损失按 design.md 口径量化（render 64ms / 7 页窗口；XeLaTeX 侧 C 实测 102ms→7ms）并记入 §11 未验证清单 | P0 | INT-30 | `incremental-edit-x-dvi.md §2.2/§2.3`、`modules.md §9.4` |
| INT-34 | 切引擎时基线不串 | XeLaTeX 编译 → 切 Tectonic → 再编译 | Tectonic 侧既不读 XeLaTeX 的 `.pages`，也不写自己的 | 断言 `tmp/<stem>.xelatex.pages` 存在且 mtime 不变；无 `tmp/<stem>.tectonic.pages`；Tectonic 首轮 `pages == 0` | P0 | INT-31 | B8（按 `binary_name()` 分文件）、`modules.md §12.1 #25` |
| INT-35 | **路线②（P1 独立分支）**：恢复 B/C 的分支判据。**定位 = "有 TL 机器上的可选转换路径"**（**不是**"免装 TL 的出路"：T1 E2.12 实测，Tectonic 的 XDV 把字体记成**裸名**（`FandolSong-Regular.otf`、`lmroman12-regular`，无路径），外部转换成功**靠 TL 的字体树 + kpathsea** 解析，且 `xdvipdfmx.exe` 本身随 TL 分发——与 G2（synctex 依赖 TL）同类） | 仅当产品实现路线② 时执行；前置探测 = 本机存在能解析本文档字体的 XDV→PDF 转换器（Tectonic **没有**可单独调用的转换步骤：转换合在同一进程里；只有装了 TL 才有 `xdvipdfmx`，见 INT-38/38b） | B/C 恢复（`pages > 0`、`changed_pages` 正确、跳过转换/重载的日志证据） | 四条**全中**才算通过：① **触发条件** = 显式开关，**不得自动选择**；② **前置探测** = 转换器存在**且能解析本文档字体**才可启用，缺失时给可见提示并**退回路线①**（INT-38）；③ **成功判据** = 由**收敛档**产出的 XDV（`--outfmt xdv` 默认 pass）→ **外部 `xdvipdfmx` 转换 exit 0 且 PDF 页数 == XDV 页数**（**已实测**：264,728 B XDV → exit 0、617–1,091 ms、79,068/79,072 B、`pdfinfo` 28 页 == `xdv-report` 28 页、stderr 零 error、嵌入字体与 Tectonic 自带 PDF **同一组无替换**、`pdftotext` 去空白后逐字符相同（各 13,559 字符），字节差异只因转换器版本不同）→ 再套 `incremental-edit-x-dvi.md §2.3` 三条口径；**不得**拿 `--pass tex` 单趟 XDV 转出的 PDF 当最终产物（本夹具单趟 **26 页** vs 收敛 **28 页**，27/28 页不同 ⇒ 用户会看到未收敛的文档）；④ **失败判据** = 转换失败必须进错误列表且**不得**发 `pdf-updated`（否则预览指向不存在的 PDF）。**与① 互斥**：同一轮只能走一条（INT-37/39）。**当前未实现 ⇒ 不作验收项** | P1 | D2（已裁决：记 P1）、INT-37/38/38b/39 | T1 E2.12/E2.13（字体裸名 + 等价性）、T1 E3.9（转换步确定性：同路径 + epoch=0 三次 SHA-256 全等；不设 epoch 两次不等；换输出目录漂 79,059–79,071 B）、路线② 转换实测（队长 1,091 ms / 79,072 B；本册 617 ms / 79,068 B / 28 页）、T-p、探针 p3/p5 |
| INT-36 | **确定性断言在 (b) 下的正确形态**：**XDV 逐字节稳定**，PDF **不保证** | 不设 `SOURCE_DATE_EPOCH`、**同日**连跑两次同一源；**同一输出路径**；比对 XDV 的 SHA-256（对 PDF 只做非断言性观察） | XDV 逐字节相同；PDF 允许不同 | 断言：① 两次 **XDV** SHA-256 相等（样本 264,736 B）；② **禁止**断言"两次 PDF 逐字节相等"——实测差异 **56 B / 80,509 B（0.07%）**（trailer `/ID` + Info 时间戳；长度与 `pdftotext` 文本相同）；③ 跨天/会印日期的文档按 INT-31b 声明**原始/归一化**口径；④ **只需固定 epoch，输出路径无关**——**XDV 侧已实测不依赖输出路径**（T1 新证据：同源 + `epoch=0`，输出到两个**长度差 30+ 字符**的目录 → XDV **逐字节完全相同**，264,728 B / `738F96DD…71FEB`）；"同路径"那条纪律**只对转换步成立**（INT-20b④） | P0 | D4=(b)（已裁决）、T1 E3.1/E3.6/E3.9、INT-31b | 队长裁决 D4=(b)；T1 实测（56/80,509 B = 0.07%；E3.9 + XDV 路径无关性） |
| INT-36b | **D4=(b) 落地：Tectonic 档不固定 epoch** —— 正文日期必须正确，确定性义务转移给 XDV 与转换步骤 | ① 不设 epoch 跑一次，取 `pdftotext` 的日期串；② 同日连跑两次比对 XDV；③ 路线② 时给外部转换进程设 `0` | `\today` = **当天**；XDV 稳定；转换步骤承担 ㉚ | 断言（**只有 (b) 下成立的**）：① `pdftotext` 的日期串 == **当天**（**不得**出现 `1970 年 1 月 1 日`）；② 两次 XDV SHA-256 相等；③ Tectonic 子进程 env **不含** `SOURCE_DATE_EPOCH`（INT-20b）；④ **禁止**断言 PDF 逐字节相等（INT-36）；⑤ 比较时**固定输出路径与 epoch**（INT-20b 的纪律；路线② 的转换步必须显式设 `0`，否则两次结果不等）。**反例自证**：给 Tectonic 进程设 `0` ⇒ 文档印 `1970 年 1 月 1 日`（本册实测：80,203 B vs 80,509 B；去空白文本首个差异 @18） | P0 | D4=(b)（已裁决）、INT-20b、INT-31b/36 | 队长裁决（2026-09-14）、T1 E3.6/E3.9、本册实测（`_t2-probe/r2/{nodate,witdate}`）、`runner.rs:275` |
| INT-37 | **路线决议落地 + 两条路线不得混测**（D2 已裁决） | ① 断言路线① 命令（`--outfmt pdf -k -p`，Quick 加 `-r 0`）里**不含** `--outfmt xdv` / `--pass tex` / `xdvipdfmx`；② 若实现路线②，断言其命令与① 不同且**不同轮**出现 | ① 是本册默认，也是 P0 验收对象；② 是 P1 分支，可后做但不得污染① | 判据：命令构造单测**按档**断言（同一次编译不得同时假设两档的产物）；页哈希**不得跨档比较**（`.pages` 基线按档隔离，或② 用独立缓存文件，见 INT-39）；`writes_xdv() == false` 时任何 `.pages` 读写路径都不得被走到 | P0 | D2（已裁决）、INT-38/39 | T-b/T-d/T-p；探针 p3/p4/p5 |
| INT-38 | 路线② 的**前置探测**：转换器来源必须可判定，且**要能解析本文档用到的字体** | 在"有 `xdvipdfmx`"与"无 `xdvipdfmx`"两种机器上各跑一次路线② 开关；前者用**本文档实际用到的字体**验证 | 有且可解析 → 允许启用；无或解析不了 → **拒绝启用**并给可见提示，自动退回路线① | 断言探测结果驱动开关可用性（不可用时开关禁用，或点了给明确提示）；不可用时的命令里**不得**出现 `--outfmt xdv` / `--pass tex`；**反例**：无转换器仍走② → 得到"有 XDV、无 PDF" ⇒ 预览永远不更新。**机制（T1 E2.13）**：Tectonic 的 XDV 里字体是**裸名**（`FandolSong-Regular.otf` / `lmroman12-regular`，后者连扩展名都没有），要靠 **kpathsea + TL 字体树**才能解析（实测 `kpsewhich FandolSong-Regular.otf` → TL 路径）；而 XeLaTeX 的 XDV 记**绝对路径**（`C:/Windows/fonts/simsun.ttc`）⇒ **"能解析"≈"本机有 TL"**，探测必须按此判定，不能只看 `xdvipdfmx` 是否存在 | P1 | D2（P1）、INT-35、INT-38b | T1 E2.12/E2.13、T-d（`--pass tex` 无 PDF）、探针 p3/p5 |
| INT-38b | **"无 TL"的外部转换路线**（bundle 字体目录喂给第三方转换器）——路线② 能不能脱离 TL | 造最小文档：只用 **bundle 专属字体**（本机 TL 之外）→ `--outfmt xdv` 出 XDV → 用**不依赖 TL 字体树**的转换器（或给 `xdvipdfmx` 显式喂字体目录/map）转换 | 明确结论：可转 / 不可转。**若不可转**（当前倾向），路线② 的定位必须写成"有 TL 机器上的可选路径"，并在开关处说清 | 断言：转换 exit code + `pdfinfo` 页数 == XDV 页数 + `pdftotext` 无缺字（配合 INT-66）+ 嵌入字体无替换（与 Tectonic 自带 PDF 对比，手法同 T1 E2.12）；**若失败**：该失败必须能被 INT-38 的前置探测拦住（否则用户开② 后每次编译都失败） | P1 | T1 §4.A 6b（已列此待测项，归 T2/T4）、INT-38 | 已通过样本 = Windows 系统字体 + TL Latin Modern（靠 TL 解析）；**bundle-only 字体未验** |
| INT-39 | 路线② ↔ ① 的**切换与基线隔离**（含代价说明） | ① 跑一次路线②（产生 `.xdv` 与页哈希）；② 切回路线① 再编译；③ 再切回② | 切换不产生误判：不拿② 的页哈希判① 的产物，反之亦然 | 断言：① 后存在 `.xdv`（及启用时的页哈希缓存）；② 路线① 编译时**不读**该 `.xdv`（INT-31 的陈旧 XDV 规则在此同样适用）；③ 路线① 恒 `pages == 0`、路线② 可 `pages > 0`，两条互不污染；④ **② 内部也不得混档**：`--pass tex` 单趟 XDV（26 页）与 `--outfmt xdv` 收敛 XDV（28 页）**页哈希不可互比**（T1 实测 27/28 页不同）；⑤ **口径与跨天纪律**：涉及页哈希的用例必须**显式声明原始/归一化口径**（INT-31b；口径选择权归本仓 **#26**，本仓现为原始口径 `xdv.rs:211`）——`\today` 在首屏 ⇒ 跨天时**页 1 与 prev 连锁页（页 3..N；页 2 逐字节相同）**会变；声明纪律见 P-G19 / 纪律 6（T5 编号）。**代价（判据取舍用）**：外部转换 **+0.6–1.1 s/次**（两次独立测量：617 ms / 1,091 ms；路线① 没有这一步）⇒ 路线② 的价值是**换回 B/C**，不是更快 | P1 | D2（P1）、INT-31/34/35 | 路线② 转换实测（队长 1,091 ms；本册 617 ms）、T1 E2.12/E3.6、T-p、B8、`types.rs:138-153` |

---

## 4. 编译中状态 / 进度 / 错误（Tectonic 的通道差异）

> **核心差异（Rev.2，已按 T1 G1/E5.2/E5.3 + T2 探针校验）**：① Tectonic 的 `.log`/`.pdf` 都在**内存层**，整个 session 结束才一次性落盘（`FilesystemIo::new(..., writes_allowed=false)`，`driver.rs:1121-1205`；只有 `driver.rs:1506/1528` 调用 `write_files`）⇒ 现有"尾随 `tmp/<stem>.log`"的实时通道**命中 0 次**；② `note:` 走 stdout、`warning:`/`error:` 走 stderr（T-i），但 **stdout 里没有 `[N]` 页标记**，除非加 `-p/--print`（T-q）；③ 失败路径也会写 `.log`（`driver.rs:1506` 用 `only_logs=true` + `--keep-logs`）⇒ **终态**错误清单仍可读。⇒ 结论：**实时页进度的唯一来源是 `-p` 的 stdout**，其"是否逐字节流式"必须实测（INT-40）。

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-40 | 实时页进度可达（唯一通道 = `-p`） | 大夹具（T3 的 400KB/162 页档）Tectonic 首编；注入 `window.__TAURI__.event.listen` 记录 `compile-progress` 与到达时刻 | `running` 后进度单调递增到接近总页数，且明显早于终态 | ① 命令含 `-p/--print`（单测断言）；② 时间线：`running` 后 ≤1.5s 收到首个 `pages>=1`，首个 `[N]` 远早于进程退出、最后 3 个 `[N]` 到达时间**方差 > 0**（否则是"结束前一次性 dump"= T1 E5.5 的缺口）；③ **多趟重数**：实测 28 页档 `-p` 下 stdout 有 110 个标记（前三个 `[1] [1] [1]`）⇒ 必须靠"只升不降"保证 UI 不回落（见 INT-44） | P0 | T3 夹具、T1 E5.5 | T-q（实测）、`driver.rs:1168-1172`、`io_base/stdstreams.rs:29-34`、`modules.md §12.1 #20`（XeLaTeX 实测） |
| INT-41 | 实时错误可达 | 注入 `\undefinedmacrohere` | `compile-errors` 在进程结束前到达 | 断言「首条实时错误时刻 + 100ms < 终态时刻」（复用现有 `-- --ignored live_errors` 口径）；前提是 `-p` 把引擎的 `! …` 行带到 stdout（T-j），且 `LiveFeedback` 的"行首 `!` 立即解析"路径仍然生效 | P0 | INT-40 | `runner.rs:1347`（现有 XeLaTeX 用例）、`modules.md §2.6.1`、T-j |
| INT-42 | 终态错误列表完整可读 | 制造内容错误（缺宏包/语法错） | `errors-updated` 非空、含警告清单（`Overfull` 等）、**不是** IoError | 断言：错误条数 > 0 且 `kind==content_error`；**不得**出现「编译失败且无法读取日志」；命令含 `--keep-logs`（单测断言，T-e）；依据：失败路径确实写 `.log`（`driver.rs:1506`，`only_logs=true`） | P0 | INT-20 | `runner.rs:373-399`、`driver.rs:1506`、`design.md §错误列表` |
| INT-43 | bib 的成败判据必须走**产物级**（日志与 `.blg` 都不可信；T1 G3 修订版 + E6.2d 定案） | 两个夹具：(a) 正常项目（bib **成功**）；(b) `\bibliography{nosuchrefs}` / 坏 `.bib`（bib **失败**） | (a) 不报任何 bib 相关警告；(b) 给出 ≥1 条可见错误 | **(a) 成功档**：`<outdir>/<root_file stem>.bbl` 存在且 `\bibitem` 条数 == `.bib` 条目数（本册实测 9/9）——**定位规则是 `<outdir>/<root_file stem>.bbl`**（`tmp/main.bbl`、`tmp/thesis.bbl`、`tmp/中文主文件.bbl`；**不得硬编码 `main.bbl`**，T1 E6.2d）；另断言 `pdftotext` 命中 `参考文献`、`main.log` 的 `Citation … undefined` == 0 ⇒ UI **0 条** bib 警告。**(b) 失败档**：`Citation … undefined` > 0（或 `.bbl` 缺失/条目数 0）⇒ UI ≥1 条可见条目。**负例断言**：全量构建后 `tmp/**/chapters/*.bbl` **必须不存在**（章级空跑被 ``note: Not writing `chapters/chNN.bbl`: it would be empty.`` 跳过，T1 E6.2d 实测 8 条 ch01..ch08）——该 note 只作**辅助断言**（它显式给出路径），**不作**成败判据（与 G6 同源：日志自述不可信）。**两条禁止**（做了必错）：① 计日志里的 `errors were issued by BibTeX` —— 成功档 **8 条假阳性**（章级空 aux）、失败档**无信号**；② 查 `.blg` 的 `You've used N entries` —— Tectonic 的 BibTeX 端口无该统计块 ⇒ **100% 假阴性** | P0 | T1 G3/G6、E6.2d | T-s（本册实测：`Running BibTeX`=9 / 小写警告=8 / `.blg` 无摘要行 / `.bbl` 680 B 9 条）、`driver.rs:1644,1652-1654,1949-1959` |
| INT-43b | **判定器回归样本**：`成功但无摘要行` 的 `.blg` 必须判 `unknown`，**不是** `fail`（T1 G6 的落地面） | 把 Tectonic 的 `main.blg`（537 B、无 `You've used`）喂给任何"bib 成功判定器" | 判 `unknown`（信息不足），且**不**产生 UI 警告 | 断言判定器返回 `unknown`；**反例判据**：判 `fail` → 用户每次成功构建都收到报错（跨引擎日志契约不同 ≠ 数据损坏） | P1 | INT-43、T1 G6 | 本册实测（`.blg` 无摘要行）、T1 G6 |
| INT-44 | 多趟重跑时页数不回落 | Tectonic Full（自动收敛多趟，`-p`） | 状态栏页数只升不降 | 事件序列里 `pages` 单调不减；**实测依据**：3 趟运行中 stdout 的 `[N]` 从 `[1]` 重新数起（p/q 实测）⇒ `PageMarkerScanner` 的"只升不降"是硬需求 | P0 | INT-24, INT-40 | `modules.md §2.6.1`、`log_parser/progress.rs`、T-q |
| INT-45 | 良性噪音与**假警报**都不能进错误列表（T1 G6 家族） | 用 Tectonic 跑 (a) 中文夹具、(b) 带 bib 的正常项目 | (a) 不显示 `Fontconfig error`；(b) 不显示那 8 条小写 bibtex 假警报 | 断言错误列表不含 `Fontconfig error`，也**不含** `errors were issued by BibTeX`（成功档 stderr 稳定 8 条，本册实测）；G6 同族：Tectonic 的 `.log` **没有** `This is XeTeX …` 抬头 ⇒ 任何"从日志识别引擎/版本"的逻辑必须按"未知"处理而不是报错。**风险提示**：`parse_log` 现状**碰巧**收不到小写 `warning:`（要求字面量 `Warning`）——为其它目的放宽警告判定时必须**同时**加白名单/过滤，否则立刻引入 8 条假警报 | P1 | INT-43 | 探针 stderr（实测，p1–p5 与 thesis 均出现 `Fontconfig error`）、T1 G6、`scan.rs:89-93` |
| INT-46 | 超时诊断在 `.log` 缺失时仍成立 | 夹具：把超时设为 5s 跑大文档（Tectonic） | 证据化诊断仍给"首编/源文件数/可用页证据"，且不误导 | 断言 `diagnosis.suggested_timeout_secs` 有值；`message` **不得**只说"日志无页输出"而不给替代证据（应用 stdout 的 `[N]` 计数）；**反例自证**：Tectonic 被杀后 `.log` 很可能**根本不存在**（内存层未落盘，T1 E7.7）⇒ 现状 `pages_typeset(&log)` 恒为 `None` → 诊断退化为"无证据" → FAIL | P1 | INT-40、T1 E7.7 | `runner.rs:632-686`（`pages_typeset(&log)`）、T-h、`design.md §失败语义` |
| INT-47 | 首跑下载不得被默认超时"判死" | 冷缓存（清空 `%LOCALAPPDATA%\TectonicProject`）+ 默认 `timeout_secs=120` 首跑 | 要么不超时，要么超时后的诊断明确指向"首次下载资源（非排版慢）"并给一键提高超时 | 判据二选一：① 首跑成功；② 失败时错误条目 message/诊断含"下载/获取资源"字样且 `suggested_timeout_secs >= 300`。**反例判据**：出现"疑似卡住（日志无页输出）"这类把下载误判为卡住的文案 → FAIL | P0 | T4（首跑口径）/ 产品决策 | T-n（实测首跑 189s/214s/51s > 120s `timeout_secs` 默认值） |
| INT-48 | 中止与残留 | 编译中按停止 | `aborted`、无残留进程 | `tasklist` 无 `tectonic.exe`；状态 `failed{kind:aborted}`；队列被清空（`design.md §失败语义`）；**预期**：`tmp/` 无半成品 XDV（内存层未落盘，T1 E7.8） | P1 | 无 | `runner.rs:692-710`（`kill_tree`）、`design.md §失败语义` |
| INT-49 | 中间态不顶掉权威列表 | 复刻超时/收尾补发场景（Tectonic 下 `--print` 的 stdout 也可能在终态后才被读尽） | 终态的完整清单不被后来的中间态覆盖 | 断言：`errors-updated` 之后不再接受 `compile-errors`（前端 `phase` 守卫）；事件序列里终态条目数 == 列表最终条目数 | P1 | 无 | `modules.md §12.2`（流式必须走独立事件 + `phase` 守卫；实测 1 条被 30 条覆盖） |

---

## 5. 既有功能在「不读本机 TeX Live」下的降级

> 这一节是**摩擦点 #1** 的集成面。原则：**能力可以没有，但"没有"必须可见且不误导**（错误文案不得指向用户装不了/用不上的工具）。

### 5.1 ㉒ 生成产物与依赖面

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-50 | 触发面不因引擎而变 | 三文件矩阵：改 `main.tex` / 改被引用的 `refs.bib` / 改 `fig.png`，在 XeLaTeX 与 Tectonic 两种引擎下各跑一遍 | 两种引擎的**触发矩阵逐格一致**（现状：只有 `.tex` 触发；`.bib` 靠空闲收敛/手动兜底） | 断言三种改动的 `compile_request_for_change` 结果相同；真机：改 `.bib` 后**不**出现新编译（与 XeLaTeX 现状同） | P0 | 无 | B13、`watch.rs:178`（只认 `.tex`）、`modules.md §12.1 #23`、roadmap ㉜/㉝ |
| INT-51 | 依赖信息在 Tectonic 下的替代面 | 给 Tectonic 命令加 `--makefile-rules tmp/<stem>.deps`，与 XeLaTeX 的 `.fls` 对同一源做差集 | 依赖集合可用于"精确失效"；不给用户添新噪音 | 断言 `tmp/<stem>.deps` 存在、含主输入与项目内被读文件、**不含** bundle 内宏包路径；**不得**把 `.fls` 的解析器（`INPUT ` 行）直接套到 makefile 规则上（格式不同 → 会被静默解析成空集） | P1 | T1（真机产出样本）、产品决策（是否接线 `--makefile-rules`） | T-l、B13、`scripts/fls-report.mjs`、`docs/research/g1-…§3.1` |
| INT-52 | 生成产物永不当作源码打开 | 真机：在 PDF 的目录区 / 参考文献区点击（反向定位命中 `tmp/main.toc` / `tmp/main.bbl`） | 就近回落真实源码并提示"已回落"；落空只给工具条提示，**不打开** `tmp/` 文件 | 断言不出现 `tmp/*` 标签页；`InverseResultDto.note` 非空。**Tectonic 侧差异**：产物同名且 `-k` 下确实存在（实测 `main.toc`/`main.bbl`），但**没有** `.fls`/`.fdb_latexmk` ⇒ 分类器无需改，缺的是依赖面（INT-51） | P0 | INT-53 | B12、`modules.md §12.2`、`synctex/classify.rs:31-75`（实测样本）；探针 thesis |
| INT-53 | 编译中的 SyncTeX 竞争 | 编译进行中连续点击 PDF | 不静默失败；退避重试后成功或给可见提示 | 断言 UI 出现同步提示（工具条 5s 自动消失）而非只有 console；重试次数与退避沿用 100/200/300ms。**Tectonic 侧差异**：`.synctex.gz` 也是 session 收尾才落盘（T-h）⇒ 编译期"文件不存在"而非"被重写"，重试窗口语义变了，必须实测（T1 E4.2 指出该文件由引擎写入、不受 `-k` 影响） | P0 | 无 | B11 `infra/synctex.rs:5-24,19-24`、T-h、T1 E4.2 |
| INT-54 | SyncTeX 的**二进制依赖**必须可见（T1 G2） | TL-less 机器（无 `synctex.exe`）用 Tectonic 编译成功（`.synctex.gz` 有产物）→ 点 PDF 双向定位 | 可见提示说清"本机没有 synctex 工具，故无法双向定位"（或改为随包分发/自解析——产品决策） | 断言失败落到 UI（状态栏/工具条），**不得**只有 `console.error`；`note` 含 "synctex"。**实测依据**：Tectonic 的 26 个 crate 里**没有 synctex 解析器/CLI**，本机 `synctex.exe` 来自 `C:\texlive\2026\bin\windows`（v1.5）⇒ "免装 TeX Live"下该能力直接不可用；另有 `-d` 硬编码待解（INT-68）。**两条"不要拿日志当判据"**（G6）：① `.log` 自述被证伪——本册复现 PDF 档 `main.log` 写 `Output written on main.xdv (28 pages, 264728 bytes).` 而 `main.xdv` **不存在**；② synctex 头里的 **`Output:` 是常量 `pdf`**，与 `--outfmt`/`--pass` 无关（T1 E4.5：连"没有 PDF"的 `--pass tex` 档也写 `Output:pdf`）⇒ 不得据它分支 | P0 | 产品决策（T4：是否随包发 `synctex.exe` 或自解析 `.synctex.gz`） | T1 G2/E4.1/E4.4/E4.5、B11、T-o、本册复现（`_t2-probe/thesis/tprint`） |
| INT-54b | **`--synctex` 对 XDV 字节零影响** ⇒ 页级复用与 SyncTeX **可以并存**，无需为"synctex 会改 XDV"做规避 | 同源、`--pass tex --keep-logs` 与 `--pass tex --synctex --keep-logs` 各跑一次 → 比 XDV 字节；并在带 `--synctex` 的 XDV 里搜源码绝对路径 | 两次 XDV **逐字节相同**；XDV 内**不含**源码绝对路径 | 断言（T1 已实测，本册复核用同口径）：① 两次 XDV SHA-256 相等（样本 242,960 B / `08E7219F…`）；② XDV 内搜不到源码绝对路径段（T1 实测：`tect-thesis` 未命中）；③ **运营结论**：路线② 的页哈希与 `--synctex` 可同时开（INT-35/INT-54 不必互相规避）。**附撤回声明**：T1 此前"`--synctex` 会逐页加 special ⇒ 两次运行不可比"的说法**已撤回**（其对照因 `--synctex` 无影响而本已有效，重跑仍得同一页数结果）——本册不再沿用该说法 | P1 | T1（新实测 + 撤回）、INT-35/INT-54 | T1 E3 证据段新增条目（`--synctex` 零影响 + 撤回） |

### 5.2 ㉖ 模板 `.cls` 探测与缺文件诊断

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-55 | 根文件探测与引擎无关 | 同一夹具（唯一/多候选/无候选三形态）在两种引擎下打开项目 | `root_candidates` 与 `root_file` 判定完全一致 | 单测（core `root_detect`）+ 真机：`get_project` 返回的候选逐条相同 | P0 | 无 | `project/root_detect.rs:22-104`（纯文件系统/正则）、`[推断]` 无引擎耦合 |
| INT-56 | 缺 `.cls` 的文案不得指向 `tlmgr` | 夹具：`\documentclass{nosuchthesis}` 且项目里有 `nosuchthesis.ins` → Tectonic 编译 | 用户看到"确认项目根目录 / 源码版模板需先生成 .cls"等**可执行**建议；不出现 `tlmgr` | 断言 Tectonic 模式 `diagnosis.hint` 不含 `tlmgr`；`diagnosis.kind` 仍为 `MissingClass`（前端渲染契约不变） | P0 | 产品决策（文案分流） | B14 `diagnosis.rs:394`、`runner.rs:587-624`（读项目根） |
| INT-57 | 缺宏包的文案同理 | `\usepackage{no-such-package-xyz}` | 建议为"bundle 内不含该宏包 / 需联网获取"或"切到 XeLaTeX 用本机 TeX Live" | 断言 hint 不含 `tlmgr`；`kind==MissingPackage` 不变 | P1 | INT-56 | B14 `diagnosis.rs:383` |
| INT-58 | 引擎不匹配的诊断要覆盖 Tectonic | `\documentclass` 要求 XeTeX 而用户设了 pdfLaTeX | 建议里可选项包含 Tectonic（若产品把 Tectonic 列为可选引擎） | 断言 hint 提到的引擎名集合 ⊆ 面板可选引擎集合 | P1 | INT-19 | B14 `diagnosis.rs:328-341`、`docs/research/template-corpus-survey.md`（19 模板 0 例自动推断） |
| INT-59 | 真实 Tectonic 错误语料的诊断命中率 | 把 T1 采到的 Tectonic 失败日志喂 `parse_log`+`diagnose`（缺包/缺类/缺文件/语法/字体五类） | 命中率不低于 XeLaTeX 语料基线；未命中退回原文+行号（不瞎猜） | 与 `real_error_corpus.rs` 现有基线逐类对拍，给出命中数与漏检清单；漏检项登记为 P1 | P0 | T1（Tectonic 日志语料） | `log_parser/real_error_corpus.rs`、`design.md §错误列表`（22 类） |

### 5.3 ㉗ `.ins`/`.dtx` 源码版模板提示

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-60 | `.ins`/`.dtx` 识别逻辑不变 | 复用 core 既有四条单测（同名 `.ins` 优先 / 单一 `.ins` / 只有 `.dtx` 不给错命令 / 多候选不猜） | 引擎无关地保持 | `cargo test -p latteset-core source_release` 全绿 | P0 | 无 | `diagnosis.rs:110-143`、`diagnosis_tests.rs:294-347` |
| INT-61 | 提示里给出的**命令必须能跑** | Tectonic 模式下缺 `.cls` 且项目里有 `X.ins` | 提示给出的命令在"不读本机 TL"的机器上可执行（`tectonic X.ins`）**或**明确说"需要本机 XeLaTeX" | 断言 hint 里的命令前缀 ∈ {`tectonic`, 显式提示切引擎}；**反例判据**：仍给 `xelatex X.ins` 而机器无 xelatex → FAIL | P0 | T1（Tectonic 能否编 `.ins` 生成 `.cls`：docstrip 只用 TeX 原语，`[推断]` 可行但未实测） | B14 `diagnosis.rs:132-136`、`runner.rs:618-621` |
| INT-62 | 生成出来的 `.cls` 能被后续编译用上 | 先 `tectonic X.ins`（或切 XeLaTeX 生成），再回 Tectonic 编译主文档 | 主文档编译成功，用到项目内生成的 `.cls` | 断言项目根出现 `X.cls` 且主文档 exit 0、无 `File 'X.cls' not found` | P1 | INT-61 | `[推断]`（依赖 T1 真机） |

### 5.4 其它"本机 TL"假设

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-63 | 字体集建议不误导 | `fontset=fandol` / `fontset=windows` 建议 | Tectonic 下建议仍可行（bundle 是否含 fandol 未验证） | 断言 hint 不指向用户装不了的东西；若 fanbundle 不含 fandol → 改为 `fontset=windows`（系统字体） | P1 | T1（bundle 内是否有 fandol） | B14 `diagnosis.rs:358,368`、`modern-engines-zh.md §8.4`（中文走 `fontset=windows` + fontconfig，0 缺字） |
| INT-64 | 非 UTF-8 源文件的建议仍成立 | GBK 源 + 各引擎 | 建议仍为"另存 UTF-8 或改用 XeLaTeX"（Tectonic 是 XeTeX 同源，应同样宽容） | 断言 hint 不指向不存在的引擎；真机验证 Tectonic 对 GBK 源的实际行为（记入 T1 未验证清单） | P1 | T1 | B14 `diagnosis.rs:320-326`、`unicode_path_tests.rs:12-14` |
| INT-65 | 依赖工具链缺位时不静默 | `scripts/fls-report.mjs` 在 Tectonic 项目上跑（无 `.fls`） | 工具明确报"没有 `.fls`"，不静默输出空报告 | 断言脚本非零退出或明确提示（另注：T1 发现该脚本头部用法 `--diff <b>` 空格形式**静默不生效**，只认 `--diff=<b>` ⇒ 任何按文件头写的差分用例会永远绿，见 T1 §0 尾） | P1 | 无 | B13、`scripts/fls-report.mjs`、T1 E2.4 |

### 5.5 静默失败类（T1 G3/G4 + SyncTeX 细则的落地面）

> 这一组不是"功能缺失"，而是"**看起来成功、实际不对**"——最坏的失败形态。判据统一是"**至少一条可见警告**"。

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-66 | 缺字（`Missing character`）不得静默 | 夹具：`\setCJKmainfont{不存在的字体}` 或生僻字（T1 E1.4 的三段式夹具） | 编译"成功"但 UI 有可见警告 | 断言日志含 `Missing character` **且** UI 出现警告级条目；**反例自证**：现状 `scan.rs` 无该规则、流式通道只收 Error（`runner.rs:169-184`）⇒ 0 条 → FAIL | P1（失败形态最坏：静默缺字） | T1 G4/E1.4 | `log_parser/scan.rs:31-109`、`runner.rs:169-184`、T1 G4 |
| INT-67 | `-Z deterministic-mode` 与 SyncTeX **互斥**，产品选择必须落地 | 分别以开/关生成 synctex，解压比 `Input:` 行 | 产品**不同时**主张"逐字节确定"与"SyncTeX 可用"；我们的确定性靠 `SOURCE_DATE_EPOCH`（已有），**禁用** `-Z deterministic-mode` | 断言命令构造里不出现 `-Z deterministic-mode`；INT-36 的确定性用例只用 `SOURCE_DATE_EPOCH=0`（Tectonic 自述该模式会破坏 synctex 的绝对路径） | P0 | T1 E3.3 | `tectonic -Zhelp` 原文、`runner.rs:275-280` |
| INT-68 | SyncTeX 数据目录不能硬编码 `tmp/` | 若 outdir ≠ `tmp`（Tectonic 默认 outdir = 输入目录；或 T4 的形态变化），跑 forward/inverse | `-d` 指向**运行期实际 outdir** | 断言 `synctex_dir()` 从运行期参数取（现状是 `pdf.parent()/tmp`）；反例：outdir 一变，forward 成功率 0%（T1 E4.4） | P1 | INT-54 | B11 `synctex.rs:29-31`、T1 E4.4 |
| INT-69 | Tectonic synctex 的**空 `Input:` 记录**不得让"回落"误报 | 用 Tectonic 的 `.synctex.gz`（PDF 档实测：非空 `Input:` 9 条 / **空 132 条** / `!` 58 条；`--pass tex --synctex` 档：空 **121 条**、非空路径末段用 `/`）做正/反向定位 | 命中项目内真实源码，或给出确切提示 | 断言 `source` 要么是项目内真实路径、要么 `None` + 非空 `note`；**不得**返回空串路径或把空 `Input` 当文件名；**不得依据 synctex 头的 `Output:` 字段分支**（T1 E4.5：该字段恒为 `pdf`，与档位无关） | P1 | T1 E4.2/E4.5 | T1 E4.2/E4.5（实测）、`synctex/provider.rs:66-102` |

---

## 6. 项目打开全链路（根文件、中文路径、子目录 include、关闭/重开）

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-70 | 中文目录 + 中文文件名 + 中文子目录全链路 | 真机 / headless：`E:\项目\中文测试工程\中文主文件.tex` + `\include{章节/第一章}`，engine=tectonic | exit 0；`tmp/中文主文件.{pdf,log,aux,toc}` 与项目根 `中文主文件.pdf` 均产出 | 断言上述产物存在（**注意：不含 `.xdv`**，T-p）+ PDF 可渲染；沿用 XeLaTeX 侧既有断言集合（去掉 XDV 那条） | P0 | T1（Tectonic 对非 ASCII 输入路径的实测） | `runner.rs:1420-1467`（中文路径实测基线）、`unicode_path_tests.rs:8-14`、T-p |
| INT-71 | **搜索根差异**：项目根的子目录 `\include` | 夹具：`main.tex`（项目根）+ `\include{章节/第一章}`，engine=tectonic | 编译成功、章节内容在 PDF 里 | 断言 exit 0 且 PDF 文本含章节标题；**若失败** → 判定为"必须传 `-Z search-path=<项目根>`"或改产品约定，并把结论回写本册 | P0 | T1（这条是本册最高风险未验证项） | T-k（filesystem_root = 主输入父目录）`[推断]`：主输入在项目根时搜索根==项目根 ⇒ 大概率通过，但**未实测** |
| INT-72 | **嵌套根文件**（`css/thesis.tex`） | 夹具：根文件在子目录，正文用项目根相对路径 `\input{chapters/x}` | 编译成功 | 断言 exit 0；**反例判据**：不传 `-Z search-path` 时若报 `File 'chapters/x.tex' not found`，则此条必须记为"需 `-Z`（不稳定选项）或禁止嵌套根"，交 T5 决策 | P0 | T1、产品决策 | T-k、`runner.rs:220-230`（现有 latexmk 路径**支持**嵌套根文件——行为差异必须显式处理） |
| INT-73 | 项目切换（"关闭/重开"）不串味 | A 项目 engine=tectonic → 打开 B 项目（engine 默认）→ 回到 A | 每轮 `settings.compile.engine` 与 `CompileRequest.engine` 正确；页哈希分文件基线不串 | 断言三次编译的引擎按时间线正确；`tmp/*.pages` 命名按引擎；B 项目里**不生成** `tmp/*.tectonic.pages` | P0 | INT-12 | `commands.rs:118-176`、`modules.md §12.2`（设置读入口必须 `load_global`） |
| INT-74 | 重启应用后重开项目 | 关掉应用 → 重开 → 打开同一项目 | 全局设置生效（仍 Tectonic）；下载缓存复用（不再下载） | 断言第二轮首跑 stdout 中 `note: downloading` 行数 == 0；编译耗时回落到正常档 | P1 | INT-75 | T-n、`modern-engines-zh.md §8.2`（缓存 426 文件/62MB 复用） |
| INT-75 | 每次 `open_project` 重扫不影响引擎 | open → 关（切项目）→ open 同一项目 3 次 | 根文件/候选稳定；不因 Tectonic 新增状态（缓存/下载标记）产生跨项目污染 | 断言 `root_candidates` 三次一致；无"上次项目的 Tectonic 状态"残留（编译日志里引擎名与设置一致） | P1 | 无 | `commands.rs:118-196`、`modules.md §12.1 #14` |
| INT-76 | 未确定根文件时不启动引擎 | 多候选项目（不选根）→ 点编译 | 提示可读、**不**起 tectonic 进程 | 断言 `compile_now` 返回 `Invalid("未确定根文件，无法编译")` 且日志无 `tectonic` 进程启动 | P0 | 无 | `commands.rs:334-354`、`StatusBar.vue:65-72` |

---

## 7. 分发前置的集成面（bundle 存在性 / 首跑下载 / 离线开关 / 缺失降级）

> 边界：**打包与许可是 T4**；本册只写"应用要检查什么、给用户看什么、怎么测"。首跑时长的测量口径归 T3。

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-80 | bundle 存在性前置检查 | 把 `-b` 指向不存在/损坏的 bundle → 编译；并覆盖四种形态：目录 / `.zip` / `.ttb` / `.tar` | 编译前（或失败时）给中文提示："随包资源缺失/损坏：期望路径 X"；不让用户看到英文 CLI 错误原文；**`.tar` 必须被拒**（只有 dir/`.zip`/`.ttb` 是合法形态） | 断言用户可见文案含期望路径；**反例判据**：① 只显示 `` `X` doesn't specify a valid bundle.`` → FAIL；② `-b x.tar` **静默回退到默认 bundle** → FAIL（会偷偷联网下载，违反离线承诺） | P0 | T4（bundle 落地形态与路径） | T-m、T1 E7.4/E7.5（`bundles/src/lib.rs:275-287` 只认 dir/`.zip`/`.ttb`） |
| INT-81 | 首跑下载有可见反馈 | 清空缓存后首跑（联网） | 状态栏/错误面板有**单调递增**的可见反馈（文件计数或"正在获取资源 N 个"），不是"排版中…"静止 3 分钟 | 断言 `running` 期间至少有 ≥3 次可见反馈更新（现有 `compile-progress` 只报 `[N]` 页标记 → Tectonic 首跑无页标记 ⇒ `[推断]` 现状零反馈）；反馈内容含下载计数 | P0 | INT-82、T1（`downloading` 行的准确形态） | T-n（逐文件 `note: downloading`）、T-h/T-i（stdout 实时） |
| INT-82 | 离线开关（仅用本地缓存） | 设置项"仅使用本地资源"打开 → `-C`；断网编译 | 断网也能编；缓存缺失时给出**明确**提示（"离线模式且缓存里没有 X"） | 断言命令含 `-C`；断网 + 缓存齐 → exit 0；断网 + 缺资源 → 错误条目含"离线/缓存"字样 | P0 | T4（离线策略） | `compile.rs:183-185`（`-C` 会打印 `using only cached resource files`）、`modern-engines-zh.md §8.2` |
| INT-83 | 缓存目录不可写时的降级 | 把 Tectonic 缓存目录设成只读（或用 FakeFS 注入失败） | 可读诊断，不 panic；提示缓存不可写的路径 | 断言错误列表 1 条含缓存路径；无 panic/无英文栈 | P1 | T1 | `%LOCALAPPDATA%\TectonicProject\Tectonic\cache`（`modern-engines-zh.md §8.2`） |
| INT-84 | 不污染项目目录 | 编译前后对项目做文件快照（含隐藏项） | 只新增白名单：`tmp/**`、项目根 `<stem>.pdf`（+ 页哈希缓存 `tmp/<stem>.tectonic.pages`） | 断言新增文件集合 ⊆ 白名单；不得出现 `Tectonic.toml`、`.tectonic/`、bundle/format 缓存落在项目内 | P1 | 无 | `modules.md §12.3`（文件树排除规则）、`T-b/T-f` |
| INT-85 | 网络半失败可重试且有证据 | 慢网/丢包下首跑 | 最终成功，或失败时给出重试证据 | 断言 stderr 出现 `failure downloading … (n/N)` 时可重试；成功/失败都有记录 | P1 | T1 | `crates/bundles/src/ttb_net.rs:201`、`itar.rs:66` |
| INT-86 | 排障入口：用了哪个 tectonic | 编译日志/诊断 | 包含 tectonic 可执行路径 + `--version` 输出 | 断言日志含绝对路径与版本串（用户可上报） | P1 | 无 | 现状无（`runner.rs` 只记 engine 名）`[推断]` |
| INT-87 | 首次使用引导 | 首次切到 Tectonic（缓存空） | 提示"首次需要下载资源，约数十 MB / 1–4 分钟（实测 51–214s）"，并建议先别把超时调小 | 断言引导文案存在（P1：形态可讨论）；与 INT-47 的超时口径一致 | P1 | T3/T4（体积与时长结论） | T-n（实测 51–214s） |

---

## 8. 回退：Tectonic 不可用/失败时怎么办

> **D1 已裁决（2026-09-14）**：**不自动回退**。失败必须可见；**仅当检测到本机有 TeX Live 时**，在错误条目上给「一键切到 XeLaTeX 重编」（INT-95/96）。方案里**不得**出现"静默回退"。

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| INT-90 | **D1 = 不自动回退**：失败必须可见，且**仅在检测到本机 TeX Live 时**给「一键切到 XeLaTeX 重编」 | 三种失败各跑一次（`tectonic` 缺失 / 超时 / 内容错误），分别在"有 TL"与"无 TL"机器上 | 失败可见；有 TL → 错误条目上出现「切到 XeLaTeX 并重编」动作；无 TL → **不出现**该动作，也不出现任何"已自动回退"文案 | 断言：① 无论哪种情况**都没有**静默回退（引擎状态与用户认知一致）；② 动作可见性 **==** TL 探测结果（INT-95）；③ 点击后的行为按 INT-96。**反例判据**：无 TL 机器上给出"切到 XeLaTeX"按钮 → FAIL（点了必然失败）；出现"已回退"文案而引擎并未改变 → FAIL | P0 | D1（已裁决）、INT-95/96 | `design.md §失败语义`、`runner.rs:321-322`（既有"TeX Live 未安装？"文案） |
| INT-95 | **TeX Live 探测**（决定该按钮出不出现的唯一输入） | 实现探测（PATH 上找 `latexmk`/`xelatex`，或调一次 `--version`）+ 两个环境各验一次 | 结果稳定；不阻塞 UI、不产生可见噪音；探测失败按"没有"保守处理 | 断言：无 TL 的 PATH → `false`；有 TL → `true`；探测超时/权限错误 → 按 `false`（**不得**按 true 而让用户点了失败）；耗时 ≤50ms 或异步（真机测）；**反例**：做成"每次编译前同步扫盘" → 卡 UI | P0 | D1（已裁决）、INT-90 | 现状无该能力（全仓 grep `latexmk` 只在 runner 的命令构造里）`[推断]` |
| INT-96 | 「一键切到 XeLaTeX 重编」的端到端行为 | 有 TL 机器：Tectonic 编译失败 → 点该动作 | 引擎切到 XeLaTeX（**可见指示同步**，INT-17）→ 触发一次编译 → 结果可见 | 断言：① 切换的持久化语义明确（全局设置 or 项目覆盖，二者选一并在文档写明）；② 触发的是 **Full**（用户此刻要正确结果，不是 Quick）；③ 成功 → PDF 更新且状态栏引擎显示 XeLaTeX；④ **失败 → 再给错误列表**（不得静默、不得自动循环重试）；⑤ 页哈希基线不串（INT-34） | P0 | D1（已裁决）、INT-17/95 | `commands.rs:334-354`（`compile_now` 命令面）、`settings/model.rs`（引擎落盘）、INT-34 |
| INT-91 | 不可用即失败可见 | 模拟 `tectonic.exe` 缺失/无执行权限 | 错误列表 1 条 + 状态栏 `failed` | 断言 `errors-updated` 长度==1 且 message 含 "tectonic"；`compile-status.phase=="failed"` | P0 | INT-16 | B16、`policy.rs:53-59`（IoError → ContentError） |
| INT-92 | 切回 XeLaTeX 后能力等价 | Tectonic 失败 → 设置面板切 XeLaTeX → 重新编译同一项目 | 编译成功、PDF 覆盖、页哈希基线不串、SyncTeX（若本机有 synctex）可用 | 断言新 `tmp/<stem>.xelatex.pages` 生成且首轮 `changed_pages` 为全部页；正向/反向定位各成功一次 | P0 | INT-34 | B8、B11、`modules.md §12.2`（路径与 SyncTeX 策略只有一份实现） |
| INT-93 | 失败后的队列语义与引擎无关 | Tectonic 失败 + 队列里已有新请求 | 直接跑新请求、不重试旧内容 | 断言事件序列 == `[running, running(新), failed/success]`（沿用 `policy.rs` 决策表口径） | P1 | 无 | `policy.rs:37-68`、`actor.rs:591-660` |
| INT-94 | 回退相关文案可读 | 三种失败各一次（不可用/超时/内容错误） | 文案同时说清"哪个引擎失败了""下一步能做什么" | 断言每条 message 或 diagnosis.hint 至少含一个可执行动作（切引擎/提高超时/改源码） | P1 | INT-90 | `design.md §失败语义`、`diagnosis.rs`（22 类诊断） |

---

## 9. 反例清单（写进测试就会被抓到的"退化"）

| # | 反例 | 为什么是退化 | 抓它的用例 |
|---|---|---|---|
| 1 | **Tectonic 模式读陈旧 XDV 算页哈希** | Tectonic 的 PDF 档不产 XDV，但 `tmp/` 里可能留着 `--outfmt xdv`/`--pass tex`/用户留下的旧 `.xdv` → 哈希与上一轮相同就报"逐页未变" → 前端**跳过重载** → 屏幕上是旧 PDF 而文件已改（比 `runner.rs:463-466` 的"覆盖 PDF"更隐蔽） | INT-31 |
| 2 | Tectonic 模式调用 `xdvipdfmx` | TL-less 机器上必然 `无法启动 xdvipdfmx` → 把"引擎成功"报成失败 | INT-31 |
| 3 | 假定"PDF 档也落 XDV、用 `-k` 能补救" | 该假定**已被实测推翻**（T-p：转换后从内存删除）→ 若照此实现，B/C 永远拿不到输入，而用例按"应该生效"写 → 永远红 | INT-30, INT-35 |
| 4 | 命令不带 `-k` | `.aux` 不落盘 → Quick 恒被升级为 Full（省 40% 的核心机制静默失效）；`.toc` 也缺 → 收敛判据无输入 | INT-22, INT-23, INT-24 |
| 5 | 拿 `--pass tex` 或 `--outfmt xdv` 当 Quick | 两者都**不出 PDF**（p3/p5）；`--outfmt xdv` 还**不是单趟**（照样自动收敛） | INT-37 |
| 6 | 缺 `.log` 导致"编译失败却拿不到任何错误" | 退化为 IoError 文案，用户没有改错线索 | INT-42 |
| 7 | 用**日志**或 `.blg` 判 bib 成败 | 两个方向都错：计小写 `errors were issued by BibTeX` ⇒ 成功档 **8 条假阳性**（章级空 aux）、失败档无信号；查 `.blg` 的 `You've used` ⇒ **100% 假阴性**。唯一可用判据是**产物**（`.bbl` 的 `\bibitem` 条数 / `Citation … undefined`） | INT-43, INT-43b |
| 8 | `Missing character` 静默 | 编译成功、预览静默缺字（T1 G4） | INT-66 |
| 9 | 文案指向 `tlmgr` / `xelatex X.ins` / `TeX Live 未安装？` | 在不读本机 TL 的模式下这些命令不存在，用户照做必然失败 | INT-16, INT-56, INT-57, INT-61 |
| 10 | 编译中途没有任何反馈（首跑下载 + 漏 `-p`） | 状态栏静止数分钟，用户以为卡死或以为软件坏了 | INT-40, INT-47, INT-81 |
| 11 | 页哈希缓存跨引擎共用 / 基线混用 | 拿两套口径比大小 → 误判"逐页未变" → 预览不刷新 | INT-34 |
| 12 | 未知 engine 值把整份设置重置 | 用户在 `settings.json` 里试一下新值 / 降级回旧版 → 所有设置被抹掉并写回磁盘 | INT-13 |
| 13 | 静默回退到 XeLaTeX | 用户以为在用 Tectonic（免 TL），实际用了另一条路径 | INT-90 |
| 14 | 把良性噪音（`Fontconfig error`）当致命错误 | 错误列表被噪音占据（已有 27/30 是 Overfull 的前车之鉴） | INT-45 |
| 15 | 用 `scripts/xdv-report.mjs --diff <b>`（**空格**）判页级差分 | 脚本静默返回普通摘要、exit 0 ⇒ 差分用例**永远绿**（必须用 `--diff=<b>`，T1 §0 尾） | INT-30, INT-33 |
| 16 | 产物有效性只按"引擎名"分流，不按"本次运行的档位" | 同一引擎在 `--outfmt pdf` 与 `--outfmt xdv` 下的产物集合不同（T-p）⇒ 按引擎名判断必然出错 | INT-20, INT-31 |
| 17 | 把那 8 条小写 bibtex 假警报直通到 UI | 用户**每次成功构建**都会看到 8 条假警报（`parse_log` 现状**碰巧**收不到它们——要求大写 `Warning`；一旦为其它目的放宽警告判定，必须同时加白名单/过滤） | INT-43, INT-45 |
| 18 | 把"跨引擎日志契约不同"当成"数据损坏" | 例：Tectonic 的 `.blg` 无 `You've used` 摘要、`.log` 无 `This is XeTeX` 抬头（T1 G6）⇒ 判定器必须给 `unknown` 而不是 `fail`，否则每次构建都误报 | INT-43b, INT-45 |
| 19 | 在**没有 TeX Live** 的机器上给出「切到 XeLaTeX」按钮 | 点了必然失败（`latexmk` 不存在），等于把用户推进死路；反之有 TL 却不给按钮 = 少了唯一的自救路径 | INT-90, INT-95 |
| 20 | 两条路线混测（拿路线② 的页哈希判路线① 的产物，或反之） | 页哈希不可跨档比较 ⇒ 误判"逐页未变" ⇒ 预览不刷新；且会掩盖① 应有的"全量刷新"结论 | INT-37, INT-39 |
| 21 | 给 **Tectonic 进程**设 `SOURCE_DATE_EPOCH`（照抄 XeLaTeX 路径） | 该变量在 Tectonic 上是**正文时间源** ⇒ 每份用 `\today` 的文档（学位论文标题页普遍有日期）都印 **1970-01-01**；而 XeLaTeX 下它只动 `/ID`（正文零变化）⇒ **同一变量、两引擎语义不同**（D4=(b) 已裁决：Tectonic 不设、外部转换步骤设） | INT-20b, INT-36b |
| 22 | 把 `.log` 的自述当判据（如 `Output written on main.xdv`） | 本册复现：PDF 档日志写"已写出 main.xdv (28 pages, 264728 bytes)"，而该文件**根本不存在**（G6）⇒ 判据只落产物 | INT-54, INT-43, INT-43b |
| 23 | 把路线② 说成"免装 TeX Live 的出路" | T1 E2.12/E2.13：Tectonic 的 XDV 记**裸字体名**，外部转换靠 **TL 字体树 + kpathsea** 解析，`xdvipdfmx.exe` 本身也随 TL 分发 ⇒ 路线② 与 G2 同类（都要 TL）。按"免装 TL"宣传会让目标用户在没 TL 的机器上开了开关却编不出图 | INT-35, INT-38, INT-38b |
| 24 | 拿 `--pass tex` 单趟 XDV 转出的 PDF 当最终产物 | 单趟 XDV = **26 页** vs 收敛 **28 页**（27/28 页不同，T1 E2.12）⇒ 用户看到的是未收敛的文档；且单趟/收敛两档页哈希不可互比 | INT-35, INT-39 |
| 25 | 跨天对比页哈希/产物而**不声明口径** | `\today` 在首屏 ⇒ 跨天时页 1 与 prev 连锁页（**页 3..N；页 2 逐字节相同**）会变 ⇒ 假的"页变了"；用例必须声明**原始（含 prev，`bop..eop`）/ 归一化（`bop+45..eop`）**口径（INT-31b；口径选择权归本仓 **#26**，本仓现为原始口径 `crates/latteset-core/src/xdv.rs:211`） | INT-31b, INT-36, INT-39 |
| 26 | 断言"两次编译的 **PDF** 逐字节相等" | (b) 下**不成立**：trailer `/ID` + Info 时间戳每次都不同（实测差异 **56 B / 80,509 B = 0.07%**，长度与 `pdftotext` 文本相同）⇒ 确定性断言只能落在 **XDV**（与路线② 的转换步骤）上 | INT-36, INT-36b |
| 27 | **转换步**两次跑写**不同输出目录**再比字节 | 输出路径会进**转换产物**：T1 E3.9 实测换目录后字节在 **79,059–79,071 B** 间漂，看起来像"设了 epoch 也不确定" ⇒ **假阴性**，会把"确定性由转换步骤承担"的正确结论推翻掉。纪律：**转换步**比较必须**同一输出路径 + 固定 epoch**。**注意**：该假阴性**只发生在转换步**——XDV 侧已实测**不依赖输出路径**（换目录仍逐字节相同），XDV 上复现不出来 | INT-20b, INT-36, INT-36b |

---

## 10. 与 T1 / T3 / T4 的接口（需要对方给什么，才能把这些判据变成事实）

| 需要 | 状态与结论（Rev.2） | 影响条目 | 兜底写法 |
|---|---|---|---|
| **T1**：`-r 0` vs `--pass tex` vs `--outfmt pdf -k` 三态裁定 | ✅**已到（T1 E2.10/E6.4，本册探针 p1–p5 复核）**：`-r 0` = 1 趟 + 出 PDF；`--pass tex` = 1 趟 + **无 PDF**；`--outfmt xdv` 仍自动收敛且**无 PDF**；**PDF 档无 XDV**（T-p） | INT-20, 23, 28, 30, 31, 37 | 无（Rev.2 已并入） |
| **T1**：确定性的**分步口径**、转换步与 XDV 的路径相关性（E3.9/E3.10 + 终版） | ✅**已到**：(b) 落地为"Tectonic 进程不设 / 外部 `xdvipdfmx` 设 `0`"；转换步**非天生确定**（同路径 + epoch 三次全等；不设 epoch 两次不等）⇒ **转换产物**须"同路径 + 固定 epoch"；**XDV 侧已实测不依赖输出路径**（换长目录名仍逐字节相同）⇒ XDV 只需固定 epoch；另证 **`--synctex` 对 XDV 字节零影响**（→ INT-54b）。证据侧 T1 `engine.md` **终版** SHA-256 = `B13CCA1006A2BE9666D940CA1212CEB6E23514EEAA18FE873C249B9D0F83852D`（77,491 B；上一版 `C8266DBA…654B` 作废） | INT-20b, INT-35, INT-36, INT-36b, INT-54b | 无：判据已并入（含反例 #27） |
| **T1**：搜索根与嵌套根文件 | 🟡**部分**：T1 夹具 `tect-thesis` 的 `\include{chapters/chNN}` 在"根文件位于项目根"时成功（INT-71 的正面证据）；**嵌套根文件（INT-72）仍未测** | INT-70, INT-71, INT-72 | 保留 INT-72 为"最高风险未验证项" |
| **T1**：失败日志语料、bib 判据与 `downloading` 行形态 | ✅**bib 部分已到且结论被修订**：T1 G3/G6 给出"产物级判据 + 判定器负例（`unknown` 而非 `fail`）"（→ INT-43/43b/45）；🟡`downloading` 行的频率/形态仍为源码级 | INT-43, 43b, 45, 59, 81, 85 | 按 T-n 源码级推断 |
| **T1**：`--synctex` 产物 + `.ins` 编译 | 🟡**部分**：T1 E4.1/E4.2/E4.4 给了 synctex 口径与 `-d` 硬编码（→ INT-68/69）；**`.ins` 生成 `.cls` 仍未测** | INT-53, 54, 61, 62, 68, 69 | INT-61/62 维持 `[推断]` |
| **T3**：页级真值（哪几页应变化）与中文夹具、首跑时长口径 | ⏳待对齐 | INT-33, INT-40, INT-47, INT-87 | 用 `scripts/xdv-report.mjs`（**必须 `--diff=` 等号形式**）自算页集合作对拍基线；注意 Tectonic 默认档无 XDV（INT-30） |
| **T4**：bundle 落地形态/路径、离线策略、是否随包 `synctex.exe`、首跑体积与时长口径 | ⏳待对齐（本册 §7 只写"应用侧检查与可见性"） | INT-54, INT-80, INT-82, INT-87 | 判据写成"存在性检查 + 中文提示"，路径用占位符 |
| **队长/T5**：产品决策 D1 / D2 / D3 | ✅**已裁决（2026-09-14）**：D1 = 不自动回退（仅"检测到本机 TeX Live"时给一键切 XeLaTeX 重编）；D2 = 路线① 落 **P0**、路线② 记 **P1**（独立分支，含触发/前置探测/成败判据/互斥切换）；D3 = 状态栏显示引擎 + 强度。**⚠️ D2 的一处口径补充（T1 E2.12/E2.13，不需新决策，但请并入最终文档）**：路线② 的定位应是"**有 TL 机器上的可选转换路径**"，不是"免装 TL 的出路"（字体裸名 + kpathsea + `xdvipdfmx.exe` 都随 TL） | INT-17, INT-35, INT-37, INT-38, INT-38b, INT-39, INT-90, INT-95, INT-96 | 无需兜底：判据已按裁决改写 |
| **队长/T5**：产品决策 **D4** | ✅**已裁决（2026-09-14）= (b)：Tectonic 档不固定 epoch**。落地要求：① `SOURCE_DATE_EPOCH` **按子进程**施加（Tectonic 不设；同轮的外部 `xdvipdfmx` 设 `0`）——现实现**无条件**（`runner.rs:275`）⇒ 改造成 **P0 前提**（INT-20b）；② (b) 下**成立且必须断言**的：XDV 逐字节稳定（同日两次 SHA-256 相等）、正文日期 = 当天；③ **禁止断言** PDF 逐字节相等（实测差异 56/80,509 B = 0.07%）；④ 涉及页哈希的用例按纪律声明**原始/归一化**口径（归一化的选择权归本仓 **#26**，不在本计划） | INT-20b, INT-31b, INT-36, INT-36b, INT-39 | 无：判据已写实（不再"登记差异"） |

---

## 11. 未验证清单（本册自己的）

1. ✅**已解决（Rev.2）**：~~"Tectonic 的 `pdf --keep-intermediates` 是否真把 `.xdv` 写到磁盘"~~ → **不能**（T-p：`driver.rs:1985` 在转换后把 XDV 从内存文件表删除；探针 p2 与 28 页档均无 `.xdv`）⇒ 结论已并入 §3 / INT-30/31。
2. **搜索根行为**（T-k）：根文件位于项目根时 `\include{章节/x}` 已被 T1 夹具证实可用（INT-71 部分成立）；**嵌套根文件**与 `-Z search-path` 可用性**未实测**——本册 P0 里风险最高的一条（INT-72）。
3. **Tectonic 的实时输出是否够"实时"**：`-p` 走真 stdout，但页标记的刷出节奏、`note: downloading` 的频率与缓冲行为**都未实测**（INT-40/41/81 的时间阈值是占位值，需 T1/T3 校准）。
4. **Tectonic 编 `.ins` 生成 `.cls`** 未验证（INT-61/INT-62 的命令形态待定）。
5. **bundle 内是否有 `fandol`**（INT-63）与字体建议是否仍成立。
6. **超时口径**：默认 120s 与首跑 51–214s 的关系需要产品决策（提高默认值 / 把"首次下载"从超时预算里摘出去）——本册只给判据（INT-47），不给结论。
7. **`--makefile-rules` 的产物能否支撑"精确失效"**（INT-51）：格式、是否含 bundle 内文件、与 `.fdb_latexmk` 的差集，均未实测。
8. **`IoError` 文案改造后的回归面**：现有三条 "TeX Live 未安装？" 文案被真机测试引用（grep 命中 3 处），改动要连带更新（INT-16）。
9. **本册的 P0/P1 划分未与 T1/T3/T4 对齐**（合并时可能升/降级）。
10. **B/C 失效带来的体感损失未量化**：Tectonic 下每次编译预览都全量重载（B 失效）+ 全量重绘（C 失效）。用 XeLaTeX 侧实测可估上界（跳过重载省 fetch 11 + parse 39 + render 64ms；C 省 render 102→7ms @74 页），但真实使用中"无可见变化编辑 / 只影响少数页的编辑"的频率**没有统计**（同 `incremental-edit-x-dvi.md §7 局限 1`）。
11. **`-p` 混流后的解析表现未测**：`-p` 会把引擎原始输出（`[N]`、`!` 行、`l.N`、`(file.name` 括号栈）灌进 stdout，现有 `LiveFeedback` 的去重指纹、4096 回溯窗口，以及 `parse_log` 的括号栈（已知债 #22 文件归属错一章）在这种混流下的表现**未测**。
12. **`.toc` 的存在性只在本机探针上验过**（`-k` 下 1623 B）；若 T3/T5 把收敛/落后一趟判据建在 `.toc` 上，需在各自夹具上复核（不带 `-k` 时它属 `ALWAYS_INTERMEDIATE_EXTENSIONS`，恒被跳过）。
13. ✅**已由 T1 定案（E6.2d）**：章级 aux **不会**写出 `.bbl`——它们既无 `\bibdata` 也无 `\bibstyle` ⇒ bibtex 产出**空** `.bbl` ⇒ `write_files` 以 ``note: Not writing `chapters/chNN.bbl`: it would be empty.`` 跳过（探针 `run.log` 恰好 8 条 ch01..ch08）。⇒ **主 `.bbl` 唯一**，落点 = `<outdir>/<root_file stem>.bbl`（**不要硬编码 `main.bbl`**：`css/thesis.tex` → `thesis.bbl`、`中文主文件.tex` → `中文主文件.bbl`）。INT-43 已据此写死定位规则与负例断言（`tmp/**/chapters/*.bbl` 必须不存在；`Not writing … empty` 只作辅助断言，不作判据——G6）。
14. **(b) 裁决后仍未验的**：① **路线② 的外部转换步骤设 `0` 后 PDF 是否逐字节稳定**——㉚ 的结论是在 `xelatex → xdvipdfmx` 链路上得的，**Tectonic XDV → 外部 `xdvipdfmx`** 未测；② **归一化口径**（`bop+45..eop`）在 Tectonic XDV 上的可用性——**不属本计划**（队长裁决归本仓 **#26**），但 T3 与我们的页级用例若采用归一化，必须先有 #26 的结论；③ 样本只有 1 个 ctexbook 夹具（`\today` 家族：`\year`/`\month` 等未验）。

---

## 12. 给 T5（合并）的说明

- **ID 方案**：`INT-nn`，本册内稳定；分册合并时建议保留前缀（如 `T2-INT-nn`）或统一改为最终文档的编号，但**必须保留一处映射**（其它分册可能已引用）。
- **字段**：7 列（ID/目的/操作/期望/判据/优先级/依赖）+ 可选「证据·落点」列；与其它分册若字段名不同，以本条为准或由 T5 统一。
- **可能与其它分册重叠的条目**（请 T5 去重时注意归属）：
  - INT-20/INT-23/INT-28/INT-37（Quick/Full 映射）与 **T1** 的命令行语义高度相邻——建议 M 层判据归 T1，本册只保留"应用侧映射与 UI 语义"；
  - INT-47/INT-81/INT-82/INT-87（首跑与离线）与 **T4** 的分发面重叠——建议保留本册的"可见性/判据"，把体积与时长数字指回 T4；
  - INT-33/INT-40（阈值）需要 **T3** 的夹具与口径，本册不重复给测量方法。
- **三项产品决策已裁决（2026-09-14，见 §12.3 Rev.4）**：D1 = 不自动回退（仅"检测到本机 TL"时给一键切 XeLaTeX，INT-90/95/96）；D2 = 路线① 落 P0、路线② 记 P1 独立分支（INT-35/37/38/39）；D3 = 状态栏显示引擎 + 强度（INT-17，已升 P0）。

### 12.1 Rev.2（T1 复核后，2026-09）——合并时以此为准

**三处口径变更**：

1. **§2 Quick/Full 映射**：命令改为 `-C -o tmp --synctex --keep-logs -k -p`（Quick 追加 `-r 0`）；**`--outfmt xdv` 与 `--pass tex` 都不出 PDF**，不得当 Quick；`-p/--print` 为**必带**（页进度的唯一来源）。
2. **§3 定位变更**：从"B/C 生效判据"改为"**可用性门禁 + 透明降级判据**"；`INT-35`（B/C 生效）降为**条件性门禁**，当前**不作为验收项**；新增 INT-31 的"陈旧 XDV"正确性用例（P0）。
3. **证据口径限定**：`modern-engines-zh.md §8.4` 的"XDV 就是我们的格式 / A/B/C 无需改代码"必须加限定——只对 **`--outfmt xdv` 档**与**页哈希解析代码**成立，对产品默认档（出 PDF 的形态）**不成立**（T-p）。

**修订过的条目**：INT-11, 20, 21, 22, 23, 24, 25, 28, 29, 30–37, 40–46, 52, 53, 54, 65, 70, 80；**新增** INT-66..INT-69（§5.5 静默失败类）。其余条目未动。

**给最终文档的一句话**：Tectonic 是"能出 PDF、能编中文、能确定性构建"的后端，但**代价是放弃页级复用（B/C）与 `.log` 实时通道**（改用 `-p` 的 stdout，且其流式性待验）；首跑下载（51–214s）与 bundle 许可链决定它能否作为默认形态。

### 12.2 Rev.3（T1 的 G3 修订后，2026-09）——bib 判据整体换面

T1 把 G3 从"bibtex 失败被静默"改成"**引擎自述 ≠ 事实**（新门禁 G6）"，本册随之改动：

1. **INT-43 换判据**：不再"检测 bibtex 失败"，改为"**产物级**判成败"——成功档用 `.bbl` 的 `\bibitem` 条数（== `.bib` 条目数）+ `main.log` 的 `Citation … undefined` == 0；失败档看 `Citation … undefined` > 0 或 `.bbl` 缺失。**明令禁止**两条实现：计日志里的小写 `errors were issued by BibTeX`（成功档 8 条假阳性）、查 `.blg` 的 `You've used`（100% 假阴性）。
2. **新增 INT-43b**：判定器必须对"成功但无摘要行的 `.blg`"返回 `unknown` 而非 `fail`（G6 的落地面）。
3. **INT-45 扩面**：从"良性噪音"扩到"良性噪音 + 假警报"——那 8 条小写 bibtex 警告**不得**直通 UI；并登记 G6 同族（`.log` 无 `This is XeTeX` 抬头）对"识别引擎/版本"逻辑的影响。
4. **INT-29 的门禁换掉**：夹具有效性证明从 `.blg` 的 `You've used` 改为 `.bbl` 的 `\bibitem` 条数。
5. **§9 反例 #7 重写**，新增 #17（假警报直通 UI）与 #18（把日志契约差异当数据损坏）。
6. **本册独立复现**（`_t2-probe/thesis/tprint`）：`.bbl` 680 B / 9 条 `\bibitem`；`Running BibTeX` = 9；小写警告 = 8；`.blg` 无 `You've used`；`Citation … undefined` = 0。

**ID 约定补充**：本次新增用后缀 ID（`INT-43b`）表示"插在同族条目之后"，合并时按同族归并即可。

### 12.3 Rev.4（队长裁决 D1/D2/D3 + T1 E6.2d 后，2026-09-14）——三项决策全部落成判据

**D1 = 不自动回退**：§8 改写——失败可见；**仅当探测到本机 TeX Live 时**在错误条目上给「一键切到 XeLaTeX 重编」；新增 **INT-95**（TL 探测是"按钮出不出"的唯一输入；探测失败按"没有"保守处理）与 **INT-96**（点击后的端到端：持久化语义明确、触发 **Full**、成功/失败都可见、基线不串）；`INT-90` 的反例判据写死"无 TL 却给按钮 = FAIL""出现'已回退'文案而引擎未变 = FAIL"；§9 新增反例 #19。

**D2 = ① 落 P0、② 记 P1**：§3 头部加 Rev.4 说明（两条路线、互斥、不得混测）；`INT-30` 增加"A 的缺失必须**显式登记为「该引擎不支持」**"；`INT-35` 从"条件性门禁"改写成**路线② 的独立分支判据**（触发条件 = 显式开关且不得自动选择；前置探测 = 转换器存在才可启用；成功判据 = B/C 三条口径；失败判据 = 转换失败进错误列表且**不发** `pdf-updated`）；`INT-37` 改为"路线决议落地 + 两条路线不得混测"；新增 **INT-38**（转换器前置探测，无则拒绝启用并退回①）与 **INT-39**（② ↔ ① 切换与基线隔离）；§9 新增反例 #20。

**D3 = 状态栏显示引擎 + 强度**：`INT-17` 从 P1 升 **P0**，判据改为"必须有可见指示"（引擎名 + Quick/Full 强度同处可见，Quick 后与「引用待更新」chip 同时出现、Full 后 chip 消失）。

**T1 E6.2d 收口**：§11 第 13 条标记为**已定案**（章级 aux 不产生 `.bbl`；主 `.bbl` 唯一，落点 = `<outdir>/<root_file stem>.bbl`，不得硬编码 `main.bbl`）；`INT-43` 据此补齐定位规则、`tmp/**/chapters/*.bbl` 负例断言，并把 `Not writing …: it would be empty.` 明确降为**辅助断言**（不许当判据，与 G6 同源）。

**当前条目总数**：83 条（新增 INT-38/39/95/96；另有后缀 ID INT-43b）。

### 12.4 Rev.5（路线② 转换实测 + G6 日志自述 + 新发现 D4，2026-09-14）

1. **路线② 的转换环节已实测通过**（队长：`xdvipdfmx -q -o from-tectonic.pdf main.xdv` → exit 0、1,091 ms、79,072 B；**本册复跑**：exit 0、617 ms、79,068 B、`pdfinfo` **28 页** == `xdv-report` 报的 **28 页**、stderr 零 error、`pdftotext` 中文正常）⇒ `INT-35` 的成功判据写成"XDV → 外部转换 exit 0 且页数一致 → 再套 B/C 三条口径"；`INT-39` 记下代价 **+0.6–1.1 s/次**（路线② 的价值是**换回 B/C**，不是更快）。
2. **`INT-38` 收紧**：前置探测从"有 `xdvipdfmx`"改成"**该 `xdvipdfmx` 能解析本文档用到的字体**"（已通过样本 = Windows 系统字体 + TL Latin Modern）；新增 **`INT-38b`** 收 bundle-only 字体的残余风险（P1 前置实测）。
3. **`Output:` 字段不作判据**（G6）：本册复现 PDF 档 `main.log` 写 `Output written on main.xdv (28 pages, 264728 bytes).` 而 `main.xdv` **不存在** ⇒ 判据只落产物；`INT-54` 已据此改写，§9 加反例 #22。
4. **新发现 D4（需裁决）**：Tectonic 下 `SOURCE_DATE_EPOCH` **会改文档印出来的日期**——不设 = `2026 年 9 月 14 日`，设 `0` = `1970 年 1 月 1 日`；而同设置的 XeLaTeX **不受影响**（㉚ 实测）。产品不变式要求两条路径都固定 epoch ⇒ Tectonic 路径会让每份印日期的文档印 1970-01-01。已落 **`INT-36b`**（P0，先登记差异、D4 定前不判失败）+ §9 反例 #21 + §11 第 14 条。

**当前条目总数**：**85 条**（Rev.5 新增 INT-36b、INT-38b；含后缀 ID INT-43b）。

### 12.5 Rev.6（T1 的 E2.12/E2.13/E4.5，2026-09-14）——路线② 重新定位 + 两处"日志不可当判据"

1. **路线② 重新定位**（`INT-35` 目的列）：从"免装 TL 的出路"改为"**有 TL 机器上的可选转换路径**"。依据 T1 E2.12/E2.13：外部转换之所以成功，是 Tectonic 的 XDV 把字体记成**裸名**（`FandolSong-Regular.otf`、`lmroman12-regular`）而 **kpathsea + TL 字体树**把它解析出来，且 `xdvipdfmx.exe` 随 TL 分发 ⇒ 与 **G2（synctex 依赖 TL）同类**。§9 新增反例 #23。
2. **`INT-38` 收紧到机制层**：探测不能只看"`xdvipdfmx` 存在"，要按"**字体能否解析**"判定（"能解析"≈"本机有 TL"）。
3. **`INT-38b` 改成"无 TL 的外部转换路线"**（T1 §4.A 6b 明确归 T2/T4）：只用 bundle 专属字体的最小文档 + 不依赖 TL 字体树的转换器/显式字体目录，测通才谈得上"无 TL"。
4. **`INT-39` 增加"② 内部也不得混档"**：`--pass tex` 单趟 XDV（**26 页**）与收敛档 XDV（**28 页**）页哈希不可互比（T1 实测 27/28 页不同）；§9 新增反例 #24。
5. **`Output:` 彻底出判据**（`INT-54`/`INT-69`）：T1 E4.5 实测 synctex 头的 `Output:` **恒为 `pdf`**（连"没有 PDF"的 `--pass tex` 档也是），与 `--outfmt`/`--pass` 无关 ⇒ 不得据它分支；且与"`.log` 自述不可信"（本册复现的 `Output written on main.xdv` 而文件不存在）合起来构成 G6 的两条实例。
6. **等价性证据入库**（`INT-35` 证据列）：外部转换 PDF 与 Tectonic 自带 PDF 的 `pdftotext` **去空白后逐字符相同（各 13,559 字符）**、嵌入字体同一组无替换、字节差异仅因转换器版本（`xdvipdfmx (0.1)` vs `(20260113)`）⇒ 属外观差异。

**当前条目总数**：**85 条**（Rev.6 未新增 ID，只收紧既有条目与反例）。

### 12.6 Rev.7（T1 E3.6 加硬 D4，2026-09-14）——变量语义按引擎分支 + 跨天纪律

T1 独立复核了我的 D4 现象（数字一致）并写成 **E3.6（P0）**；据此把 D4 从"确定性的代价"重述为"**同一变量在两引擎语义不同**"，并补两条纪律：

1. **`INT-36b` 重写**：XeLaTeX 下固定 epoch **只动 PDF `/ID`、正文零变化**（所以当年判为免费）；Tectonic 下它**改正文**（`\today`）⇒ **必须按引擎分支**，不得默认沿用 ㉚ 的结论。证据补齐：去空白文本 **13,559 vs 13,560 字符、首个差异 @18 = 日期处、其余逐字符全同**；XeLaTeX 对照 `xepdf/main.pdf`（**同样设 epoch=0**）印 `2026 年 9 月 13 日`（本册已复现）。**现状风险**：`compile_command` **无条件**设该变量（`runner.rs:275`，与引擎无关）⇒ 不加分支即让**所有** `\today` 文档印 1970（T1 记为引擎层 P0 提示，**不是**可选优化）。
2. **跨天纪律（新）**：`INT-36` 与 `INT-39` 各补一条——**任何跨天对比产物/页哈希的用例必须固定 `SOURCE_DATE_EPOCH`**，否则 `\today` 在首屏 ⇒ 第 1 页字节每天变 ⇒ 得到假的"第 1 页变了"（判定本身正确，但用例基线失效）；§9 新增反例 #25（与 D4 是**同一变量的两面**）。
3. **`§10` 的 D4 行同步加硬**：明确"XeLaTeX 只动 `/ID`、Tectonic 改正文 ⇒ 按引擎分支"，并把"不加分支即出错"从提示升为 P0 前提。

**当前条目总数**：**85 条**（Rev.7 未新增 ID）。

### 12.7 Rev.8（D4=(b) 裁决落地，2026-09-14）——env 分步 + 断言写实 + 口径声明

队长裁决 **D4 = (b)：Tectonic 档不固定 epoch**。本册据此（不再"登记差异"，全部改成写实断言）：

1. **env 必须按子进程施加**（新 **`INT-20b`**，P0 前提）：Tectonic 进程**不设** `SOURCE_DATE_EPOCH`；同一次编译里若还要调**外部 `xdvipdfmx`**（路线②），**该进程设 `0`**。现实现**无条件**设置（`runner.rs:275`）⇒ 单测里"Tectonic 进程 env 不含该变量"这条**现在应当 FAIL**，落地时才可转绿（§2 映射表头也加了同一条 env 口径）。
2. **`INT-36` 改成 (b) 下成立的形态**：断言 **XDV 逐字节稳定**（同日两次 SHA-256 相等，264,736 B 样本）；**禁止**断言 PDF 逐字节相等（实测差异 **56/80,509 B = 0.07%**，来自 `/ID` 与 Info 时间戳，长度与 `pdftotext` 文本相同）。
3. **`INT-36b` 切成写实断言**：① 不设 epoch ⇒ `\today` = **当天**（不得出现 1970）；② 两次 XDV 相等；③ Tectonic 子进程 env 不含该变量；④ 禁止 PDF 逐字节断言。反例自证保留本册实测（80,203 vs 80,509 B；去空白文本首个差异 @18）。
4. **页哈希口径声明**（新 **`INT-31b`**，P0 纪律项）：逐用例声明**原始（含 prev，`bop..eop`）**或**归一化（`bop+45..eop`）**；本仓现实现为**原始**口径（`crates/latteset-core/src/xdv.rs:211`），已登记为已知债 **#26**；**口径选择权归 #26，不在本计划内**（本册只声明、不改判定）。`INT-39` 的跨天纪律同步改写为"页 1 与 prev 连锁页（**页 3..N；页 2 逐字节相同**）会变"。
5. **§9 反例**：#21 改为"给 Tectonic 进程设 epoch"、#25 改为"跨天不声明口径"、新增 **#26**（断言 PDF 逐字节相等）；`§10` 的 D4 行标 ✅已裁决并列出四条落地要求；`§11` 第 14 条换成 (b) 下仍未验的三项（外部转换的 PDF 逐字节稳定性、#26 归一化在 Tectonic XDV 上的可用性、`\year`/`\month` 家族）。

**当前条目总数**：**87 条**（Rev.8 新增 INT-20b、INT-31b；含后缀 ID INT-43b）。

### 12.9 Rev.9（T1 E3.9/E3.10 + 比较纪律，2026-09-14）——转换步确定性已实测 + 一条假阴性坑

1. **(b) 的核心假设已被 T1 实测证实（E3.9）**：同一份 Tectonic XDV → **同一 `-o` 路径**、`SOURCE_DATE_EPOCH=0` 连跑 **3 次 ⇒ SHA-256 全等**（79,065 B）；同样的条件**不设 epoch** 两次 ⇒ **不等**（79,073 B）⇒ **转换步不是天生确定的，必须显式设 epoch**；顺带证明 TL2026 的 `xdvipdfmx` **认** `SOURCE_DATE_EPOCH`。⇒ "㉚ 的逐字节确定性由转换步骤承担"这句在 (b) 下**成立**，但前提是**显式设变量**。
2. **新增比较纪律（防假阴性）**：T1 首版测试让两次跑写**不同目录**（`conv-ep-1/` vs `conv-ep-2/`），字节在 **79,059–79,071 B** 间漂，看起来像"设了 epoch 也不确定"——**输出路径会进产物**。⇒ 任何字节级确定性比较必须**同一输出路径 + 固定 epoch**；已写进 **INT-20b④ / INT-36④ / INT-36b⑤**，并立为反例 **#27**。
3. **§10 增一行**（T1 E3.9/E3.10 已到，含 `engine.md` 当前 SHA-256 `C8266DBA…9654B` 便于交叉引用）；**INT-35** 的证据列补上转换步确定性实测（路线② 若要成立，转换步必须显式设 epoch 且同路径比较）。
4. T1 已按其侧更新 `engine.md`（E3.9/E3.10 + E2.10 的口径声明 + §4.C 接口），并声明**本轮之后其册不再改动**（除非新实测推翻某条）。

**当前条目总数**：**87 条**（Rev.9 未新增 ID；反例增至 **27 条**）。

### 12.10 Rev.10（T1 的 XDV 路径无关性 + `--synctex` 零影响，2026-09-14）——措辞从"沿用纪律"改成"实测口径"

1. **XDV 侧已实测不依赖输出路径**（T1 新证据：同源 + `epoch=0`、输出到两个**长度差 30+ 字符**的目录 → XDV **逐字节完全相同**，264,728 B / `738F96DD…71FEB`）。⇒ 据此收紧两处措辞：`INT-36④` 从"按同一纪律执行"改为"**只需固定 epoch，输出路径无关**"；`INT-20b④` 明确"**同路径 + 固定 epoch** 这条纪律**只对转换步成立**"。
2. **反例 #27 限定范围**：该假阴性（79,059–79,071 B 漂移）**只发生在转换步**，XDV 上复现不出来——避免被后人当成通用结论。
3. **新增 `INT-54b`（P1）**：**`--synctex` 对 XDV 字节零影响**（同源两跑逐字节相同，242,960 B / `08E7219F…`；XDV 内**不含**源码绝对路径）⇒ 路线② 的页哈希与 SyncTeX **可同时开**（INT-35/INT-54 无需互相规避）；并**记录 T1 的撤回声明**（"`--synctex` 会逐页加 special"的说法作废），本册不再沿用。
4. **§10 更新**：T1 `engine.md` **终版** SHA-256 = `B13CCA10…852D`（77,491 B；上一版 `C8266DBA…654B` 作废），并写入上述两条新实测。

**当前条目总数**：**88 条**（Rev.10 新增 INT-54b；反例仍 27 条）。
