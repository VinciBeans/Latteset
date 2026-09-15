# Tectonic 库形态集成方案

> 文档状态：**已交付（t6 attempt 1）**。依赖 t1/t2/t3/t4/t5 全部已回数并落地：t1 → §2/§4 的现状与契约面；t2 → §3.1/§3.3/§3.4 的 API 面；t3 → §4/§5 的 I/O·确定性·bundle/缓存；t4 → §3.1.3/§6 P1/§8.3 的构建代价；t5 → §0/§6 P4–P6/§7/§8.2 的判据实测。t7（运行时对比）已回数（"库形态对击键路径无可证实改善"）⇒ §8.2 的「相对子进程对比列」如需更新按 §6.1 的 release 口径。
> **实施状态（2026-09-15）**：**P0 / P1（落盘部分）/ P2 / P3 已落地** —— 库形态能真编出中文 PDF，见 **§6.1**（含 4 个实测缺陷与修法、复核命令的完整环境前置、release 口径的性能对照表）。P4–P7 仍按各自闸门未开工。
> 证据来源命名：`t1` = `test_file/research-tectonic-lib/repo-map.md`（现状清点 §1–§7）；`t2` = `test_file/research-tectonic-lib/upstream-api.md`（上游 API 事实 §0–§9）；`t3` = `test_file/research-tectonic-lib/io-determinism.md`（I/O·确定性·bundle/缓存 §1–§12 + HL-1..HL-13）；`t5` = `test_file/research-tectonic-lib/engine-spike.md`（三项准入判据实测 §0–§6）。其余一律写 `文件:行号` 或 `文档 §`。
> **行号纪律**：凡引用上游 `src/driver.rs` 的条目，**一律用符号名**（如 `do_not_write_output_files()`、`build_date_from_env`），不写行号 —— 该文件位于**主 crate 的 `test_file/tectonic-src/src/driver.rs`**（0.17 起 `ProcessingSession` 在主 crate，不在 `crates/engine_xetex/`）；t3 报告中的部分 `driver.rs` 行号与本机 tarball 对不上，属已知复审项。本文件中带 `[已复核]` 的行号由我本轮亲自 `read` 过。
> 约束：本方案不写产品代码；本轮交付物只有本文档（t6 任务契约：「非目标：不写产品代码（只写这一份文档）」）。

## §0 摘要与结论

**一句话结论**：**值得做，但只做被实测支持的那两刀 ——「整份 XDV 在内存里同进程出 PDF」与「自定义 I/O + 可离线 bundle/缓存」都已被 t5 实测成立且产物与官方 CLI 等价；而「编译中逐页事件」按官方 API 不成立（只能走 §3.4 选定的路径 B：自持输出层 + `XdvParser`），「常驻省地板」至今没有实测判决**。同时必须把它与「让 Tectonic 可用」解耦 —— 后者被一个与库形态无关的现状缺陷卡着（E-1），要先单独处置。

**必须先处置的既有缺陷（与库形态解耦，属 P0）**：

| # | 事实 | 出处 | 影响 |
|---|---|---|---|
| E-1 | `tectonic_command` **无条件**加 `-C` —— **命令构造里只此一处**；另有 `runner.rs:891`、`:909` **两条单测断言**需同步 | `crates/latteset-infra/src/runner.rs:317`；断言 `:891`/`:909`（F-12 更正：原文档误写成"全仓只有一处"） | 全新的 Windows 机器 + 空缓存下，Tectonic 因此**拒绝联网取 bundle**；与 `SettingsPanel.vue:25` 的文案「首次编译要联网下载 bundle…」方向完全相反。**t14 改判定逻辑时，`:891`/`:909` 两条断言必须同步**，否则单测锁的是旧行为 |
| E-2 | 测试方案对 `-C` 的纪律是「`-C` 只在资源已就绪形态下加；**首跑档不加**（否则永远拿不到 bundle）」 | `docs/research/tectonic-test-plan.md:130` | 现状实现与本仓自己的测试纪律冲突 |
| E-3 | 冷缓存 + `-C` 的实测文案：`this bundle isn't cached, and we couldn't get it from the internet. …index.gz` | `docs/research/tectonic-test-plan.md:200`；库层同义文案 t5 §3.4 | A 处置：条件化 `-C` + 首次引导；B 处置：预置 bundle 兜底。**二者都不属于库形态**；已立独立任务 **t14**（§6 P0） |

**判据实测判决（t5）与推荐档位**：

| 判据（t1 §3 的编号） | t5 实测判决 | 观测依据 |
|---|---|---|
| **J3** 内存 XDV → PDF | **整份成立；前缀不成立** | t5 §2.2/§2.3：243,012 B XDV → 74,389 B PDF（26 页），与 CLI 页数/文本字符/全文文本 SHA256 前缀**全等**（字节差 −29 B）；截断到页 5 尾 / 页 6 中 / 64 B 一律 exit=1 **且 0 字节产出**（`Are you sure this is a DVI file?`）⇒ XDV 必须含 postamble |
| **J2** 编译中页事件 | **官方 API 不成立；自建输出层替代路径成立** | t5 §4.1：CLI 的 `.xdv` 在 996 ms 编译里只有 2 个采样点（35 ms=0 B → 966 ms=243,012 B）；t5 §4.2：自建输出层 + `XdvParser::parse` 逐块 ⇒ 16 KB 时已完成 4 页、243 KB 时 26 页，且转换段 PDF 字节**运行中逐段增长**（15 B→74,389 B，约 70 个采样/2.1 s） |
| **J1** 常驻后的击键 pass 成本 | **不成立（2026-09-15 实测判决，§6.4）** | 「常驻 vs 每次起进程」的对照已补（`examples/bench.rs` + `bench-tectonic-lib.mjs --mode resident`）：进程地板仅 **9.3 ms**，轻档净收益 −26 ms、**重档为负（+31%，第 3 轮起退化）** ⇒ 达不到 P4 的「< 子进程地板×0.8 且省 ≥100 ms」。**按 P4 回退点处理：不投入常驻**；改投 bib 跳过判据与 bundle/format 常驻化 |

| 档 | 内容 | 准入条件（按实测） | 推荐 |
|---|---|---|---|
| **不做** | 维持子进程形态 | — | 不推荐：放弃已经**实测成立**的 J3 与自定义 I/O 两刀 |
| **推荐档（t5 支持）** | **J3 + 判据②**：内存产物（整份 XDV → 同进程出 PDF）+ 自定义 I/O/bundle/缓存的离线隔离 | J3 整份成立（t5 §2.2；**前缀反例见 t5 §2.3**）、判据② 成立（t5 §3）；**调用路径 = §3.4 的路径 B** ⇒ 触发 **§4.7 的 ADR-0005 决策门** | **推荐起步**：这两条已实测。**引用更正（F-09）**：原稿引 `roadmap §6.5:246`「只取第 1 条（缓冲直喂）也仍然值钱」来支撑本档，属**引用错位**——那句的"第 1 条"指**判据①（常驻 pass 成本）**，与本档无关；本档的直接依据是 **t5 的实测判决** |
| **实时档（条件档）** | 上一档 + J2 的**自持输出层**逐块页事件 | t5 实测替代路径成立；路径 B **本就包含**自持输出层（§3.4）⇒ 本档不再额外引入新路径 | 需要时才做；**"官方逐页钩子"不存在，不得按它排期** |
| **常驻档** | + J1 的进程常驻省地板 | **等 t7**；「常驻」的口径按 §5.2 分路径写（A 下不可能、B 下可能但 `[推断]`） | 未判定前不投入 |

> 三条判据的原文见 `docs/research/tex-ide-roadmap-priority.md:246`（roadmap §6.5）。**分档依据 = 同段的原文判断「②③ 任一为否，库形态的实时价值就要打折 —— 但只取第 1 条也仍然值钱」**：本方案据此把**实时档列为条件档**（判据③ 按官方 API 为否），并把**推荐档收缩到 J3 + 判据②**（两者均已被 t5 实测）。**注意判据③（逐页 flush）已由 t3+t5 定案为"官方 API 不成立"** ⇒ 本方案按「自持输出层」口径重写，不保留悬空表述；分档**不再**引用"第 1 条（缓冲直喂）"那句括注（F-09 更正）。

---

## §1 目标与非目标

### 1.1 目标

| # | 目标 | 可追溯依据 |
|---|---|---|
| G1 | 把 `tectonic` 作为库嵌进**新 crate `crates/latteset-tectonic/`**（§3.2 的落点裁决），实现与 `LatexmkRunner` 同级的 `CompileRunner`；**子进程路径零回归** | t1 §4.1、`crates/latteset-core/src/scheduler/runner.rs:16-19` |
| G2 | 用**自实现 `IoProvider`（自持输出层）**接管输入/输出 —— 调用路径 = §3.4 的**路径 B**；装配后上层（src-tauri）仍不出现 `std::fs`；对 ADR-0010 的结构性偏离登记为 §4.2 的 **X-5** | t1 §4.1；t3 HL-1/HL-2；t5 §2.2 |
| G3 | bundle 与缓存**可指定、可离线**：指定本地 bundle / 缓存目录，失败文案可读 | t1 §6①、t2 §8 |
| G4 | 保持 D1（失败不自动回退）、D3（状态栏显示引擎）、D4（epoch 分档）三条既有裁决不变 | `docs/research/tectonic-test-plan.md:842-847` |
| G5 | 判定并记录 J1/J2/J3 三判据的成立/不成立，写进门禁编号（§7） | `roadmap:246`、t1 §3 |

### 1.2 非目标（本轮明确不做）

| # | 非目标 | 理由 / 出处 |
|---|---|---|
| N1 | 不替换子进程形态 | `roadmap §6.5:244`「官方 MSVC 二进制**只能**当子进程用」；子进程路径是 TL-less 与打包分发的既成出路 |
| N2 | 不改默认引擎 | `crates/latteset-core/src/settings/model.rs:40` 默认 `XeLaTeX`；t1 §1.1 T-07 |
| N3 | 不重造引擎 / 不 fork | `docs/research/tex-ide-roadmap-priority.md:263`（明确不做「自建/改造 TeX 引擎」） |
| N4 | 本轮**不**接「编辑器未保存缓冲直喂引擎」 | 与 ADR-0007「文件系统为内容真相源」直接冲突（t1 §4.6）；先只做「磁盘输入 → 内存/磁盘输出」 |
| N5 | 不给许可结论 | t2 §2.3：C 依赖许可分布 `[未定]`；§10 只写下一批工作与门禁 |
| N6 | 不新增 ADR 之外的文档改动 | 收口归 T13（t12 任务契约 inScope） |

---

## §2 形态 B（已落）vs 形态 A（库内嵌）信息增量对照表

### 2.1 形态 B 已兑现（逐条可追溯，来自 t1 §2.1）

| # | 已兑现能力 | 落点 |
|---|---|---|
| B1 | 第二个引擎进产品（设置 + UI + headless 报告） | t1 T-01 / F-01 / F-02 / S-01 |
| B2 | 免装 TeX Live 的**方向**（自带 bundle、不读用户 TL） | t1 R-04；**但见 §0 的 E-1：冷缓存下产品跑不通** |
| B3 | Quick 单趟出 PDF | t1 R-01/R-04 |
| B4 | 首趟成本低于现状两段（28 页 0.88–1.07 s vs xelatex ≈1.96 s） | `docs/research/realtime-preview-cost.md:39,52,73` |
| B5 | 收尾/产物路径与其它引擎一致 | t1 R-11 |
| B6 | 报错文案指对方向（不再说「TeX Live 未安装」） | t1 R-07 |
| B7 | D4：Tectonic 进程不设 epoch（`\today` 正确） | t1 R-03 + 单测 U-01 |
| B8 | 输出目录由产品自建 | t1 R-05 |

### 2.2 形态 A 独有能力（本方案要买的）与形态 B 的现状对照

| # | 形态 A 独有（`roadmap §6.5:238-242`） | 形态 B 现状 | 库形态 API 依据（t2） | 本方案取舍 |
|---|---|---|---|---|
| A1 | `IoProvider` 接管 IO ⇒ 编辑器未保存缓冲直喂引擎 | 只能读磁盘；编译触发靠 notify + 自动保存（ADR-0007） | `IoProvider` trait：`crates/io_base/src/lib.rs:438-533`（t2 §4.1） | **本轮不接**（N4，与 ADR-0007 冲突）；**且只对主文件成立**——高层 API 下非主文件（`\input` 章/宏包）**无法**内存直喂（HL-1，t3 §2.4 `[本地源码]`） |
| A2 | 常驻 format 与进程 ⇒ 省掉每次运行地板的一部分 | 每次起进程；地板 283.2 ms（英文空体、含 PDF） | `FormatCache::new(bundle_digest, formats_base)`（t2 §3.2） | **候选主卖点（J1），但不在 §0 的推荐档内**：t5 **未测**「常驻 vs 起进程」对照 ⇒ 判决待 t7（F-14 更正）。且「常驻」须重新定义：bundle/format **不能常驻内存**跨编译（HL-10，t3 §5.2）——可省的是「磁盘 `.fmt` 命中 + 每次进程启动与引擎初始化」 |
| A3 | `XdvEvents` 页事件 + 逐字形绝对坐标 | 只有 `-p` 的 `[N]` 页计数 | `handle_begin_page(&mut self, counters: &[i32], previous_bop: i32)`（t2 §6.1，调用点 `crates/xdv/src/lib.rs:716`） | **t5 判决：官方 API 不成立 / 自建输出层实测成立**——CLI 的 `.xdv` 在 996 ms 编译里只有 2 个采样点（35 ms=0 B → 966 ms=243,012 B）；自建输出层 + `XdvParser::parse`（`xdv/src/lib.rs:429`）逐块 ⇒ **16 KB 时已完成 4 页、243 KB 时 26 页**，并产出精确页表（t5 §4.2）。粒度是 **≤16 KB 的字节块**，不是页；`DviState` 在 0.17 不存在、`Cursor`/`checkpoint()` 私有（t2 §6.3） |
| A4 | 三个 pass 自己编排；产物全程在内存 | 引擎内部编排；产物落盘才能读 | `TexEngine::process(launcher, format_file_name, input_file_name)`（t2 §3.1）；`XdvipdfmxEngine::process(launcher, dvi: &str, pdf: &str)`（同） | **本方案选定 = 自持输出层的低层引擎路径（= §3.4 的路径 B）**：pass 编排 / format 生成 / 产物落盘都由我们实现 ⇒ 触发 **ADR-0005 决策门**（§4.7）。**全文不再采用**「把自实现 `IoProvider` 注入 `ProcessingSession`」这种写法（该入口不存在：`driver.rs:829-1112`，t3 HL-1） |
| A5 | 结构化状态与错误（`StatusBackend`/`Result`） | 仍解析 `tmp/<stem>.log` | `StatusBackend` trait（t2 §5.2）；**`ErrorKind` 已不存在**，只剩 `anyhow::Error` + `EngineError(engine)` 标记（t2 §5.1） | 取「`StatusBackend` 捕获 + 内存 `.log`」；**不承诺**机器可判别分类 |

### 2.3 子进程 vs 库：逐项信息增量（t2 §6.4 压缩）

| 信息 | 子进程 `tectonic.exe` | 库形态 |
|---|---|---|
| PDF 字节 | 只能读磁盘 | 内存层 `files["main.pdf"].data` |
| `.log` 全文 | 磁盘 `tmp/main.log`（依赖 `--keep-logs`） | 内存层一定有，**失败路径也有** |
| SyncTeX | 磁盘 `main.synctex.gz` | 内存层 `files["main.synctex.gz"].data`（**已是解压文本**，见 t2 §4.1） |
| XDV | `--outfmt xdv` 落盘 | 内存层（需 `OutputFormat::Xdv`；默认 PDF 档会 `remove(xdv)`） |
| 引擎崩溃现场 | `--print` 走终端 | `get_stdout_content()` |
| 页级 `counters[]` / 逐字形坐标 | ✗ | `XdvEvents` 回调 |
| 每页字节偏移 | ✗ | **可得到，但粒度是 8 KiB 而不是页**：`dvi_swap` 每次写 `HALF_BUF = 8192` B（`xetex-shipout.c:12,2395-2413`，`DVI_BUF_SIZE = 16384`）；页边界靠 `XdvParser` 解析恢复 |

---

## §3 集成骨架

### 3.1 依赖钉定（按**两种引入形态**组织）

> **能否单独引入**：t2 实测「全部 `crates/*` 均未标 `publish = false`」⇒ 结构上都可单独引入（t2 §1.2）；但 **crates.io 发布态只逐个核了 3 个** crate（`tectonic` / `tectonic_xdv` / `tectonic_engine_xetex`，t2 §9.1 U1），其余子 crate 未逐个 API 核 ⇒ **不得写成「全部子 crate 已实测可单独引入」**（F-15 更正）。**t4 已交付**（`test_file/research-tectonic-lib/build-spike.md`），它给出的关键事实是：**两种形态的 C 链完全相同**、且本会话**构建从未开始**（代价是 `[未测到]`，不是估值）。

#### 形态 I｜只引主 crate `tectonic`（`ProcessingSession` 全流程）——**本方案不选**

> 本方案 §3.4 选定的是**路径 B（引擎 crate + 自持输出层）**，因此**不用**主 crate 的 `ProcessingSession`；本节保留其事实，作为"为什么不用"的对照（F-01 决定 1）。

| 项 | 值 / 事实 | 出处 |
|---|---|---|
| 主 crate | **`tectonic = "0.17.0"`**（crates.io max_stable；无需 git pin） | t2 §1.1 |
| 主 crate 自身 | 0.17.0 的 crates.io `linecounts` = **C: 0 files / Rust: 4764 行**（C/C++ 源码已搬进引擎 crate） | t2 §1.4 |
| **必然闭包** | 主 crate 内部依赖引擎 crate ⇒ **引它 = 引全部 C/C++ 链**（freetype2 / graphite2 / harfbuzz / icu / flate）。**不存在「只引主 crate 就躲开 C」这条路** | t2 §1.2、§1.4 |
| 默认 feature | `default = ["geturl-reqwest", "serialization"]`（网络栈默认开） | t2 §8 |
| 构建代价 | **`[未测到]`**：构建从未开始（三重独立阻塞）⇒ 无冷/热墙钟、无体积增量；**不填估值**，见 §3.1.3 | t4 §5.2 |
| 适用 | **本方案不选**：它比引擎 crate 形态多 **163 个 CLI/watcher/bundle 下载栈包**（461 vs 278，t4 §3.1） | t4 §3.1 |

#### 形态 II｜只引叶子 crate（不引引擎，纯 Rust）

| crate | 版本 | 单独引入可行性 | 关键前置 |
|---|---|---|---|
| `tectonic_xdv` | `0.3.0` | **可以，且最轻** | 唯一依赖 `byteorder ^1.4`；纯解析器，无 C、无网络 |
| `tectonic_status_base` | `0.2.2` | **可以，且最轻** | 只依赖 `tectonic_errors`（`anyhow` 再导出） |
| `tectonic_io_base` | `0.6.0` | **可以**（纯 Rust） | `directories` / `flate2` / `libc` / `sha2` / `thiserror` |
| `tectonic_bundles` | `0.4.2` | 可以，但**默认拖进 HTTP 栈** | 纯本地须 `default-features = false` + 只开 `geturl-curl` 或不装 |
| `tectonic_engine_bibtex` | `0.3.4` | 可以（纯 Rust 重写版） | 但 `process()` 要 `CoreBridgeLauncher` ⇒ 实际还要 `tectonic_bridge_core` |

- **能力边界**：形态 II 只够做「内存 XDV 解析（`tectonic_xdv`）+ `IoProvider` trait 定义（`tectonic_io_base`）+ status DTO（`tectonic_status_base`）」，**不能排版**。
- **一旦要跑 TeX/PDF 引擎**，就必须引 `tectonic_engine_xetex 0.5.3` / `tectonic_engine_xdvipdfmx 0.4.4` —— **这正是本方案的选择**（§3.4 路径 B）：crates/latteset-tectonic 的依赖组合 = `tectonic_engine_xetex` + `tectonic_engine_xdvipdfmx` + `tectonic_xdv` + `tectonic_bridge_core` + `tectonic_io_base` + `tectonic_bundles` + `tectonic_status_base`（即 t4 §3.1 的 **278 包**那一档）⇒ **C 链无法避免**（t2 §1.2：「**没有『只要 XeTeX 引擎的小依赖』这条捷径**」；t4 §3.1：两形态 native 标记均为 21 个）。
- **构建代价**：`[未测到]`；**且与形态 I 的 C 链完全相同**（两者各 21 个 native 标记）⇒ 引引擎 crate **省不掉原生依赖**，只省 183 个 CLI/watcher/bundle 下载栈包（t4 §3.1）。

#### 两形态共同纪律

| 项 | 内容 | 出处 |
|---|---|---|
| 版本钉定 | 上游子 crate 用 `>=x,<1` 开放区间 ⇒ **必须提交 `Cargo.lock`** 才能复现今天的 0.17.0 组合 | t2 §1.1 `[推断]` |
| 形态 I 的 feature 收敛 | 若要「纯本地」，需显式 `default-features = false` 并只留必要 feature（默认档带 reqwest） | t2 §8 |
| 官方产物兜底 | 不自己构建也能拿到可运行的 0.17.0（zip / exe，见 §8.3） | t2 §7.2 |

`[未定]`：全部子 crate 的 crates.io 发布态只逐条核了 3 个（t2 §9.1 U1）；MSVC + `crt-static` 组合下 `links` 冲突是否成立未验（t2 U12）。**因此「已实测可构建」在本方案里不得写**（t4 也确认本会话无任何成功样本）。

#### 3.1.3 构建代价（t4）与「不可构建」的准确边界

**一句话**：本会话**构建从未开始**（无成功样本，全部 exit 101/1）⇒ 冷/热墙钟、峰值内存、体积增量一律记 **`[未测到] + 逐条原因`**，**不得填估值**（t4 §0/§5.2）。

| 项 | 事实 | 出处 |
|---|---|---|
| 三条独立阻塞 | ① crates.io 不可达（Schannel `SEC_E_NO_CREDENTIALS`）；② `pkg-config`/`cmake`/`vcpkg` 均缺失且全盘未见；③ 本地源码树 `bridge_harfbuzz/harfbuzz/` 为空 | t4 §1 |
| **阻塞的性质（队长校正，按此写）** | ②③ 是**有界的外部前置**：装 vcpkg（含 cmake / pkg-config / nasm）+ 放开网络即可补测 —— 属**用户可执行的外部动作，不是「做不到」**；补测入口 = `test_file/research-tectonic-lib/build-spike/run-samples.ps1`（15 样本矩阵） | 队长校正 2；t4 §11 |
| 两形态依赖闭合 | A（只引主 crate）= **461 包**（25 `tectonic_*` + 436 第三方）；B（引擎 crate）= **278 包**（21 + 257）；**native/C 链标记两者都是 21 个、完全相同** | t4 §3.1 |
| **口径警示（队长校正）** | 上述包数是**按上游 `Cargo.lock` 离线计算的闭合上界，不是实测 resolve**；真实解析在「除 tectonic 家族与 harfbuzz 源码外」的部分**可离线完成**（工作区 vendor 目录 **437 个目录 / 547.3 MB**，含第三方包、不含 tectonic 系列）；实测 resolve 需联网复跑 `cargo tree` | 队长校正 1；t4 §3.1/§10 |
| 与本仓 lock 的冲突面 | SAME 161 / UNIFY 62 / **DUP 19** / 新增 169；`notify 8.2.0` **SAME**，`tauri 2`（无交集）/`tokio 1`/`thiserror 2`/`serde 1` 均 UNIFY 或 SAME；合并后 lock 规模上界 ≈ **728 包** | t4 §3.2 |
| Windows MSVC 唯一官方路线 | `vcpkg + x64-windows-static-release + RUSTFLAGS=-Ctarget-feature=+crt-static`；**graphite2 / ICU / freetype2 / fontconfig / libpng 永远走外部探测**（pkg-config 或 vcpkg），**探测失败即 build script panic** | t4 §4 |
| harfbuzz 的坑（**t2 措辞更正**） | `harfbuzz/` 空目录导致 `exit(1)` **只对「git 检出但未 `git submodule update --init`」的源码树成立**；crates.io 发布的 `tectonic_bridge_harfbuzz` 包**内含** vendored harfbuzz 源码 ⇒ **走发布版不踩这个坑**（本方案走发布版） | t4 §3.3；t2 §7.1 的适用范围按此收窄 |
| CI 参照（**跨机器，只作参照，不作比较**） | 上游同构 job `vcpkg (x86_64-pc-windows-msvc)` **19m05s**（装依赖 2m24s 缓存命中 + Build&Test 14m51s）；稳态增量粗估 **+15–20 min/冷、+2–5 min/热** `[推断]` | t4 §7 |
| 体积参照（**随包形态，不是库形态**） | 官方 Windows/MSVC 0.17.0 静态单文件 **51,538,432 B（49.1 MB，单文件、无 DLL）** = 「随包 exe」形态的锚点；**库形态的体积增量未测到**（"数十 MB"仅为 `[推断]`） | t4 §5.3 |
| feature 门控的硬限制 | 可选依赖 + feature 接线**语法可行**（`cargo metadata --offline --no-deps` exit 0），**但关闭 feature 仍要解析整棵（含可选）依赖图**（`--offline` 照样 101）⇒「不启用就零成本」**只在 cargo registry 缓存预热时成立** | t4 §9（样本 A3/E3） |
| t4 建议的折中（**已被本方案采纳并按决定 2 具体化**） | 把库形态封成**独立 workspace 成员**（`crates/latteset-tectonic`，见 §3.2），`src-tauri` 以 optional 依赖 + feature 挂载；**配套根 `Cargo.toml` 加 `default-members`** ⇒ 主产物保持零原生依赖 | t4 §9；队长决定 2/3；本方案 §3.2 |
| 基线「测量地板」（**不可外推**） | 零依赖 crate：debug 冷 817 / 热 818 ms、release 冷 793 / 热 682 ms，峰值 RSS ≈ 190 MB（含 217 ms 包装开销） | t4 §5.1 |
| 缓存命中率的口径 | t4 报「本机 cargo registry cache 对上游 lock 命中 134/437 = 30.7%」，与队长实测 `cache` 目录 403 个 `.crate` 对不上（**口径可能不同**）⇒ 本方案**引用 t4 报告并标口径，不自行重算** | t4 §1；队长校正 3 |

### 3.2 模块划分（改动面清单）——**代码落点在此唯一裁决（F-02 闭合）**

> **落点裁决（择一，本文档全文以此为准）**：候选 ①「独立 workspace 成员 `crates/latteset-tectonic/`，`src-tauri` 依赖它、`latteset-infra` 不依赖」；②「独立成员且主 workspace 完全不依赖」；③「直接进 `crates/latteset-infra/`」。
> **选 ①**。理由：③ 会让**整仓默认构建**永远需要 vcpkg/原生链（t4 §9 实测：关闭 feature 仍要解析整棵依赖图）；② 则 `src-tauri` **没有任何装配通道**（本文档不引入运行时插件机制），要求写清的"如何装配"无解。
>
> **配套硬前置（队长决定 3 / N-04；不做这步选 ① 就不自洽）**：根 `Cargo.toml:3` 目前**只有 `members`、没有 `default-members`** ⇒ workspace 成员会被根 `cargo build`（不带 `-p`）**默认构建**，optional 依赖 + feature **挡不住**（feature 只影响依赖解析）。因此必须同时：**根 `Cargo.toml` 增加 `default-members`**（列出 `src-tauri`、`crates/latteset-core`、`crates/latteset-infra`、`crates/latteset-server`），把 `crates/latteset-tectonic` **排除在默认构建之外**；该改动由 **t9 落地**（`Cargo.toml` 在 t9 的 inScope 内）。

| 层 | 文件（拟） | 内容 | 不可越界 |
|---|---|---|---|
| **新 crate（库形态的唯一代码落点）** | `crates/latteset-tectonic/`（`Cargo.toml` + `src/lib.rs`、`src/io.rs`、`src/runner.rs`、`src/bundle.rs`、`src/status.rs`） | 自持输出层的 `IoProvider`、`CoreBridgeLauncher` 装配、三引擎编排、`CompileRunner` 实现 `TectonicLibRunner`、bundle/缓存解析 | ① 它是**外部依赖 / C 链的第二个落点** ⇒ 对 ADR-0010「infra 是唯一落点」的**结构性偏离**，登记为 §4.2 的 **X-5**（F-03）；② **`latteset-infra` 不依赖它**（否则 ③ 的代价原样回来）；③ 它是主 workspace 的 **member** |
| `latteset-core` | `crates/latteset-core/src/types.rs` | 形态位 = **可选附加字段**（不是新增 `Engine` 变体） | t1 §4.6：改变体集合会让旧版读新设置**整份重置**（`crates/latteset-infra/src/storage.rs:41-66`；`Engine` 无 `serde(other)`，`types.rs:16-31`） |
| `latteset-core` | `crates/latteset-core/src/scheduler/runner.rs` | `CompileRunner` 接口**不变** | 契约「cancel 尽快终止并返回 `Aborted`」`runner.rs:12-14` |
| `src-tauri` | `src-tauri/Cargo.toml` | `latteset-tectonic = { path = "crates/latteset-tectonic", optional = true }` + `[features] tectonic-lib = ["dep:latteset-tectonic"]`；**默认关** | 主产物保持零原生依赖（t4 §9） |
| `src-tauri` | `src-tauri/src/lib.rs:83-84` | 装配点：feature 开 → `TectonicLibRunner`，否则 → `LatexmkRunner`（同一行 trait 注入） | t1 A-01：该层不出现 `Engine`，只有 trait 注入 |
| 前端 | `src/components/StatusBar.vue` / `SettingsPanel.vue` | 形态可见性（D3 的衍生义务：两形态并存时必须能区分） | t1 §4.4 |

### 3.3 类型与 trait 草图（**仅签名草图，不是产品代码**）

```rust
// latteset-core::types —— 形态位（可选附加字段，默认缺省 = 子进程）
pub enum EngineForm { Subprocess, Embedded }          // serde 默认 Subprocess

// crates/latteset-tectonic/src/io.rs —— 自持输出层：IoProvider → core::project::FileSystem
// 注入目标 = **引擎的 CoreBridgeLauncher**（不是 ProcessingSession —— 那里没有该入口）
struct TectonicIo<'a> { fs: &'a dyn latteset_core::project::FileSystem, root: PathBuf,
                        xdv_probe: Option<Arc<dyn Fn(&[u8]) + Send + Sync>> } // 页事件探针
impl tectonic_io_base::IoProvider for TectonicIo<'_> {
    // 0.17 只有这些入口（t2 §4.1）；不存在 create_output/output_file_names/output_open
    fn output_open_name(&mut self, name: &str) -> OpenResult<OutputHandle> { … }   // ← 逐块可见（t5 §4.2）
    fn output_open_stdout(&mut self) -> OpenResult<OutputHandle> { … }             // ← 引擎 chatter/错误现场
    fn input_open_name(&mut self, name: &str, status: &mut dyn StatusBackend) -> OpenResult<InputHandle> { … }
    fn input_open_name_with_abspath(&mut self, name: &str, status: &mut dyn StatusBackend)
        -> OpenResult<(InputHandle, Option<PathBuf>)> { … }
    fn input_open_format(&mut self, name: &str, status: &mut dyn StatusBackend) -> OpenResult<InputHandle> { … } // format 自管
    fn write_format(&mut self, name: &str, data: &[u8], status: &mut dyn StatusBackend) -> Result<()> { … }     // format 自管
}

// crates/latteset-tectonic/src/status.rs —— StatusBackend 捕获（Arguments 不可存储，必须立即 to_string）
struct ProgressStatus { sink: Arc<dyn CompileProgress> }
impl StatusBackend for ProgressStatus { fn report(&mut self, kind: MessageKind, args: Arguments, err: Option<&Error>) { … } }

// crates/latteset-tectonic/src/runner.rs —— 与 LatexmkRunner 实现同一个 core trait
struct TectonicLibRunner { … }
impl latteset_core::scheduler::CompileRunner for TectonicLibRunner {
    async fn compile(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome { … }
}
```

> **为什么探针在 `IoProvider` 上**：t5 实测的"逐块可见"来源就是**我们返回的写句柄**（`engine-spike/src/lib.rs:103` 的 `SpikeIo`；`bridge_core/src/lib.rs:570-582` 的 `output_write` → `Write::write_all`），**不是** `ProcessingSession` 的任何回调。

### 3.4 编译调用序列 —— **选定路径 B：引擎 crate + 自持输出层**（F-01 / 队长决定 1）

**为什么是 B（三条，全部可核）**：
1. 推荐档的两刀都是 **B 实测**的：J3 整份 XDV→内存 PDF（t5 §2.2 的 A1/A5/A6/A8 用 `XdvipdfmxEngine::process` + 自持 `SpikeIo`）、判据② 的内存输入/输出等价（EQ1/EQ2）与 bundle/缓存隔离（B1..B10）。
2. **路径 A 里没有 IoProvider 入口**：`ProcessingSessionBuilder` 只有 `filesystem_root` / `output_dir` / `do_not_write_output_files` / `format_cache_path`（`test_file/tectonic-src/src/driver.rs:829-1112`；t3 HL-1）⇒「把自实现 IoProvider 注入 session」在 0.17 **不成立**，全文已无此说法。
3. 路径 A 的能力上限 = 内存输入（**仅主文件**）+ 内存产物 + `StatusBackend`，**没有**逐页事件（t3 HL-2；t5 判据③）。

**路径 B 的唯一开工序列**：

| 步 | 动作 | 依据 / 证据等级 |
|---|---|---|
| 1 | 建 `CoreBridgeLauncher`（`new_with_security`）+ 自持 `TectonicIo`（§3.3） | t2 §3.1；`engine-spike/src/lib.rs:103` `[实测同构]` |
| 2 | bundle：`tectonic_bundles::detect_bundle(source, only_cached, Some(产品缓存目录))` | t2 §8；t5 §3.2（B1–B10） |
| 3 | **format 自管**：`input_open_format` 命中即复用，否则 `TexEngine::initex_mode(true)` 生成后 `write_format` 落产品缓存 | t3 §2.1 第 7/8 行、§5.1 |
| 4 | 排版趟：`TexEngine::process(&mut launcher, "latex", "<stem>.tex")`，**必须先** `build_date(SystemTime::now())`（§4.3 D-2） | t2 §3.1；t3 §8.1bis |
| 5 | 中间产物（`.aux`/`.bbl`/`.toc`）由**我们自己的 I/O 层**持有；需要落盘时只落我们显式指定的目录 | 本方案 §5.4 |
| 6 | bib 趟（需要时）：`BibtexEngine::process(&mut launcher, "<stem>.aux")` | t2 §3.1 |
| 7 | 转换趟：`XdvipdfmxEngine::process(&mut launcher, "<stem>.xdv", "<stem>.pdf")`；XDV/PDF 以**我们 I/O 层的名字**可达 | t2 §3.1；**t5 §2.2 实测整份可行** |
| 8 | 页事件（条件档）：把 `output_open_name` 的写句柄 tee 给 `XdvParser::parse(chunk)` | t5 §4.2（粒度 ≤16 KB） |
| 9 | 收尾：PDF 原子拷贝到项目根（沿用 `runner.rs:564-585` 形态）；`.log` 从我们 I/O 层取（与磁盘层逐字节一致，t3 §3.3） | t1 R-11；t3 §3.3 |

**代价（必须一起读；它是 P5 与 §4.7 决策门的输入）**：路径 B = **自实现** format 生成、多趟收敛判定、产物落盘、SyncTeX 路径、bib 编排 —— 即 `ProcessingSession` 内部 `driver.rs` 的那套逻辑（t3 §4.4(a)）⇒ 触发 **ADR-0005 决策门（§4.7）**。

**路径 A（已评估、未选；保留记录不删）**：`ProcessingSessionBuilder` → 必填三项 `bundle` / `format_name` / `tex_input_name`（t2 §3.2）→ `primary_input_path|primary_input_buffer` → `do_not_write_output_files()` **或** `output_dir(tmp)`（两者不互斥，t2 §4.4）→ `create(&mut status)` / `run(&mut status)` → `into_file_data()`（t2 §4.2）。**未选理由** = 上条 1–3；其全管线（TeX 源码内存输入 → 内存输出）仍是 **t5 U1 未实测**。若将来改选 A，`ProcessingSession` **不是 `Send`**（t2 §4.5 `[推断]`）这条约束仍适用。

**线程模型（B 同样适用）**：引擎有**进程内全局互斥**（`ENGINE_LOCK`，t2 §7.4 第 5 条）⇒ 编译在 `spawn_blocking` 里自建自跑，同一进程内不并发跑两个引擎。

### 3.5 设置面与回退开关

| 项 | 取值 | 理由 |
|---|---|---|
| 默认 | **子进程形态**（形态位缺省） | N1；`model.rs:40` 默认引擎本就不是 Tectonic |
| 切换粒度 | 全局设置 + 项目覆盖（沿用现有三层设置） | t1 §1.1 T-07 / `modules.md §2.5` |
| 回退开关 | 「子进程形态」永远可选；库形态失败**不自动回退**（D1） | `tectonic-test-plan.md:842` |
| 未知值 | 形态位必须**可缺省**（`#[serde(default)]`） | 否则旧版读新设置整份重置（t1 §4.6） |

---

## §4 I/O 与 ADR-0010：裁决与必须的例外

### 4.1 遵守方式（无例外部分）

| 约束（ADR-0010 原文要点） | 库形态落地 | 出处 |
|---|---|---|
| infra 是文件系统与进程的唯一落点；上层不出现 `std::fs` | **本方案把它扩到第二个落点**：`IoProvider` 适配器落在新 crate `crates/latteset-tectonic/`，把 `output_open_name` / `output_open_stdout` / `input_open_name` / `input_open_name_with_abspath` / `input_open_format` / `write_format` 代理到 core 的 `FileSystem`；**该扩展是对 ADR-0010 的结构性偏离，登记为 §4.2 的 X-5** | t1 §4.1；t2 §4.1；§4.2 X-5 |
| core 不依赖 infra | `TectonicLibRunner`（`CompileRunner` 实现）放 `crates/latteset-tectonic/`，与 `LatexmkRunner` **同级但不同 crate**；`latteset-infra` **不依赖**它（§3.2 落点裁决） | `crates/latteset-core/src/scheduler/runner.rs:16-19` |
| 装配点单一 | 仍是 `src-tauri/src/lib.rs:83-84` 那一行 | t1 A-01 |

### 4.2 必须记录的结构性例外

| # | 例外 | 证据 | 处置口径（三选一） |
|---|---|---|---|
| X-1 | **C 库自己读文件**：fontconfig/freetype2/harfbuzz/graphite2/ICU 的字体与 config 读取**无法经我们的 `FileSystem`** | t1 §4.1；`Cargo.toml:156-173`（t2 §7.2） | **(b) 受控例外并记 ADR**：ADR 里写明「哪一类 IO 不受 ADR-0010 管」 |
| X-2 | **crate 自带网络栈**：`tectonic_bundles` 默认 `geturl-reqwest`，`ItarBundle` 走 HTTP Range | t2 §8；t1 §4.1 | **(a)** `default-features = false` 关掉 HTTP；或 **(b)** 显式登记「bundle 获取是 crate 内部行为」 |
| X-3 | **C 代码的 abort/panic 无法被树杀**：`kill_tree`（`runner.rs:770-793`）在库形态下没有对应物 | t1 §4.3 | 取消语义重设计：取消只能做到「下一个 IoProvider 回调返回错误」⇒ **取消延迟必须量化并写进 UI 语义**（D1「失败可见」） |
| X-4 | ~~**环境变量是进程级**：无法「给 Tectonic 不设、给转换步设 0」~~ | — | **已被 t3 证伪**：两步都有非 env 注入点 ⇒ 该「三选一」撤回，改按 §4.3 的两条纪律 |
| **X-5** | **新增 workspace 成员 crate 承载 C 链**：`crates/latteset-tectonic/` 是**外部依赖 / 原生链的第二个落点**，偏离 ADR-0010「infra 是唯一落点」（F-03） | 队长决定 2 与 §3.2 落点裁决；t4 §3.1（21 个 native 标记）、§4 | **登记 + 新立 ADR**（§4.4 的 ADR-A）：写明「本 crate 承载哪类外部依赖、为什么不能放进 infra（放进 infra 会让**整仓默认构建**需要 vcpkg，t4 §9）、以及用 `default-members` 隔离的方式」 |

> ADR 动作：`roadmap §6.5:250` 已预留「必要时另立 ADR」；新增 ADR 编号 = `docs/adr/` 现有最大 +1（T13 任务契约）。候选面见 §4.4。

### 4.3 确定性纪律（t3 定案：**不开 ADR**）

> t1 §4.5 提出的「放弃 `\today` 正确性 / 放弃转换步确定性 / 保持子进程」这条**三选一已被 t3 证伪** —— 库形态下两步**都有**非 env 的显式注入点（t3 §8.1bis；我本轮已 `read` 复核 `test_file/tectonic-src/crates/engine_xetex/src/lib.rs:154-160` 的 `build_date(SystemTime)`，其文档注释自带「默认是 Unix epoch，应当总是覆盖」）。⇒ **不再有对应 ADR 候选**，改为两条必须钉住的纪律：

| # | 纪律 | 不遵守的后果 | 依据 |
|---|---|---|---|
| D-1 | 库内**禁止**调用 `build_date_from_env` | 会把进程环境里的 `SOURCE_DATE_EPOCH` 吃进来 ⇒ `\today` 冻结（违反 D4 (b)） | t3 §8.1（引用 **`src/driver.rs` 的符号名 `build_date_from_env`**，不用行号 —— 见 §13 的行号说明） |
| D-2 | 库内**必须**显式 `build_date(SystemTime::now())` | 默认值是 `UNIX_EPOCH` ⇒ 忘写一行就把「`\today` 印 1970」变成**默认行为**（HL-5） | t3 §8.2；`crates/engine_xetex/src/lib.rs:154-160` `[已复核]` |
| D-3 | 一切 env **只经 `Command::env` 给子进程**，库内**不** `std::env::set_var` | 进程级全局会污染同进程其它步骤 | 现状 `crates/latteset-infra/src/runner.rs:299-301` 已如此；t3 §8.2 |
| D-4 | 「PDF 逐字节相等」**仍不得**作为 Tectonic 档断言 | 元数据/加密层不可控 | `docs/research/tectonic-test-plan.md:187,845` |

**正向结论**：`\today` 正确性与转换步确定性**可以同时保留**（TeX 步给 `build_date(now)`、转换步给固定值）⇒ **不需要为它开 ADR**。

### 4.4 ADR 候选（**两份**：一份改判为"已定 + 新立"，一份仍是未做决策）

| 候选 | 内容 | 状态 |
|---|---|---|
| ~~t1 §4.5 的三选一~~ | 已由 t3 证伪（见 §4.3） | **撤回**（保留证伪记录，不删历史判断） |
| **ADR-A：库形态的 IO / 依赖边界**（新增，F-03 的结论） | 三件事写进同一份：① 新 crate `crates/latteset-tectonic/` 作为外部依赖与 C 链的**第二落点**（X-5）对 ADR-0010 的结构性偏离；② §4.2 的受控 I/O 例外（X-1 / X-2 / X-3 / X-5）**穷举登记**；③ §4.7 的 **ADR-0005 决策门**在"采用路径 B"时的批准记录 | **新立**：编号 = `docs/adr/` 现有最大 +1（当前 = **0012**）；文件由 T12 落，本方案只登记决策 |
| **ADR-B：HL-4 —— PDF 加密路径不可注入** | C 层 `getenv("SOURCE_DATE_EPOCH")`（`crates/pdf_io/pdf_io/dpx-dpxutil.c:177`，本机已 `read` 复核），唯一调用者 `dpx-pdfencrypt.c:94` | **未做决策（不是结论）**：① 不支持并报错/警告 ② 该路径回子进程 ③ 接受非确定性并登记；**并入 ADR-A 作一小节** |

> **结论（F-03 要求）**：**新立一份 ADR（编号 0012）**，即上表 ADR-A（HL-4 作为其未决小节并入）。理由：X-5 是**结构性偏离**，不登记＝默认放弃 ADR-0010 的"唯一落点"纪律；而 §4.3 的确定性"三选一"已被 t3 证伪 ⇒ **它不需要 ADR**（保留撤回记录即可）。
>
> **ADR-0005 的处置（F-04 要求，结论落在这里与 §4.7 两处）**：**不修改 ADR-0005 本身**，而是新增 §4.7 的**决策门**并在 ADR-A 里登记批准记录——理由是 ADR-0005 自己就写明「性能瓶颈出现时以预算为标尺评估是否自研驱动」，本方案正是**触发该保留条款**，属"按 ADR-0005 的既有入口走"，不是推翻它。

### 4.5 输入路径安全：上游**设计立场**，不是我们引入的漏洞

| 项 | 事实 | 出处 |
|---|---|---|
| 无包含性检查 | `FilesystemIo` 的文档注释原文：*"no effort is made to contain I/O within the specified root!! We have an option to disallow absolute paths, but we don't do anything about `../../../....` paths."* | `crates/io_base/src/filesystem.rs:78-80` `[已复核，逐字一致]` |
| 构造口径 | 项目文件系统 provider 以 `absolute_allowed = true` 构造 | t3 §2.5 |
| 处置口径（**如实登记项，不是阻塞项**） | 子进程形态下用户本就拥有其可读文件 ⇒ **信任模型等价**，库形态**不改这一点**；可做的只有 `hide()` 弱缓解 + 文档告知；真正收口要**路径 B 的自持输出层**（§3.4）而不是改配置 | t3 §2.5、HL-12 |

### 4.6 I/O 例外的登记口径

`t3 §4.4` 给三条口径；本方案选 **(b) 受控例外并记 ADR**。**在路径 B 下受控面更强**：输入/输出全部经**我们自己的 `IoProvider`**（§3.4 第 1 步），受控面 = format 缓存目录 / bundle 缓存目录 / 我们显式选择的落盘目录 / OS 临时目录（仅 biber 趟）/ 进程 CWD（**绝不动**）。

**结论（F-03 要求，不推给 T12）**：**新立一份 ADR（编号 0012，题目 = 库形态的 IO / 依赖边界）** —— 即 §4.4 的 ADR-A，把① X-5 的新 crate 落点偏离、② 上述受控例外面、③ §4.7 的 ADR-0005 决策门批准记录写在同一份里；HL-4 作为其"未做决策"小节并入。**理由**：X-5 是**结构性偏离**（ADR-0010 的"唯一落点"纪律被打破），不落 ADR 就等于默认放弃该纪律；而 §4.3 的确定性"三选一"**已被 t3 证伪 ⇒ 不需要 ADR**（保留撤回记录）。**T12 只负责把本结论写成文件，不负责再决策。**

### 4.7 ADR-0005 决策门：是否采用「自驱引擎 / 自持输出层」（F-04 / N-01 / N-03）

| 项 | 内容 |
|---|---|
| 事实 | ADR-0005 的结论是「v1 用 latexmk，**不重造引擎驱动**」，同时明确保留「性能瓶颈出现时以预算为标尺评估是否自研驱动」（`docs/adr/0005-latexmk-first-incremental-next.md:3,5,7`） |
| 本方案的触发点 | §3.4 选定**路径 B** ⇒ 要**自实现** format 生成 / 多趟收敛判定 / 产物落盘 / SyncTeX 路径 / bib 编排（t3 §4.4(a)）—— 正是 ADR-0005 保留的那条评估路径 |
| **决策门（谁 / 何时 / 批什么）** | **批准人 = 产品负责人（人类；由队长转达）**；**触发条件 = 本方案进入 t9 且采用路径 B**；**批准内容 = 明确接受"自研驱动（Tectonic 版）"这一取向偏离及其维护面** |
| 不批准时的降级 | 退回**路径 A（`ProcessingSession`）**：能力上限 = 内存输入（**仅主文件**）+ 内存产物 + `StatusBackend`，**放弃**逐页事件（t3 HL-2）与 IoProvider 级 I/O 接管 ⇒ 推荐档缩为「J3 转换段 + bundle/缓存隔离」 |
| 登记位置 | **并入 §4.4 的 ADR-A**（同一条偏离记录里写清方向与批准） |
| 与 §11 Z-2 的关系 | §11 Z-2 已同步改写：「自己编排三 pass」**不再是"不做项"**，而是"**已选定路径所需 + 走本决策门**"（N-03 修正） |

---

## §5 bundle / 缓存 / 确定性与分发

> **口径声明**：本节以 t3（`test_file/research-tectonic-lib/io-determinism.md`，542 行）为准，**t3 已交付并验收**；标 `[已复核]` 的源码行由我在本轮亲自 `read` 过（不沿用二手行号）。

### 5.1 bundle 注入点

| 项 | 事实 | 出处 |
|---|---|---|
| bundle 的来源 | `tectonic_bundles::detect_bundle(source, only_cached, custom_cache_dir)` —— 由 `crates/latteset-tectonic` 解析后交给引擎的 launcher（`ProcessingSessionBuilder::bundle` 是**路径 A** 的注入点） | t2 §8；t5 §3.2 |
| 自定义 bundle 只需 | 实现 `IoProvider`（`Bundle` trait 只有 `get_digest()`/`all_files()`） | t2 §8 |
| 现成实现 | `cache` / `dir` / `itar` / `ttb_fs` / `ttb_net` / `zip` | t2 §8 |
| 路径自动识别 | `detect_bundle(source, only_cached, custom_cache_dir)`：目录 → `DirBundle`、`.zip` → `ZipBundle`、`.ttb` → `TTBFsBundle`，**其他扩展名 → `None`** | t2 §8 |
| 已证伪 | `-b <本地 .tar>` 不可用（`error: … doesn't specify a valid bundle.`） | `t1 §1.8 X-04`；`docs/research/tectonic-test-plan.md:200` |

### 5.2 缓存与离线

| 项 | 事实 | 出处 |
|---|---|---|
| format 缓存键 | 路径 A：`FormatCache::new(bundle.get_digest()?, format_cache_path)`；格式名与 `FORMAT_SERIAL = 33` 绑定。**路径 B（本方案）**：format 存取由我们自己的 `input_open_format` / `write_format` 实现（§3.4 第 3 步） | t2 §7.4；t3 §2.1 第 7/8 行 |
| 默认缓存落点 | `%LOCALAPPDATA%\TectonicProject\Tectonic\cache\{bundles,formats}`；`TECTONIC_CACHE_DIR` 是**唯一运行期开关** | `tectonic-test-plan.md:111` |
| 网络依赖是默认项 | `crates/bundles/Cargo.toml:33` `default = ["geturl-reqwest"]`；根包默认 feature 也含它 | t2 §8 |
| `PersistentConfig` 可绕开 | `open(false)` / `default_bundle(only_cached)` / `format_cache_path()` | t2 §8 |
| **`format_cache_path` 默认 = 项目目录**（**硬约束**） | 不显式注入就往用户项目里扔 `.fmt`（本机实测该文件 **24,451,466 B**）⇒ **t9/t10 必须显式注入产品缓存目录** | t3 §6.4 / HL-11 |
| 缓存写入原子性 | format 缓存 = `tempfile_in` + `persist`；bundle 数据/索引 = `<path>-tmp-pid<pid>` + `rename`（Windows 均为 `MoveFileExW \| MOVEFILE_REPLACE_EXISTING`）；hash/check 文件**非原子**（65 B / 10 B，可自愈） | t3 §6.3 |
| **无进程间锁**（**硬约束**） | `.lock` 内容实测是 Unix 秒（**不是锁**）；临时名用 `process::id()` ⇒ **同进程内并发会撞同一临时名**（HL-8）⇒ 产品侧必须按缓存目录串行化，或每任务独立缓存目录 | t3 §6.2 |
| 库形态下是否仍需外部 exe | **不需要**（引擎 crate 直链：`src/lib.rs:96-104`） | t3 §7.2 |
| bundle/format 能否常驻内存 | **路径 A 下不能**（HL-10：`Box<dyn Bundle>` 被 driver 按值收走、`ProcessingSession` 无 getter、`into_file_data()` 又消费会话）；**路径 B 下由我们持有**（bundle 对象与 `.fmt` 字节都可留在进程内，`input_open_format` 由我们从内存返回）⇒ **「常驻」在 B 下才可能有对象级含义**；该收益**未实测**（属 t7 的口径）`[推断]` | t3 §5.2（A 侧为 `[本地源码]`） |

### 5.3 确定性（㉚）与 D4 落地口径

| 项 | 事实 | 出处 |
|---|---|---|
| 子进程形态现状 | 非 Tectonic 无条件 `SOURCE_DATE_EPOCH=0`；Tectonic 不设（D4 (b)） | `runner.rs:299-301`；`tectonic-test-plan.md:845` |
| **t1 设想的「载体问题」已被推翻** | 库形态下 TeX 步与转换步**都有**非 env 的显式注入点 ⇒ 不必「三选一」 | t3 §8.1bis；本文件 §4.3 |
| TeX 步（`\today`） | **路径 B 直接调** `TexEngine::build_date(SystemTime)`（t3 §8.1bis 的注入链）；路径 A 则经 `ProcessingSessionBuilder::build_date` 传递。**默认都是 `UNIX_EPOCH`** | t3 §8.1/§8.2；`crates/engine_xetex/src/lib.rs:154-160` `[已复核]` |
| 转换步（PDF `/ID`、`CreationDate`） | `XdvipdfmxEngine::build_date`（路径 A 下 driver 用同一个 session 值喂它） | t3 §8.1 |
| **不可注入的例外** | PDF **加密**路径的 C 层 `getenv("SOURCE_DATE_EPOCH")`（HL-4） | `crates/pdf_io/pdf_io/dpx-dpxutil.c:177` `[已复核]` |
| 可复现硬约束 | **「PDF 逐字节相等」不得作为 Tectonic 档的断言** | `tectonic-test-plan.md:187,845` |
| 结论 | **`\today` 正确性与转换步确定性可同时保留**（前提 = §4.3 的 D-1/D-2 两条纪律） | t3 §8.1bis |

**旧判断的更正（保留记录，不删）**：t1 §4.5 的「三选一」**已被 t3 证伪**（不再是 ADR 候选）；t1 §4.5 里「库形态可能更容易满足 D4」的 `[推断]` **已由 t3 升格为构造性结论**——「有没有注入点」不再是未知项；仍标 `[待实测]` 的只剩「按 D-1/D-2 注入后，跨天两次跑的 PDF 是否逐字节相等」（即 `tectonic-test-plan.md:913` 的 U-31）。

### 5.4 输出目录与产物落点（必须逐条回答）

| 依赖磁盘契约的下游机制 | 现状 | 库形态处置（候选） |
|---|---|---|
| Quick 升级判据 `tmp/<stem>.aux` 存在 | `runner.rs:354-359` | **路径 B 下我们自己的 I/O 层直接持有 `<stem>.aux`**（内存为主）⇒ 判据改读我们 I/O 层的文件表，**不再依赖 `-k`**；是否需要落盘由我们显式决定（**不用** `output_dir`/`do_not_write_output_files` 这类 session 开关） |
| SyncTeX CLI（外部 `synctex.exe`，`-d` 固定指 `pdf.parent()/tmp`） | `crates/latteset-infra/src/synctex.rs:26-31` | **库形态不解决 TL-less 的 SyncTeX**（G2/R-1）；内存层的 `main.synctex.gz` 是**解压文本**（t2 §4.1），落盘需再压一次 |
| ㉒「生成产物不当源码打开」（`modules.md:860`） | `docs/modules.md:860`；实现 `:490/:520/:617` | 纯逻辑、引擎无关（t1 §7.2-1）⇒ **库形态不改它**；但反向定位的输入仍来自 `.synctex.gz` |
| 页哈希缓存 `tmp/<stem>.<engine>.pages` | `runner.rs:199-211` | 路径 B 下我们**自己产出 XDV**（`XdvipdfmxEngine::process` 的输入就是我们 I/O 层的名字）⇒ 页哈希可继续算；`Engine::writes_xdv()` 的能力位需拆（t1 §6③） |
| `.log`（`parse_log` 的输入） | 磁盘 `tmp/<stem>.log` | **内存层与磁盘层逐字节一致**（构造性等价，t3 §3.3）⇒ `parse_log` 的输入契约**不用改**；两条边界：`keep_logs=false` 时日志**不落盘**、空文件被显式跳过 ⇒ **失败且 `keep_logs=false` 时内存 `.log` 是唯一副本**（对今天 `-k --keep-logs` 的子进程档是信息增量） |

> **文档缺陷（不得照抄）**：`docs/design.md:196` 把 ㉒ 记成「`.fls` 触发面」是**错的** —— ㉒ 在本仓是「生成产物永不当作源码打开」（`docs/modules.md:860`）；`.fls` 触发面属 ㉝/G1（`docs/research/tex-ide-roadmap-priority.md:69`）。t1 §7.2-1 已登记，收口归 T13。

### 5.5 分发形态

| 维度 | 库形态（M3） | 子进程（M1：随包 exe） |
|---|---|---|
| 用户前置 | 零 | 零 |
| 体积 | 库形态增量 **`[未测到]`**（t4 §5.2；"数十 MB"仅为 `[推断]`） | 「随包 exe」锚点：解包 **51,538,432 B** / zip **21,060,223 B**（t4 §5.3；`tectonic-test-plan.md:107`） |
| 构建链 | `vcpkg + x64-windows-static-release + crt-static`；**21 个 native 标记**（graphite2 / ICU / freetype2 / fontconfig / libpng **永远走外部探测**，探测失败即 build script panic）；harfbuzz **随 crates.io 发布版内含源码，不踩空子模块坑** | 无需构建（用上游产物） |
| 许可面 | 静态链接把 C 库义务带进**我们的产物** | 再分发者义务仍在（`tectonic-test-plan.md:727,746`） |

### 5.6 本地 bundle 的 Windows 现实（五条坑 = **t10 的入口条件**）

| # | 坑 | 实测 / 源码 | 后果与处置 |
|---|---|---|---|
| **LB-1** | **绝对路径的 `-b` 被 URL 解析吃掉**：`detect_bundle` 先 `Url::parse(source)`，`E:\...` 与 `E:/...` 都被当成 scheme `e` 解析成功 ⇒ 返回 `Ok(None)` ⇒ 报 `doesn't specify a valid bundle.` | `t3 §7.1` **实测** B1/B2 失败；**t5 §3.2 独立复现+扩展**：B5（绝对路径）、B5b（`file:///…` 但路径不存在）都判 `None`；**B8/B9/B10 证明可行的三种给法**：`file:///` 绝对 URL、**相对路径**、以及 **ACL 只读目录**（均 open_ok + 读到字体 4,947,904 B） | 产品要指本地 bundle，**存储形态必须用 `file://` URL 或相对路径**；t5 的 A6 端到端印证：换成本地只读目录 bundle 后产物**同字节数**（74,389 B / 26 页）。设置面把 `E:\…` / `E:/…` / 带引号的「复制为路径」都补成 `file:///…`（`TectonicSettings::normalize_bundle`，见 [modules.md](./modules.md) §6） |
| **LB-2** | **目录 bundle 必须自带 `SHA256SUM`**：`DirBundle::get_digest` 读它，缺失即 `bail!("bundle does not provide needed SHA256SUM file")`；路径 A 的 `create()` **无条件**调 `get_digest()?`，路径 B 则由我们**显式**调用（缓存键需要它） | `t3 §7.1`：实测报错原文即此句（另注 `DirBundle::all_files` **非递归**） | **裸 TeX Live 目录不能直接当 bundle**；须生成 `SHA256SUM`，或改用 zip/ttb |
| **LB-3** | **当前工作区没有任何"可直接当 bundle 用"的本地 bundle** | t3 附录 A；t5 §3.2 只用到缓存里的 `data/<digest>/` 解包目录与探针目录 | **t10 必须先造一个合规本地 bundle**（含 `SHA256SUM`）才能做离线验证 |
| **LB-4** | **空目录 bundle 静默"成功"**：`detect_bundle` 对空目录返回 `Ok(DirBundle)`，`file_count=0`，直到引擎要文件时才 `NotAvailable` | `t5 §3.3-2` **实测** B6（`file:///.../empty-bundle-dir` → `open_ok=true`、`file_count=0`） | 产品允许自定义 bundle 目录时**必须自己校验**（要求存在 `SHA256SUM` 或抽查关键文件），否则用户看到的是"编译成功但啥都没有" |
| **LB-5** | **错误文案无分类**：路径不存在 / 空目录 / 非 bundle 文件，库层一律同一个 `None`，CLI 一律同一句 `doesn't specify a valid bundle` | `t5 §3.3-3` 实测 B5/B5b/B7 | **分类必须我们在 infra 层自己补**（=门禁 **LIB-16**） |

⇒ §6 的 **P0/P4 入口条件按这五条写**，不得简写成「配个路径就行」。

---

## §6 分阶段计划

> 每阶段：目标 / 交付物 / 入口条件 / 验收判据 / 回退点。与 T9/T10/T11/T12 的任务边界自洽（t9 = 库形态引擎路径；t10 = 本地 bundle 与缓存隔离；t11 = 独立验证；t12 = 收口）。

### P0 前置：`-C` 条件化与首次获取引导（**不属于库形态，必须先做**）—— 已立任务 **t14**

> 任务链（reviewedTaskId）：**t8 → t14 → t9 / t10**。t14 =「修 P0：Tectonic `-C` 无条件离线导致冷缓存/首次使用必失败（含首跑引导与测试）」，assignee = engine-impl（他同时持有 t9/t10，串行不打架）。

| 项 | 内容 |
|---|---|
| 目标 | 让全新机器上的 Tectonic 子进程形态**能拿到 bundle**：首次编译不加 `-C`（或预置 bundle 兜底），此后加 `-C` |
| 出处 | `runner.rs:317`（无条件 `-C`）；`tectonic-test-plan.md:130`（纪律 3）、`:200`（冷缓存实测） |
| 交付物 | `tectonic_command` 的 `-C` 条件化 + 首次引导文案（与 `SettingsPanel.vue:25` 的 hint 对齐）+ 对应单测；**单测必须同步 `crates/latteset-infra/src/runner.rs:891` 与 `:909` 这两条既有 `-C` 断言**（F-12） |
| 验收判据 | ① 命令构造单测：首跑档 argv **不含** `-C`、资源就绪档 **含** `-C`（反例自证：恢复无条件 `-C` 必红）；**并在 `runner.rs:891`/`:909` 两条断言上体现新判定**（不同步 ⇒ 单测仍锁旧行为）；② ENV-B（TL-less 干净 Windows）首编 exit 0 或给可操作错误（`tectonic-test-plan.md:910` U-28） |
| 回退点 | 保持现状（不修）⇒ 库形态**不得**开工（因为可用性缺陷会被错误归因到库形态） |
| 与 t9/t10 的关系 | **t9/t10 的入口条件**（已实体化为 t14）：t14 未落地前，库形态实现不得进入 t9/t10 |
| 独立任务的理由 | P0 有**独立的用户可见面**（首次引导文案 + 冷/热缓存两条路径）。并入 t9 会让「库形态引擎路径」混进形态 B 的既有缺陷修复，验收时说不清是哪件事在生效 |
| t14 的验收要求（依队长裁决） | ① `-C` 条件化必须给**可复核的判定依据**（不许猜的启发式）；② 文案与实际行为一致；③ **冷/热缓存两种情况都要有证据**；④ `runner.rs` 命令构造加两条断言；⑤ `docs/design.md` 同步 |

| 与 §5.6 的关系 | 首次引导路径与本地 bundle 的三条坑（LB-1..LB-3）共用同一入口；若选「预置 bundle 兜底」方案，**必须先满足 LB-2**（目录 bundle 自带 `SHA256SUM`） |

### P1 依赖引入与最小可编译

| 项 | 内容 |
|---|---|
| 目标 | 钉 0.17.0 组合 + **提交 `Cargo.lock`**；在**有网 + 已装 vcpkg 的机器/CI** 上补测构建（t4 已给出补测入口） |
| 前置（t4 的边界，按「外部可执行动作」写） | 装 vcpkg（含 cmake / pkg-config / nasm）+ 放开 crates.io 网络；**不是「本会话做不到」** | 
| 交付物 | `Cargo.toml` 改动 + **形态选择决议** + 构建记录（补齐 t4 三项未测到的数：冷/热墙钟、峰值内存、体积增量） |
| **落点决议（= §3.2 的唯一裁决）** | 新建 workspace 成员 **`crates/latteset-tectonic/`**；`src-tauri` 以 **optional 依赖 + feature `tectonic-lib`** 依赖它；`latteset-infra` **不依赖**它；**根 `Cargo.toml` 同步加 `default-members`**（排除本 crate 的默认构建）—— 这三条由 **t9 落地** |
| 入口条件 | t4 已回（构建代价面已量清到它的边界）。**与 t14 的关系（round-3 消歧）**：构建代价的**补测**本身与 t14 无依赖，可与 P0 并行准备；但 P1 的**落盘动作**（新增 `crates/latteset-tectonic/`、根 `Cargo.toml` 与 `default-members`、提交 `Cargo.lock`）就落在 t9 的 inScope 内 ⇒ **仍受 P0 闸门约束：t14 未落地前不得落盘** |
| 验收判据 | `cargo build` exit 0，且三项 t4 未测到的指标**此时必须有数字**；harfbuzz 走 crates.io 发布版（不踩空子模块坑）；**不带 `-p` 的根 `cargo build` 不得触发本 crate**（`default-members` 生效的证据） |
| 回退点 | 构建链不可行 ⇒ 停在子进程形态（N1） |

### P2 骨架：库形态 runner + 内存产物（等价子进程产物）

| 项 | 内容 |
|---|---|
| 目标 | `TectonicLibRunner` 按 **§3.4 路径 B** 跑通「磁盘输入 → 内存产物 → PDF」，产物与子进程档等价（页数级） |
| 交付物 | `crates/latteset-tectonic/src/{lib,io,runner,status,bundle}.rs`；core 形态位字段；`src-tauri` 的 feature 装配分支（§3.2） |
| 入口条件 | P1 通过；t3 已回（I/O 与缓存口径已定）；**§4.7 的 ADR-0005 决策门已获批准**（路径 B = 自研驱动取向） |
| 验收判据 | `cargo build -p latteset-tectonic` exit 0；`latteset-cli --project <dir> compile` exit 0；`pdfinfo` 页数 == 子进程档；我们 I/O 层的 `.log` 可被 `parse_log` 解析（与磁盘层逐字节一致，t3 §3.3） |
| 回退点 | 库形态 runner 整体删除即可回到现状（装配点一行） |

### P3 自定义 I/O + 缓存隔离 + 离线（**验收门槛 = t10 的边界**）

| 项 | 内容 |
|---|---|
| 目标 | 输入输出**全走我们自持的 `IoProvider`**（§3.4 第 1 步）；bundle 与缓存指到自定义路径；format 自管并落产品缓存；离线可跑 |
| 交付物 | `crates/latteset-tectonic/src/bundle.rs` + `io.rs` 的 format 存取；缓存目录参数；分类失败文案（bundle 不存在 / 空目录 / 非 bundle / 空缓存 / 无网络） |
| 入口条件 | P2 通过；P0 已落地；且 §5.2 的两条硬约束已进设计——format 缓存**由我们显式落产品目录**（HL-11）、缓存目录**串行化或每任务隔离**（HL-8） |
| 验收判据 | 反例自证四条：① 缓存目录为空时的行为有明确文案（不是 panic、不是静默）；② **三类 bundle 源给三种不同文案**（路径不存在 / 空目录 / 非 bundle 文件，t5 §3.3）；③ **项目目录里不得出现 `.fmt`**（反例自证：若 format 落点写成相对/默认路径，24 MB 文件会落进用户项目）；④ 内存产物与 CLI 同口径等价（四维，t5 §3.1） |
| 回退点 | 缓存共享用户 Tectonic 默认目录（D6 的保守选项） |

### P4 常驻进程与 format 复用（J1）

| 项 | 内容 |
|---|---|
| 目标 | 同进程内连续编译复用**磁盘 `.fmt` 命中 + 省掉进程启动与引擎初始化**；量出省下的地板。**「常驻」的口径**：路径 A 下不可能做对象级常驻（HL-10，t3 §5.2）；**路径 B 下 bundle 对象与 `.fmt` 字节可由我们持有**（§5.2 已更正）⇒ 对象级常驻**可能**成立，但**未实测**，`[推断]` |
| 交付物 | 宿主进程常驻形态设计 + 计时对比（对齐 t7 的「常驻复用」口径） |
| **准入判决（t5 后）** | **`[仍无法判定]`**：t5 只判了三条判据（①②③），**没有测「常驻 vs 每次起进程」的对照**；它给的只是拆分数字（CLI 排版 824 ms / 排版+转换 1,155 ms / 进程内转换 337 ms，t5 §2.2）⇒ **本阶段的闸门是 t7**，不是 t5。按契约：t5 未测的不得当成立（t5 §6） |
| 入口条件 | **t7 给出"常驻复用"判决**（且判据 = t1 §3 J1 的通过线/FAIL）；§5.6 的 LB-3 已解决（t10 先造出可用本地 bundle） |
| 验收判据 | 库形态同档中位 **< 子进程地板 × 0.8** 且绝对节省 ≥ 100 ms（t1 §3 J1 `[阈值·本文提出]`）；否则记「不成立」并停止本阶段。附带必查：format 缓存**必须**落产品目录（§5.2 硬约束），不得落进用户项目 |
| 回退点 | J1 不成立 ⇒ 只保留 P2/P3（内存产物与 I/O 接管），不投入常驻 |
| **复核命令的可用性（F-13 / round-3）** | `scripts/bench-tectonic-lib.mjs` **尚未入库**（§6 表已标"计划新增"）⇒ **该脚本由 t7 交付；t7 交付前 P4 没有可复核命令 ⇒ P4 不得开工**（本条与"等 t7 判决"并列，构成 P4 的双重闸门） |

### P5 编译中页事件（J2，条件阶段）

| 项 | 内容 |
|---|---|
| 目标 | 把「编译中页事件」接到 `CompileProgress`，并给出首事件提前量的量化 |
| 交付物 | `crates/latteset-tectonic/src/io.rs` 的输出层探针（tee/观测写句柄）+ `XdvParser` 消费层 + 页事件时间线证据 |
| **准入判决（t5）** | **"官方 API 逐页事件"= 不成立；"自建输出层逐块解析"= 实测成立**（t5 §4.1/§4.2）：CLI 的 `.xdv` 在 996 ms 编译里只有 2 个采样点（35 ms=0 B → 966 ms=243,012 B，中间零增长）；自建输出层 + `XdvParser::parse(chunk=16 KB)` ⇒ **16 KB 时已完成 4 页、243 KB 时 26 页**，并产出精确页表；转换段 PDF 字节**运行中逐段增长**（15 B→74,389 B，约 70 个采样/2.1 s） |
| **实现口径（不得再写成「取决于我们的实现」）** | 两条腿，缺一不可：① **自己持有输出层**（自实现 `IoProvider` 的 `output_open_name`/写句柄），② 自己跑 `XdvParser`。C 侧阻塞点：`DVI_BUF_SIZE=16384` 才 `dvi_swap`（`xetex-shipout.c:11/67-77/2384-2411`），9 处 `ttstub_output_flush` 参数全是 `rust_stdout`（XDV 句柄**从不** flush），且根 crate 全仓**零** `XdvParser/XdvEvents` 使用（t5 §4.1） |
| 入口条件 | **§4.7 的 ADR-0005 决策门已获批准**（路径 B 已在 §3.4 选定 ⇒ 自研驱动取向被接受）；并补测 t5 **U1/U2**（排版段以我们 I/O 层观测） |
| 验收判据 | 页事件单调递增、数量 == 页数；**粒度如实记为「字节块 ≤ 16 KB、页 k 的事件滞后 ≤ 一块」**；转换段只断言"字节逐段增长"（**不断言"页 k 的 PDF 已可渲染"** —— t5 §4.2(b) 明确该口径未测，属 t5 §6 U4） |
| **提前量阈值的处置（F-08 / round-3）** | **保留但本轮不判**：t1 §3 J2 的通过线「末页事件早于编译返回 ≥ 200 ms」原样保留为 **LIB-3d**（`[待补测]`）。**本轮不判的理由**：① t5 测的是"字节块粒度与到达顺序"，**没有测该差值**；② t3 §9.3bis 的 N2 指出**尾页可能迟到最末**（残余缓冲只在 `finalize_dvi` / `output_close` 吐出）⇒ 此时判该阈值只会得到一个由收尾残余决定的假数。**补测条件** = t5 U2（能构建 `tectonic_engine_xetex` 的环境） |
| **明确不作为本轮可做项** | ①「页级即时 flush」= **不成立**，不经上游改 C 不可能（把 `dvi_swap` 改成页末 flush，或把 `EOP` 暴露成回调）⇒ **写成需求项交上游**；②「官方逐页钩子」不存在，**不得按它排期** |
| 回退点 | 不接受自建输出层 ⇒ 本阶段取消；页进度退回 `StatusBackend` + 内存 `.log`（即"收尾一次性"，UI 必须可见该语义） |

### P6 内存 XDV → PDF（J3；**整份路径 = 实测成立，变更本阶段的可做范围**）

| 项 | 内容 |
|---|---|
| **准入判决（t5）** | **整份 XDV 走内存同进程出 PDF = 成立**（243,012 B → 74,389 B / 26 页；与 CLI 页数、文本字符、全文文本 SHA256 前缀**全等**，字节差 −29 B；t5 §2.2）；**前缀（截断）喂进去 = 不成立**（页 5 尾 / 页 6 中 / 64 B 三种截断点一律 exit=1、**0 字节产出**、同一句 `Are you sure this is a DVI file?`；t5 §2.3） |
| 目标（据判决改写） | **主目标**：整份 XDV（内存或 IoProvider 名字可达）→ `XdvipdfmxEngine::process` → 内存 PDF。**前缀路线**若要做，必须**自己定义前缀协议并在自己这侧补后同步/补页**（t5 §2.3 结论），不能指望引擎接受裸前缀 |
| 入口条件 | `process` 收的是**文件名 `&str`**，不是内存切片（t2 §3.1）⇒ XDV 必须以 IoProvider 内的名字可达；与 P5 同属"自建输出层"口径 |
| 验收判据 | ① 整份：`Ok(())` + 页数/文本与 CLI 等价（t5 的四维口径：页数 / 文本字符 / 逐页文本长度 / 全文文本 SHA256 前缀）；② 前缀（**若**做）：**本轮不沿用 t1 §3 J3 的「12/30/60/90/121 五档」通过线** —— 那是**外部 `xdvipdfmx` + 合成 postamble** 的形态（`docs/modules.md:846`），**与库内裸前缀不是同一实验**（t5 §2.3 已证伪裸前缀） |
| 回退点 | 前缀路线失败 ⇒ 退守整份 XDV（保持"要 PDF 就整篇转换"，与 D2 ① 一致）；仍不落地真实磁盘文件 |

### P7 UI / 状态 / 回退可见性

| 项 | 内容 |
|---|---|
| 目标 | D3 衍生义务：两形态并存时能区分「哪个形态在跑」；D1：失败可见、不自动回退 |
| 验收判据 | ① 状态栏可见形态位；② 无「已回退/静默回退」文案（`tectonic-test-plan.md:842`）；③ **feature 开/关两种构建下都能装配**：`--features tectonic-lib` 时用 `TectonicLibRunner`，默认构建用 `LatexmkRunner`，且装配点仍是 `src-tauri/src/lib.rs:83-84` 那一行（§3.2） |
| ↳ 装配现状 | GUI 的两个构建都装 `SwitchableRunner`，由**每趟编译读的设置**（`use_library_form(engine, env_forced)`）决定用哪个 runner；**默认构建永不使用库形态**。① 由状态栏的形态 chip 满足（子进程 / 库内嵌 / 库形态不可用），判定来自 `engine_form` 命令，它复用 runner 的同一个纯函数 |
| 回退点 | 形态位不显示 ⇒ 库形态不得默认开启 |

### P8 验证与收口

| 项 | 内容 |
|---|---|
| 目标 | 独立验证（t11）+ 文档收口（t12） |
| 验收判据 | t11 的真机/headless/反例三件套；t12 的索引、路线图 §6.5 判据状态、门禁编号落地 |
| 回退点 | 验证不过 ⇒ 库形态保持「实验开关」而非发布路径 |

**阶段依赖 DAG**：`P0 → P2`（P0 是 t9/t10 的入口：**P1 的落盘动作与 P2 起都在其内**，仅 P1 的构建补测可与 P0 并行准备，见 P1 入口条件）、`P0 → P3`、`P1 → P2 → P3`、`P3 → P4/P5/P6`、`{P4,P5,P6} → P7 → P8`；另有一条**决策边**：`§4.7 决策门 → P2`（不批准则整体退回路径 A）。

**每阶段的独立复核命令（**全部落在可提交路径** —— `.gitignore:69,83` 覆盖 `test_file/**`，不得作为复核入口；F-05/F-13 修正）**：

| 阶段 | 复核命令 / 入口 | 对应判据 |
|---|---|---|
| P0 | `cargo test -p latteset-infra runner`（命令构造断言；**含 `runner.rs:891`/`:909` 两条随 t14 同步的 `-C` 断言**）+ 冷/热缓存各一次首编记录（记录落 `docs/research/tectonic-lib-evidence.md`，**入库**） | LIB-8；t14 契约 |
| P1 | `pwsh -NoProfile -File scripts/bench-tectonic-build.ps1`（**计划新增、入库**；当前实现先在 `test_file/research-tectonic-lib/build-spike/run-samples.ps1`，由 T12 提升）+ `cargo metadata --offline --no-deps` | §3.1.3；LIB-15 |
| P2 | `cargo build -p latteset-tectonic` → `cargo build -p latteset-server --features tectonic-lib` → `latteset-cli --project <dir> compile`；产物过 `node scripts/validate-pdf.mjs --pdf <out.pdf> --expect-pages N --expect-text <子串>`（**已入库，2026-09-15**） | — |
| P3 | 同上 +「项目目录无 `.fmt`」断言 + 三类 bundle 源反例（不存在 / 空目录 / 非 bundle / **绝对 Windows 路径** / **未配置 bundle**） | LIB-11 / LIB-16 |
| P4 | `node scripts/bench-tectonic-lib.mjs --runs 3 --mode resident|fresh`（**计划新增、入库**；口径与 `tectonic-test-plan.md:935-938` 的 `bench-tectonic*` 系列一致） | LIB-1 / LIB-2 |
| P5 | `node scripts/tectonic-lib-xdvscan.mjs --chunk 16384 --pages-json <入库路径>`（**计划新增、入库**；算法同 `engine-spike/src/bin/xdvscan.rs:120`） | LIB-3 / LIB-3c / LIB-5 |
| P6 | `node scripts/tectonic-lib-xdv2pdf.mjs`（整份 + 前缀反例两种调用；**计划新增、入库**） | LIB-6 / LIB-7 |
| P7 | 真机窗口：切形态 → 编译 → 状态栏可见形态位；无「已回退」文案 | D1/D3 |
| P8 | t11 的三件套（headless + 真机 + 反例）+ `npm run build` / `cargo test -p latteset-core` | 全量回归 |

> **证据落点约定（F-05）**：复核脚本与结论一律进 **`scripts/`**（脚本）或 **`docs/research/tectonic-lib-evidence.md`**（结论与原始摘要，**入库**）；`test_file/research-tectonic-lib/**` 只作为**开发期原始证据**，不作为可复核入口（换机器/清仓即丢）。当前脚本仍在 `test_file/`，**提升动作归 T12/T13**（§13）。

### §6.1 已落地状态（2026-09-15，t10 收口）

**库形态已能真编出 PDF**（中文夹具，1 页，文本层可抽回「你好，世界」）。复核命令的**完整前置**（缺一项就失败，且失败信息不指向环境）：

```powershell
$env:TECTONIC_DEP_BACKEND='vcpkg'
$env:VCPKG_ROOT='<repo>\test_file\vcpkg'          # pin rev a62ce77d56 的检出（§3.1.3）
$env:VCPKG_DEFAULT_TRIPLET='x64-windows-static-release'
$env:VCPKG_DEFAULT_HOST_TRIPLET='x64-windows-static-release'
$env:VCPKGRS_TRIPLET='x64-windows-static-release'  # ⚠ 非它不可：release profile 下 vcpkg-rs 会自己算成
                                                  #   `x64-windows-static`（未装）⇒ build script panic
$env:RUSTFLAGS='-Ctarget-feature=+crt-static'
cargo build -p latteset-server --features tectonic-lib --bin latteset-cli
$env:LATTESET_TECTONIC_LIB='1'                     # 运行期开关（装配点见 §3.2）
$env:LATTESET_TECTONIC_BUNDLE='file:///E:/.../bundle'   # 目录 bundle，须自带 SHA256SUM（LB-2/LB-4）
$env:LATTESET_TECTONIC_CACHE='E:\...\cache'        # 产品缓存：<cache>/formats 与 <cache>/bundles
& src-tauri\target\debug\latteset-cli.exe --project <dir> compile
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

**仍然不成立的（不得当通过，与 §13 的未实测项一致）**：页哈希（`PAGE_HASH_SUPPORTED=false`，显式空表）、外部工具类（biber / makeindex / glossaries —— 库形态不跑外部进程；检出即 `warn!` 且 `kind` 退回 `Quick`）、常驻复用（P4，闸门仍是 J1）。

### §6.2 bib 趟与收敛（2026-09-15 补齐，V-03/V-04 收口）

**结论先行**：`latex → bibtex → latex` **不等于引用解析好了** —— 实测第二趟读到 `.bbl` 之后 LaTeX 仍报
`Citation 'knuth1984' undefined` + `Label(s) may have changed. Rerun to get cross-references right.`，
因为 `\bibcite`（编号）是上一趟才写进 `.aux` 的。⇒ **只做 bib 趟而不做重跑循环，等于白跑**。
所以本轮把上游 `default_pass` 的重跑循环一并落地，并按 ㉘ 的产品语义分档：

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

**形态段只在选中 Tectonic 引擎时渲染**（见 [modules.md](./modules.md) §6）⇒ 存过 `lib_form` 却切到别的
引擎时它是不可见的。引擎那一栏为此点名提示「已保存 Tectonic 形态设置（库内嵌 / 宏包集 / 缓存目录）；
仅在选择 Tectonic 引擎时生效」。

### §6.4 P4 常驻档实测（J1 判决：**不成立**，2026-09-15）

复核入口：`scripts/bench-tectonic-lib.mjs --mode resident` 驱动
`crates/latteset-tectonic/examples/bench.rs`（本轮新增：**同一个进程里连续编译 N 次** ——
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

**④ 真正的固定成本（都不是进程，本轮的 LIB-2 分段数字）**

| 段 | 数字 | 说明 |
|---|---|---|
| format 文件读 | **7.3–15 ms/趟**（24.45 MB） | 归属我们 I/O 层；引擎侧对 dump 的**解析**在 C 里，未测 |
| bundle 读 | **2009 次/编译 → 23–41 ms** | `req_bundle=1413` + miss 596；常驻化它是比进程更值钱的杠杆 |
| **Full 的无条件 bib 重跑** | **≈470 ms（−42%）可省** | `.aux` 含 `\bibdata` ⇒ 跑 bibtex ⇒ **强制**重跑一趟（上游同口径）。按 latexmk 的"`.bib` 比 `.bbl` 新才跑 bibtex"判据，**内容未变的文档可以只跑 1 趟** |

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

⇒ 库形态与子进程**同速**（与 t7 的"无可证实改善"一致；t7 的常驻复用 −19%~−26% 仍未被本轮证伪，因为本轮**没有**做常驻）。

> ⚠ **测量纪律（本轮踩过）**：`dev` profile 下 C 引擎（xetex/xdvipdfmx）按 `-O0` 编译，同一夹具的库形态数字会比 release 大 **6–8×**（实测 debug 3485 ms vs release 543 ms / 同一文档）。**凡与 `tectonic.exe` 比性能，必须先 `--release`**，否则结论完全相反。

---

## §7 候选可测判据 → 测试方案门禁编号映射表

> 现有编号空间：`P-G1..P-G19`（前置门禁）、`E1–E7`（引擎层）、`INT-10..INT-96`（集成层，含后缀 `INT-20b/31b/36b/38b/43b/54b`）、`DIST-*`/`MB-*`/`G-L1..G-L9`、`U-1..U-31`、`R-1..R-14`（`docs/research/tectonic-test-plan.md:5,216,1014,1017`）。
> **新判据用新前缀 `LIB-*`，不与上述任何编号段冲突**（T13 落地）。
> **本文 §12 的风险用 `LBR-*` 前缀**（Latteset 库形态风险），**不占用** test-plan §6.1 的 `R-1..R-14`；跨文档引用时写「本文 LBR-n」或「test-plan §6.1 R-n」（round-3 修正，F-10）。

| 新编号 | 判据（来源） | 观测点 | 通过线 | 与被替换的既有编号的关系 |
|---|---|---|---|---|
| **LIB-1** | J1：常驻后的 pass 成本（t1 §3 J1） | `TexEngine::process` 段计时 vs 子进程 `-r 0` | 中位 < 子进程同档地板 × 0.8 且省 ≥ 100 ms | **取代** P-G5 的「`-p` 页通道」作为实时性判据的位置（P-G5 对子进程档仍保留） |
| **LIB-2** | J1 归因：地板内部拆分（启动 / bundle 缓存 / format 加载） | 分段计时 | 三段各有数字（无阈值） | 新增（`realtime-preview-cost.md:89` 明确未测） |
| **LIB-3** | J2：编译中页事件（**t5 判决：官方 API 不成立 / 自建输出层成立**） | 自建 `IoProvider` 写句柄收到的字节块 → `XdvParser::parse` → `handle_begin_page` 到达时刻 | 页事件单调递增、数量 == 页数；**粒度如实记「块 ≤ 16 KB、滞后 ≤ 一块」**（t5 §4.2：16 KB→4 页、243 KB→26 页） | **取代** U-8 在库形态下的地位（U-8 仍管子进程 `[N]`）；**不得**写成"逐页 flush"（那是 LIB-3b） |
| **LIB-3b** | 「页级即时 flush」（**t3+t5 定案：不成立**） | `dvi_swap` 的调用时机 / `ttstub_output_flush` 的参数 | 本轮**不判**（不经上游改 C 不可能） | **需求项交上游**，不作为本轮判据（t3 HL-3；t5 §4.1） |
| **LIB-3c** | 转换段"PDF 字节运行中逐段增长"（t5 §4.2(b)） | 内存 PDF 字节数时间线 | 严格递增且首段为 `%PDF-1.5` | 新增；**只断言字节增长，不断言"页 k 已可渲染"**（t5 §6 U4 未测） |
| **LIB-3d** | **t1 §3 J2 的原通过线（F-08：恢复，不静默替换）** | `XdvEvents::handle_begin_page` 的**末页**事件时刻 vs **整轮编译返回**时刻 | 末页事件早于编译返回 **≥ 200 ms** `[阈值·原文档提出]`，且页事件单调递增、数量 == 页数 | **恢复** `test_file/research-tectonic-lib/repo-map.md:186` 的原文；t5 未直接测该差值（它测的是"字节块粒度"），且 t3 §9.3bis 注明**尾页可能迟到最末** ⇒ 本条标 `[待补测]`，与 LIB-3（字节粒度，已实测）**并列**，不互相替代 |
| **LIB-4** | J2 的草案层价值：`x[]/y[]` 非空 | `handle_text_and_glyphs` 的 `glyphs/x/y` | 非空且与页对应 | 新增（`roadmap §6.5:240`）；**t5 只实测到"页事件/字节层"，字形数组仍 `[未实测]`** |
| **LIB-5** | 逐页字节偏移（t3 给机制、t5 给实测） | `handle_begin_page` 的 `previous_bop` + `counters`；`XdvParser::current_offset()` 簿记 | 页起始偏移可复现、与页表自洽 | **取代** t2 §6.3 的 `[未定]`；t5 已产出精确页表（26 页，postamble 偏移 == 文件尾；开发期原始证据在 `engine-spike/runs/pages-gui-p26.json`，**入库摘要归 §13 的证据文档**） |
| **LIB-6** | J3：内存 XDV → PDF（**t5 判决：整份成立 / 前缀不成立**） | `XdvipdfmxEngine::process(launcher, dvi, pdf)` | 整份：`Ok(())` + 四维等价（页数 / 文本字符 / 逐页文本长度 / 全文文本 SHA256 前缀，t5 §3.1）；前缀：**本轮不沿用五档通过线**（t1 §3 J3 那套是**外部 xdvipdfmx + 合成 postamble**，`docs/modules.md:846`，与库内裸前缀不是同一实验） | 新增；两形态**不得混引**（t5 §2.3 已证伪裸前缀） |
| **LIB-7** | J3 的字体解析不依赖本机 TL/fontconfig | bundle 被请求的文件清单 | 字体从 bundle 解析成功 | **t5 实测**：A1 记录 **76 次 bundle 输入打开**（含 12 个 native font，如 `FandolSong-Regular.otf`）；A6 把 bundle 换成 `file://` 本地只读目录后**产物同字节数**（t5 §2.2/§3.2）⇒ 库内转换不依赖本机 TL。与 **INT-38**（外部转换器探测）区分：本条判「库内」 |
| **LIB-8** | 首次 bundle 获取（P0 的验收） | 首跑 argv + 冷缓存行为 | 首跑不含 `-C`；资源就绪档含 `-C` | **扩展** P-G15/DIST-2.8（离线档）与 U-5（`-C` 缺文件/断网退出码）：LIB-8 判「命令构造的分档」，P-G15 判「离线可达」 |
| **LIB-9** | 库形态的取消可见性（X-3） | 取消点（IoProvider 回调）延迟 | 取消后 ≤ 一个回调周期内返回 `Aborted` | 新增；**取代** `kill_tree` 在库形态下的地位（`runner.rs:770-793` 对子进程档保留） |
| **LIB-10** | 库形态 `.log` 与子进程 `.log` 的可解析等价 | `parse_log` 输入 | 同文档同引擎，两者解析出的错误条目一致 | **取代** U-3 的怀疑（t3 §3.3 已给出**构造性等价**：逐字节一致 ⇒ 可关闭 U-3） |
| **LIB-11** | format 落点（HL-11，**路径 B 口径**） | 编译后项目目录内容 + 我们的 format 缓存目录 | 项目目录**无 `.fmt`**；产品缓存目录出现 `<bundle digest>-latex-33.fmt` | 新增；反例自证：format 落点若用相对/默认路径，24 MB 文件会落进用户项目（t3 §6.4） |
| **LIB-12** | `build_date` 纪律（D-1/D-2） | 单测：库内不调 `build_date_from_env`；显式 `build_date(now)` | `\today` == 当天（不是 1970）；去掉显式注入即红 | 新增（HL-5）；**路径 B 直接对 `TexEngine` 调用**（§5.3） |
| **LIB-13** | 缓存目录并发安全（HL-8） | 同进程两个编译任务的临时文件路径 | 不出现同一 `-tmp-pid<pid>` 争用（串行化或独立缓存目录） | 新增 |
| **LIB-14** | 不往进程 CWD 写产物（HL-13，**路径 B 口径**） | 我们 I/O 层的**每一个**输出名字 | 每个输出都落到我们显式选择的落点（内存或指定目录），**没有任何相对名字直接落 CWD** | 新增（t3 HL-13 是路径 A 的形态：buffer 输入 + 未设 `output_dir` ⇒ 默认落 CWD；路径 B 下风险转移到我们自己的落点决策） |
| **LIB-15** | 默认构建的零成本边界（**队长决定 3 降级后**） | ① 未预热 registry 缓存的默认构建；② **不带 `-p` 的根 `cargo build`** | ① 不因 tectonic 可选依赖而**解析**失败；② 根 `cargo build` **不构建** `crates/latteset-tectonic`（靠 `default-members` 排除） | **降级说明**：feature 门控只带来"**依赖解析层面**"的零成本（t4 §9 样本 A3/E3：关闭 feature 仍要解析整棵图）；**成员构建层面**的零成本**必须**靠 `default-members` 配合（§3.2），**不是**feature 自动给的 |
| **LIB-16** | bundle 源的三类失败必须可分 | 「路径不存在」「目录存在但不是 bundle（空目录）」「非 bundle 文件」三种输入 | infra 层给出**三种不同**的可读文案 | 新增；t5 §3.3 实测：库层一律 `None`、CLI 一律 `doesn't specify a valid bundle`，**空目录还会"静默 open 成功但 0 文件"**（B6）⇒ 分类必须我们自己做 |

**保留不变的既有门禁**（库形态**不得**顺手改判）：`P-G9`（设置重置）、`P-G14/G-L1..G-L9`（许可）、`P-G17`（SyncTeX 往返）、`INT-90/95/96`（D1 一键切引擎）、`E1.1`（中文正确性）。

---

## §8 性能与成本预算表

> 口径声明：本节只允许两类数字 —— `[实测]`（指到 t5 的 evidence.jsonl 或既有实测）与 `[待 t7]`。**本方案不产生新的绝对毫秒数字**（跨 t5/t7 两批不可比，t7 任务契约）；每个数字必须连**夹具与命令口径**一起引（见 §8.2 注）。

### 8.1 已实测基线（子进程形态）

| 项 | 值 | 出处 |
|---|---|---|
| Tectonic 单趟含 PDF：en-empty / zh-min / thesis(28p) | **283.2 / 481.7 / 881.8 ms**（min） | `docs/research/realtime-preview-cost.md:36-39` |
| Tectonic XDV 档：thesis / multifile(74p) | **598.5 / 585.1 ms**（min） | 同上 `:40-41` |
| Tectonic 收敛档 thesis（对照） | **2276.8 ms**（min） | 同上 `:42` |
| 产品 Quick 两段合计（xelatex + xdvipdfmx） | **≈1.96 s** | 同上 `:52` |
| 击键路径（单趟 + 出 PDF）现状 vs Tectonic 子进程 | ≈1.96 s → **0.88–1.07 s**（≈ −50%） | 同上 `:73` |
| 地板内部拆分（启动/bundle 缓存/format 加载） | **未测** ⇒ 库形态节省只有上界（≤283 ms） | 同上 `:89` |

### 8.2 库形态的预算（**t5 拆分数字已落；对比列待 t7**）

> 回填顺序（依队长裁决）：**t5 先出数**（准入判据的拆分数字），t7 回来后**再补「相对子进程」的对比列** —— 两列表不得混成一张无口径的表。t5/t7 同属 integration-spike，先后串行。

| 项 | 拆分数字（t5 实测口径） | 相对子进程对比列（t7） |
|---|---|---|
| **排版段（CLI `--outfmt xdv`，3 次中位）** | **824 ms**（748/824/872） | **`[待 t7]`** |
| **排版+转换（CLI `--outfmt pdf`，3 次中位）** | **1,155 ms**（1,072/1,155/1,224） | **`[待 t7]`** |
| ⇒ CLI 内转换增量 | **≈331 ms** | — |
| **进程内转换（`XdvipdfmxEngine`，release 3 次中位）** | **337 ms**（317/337/449）——与 CLI 增量差 ≈2%，两条腿互相印证 | **`[待 t7]`** |
| 首编（冷缓存）/ 暖编（热 format） | **`[未测]`**（t5 未做冷/热对照） | **`[待 t7]`** |
| 常驻复用（同进程连续编译 vs 每次起进程） | **已实测（2026-09-15，本文 §6.4）：J1 不成立** —— 轻档（1 页）净收益 **−26 ms**、重档（28 页 Full）**+341 ms（+31%，第 3 轮起退化）**；进程地板本身只有 9.3 ms ⇒ 天花板远低于 P4 要求的 ≥100 ms。**按回退点处理：不投入常驻** | §6.4 |
| 内存峰值工作集 | — | **`[待 t7]`**（测不到则写明为什么） |
| 二进制/磁盘体积 | 库形态增量 **`[未测到]`**（t4 §5.2） | `[待 t7]` |
| CI 构建时长 | **已回填（t4）**：稳态增量粗估 **+15–20 min/冷、+2–5 min/热** `[推断]`（上游同构 job 19m05s 为参照） | — |

> **t5 数字的口径（必须连口径一起引）**：夹具 = `test_file/projects/gui-p26`（ctexbook，8 章 include + bib，**26 页 / 243,012 B XDV**）与 `bench/_e1/zh-min.tex`（1 页）；命令 = `tectonic -C -r 0 --outfmt {xdv,pdf} -o <fresh dir> main.tex`；**每个样本都过 pdf.js 校验**（t5 §1.2）。与 `realtime-preview-cost.md` 的 28 页 thesis 夹具**不是同一份**，绝对值**不得跨夹具相减**（t5 §2.2 的注）。
> 纪律（t7 任务契约）：**同批内比较才有结论**；跨会话绝对毫秒只作记录，不得写进结论。

### 8.3 成本侧已知事实（t4 已回填；**不含任何估值**）

| 项 | 事实 | 出处 |
|---|---|---|
| 冷/热构建墙钟、峰值内存、体积增量 | **`[未测到]` + 逐条原因**（本会话构建从未开始）⇒ 本方案**不给这三个数**；补测入口 = `build-spike/run-samples.ps1`（需 vcpkg+cmake+pkg-config+nasm 与网络） | t4 §5.2；§3.1.3 |
| C 依赖闭包 | `tectonic_engine_xetex` 直接依赖 freetype2 / graphite2 / harfbuzz / icu / flate（zlib）桥接；**两形态的 C 链完全相同（各 21 个 native 标记）** ⇒ **没有「只要 XeTeX 引擎的小依赖」这条捷径** | t2 §1.2；t4 §3.1 |
| 系统工具前置 | Windows MSVC 唯一官方路线 `vcpkg + x64-windows-static-release + crt-static`；**graphite2/ICU/freetype2/fontconfig/libpng 永远走外部探测**（探测失败 = build script panic） | t4 §4 |
| 依赖冲突面 | 与本仓 lock：SAME 161 / UNIFY 62 / **DUP 19** / 新增 169；合并后 lock 上界 ≈ **728 包** | t4 §3.2 |
| 本地工作树坑（**适用范围已更正**） | `crates/bridge_harfbuzz/harfbuzz/` 为空导致 `exit 1` —— **只对 git 检出未 init 子模块成立**；crates.io 发布包内含 harfbuzz 源码 ⇒ **本方案走发布版，不踩** | t4 §3.3 |
| 随包形态的体积锚点 | 官方 Windows/MSVC 0.17.0 **静态单文件 51,538,432 B（49.1 MB，无 DLL）**；zip 21,060,223 B | t4 §5.3；`tectonic-test-plan.md:107` |
| CI 参照 | 上游同构 job `vcpkg (x86_64-pc-windows-msvc)` **19m05s**（装依赖 2m24s + Build&Test 14m51s，缓存命中）——**跨机器，只作参照** | t4 §7 |

---

## §9 失败模式与降级

| # | 失败模式 | 现象 | 处置（D1：不自动回退、失败可见） |
|---|---|---|---|
| F1 | 无 TeX Live + 无 synctex.exe | SyncTeX 正反向不可用 | 可见降级（状态栏/工具条提示），工程为 P-G17/INT-54（`tectonic-test-plan.md:240`） |
| F2 | bundle 缺失 / 冷缓存 | 库层 `this bundle isn't cached, and we couldn't get it from the internet. <backend error>`（t5 §3.4；本 spike 的 null 后端文案为 `no get-URL backend was enabled`） | 文案可操作；**不得**静默回退到 XeLaTeX（`tectonic-test-plan.md:842,200`）。**真实"空缓存+有网"的行为仍未实测**（t5 §6 U3） |
| F2b | **空目录 bundle 被静默当成合法 bundle** | `detect_bundle` 返回 `Ok(DirBundle)`、`file_count=0`，直到引擎要文件才 `NotAvailable`（t5 §3.3-2 实测 B6） | 产品侧自校验（要求 `SHA256SUM` 或抽查关键文件）+ 分类文案（门禁 **LIB-16**） |
| F2c | **三类 bundle 错误文案无区分** | 路径不存在 / 空目录 / 非 bundle 文件在库层都是同一个 `None`（t5 §3.3-3） | infra 层自己补分类（**LIB-16**） |
| F3 | 离线且无可达 bundle | 网络失败 | P0/P3 处置：预置 bundle 或明确「需要一次性联网」引导 |
| F4 | 引擎崩溃（C abort） | 同进程可能整进程挂 | 结构性代价（`roadmap:244`）：**必须**在 ADR 里写清「隔离能力下降」；UI 不承诺「编译失败=应用不崩」 |
| F5 | 超时 | runner 自查超时 | 沿用「不自动重试 + 证据化诊断」（`AGENTS.md` roadmap ㉕） |
| F6 | 上游 API 变更（0.x SemVer 不保证） | 编译期 break | 钉 `Cargo.lock` + 提交后 P-G18 式「三件套唯一真相源」检查 |
| F7 | 形态位被旧版读到 | 旧版整份设置重置 | 形态位**必须可缺省**（§3.5、t1 §4.6） |
| F8 | 库形态 runner 失败而子进程可用 | 用户想用但用不了 | 回退开关 = 手动切回子进程形态（不是自动回退） |

---

## §10 分发与许可的下一批工作

> **本节不得写成结论**：t2 §2.3 明确「C 依赖许可分布 `[未定]`」；Cargo 元数据里 Tectonic 本体与 28/28 crate 的 `license = "MIT"`（t2 §2.1），但 HarfBuzz / FreeType / Graphite2 / ICU / fontconfig / libpng 的**许可正文在仓内不存在**（t2 §2.3）。

| # | 下一批工作 | 门禁（既有编号） | 阻塞关系 |
|---|---|---|---|
| W-1 | 三张清单：A（`cargo metadata` 空值）、B（bundle FILELIST → TL 包 → 许可）、C（二进制 + C 依赖） | `tectonic-test-plan.md:751-753` | 清单 A 纯本地可先做 |
| W-2 | 产出 `docs/research/tectonic-bundle-licensing.md`（**目前不存在**） | 文件存在性 = G-L1 | 阻塞发包 |
| W-3 | 跑 G-L1..G-L9 全绿 | `tectonic-test-plan.md:766-778`；P-G14 | **未全绿前不得把 Tectonic/bundle 放进任何发行物** |
| W-4 | 为**许可清单 C**取 HarfBuzz 的 `COPYING` 正文（**与构建无关**：走 crates.io 发布版构建时包内含源码、不踩空子模块坑，t4 §3.3） | 清单 C 的前置 | 本机检出无该子模块（t2 §7.1）；发布日期包内含源码 |
| W-5 | 确定 Windows 侧实际链接的 C 库来源与许可 | 清单 C | `[未定]`（t2 §2.3） |
| W-6 | 若引入库形态：**落 ADR-A（编号 0012）** —— 记录「新 crate 承载 C 链（X-5）+ 哪类 IO 不受 ADR-0010 管 + §4.7 的 ADR-0005 决策门批准」 | §4.4 的 ADR-A（T12 落文件） | §4.2 / §4.4 / §4.7 |

---

## §11 后置项（不做清单）

| # | 后置项 | 理由 |
|---|---|---|
| Z-1 | 「编辑器未保存缓冲直喂引擎」 | 与 ADR-0007 冲突（t1 §4.6）；同源先例 ㉙ 被评 P2 |
| Z-2 | ~~自己编排三 pass（替代 `ProcessingSession`）~~ **已改判（N-03）** | **不再是"不做项"**：§3.4 已**选定路径 B**（引擎 crate + 自持输出层），"自己编排三 pass"正是该路径的一部分；它与 ADR-0005 的取向冲突**由 §4.7 的决策门批准后消解**。保留本条以记录改判前后的两种定性 |
| Z-3 | 「逐页字节偏移要改上游」 | **已被 t5 推翻**：公开 API 足够（`handle_begin_page` 的 `previous_bop` + `XdvParser::current_offset()`），t5 已产出精确页表 ⇒ **不做**（**只有"页级即时 flush"才需要上游改 C**，见 P5 的需求项） |
| Z-4 | `-b <本地 .tar>` | 已证伪（t1 X-04；t5 §3 亦只验了目录 bundle） |
| Z-5 | 「**前缀** XDV → 部分 PDF」 | **t5 已证伪**：截断到页边界/页中/64 B 一律 exit=1 且 0 字节产出（t5 §2.3）；若将来要"边排边转"，得**自己定义前缀协议并补后同步**，属新需求而非现有能力 |
| Z-5b | 页级复用 A/B/C 在库形态的恢复 | **整份 XDV 路线下可行**（内存里拿到完整 XDV ⇒ 可算页哈希）；但"部分 PDF 复用"随 Z-5 一起搁置 |
| Z-6 | 本地 bundle 自建/裁剪（CJK 字体） | 体积与许可杠杆，但属分发批次（`tectonic-test-plan.md:744`） |
| Z-7 | 多窗口并发编译的库形态隔离 | 全局互斥锁下库形态不带来并行（t2 §7.4 第 5 条 `[推断]`），风险见 **LBR-7** |

---

## §12 风险表

> **编号前缀（round-3 修正）**：下表风险一律用 **`LBR-n`** 前缀（"Latteset 库形态风险"），与 `docs/research/tectonic-test-plan.md` §6.1 的 `R-1..R-14` **彻底区分**；跨文档引用时写「本文 LBR-n」或「test-plan §6.1 R-n」。§7 的编号空间说明同步（见 §7 抬头）。

| # | 风险 | 后果 | 现有处置 / 判据 | 依据 |
|---|---|---|---|---|
| LBR-1 | **构建链变重（C 依赖）** | CI 明显变重，Windows 需要 vcpkg；**21 个 native/C 链标记**（t4 §3.1），harfbuzz **随 crates.io 发布版内含源码 ⇒ 不踩空子模块坑**（t4 §3.3） | P1 阶段先做构建实测；不可行则停在子进程（N1） | `roadmap:244`；t4 §3.1/§3.3（**t2 §7.1/§7.2 的"子模块硬 exit 1"只对 git 检出未 init 子模块成立，见 §3.1.3**） |
| LBR-2 | **CJK 字体 / fontconfig** | 字体解析依赖系统 fontconfig ⇒ 与「免装 TL」冲突 | E1.2 必须坐实「中文不依赖 bundle 字体」；库形态字体走 bundle 而非本机 fontconfig | `tectonic-test-plan.md:255,197` |
| LBR-3 | **bundle 体积与许可** | 审计不过不能发包；预置缓存刷新即失效 | §10 的门禁链（G-L1..G-L9） | `tectonic-test-plan.md:237,868` |
| LBR-4 | **Windows 分发** | 未签名 exe + 未签名安装包，SmartScreen 双门槛；库形态不改变这点 | 与 ADR-0003 叠加；不新增处置 | `tectonic-test-plan.md:730`；`docs/adr/0003-*` |
| LBR-5 | **SyncTeX 等价性** | TL-less 下不可用；库形态不解决 | P-G17；内存层 `main.synctex.gz` 是解压文本，落盘需再压 | `tectonic-test-plan.md:240,901`（U-19）；t2 §4.1 |
| LBR-6 | **㉒/㉖/㉗ 对 Tectonic 档的适用性**（**编号已更正，F-06 / round-3**） | 反向回落、模板 `.cls` 探测、源码版模板提示失去输入 | **㉖：Tectonic 两形态都不经 latexmk ⇒ ㉖ 对 Tectonic 档不适用**（㉖ 的正身 = 真实论文模板 hithesis 的自带 latexmkrc × `-outdir=tmp` 约定，`docs/modules.md:825`；「不经 latexmk」的落点在 `crates/latteset-infra/src/runner.rs:365-368` 的 Full 分支与 `crates/latteset-core/src/types.rs:36-43`，另见 `docs/research/tex-ide-roadmap-priority.md:61,198`）。**㉒**（生成产物不当源码打开，`docs/modules.md:860`）= 纯逻辑、引擎无关 ⇒ **不改**。**㉗**（源码版模板提示）= 只读项目根 ⇒ **引擎无关**。**遗留的下游症状**（子目录 `\include` 的中间文件/搜索根）不随 ㉖ 一起消失，它正是 `tectonic-test-plan.md` 的最高风险未验证项 **U-14** | `docs/design.md:196`（**该行对 ㉒ 的归因是错的，勿照抄**）；t1 §7.2-1；`docs/modules.md:825,860`；`runner.rs:365-368`；`roadmap:61,198`；`tectonic-test-plan.md:896`（U-14） |
| LBR-6b | **`had_aux` 判据的归属**（原表误挂在 ㉖ 名下，F-06） | Quick 升级判据 `tmp/<stem>.aux` 失去磁盘载体 | 它属 **㉘（编辑期单趟）**，不是 ㉖：`runner.rs:354-359`；路径 B 下改读我们 I/O 层的文件表（§5.4） | `crates/latteset-infra/src/runner.rs:354-359` |
| LBR-7 | **内存占用与多窗口并发** | 库形态把引擎搬进宿主进程，峰值工作集不可控；全局互斥锁下无并行收益 | `[待 t7 回填]` 内存与并发实测；失败则库形态只做单实例 | t2 §7.4 第 5 条（`[推断]`） |
| LBR-8 | **上游 API 变更** | 0.x 子版本不保证兼容（今日名字已与 0.13/0.14 不同） | 钉 `Cargo.lock`；用 0.17 名字（`IoProvider` 无 `create_output`/`output_file_names`/`output_open`） | t2 §4.1/§1.3/§9.2 |
| LBR-9 | **`-C` 无条件（E-1）** | 全新机器拿不到 bundle；库形态不解决 | P0 必须先行；未落地前 t9/t10 不得开工 | `runner.rs:317`；`tectonic-test-plan.md:130,200` |
| LBR-10 | **形态位破坏设置兼容** | 旧版读新设置整份重置（超时/根文件被抹） | 形态位用可选附加字段；`Engine` 变体集合不动 | `crates/latteset-infra/src/storage.rs:41-66`；t1 §4.6 |
| LBR-11 | **取消语义退化（X-3）** | C 代码 abort 无法树杀；取消延迟不可控 | LIB-9 量化取消延迟；ADR 记录隔离能力下降 | t1 §4.3；`runner.rs:770-793` |
| LBR-12 | **「免装 TeX Live」价值主张被误挂到库形态** | 归因错误：真正卡住的是 E-1 与 ENV-B 未测 | §0 明确解耦；U-28 单列 | t1 C-01；`tectonic-test-plan.md:910` |
| LBR-13 | **`.fmt` 落进用户项目目录**（HL-11） | 24,451,466 B 的 `.fmt` 出现在用户项目里；违反 D7「文件系统为内容真相源」精神 | §5.2 硬约束 + LIB-11 断言（反例自证） | t3 §6.4/HL-11 |
| LBR-14 | **缓存无进程间锁 + 同 pid 临时名冲突**（HL-8） | 多窗口/多任务并发写同一缓存时可能互相覆盖或重复下载 | 产品侧按缓存目录串行化或每任务独立缓存目录；LIB-13 | t3 §6.2 |
| LBR-15 | **路径 B = 自研驱动取向** ⇒ 与 ADR-0005「不重造引擎驱动」冲突 | 路径 B 要自实现 format 生成 / 多趟收敛 / 产物落盘 / SyncTeX，工作量与维护面大幅上升 | **登记为决策门 §4.7**（批准人 = 产品负责人，触发 = 进入 t9 且采用路径 B）；不批准则整体退回**路径 A**（放弃逐页事件，推荐档缩为 J3 转换段 + bundle/缓存隔离）；并入 §4.4 的 ADR-A | `docs/adr/0005-latexmk-first-incremental-next.md:3,5,7`；t3 §4.4(a)/HL-2 |
| LBR-16 | **对象级常驻在路径 A 下不可能**（HL-10） | 若按"format 常驻内存"承诺 J1 收益，在路径 A 下会落空 | §5.2 已按两条路径分别写：A 下不可能、**B 下可能但 `[推断]` 未实测**；P4 目标同步改写；真实收益交 t7 | t3 §5.2 |
| LBR-17 | **路径 A 下非主文件不能直喂**（HL-1） | A1 的价值**只对根文件成立**；章文件/宏包仍走磁盘。（路径 B 下由我们自己的 `IoProvider` 决定输入来源，可绕开该限制，但 A1 本就被 N4 排除） | §2.2 A1 已标注 | t3 §2.4/HL-1 |
| LBR-18 | **feature 门控不免除依赖解析，也不免除成员构建** | ① 清单里有 tectonic 可选依赖 ⇒ **任何**构建都要能解析整棵图（缓存未预热即失败）；② 若不加 `default-members`，**根 `cargo build` 会连新 crate 一起构建**（需要 vcpkg） | §3.2 的落点裁决 + `default-members`（队长决定 3）；LIB-15 判据 | t4 §9；根 `Cargo.toml:3` |
| LBR-19 | **原生探测是硬失败**：graphite2/ICU/freetype2/fontconfig/libpng 永远外部探测，失败即 build script **panic** | 开发机/CI 少一个 port 就整仓构建失败；Windows 必须维护 vcpkg + triplet | P1 前置；CI 增量 +15–20 min/冷 `[推断]` | t4 §4/§7 |
| LBR-20 | **构建代价尚未实测**（t4 三项未测到） | 「值不值得做」在构建侧**不能终判**（t4 §8 结论 3） | §3.1.3 如实记 `[未测到]`；P1 验收判据要求补测后有数字 | t4 §5.2/§8 |

---

## §13 证据、判决与剩余未实测清单

| 项 | 状态 |
|---|---|
| §5 bundle/缓存口径（含 bundle 常驻、无锁、`format_cache_path`、本地 bundle 五坑） | **已回填（t3 + t5）** |
| §4 ADR 登记（t15 + round-3 收口） | **结论已在本文件作出、不推给 T12**：**ADR-A（新立，编号 0012）**= 新 crate 落点偏离（X-5）+ 受控 I/O 例外（X-1..X-3、X-5）+ **§4.7 的 ADR-0005 决策门**；**ADR-B** = HL-4（并入 ADR-A 作未决小节）。§4.3 的确定性"三选一"**已撤回、不需 ADR**；**T12 只负责把结论写成文件** |
| round-3 改动记录（对 660 行快照的那批评审） | ① 风险前缀 `R-*` → **`LBR-*`**（§12 + §7 抬头 + §11 Z-7 引用同步，彻底与 test-plan §6.1 的 `R-1..R-14` 区分）；② P5 新增**提前量阈值处置**行（保留 LIB-3d、本轮不判 + 理由）；③ P4 新增**复核命令可用性**行（脚本由 t7 交付，未交付前 P4 不得开工）；④ P0 交付物/验收判据点名 `runner.rs:891`/`:909`；⑤ §4.6/§4.4 各给人 ADR 结论（不再"推给 T12"）；⑥ LBR-6 补 `roadmap:61,198` + `runner.rs:365-368` 出处并按「两形态都不经 latexmk ⇒ ㉖ 不适用」改写；⑦ W-4 标明"与构建无关，仅供许可清单"；⑧ **P1 入口条件与 §6 DAG 消歧（round-3 repair）**：P1 的**落盘动作**（新增 `crates/latteset-tectonic/`、根 `Cargo.toml` 与 `default-members`、提交 `Cargo.lock`）属 t9 的 inScope ⇒ **受 P0 闸门约束（t14 未落地前不得落盘）**，仅其构建补测可与 P0 并行准备 —— 消除「t14 未落地前库形态不得开工」与旧文「P1 与 t14 无依赖关系」之间的两种读法 |
| §3.2 落点裁决与配套改动（t15） | 落点 = `crates/latteset-tectonic/`（workspace member；`src-tauri` optional dep + feature；`latteset-infra` 不依赖）；**配套 = 根 `Cargo.toml` 加 `default-members` 排除本 crate 的默认构建（t9 落地）** |
| §7 门禁 | **LIB-1..LIB-16**（含 LIB-3b/3c/**3d**）；LIB-15 已按队长决定 3 **降级**为"解析层面零成本，成员构建需 `default-members` 配合" |
| §6 P4–P6 的**准入判决** | **已回填（t5）**：J3 整份成立/前缀不成立；判据② 成立（含三类坑）；判据③ 官方 API 不成立、自持输出层成立；**J1 仍无法判定（t5 未测，闸门 = t7）** |
| §8.2 判据拆分数字 | **已回填（t5）**；「相对子进程对比列」`[待 t7]` |
| §3.1 两形态依赖闭合 / 冲突面 / C 链 / CI 参照 / 体积锚点 / 折中建议 | **已回填（t4）**；**三项构建数字（冷热墙钟/峰值内存/体积增量）仍是 `[未测到]`**，补测入口 = `build-spike/run-samples.ps1` |
| t5 未实测项（**不得当成立**，t5 §6） | U1 `ProcessingSession` 全管线"TeX 源码内存输入→内存输出"；U2 排版段字节 ≤16 KB 粒度直接观测；U3 空缓存+有网的真实 bundle 下载行为与文案；U4 转换段"逐页 PDF 可渲染"；U5 内存层 `.log`/`.synctex.gz` 的可用性 |
| t3 的 `driver.rs` 行号偏差（**已知复审项**） | 本文件已改为**符号名引用**（§1 抬头的行号纪律）；t5 用的 `src/driver.rs:1755` 等其他行号同样按符号名复核 |
| §3.1 子 crate 发布态逐条核实（t2 U1） | **部分**：t4 §3.3 核了主 crate 的 crates.io 元数据；其余子 crate 仍未逐个 API 核 |
| MSVC + `crt-static` 下 `links` 冲突是否成立（t2 U12） | `[待补测]`（t4 §5.2：构建从未开始） |
| §11 复核脚本入仓（`run-samples.ps1` / `dep-closure.py` / `conflict-check.py`，以及 §6 计划新增的 `bench-tectonic-build.ps1` / `bench-tectonic-lib.mjs` / `tectonic-lib-xdvscan.mjs` / `tectonic-lib-xdv2pdf.mjs`） | **部分已落地（2026-09-15）**：`scripts/validate-pdf.mjs`（P2/P3 入口，本轮新增）、`scripts/bench-tectonic-lib.mjs`（P4）、`scripts/tectonic-lib-xdvscan.mjs`（P5）**已入仓**；**仍缺** `bench-tectonic-build.ps1`（P1，实现现在 `test_file/research-tectonic-lib/build-spike/run-samples.ps1`）与 `tectonic-lib-xdv2pdf.mjs`（P6）——P1 的构建补测受外部前置阻塞（见 §3.1.3），P6 的等价能力已由 P2 的整链路覆盖 |
| §10 许可分布 | `[未定]`（t2 §2.3；不得写成结论） |
| T13 收口项 | `docs/README.md` 索引行；`roadmap §6.5` 判据状态（含 J1 未判定、判据③ 改口径）；`tectonic-test-plan.md` 增补 `LIB-1..LIB-16`；`docs/design.md:196` 的 ㉒ 归因更正；`docs/modules.md §2.6/§12.1` 补 Tectonic 条目（`docs/modules.md:243-266`、`:816-848` 目前**没有** Tectonic 条目）；**落 ADR-A（0012）文件** |
