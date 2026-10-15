# 在线 LaTeX 编辑器/协作平台核心痛点调研

> 对象：Overleaf（主体），兼顾 Papeeria、Authorea、Typst 官方 Web App
> 方法：web_search（12 轮、40+ 查询）+ web_fetch 抓取一手页面（官方文档、定价页、status 页、博客公告、GitHub issue、HN/TeX SE API）
> 调研时间：以各来源页面自身显示的抓取/更新日期为准（多数官方页面为 2026 年快照）
> 标注约定：**事实** = 有 URL 支撑；**[推断]** = 我的判断，无直接来源

---

## 0. 一句话结论

Overleaf 的痛点不是「功能少」，而是**平台把「编译资源」当成商品来切分**：免费版编译超时被压到 **10 秒**、协作者压到 **1 人**，而把 Git、track changes、完整历史、离线同步全部锁进 $199/$399 一年的付费墙。官方文档自己承认「有些项目即使付费版时间也不够，需要下载到本地编译」——这句话就是在线方案结构性天花板的官方注脚。

---

## 1. 痛点排序（按出现频率 × 严重度）

### ① 免费版编译超时过短（10 秒）—— 最高频、最致命

**事实**

- Overleaf 官方《Plan limits》表格：**Free plan 编译超时 10 秒，Premium plans 240 秒**（正好 24×）。
  来源：[docs.overleaf.com/…/plan-limits](https://docs.overleaf.com/getting-started/free-and-premium-plans/plan-limits)
- 定价页把免费版标为 "Basic compile timeout"，付费版标为 "24x Basic compile timeout"。
  来源：[overleaf.com/user/subscription/plans](https://www.overleaf.com/user/subscription/plans)
- 官方《Fixing and preventing compile timeouts》原文：「Some projects may even need a longer compile time than is available on our paid plans. In those cases, **you may need to download your project and compile it locally.**」
  来源：[docs.overleaf.com/…/fixing-and-preventing-compile-timeouts](https://docs.overleaf.com/troubleshooting-and-support/fixing-and-preventing-compile-timeouts)
- 官方给出的主要规避手段是**降级画质**：切到 `Fast [draft]` 模式（把所有图片替换成方框）后再编译。
  来源：同上
- 2025-09 TeX Stack Exchange 提问（1,812 次浏览，被标记为 duplicate）原文：「the limitations on free use are now too restrictive… **Now I can't even compile a simple PDF with one image.**… Overleaf has lost what we loved about it: simple, fast, and free use.」
  来源：[tex.stackexchange.com/questions/750873](https://tex.stackexchange.com/questions/750873/alternative-to-overleaf-with-a-better-free-plan)
- Reddit r/LaTeX「Overleaf's new compilation timeout is a joke」：**171 upvotes / 70 comments**（数字来自搜索引擎摘要）。
  来源：[reddit.com/r/LaTeX/comments/1ndn0r3](https://www.reddit.com/r/LaTeX/comments/1ndn0r3/overleafs_new_compilation_timeout_is_a_joke/)
- Reddit r/LaTeX「Over-leaf will make me fail! How to migrate to another…」：用户直接引用报错「Your project exceeded the compile timeout limit on our free plan.」
  来源：[reddit.com/r/LaTeX/comments/1hhz9z7](https://www.reddit.com/r/LaTeX/comments/1hhz9z7/overleaf_will_make_me_fail_how_to_migrate_to/)
- Reddit r/LaTeX「Overleaf projects don't compile anymore since compile time got…」
  来源：[reddit.com/r/LaTeX/comments/1arbw21](https://www.reddit.com/r/LaTeX/comments/1arbw21/overleaf_projects_dont_compile_anymore_since/)
- Reddit r/LaTeX「Compile time overleaf exceeded for free plans」最高票建议是「装 TeX Live 用本地编辑器」。
  来源：[reddit.com/r/LaTeX/comments/18s5x5l](https://www.reddit.com/r/LaTeX/comments/18s5x5l/compile_time_overleaf_exceeded_for_free_plans_i/)
- Hacker News（2024，246 点/127 评论的 Overleaf 主题帖）高票评论：「my dissertation would no longer compile… **dissertations are so long they time out the build on the free plan**」——毕业后失去学校账号即无法再编译自己的博士论文。
  来源：[news.ycombinator.com/item?id=40834159](https://news.ycombinator.com/item?id=40834159)
- Hacker News（2026，TeXbrain 帖）评论：「I had to compile a rather heavy presentation template and **Overleaf would simply time-out**, Crixet being client based was the only option.」
  来源：[news.ycombinator.com/item?id=49448837](https://news.ycombinator.com/item?id=49448837)
- TeX SE「Your compile timed out [closed]」（2024-04）
  来源：[tex.stackexchange.com/questions/715221](https://tex.stackexchange.com/questions/715221/your-compile-timed-out)

**[推断]** 10 秒对「首次编译 + 参考文献多趟 + TikZ/pgfplots 重绘」的正常学位论文几乎不可能通过，所以免费版实际上是「只能写短文」的试用版，而不是可用的免费版。这是流失的第一驱动因素。

---

### ② 协作者限制收紧：免费版 1 人，2024-10-14 强制执行

**事实**

- 官方 2024-07-01 公告《Changes to project sharing》：「If you are on our free plan, **you can have one collaborator per project**」，并且通过 link sharing 获得编辑权的人**现在也计入协作者上限**；「All projects have the collaborator limit applied **as of October 14, 2024**… Any project where the number of editors is too high will have **all editors switched to view-only access**」。
  来源：[overleaf.com/blog/changes-to-project-sharing](https://www.overleaf.com/blog/changes-to-project-sharing)
- 官方《Premium features》协作者表格：Free **1**、Student 10、Standard 10、Pro unlimited；Viewer 全部 unlimited。
  来源：[docs.overleaf.com/…/premium-features](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features)
- 官方还写明降级后果：「You will be allowed only one collaborator on new and existing projects… **all collaborators will be moved to view-only permissions**」。
  来源：同上
- 用户侧反应：r/LaTeX「Free alternative to Overleaf」——「Just found out that Overleaf decided to limit the number of editors per document to two people if the creator is on a free plan. prices are completely unreasonable…」
  来源：[reddit.com/r/LaTeX/comments/1gdcii0](https://www.reddit.com/r/LaTeX/comments/1gdcii0/free_alternative_to_overleaf/)
- r/LaTeX「How to edit with more than 2 editors for free?」——「it only allows two editors and i need four」。
  来源：[reddit.com/r/LaTeX/comments/1j2vv03](https://www.reddit.com/r/LaTeX/comments/1j2vv03/how_to_edit_with_more_than_2_editors_for_free/)
- 第三方评测站（Capterra）用户评价同样复述「The free plan allows one collaborator per project」。
  来源：[capterra.com/p/229381/Overleaf](https://www.capterra.com/p/229381/Overleaf/)

**[推断]** 「先公告、再延期、最后强制把超额 editor 全部降级为只读」这种执行方式，比数字本身更容易引发信任流失——用户的核心项目在无预警的情况下失去编辑权。

---

### ③ 关键能力全部在付费墙后 + 价格（$199 / $399 每年）

**事实**

- 官方《Premium features》逐项列出仅付费可用的功能：**Git / GitHub 集成、Dropbox 双向同步、ReadCube/Mendeley/Zotero 引用管理器集成、real-time track changes、完整项目历史（免费版只有 24 小时历史）、Symbol Palette、优先支持、更长编译时间**。
  来源：[docs.overleaf.com/…/premium-features](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features)
- 定价（2026 年快照）：Student 免费 $0（1 collaborator、5 AI 次/天、unlimited projects）；Standard **$16.75/月（年付 $199/年）**或 $25/月；Pro **$33.25/月（年付 $399/年）**或 $45/月。付费版均为 "24x Basic compile timeout"。
  来源：[overleaf.com/user/subscription/plans](https://www.overleaf.com/user/subscription/plans)
- Hacker News Show HN（TeXbrain，2026-08，118 点）作者自述：「I have used Overleaf and that was fine **until I wanted to git sync, which unfortunately sits behind a paywall**. Since I didn't want to pay subscriptions for things that should simply just work, I built the editor I wanted.」
  来源：[news.ycombinator.com/item?id=49441375](https://news.ycombinator.com/item?id=49441375)
- r/LaTeX 讨论「Overleaf Pro: Worth it or not?」——社区共识偏「协作密集型工作流值，个人用户性价比不高」。
  来源：[reddit.com/r/LaTeX/comments/1lt8qro](https://www.reddit.com/r/LaTeX/comments/1lt8qro/overleaf_pro_worth_it_or_not_your_honest_opinion/)
- Trustpilot 上 Overleaf 评分为 **2.3 / 5（33 条评价）**（数字来自搜索引擎摘要，页面被反爬拦截无法直接抓取）。
  来源：[trustpilot.com/review/overleaf.com](https://www.trustpilot.com/review/overleaf.com)

**[推断]** 「Git 同步」在开发者心智里是基础设施级能力，被锁进订阅是最容易激发「我要自己造一个」的点——HN 上两个独立项目（TeXbrain、Crixet/Prism）都以此为动机。

---

### ④ Git 集成体验差（而且官方自己列了一长串「不支持」）

**事实**

- 官方 Git 集成文档《Known limitations》明确列出**不支持**：分支（branching）、标签（tags）、Git LFS、Git submodules、符号链接（会被转成普通文件）、执行权限、文件夹重命名（会残留旧空目录）；重命名/移动文件会变成 delete+create，**丢失 tracked changes 与 comments**；原文警告「**we do not recommend mixing active use of Git and the use of track changes and/or comments**」；还有 rate limiting 与大 commit 超时问题（建议把 `http.postBuffer` 调到 10 MB）。
  来源：[docs.overleaf.com/…/git-integration](https://docs.overleaf.com/integrations-and-add-ons/git-integration-and-github-synchronization/git-integration)
- Git 集成是 premium 功能：「For Overleaf cloud, Git integration is a premium feature」。
  来源：同上
- 使用 Git / GitHub sync 时，官方建议项目体积 ≤ **100 MB**（普通项目建议 ≤500 MB）。
  来源：[docs.overleaf.com/…/plan-limits](https://docs.overleaf.com/getting-started/free-and-premium-plans/plan-limits)
- 官方特性矩阵：**Community Edition 没有 Git integration、没有 commenting、没有 track changes、没有 Symbol Palette、没有 SSO**。
  来源：[docs.overleaf.com/…/server-pro-vs.-community-edition](https://docs.overleaf.com/on-premises/welcome/server-pro-vs.-community-edition)
- 第三方 issue（2020，长期未满足）：「Support git integration in the community edition」。
  来源：[github.com/overleaf/overleaf/issues/782](https://github.com/overleaf/overleaf/issues/782)
- r/LaTeX「Overleaf community edition git integration?」——「the git integration is a complete mess. I would advise you to avoid using overleaf for team work.」
  来源：[reddit.com/r/LaTeX/comments/hi9xct](https://www.reddit.com/r/LaTeX/comments/hi9xct/overleaf_community_edition_git_integration/)
- Stack Overflow「Overleaf does not in synch with git」——push 后网页端未反映改动。
  来源：[stackoverflow.com/questions/57659372](https://stackoverflow.com/questions/57659372/overleaf-does-not-in-synch-with-git)
- TeX SE「How to use git status, reset, revert with overleaf git project?」——只有 push/pull，缺完整 Git 能力。
  来源：[tex.stackexchange.com/questions/582286](https://tex.stackexchange.com/questions/582286/how-to-use-git-status-reset-revert-with-overleaf-git-project)

**[推断]** Overleaf 的 Git 是「用 Overleaf 的 history 系统反向翻译成 Git commit」的桥接层（官方原文承认「it doesn't allow you to work within Overleaf as if it was a complete Git implementation」），因此与真实 Git 工作流存在结构性冲突；对以 Git 为主工作流的团队，这是不可调和的。

---

### ⑤ 服务器排队 / 编译慢 / 服务不可用

**事实**

- Overleaf 官方 status 页（2026 年快照）：overleaf.com 5 个组件 **98.16% uptime**（Jun–Sep 2026），并有一条持续 2 天的 "Log In With Google" SSO 事故。
  来源：[status.overleaf.com](https://status.overleaf.com/)
- Reddit r/LaTeX「Thesis submission but overleaf is down....」
  来源：[reddit.com/r/LaTeX/comments/1h5mrun](https://www.reddit.com/r/LaTeX/comments/1h5mrun/thesis_submission_but_overleaf_is_down/)
- Reddit r/LaTeX「(Premium) Overleaf suddenly not compiling my project. No…」
  来源：[reddit.com/r/LaTeX/comments/1aodgrz](https://www.reddit.com/r/LaTeX/comments/1aodgrz/premium_overleaf_suddenly_not_compiling_my/)
- 迁移类博客（dev.to，2026-03）第一手叙述：「Overleaf hung. Not an error. Not a crash. Just... spinning. "Compiling" for **four minutes**. Then a timeout… **Overleaf was clearly under load.** Apparently thesis season is thesis season for everyone.」（注：该文为 TeX64 的商业推广内容，需打折看待，但描述的现象与上述多个独立来源一致）
  来源：[dev.to/boson_jp/…](https://dev.to/boson_jp/i-finally-ditched-overleaf-for-a-local-latex-editor-heres-what-actually-works-2cgg)
- 竞品 Typst 定价页引用的用户证言：「The one thing I always hated about Overleaf is that **it takes so much time to compile**」。
  来源：[typst.app/pricing](https://typst.app/pricing/)
- 第三方可用性监测站点（Downdetector / StatusGator / isdown 等）会记录「compile servers down」类用户报告。
  来源：[downdetector.com/status/overleaf](https://downdetector.com/status/overleaf/)、[statusgator.com/services/overleaf/overleaf](https://statusgator.com/services/overleaf/overleaf)

**[推断]** 在线编辑器把「截止日前夜」这个最高峰负载时刻与「所有用户同时要编译」绑定，是结构性的：负载尖峰无法靠扩容廉价解决，而本地编译的耗时与并发无关。

---

### ⑥ 无法真正离线：断网 = 停工

**事实**

- Overleaf 的离线方案是 Git/GitHub/Dropbox 同步（且都在付费墙后），官方描述为「This allows you to work offline and synchronize your files whenever you come back online」——即离线只是「同步」而非「在线编辑器的离线模式」。
  来源：[docs.overleaf.com/…/premium-features](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features)
- 官方明确：付费版时间也不够时「download your project and compile it locally」。
  来源：[docs.overleaf.com/…/fixing-and-preventing-compile-timeouts](https://docs.overleaf.com/troubleshooting-and-support/fixing-and-preventing-compile-timeouts)
- r/LaTeX「Offline review feature similar to Overleaf?」——用户明确想要「run completely offline」的批注/审阅方案。
  来源：[reddit.com/r/LaTeX/comments/1de9evq](https://www.reddit.com/r/LaTeX/comments/1de9evq/offline_review_feature_similar_to_overleaf/)
- r/LaTeX「Alternatives to Overleaf」高票回复：「Just use Crixet. Pretty much does everything Overleaf does but for free. **No sign up, can run even if your internet goes down.**」
  来源：[reddit.com/r/LaTeX/comments/1lqomjy](https://www.reddit.com/r/LaTeX/comments/1lqomjy/alternatives_to_overleaf/)
- r/LaTeX「Is there some offline premade alternative for Overleaf?」
  来源：[reddit.com/r/LaTeX/comments/11fjuxw](https://www.reddit.com/r/LaTeX/comments/11fjuxw/is_there_some_offline_premade_alternative_for/)
- 迁移博客把「runs entirely offline / works on trains, planes, cafes with spotty Wi-Fi」列为核心收益。
  来源：[dev.to/boson_jp/…](https://dev.to/boson_jp/i-finally-ditched-overleaf-for-a-local-latex-editor-heres-what-actually-works-2cgg)

---

### ⑦ 大项目卡顿（编辑器 lag + 慢包 + 图片）

**事实**

- TeX SE「Typing in Overleaf editor is laggy on Safari」（2024-12）：lag 在缩小预览后消失 → 与浏览器端渲染相关。
  来源：[tex.stackexchange.com/questions/733120](https://tex.stackexchange.com/questions/733120/typing-in-overleaf-editor-is-laggy-on-safari)
- Reddit r/Overleaf 多个 lag 主题：「Why is editing files in an overleaf project extremely slow?」「Why is overleaf lagging?」；r/LaTeX「Poor Overleaf performance」。
  来源：[reddit.com/r/Overleaf/comments/1fddab7](https://www.reddit.com/r/Overleaf/comments/1fddab7/why_is_editing_files_in_an_overleaf_project/)、[reddit.com/r/Overleaf/comments/w6350z](https://www.reddit.com/r/Overleaf/comments/w6350z/why_is_overleaf_lagging/)、[reddit.com/r/LaTeX/comments/k9torf](https://www.reddit.com/r/LaTeX/comments/k9torf/poor_overleaf_performance/)
- 官方承认一批「time-consuming practices and packages」会拖垮编译：TikZ/pgfplots 复杂图、`mhchem`、`acro` v3、EPS/SVG 转换（Ghostscript/Inkscape）、`tabularray`、`\tracingall`、递归命令等，并给出「换包/降版本/外部化图形」的规避建议。
  来源：[docs.overleaf.com/…/fixing-and-preventing-compile-timeouts](https://docs.overleaf.com/troubleshooting-and-support/fixing-and-preventing-compile-timeouts)
- 容量上限：单项目最多 **2,000 个文件**、可编辑内容 **7 MB**、单个可编辑文本文件 **2 MB**、单次上传 **50 MB**；超大项目建议 ≤500 MB（用 Git/GitHub sync 时 ≤100 MB），>4 GB 项目在复制等操作上会出问题。
  来源：[docs.overleaf.com/…/plan-limits](https://docs.overleaf.com/getting-started/free-and-premium-plans/plan-limits)

---

### ⑧ 自定义编译链 / 环境受限

**事实**

- 引擎与 TeX Live：Overleaf 用官方 TeX Live 年度发行版，**只能在 Overleaf 提供的版本里选**；想用最新滚动版必须加入 Labs 实验计划，且官方声明「**not tested by our team. We will not provide support for projects using this version**」；旧版本被归为 "Legacy" 且「may eventually be phased out」。
  来源：[docs.overleaf.com/…/tex-live](https://docs.overleaf.com/troubleshooting-and-support/tex-live)
- 自建（Community Edition）反而能力更弱：**无 Git、无 track changes、无 commenting、无 SSO**，且**没有 sandboxed compiles**，官方警告「Without sandboxing, LaTeX compiles run with the same privileges as the container… Non-sandboxed compiles should only be used in fully trusted environments」，并明确 CE「is **not** appropriate for scenarios where isolation of users is required」。
  来源：[docs.overleaf.com/…/server-pro-vs.-community-edition](https://docs.overleaf.com/on-premises/welcome/server-pro-vs.-community-edition)
- Server Pro（自托管商业版）与个人 Pro 订阅是**两个独立产品**：「An Overleaf Pro plan subscription will not grant you access to, or a license for, Overleaf Server Pro」，价格不公开，只能「Talk to Sales」。
  来源：同上、[overleaf.com/for/enterprises](https://www.overleaf.com/for/enterprises)
- shell-escape / 自定义外部工具链属于 TeX 生态常见需求，社区长期在 TeX SE 上询问如何在编辑器中启用（不同环境能力差异大）。
  来源：[tex.stackexchange.com/questions/598818](https://tex.stackexchange.com/questions/598818/how-can-i-enable-shell-escape)
- 官方博客显示 GitHub 直连同步在 **Server Pro 上不可用**：「A direct GitHub Synchronization is available as a third-party integration on our Overleaf cloud solution, but **isn't available for Server Pro**」。
  来源：[overleaf.com/blog/creating-connections-using-overleafs-git-integration-and-github](https://www.overleaf.com/blog/creating-connections-using-overleafs-git-integration-and-github)

**[推断]** 「想要本地可控」的机构并不天然有解：Overleaf 提供的自托管版本（CE）功能被砍到低于云端免费版体验，商业版（Server Pro）价格不透明且功能仍有缺口。这给第三方本地/自托管工具留出了明确空间。

---

### ⑨ 隐私 / 数据主权 / 合规

**事实**

- Overleaf 法务条款原文：「You acknowledge that if you wish to protect your transmission of data or files to Overleaf, **it is your responsibility to use a secure encrypted connection**…」——传输安全责任在用户。
  来源：[overleaf.com/legal](https://www.overleaf.com/legal)
- AI 功能会**把内容发给第三方（包括 OpenAI）**：官方《AI features》原文「Some AI features send content to third-party services, including OpenAI」；同时声明「Your data is never used to train AI models」。发送内容包括错误信息、相关代码行、文件名、选中的文本等。
  来源：[docs.overleaf.com/integrations-and-add-ons/ai-features](https://docs.overleaf.com/integrations-and-add-ons/ai-features)
- 归属与合规：Overleaf 隶属于 **Digital Science**（Holtzbrinck 集团），企业页自述 ISO/IEC 27001:2022 合规、遵守 GDPR/UKGDPR。
  来源：[overleaf.com/for/enterprises](https://www.overleaf.com/for/enterprises)、[digital-science.com/security-certifications](https://www.digital-science.com/security-certifications)
- 第三方（EU 竞品 inscrive.io）对 GDPR 的分析：核心风险是**美国关联所有权 + 数据跨境传输的 Schrems II / DPF 不确定性**，EU 高校需自行完成 DPA、数据存放地、传输机制、子处理者清单、AI 训练立场等核查。
  来源：[inscrive.io/articles/overleaf-gdpr](https://inscrive.io/articles/overleaf-gdpr)
  ⚠️ **来源偏见提示**：该站是竞品，其「Overleaf 不合规」的暗示不能当作事实；但其列举的**合规核查清单**与 Overleaf 的企业页/法务页事实可以交叉验证。所有权事实（Digital Science）以 Overleaf/Digital Science 官方页面为准。
- 学术社区长期存在「把未发表论文放上 Overleaf 是否安全」的担忧。
  来源：[academia.stackexchange.com/questions/164393](https://academia.stackexchange.com/questions/164393/is-writing-a-research-paper-on-overleaf-risky-for-plagiarism)
- 对照组：Typst 官方定价页明确「your data will be stored in a data center in **Germany**」，并提供 On-Premises 版「**No data ever leaves your infrastructure**」。
  来源：[typst.app/pricing](https://typst.app/pricing/)

**[推断]** 数据主权是最难用「功能」补偿的一类痛点：一旦机构的 DPO/采购把「第三国传输」列为红线，再好的协作体验也无法救场。

---

### ⑩ 免费版额度琐碎且不透明（历史 24h / AI 5 次 / 容量）

**事实**

- 免费版**项目历史只有 24 小时**（付费为完整历史）。
  来源：[docs.overleaf.com/…/premium-features](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features)
- 免费版 AI 额度：定价页写 **5 AI uses/day**，文档表格写 "Basic"，且**不含 AI assistant**；付费 Standard 10 次/天，Pro「Max（fair use applies）」。
  来源：[overleaf.com/user/subscription/plans](https://www.overleaf.com/user/subscription/plans)、[docs.overleaf.com/…/ai-features](https://docs.overleaf.com/integrations-and-add-ons/ai-features)
- 容量类限制：2,000 文件 / 7 MB 可编辑内容 / 2 MB 单文本文件 / 50 MB 单次上传；项目总量「无强制上限」但建议 ≤500 MB，Git/GitHub sync 时建议 ≤100 MB。
  来源：[docs.overleaf.com/…/plan-limits](https://docs.overleaf.com/getting-started/free-and-premium-plans/plan-limits)
- 免费版**没有 track changes**（表格中 Free = No）。
  来源：[docs.overleaf.com/…/premium-features](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features)
- 大量第三方站点以「Overleaf free limits」为 SEO 主题反复解释这些额度，本身即说明额度复杂度是普遍困惑点。
  来源：[letx.app/blog/overleaf-free-plan-limits-2026](https://letx.app/blog/overleaf-free-plan-limits-2026/)、[thelatexlab.com/blog/overleaf-compile-timeout-fix](https://thelatexlab.com/blog/overleaf-compile-timeout-fix/)

---

## 2. 免费版 vs 付费版：具体限制数字（含来源）

| 维度 | Free / Student(免费) | Standard | Pro | 来源 |
|---|---|---|---|---|
| 编译超时 | **10 秒** | **240 秒**（24×） | **240 秒**（24×） | [plan-limits](https://docs.overleaf.com/getting-started/free-and-premium-plans/plan-limits) |
| 协作者（Editor/Reviewer） | **1 人/项目** | 10 人/项目 | 不限 | [premium-features](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features) |
| Viewer | 不限 | 不限 | 不限 | 同上 |
| 项目数 | 不限 | 不限 | 不限 | [plan-limits](https://docs.overleaf.com/getting-started/free-and-premium-plans/plan-limits) |
| 每项目文件数 | 2,000 | 2,000 | 2,000 | 同上 |
| 可编辑内容总量 | 7 MB | 7 MB | 7 MB | 同上 |
| 单个可编辑文本文件 | 2 MB | 2 MB | 2 MB | 同上 |
| 单次上传 | 50 MB | 50 MB | 50 MB | 同上 |
| 项目总量 | 无强制上限（建议 ≤500 MB；Git/GitHub sync ≤100 MB；>4 GB 有已知问题） | 同 | 同 | 同上 |
| 项目历史 | **仅 24 小时** | 完整 | 完整 | [premium-features](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features) |
| Track changes | **无** | 有 | 有 | 同上 |
| Git / GitHub / Dropbox / Zotero / Mendeley / ReadCube | **无** | 有 | 有 | 同上 |
| Symbol Palette | **无** | 有 | 有 | 同上 |
| AI 额度 | 5 次/天，**无 AI assistant** | 10 次/天，含 assistant | Max（fair use），含 assistant | [pricing](https://www.overleaf.com/user/subscription/plans)、[ai-features](https://docs.overleaf.com/integrations-and-add-ons/ai-features) |
| 价格 | $0 | $16.75/月（年付 **$199/年**）/ $25 月付 | $33.25/月（年付 **$399/年**）/ $45 月付 | [pricing](https://www.overleaf.com/user/subscription/plans) |
| Server Pro（自托管） | — | 价格不公开，需联系销售 | 与个人 Pro 订阅**互相独立**、不互通 | [server-pro-vs-ce](https://docs.overleaf.com/on-premises/welcome/server-pro-vs.-community-edition) |

> ⚠️ 数字一致性提示：抓取到的官方文档写 **10 秒**；但部分第三方博客（如 thelatexlab，2026-06）仍写「免费版 20 秒」，属于过时信息。以官方文档为准。

**竞品对照（免费额度）**

| 平台 | 免费额度要点 | 来源 |
|---|---|---|
| Papeeria | Epsilon 免费版：公共项目与协作者不限、**仅 1 个活跃私有项目**、Git 同步仅限公开仓库、编辑历史 24h；Delta 起 **$5/月**：10 个私有项目、自动/优先编译、私有仓库、30 天历史、Dropbox/Google Drive 同步 | [papeeria.com/about/pricing](https://papeeria.com/about/pricing) |
| Papeeria 编译超时 | 官方文档：「By default, compilation times out in **paid** projects in **120 seconds**. This period can be increased on per-project basis on demand.」 | [docs.papeeria.com/compiler/timeout](http://docs.papeeria.com/compiler/timeout/) |
| Typst Web App | 免费 $0：200 MB 存储、每项目 100 文件；Pro **$7.99/月**：2 GB、1,000 文件、GitHub/GitLab 同步（实验性）、Zotero/Mendeley；On-Premises 定制报价 | [typst.app/pricing](https://typst.app/pricing/) |
| Authorea | 免费版历史上限约 **10 篇文档**（2021 年第三方评测，时效性存疑） | [shantoroy.com/…](https://shantoroy.com/latex/authorea-dot-com-an-alternative-to-overleaf-researchgate-arxiv/) |

---

## 3. 2023 年以来「限制收紧」的时间线与反弹规模

### 时间线（均为有来源的官方动作）

| 时间 | 事件 | 来源 |
|---|---|---|
| 2023-10 | 官方宣布**把免费版编译超时降到 20 秒**；社区在 GitHub issue 中记录该公告链接（原博客 URL 现已 404） | [HagenbergThesis#163](https://github.com/Digital-Media/HagenbergThesis/issues/163) |
| 2023-12 | r/LaTeX 出现「Compile time overleaf exceeded for free plans」求助帖 | [reddit 18s5x5l](https://www.reddit.com/r/LaTeX/comments/18s5x5l/compile_time_overleaf_exceeded_for_free_plans_i/) |
| 2024-07-01 | 官方《Changes to project sharing》：link sharing 的 editor 计入上限，免费版 **1 名协作者** | [blog](https://www.overleaf.com/blog/changes-to-project-sharing) |
| 2024-08-26（原定 7-29，后延期） | 开始把超限项目的 editor 降级为 view-only | 同上（公告内 Edit 说明） |
| 2024-10-14 | **强制执行完成**：所有项目适用协作者上限，超限项目全部 editor 变 view-only | 同上 |
| 2025-08-27 | TeX 社区聊天记录讨论「Overleaf again reduce the free compilation time」；同一串里有人说「it seems that the timeout has been 10 sec since 2023」 | [chat.stackexchange.com/transcript/message/68192131](https://chat.stackexchange.com/transcript/message/68192131) |
| 2025-09 | TeX SE 出现「Alternative to overleaf (with a better free plan)」，1,812 浏览 | [tex.se 750873](https://tex.stackexchange.com/questions/750873/alternative-to-overleaf-with-a-better-free-plan) |
| 2026（抓取快照） | 官方文档免费版超时 = **10 秒** | [plan-limits](https://docs.overleaf.com/getting-started/free-and-premium-plans/plan-limits) |

**未能核实的两点（明确说明没找到）**
- **60 秒 → 20 秒之前的历史基线**：我没有找到可核实的一手来源证明免费版曾经是 60 秒（只能确认 2023-10 的 20 秒与当前的 10 秒）。
- **20 秒 → 10 秒的确切变更日期/公告**：`overleaf.com/blog/changes-to-free-compile-timeouts-and-servers` 现已 **404**；archive.org 在本环境不可访问。只能由社区聊天记录推断「2023 年以来就是 10 秒」。

### 反弹规模（能量化的部分）

| 指标 | 数值 | 来源 |
|---|---|---|
| Reddit「Overleaf's new compilation timeout is a joke」 | **171 upvotes / 70 comments** | [reddit 1ndn0r3](https://www.reddit.com/r/LaTeX/comments/1ndn0r3/overleafs_new_compilation_timeout_is_a_joke/) |
| TeX SE「Alternative to overleaf (with a better free plan)」 | **1,812 views**（被标记 duplicate 关闭） | [tex.se 750873](https://tex.stackexchange.com/questions/750873/alternative-to-overleaf-with-a-better-free-plan) |
| HN 2024 Overleaf 主题帖 | **246 points / 127 comments** | [hn 40832930](https://news.ycombinator.com/item?id=40832930) |
| HN 2026 TeXbrain（因 Git 付费墙而自建） | **118 points / 28 comments** | [hn 49441375](https://news.ycombinator.com/item?id=49441375) |
| Trustpilot 评分 | **2.3 / 5（33 条）** | [trustpilot](https://www.trustpilot.com/review/overleaf.com) |

**规模判断**
- **事实**：无法获得 Overleaf 的流失/续订数据（官方不公开）。可观测的「反弹规模」集中在社区讨论热度与第三方替代品的出现频率上。
- **事实**：r/LaTeX 上「Alternatives to Overleaf」「Building an overleaf alternative from scratch because I can」「Best online LaTeX editor for my needs」等主题在 2025–2026 持续出现，且出现多个「自己造一个」的项目帖。
  来源：[1lqomjy](https://www.reddit.com/r/LaTeX/comments/1lqomjy/alternatives_to_overleaf/)、[1m5f9zo](https://www.reddit.com/r/LaTeX/comments/1m5f9zo/building_an_overleaf_alternative_from_scratch/)、[1lka6x4](https://www.reddit.com/r/LaTeX/comments/1lka6x4/best_online_latex_editor_for_muy_needs_overleaf/)
- **[推断]** 反弹「有热度但未形成大规模迁移」：Overleaf 的护城河是模板库、协作链接与机构账号，而不是编译能力。真正流失的是**没有机构订阅的个人**（学生、独立研究者、小团队），以及**有强合规/离线要求**的机构用户。

---

## 4. 在线方案的结构性劣势（编译速度 / 隐私 / 离线 / 可扩展）

### 4.1 编译速度：把「算力」变成订阅商品，且与峰值负载耦合

- **事实**：编译时间被明确切成 10s / 240s 两档（[plan-limits](https://docs.overleaf.com/getting-started/free-and-premium-plans/plan-limits)），官方承认存在「付费版也不够、必须本地编译」的项目（[timeouts doc](https://docs.overleaf.com/troubleshooting-and-support/fixing-and-preventing-compile-timeouts)）。
- **事实**：服务器侧会因负载出现长时间等待/超时（[dev.to 叙述](https://dev.to/boson_jp/i-finally-ditched-overleaf-for-a-local-latex-editor-heres-what-actually-works-2cgg)、[HN 评论](https://news.ycombinator.com/item?id=49448837)）；服务可用性有官方 uptime 记录（[status](https://status.overleaf.com/)）。
- **[推断]** 本地编译的耗时只取决于本机 CPU 与文档复杂度，与「今天是不是交稿季」无关；且可以无限重跑、无超时。这是本地方案唯一无法被云端抹平的硬优势。

### 4.2 隐私 / 数据主权：跨境与第三方处理是结构性负担

- **事实**：内容可能被发送给 OpenAI 等第三方（[ai-features](https://docs.overleaf.com/integrations-and-add-ons/ai-features)）；传输安全责任在用户（[legal](https://www.overleaf.com/legal)）；公司归属 Digital Science（[enterprises](https://www.overleaf.com/for/enterprises)）。
- **事实**：自托管商业版 Server Pro 才提供沙箱编译与 SSO，而免费自托管版（CE）连 Git 都没有（[server-pro-vs-ce](https://docs.overleaf.com/on-premises/welcome/server-pro-vs.-community-edition)）。
- **[推断]** 「数据不出机器」是本地方案在合规上的最强卖点，尤其对 EU 公共机构、涉密/涉个人数据研究、企业专利文档。

### 4.3 离线：在线编辑器没有真正的离线编辑模式

- **事实**：Overleaf 的离线能力 = Git/GitHub/Dropbox **同步**，且是付费功能（[premium-features](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features)）；社区持续询问离线方案（[1de9evq](https://www.reddit.com/r/LaTeX/comments/1de9evq/offline_review_feature_similar_to_overleaf/)）。
- **[推断]** 对经常出差/野外/网络受限环境的用户（田野调查、船上、飞机、内网机房），在线方案的可用性等于网络可用性。

### 4.4 可扩展 / 可控性：你无法控制编译器、包与工具链

- **事实**：只能选 Overleaf 提供的 TeX Live 版本，最新滚动版无支持、旧版最终会退役（[tex-live](https://docs.overleaf.com/troubleshooting-and-support/tex-live)）；Git 能力被官方列出一长串不支持项（[git-integration](https://docs.overleaf.com/integrations-and-add-ons/git-integration-and-github-synchronization/git-integration)）；CE 缺少沙箱与关键协作功能（[server-pro-vs-ce](https://docs.overleaf.com/on-premises/welcome/server-pro-vs.-community-edition)）。
- **[推断]** 需要自定义 `.sty/.cls`、shell-escape、外部工具（biber/inkscape/ghostscript 版本控制）、CI 编译流水线的团队，最终必然要在本地或自建 CI 里跑编译——在线编辑器只能作为「写作前端」。

---

## 5. 哪些用户群体会为本地/离线能力离开 Overleaf

**有来源支撑的迁移动机**

| 用户群 | 迁移动机（来源证据） |
|---|---|
| 写学位论文的研究生（免费版 / 已毕业失去机构账号） | 论文太长必然超时；毕业后失去机构账号后自己的论文**再也编译不了**（[HN 评论](https://news.ycombinator.com/item?id=40834159)）；r/LaTeX「will make me fail」（[1hhz9z7](https://www.reddit.com/r/LaTeX/comments/1hhz9z7/overleaf_will_make_me_fail_how_to_migrate_to/)） |
| 需要 Git 工作流的开发者型研究者/技术写作者 | Git 同步在付费墙后（[HN TeXbrain](https://news.ycombinator.com/item?id=49441375)）；Git 集成缺分支/标签/LFS 且与 track changes 冲突（[官方 limitations](https://docs.overleaf.com/integrations-and-add-ons/git-integration-and-github-synchronization/git-integration)） |
| 重图形/重计算的作者（TikZ、pgfplots、大型图片） | 官方列出这些是超时主因，规避手段是降画质或本地编译（[timeouts doc](https://docs.overleaf.com/troubleshooting-and-support/fixing-and-preventing-compile-timeouts)） |
| 无机构订阅的个人 / 小团队 | 价格与额度不成比例，社区评价「prices are completely unreasonable」（[1gdcii0](https://www.reddit.com/r/LaTeX/comments/1gdcii0/free_alternative_to_overleaf/)）、Pro 对个人性价比不高（[1lt8qro](https://www.reddit.com/r/LaTeX/comments/1lt8qro/overleaf_pro_worth_it_or_not_your_honest_opinion/)） |
| 有数据主权/合规要求的机构（EU 高校、政府、企业专利/法务） | GDPR 跨境传输核查负担、需要 DPA 与数据落地地（[inscrive 分析](https://inscrive.io/articles/overleaf-gdpr)，竞品来源）+ Overleaf 自身法务条款（[legal](https://www.overleaf.com/legal)） |
| 网络受限/离线环境用户 | 田野、差旅、内网；社区持续求「完全离线」方案（[1de9evq](https://www.reddit.com/r/LaTeX/comments/1de9evq/offline_review_feature_similar_to_overleaf/)、[dev.to](https://dev.to/boson_jp/i-finally-ditched-overleaf-for-a-local-latex-editor-heres-what-actually-works-2cgg)） |
| macOS/Windows 桌面原生体验偏好者 | 迁移叙述里明确提到「想要像样的原生 App、不想花周末配置 VS Code + LaTeX Workshop」（[dev.to](https://dev.to/boson_jp/i-finally-ditched-overleaf-for-a-local-latex-editor-heres-what-actually-works-2cgg)） |

**[推断]** 反之，**不会**离开的核心人群是：导师-学生之间的即时协作（评论/track changes 是刚需）、需要机构模板库与一键分享链接的人、以及只需要写短文档的轻度用户。

---

## 6. 在线 vs 本地：取舍结论

### 在线方案不可替代的价值（**事实**）
1. **零安装上手**：「nothing to install」、注册即用（[free-and-premium-plans](https://docs.overleaf.com/getting-started/free-and-premium-plans)）。
2. **真·实时协作**：多人同时编辑同一份源码、评论、track changes（[premium-features](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features)）。
3. **模板库与分享链接**：巨大的模板库、view-only 链接（[blog](https://www.overleaf.com/blog/changes-to-project-sharing)）。
4. **免维护 TeX 发行版**：5,000+ 包开箱可用（[tex-live](https://docs.overleaf.com/troubleshooting-and-support/tex-live)）。

### 在线方案的结构性代价（**事实 + 推断**）
| 维度 | 在线 | 本地 |
|---|---|---|
| 编译时长 | 硬上限 10s/240s，官方承认有项目必须本地编译 | 仅受本机性能限制，无超时（**[推断]**） |
| 峰值负载 | 与全站用户共享编译资源，交稿季排队（**[推断]**，有用户叙述佐证） | 不受他人影响（**[推断]**） |
| 离线 | 无真正离线编辑，仅付费同步 | 天然离线（**事实**：离线是本地方案的核心卖点） |
| 隐私 | 数据出境 + AI 送第三方（**事实**） | 数据不出机器（**事实**：Typst On-Premises 亦以此为卖点） |
| 可扩展 | 编译器/包/版本受平台约束（**事实**） | 完全可控（**推断**） |
| 协作 | 强（**事实**） | 弱，需 Git/共享盘替代（**事实**：迁移叙述承认「实时协作不存在」） |

### 落地建议（**[推断]**，供产品定位参考）
1. **不要和 Overleaf 拼「协作」**，拼「编译自由 + 离线 + 隐私 + 大项目」。
2. **切入人群优先级**：写学位论文的研究生（免费版超时受害者）→ 需要 Git 的技术型作者 → 有合规要求的机构。
3. **必须补齐的对照能力**：无超时编译、真实的增量/多趟编译、SyncTeX 双向定位的**响应速度**（本地天然占优）、大项目编辑器不卡顿。
4. **协作不必自建**：以 Git/文件系统为真相源，让协作发生在 Git 上（这恰好是 Overleaf 官方承认做不好的地方）。
5. **合规叙事**：明确「数据不出本机」是最强差异化，对应 EU/涉密场景。

---

## 7. 调研局限（明确没找到的东西）

1. **Overleaf 官方流失率 / 涨价幅度数据**：不存在公开来源。所谓「涨价」我只找到**额度收紧**与当前价格，**没有找到可核实的历次价格上调公告或幅度**。
2. **`changes-to-free-compile-timeouts-and-servers` 原始公告**：URL 现已 404，archive.org 在本环境不可访问，因此 20s→10s 的确切时间点无法用一手来源确认。
3. **60 秒基线**：未找到可核实来源。
4. **Reddit 原文**：本环境对 `www.reddit.com` 的抓取被网络策略拦截（解析到非公网 IP），所有 Reddit 证据均为**搜索引擎摘要级**，引文可能与原文有细微出入；已逐条标注。
5. **Trustpilot / StatusGator / TeX SE 页面**：被反爬或 403 拦截，数值来自搜索摘要。
6. **Server Pro 价格**：官方不公开，仅「Talk to Sales」，无法给出数字。
7. **Authorea 现状**：仅找到 2021 年的第三方评测（免费限 10 篇），未能获取官方当前定价页，时效性不足。
