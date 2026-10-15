# docs/archive —— 已归档文档（原文保留，不再更新）

> 归档纪律：**移动不删改** —— 原件逐字保留（只修指向本目录的链接与位置说明），状态批注写在索引与现役文档里。
> 状态表见 [docs/archive/research/README.md](research/README.md)（「存档」行）；归档原因与现役替代见下表。

| 文件 | 原位置 | 归档（月） | 为什么归档 | 现役替代 / 证据在哪 |
|---|---|---|---|---|
| [architecture.md](./architecture.md) | `docs/` | 2026-09 | 早期架构叙述 | 已并入 [../design.md](../design.md) |
| [architecture-diagram.md](./architecture-diagram.md) | `docs/` | 2026-09 | 架构图（Mermaid 源码 + diagrams/ 渲染产物） | 已并入 [../design.md](../design.md)；渲染入口 `scripts/render-diagrams.mjs` |
| [cli-mcp-plan.md](./cli-mcp-plan.md) | `docs/` | 2026-09 | headless 入口计划与落地偏差（§4 偏差、§6 编号对照属过程史） | 使用文档已转正为 [../cli.md](../cli.md)；契约见 [../dev/modules.md](../dev/modules.md) §8.1 |
| [tectonic-library-plan.md](./tectonic-library-plan.md) | `docs/` | 2026-09 | 库形态接入逐条计划 | 现状提炼为 [../dev/tectonic-library.md](../dev/tectonic-library.md) |
| [texpresso-live-rendering-roadmap.md](./texpresso-live-rendering-roadmap.md) | `docs/` | 2026-09 | 外部方案（上游 texpresso 实时渲染）研究 | 该路线不采用；结论见 [../roadmap.md](../roadmap.md) |
| [troubleshooting-history.md](./troubleshooting-history.md) | `docs/troubleshooting.md` | 2026-10 | 已修问题的定位过程，对现役无指导意义 | 现役排障见 [../troubleshooting.md](../troubleshooting.md)；测试见 [../dev/testing.md](../dev/testing.md) |
| [research/](./research/) | `docs/research/` | 2026-09 | 24 份调研与证据（引擎实测、可行性与量化报告） | 逐份状态表见 [research/README.md](research/README.md)；结论已提炼进现役文档 |
| [tectonic-integration-plan.md](./tectonic-integration-plan.md) | `docs/research/` | 2026-09 | Tectonic 接入首版路线与摩擦点记录；§1 路线②/§5-2 的 `-b <本地 tar>` 与 `-C` 语义已被实测作废（C-6/C-7） | [tectonic-test-plan.md](research/tectonic-test-plan.md) §8 裁决台账 + [tectonic-library-plan.md](tectonic-library-plan.md) |
| [tectonic-plan-parts/engine.md](./tectonic-plan-parts/engine.md) | `docs/research/tectonic-plan-parts/` | 2026-09 | 引擎层逐条明细（E1–E7 / 56 条 + G1–G6） | test-plan §2.2（已按实测改写）；test-plan §9.2 已列「需修订」章节 |
| [tectonic-plan-parts/integration.md](./tectonic-plan-parts/integration.md) | 同上 | 2026-09 | 集成层 INT-10..INT-96（Rev.10，88 条） | test-plan §2.3；test-plan §9.2 |
| [tectonic-plan-parts/perf.md](./tectonic-plan-parts/perf.md) | 同上 | 2026-09 | 测量层 R*/M*/H_*/U1–U12（Rev.4） | test-plan §3；test-plan §9.2 |
| [tectonic-plan-parts/dist.md](./tectonic-plan-parts/dist.md) | 同上 | 2026-09 | 分发与许可 DIST-*/MB-*/G-L*/F1–F8 | test-plan §2.4/§4；test-plan §9.2 |

> **结果性证据不在这里**：库形态的实测收口（§6.1–§6.5）在 [docs/archive/research/tectonic-lib-evidence.md](research/tectonic-lib-evidence.md)。
