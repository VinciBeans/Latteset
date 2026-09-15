# Tectonic 库形态：选定路径 B（自驱引擎 + 自持输出层）

**Tectonic 既有的「子进程形态」已落，但库形态（把 `tectonic` 当 crate 嵌进产品）能多拿到的能力，只有绕过高层 `ProcessingSession`、自己驱动三个引擎并自持 I/O 输出层才拿得到。** 本 ADR 记录：产品负责人批准采用**路径 B**，以及由此产生的对 ADR-0010 的结构性偏离（例外 X-5）。

> 依据文档：[Tectonic 库形态集成方案](../tectonic-library-plan.md)（705 行 / sha256 `57C39565…`，已通过 t18 评审）§3.2/§3.4/§4.2/§4.4/§4.7/§8；实测分册 `test_file/research-tectonic-lib/` 下的 `engine-spike.md`（t5）、`io-determinism.md`（t3）、`runtime-compare.md`（t7）、`build-spike.md`（t4）。

## 背景

**形态 B（子进程，已落）**：`Engine::Tectonic` 驱动官方 `tectonic.exe`，自带 bundle、不读用户 TeX Live，Quick 单趟出 PDF（快速路径实测 0.88–1.07 s 对 xelatex 两段 ≈1.96 s）。它的价值主张与库形态**解耦**：冷缓存 + 无条件 `-C` 导致全新机器拿不到 bundle 这件事，是形态 B 自身的缺陷，已另立任务修复，本 ADR 不涉及。

**形态 A（库内嵌）的信息增量**（roadmap §6.5）：① `IoProvider` 接管 IO；② 常驻 format / 省进程启动；③ `XdvEvents` 页事件与逐字形坐标；④ 三个 pass 自己编排、产物全程在内存；⑤ 结构化状态与错误。

**但高层路径拿不到其中两项**（t3 结论，`[本地源码]`）：

- `ProcessingSessionBuilder`（0.17）**没有**注入自定义 `IoProvider` 的入口——只有 `filesystem_root` / `output_dir` / `do_not_write_output_files` / `format_cache_path`（`test_file/tectonic-src/src/driver.rs:829-1112`）；
- 高层路径的能力上限 = 内存输入（**仅主文件**）+ 内存产物 + `StatusBackend`，**没有**编译中的逐页事件。

**实测判决**（t5 / t7）：

| 判据 | 结果 | 依据 |
|---|---|---|
| 内存 XDV → PDF（整份） | **成立** | 243,012 B XDV → 74,389 B PDF（26 页），与 CLI 页数/文本字符/全文文本 SHA256 前缀全等（t5 §2.2） |
| 内存 XDV **前缀** → PDF | **不成立** | 三种截断点一律 exit=1 且 0 字节产出（t5 §2.3） |
| 自定义 I/O 与 bundle/缓存隔离 | **成立** | 内存↔CLI 等价；`file://` 本地只读目录 bundle 可离线跑通（t5 §3） |
| 编译中逐页事件 | **官方 API 不成立；自建输出层成立** | 自建输出层 + `XdvParser::parse`：16 KB→4 页、243 KB→26 页（t5 §4.2） |
| 库形态对击键路径的延迟改善 | **无可证实改善** | PDF 后端腿：库形态 307 ms（常驻）/390 ms（新进程） vs CLI 同段 279 ms（t7） |
| 常驻复用收益 | **实测为正但幅度有限** | 同进程 3 次比 3 个新进程 −19%；同进程内复用 bundle 比每轮重开 −26%（约 −109 ms/次）；折算单次暖编 6–9%（t7） |
| 常驻 **format** 收益（核心卖点） | **未测到** | 需 `tectonic_engine_xetex` 全管线，本机缺 pkg-config/vcpkg（t7、t4 同因） |

⇒ 库形态的价值目前**由能力面（逐页事件 / I/O 接管 / 产物在内存）支撑，不是由已证实的延迟收益支撑**。这是一次取向选择，需要显式裁决与记录。

## 决策

**选定路径 B：自驱引擎 + 自持输出层。**

调用序列（方案 §3.4，九步）的要点：建 `CoreBridgeLauncher` + 自实现 `IoProvider` → `tectonic_bundles::detect_bundle` 取 bundle → **format 自管**（`input_open_format` / `write_format`）→ `TexEngine::process` 排版 → 中间产物留在我们 I/O 层 → `BibtexEngine::process`（需要时）→ `XdvipdfmxEngine::process` 转换 → 页事件把写句柄 tee 给 `XdvParser::parse(chunk)` → 收尾原子拷贝。

- **`ProcessingSession` 只作能力上限对照**，不作实现路径：它既不能注入 `IoProvider`，也没有逐页事件（见「背景」）。
- **规则落在新 crate `crates/latteset-tectonic/`**：workspace 成员；`src-tauri` 以 **optional 依赖 + feature `tectonic-lib`**（默认关）依赖它；**`latteset-infra` 不依赖它**；`CompileRunner` 的实现 `TectonicLibRunner` 与 `LatexmkRunner` 同级但不同 crate。
- **受根 `Cargo.toml` 的 `default-members` 约束**：当前根 `Cargo.toml` 只有 `members`、没有 `default-members` ⇒ workspace 成员会被根 `cargo build`（不带 `-p`）**默认构建**，optional 依赖 + feature **挡不住**（feature 只影响依赖解析）。因此必须同时把 `default-members` 列为 `src-tauri` / `crates/latteset-core` / `crates/latteset-infra` / `crates/latteset-server`，**把 `crates/latteset-tectonic` 排除在默认构建之外**。
- **批准事实**：**产品负责人于 2026-09-15 批准路径 B**（方案 §4.7 决策门据此闭合：批准人 = 产品负责人；触发条件 = 方案进入 t9 且采用路径 B；批准内容 = 明确接受「自研驱动（Tectonic 版）」这一取向偏离及其维护面）。
- **与 ADR-0005 的关系**：**不修改 ADR-0005**。ADR-0005 自己写明「性能瓶颈出现时以预算为标尺评估是否自研驱动」——本次正是**触发该保留条款**并获批准，属按其既有入口走，不是推翻它（方案 §4.4/§4.7）。
- **不批准时的退路**：退回**路径 A（`ProcessingSession`）**：放弃逐页事件与 `IoProvider` 级 I/O 接管，推荐档缩为「J3 整份 XDV→内存 PDF + bundle/缓存隔离」——这两项均已由 t5 实测成立，因此降级**不会**让该里程碑失去可交付成果。

## 对 ADR-0010 的结构性偏离与例外 X-5

ADR-0010 的纪律是「**infra 是文件系统与进程的唯一落点**；上层（src-tauri）不出现 `tokio::fs` / `std::fs` / `std::process`」。本决策在该纪律上开一个**范围明确的例外**：

- **偏离内容（X-5）**：新 crate `crates/latteset-tectonic/` 成为**外部依赖 / 原生链（C 链）的第二个落点**。
- **偏离范围的边界（必须保持）**：
  - 该例外**只覆盖这一个 crate 的外部依赖与 C 链**；
  - **`latteset-infra` 仍是默认依赖图中外部依赖、文件系统与进程的唯一落点**（`latteset-infra` **不依赖** `latteset-tectonic`）；
  - **上层（src-tauri）仍不出现 `std::fs` / `std::process`**：装配点仍是 `src-tauri/src/lib.rs:83-84` 那一行 trait 注入，且该层不出现 `Engine`；
  - 文件读写仍经 core 的 `FileSystem` trait——库形态的自实现 `IoProvider` 把它代理过去，而不是绕开它。
- **为什么不能放进 infra**：放进去会让**整仓默认构建**永远需要 vcpkg / 原生链（t4 §9 实测：即便关闭 feature，cargo 仍要解析整棵含可选依赖的依赖图）；配合 `default-members` 才能让默认构建保持零原生依赖。
- **同一份登记里的其余受控例外**（口径 (b)：受控例外并记 ADR）：
  - **X-1**：C 库（fontconfig / freetype2 / harfbuzz / graphite2 / ICU）自行读取字体与 config，**无法**经我们的 `FileSystem`；
  - **X-2**：`tectonic_bundles` 默认带 `geturl-reqwest` 网络栈（`ItarBundle` 走 HTTP Range）——须 `default-features = false`，或显式登记「bundle 获取是 crate 内部行为」；
  - **X-3**：C 代码的 abort/panic **无法被「树杀」**，取消只能做到「下一个 IoProvider 回调返回错误」⇒ 取消延迟须量化并写进 UI 语义（D1「失败可见」）；
  - ~~X-4（环境变量是进程级、无法分步施加）~~ **已证伪**：TeX 步与转换步都有非 env 的显式注入点，该「三选一」撤回，改为两条纪律——库内**禁止** `build_date_from_env`、**必须**显式 `build_date(SystemTime::now())`（默认值是 `UNIX_EPOCH`，忘写即把「`\today` 印 1970」变成默认行为）。
- **未做决策的小节（HL-4）**：PDF **加密**路径的 C 层 `getenv("SOURCE_DATE_EPOCH")` 无 API 可注入/屏蔽 ⇒ 可选「① 不支持加密 PDF 并报错/警告 ② 该路径回子进程 ③ 接受非确定性并登记」。**此项未定，不是结论。**

## 后果

**正向**：

- 拿到**编译中页事件**：自建输出层 + `XdvParser::parse`，粒度为 **≤16 KB 的字节块**（滞后 ≤ 一块，**尾页可能迟到最末**）——这是官方 CLI 与高层 `ProcessingSession` 都拿不到的能力（t5 §4.2；t3 §9.3bis）。
- 拿到 **I/O 级接管**与**产物在内存**：输入输出经我们的 `IoProvider`，中间产物落点由我们决定；J3（整份 XDV → 同进程内存 PDF）与 bundle/缓存离线隔离**已实测成立**（t5 §2.2/§3）。

**代价（如实记录，含未测项）**：

- **拖整条 C 链**：引引擎 crate ⇒ freetype2 / graphite2 / harfbuzz / ICU / fontconfig / libpng；其中 **graphite2 / ICU / freetype2 / fontconfig / libpng 永远走外部探测**（探测失败 = build script panic），harfbuzz 随 crates.io 发布版内含源码。**两种引入形态的 native 标记同为 21 个** ⇒ 不存在「更轻的引入方式」（t4 §3.1/§4）。
- **Windows 构建路线被限定**：只能走 `vcpkg + x64-windows-static-release + RUSTFLAGS=-Ctarget-feature=+crt-static`（官方口径），开发机/CI 需备 vcpkg（含 cmake / pkg-config / nasm）。
- **分发体积锚点**：官方 Windows/MSVC 0.17.0 **静态单文件 51,538,432 B（49.1 MB，无 DLL）**——这是「随包 exe」形态的数字；**库形态自身的体积增量 `[未测到]`**（"数十 MB"只是 `[推断]`）。
- **CI 代价**：稳态增量粗估 **+15–20 min/冷、+2–5 min/热**（`[推断]`，参照上游同构 job 19m05s，缓存命中）；**冷/热构建墙钟、峰值内存、体积增量三项均 `[未测到]`**（本机构建从未开始，补测入口见 `build-spike.md` §11）。
- **`default-members` 调整**：根 `Cargo.toml` 必须同步改，否则该 crate 被默认构建（本 ADR 已定为硬前置）。
- **自实现面上升**：format 生成、多趟收敛判定、产物落盘、SyncTeX 路径、bib 编排都由我们承担（即 `ProcessingSession` 内部 `driver.rs` 的那套逻辑）；与 ADR-0005「不重造引擎驱动」的取向冲突由本决策显式接受。
- **收益面仍未证实**：库形态对**击键路径**的延迟**无可证实改善**（PDF 后端腿 307 vs 279 ms）；常驻复用实测 −19%…−26%（PDF 腿口径，约 76–109 ms/次，折算单次暖编 6–9%）；**常驻 format 收益 `[未测到]`**（需 xetex 全管线）。⇒ 不得对外承诺「库形态更快」。
- **其余仍开放的风险**：缓存目录无进程间锁（同 pid 临时名冲突）；format 落点若写错会把 24 MB `.fmt` 扔进用户项目；取消语义退化（X-3）；上游 0.x API 变更须锁 `Cargo.lock`。

## 替代方案

- **路径 A（高层 `ProcessingSession`）——已评估、未选**。否决理由：① 它没有 `IoProvider` 注入入口，拿不到 I/O 级接管（且"注入 IoProvider 到 session"这一说法在 0.17 不成立）；② 它没有编译中的页事件；③ 其全管线（TeX 源码内存输入 → 内存输出）**未实测**。保留为**不批准时的退路**（见「决策」）。
- **维持形态 B（子进程）**——不采用，但**保留且不替换**：官方 MSVC 二进制只能当子进程用，且这是 TL-less 与分发的既成出路。库形态失败**不自动回退**（D1），回退是用户的显式选择。
- **把库形态代码直接放进 `crates/latteset-infra/`（方案 §3.2 候选③）**——否决：会让整仓默认构建永远需要 vcpkg / 原生链。
- **独立 workspace 成员但主 workspace 完全不依赖（候选②）**——否决：`src-tauri` 因此**没有任何装配通道**（本项目不引入运行时插件机制）。
- **不做库形态，只共享 bundle 与缓存（t3 §4.4 口径 c）**——零结构风险，但拿不到本 ADR「正向」一节的全部能力；不在本决策范围内，保留为纯分发优化选项。

- **状态**：已接受（2026-09-15；产品负责人批准路径 B，本文件为该批准的正式记录）
- **影响**：新增 crate 与 `default-members` 改动；`src-tauri` 多一个默认关闭的 feature；CI 与构建环境要求上升（vcpkg）；既有子进程路径与 ADR-0005 的决策**均不改动**，库形态与子进程形态并存由设置面选择。

## 补充：库形态是**正式的构建变体**，不是实验开关（2026-09-15）

原「影响」一节只说了"多一个默认关闭的 feature"，容易被读成"藏在特性后面的实验通道"。实际定位：

- **开发默认构建不含它** —— 主产物保持零原生依赖、不需要 vcpkg。这条**不变**（否则整仓默认构建、含 Linux 上的 `check` 档，都会被迫装 C 链）。
- **发布构建含它** —— `npm run lib:build`（等价 `tauri build --features tectonic-lib`），CI 由按 tag/手动触发的 `build-windows-tectonic-lib` 档守着。**用户装到的包两种形态都能选**，设置面板里的「库内嵌」不是灰的。
- **构建前置有唯一落点** —— `scripts/with-tectonic-lib.ps1`：校验 vcpkg 与 triplet（缺项给可执行修复命令）→ 设环境变量 → 跑命令。此前这套环境散在文档与记忆里，缺一项的失败信息不指向环境（实测：`pkg-config` 缺失会以 `tectonic_bridge_png` 的构建错误出现）。
- **形态的默认值不变** —— 仍是**子进程**。把默认切到库内嵌是产品语义变更（库形态不跑外部进程 ⇒ biber/makeindex/glossaries 文档会失败），需要单独裁决，不在本文范围。
