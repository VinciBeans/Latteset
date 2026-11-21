# 内嵌 Tectonic 引擎（唯一形态）：构建、装配与契约

> **这是什么、给谁看**：阶段②（[ADR-0015](../adr/0015-tectonic-only-embedded-engine.md) 决策 2）之后，Tectonic 以**内嵌**为**唯一引擎形态**。本文写它的构建入口、环境前置、装配点、能力面、随包分发（③段）、风险与验证入口，面向要构建 / 装配 / 打包 / 复核这条路径的开发者。产品侧行为（Quick/Full 语义、超时与诊断、设置面）见 [design.md](../design.md) §2/§3；runner 的逐层实现契约（输入层、format、页哈希、bib 编排）见 [modules.md](./modules.md) §2.7。
>
> **本文事实有效期**：阶段②以 `test_file/phase2/REPORT-gates.md`（T9）为准；**阶段③（随包精简集 / 六档基线 / 预热 / 许可资源）以 `test_file/phase3/REPORT-gates.md`（T19）、`BASELINE.md`、`BUNDLE.md`、`PRODUCT-BUNDLE.md`、`T17-REPORT.md`、`T18-REPORT.md`、`test_file/phase3/*-verify/` 为准**（均为工作区本地证据、未提交）。

## 1. 定位

- **是什么**：把 Tectonic 引擎 crate 直接嵌进产品进程——自持 I/O 层（`IoProvider`：内存直喂输入 + 输出捕获 + format 自管）驱动 `CoreBridgeLauncher` → `TexEngine` → `XdvipdfmxEngine::process`，PDF 收尾原子替换到项目根。
- **唯一形态，不是构建变体**：子进程路径（`tectonic.exe` 命令构造、`-C` 缓存判定、进程树杀、stdout `[N]` 页进度）与 `tectonic-lib` feature / `LATTESET_TECTONIC_LIB` / `settings.tectonic.lib_form` **已全部删除**（`crates/latteset-infra/src/{runner.rs,tectonic/}` 整文件删除；`infra` 不再有 runner / tectonic 模块）。
- **所有构建都需要 vcpkg**：`latteset-tectonic` 是 `src-tauri` 与 `crates/latteset-server` 的**非可选依赖**，不是"发布构建才带"的变体。`npm run tauri dev/build`、`cargo check/test -p latteset-server/-p latteset`、CI 默认 Windows 档、CLI 构建全部要过 §2 的环境前置。
- **不启动外部进程、不读用户 TeX Live**：biber / makeindex / glossaries **不会**被自动运行（biber 的实测口径见 §4.3）。产品**没有回退形态**——bundle/缓存配置错误走 runner 的显式失败文案，不悄悄降级。
- **ADR-0010 的第二落点**：`latteset-infra` 仍是默认依赖图里文件系统与进程的唯一落点；本 crate 是**外部依赖与 C 链的第二个落点**（ADR-0012 的 X-5 例外登记，见 §7）。
- **随包分发（③段，ADR-0016 候选 A）**：产品默认 bundle = **随包精简宏包集**（`src-tauri/resources/tectonic-bundle.zip`，517 文件 / 30.43 MiB zip / digest `9e8c63ca…`）⇒ **离线开箱**；在线完整集 / 本地目录是用户显式选项。许可材料随安装包（`licenses/`，308 文件 / 48.95 MiB）。**最终安装包（T19 实测）：NSIS 88.46 MiB / MSI 109.63 MiB ≤200 MB**。细节见 §2.1、§4.5。

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

**CI**：Windows 默认档就是内嵌档——checkout 钉 rev 的 vcpkg → bootstrap → 缓存 `installed/`+`downloads/` → 装六端口 → `-Mode check` → **无条件构建 release CLI**（`-Mode cli`；它是代表集 bundle 生成与离线首编的共同前置，T22 B2，**不得**塞回缓存分支）→ 代表集缓存/重建（`test_file/tectonic-bundle`，digest `9e8c63ca…`；未命中则 `gen-bench-bundle.mjs` 重建）→ `make-bundle.mjs` 打产品 zip + 自校验 → **离线首编验收**（坏代理 + 随包 zip + `validate-pdf`）→ `-Mode test` → `-Mode tauri build` → **安装包体积门禁 ≤200 MB**。**不再有** `build-windows-tectonic-lib` 窄触发档（D5）。Ubuntu 档只跑前端 + core/infra（server 已非可选内嵌，Linux 不跑 server，R11 决策）。

### 2.1 打包 / 分发入口（③段）

| 命令 | 作用 |
|---|---|
| `node scripts/gen-bench-bundle.mjs --phase=all` | 从上游 `default_bundle_v33.tar` 按需取文件生成/合并 `test_file/tectonic-bundle`（517 文件、`SHA256SUM`、digest `9e8c63ca…`）；覆盖六档 + beamer + biblatex/bibtex + Fandol 四款 + lmodern 全字号（**测试夹具，不入库**） |
| `node scripts/make-bundle.mjs --max-zip-mb=120` + `--verify` | 把源目录打成**确定性** `src-tauri/resources/tectonic-bundle.zip`（30.43 MiB；同输入同 zip），自校验 + raw zip 体积门禁 |
| `node scripts/make-notices.mjs --check` / `--collect` | 许可资源离线一致性复核（CI 门禁）/ 换 bundle 后重采集 `src-tauri/resources/licenses/` |
| `npm run tauri:build` | 唯一发布入口：wrapper 先跑上面两步，再 `npx tauri build`（NSIS + MSI） |

- **随包资源**（`tauri.conf.json` `bundle.resources`）：`resources/tectonic-bundle.zip` → 安装目录 `tectonic-bundle.zip`（运行时 `<resource_dir>/tectonic-bundle.zip`）；`resources/licenses` → 安装目录 `licenses/`。`.fmt` **不随包**（首启预热生成，见 §4.6）。
- **安装包体积（本机 2026-11-21 T19 终版实测）**：NSIS **92,755,399 B = 88.46 MiB**、MSI **114,954,456 B = 109.63 MiB**，均 ≤ 200 MB 产品上限（含随包 zip 30.43 MiB + `licenses/` 解包 48.95 MiB）。CI 在打包后自动打印并硬门禁 ≤200 MB。
- ⚠ **Windows verbatim 路径坑**：release 下 `resource_dir()` 返回 `\\?\E:\…` 长路径形态，`path_to_file_url` 必须**先剥 `\\?\` / `\\.\` / `\\?\UNC\`** 再拼 `file://`；否则上游 `Url::to_file_path()` 失败（`failed to parse local path`）⇒ 预热与默认离线编译整体不可用（T19 阻断回归，含单测 + `Shipped` 真编）。

## 3. 装配点

内嵌唯一后只剩"**每趟按当前设置重建 runner**"这一种装配，没有形态判定、没有 feature 门槛：

| 入口 | 落点 | 契约 |
|---|---|---|
| GUI 主编译 | `src-tauri/src/runner.rs` 的 `EmbeddedRunner`（薄壳） | 只持 `fs / progress / settings / app_cache_dir`；每趟 `compile` 读一次 `settings.tectonic` 的 bundle / cache 后重建 `TectonicLibRunner` ⇒ **资源设置改完即生效、不需要重启**（D1 = R3 选 A） |
| GUI 公式预览 | `src-tauri/src/commands.rs` 的 `build_compile_runner` | 与主编译**共用**同一装配函数；不再有第三处口径漂移 |
| GUI 首启预热 | `src-tauri/src/lib.rs` setup 末尾 `spawn_blocking(prewarm_format)` | 预热用的 bundle/cache **与 runner 同源**（settings > env > 随包 > 在线；cache 同序）⇒ 预热出的 `.fmt` 恰好是首编要用的那一份；失败静默、不挡启动 |
| headless（CLI/MCP） | `crates/latteset-server/src/lib.rs` 的 `build_runner` | 只构造 `TectonicLibRunner`；读 `Session.settings.tectonic`（D4）。**无资源目录 ⇒ 没有"随包内置集"一档**（显式 > 环境 > 在线兜底） |

**资源优先级 = 显式设置 > 环境变量 > 随包内置集 > 在线兜底**（`latteset_tectonic::resolve_source`；headless 同序但无"随包"档）：

- **bundle**：`settings.tectonic.bundle`（非空）→ `LATTESET_TECTONIC_BUNDLE` → **随包精简集**（release 有 `resource_dir()/tectonic-bundle.zip` 时）→ 上游在线完整 bundle（`network-bundle` 特性由两个上层 manifest 打开）。随包集缺失/损坏 ⇒ `SHIPPED_BUNDLE_HINT` 显式报错（修复安装 / 切在线），**不静默联网**；在线完整集是**用户显式选择**。本地目录 bundle 必须写 `file:///…` 或相对路径（**绝对 Windows 路径会被上游当 URL scheme 吃掉**），目录需自带 `SHA256SUM`。
- **缓存目录**：`settings.tectonic.cache_dir`（非空）→ `LATTESET_TECTONIC_CACHE` → 宿主应用缓存目录（GUI：Tauri 应用缓存；headless：`<配置目录父目录>/cache`）。缓存**必须**有落点：上游 format 默认落**项目目录**，不注入会把 ~24 MB 的 `.fmt` 扔进用户项目。启动空闲 `gc_cache` 按"**当前 bundle digest**（同一 bundle 解析器：随包/显式/在线；在线只读缓存、零网络；解析不出则不动）+ 最近 N（`LATTESET_CACHE_KEEP`，默认 3）"清理；缓存目录由 `resolve_cache_dir`（设置 > env > 宿主）统一解析，runner/预热/GC 共用（T22 B3）。

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

- **biber**：内嵌形态**不会调用外部 biber**。T9 实测：本机 PATH 上有真 biber 2.21，编译**不跑它**、**不产 `.bbl`**，只按 `.run.xml` 里的 `<binary>biber</binary>` 发一条 WARN 级 `compile-notice`，`kind` 如实退回草稿态。`backend=bibtex` 的文档走**内建 `BibtexEngine`** 正常生成 `.bbl`（实测 PDF 文本含 `Knuth`/`TeXbook`），**不会**误报 biber。T19 在随包精简集内复核：biber 夹具 exit 0 / `kind=quick` / 无 `.bbl`；bibtex 夹具 exit 0 / `kind=full` / `.bbl` 1065 B。
- **makeindex / glossaries**：同样不自动运行，检出 `.idx` / `.glo` 即 `compile-notice`（索引 / 术语表为空）；**真编夹具 `[未测]`**（仅单测覆盖三类提示）。
- **`.ins` / `.dtx` 源码版模板提示保留**：阶段① `enrich_source_release_hints` 的语义已搬进内嵌 runner 的失败路径（D7），缺 `.cls` 且项目里有同名 `.ins` 时仍给"先执行 `xelatex X.ins`"。
- **文案纪律**：不得写成"引擎会调用 biber"或"引擎不支持 biber"；正确口径是"**引擎依赖外部 biber，而内嵌形态不会替你运行**"。

### 4.4 Quick / Full

- **Quick = 请求强度，不是固定档**：编辑触发请求同样走重跑循环（含 bib 趟），**预算内**收敛就按实际结果报 `full`（前端 `draft=false`，不排空闲收敛）；三重闸门 = `passes < 3` ∧ 第 1 趟 < 1000 ms ∧ 累计 < 2000 ms。超出预算或检出外部工具即停在草稿态。
- T9 实测：热缓存 Quick 请求 75 ms、`upgraded_from_quick=true`（已收敛如实报 full，不虚报草稿）；冷缓存 Full 1,200 ms（format 974 ms + 排版 149 + 转换 76）。

### 4.5 页哈希、A 闸门与 bundle

- **页哈希恒可用**（XDV 在捕获表里）：`page_hashes` 喂 `core::xdv`，缓存名固定 `tmp/<stem>.tectonic.pages`；**空表仍只表示"无法判定"**（XDV 缺失/损坏）⇒ 前端全量刷新，不能当死代码删。
- **A 闸门（PDF 复用）在 T17 扩到 Full**：判据 = 页哈希逐页相同 **且** 磁盘 PDF 身份未变（`tmp/<stem>.tectonic.pdfid`，`{len, mtime_ns}` + 版本行 `v1`）⇒ 跳过 XDV→PDF 与拷贝。缺失/损坏/页数页序变/PDF 被手改/被删 ⇒ 保守转换一次。T17 A/B：`noop` 六档 **−7.9%…−51.6%**（convert 归零）；这是"内容没变的编辑/重复 Full"的快速路径。
- **bundle 三类**：目录 → `DirBundle`（必须自带 `SHA256SUM`，裸 TeX Live 目录不能直接用）、`.zip` → ZipBundle、`.ttb` → TTBFsBundle，其他扩展名 → `None`；空目录会被静默当合法 bundle（`file_count=0`）⇒ 产品侧自校验 + 分类文案。`-b <本地 .tar>` 不可用。
- **③段随包精简集（默认来源）**：`zip` 30.43 MiB / 517 文件 / digest `9e8c63ca…`，覆盖六档基线文档集 + beamer + biblatex/bibtex（biblatex 可载入；内嵌不跑外部 biber）+ 中文默认（ctex + Fandol 四款）+ lmodern 5–17pt + tikz/pgf + 常用包。**不是完整 TeX Live** ⇒ 缺包给"切在线完整集 / 补本地目录 bundle"的可行动诊断，**不静默回退**（ADR-0016 决策 2）。完整默认 bundle ≈ 2.88 GB，不随包。
- **许可资源随包**：`licenses/`（THIRD-PARTY-NOTICES + 261 份许可文本 + Fandol Corresponding Source + TL patch + 可复核清单，解包 48.95 MiB），`make-notices.mjs --check` 进 CI；许可结论见 [tectonic-licensing.md](./tectonic-licensing.md) §8（G-L1..L9 全绿，残余风险 R-L10）。
- **format**：键 = bundle digest（`{digest}-latex-33.fmt`，换 bundle 必换文件），落点 `<cache>/formats/`；算不出缓存落点显式报错，**不落项目目录**。缓存目录无进程间锁，产品侧按缓存目录串行化（引擎全局锁天然串行）。
- **确定性**：库内**禁止** `build_date_from_env`、**必须**显式 `build_date(SystemTime::now())`；"PDF 逐字节相等"不得作为 Tectonic 档的断言（元数据/加密层不可控）。

### 4.6 首启预热与启动（T18/T19）

- `runner::prewarm_format(&BundleSource, &Path) -> PrewarmOutcome`（`Generated / AlreadyCached / SkippedNoBundle / Failed`）：只跑 initex+dump、**不跑用户文档**、原子落盘（`.fmt.tmp` → rename）、失败静默且不留半成品；`src-tauri` setup 末尾 `spawn_blocking` 派发，**不占启动关键路径**。
- **与首编互斥**：进程级 `PrewarmGate`（`Idle/Running/Done/Failed` + `Condvar`）+ 编译入口 `wait_for_prewarm()`（约 60 s 超时后按"无缓存"继续），避免预热与首编各跑一趟 initex。
- **在线源零网络判定**：`bundle::prewarm_plan` 在 `open_bundle` 之前纯决策（本地源主动预热；在线源只读缓存标记 = digest + `.lock` ≤7 天 + `data/<digest>/` 非空；不齐 ⇒ `Skip`）⇒ 启动预热在线默认源**不联网、不建目录**。
- **实测（T19 release GUI 直启，默认随包 zip）**：进程→setup/Ready **126 ms**、后台预热 **992 ms**、`.fmt` 24,451,466 B 落盘、首编 `format_cached=true / format_ms=0`；温启动 **144–171 ms**（`AlreadyCached`）。预热前后页哈希逐页相等（真编用例断言）⇒ 预热只省等待、不改结果。安装版（NSIS）冷启动 `[未测-环境会话限制]`（安装器在受限会话不可交互），替代证据 = 直启分段 + 安装包体积门禁。

## 5. 风险与未测

**风险**：

- **bundle 分发（③段已落地，ADR-0016 候选 A）**：默认随包精简集（30.43 MiB / 517 文件）⇒ 离线开箱；完整 bundle ≈2.88 GB 不随包；缺包走"显式切在线完整集 / 补本地 bundle"通道（**不静默回退**）。**残余风险 R-L10**：Fandol 源码取自社区镜像、**未做"源码 → OTF"重建验证**（需 Adobe AFDKO）；已如实登记在许可审计 §8.6，不阻断发包。
- **构建面变重**：所有构建（含 `tauri dev`、CI）都需要 vcpkg + 六端口；缺一个 port 即整仓构建硬失败。构建代价三项（干净机器冷/热墙钟、峰值内存）`[未测]`。
- **没有回退形态**：内嵌引擎的 bug 没有"切回子进程"的逃生门；C 层 abort/panic 不可树杀，取消延迟如实写进 UI。
- **U-14（子目录 `\include` + 中文路径）**：T9 内嵌形态已跑绿，T19 在**随包集内**复验（中文根文件名 + `章节/` `\include`，exit 0 / 3 页 / 0 个 U+FFFD，产品自建 `tmp/章节/*.aux`）；CI 夹具仍待补。
- **安装包体积已受控**：NSIS 88.46 MiB / MSI 109.63 MiB ≤200 MB（T19 终版）；`licenses/` 解包 48.95 MiB 是主要增量。
- 上游 0.x API 变更（钉 `Cargo.lock`）；`.fmt` 落错目录；缓存无进程间锁；**常驻档（P4）判决不成立**（历史，不再投入）；**每进程 +200–240 ms 一次性开销（T14→T19）已归因：fontconfig 宿主字体扫描环境税（2026-11-21）**——用最小 fontconfig 配置 + 可写缓存即清零（tiny 热路径 `typeset_ms` 365→73 ms），**与产品代码无关；T19 绝对值含税、跨版本绝对比较作废**，相对收益（L4b/L5）仍有效（同进程稳态无回归）。

**未测（不得当成立）**：

- **真机窗口 / 窗口内可编辑 / About 许可入口点击**：T9/T19 会话因工作区 Low 完整性标签写不了 `%LOCALAPPDATA%`，WebView2 起不来 ⇒ `[未测-环境限制]`；替代证据 = 启动分段（进程→Ready 126 ms）+ MCP bridge + watch 线程 + bindings 13/13 + headless 全链路真编（§6）。
- **NSIS 安装版冷/温启动**：受限会话安装器静默安装挂起；替代证据 = `tauri build` exit 0 + 体积门禁 + release 直启分段。
- **在线完整 bundle 的反向切换**（在线 → 随包/本地）未单独重测（优先级低；同一 `resolve_source` 优先级有单测）；**在线首次取包成本已测**（344 s，一次性）。
- **真实超时真编**、**makeindex / glossaries 真编**（biber 与 bibtex 已在 T9/T19 真编覆盖）。
- **缓存 GC 与预热/编译的并发对抗样例**（T22 review 结论：单测已覆盖"保留当前 digest / 解析不出不动 / 未知条目不删"，并发压测仍未构造）。

## 6. 验证入口

| 要验的东西 | 入口 | 当前结果（T22 复核，2026-11-21） |
|---|---|---|
| 构建前置 | `npm run lib:check` | exit 0（仅 nasm/pkg-config 不在 PATH 的警告） |
| core | `cargo test -p latteset-core` | **269 passed / 0 ignored** |
| infra | `cargo test -p latteset-infra` | **40 passed / 0 ignored** |
| 内嵌 crate | `cargo test -p latteset-tectonic` | **65 passed / 3 ignored**；`-- --ignored` 下 3 个真编全过（真编 / 预热后首编 / 随包 `Shipped` verbatim） |
| headless | `cargo test -p latteset-server` | lib 12 passed / 4 ignored + cli 8 passed；`-- --ignored` 4 个真编全过 |
| MCP 协议链路 | `cargo test -p latteset-server --test mcp_stdio` | 5 passed / 1 ignored（`--ignored` 下真编过） |
| 全仓 Rust（唯一入口） | `npm run lib:test` | **399 passed / 8 ignored** |
| 前端构建 / 单测 | `npm run build` / `npm run test` | 0 error；**18 files / 189 tests passed**（T38/T41 复核） |
| bindings 生成物 | `src/bindings.ts` + T7 核对脚本 | **13/13 PASS**（命令 **19 条** / 事件 9 个与 Rust 一致） |
| 六档性能基线 | `node scripts/bench-embedded.mjs --samples=3` | **T22 环境税清零版**：三轮独立运行取中位、极差除 tiny 热路径 ≤7.2%；完整表进 [design.md](../design.md) §2.8；`multifile` 归 large 后 cold 2.94 s / e2e 2.01 s EXCELLENT（**临界**）；**唯一硬缺口 `large` cold 6.41 s 超 5 s（已登记）** |
| 随包断网首编 | `make-bundle.mjs` + 坏代理 + 空缓存 + `latteset-cli compile` + `validate-pdf.mjs` | **exit 0 / 4.4 s**（Fandol 四款、0 U+FFFD）；二次 1.2 s；缺包 `enumitem` ⇒ `missing_package` + hint，显式切在线后 exit 0（首编含取包 344 s） |
| 首启预热 / 启动 | release GUI 直启 + `RUST_LOG=info` | 进程→Ready **126 ms**、预热 **992 ms**、首编 `format_ms=0`；温启动 144–171 ms |
| 真机窗口/可编辑 | 同一直启 + tauri server MCP | **`[未测]`（会话环境限制）**，替代证据见 §6/§5；需在可写 `%LOCALAPPDATA%` 的会话补做 |
| 许可 / 安装包 | `node scripts/make-notices.mjs --check`；`tauri build` 后读安装包体积 | `--check` 通过；NSIS **88.46 MiB** / MSI **109.63 MiB** ≤200 MB |
| CI 发布前链路 | `.github/workflows/ci.yml` 的 `build-windows` | release CLI **无条件先建**（T22 B2）；离线首编不再因 rust-cache 无 release 产物而红 |

复核脚本仍在仓：`scripts/validate-pdf.mjs`（PDF 页数/文本层断言）、`scripts/tectonic-lib-xdvscan.mjs`（XDV 页表复核）、`scripts/bench-embedded.mjs`（六档基线）、`scripts/make-bundle.mjs` / `gen-bench-bundle.mjs`（随包集）、`scripts/make-notices.mjs`（许可资源）。`scripts/bench-tectonic-lib.mjs` 原用途是**库形态 vs 子进程 A/B 对照**——对照对象已删除，只作历史入口保留，不要拿它判定当前实现。

## 7. ADR-0010 裁决与必须的例外（X-5，继续有效）

`latteset-infra` 仍是默认依赖图里文件系统与进程的唯一落点；`src-tauri` 仍不出现 `std::fs`/`std::process`；**`IoProvider` 那条读写路径必须代理到 core `FileSystem`**（引擎向我们要文件时不许直读盘）。登记在案的例外：

- `IoProvider` 是**同步** trait、core `FileSystem` 是 async ⇒ 同步上下文的磁盘读写只能在 `spawn_blocking` 里用 `std::fs`（建多层 format 目录、pages/bibsig 缓存、预热中间产物、PDF 原子替换、自建 `tmp/`）。**同步上下文是这条例外的根因**。
- **C 库自己读文件**（fontconfig/freetype2/harfbuzz/graphite2/ICU 的字体与 config）**无法**经 `FileSystem`（X-1）。
- **C 代码的 abort/panic 无法树杀**，取消只能做到"下一个 `IoProvider` 回调返回错误"（X-3）⇒ 取消延迟必须如实写进 UI 语义。
- **PDF 加密路径**的 C 层 `getenv("SOURCE_DATE_EPOCH")` 无 API 可注入/屏蔽——**未决策**，不是结论。
- 输入路径**无包含性检查**是上游设计立场（`FilesystemIo` 文档原文：不做 `../../../` 约束）⇒ 内嵌形态不改这一点。

## 8. 证据与历史

- **阶段②证据**：`test_file/phase2/REPORT-gates.md`（T9 全量门禁 + 内嵌真编 + 启动时长；工作区本地、未提交）、`test_file/build-env/REPORT-build-env.md`（D8050 根因与环境结论）。
- **阶段③证据**：`test_file/phase3/REPORT-gates.md`（T19 门禁 + 六档基线终版 + 随包/断网/缺包/在线真编 + 启动分段 + 两个缺陷修复；§11 为 T22 归因与受控重测）、`BASELINE.md`（§0.1 = T22 环境税清零版，§1–§10 = 含税版）/`baseline.json`/`baseline-fc*.json`、`BUNDLE.md`（测试集覆盖率/digest）、`PRODUCT-BUNDLE.md`（随包 zip 规格/体积/验收）、`T17-REPORT.md`（L4b/L5）、`T18-REPORT.md`（预热/启动分段/零网络判定）、`test_file/phase3/t16-verify/`、`t21-*`（许可资源与体积）、`evidence/t22fc-fontconfig/`（字体扫描税证据）。
- **ADR**：[ADR-0015](../adr/0015-tectonic-only-embedded-engine.md)（唯一引擎 + 唯一形态，②段执行记录见文末）、[ADR-0012](../adr/0012-tectonic-library-form-engine.md)（路径 B + X-5 例外）、[ADR-0010](../adr/0010-infra-crate-boundary.md)（crate 边界）、[ADR-0016](../adr/0016-ship-bundle-offline.md)（bundle 分发：③段已按候选 A 落地，执行记录见文末）。
- **许可**：[tectonic-licensing.md](./tectonic-licensing.md)（G-L1..G-L9 全绿 + R-L10 残余风险）。
- **历史方案（只读素材）**：[../archive/tectonic-library-plan.md](../archive/tectonic-library-plan.md)、[../archive/research/tectonic-lib-evidence.md](../archive/research/tectonic-lib-evidence.md)、[../archive/research/tectonic-test-plan.md](../archive/research/tectonic-test-plan.md)。**`docs/archive/` 不再接收新证据**；新结论写进本文档或 [design.md](../design.md) 对应小节，复核脚本进 `scripts/`。
- **P0–P8 闸门表**：已随②段收口退役（本文 §1–§6 是它的现行替代）；历史判据留在归档测试计划里。
