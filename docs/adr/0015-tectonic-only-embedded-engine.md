# 0015 Tectonic 唯一引擎：删除其它引擎与 latexmk 路径，只保留内嵌形态

- **状态**：已接受（2026-11-16，产品负责人）
- **相关**：[0005](./0005-latexmk-first-incremental-next.md)（latexmk 起步／增量后续——其"latexmk 起步"部分被本 ADR 取代）、[0012](./0012-tectonic-library-form-engine.md)（库形态引擎——"两形态可选"被取代，路径 B 与 X-5 例外继续有效）、[0013](./0013-synctex-in-process-parser.md)（进程内自解析——唯一形态下更加必需）、[0014](./0014-tectonic-first.md)（Tectonic-first——"两形态基准"被取代）

## 背景：矩阵还在，简化没发生

[ADR-0014](./0014-tectonic-first.md) 把 Tectonic 立为设计与验收基准，并把默认引擎切到 Tectonic，但没有删掉别的引擎：代码里仍是 **4 个引擎（Tectonic / XeLaTeX / LuaLaTeX / pdfLaTeX）× Tectonic 两种形态（子进程 / 库内嵌）**。结果是"简化"只发生在文档口径上，实现里仍要为每一步变更回答"另一条路怎么办"：

- **能力与形态绑定**：页哈希、编译中预览、内存产物、format 缓存只有库形态有；子进程形态不落 XDV ⇒ 页级复用与编译中预览在那一档根本不存在。
- **兼容路径与引擎绑定**：模板自带 `latexmkrc` 的中和（㉖）、`latexmk -e` 之类的判定只对 latexmk 存在；换引擎即换一套失败语义。
- **设置面与展示面为此付税**：引擎清单、可用性探测、形态位 chip、`lib_form` 闸门、`LATTESET_TEX_ENGINES` / `LATTESET_TECTONIC_LIB` 两套覆盖开关，全是为"多档并存"服务的。

产品决定：**取消矩阵**。引擎只剩 Tectonic，形态只剩内嵌，后续所有性能工作只对这一条路径负责。

## 决策

1. **唯一引擎 = Tectonic**。删除 XeLaTeX / LuaLaTeX / pdfLaTeX 的枚举、命令构造、可用性探测、设置项、展示文案，以及 **latexmk 驱动路径**（含 `latexmkrc` 中和、`-e` 注入、`-outdir` 语义、依赖 latexmk 产物的诊断）。
2. **唯一形态 = 内嵌引擎**。`tectonic-lib` 由"可选构建变体"变为**唯一构建形态**：删除子进程路径（`tectonic.exe` 命令构造、bundle 缓存正向证据判定 `-C`、进程树杀、stdout `[N]` 页进度）、删除运行期开关 `LATTESET_TECTONIC_LIB`、删除设置项 `tectonic.lib_form` 与 UI 形态位。
3. **设置迁移必须如实**：删除 `compile.engine`（全局与项目覆盖两层），`schema_version` 升到 2；旧值不作为错误、被忽略；升级后**首次启动给一次性提示**（状态栏 + 日志）"引擎已统一为内嵌 Tectonic，原引擎设置不再生效"。不保留"看着能选、实际不生效"的假字段。
4. **兼容边界照单接受，且不回退**：biber / makeindex / glossaries、依赖模板 `latexmkrc` 的构建约定、bundle 里没有的 TeX Live 专有包与字体，**不再支持**。检出即在诊断里显式说明"当前引擎不支持"，**不静默降级、不自动切回别的工作流**（切回等于把刚删掉的路径请回来）。**2026-11-16 实测澄清（不改本决策的边界，只校正机制与措辞）**：Tectonic 0.17 **会调用外部 biber**（上游 `external_tool_pass`；直跑日志 `note: Running external tool biber`），只有 biber **缺失**或**版本与文档用的 biblatex 不匹配**时才失败（实测 bundle 内 biblatex 3.17 vs 本机 biber 2.21）；真正"不自动跑"的是 **makeindex / glossaries**。所以上面"不再支持 biber"应读作**"不自带、不保证可用"**（成功路径依赖用户自装且版本匹配），**不是"引擎不会调用它"**；库内嵌形态才是"完全不调用外部 biber"的那一档。biber 缺失/版本错配时仍按本决策给显式诊断、不静默降级，但**文案不得写成"引擎不支持 biber"**。证据 `test_file/acc/REPORT-acceptance.md` §2.5。
5. **性能口径不变，且先测后调**：验收仍用既有[延迟预算](../glossary.md)（小文档 1s 优秀 / 2s 及格，大文档 3s / 5s）；本次重构的第一个交付是 **release 内嵌形态的六档基线**（cold / noop / edit），之后的优化必须拿这条基线说话，不得用跨形态或 debug 数字充当结论。
6. **本 ADR 取代**：ADR-0005 的"latexmk 起步"取向、ADR-0012 的"两形态可选"、ADR-0014 的"两形态基准"。**继续有效**：ADR-0012 的路径 B 实现与 X-5 例外登记（同步上下文里的 `std::fs`、C 库自己读字体/config、C 层 abort 不可树杀、PDF 加密路径的 `SOURCE_DATE_EPOCH` 未决）、ADR-0013 的进程内 SyncTeX 自解析、ADR-0010 的 crate 边界。

## 后果

**正面**：① 一条证据链、一条优化落点——每一步只回答"这一档怎么样"；② 产品不再依赖用户本机装 `tectonic.exe` 或 TeX Live，能力面（页哈希、编译中预览、内存产物、format 缓存）对所有用户一致可用；③ 设置面、状态栏、CLI/MCP 的引擎开关整体消失，展示面不再需要"如实标注哪一档不可用"这类补丁式诚实。

**负面／风险**（必须逐条跟进）：

- **构建面变重**：所有构建（含 `npm run tauri dev`、CI 默认档）从此需要 vcpkg + `x64-windows-static-release` + cmake/pkg-config/nasm，且缺一个 port 就是硬失败。本机当前**没有 vcpkg**。
- **没有回退**：内嵌形态的 bug 不再有"切回子进程"这条逃生门；C 层 abort/panic 无法树杀 ⇒ 取消语义只有 X-3 那一种（下一个 I/O 回调返回错误），必须如实写进 UI。
- **U-14 从"风险"升级为"唯一路径的风险"**：子目录 `\include` 的中间文件与搜索根至今未验证，发布前必须跑绿。
- **体积与许可**：内嵌形态的安装包体积增量 `[未测到]`；bundle 许可审计已出（[tectonic-licensing.md](../dev/tectonic-licensing.md)：R3 = 0，结论"允许、带 R2 义务"），随包决策见 [ADR-0016](./0016-ship-bundle-offline.md)；发包前仍须对**实际随包的 bundle** 跑绿 G-L1..G-L9。
- **外部工具文档永久失效**：需要 biber / makeindex / glossaries 的文档在 Tectonic 下没有替代路径，只能给出可行动诊断（本 ADR 第 4 条）。⚠ **biber 例外（2026-11-16 实测）**：子进程形态会调用本机 biber，装了匹配版本即可正常生成参考文献（库形态仍不调用）；"没有替代路径"只对 makeindex / glossaries 严格成立。
- **发布构建时间**：CI 的内嵌档冷构建约 +15–20 min（现有记录），从"版本 tag 才跑"变成"每次都要跑"。

## 验收与退出条件（本次重构）

1. 代码中不再存在非 Tectonic 引擎分支与 latexmk 路径（`grep` 可证），`Engine` 只剩一个值或被类型收窄。
2. release 内嵌形态在**六档夹具**上出 cold / noop / edit 基线，报告进 `docs/design.md`；编辑→出图不劣于改动前的内嵌档实测（1 页暖编 372–642ms）。
3. U-14（子目录 `\include`）跑绿，含中文路径。
4. 旧 `settings.json`（`engine: "xelatex"`、`tectonic.lib_form: true`）升级后**不报错**，首启出现一次性提示，日志有记录。
5. 需要 biber 的文档产出显式诊断而非笼统失败。
6. 真机启动到可编辑 ≤60s；`npm run build` 通过。

## 回退

**回退成本高**：删掉的是整条子进程路径与三个引擎的命令构造，恢复要靠 git 历史反打，并且用户可见行为（不再要求装 Tectonic/TeX Live）会随之改变；这正是本决策需要单独成 ADR 的原因。若内嵌形态出现阻塞级问题，优先修内嵌形态，而不是恢复矩阵。

## ②段执行记录（2026-11-20，只记录执行事实，不改本决策正文）

> 性质：本 ADR 决策 2 的施工记录与验收证据。数字与结论来自 `test_file/phase2/REPORT-gates.md`（T9，工作区本地、未提交）与 `test_file/build-env/REPORT-build-env.md`。**不改变**上面任何决策条文。

**落地范围**

1. **代码**：`crates/latteset-infra/src/runner.rs`（1844 行）与 `crates/latteset-infra/src/tectonic/` **整文件删除**；`src-tauri/src/runner_switch.rs` 删除，换成 `src-tauri/src/runner.rs` 的 `EmbeddedRunner` 薄壳（每趟按当前设置重建 `TectonicLibRunner` ⇒ bundle/cache 改完即生效）；`src-tauri` 与 `latteset-server` 对 `latteset-tectonic` 改为**非可选依赖 + `network-bundle`**；根 `Cargo.toml` 删除 `default-members`。headless `build_runner` 读 `Session.settings.tectonic`（显式设置 > 环境变量 > 宿主默认）。`lib_form_available` / `engine_form` / `EngineForm*` 命令与 DTO 删除（命令面 18 条），`src/bindings.ts` 重新生成，T7 核对 13/13 PASS。
2. **设置 v3**：`SCHEMA_VERSION = 3`；`TectonicSettings` 只剩 `bundle` / `cache_dir`；迁移链 **v1→v2→v3**，按迁移源版本备份 `.v1.bak` / `.v2.bak`（已有不覆盖），两条 v3 notice（`form-embedded-v3` 恒加、`subprocess-form-removed-v3` 仅旧值 `lib_form == false`）。`LATTESET_TECTONIC_LIB` 已删除，启动时检测到打一条 warn。
3. **构建/CI**：`scripts/with-tectonic-lib.ps1` 成为唯一入口（`-Mode check|tauri|test|cli`），`npm run tauri dev/build`、`lib:test`（全仓 4 crate）、`lib:cli` 全走它；CI 默认 Windows 档改为内嵌构建并**删除**独立的 `build-windows-tectonic-lib` 档；Ubuntu 档不再跑 server（R11）。
4. **前端**：状态栏形态 chip、设置页形态选择、「已排版 N 页」保持（页进度改由内嵌形态给出）；两个 spec 同步改，vitest 151 passed。
5. **文档**：`docs/` 与 `README.md` 按"内嵌唯一"收口（`docs/dev/tectonic-library.md` 重写、`modules.md`/`design.md`/`glossary.md`/`cli.md`/`troubleshooting.md`/`roadmap.md` 同步）。

**验收结果（T9，全部 PASS；证据见报告）**

| 项 | 结果 |
|---|---|
| Rust 用例 | core 268 passed；infra 40 passed；tectonic 44 passed / 1 ignored（`--ignored` 真编通过）；server lib 12+4 / cli 8 / mcp_stdio 5+1；`npm run lib:test` 合计 **377 passed / 6 ignored** |
| headless 真编（唯一证据链） | U-14 中文目录+中文子目录 `\include` exit 0（3 页、0 U+FFFD）；冷缓存 Full 1,200 ms；热缓存 Quick 请求 75 ms（如实报 full）；内容错误精确归属 `chapters/ch1.tex:3`；PDF 校验全过 |
| 启动 | 热增量到 watch 线程 **5.78 s**；强制重编 **12.96 s**（≤60 s） |
| 前端 | `npm run build` 0 error；`npm run test` 17 files / 151 tests |
| 绑定生成物 | 13/13 PASS |
| 真机窗口/可编辑 | **`[未测]`（会话环境限制）**：工作区 Low 完整性标签导致 WebView2 起不来；替代证据（启动时长 / MCP bridge / watch 线程 / bindings 自动导出 / headless 全链路真编）见报告 §2.1 |

**实测口径修正（记录用，不改决策）**

- **biber**：②段内嵌形态**不调用外部 biber**（T9 实测：本机 PATH 上有真 biber 2.21 也不被调用、不产 `.bbl`，只发 `compile-notice`；`backend=bibtex` 由内建 BibtexEngine 出 `.bbl`）。本 ADR 第 4 条 2026-11-16「子进程形态会调用本机 biber」那半，随子进程形态一并删除 ⇒ **②段起应读作"内嵌形态不跑外部 biber"**；决策 4 的"显式诊断、不静默降级"要求不变。
- **`-C` / stdout `[N]` / 树杀**：随子进程路径删除，不再有对应语义；取消改为协作式（X-3），延迟如实写进 UI。
- **构建环境**：本机此前记录的"无 vcpkg / `cl D8050`"被证伪——vcpkg 在 `test_file/vcpkg` 可用，D8050 根因是工作区 Low 完整性标签（绕法：`TMP`/`TEMP` 指到工作区内目录；彻底修：`icacls` 改回 Medium）。属环境归属，不是本决策的代价项变化。

**遗留（交③段）**：六档 cold/noop/edit 基线 `[未测]`；联网默认 bundle 的下载行为 `[未测]`；安装包体积增量 `[未测]`；真机窗口可编辑性需在可写 `%APPDATA%` 的会话补验；bundle 分发方案按 ADR-0016「实测修正」重定。
