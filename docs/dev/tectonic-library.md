# Tectonic 库内嵌形态（契约与现状）

> **这是什么、给谁看**：Tectonic「库内嵌」形态（路径 B）的构建入口、集成契约与验证现状。面向要构建/装配/复核这条路径的开发者与维护者。产品侧行为（引擎清单、形态位、超时与诊断语义）见 [design.md](../design.md) §3.2；runner 的逐层契约（输入四层、format、页哈希、bib 编排）见 [modules.md](./modules.md) §2.7。

## 1. 定位

- **是什么**：把 Tectonic 引擎 crate 直接嵌进产品进程——自持 I/O 层（`IoProvider`：内存直喂输入 + 输出捕获 + format 自管）驱动 `CoreBridgeLauncher` → `TexEngine` → `XdvipdfmxEngine::process`，PDF 收尾原子替换到项目根。
- **与子进程形态的关系**：两者并存，**设置面二选一**。形态位是全局设置 `settings.tectonic.lib_form`（GUI 的 `SwitchableRunner` 每趟编译读一次 ⇒ 改完即生效、不需重启）；它受**引擎闸门**约束——`TectonicSettings::use_library_form(engine, env_forced)` 规定 `lib_form` **只在 `engine == Tectonic` 时生效**（少了这半条，`engine=xelatex` + `lib_form=true` 会静默跑库形态而状态栏报 XeLaTeX）；运行期 `LATTESET_TECTONIC_LIB=1` 是**显式覆盖**，不受闸门约束。**失败不自动回退**（D1）——回退是用户显式选回子进程形态。
- **默认值是子进程**（`lib_form=false`）：库形态**不跑外部进程** ⇒ biber / makeindex / glossaries 的文档在它下面直接失败（检出即 `warn!`、`kind` 退回 `Quick`，由「引用待更新」兜底）。把默认切过去等于让这类文档开箱即败。
- **它是正式的构建变体，不是实验开关**：开发默认构建**不含**它（主产物零原生依赖、不需要 vcpkg），**发布构建含**它（用户装到的包两种形态都能选）。
- 子进程形态本身（命令、`-C` 条件化、缓存判定、失败文案）见 [design.md](../design.md) §3.2。

## 2. 构建入口

**唯一落点**：`scripts/with-tectonic-lib.ps1`（npm 侧 `lib:*`）。它一次做完三件事：① 校验 vcpkg 检出 / triplet 依赖 / 工具链，**缺项给可执行的修复命令**（此前是 build script 以 pkg-config 或 panic 的形式失败，文案不指向环境）；② 设 5 个环境变量（`TECTONIC_DEP_BACKEND=vcpkg`、`VCPKG_ROOT`、`VCPKG_DEFAULT_TRIPLET`、`VCPKG_DEFAULT_HOST_TRIPLET`、`VCPKGRS_TRIPLET`）并给 `RUSTFLAGS` 补 `-Ctarget-feature=+crt-static`（triplet 用 static CRT，Rust 侧必须同用，否则链接冲突）；③ 跑对应命令。**别再在别处抄这套环境**；CI 的 `build-windows-tectonic-lib` 档（版本 tag / 手动 dispatch）也走它。

| npm 脚本 | 作用 |
|---|---|
| `npm run lib:check` | 只校验前置（vcpkg / triplet / 工具链），不跑构建 |
| `npm run lib:dev` | 起带 `tectonic-lib` 的 `tauri dev`（真机验证库形态） |
| `npm run lib:build` | 出**含库形态**的发布包（等价 `tauri build --features tectonic-lib`） |
| `npm run lib:test` | 跑带库形态的用例 |
| `npm run lib:cli` | release 构建 headless CLI（`latteset-cli`，带库形态） |

**前置**：vcpkg + `x64-windows-static-release` triplet + cmake / pkg-config / nasm + crates.io 网络。其中 **graphite2 / ICU / freetype2 / fontconfig / libpng 永远走外部探测，探测失败即 build script panic**（harfbuzz 随 crates.io 发布版内含源码，不踩空子模块坑）；这正是库形态与主产物必须分开构建的原因。

- **`build.rs` 的既有动作**：在 Windows 补发静态 ICU 需要的 8 个 MSVC 隐式系统库（`advapi32` 等），出处是 vcpkg 自己 `lib/pkgconfig/icu-uc.pc` 的 `baselibs`。**不加它 `cargo check` 过、`cargo test`/example 链接必失败**（`icuuc.lib(wintz.ao)` 未解析 `__imp_RegCloseKey`）。
- **默认构建不含库形态的原因**：它的依赖闭包必然拉 C/C++ 链（freetype2 / harfbuzz / graphite2 / ICU / fontconfig / libpng，**21 个 native 标记**）。crate 因此是独立 workspace 成员，由根 `Cargo.toml` 的 `default-members` 排除在**不带 `-p`** 的构建之外；`tectonic-lib` feature 默认关。⚠ **feature 门控不免除依赖解析**：清单里存在可选依赖，任何构建都要能解析整棵依赖图。
- `latteset-infra` **不依赖** `latteset-tectonic`（否则上面那条代价原样回来）；要用库形态只能经 `src-tauri` / `latteset-server` 的 feature。

## 3. 集成要点

- **crate 落点**：`crates/latteset-tectonic/`（`src/{lib,io,status,bundle,runner,synctex}.rs` + `examples/`）。
- **装配点两处**：GUI（`src-tauri`）装 `SwitchableRunner`，由每趟编译读到的全局设置决定形态；headless（`latteset-server`）的 `build_runner()` 在**装配期一次性**决定、**只看 `LATTESET_TECTONIC_LIB`**（不读 `settings.tectonic.lib_form`，也不看 `engine`）⇒ GUI 里打开「库内嵌」**不会**影响 CLI/MCP，headless 要用库形态必须显式给环境变量。
- **调用序列摘要**：`TectonicIo`（`IoProvider`）→ `MinimalDriver` → `CoreBridgeLauncher` → `TexEngine`（format 趟 → 排版趟）→ `XdvipdfmxEngine::process`。**没有**"向 `ProcessingSession` 注入 `IoProvider`"这条写法（0.17 无该入口，其能力上限也只有内存输入 + 内存产物 + `StatusBackend`，没有逐页事件）。逐层契约（输入四层与"本次产物层"、输出镜像与 format 不镜像、format 键 = bundle digest、时间源、页哈希、bib 与重跑循环、失败面、分阶段计时）见 [modules.md](./modules.md) §2.7。
- **同进程引擎全局互斥**：`with_global_lock` + `static ENGINE_LOCK` 把**整个 C 引擎调用**串起来（上游理由：C 侧 `setjmp/longjmp` 跨 FFI 是 UB）⇒ 排版趟与转换趟**不可能并行**（实测：4 线程并发墙钟 533 ms == 单线程跑 4 次 541 ms，零并行收益），编译在 `spawn_blocking` 里自建自跑。**这条决定了「编译中预览」只能在趟边界出图**。
- **ADR-0010 裁决与必须的例外（X-5）**：本 crate 是**外部依赖 / C 链的第二个落点**；`latteset-infra` 仍是默认依赖图里文件系统与进程的唯一落点；`src-tauri` 仍不出现 `std::fs`/`std::process`；**`IoProvider` 那条读写路径必须代理到 core `FileSystem`**（引擎向我们要文件时不许直读盘）。登记在案的例外：
  - `IoProvider` 是**同步** trait，而 core `FileSystem` 是 async ⇒ 同步上下文的磁盘读写只能在 `spawn_blocking` 里用 `std::fs`：建多层 format 缓存目录（trait 的 `create_dir` 是**单层**语义）、`pages`/`bibsig` 缓存读写、预热上一趟中间产物、PDF 原子替换、自建 `tmp/`。**同步上下文是这条例外的根因**；将来出现第二个消费者时应给 trait 补方法，而不是继续扩散 `std::fs`。
  - **C 库自己读文件**（fontconfig/freetype2/harfbuzz/graphite2/ICU 的字体与 config）**无法**经 `FileSystem`（X-1）。
  - **C 代码的 abort/panic 无法树杀**，取消只能做到"下一个 `IoProvider` 回调返回错误"（X-3）⇒ 取消延迟必须如实写进 UI 语义。
  - **PDF 加密路径**的 C 层 `getenv("SOURCE_DATE_EPOCH")` 无 API 可注入/屏蔽——**未决策**，不是结论。
  - 输入路径**无包含性检查**是上游设计立场（`FilesystemIo` 文档原文：不做 `../../../` 约束）；子进程形态下用户本就拥有其可读文件 ⇒ 信任模型等价，库形态不改这一点。
- **bundle 口径**：`tectonic_bundles::detect_bundle(source, only_cached, Some(产品缓存目录))`；目录 → `DirBundle`（**必须自带 `SHA256SUM`**，裸 TeX Live 目录不能直接用）、`.zip` → ZipBundle、`.ttb` → TTBFsBundle，**其他扩展名 → `None`**。**绝对 Windows 路径会被当 URL scheme 吃掉** ⇒ 存储形态必须是 `file:///…` 或相对路径（设置面入口把 `E:\…` / `E:/…` / 带引号的"复制为路径"统一补成 `file:///…`）。空目录会被**静默当合法 bundle**（`file_count=0`，直到引擎要文件才报错）⇒ 产品侧自校验 + 分类文案（路径不存在 / 空目录 / 非 bundle 三类在库层是同一个 `None`）。`-b <本地 .tar>` 不可用。
- **缓存口径**：优先序 **显式设置 > 环境变量 > 宿主默认**；`<cache>/formats` 存 format、`<cache>/bundles` 存 bundle。format 键 = **bundle digest**（`{digest}-latex-33.fmt`，换 bundle 必换文件）；**默认落点是项目目录——不显式注入就会把 24 MB 的 `.fmt` 扔进用户项目**。缓存目录**无进程间锁**（`.lock` 内容实测是 Unix 秒、不是锁），临时名用 `process::id()` ⇒ 同进程内并发会撞同一临时名：产品侧必须按缓存目录串行化或每任务独立目录。
- **确定性口径**：库内**禁止** `build_date_from_env`（会把进程环境里的 `SOURCE_DATE_EPOCH` 吃进来）、**必须**显式 `build_date(SystemTime::now())`（上游默认值是 `UNIX_EPOCH`，忘写一行即把"`\today` 印 1970"变成默认行为）；一切 env 只经 `Command::env` 给子进程，库内**不** `std::env::set_var`。**"PDF 逐字节相等"不得作为 Tectonic 档的断言**（元数据/加密层不可控）。

## 4. 实施现状（P0–P8 闸门）

| 闸门 | 现状 |
|---|---|
| **P0** `-C` 条件化与首次获取引导 | **已落地**（属子进程档，不属库形态）：`-C` 只在拿到缓存正向证据时加；判据 LIB-8，命令构造有单测断言 |
| **P1** 依赖引入与最小可编译 | 落盘部分**已落地**（crate 落点 + `default-members` + 提交 `Cargo.lock`）；**构建代价三项仍未测**——干净机器的冷/热墙钟、峰值内存、体积增量（受外部前置阻塞：vcpkg + cmake/pkg-config/nasm + 网络） |
| **P2** 骨架：库形态 runner + 内存产物 | **已落地**；可复核入口 `node scripts/validate-pdf.mjs` |
| **P3** 自定义 I/O + 缓存隔离 + 离线 | **已落地**：三类 bundle 失败各有文案、项目目录不落 `.fmt`、format 落产品缓存 |
| **P4** 常驻进程与 format 复用（J1） | **判决不成立**：进程地板仅 **9.3 ms**，轻档净收益 −26 ms，**重档（28 页多文件 Full）第 3 轮起退化、稳态比每次起进程慢 31%**（1450 vs 1109 ms，`passes`/`req_*` 每轮相同 ⇒ 是同进程内存压力把全部操作一起拖慢）。⇒ 按回退点**不投入常驻**，改投 bib 跳过判据与 bundle/format 常驻化 |
| **P5** 编译中页事件（J2） | 官方 API **不成立**，自持输出层逐块解析**成立**（粒度 ≤16 KB 字节块，尾页可能迟到最末）。I/O 层的逐块钩子已预留，但**当前没有产品侧消费者**（`XdvParser` 只出现在 `examples/xdvscan.rs`）；提前量阈值 LIB-3d **未补测**。⚠ 产品侧的「编译中预览」走的是**趟边界出图**（§3 线程模型），不是这条逐块钩子 |
| **P6** 内存 XDV → PDF（J3） | **已落地（整份路线）**：捕获表里的 XDV → `XdvipdfmxEngine` → 原子替换到项目根。**前缀（截断）路线按判决不做**（三种截断点一律 exit=1、0 字节产出） |
| **P7** UI / 状态 / 回退可见性 | **已落地**：状态栏形态 chip（`子进程` / `库内嵌` / `库形态不可用`），判定由后端纯函数算一次；D1 的显式失败已真机验证（无"已回退"文案） |
| **P8** 验证与收口 | 文档与 ADR **已收口**（ADR-0012 已接受、本文档与 [modules.md](./modules.md) §2.7 已成文）；计划里的"一次独立验证"**没有单独进行**——验证随各次改动做 |

## 5. 风险与未测清单

**风险（编号用 `LBR-*`，与原测试计划的 `R-*` 区分）**：构建链变重与 CI 成本（21 个 native/C 链标记，Windows 必须维护 vcpkg）；**原生探测是硬失败**（少一个 port 即整仓构建失败）；CJK 字体 / fontconfig（库形态字体走 bundle，须坐实"中文不依赖本机 fontconfig"）；bundle 体积与许可（**G-L1..G-L9 未全绿前不得把 Tectonic/bundle 放进任何发行物**；C 依赖许可分布 `[未定]`）；Windows 未签名分发（与 ADR-0003 叠加，库形态不改变这点）；TL-less 下 SyncTeX 不可用（**库形态不解决**，仍依赖本机 CLI）；上游 0.x API 变更（钉 `Cargo.lock`，`IoProvider` 无 `create_output`/`output_file_names`/`output_open`）；取消语义退化（X-3）；缓存目录无进程间锁；`.fmt` 落错目录；**子目录 `\include` 的中间文件与搜索根**（U-14，最高风险的未验证项）；路径 B = 自研驱动取向（已走 ADR-0005 的决策门并获批准，登记在 ADR-0012）。

**未测（不得当成立）**：构建代价三项；"空缓存 + 有网"的真实 bundle 下载行为与文案（U3）；`ProcessingSession` 全管线"TeX 源码内存输入 → 内存输出"（U1，仅作路径 A 的保留记录）；排版段 ≤16 KB 粒度直接观测（U2）；转换段"逐页 PDF 已可渲染"（U4）；内存层 `.log`/`.synctex.gz` 的可用性（U5）；**编辑触发档收敛预算的机器自适应**（阈值现为常数、按发布档标定，其它机器/CI 上"一趟耗时"的分布未测）；按 D-1/D-2 注入时间源后跨天两次运行的 PDF 是否逐字节相等；PDF 加密路径的 `SOURCE_DATE_EPOCH`（**未决策**）。

**明确不做（不是未完成项）**：页级即时 flush（不经上游改 C 不可能，已写成需求项交上游）；字节级 I/O 拦截（需改引擎/文件系统驱动/API hook，且无消费方）；"编辑器未保存缓冲直喂引擎"（与 ADR-0007 冲突）；裸前缀 XDV → 部分 PDF（**已证伪**：要做得自己定义前缀协议并补后同步）。

## 6. 证据

- **收口证据** [../archive/research/tectonic-lib-evidence.md](../archive/research/tectonic-lib-evidence.md)：能真编出 PDF（中文夹具 1 页 / 7590 B，文本层可抽回）、bib 趟与收敛实测、页哈希与 A/B/C 的 GUI 真机矩阵 V1–V7、常驻档判决数字、编辑触发档收敛预算数字与反例。
- **门禁与纪律** [../archive/research/tectonic-test-plan.md](../archive/research/tectonic-test-plan.md)：编号空间与判据（`LIB-*` / `INT-*` / `G-L*` / `U-*` / `R-*`）。
- **方案与裁决（只读素材）** [../archive/tectonic-library-plan.md](../archive/tectonic-library-plan.md)：§3.2 crate 落点裁决、§3.4 路径 B 九步调用序列、§4.2 必须登记的例外、§5 bundle/缓存/确定性、§6 P0–P8 判据与复核命令。
- **ADR**：[ADR-0012](../adr/0012-tectonic-library-form-engine.md)（选定路径 B + X-5 偏离 + 批准记录）、[ADR-0010](../adr/0010-infra-crate-boundary.md)（infra 是默认依赖图的唯一落点）、[ADR-0005](../adr/0005-latexmk-first-incremental-next.md)（**保留条款**："性能瓶颈出现时以预算为标尺评估是否自研驱动"——本次正是触发该条款并获批准，ADR-0005 本身**不改**）、[ADR-0014](../adr/0014-tectonic-first.md)（Tectonic-first：新功能的基准形态，默认引擎已切 Tectonic、形态位仍默认子进程）。
- **复核脚本（已入仓）**：`scripts/validate-pdf.mjs`、`scripts/bench-tectonic-lib.mjs`（`--runs 3 --mode fresh|resident`）、`scripts/tectonic-lib-xdvscan.mjs`。**仍缺** `scripts/bench-tectonic-build.ps1`（P1 的构建补测入口，当前实现只在 `test_file/research-tectonic-lib/build-spike/` 下的开发期脚本里）。`test_file/research-tectonic-lib/**` 只作**开发期原始证据**，不作复核入口。

## 7. 新实测证据的落点约定

- 新的实测结论沉淀进**本文档**或 [design.md](../design.md) 的对应小节；复核脚本进 `scripts/`。
- **`docs/archive/` 不再接收新证据**（归档只读）：历史约定「结论文档放 `docs/research/`」已随归档失效，历史结论只留在 `../archive/research/` 里作为带日期的证据。
- `test_file/**` 与 `examples/` 下的原始证据不入库、不作复核入口。
