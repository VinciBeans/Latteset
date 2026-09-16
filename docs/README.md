# Latteset 文档索引

> 项目状态：**Windows 首发 MVP 已落地，迭代中**——项目/编辑/编译调度/错误诊断/连续 PDF 预览 + SyncTeX/设置页均可用，编译期已有**流式反馈**（状态栏「已排版 N 页」+ 错误列表在编译结束前就报致命错误），**界面有深浅两套主题（Candy Desk，含跟随系统）**，并已提供无 GUI 的 CLI + MCP 入口（Agent 可驱动「读 → 改 → 编译验证 → 修」）。
> **引擎形态**：XeLaTeX / LuaLaTeX / pdfLaTeX 之外还有 **Tectonic**，含**子进程**与**库内嵌**两种形态（设置面选）。库内嵌是**正式构建变体**（`npm run lib:*`，需 vcpkg），默认构建不含它以保证主产物零原生依赖 —— 见 [ADR-0012](./adr/0012-tectonic-library-form-engine.md) 与 [方案「实施现状」](./tectonic-library-plan.md)。
> 仓库：GitHub 为 truth、Gitee 为镜像；CI 跑 cargo test + `vue-tsc --noEmit` + 前端 vitest，另有按 tag/手动触发的**库形态发布构建**档。
> 产品入口与功能清单见[根 README](../README.md)；已完成 roadmap 项及其证据见 [roadmap §1 基线](./research/tex-ide-roadmap-priority.md)；实测数字与结论见 [design.md](./design.md)。
> e2e 以 **tauri server MCP 驱动真实窗口**为主（WebDriver 半配置、仅作备选），操作要点见 [troubleshooting.md](./troubleshooting.md)。

## 文档结构

| 文件 | 内容 |
|---|---|
| [根 README](../README.md) | 项目门面：安装、快速开始、特性、许可（仓库结构见 [AGENTS.md](../AGENTS.md)，设计与 Roadmap 见 [design.md](./design.md)） |
| [CONTEXT.md](../CONTEXT.md) | 术语表（ubiquitous language） |
| [design.md](./design.md) | 完整设计：产品定位、技术栈、编译子系统、MVP 边界、分发与质量底线；含延迟预算、基准脚本、预览重载、编辑期单趟的实测与结论 |
| [architecture.md](./architecture.md) | 分层设计：Rust/前端模块、层间接口契约、数据流、技术栈冻结、安全与工程基建 |
| [architecture-diagram.md](./architecture-diagram.md) | 架构图（Mermaid 源码 + [diagrams/](./diagrams/) SVG/PNG）：分层与依赖、编译触发链路、调度器语义、SyncTeX 双向定位 |
| [modules.md](./modules.md) | 模块详细设计：大模块拆分、函数签名与算法、通信契约、信息局部性；§12 是当前行为基线、已知债与验证入口 |
| [cli-mcp-plan.md](./cli-mcp-plan.md) | **已实现**：CLI + MCP 交互接口（headless 让 harness/Agent 不经 GUI 驱动「读 → 改 → 编译验证 → 修」）——用法与工具表、实测证据、与原计划的偏差、已知限制、DSH 接线示例 |
| [tectonic-library-plan.md](./tectonic-library-plan.md) | **实施中**：Tectonic 库形态（路径 B）集成方案——**开头有「实施现状」节**（按 §6 闸门 P0–P8 逐条：谁已落地、谁已判不成立、谁未开工、仍未测的是什么）；§3 落点与调用序列、§5 bundle/缓存口径、§6 分阶段闸门与复核命令（**§6.1–§6.4**：能真编出 PDF、实测缺陷与修法、release 性能对照、常驻判决）、§12 风险。**构建入口**：`npm run lib:check\|lib:dev\|lib:build\|lib:test`（`scripts/with-tectonic-lib.ps1` 是前置与环境的唯一落点） |
| [adr/](./adr/) | 决策记录（ADR），当前 13 项 |
| [troubleshooting.md](./troubleshooting.md) | 排障记录与真机验收清单（Windows 路径/工具链/日志解析/MCP 驱动实操） |
| [research/](./research/) | 市场调研：TeX IDE 核心痛点（[总览](./research/tex-ide-pain-points.md)、[桌面编辑器专项](./research/desktop-latex-editor-pain-points.md)、[在线平台专项](./research/online-latex-editor-pain-points.md)、[VS Code 专项](./research/vscode-latex-workshop-pain-points.md)）、[**Roadmap（优先级与批次）**](./research/tex-ide-roadmap-priority.md)、[P0-② 分析](./research/p0-2-first-open-analysis.md)、[**P1-⑦ 大文档编辑器侧性能实测**](./research/p1-large-doc-editor-analysis.md)、[**P1-⑦c 多文件大项目（夹具/口径/结论）**](./research/p1c-multifile-large-project.md)、[**DVI/XDV 预览可行性实测**](./research/dvi-preview-feasibility.md)、[**G2 字节偏移重同步深挖**](./research/g2-byte-offset-resync.md)、[**G1「拦截每一次 I/O」深挖**](./research/g1-read-interception-feasibility.md)、[**上游阶段 2（流式输出）可行性与落地**](./research/stage2-streaming-feasibility.md)、[**阶段 3（增量输出解析）深挖**](./research/stage3-incremental-output-parsing.md)、[**增量编辑 × DVI/XDV 五点判定**](./research/incremental-edit-x-dvi.md)、[**债 #26 量化（页哈希含 `bop.prev`：假阳性/假阴性、A/B 命中、重绘延迟、缓存迁移、改不改）**](./research/page-hash-prev-quant.md)、[**没有 `fork()` 时的替代路径**](./research/no-fork-alternatives.md)、[**比 XeLaTeX 更现代的中文引擎实测**](./research/modern-engines-zh.md)、[**Tectonic 集成落地方案**](./research/tectonic-integration-plan.md)、[**Tectonic 测试方案（引擎/集成/分发矩阵 + 测量口径 + 上线门禁；四份逐条明细见 [`tectonic-plan-parts/`](./research/tectonic-plan-parts/engine.md)）**](./research/tectonic-test-plan.md)、[**实时预览 + 正确编译的成本实测（单趟地板 / PDF 后端 / 草案层）**](./research/realtime-preview-cost.md)、[模板语料调研](./research/template-corpus-survey.md) 与 [中文高校模板引擎要求](./research/cn-thesis-template-engines.md)；用于产品取舍的外部事实依据 |
| [texpresso-live-rendering-roadmap.md](./texpresso-live-rendering-roadmap.md) | 外部方案研究：对上游 [let-def/TeXpresso](https://github.com/let-def/TeXpresso)（改造 XeTeX 的实时预览器）的源码通读与 8 阶段复刻路线；**本项目评估结论见 [roadmap](./research/tex-ide-roadmap-priority.md) §5（路线不采用；吸收清单与四条前提的现状见 §5.2/§5.4）** |

## 阅读顺序

新成员：`根 README → CONTEXT.md → design.md → architecture.md → architecture-diagram.md → modules.md → adr/0001-0013`

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
- [0010 基础设施层独立成 crate：latteset-infra](./adr/0010-infra-crate-boundary.md)
- [0011 项目更名：TeXPresso → Latteset](./adr/0011-rename-to-latteset.md)
- [0012 Tectonic 库形态引擎（路径 B + X-5 偏离 + 触发 ADR-0005 保留条款）](./adr/0012-tectonic-library-form-engine.md)
- [0013 SyncTeX 改为进程内自解析（触发 ADR-0008 预留的替换路径）](./adr/0013-synctex-in-process-parser.md)
