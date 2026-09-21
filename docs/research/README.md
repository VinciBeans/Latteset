# docs/research —— 研究文档状态表

> 作用：一眼看出**哪份是现状、哪份是历史**。四列 = 路径 / 状态 / 一句话结论 / 主要被谁引用。
> 状态口径：**现状**（在用的唯一真相源）｜**已落地**（结论已进产品）｜**已否决**（评估过、不做）｜**待办**（未开工）｜**存档**（原件移到 [docs/archive/](../archive/)，不再更新）｜**带日期证据**（当时实测/调研快照，引用时必须连日期与前提一起引）。
> 计数：表内 **29 行** = 原 26 份研究文档 + 本轮新增 3 份（本索引、[roadmap-item-specs.md](./roadmap-item-specs.md)、[tectonic-lib-evidence.md](./tectonic-lib-evidence.md)）。
> 阅读入口：[roadmap（唯一排序源）](./tex-ide-roadmap-priority.md) → 本文状态表 → 需要的专项报告。

| 路径 | 状态 | 一句话结论 | 主要被谁引用 |
|---|---|---|---|
| [tex-ide-pain-points.md](./tex-ide-pain-points.md) | 带日期证据 2026-08/09 | 三路专项调研总览：痛点排序 + 证据等级 A/B/C | roadmap 引言、三份专项 |
| [desktop-latex-editor-pain-points.md](./desktop-latex-editor-pain-points.md) | 带日期证据 2026-09 | 桌面编辑器（TeXstudio/TeXmaker 等）痛点量化 | 总览、roadmap §2 |
| [online-latex-editor-pain-points.md](./online-latex-editor-pain-points.md) | 带日期证据 2026-09 | 在线平台（Overleaf 等）痛点：协作/等待/离线 | 总览、roadmap §7 |
| [vscode-latex-workshop-pain-points.md](./vscode-latex-workshop-pain-points.md) | 带日期证据 2026-09 |  LaTeX Workshop：配置复杂、预览二等公民 | 总览、roadmap §2 |
| [cn-thesis-template-engines.md](./cn-thesis-template-engines.md) | 带日期证据 2026-09 | 19 所高校模板的官方引擎要求逐条取证 | template-corpus-survey |
| [template-corpus-survey.md](./template-corpus-survey.md) | 已落地（⑲） | 19 模板实测 **0 例**因默认引擎选错 ⇒ 砍 ②-2 | roadmap §1/§7、design.md:57 |
| [tex-ide-roadmap-priority.md](./tex-ide-roadmap-priority.md) | **现状（唯一排序源）** | §1 基线 + §3 待办总表 + §4 批次 + §5 外部方案评估 | 全仓（design/modules/README） |
| [roadmap-item-specs.md](./roadmap-item-specs.md) | **现状（2026-09 新建）** | roadmap §6 逐项详报（§6.1–§6.15，编号不变） | roadmap §9、各专项报告 |
| [tectonic-test-plan.md](./tectonic-test-plan.md) | **现状（门禁唯一真相源）** | 引擎/集成/分发矩阵 + 测量口径 + 上线门禁 | roadmap §6.5、tectonic-library-plan §7 |
| [tectonic-lib-evidence.md](./tectonic-lib-evidence.md) | **现状（2026-09 新建）** | 库形态实测收口（原 library-plan §6.1–§6.5 迁出） | `.gitignore:124`、library-plan §6/§13 |
| [p0-2-first-open-analysis.md](./p0-2-first-open-analysis.md) | 带日期分析（②-1 已落地） | 首次打开即用：②-1 落地、②-2 砍掉、④ 诊断落地 | roadmap §1、modules §8 |
| [p1-large-doc-editor-analysis.md](./p1-large-doc-editor-analysis.md) | 已落地（⑦a） | ⑦ 拆四份：⑦a 大纲增量落地、⑦b 缓做 | roadmap §6.2、modules §3.5 |
| [p1c-multifile-large-project.md](./p1c-multifile-large-project.md) | 已落地（⑦c） | 多文件大项目五项门槛全过 ⇒ **不需要优化** | roadmap §1、modules §12.2 |
| [dvi-preview-feasibility.md](./dvi-preview-feasibility.md) | 已否决（显示格式） | DVI/XDV 预览慢 57–68×；流式出图另立 ㉞ | roadmap §5.6/§7、modules #21 |
| [g2-byte-offset-resync.md](./g2-byte-offset-resync.md) | 索引吸收 / 渲染否决 | 半成品可读 + 页索引成本；不做 DVI 渲染 | roadmap §5.7、modules #19 |
| [g1-read-interception-feasibility.md](./g1-read-interception-feasibility.md) | 降级为依赖记录 | 字节级 I/O 拦截不做；改用 `.fls`/`.fdb_latexmk` | roadmap §9、modules #23/#24 |
| [stage2-streaming-feasibility.md](./stage2-streaming-feasibility.md) | 已落地 | 流式输出（页进度 + 编译中致命错误）已进产品 | roadmap §1、design §错误列表 |
| [stage3-incremental-output-parsing.md](./stage3-incremental-output-parsing.md) | 已落地（未接线） | 追加式页索引达成但无消费方；修掉 2 个静默错误 | modules #19、g2 §3 |
| [incremental-edit-x-dvi.md](./incremental-edit-x-dvi.md) | 已落地 | 页哈希差分 → 跳过重载 / 只重绘 / 跳过转换 | design §预览、modules §9.4 |
| [page-hash-prev-quant.md](./page-hash-prev-quant.md) | 带日期证据（只量化） | 债 #26：Σ假阳性 375 页/15 例；V1 精确率 100% | modules #26、roadmap §6.9 |
| [no-fork-alternatives.md](./no-fork-alternatives.md) | 已否决（fork 路线） | Windows 无 fork 等价物；导言区占 71–89.5% | roadmap §5.5/§9、troubleshooting |
| [modern-engines-zh.md](./modern-engines-zh.md) | 带日期证据 2026-09 | 中文 LuaLaTeX 慢 3.5×；Tectonic 引入依据 | roadmap §6.5、test-plan §1.4 |
| [realtime-preview-cost.md](./realtime-preview-cost.md) | 带日期证据 2026-09-14 | 单趟地板 / PDF 后端 / 草案层成本拆分 | library-plan §8、design §草案层 |
| [../archive/tectonic-integration-plan.md](../archive/tectonic-integration-plan.md) | **存档** | 首版集成路线；两处口径已作废（C-6/C-7） | test-plan §8/§9.2、modern-engines-zh |
| [../archive/tectonic-plan-parts/engine.md](../archive/tectonic-plan-parts/engine.md) | **存档** | 引擎层 E1–E7 逐条（56 条 + G1–G6） | test-plan §2.2/§9.1 |
| [../archive/tectonic-plan-parts/integration.md](../archive/tectonic-plan-parts/integration.md) | **存档** | 集成层 INT-10..96（Rev.10，88 条） | test-plan §2.3/§9.1 |
| [../archive/tectonic-plan-parts/perf.md](../archive/tectonic-plan-parts/perf.md) | **存档** | 测量层 R*/M*/H_*/U1–U12（Rev.4） | test-plan §3/§9.1 |
| [../archive/tectonic-plan-parts/dist.md](../archive/tectonic-plan-parts/dist.md) | **存档** | 分发/许可 DIST-*/MB-*/G-L*/F1–F8 | test-plan §2.4/§4/§9.1 |
| [README.md](./README.md)（本文件） | 索引（2026-09 新建） | 研究文档的路径/状态/结论/引用表 | docs/README.md:23 |

## 归档说明（2026-09）

> 四份分册原件（`engine/integration/perf/dist`）与 `tectonic-integration-plan.md` **已从 `docs/research/` 归档到 [docs/archive/](../archive/)**：分册内容已被 [tectonic-test-plan.md](./tectonic-test-plan.md) **§2–§4 收编**，且该文 **§9.2 已把它们列为「需修订」**；归档只为让现役文档与逐条证据分开，原件一字未改（见 [docs/archive/README.md](../archive/README.md)）。

## 已过期口径（引用前必看）

- **默认引擎已改 Tectonic**（2026-09-17，ADR-0014 修订 1）：凡写「默认 XeLaTeX」的文档/表格（test-plan §0.2/§7、perf.md、integration.md INT-19、design.md:57 的语料结论、CONTEXT.md）均已加状态批注，**引用时按批注口径读**。
- **Tectonic Quick 用 `-r 1` 两趟**（`-r 0` 单趟产物整段没有目录）——test-plan §1.2.2 与 engine.md E2.10 的 `-r 0` 只作引擎层事实保留。
- **SyncTeX 默认进程内自解析**（ADR-0013），CLI 仅 `LATTESET_SYNCTEX=cli` 备选。
