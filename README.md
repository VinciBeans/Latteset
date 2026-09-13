<div align="center">

# Latteset

**一杯拿铁的时间，实时编译出你的 LaTeX 成品**

打开含 `.tex` 的文件夹即可写作：编辑即编译，右侧 PDF 实时更新，并支持源码与 PDF 双向定位。

`Latteset` = **Latte + Typeset**。

</div>

## 界面预览

Latteset 主界面：左侧文件树、中间编辑区、右侧 PDF 预览。

<img src=".github/assets/Latteset运行主界面.png" alt="Latteset 主界面" width="800" />

## 安装

依赖 **Windows 10/11**、Node.js ≥ 18、Rust（stable-x86_64-pc-windows-msvc，含 VS Build Tools C++ 工具链）及 **TeX Live 或 MiKTeX**（提供 `latexmk`/`xelatex`，缺失时应用会提示安装）。

```bash
npm install && npm run tauri dev
```

首次启动会编译 Rust（约几分钟），随后自动打开应用窗口。

## 快速开始

1. 启动应用，点「打开项目」，选择一个含 `.tex` 的文件夹（仓库内示例：`test_file/projects/multifile/`，多文件 + 跨文件引用结构）。
2. 编辑 `main.tex` → 自动编译 → 右侧 PDF 实时更新（首次自动「适应宽度」）。
3. `Ctrl+点击` 源码 → PDF 对应位置高亮；点击 PDF → 跳回源码。
4. 点「编译」手动编译；「⚙ 设置」调整引擎、编译模式、超时等。

## 实时编译

- **双模式触发**：**连续编译**（默认，输入停顿约 500ms 即编译，无需保存）+ **保存触发编译**，随时切换；另有手动「编译」按钮。
- **编辑期快速出图**：编辑触发单趟草稿编译（实测比完整 latexmk 快约 40% 中位），停手 2s 自动补一次完整编译把目录/引用页码追上——期间状态栏提示「引用待更新」。
- **编译中就有反馈**：状态栏显示「已排版 N 页」并随引擎输出实时递增；错误列表在**编译还没结束**时就报出致命错误（`Overfull` 这类警告留到最终清单）——长文档不必干等到最后才知道出错。
- **调度与失败语义**：合并队列（待编译只保留最新一条）、超时自动终止（默认 120s、上限 1800s）、手动终止；超时与内容错误都**不自动重试**，超时进错误列表给证据化诊断 + 一键「提高到 Ns 并重试」。
- **错误列表**：解析 `.log`，把原始报错翻译成「原因 + 怎么改」（匹配不到就退回原文 + 行号）；同源去重/截断（错误雪崩不刷屏），点击跳转源码行。

> 说明：`latexmk`「增量」是对整份文档重跑一遍 xelatex 单遍（引擎特性），不跳过未改动文件；超大文档的单遍耗时可能超延迟预算，详见 [ADR-0005](./docs/adr/0005-latexmk-first-incremental-next.md)。

## 编辑与预览

- **编辑**：Monaco 编辑器 + 自研 LaTeX Monarch 语法高亮（Candy 配色），环境块折叠、多光标、代码片段（Tab 展开）、错误跳转。
- **预览**：pdf.js 内嵌连续分页（视口感知按需渲染），缩放（±/适应宽度/Ctrl+滚轮），高 DPI 渲染修复。
- **SyncTeX 双向定位**：`Ctrl+点击` 源码 → PDF 高亮；点击 PDF → 跳回源码。

## 项目与设置

- **文件夹即项目**：打开文件夹 → 文件树 → 多标签页；自动保存（防抖）+ 外部修改检测与冲突提示。
- **根文件**：正则启发式自动探测（含 `\documentclass` 的顶层 .tex），可手动覆盖。
- **设置**：引擎（XeLaTeX 默认，可切 LuaLaTeX/pdfLaTeX）、编译模式、防抖、超时、根文件覆盖；全局 + 项目（`.latteset/settings.json`）两层，改即生效。

## 命令行与 MCP（无 GUI，面向自动化）

GUI 之外，同一套核心也能被脚本与 AI Agent 直接驱动：`crates/latteset-server` 提供 `latteset-cli`（一条命令一个 JSON）与 `latteset-mcp`（MCP over stdio），可完成打开项目、编译并拿到结构化错误与诊断、读写文件、大纲、SyncTeX 双向定位。

```bash
cargo build -p latteset-server            # 产物在工作区 target/debug（未打进安装包）
latteset-cli --project <项目目录> compile # 退出码 0=编译通过，1=编译未通过
latteset-mcp --project <项目目录>         # 供 harness / Agent 拉起（stdio）
```

用法、工具清单与限制见 [docs/cli-mcp-plan.md](./docs/cli-mcp-plan.md)；注意同一项目**不要**同时用 GUI 和 CLI 编译（会抢 `tmp/`）。

> 开发与设计细节（构建、测试、Roadmap 等）见 [docs/](./docs/) 与 [CONTEXT.md](./CONTEXT.md)。

## 致谢与思路来源

**Latteset 的若干设计思路，学习自 [TeXPresso](https://github.com/let-def/texpresso) 项目**（作者 [let-def](https://github.com/let-def)，OCaml 实现，改造 XeTeX 做 LaTeX live rendering）——它把"边改边出图"从概念做到了工程级，是本项目立项时最重要的参考对象。

**已吸收并落地**（均为独立实现，未复用其源码）：

- **空闲收敛**：编辑期跑轻的、停手后补一次全的 → 本项目的 Quick 单趟 + 2s 空闲收敛；
- **把决策过程打出来**：调度/收敛决策留日志，排障时可视化；
- **确定性验证**：连跑两次逐字节比对，作为一切输出优化的前提；
- **引擎输出管道化**：把引擎输出从丢弃改为管道化，换来编译期进度与实时错误 → 状态栏「已排版 N 页」+ 错误列表在编译结束前就报致命错误。

**评估后未采用**（其路线要求拥有引擎内部，与 ADR-0007 冲突）：fork 快照 / seen 水位、VFS 事务回滚、以 DVI/XDV 代替 PDF 作为预览格式。

完整的源码通读与逐阶段复刻路线见 [docs/texpresso-live-rendering-roadmap.md](./docs/texpresso-live-rendering-roadmap.md)，本项目的取舍与吸收清单见 [docs/research/tex-ide-roadmap-priority.md](./docs/research/tex-ide-roadmap-priority.md) §5。

> **更名的起因也是一次善意的提醒**：GitHub 用户 [@mathlab08](https://github.com/mathlab08) 指出了本项目与上游 TeXPresso 的重名问题。**特此感谢**——正因这条提醒才有了 TeXPresso → Latteset 的更名（见 [ADR-0011](./docs/adr/0011-rename-to-latteset.md)）。同名是误会，致敬是本意。

## 许可

[MIT](./LICENSE) © 2026 WenqiBian
