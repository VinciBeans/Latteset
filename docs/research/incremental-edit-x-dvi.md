# 增量编辑 × DVI/XDV：五个组合点的判定

> 与既有报告的关系：[dvi-preview-feasibility.md](./dvi-preview-feasibility.md)（XDV **渲染**已否决）、[g2-byte-offset-resync.md](./g2-byte-offset-resync.md)（页索引/页哈希差分）、[stage3-incremental-output-parsing.md](./stage3-incremental-output-parsing.md)（追加式解析：达成但无消费方）、roadmap ⑪（预览状态保持）。
> **结论一句话**：五个组合点里**只有一个是"成立且值得做"的**——用 XDV **页哈希差分**决定"能不能复用旧页 / 要不要刷新预览"（实测：只加一行注释 → 125 页**逐页字节完全一致**，于是这次编译的转换与预览刷新都可跳过）。**局部重编译 + 页级拼接被实测证伪**（`\includeonly` 让章节号/页码/目录全部错乱：74 页 → 33 页、"第 3 章"显示成"第一章"）。剩下两个（XDV 半成品渲染、编译期页信息）分别由 DVI 报告与阶段 3 否决。

## 0. TL;DR

| 组合点 | 判定 | 依据 |
|---|---|---|
| ① **页哈希差分 → 复用旧 PDF / 跳过预览重载 / 只重绘变化页** | ✅ **成立且值得做**（⑪ 的机制输入） | E1 实测 0 页变；G2 + 阶段 3 的差分已有 SyncTeX 对拍 |
| ② 局部重编译（`\includeonly`）**+ 页级拼接** | ❌ **证伪** | E2 实测：74 → 33 页、章节号与页码错乱、目录重写 |
| ③ XDV **半成品渲染**（边编边出图） | ❌ 已否决 | [DVI 报告](./dvi-preview-feasibility.md)：比 PDF 链路慢 **57–68×** |
| ④ 编译期页信息（进度/页尺寸） | ➖ 无消费方 | [阶段 3 报告](./stage3-incremental-output-parsing.md) §8：页进度已由 `[N]` 标记满足 |
| ⑤ 追加式解析器（阶段 3 本体） | ➖ 达成但不接线 | 同上：它的消费方是④，而④没有消费方 |

## 1. 先说清"增量编辑"在我们这儿指什么

它有三个层次，混着谈容易得出错误结论：

| 层次 | 现状 | 说明 |
|---|---|---|
| **触发强度**（改写多少就重排多少） | ✅ 已有：Quick 单趟（编辑期）/ Full 收敛（首编、手动、空闲 2s） | ㉘ 落地；实测比全量 latexmk 快 40% 中位 |
| **编辑器侧增量** | ✅ 部分：⑦a 大纲只重扫变化的文件（55.5→9.7ms）、`lastSent` 差分、错误/预览的事件分离 | ⑦c 已证明"多文件大项目"在编辑期无病 |
| **编译范围增量**（只重排改动的那部分文档） | ❌ **没有**，且本轮 E2 说明它在 LaTeX 语义下不成立 | 见 §4 |

所以"增量编辑 × DVI"实际上是在问：**在"每次仍整份重编译"的前提下，XDV 的页级信息能不能让下游（转换 / 预览）少做重复工作？**

## 2. 成立的那一个：页哈希差分驱动"复用"

### 2.1 E1 实测：不影响排版的内容改动 → **0 页变化**

夹具 `bench/large`（125 页 / 4.38MB XDV）：在原文件末尾**加一行注释**（`% probe comment…`，不影响排版），重新编译后用 `--diff` 比对页哈希：

```
页级差分：A=125 页 / B=125 页
内容变化的页：0 页（逐页字节完全一致）
```

也就是说：**"源文件变了"并不意味着"排版结果变了"**——而 XDV 页哈希能把这件事**精确**判出来（字节级，不漏报；且已与 SyncTeX 对拍：差分命中的页 = 源码位置所在页）。

### 2.2 三个子场景，收益与成本

| 子场景 | 触发条件 | 省掉什么（实测/已测数据） | 成本 |
|---|---|---|---|
| **A. 跳过 PDF 转换** | 全部页哈希不变 | `xdvipdfmx` **0.65–0.94s/次**（[DVI 报告](./dvi-preview-feasibility.md) §3）；需要把 Quick 改成 `-no-pdf` + 条件转换 | C2（改 runner 的命令构造 + 条件分支） |
| **B. 跳过预览刷新** | 全部页哈希不变 | pdf.js `fetch 11ms / parse 39ms / render 64ms`（design.md §预览重载实测，46 页档） | **C1**：后端"页哈希全同"时**不发** `pdf-updated` 事件即可 |
| **C. 只重绘变化页** | 部分页变化（⑪ 的机制） | render 里未变化页的那部分（可见窗口 7 页时，若只 1 页变 → 省 ~6/7 的 render ≈ 55ms） | C2–3：`PreviewPane` 要接收"变化页集合"、在新旧 PDF 文档对象之间对齐页号、判页尺寸 |

**推荐顺序**：B（最便宜、零风险）→ A（省的是大头 0.65s，但要动命令构造）→ C（收益中等、要动预览渲染契约，且应并入 ⑪）。

### 2.3 ✅ 已落地（2026-09，按 B → C → A 顺序完成）

| 功能点 | 提交 | 做法 | 真机实测 |
|---|---|---|---|
| **B 跳过预览重载** | `bd16565` | core 新增 `xdv` 模块（页索引 + 页哈希，9 条单测）；`CompileOutcome::Success` 带 `page_hashes`；调度器持 `last_page_hashes` 算 `changed_pages`/`pages` 随 `pdf-updated` 下发；前端 `previewStore` 在"`pages>0` 且变化表为空"时**不递增 reloadKey** | 同内容编译：`reloadKey 2→2`、`skippedReloads 1→2`、`changedPages []`；真改内容：`reloadKey 2→3`、`changedPages [1,2,3]` |
| **C 只重绘变化页** | `89fd2f4` | `PreviewPane.load` 不再无条件 `renderedScale.clear()`：同文件 + 页数一致 + 有变化页集合时**只让变化页失效**，其余页沿用旧 canvas（`renderPage` 的 `renderedScale` 闸门自然跳过） | 74 页文档改**最后一章**：`pagesRendered 7→0`、`pagesReused 0→7`、**render 102ms → 7ms**、total 183→71ms |
| **A 跳过 PDF 转换** | `1b32e3b` | Quick 改用 `xelatex -no-pdf`（只产 XDV）+ `finish_success()` 按需调 `xdvipdfmx`；页哈希缓存 `tmp/<stem>.pages` 落磁盘（runner 保持无状态，且 Quick/Full 共享基线） | 连续 3 次"改注释 → Quick"全部命中 `跳过 xdvipdfmx 转换与 PDF 拷贝`；集成测试断言"第二次 Quick 不重写 `tmp/main.pdf`" |

**两条落地时才暴露的坑**（详见 §7 与 [troubleshooting](../troubleshooting.md)）：

1. **判据别用 mtime**：A 的验证最初用 `tmp/main.pdf` 的 mtime，结果被"空闲收敛"干扰——Quick 成功后 2s 的收敛 Full 走 latexmk，它自己会重写该文件，看起来像"Quick 没跳过"。改用日志行判定才看清（连 `#1` 都跳过了）。
2. **WS 探针脚本不能以 `//` 开头**：应用的执行器把脚本包进括号求值，`//` 开头的脚本会**静默**返回 `null`（不是 error），极难排查。`scripts/editor-report.mjs` 的 `Session.eval` 已加前导换行兜底。

### 2.3 成立条件与安全边界（写清楚，避免误用）

1. **必须是同一强度、同一收敛状态的两次编译**：Quick（单趟）与 Full（latexmk 收敛）的页可能本来就不同（目录页码落后一趟）→ 混比时"不同即视为变化"，**保守触发刷新**（这正是"空闲收敛"存在的理由）。
2. **页哈希是字节哈希**：内容相同但位置不同 = 变了（重排场景的诚实反映），所以 C 场景的收益**取决于编辑位置**——G2 实测：等长替换 1 页变 / 章首插入 108 页变 / 只加注释 2 页变；本轮样本：等长替换 1 页变 / 文首插入 120 页变。
3. **只能"少做重复工作"，不能"少排版"**：页哈希在**编译结束**才有（XDV 是引擎的输出），所以它无法让这一次编译变快——省的是它下游的转换与显示。
4. **跳过转换时 PDF 必须仍然是"上一次那份"**：因为页内容逐字节相同，复用旧 PDF 在视觉上等价（确定性构建 ㉚ 已证同一源两次编译 PDF 逐字节一致；跨页共享资源如字体的对象编号变化不影响页面观感）。

## 3. 被证伪的：局部重编译 + 页级拼接

想法很自然：只重编译改动的那一章（`\includeonly`），再把新页替换进旧 PDF。**实测不成立**。

夹具 `bench/multifile`（20 章 ctexbook / 74 页）：

| | 完整编译 | `\includeonly{chapters/ch03}` |
|---|---|---|
| 页数 | **74** | **33**（只剩被 include 的那一章） |
| 目录首条 | `第 1 章 … {7}`（第 7 页起） | `第 3 章 … {5}`，且章节号渲染成 **「第一章」** |
| 页码 | 7、8、9…（全书连续） | 5、6…（**从被 include 的章重新起算**） |

三处硬伤，任何一处都足以否决"页级拼接"：

1. **章节号错乱**：`\chapter` 计数器从头开始 → 第 3 章自己认为是"第一章"；
2. **页码错乱**：被 include 的章从第 5 页起（`\frontmatter`/页计数重新开始），与全书页码无关；
3. **页数缩水**：未被 include 的章**根本不排版**，产物里没有它们的页，无法"替换"只能"拼接"；而一旦改动让章页数变化（42–55 → 42–58），其后所有页的**页码文本**就写死在旧页里了 ✗。

> `\includeonly` 本身没坏——它的设计语义就是"写书时只编一章求快，**接受页码不对**"（最终仍要完整编译）。把它当增量机制用，等于要求 LaTeX 放弃"页码依赖全文"这一根本特性。
> 唯一的理论出路是"引擎支持从第 N 页、带完整前文状态继续排版"，那等价于**拥有引擎内部状态**——即上游的 fork 快照路线（Windows 无 `fork()`，已被否决）。

## 4. 已被否决 / 无消费方的两个

- **XDV 半成品渲染**（③）：[DVI 报告](./dvi-preview-feasibility.md) 实测 DVI 链路渲染比 PDF 链路慢 **57–68×**（页级 190–360ms，单页独立进程 0.77–1.03s）；而"页前缀 + 合成 postamble → 部分 PDF"虽然做通了（`xdvipdfmx` 接受、页数正确），但 0.65–0.94s/次 的转换成本与"只对 >5s 长编译有意义"的门槛，让它至今没有落地理由。
- **编译期页信息**（④）与其**追加式解析器**（⑤）：[阶段 3 报告](./stage3-incremental-output-parsing.md) 已论证"达成但无消费方"——上游 incdvi 的消费方是它自己的渲染闭环（每帧问 `page_count`/`render_page`），我们没有那个闭环。

## 5. 建议

1. ~~先做 B~~ ✅ **已落地（`bd16565`）**：见 §2.3。
2. ~~再做 ⑪ 的 C~~ ✅ **已落地（`89fd2f4`）**：`PreviewPane` 已能"只重绘变化页"（render 102ms → 7ms @74 页文档改末章）。⑪ 剩下的仍是"滚动/缩放保持"本身，与本机制正交。
3. ~~A 视情况~~ ✅ **已落地（`1b32e3b`）**：Quick 已改 `-no-pdf` + 条件 `xdvipdfmx`；顺带让 XDV 页索引进入了**产品路径**（不再只是诊断工具）。
4. **不做**：局部重编译 + 页级拼接（§3 证伪）；XDV 渲染（③）。
5. **仍未量化**：真实使用中"无可见变化的编辑"与"只影响少数页的编辑"各占多少——它决定 B/C/A 的实际收益规模（§7 局限 1）。

## 6. 复现方法

```bash
# E1：不影响排版的内容改动是否改变页哈希
cp -r test_file/projects/bench/large test_file/projects/_inc-dvi && cd $_
cp ../bench/large/tmp/main.xdv xdvA.xdv          # 基线（源未变）
echo '% probe comment' >> main.tex
SOURCE_DATE_EPOCH=0 latexmk -xelatex -outdir=tmp -synctex=1 -interaction=nonstopmode main.tex
node scripts/xdv-report.mjs xdvA.xdv --diff=tmp/main.xdv      # → 内容变化的页：0

# E2：\includeonly 的页码/章节号
cp -r test_file/projects/bench/multifile test_file/projects/_inc-dvi2 && cd $_
latexmk -xelatex -outdir=tmp  -synctex=1 -interaction=nonstopmode main.tex   # 74 页
sed -i 's/\\begin{document}/\\includeonly{chapters\/ch03}\n\\begin{document}/' main.tex
latexmk -xelatex -outdir=tmp2 -synctex=1 -interaction=nonstopmode main.tex   # 33 页
head -3 tmp2/main.toc        # 第 3 章 → 页码 5、章节号"第一章"
```

## 7. 未验证与局限

1. **未实测"跳过转换/刷新"的真实体感收益**：省的是已测的 `xdvipdfmx 0.65–0.94s` 与 `pdf.js 103ms`，但"无变化编辑"在真实使用中的**频率**没量过（需要挂钩子统计）。
2. **未实测"只重绘变化页"的实现**：涉及 pdf.js 新旧文档对象的页对齐、页尺寸变化、缩放与滚动恢复，本轮只给了收益上界估算（按 design.md 的 render 64ms / 7 页）。
3. **页哈希 ⟷ PDF 页的对应只做到"1:1 同序 + SyncTeX 对拍"**：没有逐页渲染像素级比对（那需要 MuPDF/poppler 之类的外部渲染器）。
4. **E1 只测了一个"无变化"样本**（加注释）；"改空白/撤销回原状/改被注释掉的内容"等形态未逐一验证（机理相同，但未穷举）。
5. **多文件工程下"只改一章"的差分形态未测**：本轮 E1 用单文件工程；多文件时"改动一章 → 后续页重排"的比例可能更高（也更快暴露给 C 场景）。
