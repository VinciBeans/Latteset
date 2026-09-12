# TeX IDE 市场痛点调研

> 状态：**三路专项调研全部完成并已合并**（桌面编辑器 / 在线协作 / VS Code 生态）。
> 调研时间：2026-08 / 2026-09（各专项报告标注了各自的数据采集时点）
> 目的：为 TeXPresso 的产品取舍提供外部事实依据；只记录可溯源结论，推断一律标注 `[推断]`。
> 证据等级：**A** = 官方 API / 官方文档 / 仓库 issue 可直接核实；**B** = 社区讨论（来源为检索摘要或归档，未逐帖全文核实）；**C** = 厂商推广内容 / 本文推断。

## 配套专项报告

| 文档 | 覆盖范围 | 状态 |
|---|---|---|
| 本文 | 总览 + 跨类别归纳 + 对 TexPresso 的机会点映射 | 迭代中 |
| [desktop-latex-editor-pain-points.md](./desktop-latex-editor-pain-points.md) | 桌面原生编辑器（TeXstudio/TeXworks/WinEdt/Texifier/Kile…）量化痛点 | **已完成**（2026-09） |
| [online-latex-editor-pain-points.md](./online-latex-editor-pain-points.md) | 在线协作平台（Overleaf 为主 + Papeeria/Typst 对照） | **已完成**（2026-09） |
| [vscode-latex-workshop-pain-points.md](./vscode-latex-workshop-pain-points.md) | 通用编辑器生态（VS Code + LaTeX Workshop，含迁移动机与回迁） | **已完成**（2026-09） |
| [tex-ide-roadmap-priority.md](./tex-ide-roadmap-priority.md) | 由本调研派生的**可执行 Roadmap 排序**（W/D/C/V 评分 + 分档 + DoD + 批次） | **已完成**（2026-09） |

## 0. 摘要（TL;DR）

市面 TeX 工具的核心痛点集中在 **「反馈循环太慢」** 和 **「错误不可读」** 两条主线上，其余痛点大多是这两条的衍生：

1. **入门门槛极高**：装 TeX Live 动辄 5–7 GB、数小时，中文还要额外配引擎/字体/宏包（A/B）。
2. **编译-预览循环慢**：绝大多数桌面编辑器仍需手动编译才能看效果，实时预览长期是"想要但拿不到"的功能（B）。
3. **错误信息不可读**：`.log` 原文对新手几乎无用，社区长期把"调试错误"列为最劝退环节（B）。
4. **SyncTeX 不可靠**：正反向定位错位/失效在 TeXstudio、LaTeX Workshop、SumatraPDF 等多个项目里都有长期问题；**LaTeX Workshop 官方 wiki 直接写明外部查看器的 SyncTeX "As of mid-2025, this function is not working"**，且 external viewer 整体标注 "not officially supported / Experimental"（A）。
5. **大文档/多文件卡顿**：TeXstudio、TeXworks、LaTeX Workshop 均有"大文件打开慢、编译时编辑器卡死"的 issue（A）。
6. **在线方案的限制被收紧**：Overleaf 免费版 10 秒编译超时、1 位协作者、7 MB 可编辑内容，2025-09 前后引发集中反弹（A/B）。
7. **中文用户被系统性忽视**：中文文件名/路径编译失败、ctex 字体配置、输入法光标错位等问题在中文社区高频出现（A/B）。
8. **没有任何一条迁移路径是低摩擦的**：桌面→VS Code 要"跟 IDE 打架"地写配置、有人迁回；Overleaf→本地要自己 debug 编译链、有人留回（A/B）。

**结构性结论**：LaTeX 社区反复出现「不存在一个完整的 LaTeX IDE」的讨论——通用编辑器（VS Code）缺 LaTeX 原生语义，LaTeX 原生编辑器（TeXstudio 系）缺现代 IDE 体验与性能，在线平台（Overleaf）缺本地能力与隐私（B/C）。

## 1. 调研范围与方法

- **对象**：桌面原生（TeXstudio、TeXmaker、TeXworks、WinEdt、TeXShop、Texifier/Texpad、LyX）、通用编辑器扩展（VS Code + LaTeX Workshop）、在线协作（Overleaf 及同类）、以及 2025–2026 新出现的 AI 编辑器。
- **方法**：搜索引擎检索 + 官方文档/仓库 issue 定向核实 + 中文社区（知乎/CSDN/博客园）补充。
- **局限**：Reddit 正文在当前网络环境下无法直接抓取（域名解析被拦），相关结论以**检索摘要**为依据，标注为 B 级；未做问卷/访谈，故没有一手量化数据。

## 2. 市场格局速览

| 类别 | 代表 | 规模/状态证据 |
|---|---|---|
| 在线协作 | Overleaf | 2025-06 官方口径约 2000 万用户（[newswise](https://www.newswise.com/articles/digital-science-launches-new-cutting-edge-ai-writing-tools-for-20-million-overleaf-users)）；2021-2022 曾从 900 万到 1000 万（[Overleaf blog](https://www.overleaf.com/blog/wow-ten-million-users)） |
| 桌面原生 | TeXstudio | 仍在积极维护，2026-08-14 发布 4.9.7（[texstudio.org](https://www.texstudio.org/)） |
| 通用编辑器 | VS Code + LaTeX Workshop | VS Code 在开发者中约 75.9% 使用率（2025，[commandlinux](https://commandlinux.com/statistics/ide-and-text-editor-usage-among-linux-developers-vs-code-vim-emacs-stats/)）；LaTeX Workshop **560.9 万次安装 / GitHub 12,308 stars**（2026-09 核实） |
| 本地优先新玩家 | TeXlyre、TurnALeaf、TeX64 等 | 2025–2026 出现多个"本地优先 / 离线 Overleaf 替代"项目（[TeXlyre](https://www.reddit.com/r/LaTeX/comments/1lzmvus/texlyre_free_localfirst_latex_editor_alternative/)、[TurnALeaf](https://github.com/fjwillemsen/TurnALeaf)、[TeX64 体验文](https://dev.to/tex64/i-finally-ditched-overleaf-for-a-local-latex-editor-heres-what-actually-works-2cgg)） |
| AI 原生新玩家 | Overleaf AI Assist、Octree、Crixet、OpenAI Prism | 2025-2026 集中出现，被讨论为对传统平台的威胁（[Reddit](https://www.reddit.com/r/LaTeX/comments/1r3v5e6/is_overleaf_facing_a_slow_death_in_the_ai_era/)、[对比文](https://www.useoctree.com/blog/top-ai-latex-tools-compared-pricing-speed-accuracy)） |
| 替代排版系统 | Typst | 语法/编译速度/错误信息被公认为优于 LaTeX，但生态与期刊接收是主要阻力（[Reddit](https://www.reddit.com/r/LaTeX/comments/1rw5s4b/typst_isnt_losing_to_latex_because_of_missing/)、[Reddit](https://www.reddit.com/r/LaTeX/comments/1d5lw63/debate_2024_whats_stopping_you_from_switching/)） |

> ⚠️ **命名冲突提示**：GitHub 上已存在同名开源项目 `let-def/texpresso`（LaTeX 实时渲染 + 错误报告，配合已有编辑器使用），见 [let-def/texpresso](https://github.com/let-def/texpresso)。这会直接影响搜索可见性与品牌识别 `[推断]`。

## 2.1 量化基线（来自桌面专项报告，A 级）

| 指标 | 数值 | 来源 |
|---|---|---|
| TeXstudio stars / issue 总数 / open | 3,619 / 3,160 / 431 | [GitHub API](https://api.github.com/repos/texstudio-org/texstudio)（本文已复核） |
| TeXstudio 关键词命中 | `crash` 293、`completion` 263、`synctex` 148、`slow` 76、`freeze` 31 | GitHub Search API |
| TeXworks stars / issue / open | 778 / 1,012 / 227 | [GitHub API](https://api.github.com/repos/TeXworks/texworks) |
| TeXstudio 4.9.7 单版本下载 | Win exe 61,807 / macOS-arm 64,281 | Releases API |
| TeXworks 0.6.10 Win 下载 | 34,589 | Releases API |
| TeX SE 标签题数 | overleaf 2,348、texstudio 2,075、texmaker 1,558、texshop 727、texworks 516、winedt 395、kile 263、**forward-inverse-search 327** | [SE tags API](https://api.stackexchange.com/2.3/tags/texstudio;texmaker;texworks;winedt;texshop;kile;overleaf;latex-workshop/info?site=tex.stackexchange) |
| 单条最热配置类问题 | "怎么设深色主题" **206 票 / 230,010 浏览**；词典 115 票 / 190,639 浏览 | SE API |
| VS Code LaTeX Workshop | **5,608,577** 次安装 / GitHub **12,308 stars / 587 forks**（2026-09 核实，当前 open issue = 0） | [Marketplace](https://marketplace.visualstudio.com/items?itemName=James-Yu.latex-workshop) · [repo API](https://api.github.com/repos/James-Yu/LaTeX-Workshop) |
| 桌面闭源/商业定价 | WinEdt $50/$80/$150（**不可退款**）、Texifier $39.99 单席 | [winedt](https://www.winedt.com/registration.html) · [texifier](https://www.texifier.com/purchase) |

> 解读：`synctex` 148 个 issue + 327 个 SE 题说明**双向定位是行业级长期缺陷**；`crash` 293 说明稳定性是桌面编辑器的持续成本；"深色主题"23 万次浏览说明**基础体验配置**反而是最大的重复踩坑点 `[推断]`。

## 2.2 桌面编辑器的结构性代价（来自专项报告）

| 优势（在线无法满足） | 劣势（持续丢用户） |
|---|---|
| 离线可用、不受宕机影响 | 无实时协作 |
| 数据/合规可控（企业、涉密） | 首次配置成本高（"配好花了三小时，SyncTeX 还坏了两次"） |
| **无编译超时** | 错误诊断落后于 Overleaf |
| 本地 Git / 任意工具链共存 | 需自己维护 TeX 发行版（~4 GB / 20 分钟） |
| 可自由装宏包/字体/自定义类 | 跨平台体验割裂（macOS 尤甚） |

> 以上来源与原文摘述见 [desktop 专项报告 §4](./desktop-latex-editor-pain-points.md)。

## 2.3 用户场景与痛点权重 `[本表为推断，仅用已收集来源做支撑]`

| 场景 | 首要痛点 | 支撑证据 |
|---|---|---|
| 学生 / 毕业论文 | 学校模板编译失败、环境配置、在线编译超时 | [模板报错](https://ask.latexstudio.net/ask/question/17556.html)、[北航模板撞超时](https://blog.csdn.net/yyywxk/article/details/148696458)、[写论文最难的是什么](https://www.reddit.com/r/LaTeX/comments/106gn3z/what_iswas_most_difficult_for_you_when_writing/) |
| 科研人员 / 投稿 | 期刊模板、参考文献格式、隐私（未发表数据）、协作 | [Wiley 模板与 bibtex 要求](https://wap.sciencenet.cn/home.php?mod=space&uid=3411312&do=blog&id=1467943)、[学术写作隐私](https://www.thetapad.com/blog/academic-writing-privacy-concerns)、[本地优先理念](https://www.inkandswitch.com/essay/local-first/) |
| 教师 / 讲义试卷 | 批量内容、公式密集、导出分享 | [中学数学老师用 LaTeX 的顾虑](https://zhuanlan.zhihu.com/p/316150071)（分享不便、集体备课） |
| 非技术写作者 | 入门门槛（安装 + 语法 + 错误） | [Quora 讨论](https://www.quora.com/What-are-some-recommended-LaTeX-editors-for-non-technical-individuals-looking-to-create-professional-documents-like-books-and-articles)、[新手教程推荐 TeXworks 以规避环境问题](https://zhuanlan.zhihu.com/p/456055339) |

**共性**：四类场景的痛点**几乎都落在"工具链"而非"排版语言"**上——这正好是 IDE 能改善、而 LaTeX 本身无法改善的部分。`[推断]`

## 2.4 中文市场现状（B 级）

- 中文社区的编辑器推荐排序近年趋于一致：**"VS Code + TeX Live + LaTeX Workshop"被列为第一推荐**，其次是 TeXstudio；新手教程则常推荐 TeXworks 以"避免配置环境带来的问题"（[知乎汇总, 2024](https://zhuanlan.zhihu.com/p/607473890)、[知乎新手教程](https://zhuanlan.zhihu.com/p/456055339)）。
- 也就是说：**中文用户的主流路径是"通用编辑器 + 手工配置"**，而不是原生 IDE——这正是 TeXPresso 想替代的路径 `[推断]`。
- 中文环境的额外摩擦（引擎/字体/宏包、文件名编码、输入法）见 P7；这些在英文社区讨论中基本不存在，属于**本地化空白**。

## 3. 核心痛点（按「出现频率 × 严重度」排序）

### P1. 环境与安装门槛（最高频、最劝退）

- **体积与耗时**：TeX Live 完整安装约 7 GB，"取决于镜像和网速，可能要很久"（[Reddit](https://www.reddit.com/r/LaTeX/comments/gieb17/does_texlive_usually_take_hours_to_install/)）；Windows 上有"预计 17 小时 46 分"的实例（[TeX SE](https://tex.stackexchange.com/questions/271931/should-texlive-installation-take-17-hours-windows)）；完整安装体积讨论见 [TeX SE](https://tex.stackexchange.com/questions/302676/how-large-is-the-full-install-of-texlive)。**证据等级 A/B**。
- **中文环境叠加成本**：中文 LaTeX 环境配置"劝退了不少新手"（[CSDN](https://blog.csdn.net/k0l1m2n3o/article/details/149700215)）；主流方案是 `xelatex + ctex`，Windows 下 `ctex-fontset-windows` 的默认粗体还不符合中文排版习惯（[知乎](https://zhuanlan.zhihu.com/p/538459335)）。**证据等级 B**。
- **痛点本质**：用户只想写文档，却要先成为"工具链工程师"。`[推断]`

### P2. 编译-预览反馈循环慢（实时性缺失）

- 桌面编辑器长期"不能预览除非先编译"是明确抱怨点（[Reddit: Automatic preview TexStudio](https://www.reddit.com/r/LaTeX/comments/3rszm7/automatic_preview_texstudio/)）。
- "有没有办法让 LaTeX 实时编译"是 Stack Exchange 上的长期问题（[TeX SE](https://tex.stackexchange.com/questions/633/is-there-any-way-to-get-real-time-compilation-for-latex)），说明这是社区**公认未满足**的需求。
- 2026 年仍有团队在做"实时预览 LaTeX 编辑器"并征求反馈（[Reddit](https://www.reddit.com/r/LaTeX/comments/1w37cf6/feedback_wanted_a_livepreview_latex_editor_that/)），说明**窗口仍然开着**。**证据等级 B**。
- 大文档编译耗时是根因之一："LaTeX 编译很慢，尤其带图形和需要跑 bibtex 时"（[Reddit](https://www.reddit.com/r/LaTeX/comments/1h5ifcx/build_time_very_slow/)）。**证据等级 B**。

### P3. 错误诊断不可读（最劝退的学习曲线）

- "LaTeX 错误信息完全帮不上忙……学了两天，真的非常沮丧"（[Reddit](https://www.reddit.com/r/LaTeX/comments/ijbtwp/latex_error_messages_do_not_help_at_all_any_tips/)）。
- "处理错误既麻烦又伤神，事实上这是阻止我使用 LaTeX 的主要原因"（[TeX SE: How to trace LaTeX errors efficiently](https://tex.stackexchange.com/questions/125399/how-to-trace-latex-errors-efficiently)）。**证据等级 B**。
- 具体表现：`.log` 是引擎原始输出，错误行号指向宏包内部而非用户源码、错误雪崩、多层宏展开后定位困难。`[推断，但由上述来源强支持]`

### P4. SyncTeX 双向定位不可靠

- TeXstudio：Windows 10 上正反向搜索失效，重建配置/重装均无效（[issue #205, 2018-06](https://github.com/texstudio-org/texstudio/issues/205)）。
- 定位错位："右键 PDF 跳不到正确的行/段"（[TeX SE](https://tex.stackexchange.com/questions/449673/wrong-placement-of-cursor-position)）。
- 外部查看器链路同样脆弱：SumatraPDF v3.6.1 出现 SyncTeX 回归，回退 3.5.2 才恢复（[issue #5594](https://github.com/sumatrapdfreader/sumatrapdf/issues/5594)）。
- Arara + SyncTeX 组合下直接不工作（[TeX SE](https://tex.stackexchange.com/questions/478314/arara-synctex-in-editing-a-book)）。**证据等级 A**。

### P5. 大文档 / 多文件项目性能

- TeXstudio：一次打开多个大文件"要很久，像是在做过多处理"（[issue #4410](https://github.com/texstudio-org/texstudio/issues/4410)）；编辑主文档时"极其缓慢"，编辑被 `\input` 的子文件反而不慢（[TeX SE](https://tex.stackexchange.com/questions/507050/texstudio-very-slow)）。
- TeXworks：中等规模文件就卡（[issue #963](https://github.com/TeXworks/texworks/issues/963)）。
- LaTeX Workshop：命令行 latexmk 与扩展内构建耗时差距大，"约 20 秒构建，整个构建过程编辑器都很卡"（[issue #1933](https://github.com/James-Yu/LaTeX-Workshop/issues/1933)）；2025-11 还有 VS Code 侧的性能/内存泄漏报告（[microsoft/vscode#277137](https://github.com/microsoft/vscode/issues/277137)）。**证据等级 A**。
- 多文件语义：打开子文件保存时只编译当前文件而非项目根文件（[LaTeX Workshop #1272](https://github.com/James-Yu/LaTeX-Workshop/issues/1272)）——**"项目"概念缺失**的典型症状。**证据等级 A**。

### P6. 在线方案：限制收紧 + 性能 + 数据主权

- **官方限额（A 级）**：免费版编译超时 **10 秒**、付费 240 秒；每项目最多 2000 文件；可编辑内容上限 **7 MB**（[Overleaf docs: Plan limits](https://docs.overleaf.com/getting-started/free-and-premium-plans/plan-limits)）；免费版每项目 **1 位协作者**（Standard 10 / Pro 不限），且 **2024-10-14 起强制执行**——超限项目**全部 editor 被降级为只读**（[官方公告](https://www.overleaf.com/blog/changes-to-project-sharing)）。
- **官方自认天花板（A 级，最有力的证据）**：Overleaf 自己的排障文档写道 "Some projects may even need a longer compile time than is available on our paid plans. In those cases, **you may need to download your project and compile it locally.**"，且主推规避手段是把图片换成方框的 `Fast [draft]` 模式（[docs](https://docs.overleaf.com/troubleshooting-and-support/fixing-and-preventing-compile-timeouts)）。
- **付费价格（官方，A 级）**：Standard 年付 $199/年（约 $16.75/月，月付 $25）；Pro 年付 $399/年（约 $33.25/月，月付 $45）（[Overleaf 定价页](https://www.overleaf.com/user/subscription/plans)）。
- **关键能力在付费墙后**：Git/GitHub 同步、track changes、完整历史（免费仅 24h）、Dropbox/Zotero/Mendeley 集成均为 premium（[premium features](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features)）。
- **Git 集成本身也有官方承认的硬缺口**：无 branch/tag/LFS/submodule，符号链接被转普通文件，重命名/移动会丢失 tracked changes 与 comments，且官方警告不要混用 Git 与 track changes（[Git 集成文档](https://docs.overleaf.com/integrations-and-add-ons/git-integration-and-github-synchronization/git-integration)）。
- **反弹**：2025-09 前后"Overleaf 新的编译超时就是个笑话"（[Reddit](https://www.reddit.com/r/LaTeX/comments/1ndn0r3/overleafs_new_compilation_timeout_is_a_joke/)，约 171 赞 / 70 评）；同期 Stack Exchange "求免费额度更宽松的替代品——**现在连一张图的简单 PDF 都编译不了**"（[TeX SE, 2025-09-10](https://tex.stackexchange.com/questions/750873/alternative-to-overleaf-with-a-better-free-plan)）；HN 高票评论：博士论文毕业后失去学校账号即**再也编译不了自己的论文**（[HN](https://news.ycombinator.com/item?id=40834159)）。
- **可用性与性能**：官方 status 显示核心组件约 **98.16% uptime**（2026-06~09 区间）并有持续两天的事故（[status.overleaf.com](https://status.overleaf.com/)）；打字卡顿、加载 5 秒后整页卡死（[Reddit](https://www.reddit.com/r/LaTeX/comments/k9torf/poor_overleaf_performance/) 等）。
- **中国网络**：Overleaf 加载会请求 Google 服务导致访问缓慢，中文社区给出屏蔽方案（[知乎](https://zhuanlan.zhihu.com/p/362307389)）——**对中文用户是结构性劣势**。**证据等级 B**。
- **隐私与 AI**：AI 功能会把文档内容发给第三方（含 OpenAI）（[AI 文档](https://docs.overleaf.com/integrations-and-add-ons/ai-features)）；AI 免费额度有限（5 次/天），超出需购买 add-on。`[推断]` 这会加剧"免费版体验被压缩"的观感。
- **无法真正离线**：离线只能靠付费的 Git/Dropbox 同步，没有离线编辑模式；自建 Community Edition 反而缺 Git/track changes/评论/SSO/沙箱（[CE 说明](https://docs.overleaf.com/on-premises/welcome/server-pro-vs.-community-edition)）。

> 完整原文摘述、时间线与竞品对照见 [online 专项报告](./online-latex-editor-pain-points.md)。

### P7. 中文用户特有痛点

- **中文文件名/路径导致编译失败**（多来源，等级 A/B）：
  - "tex 编译中文文档类 ctexart 时，源文件名不可包含中文，否则会出现编译错误"（[博客园](https://www.cnblogs.com/Gelthin2017/p/8021971.html)）。
  - 根因是**编码不一致**：`.log`/`.fls`/`.aux` 用 UTF-8，而 Windows 系统代码页通常不是 UTF-8（[TeX Live 邮件列表, 2022-03](https://ftp.tug.org/pipermail/tex-live/2022-March/047875.html)）。
  - 实操建议直接是"避免使用中文、空格或非 ASCII 字符作为 TeX 文件名，这类字符会干扰 latexmk"（[火山引擎文档, 2026-06](https://www.volcengine.com/article/1360494)）。
  - 影响面不止文件名：**Windows 用户名含非 ASCII 字符会导致 TeX Live 安装失败**（[TeX SE](https://tex.stackexchange.com/questions/662796/tex-live-installation-fails-on-windows-10-if-username-contains-non-ascii-charact)）；中文路径下 latexmk 报错的解决方案是改环境变量（[CSDN](https://blog.csdn.net/qq_41554005/article/details/120698428)）。
  - → **对 Windows 首发 + 中文友好的产品，这是必须自测的高危项**。
- **编辑器中文输入光标错位**：TeXstudio 下"光标明明在句号后面，输入的文字却插在句号前面"（[知乎](https://www.zhihu.com/question/27604588)）。
- **输入法渲染问题不止 LaTeX**：VS Code 在 2025-10 前后版本出现"中文输入法无法正常工作 / 不选备选词直接回车导致内容错乱"（[microsoft/vscode#279762, 2025-11-27](https://github.com/microsoft/vscode/issues/279762)）——说明这是**编辑器内核层**问题，不是 LaTeX 专属。**证据等级 A**。
- **中文排版细节**：`ctex` 默认粗体不符合中文习惯、字体授权（商业字体）问题（[知乎](https://zhuanlan.zhihu.com/p/538459335)）。**证据等级 B**。
- **VS Code 侧的中文缺陷（A 级，来自 VS Code 专项）**：中文路径导致 `! Emergency stop.`（[#1245](https://github.com/James-Yu/LaTeX-Workshop/issues/1245)）；**格式化 PR #4433 会把中文截断**（`不等于语言` → `不等于���言`）；出现 TeXstudio 能编译、VS Code 不能的对照案例（[TeX SE 553023](https://tex.stackexchange.com/questions/553023)，39,902 浏览）；用 xelatex 需手改 JSON 且"有点让人困惑"（[TeX SE 564758](https://tex.stackexchange.com/questions/564758)，33,998 浏览）。→ 说明**中文支持在通用编辑器里同样是空白**。

### P8. 编辑体验与 IDE 能力缺失

- **拼写/语法检查弱**："为什么 LaTeX 的拼写检查这么差"——答案往往是"LaTeX 没有拼写检查，靠编辑器"，配置成本高（[Reddit](https://www.reddit.com/r/LaTeX/comments/j4d4ml/why_is_the_spell_checker_in_latex_so_bad/)）。
- **全项目查找替换缺失**：博士生收尾时想全项目替换词，需要专门求助（[TeX SE](https://tex.stackexchange.com/questions/526422/find-and-replace-words-from-an-entire-project)）。
- **引用/标签补全长期欠缺**：Overleaf（ShareLatex）自 2015 年就有 `\ref`/`\cite` 补全的 feature request（[sharelatex#354](https://github.com/sharelatex/sharelatex/issues/354)）。
- **可扩展性差**："除非用 LuaTeX 或你是 TeX 宏专家，否则扩展性非常差"（[Hacker News](https://news.ycombinator.com/item?id=21711364)）。**证据等级 B**。

### P9. 模板/文档类（.cls）开箱即用性差

- 下载的学校/期刊模板"未做任何操作就编译报错"：`cls 文件未找到`、`undefined sequence` 等（[LaTeX 问答, 2024-12](https://ask.latexstudio.net/ask/question/17556.html)）。
- 毕业论文场景叠加在线限额：用 Overleaf 编译超长毕业论文时**撞上编译时长限制**（[CSDN 北航模板踩坑, 2025-06](https://blog.csdn.net/yyywxk/article/details/148696458)）。
- 论文写作本身的高频困难集中在"模板/环境/编译"而非写作（[Reddit: 写论文时最难的是什么](https://www.reddit.com/r/LaTeX/comments/106gn3z/what_iswas_most_difficult_for_you_when_writing/)）。**证据等级 B**。
- → 对 IDE 的含义：**首次打开一个陌生模板必须能自动推断出正确的引擎/编译链**，否则用户会在第一步流失。`[推断]`

### P10. 通用编辑器的结构性代价

- **编译 recipe / 工具链配置复杂（VS Code 侧最高频痛点）**：LaTeX Workshop 要求手写 `recipes` + `tools` 两套 JSON、支持 13 个占位符；"自定义 recipe 文件放哪"66,470 浏览、"如何在 VS Code 用 LaTeX"299,276 浏览（[VS Code 专项 §2](./vscode-latex-workshop-pain-points.md)）；社区原话把配置称为 "fighting the IDE"。
- VS Code 本身资源占用高（Electron）——但**该结论证据最弱**：只有定性描述，未找到 LaTeX Workshop 的内存数值；曾被认为是扩展问题的 #2736 实为 LTeX 并已标 `external issue`（详见 [VS Code 专项 §8](./vscode-latex-workshop-pain-points.md)）。**证据等级 B/C**。
- LaTeX 在 VS Code 里是"二等公民"：PDF 预览、项目模型、编译 recipe 都要靠扩展 + 手写配置拼出来。`[推断，由 #1272/#1933 与配置类求助帖支持]`
- **缺"项目"概念**：跨文件操作（批量改 label/citation key、章节重排）在 VS Code 里没有对应能力，有用户因此保留 TeXstudio 做 outline/label/table。**证据等级 B**。
- **LaTeX Workshop 官方 wiki 自认的结构性限制（A 级，2026-09 核实）**：
  - 根文件靠启发式 + 正则静态解析，"**we cannot compute file inclusions if user defined macros are used to include files**"；找不到根文件时"**most of the features… will not work**"。
  - `auxDir` 设置下"**compilation may fail** if LaTeX-Workshop cannot properly detect files inclusion"。
  - PDF 刷新会"repeated viewer refreshes and/or **loss of PDF scrolling position**… potentially causing **file corruption**"，官方建议调大 `latex.watch.pdf.delay`。
  - 外部 PDF 查看器路径 "not officially supported"，`view.pdf.viewer: "external"` 标注 **Experimental**；SyncTeX 自 2025 年中期起不工作（[wiki/View](https://github.com/James-Yu/LaTeX-Workshop/wiki/View) 原文 "As of mid-2025, this function is not working"，指向 [#4584](https://github.com/James-Yu/LaTeX-Workshop/issues/4584)）。
- **PDF 预览的架构问题被明确拒绝修复**：贡献者提出改用 VS Code `customEditor` API 重写预览器，原文称其实现在 **PR #3069 被直接拒绝并锁定**，此后自己维护 fork 半年（[#3337](https://github.com/James-Yu/LaTeX-Workshop/issues/3337)，该请求当天即被关闭并锁定，标签 "discussion needed & close for now"）；大量渲染缺陷被维护者打上 `pdf.js issue` + `external issue` 标签，即"不是本扩展能修的"。**证据等级 A**。
- → 含义：通用编辑器方案在**项目模型、根文件探测、PDF 预览**三处有天花板，而这正是原生 IDE 的机会 `[推断]`。
- **迁移动机与"回迁"（A/B 级）**：迁到 VS Code 的理由是"一个编辑器统一所有工作 + 扩展生态 + Git 连通性"；迁走后的新痛点是"配置就是在跟 IDE 打架"、跨文件 label 操作缺失、引用失效（需先在 TeXstudio 编译一次）；**有用户明确迁回 TeXstudio**（"I keep coming back to Texstudio"）。从 Overleaf 迁到本地的理由同样是离线/隐私/免费层限制，但新问题是"一个接一个的头痛"、报错即停需自己 debug、参考文献变 key、TeX Live >7 GB / 约 2 小时安装，**也有人留回 Overleaf**。→ **没有任何一条路径是低摩擦的**，这是本调研最重要的元结论之一 `[推断]`。

### P11. 桌面编辑器专属补充痛点（来自桌面专项报告，A 级）

桌面专项调研另外量化出 7 类本文前述未覆盖的痛点，完整原文摘述见 [desktop 报告 §2](./desktop-latex-editor-pain-points.md)：

| 痛点 | 代表证据 | 性质 |
|---|---|---|
| **内置 PDF 预览器质量差** | TeXstudio #4023（47 评论）：打开内置预览即崩溃；换 Arthur 后端不崩但"中文以外的数字和字母全部消失" | 崩溃多为版本 bug；"内置预览不如外部阅读器"是架构性 |
| **崩溃 / 卡死 / 无响应** | #1100 改设置点 Apply 即 SIGSEGV；#4629 词典对话框空输入点击永久冻结；TeXworks #1026 编译后约半数概率报错退出 | 主要为版本 bug（Qt5→6 升级回归反复出现） |
| **配置"可发现性"门槛** | "怎么设深色主题"206 票 / 23 万浏览；词典 115 票；`-shell-escape` 104 票；显示行号 84 票 | 架构性 UX + 文档债（官方 #2608 自认手册过时） |
| **补全 / cwl 生态不可靠** | #744 数学模式补全变慢并残留浮动框；#4622 自定义文档类的 cwl 无法放置；TeXworks #776 语法高亮不认关键字被标 **wontfix** | 架构性（静态词典模型 + 主线程耦合） |
| **深色主题 / UI 一致性** | #45（54 评论）"页面周围一圈去不掉的浅灰边框"；TeXworks #932 深色模式 2021 年提，至今 open | 架构性、长期未解决 |
| **跨平台原生体验差** | #3637（44 评论）macOS 最小窗口高度过大、鼠标够不到 resize 把手；TeXworks #905 macOS Big Sur 打不开；Kile 强依赖 KDE | 架构性（技术栈决定） |
| **多文件工程 / 构建整合薄弱** | TeXworks #784 "加 make"请求 **2017 年提出至今 open**："Adding make is the single most common configuration change…"；TeXstudio #54 宏不随项目走 | 架构性（"项目"概念弱于 IDE） |

> **分类结论**：桌面编辑器 10 类主要痛点中至少 **7 类属架构性/长期未解决**（错误诊断、性能、SyncTeX、配置可发现性、补全、深色主题、跨平台、工程管理），仅崩溃与部分 PDF 渲染缺陷主要是版本 bug。这与"版本号十年未变"的观察一致（TeXstudio 4.9.7 / 2026-08，TeXworks 0.6.11 / 2026-02）。

## 4. 痛点优先级矩阵

| 痛点 | 出现频率 | 严重度 | 现有工具解决程度 | 备注 |
|---|---|---|---|---|
| 环境/安装门槛 | 极高 | 高 | 差（Overleaf 绕过但受限） | 一次性成本，但决定新用户去留 |
| 实时编译/反馈延迟 | 高 | 高 | 差（TeXP/TeXpresso 类项目在攻） | TexPresso 的核心设计因素 |
| 错误信息不可读 | 极高 | 高 | 差 | 与 .log 解析质量直接相关 |
| SyncTeX 不可靠 | 高 | 中高 | 中 | 多项目长期 issue |
| 大文档/多文件性能 | 高 | 高 | 差 | 与编辑器内核强相关 |
| 在线限额与隐私 | 高 | 高 | 在线方案自身取舍 | 本地方案的机会窗口 |
| 中文支持 | 中（中文用户内为极高） | 高 | 差 | 差异化最明确的一块 |
| IDE 能力（补全/替换/拼写） | 中高 | 中 | 中 | LSP 可部分解决 |
| 模板/cls 开箱即用 | 高 | 高 | 差 | 决定"第一次打开"的成败 |
| 内置 PDF 预览器 | 高 | 中高 | 差 | 自研渲染难追上成熟阅读器 |
| 崩溃/稳定性 | 高（293 个 crash issue） | 高 | 中 | 桌面编辑器持续维护成本 |
| 配置可发现性 | 极高（23 万浏览） | 中 | 差 | 基础功能藏得深，重复踩坑 |
| 编译 recipe / 工具链配置 | 极高（VS Code 侧第一痛点） | 高 | 差 | 手写两套 JSON + 13 个占位符 |
| 补全 / cwl 生态 | 高（263 个 issue） | 中高 | 差 | 静态词典无法理解自定义宏包 |
| 深色主题 / UI 一致性 | 高（206 票） | 中 | 差 | 长期未解决，Qt 自绘混用 |
| 跨平台原生体验 | 中 | 中高 | 差 | macOS/Linux 体验割裂 |
| 协作/版本控制 | 中 | 中高 | 在线强、本地弱 | 本地方案的结构性缺口 |

> 排序方法：以"独立来源提及次数"为频率代理、以"是否导致放弃工具"为严重度代理，均为本文评估 `[推断]`，非量化调查。

## 5. 结构性结论

1. **没有"完整的 LaTeX IDE"**：社区 6 年前就在问"Is there a complete LaTeX editor?"，并指出"在学术圈 LaTeX 无处不在，却没有任何编辑器像通用编程 IDE 那样完整"（[Reddit](https://www.reddit.com/r/LaTeX/comments/hqxk64/is_there_a_complete_latex_editor/)）。
2. **三条路各有硬伤**：原生编辑器 = 性能/现代体验差；通用编辑器 = LaTeX 语义缺失 + 配置门槛；在线 = 限额/性能/隐私/网络。
3. **AI 正在改变竞争格局**：2025-2026 出现"Overleaf 在 AI 时代是否会慢慢衰落"的讨论（[Reddit](https://www.reddit.com/r/LaTeX/comments/1r3v5e6/is_overleaf_facing_a_slow_death_in_the_ai_era/)），以及专门的 AI LaTeX 编辑器横评（[Octree](https://www.useoctree.com/blog/top-ai-latex-tools-compared-pricing-speed-accuracy)、[2026 云编辑器对比](https://tex.stackexchange.com/questions/761024/what-are-the-meaningful-differences-between-cloud-latex-editors-in-2026-especia)）。
4. **本地优先是明确的未满足需求**：2025-2026 至少出现 3 个"离线/本地优先 Overleaf 替代"项目，以及"我终于为本地 LaTeX 编辑器放弃了 Overleaf"的体验文（[dev.to](https://dev.to/tex64/i-finally-ditched-overleaf-for-a-local-latex-editor-heres-what-actually-works-2cgg)）。
5. **通用编辑器有官方承认的天花板**，且集中在三处——这三处恰好只有原生 LaTeX IDE 能改 `[推断]`：
   - **项目模型 / 根文件探测**：LaTeX Workshop 用 5 步启发式 + 正则静态解析，官方写明"用户用自定义宏包含文件时无法计算文件包含关系"，"找不到根文件则大多数功能不工作"；"编译工作目录可配置"这一请求被标为 `not_planned`（[#3467](https://github.com/James-Yu/LaTeX-Workshop/issues/3467)）。
   - **PDF 预览不是一等公民**：预览器基于 pdf.js，改用 VS Code `customEditor` API 重写的实现（PR #3069）被拒绝并锁定（[#3337](https://github.com/James-Yu/LaTeX-Workshop/issues/3337)），大量渲染缺陷被归因为 `pdf.js issue`；官方承认刷新会"丢失滚动位置、潜在文件损坏"。
   - **跨文件 LaTeX 语义操作缺失**：批量改 label/citation key、章节重排、宏感知分析都做不到——因为底座是正则静态解析，不是 LaTeX AST/索引层。
   > 补充：维护者自建的"暂时无解"标签（`come back later (hopefully)`）**全仓库仅 2 个 issue**（#3467、#3758），因为该仓库当前 open issue 数为 0、新 issue 数小时内即被关闭——所以"作者无解"只能靠官方 wiki 明文声明与标签来识别，不能靠 open 状态。

## 6. 对 TexPresso 的机会点映射

| 市场痛点 | TexPresso 现状 | 机会 / 风险 |
|---|---|---|
| 实时编译 | **已作为核心设计因素落地**（连续编译，防抖 ~500ms；见 [design.md](../design.md)、[ADR-0005](../adr/0005-latexmk-first-incremental-next.md)） | 直接命中 P2；但 latexmk 单遍重排的引擎上限意味着**超大文档延迟无法靠调度解决**，需要明确定位与预期管理 |
| 错误不可读 | 已有 `.log` 解析 + 同源去重 + 点击跳转（[README](../../README.md)） | 命中 P3；但"解析+去重"只是及格线，**错误解释/修复建议**才是差异化 `[推断]` |
| SyncTeX 双向定位 | 已实现正反向（[README](../../README.md)） | 命中 P4；需持续以真实多文件项目验证，否则会重演社区长期 issue |
| 中文支持 | 中文 UI v1、XeLaTeX 默认引擎、Monaco 自研高亮 | 命中 P7，是**最明确的差异化**；但中文文件名/路径、输入法光标、ctex 字体仍是空白，需要逐项验证 |
| 大文档性能 | 预览做了分页虚拟化 + canvas 复用；编译为整份单遍 | P5 部分缓解；**超大文档**仍是风险 |
| 环境门槛 | 依赖系统 TeX Live/MiKTeX，缺失时提示安装 | 未解决 P1。捆绑 TinyTeX 在 [design.md](../design.md) 后置清单里——**这是新手转化的关键一环** `[推断]` |
| **配置零门槛** | 设置页图形化（引擎/模式/防抖/超时/根文件），无需手写 JSON | 直接命中"配置=跟 IDE 打架"这一 VS Code 第一痛点 `[推断]` |
| 本地/隐私 | 本地运行、文件夹即项目、`.texpresso/settings.json` 可进 git | 命中 P6 的本地优先需求 |
| **无编译超时** | 本地编译，超时上限可配置（默认 120s，可调） | **这是相对在线方案最锋利的一刀**：Overleaf 官方承认部分项目必须本地编译 `[推断]` |
| 协作 | 无实时协作 | 本地方案的公认缺口；`[推断]` 与 Overleaf 竞争时会被追问，但也可定位为"个人/离线优先" |
| Git 工作流 | 文件夹即项目，可直接用本地 git | Overleaf 把 Git 放在付费墙后且官方承认有硬缺口——**本地方案天然更强** `[推断]` |
| AI | 无 | 2026 年竞争主轴之一；缺失会成为短板 `[推断]` |

**定位建议（综合三路调研，`[推断]`）**：不要拼实时协作（打不过 Overleaf），拼 **「无超时编译 + 离线 + 数据不出本机 + 大项目不卡 + SyncTeX 响应快 + Git 原生 + 配置零门槛 + 中文开箱即用」**。优先客群：写学位论文的研究生 → 需要 Git 工作流的开发者型作者 → 有数据合规要求的机构/企业。

## 7. 存疑与反证

- **"中文文件名不可编译"**：已从单一来源升级为**多来源**（TeX Live 邮件列表的编码根因 + 实操文档 + 中文路径案例），但**仍未在 TexPresso 里实测**——Windows 首发产品必须逐项复核后再当作结论。
- **"桌面编辑器不维护了"**：**证伪**。TeXstudio 2026-08-14 仍在发版（4.9.7），TeXworks 2026-02 仍在发版（0.6.11），均活跃维护。
- **"LaTeX Workshop 有 open 多年的 issue"**：**不可直接观察**。2026-09 核实该仓库 `open_issues_count = 0`——所有 issue 均已关闭并锁定（`locked: true, active_lock_reason: "resolved"`），讨论疑似转移到 Discussions。因此凡涉及"长期未解决"的论断，本文改用官方 wiki 的明文声明或维护者标签作为依据，而非 issue 的 open 状态。
- **"用户会为本地能力付费"**：**未验证**。找到的是"换工具"的意愿证据，不是付费证据；付费意愿仍需一手调研。
- **Reddit 证据**：本环境下 Reddit 正文不可抓取（域名解析为非公网 IP），相关引用来自检索摘要或 PullPush 归档片段，存在失真风险，标为 B 级。
- **明确未取得的数据**（诚实声明，勿在后续引用中当作已知）：
  1. Kile 的 Bugzilla bug 总数（`bugs.kde.org` REST 不返回总数）；
  2. TeXmaker / TeXShop / WinEdt 的官方 issue 计数（三者无公开 issue 库）；
  3. Reddit 原帖完整票数/评论数；
  4. TeXstudio 真实用户数（GitHub `download_count` 不含 apt/AUR/Flatpak/TeX Live 内置分发）；
  5. Overleaf 当前（2026）用户规模——官方最新公开数字停在 2021 年的 900 万；
  6. 任何一手问卷/访谈数据（本文全部为二手来源）；
  7. LaTeX Workshop 的内存占用数值（只有定性描述；曾疑为扩展问题的 #2736 实为 LTeX）；
  8. Overleaf→本地、TeXstudio→VS Code 的**回迁率**统计（只有个案）；
  9. 同机端到端编译耗时对照实验。

## 8. 来源清单

官方/仓库（A）：
- [Overleaf 免费与付费限额](https://docs.overleaf.com/getting-started/free-and-premium-plans/plan-limits) · [Overleaf 定价](https://www.overleaf.com/user/subscription/plans) · [premium 功能表](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features) · [编译超时排障（官方自认需本地编译）](https://docs.overleaf.com/troubleshooting-and-support/fixing-and-preventing-compile-timeouts) · [2024 协作者规则变更公告](https://www.overleaf.com/blog/changes-to-project-sharing) · [Git 集成限制](https://docs.overleaf.com/integrations-and-add-ons/git-integration-and-github-synchronization/git-integration) · [AI 功能](https://docs.overleaf.com/integrations-and-add-ons/ai-features) · [CE vs Server Pro](https://docs.overleaf.com/on-premises/welcome/server-pro-vs.-community-edition) · [Overleaf 服务状态](https://status.overleaf.com/)
- [TeXstudio issue #4410 大文件打开慢](https://github.com/texstudio-org/texstudio/issues/4410) · [issue #205 SyncTeX 失效](https://github.com/texstudio-org/texstudio/issues/205)
- [TeXworks issue #963 卡顿](https://github.com/TeXworks/texworks/issues/963)
- [LaTeX Workshop issue #1933 构建性能](https://github.com/James-Yu/LaTeX-Workshop/issues/1933) · [issue #1272 多文件/子文件](https://github.com/James-Yu/LaTeX-Workshop/issues/1272)
- [microsoft/vscode #277137 性能/内存](https://github.com/microsoft/vscode/issues/277137) · [#279762 中文输入法](https://github.com/microsoft/vscode/issues/279762)
- [SumatraPDF #5594 SyncTeX 回归](https://github.com/sumatrapdfreader/sumatrapdf/issues/5594)
- [sharelatex #354 ref/cite 补全请求](https://github.com/sharelatex/sharelatex/issues/354)
- [texstudio.org 发布记录](https://www.texstudio.org/) · [let-def/texpresso（同名项目）](https://github.com/let-def/texpresso)

桌面专项报告补充的 A 级来源（完整清单见 [desktop 报告 §5](./desktop-latex-editor-pain-points.md)）：
- TeXstudio issue：[#50 错误解析](https://github.com/texstudio-org/texstudio/issues/50) · [#4410 大项目](https://github.com/texstudio-org/texstudio/issues/4410) · [#4642 SyncTeX 跳错](https://github.com/texstudio-org/texstudio/issues/4642) · [#3825 synctex(busy)](https://github.com/texstudio-org/texstudio/issues/3825) · [#4023 内置预览崩溃/丢字](https://github.com/texstudio-org/texstudio/issues/4023) · [#45 深色主题](https://github.com/texstudio-org/texstudio/issues/45) · [#2608 手册过时](https://github.com/texstudio-org/texstudio/issues/2608) · [#744 补全变慢](https://github.com/texstudio-org/texstudio/issues/744) · [#4622 cwl 路径](https://github.com/texstudio-org/texstudio/issues/4622) · [#3637 macOS 窗口](https://github.com/texstudio-org/texstudio/issues/3637) · [#54 宏不随项目走](https://github.com/texstudio-org/texstudio/issues/54)
- TeXworks issue：[#963 大文件卡](https://github.com/TeXworks/texworks/issues/963) · [#784 加 make（2017 至今 open）](https://github.com/TeXworks/texworks/issues/784) · [#932 深色模式（2021 至今 open）](https://github.com/TeXworks/texworks/issues/932) · [#776 语法高亮 wontfix](https://github.com/TeXworks/texworks/issues/776) · [#1037 渲染](https://github.com/TeXworks/texworks/issues/1037) · [#892 模糊](https://github.com/TeXworks/texworks/issues/892) · [#1026 编译后退出](https://github.com/TeXworks/texworks/issues/1026)
- TeX SE 高票配置类：[深色主题 206 票](https://tex.stackexchange.com/questions/108315/how-can-i-set-a-dark-theme-in-texstudio) · [词典 115 票](https://tex.stackexchange.com/questions/87650/dictionary-for-texstudio-no-dictionary-available) · [-shell-escape 104 票](https://tex.stackexchange.com/questions/99475/how-to-invoke-latex-with-the-shell-escape-flag-in-texstudio-former-texmakerx) · [行号 84 票](https://tex.stackexchange.com/questions/183801/texstudio-show-line-numbers-in-editor) · [字数统计 82 票](https://tex.stackexchange.com/questions/215692/is-there-a-direct-way-to-get-word-count-in-texstudio)
- TeX SE 痛点原文：[错误提示只说 Process exited（2,325 浏览）](https://tex.stackexchange.com/questions/574762/texstudio-error-messages-just-say-process-exited-with-errors-without-any-oth) · [find 卡 1 分钟](https://tex.stackexchange.com/questions/706640/texstudio-is-exceptionally-slow-when-using-find) · [150 页首编 >10 分钟](https://tex.stackexchange.com/questions/434030/slow-compiling-of-large-documents-in-texstudio) · [反向搜索跳错位置](https://tex.stackexchange.com/questions/400449/inverse-search-jumps-to-wrong-position)
- 其他：[HN 本地工具链/File System Access API](https://news.ycombinator.com/item?id=49441375) · [WinEdt 定价](https://www.winedt.com/registration.html) · [Texifier 定价](https://www.texifier.com/purchase) · [LaTeX Workshop Marketplace 安装量](https://marketplace.visualstudio.com/items?itemName=James-Yu.latex-workshop)

VS Code 专项报告补充的 A 级来源（完整清单见 [VS Code 报告 §1/§8](./vscode-latex-workshop-pain-points.md)）：
- 官方 wiki：[Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile)（根文件/宏包含/auxDir 限制）· [View](https://github.com/James-Yu/LaTeX-Workshop/wiki/View)（外部查看器不受支持、SyncTeX 自 2025 年中不工作、丢滚动位置/文件损坏）
- 维护者标签与架构性 issue：[#3467 编译工作目录 not_planned](https://github.com/James-Yu/LaTeX-Workshop/issues/3467) · [#3758 PDF.js 引用预览](https://github.com/James-Yu/LaTeX-Workshop/issues/3758) · [#3337 customEditor API 被拒](https://github.com/James-Yu/LaTeX-Workshop/issues/3337) · [#4584 外部 viewer SyncTeX 失效](https://github.com/James-Yu/LaTeX-Workshop/issues/4584)
- 中文/Windows 缺陷：[#1245 中文路径 Emergency stop](https://github.com/James-Yu/LaTeX-Workshop/issues/1245) · [PR #4433 中文格式化截断](https://github.com/James-Yu/LaTeX-Workshop/pull/4433) · [#4976 magic options 解析错误](https://github.com/James-Yu/LaTeX-Workshop/issues/4976)
- 需求侧浏览量：[如何在 VS Code 用 LaTeX（299,276）](https://tex.stackexchange.com/questions/462365) · [自定义 recipe 放哪（66,470）](https://tex.stackexchange.com/questions/478865) · [Go to Source 不工作（43,680）](https://tex.stackexchange.com/questions/538797) · [引用失效但 TeXstudio 正常（39,902）](https://tex.stackexchange.com/questions/553023)

社区/讨论（B）：
- Reddit r/LaTeX：[Your Pains with LaTeX](https://www.reddit.com/r/LaTeX/comments/tdtw79/your_pains_with_latex/) · [Is there a complete LaTeX editor?](https://www.reddit.com/r/LaTeX/comments/hqxk64/is_there_a_complete_latex_editor/) · [错误信息无帮助](https://www.reddit.com/r/LaTeX/comments/ijbtwp/latex_error_messages_do_not_help_at_all_any_tips/) · [TeX Live 装几小时](https://www.reddit.com/r/LaTeX/comments/gieb17/does_texlive_usually_take_hours_to_install/) · [构建很慢](https://www.reddit.com/r/LaTeX/comments/1h5ifcx/build_time_very_slow/) · [Overleaf 超时吐槽](https://www.reddit.com/r/LaTeX/comments/1ndn0r3/overleafs_new_compilation_timeout_is_a_joke/) · [Overleaf 性能差](https://www.reddit.com/r/LaTeX/comments/k9torf/poor_overleaf_performance/) · [免费替代](https://www.reddit.com/r/LaTeX/comments/1gdcii0/free_alternative_to_overleaf/) · [AI 时代 Overleaf](https://www.reddit.com/r/LaTeX/comments/1r3v5e6/is_overleaf_facing_a_slow_death_in_the_ai_era/) · [TeXstudio 体验差](https://www.reddit.com/r/LaTeX/comments/1iq346a/my_experience_with_texstudio_wasnt_very_good/) · [不能预览除非编译](https://www.reddit.com/r/LaTeX/comments/3rszm7/automatic_preview_texstudio/) · [VS Code 体验差](https://www.reddit.com/r/LaTeX/comments/eqgxl4/my_short_and_terrible_experience_with_vs_code/) · [实时预览编辑器征求反馈](https://www.reddit.com/r/LaTeX/comments/1w37cf6/feedback_wanted_a_livepreview_latex_editor_that/) · [Typst 讨论](https://www.reddit.com/r/LaTeX/comments/1rw5s4b/typst_isnt_losing_to_latex_because_of_missing/)
- TeX SE：[追踪错误](https://tex.stackexchange.com/questions/125399/how-to-trace-latex-errors-efficiently) · [TeXstudio 很慢](https://tex.stackexchange.com/questions/507050/texstudio-very-slow) · [实时编译可行性](https://tex.stackexchange.com/questions/633/is-there-any-way-to-get-real-time-compilation-for-latex) · [免费额度更宽的替代](https://tex.stackexchange.com/questions/750873/alternative-to-overleaf-with-a-better-free-plan) · [TeX Live 体积](https://tex.stackexchange.com/questions/302676/how-large-is-the-full-install-of-texlive) · [Windows 装 17 小时](https://tex.stackexchange.com/questions/271931/should-texlive-installation-take-17-hours-windows) · [全项目替换](https://tex.stackexchange.com/questions/526422/find-and-replace-words-from-an-entire-project) · [2026 云编辑器对比](https://tex.stackexchange.com/questions/761024/what-are-the-meaningful-differences-between-cloud-latex-editors-in-2026-especia)
- 中文社区：[知乎：有哪些好的 LaTeX 编辑器](https://www.zhihu.com/question/19954023) · [知乎：TeXstudio 中文光标定位](https://www.zhihu.com/question/27604588) · [知乎：中文字体配置](https://zhuanlan.zhihu.com/p/538459335) · [知乎：Overleaf 加载缓慢](https://zhuanlan.zhihu.com/p/362307389) · [博客园：中文文件名编译问题](https://www.cnblogs.com/Gelthin2017/p/8021971.html)
- 其他：[Hacker News 可扩展性讨论](https://news.ycombinator.com/item?id=21711364) · [本地优先软件](https://www.inkandswitch.com/essay/local-first/)
