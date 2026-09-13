# Tectonic 集成落地方案（评估 + 明天要做的四件事）

> 背景：本轮已把 Tectonic 0.17.0 装好并实测完（安装记录与数字见 [modern-engines-zh.md](./modern-engines-zh.md) §8）。
> 本文是**集成可行性结论 + 可执行计划**，回答"能否把 Tectonic 集成进 Latteset、连缓存一起带上，让大多数情况下直接读缓存、用户开箱即用"。
> 结论先行：**能，两半都成立**（Tectonic 官方自我定位就是 *embeddable*；缓存是普通文件、且可以指向本地 bundle 完全离线）。
> 但**不能直接打包**——先过许可审计与三条未验证项（见 §4）。明天（2026-09-14）按 §5 的顺序做。

## 1. 结论：能集成，且"带缓存"有三条路

| 做法 | 机制 | 实测依据 |
|---|---|---|
| ① 预置缓存目录 | 缓存就是文件：`%LOCALAPPDATA%\TectonicProject\Tectonic\cache\{bundles,formats}`；安装器拷一份即可。**formats 按 bundle digest 缓存**，换 bundle 自动失效，不会串味 | `src/io/format_cache.rs`；本机实测缓存 426 文件 / 62 MB（**但只是我们夹具用到的部分**） |
| ② 自带 bundle（**推荐**） | `-b/--bundle <URL\|path>`（帮助原文 "Use this URL or path to find resource files instead of the default"）+ `-C/--only-cached`（只用本地缓存）⇒ **随安装包发 bundle、运行期完全不联网**，也不依赖 `relay.fullyjustified.net` | CLI `--help` 实测；**`-b` 指本地 tar 尚未实测** |
| ③ 混合（子集 + 按需） | 装"中文常用子集"，缺什么再懒下载（Tectonic 默认形态） | 首跑实测：`hello` 189 s、`min-zh` 214 s（104 个按需下载）、thesis 51 s |

**体积上的一个好消息**：中文实测**不依赖 bundle 字体**也能排（ctex → fontset=windows + fontconfig，0 缺字），所以 bundle 可以不塞 CJK 字体，压力主要来自宏包集合。

## 2. 嵌入的两种形态

**形态 B（先做，低风险）：把官方 `tectonic.exe` 当"第二个引擎"用子进程驱动。**
- 我们现在的架构**本来就在起子进程**（`xelatex`/`latexmk`/`xdvipdfmx`），所以改动集中在一个新的引擎分支 + 命令构造 + 收尾逻辑。
- 已实测就绪的三件事：**`--outfmt xdv` 的产物是我们 `xdv.rs` 认的格式**（264,736 B、`scripts/xdv-report.mjs` 解出 28 页）⇒ 页级复用 A/B/C 不用改；`--synctex` 有产物；`SOURCE_DATE_EPOCH=0` 下 PDF/XDV 逐字节可复现（㉚ 前提成立）。

**形态 A（可选，后做）：把 `tectonic` crate 作为库内嵌。**
- 好处：XDV 可留在内存、能拿到它的 `XdvEvents`（`handle_begin_page`）做页事件，不装外部可执行。
- 代价：把一大坨 C 依赖拖进构建链（harfbuzz 需子模块源码；Windows 官方路线是 **vcpkg + 他们的 `cargo-vcpkg` 分支**）⇒ CI 明显变重；且要重新划 ADR-0010 的边界。**除非确实要页级实时，否则不必。**

## 3. 与我们现有架子的摩擦点（这才是真工作量）

| # | 摩擦点 | 具体影响 |
|---|---|---|
| 1 | **文件来源模型不同**：Tectonic 不读用户 TeX Live | ㉒（`.fls` 触发面）、㉖（模板 `.cls` 探测 / `kpsewhich` 回落）、㉗（`.ins`/`.dtx` 源码版模板提示）都建立在"本机 TL"上 → 该模式下要么换成 bundle 内查询，要么明确不适用 |
| 2 | **流式反馈要适配** | 阶段 2 的"编译中看错误/页数"是按 XeTeX/latexmk 输出形态做的（尾随 `tmp/<stem>.log` + stdout `[N]` 页标记）；Tectonic 的 stdout 是 `note: …`、有 `--chatter` 档位，且 **bibtex 错误默认被吞**（要 `--print`） |
| 3 | **Quick/Full 语义要重新定义** | 可用 `--outfmt xdv` 当"只排版"档（≈ `-no-pdf`）、`-r/--reruns` 控收敛；但状态栏"引用待更新"那套提示要重新对齐 |
| 4 | **SyncTeX 只验到"有产物"** | 77,983 B 的 `main.synctex.gz` 存在，但**没验** 我们 CLI 的 `forward`/`inverse` 在它上面是否与 XeLaTeX 基线等价 |
| 5 | **安装包体积** | 带 bundle 就是几百 MB 级，需要产品决策（"下得动" vs "装完就能用"） |

## 4. 许可证：**先审计，别直接打包**

- **Tectonic 本体 MIT**（仓库 LICENSE，"Copyright 2016-2023 the Tectonic Project"）⇒ 嵌入 / 静态链接 / 闭源分发都可以。
- **bundle 是 TeX Live 文件集合**：TL 整体不是单一许可证，**逐包**混着 LPPL / GPL / MIT / OFL…，且修改过的文件有"改名"之类条件（LPPL）。
- **C 依赖里可能有 LGPL**（如 graphite2 一类）——静态链接到闭源产品时有"允许重新链接"的义务。
- ⇒ **必须走一遍正式审计**（`cargo metadata` 全依赖 + bundle 内 TL 包清单），产出 `docs/research/tectonic-bundle-licensing.md`。**在审计完成前不要把 bundle 放进发行物。**

## 5. 明天（2026-09-14）要做的四件事（按顺序，都可独立验证）

1. **许可审计**：列全依赖与 bundle 内包清单 → `docs/research/tectonic-bundle-licensing.md`；标出 LGPL/GPL/OFL 风险项与结论（可否闭源分发、需要哪些声明）。
2. **本地 bundle 实测**：按 `bundles/README.md` 造一个"中文常用"bundle（需要 TeX Live tarball + GNU patch），用 `-b <本地tar> -C` **离线**编译 `bench/_zhcmp/thesis`，记录 bundle 体积与编译耗时。
3. **SyncTeX 往返等价性**：用 `latteset-cli --project … forward/inverse` 在 Tectonic 产物上跑，与 XeLaTeX 基线对拍（页码/行号偏差），产出结论。
4. **日志/流式适配调研**：确认 `--keep-logs` 产出的 `main.log` 是否仍能被现有 `parse_log`/`pages_typeset` 直接吃（XeTeX 同源，**可能零改动**）；不行就列出需要新增的解析规则。

**四件事做完后的决策点**：若 ①②③ 通过 → 进入形态 B 的落地（`Engine::Tectonic` 分支 + 设置项 + 收尾 + 真机验证闭环），验收标准：
- 装了 TeX Live 的机器与**没装 TeX Live 的机器**都能打开中文项目并出 PDF/预览；
- 页级复用生效（`pages > 0` 且 `changed_pages` 正确）；
- SyncTeX 双向可用；`SOURCE_DATE_EPOCH` 确定性保持；`npm run build` 通过；文档同步。

## 6. 本机现成的东西（明天可直接用）

| 类别 | 位置 |
|---|---|
| 已安装可执行 | `C:\Users\Vinci\.cargo\bin\tectonic.exe`（0.17.0；该目录已在 PATH 上） |
| 官方安装包（留档） | `test_file/tectonic-install/tectonic-0.17.0-x86_64-pc-windows-msvc.zip`（20.1 MB，SHA-256 与官方 digest `f61ce51f…845f` 一致）+ `unpacked/tectonic.exe` |
| 源码副本（结构分析用） | `test_file/tectonic-src`（871 文件 / 12.6 MB；tarball 不含 harfbuzz 子模块） |
| 缓存 | `%LOCALAPPDATA%\TectonicProject\Tectonic\cache\{bundles,formats}`（426 文件 / 62 MB） |
| 中文夹具与产物 | `test_file/projects/bench/_zhcmp/tect/`（hello、min-zh）、`tect-thesis/`（thesis 副本 + `main.xdv`(264,736 B) + `main.synctex.gz`） |
| 对比脚本 | `test_file/projects/bench/_zhcmp/tectonic-bench.ps1`（交错 3 轮、逐次校验产出） |
| 实测数字 | [modern-engines-zh.md](./modern-engines-zh.md) §8.3（速度）/ §8.4（机制对接） |

> 以上 `test_file/**` 实验件均已 gitignore，不随仓库分发。

## 7. 明确未验证（别当结论）

bundle 最终体积（取决于裁剪策略）、`-b` 指本地 tar 的实测、逐包许可证清单、真实学位论文模板（图表/公式密集，如 hithesis）上的表现、bibtex/biber 全链路、以及 ㉒/㉖/㉗ 在 Tectonic 模式下的适配成本。
