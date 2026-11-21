# 0016 随包分发内置宏包集（离线开箱）

- **状态**：已接受（2026-11-16，产品负责人）；**带门禁**：G-L1..G-L9 未全绿前不得进入发行物。
  **2026-11-16 实测修正**：完整默认 bundle ≈ **2.88 GB** ⇒ 决策 1（随包**完整**宏包集）不可行。
  **2026-11-20 重定（已落定，③段）**：决策 1 改为 **候选 A「随包精简集 + 缺包补全通道」**；离线首编实测 exit 0。**T16 时** NSIS 安装包 **48.27 MiB（不含 licenses）**，**T21 加许可资源后 88.43 MiB**，**T19 终版重建 88.46 MiB**（均 ≤ 200 MB 产品上限）。详见文末「③段执行记录」与「T21 记录」。
  **③段出口（2026-11-21，T19）**：六档基线、离线首编、缺包切在线、G-L1..L9 全绿（残余风险 R-L10）逐条达成；**安装版冷启动与窗口内可编辑 `[未测-会话环境限制]`**（不阻断文档收口，发布前需正常权限会话补做）。
  **2026-11-20 T21 收尾（G-L3 闭合）**：第三方声明 / 许可全文 / GPL 字体 Corresponding Source 随安装包分发（`licenses/`，解包 48.95 MiB）；G-L1..L9 **全绿**，残余风险 **R-L10（Fandol 源码 provenance）** 登记于 [tectonic-licensing.md](../dev/tectonic-licensing.md) §8.6。**About 页入口**由 T21b frontend 接 `licenses/`（资源路径已交付），在 UI 落地前"可发布"口径仍需 T21b 验收。
- **相关**：[0015](./0015-tectonic-only-embedded-engine.md)（唯一引擎 + 内嵌形态）、[0014](./0014-tectonic-first.md)（Tectonic-first）、[../dev/tectonic-licensing.md](../dev/tectonic-licensing.md)（本决策的许可依据）

## 背景

[ADR-0015](./0015-tectonic-only-embedded-engine.md) 把产品收成"Tectonic + 内嵌形态"一条路径。宏包集（bundle）的来源随之成为产品承诺的一部分：现状是 `tectonic.bundle = null` ⇒ **首次编译联网下载**（文档记约 60 MB；⚠ **该数字是"按需取包后的缓存量级"，不是完整 bundle 体积**，见文末「2026-11-16 实测修正」），离线/弱网/公司代理环境下首次编译会失败。

产品负责人的决策规则是：**先做许可审计；允许则随包，不允许才首次联网**。审计已出（[tectonic-licensing.md](../dev/tectonic-licensing.md)）：**R3 = 0，结论为"允许分发（R2：需声明与履约）"** ⇒ 走"随包"。（⚠ **该走向的前提"完整 bundle 体积可接受"已被实测推翻**，见文末「实测修正」。）

## 决策

1. **随安装包分发「代表性文档集」精简宏包集（候选 A）**，作为**默认 bundle**，首次编译**不需要联网**：
   - 形态 = **单个 `.zip`**（上游 `detect_bundle` 一等公民：本地 `.zip` → `ZipBundle`；也是 NSIS 里 1 个资源文件，不是 500+ 个）。**不用**完整 `default_bundle_*.tar`（2.88 GB，且不是 `-b` 的合法输入）。
   - 内容 = 517 文件 / 52.02 MiB（解包）/ 30.43 MiB（zip），digest `9e8c63ca…` 钉死；覆盖：六档基线文档集 + `beamer` + `biblatex`/`bibtex` + 中文默认（ctex + Fandol 四款 + 中文 beamer）+ lmodern 全光学字号（5–17pt）+ tikz/pgf + 常用字体/宏包。生成器 `scripts/gen-bench-bundle.mjs`（从上游 bundle 按需取文件）、打包器 `scripts/make-bundle.mjs`（确定性 zip + 自校验）。
   - **`.fmt` 不随包**（每份 24.45 MB 且是"首次编译预热产物"）：首启/首编按现有 format 缓存机制生成（T18 做预热）。
   - **不静默回退**（决策 2 不变）：内置集缺失/损坏 ⇒ 显式错误 + "修复安装 / 显式切在线"，不偷偷联网。
   - **缺包补全通道**（新增的本决策配套）：内置集不是完整 TeX Live。编译缺 `.sty`/`.cls`/文件时，诊断给可行动出路「设置 → 引擎 → 宏包集来源 → 在线完整宏包集」+ 错误面板一键切换重试；在线完整集是**用户显式选择**，不是自动回退。
   - **更新成本**：宏包集随应用版本走（换 bundle = 换安装包）；用户要"只更新宏包"走显式在线集。每次换集合必须重跑 G-L2/G-L6/G-L9（决策 4）。
2. **不静默回退**：随包资源缺失/损坏时给显式错误与"修复安装"引导，不偷偷改用网络 bundle。设置面保留"使用在线 bundle / 本地 bundle"的能力作为**用户显式选择**。
3. **许可义务是发布门禁的一部分**：`THIRD-PARTY-NOTICES`、LPPL 变更说明与原件获取信息、OFL/GFL/Knuth 全文、GPL 字体源码可得性（随附或镜像 + 书面要约）、Tectonic MIT 全文、C 依赖声明，全部按 [tectonic-licensing.md](../dev/tectonic-licensing.md) §5 执行。
4. **必须审计的是"实际随包的那份 bundle"**：用它的文件清单重跑审计脚本，而不是复用实验室夹具（424 文件）的结论；`G-L2/G-L6/G-L9` 以那份清单为准。
5. **体积账先测后发**：安装包增量、解包后磁盘占用、冷启动首次编译都在发布前实测并记录；未测数字不得写进宣传口径。

## 后果

**正面**（⚠ 决策 1 已于③段落定为候选 A，下列收益按"随包精简集 + 缺包补全"的口径成立）：① "零预装 + 离线可用"成为真的产品承诺，而不是"首次联网成功之后"；② 首次编译不再受网络/代理影响，也让"60 秒内可编辑"的验收不受下载时间污染；③ 宏包版本与引擎版本绑定，可复现（同一安装包 → 同一 bundle digest → 同一 format）。

**负面／风险**：

- **安装包变大**（⚠ **2026-11-16 实测修正：完整 bundle 压缩包 ≈2.88 GB**，原写"约 60 MB 量级"是子集缓存量级、**已作废**，见文末）；NSIS 打包与签名时间上升。
- **宏包更新节奏跟随应用版本**：用户拿不到"只更新 bundle"的路径 ⇒ 必须保留"显式切到在线 bundle"这个出口，并在文档里写清楚取舍。
- **许可义务长期化**：每次换 bundle/TL 版本都要重跑 G-L1..G-L9（G-L9），NOTICE 与源码镜像要跟着更新；GPL 字体的源码提供（Fandol/ZapfDingbats）不能只写一句"见上游"。
- **磁盘占用**：bundle + format 缓存叠加，需要在安装时检查空间并给可处置文案。
- **审计覆盖面**：完整 bundle 的包集合远大于夹具，`hyphen-*` 这类无 `catalogue-license` 的包必须逐文件扫描后才能算绿。

## 2026-11-16 实测修正（体积假设被推翻；决策 1 后于③段落定为候选 A，见状态行）

**实测**（验收阶段①，证据 `test_file/acc/REPORT-acceptance.md`，工作区未提交）：

| 项 | 值 | 出处 |
|---|---|---|
| 完整默认 bundle tar | `https://relay.fullyjustified.net/default_bundle_v33.tar`，`HEAD` → `Content-Length: 2881562112 B` ≈ **2.88 GB**（压缩态；解包后只会更大） | `REPORT-acceptance.md:61` |
| 本机 Tectonic 缓存 | 446 文件 / **63.2 MB**——只含此前编译**按需取到**的子集（419 个 bundle 文件 + 1 个 format），**不含** `FandolFang-*`/`FandolHei-*` | `REPORT-acceptance.md:36,51-53` |
| 实验室夹具 bundle | 424 文件 / 35,778,077 B（≈34 MB），只作测试素材 | [tectonic-licensing.md](../dev/tectonic-licensing.md) §1 结论 4 |

**结论**：

1. 原决策 1 依据的"约 60 MB"**不是完整 bundle 的体积**，而是按需取包后的缓存量级 ⇒ **体积假设错误**。
2. **2.88 GB 压缩包随 NSIS 安装包分发不可行**：安装包体积/签名与打包时间、安装时磁盘写入、首次启动体验、自动更新差分都会失控（"随包 = 离线开箱"的收益抵不过分发成本）。
3. ⇒ 决策 1 **撤回施工依据、待③段前重定**；状态行已注明。决策 2（不静默回退、保留显式选择）、3（许可义务门禁）、4（审计实际随包物）、5（体积账先测后发）**继续有效**——尤其第 5 条：本次正是"未测就写数"的反例。

**重定候选（③段前定稿，需产品负责人裁决）**：

| 候选 | 形态 | 主要代价 |
|---|---|---|
| **A 随包精简集** | 只带高频必用集合（`ctex`/`article`/`beamer`/`amsmath`/常用字体等），缺包时给"按包下载/换完整 bundle"的可操作路径 | 需要缺包检测与补包通道；"离线开箱"仍非全量（夹具 424 文件的教训：精简集漏 `beamer.cls` 就是默认体验塌陷） |
| **B 首次联网 + 可选本地 bundle** | 默认沿用联网（现状）；设置面提供"把完整 bundle 下载到本机磁盘后离线复用" | 离线承诺降级为"第二次起离线"；需要下载器（进度/断点/校验/代理） |
| **C 混合** | 安装包带最小必需集，首次运行后台补齐完整 bundle 到缓存 | 实现最复杂（两套 bundle + 切换判据 + 空间检查），但最接近"离线开箱 + 完整能力" |

**重定时必须一起回答的三问**：① 用户机器上"离线可用"的验收判据是什么（一份含 `beamer`+中文的夹具在断网下编过？）；② 安装包体积上限（产品负责人给数，不是工程自定）；③ 许可清单（G-L2/G-L6/G-L9）对着**最终随包物**重跑——精简集与完整集是两份不同的审计对象。

## 替代方案（已否决）

- **首次联网下载（现状）**：否决。它把"离线可用"押在第一次编译能连通上游上，且与"零预装"承诺不一致；保留为**显式出口**而非默认。
- **随包精简 bundle（424 文件实验室夹具）**：否决（⚠ **2026-11-16 重定后「精简集」重新成为候选之一，见上节候选 A；否决理由从"体积够小"改为"必须有缺包补全通道"**）。它缺 `beamer.cls` 等常用包（`docs/troubleshooting.md` 已记录），等于把"缺包"变成默认体验；夹具继续只作测试素材。
- **要求用户自备 bundle**：否决。与"开箱可用"直接冲突。

## 回退

把默认 bundle 指回网络地址即可（设置面与 `tectonic_bundles` 层都支持），代码无需回滚；但安装包体积与许可义务的变更不可自动回退，故本决策需要单独成文。

## ③段执行记录（2026-11-20，T16 落地；只记录，不改上面决策正文）

**落点**

- 默认语义：GUI 装配点 `src-tauri/src/runner.rs` + `latteset_tectonic::resolve_source` ⇒ **显式设置 > 环境变量 > 随包内置集 > 在线兜底**；release 构建"应有内置集却缺失"传 `Some(缺失路径)` ⇒ 打开时显式报错（`SHIPPED_BUNDLE_HINT`：修复安装 / 显式切在线；不静默联网）。headless 无资源目录 ⇒ 同序但无"内置集"一档（在线/显式）。
- 随包资源：`src-tauri/tauri.conf.json` `bundle.resources = { "resources/tectonic-bundle.zip": "tectonic-bundle.zip" }`；运行时 `<resource_dir>/tectonic-bundle.zip`（开发构建回退仓库产物/测试目录，便于 `tauri dev`）。
- 缺包补全：core 诊断（`MissingPackage/MissingClass/MissingFile/MissingFont`）的 hint 改为产品内出路（切在线完整集 / 补本地 bundle / 确认模板根目录，**不再建议 `tlmgr`**）；错误面板对缺文件类诊断加「切到在线完整宏包集并重试」按钮（`src/components/ErrorList.vue`）；设置面三档来源 + 当前来源展示（`src/components/SettingsPanel.vue`）。
- 缓存 GC：`latteset_tectonic::gc_cache`（formats / data / hashes 按"当前 digest + 最近 N"清理，`LATTESET_CACHE_KEEP` 可配，默认 3）；GUI 启动时空闲执行；单测覆盖保留当前/删旧/不误删未知条目/空缓存不动。**`.fmt` 不随包**。

**实测（证据 `test_file/phase3/t16-verify/`）**

| 验收项 | 结果 |
|---|---|
| 安装包体积（NSIS 冒烟；**T16 时，不含 licenses**） | **50,611,606 B = 48.27 MiB** ≤ 200 MB（含随包 zip 30.43 MiB / 解包 52.02 MiB） |
| 断网首编（内置集 + 空缓存 + 不可用代理） | **exit 0，4.2 s**，中文 Fandol 四款 + `validate-pdf` 全 PASS（0 U+FFFD） |
| 二次编译断网 | **exit 0，1.6 s**（format 已预热） |
| 缺包文档 | 诊断 `missing_package`（`enumitem.sty`）+ 可操作 hint；**不自动改来源** |
| 切在线后编过 | 显式切在线完整 bundle 后 **exit 0**（首编含联网取包 386 s；PDF 校验 PASS） |
| 来源优先级 / 内置集缺失文案 / GC | 单测绿（`cargo test -p latteset-tectonic`：55 passed） |
| 许可（G-L*） | **T21 后全绿**：G-L1..L9 见 `docs/dev/tectonic-licensing.md` §5；G-L3 = NOTICE/全文/源码/patches 随包（`licenses/`）+ CI `make-notices --check`；残余风险 R-L10 见 §8.6 |

**未做 / 交回**

- ~~`THIRD-PARTY-NOTICES` 生成、GPL 字体源码镜像/书面要约（G-L3/L5/L6 文本）~~ → **T21 已闭合**（见下节）。
- About 页入口（T21b frontend 接安装目录 `licenses/`）：资源路径已交付（`src-tauri/resources/licenses/` → 安装目录 `licenses/`），UI 未做。
- `.fmt` 首启预热（T18）；bundle 随包后的自动更新通道（决策 5 的"只更新宏包"出口仍是显式在线集）。

## T21 记录（2026-11-20，G-L3 收尾；只记录，不改决策正文）

**交付物**（全部入库、随安装包；生成器 `scripts/make-notices.mjs`）

- `src-tauri/resources/licenses/` = **308 文件 / 51,331,630 B（48.95 MiB）**：`THIRD-PARTY-NOTICES.md`（Tectonic MIT 全文、Rust 389 crate 分组、C 11 port 全文、bundle 126 包许可分布、patch 说明、GPL 字体源码映射、10 个 unmatched 归属）+ `LICENSES/`（261 份文本，含 Fandol GPLv3+字体例外、LPPL/OFL/GFL/Knuth/GPL/LGPL/GFDL/MPL 全文）+ `sources/`（Fandol 6 套 AFDKO 源码 47.77 MiB + zapfding AFM + SOURCE.md）+ `patches/`（4 个 TL patch 原文）+ `data/`（8 份可复核清单）。
- `tauri.conf.json` `bundle.resources` 增加 `"resources/licenses": "licenses"`；Tauri 打包前铺到 `target/release/licenses/`，安装目录 `licenses/`。
- CI 增加 G-L3 门禁：`node scripts/make-notices.mjs --check` + NOTICE/manifest 存在 + Fandol `cidfont.ps` ×6 + zapfding AFM + `licenses` 体积 ≤80 MiB。

**实测**

| 项 | 结果 |
|---|---|
| 最终 NSIS 安装包（**T19 终版重建**） | **92,755,399 B = 88.46 MiB ≤ 200 MB**（T21 时 92,723,655 B / 88.43 MiB；T16 时 48.27 MiB；增量 = licenses 约 40 MiB） |
| MSI | **114,954,456 B = 109.63 MiB**（T19 终版） |
| `make-notices --check` | 通过（离线确定性；NOTICE 16,170 B / manifest 308 文件逐 sha256） |
| G-L 状态 | **G-L1..L9 全绿**（G-L3/L4/L5/L6 由 T21 闭合；R-L10 残余风险登记） |
| About UI | 未做（T21b）——资源路径已交付 |

## T22 记录（2026-11-21，出口复核；只记录，不改决策正文）

- **B2（CI 前置）**：`build-windows` 改为**无条件先构建 release CLI**（`-Mode cli`）——它同时是代表集 bundle 生成与离线首编验收的共同前置；此前挂在"代表集缓存未命中"分支，代表集缓存命中而 rust-cache 无 release 产物时离线首编必红。
- **B3（缓存目录与 GC 口径）**：GC / runner / 首启预热**共用 `resolve_cache_dir`**（设置 > env > 宿主）；GC 的"当前 digest"由**同一 bundle 解析器**给出（在线只读缓存、零网络），**解析不出当前 digest 时整体不动**、未知条目不删。
- **fontconfig 归因（影响本 ADR 的离线/启动判据吗？不影响）**：T19 的"每进程 +200–240 ms"已定论 = **fontconfig 宿主字体扫描环境税**（旧缓存因字体目录 mtime 变化失效、每个 CLI 进程重扫 `C:/Windows/Fonts`；用最小 fontconfig 配置 + 可写缓存即清零，tiny 热路径 365→73 ms），**与随包/离线链路无关**；但 **T19 的六档绝对值含该税、跨版本绝对比较作废**（相对收益仍有效）。六档基线以 `test_file/phase3/BASELINE.md` §0.1 的 T22 清零版为准。
