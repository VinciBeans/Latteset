# 内嵌 Tectonic 引擎（唯一形态）：构建、装配与契约

> **这是什么、给谁看**：阶段②（[ADR-0015](../adr/0015-tectonic-only-embedded-engine.md) 决策 2）之后，Tectonic 以**内嵌**为**唯一引擎形态**。本文写它的构建入口、环境前置、装配点、能力面、风险与验证入口，面向要构建 / 装配 / 复核这条路径的开发者。产品侧行为（Quick/Full 语义、超时与诊断、设置面）见 [design.md](../design.md) §2/§3；runner 的逐层实现契约（输入层、format、页哈希、bib 编排）见 [modules.md](./modules.md) §2.7。
>
> **本文事实有效期**：以 `test_file/phase2/REPORT-gates.md`（2026-11-20，T9）与 `test_file/build-env/REPORT-build-env.md` 的实测为准；这两份是工作区本地证据（未提交）。

## 1. 定位

- **是什么**：把 Tectonic 引擎 crate 直接嵌进产品进程——自持 I/O 层（`IoProvider`：内存直喂输入 + 输出捕获 + format 自管）驱动 `CoreBridgeLauncher` → `TexEngine` → `XdvipdfmxEngine::process`，PDF 收尾原子替换到项目根。
- **唯一形态，不是构建变体**：子进程路径（`tectonic.exe` 命令构造、`-C` 缓存判定、进程树杀、stdout `[N]` 页进度）与 `tectonic-lib` feature / `LATTESET_TECTONIC_LIB` / `settings.tectonic.lib_form` **已全部删除**（`crates/latteset-infra/src/{runner.rs,tectonic/}` 整文件删除；`infra` 不再有 runner / tectonic 模块）。
- **所有构建都需要 vcpkg**：`latteset-tectonic` 是 `src-tauri` 与 `crates/latteset-server` 的**非可选依赖**，不是"发布构建才带"的变体。`npm run tauri dev/build`、`cargo check/test -p latteset-server/-p latteset`、CI 默认 Windows 档、CLI 构建全部要过 §2 的环境前置。
- **不启动外部进程、不读用户 TeX Live**：biber / makeindex / glossaries **不会**被自动运行（biber 的实测口径见 §4.3）。产品**没有回退形态**——bundle/缓存配置错误走 runner 的显式失败文案，不悄悄降级。
- **ADR-0010 的第二落点**：`latteset-infra` 仍是默认依赖图里文件系统与进程的唯一落点；本 crate 是**外部依赖与 C 链的第二个落点**（ADR-0012 的 X-5 例外登记，见 §7）。

## 2. 构建入口（唯一）

**唯一落点**：`scripts/with-tectonic-lib.ps1`。它一次做完三件事：① 校验 vcpkg 检出 / triplet / 端口 / 工具链，**缺项给可执行的修复命令**（而不是让 vcpkg-rs 或 build script 以不指向环境的文案 panic）；② 设 5 个环境变量（`TECTONIC_DEP_BACKEND=vcpkg`、`VCPKG_ROOT`、`VCPKG_DEFAULT_TRIPLET`、`VCPKG_DEFAULT_HOST_TRIPLET`、`VCPKGRS_TRIPLET`）并给 `RUSTFLAGS` 补 `-Ctarget-feature=+crt-static`（triplet 用 static CRT，Rust 侧必须同用，否则链接冲突）；③ 跑对应命令。**别再在别处抄这套环境**。脚本被 dot-source 时只设环境、不跑命令（CI / 手工 cargo 复用同一份口径）。

| npm 脚本 | 透传 | 作用 |
|---|---|---|
| `npm run lib:check` | `-Mode check` | 只校验前置（vcpkg / triplet / 端口 / 工具链），不跑构建 |
| `npm run tauri dev` / `npm run tauri build` | `-Mode tauri dev|build` | 透传 `npx tauri …`；`tauri.conf.json` 的 beforeDev/Build 命令不变 |
| `npm run lib:test` | `-Mode test` | 全仓 Rust 测试：`cargo test -p latteset-core -p latteset-infra -p latteset-server -p latteset-tectonic` |
| `npm run lib:cli` | `-Mode cli` | release 构建 headless CLI（`latteset-cli`） |

**环境前置**：

- **vcpkg 检出**：默认 `$env:VCPKG_ROOT`，否则 `<repo>/test_file/vcpkg`（本机夹具，被 gitignore）；pin rev `a62ce77d56ee07513b4b67de1ec2daeaebfae51a`（与 `.gitignore` 的复现命令、CI 一致）。
- **triplet**：`x64-windows-static-release`，六个直接端口 `freetype harfbuzz graphite2 icu fontconfig libpng`（传递依赖 brotli / bzip2 / dirent / expat / zlib 由 vcpkg 拉）。缺项时 `lib:check` 直接给出 `vcpkg install …` 命令（与 CI 同一条）。
- **工具链**：`cmake` / `pkg-config` / `nasm` 只在**要重建 ports** 时才需要；端口已装好时它们不在 PATH 只产生警告，不阻塞构建。
- **MSVC**：任意 VS 2022/2026 均可（本机实测 MSVC 14.44 与 14.51 都能编）。
- **本机 DSH 会话的额外一步**：若工作区被沙箱打了 **Low 完整性标签**（现象是 `cl.exe D8050` / `lib.exe LNK1104`），必须把 `TMP`/`TEMP` 指到工作区内目录再跑构建：`$env:TMP=$env:TEMP='<repo>\src-tauri\target\.tmp'`。根因、`icacls` 彻底修法与绕法见 [troubleshooting.md](../troubleshooting.md)「D8050 / LNK1104」条。**CI 与普通终端没有这个标签，不需要这一步。**
- **构建代价（三项 `[未测]`）**：干净机器冷/热墙钟、峰值内存、安装包体积增量均未测；CI 内嵌档冷构建记录为 +15–20 min，靠 `rust-cache` 与 `installed/` 缓存缓解（见 `.github/workflows/ci.yml` 的 `build-windows`）。

**CI**：Windows 默认档就是内嵌档——checkout 钉 rev 的 vcpkg → bootstrap → 缓存 `installed/`+`downloads/` → 装六端口 → `-Mode check` → `-Mode test` → `-Mode tauri build`（NSIS 冒烟）。**不再有** `build-windows-tectonic-lib` 窄触发档（D5）。Ubuntu 档只跑前端 + core/infra（server 已非可选内嵌，Linux 不跑 server，R11 决策）。

## 3. 装配点

内嵌唯一后只剩"**每趟按当前设置重建 runner**"这一种装配，没有形态判定、没有 feature 门槛：

| 入口 | 落点 | 契约 |
|---|---|---|
| GUI 主编译 | `src-tauri/src/runner.rs` 的 `EmbeddedRunner`（薄壳） | 只持 `fs / progress / settings / app_cache_dir`；每趟 `compile` 读一次 `settings.tectonic` 的 bundle / cache 后重建 `TectonicLibRunner` ⇒ **资源设置改完即生效、不需要重启**（D1 = R3 选 A） |
| GUI 公式预览 | `src-tauri/src/commands.rs` 的 `build_compile_runner` | 与主编译**共用**同一装配函数；不再有第三处口径漂移 |
| headless（CLI/MCP） | `crates/latteset-server/src/lib.rs` 的 `build_runner` | 只构造 `TectonicLibRunner`；读 `Session.settings.tectonic`（D4） |

**资源优先级（GUI 与 headless 同序）= 显式设置 > 环境变量 > 宿主默认**：

- **bundle**：`settings.tectonic.bundle`（非空）→ `LATTESET_TECTONIC_BUNDLE` → 上游默认。上游默认是**网络地址**（`network-bundle` 特性由 `src-tauri` / `latteset-server` 的 manifest 打开）；本地目录 bundle 必须写 `file:///…` 或相对路径（**绝对 Windows 路径会被上游当 URL scheme 吃掉**）。
- **缓存目录**：`settings.tectonic.cache_dir`（非空）→ `LATTESET_TECTONIC_CACHE` → 宿主应用缓存目录（GUI：Tauri 应用缓存；headless：`<配置目录父目录>/cache`）。缓存**必须**有落点：上游 format 默认落**项目目录**，不注入会把 ~24 MB 的 `.fmt` 扔进用户项目。

## 4. 能力面（阶段②实测口径）

### 4.1 页进度与中间态

- **页进度来自捕获的 XDV 完整页数**：内嵌形态没有子进程 stdout 的 `[N]` 可扫；runner 在**每趟排版收尾**从捕获表里的 `<stem>.xdv` 数完整页（`core::xdv::complete_pages`），数值变大才 `progress.pages` ⇒ `compile-progress`。**粒度 = 趟边界**，与终态页哈希同一份权威数据。
- **编译中预览**（`compile-preview`）：趟边界把已完成页缝成 `tmp/<stem>.preview.pdf` 原子替换；同一进程内引擎有全局锁（`ENGINE_LOCK`），排版趟与转换趟不能并行 ⇒ **只能在趟边界出图**。内嵌唯一后该事件**恒可用**。
- **编译中错误**（`compile-errors`）：由引擎状态后端在 `MessageKind::Error` 时上报**非权威中间态**（无文件/行号、无诊断）；终态 `ContentError` 仍由 `.log` 解析给出完整清单。**提前量未测** `[未测]`（原 `live_errors` 用例已随子进程路径删除）。
- **`compile-notice`**：由内嵌 runner 在**成功路径**发（外部工具能力边界，见 §4.3），GUI 落到状态栏 chip，不进 `errors`。

### 4.2 超时与取消

- **超时证据 = `.log` + 引擎状态环尾部 + 已上报页**：内嵌形态的日志边写边在**内存**里（不像子进程要等收尾落盘），状态后端另外保留状态环；证据合并后交给同一套 `diagnose_timeout` ⇒ 诊断文案以"**引擎输出显示…**"起头，不猜因果。
- **取消 / 超时是协作式的（X-3）**：没有进程可树杀，置位后每个 `IoProvider` 回调入口立刻报错，引擎在**下一个 I/O 回调**处中止。延迟必须如实写进 UI 语义（ADR-0015 负面项）；手动终止仍按设计只呈现 `Aborted`、不产证据条目。
- **真实超时路径 `[未测]`**：由单测（`timeout_evidence_merges_log_and_status_tail_and_observed_pages` 等）覆盖，未构造真实超时真编。

### 4.3 外部工具（重要：内嵌**不跑**外部工具）

- **biber**：内嵌形态**不会调用外部 biber**。T9 实测：本机 PATH 上有真 biber 2.21，编译**不跑它**、**不产 `.bbl`**，只按 `.run.xml` 里的 `<binary>biber</binary>` 发一条 WARN 级 `compile-notice`，`kind` 如实退回草稿态。`backend=bibtex` 的文档走**内建 `BibtexEngine`** 正常生成 `.bbl`（实测 PDF 文本含 `Knuth`/`TeXbook`），**不会**误报 biber。
- **makeindex / glossaries**：同样不自动运行，检出 `.idx` / `.glo` 即 `compile-notice`（索引 / 术语表为空）；**真编夹具 `[未测]`**（仅单测覆盖三类提示）。
- **`.ins` / `.dtx` 源码版模板提示保留**：阶段① `enrich_source_release_hints` 的语义已搬进内嵌 runner 的失败路径（D7），缺 `.cls` 且项目里有同名 `.ins` 时仍给"先执行 `xelatex X.ins`"。
- **文案纪律**：不得写成"引擎会调用 biber"或"引擎不支持 biber"；正确口径是"**引擎依赖外部 biber，而内嵌形态不会替你运行**"。

### 4.4 Quick / Full

- **Quick = 请求强度，不是固定档**：编辑触发请求同样走重跑循环（含 bib 趟），**预算内**收敛就按实际结果报 `full`（前端 `draft=false`，不排空闲收敛）；三重闸门 = `passes < 3` ∧ 第 1 趟 < 1000 ms ∧ 累计 < 2000 ms。超出预算或检出外部工具即停在草稿态。
- T9 实测：热缓存 Quick 请求 75 ms、`upgraded_from_quick=true`（已收敛如实报 full，不虚报草稿）；冷缓存 Full 1,200 ms（format 974 ms + 排版 149 + 转换 76）。

### 4.5 页哈希与 bundle

- **页哈希恒可用**（XDV 在捕获表里）：`page_hashes` 喂 `core::xdv`，缓存名固定 `tmp/<stem>.tectonic.pages`；**空表仍只表示"无法判定"**（XDV 缺失/损坏）⇒ 前端全量刷新，不能当死代码删。
- **bundle 三类**：目录 → `DirBundle`（必须自带 `SHA256SUM`，裸 TeX Live 目录不能直接用）、`.zip` → ZipBundle、`.ttb` → TTBFsBundle，其他扩展名 → `None`；空目录会被静默当合法 bundle（`file_count=0`）⇒ 产品侧自校验 + 分类文案。`-b <本地 .tar>` 不可用。
- **format**：键 = bundle digest（`{digest}-latex-33.fmt`，换 bundle 必换文件），落点 `<cache>/formats/`；算不出缓存落点显式报错，**不落项目目录**。缓存目录无进程间锁，产品侧按缓存目录串行化（引擎全局锁天然串行）。
- **确定性**：库内**禁止** `build_date_from_env`、**必须**显式 `build_date(SystemTime::now())`；"PDF 逐字节相等"不得作为 Tectonic 档的断言（元数据/加密层不可控）。

## 5. 风险与未测

**风险**：

- **bundle 分发方案待③段重定**：[ADR-0016](../adr/0016-ship-bundle-offline.md) 原定"随包完整宏包集"，实测完整默认 bundle ≈ **2.88 GB** ⇒ 随包不可行。当前默认走 `network-bundle`（bundle = null ⇒ 上游网络地址 ⇒ **首次联网取包**，之后复用本机缓存），用户可显式配本地 bundle。**联网默认路径 `[未测]`**（T9 用本地 `DirBundle` 夹具验证）。
- **构建面变重**：所有构建（含 `tauri dev`、CI）都需要 vcpkg + 六端口；缺一个 port 即整仓构建硬失败。
- **没有回退形态**：内嵌引擎的 bug 没有"切回子进程"的逃生门；C 层 abort/panic 不可树杀，取消延迟如实写进 UI。
- **U-14（子目录 `\include` + 中文路径）**：已从"最大未验证风险"变为**已跑绿**（T9 内嵌形态 exit 0 / 3 页 / 0 个 U+FFFD，`tmp/章节/` 由产品自建）；CI 夹具仍待补。
- 上游 0.x API 变更（钉 `Cargo.lock`）；`.fmt` 落错目录；缓存无进程间锁；**安装包体积增量 `[未测]`**；常驻档（P4）判决不成立（历史，不再投入）。

**未测（不得当成立）**：

- 构建代价三项（干净机器冷/热墙钟、峰值内存、体积增量）；
- **真机窗口 / 窗口内可编辑**：T9 会话因工作区 Low 完整性标签写不了 `%LOCALAPPDATA%`，WebView2 起不来 ⇒ `[未测-环境限制]`；替代证据 = 启动时长 + MCP bridge + watch 线程就绪 + bindings 自动导出 + headless 全链路真编（见 §6）；
- 联网默认 bundle 的真实下载行为与文案；
- 真实超时真编；makeindex / glossaries 真编；
- 安装包（NSIS）冷启动与体积。

## 6. 验证入口

| 要验的东西 | 入口 | 当前结果（T9，2026-11-20） |
|---|---|---|
| 构建前置 | `npm run lib:check` | exit 0（仅 nasm/pkg-config 不在 PATH 的警告） |
| core | `cargo test -p latteset-core` | 268 passed / 0 ignored |
| infra | `cargo test -p latteset-infra` | 40 passed / 0 ignored |
| 内嵌 crate | `cargo test -p latteset-tectonic` | 44 passed / 1 ignored；`-- --ignored` 下真编 1 passed |
| headless | `cargo test -p latteset-server` | lib 12 passed / 4 ignored + cli 8 passed；`-- --ignored` 4 个真编全过 |
| MCP 协议链路 | `cargo test -p latteset-server --test mcp_stdio` | 5 passed / 1 ignored（`--ignored` 下真编过） |
| 全仓 Rust（唯一入口） | `npm run lib:test` | **377 passed / 6 ignored** |
| 前端构建 / 单测 | `npm run build` / `npm run test` | 0 error；**17 files / 151 tests passed** |
| bindings 生成物 | `src/bindings.ts` + T7 核对脚本 | 13/13 PASS（命令 18 条 / 事件 9 个与 Rust 一致） |
| headless 真编（唯一证据链） | `latteset-cli --project <夹具> compile` + `node scripts/validate-pdf.mjs` | U-14 中文/子目录 exit 0（3 页、0 U+FFFD）；Full 1,200 ms；Quick 75 ms；内容错误精确归属 `chapters/ch1.tex:3`；biber/bibtex 对照见 §4.3 |
| 启动时长 | `npm run tauri dev`（唯一入口） | 热增量到 watch 线程 **5.78 s**；强制重编 **12.96 s**（≤60 s） |
| 真机窗口/可编辑 | 同一命令 + tauri server MCP | **`[未测]`（会话环境限制）**，替代证据见上；需在可写 `%LOCALAPPDATA%` 的会话补做 |

复核脚本仍在仓：`scripts/validate-pdf.mjs`（PDF 页数/文本层断言）、`scripts/tectonic-lib-xdvscan.mjs`（XDV 页表复核）。`scripts/bench-tectonic-lib.mjs` 原用途是**库形态 vs 子进程 A/B 对照**——对照对象已删除，只作历史入口保留，不要拿它判定当前实现。

## 7. ADR-0010 裁决与必须的例外（X-5，继续有效）

`latteset-infra` 仍是默认依赖图里文件系统与进程的唯一落点；`src-tauri` 仍不出现 `std::fs`/`std::process`；**`IoProvider` 那条读写路径必须代理到 core `FileSystem`**（引擎向我们要文件时不许直读盘）。登记在案的例外：

- `IoProvider` 是**同步** trait、core `FileSystem` 是 async ⇒ 同步上下文的磁盘读写只能在 `spawn_blocking` 里用 `std::fs`（建多层 format 目录、pages/bibsig 缓存、预热中间产物、PDF 原子替换、自建 `tmp/`）。**同步上下文是这条例外的根因**。
- **C 库自己读文件**（fontconfig/freetype2/harfbuzz/graphite2/ICU 的字体与 config）**无法**经 `FileSystem`（X-1）。
- **C 代码的 abort/panic 无法树杀**，取消只能做到"下一个 `IoProvider` 回调返回错误"（X-3）⇒ 取消延迟必须如实写进 UI 语义。
- **PDF 加密路径**的 C 层 `getenv("SOURCE_DATE_EPOCH")` 无 API 可注入/屏蔽——**未决策**，不是结论。
- 输入路径**无包含性检查**是上游设计立场（`FilesystemIo` 文档原文：不做 `../../../` 约束）⇒ 内嵌形态不改这一点。

## 8. 证据与历史

- **阶段②验证据**：`test_file/phase2/REPORT-gates.md`（T9 全量门禁 + 内嵌真编 + 启动时长；工作区本地、未提交）、`test_file/build-env/REPORT-build-env.md`（D8050 根因与环境结论）。
- **ADR**：[ADR-0015](../adr/0015-tectonic-only-embedded-engine.md)（唯一引擎 + 唯一形态，②段执行记录见文末）、[ADR-0012](../adr/0012-tectonic-library-form-engine.md)（路径 B + X-5 例外）、[ADR-0010](../adr/0010-infra-crate-boundary.md)（crate 边界）、[ADR-0016](../adr/0016-ship-bundle-offline.md)（bundle 分发，待③段重定）。
- **历史方案（只读素材）**：[../archive/tectonic-library-plan.md](../archive/tectonic-library-plan.md)、[../archive/research/tectonic-lib-evidence.md](../archive/research/tectonic-lib-evidence.md)、[../archive/research/tectonic-test-plan.md](../archive/research/tectonic-test-plan.md)。**`docs/archive/` 不再接收新证据**；新结论写进本文档或 [design.md](../design.md) 对应小节，复核脚本进 `scripts/`。
- **P0–P8 闸门表**：已随②段收口退役（本文 §1–§6 是它的现行替代）；历史判据留在归档测试计划里。
