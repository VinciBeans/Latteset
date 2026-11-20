# Latteset 文档导航

Latteset 的全部文档从这里进入。当前是 **Windows 首发 MVP 已落地、迭代中**：项目 / 编辑 / 编译调度 / 错误诊断 / 连续 PDF 预览 + SyncTeX / 设置页均可用，另有深浅两套主题与无 GUI 的 CLI + MCP 入口。

## 三条阅读路径

- **使用者**（装、用、遇坑）：[根 README](../README.md) → [troubleshooting.md](./troubleshooting.md) → [cli.md](./cli.md)
- **开发者**（改代码、加功能）：[AGENTS.md](../AGENTS.md) → [dev/README.md](./dev/README.md) → [dev/modules.md](./dev/modules.md) / [design.md](./design.md) → [adr/](./adr/)
- **决策与历史**（为什么这么做、当时怎么试的）：[adr/](./adr/) → [archive/README.md](./archive/README.md)

## 现役文档一览

| 文档 | 给谁看 | 一句话职责 |
|---|---|---|
| [根 README](../README.md) | 使用者 / 新访客 | 产品门面：安装、快速开始、功能与限制、许可 |
| [design.md](./design.md) | 开发者 / 评审 | 完整设计：产品定位、技术栈、编译子系统、MVP 边界、分发与质量底线，含实测数字 |
| [roadmap.md](./roadmap.md) | 开发者 / 产品 | 产品路线与优先级的**唯一排序源**；全仓引用的编号含义以此为准 |
| [cli.md](./cli.md) | 自动化 / Agent | `latteset-cli` 与 `latteset-mcp` 的用法、工具清单与限制 |
| [troubleshooting.md](./troubleshooting.md) | 使用者 / 测试 | 排障：现象 → 原因 → 处理（环境坑与工具链问题） |
| [glossary.md](./glossary.md) | 所有人 | 术语表（ubiquitous language）：概念边界与 `_Avoid_` 禁用词 |
| [logo.md](./logo.md) | 设计 / 分发 | 品牌标识：概念、色板、文件清单与使用规则 |
| [dev/README.md](./dev/README.md) | 开发者 | 开发者导航：环境、构建、验证入口与代码结构 |
| [dev/modules.md](./dev/modules.md) | 开发者 | 模块实现契约：拆分、接口、通信契约、已知债与验证入口 |
| [dev/testing.md](./dev/testing.md) | 开发者 | 测试与验证：各层测试、真机验收、基准与确定性脚本 |
| [dev/tectonic-library.md](./dev/tectonic-library.md) | 开发者 | 内嵌 Tectonic（唯一形态）：构建入口、装配点、能力面与验证入口 |
| [dev/tectonic-licensing.md](./dev/tectonic-licensing.md) | 开发者 / 分发 | 分发许可审计：Rust/C 依赖与宏包集的许可证、义务清单、G-L1..G-L9 门禁状态 |
| [adr/](./adr/) | 开发者 | 决策记录 16 项：每个关键取舍的背景、决定与后果 |
| [archive/](./archive/) | 追溯证据的人 | 归档：原文保留的旧方案与调研证据（[索引](./archive/README.md)、[调研证据状态表](./archive/research/README.md)） |

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
- [0014 以 Tectonic 为准（Tectonic-first）：新功能的设计与验收基准](./adr/0014-tectonic-first.md)
- [0015 Tectonic 唯一引擎：删除其它引擎与 latexmk 路径，只保留内嵌形态](./adr/0015-tectonic-only-embedded-engine.md)（**②段已执行，记录见文末**）
- [0016 随包分发内置宏包集（离线开箱）](./adr/0016-ship-bundle-offline.md)
