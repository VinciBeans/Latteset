# Latteset 第三方声明（THIRD-PARTY-NOTICES）

本安装包包含第三方软件与字体。本文件列出其版权与许可信息；完整许可文本见同目录 `LICENSES/`，
GPL/LGPL 组件的对应源码见 `sources/`，被修改过的 TeX Live 文件补丁见 `patches/`。

- 生成时间：2026-11-20T15:29:34.699Z
- 审计日期：2026-11-20（G-L1..G-L9，见仓库 `docs/dev/tectonic-licensing.md`）
- 引擎：tectonic 0.17.0（format serial 33，MIT）
- 内置精简宏包集（bundle）：digest `9e8c63ca5624438fba4bf4587afbfe3ddcfb82745769970ca2fe1f3ea392fde6`，517 文件 / 54550792 B（zip 31908772 B，sha256 `962bb281b201b4780d67d43662adff32a30037815044b8287e6f7823ad5da119`）
- bundle 来源：https://relay.fullyjustified.net/default_bundle_v33.tar（TeX Live 血统：tectonic-src/bundles/bundles/texlive2024-0312）
- C 依赖：vcpkg x64-windows-static-release（pin `a62ce77d56ee07513b4b67de1ec2daeaebfae51a`）

> 若本文件与随包文件不一致，以 `manifest.json` 记录的 sha256 为准（`node scripts/make-notices.mjs --check` 可复核）。

## Tectonic（MIT）

本产品的 TeX 引擎 Tectonic 以 MIT 许可分发：

```
(Tectonic is licensed under the MIT License. Elements of the system from which
it is derived are licensed under an extremely wide variety of open-source
licenses.)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the “Software”), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Rust 依赖（清单 A）

随二进制链接的 crates.io 运行时依赖共 **389** 个（不含本仓库 workspace 成员）。逐 crate 的许可标识与许可文本路径：

| 许可标识 | crate 数 | crate（name version） |
|---|---|---|
| `MIT OR Apache-2.0` | 179 | `anyhow 1.0.104`、`arrayvec 0.7.8`、`async-trait 0.1.92`、`base64 0.22.1`、`bitflags 2.13.1`、`block-buffer 0.10.4`、`block-buffer 0.12.1`、`bumpalo 3.20.3`、`camino 1.2.5`、`cargo-platform 0.1.9`、`cfg-if 1.0.4`、`chacha20 0.10.2`、… |
| `MIT` | 95 | `bytes 1.12.1`、`cargo_metadata 0.19.2`、`cfb 0.7.3`、`darling 0.23.0`、`darling_core 0.23.0`、`darling_macro 0.23.0`、`data-encoding 2.11.1`、`derive_more 2.1.1`、`derive_more-impl 2.1.1`、`dom_query 0.27.0`、`fern 0.7.1`、`generic-array 0.14.7`、… |
| `Apache-2.0 OR MIT` | 33 | `atomic-waker 1.1.2`、`bit-set 0.8.0`、`bit-vec 0.8.0`、`const-oid 0.10.2`、`ctor 0.8.0`、`ctor-proc-macro 0.0.7`、`dtor 0.3.0`、`dtor-proc-macro 0.0.6`、`equivalent 1.0.2`、`fastrand 2.5.0`、`idna_adapter 1.2.2`、`indexmap 1.9.3`、… |
| `Unicode-3.0` | 18 | `icu_collections 2.2.0`、`icu_locale_core 2.2.0`、`icu_normalizer 2.2.0`、`icu_normalizer_data 2.2.0`、`icu_properties 2.2.0`、`icu_properties_data 2.2.0`、`icu_provider 2.2.0`、`litemap 0.8.2`、`potential_utf 0.1.5`、`tinystr 0.8.3`、`writeable 0.6.3`、`yoke 0.8.3`、… |
| `MIT/Apache-2.0` | 11 | `bitflags 1.3.2`、`bs58 0.5.1`、`core_detect 1.0.0`、`ident_case 1.0.1`、`json-patch 3.0.1`、`siphasher 1.0.3`、`unic-char-property 0.9.0`、`unic-char-range 0.9.0`、`unic-common 0.9.0`、`unic-ucd-ident 0.9.0`、`unic-ucd-version 0.9.0` |
| `Unlicense OR MIT` | 9 | `aho-corasick 1.1.5`、`byteorder 1.5.0`、`byteorder-lite 0.1.0`、`jiff 0.2.35`、`jiff-core 0.1.0`、`jiff-tzdb 0.1.8`、`jiff-tzdb-platform 0.1.3`、`memchr 2.8.3`、`winapi-util 0.1.11` |
| `MPL-2.0` | 5 | `cssparser 0.36.0`、`cssparser-macros 0.6.1`、`dtoa-short 0.3.5`、`option-ext 0.2.0`、`selectors 0.36.1` |
| `MIT OR Apache-2.0 OR Zlib` | 5 | `lru-slab 0.1.3`、`raw-window-handle 0.6.2`、`tinyvec_macros 0.1.1`、`zune-core 0.5.3`、`zune-jpeg 0.5.15` |
| `BSD-3-Clause` | 3 | `alloc-no-stdlib 2.0.4`、`alloc-stdlib 0.2.4`、`subtle 2.6.1` |
| `Zlib OR Apache-2.0 OR MIT` | 3 | `bytemuck 1.25.2`、`bytemuck_derive 1.12.1`、`tinyvec 1.12.0` |
| `Apache-2.0` | 3 | `sync_wrapper 1.0.2`、`tao 0.35.3`、`zopfli 0.8.3` |
| `Zlib` | 2 | `foldhash 0.2.0`、`zlib-rs 0.6.7` |
| `Apache-2.0 OR ISC OR MIT` | 2 | `hyper-rustls 0.27.9`、`rustls 0.23.45` |
| `BSD-3-Clause OR Apache-2.0` | 2 | `moxcms 0.8.1`、`pxfm 0.1.30` |
| `ISC` | 2 | `rustls-webpki 0.103.15`、`untrusted 0.9.0` |
| `Unlicense/MIT` | 2 | `same-file 1.0.6`、`walkdir 2.5.0` |
| `0BSD OR MIT OR Apache-2.0` | 1 | `adler2 2.0.1` |
| `ISC AND (Apache-2.0 OR ISC)` | 1 | `aws-lc-rs 1.18.1` |
| `ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0)` | 1 | `aws-lc-sys 0.45.0` |
| `BSD-3-Clause AND MIT` | 1 | `brotli 8.0.4` |
| `BSD-3-Clause/MIT` | 1 | `brotli-decompressor 5.0.3` |
| `Apache-2.0 AND MIT` | 1 | `dpi 0.1.2` |
| `CC0-1.0 OR MIT-0 OR Apache-2.0` | 1 | `dunce 1.0.5` |
| `(Apache-2.0 OR MIT) AND BSD-3-Clause` | 1 | `encoding_rs 0.8.41` |
| `Apache-2.0 / MIT` | 1 | `fnv 1.0.7` |
| `BSD-2-Clause` | 1 | `Inflector 0.11.4` |
| `MIT OR Zlib OR Apache-2.0` | 1 | `miniz_oxide 0.8.9` |
| `CC0-1.0` | 1 | `notify 8.2.0` |
| `Apache-2.0 AND ISC` | 1 | `ring 0.17.14` |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 | `unicode-ident 1.0.24` |
| `BSD-2-Clause OR Apache-2.0 OR MIT` | 1 | `zerocopy 0.8.56` |

> 逐个 crate 的版权行/许可原文：见 `LICENSES/rust/`（按文件内容去重，路径与 crate 的对应关系见 `data/rust-deps.json`）。
> MPL-2.0 组件（如 `windows`/`winapi` 系、`option-ext` 等）按 MPL-2.0 §3.1 以**未修改**形式静态链接；其源码可从 crates.io 对应版本获取（`Cargo.lock` 钉版本）。

## C/C++ 依赖（清单 B：vcpkg 静态链接，11 个 port）

| port | 许可摘要 | 版权/许可全文 |
|---|---|---|
| `freetype` | 见全文（27420 B） | `LICENSES/c-deps/freetype.txt` |
| `harfbuzz` | 见全文（1971 B） | `LICENSES/c-deps/harfbuzz.txt` |
| `graphite2` | 见全文（28330 B） | `LICENSES/c-deps/graphite2.txt` |
| `icu` | 见全文（25185 B） | `LICENSES/c-deps/icu.txt` |
| `fontconfig` | 见全文（8616 B） | `LICENSES/c-deps/fontconfig.txt` |
| `libpng` | 见全文（5345 B） | `LICENSES/c-deps/libpng.txt` |
| `brotli` | 见全文（1084 B） | `LICENSES/c-deps/brotli.txt` |
| `bzip2` | 见全文（1896 B） | `LICENSES/c-deps/bzip2.txt` |
| `dirent` | 见全文（1083 B） | `LICENSES/c-deps/dirent.txt` |
| `expat` | 见全文（1144 B） | `LICENSES/c-deps/expat.txt` |
| `zlib` | 见全文（1002 B） | `LICENSES/c-deps/zlib.txt` |

- FreeType 采用 **FTL**（FreeType License，BSD 风格）或其 GPLv2 双许可；本产品选择 **FTL**，因此不触发 GPL 传染。
- fontconfig / graphite2 为 **MPL-2.0**；本产品未修改其源码，按 MPL-2.0 静态链接。
- ICU 为 Unicode License；harfbuzz/brotli/dirent/expat/zlib/bzip2/libpng 为宽松许可。全文见上表文件。
- `fontconfig` 的 `nls` 特性**未开启**（避免引入 LGPL 的 gettext-libintl，见许可审计 §3 约束②）。

## 内置精简宏包集（bundle）逐包许可证

随包 bundle 共 126 个 TL 包（归属自 TL 2026 tlpdb 映射；10 个生成物/产品文件见文末）。许可分布与全文：

| 许可证 | 包数 | 对应规范全文 |
|---|---|---|
| `(none)` | 47 | 见该包 TL 上游 |
| `lppl1.3c` | 25 | LPPL-1.3c |
| `lppl1.3` | 22 | LPPL-1.3c |
| `lppl` | 7 | LPPL-1.3c |
| `gpl` | 6 | GPL-2.0 |
| `knuth` | 3 | Knuth（见 §6 说明） |
| `pd` | 3 | 公有领域（无义务） |
| `other-free` | 3 | 见该包 TL 上游 |
| `ofl` | 1 | OFL-1.1 |
| `lppl1.3c gpl2+ fdl` | 1 | 见该包 TL 上游 |
| `gpl2` | 1 | GPL-2.0 |
| `lgpl` | 1 | LGPL-2.1 |
| `mit` | 1 | LICENSES/rust/ 或上游（见 §3） |
| `gfl` | 1 | GFL-1.0 |
| `lppl1.3c gpl2 fdl` | 1 | LPPL-1.3c + GPL-2.0 + GFDL-1.3 |
| `lppl gpl` | 1 | 见该包 TL 上游 |
| `lppl1.3c other-free` | 1 | 见该包 TL 上游 |
| `x11` | 1 | X11/MIT 风格（见 §3 LICENSES/rust/） |

### 5.1 完整文本文件


### 5.2 字体类许可（G-L5）

- **Fandol**（6 个 OTF）：GPLv3 + 字体嵌入例外 ⇒ 见 §7 的 Corresponding Source；全文 `LICENSES/gpl-3.0-fandol.txt`。
- **Latin Modern**（lmroman/lmsans/lmmono，GFL-1.0）：GFL 全文 `LICENSES/gfl-1.0.txt`；本产品未改字体名、未修改字体文件。
- **AMS fonts**（amsfonts，OFL-1.1）：`LICENSES/ofl-1.1.txt`（含 Reserved Font Names 清单，未改名）。
- **Computer Modern 系**（cm/mm/cm-super 的 TFN/PFB，Knuth 许可）：按 §6 说明分发；未修改。
- **zapfding**（pzdr.tfm，GPL）：源码见 §7；AFM 随包。

### 5.3 无 catalogue-license 文件（47 项）的文件头声明

TL 的 hyph-utf8 系列（`hyphen-*` patterns）与 `tlperl.windows`（Unicode 数据文件）在 tlpdb 里无 `catalogue-license` 字段，但**逐文件头**均有版权/许可声明（GPL 或 LPPL 或 Unicode 许可）。审计脚本 `test_file/audit_bundle_heads.py` 的输出（47/47 覆盖，0 项无声明）见许可审计 §8.3；随包文本：`dehyph*-x-2022-03-16.*`、`hyph-quote-rm.tex`、`SpecialCasing.txt`、`CaseFolding.txt`、`UnicodeData.txt` 等。

## Knuth 字体许可（Computer Modern / TeX 字体）

随包 bundle 含 Computer Modern 家族（`cmr10.pfb`、`cmmi*.pfb`、`cmsy*.pfb` 等，tlpdb 许可证字段 `knuth`）。
Knuth 的 TeX 字体许可允许**未修改**分发并用于排版文档，但要求：① 分发不得修改文件内容；② 必须附带本声明（本文件即声明）；
③ 完整许可原文以上游为准：CTAN `systems/knuth/dist/` 的 `license` 文本（https://ctan.org/pkg/knuth）。
本产品**未修改**这些字体文件（逐文件 sha256 由 `data/bundle-packages.json` + bundle SHA256SUM 锚定）。

## GPL 字体的 Corresponding Source（随包）

GPL 许可要求随二进制提供**对应源码**（GPLv3 §6）。本安装包 `licenses/sources/` 内附：

| 随包二进制 | 包/许可 | 源码目录 | 文件数 / 字节 |
|---|---|---|---|
| `FandolSong-Regular.otf` | fandol / GPL-3.0-or-later WITH font-exception | `sources/fandol/FandolSong-Regular/` | 5 / 7012478 |
| `FandolSong-Bold.otf` | fandol / GPL-3.0-or-later WITH font-exception | `sources/fandol/FandolSong-Bold/` | 5 / 7502018 |
| `FandolKai-Regular.otf` | fandol / GPL-3.0-or-later WITH font-exception | `sources/fandol/FandolKai-Regular/` | 5 / 10132945 |
| `FandolHei-Regular.otf` | fandol / GPL-3.0-or-later WITH font-exception | `sources/fandol/FandolHei-Regular/` | 5 / 8994678 |
| `FandolHei-Bold.otf` | fandol / GPL-3.0-or-later WITH font-exception | `sources/fandol/FandolHei-Bold/` | 5 / 6559746 |
| `FandolFang-Regular.otf` | fandol / GPL-3.0-or-later WITH font-exception | `sources/fandol/FandolFang-Regular/` | 5 / 9844429 |
| `pzdr.tfm` | zapfding / GPL (TL zapfding) | `sources/zapfding/` | 1 / 9467 |

- Fandol 源码来自上游仓库 `https://github.com/Yixf-Self/fandol-fonts`（pin commit `b93300821373d3092e378e57ae22ecb8e9082c62`），含 `cidfont.ps`/`AFM`/`features`/`fontinfo` 等 AFDKO 构建输入；见 `sources/fandol/SOURCE.md`。
- 字体嵌入例外：Fandol 的 GPL 附「将字体嵌入文档不使文档受 GPL 约束」例外（`LICENSES/gpl-3.0-fandol.txt` 文末）。
- 可复现：用 Adobe AFDKO `makeotf` 在各字目录生成 OTF；构建工具与步骤见 SOURCE.md。

其余 GPL/LGPL 组件（CJKfntef.sty / fonttext.cfg / ckx.map / hyphen 系列 patterns）的 preferred form 即文本文件本身或其上游 TL 源（cslatex.dtx、dvipdfmx、hyph-utf8），见 `data/sources.json` 的 `selfSourced`。

## 被修改的 TeX Live 文件（G-L6）

Tectonic 对 TeX Live 打补丁；本随包代表集里**实际包含**的 patch 文件如下（原件获取方式与变更说明）：

### latex.ltx（随包）

- 许可：LPPL 1.3c
- 变更说明：Tectonic 对 latex.ltx 打补丁以支持其「格式化/预编译 format」与跨平台确定性输出（见 patches/latex.ltx.diff）；文件名保持 latex.ltx（LPPL 1.3c 允许分发修改版，但须明确标注为修改版并附变更说明）。
- 许可影响：修改版仍按 LPPL 1.3c 分发（LICENSES/lppl-1.3c.txt）；变更说明与 diff 随包，原件可从上方 originalSource 获取。
- 补丁：`patches/latex.ltx.diff`（sha256 `83e9f27ad334208c58fbf9237488f3e07eb15cda24b3e3bf157ee8d5e660d4e3`）
- 随包文件 sha256：`70ba1d0d113a986e4966e5a7ebb5afa3fbb17843ec4395bd0187d2aa266b5646`
- 原件：TeX Live 2024-03-12 的 latex-base（CTAN macros/latex/base，2024 快照；Tectonic bundles/bundles/texlive2024-0312 取自该版本）

### listings.sty（未随包）

- 许可：LPPL 1.3
- 变更说明：不在随包代表集内（未分发），仅登记 Tectonic 对该文件的 patch 存在。
- 许可影响：未随包 ⇒ 无分发义务；补足完整 bundle 时需重新审计。
- 补丁：`patches/listings.sty.diff`（sha256 `4beb0d8453aa8faef77d27a7044686720311dd45592d81de760a490b3120f133`）
- 原件：CTAN macros/latex/contrib/listings（TL 2024-03-12）

### fontawesome.sty（未随包）

- 许可：LPPL 1.3c / OFL（字体部分）
- 变更说明：不在随包代表集内（未分发），仅登记 patch 存在。
- 许可影响：未随包 ⇒ 无分发义务。
- 补丁：`patches/fontawesome.sty.diff`（sha256 `4c525257216d26e109c12c0d1dc0719e0118514e1504966ebc3ee95fbb8df78b`）
- 原件：CTAN fonts/fontawesome（TL 2024-03-12）

### fithesis-mu-base.sty（未随包）

- 许可：LPPL 1.3c
- 变更说明：不在随包代表集内（未分发），仅登记 patch 存在。
- 许可影响：未随包 ⇒ 无分发义务。
- 补丁：`patches/fithesis-mu-base.sty.diff`（sha256 `0fabb9350a03fac9a184773c73a8cecca1790a900f2261fe8cf593873e535ef2`）
- 原件：CTAN macros/latex/contrib/fithesis（TL 2024-03-12）

> LPPL 1.3c 允许分发修改版，但要求明确标注为修改版并给出变更说明（本文件即该声明）；补丁与原件获取方式随包。

## 无法归属的文件（10 项）

下列文件按 basename 映射不到 TL 包（多为生成物/产品文件），人工归属如下：

| 文件 | 归属 |
|---|---|
| `SHA256SUM` | 本产品目录 bundle 的 digest 文件（非 TL 内容） |
| `dehyphn-x-2022-03-16.pat` | TL hyph-utf8 生成物（TeX Live 构建期由 dehyphn-x 生成）；来源 TL 2024-03-12，许可见 hyph-utf8（LPPL/GPL 组合，逐文件头声明） |
| `dehyphn-x-2022-03-16.tex` | 同上 |
| `dehypht-x-2022-03-16.pat` | 同上 |
| `dehypht-x-2022-03-16.tex` | 同上 |
| `hyph-quote-rm.tex` | TL hyph-utf8（文件头声明，见 LICENSES/gpl-2.0.txt / knuth 相关声明） |
| `kanjix.map` | TL uptex-base 生成物（map 数据，来源 TL 2024-03-12；许可随 uptex-base） |
| `pdftex.map` | TL pdftex 生成物（由 updmap 汇聚各 fontmap；逐包许可见表内各字体/包） |
| `pgfkeysfiltered.code.tex` | TL pgf 生成物（pgf 许可 LPPL1.3c + GPL2 + FDL） |
| `tectonic-format-latex.tex` | Tectonic 自带（MIT，Tectonic 项目） |

## 声明获取与联系方式

- 本文件与全文位于安装目录的 `licenses/` 下；源码在 `licenses/sources/`。
- 复审与生成：仓库 `scripts/make-notices.mjs`（`--check` 可对随包物复核；`--collect` 用于换 bundle/依赖后重采集）。
- 许可审计与门禁记录：仓库 `docs/dev/tectonic-licensing.md`（G-L1..G-L9）与 `docs/adr/0016-ship-bundle-offline.md`。
