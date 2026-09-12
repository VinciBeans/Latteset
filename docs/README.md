# TeXPresso 文档索引

> 项目状态：**已实现并迭代中**（Windows 首发 MVP 落地：项目/编辑/编译调度/错误去重/连续 PDF 预览+SyncTeX/设置页均可用）。2026-08 演进：GitHub 为 truth + Gitee 镜像同步、GitHub Actions CI（cargo test + `vue-tsc` + 前端 vitest）、tauri server MCP 验证基建、预览重载 A/B 优化（分页虚拟化 + 同文件 canvas 复用）、文件树增量刷新、SyncTeX 契约定稿、前端 vitest 单测、编辑器空文件占位修复、多文件测试工程（`test_file/projects/multifile/`）。2026-09 演进：中文路径/编码实测复核（①）、根文件候选可见可交互（②-1）、错误诊断升级（④，19 类「原因 + 怎么改」）、性能基准与回归基建（③）、**编辑期单趟编译 + 空闲收敛（㉘：编辑走 Quick 直调引擎省 40% 中位，停手 2s 后 Full 收敛，状态栏提示「引用待更新」）**。增量编译结论（**暂不过 latexmk**，但 ㉘ 用「单趟草稿 + 收敛」绕开了它的外层开销）见 [ADR-0005](./adr/0005-latexmk-first-incremental-next.md) 与 [design.md](./design.md) §延迟预算实测附节；e2e 以 **tauri server MCP 驱动真实窗口**为主（WebDriver 半配置、仅作备选），操作要点见 [troubleshooting.md](./troubleshooting.md)。**CLI + MCP 交互接口**为计划任务（仅记录，待实现，见 [cli-mcp-plan.md](./cli-mcp-plan.md)）。
> 产品入口与功能清单见 [根 README](../README.md)；设计权威仍是本目录。

## 文档结构

| 文件 | 内容 |
|---|---|
| [根 README](../README.md) | 项目门面：安装、快速开始、特性、许可（仓库结构见 [AGENTS.md](../AGENTS.md)，设计与 Roadmap 见 [design.md](./design.md)） |
| [CONTEXT.md](../CONTEXT.md) | 术语表（ubiquitous language） |
| [design.md](./design.md) | 完整设计：产品定位、技术栈、编译子系统、MVP 边界、分发与质量底线（含后置/未决清单） |
| [architecture.md](./architecture.md) | 分层设计：Rust/前端模块、层间接口契约、数据流、技术栈冻结、安全与工程基建 |
| [architecture-diagram.md](./architecture-diagram.md) | 架构图（Mermaid 源码 + [diagrams/](./diagrams/) SVG/PNG）：分层与依赖、编译触发链路、调度器语义、SyncTeX 双向定位 |
| [modules.md](./modules.md) | 模块详细设计：大模块拆分、函数签名与算法、通信契约、信息局部性 |
| [cli-mcp-plan.md](./cli-mcp-plan.md) | 计划任务：CLI + MCP 交互接口（现状盘点、工具清单、P0/P1 优化点与落地顺序；**仅记录，待实现**） |
| [adr/](./adr/) | 决策记录（ADR），当前 10 项 |
| [troubleshooting.md](./troubleshooting.md) | 排障记录（Windows 路径/工具链/日志解析场景） |
| [research/](./research/) | 市场调研：TeX IDE 核心痛点（[总览](./research/tex-ide-pain-points.md)、[桌面编辑器专项](./research/desktop-latex-editor-pain-points.md)、[在线平台专项](./research/online-latex-editor-pain-points.md)、[VS Code 专项](./research/vscode-latex-workshop-pain-points.md)）、[**Roadmap（第 3 版 · 重构）**](./research/tex-ide-roadmap-priority.md)、[P0-② 分析](./research/p0-2-first-open-analysis.md)、[模板语料调研](./research/template-corpus-survey.md) 与 [中文高校模板引擎要求](./research/cn-thesis-template-engines.md)；用于产品取舍的外部事实依据 |
| [texpresso-live-rendering-roadmap.md](./texpresso-live-rendering-roadmap.md) | 外部方案研究：对上游 [let-def/texpresso](https://github.com/let-def/texpresso)（改造 XeTeX 的实时预览器）的源码通读与 8 阶段复刻路线；**本项目评估结论见 [roadmap](./research/tex-ide-roadmap-priority.md) §5（不采用，吸收 3 条）** |

## 阅读顺序

新成员：`根 README → CONTEXT.md → design.md → architecture.md → architecture-diagram.md → modules.md → adr/0001-0009`

## 决策记录索引

- [0001 合并队列调度 + 延迟预算](./adr/0001-compile-scheduling-and-latency-budget.md)
- [0002 Tauri 2 + Vue 3 技术栈](./adr/0002-tauri-vue-tech-stack.md)
- [0003 Windows 首发 + 不签名分发策略](./adr/0003-windows-first-unsigned-distribution.md)
- [0004 MVP 边界](./adr/0004-mvp-scope.md)
- [0005 latexmk 起步，增量编译为首要后续任务](./adr/0005-latexmk-first-incremental-next.md)
- [0006 workspace 拆分：core 与 src-tauri](./adr/0006-workspace-split-core-and-tauri.md)
- [0007 文件系统为内容真相源](./adr/0007-filesystem-as-source-of-truth.md)
- [0008 SyncTeX 走 CLI + 接口抽象](./adr/0008-synctex-via-cli-with-interface.md)
- [0009 根文件探测正则启发式](./adr/0009-regex-heuristic-root-detection.md)
- [0010 基础设施层独立成 crate：texpresso-infra](./adr/0010-infra-crate-boundary.md)
