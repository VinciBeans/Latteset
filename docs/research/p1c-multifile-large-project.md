# ⑦c 多文件大项目：夹具、口径与结论（编辑器件）

> 背景与拆项依据：[p1-large-doc-editor-analysis.md](./p1-large-doc-editor-analysis.md)（⑦ 的四拆分与 DoD 建议）。
> 配套：`scripts/gen-large-project.mjs`（夹具，三档）、`scripts/editor-report.mjs`（口径脚本，一条命令跑真机并判门槛）。
> **结论一句话**：三档夹具（9 / 21 / 41 文件 × 2.2–5.5 MB）跑完五项门槛**全部通过**——每击键净开销 **0.077–0.097 ms 且绝对值恒定**（与文件数、总量都无关），打开全部标签约 **14–27 ms/文件**（线性），关掉后 model 与缓冲**完全释放**。据此 **⑦c 的出口条件满足：不投入编辑器侧优化**；只留两条低优先观察（冷路径大纲全量 57–182 ms；打开标签的线性成本）。

## 0. TL;DR

| 问题（调研 #4410 的症状：20 文件 / 近 7000 页"卡"） | 实测答案 |
|---|---|
| 每击键成本随文件数/总量增长吗 | ❌ **不增长**：0.077–0.097 ms 绝对值恒定（9→41 文件、2.2→5.5 MB 全在同一区间） |
| 打开几十个标签会拖垮吗 | 线性 ~14–27 ms/文件：21 文件 507–536 ms、41 文件 675 ms；真实工作流同时只开几个 |
| 长会话会漏内存吗 | ❌ 不漏：21 个标签全关后 model 25→4（回基线）、缓冲 0、堆 +0.35 MB |
| 大纲往返（⑦a 之后）达标吗 | ✅ 5–6 ms/MB（门槛 20）：编辑 1 文件 14.1–28.2 ms、无变化 7.1–25.8 ms |
| 有无 >50 ms 长任务 | 无。同步击键与全部探针期间 `longtask` 观测为空 |
| 需要优化吗 | **不需要**。"竞品的架构病我们没有"这一结论在多文件维度上同样成立 |

## 1. ⑦c 这一步要回答什么

[roadmap §6.2](../research/tex-ide-roadmap-priority.md) 定的 ⑦c 第一步是**只做夹具与口径**，出口条件是"**再据数决定是否优化**"。要补的未知项（[p1 分析](./p1-large-doc-editor-analysis.md) §5.1）：

1. 打开全部标签的内存与耗时；
2. include 图解析（>20 文件时）；
3. 文件树规模；
4. "编译中被 watch 事件淹没"（**本文未覆盖**，见 §8）。

口径沿用 p1 §2：真机 + `performance` 探针、**不改产品源码**；A/B 顺序受控交替并丢弃首轮；击键走 `editor.trigger('keyboard','type')`。门槛沿用 p1 §7 的建议值。

## 2. 夹具：三档，刻意分离"文件数"与"总量"

`node scripts/gen-large-project.mjs` → `test_file/projects/editor/<tier>`（不入库；生成器入库，内容由确定性 LCG 生成、可再生成）：

| 档 | 文件数 | 总量 | 单章大小 | 设计意图 |
|---|---|---|---|---|
| `multi8` | 9 | 2.2 MB | ~288 KB | 文件数少、单文件大（与 p1 的单文件 1.36 MB 档可比） |
| `multi20` | 21 | 5.5 MB | ~288 KB | **主档**：文件数 = 调研症状（20 文件） |
| `multi40` | 41 | 5.5 MB | ~144 KB | 与 `multi20` **总量相同、文件数翻倍** → 分离"文件数"变量 |

结构是 ctexbook + `\include{chapters/chNN}` + `refs.bib`（可被 xelatex 编译）。章节用 `\include` 而不是 `\input`：与 `bench/multifile` 同构，顺带覆盖 include 图解析。生成器刻意**不用** `\include{子目录/文件}` 之外的坑（见 [troubleshooting](../troubleshooting.md)：子目录 + `-outdir` 会另开一条失败路径）。

## 3. 口径固化：`scripts/editor-report.mjs`

一条命令跑完并判门槛（**零第三方依赖**）：

```bash
VITE_TEXPRESSO_PROJECT=<...>/editor/multi20 npm run tauri dev
node scripts/editor-report.mjs --tier multi20 --json out.json     # 退出码 1 = 有门槛未过
```

**它怎么自己驱动真机**：MCP Bridge 插件在应用侧监听 `ws://0.0.0.0:9223`，协议是
`{id, command:'execute_js', args:{script, windowLabel}}` → `{id, success, data}`
（读自 `@hypothesi/tauri-mcp-server/dist/driver/webview-executor.js`）。Node 22+ 自带 WHATWG `WebSocket`，
所以脚本**不需要 npm 依赖、也不需要 MCP 会话在场**（这一点比 p1 的"会话内手动注入"更进一步）。
脚本还能用 `--eval-file` 当调试入口（排"模块 URL/字段名变了"这类问题）。

七个探针（都不改产品源码，靠 `performance.getEntriesByType('resource')` 找模块 URL 后 `await import`）：

| 探针 | 测什么 |
|---|---|
| P0 切换/确认项目 | 会话内自动切到目标档（一个窗口可连跑三档） |
| P1 环境 | 文件数、树节点、根文件、已打开标签 |
| P2 打开全部 `.tex` 标签 | 读盘 + store（`openFile`）+ 等 EditorPane 把 model 建完 = **端到端** |
| P3 每击键 A/B | 真实编辑器实例 vs 同内容裸 Monaco，**5 轮交替弃首轮** |
| P4 折叠提供者 | `latexFoldingProvider.provideFoldingRanges(model)` × 5 取中位 |
| P5 大纲往返 | 真实 IPC 三条路径：全量 / 编辑 1 文件 / 无变化 |
| P7 关闭全部标签 | model 数与缓冲是否释放（长会话泄漏） |
| P6 20 个 1MB model | 纯建模成本（门槛原意，不含读盘） |

## 4. 实测数据

环境：Windows / 24 硬件线程 / WebView2 / `devicePixelRatio=1.5` / dev 模式真实窗口。

**门槛判定（最后一次运行）**：

| 项 | multi8 | multi20 | multi40 | 门槛 |
|---|---|---|---|---|
| 每击键净开销（绝对值） | 0.078 ms ✅ | 0.077–0.097 ms ✅ | 0.085 ms ✅ | ≤ 0.5 ms |
| 最长 longtask | 0 ✅ | 0 ✅ | 0 ✅ | ≤ 50 ms |
| 折叠重算（按活动文件归一） | 0.73 ms/MB ✅ | 1.09 ms/MB ✅ | 1.45 ms/MB ✅ | ≤ 2 ms/MB |
| 大纲往返：编辑 1 文件 | 6.41 ms/MB（14.1 ms）✅ | 5.09 ms/MB（28 ms）✅ | 5.07 ms/MB（27.9 ms）✅ | ≤ 20 ms/MB |
| 20 个 1MB model 创建 | 22.3 ms ✅ | 30.3–45.9 ms ✅ | 34.9 ms ✅ | ≤ 100 ms |

**明细**：

| 指标 | multi8（9 文件 / 2.2 MB） | multi20（21 文件 / 5.5 MB） | multi40（41 文件 / 5.5 MB） |
|---|---|---|---|
| 打开全部标签（端到端） | 252 ms（27.2 ms/文件） | 507–536 ms（24–26 ms/文件） | 675 ms（14.3 ms/文件） |
| 文件树节点 | 11 | 23 | 43 |
| 大纲：编辑 1 文件 | 14.1 ms | 28.0–28.2 ms | 27.9 ms |
| 大纲：无变化 | 7.1 ms | 21.1–22.2 ms | 25.8 ms |
| 大纲：全量（冷路径） | 57.5 ms | 139–181.8 ms | 153.5 ms |
| 折叠（活动文件 282/144 KB） | 0.2 ms | 0.3 ms | 0.2 ms |
| 关闭全部标签 | — | 21→0 标签、缓冲 0、model 25→4、堆 +0.35 MB | — |

**逐条读法**：

- **每击键**：A（真实编辑器含我们的 `onDidChangeModelContent` → `getValue()` + `markDirty`）与 B（裸 Monaco）的差，5 轮交替后非常稳定（A 各轮 0.11–0.16 ms、B 0.05–0.07 ms）。绝对值在 0.077–0.097 ms 区间**与规模无关**——9 文件 2.2 MB 与 41 文件 5.5 MB 同值。这与 p1 的单文件结论（0.02–0.05 ms）一致（略高是因为本轮在 dev + 21–41 个 model 常驻的环境下测）。
- **大纲**：成本随**总量**近线性（2.2 MB → 14.1 ms；5.5 MB → 28 ms），**不随文件数**（同为 5.5 MB 的 21 与 41 文件：28.0 vs 27.9 ms）——⑦a 的"内容指纹复用"让多文件几乎不额外收费。
- **冷路径**（全量提交 + 全量解析）57–182 ms，只在项目打开 / 根文件切换发生一次，属"打开体验"而不是编辑循环。
- **内存**：文本量口径可靠（缓冲 5.5 MB、model 数 = 文件数 + 基线 4），**堆读数不可靠**（见 §6.5）。

## 5. 结论：据数决定**不优化**

| 候选优化 | 数据 | 决定 |
|---|---|---|
| 每击键路径（⑦d 的 `getValue` 去重） | 净 0.077–0.097 ms，门槛 0.5 ms | **不做**（维持 p1 的 ⑦d 否决） |
| 折叠提供者增量（⑦b） | 0.2–0.3 ms @282 KB；按行数外推 @1 MB ≈ 1.1 ms < 2 ms 门槛 | **维持缓做**（多文件不改变这个判断） |
| 打开标签耗时/内存 | 线性 14–27 ms/文件；关掉后完全释放 | **不做**（真实工作流同时只开几个；无一屏 40 标签的用法） |
| 大纲往返 | 5–6 ms/MB，门槛 20 | **不做**（⑦a 已解决） |
| 冷路径大纲全量 57–182 ms | 一次性、在"打开项目"里 | **不开新优化**；并入 ③ 的"打开耗时"观测口径一起看 |

⇒ ⑦c 的出口条件（"据数决定是否优化"）**已满足**：这一维度上没有需要投入的优化，剩余项都归到既有条目（③ 打开耗时 / ⑦b 缓做）。

## 6. 口径自身的七个坑（回写以免重犯）

1. **Vite 预打包依赖没有源码路径**：Monaco 在 dev 下是 `/node_modules/.vite/deps/monaco-editor.js?v=…`，needle 写成 `monaco-editor/esm/vs/editor/editor.api.js` 会匹配到 `.css`/`worker`，导入后 `monaco.editor` 为 `undefined`。
2. **只能用它加载时那个 URL**：Monaco 的 model 注册表是模块级单例；给 URL 换 query 会拿到新实例、看不见已有 model。
3. **活动文件必须是"大文件"**：打开全部标签后停在的是**排序最后**的文件（本次是几百字节的 `main.tex`），在它上面测每击键/折叠会得到荒唐数字（实测 0.43 ms@**1 KB**）。口径里先切到 fixture 的 `editTarget`。
4. **`.length` 不是字节数**：JS 字符串是 UTF-16 码元，中文下比 UTF-8 小约 3 倍——第一次跑把 5.5 MB 文档记成 1.87 MB，门槛归一跟着错。规模口径一律 `new Blob([s]).size`。
5. **堆读数在 dev 下不能当结论**：`performance.memory` 抖动极大（实测出现"打开 41 个标签后堆反而 **−17 MB**"）。可用的替代是**文本量**（缓冲/model 字节数）与"关闭后是否回落"。
6. **`editor.dispose()` 不 dispose model**：裸 Monaco 对照里若写成 `create(host,{model:createModel(...)})`，那个 model 会泄漏并污染后面的 model 计数（实测多出 3 个）。
7. **探针体是 `String.raw` 模板**：里面的注释/字符串**不能出现反引号**，否则模板提前终止（`SyntaxError: Unexpected identifier`）。

另有一条**门槛形式的修正**：p1 §7 把每击键门槛写成"≤0.5 ms/**MB**"——实测证明该成本**不随规模增长**，按 MB 归一只会在小文件档虚高（141 KB 档把 0.08 ms 判成"超 0.5 ms/MB"）。门槛应按**绝对毫秒**（0.5 ms@1 MB 的原意），本文与脚本已按此判定。

## 7. 复现方法

```bash
node scripts/gen-large-project.mjs                 # 生成三档夹具（幂等，--force 重建）
VITE_TEXPRESSO_PROJECT=<repo>/test_file/projects/editor/multi20 npm run tauri dev
node scripts/editor-report.mjs --tier multi20 --json out.json
node scripts/editor-report.mjs --tier multi8      # 一个窗口可连跑三档（脚本会自己切项目）
node scripts/editor-report.mjs --eval-file <path> # 调试入口：把文件当探针跑，打印返回值
```

## 8. 未验证与局限

1. **"编译中被 watch 事件淹没"未测**（⑦c 的第 4 个未知项）：夹具是 5.5 MB 中文 ctexbook，编译一次可能数十秒到超时，属 ③ / 流式输出的账，不在本文口径内。
2. **未测 `Ctrl+F` / 大范围替换**（p1 §5.2 的遗留项），也未测"多光标 + 大文件"。
3. **堆内存没有可信读数**（§6.5）：只有文本量代理与"关闭后是否回落"。要精确需要 CDP 的 `Runtime.getHeapUsage` 或 `--enable-precise-memory-info`。
4. **只测了 dev 模式**：生产构建（无 source map、模块合并）下这些数字只会更好，但**未实测**。
5. **主观手感未评分**（p1 同理）：本文全是客观代理（耗时/longtask/帧）。
6. **`multi40` 的单章只有 144 KB**：与 `multi20` 同总量但单文件更小，所以它的"折叠/每击键归一值"天然偏高——读表时看绝对值，不要横向比归一值。
