# Tectonic 分发许可审计（P-G14 / G-L1..G-L9）

> **这是什么**：把「Tectonic 唯一引擎 + 内嵌形态 + 随包宏包集」这件事的**分发许可**审一遍，给出可执行的义务清单与门禁状态。面向要发包/改依赖的人。
> **版本与摘要（G-L9 的锚点）**：审计日期 2026-11-16；**③段重审计 2026-11-20（对象 = 最终随包物）**；引擎 `tectonic 0.17.0`（`Cargo.lock` 钉住，仓库 `test_file/tectonic-src` 为同版本 checkout）；C 依赖 = vcpkg `x64-windows-static-release`（port 版本见 §3）；**随包 bundle 样本 = `test_file/tectonic-bundle`（517 文件 / 54,550,792 B，产品化 `.zip` 30.43 MiB / digest `9e8c63ca5624438fba4bf4587afbfe3ddcfb82745769970ca2fe1f3ea392fde6`，见 §8）**，TeX Live 2024-0312 血统，用**本机 TL 2026 tlpdb** 做包归属映射。
> **性质**：本审计是**工程 + 事实核查**，不是法律意见。标 `R3` 的条目才是"需要法务/作者书面确认"的；本文没有 R3。
> **体积决策（2026-11-20，产品负责人）**：**候选 A「随包精简集 + 缺包补全通道」**；安装包上限 **≤200 MB**；`.fmt` 不随包。完整默认 bundle tar = `2,881,562,112 B` ≈ **2.88 GB**（`test_file/acc/REPORT-acceptance.md:61`）⇒ 不随包（原因见 §6）。安装包体积：T16 时 48.27 MiB（不含 licenses）→ T21 加许可资源 88.43 MiB → **T19 终版重建 88.46 MiB（NSIS）/ 109.63 MiB（MSI）**，见 §8.1。本审计的许可结论对**最终随包的任何 bundle**适用，发包前重跑（G-L9）。
> **与归档的关系**：原计划把产物写到 `docs/archive/research/tectonic-bundle-licensing.md`，但 `docs/archive/` 已冻结（只读）。本文件取代该路径成为 G-L1 的落点，`docs/archive/…` 里对 G-L1 的旧引用按"历史计划"读。

## 1. 结论

1. **Tectonic 本体**：MIT（`test_file/tectonic-src/LICENSE`，版权行 "the Tectonic Project"）⇒ 可嵌入、可静态链接、可闭源分发；义务 = 随发行物带 MIT 全文与版权行。
2. **Rust 依赖**：全图（`--all-features`，含 dev/build/非 Windows 目标）共 **654** 个包；**随二进制链接的 Windows 运行时闭包 = 389 个 crate**（`cargo metadata --filter-platform x86_64-pc-windows-msvc`，仅 normal 依赖；清单与逐 crate 许可文本见 `src-tauri/resources/licenses/data/rust-deps.json`）。许可证全是宽松型（MIT / Apache-2.0 / BSD / ISC / Zlib / Unicode-3.0 / MPL-2.0），**没有 GPL / AGPL**。MPL-2.0 有 5 个（`cssparser`、`cssparser-macros`、`dtoa-short`、`option-ext`、`selectors`）⇒ 义务 = 声明 + 保证 MPL 覆盖文件的源码可得（我们仓库开源，天然满足）。
3. **C 依赖（静态链接，11 个 port）**：**没有 LGPL-only / GPL-only 组件**。`graphite2` 是 **LGPL-2.1+ / MPL-2.0 / GPL-2.0+ 三选一**，本项目**选 MPL-2.0** ⇒ 不触发 LGPL 的"允许重新链接"义务；`freetype` 是 **FTL / GPL-2.0+ 双许可**，选 **FTL**；`gettext` / `libintl` **不在当前依赖闭包里**（`installed/vcpkg/info` 无该 port、`lib/` 无 `libintl`）⇒ GPL-3 的链接风险不成立。
4. **bundle（③段起对象 = 最终随包的精简集）**：**518 个条目（517 文件 + SHA256SUM）**可映射到 **508 个文件→包**（10 个按 basename 对不上 tlpdb，逐个人工核对，见 §8.3）；许可证以 **LPPL 系**为主（LPPL 映射合计 250），另有 **GPL 字体**（`fandol`、`zapfding`）与 **GPL/LGPL 宏包**（`cjk`(GPL-2)、`cslatex`、`dvipdfmx`、`hyphen-hungarian`、`hyphen-italian`(LGPL)、`hyphen-serbian`）。`(none)` 的 **137 个文件→包映射（去重 47 个包）已全部完成逐文件头扫描**（§8.2），结论 = 逐文件有 `% copyright` / 许可声明或属 Unicode/PD 数据 ⇒ 无"无主文件"。GPL 组件的 **Corresponding Source 已随安装包分发**（Fandol 6 款字体的 `cidfont.ps`/AFM/features 等 47.77 MiB；`zapfding` 的 AFM）⇒ 履约方式 = 随附源码（§8.4），属 `R2` 且已交付。
5. **判定：允许分发（R2：需声明与履约），未发现 R3 硬阻断。** ③段重审计对**最终随包物**（517 文件精简集）复跑清单 C：无新增 R3；`R2` 义务已在 T16/T21 全部落地——**第三方声明 `THIRD-PARTY-NOTICES.md` 随安装包分发（G-L3 绿）**、LPPL/OFL/GFL/Knuth/GPL/FDL 全文与 C/Rust 依赖文本入库、`latex.ltx` patch 的变更说明/归属/许可影响三项成文（G-L6 绿）、GPL 字体源码随包（§8.4）。
6. **两条必须保持的构建约束**：① `graphite2` 必须按 MPL-2.0 使用（不得改用 LGPL 条款做默认解释）；② **不得打开 `fontconfig` 的 `nls` 特性**（一旦引入 `gettext-libintl`，就是 LGPL-2.1+ 静态链接，需重新审计并补"允许重新链接"方案）。

R 档计数：R1 = 绝大多数（只需随附声明）；R2 = 5 个 MPL crate + Unicode/CDLA 数据包 + `freetype`/`graphite2` 两个需要"选许可"的 C port + bundle 全体；**R3 = 0**（截至本审计范围）。

## 2. 清单 A｜Rust 依赖

- **复核命令**：`cargo metadata --offline --all-features --format-version 1` → 提取 `license` / `license_file`。
- **结果**：654 个包；无 `license` 字段的 5 个是**本仓自己的 crate**（`latteset*`，根 `LICENSE` = MIT）；无其它空值。
- **非纯 R1 项**（必须进 NOTICE）：

| 许可证 | 包数 | 义务 |
|---|---|---|
| `MPL-2.0` | 5（见 §1.2） | 声明 + MPL 覆盖文件源码可得；不改这些文件则无额外义务 |
| `Unicode-3.0` | 18（`icu_*`、`zerovec` 等） | 随附 Unicode 许可与版权行 |
| `CDLA-Permissive-2.0` | 1（`webpki-root-certs`，证书数据） | 随附许可文本 |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | 2（`r-efi`） | 按 MIT 使用即可，不触发 LGPL |

## 3. 清单 B｜C 依赖（vcpkg `x64-windows-static-release`，全部静态链接）

| port | 版本 | 许可证（port 声明） | 义务档 | 动作 |
|---|---|---|---|---|
| freetype | 2.13.3 | `FTL OR GPL-2.0-or-later` | R2 | 选 **FTL**；NOTICE 保留 "Portions of this software are copyright © The FreeType Project" |
| harfbuzz | 11.3.3 | `MIT-Modern-Variant`（"Old MIT"） | R1 | 全文 + 版权行 |
| graphite2 | 1.3.14 | LGPL-2.1+ / MPL-2.0 / GPL-2.0+（`COPYING` 三选一） | R2 | **选 MPL-2.0**；声明 + 源码可得；**不得**按 LGPL 解释静态链接 |
| icu | 74.2 | `ICU`（= Unicode-3.0） | R1 | 全文 |
| fontconfig | 2.15.0 | `MIT` | R1 | 全文；**`nls` 特性保持关闭** |
| expat | 2.7.3 | `MIT` | R1 | 全文 |
| libpng | 1.6.50 | `libpng-2.0` | R1 | 全文 |
| zlib | 1.3.1 | `Zlib` | R1 | 全文 |
| brotli | 1.1.0 | `MIT` | R1 | 全文 |
| bzip2 | 1.0.8 | `bzip2-1.0.6` | R1 | 全文 + 版权行 |
| dirent | 1.26 | `MIT` | R1 | 全文 |

- **未链接（重要）**：`gettext` / `gettext-libintl` 不在 `installed/vcpkg/info`，`lib/` 里也没有 `libintl`；`share/gettext/its` 只是数据目录残留。⇒ 当前闭包**不携带 GPL-3/LGPL 的 gettext**。
- 证据：`test_file/vcpkg/installed/x64-windows-static-release/share/<port>/copyright` 与 `lib/*.lib`。

## 4. 清单 C｜bundle

**方法**：`test_file/tectonic-bundle`（**③段最终随包物的源目录**：517 个扁平文件 + SHA256SUM；产品 `.zip` 由 `scripts/make-bundle.mjs` 逐字节打包，不改文件内容）按 basename 映射到 `C:\texlive\2026\tlpkg\texlive.tlpdb` 的 `runfiles`；许可证取 `catalogue-license`；无该字段的包回落到**逐文件头声明**（③段已对全部 47 个 `(none)` 包做完整扫描）。脚本见 §6。**复核原始输出**：`test_file/phase3/audit-map.log`、`test_file/phase3/audit-heads.log`。

**许可证分布（③段，按"文件→包"计，合计 508）**：`(none)` 137、`lppl1.3c` 94、`lppl1.3c gpl2 fdl`（pgf）64、`lppl1.3c gpl2+ fdl`（beamer）43、`lppl1.3` 40（+`lppl` 18、`lppl gpl` 7、`lppl1.3c other-free` 3 ⇒ LPPL 系合计 **250**）、`gfl` 23（Latin Modern）、`ofl` 21（amsfonts + 其它）、`knuth` 21（CM 字体）、`gpl` 15、`other-free` 8、`pd` 7、`lgpl` 3、`mit` 2、`gpl2` 1（cjk）、`x11` 1。

**GPL / LGPL 组件（去重 8 个包，逐文件已定位）**：`cjk`[gpl2] `CJKfntef.sty`；`cslatex`[gpl] `fonttext.cfg`；`dvipdfmx`[gpl] `ckx.map`；`fandol`[gpl] 6 个 OTF；`hyphen-hungarian`[gpl] 2 个文件；`hyphen-italian`[lgpl] 3 个文件；`hyphen-serbian`[gpl] 4 个文件；`zapfding`[gpl] `pzdr.tfm`。

**义务要点**（针对③段最终随包物）：

| 类别 | 例子 | 义务 | 状态（T16/T21 后） |
|---|---|---|---|
| LPPL 未修改 | kernel/宏包 | 随附 LPPL 全文；6d 要求"完整未修改原件"或"足以取得它的信息" | **绿**：`THIRD-PARTY-NOTICES.md` §5 + `LICENSES/lppl-1.3c.txt`；随包文件逐字节来自上游 bundle（digest 锚定） |
| LPPL 已修改 | `latex.ltx`（Tectonic patch） | 6b 变更说明 + 6d 原件获取信息；patch 文本随包 | **绿**：NOTICE §8（变更说明/归属/许可影响三项）+ `patches/latex.ltx.diff` + 原件获取方式；`listings`/`fontawesome`/`fithesis` 不在随包集合（已登记） |
| OFL | `amsfonts` 等 | 随附 OFL 全文；未改字体名即可用 RFN 例外 | **绿**：`LICENSES/ofl-1.1.txt`（含 AMS Reserved Font Names）+ NOTICE §5.2；未改名 |
| GFL | `lm`（Latin Modern） | 随附 GUST Font License；改过须改名 | **绿**：`LICENSES/gfl-1.0.txt` + NOTICE §5.2；未改 |
| Knuth | CM 字体（`.pfb`/`.tfm`） | 随附声明 | **绿**：`LICENSES/knuth-tex-fonts.txt`（TL CM 源文件头声明原文）+ NOTICE §6 |
| GPL 字体（二进制） | `fandol`（GPLv3+字体例外）、`zapfding`（`.tfm`） | 随附 GPL 全文 + **Corresponding Source**（Fandol：`cidfont.ps`/AFM/`features`；ZapfDingbats：AFM） | **绿（随包附源码）**：`sources/fandol/`（6 套源码 47.77 MiB）+ `sources/zapfding/pzdr.afm` + `LICENSES/gpl-3.0-fandol.txt`；NOTICE §7/SOURCE.md 说明二进制↔源码映射与重建方式；残余风险见 §8.6 |
| GPL/LGPL 宏包 | `cjk`、`cslatex`、`dvipdfmx`、`hyphen-hu`、`hyphen-it`(LGPL)、`hyphen-sh` | 源码即文件本身 + 随附许可文本 | 文本待进 NOTICE |
| 无 catalogue-license | `hyphen-*`、`glyphlist`、`unicode-data` 等 47 个包 | 逐文件头声明 | **③段已完成**：47/47 扫描（`audit-heads.log`），全部有版权/许可头或 Unicode/PD 声明 |

**必须知道的三条硬事实**：

1. **bundle 里没有任何许可证文本**：`bundle.toml` 的 `ignore` 剔除了 `README`/`LICENSE.md`/`doc/.*`，所以"包里带了许可"不成立 —— NOTICE 必须我们自己产出。
2. **版本漂移**：映射用的是 TL 2026 的 tlpdb，而 bundle 是 TL 2024-0312 血统；同名包跨年可能换许可证或换维护者。③段已用**随包物自己的文件清单**重跑；换 bundle 必须重跑（G-L9）。
3. **bundle 与产品能力绑定**：③段已把随包物从"实验室夹具"升级为**覆盖代表集的产品精简集**（517 文件：含 `beamer.cls`/`biblatex.sty`/`lmroman5..17`/Fandol 四款/ctex/tikz/pgf/常用包；体积与覆盖见 `test_file/phase3/BUNDLE.md`）。它**不是完整 TeX Live** ⇒ 缺包由产品内"切在线完整 bundle / 补本地 bundle"通道兜底（ADR-0016 候选 A），并已用 `enumitem` 例验证"切在线后编过"。

## 5. 义务动作清单（发包前必须全绿）

| # | 门禁 | 动作 | 验收 | ③段（2026-11-20）状态 |
|---|---|---|---|---|
| G-L1 | 审计文档 | 本文存在且结论完整 | §1 无"待定"式结论 | **绿**（决策已重定为候选 A，§1/§8 均为结论） |
| G-L2 | 三张清单 | 清单 A/B 已完成；清单 C 对**最终随包物**重跑 + 补齐无字段包文件头扫描 | 空许可证计数 = 0（"逐文件声明"也算有结论） | **绿**：517 文件 → 508 文件→包映射；`(none)` 47 包全部逐文件头扫描（§8.2） |
| G-L3 | 第三方声明 | 生成 `THIRD-PARTY-NOTICES`，随安装包 + About 页可达 | 解包可见、UI 可打开 | **绿（T21 已闭合）**：`src-tauri/resources/licenses/THIRD-PARTY-NOTICES.md` + `LICENSES/` + `sources/` + `patches/` 随安装包（`bundle.resources` 映射到 `licenses/`）；`node scripts/make-notices.mjs --check` 进 CI；About 入口由 T21b frontend 接（资源相对路径 `licenses/`） |
| G-L4 | Tectonic MIT | NOTICE 含 MIT 全文 + "the Tectonic Project" | 文本比对 | **绿**：NOTICE §2 含 `test_file/tectonic-src/LICENSE` 全文（`LICENSES/mit-tectonic.txt` 同源） |
| G-L5 | OFL/GFL/Knuth | 逐字体族附许可全文；未改字体名 | 人工 1 项（记录判定人） | **绿**：OFL（amsfonts）/GFL（lm）/Knuth（CM）/GPL（Fandol+zapfding）全文与声明入 `LICENSES/` + NOTICE §5.2/§6；未改字体名（bundle digest 锚定）；判定人 = T21 执行 agent（2026-11-20） |
| G-L6 | 被 patch 的 TL 文件 | 变更说明 + 归属 + 许可影响三项 | 审计文档对应小节三项非空 | **绿**：随包集只含 `latex.ltx` 一个 patch 文件；NOTICE §8 三项成文 + `patches/latex.ltx.diff` + 原件获取方式；其余 3 个 patch 文件未随包（已登记） |
| G-L7 | C 依赖 | 无 R3；MPL/FTL 选择写进 NOTICE | 清单 C 无 LGPL-only | **绿**：NOTICE §4 写明 FTL/MPL 选择 + 11 port 的 copyright 全文（`LICENSES/c-deps/`）；无 LGPL-only |
| G-L8 | 结论入 ADR | 分发决策与许可结论进 ADR-0016 | ADR 已接受 + 重定结论 + 对最终随包物的许可结论都落盘 | **绿**：ADR-0016 已重定为候选 A 并附③段实测（本文 §8）；T21 后门禁状态更新为「G-L1..L9 全绿（残余风险 R-L10 登记见 §8.6）」 |
| G-L9 | 版本锚点 | 审计头部版本/摘要与实际发行物一致 | 可 diff | **绿**：头部锚点 = 产品 zip digest `9e8c…` / 517 文件 / 54,550,792 B；T21 的 `licenses/manifest.json` 记录 NOTICE/全文/源码逐文件 sha256，`make-notices --check` 可 diff |

## 6. 复核方式与未决

**已入仓的复核入口**：本文件 + §3 的 vcpkg 路径 + §2 的命令 + **`scripts/make-notices.mjs`（`--check` 离线复核 NOTICE/manifest/源码完整性；`--collect` 开发机重采集）**。

**工作区脚本（未提交，配 `test_file/` 素材使用）**：

- `test_file/audit_bundle_map.py`：bundle 文件 → TL 包 → 许可证直方图；产出 `test_file/audit-bundle-map.json`。
- `test_file/audit_bundle_heads.py`：对无 `catalogue-license` 的包抽样文件头。
- 两者都是 Python（用 DSH 自带解释器跑），**未进 `scripts/`**：要作为发布门禁，需先改写成仓库既有的 `scripts/*.mjs` 形态并接进 CI。

**未决 / 未测**（③段更新后）：

- ~~完整默认 bundle 的文件清单未取到~~ → **已解决**：③段用上游 `default_bundle_v33.tar` 按需取文件生成了最终随包集（517 文件），清单见 §8。
- ~~bundle 体积上限已测、精确体积未测~~ → **已测**：完整默认 tar `2,881,562,112 B`（2.88 GB，HEAD）；**随包精简集** = 目录 54,550,792 B / 产品 zip 31,908,772 B；**安装包 NSIS：T16 时 50,611,606 B（48.27 MiB，不含 licenses）→ T21 加许可资源 92,723,655 B（88.43 MiB）→ T19 终版重建 92,755,399 B（88.46 MiB）；MSI 114,954,456 B（109.63 MiB）**（`test_file/phase3/t16-verify/installer-size.json`、`t19-tauri-build.log`），均 ≤ 产品负责人给的 200 MB 上限。
- ~~**G-L3（NOTICE）+ G-L4/L5/L6 文本落地未做**~~ → **已闭合（T21，2026-11-20）**：声明/全文/源码/patches 全部随包，`make-notices --check` 进 CI（§8.4）。
- **Fandol 源码 provenance（残余风险 R-L10，不阻断但必须如实登记）**：上游 Fandol 团队**未发布官方源码包**（CTAN `fonts/fandol` 只有 OTF + COPYING）；本包采用的源码来自社区镜像 `Yixf-Self/fandol-fonts@b9330082…`（含 `cidfont.ps`/AFM/features 等 AFDKO 构建输入与 GPLv3+字体例外 COPYING）。**未做「源码 → OTF」重建验证**（需 Adobe AFDKO，本机未装）⇒ 若日后对源码真实性有争议，需向 Fandol 作者书面确认或改用可完全重建的字体集（见 §8.6）。
- 本审计**不覆盖**代码签名、Windows SmartScreen、第三方商标（TeX 用户组标识）等非许可事项。

## 7. 参考

- [LPPL 1.3c 全文](https://www.latex-project.org/lppl/lppl-1-3c.txt)（6b 变更说明、6d 原件或取得信息）
- [CTAN: fandol](https://ctan.org/pkg/fandol)（GPL；CTAN 只发 OTF + COPYING，**无源码**）
- [Yixf-Self/fandol-fonts](https://github.com/Yixf-Self/fandol-fonts)（源码形态：`cidfont.ps`、AFM、`features`）
- [CTAN: psnfss-source](https://ctan.org/pkg/psnfss-source)（`zapfding` 的 AFM 与构建脚本）
- [HarfBuzz COPYING](https://github.com/harfbuzz/harfbuzz/blob/main/COPYING)（"Old MIT"）

## 8. ③段重审计：最终随包物（2026-11-20，T16 / ADR-0016 候选 A）

### 8.1 随包物锁定（G-L9 锚点）

| 项 | 值 | 证据 |
|---|---|---|
| 文件数 / 解包字节 | **517 文件** / **54,550,792 B**（另有 `SHA256SUM` 65 B） | `test_file/phase3/BUNDLE.md`、`test_file/phase3/product-bundle.json` |
| bundle digest | `9e8c63ca5624438fba4bf4587afbfe3ddcfb82745769970ca2fe1f3ea392fde6` | 同上（digest = 对 `sha256␠␠文件名` 行排序拼接再 sha256） |
| 产品 `.zip` | **31,908,772 B（30.43 MiB）**，sha256 `962bb281b201b4780d67d43662adff32a30037815044b8287e6f7823ad5da119` | `scripts/make-bundle.mjs`（确定性、固定时间戳） |
| NSIS 安装包（T16 时，不含 licenses） | **50,611,606 B（48.27 MiB）** ≤ 200 MB 上限 | `test_file/phase3/t16-verify/installer-size.json` |
| 第三方声明与源码（T21） | `src-tauri/resources/licenses/` = **308 文件 / 51,331,630 B（48.95 MiB）**（NOTICE 16,170 B；`LICENSES/` 261 份文本；`sources/` 34 文件 47.77 MiB；`patches/` 4 个 diff；`data/` 8 份复核清单） | `src-tauri/resources/licenses/manifest.json` |
| **NSIS 安装包（T19 终版重建）** | **92,755,399 B（88.46 MiB）** ≤ 200 MB 上限（T21 时 92,723,655 B / 88.43 MiB；MSI 114,954,456 B / 109.63 MiB） | 本机 2026-11-21 实测（`tauri build` 输出；Tauri 先把 resources 铺到 `target/release/licenses/` 再打包） |
| 上游来源 | `default_bundle_v33.tar`（format serial 33），上游 digest `6ffe0558…` | `scripts/gen-bench-bundle.mjs` 头部注释 |
| `.fmt` | **不随包**（首启预热，属 T18） | `src-tauri/tauri.conf.json` resources 只含 zip |

### 8.2 清单 C 重跑结果

- **映射**：518 条目 → **508 个文件→包**（10 个 unmatched 见 §8.3）；脚本 `test_file/audit_bundle_map.py`，原始输出 `test_file/phase3/audit-map.log`。
- **无字段包**：`(none)` = 137 个文件→包（去重 **47 个包**，主要是 `hyphen-*` + `glyphlist` + `tlperl`）→ **47/47 逐文件头扫描完成**（`test_file/audit_bundle_heads.py`，输出 `audit-heads.log`）：这些文件均带 `% copyright` + `% notice`（hyph-utf8 系）或 Unicode/PD 声明（`glyphlist`、`SpecialCasing.txt`）⇒ **空许可证计数 = 0（以"逐文件声明"结案）**。
- **GPL/LGPL（8 个包）**：见 §4 表格；全部为可读源码/数据或字体的二进制形态，源码可得性已核实（§1.4）。
- **R 档**：**无新增 R3**。GPL 仅出现在可再分发的字体/宏包/数据上（R2：全文 + 源码可得）；LPPL/OFL/GFL/Knuth 为 R1/R2（声明类）。

### 8.3 10 个 unmatched 文件（按 basename 对不上 tlpdb）人工判定

| 文件 | 归属 / 判定 |
|---|---|
| `SHA256SUM` | 本产品自产的 bundle 元数据（digest 文本），非上游文件 |
| `dehyphn-x-2022-03-16.pat/.tex`、`dehypht-x-2022-03-16.pat/.tex` | `dehyph-exptl`（德语实验断词，LPPL-1.3c）；tlpdb 里对应 runfiles 带 `x-` 变体名，basename 直配失败 |
| `hyph-quote-rm.tex` | `hyph-utf8` 子集（逐文件 `% copyright/notice`，同 §8.2 结论） |
| `kanjix.map`、`pdftex.map` | TL 的**生成型 map 文件**（由 updmap 汇总；无独立许可头）。归属 = 各字体/宏包声明 + TL 聚合，随包声明里说明是生成物 |
| `pgfkeysfiltered.code.tex` | `pgf`（LPPL-1.3c + GPL-2+ + FDL，三选一：按 LPPL 使用） |
| `tectonic-format-latex.tex` | Tectonic 自己的 format 驱动文件（MIT，"the Tectonic Project"） |

**结论**：10 个 unmatched 全部有归属，**无孤儿文件、无 R3**；其中 5 个需要"生成物/变体名"在 NOTICE 里逐条说明（已入 §8.4 清单）。

### 8.4 G-L3 交付物（**T21 已闭合，2026-11-20**）

**入库位置**：`src-tauri/resources/licenses/`（`bundle.resources` 映射到安装目录 `licenses/`；Tauri 打包前会把整棵树铺到 `src-tauri/target/release/licenses/`，可从安装包解包验证）。**生成器**：`scripts/make-notices.mjs`（默认/`--check` 离线确定性；`--collect` 开发机重采集）。

| 交付物 | 内容 | 复核方式 |
|---|---|---|
| `THIRD-PARTY-NOTICES.md`（16,170 B） | ① Tectonic MIT 全文+版权行；② Rust **389** 个随包 crate 的许可分组表（MPL×5 说明、Unicode-3.0/CDLA、其余宽松）；③ C 依赖 **11 port** 的 copyright 全文 + FTL/MPL 选择说明；④ bundle **126 包**逐包许可分布 + 全文索引；⑤ 字体族（Fandol/LM/AMS/CM/zapfding）全文索引；⑥ `latex.ltx` patch 的变更说明/归属/许可影响；⑦ GPL 字体 Corresponding Source 映射；⑧ 10 个 unmatched 文件归属 | `make-notices --check` + 人工通读 |
| `LICENSES/`（261 份文本，1.25 MiB） | 规范全文：MIT-Tectonic、LPPL-1.3c、GPL-2.0、GPL-3.0、**GPL-3.0+Fandol 字体例外**、LGPL-2.1、GFL-1.0、OFL-1.1（AMS RFN）、GFDL-1.3、MPL-2.0、Knuth 声明；Rust 逐 crate 许可文本（按内容去重）；11 个 C port 的 copyright | `manifest.json` 逐文件 sha256 |
| `sources/fandol/`（6 套 / 47.6 MiB） | 6 个随包 OTF 的 **Corresponding Source**：每字目录 `cidfont.ps`/`AFM`/`features`/`fontinfo`/`cidfontinfo`/`FontMenuNameDB`（AFDKO `makeotf` 构建输入）+ `COPYING`（GPLv3+例外）+ `SOURCE.md`（upstream `Yixf-Self/fandol-fonts@b9330082…`、引脚、重建方式） | `--check` 断言 6 个 `cidfont.ps` 存在；`data/sources.json` 逐文件字节 |
| `sources/zapfding/` | `pzdr.tfm` 的 AFM 源数据 + `SOURCE.md`（`afm2tfm` 生成命令、dvips 源码指针） | 同上 |
| `patches/` | Tectonic 对 TL 2024-0312 的 4 个 patch 原文（`latex.ltx` 随包；其余 3 个未随包，如实登记） | NOTICE §8 + `data/patched-tl-files.json` |
| `data/`（8 份清单） | `meta.json`（G-L9 锚点）、`bundle-packages.json`（126 包）、`unmatched-files.json`、`rust-deps.json`（389）、`c-deps.json`（11）、`patched-tl-files.json`、`sources.json`、`license-texts.json` | `manifest.json` 收录 |

**安装包体积影响**：NSIS T16 48.27 MiB → T21 **88.43 MiB** → **T19 终版 88.46 MiB**（+40.16 MiB，licenses 贡献），仍 ≤200 MB 上限；`licenses/` 解包后 48.95 MiB。
**About 入口**：由 T21b（frontend）做；资源相对路径 = 安装目录 `licenses/`（Rust 侧可用 `app.path().resource_dir().join("licenses")`，无需新命令）。

**当前状态：绿（G-L3 闭合）** —— 声明/全文/源码/patches 全部随包，CI 断言 `make-notices --check` + NOTICE/manifest/sources 完整性 + 体积门禁。**残余风险 R-L10 见 §8.6**（不阻断，但发布说明需照实写）。

### 8.5 复核命令（可重跑）

```powershell
# T21 门禁（离线、确定性；CI 同一命令）
node scripts/make-notices.mjs --check

# T16 bundle 门禁
node scripts/make-bundle.mjs --verify

# 许可映射/文件头审计（需要本机 TeX Live 2026，开发机复核用）
python test_file/audit_bundle_map.py | Tee-Object test_file/phase3/audit-map.log
python test_file/audit_bundle_heads.py | Tee-Object test_file/phase3/audit-heads.log

# 换 bundle/依赖后重采集（开发机：TL + cargo registry + vcpkg installed + tectonic-src + 已下载字体源码）
node scripts/make-notices.mjs --collect
git status --short src-tauri/resources/licenses
```

### 8.6 残余风险（T21 收尾时登记，不阻断发包）

1. **R-L10 Fandol 源码 provenance**：上游团队未发布官方源码包（CTAN 仅 OTF + COPYING）。本包源码取自社区镜像 `Yixf-Self/fandol-fonts@b9330082…`（含 `cidfont.ps`/AFM/features 与 GPLv3+例外 COPYING），与随包 OTF 的对应关系按字族目录名与上游 v0.3 一致；**未做「源码 → OTF」重建验证**（需 Adobe AFDKO `makeotf`，本机未装）。缓解：`sources/fandol/SOURCE.md` 写清来源/引脚/重建方式；若需更强证据，后续可装 AFDKO 重建并比对 AFM 度量/字形数。
2. **Knuth 许可正文形态**：Knuth 许可本身是短声明（"未改名不得修改" + 允许未修改分发），`LICENSES/knuth-tex-fonts.txt` 为 TL 的 CM 源文件头原文 + 说明；随包 CM 字体未修改（bundle digest 锚定）。
3. **`licenses/` 体积**：解包 48.95 MiB（源码 47.77 MiB 为主体），安装包 **88.46 MiB（T19 终版）**；若未来需要瘦身，优先方案 = 源码打 `.tar.xz`（GPL 允许源码以归档形式提供），但需重新定义"安装包内可见"验收（当前保留目录形态）。
4. **About UI**：G-L3 验收的"UI 可打开"依赖 T21b frontend 接 `licenses/`；本任务交付资源与路径，未改 UI。发布前需 T21b 落地并真机点开一次。
