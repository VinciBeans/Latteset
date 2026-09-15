# Tectonic 测试方案（接入前门禁 · 引擎/集成/分发矩阵 · 测量口径 · 上线判据）

> 团队 `tectonic-test-plan` · **T5 汇总与红队评审** · 2026-09-14 · **本文只出方案，不改产品代码。**
>
> **四份分册（逐条明细附录，已复制入库）**：引擎层 [`engine.md`](./tectonic-plan-parts/engine.md)（E1–E7，**56 条** / G1–G6 六个门禁）、集成层 [`integration.md`](./tectonic-plan-parts/integration.md)（**Rev.10**，INT-10..INT-96，**88 条**）、测量 [`perf.md`](./tectonic-plan-parts/perf.md)（Rev.4，R*/M*/H_*/U1–U12）、分发与许可 [`dist.md`](./tectonic-plan-parts/dist.md)（DIST-*/MB-*/G-L*/F1–F8）。
> **版本冻结（合并基线 = 最终值，引用前请核对）**
> **复制时刻：2026-09-14 16:47:28**（`test_file/tectonic-plan-parts/` → `docs/research/tectonic-plan-parts/`；该时刻**四对文件已逐一复核 SHA-256 相等**）。**入库物以 docs 侧为准**（lab 源 `test_file/**` 被 gitignore，不进仓库）。
>
> | 分册 | 行 | 字节 | SHA-256（全量） |
> |---|---:|---:|---|
> | [`engine.md`](./tectonic-plan-parts/engine.md) | **285** | **77,491** | `B13CCA1006A2BE9666D940CA1212CEB6E23514EEAA18FE873C249B9D0F83852D` |
> | [`integration.md`](./tectonic-plan-parts/integration.md) | **460** | **105,367** | `5A67A5241BD3CD0546992922155E9544E7B07A8F3757C35F10F53CB276C4DF74` |
> | [`perf.md`](./tectonic-plan-parts/perf.md) | 383 | 63,356 | `F68A711E6278ABCAF9862591B66B06DE80C444DC93D44D0BAEE93320CAD35F3D` |
> | [`dist.md`](./tectonic-plan-parts/dist.md) | 340 | 51,550 | `816EC45673CA520F5BB55B900E17BB70C79FF44B41D03C39660198BAD040158B` |
>
> 分册内容：engine = E1–E7（**56 条**）+ G1–G6（含 E3.7–E3.10）；integration = **Rev.10**（88 条，反例 27，含 INT-54b）；perf = Rev.4；dist = 分发/缓存/许可。**分册可能继续修订**；本文与分册冲突处一律以本文 §8 裁决台账为准。
> **冲突处理**：分册之间、以及分册与 `docs/research/tectonic-integration-plan.md` 之间的冲突，**一律以本文 §8 裁决台账为准**；附录原文保留（它们是逐条证据与出处，不重写）。
>
> **已有实测数字的来源**：`docs/research/modern-engines-zh.md` §2/§7.4/§7.7/§8.2/§8.3/§8.4、`docs/design.md`（基准表、延迟预算、失败语义）、`docs/research/incremental-edit-x-dvi.md` §2.1–2.3、`docs/research/tectonic-integration-plan.md`、`docs/modules.md` §12，以及四份分册本轮实读的工件（`test_file/projects/bench/_zhcmp/**`）与队长 2026-09-14 的干净目录落盘实测。
>
> **标记约定**：`[实测]` 有命令与输出/工件；`[源码]` `file:line`；`[推断]` 由源码或机制外推、**未跑**；`[未测]` 无任何证据。**本文不虚构任何未实测的数字**——凡无实测来源者一律标 `[推断]`/`[未测]` 并进 §6.2。

---

## 0. 目的、判据规范与非目标

### 0.1 目的与判据规范

**目的**：把"要不要接 Tectonic、怎么接、接完怎么验、什么时候能发"变成一组**可判定**的条目，供接入实现、回归、发布三类角色直接执行。

每条目的写法固定：**ID / 目的 / 操作（可跑的命令或入口）/ 期望 / 判据（二值 + 反例自证）/ 优先级 / 证据·出处**。判据必须满足三条：

1. **可二值判定**：能写成"计数 == N""文件存在/不存在""SHA-256 相等/不等""事件序列单调"这类断言；
2. **带反例自证**：凡"改了 A 就应该挂"的判据，必须先证明它**在旧行为下会失败**（否则是恒绿用例）；
3. **有有效样本定义**：判据只在**通过逐次校验（§3.2 V1–V6）**的样本上判；无效样本不许静默丢弃。

4. **引擎自述 ≠ 事实（方法学纪律，G6）**：`.log`/`.blg` 里写的**任何**内容都不得单独作为判据 —— 旗舰反例：PDF 档的 `.log` 写 `Output written on main.xdv` 而该文件根本不存在（`driver.rs:1985`）；`.blg` 的收尾统计块在 Tectonic 的 BibTeX 端口里**未实现**。判据一律落在**产物**（`.xdv`/`.pdf`/`.aux`/`.bbl` 的存在与内容、页数、SHA-256、事件序列）。因此 **E4.5 的 `Output:` 字段一律只"记录"、不作判据**（它决定 `synctex view -o` 传什么，属排障信息）。

**优先级**：`P0` = 上线前必须（不过就是用户可见错误或静默退化）；`P1` = 可后补但必须记录。

5. **固定 `SOURCE_DATE_EPOCH`（按子进程）**：比较产物或页哈希时必须固定，否则 `\today` 出现在首屏 ⇒ 第 1 页每天都变 ⇒ 得到**假的"第 1 页变了"**（反例：同一源隔天跑两次，`changed_pages` 里出现第 1 页）。**施加方式已裁决（D4=(b)）**：**Tectonic 主进程不设**、**外部转换步设 `0`**（E3.10）⇒ **XDV 与转换步产物可断言逐字节稳定，PDF 档不得断言逐字节相等**。

6. **页哈希口径必须声明（红队新增，机制已核实 = T1 E3.8 / 已知债 #26）**：DVI/XDV 每页头 `bop`（45 B）的**最后 4 B 是"前一页 `bop` 的绝对字节偏移"（`prev`）** ⇒ **前面某页变长，会让其后所有页的 `prev` 变化**，从而"页字节不同"。**逐页字节复核（T1）**：仅 epoch 不同时（日期串差一位）**页 1 变长 10 B**、**页 2 逐字节相同**、**页 3–26 长度全同、差异只落在页内偏移 43–44**（= `prev` 低字节，`prevB − prevA = +10`）⇒ **"25/26 页被判变化"是 `prev` 绝对偏移连锁，不是"每页内容都变了"**。本仓当前实现 `crates/latteset-core/src/xdv.rs:211` 是 `hash_page(&bytes[bop..p])` —— **含 `prev` 的原始口径**（候选修法 `bytes[bop+45..p]`，**收益/风险未量化 ⇒ 未修**）。⇒ 凡**跨档 / 跨引擎 / 跨次**比较页哈希的用例，**必须声明口径**：
   - **原始（含 `prev`）**：现状口径，反映"原始字节是否相同"；
   - **归一化（只哈希 `bop+45..eop`）**：反映"**该页内容**是否变化"，可把连锁收窄到真正变化的那一页。

   **这同时是 `docs/modules.md` 已登记的已知债 #26，不是 Tectonic 新问题**：差别只在**触发频率** —— XeLaTeX 侧固定 epoch、日期永不变 ⇒ 该债几乎不触发；Tectonic 选 D4 的 (b)（不固定 epoch）就把它变成"**每天发生一次**"。两种口径会给出**不同的"变化页集合"**；**不声明口径的页哈希判据不可复核**（H_C 在原始口径下会因首页变长而整体塌陷，却被误读成"内容大片变化"）。本文以纪律 + **P-G19** 登记；与 T1 的 **E3.8** 同源，合并时以 E3.8 的 ID 为准。

7. **输出路径：只对"转换产物（PDF）"成立的纪律**（别写宽了 —— T1 实测 + 队长独立复算）：
   - **转换产物（PDF）的字节级比较**必须**同时**满足：**① 固定完整输出路径（目录 + 文件名，逐字符相同）**；**② 固定 epoch（纪律 5）**。队长的完整样本（同一份 28 页 XDV、`epoch=0`）：**同路径两跑全等**（79,354 B / `9310317941DB7975`）；**同目录仅换等长文件名** `bb.pdf` ⇒ **不等**（79,348 B）；换更长名 ⇒ 不等（79,343 B）；**异目录同名** ⇒ 不等（79,336 B）；不设 epoch ⇒ 不等（79,346 B）。⇒ **该效应不由路径长度驱动**（等长名同样不等），**不能靠长度对齐绕过**。
   - **XDV：路径无关**（实测：同源 + `epoch=0`、输出到**目录名长度差 30+ 字符**的两个目录 ⇒ XDV **逐字节完全相同**，264,728 B / `738F96DD…71FEB`）⇒ XDV 比较**只需固定 epoch** —— **此结论只对 Tectonic 的 XDV 成立**（见下一条反例）。
   - **反例（2026-09-14：T2 实测 42/42 用例 + 队长复算原始字节）：TeX Live 的 `xelatex` 把本地墙钟写进 XDV 的 `pre` 注释**（` XeTeX output YYYY.MM.DD:HHMM`，注释长 29 字节；`pre` 区原始字节 `f7 07 01 83 92 c0 1c 3b 00 00 00 00 03 e8 1d 20 58 65 54 65 58 …`），**`SOURCE_DATE_EPOCH` 管不住它** ⇒ XeLaTeX 的 XDV **整文件 SHA 跨分钟变化**（同批用例整文件 SHA 各不相同，而「首 BOP→EOF」与逐页 raw/V1/V2 **全部相同**）。对照：**Tectonic 的 `pre` 注释是固定串 `tectonic`（注释长 8 字节：`… 03 e8 08 74 65 63 74 6f 6e 69 63 …`）**，其 XDV 无时钟派生字节（epoch 互换实验逐字节相同）。⇒ **凡「整文件 XDV 逐字节相等」类断言或整文件指纹式缓存，必须排除/归一化 `pre` 注释**；**页切片比较不受影响**，故页哈希判据（纪律 6 / P-G19）不因此改变。
   - 另：**`--synctex` 对 XDV 字节零影响**（同源两跑逐字节相同，242,960 B / `08E7219F…`；XDV 内不含源码绝对路径）⇒ **页哈希与 SyncTeX 互不污染、可同时开启**（INT-54b；T1 已撤回"synctex 会逐页加 special"的旧说法）。
   - 反例 **#27**（T2 编号）的适用范围**仅限转换步**，XDV 上复现不出来；且其现有"不同输出目录"的字面**比实测效应窄** —— **同目录换文件名（即使等长）同样不等**。

**ID 规则（沿用分册，避免映射噪音）**：`E*` 引擎层、`INT-*` 集成层、`DIST-*` 分发层、`R*/M*/H_*` 测量层、`MB-*/G-L*` 流水线与许可门禁；**本文新增** `P-G1..P-G19`（前置门禁，§2.1）、`U-1..U-31`（未验证清单，§6.2）、`C/DUP/GAP`（裁决台账，§8）。分册与本文的对应关系见 §9。

**判据纪律（P0，见 P-G13）**：凡使用 `scripts/xdv-report.mjs` 做页级差分的用例，**必须见到 `页级差分` 输出才算有效样本**；该工具已修（`--diff <f>` 与 `--diff=<f>` 两种形态都支持，漏值/多余位置参数一律 `exit 2`，见 `scripts/xdv-report.mjs:273-308`），修之前"空格形态静默不生效、用例恒绿"的坑不复存在，但**"必须见到差分行"这条纪律必须写进用例**。

### 0.2 非目标（本方案明确不覆盖什么）

| # | 不覆盖 | 理由 / 边界 |
|---|---|---|
| N1 | **不改任何产品代码** | 本文所有"必须修"只写成门禁与判据（§2.1 P-G*），落地是后续任务；本轮不新增/修改 `src-tauri`、`crates/**`、`src/**` |
| N2 | **不给法务结论** | 许可只给审计清单、门禁（§4.3 G-L1..G-L9）与"待审计"标记；"能否闭源分发/是否必须改名"不由本文裁定 |
| N3 | **不真实建包** | 不下载 TL tarball、不跑 `bundle create`、不做 CI 接线；只给流水线步骤与门禁判据（MB-1..MB-8） |
| N4 | **不做形态 A（crate 内嵌 / vcpkg 静态构建）** | 只登记风险与触发条件（DIST-1.6）；Windows 官方构建链 = `vcpkg` + harfbuzz 子模块 + 30+ C 依赖 `[源码]` |
| N5 | **不覆盖不稳定 CLI 面** | `--outfmt html`（`.spx`）、`-X/nextonic` V2 子命令（**没有** `--synctex/--outfmt/-r`，不要走）、`-Z search-path`/`shell-escape` 等 `-Z` 项 |
| N6 | **不重测既有引擎的行为面** | XeLaTeX/LuaLaTeX/pdfLaTeX 只测"回归面"（默认引擎仍是 XeLaTeX、`npm run build`、`cargo test` 不回归） |
| N7 | **不决定产品决策本身** | 是否随包分发 exe/bundle/TL 工具、缓存目录归谁、首跑超时策略等 → 本文给**仍待决的槽位 D5–D9**（§5.4）与判据，不替产品选；D1–D4 **均已裁决** |
| N8 | **不承诺真实学位论文模板（hithesis 档）的数字** | 该档当前受阻（`I can't write on file 'body/introduction.aux'` → Emergency stop → latexmk 挂住，`modules.md §12.1 #6`），**排除在阈值判定之外**（§3.1.3） |
| N9 | **不覆盖 Windows 之外的平台** | 本机与全部实测都是 Windows x64（官方 exe 只有 MSVC 构建）；macOS/Linux 不在本轮 |

### 0.3 结论先行（12 条）

1. **P0 条目共 177 条**（去重口径，构成见 §2.0）；**P1 共 48 条**；另有 §2.5 一票否决 13 条、§6.1 风险 14 条、§6.2 未验证清单 31 条（其中 P0 **15** 条 —— 属"待跑视图"，与门禁条目同源，**不重复计入** 177）。
2. **三个最关键的前置门禁**：① **P-G1 落盘事实对拍与 D2 产品取舍**（决定"要 PDF 还是要页级复用"，见第 5 条）；② **P-G14 许可审计**（G-L1..G-L9 未全绿前不得把 Tectonic/bundle 放进发行物）；③ **P-G15 `-b <ttb> -C` 离线实测**（"免装 TeX Live"这个价值主张全押在它上面，冷缓存下 `-C` 单独并不等于离线）。次一级前置：P-G17 SyncTeX 往返、P-G5 实时反馈通道（日志适配）。
3. **速度不亏，但别按"更快"卖**：同批中位比值 `Tectonic 出 PDF ÷ 裸 xelatex 出 PDF` = **0.990**（thesis：2515 vs 2539 ms）/ **0.675**（min-zh：1586 vs 2350 ms）`[实测]` §8.3。阈值形态**固定**为这个比值，**显式禁令**：不许中途把分母换成 `latexmk -xelatex` Full（分母变大 ⇒ 比值系统性变小，两组数字不可混用）。
4. **引擎层六个门禁 G1–G6**（全部有证据）：G1 编译中零实时反馈（产物写在内存层、进程结束才落盘）；G2 免装 TL 下 forward/inverse 不可用（Tectonic 无 synctex 解析器，我们起外部 `synctex.exe`）；G3 **bibtex 告警是"假警报"且无真信号**（8 条章级空跑警告每次成功构建都会出现；真信号只能从产物取）；G4 `Missing character` 无任何消费方（P1，但失败形态最坏：编译成功、预览静默缺字）；G5 **默认 PDF 档不落 `.xdv`**（机制 = `driver.rs:1985` 内存移除）；**G6 引擎自述 ≠ 事实**（`.log`/`.blg` 一律不得单独作判据）。
5. **D2 = ① 先落、② 记 P1**（人类已裁决）**，并已由实测 + 源码机制定位坐实**：干净目录各跑一次 —— 默认档：**无 `.xdv`**、无 `.aux`、`main.pdf` 80,203 B；加 `-k`：**仍无 `.xdv`**，只多出 `.aux`（443 B）与 `.bbl/.toc`。**机制**：`xdvipdfmx_pass` 结束时把 XDV 从**内存文件表**里移除（`driver.rs:1985`）⇒ `write_files` 无论 `keep_intermediates` 与否都看不到它。⇒ **要 PDF 就没有页哈希**（`pages == 0` ⇒ B/C 退化为全量刷新、A 标"该引擎不支持"）⇒ **① 是本迭代 P0**；**② = 收敛档 `--outfmt xdv`（28 页）+ 外部 `xdvipdfmx`**（实测：+617–1,091 ms/次、`pdftotext` 去空白后逐字符相同 13,559 字符）能恢复 B/C，但**它需要 TL 的转换器与字体树，不是"免装 TL 的出路"** ⇒ 记 **P1 独立分支**（五要素判据见 §1.2.2 与 INT-35/38/39；**两路线不得混测**）。
   另 **D4 已裁决 (b)（2026-09-14）**：Tectonic 档**不设** `SOURCE_DATE_EPOCH`（正文日期正确），外部转换步设 `0`；因此 **XDV 可断言逐字节稳定、PDF 不得断言逐字节相等**（§5.4 D4 / INT-20b / INT-36b / E3.10）。
6. **功能点 A 在 Tectonic 下无对应物**（内置 xdvipdfmx 与 TeX 同进程，没有可跳过的转换边界）⇒ "无需改代码"只能写成**只对页哈希解析（B/C 的解析代码）成立**；A 必须显式标"该引擎不支持"，否则误接会在 TL-less 机器上报 `无法启动 xdvipdfmx`（`runner.rs:538`）。
7. **`-k` 仍然必传，但理由不是 XDV**：为 `.aux`（Quick 的前置条件 `tmp/<stem>.aux`，`runner.rs:293-298`）与 `.log`（错误列表）；不传 ⇒ **Quick 恒被升级为 Full**（省 40.2% 中位的机制静默失效）+ 失败时拿不到错误线索。
8. **人类裁决按此写**：**D1** 失败**不自动回退**，失败必须可见，检测到本机有 TeX Live 时给「一键切到 XeLaTeX 重编」；**D3** 状态栏必须可见当前引擎（同处体现 Quick/Full 强度）。**全文不得出现"静默回退"**。
9. **集成方案三处原文必须改**：`-b <本地 .tar>` → **`-b <name>.ttb`**（0.17.0 只认 目录/`.zip`/`.ttb`，实测报 `doesn't specify a valid bundle.`）；`-C` ≠ "绝不联网"（冷缓存仍拉 `<url>.index.gz`）；`--outfmt xdv` 让"A/B/C 无需改代码"只在**能拿到 XDV 的档**成立，且该档不出 PDF。
10. **全部已有实测都在"装有 TeX Live 2026 的机器"上** ⇒ **TL-less 机器从未测过**；G2、B14/B15 的性能与文案、以及"免装 TL"这条价值主张全靠它（本文新增 ENV-B 环境，§1.3）。
11. **交付前必须先做的四件事**（§5.2）：许可审计（清单 A 纯本地可先做）→ `-b <ttb> -C` 离线实测 → SyncTeX 往返对拍 → 日志/实时反馈适配。
12. **harness 必须提升到 `scripts/`（tracked）**：`test_file/projects/` 与 `test_file/research/` 均被 `.gitignore` 覆盖（`.gitignore:66,69`），现有 `_zhcmp/*.ps1` 与 `bench-report.json` 清仓即失（§7）。

### 0.4 怎么用这份文档

| 角色 | 先看 | 再看 |
|---|---|---|
| 接入实现（写 `Engine::Tectonic` 分支） | §2.1 前置门禁（18 条 P0） | §1.2 四档命令表、§2.3 集成层矩阵、§5.2 里程碑 |
| 引擎行为验证 | §2.2 引擎层矩阵、§1.4 证据索引 | §3 测量协议（样本有效性） |
| 发布/分发 | §4 许可与分发门禁 | §5.4 待决槽位、§6.1 风险表 |
| 任何人（判"能不能发"） | §2.5 一票否决清单 | §4.3 流水线门禁、§8 裁决台账 |

---

## 1. 被测对象与运行环境

### 1.1 被测对象：版本"三件套" + 一个格式号

| 项 | 值 | 出处 |
|---|---|---|
| 引擎 exe | `tectonic 0.17.0`；官方 zip `21,060,223 B`（SHA-256 `f61ce51f…845f`）；**解包后 `tectonic.exe` `51,538,432 B`，SHA-256 `99ffcfdb…bf08d`** | T4 §1 B1 `[实测]` |
| bundle 规格 | `texlive2024-0312`：tarball `texlive-20240312-texmf.tar` sha256 `87f7f5fd…8d20`；`expected_hash=8fae742a…1312`（= sha256(content/FILELIST)） | T4 §1 B8/B11 `[源码]` |
| 默认 bundle 来源 | `https://relay.fullyjustified.net` → `<prefix>/default_bundle_v33.tar`（`FORMAT_SERIAL ≥ 32` 才带版本号）；**不是 ttb**（走 HTTP Range + `<url>.index.gz` 的 `ItarBundle`） | T4 §1 B3/B4 `[源码]` |
| 引擎格式号 | `tectonic_engine_xetex::FORMAT_SERIAL = 33` ⇒ 缓存文件名 `<bundle digest>-latex-33.fmt` | T4 §1 B2/B13 `[源码]`+`[实测]` |
| 缓存落点 | `%LOCALAPPDATA%\TectonicProject\Tectonic\cache\{bundles,formats}`；`TECTONIC_CACHE_DIR` 是**唯一运行期开关** | T4 §0 F5 `[实测]` |

> **P0 门禁（P-G18）**：**三件套必须钉成一处真相源**：`exe 0.17.0 ↔ bundle 规格/摘要 ↔ FORMAT_SERIAL=33/`.fmt` 后缀`。任何一处独立升级都会静默改变行为。判据：三者不一致时**拒绝启动编译**（不是 panic、不是静默跑），并在 UI 给出可操作错误（"引擎/bundle 版本不匹配，期望 …"）。

### 1.2 四种调用档（**唯一真相源**；其它章节只引用本表）

`cwd` = 项目根；`<P>` = 输出目录，**必须预先存在**（Tectonic 不建目录，📖 `compile.rs:163-171`）；`<stem>` = 根文件 stem。

| 档 | 命令 | 落盘产物 | 趟数 | 用途 / 页级复用 |
|---|---|---|---|---|
| **① FULL（默认路线，推荐）** | `tectonic [-C] -o tmp --synctex --keep-logs -k -p <rel>` | `tmp/<stem>.pdf`、`.log`、`.aux`、`.bbl`、`.toc`、`.synctex.gz`；**无 `.xdv`** `[实测]` | 3 趟 TeX + 9 次 BibTeX（thesis 夹具）`[实测]` | 首编 / 手动编译 / 空闲收敛；**`pages == 0` ⇒ B/C 退化为全量刷新、A 不支持** |
| **② QUICK = ① + `-r 0`** | `tectonic [-C] -o tmp --synctex --keep-logs -k -p -r 0 <rel>` | 同 ① | 1 趟 TeX（仍会跑 bibtex）`[源码]` `driver.rs:1704-1711` | 编辑触发；"落后一趟"语义见 INT-25 |
| **③ XDV 档（L2 诊断，不是 Quick）** | `tectonic -C --outfmt xdv --keep-logs -o <P> <rel>` | `<P>/<stem>.xdv`（28 页 `[实测]`）、`.log`；**无 `.pdf`**（`--outfmt xdv` 不跑内置 xdvipdfmx） | 3 趟 + 9 次 BibTeX `[实测]` | 页哈希口径 / 对拍；**不是 Quick**（照样 3 趟） |
| **④ TEX-PASS（真单趟）** | `tectonic -C --pass tex --keep-logs -o <P> <rel>` | `.xdv`〔源码推断，**待核 U-2**〕、`.log`；**无 `.pdf`、无 bibtex** | 1 趟 `[源码]` `driver.rs:1492-1503` | 与 `xelatex -no-pdf` 对拍（L2 成本诊断）；**不能当 Quick**（不出 PDF） |

**三条纪律**：

1. **`-k` 的理由**：`.aux`（Quick 前置）与 `.log`（错误列表），**不是** `.xdv`（已实测：加 `-k` 仍不落 `.xdv`，只多出 `.aux` 443 B 与 `.bbl/.toc`；原因是 `xdvipdfmx_pass` 把它从内存文件表移除，`driver.rs:1985`）。
2. **`-p/--print` 在 ①② 里是必带参数**（T2 Rev.2 口径）：它是**页进度的唯一来源**（`.log` 尾随命中 0 次、stdout 默认无 `[N]`）；代价是引擎 chatter（含 `!` 错误行）会走真 stdout，需与 `note:/warning:` 分层解析同时适配（E5.3/E5.5）。
3. **`-C` 只在"资源已就绪"形态下加**（预置缓存 / 本地 `.ttb`）；**首跑档不加 `-C`**（否则永远拿不到 bundle）——`-C` 与首跑互斥，这条必须写进命令构造的分支里，不能一律加上（见 C-7 与 DIST-2.12）。
4. **探针纪律**：任何"默认档是否落 XDV"的探针，必须用**不带 `-k` 也不带 `--outfmt`** 的默认档；**`-k` 不是"打开 XDV 的开关"**（实测已证伪该假设）。
5. **`--pass tex` 不得当 Quick**：不出 PDF ⇒ 预览长期停在旧图，用户以为编译成功（T2 §9 反例 #4）。

> ⚠️ **引用纪律（XDV 落盘 + bib 判据）**：**本节证据日期 2026-09-14；"XDV 能否落盘"与"bib 是否成功"两条已反转 3 次（一度也写错过机制）**。引用时请核对版本，验收线是：**产物级判据 + G6「引擎自述 ≠ 事实」**（`.bbl` 条目数 / PDF 文本 / `Citation … undefined` 计数；**不用** `.blg` 摘要行、不用日志里的告警计数）。

#### 1.2.2 D2 **产品取舍（人类已裁决 2026-09-14）：① 先落（P0）、② 记 P1 独立分支**

| 路线 | 形态 | 得到 | 放弃 | 前提 | 优先级 |
|---|---|---|---|---|---|
| **① PDF 档（默认，本迭代落地）** | `tectonic -C -o tmp --synctex --keep-logs -k -p <rel>`（Quick 追加 `-r 0`） | PDF + `.aux`/`.log`/`.bbl`/`.synctex.gz` 齐全；Quick 判定（`.aux` 存在）可用；runner 侧形态**与 LuaLaTeX 完全同类**（引擎自己出 PDF、不算页哈希、不调 `xdvipdfmx`） | **页级复用 B/C**（`page_hashes` 恒空 ⇒ 每次全量刷新）、A 标「该引擎不支持」 | 无（TL-less 也成立） | **P0** |
| **② XDV 档 + 外部转换（独立分支，P1）** | 收敛档 `tectonic -o tmp --outfmt xdv --keep-logs -k <rel>`（3 趟 + 9 BibTeX，**28 页**）→ **外部 `xdvipdfmx`（TL 自带）** → PDF + 页哈希 | 恢复 B/C（页级复用） | ① 的"免装 TeX Live"——**② 既需要 `xdvipdfmx.exe`、又需要 TL 的字体树（见下）**，与 G2（synctex 要 TL）同类；且该档仍 3 趟 + 9 次 BibTeX，**不是 Quick** | 本机装有 TeX Live（转换器与字体树都在 TL 里）；字体可被 kpathsea/TL 树解析 | **P1** |

**路线 ② 的定位（Rev.6 更正，勿再写成"免装 TL 的出路"）**：Tectonic 的 XDV 里字体记的是**裸名**（`FandolSong-Regular.otf`、`lmroman12-regular` —— 无路径，`lmroman` 连扩展名都没有），而 XeLaTeX 侧是**绝对路径** ⇒ 外部转换成功靠的是 **TeX Live 的字体树 + kpathsea**（实测三类字体名 + `kpsewhich` 全命中 TL 路径），且 **`xdvipdfmx.exe` 本身也来自 TL**。⇒ **② =「有 TL 机器上的可选转换路径」**，与 G2 同类；**不得**用它论证免装 TL（T1 E2.12/E2.13，P0）。

**路线 ② 的可行性已实测（2026-09-14，本机 / 既有产物 / 无下载无构建）**：`xdvipdfmx -q -o from-tectonic.pdf <tect-thesis>/main.xdv`（输入 **264,728 B**）→ **exit 0**、**1,091 ms**（T2 复跑 **617 ms**）、产出 **79,070 B / 28 页**、转换日志零 error/warning，`pdftotext` 抽样正常（`学位论文基准（合成）`、`Latteset`、`目录` …）；与 Tectonic 自带 PDF 对拍：**去空白后逐字符相同（各 13,559 字符）**、嵌入字体组一致（无替换字体），字节差异归因于转换器版本（`Producer: xdvipdfmx (0.1)` vs `(20260113)`）。
⇒ **它是"把既有机制换个引擎"**：与我们现在的 XeLaTeX Quick 路径**同构**（只排版 → `xdvipdfmx` 转换 → 页哈希）。**代价量化：外部转换 +617–1,091 ms/次**（Tectonic 内部转换是同进程、不额外计费），且该档仍 3 趟 + 9 次 BibTeX ⇒ 舍掉的是"① 的零额外转换"，换来的是 B/C。
> ⚠️ **单趟 vs 收敛不可混比**：`--pass tex`（1 趟）产出的 XDV 是 **26 页**，而收敛档是 **28 页** ⇒ **路线 ② 的转换输入必须来自收敛档**；`--pass tex` 只能作 L2 成本诊断，**不得**当转换输入（否则等于拿落后一趟的产物去转 PDF）。

**路线 ② 的分支判据（不是"未来可做"，而是可执行方案）**

| 要素 | 判据 |
|---|---|
| **触发条件** | 用户在设置里显式打开"页级复用（需本机 TeX Live）"；**默认关闭**（不是按机器能力自动开） |
| **前置探测（开不出就不许切）** | 探测 `xdvipdfmx` 可执行：存在 ⇒ 允许；不存在 ⇒ 开关**置灰并说明原因**（不得静默改用 ②，见 D1） |
| **命令构造** | ② 下 argv 必须出现 `--outfmt xdv`（**收敛档**，28 页）且**不得**与 ① 混用；两条路线的 argv 由**互斥**的分支产生（同一个 `Engine::Tectonic` 变体下的两条互斥形态）。**`--pass tex`（1 趟 / 26 页）不得作为转换输入**（单趟产物落后一趟）。**epoch 按步施加**（D4 落地建议 / U-31）：**Tectonic 进程不设** `SOURCE_DATE_EPOCH`、**外部 `xdvipdfmx` 进程设 `0`** ⇒ 理论上有"正文日期正确 + PDF 逐字节可复现"两全，**待实测** |
| **成功判据** | ① `.xdv` 落盘且 `xdv-report` 解出页数 == `pdfinfo` 页数（28）；② `page_hashes` 非空、`.pages` 行数 == 页数；③ **外部转换 exit 0**（实测形态：`-q -o <pdf> <xdv>`，28 页、零 error/warning）且与 Tectonic 自带 PDF 对拍 **`pdftotext` 去空白后逐字符相同（锚点：各 13,559 字符）**、嵌入字体组一致（**输出等价性只在同档内比**） |
| **失败判据** | `xdvipdfmx` 退出非 0 / PDF 缺失 ⇒ 必须**可见失败**（不得回退到 ①、不得复用旧 PDF、不得发 `pdf-updated`）；页数不一致 ⇒ 判 FAIL 并按 V9 处理 |
| **字体解析（② 的硬前置，见 INT-38②）** | Tectonic XDV 记**裸字体名**（`FandolSong-Regular.otf`、`lmroman12-regular`），解析靠 **TL 字体树 + kpathsea** ⇒ 若文档用了**TL 树里没有**的字体（含 bundle-only 字体），外部转换解析不到 ⇒ **必须先测"bundle-only / 非 TL 字体能否被外部 `xdvipdfmx` 解析"**（最小文档 + `--outfmt xdv` + 外部转换，一条命令可判）；不能解析时不给出"路线 ② 可用于所有文档"的承诺 |
| **与 ① 的互斥** | 切换路线必须**清空 `tmp/<stem>.tectonic.pages` 基线**（跨档页哈希不可比）；切换后的首轮 `changed_pages` 必须是"全量"而不是"逐页相同" |
| **里程归属** | 在 §5.1 里属 M5 之后的独立分支；**不阻塞 ① 的上线**。对应测试项：**INT-35（五要素）/INT-38（转换器探测）/INT-39（切换与基线隔离）** |

> **硬性纪律**：**两路线不得混测**；**页哈希不可跨档比较**（E2.11①③）。① 落地时 `Engine::Tectonic.writes_xdv() == false`（P-G2）；② 启用时必须有"本轮产物凭证"（防陈旧 XDV，INT-31），并把 `writes_xdv()` 语义拆成新的能力位。

### 1.3 运行环境矩阵（**ENV-B 是最大缺口**）

| 环境 | 定义 | 为什么必须 | 现状 |
|---|---|---|---|
| **ENV-A** 有 TL 的 Windows（本机） | TeX Live 2026 + `tectonic 0.17.0` + 426 文件缓存 | 全部已有实测都在这里 | `[实测]` |
| **ENV-B** **TL-less 干净 Windows** | 无 TL、无 `latexmk`/`xelatex`/`synctex.exe` | "免装 TeX Live"这条价值主张**只在这里成立**；G2（SyncTeX）、B14/B15 文案、`writes_xdv()==false` 的收尾路径全依赖它 | **`[未测]`（P0）** |
| **ENV-C** 离线 | 断网；或 `-C` + 预置缓存 / 本地 `.ttb` | 决定分发形态（G-2） | 部分 `[实测]`：热缓存 + `-C` 编 1 页英文 → exit 0 / **429 ms** / 零 `downloading`；**冷缓存 + `-C` 仍会拉 `<url>.index.gz`** |
| **ENV-D** 中文区域 / 中文路径 | 中文 Windows + 中文项目路径 | 文案不得依赖引擎本地化文本（引擎错误原文实测为本地化中文且不含路径，如 `os error 183`）；中文路径端到端 | XeLaTeX 侧 `[实测]`（中文路径用例），Tectonic 侧 `[未测]` |
| **ENV-E** 企业网 / 代理 | 坏代理、好代理、TLS 中间人 | DIST-2.15 / DIST-5.2 | `[未测]`（`geturl` 默认 reqwest `Client::new()`，**未设超时/代理**，`[推断]` 是否读 `HTTPS_PROXY`） |

**环境纪律（P0）**：

- **性能测量不得在 DSH 沙箱里做**：`fc-cache -f` 在沙箱内写 `C:\texlive\<年>\texmf-var\fonts\cache` 会 `Permission denied` ⇒ 缓存永远建不起来、每次编译都慢（`troubleshooting.md` 同名条目）。测量必须在提权会话或正常用户权限下跑。
- **测量期不同时跑真机 GUI**：并发写同一 `tmp/` 会得到 `I can't write on file '<stem>.log'`（§7.7 ② 的现场），该样本判 `log-locked` 无效。
- **先跑控制组，不合格就不测**（§3.3）。

### 1.4 已有实测证据索引（每条判据引用时只指到本表或分册章节）

| 类别 | 关键数字 | 出处 |
|---|---|---|
| **速度（L1 用户视角）** | Tectonic `min-zh` **1586 ms** / `thesis` **2515 ms**；`xelatex` 出 PDF **2350 / 2539**；`xelatex -no-pdf` **1378 / 1681**；`lualatex` 出 PDF **3887 / 5932** | `modern-engines-zh.md` §8.3 |
| 中文正确性 | `--keep-logs` 里 **0 条** `Missing character`；`pdftotext` 出中文正文；**28 页** | §8.4 |
| XDV 兼容 | `--outfmt xdv` → `main.xdv`（§8.4 记 **264,736 B**；现存文件 **264,728 B**，见 C-12）；`xdv-report` **28 页**、页字节 1038/9432/16101、解析 **5.66 ms**（44.6 MB/s） | §8.4 / engine.md §3 |
| 确定性 | `SOURCE_DATE_EPOCH=0` 两次：PDF **80,203 B**、XDV **264,728 B** 逐字节一致（PDF SHA-256 `926683A0…7F9EE0`；XDV `738F96DD…71FEB`，三份全等）〔**注**：这是"**设 epoch**"分支的实测；**D4 已裁决 (b)** ⇒ Tectonic 档默认**不设** epoch，此时 PDF 只失去元数据层面的逐字节确定性，**XDV 仍逐字节稳定**（`63531F94…4BAC`/264,736 B，E3.7）⇒ **"PDF 逐字节相等"不得作为 Tectonic 档的断言**〕 | §8.4 / engine.md E2.6/E3.7 |
| SyncTeX | `main.synctex.gz` **77,983 B**；解压 **18,582 行 / 508,271 字符**；非空 `Input:` 9 条、**空 `Input:` 132 条**、`!` **58 条** | §8.4 / engine.md E4.2 |
| 首跑 | `hello` **189 s**、`min-zh` **214 s**（104 行 `downloading`）、`thesis` **51 s**（73 行） | §8.2 |
| 缓存 | 426 文件 / **65,254,100 B**（62.2 MB）；`formats/…-latex-33.fmt` **24,451,466 B**；`bundles/data/<digest>` **421 文件 / 35,652,448 B**（扁平）；`hashes/<sanitize(url)>`（65 B）+ `.lock`（10 B，7 天重检） | perf.md §3.2 / dist.md F5 |
| **bib 链路（`[实测]` 更正的**第二轮**）** | 干净目录 `-k` 档：`main.bbl` **680 B / 9 条 `\bibitem{ref1..ref9}`**；同次运行 `main.pdf` **28 页**，`pdftotext` 文末有 `参考文献` + `[1]…[9]` ⇒ **主 bibtex 调用成功**；`Running BibTeX` = **9**、`Citation … undefined` = **0**、小写 bibtex 警告 = **8**（章级良性）；而 `main.blg` 仅 **537 B**、止于 `Database file #1: refs.bib`、**零 error 行、也无摘要行**。**机制**：① `You've used N entries` 这个串在 Tectonic 源码树里**根本不存在**（全树 0 命中）⇒ 成功也不会有；② 端口只在**有警告/错误**时才写 `(There were N …)`（`engine_bibtex/src/lib.rs:397-416`）⇒ 干净运行本来就没有收尾汇总 | 队长 2026-09-14 `[实测]` + engine.md Rev.2 G6 `[源码]`；工件 `test_file/projects/_t2-probe/thesis/tprint`（T2 Rev.3 独立复现） |
| `.blg` 尾部不可信（新测试对象） | 成功运行也会缺摘要行 ⇒ 任何"按 `.blg` 摘要行/error 行计数判 bib 成败"的工具都会**误判**；Tectonic 那句 `warning: errors were issued by BibTeX, but were ignored` 的来源是 **8 次章级 `no \bibdata` 良性调用** | 队长 2026-09-14 `[实测]`；`driver.rs:1934-1939` |
| **落盘形态（D2 定论）** | 干净目录：默认档 → **无 `.xdv`/无 `.aux`**，`main.pdf` **80,203 B**；`-k` 档 → **仍无 `.xdv`**，多出 `.aux` **443 B** 与 `.bbl/.toc` | 队长 2026-09-14 `[实测]` |
| **路线 ② 可行性（外部转换）** | `xdvipdfmx -q -o from-tectonic.pdf <tect-thesis>/main.xdv`（输入 **264,728 B**）→ **exit 0 / 1,091 ms（T2 复跑 617 ms）/ 79,070 B / 28 页 / 零 error-warning**；与 Tectonic 自带 PDF 对拍：`pdftotext` **去空白后逐字符相同（各 13,559 字符）**、嵌入字体组一致；字体名是**裸名**（靠 TL 树 + kpathsea 解析）、`xdvipdfmx.exe` 与 `synctex.exe` 都来自 TL | 队长 2026-09-14 + T2 Rev.6 `[实测]`（本机、既有产物、无下载无构建） |
| **`\today` 与 epoch（**D4 已裁决 (b)**）** | Tectonic：不设 `SOURCE_DATE_EPOCH` → `2026 年 9 月 14 日`（PDF 80,509 B，**= 裁定后的产品行为**）；设 `0` → **`1970 年 1 月 1 日`**（PDF 80,203 B，**已被裁定排除**）；**XeLaTeX 侧不受该变量影响** | T2 `[实测]`（判据见 integration.md Rev.10 / INT-36b） |
| 产品侧基线（XeLaTeX+latexmk） | `tiny` cold 2307 / noop 500 / edit 1625；`small-article` 6476/502/2593；`multifile` 6465/495/2463；`thesis` 9412/507/3143；`large` 13532/486/4104；`graphics` 2631/499/1826；`thesis-real-hithesis` **>240 s 未收敛（不计入）** | `design.md` 基准表 |
| 单趟 vs Full | 单趟比 `latexmk` 快 **40.2% 中位**（27.8%–55.5%，六档全部达标） | `scripts/bench-single-pass.mjs` |
| 页级复用（XeLaTeX 侧） | 125 页加一行注释 → **0 页变化**；跳过 `xdvipdfmx` 省 **0.65–0.94 s/次**；74 页改末章 `render` **102 → 7 ms**、`pagesReused 0→7`、`total 183→71 ms` | `incremental-edit-x-dvi.md` §2.1–2.3 |
| 控制组（本机抽检） | `fc-list` **3862（冷）/545/446/441 ms**、**3640 条**字体；`fc-cache -v` 0 条 invalid；`cmd /c exit` 133 / `tectonic --version` 32 / `lualatex --version` 123 / `xelatex --version` 819 ms；400 文件热读 **178 MB/s** | perf.md §3.2 |
| 分发 / 许可 | `-b *.tar` → `error: … doesn't specify a valid bundle.`；冷缓存 `-C` → `this bundle isn't cached, and we couldn't get it from the internet. …index.gz`；`TECTONIC_CACHE_DIR` 不可用 → `error: 当文件已存在时，无法创建该文件。 (os error 183)`（**本地化、不含路径**）；Tectonic 本体 **MIT**；bundle 内被 patch 的 4 个 TL 文件、OFL 字体、静态链接 C 依赖 → **待审计** | dist.md §0/§1/§5 |

---

## 2. 测试分层与完整矩阵

### 2.0 总览

| 层 | 来源分册 | 条目 | **P0** | P1 | 本文位置 |
|---|---|---|---|---|---|
| 前置门禁（接入前必须解决/先测） | 本文新增（由四册归纳） | **19** | **19** | 0 | §2.1 |
| 引擎层（进程/产物/日志/失败面） | engine.md（refreeze，E1–E7 / G1–G6） | **56** | **46** | 10 | §2.2 |
| 集成层（`Engine::Tectonic` 分支、UI、降级） | integration.md **Rev.10**（INT-*，88 条） | **79** | **57** | 22 | §2.3 |
| 分发层（安装形态、bundle 三路线、缓存、环境失败） | dist.md（DIST-*） | **34** | **22** | 12 | §2.4 |
| 测量层（阈值 R1–R5 / 命中率 H_A,H_B,H_C / 首跑 M1–M4） | perf.md Rev.4（R*/H_*/M*） | **12** | **8** | 4 | §3.4–3.6 |
| 许可 / 流水线 / 上线门禁 | dist.md §4–§7 | **25** | **25** | 0 | §4.3–4.5 |
| **合计（去重口径）** | — | **225** | **177** | **48** | — |

> **计数口径**：按**表格行**计（同一行聚合的条目 `INT-x·INT-y` 取其中最高优先级，并在判据里写明）；**不含** §2.5（一票否决 13）、§6.1（风险 **14**）、§6.2（未验证 31：P0 15 / P1 16）；**协议类规则**（§3.2 参数表与 V1–V6b）不按 P0/P1 计 —— 它们对每一条测量都是**强制**的。

### 2.1 前置门禁 P-G（**全部 P0**；P-G1..P-G5 是"先测出事实"，P-G6..P-G14 是"必须先修"，P-G15..P-G19 是"上线硬门 / 判据口径"）

| ID | 目的 | 操作 | 期望 | 判据（可判定，含反例） | P | 证据·出处 |
|---|---|---|---|---|---|---|
| **P-G1** | 锁定四档的**落盘事实**（D2 的判定输入） | 干净目录各跑一次：① 默认（无 `-k`/无 `--outfmt`）② `-k` ③ `--outfmt xdv` ④ `--pass tex`；逐次列出产物文件集 | ① 无 `.xdv`、无 `.aux`，有 `.pdf`；② **无 `.xdv`**，有 `.aux/.bbl/.toc/.pdf`；③ 有 `.xdv`（28 页）、无 `.pdf`；④ 待核（U-2） | 判据：① `<P>/<stem>.xdv` **不存在** 且 `main.pdf` SHA-256 == 干净目录基线；② 同上且 `.aux` 存在；③ `<P>/<stem>.xdv` 存在、`xdv-report` 解出 **28 页**、`<P>/<stem>.pdf` **不存在**；④ 记录事实即可（不判）。**本次目标已从"证明 `-k` 能拿 XDV"改为"记录事实"**（该假设已被实测推翻） | P0 | 队长 2026-09-14 `[实测]`（①②）；engine.md E2.1/E2.8/E2.10（③） |
| **P-G2** | ① 路线下**页级复用必须关闭**，且不得读陈旧 XDV | ① 单测：`Engine::Tectonic.writes_xdv() == false`；② 复刻 `runner.rs:948-1007` 的两个现场：目录里**有**陈旧 XDV（先跑一次 XeLaTeX）/ **没有** XDV，再跑 Tectonic Quick | 不调 `xdvipdfmx`、不读 XDV、`page_hashes` 为空；项目根 PDF 就是引擎刚写出的那份 | 判据（三条）：① 单测断言 `writes_xdv()==false`；② 有陈旧 XDV 时项目根 `main.pdf` 与"干净目录只跑 Tectonic"的 PDF **SHA-256 相等**；③ 无 XDV 时**不得**出现 `xdvipdfmx: Could not open specified DVI` | P0 | `runner.rs:463-471`（陈旧 XDV 覆盖事故：LuaLaTeX 自出 **109,732 B** 被覆盖成 **70,193 B**）`[实测]` 于 XeLaTeX/LuaLaTeX 侧 |
| **P-G3** | `-k` + `--keep-logs` **必带**（理由 = `.aux`/`.log`） | 命令构造单测断言 argv 含两者；反例：去掉 `-k` 后跑 Quick | Quick 前置（`tmp/<stem>.aux`）成立；失败时能读 `.log` | 判据：argv 断言；**反例自证**：不带 `-k` ⇒ `tmp/<stem>.aux` 不存在 ⇒ 日志出现「无构建产物，Quick 升级为 Full」（即该测试在旧行为下必须失败） | P0 | 队长实测（`-k` 才落 `.aux`）；`runner.rs:293-298`、`driver.rs:1615-1629` |
| **P-G4** | 输出目录由**产品**自建 | 冷项目（无 `tmp/`）Tectonic 首编 | exit 0、`tmp/<stem>.pdf` 存在 | 判据：exit 0 + 产物存在；**反例自证**：去掉 `create_dir_all` 必现 `error: output directory "tmp" does not exist` | P0 | 📖 `compile.rs:163-171`；`runner.rs` 全文**无** `create_dir`（现状依赖 latexmk 建目录）`[源码]` |
| **P-G5** | 选定实时反馈通道（`-p` 必带） | ① 编译期每 200 ms 轮询 `tmp/<stem>.log` 是否存在/增长；② 带 `-p/--print` 抓 stdout 页标记 | ① 命中 **0 次**；② `[N]` 逐字节流式到达 | 判据：① `.log` 首次 `Length>0` 的时刻 **≥ 进程退出时刻 − 0.5 s**；② `running` 后 **≤1.5 s** 收到首个 `pages>=1` 且早于终态 **≥500 ms**；③ 不带 `-p` 时 stdout 的 `[N]` 计数 == **0**；④ 命令构造里 `-p` 是**必带**（T2 Rev.2 口径）。时间阈值待 T3 校准（占位值） | P0 | engine.md E5.2/E5.3/E5.5；`driver.rs:1121-1205`（`writes_allowed=false`）、`runner.rs:134-161` |
| **P-G6** | **bib 链路**：判据基于**产物**；真失败必须可见 | ① 干净目录 FULL（`-k`）跑 thesis；② 造一个**真坏**的 `.bib`（缺失/语法错）再跑 | ① bib 正常、产物正确；② 真失败时 UI 可见 | 判据：① `main.bbl` 存在且 `\bibitem` 条数 **== 9**，产出 PDF 文本含 `参考文献` 与 `[1]…[9]`；② 真坏 `.bib` 时 `compile_get_errors()` ≥1 条且 message 指向该 `.bib`（**反例**：`status==success` 且 0 条 ⇒ FAIL）。**禁用判据**：`.blg` 摘要行（`You've used N entries`）与 error 行计数 —— 成功运行也会缺摘要 `[实测]`；那句 `warning: errors were issued by BibTeX, but were ignored` **不得**当失败信号（来源是 8 次章级良性 `no \bibdata` 调用） | P0 | 队长 2026-09-14 `[实测]`（`.bbl` 680 B / 9 条；PDF 28 页 + `参考文献`）；engine.md E6.1/E6.2 与 perf.md V6b 的"bib 失败"结论**已被实测覆盖** |
| **P-G7** | **缺字不得静默**（G4） | 构造缺字夹具（`\setCJKmainfont{不存在的字体}` 或生僻字） | 编译"成功"但必须有可见信号 | 判据：`Missing character` > 0 **且** `pdftotext` 抽检该字缺失 **且** 页数不变（⇒ 是缺字而非排版变化）；随后断言 UI ≥1 条警告（现状：全仓 grep 无消费方 ⇒ FAIL） | P0 | engine.md E1.4；`scan.rs:31-109`、`runner.rs:169-184` |
| **P-G8** | TL-less 文案分流（不许指向用户装不上的工具） | Tectonic 模式触发四类文案：引擎缺失、缺 `.cls`、缺宏包、源码版模板 | 文案可执行、不误导 | 判据（grep 断言）：Tectonic 模式的错误/建议文本**不含** `tlmgr`、`xelatex X.ins`、`TeX Live 未安装？`、`无法启动 xdvipdfmx`；引擎缺失条目 message 含 `tectonic` | P0 | `diagnosis.rs:383,394,136,143`；`runner.rs:321,322,538` |
| **P-G9** | 加枚举值**不得重置整份设置** | 用含 `"engine":"tectonic"` 的 `settings.json`（当前构建无该变体）启动，检查磁盘文件 | 其它字段（`timeout_secs`、`root_file`…）保留 | 判据：加载后 `timeout_secs` 等字段保留、磁盘文件**未被默认值覆盖**；**反例自证**：现状会 `save_global(default)`（用户设的 300 s 被写成 120 s）⇒ FAIL | P0 | `storage.rs:41-67`（解析失败/范围非法均整份重置并落盘）、`storage.rs:91-106` |
| **P-G10** | 引擎名唯一 + 页哈希分文件 | 同一项目先 XeLaTeX 后 Tectonic，检查 `tmp/*.pages` | 两个缓存文件并存 | 判据：`tmp/<stem>.xelatex.pages` 与 `tmp/<stem>.tectonic.pages` 同时存在；切引擎后首轮 `changed_pages`（若该档有页哈希）== 全部页 | P0 | `runner.rs:209-211`；`types.rs:26-59`（当前只有三个变体） |
| **P-G11** | 首跑不得被默认超时"判死" | 冷缓存 + 默认 `timeout_secs=120` 首跑（实测 189/214/51 s > 120 s） | 不出现误导性超时诊断 | 判据（二选一）：① 首跑成功；② 失败时诊断含"首次获取资源/下载"字样且 `suggested_timeout_secs ≥ 300`。**反例**：出现"疑似卡住（日志无页输出）"这类把下载误判为卡住的文案 ⇒ FAIL | P0 | `modern-engines-zh.md` §8.2；`AGENTS.md` roadmap ㉕（超时不自动重试） |
| **P-G12** | D1/D3 落地（人类已裁决） | ① 令 Tectonic 失败（删 exe / 造内容错误）；② 看状态栏；③ 点"一键切到 XeLaTeX 重编" | 失败**可见**、有出路、当前引擎可见 | 判据：① 错误列表条目 message 或 `diagnosis.hint` 含可执行动作（切引擎/提高超时/改源码）且**不自动回退**（全文无"已回退/静默回退"文案）；② 状态栏**不打开设置面板**即可见引擎名，且同处体现 Quick/Full 强度；③ 切引擎 ≤2 次点击且切换后编译成功 | P0 | human 裁决 D1/D3（2026-09-14）；`StatusBar.vue:48-99`（现状无引擎字段）、`policy.rs:53-59` |
| **P-G13** | 判据工具纪律（防恒绿用例） | 用 `--diff <f>` 与 `--diff=<f>` 各跑一次；再故意漏值/给多余位置参数 | 两种形态都出差分；错用则非零退出 | 判据：**stdout 必须含 `页级差分`** 才算有效样本；漏值/多余参数 `exit 2`（已修，`scripts/xdv-report.mjs:273-308`） | P0 | engine.md 工具-①/②；队长已修 |
| **P-G14** | 许可门禁（=G-1） | 跑 `docs/research/tectonic-bundle-licensing.md` 的 G-L1..G-L9 | 全绿 + 发行物含 NOTICE | 判据：见 §4.3；**未全绿前不得把 Tectonic exe/bundle 放进任何发行物** | P0 | dist.md §5 |
| **P-G15** | 离线形态门禁（=G-2） | 空缓存 + 断网，`-b <name>.ttb -C` 编中文夹具；另测"预置缓存 + `-C`" | exit 0、中文 0 缺字、页数一致 | 判据：exit 0 + `Missing character`==0 + `cache/bundles` **不存在**（证明没走 BundleCache）+ 日志 `downloading` 行数 == 0 | P0 | dist.md DIST-2.1/2.8；`lib.rs:275-287` |
| **P-G16** | 夹具侧前置（漏了整批静默作废） | `<outdir>/chapters/` 必须预先创建；每档前后清产出；编辑类样本必须还原源文件 | 样本有效 | 判据：不建 ⇒ 必现 `I can't write on file 'chapters/ch01.aux'` → Emergency stop（**反例自证**）；未还原源文件 ⇒ 页哈希差分判据全错 | P0 | perf.md §2.4；`recheck.ps1:20-22`、`bench.mjs:145-147` |
| **P-G17** | SyncTeX 往返与"可见降级" | ① 对拍三判据（E4.3）；② 在无 `synctex.exe` 的 PATH 下点 PDF 双向定位 | ① 等价；② **可见提示**（不是只进 console） | 判据：① 三判据全中（forward 页号与 `pdfinfo` 一致；inverse 落同一子文件且行号差 ≤ 2；每个子文件 ≥1 次命中）；② 失败落到状态栏/工具条且 `note` 含 "synctex" | P0 | engine.md E4.3/E4.4；`synctex.rs:29-31,60-68`（`-d` 硬编码 `tmp/`、起外部 `synctex.exe`） |
| **P-G18** | 版本三件套唯一真相源 | 见 §1.1；造一次"版本不匹配"（如 `.fmt` 后缀 `-32`） | 拒绝运行 + 可操作错误 | 判据：不匹配时**不进入编译**、错误文案含期望版本与处置动作 | P0 | dist.md DIST-1.2/2.18；`FORMAT_SERIAL=33` |
| **P-G19** | **页哈希口径纪律**（**取舍不属本计划**：归本仓已知债 **#26**；与 T1 **E3.8** 同源） | 凡**跨档 / 跨引擎 / 跨次**的页哈希判据，**必须在文档与脚本里声明口径**：**原始**（含 45 B `bop`/`prev`）或**归一化**（只哈希 `bop+45..eop`） | 判据可复核、报告可对齐 | 判据（两条）：① 每条页哈希用例写明口径（`xdv.rs:211` 现状 = 原始）；② 两种口径会给出**不同的"变化页集合"** ⇒ 报告必须标明所用口径并给出该口径下的集合（例：页 1 +10 B ⇒ 原始口径 25/26 页变化；归一化收窄到真正变化的那一页）。**本项只保留纪律，不指派实现落地**（候选修法 `bytes[bop+45..p]` 的收益/风险归已知债 #26） | P0 | 队长裁决（归一化口径选择权归已知债 #26）；T1 E3.8；T2 INT-31b；`xdv.rs:211`；`docs/modules.md` #26 |

### 2.2 引擎层矩阵（E1–E7，56 行；来源 engine.md refreeze（285 行，含 E3.7–E3.10），本文压缩判据、保留全部可判定要素）

> **引擎层 6 个门禁（G1–G6，全 P0/P1，见 engine.md §0）**：G1 编译中零实时反馈、G2 免装 TL 下 forward/inverse 不可用、G3 bibtex 告警是"假警报"且无真信号、G4 `Missing character` 不可见、G5 默认 PDF 档不落 `.xdv`（机制 = `driver.rs:1985`）、**G6 引擎自述 ≠ 事实**（Tectonic 日志/`.blg` 形态与 TeX Live 不同，**不能拿 TeX Live 的日志契约当判据**；旗舰例：`You've used N entries` 在源码树里 **0 命中**，成功也不会有）。

**夹具记号**：`<FIX>` = `test_file/projects/bench/_zhcmp`；`<P>` = 单用例独立输出目录（**先建**，不要写进夹具目录、也不要写进应用 `tmp/`）；`<T>` = `tectonic.exe` 0.17.0；除 E7.2/E7.3 外一律 `-C`（只用缓存，**本矩阵不下载 bundle**）；全部确定性用例固定 `SOURCE_DATE_EPOCH=0`。

**E1 中文正确性**

| ID | 目的 | 操作 | 期望 | 判据（可判定） | P | 证据·出处 |
|---|---|---|---|---|---|---|
| E1.1 | 基线：中文能排、无缺字 | `<T> -C --keep-logs --outfmt pdf -o <P> <FIX>/tect-thesis/main.tex` | exit 0；`main.pdf`/`main.log` 在 | `Missing character` 计数 **== 0**；`pdftotext` 含中文正文；`pdfinfo` 页数 **== 28** | P0 | §8.4 `[实测]`；`main.log` 22,861 B、`[N]`=28、尾行 `Output written on main.xdv (28 pages, 264728 bytes).` |
| E1.2 | 字体回退路径 = `ctex → fontset=windows + fontconfig`，不依赖 bundle 内 CJK 字体 | ① 编 `<FIX>/tect/min-zh.tex`；② 在 `main.log` 里找 fontspec/xeCJK/`TU/…` 声明 | 排版成功、无缺字 | ① `Missing character` == 0；② 日志出现**系统字体名**（不是 bundle 内字体文件路径） | P0 | 集成方案 §1 的"中文不依赖 bundle 字体"是 `[推断]`，**必须由本用例坐实** |
| E1.3 | `fontset` 档位不改变正确性 | 副本各加 `\ctexset{fontset=…}`（分别取 windows / mac / fandol / ubuntu）各编一次 | 每档可解释 | `windows` 档 == 0；其它档若 >0，**必须**同时在 UI 有可见信号，否则记为"静默缺字"缺陷 | P1 | `[未测]` |
| E1.4 | 缺字可判定（把静默缺字变红灯） | 缺字夹具（`\setCJKmainfont{不存在的字体}` 或生僻字） | "成功"但日志有缺字 | 三段式：`Missing character` > 0 **且** `pdftotext` 抽检该字缺失 **且** 页数不变 ⇒ 定义为缺字样本；随后 UI 必须 ≥1 条警告（= P-G7） | P0 | `scan.rs:31-109` 无该规则；`runner.rs:169-184`（流式只收 Error） |
| E1.5 | 与 XeLaTeX 的**产出等价性口径**（先统一趟数） | Tectonic 收敛档 vs `latexmk -xelatex` 收敛档；`xelatex -no-pdf` 单趟**仅作反例** | 页数差 ≤ 1，差异逐条可归因 | 判据：`pdfinfo` 页数、`pdftotext` 字符数、`diff` 后**每条差异都能归因**到包版本/趟数/目录页码；不得出现"整章丢失""空白页"；**③ 不得作等价性基线** | P0 | §8.4 的 28 vs 27 页正是趟数差；Tectonic 侧 LaTeX2e `<2021-11-15>`/ctexbook v2.5.8 vs XeLaTeX 侧 TL2026 |

**E2 `--outfmt xdv` 与 `xdv.rs` / `xdv-report.mjs` 的兼容**

| ID | 目的 | 操作 | 期望 | 判据（可判定） | P | 证据·出处 |
|---|---|---|---|---|---|---|
| E2.1 | 产物是我们认的 XDV | `-C --outfmt xdv --keep-logs -o <P>` → `node scripts/xdv-report.mjs <P>/main.xdv` | 工具解出 28 页、errors 空 | `页数 == 28` 且 `errors == []` 且 `pre: version=7 num=25400000 den=473628672 mag=1000` | P0 | `[实测]` 本轮：28 页、页字节 1028/9431/16101、5.6–6.7 ms、37–45 MB/s |
| E2.2 | Rust 路径与 JS 工具**同解** | 同一份 XDV 分别过 `xdv-report.mjs --json` 与 `latteset_core::xdv::page_hashes` | 页数相同；逐页"是否变化"判定相同 | `len(page_hashes) == pages.length` 且 `changed_pages()` == JS `--diff=` 的 `changedPages`；**两者必须同口径**（`xdv.rs:211` = 原始含 `prev`；JS 侧须核对，P-G19） | P0 | 两套哈希算法**本就不同**（Rust `DefaultHasher` vs JS SHA-1 前 16 hex）⇒ 测的是语义一致，不是字节相同 |
| E2.3 | xdv 档与 pdf 档**页数一致**（同批） | 同夹具同批两档 | 页数相同 | `xdv-report` 页数 == `pdfinfo` 页数（偏差 0；不一致即 P0 缺陷） | P0 | 现状是两次独立运行（28/28）⇒ 需**同批复验** |
| E2.4 | 页级差分口径 | `xdv-report a.xdv --diff=<P>/b.xdv` | 输出 `页级差分：…` | **stdout 含 `页级差分`**（不含即样本无效）；再比 `changedCount` | P0 | 工具-② 等号形式：`A=28/B=27，变化 28 页`；工具已修（P-G13） |
| E2.5 | 页哈希缓存不跨引擎污染 | 同项目先 XeLaTeX 全量、再 Tectonic | 两缓存文件并存 | `tmp/<stem>.xelatex.pages` 与 `tmp/<stem>.tectonic.pages` 是两个文件 | P0 | `runner.rs:209-211`；`[源码]`+`[推断]` |
| E2.6 | 页哈希稳定性（同源两次 XDV 逐页一致） | 两次独立运行（`SOURCE_DATE_EPOCH=0`）→ `--diff=` | 0 页变化 | `changedCount == 0` **且** 两文件 SHA-256 相同；**口径须声明**（原始含 `prev`；若两次产物页长不同，只有归一化口径才反映"内容未变"，P-G19） | P0 | `[实测]` `d1.xdv` == `d2.xdv` == `main.xdv`（264,728 B，`738F96DD…71FEB`） |
| E2.7 | 半成品/截断容忍 | `xdv-report <P>/main.xdv --truncate-at=8192` | 前缀解出的页不越界 | 所有 `eopOffset < 8192`；未见 `eop` 的页不出现在结果里 | P1 | 该缺陷 2026-09 已修；Tectonic 产物从未跑过 `--truncate-at` |
| E2.8 | **默认档不产 `.xdv`**（D2 事实） | ① **默认档**（无 `-k`/无 `--outfmt`）；② 加 `-k`；③ `--outfmt xdv` | ①② 不产 `.xdv`；③ 产 | 判据三条：① 运行后 `<P>/main.xdv` **不存在**；② 该次 stdout 无 ``Writing `main.xdv` `` 但有 `Skipped writing … intermediate files`；③ 存在且 `xdv-report` 解出 28 页 | P0 | ①`[实测]` 两路证据（PDF 档日志无 `main.xdv` 字样；`main.xdv` mtime 停在 `--outfmt xdv` 档）+ 队长干净目录复现；②`[实测]` 队长：`-k` 只多出 `.aux/.bbl/.toc` |
| E2.9 | 陈旧 XDV 污染 | 同一 `<P>` 先造 `.xdv`（XeLaTeX 或 xdv 档），**再**跑 FULL | 最终 PDF 必须是 FULL 自己那份 | 比较"跑过全流程"与"干净目录只跑 FULL"的 `main.pdf` SHA-256 —— **必须相等** | P0 | `[推断]`（Tectonic 侧未测）；同款事故在 XeLaTeX/LuaLaTeX 侧 `[实测]`：LuaLaTeX 自出 **109,732 B** 被陈旧 XDV 转换结果覆盖成 **70,193 B**（`runner.rs:463-466`） |
| E2.10 | 四档语义（**`--outfmt xdv` ≠ Quick**） | 四档各统计 `Running TeX` + `Rerunning TeX` | 档位语义唯一 | `①默认 == 3`、`②(-r 0) == 1`、`③(--outfmt xdv) == 3`、`④(--pass tex) == 1`；④ 下 `.xdv` 存在、`.pdf` **不存在**〔**待核 U-2**〕 | P0 | `[实测]` 7 份日志：PDF 档 3/9/1、xdv 档 3/9/0；`DEFAULT_MAX_TEX_PASSES = 6` |
| E2.11 | **PDF 档"无页哈希"语义**：`pages == 0` 是**无法判定**，不是"零页变化"；**PDF 档与 xdv 档的页哈希不可跨档混比** | 走产品链路：PDF 档编译后读 `tmp/<stem>.<engine>.pages` 与 `ChangedPages` 事件 | 保守全量刷新；任何"跳过重载/跳过转换"都不得触发 | 判据：① PDF 档 `page_hashes` 为空；② `ChangedPages` 不发出"零页变化"信号；③ 拿 xdv 档产出的哈希当 PDF 档基线 ⇒ **必须判为不同**（不得因都是 28 页就算"逐页相同"） | P0 | `xdv.rs:216-233`（`cur` 为空 = 无法判定，**不要伪造页号**）、`runner.rs:426-438`、`runner.rs:467-471` |
| E2.12 | **路线② 可行性**：外部 `xdvipdfmx`（TL 自带）能否吃 Tectonic 的 XDV（含字体解析） | ① `xdv-report <tectonic.xdv> --opcodes`（看 `define_native_font` 的字体名形态）；② `xdvipdfmx -q -o <P>/out.pdf <tectonic.xdv>`；③ `pdffonts` + `pdftotext` 与 Tectonic 自带 PDF 对拍 | 转换 exit 0、页数与字体组一致、文本等价 | 四条：① exit 0 且产出 PDF；② `pdfinfo` 页数 == **28**；③ 嵌入字体组一致（FandolSong/Kai + LMRoman，**无替换字体**）；④ `pdftotext` **去空白后逐字符相同**（实测各 **13,559 字符**；字节不同 79,070 vs 80,203 B 归因于转换器版本） | P0 | engine.md Rev.6 E2.12 `[实测]`；T2 Rev.6 复跑 617 ms |
| E2.13 | **Tectonic XDV 的字体是"裸名"，路线② 依赖 TL 字体树**（E2.12 的限定） | `kpsewhich FandolSong-Regular.otf`；再把 Tectonic XDV 的字体名与 XeLaTeX XDV 对照 | Tectonic = 裸名；解析靠 TL | ① Tectonic 的 `define_native_font` 名 == `FandolSong-Regular.otf` / `lmroman12-regular`（**无路径**，`lmroman` 连扩展名都没有）；② XeLaTeX 侧是**绝对路径**（`C:/Windows/fonts/…`、`c:/texlive/2026/texmf-dist/…`）⇒ 外部转换成功靠 **TL 字体树 + kpathsea**，`xdvipdfmx.exe` 也来自 TL ⇒ **路线② 不满足"免装 TeX Live"**（与 G2 同类） | P0 | engine.md Rev.6 E2.13 `[实测]` |

**E3 确定性（`SOURCE_DATE_EPOCH`）**

| ID | 目的 | 操作 | 期望 | 判据（可判定） | P | 证据·出处 |
|---|---|---|---|---|---|---|
| E3.1 | 逐字节可复现（**按 D4=(b) 分档断言**） | 连跑两次：① 设 `SOURCE_DATE_EPOCH=0`；② **不设**（Tectonic 档的产品默认形态） | XDV 两种情形都全等；PDF 只在①全等 | 判据：① 两次 PDF SHA-256 相等 **且** 两次 XDV 相等（`[实测]` 80,203 B / 264,728 B）；② **两次 XDV SHA-256 必须相等**（`63531F94…4BAC`/264,736 B），而 **PDF 允许不等**（差异仅在 `/ID`/Info 元数据、字节数与 `pdftotext` 相同）⇒ **"PDF 逐字节相等"不得作为 Tectonic 档的断言** | P0 | E3.7 `[实测]`；§5.4 D4 已裁决 (b) |
| E3.2 | `--keep-logs` 不引入产物抖动 | 同 E3.1① 但 `--keep-logs`，比 `main.log` | 日志允许差异、**XDV** 必须一致 | `.xdv` SHA-256 相等；`.pdf` 按 E3.1 的档位判（② 下不作断言）；`.log` 差异须逐条落在 `Input:` 绝对路径或下载顺序 | P1 | `[未测]` |
| E3.3 | 确定性与 SyncTeX 的**冲突** | 开/关 `-Z deterministic-mode` 各产一份 synctex，比 `Input:` 行 | 开档时不兼容 | 开档时出现相对/空路径 ⇒ **产品不得同时承诺"逐字节确定"与"SyncTeX 可用"**（写进产品说明） | P0 | `tectonic -Zhelp` 原文：deterministic mode breaks SyncTeX |
| E3.4 | 边界：首次未收敛（空 `<P>`） | 空目录跑一次（不喂上次 `.aux`） | 仍收敛 | `main.log` 页数 == 稳态页数；`main.aux` 存在；再跑一次 `--diff=` **0 页变化** | P0 | `[未测]`；"首次"正是 Quick/Full 升级判据的输入（`runner.rs:293-298`） |
| E3.5 | 边界：bibtex 参与时 | FULL 跑两次 | 仍可复现 | 两次 PDF SHA-256 相等；**`refs.bib` 修好后必须重跑**（现状 bibtex 中止被吞 = G3） | P0 | `[未测]`（未在 bibtex 成功前提下测过确定性） |
| E3.6 | **`SOURCE_DATE_EPOCH` 会改"印出来的日期"**（引擎行为差异，直接命中产品） | 同夹具两次：① `SOURCE_DATE_EPOCH=0` ② **不设**；各跑 `-C --outfmt pdf -o <P> main.tex`，再 `pdftotext` 取日期行 | **裁定后的产品行为 = ②**（Tectonic 档不设）；① 是**反例形态** | 三条：① 设 epoch 的 PDF **文本**含 `1970 年 1 月 1 日`（反例，不得出现在产品输出里）；② 两份 PDF 去空白文本**唯一差异就是日期**（实测首个差异 @18 = 日期处，其余逐字符全同，13,559 vs 13,560 字符）；③ **XeLaTeX 对照两份日期相同** ⇒ 差异是**引擎级**、不是夹具级 | P0 | engine.md E3.6 `[实测]`；T2 `[实测]`；`_t2-probe/r2/{nodate,withdate}`；`runner.rs:269-280`；§5.4 D4 |
| E3.7 | **D4 三选项的引擎层代价**（E3.6 的量化，给 D4 裁决用） | ① **不设 epoch** 各连跑两次：`-C --outfmt pdf` 与 `-C --outfmt xdv`，分别比 SHA-256；② **仅 epoch 不同**的两次 `--pass tex --keep-logs`（其余参数全同）比**逐页字节**；③ 根因 = `bop` 末尾 4 B 的 `prev` | (b) 的代价**只有两条**（PDF 元数据 + 跨天日期）；**XDV 逐字节稳定** | 判据：① 不设 epoch 时两次 **XDV 的 SHA-256 相等**（`63531F94…4BAC`，264,736 B）⇒ 页哈希源稳定，**(b) 不摧毁 A/B/C**；而两次 **PDF 的 SHA-256 不等**（`92240605…` vs `F766FACC…`，各 80,509 B、`pdftotext` 相同 ⇒ 差异仅在 `/ID`/Info 元数据）；② 仅 epoch 不同时：**页 1 变长 10 B（78 处差异）**、**页 2 逐字节相同**、**页 3–26 长度全同、差异只落在页内偏移 43–44**（`prevB − prevA = +10`）⇒ 机制 = **`prev` 绝对偏移连锁**，不是"每页内容都变了"；③ 设 epoch 后两次 PDF SHA-256 相等 | P0 | engine.md E3.7 `[实测]`（`_t1-probe/d4/` + 队长复算）；`docs/modules.md` **已知债 #26** |
| E3.8 | **页哈希口径必须声明**（E3.7② 的接口影响，P-G19 / T2 INT-31b 同源） | 读 `xdv.rs:211`；对同一份 XDV 分别算"原始（含 45 B `bop`）"与"归一化（`bop+45..`）"两套哈希 | 现实现 = **原始口径** | 判据：① 每条涉及页哈希/差分的用例**必须声明口径**；② **原始口径**下"任何一页变长 ⇒ 其后所有页 `prev` 连锁变化 ⇒ 被判为变化"（本夹具 **25/26 页**），**归一化口径**可把连锁收窄到"真正变化的那一页"；③ **口径选择权不在本计划**（归一化归 `docs/modules.md` 已知债 #26，候选修法 `bytes[bop+45..p]`，收益/风险未量化 ⇒ 未修） | P0 | engine.md E3.8 `[源码]` `xdv.rs:211`；队长裁决 |
| E3.9 | **"确定性由谁承担"：外部 `xdvipdfmx` 那一步是否可复现**（路线② / INT-20b 的前提） | 同一份 XDV → **同一个 `-o` 输出路径**，分两组各跑多次：① `SOURCE_DATE_EPOCH=0` ② 不设；逐次比 SHA-256 | ① 组逐次相同；② 组逐次不同 | 三条：① `epoch=0` 组**三次 SHA-256 全等**（实测 **79,065 B / `6F7EFC14…`**；队长复算另组 79,351 B 亦全等）⇒"㉚ 的逐字节确定性由**转换步骤**承担"**成立**；② 不设组两次**不等**（79,073 B，`DF49B482…` vs `C4A86ED1…`）⇒ 转换步骤并非天生确定，必须显式设 epoch；③ **本条纪律只对转换步成立**：**转换产物**须"**固定完整输出路径（目录 + 文件名）+ 固定 epoch**"（跨目录、**同目录换名（即使等长）** 都会不等 —— 队长样本 79,354（同路径）/79,348（等长改名）/79,343（换长名）/79,336（异目录同名）/79,346（不设 epoch）B；**该效应不由路径长度驱动**，不能靠长度对齐绕过）；而 **XDV 侧实测路径无关**（同源 + `epoch=0`、目录名长度差 30+ 字符 ⇒ XDV 逐字节相同，264,728 B / `738F96DD…71FEB`）⇒ XDV 比较**只需固定 epoch** | P0 | engine.md E3.9 终版 `[实测]`（`_t1-probe/d4/conv-*`）+ 队长独立复算；E3.7 + E2.12 + T2 INT-20b/INT-36④ |
| E3.10 | **D4 裁定后的分步口径**（T5 引用的权威表述） | 读 `runner.rs:275`（现状**无条件**）与 `523-548`（`convert_xdv`） | 按**子进程**分别设 epoch，而不是"整个编译设/不设" | 三条（逐进程声明）：① **Tectonic 进程不设** epoch（D4=(b)）⇒ 正文日期 = 当天、**XDV 逐字节稳定**、**PDF 不保证**；② **外部 `xdvipdfmx` 进程设 `0`**（仅路线②）⇒ 该步产物逐字节确定（E3.9① 已证）；③ **断言边界**：XDV 逐字节相等 ✅、正文日期 = 当天 ✅、**PDF 逐字节相等 ❌ 不得断言**（差异 56 B/80,509 B ≈ **0.07%**，仅在 `/ID`+Info，长度与文本相同） | P0 | engine.md E3.10 `[实测]`+`[源码]`；E3.7/E3.9；T2/D4 裁决；`runner.rs:275,523-548` |

**E4 SyncTeX（产物口径 + CLI 对拍）**

| ID | 目的 | 操作 | 期望 | 判据（可判定） | P | 证据·出处 |
|---|---|---|---|---|---|---|
| E4.1 | 文件名与位置 | `-C --synctex --keep-logs --outfmt pdf -o <P>` | `<P>/main.synctex.gz` | 文件存在、大小 > 0、gzip 可解 | P0 | `[实测]` 77,983 B；解压 18,582 行 / 508,271 字符 |
| E4.2 | 内容口径（我们得吃得下） | 解压取头部 | 结构符合 | 五条：① 首行 `SyncTeX Version:1`；② `Input:1:` 是**绝对 Windows 路径**；③ 子文件形如 `…\chapters/ch01.tex`（**混用 `\` 与 `/`**）；④ 存在 132 行**空** `Input:N:`；⑤ `!` 记录 58 条 | P0 | `[实测]` 全部本轮读出（非空 `Input` 9 条 = 主文件 + ch01..ch08） |
| E4.3 | CLI 对拍**三判据** | 分别产 Tectonic/XeLaTeX 的 synctex，各跑 `latteset-cli forward/inverse` | 两端命中同一源码位置（行号 ±小偏差） | 三条全中：① 8 章各抽 3 个章标题行，`forward` 的 `page` 与 `pdfinfo` 实际页**一致**；② 用 ① 的 `(page,x,y)` 做 `inverse`，`source` 落在**同一子文件**且行号差 ≤ 2；③ 每个子文件 ≥1 次命中（不得全落主文件或"已回落"） | P0（**G2 未解则不可执行**） | `[未测]`；集成方案 §3 摩擦点 4"只验到有产物" |
| E4.4 | `-d` 目录参数**硬编码 `tmp/`** | 读 `synctex.rs:29-31`；把 synctex 产物放 `-o` 指定目录后调用 CLI | 期望失败（现状） | `-d` 必须来自**运行期 outdir**；用例：outdir=`<P>` 时 `forward` 成功率 == 100%（现状应为 0% 或报错） | P0 | `[源码]` `fn synctex_dir(pdf) = pdf.parent()/tmp` |
| E4.5 | `--synctex` 在**非 PDF 档**下的产物与 `Output:` 字段 | `-C --pass tex --synctex --keep-logs -o <P> main.tex`（该档**不产出 PDF**） | `.synctex.gz` 仍产出；`Output:` **恒为 `pdf`** | 判据：① 文件存在且首行 `SyncTeX Version:1`；② **`Output:` == `pdf`，与是否真的产出 PDF 无关** ⇒ **该字段不可用作任何判据**（也不要拿它决定 `synctex view -o` 传什么） | P1 | engine.md Rev.6 E4.5 `[实测]`；§0.1 纪律 4（引擎自述 ≠ 事实） |

**E5 日志与进度（`--keep-logs` / `note:` / `--chatter`）**

| ID | 目的 | 操作 | 期望 | 判据（可判定） | P | 证据·出处 |
|---|---|---|---|---|---|---|
| E5.1 | `main.log` 能被 `parse_log` / `pages_typeset` **直接吃** | 拿现成 `main.log` 跑两个入口 | 能 | `pages_typeset(log) == Some(28)`；`parse_log(log)` 不 panic；`len(messages) == 0` | P0 | `[实测]` 28 个 `[N]`、0 个 `!`、0 个 `Warning`。**附注**：Tectonic 的 `main.log` **没有 `This is XeTeX …` 抬头**（grep 计数 0）⇒ 任何"从日志识别引擎/版本"的逻辑在此失效 |
| E5.2 | 日志**只在结束后落盘** ⇒ 尾随 `.log` 失效 | 编译期每 200 ms 轮询 `<P>/main.log` | 整个编译期文件不存在 | `.log` 首次 `Length > 0` 的时刻 **≥ 进程退出时刻 − 0.5 s**（即"尾随 `.log` 拿进度"命中 **0 次**） | P0 | `[源码]` `FilesystemIo writes_allowed=false`（`driver.rs:1121-1205`）+ `write_files` 只在末段调用 |
| E5.3 | `note:` / `warning:` 的通道与 `[N]` | 分开抓 stdout/stderr | 分层正确 | ① `note:` 只在 stdout、`warning:`/`error:` 只在 stderr；② 不带 `-p` 时 stdout 的 `[N]` 计数 **== 0**；③ 带 `-p` 时 == **28** | P0 | `[源码]` `plain.rs:52-58`；`[实测]` `run1.log` 115 行全是 `note:`/`warning:`，无 `[N]` |
| E5.4 | `--chatter` 档位可见面 | `--chatter minimal` / `default` 各一次（用失败夹具） | `minimal` 只压 Note | 两次运行 `warning:` 行数**相等且 > 0**；`minimal` 下 `note:` 行数 == 0 | P0 | `[源码]` `Minimal` 仅抑制 `kind == Note`；`--chatter bogus` → **exit 2** `[实测]` |
| E5.5 | `-p` 的 `[N]` 必须**逐字节流式** | 记每个 `[N]` 的到达时间 | 非一次性 dump | 最后 3 个 `[N]` 的时间方差 > 0 **且** 首个 `[N]` 远早于进程退出 | P0 | `[推断]`（`GenuineStdoutIo` 直连 fd 1，引擎是否按页 flush 未实测） |

**E6 多趟收敛与 bibtex/biber**

| ID | 目的 | 操作 | 期望 | 判据（可判定） | P | 证据·出处 |
|---|---|---|---|---|---|---|
| E6.1 | **bib 链路成功的产物级判据**（改为正向验收） | 干净目录 FULL（`-k`）跑 thesis | bib 正常、引用已解析、条目已排版 | ① `main.bbl` 存在且 `\bibitem` 条数 **== 9**；② 产出 PDF **28 页**且 `pdftotext` 含 `参考文献` 与 `[1]…[9]`；③ `main.log` **无** `Citation … undefined` | P0 | 队长 2026-09-14 `[实测]`：`.bbl` **680 B / 9 条** `\bibitem{ref1..ref9}`；PDF 28 页 + `参考文献` + `[1]…[9]`。**本行覆盖前一版"bib 中止"结论** |
| E6.2 | **`.blg` 尾部不可信 + 判定器自测**（新测试对象） | 对同一次运行比 `.blg` 与产物；再对候选规则跑反例集（见 E6.2b） | 明确记录"引擎自述 ≠ 事实" | `.blg` 仅 **537 B**、止于 `Database file #1: refs.bib`、**零 error 行、无摘要行**，而 `.bbl` 完整 ⇒ 判据必须走产物；**判定器自测**：喂一份"成功但无摘要行"的 `.blg`，必须输出 **`unknown`**，**不得输出 `fail`** | P0 | 队长 `[实测]`；`driver.rs:1934-1939`（bibtex 的 `Errors` 只 `tt_warning!`，不 `return Err`） |
| E6.2b | **"bibtex 是否成功"的判定器本身要测**（三个方向的候选规则都被证伪） | 对两侧 `.blg` + 产物跑候选规则：① 计 `error messages` 行 ② 计 `warning` 行 ③ 查摘要行 `You've used N entries` ④ **查产物**（`.bbl` 条目数 / PDF 文本 / `Citation … undefined` 计数） | **只有 ④ 可用** | 判据：① Tectonic 侧**恒错**（8 份章级空跑判失败、成功的主调用判通过）；② 无区分度；③ **Tectonic 侧恒假阴性** —— 该字符串在源码树与已安装二进制里**都不存在**（字节级复核，全树 `You've used` **0 命中**）⇒ **成功也没有**；④ 正确。回归样本必须含"成功但无摘要行"的负例 | P0 | engine.md Rev.2 E6.2b（G6）`[实测]`+`[源码]`；`engine_bibtex/src/lib.rs:397-416`（只在有 warn/err 时才写 `(There were N …)`）、`:554`（`Capacity:` 行恒写） |
| E6.2c | 章级多跑是 Tectonic 独有，且是那 8 条警告的来源 | 比较 `<P>/chapters/*.blg` 数量：Tectonic vs `latexmk -xelatex` | Tectonic 9 份；latexmk 1 份 | `Running BibTeX` 行数 == aux 文件数（9）、`warning: errors were issued by BibTeX` 行数 == 无 `\bibdata` 的 aux 数（**8**）；latexmk 基线目录只有 `main.blg` | P1 | engine.md Rev.2 E6.2c `[实测]`；`driver.rs:1949-1959` |
| E6.2d | 章级空跑的**产物落点** + 主 `.bbl` 命名唯一性（产物级 bib 判据的前提） | `-C -k --keep-logs -o <P>` 编 thesis，然后枚举 `<P>` 下所有 `.aux`/`.bbl` | `<P>/main.bbl` **唯一一份**；章级只落 `.aux`/`.blg`，**不落 `.bbl`** | 判据三条：① `Get-ChildItem -Recurse -Filter *.bbl` **恰好 1 项**且相对路径 == `main.bbl`；② 章级 `.bbl` 缺席有**显式解释**：日志 8 条 ``note: Not writing `chapters/chNN.bbl`: it would be empty.``；③ 主产物 stem == **根文件 basename**（`compile.rs:144-151`），章级产物落 `<outdir>/chapters/` 子目录（`driver.rs:1123-1127`） | P1 | engine.md Rev.2 E6.2d `[实测]`（两个独立探针目录：只有 `main.bbl` 680 B + 9 份 `.aux`）⇒ **不存在同名冲突** |
| E6.3 | **真失败**必须可见（用真正坏的 `.bib` 造） | `\bibliography{refs}` 指向缺失/语法错 `.bib` | UI 可见错误 | `compile_get_errors()` ≥1 条且 message 指向该 `.bib` 或命中"缺文件"诊断；**反例**：`status==success` 且 0 条 ⇒ FAIL | P0 | `runner.rs:169-184`（流式只留 Error）、`runner.rs:373-397`（终态只在非零退出走）、`scan.rs:89-93` |
| E6.7 | 8 次章级 `no \bibdata` 是**良性**，不得当错误 | 统计章级 bibtex 调用与那句 `errors were issued by BibTeX…` | 不误报 | 错误列表**不含**该 warning 对应条目；warning 计数与章数一致（实测 **8**） | P0 | 队长 2026-09-14 `[实测]`；§8.4（该警告原文） |
| E6.4 | `-r/--reruns` 语义 | ① `-r 0` ② `-r 2` ③ 不给 | 三档可预期 | 趟数（`Running TeX` + `Rerunning TeX` 行计数）：`-r 0` **= 1**、`-r 2` **= 3**、不给 **= 1..6**（本夹具 3）；且 `-r 0` 的 `.log` 应含 `Citation … undefined` | P0 | `[源码]` `driver.rs:1704-1711`；③ = 3 趟 `[实测]`。**注**：`-r 0` 仍跑首趟 + bibtex；"最干净的单趟"是 `--pass tex` |
| E6.5 | 收敛上限与告警 | 构造需 > 6 趟的文档（链式 `\ref`） | 打 warning | `TeX rerun seems needed, but stopping at 6 passes` 出现 ⇒ "未收敛"必须能被产品识别（否则用户看到"成功"但交叉引用错） | P1 | `driver.rs:1344`（`DEFAULT_MAX_TEX_PASSES = 6`）+ `1736-1742` |
| E6.6 | biber 路径 | `biblatex + biber` 夹具 | 能跑或明确失败 | exit code + 是否产 `.bbl`；失败必须落在日志/stderr 且能被 `parse_log` 抓到 ≥1 条 Error | P1 | `[未测]`（集成方案 §7 列为未验证） |

**E7 失败面（退出码 / 错误信息 / 我们的呈现）**

> 统一记录模板：`(exit code, stdout 关键行, stderr 关键行, 是否产 .log, 是否产 .pdf/.xdv)`，并与"产品应呈现什么"逐条对齐。

| ID | 目的 | 操作 | 期望 | 判据（可判定） | P | 证据·出处 |
|---|---|---|---|---|---|---|
| E7.0 | 退出码约定基线 | 成功 / 编译失败 / 参数错 各一次 | 0 / 1 / 2 | 三个 exit code 精确相等（0/1/2） | P0 | `[源码]` `main.rs:183-186`；`--chatter bogus` → **exit 2** `[实测]`；"编译失败 == 1"待实测 |
| E7.1 | 缺包（`\usepackage{nope}`） | 本目录内造 `bad.tex` 编译 | exit 1 + `bad.log` | ① exit == 1；② `--keep-logs` 的 `.log` **含 `!` 开头的行**；③ 该 `.log` 喂 `parse_log` 出 ≥1 条 `Error` 且 `line` 非空 | P0 | `[未测]`（错误路径确实 `write_files(only_logs=true)`，`driver.rs:1506`） |
| E7.2 | 离线 `-C` + 缓存缺文件 | 故意缺文件的夹具（`\usepackage{tikz}` 而缓存没有）跑 `-C` | exit 1、**不得偷偷联网** | ① exit == 1；② 断网重跑结果**完全相同**；③ stderr 能定位缺失文件名 | P0 | `[源码]` `cache.rs:370-372`（`only_cached → NotAvailable`）；最终表现未实测 |
| E7.3 | 网络不可用（**不加 `-C`**） | 断网跑缓存里没有的包 | exit 1、信息可读 | 失败信息含 URL 或 `download` 语义；**不得**表现为"卡住不动"（配合超时用例计时） | P0 | `[未测]` |
| E7.4 | `-b` 指错路径 | `-b <不存在路径> -C -o <P> main.tex` | exit 1 + 文案 | stderr 含 `doesn't specify a valid bundle` | P0 | `[源码]` `compile.rs:193-199`；`.tar` 三探针 `[实测]` |
| E7.5 | 本地 bundle 的**合法形态** | 分别 `-b <目录>` / `x.zip` / `x.ttb` / `x.tar` | 前三个可用、`.tar` 拒绝 | `-b x.tar` **必须**报 E7.4 的错；**若它静默回退到默认 bundle ⇒ 判失败**（会偷偷联网） | P0 | `[源码]` `lib.rs:275-287`；**注意**：集成方案 §5.2 写的"TeX Live tarball + GNU patch"**不是 `-b` 的输入格式**（见 C-6） |
| E7.6 | `-o` 指向不存在的目录 | `-o <不存在>` | exit 1 + 文案 | stderr 含 `output directory "…" does not exist` | P1 | `[源码]` `compile.rs:163-168` |
| E7.7 | 超时被我们杀掉 ⇒ **诊断输入退化** | 故意慢文档 + 产品 `timeout` | 不自动重试、诊断不误导 | ① 确认 `tmp/<stem>.log`（内存层未落盘）**不存在** ⇒ 拿不到页证据；② 必须以 **stdout 页标记**（`-p`）或替代证据补上，否则记"超时诊断退化"并交产品处置 | P0 | `[推断]`；`runner.rs:347-356`（`timeout_entry` 依赖 `tmp/<stem>.log` 的页证据） |
| E7.8 | 用户取消 | 触发 `CancellationToken` | `Aborted`、无残留 | 取消后 3 s 内无 `tectonic` 子进程存活；`tmp/` 无半成品 XDV（内存层未落盘 ⇒ 预期"什么都没有"） | P1 | `[未测]`；`runner.rs:692-710`（`kill_tree`） |

> ⚠️ **本节（E2 的 XDV 落盘 + E6 的 bib 判据）证据日期 2026-09-14；已反转 3 次，引用请核对版本。** 现行验收线：**产物级判据 + G6**（E6.1/E6.2b），XDV 一律以"默认档不落（`driver.rs:1985` 内存移除）"为准（E2.8/P-G1）。

### 2.3 集成层矩阵（INT-*，79 行；来源 integration.md **Rev.10**（88 条 / 460 行），**已按 D1–D4、G1–G6 与实测改写**）

> **起点事实**：产品代码里**目前零 Tectonic 引用**（`Engine` 枚举只有 xelatex/pdflatex/lualatex，全仓 grep 0 命中，`types.rs:17-24`）⇒ 本节全部条目都是"接入后待验"。
> **三个必现集成坑已提为前置门禁**：`-o` 目录自建（P-G4/INT-21）、`-k`/`--keep-logs` 必带（P-G3/INT-11/INT-23/INT-42）、首跑不撞默认超时（P-G11/INT-47）。
> **两个既有缺陷必须覆盖**：未知 engine 值整份重置设置（P-G9/INT-13·14）、文案写死 TeX Live 工具（P-G8/INT-16/56/57/61）。

**1) 引擎选择与设置**

| ID | 目的 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| INT-10 | 枚举与序列化正确 | 扩三条单测：serde 往返 `"tectonic"`、`binary_name()`、`writes_xdv()` 语义 | 新增变体后原三引擎行为逐条不变 | `serde_json::to_string(Engine::Tectonic) == "\"tectonic\""`；`binary_name() == "tectonic"`；**`writes_xdv() == false`**（① 路线，见 P-G2）；Tectonic 分支**不走 latexmk**（无需假 `-X` 参数） | P0 | `types.rs:17-59,300-322` |
| INT-11 | **`-k` 的理由不是 XDV**（改口径） | 命令构造断言：FULL/QUICK 都必须带 `-k`、`--keep-logs`、`-p` | `.aux`/`.log`/页进度可得；`.xdv` **不可得** | argv 断言三个参数都在；**反例自证**：断言 `-k` 档下 `<P>/<stem>.xdv` **不存在**（若存在，说明 D2 结论已变，必须回头复查 §1.2.2） | P0 | 队长实测（`-k` 仍不落 `.xdv`，只落 `.aux` 443 B）；T2 Rev.2 |
| INT-12 | 设置三层都能选 Tectonic 并真生效 | ① 面板选（全局）② 项目 `.latteset/settings.json` ③ `update_settings` | 三条路径都让下一次编译用 Tectonic | 单测合并后 `compile.engine == Tectonic`；真机：切换后日志出现 Tectonic 分支；headless 同义 | P0 | `settings/model.rs:16-91`、`src-tauri/src/commands.rs:475-560` |
| INT-13·14 | **未知 engine 值不得重置整份设置**（含项目覆盖逐字段清洗） | 用含 `"engine":"tectonic"` 的 `settings.json` 启动**当前构建**；项目覆盖里放 `{"engine":"tectonic","timeout_secs":9999}` | 逐字段容忍：合法字段保留、非法字段清理 | `timeout_secs` 等保留、磁盘**未被默认值覆盖**；项目覆盖 `engine==Some(Tectonic)`、`timeout_secs==None`、`root_file` 不受影响；**反例自证**：现状 ⇒ `save_global(default)` 把用户的 300 s 写成 120 s ⇒ FAIL | P0 | `storage.rs:41-67,91-106` |
| INT-15 | 运行中切引擎用**快照** | 编译运行中把引擎切成 XeLaTeX | 运行中那次仍用 Tectonic 跑完 | 两次 `Quick/Full 编译（…engine…）` 的引擎名按时间线正确；页哈希缓存分别落在 `tmp/<stem>.tectonic.pages` 与 `…xelatex.pages` | P0 | `types.rs:249-262`、`runner.rs:209-211` |
| INT-16 | 引擎未安装时文案指向正确 | 令 `engine=tectonic` 指向不存在的可执行 | 可读中文 + 下一步 | 错误条目 message **含 "tectonic"、不含 "TeX Live"** | P0 | `runner.rs:321,322,538`（现状三条 "TeX Live 未安装？"） |
| INT-17 | **D3：状态栏必须有可见指示**（引擎名 + 强度） | 切引擎后**不打开设置面板**看状态栏；Quick 成功后再看一次 | 引擎名与强度同处可见 | 判据：状态栏文本同处含 `Tectonic`（或 tooltip）**与** Quick/Full 强度；**Quick 成功后与「引用待更新」chip 同时可见，Full 收敛后 chip 消失**；反例：现状 `StatusBar.vue` 无引擎字段 ⇒ FAIL | P0 | human 裁决 D3；`StatusBar.vue:48-99`；T2 Rev.4 |
| INT-18·19 | headless 不漂移；恢复默认仍回 XeLaTeX | CLI/MCP 跑同一夹具；面板「恢复默认」 | 语义一致；默认仍是 xelatex | CLI 退出码 0 且 `pdf_path` 指向项目根 `<stem>.pdf`，错误条数与 GUI 一致；`Settings::default().compile.engine == XeLaTeX` | P1 | `modules.md §8.1/§12.2`、`settings/model.rs:32-45` |

**2) Quick/Full 映射（命令表见 §1.2，唯一真相源）**

| ID | 目的 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| INT-20 | 命令构造正确且可单测 | 扩 `compile_command` 单测，逐项断言 ①/② 的 argv | argv 与 §1.2 一致 | 断言 `argv == ["tectonic","-o","tmp","--synctex","--keep-logs","-k","-p", …]`；QUICK 追加 `-r 0`；两条路径都含 `SOURCE_DATE_EPOCH=0`；**不含** `latexmk`/`-no-pdf`/`--outfmt xdv`/`--pass tex` | P0 | `runner.rs:235-280,826-840`；T2 Rev.2（`-p` 必带） |
| INT-21 | `tmp/` 由产品创建 | 冷项目（无 `tmp/`）Tectonic 首编 | 编译成功 | exit 0 且 `tmp/<stem>.pdf` 存在；**反例自证**：去掉 `create_dir_all` 必现 `output directory "tmp" does not exist` | P0 | =P-G4；`runner.rs` 全文无 `create_dir` |
| INT-22 | 首编必须升级为 Full | 干净项目第一次编辑触发（Quick 请求） | 升级 Full、`draft=false` | 日志出现「无构建产物，Quick 升级为 Full」；`tmp/<stem>.aux` **确实产出**（前提：带 `-k`） | P0 | `runner.rs:293-298`、`modules.md §12.2` |
| INT-22b | **既有不变量重映射**：`无 tmp/<stem>.aux ⇒ Quick 升级 Full` 在新引擎下依赖 `-k` | 两档各跑一次序列（FULL → 改一字 → Quick）：① 带 `-k`；② 故意不带 `-k` | ① 第二次是真 Quick；② 必须**暴露**为"每次都升级"而不是静默退化 | 判据：① 第二次 `draft=true` 且日志为「Quick 编译（单趟直调引擎）」；② 不带 `-k` 时出现「无构建产物，Quick 升级为 Full」⇒ 该现象必须被**显式记录为已知形态**（因为 `.aux` 依赖 `-k`），并由 P-G3 的 argv 断言把它堵死 | P0 | perf.md Rev.4 U12；`runner.rs:293-298`；队长实测（`.aux` 只在 `-k` 档落盘） |
| INT-23 | 第二次编辑**真的**是 Quick（不许恒升级） | ① FULL 成功一次；② 改一个字触发 | 第二次是单趟、`draft=true` | 日志「Quick 编译（单趟直调引擎）」+ `draft=true`；**反例判据**：出现"无构建产物，Quick 升级为 Full"即 FAIL（说明 `-k` 缺失） | P0 | `runner.rs:293-298` |
| INT-24 | Full 的收敛语义与 XeLaTeX 基线一致 | 同一大夹具 Tectonic FULL vs `latexmk` Full | 目录页码收敛到同一值（页数允许差 1） | 解析 `tmp/<stem>.toc` 目标页码逐条相等；stdout 含 ≥1 条 `Rerunning TeX because` | P0 | `design.md`（单趟 vs 收敛的 `.toc` 29→41 实测） |
| INT-25 | Quick 的"落后一趟"可复现 | 插 400 行后 QUICK，然后 FULL | Quick 后 `.toc` 保持上一轮页码；Full 后追上 | `.toc` 目标页码：Quick 后 == Full 前，Full 后 == 新值；UI 期间显示「引用待更新」 | P0 | `design.md §延迟预算实测附节` |
| INT-26 | `draft` 只在**真单趟**时报 | Quick 未升级 / 已升级 / 失败 三情形 | 只有第一种 `draft=true` | 事件序列 `success{draft:true}` → 收敛 `success{draft:false}`；`failed` 后 `draft` 保持 true | P0 | `src/stores/compile.ts:15-46` |
| INT-27 | 空闲收敛仍走 FULL | 草稿成功后停手 2 s | 触发一次 FULL，成功后「引用待更新」消失 | 时间线 `success(draft)` → Δ≈2 s → `running` → `success(draft:false)`；判定**不依赖** `fdb_latexmk`（Tectonic 不产）⇒ 用 stdout 的 `Rerunning TeX`/`-r` 缺失证据 | P0 | `useIdleConvergence.ts:20-48`、`design.md`（Δ≈2.0 s 实测） |
| INT-28 | 自动收敛不得污染 Quick 语义 | QUICK 连跑两次观察 `.toc` | Quick 永不自己收敛 | 两次 Quick 后 `.toc` 不变；日志**无** `Rerunning TeX because` | P0 | `driver.rs:1704-1711` |
| INT-29 | bib/biber 场景下 Quick 可用性 | `small-article` 编辑正文 → Quick；再 Full | Quick 不崩、Full 正确 | Quick 后 PDF 可渲染、无 `??` 泛滥（与 XeLaTeX 同档对比）；Full 后引用编号与页码与基线一致 | P1 | `modules.md §12.1 #3` |

**3) 页级复用 A/B/C 在 Tectonic 上的生效判据（**① 路线下 B/C 不可用，判据改为"可见降级"**）**

| ID | 目的 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| INT-30 | 页哈希可得性（**按路线分叉**）+ **A 显式登记"不支持"** | ① 默认路线 FULL；② 若启用 ② 路线则跑 xdv 档 | ① 预期**不可得**（`pages == 0`）；② 可得 | ① `CompileOutcome`/`PdfUpdated.pages == 0` 且 `tmp/<stem>.tectonic.pages` **不生成**；② `tmp/<stem>.xdv` 存在且 `.pages` 行数 == 页数（与 `xdv-report` 报的页数一致）；**A 必须在产品侧被显式登记为「该引擎不支持」**（不是"未触发"、更不是"静默跳过"） | P0 | 队长实测 + `runner.rs:467-471`；T2 Rev.4 |
| INT-31 | **A 无对应物** + **陈旧 XDV 绝不可被读**（P0 正确性，T2 Rev.2） | ① 连续两次"只改注释"的 Quick；② 在**无 `xdvipdfmx`** 的 PATH 下重复；③ 目录里预先放一份陈旧 `.xdv`（来自历史 XeLaTeX 或 xdv 档）再跑 | 两次都成功出 PDF；不出现「跳过 xdvipdfmx 转换」；**不读陈旧 XDV** | 断言日志不含该行；无 `xdvipdfmx` 的 PATH 下仍 exit 0；③ 下项目根 PDF == 干净目录基线（若读到陈旧 XDV 会误判"逐页未变"⇒ 前端跳过重载 ⇒ **屏幕停在旧 PDF**）。**实现要求**：Tectonic 路线必须带"本轮产物凭证"（`writes_xdv()==false`，或显式删除/按 mtime 作废） | P0 | `runner.rs:477-493`、`runner.rs:538`；§8.4 的"无需改代码"**只对 xdv 档 + 页哈希解析代码成立**（见 C-3） |
| INT-32 | B（跳过预览重载）：**按路线分叉的判据**（T2 Rev.2） | ① 路线：改一行注释 → Quick；② 路线（若启用）：同操作 | ① 每次保守全量刷新、**不误判"无变化"**；② 跳过重载 | ① 路线判据：事件 `pages == 0 && changed_pages.is_empty()` **且** `reloadKey` **递增**（绝不允许"没变化"⇒ 不重载）；② 路线判据：`pages>0 && changed_pages.is_empty()` ⇒ `skippedReloads` 递增、`reloadKey` 不变（= `H_B`，§3.5） | P0 | `incremental-edit-x-dvi.md §2.3`（XeLaTeX 实测 `reloadKey 2→2`）；① 路线无页哈希输入 ⇒ 该档 B **不可用** |
| INT-33 | C（只重绘变化页）：**按路线分叉的判据** | ① 路线：74 页 `multifile` 改**末章** → Quick；② 路线：同操作 | ① 全量刷新可接受且正确；② 只重绘变化页 | ① 路线：`pages == 0` ⇒ 每次全量刷新，且**不得**出现"复用页"事件（`pagesReused` 不增长）；② 路线：`changed_pages ⊆ 期望页集合` 且含期望页、`render` 落到 ~10 ms 量级（= `H_C`，§3.5） | P0 | 同上（`render 102→7 ms`、`pagesReused 0→7`） |
| INT-34 | 跨引擎基线不串 | XeLaTeX 编一次 → 切 Tectonic → 再编 | 首轮 Tectonic 无基线可用 | 两个 `.pages` 文件并存；切引擎后首轮（若该档有页哈希）`changed_pages` 长度 == 页数 | P0 | `runner.rs:209-211` |
| INT-35 | **路线 ② 的五要素分支判据**（P1 独立分支，不是"未来可做"；T2 Rev.4） | 在 ② 开关打开、前置探测通过的前提下跑 FULL/QUICK 序列 | B/C 恢复且可控 | 五要素：① **触发条件** = 设置里**显式开关**（**不得**按机器能力自动选择）；② **前置探测** = 本机 `xdvipdfmx` 存在才可启用，缺失则**拒绝并退回 ①**（见 INT-38）；③ **成功判据** = B/C 三条口径（`.pages` 行数 == 页数、`changed_pages ⊆ 期望页集合`、`render` 量级）全中；④ **失败判据** = 转换失败必须进**错误列表且不得发 `pdf-updated`**（不得复用旧 PDF）；⑤ **互斥与切换** = 见 INT-39 | P1 | T2 Rev.4 INT-35；§1.2.2 路线 ② |
| INT-38 | **转换器与字体的前置探测**（路线 ② 的开关条件；T2 **INT-38b** 的落地面） | ① 探测 `xdvipdfmx`（再造一次"缺失"：改 PATH）；② 造一个使用 **bundle-only 字体**的最小文档，走 `--outfmt xdv` + 外部转换 | 有则允许、无则拒绝；bundle-only 字体是否可解析有结论 | 判据：① 存在 ⇒ 允许启用；**缺失 ⇒ 开关置灰 + 说明原因 + 不得自动改用 ②**（D1 同源纪律）；探测失败按"没有"保守处理；② **bundle-only 字体**：外部 `xdvipdfmx` 能解析 ⇒ 允许；不能 ⇒ **必须**在 UI 说明"路线 ② 仅适用于字体全部来自系统或 TL 的文档"（不得静默产出缺字 PDF；若失败必须能被①拦住） | P1 | T2 INT-38/INT-38b；队长 2026-09-14 实测（+1,091 ms / 28 页）；残余风险见 §1.2.2 |
| INT-39 | **② ↔ ① 切换与基线隔离** | 两条路线来回切各跑两次 | 基线不串、首轮全量 | 判据：切换时**清空 `tmp/<stem>.tectonic.pages`**；切换后首轮 `changed_pages` 为全量（不得因"都是 28 页"判逐页相同）；两路线的产物不做跨档等价性比较 | P1 | T2 Rev.4 INT-39；E2.11①③ |
| INT-36 | 确定性 → 页哈希稳定（② 路线前置） | ② 路线 Quick 连跑两次 | 第二次 `changed_pages` 为空 | 两次 XDV SHA-256 相同（E3.1 支撑）且第二次 `changed_pages == []` | P1 | `[实测]` 逐字节一致 |
| INT-36b | **`SOURCE_DATE_EPOCH` 在两引擎语义不同**（不是"确定性的代价"，而是**同一变量改的东西不一样**）——**D4 已裁决 (b)，本条切换为写实断言** | 同夹具两次编译：不设 / 设为 `0`；并与 XeLaTeX + `SOURCE_DATE_EPOCH=0` 对照 | 断言写实：**Tectonic 档不设 epoch**（日期 = 构建当天）、**外部 `xdvipdfmx` 步设 `0`**（PDF 可复现由它承担） | **已实测**：**XeLaTeX 只动 PDF `/ID`（正文零变化）**；**Tectonic 改正文**（不设 → `2026 年 9 月 14 日`，80,509 B / 设 `0` → **`1970 年 1 月 1 日`**，80,203 B；去空白文本 13,559 vs 13,560、首个差异 @18 = 日期处）。**判据（写实）**：① Tectonic 档**不设**该变量 ⇒ 日期为构建当天（**禁止**断言 `1970`/禁止无条件设置）；② 同轮外部 `xdvipdfmx` 进程**设 `0`** ⇒ 该步产物可复现（分步 env，两全**待实测 U-31**）；③ **"PDF 逐字节相等"不得作为 Tectonic 档的断言**（E3.1②）；④ 不印日期的文档跨次对比不受影响，**印日期的文档跨天对比必须固定 epoch**（§0.1 纪律 5） | P0 | T2 INT-36b + T1 E3.6/E3.7 `[实测]`；`runner.rs:275`（现状无条件设置，落地必须改）；§5.4 D4 已裁决 |
| INT-37 | **路线决策落到测试**（改口径） | 命令构造 + 集成用例 | 默认路线 ① | 判据：默认 argv **不含** `--outfmt xdv`；若启用 ② 必须同时有：独立能力开关、陈旧 XDV 作废规则（P-G2）、**转换器与字体树来源（D5 待决）**——三者缺一即判 FAIL；**两路线不得混测** | P0 | human 裁决 D2（2026-09-14 实测）；T2 INT-37 |

**4) 编译中状态 / 进度 / 错误**

| ID | 目的 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| INT-40 | 实时页进度可达 | 大夹具 Tectonic 首编；监听 `compile-progress` | `running` 后进度单调递增 | 命令含 `-p/--print`（单测断言）；`running` 后 ≤1.5 s 收到首个 `pages>=1` 且早于终态 ≥500 ms（**占位阈值，需 T3 校准**） | P0 | =P-G5②；`modules.md §12.1 #20` |
| INT-41 | 实时错误可达 | 注入 `\undefinedmacrohere` | `compile-errors` 在进程结束前到达 | 「首条实时错误时刻 + 100 ms < 终态时刻」 | P0 | `runner.rs:1347` |
| INT-42 | 终态错误列表完整可读 | 造内容错误（缺宏包/语法错） | `errors-updated` 非空、含警告清单 | 错误条数 > 0 且 `kind == content_error`；**不得**出现「编译失败且无法读取日志」；命令含 `--keep-logs`（单测断言） | P0 | `runner.rs:373-399` |
| INT-43 | **bib 成败一律走产物级判据**（Rev.3/Rev.4） | ① 成功档：正常夹具跑 FULL；② 失败档：`\bibliography{refs}` 指向缺失/坏的 `.bib` | 成功可证、失败可见、定位规则明确 | **成功**：`.bbl` 的 `\bibitem` 条数 **== `.bib` 条目数（实测 9/9）** 且 `main.log` 的 `Citation … undefined` == **0**；**失败**：`Citation … undefined` > 0 **或** `.bbl` 缺失 ⇒ UI 必须 ≥1 条错误且 message 指向该 `.bib`。**`.bbl` 定位规则**：`<outdir>/<root_file stem>.bbl`（**不得硬编码 `main.bbl`**，见 E6.2d③）；**负例断言**：`<outdir>/**/chapters/*.bbl` **必须不存在**；`Not writing …: it would be empty.` 只作**辅助**断言、不许当判据（与 G6 同源）。**禁止**：计小写 `errors were issued by BibTeX`（成功档 8 条假阳性、失败档无信号）；查 `.blg` 的 `You've used N entries`（端口无该统计块 ⇒ 100% 假阴性） | P0 | =P-G6；T2 Rev.4 + engine.md Rev.2 E6.2d；工件 `test_file/projects/_t2-probe/thesis/tprint` |
| INT-43b | **判定器回归样本**：成功但无摘要行的 `.blg` 必须判 `unknown`，**不得判 `fail`** | 把 E6.2/E6.2b 的负例喂给集成侧的 bib 判定器 | 三态（成功/失败/未知）可区分 | 判据：负例输出 **`unknown`**；同族三条一起覆盖 —— `.log` 无 `This is XeTeX` 抬头、告警是小写 `warning:`、`.blg` 无收尾汇总（T1 门禁 G6"引擎自述 ≠ 事实"的落地面） | P0 | T2 Rev.3 INT-43b；engine.md Rev.2 G6 |
| INT-44 | 多趟重跑时页数不回落 | Tectonic FULL（自动收敛） | 状态栏页数只升不降 | 事件序列里 `pages` 单调不减 | P0 | `modules.md §2.6.1` |
| INT-45 | 良性噪音不进错误列表（**Rev.3 扩面**） | ① 用 Tectonic 跑中文夹具（日志含 `Fontconfig error: …`）；② 同一轮观察 **8 条小写 bibtex 警告** | 两类噪音都不占错误列表 | 判据：错误列表**不含** `Fontconfig error`，也**不含**那 8 条 `warning: errors were issued by BibTeX…`（每次成功构建都会出现）。**实现注意**：`parse_log` 现状"碰巧正确"（要求字面量 `Warning`，小写 `warning:` 天然不入）⇒ **放宽警告判定时必须同时加白名单**，否则 8 条假警报会直通 UI | P1 | §8.4；`scan.rs:89-93`；T2 Rev.3（反例 #17） |
| INT-46 | **超时诊断在 `.log` 缺失时仍成立**（**升为 P0**，见 C-8） | 超时设 5 s 跑大文档（Tectonic） | 证据化诊断仍给可用项、不误导 | `diagnosis.suggested_timeout_secs` 有值；message **不得**只说"日志无页输出"而不给替代证据（stdout 页标记应被用上） | P0 | =E7.7；`runner.rs:632-686` |
| INT-47 | 首跑不得被默认超时判死 | 冷缓存 + 默认 `timeout_secs=120` 首跑 | 不报"超时"、有进度 | 判据二选一：① 首跑成功；② 失败诊断含"下载/获取资源"且 `suggested_timeout_secs ≥ 300`；**反例**：出现"疑似卡住（日志无页输出）" ⇒ FAIL | P0 | =P-G11；§8.2（51–214 s > 120 s） |
| INT-48 | 中止与残留 | 编译中按停止 | `aborted`、无残留 | `tasklist` 无 `tectonic.exe`；状态 `failed{kind:aborted}`；队列清空 | P1 | `runner.rs:692-710` |
| INT-49 | 中间态不顶掉权威列表 | 复刻超时/收尾补发场景 | 终态清单不被中间态覆盖 | `errors-updated` 之后不再接受 `compile-errors`（前端 `phase` 守卫）；终态条目数 == 列表最终条目数 | P1 | `modules.md §12.2`（实测 1 条被 30 条覆盖） |

**5) 既有功能在"不读本机 TeX Live"下的降级（能力可以没有，但"没有"必须可见且不误导）**

| ID | 目的 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| INT-50 | 触发面不因引擎而变 | 三文件矩阵（改 `main.tex` / `refs.bib` / `fig.png`）两种引擎各跑 | 触发矩阵逐格一致 | 三种改动的 `compile_request_for_change` 结果相同；改 `.bib` 后**不**出现新编译（与 XeLaTeX 现状同） | P0 | `watch.rs:178`（只认 `.tex`）、`modules.md §12.1 #23` |
| INT-51 | 依赖信息在 Tectonic 下的替代面 | 加 `--makefile-rules tmp/<stem>.deps`，与 `.fls` 对同一源做差集 | 可用于"精确失效"、不添噪音 | `tmp/<stem>.deps` 存在、含主输入与项目内被读文件、**不含** bundle 内宏包路径；**不得**把 `.fls` 解析器直接套到 makefile 规则上（格式不同 ⇒ 静默空集） | P1 | `driver.rs:1540-1573`、`scripts/fls-report.mjs` |
| INT-52 | 生成产物永不当作源码打开 | 在 PDF 目录区/参考文献区点击（反查命中 `tmp/main.toc` / `tmp/main.bbl`） | 就近回落真实源码并提示"已回落" | 不出现 `tmp/*` 标签页；`InverseResultDto.note` 非空 | P0 | `synctex/classify.rs:31-75`、`modules.md §12.2` |
| INT-53 | 编译中的 SyncTeX 竞争 | 编译进行中连续点击 PDF | 不静默失败 | UI 出现同步提示（工具条 5 s 自动消失）而非只有 console；退避沿用 100/200/300 ms | P0 | `synctex.rs:5-24,19-24` |
| INT-54 | **SyncTeX 的二进制依赖必须可见降级**（+ 与页哈希**互不污染**，T2 INT-54b） | TL-less 机器用 Tectonic 编译成功 → 点反向定位；另跑 `--pass tex --synctex --keep-logs` 与不带 `--synctex` 各一次 | 可见提示说清"本机没有 synctex 工具，故无法双向定位"；**synctex 不改变 XDV** | 判据：① 失败落到状态栏/工具条，`note` 含 "synctex"，**不得**只走 `console.error`；② **两次 XDV 逐字节相同**（242,960 B / `08E7219F…`）、XDV 内**不含**源码绝对路径 ⇒ **页哈希与 SyncTeX 可同时开启**（INT-54b；T1 已撤回"synctex 逐页加 special"的旧说法） | P0 | G2；`synctex.rs:60-68`；T2 INT-54b；随包分发/自解析 = **D5 待决**（§5.4） |
| INT-55·60 | 引擎无关的既有逻辑不退化 | 根文件探测三形态；core 既有四条 `.ins`/`.dtx` 单测 | 与引擎无关地保持 | `get_project` 候选逐条相同；`cargo test -p latteset-core source_release` 全绿 | P0 | `project/root_detect.rs:22-104`、`diagnosis_tests.rs:294-347` |
| INT-56 | 缺 `.cls` 文案不得指向 `tlmgr` | `\documentclass{nosuchthesis}` + 项目里有同 `.ins` | 给可执行建议 | Tectonic 模式 `diagnosis.hint` **不含 `tlmgr`**；`kind == MissingClass` 不变 | P0 | `diagnosis.rs:394` |
| INT-57 | 缺宏包文案同理 | `\usepackage{no-such-package-xyz}` | 建议"bundle 内不含该宏包/需联网获取"或"切到 XeLaTeX" | hint 不含 `tlmgr`；`kind == MissingPackage` 不变 | P1 | `diagnosis.rs:383` |
| INT-58 | 引擎不匹配诊断覆盖 Tectonic | 文档要求 XeTeX 而用户设了 pdfLaTeX | 建议里的引擎名 ⊆ 面板可选集合 | 断言 hint 提到的引擎名集合 ⊆ 面板可选引擎集合 | P1 | `diagnosis.rs:328-341` |
| INT-59 | 真实 Tectonic 错误语料的诊断命中率 | 把 E7 采到的五类失败日志喂 `parse_log + diagnose` | 不低于 XeLaTeX 语料基线 | 与 `real_error_corpus.rs` 基线逐类对拍，给出命中数与漏检清单；漏检登记为 P1 | P0 | `log_parser/real_error_corpus.rs` |
| INT-61·62 | 源码版模板提示里给**能跑**的命令 | 缺 `.cls` 且项目里有 `X.ins` → 按提示执行，再编主文档 | 提示可执行、生成物可用 | hint 的命令前缀 ∈ {`tectonic`, 显式提示切引擎}；**反例**：仍给 `xelatex X.ins` 而机器无 xelatex ⇒ FAIL；生成后项目根出现 `X.cls` 且主文档 exit 0 | P0 | `diagnosis.rs:132-136`；`[推断]` Tectonic 编 `.ins` 可行但未实测 |
| INT-63·64 | 字体集/编码建议不误导 | `fontset=fandol` 或 `fontset=windows` 建议；GBK 源 | 建议仍可行 | hint 不指向用户装不到的东西；若 bundle 不含 fandol ⇒ 改推 `fontset=windows`（系统字体）；GBK 源建议不指向不存在的引擎 | P1 | `diagnosis.rs:358,368,320-326` |
| INT-65 | 依赖工具链缺位时不静默 | `scripts/fls-report.mjs` 在无 `.fls` 的 Tectonic 项目上跑 | 明确报"没有 `.fls`" | 脚本非零退出或明确提示，**不得**静默输出空报告 | P1 | `scripts/fls-report.mjs` |

**5b) T2 Rev.2 新增条目（对口 T1 的 G1–G5）**

| ID | 目的 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| INT-66 | **`Missing character` 静默缺字**必须可见 | 缺字夹具（同 E1.4/P-G7）走产品链路 | UI 出现可见信号 | UI ≥1 条警告；**反例**：`status==success` 且 0 条且 PDF 文本缺字 ⇒ FAIL | P0 | `scan.rs:31-109` 无该规则；全仓 grep 无消费方 |
| INT-67 | **`-Z deterministic-mode` 与 SyncTeX 互斥** ⇒ 产品禁用该开关 | 命令构造断言 + 开档对照 | 只用 `SOURCE_DATE_EPOCH=0` | argv **不含** `-Z deterministic-mode`；确定性判据仍成立（E3.1/E3.3） | P0 | `tectonic -Zhelp` 原文；E3.3 |
| INT-68 | `synctex_dir()` 硬编码 `tmp/` | outdir ≠ 项目根时调用 forward/inverse | `-d` 来自运行期 outdir | `forward` 成功率 == 100%（现状 0%/报错） | P0 | =E4.4；`synctex.rs:29-31` |
| INT-69 | Tectonic synctex 的**132 条空 `Input:` 记录**不得崩溃解析器 | 用 Tectonic 的 `.synctex.gz` 跑 classify/回落逻辑 | 正确跳过空记录 | 空 `Input:N:` 记录不产生标签页、不 panic；非空 9 条（主文件 + ch01..ch08）全部可解析 | P0 | `[实测]` E4.2④；`synctex/classify.rs:31-75` |

**6) 项目打开全链路（根文件、中文路径、子目录 include）**

| ID | 目的 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| INT-70 | 中文目录 + 中文文件名 + 中文子目录全链路 | `E:\项目\中文测试工程\中文主文件.tex` + `\include{章节/第一章}` | exit 0；四类产物齐 | `tmp/中文主文件.{pdf,log,synctex.gz}` 与项目根 `中文主文件.pdf` 均产出 + PDF 可渲染 | P0 | `runner.rs:1420-1467`（中文路径 XeLaTeX 侧实测基线）；Tectonic 侧未测 |
| INT-71 | **搜索根差异**：项目根的子目录 `\include` | `main.tex`（项目根）+ `\include{章节/第一章}` | 编译成功、章节内容在 PDF 里 | exit 0 且 PDF 文本含章节标题；**若失败** ⇒ 判定"必须传 `-Z search-path=<项目根>`"或改产品约定 | P0 | `driver.rs:1123-1141`；**本方案最高风险未验证项** |
| INT-72 | **嵌套根文件**（`css/thesis.tex`） | 根文件在子目录，正文用项目根相对路径 `\input{chapters/x}` | 编译成功 | exit 0；**反例判据**：不传 `-Z search-path` 时若报 `File 'chapters/x.tex' not found`，则记为"需 `-Z`（不稳定选项）或禁止嵌套根"，交产品决策 | P0 | 现有 latexmk 路径**支持**嵌套根 ⇒ 行为差异必须显式处理（`runner.rs:220-230`） |
| INT-73 | 项目切换不串味 | A（engine=tectonic）→ B（默认）→ 回 A | 引擎与缓存都不串 | 三次编译引擎名按时间线正确；`tmp/*.pages` 命名按引擎；B 项目里**不生成** `tmp/*.tectonic.pages` | P0 | `commands.rs:118-176` |
| INT-74·75 | 重启/重扫不影响引擎与根文件 | 关应用重开；`open→切走→open` 三次 | 设置生效、缓存复用、候选稳定 | 第二轮 `note: downloading` 行数 == 0；`root_candidates` 三次一致 | P1 | §8.2（缓存 426 文件/62 MB 复用） |
| INT-76 | 未确定根文件时不启动引擎 | 多候选项目（不选根）→ 点编译 | 提示可读、**不起进程** | `compile_now` 返回 `Invalid("未确定根文件，无法编译")` 且日志无 `tectonic` 进程启动 | P0 | `commands.rs:334-354` |

**7) 分发前置的集成面（bundle 存在性 / 首跑 / 离线 / 卫生）**

| ID | 目的 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| INT-80 | bundle 存在性前置检查 | `-b` 指向不存在/损坏的 bundle | 中文提示含**期望路径** | 用户可见文案含期望路径；**反例**：只显示 `` `X` doesn't specify a valid bundle.`` ⇒ FAIL | P0 | `compile.rs:193-198` |
| INT-81 | 首跑下载有可见反馈 | 清缓存后首跑（联网） | 有单调递增的可见反馈 | `running` 期间 ≥3 次可见反馈更新，内容含下载计数（现有 `compile-progress` 只报 `[N]` ⇒ 首跑无页标记 ⇒ 现状零反馈） | P0 | §8.2（逐文件 `note: downloading`，**无百分比**） |
| INT-82 | 离线开关（仅用本地缓存） | 设置项打开 → `-C`；断网编译 | 断网也能编；缺资源给明确提示 | 命令含 `-C`；断网 + 缓存齐 ⇒ exit 0；断网 + 缺资源 ⇒ 错误条目含"离线/缓存"字样 | P0 | `compile.rs:183-185`；**注意 `-C` 单独不等于离线**（C-7） |
| INT-83 | 缓存目录不可写时的降级 | 缓存目录只读 / 指向普通文件 | 可读诊断、含**我们传入的路径** | 错误列表 1 条含缓存路径；无 panic/无英文栈（引擎原文实测不含路径且本地化） | P1 | dist.md DIST-5.1 `[实测]` `os error 183` |
| INT-84 | 不污染项目目录 | 编译前后项目文件快照 | 只新增白名单 | 新增文件 ⊆ {`tmp/**`, 项目根 `<stem>.pdf`, `tmp/<stem>.<engine>.pages`}；不得出现 `Tectonic.toml`/bundle/format 缓存落在项目内 | P1 | `modules.md §12.3` |
| INT-85 | 网络半失败可重试且有证据 | 慢网/丢包下首跑 | 最终成功或给出重试证据 | stderr 出现 `failure downloading …` 时可重试；成功/失败都有记录 | P1 | `ttb_net.rs:201`、`itar.rs:66` |
| INT-86 | 排障入口：用了哪个 tectonic | 编译日志/诊断 | 含 exe 绝对路径 + `--version` | 断言日志含绝对路径与版本串 | P1 | 现状无（只记引擎名） |
| INT-87 | 首次使用引导 | 首次切到 Tectonic（缓存空） | 提示资源体积/时长量级 | 引导文案存在且与 INT-47 的超时口径一致（数字引 §8.2 的 51–214 s） | P1 | `[推断]` 形态可讨论 |

**8) 回退（按 human 裁决 D1 写，**不得出现静默回退**）**

| ID | 目的 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| INT-90 | **失败不自动回退** + 一键切引擎可达 | 令 Tectonic 失败；看错误列表与操作 | 失败可见、有出路、**不自动换引擎** | ① 全程无"已回退/静默回退"文案与行为（断言语义：失败后 `CompileRequest.engine` 仍是 Tectonic）；② 检测到本机有 TeX Live 时，错误条目上有「一键切到 XeLaTeX 重编」且 ≤2 次点击可用；③ 无 TeX Live 时不提供该按钮（或置灰并说明） | P0 | human 裁决 D1（2026-09-14） |
| INT-91 | 不可用即失败可见 | 模拟 `tectonic.exe` 缺失/无执行权限 | 1 条错误 + 状态栏 failed | `errors-updated` 长度 == 1 且 message 含 "tectonic"；`compile-status.phase == "failed"` | P0 | `policy.rs:53-59`（IoError → ContentError） |
| INT-92 | 切回 XeLaTeX 后能力等价 | Tectonic 失败 → 面板切 XeLaTeX → 重编同一项目 | 编译成功、PDF 覆盖、基线不串、SyncTeX 可用 | 新 `tmp/<stem>.xelatex.pages` 生成且首轮 `changed_pages` 为全部页；正向/反向定位各成功一次 | P0 | `runner.rs:209-211`、`modules.rs §12.2`；**GUI 实测（2026-09-15）**：`TectonicSettings::use_library_form` 要求 `engine == Tectonic`，否则切到 XeLaTeX 既不产 `.xelatex.pages`、`changed` 也恒为 0（Tectonic 复现出逐页相同的页面）；判定通过时 `Full 编译 engine="-xelatex"`、`changed=28`、`tmp/main.tectonic.pages` 原样保留。矩阵见 [tectonic-library-plan.md](../tectonic-library-plan.md) §6.3.1 |
| INT-93 | 失败后的队列语义与引擎无关 | Tectonic 失败 + 队列里已有新请求 | 直接跑新请求、不重试旧内容 | 事件序列 == `[running, running(新), failed/success]` | P1 | `policy.rs:37-68` |
| INT-94 | 文案可读（说清哪个引擎失败、下一步做什么） | 三类失败（不可用/超时/内容错误）各一次 | 每条给可执行动作 | 每条 message 或 `diagnosis.hint` ≥1 个可执行动作（切引擎/提高超时/改源码） | P1 | `diagnosis.rs`（22 类诊断） |
| INT-95 | **TeX Live 探测**（"一键切引擎"按钮的开关） | 在有 TL / 无 TL 两种 PATH 下各触发一次 Tectonic 失败 | 只有真有 TL 才给按钮 | 判据：探测到 TL ⇒ 按钮出现；**未探测到 ⇒ 不给按钮**（探测**失败**按"没有"保守处理）；**反例：无 TL 却给按钮 = FAIL**（用户点了一条跑不通的路） | P0 | T2 Rev.4 INT-95；D1 裁决 |
| INT-96 | **点「一键切到 XeLaTeX 重编」之后的行为** | 点击后观察设置、编译强度与基线 | 行为明确、可见、基线不串 | 判据四条：① 持久化语义明确（是改全局设置还是仅本次重编，UI 必须说明）；② 触发的是 **Full**（不是 Quick）；③ 成功/失败都可见；④ 页哈希基线不串（新 `tmp/<stem>.xelatex.pages`，首轮全量） | P0 | T2 Rev.4 INT-96；INT-92 |

### 2.4 分发层矩阵（DIST-*，34 行；来源 dist.md，**P0 22 条**与队长口径一致）

**1) 安装形态（M1 随包 exe / M2 用户自装 / M3 crate 内嵌）**

| ID | 目的 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| DIST-1.1 | 证明"随包 exe 被真正使用"（不被 PATH 上旧版抢走） | ① PATH 前部放假 `tectonic.exe`（打印版本后 exit 1）② 启动应用编译 ③ 查记录 | 用随包 exe | 日志出现随包路径；`--version` == 钉定版本；假 exe 输出**一次都没出现** | P0 | 现状 `runner.rs:244` 只按裸名起进程（无路径配置） |
| DIST-1.2 | 三件套版本一致性（= P-G18） | ① `--version` ② 编中文夹具 ③ 断言 `.fmt` 后缀 == `-33` 且 bundle 摘要符合预期 | 三处一致；不匹配则**拒绝启动** | 版本 == 0.17.0 且 `FORMAT_SERIAL` 推论一致（本版 33）；不一致 ⇒ UI 可操作错误（不是 panic） | P0 | `engine_xetex/src/lib.rs:36`、dist.md B13 |
| DIST-1.3 | 升级路径（换 exe） | 旧 exe 建缓存 → 覆盖新 exe → 再编同一文档 | 自动失效重建 | 升级后 `formats/` 出现与新摘要匹配的 `.fmt`；编译成功；**旧 `.fmt` 不被当作新格式使用**（自证：改 `FORMAT_SERIAL` 的对照实验应走"重建"分支） | P0 | `format_cache.rs:42-61` |
| DIST-1.4 | 卸载与残留 | 安装→编一个项目→卸载，列残留 | 逐项有二进制结论 | 清单逐项（保留/删除 + 理由）；卸载后不残留 `*-tmp-pid*` | P1 | 产品决策：缓存目录归谁（D6） |
| DIST-1.5 | 用户自装形态的版本门禁 | PATH 上放 0.16.x/0.15.x（或伪造 `--version`） | 明确提示"需要 ≥ 0.17.0" | 低版本**不进入编译**；文案含版本区间 + 处置动作 | P1 | `compile.rs:196-198` |
| DIST-1.6 | 形态 A（crate 内嵌）风险清单（**不做实现**） | 只读核查 `Cargo.toml` 的 vcpkg 段、`vcpkg-deps` action | 团队明确代价 | 只产出"风险 + 触发条件"（触发条件 = 确需内存内 XDV/页事件） | P1 | `Cargo.toml:147-174`、`x64-windows-static-release.cmake` |
| DIST-1.7 | 未签名 exe 的信任门槛 | 干净 Windows 上安装并首次运行（SmartScreen） | 有可操作绕行说明 | 文档含"未知发布者/如何继续"；记录 exe 的 Authenticode 状态 | P1 | ADR-0003 |

**2) bundle 三条路线（R1 预置缓存 / R2 本地 ttb / R3 懒下载）**

| ID | 目的 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| DIST-2.1 | 预置缓存支撑**离线**首编 | ① 联网机跑一遍（生成 `data/<digest>`、`hashes/<url>`、`formats/<digest>-latex-33.fmt`）② 打包三块 ③ 目标机 `-C` 编 min-zh/thesis | 成功、零 `downloading` | exit 0 + `downloading` 行数 == 0 + `Missing character` == 0 + 产物存在；**反例自证**：拿掉 `formats/*.fmt` 仍应成功（只变慢） | P0 | `[实测]` 热缓存 + `-C` 编 1 页英文 → exit 0、**429 ms**、无 download 行 |
| DIST-2.2 | 必须连 `hashes/` 一起预置 | 只拷 `data/` 后在无网环境 `-C` 编译 | **明确失败**而非静默重下 | 文案命中 `this bundle isn't cached, and we couldn't get it from the internet`；日志**无** `downloading` | P0 | `cache.rs:179-182`；F2 实测同款文案 |
| DIST-2.3 | 体积账（服务产品决策） | 逐项测并记账 | 可复核体积表 + 一条产品上限 | 每行有"测量命令 + 字节数"；**"够用"的定义 = min-zh/thesis/multifile 都零下载** | P0 | `[实测]` exe 51,538,432 / zip 21,060,223 / `data/<digest>` 35,652,448 / `formats` 24,451,466 B；自建 ttb 体积 `[未测]` |
| DIST-2.4 | 缓存命中不全时的降级可见 | 预置"中文常用子集"（不含 multifile 依赖）→ 编 multifile | 可操作提示 | 失败文案含**缺的文件名**且 UI 提供"允许联网补齐/切换 bundle"动作；不得只给 TeX 的 `File not found` | P0 | `cache.rs:370-372` |
| DIST-2.5 | 预置 formats 的收益与风险 | 有 `.fmt` / 无 `.fmt` 各跑一次 | 有则更快、无则自动重建不报错 | 两次耗时取**同批交错中位**（§3.2 协议）+ `formats/` 文件数变化；重建耗时记为 `[未测]`（= M2） | P0 | dist.md B13；M2 见 §3.6 |
| DIST-2.6 | 缓存目录"不可用"必须可见且可处置 | `TECTONIC_CACHE_DIR` → ① 普通文件 ② 只读目录 ③ UNC/不存在的盘 | 文案含路径 + 处置建议 | 应用侧错误文本**必须包含我们传入的路径**（引擎原文不含路径且随系统语言本地化） | P0 | `[实测]` `error: 当文件已存在时，无法创建该文件。 (os error 183)`；`app_dirs.rs:82-96` |
| DIST-2.7 | 缓存清理策略 | 删整个缓存 / 只删 `data/` / 只删 `formats/` 后各编一次 | 三种都能自愈 | 逐项"删什么 → 发生什么（重下/重建/失败）+ 耗时量级"；**删 `formats` 不删 `data` 必须仍能离线成功** | P1 | B13/B14 |
| DIST-2.8 | **本地 `.ttb` 端到端离线编译**（第一优先待办） | ① 造 ttb（§4.2 流水线）② `TECTONIC_CACHE_DIR` 指向**空目录** ③ `-b <x.ttb> -C --outfmt xdv --keep-logs` 编 min-zh/thesis ④ 断网重跑 | 无网可编；缓存只长 `formats/` | exit 0 + 产物页数一致 + `cache/bundles` **不存在** + 空缓存下新建 `<新 digest>-latex-33.fmt` | P0 | `lib.rs:275-287`；本轮只到"路径被接受/坏文件报错" |
| DIST-2.9 | 路径与格式错误必须可判定 | 依次测：不存在／空目录／坏 `.ttb`／`.zip`／`file://` URL／本地 `.tar` | 每种有稳定文案、不尝试联网 | `.tar` → `doesn't specify a valid bundle.`；空目录 → `bundle does not provide needed SHA256SUM file`；坏 `.ttb` → `failed to fill whole buffer`；**全部 exit 1、无网络请求** | P0 | `[实测]` 三条探针已固化 |
| DIST-2.10 | 缓存目录可被我们独占 | `TECTONIC_CACHE_DIR=<应用私有>` 后编译；检查用户默认缓存 | 全部缓存落指定目录；用户缓存零变化 | 编译前后比两处目录的**文件数 + mtime**：我们的增长、用户的零变化；卸载只需删我们的目录 | P0 | `app_dirs.rs:82-96`、`cache.rs:129`、`config.rs:164` |
| DIST-2.11 | ttb 版本不匹配的未来兼容 | 手改头部 version=2 后编译 | 明确报"不支持该版本" | 文案含 version/不支持语义；无 panic | P1 | `bundle/create.rs:75-88`（0.17.0 只有 v1） |
| DIST-2.12 | 首跑成本可预期、**不撞超时** | 清空缓存 → 编 min-zh（104 个按需文件）/ thesis（73 个）→ 记耗时与 `downloading` 行数 | 有可见进度、不触发产品超时 | 首跑期间状态栏可见"已获取 N/M 文件"或等价；**不得**在首跑中弹"编译超时（120 s）" | P0 | §8.2：189/214/51 s |
| DIST-2.13 | 二次编译不再联网 | 首跑完成后 `-C` 再编两次 | 走缓存 | `downloading` == 0；耗时回到 §8.3 量级（min-zh **1.586 s** / thesis **2.515 s**，允许同批 ±20%） | P0 | §8.3 |
| DIST-2.14 | 网络失败的重试与最终文案 | 用"立刻拒连"（`127.0.0.1:9`）与"黑洞地址"两种 URL 各编一次 | 有限等待 + 可操作错误 | 拒连**秒级**失败并命中 `this bundle isn't cached, and we couldn't get it from the internet. Error: …`；超时场景 X 秒内同类错误（X 待实测；`NET_RETRY_ATTEMPTS=3`/500 ms 是下限，`Client::new()` **未设超时** ⇒ 上限取决于平台 TCP 超时 `[推断]`） | P0 | `cache.rs:170-182`、`geturl/src/reqwest.rs:78-119` |
| DIST-2.15 | 代理 / 企业网络 | ① 坏代理 ② 好代理 ③ `NO_PROXY` | 行为符合预期 | 三种配置下 `downloading`/错误行为符合预期；**代理是否被 reqwest 默认读取需实测**（`Client::new()` 未显式 `.proxy()`） | P1 | dist.md B19 |
| DIST-2.16 | 上游 bundle 刷新导致**预置缓存硬失效**（负测，F3） | ① 备份 `hashes/<url>` ② 改成另一个合法摘要 ③ `-C` 编译 | 失败**可见**（不静默重下几百 MB、不静默用旧文件） | 两条分支都测：**离线** → 失败在"取索引"、错误指向 `<url>.index.gz`；**在线** → 索引可下但每个缺失文件 `NotAvailable` ⇒ 编译失败且错误指向缺失文件；不带 `-C` 且在线 → 大量 `downloading`（应用须能取消/限速） | P0 | `cache.rs:191-201,271-320,351-392` |
| DIST-2.17 | 半下载/中断的健壮性 | 下载中强杀进程后重跑 | 能继续、无损坏文件 | 缓存里**不出现**零字节/半截目标文件（先写 `*-tmp-pid<pid>` 再 rename）；允许残留 tmp 但启动时应能清理 | P1 | `cache.rs:264-269,381-389` |
| DIST-2.18 | 缓存失效与升级的**单一规则** | ① 换 bundle（换 ttb / 换 URL）② 看 `formats/` 变化 | 新旧共存、互不污染 | 文件名 == `<digest>-latex-33.fmt`，digest 随 bundle 变；重编读的是新 digest 的文件（自证：删新文件会重建、删旧文件无影响） | P0 | dist.md B13 |
| DIST-2.19 | 多实例/并发安全（GUI + CLI 同时） | 两进程同时首编同一文档（共用缓存目录） | 不损坏缓存 | 两次都成功（或后到者看到正确错误）；缓存无损坏文件 | P1 | `cache.rs:264-269,299-317` |
| DIST-2.20 | 磁盘空间不足 | 缓存目录放小分区/配额制造 ENOSPC | 可见、可处置、不损坏已有缓存 | 文案含"磁盘空间"语义 + 路径；已有 `.ttb`/缓存仍可用；安装前检查阈值 = DIST-2.3 + 余量 | P1 | `[未测]` |

**3) 用户机器上的前置条件与失败可见性**

| ID | 场景 | 操作 | 期望 | 判据 | P | 证据·落点 |
|---|---|---|---|---|---|---|
| DIST-5.1 | **缓存目录不可写/不可用** | `TECTONIC_CACHE_DIR` → ① 普通文件 ② 只读目录 ③ 不存在的盘/UNC ④ 被占用 | 可见、含路径、可处置 | 应用错误文本含**我们提供的路径**与一句处置；引擎原文不够用（实测**无路径、且随系统语言变**） | P0 | `[实测]` `os error 183`；`app_dirs.rs:82-96` |
| DIST-5.2 | **网络被墙 / 代理** | ① 拒连、超时 ② 坏代理 ③ 好代理 ④ TLS 被中间人替换 | 有限等待 + 可操作错误 + **不卡死 UI** | 拒连 ≤ 秒级、超时 ≤ X s（X 待实测）；文案含"网络/代理"与处置；编译可取消；**不出现**"编译超时（120 s）"这种误导性诊断 | P0 | DIST-2.14/2.15 |
| DIST-5.3 | **磁盘空间** | 小分区/配额下首跑与建 format | 失败可见、不损坏缓存 | 文案含空间语义；已有缓存与 `.ttb` 仍可用 | P1 | `[未测]` |
| DIST-5.4 | **首跑时长 vs 产品超时**（F7） | 清缓存 + 默认超时配置下首编中文文档 | 不报"超时"、有进度 | 首跑期间状态可见"在获取资源/已获取 N 个文件"；**首跑不进入超时分支**（实测 189–214 s > 默认 120 s）；确要超时则上限必须覆盖首跑且**不自动重试** | P0 | §8.2；`AGENTS.md` roadmap ㉕ |
| DIST-5.5 | **中途取消/杀进程** | 首跑中取消编译 / 杀应用（含子进程树） | 无残留锁死、下次能继续 | 无残留 `tectonic.exe`；缓存未被破坏；重跑可成功 | P0 | `cache.rs`（tmp+rename）、`runner.rs:690-710` |
| DIST-5.6 | **杀软/企业策略拦截未签名 exe** | Defender/ASR 机器上首次运行随包 exe | 安装文档给绕行；失败可诊断 | 人工验收记录 1 项 + `troubleshooting.md` 含该场景 | P1 | ADR-0003 |
| DIST-5.7 | **中文/非英文系统区域** | 中文 Windows 上触发上述各类错误 | 我们的文案**不依赖**引擎的本地化文本 | 错误判定逻辑**不匹配引擎错误字符串**（引擎文案已实测为本地化中文）；我们自生成含路径/建议的文案 | P0 | `[实测]` `os error 183` 的本地化文案 |

### 2.5 一票否决清单（任一为真 ⇒ 不建议上线 / 该形态不得发布）

| # | 否决条件 | 一句话后果 | 对应条目 |
|---|---|---|---|
| V1 | **读到陈旧 XDV** ⇒ 项目根 PDF 不是引擎刚写出的那份（或前端跳过重载、屏幕停在旧 PDF） | 正确性事故（相当于 `runner.rs:463-466` 已实测事故重演） | P-G2 / E2.9 / INT-31 |
| V2 | **冷项目首编必失败**（`-o` 目录未自建） | 新用户第一次编译就报错 | P-G4 / INT-21 |
| V3 | **漏 `-k`/`--keep-logs`/`-p`** ⇒ Quick 恒升级 Full + 无日志 + 无页进度 | 省 40.2% 中位的机制静默失效、失败无线索 | P-G3 / P-G5 / INT-11 / INT-23 / INT-42 |
| V4 | **未知 engine 值把整份设置重置并覆盖写盘** | 用户其它设置被抹掉（降级回旧版即触发） | P-G9 / INT-13·14 |
| V5 | **文案指向 `tlmgr` / `xelatex X.ins` / "TeX Live 未安装？"** | TL-less 用户照做必然失败 | P-G8 / INT-16 / INT-56 / INT-61 |
| V6 | **真 bibtex 失败不可见**、或**按 `.blg` 摘要 / 小写 `errors were issued by BibTeX` 判 bib 成败**导致误判；或那 8 条良性假警报直通 UI | "编译成功但参考文献不对"（比报错更难排查）/ 流水线恒误报 / 每次成功构建都刷 8 条噪音 | P-G6 / E6.2 / E6.2b / INT-43 / INT-43b / INT-45 |
| V7 | **缺字（`Missing character`）无任何可见信号** | 最坏失败形态：编译成功、预览静默缺字 | P-G7 / E1.4 / INT-66 |
| V8 | **首跑被默认 120 s 超时判死**且诊断指向"疑似卡住" | 用户第一次点编译就以为软件坏了 | P-G11 / INT-47 / DIST-5.4 |
| V9 | **页数 / XDV 不兼容**（`xdv-report` 页数 ≠ 28 或 `errors` 非空） | 页哈希对拍与页级复用（② 路线）作废 | E2.1 / E2.3 |
| V10 | **SyncTeX 三判据任一不中**且没有"可见降级" | 免装 TL 环境下双向定位不可用却无人知道 | P-G17 / E4.3 / INT-54 |
| V11 | **许可门禁 G-L1..G-L9 未全绿** | 分发即带许可风险（bundle 内 LPPL/OFL + 静态链接 C 库） | P-G14 / §4.3 |
| V12 | **离线形态未过**（`-b <ttb> -C` 或预置缓存 + `-C`） | "免装 TeX Live"这条价值主张不成立 | P-G15 / DIST-2.8 / DIST-2.16 |
| V13 | **给 Tectonic 进程设了 `SOURCE_DATE_EPOCH`** ⇒ 印日期的文档输出 1970-01-01（XeLaTeX 路径不这样） | 确定性换来了肉眼可见的错误日期 | **已裁决 (b)**：Tectonic 档**不设**该变量 ⇒ 日期正确；外部转换步设 `0`；**PDF 档禁止逐字节断言**（INT-36b③ / INT-20b / E3.1②） |
| V14 | **页哈希口径未声明**（`bop.prev` 绝对偏移连锁：页 1 变长 ⇒ 原始口径报 25/26 页变化，而页 2 逐字节相同） | "只重绘变化页"整体塌陷、回归判据把偏移连带误读成"内容大片变化"；跨档/跨引擎比较不可复核 | P-G19 / E3.8（P0）；已知债 #26；Tectonic 选 D4 (b) 会让它**每天触发** |

---

## 3. 测量协议与阈值（来源 perf.md，**已按实测更正**）

> **主判据只有一条**：**「中文文档从源码到能在屏幕上看的那份 PDF」的同批墙钟中位数比值**（Tectonic 出 PDF ÷ 裸 `xelatex` 出 PDF）。绝对毫秒数跨会话可差 ±30%（§7.7 ⑤），只作记录。

### 3.1 夹具清单与覆盖矩阵

**3.1.1 主夹具（P0，全部已在磁盘上）**

| 夹具 | 路径 | 规模 | 内容特征 | 覆盖 | 现状 |
|---|---|---|---|---|---|
| `tiny` | `test_file/projects/bench/tiny/main.tex` | **1 页** | `article` 6 行，**无 CJK、无宏包** | **固定开销地板**（把"引擎固定开销"与"CJK 字体层"分离） | `[实测]` XeLaTeX cold 2307 / noop 500 / edit 1625 ms |
| `min-zh` | `_zhcmp/min-zh.tex`（Tectonic 副本 `_zhcmp/tect/min-zh.tex`） | **1 页** | `ctexart` + amsmath/amssymb/graphicx/booktabs/hyperref；行内+行间公式、三线表 | **中文 + 公式 + 表格** | `[实测]` §8.3 四档全有；Tectonic 产物 `tect/min-zh.pdf` 50,651 B |
| `thesis` | `bench/thesis`（副本 `_zhcmp/tect-thesis`） | **28 页** | `ctexbook` + amsmath/amssymb/tikz；8 章 `\include`；每章 1 图 1 式 2 cite；bibliography | **中文 + 公式 + 图 + `\include` + bibtex**（唯一同时带 bibtex 的档） | `[实测]` §8.3/§8.4 |
| `small-article` | `bench/small-article` | **3 页** | `article`（英文）+ graphicx/tikz/booktabs；1 图 1 表 2 cite + refs.bib + toc | 表/图 + bibtex 的小档 | `[实测]` XeLaTeX cold 6476 / edit 2593 ms |
| `multifile` | `bench/multifile` | **74 页** | `ctexbook` + tikz，20 章 `\include`，无 bibtex，带 toc | 多文件规模 + 页级复用 C 的标准夹具 | `[实测]` XeLaTeX cold 6465 / edit 2463 ms；**Tectonic 档 `[未测]`（= U-3）** |
| `graphics` | `bench/graphics` | — | 30 个 `tikzpicture` + 1 长表 | 图形重载 | P1；XeLaTeX cold 2631 ms |
| `large` | `bench/large`（462 KB） | 125 页 XDV | ~300 页纯文本 | 大文档压力 / 差分极端档 | P1（Tectonic 档 `[未测]`） |

**3.1.2 覆盖矩阵（别用一种夹具代替另一种）**

| 覆盖面 | `tiny` | `min-zh` | `thesis` | `small-article` | `multifile` |
|---|:-:|:-:|:-:|:-:|:-:|
| 中文正文（断行/标点/字体回退） | ✗ | ✓ | ✓ | ✗(英文) | ✓ |
| 公式 | ✗ | ✓ | ✓ | ✗ | ✗ |
| 表格 | ✗ | ✓ | ✗ | ✓ | ✗ |
| 图（TikZ/figure） | ✗ | ✗ | ✓ | ✓ | ✗ |
| 多文件 `\include` | ✗ | ✗ | ✓(8 章) | ✗ | ✓(20 章/74 页) |
| bibtex（多趟 + `.bbl`） | ✗ | ✗ | ✓ | ✓ | ✗ |
| 多趟收敛（toc/bibtex） | ✗ | ✗ | ✓ | ✓ | ✓ |

> **判据**：报告里每一行必须写清"**这行是哪个夹具、多少页、几趟、是否含 bibtex**"。缺任一项的行**不得进入阈值判定**。依据：§8.4 那行"28 页 vs 27 页、文本长度 15,227 vs 14,913"的差异就是**趟数不同**造成的。

**3.1.3 `thesis-real-hithesis`（真实学位论文模板）：当前受阻，**不得进判定****

- 阻塞点：用产品相同命令重跑，~4 分钟后报 `! I can't write on file 'body/introduction.aux'`（`\include{body/...}` 需要 `tmp/body/` 存在）→ `Emergency stop` → **latexmk/perl 进程挂住（10 分钟零 CPU）**（`design.md` §基准脚本、`modules.md §12.1 #6`）。
- 处置：① 在 ㉖ 修好前**排除在阈值判定之外**（保留为"已知不可测档"）；② Tectonic 侧要单独回答"bundle 里有没有 `hithesis.cls` / 能否按 bundle 解析"——**这是 `[未测]` 的 P0 前置**（= U-8）。
- 可测的那一半：表格/公式/图密集度由 `thesis`（28 页）代表**下界**；真实模板是**上界**，上界在 ㉖/许可审计前**不承诺任何数字**。

### 3.2 每档测量协议

**3.2.1 参数（默认值 = 现有脚本已用过的形态）**

| 参数 | 默认 | 依据 |
|---|---|---|
| 轮数 | **3** | §8.3 就是"同批交错 3 轮"；`bench.mjs` 也是 3 次取中位 |
| 预热 | **每 (夹具 × 引擎) 各 1 次，不计入** | §7.7 ①（把宏包/字体/OS 缓存焐热） |
| 交错 | **同批轮转**：外层轮次、内层夹具、再内层引擎；**引擎顺序按轮次奇偶反转**（A/B/B/A） | §7.7 ②"别先跑完 A 再跑 B（机器状态会漂）"；`bench-single-pass.mjs:82` 已用 |
| 环境固定 | `SOURCE_DATE_EPOCH=0`（全部引擎）；Tectonic 加 `-C`（**只有"首跑档"不加**） | `design.md` ㉚；§1.2 纪律 3 |
| 输出目录 | **两边都显式指定**：Tectonic `-o <scratch-tect>`、XeLaTeX `-output-directory=<scratch-xe>`；**两者都必须预先创建**；**同一次"跑两次比字节"的两轮必须用同一输出路径**（纪律 7：`-o` 路径会进产物，跨目录比对会得假阴性） | Tectonic 默认 `-o` = input 所在目录；不显式指定则 I/O 口径不等价 |
| 超时保护 | 单次 **120 s** 硬超时（与产品默认一致；上限 1800） | `settings/model.rs:39` |
| 取数 | **中位数**（只对有效样本）；同时记 min/max/**极差** | `tectonic-bench.ps1`/`bench.mjs` 同律 |

**3.2.2 逐次有效性校验 V1–V6b（**必须见到产出才计入**）**

| # | 检查 | 依据 | 为什么必须有 |
|---|---|---|---|
| V1 | 退出码 == 0 | `$p.ExitCode`（.NET Core 需**第二次 `WaitForExit()`** 才落实） | — |
| V2 | **见到产出**：XeLaTeX/LuaLaTeX 日志含 `Output written`；Tectonic = exit 0 **且**目标 PDF 存在 | `tectonic-bench.ps1:31` | 跑 4 s 的"成功"其实可能是 exit 1 |
| V3 | 页数 > 0 且与档案一致 | `pdfinfo <pdf> \| Pages` | 页数漂移是"编译成功但内容变了"的唯一低成本信号 |
| V4 | **中文夹具缺字数 == 0** | Tectonic：`--keep-logs` 后 grep `Missing character`（§8.4 实测 0）；XeLaTeX：查 `tmp/<stem>.log` | 否则"快"的一种实现就是**少排字** |
| V5 | **TeX 趟数已记录** | Tectonic：`note: Running TeX` + `Rerunning TeX` 条数（现 baseline **3 趟**）+ `Running BibTeX` 次数（**9**）；XeLaTeX 单趟=1；latexmk Full 从 `fdb_latexmk`/`Run number N of rule` 读 | 趟数不同的两个数**不可比**；bibtex 次数是隐含开销 |
| V6 | 无 `^!` 致命行（XeLaTeX 侧） | `Select-String -Pattern '^!'` | 区分"exit 0 但有错误"与真正干净 |
| **V6b** | **bib 判据必须走产物**；且必须意识到**跨引擎日志契约不同**（perf.md Rev.4 口径） | ① `-k` 档：`.bbl` 存在且 `\bibitem` 条数 == `.bib` 条目数（thesis **9/9**）且 `main.log` 的 `Citation … undefined` == **0**；② 失败档：`Citation … undefined` > 0 **或** `.bbl` 缺失；③ 辅助才用 `--print`/`--keep-logs` | Tectonic 的 BibTeX **端口未实现收尾统计段** ⇒ **成功的 `.blg` 也确定止于 `Database file #1: refs.bib`**，`You've used N entries` 全树 0 命中。**禁用**：`.blg` 摘要行、error 行计数、小写 `errors were issued by BibTeX`（成功档 8 条假阳性、失败档无信号） |

> **两栏设计**（perf.md 保留）：`timing_valid`（V1–V6b 全过）与 `output_equivalent`（产出与基线等价）。按 `[实测]`：`output_equivalent` 在 thesis 上应为**通过**（28 页 + `[1]…[9]`）。

**3.2.3 无效样本怎么记（不许静默丢弃）**

- 每条无效样本一行，字段固定：`夹具 | 引擎档 | 轮次 | 测得 ms | exit | 首条 ^! 行 | 原因枚举`。
- 原因枚举（固定集合）：`no-output` / `write-fail`（`I can't write on file …`）/ `emergency-stop` / `timeout` / `missing-glyphs` / `pagecount-drift` / `log-locked` / `crashed`。
- 汇总：**每 cell 有效样本 < 3 ⇒ 不出结论**，标 `[数据不足]` 并原样贴出无效行。
- **极差**：同 cell 3 轮 `(max−min)/median > 50%` ⇒ 判"本批不可信"、整批重跑（依据：故障期 min-zh 1370 → 5723 ms = 4.2×；正常期 < 5%）。

**3.2.4 夹具侧前置条件（P0 = P-G16，漏了就整批静默作废）**

- `\include{chapters/chNN}` 的夹具，一旦传 `-output-directory`/`-o`，**必须先建 `<outdir>/chapters/`**（`recheck.ps1:20-22`/`run4.ps1:15`/`tectonic-bench.ps1:20` 都专门建了它）。
- 每档测量前后**清理产出**（`tmp/`、PDF、`.pages`、Tectonic 落在源目录的 `.aux/.toc/.blg`）。
- **编辑类样本必须还原源文件**（`bench.mjs:145-147`）：页哈希差分对"内容变没变"是字节级的。

### 3.3 控制组与机器状态检查（**先跑，不合格就不测**）

| 组 | 命令 | 正常基线 | 故障判据 | 依据 |
|---|---|---|---|---|
| **CPU** | `lualatex --luaonly _zhcmp/bench-lua.lua 2000000` | numeric **18–24 ms**、string 92–124、table 230–253 | 任一项 > 基线 ×1.5 | §7.7 ④ |
| **磁盘 I/O** | `lualatex --luaonly _zhcmp/io-control.lua <tex-tree> <work>`（400 个 `.sty`） | 第 2 遍（热）**≥120–178 MB/s** | 热读 < 100 MB/s | §7.7 ①（冷读 1.6 MB/s **不是**故障） |
| **字体（fontconfig）** | `fc-list` 计时 + `fc-cache -v` | **连测 3 次取第 2/3 次**：~0.44–0.7 s、**3640 条**字体 | 重复出现 **4.2–4.7 s** 且 `fc-cache -v` 报 `invalid cache file: …cache-9` | `troubleshooting.md` 同名条目 |
| **进程启动地板** | `cmd /c exit`、`tectonic --version`、`xelatex --version`、`lualatex --version` | 19 ms / 32 ms / 495–690 ms / 16–123 ms | `xelatex --version` >1.5 s 且同组其他正常 ⇒ 指向 XeTeX 基座/format/fontconfig | §7.7 ① |

**控制组自身必须先预热（`[实测]` 方法学结论）**：本轮 `fc-list` 三连测 = **3862（冷）/ 545 / 446 / 441 ms**；首次 3.9 s 落在故障区间（4.2–4.7 s）**边缘**，单次值会把健康机器误判成 fontconfig 故障。⇒ **规则：控制组每条连测 3 次、判据取第 2/3 次**（第 1 次单独记录，用于说明进程/缓存冷启动）。这条与夹具预热是两件事：**夹具预热焐的是宏包与字体缓存，控制组预热焐的是探针自己的进程与缓存文件**。

**故障处置**：`fc-list` 重复 4.2–4.7 s + `invalid cache file` ⇒ 先 `fc-cache -f` 修复再测（修好后极简文档 4799→790 ms、`min-zh` 5380–5765→1371 ms）；沙箱内 `fc-cache` 得 `Permission denied` ⇒ **不在沙箱里测性能**；同批只有 XeLaTeX 慢而 Lua 微基准正常 ⇒ 判 fontconfig 故障、**当批所有绝对数字作废**；LuaLaTeX 报 `no writeable cache path` ⇒ 环境不合格（换可写 `TEXMFVAR`）；LuaLaTeX 首跑 25.8 s ⇒ **不是故障**（建字体库 23.6 MB/35 文件）。

### 3.4 阈值（**只在同批有效样本上判**）

**口径**：`R = median(Tectonic 默认档出 PDF) ÷ median(xelatex 出 PDF)`，同一夹具、同一批、同一轮换顺序。

| # | 夹具 | 判据 | 已有实测 | 不可达时怎么办 | P |
|---|---|---|---|---|---|
| **R1** | `min-zh`（1 页） | **≤1.0 优秀 / ≤1.5 及格 / >1.5 失败** | §8.3：1586 vs 2350 ms ⇒ **R = 0.675** | 稳定 >1.5 ⇒ 判"Tectonic 在小文档上不划算"，写进 §6.1 风险 | P0 |
| **R2** | `thesis`（28 页，含 bibtex） | **≤1.0 优秀 / ≤1.15 及格 / 1.15–1.30 观察带（须复测）/ >1.30 失败** | §8.3：2515 vs 2539 ms ⇒ **R = 0.990** | 先看趟数与 bibtex 次数；趟数一致仍 >1.30 ⇒ 判"中文中档退化"、标 P0 风险 | P0 |
| **R3** | `multifile`（74 页，多文件） | **首测只记基线、不判定**；随后沿用 R2 的 1.15/1.30 | **无 Tectonic 实测**（§8.3 未覆盖）；XeLaTeX 侧有 §2 的 2189 ms（`-no-pdf`）与 6465 ms（Full） | 首测出基线后**必须由复核人重设阈值**（不许拿 R2 的数字直接当结论） | P0（新增测量 = U-3） |
| **R4** | 任意中文档 vs **只排版** | **首选同引擎同夹具** `默认档 − xdv 档` = PDF 后端成本（干净口径）；跨引擎 `Tectonic ÷ (xelatex -no-pdf)` **≤2.0** 仅作诊断列 | §8.3：thesis 2515 vs 1681 ⇒ **1.50**；min-zh 1586 vs 1378 ⇒ 1.15。⚠️ 跨引擎这列**含趟数差**（Tectonic 3 趟 + 9 BibTeX vs `xelatex -no-pdf` 1 趟）⇒ 不能当作"PDF 工作 = 800 ms"的证据 | 超限 ⇒ 按干净口径定位是 PDF 后端还是趟数；本行**不判"不达标"** | P1 |
| **R5** | 任意中文档 vs **LuaLaTeX**（出 PDF） | **R₅ ≤ 1/1.5 = 0.67**（即 Tectonic 至少快 1.5×） | §8.3：2515 vs 5932 ⇒ **0.424（快 2.36×）** | 掉到 1.5× 以下 ⇒ 需重述 Tectonic 的价值主张（它的理由本来就是"免装 TL + 可复现"，不是"更快"） | P1 |

> **R2 的方向性说明（必须写进报告，否则会被误读）**：§8.3 那次 Tectonic 计时**含 9 次 bibtex 子进程 + 3 趟 TeX + 内部 xdvipdfmx**，而对照的 `xelatex` 单趟基线**不跑 bibtex** ⇒ 该比值**对 Tectonic 保守**，"打平（0.990）"的结论比表面更稳。但正因工作量不同，**2515/2539 只能当阈值依据，不能当"同工作量对拍"**。
>
> **显式禁令（P0）**：R1–R5 的分母固定为**裸 `xelatex` 单趟出 PDF**。**不许中途换成 `latexmk -xelatex` Full**（`design.md` 的 `thesis` cold 9412 / edit 3370 ms）——分母里多了 perl 启动 + 多趟收敛，**同一台机器上 R 会系统性变小**，两组数字**不可混用**；换基线必须整批重跑并重设阈值。
>
> **相对回归阈值（比上表更重要）**：同一夹具、同一引擎、同一形态，本次中位 vs **上一次固化基线** —— **恶化 ≤10% 视为噪声内；>20% 判回归；10–20% 标"观察"并要求第二次复测**。依据：`design.md` 复现性"两轮独立运行关键档差异 <5%"（tiny edit 1617→1625、multifile edit 2461→2463）；§7.7 ⑤"跨会话绝对值可差 ±30%" ⇒ **10% 是"同机器同会话"的噪声带，不是跨会话的**。

### 3.5 页级复用命中率（**判据随 D2 路线分叉**）

**前提**（`incremental-edit-x-dvi.md` §2.3 成立条件 + 本文纪律 5/7）：必须是**同一强度、同一收敛状态**的两次编译；页哈希是**字节哈希**；命中率只反映"少做下游重复工作"，**不代表排版变快**。**所有"两次对拍"必须同一输出路径 + 固定 epoch**（否则字节级比较会得假阴性），且**页哈希须声明口径**（原始含 `prev` / 归一化，P-G19）。

| 功能点 | 命中率定义 | 判据 | 已有实测 | Tectonic 侧注意 | P |
|---|---|---|---|---|---|
| **B 跳过预览重载** | `H_B = 命中"pages>0 且 changed_pages 为空"的次数 ÷ 无可见变化编辑的次数`（编辑形态固定：文件末尾追加 `% comment`） | **①（默认路线）**：`pages == 0` ⇒ **不适用**，判据改为"每次全量刷新、`reloadKey` 递增、绝不误判无变化"（INT-32①）；**②（启用页哈希的档）**：`H_B = 100%（3/3）` + `reloadKey` 不递增、`skippedReloads` +1。**两者都必须声明页哈希口径**（原始含 `prev` / 归一化，P-G19） | `incremental-edit-x-dvi.md` §2.1/§2.3（125 页加注释 → 0 页变化；真机 `reloadKey 2→2`、`changedPages []`） | ①② 都不许跨档混比页哈希；**追加注释若落在页 1 之前不影响长度 ⇒ 两种口径同结果；一旦首页长度变化，只有归一化口径才反映"内容未变"** | P0 |
| **C 只重绘变化页** | `H_C = 1 − changed_pages.length / pages`（编辑形态固定：**末章等长替换**） | **①**：不适用（同上，判据见 INT-33①）；**②**：`multifile`（74 页）**H_C ≥ 0.90**（changed_pages ≤ 7 页）+ `pagesReused` 上升、`render` 到 ~10 ms 量级；**必须声明口径**（若编辑使**任意前页变长**，原始口径会把其后所有页都算"变化"⇒ H_C 塌陷，须按 P-G19 判定是"内容变化"还是"偏移连带"） | §2.3 C 实测：改末章 → `pagesRendered 7→0、pagesReused 0→7、render 102ms→7ms、total 183→71ms` | 编辑形态必须固定：**等长替换 1 页变 / 章首插入 108 页变 / 只加注释 2 页变** —— 换个编辑位置能把 H_C 从 0.99 打到 0 | P0 |
| **A 跳过 PDF 转换** | `H_A = 命中"跳过 xdvipdfmx 转换"的次数 ÷ 页哈希全同的次数` | **Tectonic 无独立步骤**（① 路线下转换在引擎进程内；② 路线下转换虽在外部但每次都要做）⇒ 判据改为**同引擎同夹具的 `默认档 − xdv 档` 差值 = PDF 后端成本**（**不要**用 §8.3 的 2515 vs 1681 当该差值 —— 那两档趟数不同）；② 路线的外部转换成本已有实测锚点：**+1,091 ms/次**（队长 2026-09-14） | XeLaTeX 侧 `H_A = 100%`；省 `xdvipdfmx` **0.65–0.94 s/次** | A 明确标"该引擎不支持" | P1 |

### 3.6 首跑成本：**bundle 下载与 format 构建必须分开计**

| 指标 | 判据 | 依据 | 不可达时怎么办 | P |
|---|---|---|---|---|
| **M1 联网懒下载首跑**（空缓存） | `min-zh` **≤240 s**（实测 214 s，余量 12%）；`thesis` **≤90 s**（实测 51 s）；`hello` ≤210 s（实测 189 s）。同时记录**下载行数**（实测 104 / 73 行） | §8.2 | 超限 ⇒ 判"懒下载形态不可接受"，切 T4 的另两条路线（预置缓存 / 本地 `.ttb`），并把首跑成本显式做进 UI（进度 + 说明） | P0 |
| **M2 format 构建**（bundle 已热、`cache\formats\` 被清） | **阈值待定，先测基线**；预设上限 **≤60 s `[推断]`**（依据：现 `.fmt` = **24,451,466 B（23.3 MB）**，构建是"引擎初始化 + 宏包加载"，量级应远小于 214 s 的下载） | **从未记录过耗时**（本轮只复核到产物存在：`6ffe0558…-latex-33.fmt`，mtime 2026-09-13 23:32:59） | 超限 ⇒ **预置 `formats\*.fmt` 必须与预置 bundle 同批**（文件名含 bundle 摘要，换 bundle 即失效） | P0（测量 = U-4） |
| **M3 `-C` 离线**（bundle+format 都热） | **必须成功**，且耗时 ≤ 该档热态中位 × 1.1；日志 **0 行** `downloading` | §1.2 纪律 3；`[实测]` 热缓存 `-C` 1 页 → 429 ms / 零下载 | 失败 ⇒ 判"离线不可用"（本地 `.ttb` 路线是唯一出路） | P0 |
| **M4 `-b <本地 bundle> -C`** | 产物与默认 bundle **等价**（页数、缺字数 0），体积与耗时另行记录 | `-b` 可指 URL 或路径；**真 ttb 从未实测** | 未通过 ⇒ 分发形态退回"预置缓存目录" | P1（T4 主责，本册只登记测量点） |

> **首跑档单独一批**：允许单次、允许长（≥120 s 也要跑完，**不要套 120 s 超时**），不与热态交错（避免一次 214 s 把同批热样本的机器状态拖走）。

### 3.7 对拍口径：与 XeLaTeX / LuaLaTeX 比什么

| 层 | 定义 | 用途 | 判据归属 |
|---|---|---|---|
| **L1 用户视角（主判据）** | 源码 → **PDF 落地**的墙钟（单次进程调用；Tectonic 默认档，`xelatex` 单趟出 PDF） | 决定"换引擎值不值" | §3.4 R1/R2/R3/R5 |
| **L2 只排版** | 源码 → **排版结束、不含 PDF 后端**：`xelatex -no-pdf`（1 趟）；Tectonic 侧真正对齐单趟的是 **`--pass tex`**（**不是** `-r 0`，也不是 `--outfmt xdv`） | 分离"排版成本"与"PDF 后端成本" | §3.4 R4（诊断） |
| **L3 端到端（产品口径）** | `debounce(500ms) + 编译 + 预览重载` | 与 `design.md` §延迟预算对齐 | `design.md` 预算（小文档 1 s 优秀/2 s 及格；大文档 3 s 优秀/5 s 及格）；预览重载取真机实测（`multifile` 115 ms、31 页 62–69 ms） |

**单趟 vs 收敛（最容易比错的一处）**

| 引擎 | 默认形态实际几趟 | 怎么强制"单趟" | 本方案记法 |
|---|---|---|---|
| `xelatex`（裸调） | 1 趟 | 已经就是 | `passes=1` |
| `latexmk -xelatex` | 收敛（多趟） | 不用它 | 从 `fdb_latexmk`/日志读 |
| `lualatex`（裸调） | 1 趟 | 已经就是 | `passes=1` |
| **Tectonic** | **内部自己收敛**：现 baseline **3 趟 TeX + 9 次 BibTeX**（上限 `DEFAULT_MAX_TEX_PASSES = 6`） | 三个档分清：`-r 0` = 1 趟 TeX（**仍跑 bibtex**）；`--outfmt xdv` = **3 趟**（只是省 xdvipdfmx）；**真正单趟 = `--pass tex`**（不跑 bibtex、不出 PDF） | 三个数都记：`default`（进 R1–R3/R5）、`--outfmt xdv`（进 R4 同引擎差值）、`--pass tex`（与 `xelatex -no-pdf` 对拍） |

> **单趟档不能用来判"产出等价"**：带 `\tableofcontents` 的夹具单趟跑出来的目录页码**落后一趟**（实测 `multifile` 插 400 行后 `.toc` 里「第四章」页码由 29 → 41，即该趟 PDF 显示旧值 29；再跑一趟才稳定）。⇒ **分工**：单趟数字只进 **R4（成本诊断）**；**页数、页哈希、页级复用、SyncTeX 往返、产出等价性一律只在"收敛档"上判**。`thesis`/`multifile`/`small-article` 都带目录 ⇒ 不能假设 Tectonic 默认档是单趟。

**已有基线可直接复用（逐条给出处；`[实测]`，来自别的会话/批次 ⇒ 只能作阈值依据与复现目标，不能与今天的绝对值直接相减判回归）**

| 档 | 值 | 出处 |
|---|---|---|
| Tectonic `min-zh` / `thesis` | **1586 / 2515 ms** | §8.3 |
| `xelatex -no-pdf` | 1378 / 1681 ms | §8.3 |
| `xelatex` 出 PDF | 2350 / 2539 ms | §8.3 |
| `lualatex` 出 PDF | 3887 / 5932 ms | §8.3 |
| 中文单趟（27 页 / 74 页） | xe **1870 / 2189** vs lua **6468 / 7111**（3.5× / 3.2×） | §7.4、§2 |
| 英文同结构单趟 | xe 2341 vs lua **1878**（LuaLaTeX 反而快） | §7.4 |
| 仅导言区（固定开销） | xe 1565（占单趟 84%）vs lua 2869（44%） | §7.4 |
| PDF 后端成本 | `xdvipdfmx` ≈ **1125 ms** vs LuaLaTeX 引擎内写 PDF ≈ **288 ms** | §7.4 |
| 完整编译（latexmk 收敛首跑） | xe **9193** vs lua **16686** ms（1.8×） | §7.4 |
| LuaLaTeX 首用 | **25.8 s / 23.6 MB / 35 文件** | §2.1、troubleshooting |
| 产品侧六档（设计基线） | tiny 2307/500/1625、small-article 6476/502/2593、multifile 6465/495/2463、graphics 2631/499/1826、large 13532/486/4104、thesis 9412/507/3143（cold/noop/edit，ms） | `design.md` 基准表 |
| 单趟 vs Full | 平均省 **40.2%**（阈值预设 20%） | `bench-single-pass.json` |
| 页级复用 / 确定性 / 预览 | 见 §1.4 对应行 | `incremental-edit-x-dvi.md` §2.1–2.3、§8.4 |

---

## 4. 许可与分发门禁

### 4.1 发行形态（只列差异与风险；不做实现）

| 维度 | **M1 随包分发 exe**（形态 B：子进程驱动，**建议默认**） | **M2 用户自装**（走 PATH，现状） | **M3 crate 内嵌**（形态 A） |
|---|---|---|---|
| 用户前置 | 零（不需要 TeX Live） | 需 TeX Live/MiKTeX | 零 |
| 版本控制权 | **我们**（钉 0.17.0） | 用户（可能是旧版 ⇒ bundle URL/`-b` 语义可能不同） | 我们（随 Cargo 依赖树） |
| 体积/构建成本 | exe **51,538,432 B**（压缩包 **21,060,223 B**）+ bundle 体积（`[未测]`） | 0 | 构建链极重：vcpkg + harfbuzz 子模块 + 30+ C 依赖 |
| 许可面 | 我们成为"MIT 本体 + 逐包混合许可 TL 文件 + 静态链接 C 库"的**再分发者** ⇒ §4.2 全部适用 | 无（不分发） | 同 M1（静态链接发生在我们的产物里） |
| 架构边界 | 与现状（引擎=子进程，ADR-0010）兼容 | 同上 | 引擎进 core/infra 边界要重划（ADR-0010 需修订） |
| 失败可见性 | 我们可控（自己构造命令行、自己解析输出） | 依赖用户环境 | 错误变成 Rust 错误，但要重写日志/进度/页事件 |
| 与 ADR 的关系 | 与 ADR-0003（不签名分发）叠加 ⇒ **未签名 exe + 未签名安装包**，SmartScreen 双重门槛 | 无 | 不改上游，但 CI 变重 |

> **结论倾向（供决策）**：M1 为默认路线（"零前置"就是产品价值），M2 保留为"用户已有 TL"的退化路径，M3 只在确需**内存内 XDV / 页事件**时才谈。**M3 不做**（N4 非目标）。

### 4.2 许可：已知事实、风险分档与三张审计清单

**已知事实（可引用，但都不是结论）**

| # | 事实 | 风险 | 证据 |
|---|---|---|---|
| L1 | **Tectonic 本体 MIT**（"Copyright 2016-2023 the Tectonic Project"；`Cargo.toml` 亦声明 MIT） | 低（保留版权与许可文本） | `test_file/tectonic-src/LICENSE`、`Cargo.toml:28` |
| L2 | **bundle = TeX Live 文件集合**：逐包混着 LPPL/GPL/MIT/OFL…（TL 整体无单一许可证） | 中（需逐包清单 + 义务摘要） | `bundle.toml` 输入即 `texmf-dist` |
| L3 | **bundle 内被 patch 的 4 个 TL 文件**：`latex.ltx`、`listings.sty`、`fontawesome.sty`、`fithesis-mu-base.sty`（LPPL 类对"修改后的文件"有明确条款） | **中高** | `patches/texlive/*.diff`；缓存里的 `latex.ltx` 已含 patch 文本 |
| L4 | **bundle 里几乎没有许可证文本**：`ignore` 剔除了 `LICENSE.md`/`README`/`readme.txt`/`doc/.*`/`source/.*` | 中（必须自己产出 NOTICE） | `bundle.toml:9-24,69-70` |
| L5 | **字体**：`fonts//` 在 search_order 内（OFL 有"保留字体名/改名"与随附要求）；中文实测不依赖 bundle 字体 ⇒ **裁剪 CJK 字体是同时降体积与降许可面的杠杆** | 中 | `bundle.toml:77-88`；§8.4 |
| L6 | **C 依赖走 vcpkg 全静态**（`x64-windows-static-release`：fontconfig/freetype/harfbuzz[graphite2]/icu 等）⇒ **静态链接把义务带进我们的发行物** | **中高**（LGPL 组件如 graphite2 是典型风险，**具体结论待审计**） | `Cargo.toml:147-174`、`x64-windows-static-release.cmake` |
| L7 | **只分发上游预编译 exe 也不免除义务**（再分发者仍需履行） | 中 | 集成方案 §6 |
| L8 | 我们自己仓库是 **MIT**（ADR-0003：MVP 前私有、之后开源）⇒ 降低"LGPL 重新链接"类义务的难度，但**不能替代逐项审计** | — | 根 `LICENSE`、ADR-0003 |

**三张审计清单（做法）**

- **清单 A｜Rust 全依赖**：`cargo metadata --format-version 1 > deps.json`，提取 `license`/`license_file`，**列出空值**；判定 = 每条依赖都有结论（白名单/需声明/需法务），**空值不得留白**。
- **清单 B｜bundle 文件 → TL 包 → 许可证**：从我们自己的 ttb 抽索引（`dd … | gunzip`，或直接读 `content/FILELIST`）→ `tectonic -X bundle search` 列表化 → 用**同版本 TL 的 `texlive.tlpdb`**（含每包 `license=` 字段）做归属映射。判定 = FILELIST 里**每个文件都能追到一个包与一个许可证**；追不到的（自造文件、`TAR-SHA256SUM`、`tectonic/*.tex`）单独列出定性。**前置待确认**：`texmf.tar` 里是否含 `tlpkg/texlive.tlpdb` `[未测]`（= U-20）。
- **清单 C｜二进制与 C 依赖**：上游 exe 的 URL + SHA-256 + 版本（zip `21,060,223 B`/`f61ce51f…845f`；**解包 exe `51,538,432 B`/`99ffcfdb…bf08d` —— 发行物里放的是这个 exe，它的哈希才是必须钉的**）；vcpkg port 的 `vcpkg.json` 与 LICENSE。判定 = 每个 C 依赖的许可证 + 链接方式有记录；**若存在仅 LGPL 的静态链接组件**，必须给"义务满足方案"三选一（附必要材料／改动态链接／构建时裁掉）。

**风险分档（审计脚本按此分档，避免"全是高风险"式无效报告）**

| 档 | 许可 | 处置 |
|---|---|---|
| R1 放行 | MIT / BSD / Apache-2.0 / zlib / libpng-2.0 / FTL / X11 / Unicode-3.0 / public-domain / CC0 | 只需并入 NOTICE 清单 |
| R2 需声明 | OFL-1.1（字体：随附 + 保留名称）、LPPL-1.3c（宏包：副本 + **修改后的文件**条款）、GPL-2.0/3.0（聚合分发不改我们许可，但**修改**过则须提供对应源码与修改说明） | 逐条给义务动作 + 落到 NOTICE/About 页 |
| R3 需法务 | LGPL（静态链接）、AGPL、许可不明/多重许可冲突、无许可证文本 | **阻断发包**，直到有书面结论 |
| R4 排除 | 仅随 `doc/`/`source/` 分发且**已被 bundle 忽略**的内容 | 记录"为什么可以不管" |

> **待审计标记**：L2/L3/L5/L6/L7 的**具体结论**（能否闭源分发、需要哪些声明、是否必须改名/附全文）一律标 **「待审计」**，**不得**当结论使用。产出物：`docs/research/tectonic-bundle-licensing.md`（必含：范围与版本、清单 A/B/C 三表、结论（分 M1/M2/M3）、随附文件清单、**遗留待审计项**——禁止用"应该没问题"收尾）。

### 4.3 审计门禁 G-L1..G-L9（**全绿前不得把 Tectonic/bundle 放进发行物**）

| ID | 检查 | 判定方式（二进制/可 grep） | P |
|---|---|---|---|
| G-L1 | `docs/research/tectonic-bundle-licensing.md` 存在且**结论小节完整** | 文件存在；"待审计/未结论"字样**不出现在结论小节**（遗留项集中在小节 6） | P0 |
| G-L2 | 清单 A/B/C 三张表都非空且**无空许可证字段** | 表行数 > 0；空值计数 == 0 | P0 |
| G-L3 | 发行物含第三方声明（`THIRD-PARTY-NOTICES` 或等价物） | 安装包解包后文件存在；About 页可达（人工 1 项） | P0 |
| G-L4 | Tectonic 本体许可文本随发行物（MIT 全文 + 版权行） | NOTICE 含 "the Tectonic Project" 与 MIT 全文 | P0 |
| G-L5 | 若含 OFL 字体：许可证原文随附；字体文件被改过则未用 OFL 的保留字体名（RFN） | 字体文件与许可证逐项对照（人工 1 项，记录判定人） | P0 |
| G-L6 | 4 个被 patch 的 TL 文件：**修改说明 + 归属 + 许可影响**三项齐全 | 审计文档有该小节且三项非空 | P0 |
| G-L7 | C 依赖无"未结论的 R3 档" | 清单 C 中 R3 行数 == 0，或每行有"义务满足动作 + 责任人" | P0 |
| G-L8 | 结论已进 ADR（建议新增"Tectonic 集成与分发许可"）并被发布检查清单引用 | ADR 文件存在且状态"已接受"；发布清单有一条指向 G-L1..G-L7 | P0 |
| G-L9 | 每次**换 bundle/TL/引擎版本**后重跑 G-L1..G-L8 | 审计文档头部"版本与摘要"与本次发布物一致（可 diff 判定） | P0 |

### 4.4 bundle 流水线门禁 MB-1..MB-8（**必须做：上游有三处不拦**）

> **上游三处"静默成功"**：① `patch` 子进程的 `wait()` 返回码**没有被检查**（`picker.rs:196`，patch 不匹配也照样产出未打补丁的包）；② `pack` 在 `content/` 缺失时只打 error 日志后 `return Ok(())`（`bundle/actions.rs:110-116`，**exit 0**）；③ 最终 hash 与 `expected_hash` 不一致时**只 warn**（`actions.rs:89-96`）。⇒ **流水线门禁不能看退出码。**

| ID | 门禁 | 判据 | 为什么必须 | P |
|---|---|---|---|---|
| MB-1 | tarball 哈希 | 日志出现 `OK, tar hash matches bundle config`；**绝不使用 `--allow-hash-mismatch`** | 不匹配时上游会 `bail!`（这条是硬的），但该参数会把它降成 warn | P0 |
| MB-2 | patch 全部落地 | ① `patch_found == patch_applied` ②**逐个人工断言**：`content/.../latex.ltx` 含 `Tectonic: no terminal input allowed`（另 3 个 patch 各写一条） | patch 返回码未被检查 ⇒ 打不上也照样出包 | P0 |
| MB-3 | 最终摘要 | 日志出现 `final bundle hash matches configuration` 且 hash == `expected_hash`；**只看这一行，不看退出码** | hash 不匹配时上游只 warn | P0 |
| MB-4 | `content/SHA256SUM` 自校验 | 独立算 `sha256(content/FILELIST) == content/SHA256SUM`；且存在 `TAR-SHA256SUM`（== TL tarball 的 sha256） | 这是"TL 版本 + 全部 patch"的唯一传递性指纹 | P0 |
| MB-5 | ttb 产物字节 | `<build>/<name>/<name>.ttb` 存在且 > 0；头部 14 字节 == `tectonicbundle`、u32 版本 == 1、头部第 34..66 字节 == `content/SHA256SUM` | `pack` 缺 `content/` 时 exit 0 | P0 |
| MB-6 | 冒烟（用**运行期**命令形态） | ① `tectonic -C -b <ttb> --outfmt xdv` 编 `min-zh`+`thesis` 成功 ② `--outfmt fmt` 能建 latex/plain 格式 ③ 页数与基线一致（28 页） | 打包器与运行期同代码，但"能建包"证明不了"能编译" | P0 |
| MB-7 | 可复现性 | 同机同 tarball+patches **重跑一次**：`expected_hash` 与 `content/SHA256SUM` 完全相同 | "版本钉法"的前提；**ttb 逐字节可复现 `[未测]`**（gzip 层） | P0 |
| MB-8 | 许可清单随产物 | 每版 ttb 都产出/更新 `docs/research/tectonic-bundle-licensing.md` 与发行物 NOTICE | 见 §4.3 | P0 |

**升级/维护（谁负责，怎么升）**：上游 relay 域名与 Azure 订阅**由单一个人持有**（供应链单点；上游曾因"archive.org 被中国屏蔽"换服务）；官方 bundle 目前落后本机 TL 两代（TL2024 vs TL2026）⇒ **新宏包可能不在包里**。升级最小动作清单：① 改 `bundle.toml`/patches → ② 跑流水线 → ③ 过 MB-1..MB-8 → ④ 更新发布清单（ttb 大小 + SHA-256 + `expected_hash`）→ ⑤ 重跑 DIST-2.8 离线冒烟 → ⑥ 复核许可清单是否新增包/新许可。**责任人槽位**：待确认（建议 Rust/构建维护者 1 人 + 发布检查清单勾选项）。

### 4.5 上线门禁 G-1..G-8（P0 汇总，按本方案判据）

| 门禁 | 内容 | 判据来源 |
|---|---|---|
| **G-1** | 许可审计通过（G-L1..G-L9 全绿，含发行物 NOTICE） | §4.3 |
| **G-2** | 运行期**不依赖外网**的形态已实测：`-b <ttb> -C` 在空缓存 + 断网下编出中文 PDF；预置缓存形态同样过 | DIST-2.1 / DIST-2.8 |
| **G-3** | 三类环境失败（缓存不可写 / 网络不可达 / 磁盘不足）都有可见、含路径、可处置的错误 | DIST-5.1 / 5.2 / 5.3 |
| **G-4** | 版本三件套（exe ↔ bundle 摘要 ↔ `FORMAT_SERIAL`/formats 后缀）有唯一真相源 + 不一致时拒绝运行 | DIST-1.2 / DIST-2.18 / P-G18 |
| **G-5** | 体积、安装、升级、卸载、缓存清理行为已实测记录（含"缓存目录归谁"的产品决策） | DIST-1.3 / 1.4 / 2.3 / 2.10 |
| **G-6** | 上游 bundle/relay 不可用时的降级路径已定义并测过（自建 ttb 或预置缓存；含 F3 负测） | DIST-2.16 / §4.4 |
| **G-7** | 首跑（下载/建 format）不撞产品超时，且有进度与取消 | DIST-2.12 / DIST-5.4 |
| **G-8** | 流水线门禁 MB-1..MB-8 可自动跑（CI 或发布脚本），**不看 `bundle create` 的退出码** | §4.4 |

---

## 5. 执行顺序与里程碑

### 5.1 里程碑（每阶段给 entry/exit 判据）

| 阶段 | 内容 | 入口条件 | 退出判据（可判定） |
|---|---|---|---|
| **M0 门禁探针**（最快出结论） | P-G1 三形态落盘对拍；P-G13 工具纪律；E2.8/E2.10 趟数；U-2（`--pass tex` 落盘） | 无（本机即可） | P-G1 的四档落盘文件集记录完毕；U-2 有结论；D2 路线与 §1.2.2 一致 |
| **M1 四个前置** | 许可审计（清单 A 先做，纯本地）→ `-b <ttb> -C` 离线实测 → SyncTeX 往返对拍 → 日志/`-p` 适配调研 | M0 完成 | G-L2（清单 A 无空值）；DIST-2.8 exit 0 + `cache/bundles` 不存在；E4.3 三判据全中**或**落地了 INT-54 的可见降级；P-G5 选定 `-p` 且 E5.3/E5.5 有结论 |
| **M2 引擎层矩阵** | §2.2 全量（E1–E7） | M1 的日志适配结论 | E1.1/E2.1/E2.4/E2.6/E3.1 全绿；E7 失败面退出码与文案固化 |
| **M3 集成层接入与矩阵** | §2.1 的"必须先修"项（P-G3/P-G4/P-G6/P-G7/P-G8/P-G9/P-G12）+ §2.3 全量 | M2 通过 | V1 全绿（§2.5 一票否决清单无命中）；INT-22/23/30/31 全绿；D1/D3 落地 |
| **M4 测量与阈值** | §3 全量：夹具 + 协议 + 控制组 + R1–R5 + M1–M4 | M3 通过（否则测的是坏形态） | 控制组合格；每 cell ≥3 有效样本；R1/R2 判定完成、R3 只记基线；M2（format 耗时）有数字 |
| **M5 分发与终审** | §4 全上门禁 + G-1..G-8 | M4 完成 | G-1..G-8 全绿；一票否决清单无命中；发布检查清单含 G-L1..G-L7 与 MB-1..MB-8 |

### 5.2 四个前置（任务要求的"先做"清单）

1. **许可审计**：先跑**清单 A**（`cargo metadata` + 空值清单，纯本地、无网络），能在拿到 bundle 清单之前先消掉最大的一块不确定性（Rust 侧许可面）。
2. **`-b <本地 bundle> -C` 离线实测**（= DIST-2.8）：**注意不是本地 `.tar`**（F1 已实测否定），要用 `<name>.ttb`。它同时解锁 G-2、验证 F1 的替代方案、给 DIST-2.3 提供体积数字。
3. **SyncTeX 往返等价性**（= E4.3/P-G17）：现在只验到"有产物 77,983 B"；**G2（无 synctex 解析器/CLI）不解决则本条不可执行** ⇒ 与"是否随包分发 TL 工具（`synctex.exe`/`xdvipdfmx.exe`）"这个决策（**D5**）绑定。
4. **日志/实时反馈适配**（= G1/P-G5）：确认 `-p` 的 `[N]` 是否逐字节流式、`note:/warning:` 分层是否够用；不够就必须重设计流式通道（现有主力通道 `tmp/<stem>.log` 尾随在 Tectonic 下**命中 0 次**）。

### 5.3 最省钱的三个下一步（排期用）

1. **造一个 ttb 并跑 DIST-2.8**（一条离线编译）：依赖联网机器 + 一次 §4.4 流水线；收益最大（解锁 G-2）。
2. **补 DIST-2.16 的负测**（手改 `hashes/<url>` 摘要 + `-C`）：成本极低（只改文件），直接决定"预置缓存"路线能否上线、还是必须改随包 ttb。
3. **启动许可审计清单 A**：纯本地。

### 5.4 决策记录与待决槽位（**D1–D4 均已裁决**；D5–D9 待产品定）

| ID | 决策 | 现状 / 判据 |
|---|---|---|
| **D1** | 失败是否自动回退 | **已裁决（2026-09-14）：不回退**；失败必须可见 + 有 TeX Live 时给「一键切到 XeLaTeX 重编」（INT-90） |
| **D2** | 页哈希取数路线 | **已定论（实测）**：`pdf -k` 拿不到 `.xdv` ⇒ 默认路线 ①（要 PDF、放弃 B/C）；② 需自备转换器且与 TL-less 冲突（§1.2.2） |
| **D3** | 状态栏是否显示引擎 | **已裁决：显示**，且同处体现 Quick/Full 强度（INT-17） |
| **D4** | **`SOURCE_DATE_EPOCH` 的引擎语义差异 —— 已裁决：(b) Tectonic 档不固定 epoch**（用户裁决 2026-09-14；按"**已裁决记录**"写，不再列三选项） | **理由**：同一变量在两引擎语义不同 —— XeLaTeX 只动 PDF `/ID`、**正文零变化**；Tectonic **改正文**（`\today`）——且实测**不设 epoch 时 XDV 仍逐字节稳定**（`63531F94…4BAC`，264,736 B）⇒ **(b) 不摧毁 A/B/C**。**落地口径（P0 前提 = T2 `INT-20b`）**：`SOURCE_DATE_EPOCH` **按子进程施加** —— **Tectonic 进程不设**、**同轮外部 `xdvipdfmx` 进程设 `0`**（㉚ 的确定性由后者承担）；现实现**无条件设置**（`runner.rs:275`）⇒ 落地时必须改，且该断言**现在应当 FAIL**（反例自证）。**代价（两条）**：(i) PDF 档放弃逐字节确定性，且仅 `/ID`/Info 元数据层面（⇒ **"PDF 逐字节相等"禁止作为 Tectonic 档断言**，INT-36b③/E3.1②）；(ii) 会印 `\today` 的文档**跨天时页哈希变化**（牵连机制 E3.7②/已知债 #26）。判据 = INT-36b / INT-20b / E3.6 / E3.7 / **E3.9 / E3.10**；**确定性只断言在 XDV 与转换步上**（`epoch=0` 的转换步连跑 3 次逐字节相同 = 79,065 B/`6F7EFC14…`，E3.9①）；**PDF 逐字节相等不得断言**（E3.10③）；**修复空间（分步 env 两全 = U-31）保持"待实测"，不升格为已证** |
| **D4 的框架性提醒** | **这不是 Tectonic 新问题**：`bop.prev` 绝对偏移连锁是 **`docs/modules.md` 已登记的已知债 #26**（连"日期串差一位 ⇒ 页 1 +10 B、页 2 相同、页 3–26 差异只在偏移 43–44"这个样本都同款） | 差别只在**触发频率**：XeLaTeX 侧固定 epoch ⇒ 日期永不变 ⇒ 该债几乎不触发；**Tectonic 选 (b) 会把它从"理论问题"变成"每天发生一次"** ⇒ 这是 D4 真正的决策依据（详见 §0.1 纪律 6 / P-G19 / E3.8） |
| **D4 落地口径**（建议，**待实测 U-31**） | **epoch 按步施加**：**Tectonic 进程不设**（⇒ 日期正确）、**我们调的 `xdvipdfmx` 进程设 `SOURCE_DATE_EPOCH=0`**（⇒ PDF 逐字节可复现）⇒ 路线② **理论上可同时得到"正文日期正确 + PDF 可复现"** | 落地位置 = §1.2.2 路线② 的**命令构造**（分步环境变量）+ 本节 D4；**未实测，勿当已证** |
| **D5** | 是否随包分发 TL 工具（`synctex.exe` / `xdvipdfmx.exe`），或自研解析 | 待定；判据 = P-G17/INT-54/INT-38（TL-less 下必须"可见降级"或"自带能力"） |
| **D6** | 缓存目录归谁（我们独占 vs 共享用户 Tectonic） | 待定；判据 = DIST-2.10（两处目录文件数+mtime 对比） |
| **D7** | 首跑超时策略（提高默认超时 vs 首跑单列预算） | 待定；判据 = P-G11/INT-47（不得判死 + 诊断方向正确） |
| **D8** | bundle 落地形态（预置缓存 vs 随包 ttb） | 待定；由 G-2/DIST-2.16 结果决定 |
| **D9** | 是否引入 `-p/--print` 作为实时通道（有 stdout 噪音成本） | T2 Rev.2 已按"必带"给判据；最终形态待 E5.5 结论 |

> **编号说明**：T2 已把「`\today` / epoch」作为正式条目编号 **D4** 提交队长，故本文把其余待决槽位顺延为 **D5–D9**；两处编号若冲突，以本节为准（D1–D3 的裁决见下）。

---

## 6. 风险与未验证清单

### 6.1 风险表（按"后果严重度 × 现状证据"排序）

| # | 风险 | 后果 | 现有处置 / 判据 | 依据 |
|---|---|---|---|---|
| R-1 | **TL-less 下 SyncTeX 不可用**（G2） | 免装 TL 的用户点了 PDF 也跳不到源码，且可能只在 console 报错 | P-G17 + INT-54（可见降级）、D5 决策、E4.3 三判据 | Tectonic 26 个 crate 无 synctex；`synctex.rs:60-68` 起外部 exe |
| R-2 | **编译中零实时反馈**（G1） | 大文档/首跑时状态栏静止数分钟，用户以为卡死 | P-G5（`-p` 必带）、INT-40/41、E5.5 | 产物在内存层、进程结束才落盘（`driver.rs:1121-1205`） |
| R-3 | **"要 PDF 就没有页级复用"**（D2） | 产品预期落差：Tectonic 档下 B/C 退化为全量刷新 | §1.2.2 显式取舍 + INT-32/33 的 ① 分支判据 | 队长 2026-09-14 实测（默认/`-k` 都不落 `.xdv`） |
| R-4 | **引擎自述日志不完整**（`.blg` 尾部丢失、`main.log` 无 `This is XeTeX`、`.log` 结束才落盘） | 判定器按日志判成败会**误判**（恒红/恒绿） | E6.2（判定器自测）、E5.1 附注、V6b 改产物级、U-26 | 队长 `[实测]`：`.bbl` 9 条 vs `.blg` 无摘要行 |
| R-5 | **上游 relay 单点 + bundle 落后两代 + 预置缓存硬失效**（F3） | 供应链单点；新宏包可能不在包里；镜像刷新会让预置缓存整体失效 | G-6、DIST-2.16 负测、D7（随包 ttb） | `crates/bundles/CHANGELOG.md:64-88`；`cache.rs:191-201` |
| R-6 | **许可面**：bundle 内被 patch 的 TL 文件 + OFL 字体 + 静态链接 C 库（可能含 LGPL） | 审计不过 ⇒ 不能发包；事后返工成本最高 | G-1/G-L1..G-L9、清单 A/B/C | dist.md §5（全部"待审计"） |
| R-7 | **首跑成本与超时策略冲突** | 第一次点"编译"看到"超时 + 一键提高超时"，而真正在做的是下载 | P-G11、INT-47、DIST-5.4、D6 | §8.2：189/214/51 s > 默认 120 s |
| R-8 | **三个必现集成坑**（`-o` 目录、`-k`、`-p`） | 冷项目首编必失败 / Quick 恒升级 / 无页进度 | P-G3、P-G4、P-G5（均 P0 且带反例自证） | 队长实测 + T2 Rev.2 |
| R-9 | **文案指向本机 TL 工具** | TL-less 用户照做必然失败 | P-G8、INT-16/56/57/61 | `diagnosis.rs:383,394,136`；`runner.rs:321,322,538` |
| R-10 | **搜索根差异**（嵌套根文件 / 子目录 `\include`） | 现有 latexmk 支持嵌套根，Tectonic 可能不支持 ⇒ 行为回归 | INT-71/72（最高风险未验证项）+ U-14 | `driver.rs:1123-1141` |
| R-11 | **未知 engine 值重置整份设置** | 用户降级回旧版或手改配置即被抹掉全部设置 | P-G9、INT-13·14 | `storage.rs:41-67` |
| R-12 | **陈旧 XDV 覆盖** | 屏幕停在旧 PDF 还报"成功" | P-G2、E2.9、INT-31（"本轮产物凭证"） | `runner.rs:463-466` 已实测事故（109,732 → 70,193 B） |
| R-13 | **`SOURCE_DATE_EPOCH` 在两引擎语义不同**（XeLaTeX 只动 PDF `/ID`；Tectonic **改正文** `\today` → 1970-01-01） | 若照抄无条件设置，标题页等 `\today` 文档会印 1970；两个引擎在同一产品设置下行为不同 | **已裁决 (b)**：Tectonic 档不设、转换步设 `0`（INT-20b）；**确定性只断言在 XDV/转换步上**（E3.7/E3.9/E3.10）；跨次比对须**同路径 + 固定 epoch**（§0.1 纪律 5/7） | T2 `[实测]` + T1 E3.6–E3.10 `[实测]`：13,559 vs 13,560、首个差异 @18；`epoch=0` 转换步 3 次全等（79,065 B） |
| R-14 | **`bop.prev` 绝对偏移连锁使"变化页集合"失真**（页 1 变长 ⇒ 原始口径报 25/26 页变化） | 若产品/脚本不声明口径，"只重绘变化页"会整体塌陷、回归判据会把偏移连带误读成"内容大片变化" | **P-G19 / E3.8**（口径必须声明；归一化口径可收窄到真正变化的那一页）；这是**已知债 #26**，Tectonic 选 D4 (b) 会让它**每天触发** | T1 E3.7/E3.8 `[实测]`+`[源码]`；`xdv.rs:211`；`docs/modules.md` 已知债 #26 |

### 6.2 未验证清单 U-1..U-31（**不得当结论使用**；P0 项 = 15 条）

| # | 项 | 为什么关键 | 一条命令 / 最小做法 | P | 归属 |
|---|---|---|---|---|---|
| **U-1** | 陈旧 XDV 是否会让项目根 PDF 变质（E2.9/P-G2） | 正确性事故 | 同一目录先造 `.xdv` 再跑 FULL，比 PDF SHA-256 | P0 | 引擎/集成 |
| **U-2** | `--pass tex` 是否真落 `.xdv`（**源码推断已被同类证伪一次**） | 决定 ④ 档能否用于 L2 对拍 | 干净目录跑④，列产物 | P0 | 引擎 |
| **U-3** | `multifile`（74 页）在 Tectonic 上的基线 | R3 阈值无依据 | 按 §3.2 协议跑一档 | P0 | 测量 |
| **U-4** | 冷 format 构建耗时（M2） | 首跑成本的一半从未量过 | 备份并移走 `cache\formats\*.fmt` → 计时 → 还原 | P0 | 测量/分发 |
| **U-5** | `-C` 缺文件 与 断网（不加 `-C`）的真实退出码与文案 | 失败面判据（E7.2/E7.3） | 两条命令各跑一次 | P0 | 引擎 |
| **U-6** | **真正坏的 `.bib`**（缺失/语法错）的 exit code 与 UI 文案 | 旧判据已废，bib 失败面需重新建立 | 造坏 `.bib` 跑一次 | P0 | 引擎/集成 |
| **U-7** | "编译失败 ⇒ exit 1" 且失败时 `.log` 是否含 `!` 行 | E7.0/E7.1 | 造缺包文档 | P0 | 引擎 |
| **U-8** | `-p` 的 `[N]` 是否**逐字节流式**、是否与 `note:` 混流 | 决定实时页进度可行性（P-G5） | 记每个 `[N]` 到达时间 | P0 | 引擎/集成 |
| U-9 | `--outfmt xdv --synctex` 下 `Output:` 实际值 | **仅作排障信息（不作判据**，§0.1 纪律 4）；决定 `synctex view -o` 传什么 | 跑一次读头部 | P1 | 引擎 |
| U-10 | `fontset` 非 windows 档的缺字行为 | 回退健壮性（E1.3） | 四档各编一次 | P1 | 引擎 |
| U-11 | `-r 2` 的趟数计数（`-r 0` 已由 T2 Rev.2 探针实测为 1 趟且出 PDF） | E6.4 | 跑一次数日志 | P1 | 引擎 |
| **U-12** | 本地 bundle 合法形态（目录/`.zip`/`.ttb`）的端到端 | E7.5/DIST-2.8 | 造三份各编一次 | P0 | 分发 |
| **U-13** | Rust 与 JS 页哈希的**语义一致性**（算法本就不同） | E2.2 | 同一 XDV 双跑对拍 | P0 | 引擎 |
| **U-14** | 搜索根行为（嵌套根文件、`-Z search-path` 可用性） | **本方案最高风险未验证项** | INT-71/72 两组夹具 | P0 | 引擎/集成 |
| U-15 | Tectonic 编 `.ins` 生成 `.cls` | INT-61/62 | 跑一次 docstrip | P1 | 集成 |
| U-16 | bundle 内是否有 `fandol` | INT-63 建议是否成立 | `tectonic -X bundle search` | P1 | 分发 |
| U-17 | `--makefile-rules` 产物能否支撑"精确失效" | INT-51 | 跑一次读 `.deps` | P1 | 集成 |
| U-18 | IoError 三条文案改造后的回归面（现状被真机测试引用 3 处） | 改文案会连带改测试 | grep 命中位置并同步 | P1 | 集成 |
| **U-19** | SyncTeX 往返等价性（Tectonic vs XeLaTeX 基线） | E4.3/P-G17 | `latteset-cli forward/inverse` 对拍 | P0 | 引擎/集成 |
| U-20 | `texmf.tar` 是否含 `tlpkg/texlive.tlpdb` | 决定清单 B 的自动化程度 | 解包查路径 | P1 | 分发 |
| **U-21** | 逐包许可清单 与 C 依赖 LGPL 结论 | **阻塞发包** | 清单 A/B/C（§4.2） | P0 | 分发 |
| U-22 | 代理/证书在真实企业网的行为、relay 可达性（本沙箱 HTTPS 被拒） | 企业网可用性 | 三种代理配置 | P1 | 分发 |
| **U-23** | 自建 ttb 体积 / `bundle create` 在 Windows 的可复现性 / ttb 逐字节可复现 | DIST-2.3、MB-7 | 建包两次比 hash | P0 | 分发 |
| U-24 | `%APPDATA%\TectonicProject\Tectonic\config.toml` 的用户级 bundle 覆盖对我们命令行的实际影响 | 用户装了别的 bundle 时会漂 | 造一份 config.toml 编一次 | P1 | 分发 |
| U-25 | 磁盘空间阈值 / 多实例并发 / 杀软拦截 / 卸载残留 | DIST-1.4/2.19/2.20/5.6 | 四条场景各一次 | P1 | 分发 |
| U-26 | 其它"引擎自述日志"是否同样不可信（`.blg` 的机制**已定**：端口不实现 `You've used` 统计块、干净运行无收尾汇总 ⇒ 不再是未知项） | 决定还能不能信任这类信号（`main.log` 无 `This is XeTeX` 抬头、`.log` 结束才落盘、`Skipped writing N intermediate files` 的语义） | 统计多次运行的 `.log`/`.blg` 形态差异并写进 G6 清单 | P1 | 引擎 |
| U-27 | `run-xdv.log` 记 **264,736 B** 与现存 `main.xdv` **264,728 B** 差 8 B（C-12） | 若属同源同批，则该档的确定性判据会挂 | 同批双跑 xdv 档，记字节数 + SHA-256 | P1 | 引擎 |
| **U-28** | **TL-less 机器（ENV-B）从未测过** | 价值主张、G2、文案、`writes_xdv()==false` 收尾路径全押在它 | 干净 Windows 跑 E1.1/E4.3/INT-16/P-G8 | P0 | 全体 |
| U-29 | **中间产物存在性矩阵**与既有不变量的完整对应（`.aux`(443 B)/`.toc`(1623 B)/`.bbl`(680 B) 只在 `-k` 档出现；还有哪些不变量建立在 latexmk 的落盘契约上） | 单点映射（INT-22b）之外还可能有别的隐式依赖 | 对 ① 路线枚举 `tmp/**` 产物并逐条对照 `modules.md` 的行为契约 | P1 | 集成（perf.md Rev.4 U12 的扩展） |
| U-30 | **真实学位论文模板（hithesis 档）受阻**：`body/introduction.aux` 写不进 → Emergency stop → latexmk 挂住；Tectonic 侧还要先答"bundle 里有没有 `hithesis.cls`" | 真实模板是"图表/公式密集"的上界，当前**不承诺任何数字** | ㉖ 修好后重测；Tectonic 侧先 `-C --keep-logs` 试编并读缺包错误 | P1 | 测量/分发（§3.1.3；`modules.md §12.1 #6`） |
| U-31 | **D4 的"两全"路径**：epoch 按步施加（Tectonic 进程不设 + 外部 `xdvipdfmx` 设 `0`）能否同时得到"正文日期正确 + PDF 逐字节可复现" | 决定路线② 是否比 ① 多一个决定性优势；也是 D4 (c) 的实现口径 | ② 路线跑两次（跨天各一次），比 PDF SHA-256 与 `pdftotext` 日期 | P1 | 集成/引擎（队长 2026-09-14 提议；**标为待实测，勿当已证**） |

**已闭环、不再列为未验证的项（避免重复劳动）**：默认档是否落 `.xdv`（含 `-k`）→ **不落**，机制 = `driver.rs:1985` 内存移除（队长实测 + engine.md Rev.2，C-1）；bibtex 主调用是否失败 → **成功**（`.bbl` 9 条 / PDF `[1]…[9]`，C-2）；`.blg` 为何没有摘要行 → **端口不实现该统计块**（全树 0 命中）；**章级 aux 是否写出同名 `.bbl`** → **不会**（章级恒空、日志有 `Not writing 'chapters/chNN.bbl': it would be empty.`，主 `.bbl` 唯一；engine.md Rev.2 E6.2d，两个独立探针目录）；`-b <本地 .tar>` → **不成立**（实测）；`-C` 是否等于离线 → **不等于**（实测）；`xdv-report.mjs --diff` 空格形态 → **已修**（两种形态都支持、错用 exit 2）；页级复用能否与默认 PDF 档共存 → **不能**（D2 定论）；P0/P1 划分 → 已由本文统一。

---

## 7. 复现资产清单（tracked vs gitignored 必须分清）

| 类别 | 计划落点 | 入库 | 理由 |
|---|---|---|---|
| **测量 harness** | `scripts/bench-tectonic.ps1`（由现成 `test_file/projects/bench/_zhcmp/tectonic-bench.ps1` 提升）+ `scripts/bench-tectonic-report.mjs`（汇总/阈值判定/退出码） | **入库** | `test_file/projects/` 与 `test_file/research/` 均被 `.gitignore:66,69` 覆盖 ⇒ 脚本留在那里 = **换机器/清仓即丢**（现有 `_zhcmp/*.ps1` 正处于这个状态） |
| **控制组探针** | `scripts/bench-lua.lua`（CPU）、`scripts/io-control.lua`（磁盘）——由 `_zhcmp/` 提升或由 harness 内联 | **入库** | 同上 |
| **夹具副本生成** | 扩展 `scripts/gen-bench-projects.mjs`：顺带产出 `_zhcmp/tect`、`_zhcmp/tect-thesis`、`_zhcmp/thesis`、`_zhcmp/multifile` | **入库** | 现在这几份是**手抄**的（`_zhcmp/min-zh.tex` 与 `_zhcmp/tect/min-zh.tex` 同为 1007 B，但没有生成器保证同步）⇒ 夹具漂移会直接变成"阈值漂移" |
| **原始样本 / JSON 报告** | `test_file/research/bench-tectonic.json`（每次运行的 ms、exit、passes、页数、缺字数、**bib 产物判据**、无效样本、控制组值、机器指纹） | 不入库（同 `bench-report.json` 律） | 体积与机器相关 |
| **精炼基线（权威）** | ① `docs/design.md` 基准表**加一列 Tectonic**（同一张表里才可比）；② 本文 §3.4/§3.7 | **入库** | 唯一能跨机器/跨清仓存活的基线 |
| **现场证据（日志/产物）** | `_zhcmp/logs/`、`_zhcmp/tect*/`、`test_file/projects/_t2-probe/`（T2 探针） | 不入库 | 已在 gitignore 覆盖面内 |
| **四份分册明细（附录）** | `docs/research/tectonic-plan-parts/{engine,integration,perf,dist}.md`（**已复制**，原 `test_file/tectonic-plan-parts/` 保留） | **入库** | 本文是"合并 + 裁决"，逐条证据与出处仍在分册；清仓后明细不丢 |
| **许可审计产物** | `docs/research/tectonic-bundle-licensing.md` + 发行物 `THIRD-PARTY-NOTICES` | **入库** | §4.3 门禁 G-L1/G-L3 |

**harness 入口约定（建议）**

```text
scripts/bench-tectonic.ps1  -Rounds 3 -Tiers min-zh,thesis,multifile -Out test_file/research/bench-tectonic.json
                              # 探针 + 控制组（三连测取 2/3 次）+ 同批交错 + 逐次校验(V1..V6b) + 无效样本表
scripts/bench-tectonic-report.mjs --input <本次 json> --baseline <固化 json>
                              # 判定 R1..R5 / M1..M4 / H_B,H_C → 退出码 0（通过） / 1（回归）
```
（两者均为**计划新增**；现有 `scripts/bench.mjs`、`bench-single-pass.mjs`、`check-determinism.mjs`、`xdv-report.mjs` 不动。）

**机器指纹（每次写进 JSON）**：`platform/arch`、`node -v`、引擎版本三件套（`tectonic --version`/`xelatex --version`/`lualatex --version`）、TL 年份、CPU/内存型号、Tectonic 缓存文件数与字节数、`formats/*.fmt` 名字、fontconfig 缓存 mtime、控制组四组值。**依据**：§7.7 ① 那次故障复盘 —— 没有"fontconfig 缓存 mtime + `fc-list` 计时"这两条，就分不清"电脑变慢"与"缓存坏了"。

**按改动面分层的最小回归集（避免每次全跑）**

| 改动面 | 最小回归集 |
|---|---|
| 引擎命令构造 / `Engine::Tectonic` 分支 / 收尾逻辑 | ① `scripts/bench-tectonic.ps1 -Rounds 3 -Tiers min-zh,thesis`（含控制组）② `node scripts/check-determinism.mjs` ③ `node scripts/xdv-report.mjs <tmp/*.xdv> --diff=<基线.xdv>`（**必须见到差分行**） |
| 页级复用 / 预览 | `multifile` 的 §3.5 三组判据 + 真机 `window.__previewLastReload`（`skippedReloads`/`changedPages`/`pagesReused`/`render`） |
| 默认引擎仍是 XeLaTeX（回归保护） | `node scripts/bench.mjs`（超预算退出码 1） |
| 分发 / bundle / 缓存 | §3.6 M1–M4 + §4 门禁（首跑档单独一批） |
| 任何改动（提交前） | `npm run build`、`cargo test -p latteset-core`、`cargo test -p latteset-infra -- --ignored` |

---

## 8. 裁决台账：四份分册之间的矛盾 / 重复 / 缺口

### 8.1 矛盾（C）

| ID | 矛盾 | 涉及分册 | **裁决** |
|---|---|---|---|
| **C-1** | **`-k` 与 `.xdv` 落盘**：T1 G5/E2.8 的旧版与 T3 §0-6 依据 `driver.rs:1615-1623`（`!keep_intermediates && …`）推断"`-k` 会短路跳过分支 ⇒ `.xdv` 必落盘 ⇒ 探针会得到假『是』"；队长 2026-09-14 干净目录实测**推翻**该推断（默认档与 `-k` 档都不落 `.xdv`） | engine / perf | **以实测 + 源码机制为准**：PDF 档**从不 materialize `.xdv`**，原因是 `xdvipdfmx_pass` 结束时把 XDV 从**内存文件表移除**（`driver.rs:1985`）⇒ 与 `keep_intermediates` 无关；`-k` 只多落 `.aux/.bbl/.toc`。旧"短路机制段"作废。结论落点：§1.2 纪律 1/4、E2.8②、P-G1、C-10。**注意**：T1 关于"默认档不落 `.xdv`"的**结论**成立，只是旧版**理由**写错了（engine.md Rev.2 已自行更正为 G5 的内存移除机制） |
| **C-2** | **bib 链路成败**：T1 E6.1/E6.2 与 T3 V6b 判"bibtex 中止/失败"（依据 `.blg` 无摘要行）vs 队长实测：`.bbl` 680 B / 9 条 `\bibitem`、PDF 28 页含 `参考文献`+`[1]…[9]` ⇒ **主调用成功** | engine / perf | **以实测为准**：改成正向验收（E6.1）+ 新增"判定器本身要测"（E6.2/E6.2b）+ 章级良性警告说明（E6.2c/E6.7）。**第二轮机制**：`You've used N entries` 这个串在 Tectonic 源码树里 **0 命中**（G6）⇒ 成功的 `.blg` 也永远没有该摘要行；端口只在有警告/错误时才写 `(There were N …)`（`engine_bibtex/src/lib.rs:397-416`）⇒ **干净运行本来就没有收尾汇总**。判据一律走产物；凡引用 `.blg` 摘要行/error 行计数的规则**必须删掉**（engine.md Rev.2 已自行更正，perf.md 的 V6b 见本文 §3.2.2） |
| **C-3** | **"A/B/C 无需改代码"**（`modern-engines-zh.md` §8.4、集成方案 §2）vs T2 INT-31（A 无对应物）vs D2 实测（默认档无页哈希） | 文档 / 集成 / 测量 | 该表述**必须加两项限定**：① 只对**能拿到 XDV 的档**（xdv 档 / `--pass tex`）成立；② 只对**页哈希解析代码（B/C）**成立 —— **A 在 Tectonic 下无对应物**；默认 PDF 档下 B/C 也**没有输入**。判据落点：INT-31、INT-32①、INT-33① |
| **C-4** | **Quick 的等价物**：T1 E2.10（"`--outfmt xdv` ≠ Quick"）vs T2 原文（`-r 0`）vs 集成方案 §3 摩擦点 3（"`--outfmt xdv` 当只排版档"） | engine / 集成 / 文档 | **唯一真相源 = §1.2 四档表**：`-r 0` 是 Quick（1 趟 + bibtex + PDF）；`--outfmt xdv` 是 L2 诊断档（3 趟、无 PDF）；`--pass tex` 才是真单趟（无 PDF、无 bibtex） |
| **C-5** | **阈值分母**：T3 R1–R5 用"裸 `xelatex` 单趟出 PDF" vs `design.md` 基准表用 `latexmk -xelatex` Full | 测量 / 文档 | **固定为同批中位比值，分母 = 裸 `xelatex` 出 PDF**；**显式禁令**：不许中途换成 latexmk Full（分母变大 ⇒ 比值系统性变小）；两组数字不可混用（§3.4） |
| **C-6** | **`-b` 的输入格式**：集成方案 §1 路线②/§5-2 写"本地 tar" vs T4 F1 实测（只认 目录/`.zip`/`.ttb`） | 文档 / 分发 | **以实测为准**：集成方案两处原文作废，改 `-b <name>.ttb`（E7.5、DIST-2.8） |
| **C-7** | **`-C` 的语义**：集成方案 §1"运行期完全不联网" vs T4 F2 实测（冷缓存仍拉 `<url>.index.gz`）；另外 `-C` 与首跑互斥 | 文档 / 分发 | **以实测为准**：真正离线 = 预置缓存 + `-C`，或本地 `.ttb` + `-C`；产品命令构造**不能一律加 `-C`**（§1.2 纪律 3、DIST-2.1/2.2/2.16） |
| **C-8** | **超时诊断退化的优先级**：T1 E7.7 = P0 vs T2 INT-46 = P1 | engine / 集成 | **合并为一条 P0**：引擎层负责"输入退化"的事实（`.log` 不落盘 ⇒ 无页证据），集成层负责"必须有替代证据"（用 `-p` 的 stdout 页标记），阈值判定不因此变化 |
| **C-9** | **回退的产品选择**：T2 §8 给"选择 A（不回退）/ 选择 B（回退）二选一，必须显式选" vs human 裁决 D1 | 集成 / 裁决 | **按 D1 写**：失败**不自动回退**、失败必须可见、有 TL 时给「一键切到 XeLaTeX 重编」。**全文删除"静默回退"**；T2 的"选择 B"降级为附注（若将来改选，判据见 INT-90 的行内说明） |
| **C-10** | **D2 路线**：T2 INT-37 原文默认 Q1（`pdf -k`）vs T1 G5 警告"`-k` 会得到假『是』" | 集成 / engine | **显式产品取舍（§1.2.2）**：① `pdf -k -p`（默认，要 PDF、放弃 B/C）② `--outfmt xdv` + 自备转换（保 B/C，但与 TL-less 冲突）。T1 的"假『是』"警告针对的是"**默认档是否落 XDV**"这个问题，**不是**"`-k` 档是否落" —— 两问必须分开写（这也是 C-1 的教训） |
| **C-11** | **页级复用命中率的分母**：T2 §3 只说"生效"vs T3 §4.2 定义 H_B/H_C | 集成 / 测量 | **分母与阈值归 T3（唯一来源）**，T2 只写"通过/不通过 + UI 事件断言"；且 ① 路线下 H_B/H_C **没有输入**，判据换成降级断言（INT-32/33 的①分支） |
| **C-12** | **XDV 字节数不一致**：§8.4 记 264,736 B（页字节 1038/9432/16101）vs 现存 `main.xdv` 264,728 B（T1 三份 SHA-256 全等、页字节 1028/9431/16101）；`run-xdv.log` 的 `Writing ... (258.53125 KiB)` = 264,736 B | 文档 / engine | **以文件实测 + SHA-256 为准**：本文统一用 **264,728 B / 28 页 / `738F96DD…71FEB`**；264,736 标注为"另一处日志行的数字，待核"（U-27）。**不允许**在同一份报告里混用两套页字节统计 |
| **C-13** | **`-p` 是否必带**：T2 原文未含 vs T2 Rev.2"必带" | 集成 | 采 **Rev.2**：`-p` 必带（页进度唯一来源），代价是与 `note:/warning:` 分层解析同做（INT-20、P-G5、E5.3） |
| **C-14** | **`-Z deterministic-mode`**：E3.3 只说"与 SyncTeX 冲突" vs T2 Rev.2 要求禁用该开关 | engine / 集成 | **禁用**该开关，只用 `SOURCE_DATE_EPOCH=0`（INT-67；E3.3 保留为"为什么不用"的证据） |
| **C-15** | **路线 ② 的定位**：早前口径"② 是保住 B/C 的路线"（未提 TL 依赖）vs T1 E2.12/E2.13 实测（Tectonic XDV 字体是**裸名**，解析靠 TL 字体树 + kpathsea，`xdvipdfmx.exe` 也来自 TL） | engine / 集成 | **以实测为准**：② **不是"免装 TL 的出路"**，而是"**有 TL 机器上的可选转换路径**"（与 G2 同类）；凡"② 可脱离 TL"的表述一律删除（§1.2.2、INT-38） |
| **C-16** | **单趟 vs 收敛混比**：早前把 ② 的输入写成"`--pass tex`（1 趟）"vs 实测"单趟 XDV **26 页**、收敛档 **28 页**" | 测量 / 集成 | **以实测为准**：**② 的转换输入必须来自收敛档（`--outfmt xdv`，28 页）**；`--pass tex`（26 页）只作 L2 成本诊断（§1.2.2 命令构造、INT-35） |
| **C-17** | **`SOURCE_DATE_EPOCH` 的性质**：最初（含 T3 旧稿）写成"确定性的代价/已知取舍"vs T1 E3.6 + T2 Rev.7 实测（XeLaTeX 只动 `/ID`、Tectonic **改正文**） | engine / 集成 / 测量 | **以实测为准**：改为"**同一变量在两引擎语义不同**"⇒ **必须按引擎分支**（`runner.rs:275` 无条件设置，不加分支就出错）；落点 §0.1 纪律 5、E3.6、INT-36b、§5.4 D4、R-13 |

### 8.2 重复（DUP）——归属仲裁

| ID | 重复的三处 | 归谁 |
|---|---|---|
| DUP-1 | 首跑与超时：T2 INT-47/81/87 + T4 DIST-2.12/5.4 + T3 M1/M2 | **数字与形态归 T4**、**测量协议归 T3**、**用户可见性与文案归 T2**；本文只留一条 P-G11 + 交叉引用 |
| DUP-2 | Quick/Full 命令构造：T1 E2.10/E6.4 + T2 INT-20/23/28 | **命令行语义归 T1（= §1.2 四档表）**；**应用侧映射与 UI 语义归 T2** |
| DUP-3 | SyncTeX：T1 E4.3/E4.4 + T2 INT-53/54/68/69 | 引擎层"等价性与口径"归 T1；集成层"可见降级与竞争"归 T2；两条都是 P0，判据不重复 |
| DUP-4 | 页哈希落盘：T1 E2.8/E2.9 + T2 INT-11/30/31 + T3 U1 | 统一由 **P-G1/P-G2 定义一次**，其余条目只引用 |
| DUP-5 | 夹具清单：T2 §10"需要 T3 夹具" + T3 §1 | **T3 §1 是唯一来源** |
| DUP-6 | 页级复用 A/B/C：T2 §3 + T3 §4.2 | 分母/阈值归 T3；T2 只写通过/不通过 + 事件断言（= C-11） |
| DUP-7 | "文案指向 `tlmgr`"：T2 INT-16/56/57/61 + T4 DIST-1.5 的版本提示 | 统一门禁 **P-G8**；两处保留各自层判据（引擎缺失 / 缺包） |

### 8.3 缺口（GAP）——本文补齐

| ID | 缺口 | 补齐方式 |
|---|---|---|
| GAP-1 | 四份分册都只给"测试项"，**没有"接入前必须解决/先测"的门禁清单** | 本文新增 **§2.1 P-G1..P-G19（全 P0）**，其中 P-G1/P-G2 是 D2 的判定输入 |
| GAP-2 | **TL-less 机器从未被测过**（全部实测都在装有 TL2026 的机器上） | 本文新增 **ENV-B 环境**（§1.3）+ U-28；要求至少在 TL-less 上跑 E1.1/E4.3/INT-16/P-G8 |
| GAP-3 | **harness 落点**：`test_file/projects/` 与 `test_file/research/` 被 gitignore ⇒ 清仓即失 | §7：提升到 `scripts/`（tracked），含命名与入口约定；夹具生成并入 `gen-bench-projects.mjs` |
| GAP-4 | 没有"**引擎自述日志不完整**"这一测试类别 | 新增类别：`.blg` 尾部丢失（E6.2/U-26）、`main.log` 无 `This is XeTeX` 抬头（E5.1 附注）、`.log` 结束才落盘（E5.2）、`Skipped writing N intermediate files` 的语义（E2.8） |
| GAP-5 | 没有**产品决策槽位** | 本文新增 §5.4：**D1–D4 已裁决**（含 D4 = (b) 分步 epoch），**D5–D9 仍待决**（是否随包分发 TL 工具、缓存目录归属、首跑超时策略、bundle 落地形态、`-p` 通道形态） |
| GAP-6 | 没有**环境矩阵**（区域/代理/沙箱/TL-less） | §1.3 ENV-A..ENV-E + 环境纪律（不在沙箱里测性能、测量期不跑 GUI） |
| GAP-7 | 分册明细在清仓后可能丢 | 四份分册已**复制**入 `docs/research/tectonic-plan-parts/` 并在 `docs/README.md` 补链接（附录） |

---

## 9. 附：编号映射、附录与被覆盖的分册章节

### 9.1 编号映射

| 本文 | 分册原文 | 说明 |
|---|---|---|
| §2.1 `P-G1..P-G19` | 新增 | 由四册归纳；与分册条目交叉引用（P-G2↔E2.9/INT-31；P-G6↔E6.1/E6.3/INT-43…） |
| §2.2 `E1.1..E7.8` | engine.md §2（**ID 不变**，含 E2.11/E2.12/E2.13/E3.6–**E3.10**/E6.2b–d/E6.7） | 判据压缩、结论按 C-1/C-2/C-15/C-17 更正；旧"短路机制段"与旧 bib 结论已在本文标注被实测覆盖；**E3.7–E3.10 与本文 P-G19、§0.1 纪律 5/6、§5.4 D4 同源** |
| §2.3 `INT-10..INT-96` | integration.md **Rev.10**（460 行 / 88 条）§1–§8（**ID 不变**，另有后缀 ID `INT-20b/31b/36b/38b/43b/54b`） | 已按 D1–D4 与 C-1..C-4 改写；合并且保留原 ID 的行标注为 `INT-x·INT-y` |
| §2.4 `DIST-1.1..DIST-5.7` | dist.md §2/§3/§6（**ID 不变**） | 判据压缩，P0 计数与队长口径（22 条）一致 |
| §3 `R1..R5 / M1..M4 / H_A,H_B,H_C / V1..V6b` | perf.md §1–§7（**ID 不变**） | V6b 与 R2 的方向性说明按 C-2 更正；U1 已闭环 |
| §4 `G-L1..G-L9 / MB-1..MB-8 / G-1..G-8` | dist.md §5.4/§4.3/§7（**ID 不变**） | 原样收录，补责任人槽位 |
| §6.2 `U-1..U-31` | engine.md §4 + integration.md §11 + perf.md §7 + dist.md §8 | **合并去重**；已闭环项在本节末列明 |

### 9.2 被本文（或实测）覆盖 / 需修订的分册章节（读分册时请注意）

| 分册 | 章节 | 状态 |
|---|---|---|
| engine.md | §0 G5 的**旧版**"机制"段（`driver.rs:1615` 的 `!keep_intermediates &&` 决定项、"`-k` 会得到假是"） | **已被实测证伪**（C-1）；**Rev.2 已自行改为 `driver.rs:1985` 的内存移除机制**（正确）；"默认档不落 `.xdv`"的结论成立 |
| engine.md | E6.1 / E6.2（旧版判 bibtex 中止、bundle 版本落后为根因） | **Rev.2 已更正**为"主调用成功 + `.blg` 无统计块"（G6）；本文采用 Rev.2（C-2），并新增 E6.2b/E6.2c |
| engine.md | §4 未验证清单 A-1（"探针禁止用 `-k`"） | 表述需改：`-k` 与 `.xdv` **无关**；纪律改为"默认档探针不得加 `--outfmt`"（§1.2 纪律 4） |
| engine.md | **refreeze（285 行 / `B13CCA10…`，终版）**：新增 **E3.7**（D4 三选项的引擎层代价）、**E3.8**（页哈希口径）、**E3.9**（转换步确定性 + 输出路径纪律：只对转换产物成立）、**E3.10**（D4 分步口径与断言边界） | **本文已收**（§2.2 四行 + §5.4 D4 + §0.1 纪律 5/6/7 + P-G19 + R-14 + V14）；E3.7 同时**撤回了**旧版"不固定 epoch ⇒ B/C 退化为每次全量刷新"的错误描述 |
| perf.md | §0 第 6 条、§2.2 的 "V6b"、§7 的 U1/U11 | U1 已闭环；V6b 判据改产物级（§3.2.2）；U11（biber/bibtex 全链路）按 C-2 重述 |
| perf.md | §2.1/§4.3 的 Tectonic 命令形态 | 需补 `-p`（C-13）与"`-C` 与首跑互斥"（C-7） |
| integration.md | §2 映射表（未含 `-p`）、§3（旧版"B/C 生效判据"）、§8（"选择 A/B"） | **Rev.2/Rev.3/Rev.4 已修订**（含 D1 的 INT-95/96、D2 的 INT-35/38/39、E6.2d 的 `.bbl` 定位规则）；本文以 Rev.4 为准（C-9/C-10/C-13） |
| dist.md | §0 F1（`-b` 本地 tar）、§0 F2（`-C` 语义） | 与集成方案原文冲突处，**以 dist.md 实测为准**（C-6/C-7） |
| `docs/research/modern-engines-zh.md` | §8.4「XDV 就是我们的格式、A/B/C 无需改代码」、§8.5 结论 1 | **必须加限定**（C-3）：只对 xdv 档 + 页哈希解析代码成立 |
| `docs/research/tectonic-integration-plan.md` | §1 路线②、§5 第 2 条（本地 tar）、§1 表格（"运行期完全不联网"） | **原文作废**（C-6/C-7）；建议按本文口径修订该文档 |

> 本方案只出方案，不改产品代码；上述"需修订"只标注状态，实际修订留给对应文档的维护任务。

