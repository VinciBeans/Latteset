# 0013 SyncTeX 改为进程内自解析（触发 ADR-0008 预留的替换路径）

- **状态**：已接受（2026-09-15）
- **触发**：ADR-0008 自己写明的备选方案 —— *"自研 Rust 解析器（否决：格式官方明言『不应视为公开』，漂移风险；**留作 CLI 出问题时的替换路径**）"*。本次正是触发该预留路径，**不是**推翻 ADR-0008：它的核心决定（core 定义 `SyncTexProvider` 接口 + 实现可替换）原样保留，被替换的只是**默认实现**。
- **决策者**：产品负责人（2026-09-15，"1. 使用 Rust 进行自解析"）

## 背景：CLI 实现与里程碑前提冲突

⑫ 里程碑（Tectonic 集成）的业务前提是**"干净 Windows 机器零预装可用"**：Tectonic 自带 bundle，不需要 TeX Live。但 SyncTeX 的双向定位走的是**系统 `synctex` 二进制**，而它由 **TeX Live** 分发：

- 本机实测：`Get-Command synctex` → `C:\texlive\2026\bin\windows\synctex.exe`；
- Tectonic 侧只有 `--synctex`（**生成** `.synctex.gz`），**没有**查询子命令（`tectonic --help` 中无 synctex 查询项）。

后果：在目标场景（只装 Tectonic）里，`.synctex.gz` 有、但**正反向定位全废**（Ctrl+点击高亮 / 点 PDF 跳源码），只报一句 `synctex 启动失败：<OS 错误>`。也就是说 roadmap 里"零预装可用**已兑现**"这句话，当时只对"编译出 PDF"成立。

## 决策

**默认实现换成进程内自解析**（`latteset_core::synctex::parse` + `latteset_infra::synctex::SyncTexSelf`），
系统 CLI 实现**保留**为 `LATTESET_SYNCTEX=cli`（A/B 复核用，不是长期形态）。

- 解析（纯逻辑）在 **core**：`SyncTexDoc::parse(&str)`、`forward_line`、`inverse_point`、`tag_for_file`；
- gzip 解压与文件读取在 **infra**（core 保持"无 IO 依赖"，ADR-0006）——用 `flate2` 的纯 Rust 后端，不引 C。

## 证据

**① 无 TeX Live 环境下的端到端**（把 PATH 里的 `texlive` 全剔掉，先断言 `synctex` 不可达）：

| 实现 | 正向 | 反向 |
|---|---|---|
| 自解析（默认） | `{"ok":true,"page":4,"x":70.87,"y":51.07}` | `intro.tex:3`（正是正向请求的那一行） |
| CLI（`LATTESET_SYNCTEX=cli`） | `同步失败：synctex 启动失败：program not found` | 同 |

**② 与 CLI 在真实工程上逐点对拍**（`node scripts/synctex-selfcheck.mjs`，三组工程 × 各 10–12 个样本，
与 `scripts/synctex-report.mjs` 同一批样本口径）：

| 工程 | 正向命中 | 同页 | 反向同文件 | 行差 ≤3 | 往返跳到位（CLI / 自解析） |
|---|---|---|---|---|---|
| `multifile` | 12/12 | 12/12 | 12/12 | 12/12 | **12/12 / 12/12** |
| `beamer工程` | 10/10 | 10/10 | 10/10 | 10/10 | **7/10 / 7/10** |
| `bench/large` | 12/12 | 12/12 | 12/12 | 12/12 | **12/12 / 12/12** |

⇒ 三组共 **31/34 往返跳到位，与 CLI 逐项相同**（与 `docs/modules.md` 的 34 点基线同源；batch 里点数按各工程可用样本计）。

## 已知偏差（如实登记，不是"差不多"）

1. **前向坐标在 beamer 类页面上与 CLI 有差**：`dy` 中位 132 pt、最大 147 pt（往返指标不受影响，仍 7/10）——
   样本行（如 `\frametitle` 在第 12 行）在该文件的记录里**没有对应行**（记录从第 19 行起），
   CLI 的节点选择迭代器与我们的"最接近记录行 + 优先 hbox"规则挑中了同一页但不同的盒子。
2. **`column` 恒为 -1**：`.synctex` 的盒子记录里没有列号（CLI 实测也基本恒为 -1）。
3. **ADR-0008 点名的漂移风险仍在**：格式官方未承诺公开。缓解 = 对拍脚本
   `scripts/synctex-selfcheck.mjs`（同一批样本、两侧各答一遍）+ 只解析稳定子集（头部 + 盒子记录）。

## 影响与回退

- **不变量**：`SyncTexProvider` 接口、`resolve_inverse` 的回落策略、`classify_inverse_target` 全部不动 ⇒ 上层零改动。
- **回退**：`LATTESET_SYNCTEX=cli` 即可切回系统二进制（若某天自解析在某类文档上出现回归）。
- **库形态（路径 B）的额外缺陷**（本次一并查实、**未修**）：库形态产出的 `.synctex.gz` 里 `Input:` 大多为空
  （实测 141 条只有 29 条非空，且非空的是 `.aux`），因为引擎经 `input_open_name_with_abspath` 询问真实路径时，
  那一层只对"项目根内找得到的文件"给得出路径。⇒ **库形态档的反向定位目前基本不可用**（解析器命中空 tag 时
  如实返回"没有对应源码"，不伪造）。已登记为待办。
