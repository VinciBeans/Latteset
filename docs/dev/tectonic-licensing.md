# Tectonic 分发许可审计（P-G14 / G-L1..G-L9）

> **这是什么**：把「Tectonic 唯一引擎 + 内嵌形态 + 随包宏包集」这件事的**分发许可**审一遍，给出可执行的义务清单与门禁状态。面向要发包/改依赖的人。
> **版本与摘要（G-L9 的锚点）**：审计日期 2026-11-16；引擎 `tectonic 0.17.0`（`Cargo.lock` 钉住，仓库 `test_file/tectonic-src` 为同版本 checkout）；C 依赖 = vcpkg `x64-windows-static-release`（port 版本见 §3）；bundle 样本 = `test_file/tectonic-bundle`（424 文件 / 35,778,077 B，TeX Live 2024-0312 血统，用**本机 TL 2026 tlpdb** 做包归属映射）。
> **性质**：本审计是**工程 + 事实核查**，不是法律意见。标 `R3` 的条目才是"需要法务/作者书面确认"的；本文没有 R3。
> **体积上限（2026-11-16 实测，见 §6）**：完整默认 bundle tar = `2,881,562,112 B` ≈ **2.88 GB**（`test_file/acc/REPORT-acceptance.md:61`）——与 ADR-0016「随包**完整**宏包集」直接冲突，该决策已标「待③段前重定」。本审计的许可结论对"**最终随包的任何 bundle**"仍然适用，但审计对象待方案定稿后锁定。
> **与归档的关系**：原计划把产物写到 `docs/archive/research/tectonic-bundle-licensing.md`，但 `docs/archive/` 已冻结（只读）。本文件取代该路径成为 G-L1 的落点，`docs/archive/…` 里对 G-L1 的旧引用按"历史计划"读。

## 1. 结论

1. **Tectonic 本体**：MIT（`test_file/tectonic-src/LICENSE`，版权行 "the Tectonic Project"）⇒ 可嵌入、可静态链接、可闭源分发；义务 = 随发行物带 MIT 全文与版权行。
2. **Rust 依赖**：`cargo metadata --all-features` 共 **654** 个包；许可证全是宽松型（MIT / Apache-2.0 / BSD / ISC / Zlib / Unicode-3.0 / MPL-2.0），**没有 GPL / AGPL**。MPL-2.0 有 5 个（`cssparser`、`cssparser-macros`、`dtoa-short`、`option-ext`、`selectors`）⇒ 义务 = 声明 + 保证 MPL 覆盖文件的源码可得（我们仓库开源，天然满足）。
3. **C 依赖（静态链接，11 个 port）**：**没有 LGPL-only / GPL-only 组件**。`graphite2` 是 **LGPL-2.1+ / MPL-2.0 / GPL-2.0+ 三选一**，本项目**选 MPL-2.0** ⇒ 不触发 LGPL 的"允许重新链接"义务；`freetype` 是 **FTL / GPL-2.0+ 双许可**，选 **FTL**；`gettext` / `libintl` **不在当前依赖闭包里**（`installed/vcpkg/info` 无该 port、`lib/` 无 `libintl`）⇒ GPL-3 的链接风险不成立。
4. **bundle**：424 个文件可映射到 414 个文件 / 若干 TL 包；许可证以 **LPPL 系**为主，另有 **GPL 字体**（`fandol`、`zapfding`）与 **GPL/LGPL 宏包**（`cjk`、`cslatex`、`dvipdfmx`、`hyphen-hungarian`、`hyphen-italian`、`hyphen-serbian`）。GPL 组件的**源码可得**已核实（Fandol 的 `cidfont.ps`/AFM/features 在公开仓库；`zapfding` 的 AFM 在 `psnfss-source`）⇒ 可通过"随附源码 / 镜像源码 + 书面要约"履约，属 `R2`。
5. **判定：允许分发（R2：需声明与履约），未发现 R3 硬阻断。** 因此"审计允许则随包集成"的方向成立 —— **但发包前必须闭合 §5 的义务，并对"实际随包的 bundle"重跑本审计**（当前只审了实验室夹具；完整默认 bundle 的文件清单未取到，见 §6）。
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

**方法**：`test_file/tectonic-bundle`（424 个扁平文件）按 basename 映射到 `C:\texlive\2026\tlpkg\texlive.tlpdb` 的 `runfiles`；许可证取 `catalogue-license`；无该字段的包（137 处命中，主要是 `hyphen-*` / `glyphlist`）回落到**逐文件头声明**抽样。脚本见 §6。

**许可证分布（按包）**：`(none)` 137、`lppl1.3c` 81、`lppl1.3c gpl2 fdl` 63（pgf）、`lppl1.3` 24、`knuth` 21（CM 字体）、`ofl` 20（amsfonts）、`lppl` 18、`gfl` 13（Latin Modern）、`gpl` 12、`other-free` 8、`pd` 7、`lgpl` 3、`mit` 2、`gpl2` 1（cjk）、`x11` 1。

**义务要点**：

| 类别 | 例子 | 义务 | 状态 |
|---|---|---|---|
| LPPL 未修改 | `latex.ltx` 之外的 kernel/宏包 | 随附 LPPL 全文；LPPL 6d 要求"完整未修改原件"或"足以取得它的信息" | 待做（NOTICE + TL 2024-0312 获取地址） |
| LPPL 已修改 | `latex.ltx`（Tectonic patch：把"文件不存在"的交互提示改成致命错误） | 6b 变更说明 + 6d 原件获取信息；patch 文本随包 | 待做（变更日志 + patch） |
| OFL | `amsfonts` | 随附 OFL 全文；未改字体名即可用 RFN 例外；改过则不得用保留名 | 待做 |
| GFL | `lm`（Latin Modern） | 随附 GUST Font License；改过须改名 | 待做 |
| Knuth | CM 字体（`.pfb`/`.tfm`） | 随附声明 | 待做 |
| GPL 字体（二进制） | `fandol`（版权文件是 **GPLv3**）、`zapfding`（`.tfm`） | 随附 GPL 全文 + **Corresponding Source**（Fandol：`cidfont.ps`/AFM/`features`，公开仓库；ZapfDingbats：`psnfss-source` 的 AFM） | 待做（镜像源码或书面要约） |
| GPL/LGPL 宏包 | `cjk`、`cslatex`、`dvipdfmx`、`hyphen-hu`、`hyphen-it`(LGPL)、`hyphen-sh` | 这些是可读源码的脚本/数据，源码即文件本身 ⇒ 随附许可文本 + 源码可得声明 | 待做 |
| 无 catalogue-license | `hyphen-*`、`glyphlist`、`unicode-data` 等 | 逐文件头声明（抽样：`hyph-utf8` 各语言 `% copyright` + `% notice`；`glyphlist` = Apache-2.0；`unicode-data` = Unicode 许可） | **未完成**：需脚本化扫描全部 424 个文件头 |

**必须知道的三条硬事实**：

1. **bundle 里没有任何许可证文本**：`bundle.toml` 的 `ignore` 剔除了 `README`/`LICENSE.md`/`doc/.*`，所以"包里带了许可"不成立 —— NOTICE 必须我们自己产出。
2. **版本漂移**：映射用的是 TL 2026 的 tlpdb，而 bundle 是 TL 2024-0312 血统；同名包跨年可能换许可证或换维护者。**实际随包的 bundle 必须用它自己的文件清单重跑**。
3. **bundle 与产品能力绑定，且体积假设已被推翻**：这个 424 文件的夹具**不含 `beamer.cls`**（`docs/troubleshooting.md` 已记录该现象）⇒ 它只能当实验室夹具，不能当"随包资源"的产品形态；原计划随包的是**完整默认 bundle**（或等价的完整集合）。⚠ **2026-11-16 实测**：完整默认 bundle tar ≈ **2.88 GB**（§6），"完整随包"在安装包上不可行 ⇒ 随包物**待 ADR-0016 重定**（精简集 / 联网 / 混合）；清单 C 的目标随之从"完整默认 bundle"改为"**最终随包的那份**"。

## 5. 义务动作清单（发包前必须全绿）

| # | 门禁 | 动作 | 验收 |
|---|---|---|---|
| G-L1 | 审计文档 | 本文存在且结论完整 | §1 无"待定"式结论 |
| G-L2 | 三张清单 | 清单 A/B 已完成；清单 C **需对实际 bundle 重跑**并补齐 137 个无字段包的文件头扫描 | 空许可证计数 = 0（"逐文件声明"也算有结论） |
| G-L3 | 第三方声明 | 生成 `THIRD-PARTY-NOTICES`，随安装包 + About 页可达 | 解包可见、UI 可打开 |
| G-L4 | Tectonic MIT | NOTICE 含 MIT 全文 + "the Tectonic Project" | 文本比对 |
| G-L5 | OFL/GFL/Knuth | 逐字体族附许可全文；未改字体名 | 人工 1 项（记录判定人） |
| G-L6 | 被 patch 的 TL 文件 | 变更说明 + 归属 + 许可影响三项（`latex.ltx`；完整 bundle 还有 `listings.sty`/`fontawesome.sty`/`fithesis-mu-base.sty`） | 审计文档对应小节三项非空 |
| G-L7 | C 依赖 | 无 R3；MPL/FTL 选择写进 NOTICE | 清单 C 无 LGPL-only |
| G-L8 | 结论入 ADR | 分发决策与许可结论进 [ADR-0016](../adr/0016-ship-bundle-offline.md) | ADR 状态"已接受"；⚠ 决策 1（随包完整 bundle）已因 2.88 GB **待③段前重定** ⇒ 本门禁改为「**重定结论 + 对最终随包物的许可结论**都落盘」才算绿 |
| G-L9 | 版本锚点 | 审计头部版本/摘要与实际发行物一致；换 bundle/TL/引擎版本必须重跑 | 可 diff |

## 6. 复核方式与未决

**已入仓的复核入口**：本文件 + §3 的 vcpkg 路径 + §2 的命令。

**工作区脚本（未提交，配 `test_file/` 素材使用）**：

- `test_file/audit_bundle_map.py`：bundle 文件 → TL 包 → 许可证直方图；产出 `test_file/audit-bundle-map.json`。
- `test_file/audit_bundle_heads.py`：对无 `catalogue-license` 的包抽样文件头。
- 两者都是 Python（用 DSH 自带解释器跑），**未进 `scripts/`**：要作为发布门禁，需先改写成仓库既有的 `scripts/*.mjs` 形态并接进 CI。

**未决 / 未测**：

- **完整默认 bundle 的文件清单未取到**（本机到 `relay.fullyjustified.net` 的 TLS 被沙箱拦截）⇒ 清单 C 对"实际随包物"仍是**未完成**，不是"通过"。
- **bundle 体积：上限已测、精确体积仍未测**。2026-11-16 验收对完整默认 bundle tar 做 `HEAD`：`Content-Length: 2,881,562,112 B ≈ 2.88 GB`（`test_file/acc/REPORT-acceptance.md:61`）；本机缓存 446 文件 / 63.2 MB 只是**按需取到的子集**（不含 `FandolFang`/`FandolHei`，`REPORT-acceptance.md:51-53`）。原记"约 60 MB"是子集缓存量级、**不得再当完整 bundle 体积**。⇒ **整包随包与 2.88 GB 上限直接冲突**，ADR-0016 决策待③段前重定；解包后磁盘占用、不同候选方案（精简集/联网/混合）的体积账仍 `[未测]`，按 ADR-0016 决策 5「先测后发」补。
- Fandol 源码仓库与 OTF 的版本对应关系需人工确认（GFonts 仓库 vs CTAN 0.3）。
- `hyphen-*` 的逐文件许可需全量扫描（当前只抽样）。
- 本审计**不覆盖**代码签名、Windows SmartScreen、第三方商标（TeX 用户组标识）等非许可事项。

## 7. 参考

- [LPPL 1.3c 全文](https://www.latex-project.org/lppl/lppl-1-3c.txt)（6b 变更说明、6d 原件或取得信息）
- [CTAN: fandol](https://ctan.org/pkg/fandol)（GPL；CTAN 只发 OTF + COPYING，**无源码**）
- [Yixf-Self/fandol-fonts](https://github.com/Yixf-Self/fandol-fonts)（源码形态：`cidfont.ps`、AFM、`features`）
- [CTAN: psnfss-source](https://ctan.org/pkg/psnfss-source)（`zapfding` 的 AFM 与构建脚本）
- [HarfBuzz COPYING](https://github.com/harfbuzz/harfbuzz/blob/main/COPYING)（"Old MIT"）
