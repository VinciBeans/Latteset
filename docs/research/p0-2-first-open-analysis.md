# P0-② 首次打开即用 —— 可行性分析

> 状态：**分析快照（2026-09）**——文中"现状"列描述的是分析当时的实现；后续落地与证据见 [roadmap](./tex-ide-roadmap-priority.md) §1 基线（②-1 已落地、②-2 经 ⑲ 实测砍掉、④ 诊断已落地）与 [modules.md](../modules.md) §8。
> 上游：[tex-ide-roadmap-priority.md](./tex-ide-roadmap-priority.md)（②-2 已否决，见该文 §7；②-1 见该文 §1 基线）、[tex-ide-pain-points.md](./tex-ide-pain-points.md) P9/P7
> 证据约定：**实测** = 本文在本机跑出的可复现结果；**[推断]** = 分析判断；**待验证** = 尚未实测
> 结论一句话：**P0-② 目前只完成了一半的一半**——根文件探测有了，但「多候选/零候选」对用户不可见，且引擎/编译链/字体/缺包四类推断完全没有；其中**只有「引擎推断」是真正的刚需**（实测：引擎选错 = 首次打开直接失败且错误信息不可读）。

## 0. TL;DR

| # | 能力 | 现状 | 判断 |
|---|---|---|---|
| A | 找到根文件 | ✅ 已实现（ADR-0009 正则启发式） | 保留 |
| A′ | 多候选/零候选时让用户**能选** | ❌ **候选列表被丢弃，前端只 `console.warn`** | **最小改动、最高性价比** |
| B | 推断引擎（xelatex/pdflatex/lualatex） | ❌ 只有全局默认 xelatex + 手动覆盖 | **刚需**（实测见 §3） |
| C | 推断编译链（bib 工具 / shell-escape / makeindex） | ❌ 无 | **大部分不需要做**：latexmk 已自动接管 bib 工具链（实测） |
| D | 缺宏包/文档类 → 可操作诊断 | ⚠️ 只有原始 log（`File 'x.cls' not found`） | 与 P0-④ 合并，不单独做 |
| E | 中文字体可用性探测 | ❌ 无 | 低优先（属环境探测，非项目推断） |

**建议切分**：②-1 根文件候选可见可交互（小）→ ②-2 引擎推断（核心）→ ②-3 缺包诊断并入 P0-④。

## 1. 问题定义：什么叫"首次打开即用"

场景：用户下载一个学校/期刊模板 zip → 解压 → 在 TexPresso 里「打开项目」→ **期望不配任何东西就看到 PDF**。

这条链上有 5 个可能断掉的点（对应上表 A–E）。断在哪一环，用户都得自己去当"工具链工程师"——而调研里绝大多数用户不会：

- 模板报错是中文社区高频求助（[P9](./tex-ide-pain-points.md)，B 级证据）
- 通用编辑器方案的第一痛点就是"编译 recipe 要手写两套 JSON + 13 个占位符"，社区原话称之为 **"fighting the IDE"**（[VS Code 专项](./vscode-latex-workshop-pain-points.md)，A 级证据）

→ **P0-② 的本质是把这套"配置知识"从用户脑子里搬进产品里。**

## 2. 现状盘点（带证据）

| 环节 | 现状 | 证据 |
|---|---|---|
| 打开项目 | `resolve_project_root` → 读项目覆盖 → 读全局 → `effective` 合并 → 根文件探测 | `src-tauri/src/commands.rs:91-158` |
| 根文件探测 | 候选 = 含 `\documentclass` 且未被引用；`Unique` 采用，`Multiple`/`None` 返回 `None` | `root_detect.rs`、ADR-0009 |
| **多候选去向** | `RootResolution::Multiple(_)` 的 `Vec` **被直接丢弃**（`Multiple(_) \| None => None`） | `commands.rs:140-141` |
| 契约 | `ProjectInfo { root, root_file }` —— **没有候选字段，前端拿不到候选列表** | `crates/texpresso-core/src/types.rs:111-116` |
| 前端反馈 | `if (!info.root_file) console.warn("未探测到唯一根文件，请在设置中手动指定 root_file")`——**只有控制台警告，界面上没有任何提示** | `src/App.vue:94-97` |
| 引擎 | 取 `settings.compile.engine`（默认 `XeLaTeX`，可全局改或项目覆盖） | `compose.rs:37`、`settings/model.rs:40` |
| 引擎探测 | **无**。没有任何代码读取文档内容来判断该用哪个引擎 | 全仓无相关实现 |
| 设置层次 | 两层：`global ← project overrides`（`merge()` 字段级 Option） | `settings/merge.rs:8-28` |
| 项目状态 | `ProjectState { root, root_file }`——没有承载"探测画像"的位置 | `project/model.rs:8-13` |
| 编译链 | 无推断；`CompileRequest` 只带 `root_file/project_root/engine/timeout` | `types.rs` `CompileRequest` |

**小结**：`ProjectInfo` 与 `ProjectState` 两个结构都没有"探测结果"的位置——这是 P0-② 一切工作的**结构性前提**，必须先动契约。

## 3. 实测证据：引擎推断是刚需，编译链推断不是

### 3.1 引擎选错 = 首次打开直接失败，且错误不可读（实测）

用同一个中文 `ctexart` 工程，只换引擎：

| 引擎 | 结果 |
|---|---|
| `latexmk -xelatex …` | **exit 0**，PDF 正常 |
| `latexmk -pdf …`（pdflatex） | **exit 12**，log 前 6 条错误全是：`! Missing endcsname inserted.` / `! Use of ??? doesn't match its definition.`（重复） |

**含义**：用户若不知道"ctexart 必须用 xelatex/lualatex"，看到的就是一堆 `???`——**他没有任何可能自己诊断出来**。这不是"体验优化"，是"能不能用"。

### 3.2 latexmk 已自动接管 bib 工具链（实测，**改变了优先级判断**）

两个探针工程：

| 场景 | 结果 |
|---|---|
| 普通文档（含 `\tableofcontents`） | latexmk 日志出现 `Latexmk: Using bibtex to make bibliography file(s).`——**自动跑 bibtex** |
| `biblatex` + `backend=biber` | 日志出现 `=== biblatex/biber in use` / `Latexmk: Using biber to make bibliography file(s).` → **exit 0**，pdflatex → biber → pdflatex×2 全自动 |

**含义**：`bibtex`/`biber`/多次 pass 这些"编译链知识"，latexmk 自己就处理了，**TexPresso 不需要推断，也不需要让用户配**。原路线图里"编译链推断"的清单可以砍掉一大半。

> 记录一条**被证伪的假设**：biber 探针 exit 12，一度归因为 PowerShell `Set-Content -Encoding UTF8` 写了 UTF-8 BOM。**实测证伪**——本机为 pwsh 7.6.5，`-Encoding UTF8` 默认无 BOM（头字节 `5C 64`），与 `UTF8Encoding($false)` 写法一致，两者都 exit 0。真正原因是**探针含中文正文却用 pdflatex 编译**（与 §3.1 同一现象）。BOM 一条的完整账目见 [troubleshooting.md](../troubleshooting.md)。

### 3.3 仍未验证的部分

- **`-shell-escape`（minted 等）**：本机 **无 `minted`/`pygmentize`**，未实测。latexmk 默认不加 `-shell-escape`，`[推断]` 这是编译链里**唯一真正需要 TexPresso 介入**的一项。
- **`makeindex`/`glossaries`**：工具在（`makeindex.exe` 存在），但未实测 latexmk 是否全自动接管。
- **缺字体**：未构造缺字体的环境。

## 4. 各项能力的实现路径

### A′ 根文件候选可见可交互（建议先做，最小）

- **契约**：`ProjectInfo` 增 `root_candidates: Vec<PathBuf>`（`Unique` 时为空或单元素均可，语义需定）。
- **前端**：`App.vue` 的 `console.warn` 换成可见提示 + 候选选择（弹窗或状态栏）；选中 → 走既有 `update_settings({ root_file })` 写项目覆盖（**已有通路，零新增后端**）。
- **收益**：直接把"打开没反应"变成"选一个就能用"。这是 P0-② 里**唯一一个既有通路齐全、只缺一个字段和一段 UI** 的子项。
- **风险**：低。

### B 引擎推断（核心）

- **形态**：core 纯函数（符合 ADR-0006/0010：无 IO，内容由上层读好传入）——
  `infer_engine(root_content, included_contents: &[String]) -> Option<Engine>` 或返回带理由的 `EngineHint { engine, reason }`。
- **规则表（v1 最小集，`[推断]` 需实测校准）**：
  | 信号 | 推断 |
  |---|---|
  | `ctex` / `ctexart` / `ctexbook` / `\usepackage{ctex}` / `xeCJK` | `xelatex` |
  | `fontspec` / `unicode-math` | `xelatex`（或 lualatex） |
  | `\usepackage{iftex}`+`\ifPDFTeX` 分支 | `pdflatex`（弱信号） |
  | 以上皆无 | **不推断**（保持全局默认） |
- **触发时机**：打开项目时算一次（与根文件探测同处，`open_project`）。**不**每次编译重算。
- **落点**：`ProjectState` 增 `detected: DetectedProfile { engine: Option<Engine>, reasons: Vec<String> }`。
- **优先级**：`用户项目覆盖 > 探测结果 > 全局默认`——即 `merge()` 前先探测结果作为一层"软默认"，用户一旦显式设置就永久胜出。
- **关键设计决策见 §5**。

### C 编译链推断（大幅缩减）

- **保留**：`-shell-escape` 探测（`minted`/`\write18`/`\directlua` 等信号）——但需先验证 latexmk 是否真的不加。
- **删除**：bib 工具、多次 pass、makeindex 的推断（§3.2 证明 latexmk 已接管）。

### D 缺包/文档类诊断 → 并入 P0-④

- 现有 log 里已经有 `! LaTeX Error: File \`xxx.cls' not found.`；
- 把它翻译成"缺 `xxx.cls`，模板包里可能自带，请确认打开的是含该文件的目录 / 或用 tlmgr 安装"——**这是 P0-④ 错误诊断的第一个模式**，不该在 P0-② 里重复造。

### E 中文字体探测（低优先）

- `ctex-fontset-windows` 依赖系统字体（SimSun/SimHei/KaiTi/FangSong）；精简版 Windows 可能缺。
- 属**环境探测**（一次性、与项目无关），与 P0-⑫（TinyTeX/环境）同族，建议一起排。

### 额外发现：**打开错目录层级**（`[推断]`，值得单列）

下载的模板 zip 常解压出 `模板名/` 一层，用户很容易打开**外层**目录。此时：
- `collect_tex_files` 递归扫描仍能找到 .tex → 根文件探测**能成功**，但 latexmk 的 `cwd` 与相对路径语义可能错位；
- 更常见的是外层目录里也有一个 README 级别的 .tex，产生**多候选**。

→ 检测"项目根下无 .tex，但唯一子目录里有"时提示**"是否打开 `子目录/`？"**，是低成本高命中的启发式。

## 5. 关键设计决策（建议落 ADR）

### 决策 1：探测结果**不落盘**（推荐）

落盘（写进 `.texpresso/settings.json`）有三个问题：
1. 该文件是**用户可手写、可进 git** 的覆盖文件，混入自动值会**污染版本库**、且用户无法区分"我设的"与"它猜的"；
2. 它会触发 watch → `is_self_write` 过滤链路（虽然能拦住，但每次打开项目都写盘是坏味道）；
3. **一旦落盘，"探测错了"就变成持久错误**。

→ **探测结果留在内存**（`ProjectState.detected`），UI 显示"已按文档内容自动选用 XeLaTeX（检测到 ctex）"，并提供**一键"设为项目设置"**（走既有 `update_settings`）——用户显式采纳才落盘。

### 决策 2：探测错了怎么办

- UI 必须**可见**（当前引擎 + 推断理由），否则用户无法判断该不该改；
- 设置页切引擎已有通路（`SettingsPanel.vue` 的 `setEngine`）——**逃生门已存在**，不需要新增；
- 探测**只影响未显式设置的项目**（决策 1 的优先级保证）。

### 决策 3：探测的输入范围

- v1 只读**根文件**内容（探测顺序：先定根文件，再据根文件推断引擎）——依赖关系单向、成本最低；
- 是否要跟进 `\input`/`\include` 的子文件：**待验证**（很多模板把 `ctex` 放在子文件里，但主文档通常是 `\documentclass{ctexart}` 直接体现）。

### 决策 4：放哪一层

- **推断函数在 core**（纯字符串 → `EngineHint`，全量可单测）；
- **读取内容、写状态在 src-tauri**（经 `FileSystem` trait，符合 ADR-0010 的"上层不碰 tokio::fs"）。

## 6. 风险与反证

| 风险 | 说明 | 缓解 |
|---|---|---|
| 启发式长尾 | 与 ADR-0009 同类：规则表会随模板千奇百怪而增长 | 规则集可单测 + 逃生门（设置页）+ **只推断高置信信号**（宁可不推断，也不要猜错） |
| **探测错比不探测更糟？** | 引擎选错的错误信息不可读（§3.1）→ 猜错会把"没反应"变成"一堆 ???" | ① 只对高置信信号推断（ctex/fontspec 这类几乎无歧义）；② 探测理由必须在 UI 可见；③ 保存全部候选/理由便于排查 |
| 与"默认 xelatex"重叠 | 产品默认已是 xelatex，中文模板多数已能开箱即用；真正会错的是**英文期刊模板用 pdflatex** 的场景 | 需先做**一手模板样本调研**（见 §9）才能确定优先级——当前证据不足以判断"默认 xelatex"能覆盖多少 |
| 契约变更面 | `ProjectInfo`/`ProjectState` 都动，涉及 bindings 重新导出、前端 store | 属于必要成本；`specta` 自动生成 bindings，改动可控 |

## 7. DoD 与验证方法（沿用路线图，补充可执行细节）

**DoD**：3 个真实模板**零配置**打开即可编译成功，且推断结果在 UI 可见、可覆盖、可一键采纳。

**验证方法**（按 AGENTS.md 闭环）：
1. 模板固化为 `test_file/projects/` 下的工程（gitignore，与 `multifile`/`中文测试工程` 同例）：
   - `ctexbook` 中文论文（需 xelatex）
   - IEEEtran（pdflatex 更常见）
   - `biblatex` 论文（biber，验证"不推断编译链"是对的）
2. 单测锁规则表：`infer_engine` 的每个信号 + "无信号 → None"；
3. 真机：`VITE_TEXPRESSO_PROJECT=<模板>` + `npm run tauri dev` → 观察 stdout（`打开项目` / 引擎取值）+ 真窗口出 PDF；
4. `npm run build` + `cargo test -p texpresso-core` 全绿。

## 8. 建议的增量切分（3 个可独立提交的单元）

| 单元 | 内容 | 依赖 | 规模 `[推断]` |
|---|---|---|---|
| **②-1** | 根文件候选：`ProjectInfo` 加字段 + 前端可见可交互（弹窗/状态栏） | 无 | 小（后端 1 字段 + 前端 1 处） |
| **②-2** | 引擎推断：core `infer_engine` + `ProjectState.detected` + 优先级 + UI 可见 | ②-1 的契约变更可复用 | 中 |
| **②-3** | 缺包诊断（并入 P0-④ 的第一个错误模式） | P0-④ | 小（作为 ④ 的一部分） |

**附带**：实测踩到并记入 [troubleshooting.md](../troubleshooting.md) 的一条——探针文档**含中文时必须用 xelatex/lualatex**，用 pdflatex 会 exit 12 且错误不可读（§3.1 同一现象）；§3.2 注另有**被证伪的 BOM 假设**，避免重走。

## 9. 未验证 / 待补（不要当成已知）

1. **shell-escape**：本机无 `minted`/`pygmentize`，未实测 latexmk 是否真的不加 `-shell-escape`；
2. **makeindex/glossaries**：未实测 latexmk 覆盖度；
3. **一手模板样本**：完全没有统计过"真实模板里各引擎/宏包占比"——**这是决定 ②-2 优先级的最大空白**；当前"默认 xelatex 已覆盖多数"只是 `[推断]`；
4. **缺字体场景**：未构造；
5. **探测输入是否需跟子文件**：未验证（§5 决策 3）。
