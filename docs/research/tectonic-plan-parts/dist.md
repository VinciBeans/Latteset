# T4 分册：分发 / 缓存 / 许可——检查项与上线门禁

> 分册归属：团队 `tectonic-test-plan` / 成员 `dist-license` / 任务 `t4`。
> 覆盖：① 安装形态对比与测试项 ② bundle 三条路线 ③ bundle 制作流水线 ④ 许可审计门禁 ⑤ 用户机器前置条件与失败可见性。
> 不覆盖：引擎行为（T1 `engine.md`）、应用集成面（T2 `integration.md`）、测量协议与阈值（T3 `perf.md`）；本册**不改任何产品代码**。
> 标记约定：**[实测]** = 本轮只读核查中实跑并记下输出（命令与结果写在「证据·落点」）；**[源码]** = 0.17.0 源码副本（`test_file/tectonic-src`）行级证据；**[文档]** = 仓库既有文档/上游资料；**[推断]** = 由源码或机制推出、未实跑；**[未测]** = 无证据；**待审计** = 许可结论未定，不得当作结论使用。

---

## 0. TL;DR（给 T5 的 8 条；其中 F1–F3 会改集成方案原文的结论）

1. **F1｜`-b <本地 .tar>` 在 0.17.0 上不成立**：`detect_bundle` 对本地路径只接受 **目录 / `.zip` / `.ttb`**；传本地 `.tar` 直接报 ``error: `…bundle.tar` doesn't specify a valid bundle.`` **[实测]**。
   ⇒ `docs/research/tectonic-integration-plan.md` §1 表格「路线②」与 §5 第 2 条（"造一个 bundle，用 `-b <本地tar> -C` 离线编译"）**必须改为 `-b <name>.ttb`**（`bundle create … v1` 的产物）；目录形式只配调试用（还要求目录里有 `SHA256SUM`，且**没有 SEARCH/FILELIST 语义**）。
2. **F2｜`-C/--only-cached` 不是"绝不联网"**：冷缓存（没有 `hashes/<…>` 摘要文件）时它仍会去拉 `<url>.index.gz`。**[实测]**：
   `tectonic -C -b https://127.0.0.1:9/nope.tar …` → `error: this bundle isn't cached, and we couldn't get it from the internet. Error: error sending request for url (https://127.0.0.1:9/nope.tar.index.gz)`，exit 1。
   ⇒ **"离线"必须由"预置缓存 + `-C`"或"本地 `.ttb`"保证**；三条路线要各自测，不能只测一条。
3. **F3｜预置缓存路线对"上游 bundle 刷新"是硬失效**：远程摘要变化 → `bundle_hash` 变 → `data/<新 digest>` 为空 → `-C` 下每个文件 `NotAvailable` → 编译失败（不是静默重下）。**[源码]**（`crates/bundles/src/cache.rs:169-213, 351-392`）
   处置方向（二选一或都做）：把 bundle 身份**钉死在我们自己手里**（随包发 `.ttb`，身份=文件路径），或用我们自己的镜像 URL 并把它当作"不会变的快照"；负测方法见 `DIST-2.16`。
4. **F4｜`bundle create` 有两处静默成功 + 一处只 warn**，所以**流水线门禁不能看退出码**：
   ① `patch` 子进程的 `wait()` 返回码**没有被检查**（`select/picker.rs:196`，patch 不匹配也照样产出未打补丁的包）；
   ② `pack` 在 `content/` 缺失时只打 error 日志后 `return Ok(())`（`bundle/actions.rs:110-116`，exit 0）；
   ③ 最终 hash 与 `expected_hash` 不一致时**只 warn**（`actions.rs:89-96`）。
   ⇒ 判据必须是"日志里出现 `final bundle hash matches configuration` 且 hash 等于钉定值"+"ttb 文件存在且头部 digest 等于 `SHA256SUM`"。见 §4.3 的 `MB-1..MB-8`。
5. **F5｜缓存目录与命名已实测**（预置/清理/卸载/升级的判据全都基于它）：
   `%LOCALAPPDATA%\TectonicProject\Tectonic\cache\formats\<bundle digest>-latex-33.fmt`（**24,451,466 B**）、`…\cache\bundles\data\<digest>\…`（**421 文件 / 35,652,448 B，扁平布局**，如 `article.cls`、`latex.ltx`）、`…\cache\bundles\data\<digest>.index`、`…\cache\bundles\hashes\<sanitize(url)>`（65 B 摘要）与同名 `.lock`（10 B = 上次检查的 epoch 秒）。**[实测]**
   `TECTONIC_CACHE_DIR` 是 **bundles 与 formats 两个缓存的唯一运行期开关**（`crates/io_base/src/app_dirs.rs:82-96`，两处调用点：`cache.rs:129`、`src/config.rs:164`），且**指向不可用路径时立刻报错**（实测 `error: 当文件已存在时，无法创建该文件。 (os error 183)`——**本地化、不含路径**，应用侧必须自己补上下文）。**[实测]**
6. **F6｜许可是发包阻断项，且风险点已具体化**：Tectonic 本体 MIT（`LICENSE`）没问题；风险集中在 ① bundle 里**被我们的 spec patch 过的 4 个 TL 文件**（`latex.ltx`/`listings.sty`/`fontawesome.sty`/`fithesis-mu-base.sty`，其许可逐项**待审计**，而被修改过的文件在 LPPL 类条款下有明确要求）、② 官方 Windows 构建走 **vcpkg 全静态**（`x64-windows-static-release`，依赖 fontconfig/freetype/harfbuzz[graphite2]/icu），**静态链接把 C 库的许可义务带进我们的发行物**、③ bundle 内 OFL 字体与 LPPL/GPL 宏包（且 bundle 的 `ignore` 列表**剔掉了 TL 的 LICENSE/README/doc/source**，许可证文本不在包里）。**全部标「待审计」**，审计产物 `docs/research/tectonic-bundle-licensing.md` + 发行物 NOTICE。见 §5。
7. **F7｜首跑 189–214 s 与产品现状的 120 s 默认超时直接冲突**（实测数字见 `modern-engines-zh.md §8.2`；超时语义见 `AGENTS.md` roadmap ㉕：**超时不自动重试**）⇒ 首跑（下载/建 format）必须有独立的进度与超时策略，否则用户第一次点「编译」看到的是"超时错误 + 一键提高超时"，而真正需要的是"在下载/编译"。`DIST-5.4` 是 P0。
8. **F8｜形态建议（非结论）**：形态 B（随包 `tectonic.exe` 子进程驱动）风险最低；但"三件套"必须**钉成一处真相源**——**exe 版本 0.17.0 ↔ bundle 规格 `texlive2024-0312` ↔ `FORMAT_SERIAL`/`_v33` ↔ 预置 formats 的 `-33.fmt` 后缀**。任何一处独立升级都会静默改变行为（见 `DIST-1.2`/`DIST-2.18`）。

---

## 1. 基线事实（版本钉法与缓存结构，全部本次只读核查所得）

| # | 事实 | 值 / 结论 | 证据 |
|---|---|---|---|
| B1 | 运行期候选 exe | `Tectonic 0.17.0`。官方 zip：`21,060,223 B`，SHA-256 `f61ce51f0b0ade1015b7de7ef368541c5424e9756ecbd0d7af97d6d48030845f`（与官方 digest `f61ce51f…845f` 一致）；**解包后 `tectonic.exe`：51,538,432 B，SHA-256 `99ffcfdbf1ebf8bdda9e791942e3d06aedb12463fddc33f07de6f5211c8bf08d`** | **[实测]** `tectonic --version`、`Get-FileHash`；**[文档]** `tectonic-integration-plan.md` §6、`modern-engines-zh.md` §8.2（zip 20.1 MB 那一项是"MiB 口径"） |
| B2 | 引擎格式版本 | `tectonic_engine_xetex::FORMAT_SERIAL = 33` | **[源码]** `crates/engine_xetex/src/lib.rs:36` |
| B3 | 默认 bundle 来源 | `https://relay.fullyjustified.net`（`TECTONIC_BUNDLE_PREFIX` 默认值）→ URL = `<prefix>/default_bundle_v33.tar`（`format_version ≥ 32` 才带版本号） | **[源码]** `crates/bundles/src/lib.rs:39, 313-330` |
| B4 | 默认 bundle 的**格式** | 不是 ttb：URL 不以 `.ttb` 结尾 → `ItarBundle`（HTTP Range + `<url>.index.gz`，索引是**扁平 名字→offset**）+ `BundleCache` | **[源码]** `crates/bundles/src/lib.rs:242-258`、`itar.rs:115-145`；**[实测]** 缓存里文件是扁平的 `article.cls`/`latex.ltx` |
| B5 | 本地 bundle 只认三种 | 目录（`DirBundle`，需要 `SHA256SUM`；无 SEARCH 语义，按名字直接拼路径）／`.zip`／`.ttb`；**`.tar` 不在其中** | **[源码]** `lib.rs:275-287`、`dir.rs:27-39`、`crates/io_base/src/filesystem.rs:111-121`；**[实测]** 三个探针见 §0 F1 |
| B6 | 编译期可改默认 bundle | `TECTONIC_BUNDLE_PREFIX` / `TECTONIC_BUNDLE_LOCKED` 是 **`option_env!`（编译期）**，运行期改环境变量无效 | **[源码]** `lib.rs:313-330`、`tests/bundle_env_overrides.rs`、`docs/src/howto/build-tectonic/bundle-default-config.md` |
| B7 | 运行期可改默认 bundle | ① `-b/--bundle`（最高优先）② 用户配置文件 `%APPDATA%\TectonicProject\Tectonic\config.toml` 的 `default_bundles[0].url`（`PersistentConfig::open`，`serialization` 特性下才读） | **[源码]** `compile.rs:187-205`、`src/config.rs:84-157`；`app_dirs.rs:45-57` |
| B8 | bundle 规格（TL 版本钉法） | `bundles/bundles/texlive2024-0312/bundle.toml`：tarball `texlive-20240312-texmf.tar`，sha256 `87f7f5fd9618689cd9ae436ad37cefe43fabf1f2f457ac3860ec2aa854578d20`，`root_dir=texlive-20240312-texmf/texmf-dist`，`patch_dir=patches/texlive`，`expected_hash=8fae742a1d9453fc3abfe7c6971363696f611ade1266c17788ccb5c18ffd1312` | **[源码]** `bundles/bundles/texlive2024-0312/bundle.toml:1-38` |
| B9 | bundle 里被 patch 的 TL 文件（只有 4 个） | `latex.ltx`、`listings.sty`、`fontawesome.sty`、`fithesis-mu-base.sty`；patch 文件格式=**首行是目标路径**（支持 `{a,b}` 花括号），其余为 unified diff | **[源码]** `bundles/bundles/texlive2024-0312/patches/texlive/*.diff`、`bundles/bundles/README.md:55-73` |
| B10 | bundle 内名字解析规则 | 客户端用 bundle 自带的 `FILELIST`+`SEARCH` 解析（不是 kpathsea）：`FILELIST` 按路径排序；同名冲突按 SEARCH 行序→字母序 | **[源码]** `bundles/bundles/README.md:138-166`、`crates/bundles/src/ttb.rs:125-190` |
| B11 | **bundle 摘要的定义** | `content/SHA256SUM` = **sha256(`content/FILELIST`)**；`FILELIST` 里同时含（a）每个文件的 sha256、（b）为 tarball 输入生成的 `TAR-SHA256SUM`（=TL tarball 的 sha256）、（c）`SEARCH`/`SHA256SUM`/`FILELIST` 三个 `nohash` 条目 ⇒ **摘要传递性地钉住 TL tarball + 全部 patch** | **[源码]** `bundle/select/picker.rs:415-437, 483-557`、`bundles/README.md:41-51` |
| B12 | ttb v1 容器 | 66 B 头（`tectonicbundle` + u32 版本=1 + u64 索引位置 + u32 索引 gzip 长度 + u32 索引原始长度 + 32 B 摘要）+ 逐文件 gzip 块；索引可用 `dd if=x.ttb ibs=1 skip=<start> count=<len>` 取出后再 `gunzip` | **[源码]** `bundles/format-v1.md`、`bundle/pack/bundlev1.rs:22-217` |
| B13 | formats 缓存命名与失效 | `<cache>/formats/<bundle_digest>-<stem>-<FORMAT_SERIAL>.fmt`；**按 digest 自动失效**（换 bundle → 新文件名），写入用 tempfile+persist | **[源码]** `src/io/format_cache.rs:42-61, 88-103`；**[实测]** `formats/6ffe055852f8…-latex-33.fmt`（24,451,466 B） |
| B14 | bundles 缓存命名 | 摘要 `hashes/<sanitize(location)>`；数据 `data/<bundle_digest>/<文件解析名>`；索引 `data/<digest>.index`；预取清单 `data/<sanitize(location)>.prefetch`（**按 location 而非 digest**，跨版本复用）；下载走"临时文件 + rename（pid 后缀）" | **[源码]** `crates/bundles/src/cache.rs:127-320`；**[实测]** 目录清单见 §0 F5 |
| B15 | 远程摘要重检频率 | **7 天**（`.lock` 时间戳）；>7 天才会联网重取摘要；联网失败则**回退用缓存摘要**（`(Some(h),_ ) + Err → h`） | **[源码]** `cache.rs:169-213`；**[文档]** `CHANGELOG.md:17`（0.17.0 #1363） |
| B16 | 冷缓存下的预取 | 0.17.0 起冷缓存会用 `.prefetch` 清单**并发预取**已记录的工作集（#1379）；`batch_open` 只对 itar 打开 | **[源码]** `cache.rs:100-114, 219-225`、`itar.rs:310-316`；**[文档]** `CHANGELOG.md:10` |
| B17 | 上游 bundle 的运维主体 | relay 是 Azure 上的 nginx（`tectonic-relay-service` + `tectonic-cloud-infra`），**域名与 Azure 订阅由单一个人持有**；上游曾因 archive.org "被中国屏蔽"而换掉它 | **[文档]** `crates/bundles/CHANGELOG.md:64-88` |
| B18 | 上游 TL 节奏 | TL2024 bundle 由 0.16.0（2026-04-11）引入，0.17.0（2026-07-27）**仍是它** ⇒ 官方 bundle 落后本机 TeX Live 2026 两代 | **[文档]** `CHANGELOG.md:62-98` |
| B19 | 网络层 | `geturl` 默认 reqwest 后端用 `Client::new()`（未设超时/代理/UA 参数） | **[源码]** `crates/geturl/src/reqwest.rs:78-119`；代理是否走 `HTTPS_PROXY` 环境变量 **[推断]**（reqwest 默认行为），需实测 |

---

## 2. 安装形态对比与测试项（必覆盖 1）

### 2.1 三种形态的差异与风险（只列差异，不展开实现）

| 维度 | **M1 随包分发 exe**（形态 B：子进程驱动） | **M2 用户自装**（走 PATH，现状） | **M3 crate 内嵌**（形态 A：`tectonic` crate） |
|---|---|---|---|
| 用户前置 | 零（不需要 TeX Live） | 需 TeX Live/MiKTeX（现状：`latexmk`/`xelatex`/`synctex` 都按裸名走 PATH，`runner.rs:244,260`、`synctex.rs:60,86`） | 零（引擎在进程内） |
| 版本控制权 | **我们**（钉 0.17.0） | 用户（可能是旧版：`FORMAT_SERIAL` 不同 ⇒ 默认 bundle URL `_v<N>` 不同、`-b` 语义可能不同） | 我们（随 Cargo 依赖树） |
| 体积/构建成本 | exe `51,538,432 B`（压缩包 `21,060,223 B`）+ bundle 体积（**待实测**） | 0（复用用户 TeX Live） | 构建链极重：`vcpkg` + `cargo-vcpkg`（`mcgoo` 分支）+ harfbuzz 子模块 + 30+ C 依赖；Windows 官方路线见 `.github/actions/vcpkg-deps/action.yml` **[源码]** |
| 许可面 | 我们成为 **MIT 的 Tectonic 本体 + 逐包混合许可的 TL 文件 + 静态链接 C 库**的再分发者 ⇒ §5 全部适用 | 无（不分发） | 与 M1 相同（静态链接发生在我们的产物里） |
| 架构边界 | 与 Tectonic 0.17 与"引擎=子进程"的现状兼容（ADR-0010：进程调用属 infra） | 同上 | 引擎进 core/infra 边界要重划（ADR-0010 需修订） |
| 失败可见性 | 我们可控（自己构造命令行、自己解析输出） | 依赖用户环境（版本、PATH、字体） | 错误变成 Rust 错误，但要重写日志/进度/页事件 |
| 升级/卸载 | 我们要定义（见 `DIST-1.3`/`DIST-1.4`） | 用户自理 | 跟随应用版本（但引擎与 bundle 的耦合更紧） |
| 与 ADR 的冲突 | 与 ADR-0003（不签名分发）叠加：**未签名的 exe + 未签名的安装包**，SmartScreen 双重门槛 | 无 | 与 ADR-0003 的"不自维护引擎分支"不冲突（不改上游），但把 CI 变重 |

> 结论倾向（供 T5 写"决策点"）：**M1 为默认路线**（零前置是产品价值），M2 保留为"用户已有 TL"的退化路径，M3 只在需要"页级实时/内存内 XDV"时才谈（`tectonic-integration-plan.md` §2 已给结论）。

### 2.2 测试项

| ID | 目的 | 操作（可执行步骤） | 期望 | 判据（可判定） | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| DIST-1.1 | 证明"随包 exe 被真正使用"（不被用户 PATH 上旧版抢走） | ① 在 PATH 前部放一个假的 `tectonic.exe`（打印版本后退 1）② 启动应用编译 ③ 查应用侧记录的 exe 绝对路径与版本 | 应用用的是随包 exe | 应用日志/诊断里出现随包路径；`--version` 输出 == 钉定版本；假 exe 的输出**一次都没出现** | P0 | 产品需先定义 exe 解析顺序（随包目录 → 用户设置 → PATH）**[未测]** | 现状：`runner.rs:244` 只按裸名起进程（`Command::new(req.engine.binary_name())`），无路径配置 **[源码]** |
| DIST-1.2 | 三件套版本一致性 | ① `tectonic --version` ② 用它编一份中文夹具 ③ 断言 formats 目录新增文件后缀 == `-33.fmt`、`-b` 指向的 bundle 摘要 == 预期 | 三处版本一致，且不匹配时**拒绝启动**而不是静默跑 | 判据：版本号 == `0.17.0`（或显式允许的区间）**且** `FORMAT_SERIAL` 推论一致（本版=33）；不一致 → 用户在 UI 看到可操作错误（"引擎/bundle 版本不匹配"），不是英文 panic | P0 | T2 的引擎选择面 | B2/B3/B13 |
| DIST-1.3 | 升级路径（换 exe） | ① 用旧 exe 建缓存/编译 ② 覆盖为新 exe ③ 再编译同一文档 | 升级后首次编译成功；缓存按 digest/serial 自动失效重建 | 判据：升级后 `formats/` 出现与新摘要匹配的 `.fmt`；编译成功；**旧 `.fmt` 未被当作新版本的格式使用**（自证：改 `FORMAT_SERIAL` 的对照实验应走"重建"分支） | P0 | DIST-1.2 | B13 |
| DIST-1.4 | 卸载与残留 | ① 安装 → 编一个项目 → 卸载 ② 列出残留：项目文件、`%LOCALAPPDATA%\TectonicProject`、我们自己的缓存目录、`%APPDATA%\TectonicProject\config.toml` | 卸载后：项目文件不动；**我们创建的缓存目录按策略删除**；用户自己的 Tectonic 缓存不动 | 判据：清单逐项有二进制结论（保留/删除 + 理由）；卸载后不残留临时文件（`*-tmp-pid*`） | P1 | 产品决策：缓存目录放哪（见 `DIST-2.10`） | B14/B15 |
| DIST-1.5 | 用户自装形态的版本门禁 | 在 PATH 上放 0.16.x/0.15.x（或伪造 `--version`）后编译 | 明确提示"需要 ≥ 0.17.0（或我们测过的版本）"并给出下载/改用内置引擎的建议 | 判据：低版本 → 不进入编译，错误文案含**版本区间**与**处置动作**；`-b` 不支持本地 tar 这一条不成为用户困惑（因为默认路线不依赖它） | P1 | M2 是否保留 | B5/B6；`compile.rs:196-198` |
| DIST-1.6 | 形态 A（crate 内嵌）的**风险清单**（不做实现，只记录） | 只读核查：`Cargo.toml` 的 `[package.metadata.vcpkg]`、`vcpkg-deps` action、`crates/bridge_*` | 团队明确知道代价 | 判据：本项只产出"风险 + 触发条件"条目，不产出实现计划；触发条件=确需内存内 XDV/页事件 | P1 | 架构决策 | `test_file/tectonic-src/Cargo.toml:147-174`、`.github/actions/vcpkg-deps/action.yml`、`dist/vcpkg-triplets/x64-windows-static-release.cmake` **[源码]** |
| DIST-1.7 | 未签名 exe 的信任门槛 | 在干净 Windows 上安装并首次运行随包 exe（含 SmartScreen 拦截） | 用户有可操作的绕行说明 | 判据：文档（`docs/troubleshooting.md` 或安装页）含"未知发布者/如何继续"步骤；安装包内 exe 的 Authenticode 状态被记录 | P1 | ADR-0003 | `docs/design.md §分发`、`docs/adr/0003-*` **[文档]** |

---

## 3. bundle 三条路线（必覆盖 2）

三条路线的分野是**"运行期 bundle 从哪来"**：

| 路线 | 身份与机制 | 是否可能联网 | 现状 |
|---|---|---|---|
| **R1 预置缓存目录** | 拷 `cache/bundles/**` + `cache/formats/**`；运行期用官方 URL（或我们镜像）作为**身份字符串** | 有窗口：`hashes/*.lock` 超 7 天会重检远程摘要；`-C` 下缺文件即失败 | 缓存结构已实测（§0 F5），但**"只装缓存"没跑过完整离线验收** **[未测]** |
| **R2 `-b <本地 bundle>`（本地 ttb）** | `-b` 指向 `xxx.ttb`（或调试用目录）⇒ 绕过 `BundleCache`，无网络代码路径 | **完全没有**（本地包不走网络后端） | 只验证了"路径被接受/不存在的组合报错"；**真 ttb 端到端离线编译未跑** **[未测]** |
| **R3 懒下载（默认）** | 官方 URL + `BundleCache`：按需拉文件、`<url>.index.gz` 索引、`.prefetch` 并发预取 | 是（首跑必须联网） | 首跑数字已有 **[文档]** §8.2 |

### 3.1 R1 预置缓存目录

| ID | 目的 | 操作（可执行步骤） | 期望 | 判据（可判定） | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| DIST-2.1 | 预置缓存能支撑**离线**首编 | ① 在联网机用夹具跑一遍（生成 `bundles/data/<digest>`、`hashes/<url>`、`formats/<digest>-latex-33.fmt`）② 打包这三块 ③ 目标机放到缓存目录后 `-C` 编译 `min-zh`/`thesis` | 编译成功、日志**零** `downloading` | 判据：exit 0 + 日志中 `downloading` 行数 == 0 + `Missing character` == 0（中文判据沿用 §8.4）+ 产物存在；**反例自证**：拿掉 `formats/*.fmt` 后仍应成功（只是变慢） | P0 | T1 的中文判据、T3 的夹具 | 本机实测：热缓存 + `-C` + `--outfmt xdv` 编 1 页英文 → exit 0、**429 ms**、无 download 行 **[实测]**；§8.4 的中文判据 **[文档]** |
| DIST-2.2 | 必须连 `hashes/` 一起预置 | 只拷 `data/`（不拷 `hashes/`）后在无网环境 `-C` 编译 | **明确失败**而不是静默重下 | 判据：错误文案命中"this bundle isn't cached, and we couldn't get it from the internet"；且**不产生**新的下载尝试（日志无 `downloading`） | P0 | DIST-2.1 | `cache.rs:179-182`；实测同款文案见 §0 F2 **[源码]+[实测]** |
| DIST-2.3 | 体积账（判据要为产品决策服务） | 逐项测并记账：官方 exe `51,538,432 B`（压缩包 `21,060,223 B`）/ `data/<digest>`（本机 421 文件 `35,652,448 B`，**只覆盖夹具用到的包**）/ `formats` `24,451,466 B` / 自建 ttb（**待实测**）/ 安装包总量 | 得到一份可复核的体积表 + 一条产品上限 | 判据：表里每一行有"测量命令 + 字节数"；上限由 ADR 定（本册不预设数字）；**"够用"的定义=夹具全覆盖（`min-zh`/`thesis`/`multifile` 都零下载）** | P0 | 产品决策 | 本机实测数字 **[实测]**；`tectonic-integration-plan.md` §3 摩擦点 5 **[文档]** |
| DIST-2.4 | 缓存命中不全时的降级可见 | 预置"中文常用子集"（不含 `multifile` 需要的某些包）→ 编 `multifile` | 用户看到"缺包/可联网补齐"的可操作提示，或自动改用联网路线 | 判据：失败文案里含**缺的文件名**且 UI 提供"允许联网补齐/切换 bundle"动作；不得只给 TeX 的 `File not found` | P0 | T2 的错误呈现 | `cache.rs:370-372`（`only_cached` → `NotAvailable`）**[源码]** |
| DIST-2.5 | 预置 formats 的收益与风险 | ① 有 `.fmt`（24.45 MB）② 无 `.fmt` 各跑一次 | 有 `.fmt` 时首编明显更快；无 `.fmt` 时**自动重建**且不报错 | 判据：两次耗时的**同批交错中位**（`perf.md` §2 协议）+ `formats/` 目录文件数变化；重建耗时记为 **[未测]**（T3 §0 第 7 条已单列此项） | P0 | T3 协议 | B13；T3 `perf.md` §0-7 |
| DIST-2.6 | 缓存目录的"不可用"必须可见且可处置 | 把 `TECTONIC_CACHE_DIR` 指向 ① 一个普通文件 ② 只读目录 ③ UNC/不存在的盘 后编译 | 用户看到"缓存目录不可用 + 路径 + 处置建议" | 判据：应用侧错误文本**必须包含我们传入的路径**（因为引擎原文不含路径、且随系统语言本地化：实测 `error: 当文件已存在时，无法创建该文件。 (os error 183)`）；③ 类场景同样有二进制结论 | P0 | DIST-2.10 | **[实测]** + `app_dirs.rs:82-96` **[源码]** |
| DIST-2.7 | 缓存清理策略 | 删除整个缓存目录 / 只删 `data/` / 只删 `formats/` 后各编译一次 | 三种情况都能自愈（重建或重新下载），且行为可预期 | 判据：逐项给出"删什么 → 发生什么（重下/重建/失败）+ 耗时量级"；**"删了 formats 不删 data"必须仍能离线成功** | P1 | DIST-2.1 | B13/B14 |

### 3.2 R2 `-b <本地 bundle>`（**注意 F1：不是本地 tar**）

| ID | 目的 | 操作（可执行步骤） | 期望 | 判据（可判定） | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| DIST-2.8 | **本地 `.ttb` 端到端离线编译**（本轮未跑，是本册第一优先的待办） | ① 造 ttb（§4 流水线）② 把 `TECTONIC_CACHE_DIR` 指向**空目录** ③ `tectonic -b <x.ttb> -C --outfmt xdv --keep-logs` 编 `min-zh`/`thesis` ④ 断网重跑 | 无网可编；缓存目录只长出 `formats/`（不出现 `bundles/`） | 判据：exit 0 + 产物页数一致 + `cache/bundles` **不存在**（证明没走 BundleCache）+ 空缓存下首次编译会**新建 `<新digest>-latex-33.fmt`** | P0 | §4 的 ttb；T1 判据 | `lib.rs:275-287`、`ttb_fs.rs:42-82` **[源码]**；本轮实测只到"路径被接受/坏文件报错" **[实测]** |
| DIST-2.9 | 路径与格式错误必须可判定 | 依次测：不存在路径／目录（空）／坏 `.ttb`／`.zip`／`file://` URL／本地 `.tar` | 每种都有**稳定的错误文案**，且不尝试联网 | 判据（本轮已实测 3 种，可直接固化）：`.tar` → ``doesn't specify a valid bundle.``；空目录 → `bundle does not provide needed SHA256SUM file`；坏 `.ttb` → `failed to fill whole buffer`；**全部 exit 1、无网络请求** | P0 | — | **[实测]**（三条探针命令见 §0 F1） |
| DIST-2.10 | 缓存目录可被我们独占（避免与用户自己的 tectonic 打架） | 设 `TECTONIC_CACHE_DIR=<应用私有目录>` 后编译；再检查用户默认缓存未被改动 | 全部缓存落在我们指定的目录；用户 `%LOCALAPPDATA%\TectonicProject` 不变 | 判据：编译前后对比两处目录的**文件数+mtime**，我们的目录增长、用户的零变化；卸载时只需删我们的目录 | P0 | 产品决策（随包 vs 共享） | `app_dirs.rs:82-96`、`cache.rs:129`、`config.rs:164` **[源码]** |
| DIST-2.11 | ttb 版本/格式不匹配的未来兼容 | 造一个头部 version=2（手工改字节）的 ttb 后编译 | 明确报"不支持该 bundle 版本" | 判据：错误文案含"version/不支持"语义；不出现 panic；**本轮未测**（0.17.0 只有 v1：`BundleFormat::BundleV1`） | P1 | — | `bundle/create.rs:75-88`、`bundles/format-v1.md` **[源码]** |

### 3.3 R3 懒下载（默认形态）

| ID | 目的 | 操作（可执行步骤） | 期望 | 判据（可判定） | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| DIST-2.12 | 首跑成本可预期、**不撞超时** | 清空缓存 → 编 `min-zh`（104 个按需文件）与 `thesis`（73 个）→ 记录耗时与 `downloading` 行数 | 首跑有可见进度；不触发产品超时错误 | 判据：首跑期间状态栏可见"已获取 N/M 文件"或等价进度（现状是逐文件 `note: downloading <name>`、**无百分比**）；耗时 ≤ 产品为"首跑"单列的上限（**待产品定**，依据 §8.2 的 189/214/51 s）；**不得**在首跑中弹出"编译超时（120 s）" | P0 | DIST-5.4 | §8.2 **[文档]**；`itar.rs:288`、`ttb_net.rs:171` **[源码]** |
| DIST-2.13 | 二次编译不再联网 | 首跑完成后，记录缓存目录 → 断网或 `-C` 再编两次 | 二次起走缓存 | 判据：日志 `downloading` 行数 == 0；耗时回到 §8.3 量级（`min-zh` 1.59 s / `thesis` 2.52 s 的 3 轮中位，允许同批 ±20%） | P0 | T3 协议 | §8.3 **[文档]** |
| DIST-2.14 | 网络失败的重试与最终文案 | 用"立刻拒连"（`127.0.0.1:9`）与"黑洞地址"（不可达 IP，触发超时）两种 bundle URL 各编一次 | 有限等待 + 可操作错误 | 判据：拒连场景**必须**秒级失败并命中 `this bundle isn't cached, and we couldn't get it from the internet. Error: …`（实测）；超时场景**必须**在 X 秒内给出同类错误（X 由实测确定；`NET_RETRY_ATTEMPTS=3`、间隔 500 ms 是下限，`Client::new()` **未设超时** ⇒ 上限取决于平台 TCP 超时 **[推断]**） | P0 | DIST-5.2 | `cache.rs:170-182`、`lib.rs:44-45`、`geturl/src/reqwest.rs:78-119` **[源码]** + **[实测]** |
| DIST-2.15 | 代理/企业网络 | ① 设 `HTTPS_PROXY=http://127.0.0.1:1`（坏代理）② 设正确代理 ③ 设 `NO_PROXY` | ①失败可见 ②可下载 ③直连 | 判据：三种配置下 `downloading`/错误行为符合预期；**代理是否被 reqwest 默认读取需实测确认 [推断]**（`Client::new()` 未显式 `.proxy()`） | P1 | DIST-2.14 | B19 |
| DIST-2.16 | 上游 bundle 刷新导致**预置缓存失效**（负测，F3） | ① 备份 `hashes/<url>` ② 把它改成另一个合法摘要（或在镜像上换包）③ `-C` 编译 | 失败得**可见**（而不是静默重下几百 MB，也不是静默用旧文件） | 判据（两条分支都要测，因为失败点不同）：**离线** → 失败在"取索引"，错误指向 `<url>.index.gz`；**在线** → 索引可下、但每个缺失文件 `NotAvailable` → 编译失败且**错误指向缺失的文件**；不带 `-C` 且在线 → 日志出现大量 `downloading`（应用须能取消/限速） | P0 | DIST-2.1 | `cache.rs:191-201, 271-320, 351-392` **[源码]** |
| DIST-2.17 | 半下载/中断的健壮性 | 下载中强杀进程（首跑中途），再重跑 | 能继续；不产生损坏文件 | 判据：缓存里**不出现零字节/半截的目标文件**（下载先写 `*-tmp-pid<pid>` 再 rename）；允许残留 tmp 文件，但应用启动时应能清理；重跑最终成功 | P1 | — | `cache.rs:264-269, 381-389` **[源码]** |

### 3.4 三条路线的共同项

| ID | 目的 | 操作 | 期望 | 判据 | 优先级 | 依赖 | 证据·落点 |
|---|---|---|---|---|---|---|---|
| DIST-2.18 | **缓存失效与升级的单一规则** | ① 改 bundle（换 ttb / 换 URL）② 看 `formats/` 变化 | 新旧格式共存、互不污染；旧文件可安全清理 | 判据：`formats/` 中文件名 = `<digest>-latex-33.fmt`，digest 随 bundle 变；重编后**读取的是新 digest 的文件**（自证：删掉新文件会重建，删掉旧文件无影响） | P0 | — | B13 **[源码]+[实测]** |
| DIST-2.19 | 多实例/并发安全（GUI + CLI 同时） | 两个进程同时首编同一文档（共用一个缓存目录） | 不损坏缓存 | 判据：两次都成功（或后到者看到正确错误）；缓存目录无损坏文件；`*-tmp-pid*` 残留可有但不得被当成正式文件 | P1 | — | `cache.rs:264-269, 299-317` **[源码]** |
| DIST-2.20 | 磁盘空间不足 | 把缓存目录放到小分区（或配额）制造 ENOSPC | 错误可见、可处置、不损坏已有缓存 | 判据：文案含"磁盘空间"语义 + 路径；已有 `.ttb`/缓存仍可用；**阈值**（安装前检查剩多少）由 DIST-2.3 的体积表 + 产品定 | P1 | DIST-2.3 | **[未测]** |

---

## 4. bundle 制作流水线（必覆盖 3）

### 4.1 版本钉法与"真相源"

| 钉住的项 | 值 | 放在哪（唯一真相源） |
|---|---|---|
| 构建器 = 运行期引擎 | `tectonic 0.17.0`（同一版本，避免 builder/runtime 行为漂移） | 我们的版本清单 / 构建脚本常量 |
| bundle 规格 | `texlive2024-0312`（`bundle.toml` 的 `name`） | `bundle.toml`（进 git） |
| TL tarball | `texlive-20240312-texmf.tar`，sha256 `87f7f5fd…8d20`，`root_dir=…/texmf-dist` | `bundle.toml` 的 `source.tarball.{path,hash,root_dir}` |
| patches | `patches/texlive/*.diff`（4 个），首行=目标路径 | `patches/`（进 git，改动必须走 review） |
| 产物摘要 | `expected_hash`（= sha256(FILELIST)）；变更 spec/patch **必须显式更新**它 | `bundle.toml` 的 `[bundle].expected_hash` |
| 产物文件 | `<build>/<name>/<name>.ttb` + 它的 SHA-256 与字节数 | 发布物清单（安装器/CI 产物元数据） |

### 4.2 可复现步骤（从 TL tarball 到 ttb）

```text
# 0) 前置：Windows 上需要 GNU patch 在 PATH（tectonic 直接调 `patch`；spawn 失败才报错）
#    需要与运行期同版本的 tectonic（含 -X 子命令）
tectonic --version            # 必须 0.17.0
patch --version               # 必须是 GNU patch（tectonic 以 `patch --quiet --no-backup` 调用它）

# 1) 取 tarball 到 spec 目录旁（bundle.toml 里给的来源；sha256 由 select 自己校验）
#    texlive-20240312-texmf.tar.xz → 解出 texlive-20240312-texmf.tar

# 2) 准备 spec 目录（拷上游 bundles/bundles/texlive2024-0312/：bundle.toml + include/ + patches/）

# 3) 造包（select = 选择+打补丁；pack = 打包成 ttb v1）
tectonic -X bundle create --build-dir ./build ./texlive2024-0312/bundle.toml v1
#    可分开跑：--job select / --job pack（用于调试 content/）

# 4) 门禁（见 4.3；不看退出码）

# 5) 归档：ttb 的 sha256 + 字节数 + 本次 build 日志 + CONTENT 清单
```

### 4.3 流水线硬门禁（**必须做，因为上游有三处不拦**）

| ID | 门禁 | 判据（可判定） | 为什么必须 | 证据 |
|---|---|---|---|---|
| MB-1 | tarball 哈希 | 日志出现 `OK, tar hash matches bundle config`；**绝不使用 `--allow-hash-mismatch`** | 不匹配时上游会 `bail!`（这一点是硬的），但 `--allow-hash-mismatch` 会把它降成 warn | `select/picker.rs:414-438` **[源码]** |
| MB-2 | patch 全部落地 | ① patch 计数 `patch_found == patch_applied` ②**逐个人工断言**：`content/.../latex.ltx` 含 `Tectonic: no terminal input allowed`（另 3 个 patch 各写一条断言） | `patch` 子进程的 **`wait()` 返回码未被检查** ⇒ 打不上也照样出包 | `picker.rs:176-198` **[源码]**；本机缓存中的 `latex.ltx:6920-6921` 正是该 patch 文本 **[实测]** |
| MB-3 | 最终摘要 | 日志出现 `final bundle hash matches configuration`，且 hash == `expected_hash`；**只看这一行，不看退出码** | hash 不匹配时上游**只 warn** | `bundle/actions.rs:83-97` **[源码]** |
| MB-4 | `content/SHA256SUM` 自校验 | 独立计算 `sha256(content/FILELIST)` == `content/SHA256SUM`；且 `content/` 里存在 `TAR-SHA256SUM`（值 == TL tarball 的 sha256） | 这是"TL 版本 + 全部 patch"的唯一传递性指纹 | `picker.rs:525-557` **[源码]** |
| MB-5 | ttb 产物字节 | `<build>/<name>/<name>.ttb` **存在且 > 0**；头部 14 字节 == `tectonicbundle`、u32 版本 == 1、头部第 34..66 字节的 32 B == `content/SHA256SUM` | `pack` 在 `content/` 缺失时**只报错并 exit 0** | `bundle/actions.rs:102-136`、`pack/bundlev1.rs:181-217` **[源码]** |
| MB-6 | 冒烟（用**运行期**的命令行形态） | ① `tectonic -C -b <ttb> --outfmt xdv` 编 `min-zh`+`thesis` 成功 ② `--outfmt fmt` 能建 latex/plain 格式 ③ 页数与 §8.4 基线一致 | 打包器与运行期是同一份代码，但只测"能建包"证明不了"能编译" | 上游自带 `bundles/tests/test.sh <ttb> {files,classes}`（bash + `realpath`，Windows 需 WSL/Git Bash）**[源码]** |
| MB-7 | 可复现性 | 同一台机器、同一 tarball+patches，**重跑一次**：`expected_hash` 与 `content/SHA256SUM` 完全相同 | 这是"版本钉法"的前提；否则无法把摘要写进发布清单 | `picker.rs:531-540`（FILELIST 按路径排序）**[源码]**；**ttb 逐字节可复现 [未测]**（gzip 层） |
| MB-8 | 许可清单随产物 | 每一版 ttb 都产出/更新 `docs/research/tectonic-bundle-licensing.md` 与发行物 NOTICE（§5） | 见 §5 | — |

### 4.4 谁来维护 / TL 升级怎么办

| 问题 | 结论 |
|---|---|
| 谁维护**上游** bundle 与 relay | 上游 tectonic-typesetting（规格在 `bundles/` 子目录；relay 是 Azure 上的 nginx）。**域名与 Azure 订阅由单一个人持有** ⇒ 供应链单点 **[文档]** `crates/bundles/CHANGELOG.md:64-88` |
| 我们依赖官方 relay 的风险 | ① 单点运维；② 上游曾因"archive.org 被中国屏蔽"换服务（说明**中国可达性是他们的明确关切，但不是对我们的承诺**）；③ 官方 bundle 目前落后本机 TL 两代（TL2024 vs TL2026）⇒ **新宏包可能不在包里** **[文档]** |
| 谁来维护**我们**的 bundle | **[待确认]** 需要在 T5 文档里落一个责任人槽位（建议：Rust/构建维护者 1 人 + 发布检查清单勾选项）。理由：spec/patch 一改就会改变 `expected_hash`，必须有人对"包变了"负责 |
| TL 升级路线 A：跟随上游 | 等上游出 TL2025/2026 对应 `FORMAT_SERIAL` 与 bundle → **exe + bundle + 缓存三件一起换**；代价=长期等待（TL2024 自 0.16.0 起未变） |
| TL 升级路线 B：自建 bundle（推荐先做一次可行性验证） | 把 spec 指向 TL2026 的 `texlive-<date>-texmf.tar`（改 `source.tarball.*`），重跑 §4.2/§4.3；旧 patch 可能失效（`latex.ltx` 行号会漂，unified diff 可能 reject）⇒ **必须逐条重验 MB-2**，并重新钉 `expected_hash`；合法性/patch 归属见 §5 |
| 升级的最小动作清单（每次 TL 或 patch 变更） | ① 改 `bundle.toml`/patches ② 跑 §4.2 ③ 过 MB-1..MB-8 ④ 更新发布清单（ttb 大小+SHA256+`expected_hash`）⑤ 重跑 DIST-2.8 的离线冒烟 ⑥ 复核 §5 的许可清单是否新增包/新许可 |
| 什么时候**不**升级 | 官方 bundle 未变、用户模板不含新宏包时不动 —— 因为换 bundle 会**整体失效 formats 缓存**并可能让"预置缓存"路线重新下载（F3） |

---

## 5. 许可审计门禁（必覆盖 4）

### 5.1 已知事实（可引用）与待审计清单

| # | 事实（可引用） | 风险等级 | 证据 |
|---|---|---|---|
| L1 | **Tectonic 本体 MIT**（`LICENSE`："Copyright 2016-2023 the Tectonic Project"）；仓库根 `Cargo.toml` 也声明 `license = "MIT"` | 低（需保留版权与许可文本） | **[文档]** `tectonic-integration-plan.md` §4；**[源码]** `test_file/tectonic-src/LICENSE`、`Cargo.toml:28` |
| L2 | **bundle = TeX Live 文件集合**：逐包混着 LPPL/GPL/MIT/OFL…（**逐包结论待审计**）；TL 整体无单一许可证 | **中**（需逐包清单 + 义务摘要） | **[文档]** 同上；**[源码]** `bundle.toml` 的输入就是 `texmf-dist` |
| L3 | **bundle 里被 patch 的 4 个文件**：`latex.ltx`、`listings.sty`、`fontawesome.sty`、`fithesis-mu-base.sty`（**各自许可待逐项审计**；LaTeX 内核/常见宏包多为 LPPL 类，而 LPPL 对"修改后的文件"有明确条款）⇒ 需逐条定性：修改说明、文件是否需改名、许可证文本随附 | **中高** | **[源码]** `patches/texlive/*.diff`；**[实测]** 本机缓存里的 `latex.ltx` 已含 patch 文本 |
| L4 | **bundle 里几乎没有许可证文本**：`bundle.toml` 的 `ignore` 剔除了 `LICENSE.md`/`README`/`readme.txt`/`00readme.txt`/`doc/.*`/`source/.*` | **中**（我们必须自己产出 NOTICE，不能"包里带了"） | **[源码]** `bundle.toml:9-24, 69-70` |
| L5 | **字体**：`fonts//` 在 search_order 内 ⇒ 包内可能含 OFL 等字体（OFL 有"保留字体名/改名"条款与许可证随附要求） | **中** | **[源码]** `bundle.toml:77-88`；**[文档]** §1"中文实测不依赖 bundle 字体也能排" ⇒ **裁剪 CJK 字体是同时降体积与降许可面的杠杆** |
| L6 | **C 依赖走 vcpkg 全静态**：`x64-windows-static-release`（`VCPKG_LIBRARY_LINKAGE static`），依赖 `fontconfig`/`freetype`/`harfbuzz[graphite2]`/`icu`（另有 libpng/zlib/OpenSSL 等，见文档）；**静态链接 ⇒ 义务随二进制一起转移给再分发者** | **中高**（尤其若存在 LGPL-only 组件：graphite2 常被举为 LGPL 一例，**具体结论待审计**） | **[源码]** `Cargo.toml:147-174`、`dist/vcpkg-triplets/x64-windows-static-release.cmake`、`.github/actions/vcpkg-deps/action.yml`；**[文档]** `docs/src/howto/build-tectonic/index.md:31-46` |
| L7 | 我们**只分发上游预编译 exe**（不自建）时，上述义务**不消失**：再分发者仍需履行 | 中 | **[文档]** `tectonic-integration-plan.md` §6（20.1 MB 官方 zip + digest） |
| L8 | 我们自己的仓库 **MIT**（`LICENSE`：Copyright (c) 2026 WenqiBian），且 ADR-0003 说"MVP 前私有、之后开源" | 降低"LGPL 重新链接"类义务的难度（若我们同时公布源码与构建脚本），但**不能替代逐项审计** | **[源码]** 根 `LICENSE`；**[文档]** `docs/adr/0003-*` |
| — | 以上 L2/L3/L5/L6/L7 的具体结论（能不能闭源分发、需要哪些声明、是否必须改名/附全文） | — | **全部「待审计」** ——本册不给法律结论 |

### 5.2 审计做法（三条清单 + 风险筛选规则）

**清单 A：Rust 全依赖（含我们自己的 crate 树）**

```text
# 在"最终形态"的仓库里跑（形态 M1 只需 exe 的版本信息；M3 需要整棵树）
cargo metadata --format-version 1 > deps.json
# 提取 license/license_file，列出空值（空值必须手工补齐，不能算通过）
# 可选工具：cargo deny check licenses / cargo-bundle-licenses / cargo-about
```
判定：**每一条依赖的 `license` 或 `license_file` 都有结论**（白名单 / 需声明 / 需法务），空值不得留白。

**清单 B：bundle 内文件 → TL 包 → 许可证**

```text
# ① 从我们自己的 ttb 抽索引（不依赖网络）
dd if=<name>.ttb ibs=1 skip=<index_start> count=<index_gzip_len> | gunzip   # 索引位置在 pack 日志里
#    （Windows 侧用 PowerShell 读等长字节，或直接读 select 产物 content/FILELIST，含每文件 sha256）
# ② 列表化：tectonic -X bundle search            # 打印 bundle 内全部文件名
# ③ 归属映射：用同版本 TeX Live 的 texlive.tlpdb（含每包的 license= 字段）把文件映射到包
#    ⇒ 得到"包 → 许可证"分布表
```
判定：**每个出现在 FILELIST 里的文件都能追到一个包与一个许可证**；追不到的（自造文件、`TAR-SHA256SUM`、`tectonic/*.tex`）单独列出并定性。
> 前置待确认：`texlive-<date>-texmf.tar` 里是否含 `tlpkg/texlive.tlpdb`（不含就要从同版本安装器包/CTAN 取）——**列入流水线第一步要确认的事项 [未测]**。

**清单 C：二进制与 C 依赖的许可证文本**

```text
# ① 上游 exe：记录 URL + SHA-256 + 版本（已有：0.17.0；zip 21,060,223 B / f61ce51f…845f；
#    解包 exe 51,538,432 B / 99ffcfdb…bf08d —— 发行物里放的是这个 exe，它的哈希才是必须钉的）
# ② C 依赖：vcpkg port 的 vcpkg.json 与 LICENSE（fontconfig/freetype/harfbuzz/graphite2/icu/libpng/zlib/openssl）
#    形态 M3 还要 clone 子模块 crates/bridge_harfbuzz/harfbuzz（tarball 不含）
```
判定：每个 C 依赖的许可证 + 链接方式（静态/动态）有记录；**若存在仅 LGPL 的静态链接组件**，必须给出"义务满足方案"三选一：附必要材料（源码/目标文件/可重新链接形式）／改用动态链接（如 Linux 包管理形态）／构建时裁掉该组件（若功能可裁）。

**风险筛选规则（审计脚本按此分档，避免"全是高风险"式无效报告）**

| 档 | 许可 | 处置 |
|---|---|---|
| R1 放行 | MIT/BSD/Apache-2.0/zlib/libpng-2.0/FTL/X11/Unicode-3.0/public-domain/CC0 | 只需并入 NOTICE 的清单 |
| R2 需声明 | OFL-1.1（字体：许可证随附 + 保留名称条款）、LPPL-1.3c（宏包：许可证副本 + **修改后的文件**条款）、GPL-2.0/3.0（若仅作为"聚合"随包分发不改变我们代码的许可，但**修改**过则须提供对应源码与修改说明） | 逐条给义务动作 + 落到 NOTICE/About 页 |
| R3 需法务 | LGPL（静态链接）、AGPL、许可不明/多重许可冲突、无许可证文本的包 | 阻断发包，直到有书面结论 |
| R4 排除 | 仅随 `doc/`/`source/` 分发且**已被 bundle 忽略**的内容 | 记录"为什么可以不管"（注意：这也解释了 L4） |

### 5.3 必须产出的文档

`docs/research/tectonic-bundle-licensing.md`，**必须含**：
1. 范围与版本（Tectonic 版本 + bundle 规格名 + `expected_hash` + ttb 的 SHA-256 + 审计日期 + 审计人）；
2. 清单 A 表（Rust 依赖 → 许可 → 档位 → 义务）；
3. 清单 B 表（TL 包 → 许可 → 档位 → 义务；含**4 个被 patch 文件**的单独小节与处置）；
4. 清单 C 表（C 依赖/二进制 → 许可 → 链接方式 → 义务）；
5. 结论：是否可以按当前形态分发（分形态：M1/M2/M3）、必须随附的文件（NOTICE 全文/链接）、发布时需要做的动作；
6. **遗留待审计项**（明确标"未结论"，禁止用"应该没问题"收尾）。

### 5.4 「审计通过前不得发包」检查清单（可判定）

| ID | 检查 | 判定方式（二进制/可 grep） |
|---|---|---|
| G-L1 | `docs/research/tectonic-bundle-licensing.md` 存在且**结论小节完整** | 文件存在；文中不含"待审计/未结论"字样出现在"结论"小节（遗留项必须集中在小节 6） |
| G-L2 | 清单 A/B/C 三张表都非空，且**无空许可证字段** | 表行数 > 0；空值计数 == 0 |
| G-L3 | 发行物含第三方声明（`THIRD-PARTY-NOTICES` 或等价物，安装包内 + About 页可达） | 安装包解包后文件存在；About 页能打开该文件（人工 1 项） |
| G-L4 | Tectonic 本体许可文本随发行物（MIT 全文 + 版权行） | NOTICE 中含 "the Tectonic Project" 与 MIT 全文 |
| G-L5 | 若含 OFL 字体：许可证原文随附；若字体文件被改过，则未使用 OFL 的“保留字体名”（RFN） | 字体文件与其许可证逐项对照（人工 1 项，须记录判定人） |
| G-L6 | 4 个被 patch 的 TL 文件：**修改说明 + 归属 + 许可影响**三项齐全 | 审计文档中有该小节且三项非空 |
| G-L7 | C 依赖无"未结论的 R3 档"（或已有书面义务方案并落成动作） | 清单 C 中 R3 行数 == 0，或每行都有"义务满足动作 + 责任人" |
| G-L8 | 结论已进 ADR（建议新增 ADR：Tectonic 集成与分发许可），并被发布检查清单引用 | ADR 文件存在且状态为"已接受"；发布清单里有一条指向 G-L1..G-L7 |
| G-L9 | 每次**换 bundle/TL/引擎版本**后重跑 G-L1..G-L8（版本变化=新的分发物） | 审计文档头部"版本与摘要"字段与本次发布物一致（可 diff 判定） |

---

## 6. 用户机器上的前置条件与失败可见性（必覆盖 5）

| ID | 场景 | 操作 | 期望 | 判据（可判定） | 优先级 | 证据·落点 |
|---|---|---|---|---|---|---|
| DIST-5.1 | **缓存目录不可写/不可用** | `TECTONIC_CACHE_DIR` → ①普通文件 ②只读目录 ③不存在的盘/UNC ④被占用 | 可见、含路径、可处置 | 判据：应用错误文本含**我们提供的路径**与一句处置（"改到可写目录"）；引擎原文不够用（实测：`error: 当文件已存在时，无法创建该文件。 (os error 183)`，**无路径、且随系统语言变**） | P0 | **[实测]**；`app_dirs.rs:82-96` **[源码]** |
| DIST-5.2 | **网络被墙 / 代理** | ① 直连不可达（拒连、超时两种）② 坏代理 ③ 好代理 ④ TLS 被中间人替换 | 有限等待 + 可操作错误 + 不卡死 UI | 判据：拒连 ≤ 秒级、超时 ≤ X s（X 待实测）；错误文案含"网络/代理"与处置；编译可取消；**不出现**"编译超时（120 s）"这种误导性诊断 | P0 | DIST-2.14/2.15；`cache.rs:170-182`、`geturl/reqwest.rs:78-119` **[源码]** |
| DIST-5.3 | **磁盘空间** | 小分区/配额下首跑与建 format | 失败可见、可处置、不损坏缓存 | 判据：文案含空间语义；已有缓存与 `.ttb` 仍可用；安装前检查阈值（= DIST-2.3 体积表 + 余量） | P1 | **[未测]** |
| DIST-5.4 | **首跑时长 vs 产品超时**（F7） | 清缓存 + 默认超时配置下首编中文文档 | 不报"超时"；有进度 | 判据：首跑期间状态可见"在获取资源/已获取 N 个文件"；**首跑不进入超时分支**（实测首跑 189–214 s > 默认 120 s）；若确要超时，上限必须覆盖首跑且**不自动重试**（现状语义） | **P0** | §8.2 **[文档]**；`AGENTS.md` roadmap ㉕ **[文档]** |
| DIST-5.5 | **中途取消/杀进程** | 首跑中取消编译 / 杀应用（含杀子进程树） | 无残留锁死；下次能继续 | 判据：无残留 `tectonic.exe`（`taskkill /T` 语义已用于 latexmk，`runner.rs:690-710`）；缓存未被破坏；重跑可成功 | P0 | `cache.rs`（tmp+rename）、`runner.rs:690-710` **[源码]** |
| DIST-5.6 | **杀软/企业策略拦截未签名 exe** | 在开启 Defender/ASR 的机器上首次运行随包 exe | 安装文档给出绕行；失败时错误可诊断 | 判据：人工验收记录（1 项）+ `troubleshooting.md` 含该场景 | P1 | ADR-0003 **[文档]** |
| DIST-5.7 | **中文/非英文系统区域** | 在中文 Windows 上触发 §6 的各类错误 | 我们的文案不依赖引擎的本地化文本 | 判据：错误判定逻辑**不匹配引擎的错误字符串**（引擎文案已实测为本地化中文）；我们自己生成含路径/建议的文案 | P0 | **[实测]**（os error 183 的本地化文案） |

---

## 7. 上线门禁汇总（P0 清单，供 T5 直接收录）

| 门禁 | 内容 | 判据来源 |
|---|---|---|
| **G-1** | 许可审计通过（§5 的 G-L1..G-L9 全绿，含发行物 NOTICE） | §5.4 |
| **G-2** | 运行期**不依赖外网**的形态已被实测：`-b <ttb> -C` 在空缓存 + 断网下编出中文 PDF；预置缓存形态同样过 | DIST-2.1 / DIST-2.8 |
| **G-3** | 三类环境失败（缓存不可写 / 网络不可达 / 磁盘不足）都有可见、含路径、可处置的错误 | DIST-5.1 / 5.2 / 5.3 |
| **G-4** | 版本三件套（exe ↔ bundle 摘要 ↔ `FORMAT_SERIAL`/formats 后缀）有唯一真相源 + 不一致时拒绝运行 | DIST-1.2 / DIST-2.18 |
| **G-5** | 体积、安装、升级、卸载、缓存清理的行为已实测记录（含"缓存目录归谁"的产品决策） | DIST-1.3 / 1.4 / 2.3 / 2.10 |
| **G-6** | 上游 bundle/relay 不可用时的降级路径已定义并测过（自建 ttb 或预置缓存；含 F3 的负测） | DIST-2.16 / §4.4 |
| **G-7** | 首跑（下载/建 format）不撞产品超时，且有进度与取消 | DIST-2.12 / DIST-5.4 |
| **G-8** | bundle 流水线门禁 MB-1..MB-8 可自动跑（CI 或发布脚本），**不看 `bundle create` 的退出码** | §4.3 |

> **本方案不覆盖**（非目标）：不做 TL tarball 下载/真实建包（本轮只读核查）；不做法务意见；不落地任何产品代码；不决定"是否随包分发"这个产品决策本身（只给门禁与判据）。

---

## 8. 未验证清单（T5 必须保留，不得当结论用）

1. **本地 `.ttb` 的端到端离线编译**（DIST-2.8）：本轮只验证了"路径被接受"与三类错误路径；**真 ttb 未造、未编**。
2. **bundle 最终体积**：压缩包 21,060,223 B、解包 exe 51,538,432 B、本机缓存 65,254,100 B（426 文件，**只是夹具用到的子集**）；自建 ttb 的大小 **[未测]**。
3. **format 构建耗时**（无预置 `.fmt` 时的首编代价，文件 24.45 MB）：**[未测]**（与 T3 §0 第 7 条同一项）。
4. **逐包许可证清单**与 **C 依赖 LGPL 结论**：**待审计**（§5）。
5. **代理/证书在真实企业网的行为**（reqwest 默认是否读 `HTTPS_PROXY`）：**[推断]**，未实测；HTTPS 请求在本沙箱被拒（`HEAD https://relay…` 失败），**relay 在我们网络下的可达性也未验证**。
6. **上游 bundle 刷新频率**与"7 天重检"的真实用户体验影响（F3 的实际触发概率）：**[未测]**。
7. **`bundle create` 在 Windows 上的可复现性**（GNU patch 版本差异、行尾、路径大小写）：**[未测]**；ttb 逐字节可复现也是 **[未测]**。
8. **`%APPDATA%\TectonicProject\Tectonic\config.toml` 的用户级 bundle 覆盖**对我们命令行的实际影响（用户装了别的 bundle 时）：**[未测]**（机制见 B7）。
9. **磁盘空间阈值**、**多实例并发**、**杀软拦截**、**卸载残留**：均 **[未测]**。
10. **`texlive.tlpdb` 是否随 texmf tarball 提供**（决定清单 B 的自动化程度）：**[未测]**。

---

## 9. 最省钱的三个下一步（给 T5 排期用）

1. **造一个 ttb 并跑 DIST-2.8**（一条离线编译）——它同时解锁 G-2、验证 F1 的替代方案、给 DIST-2.3 提供体积数字（依赖：联网机器 + 一次 §4.2 流程）。
2. **补 DIST-2.16 的负测**（改 `hashes/<url>` 摘要 + `-C`）——成本极低（只改文件），直接决定"预置缓存"路线是否可上线，或是否必须改用随包 ttb。
3. **启动许可审计清单 A**（`cargo metadata` + 空值清单）——纯本地、无网络，能在拿到 bundle 清单之前先消掉最大的一块不确定性（Rust 侧许可面）。
