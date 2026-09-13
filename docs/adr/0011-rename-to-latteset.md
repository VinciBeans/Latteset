# 项目更名：TeXPresso → Latteset

项目正式名原为 **TeXPresso**。**GitHub 用户 [@mathlab08](https://github.com/mathlab08) 指出本项目名与既有的 [let-def/texpresso](https://github.com/let-def/texpresso) 相撞**——后者是 OCaml 实现、改造 XeTeX 做 LaTeX live rendering + 错误报告的上游项目（有配套 [VS Code 扩展](https://marketplace.visualstudio.com/items?itemName=DominikPeters.texpresso-basic)，2024 年在 r/LaTeX 有社区讨论）。**感谢 @mathlab08 的提醒**，这条提醒是本次更名的直接起因。

两者**领域相同、名称仅大小写不同**（`texpresso` vs `TexPresso`），属最坏情况的撞名：仓库搜索永远排在对方之后，用户与包管理器会把两者混淆，本项目自身差异化（Tauri 桌面 IDE、连续分页预览、中文友好、headless CLI/MCP）全部被淹没。

因此更名为 **Latteset**（**Latte + Typeset**）——"一杯拿铁的时间，排版已经跟上"，接住原有"即时感"的命名意图（原 espresso 隐喻），同时把「排版」直接写进名字。

- **命名核查（2026-09 实测）**：`latteset` 在 GitHub / npm / crates.io / PyPI 均未被占用；搜索引擎无同名软件产品（仅有无关的 Roblox 饰品名、家具套装等）。对照排除项：`texpress`（npm 被 2014 年工具占用）、`typepress`（[alitrack/typepress](https://github.com/alitrack/typepress) 纯 Rust HTML→PDF 引擎）、`ristretto`（Scala 生态知名缓存库 + npm 占用）、`cotex`（读音近卫生巾品牌）。
- **标识符映射**：
  | 位置 | 旧 | 新 |
  |---|---|---|
  | 产品名 / 窗口标题 / `productName` | `TeXPresso` | `Latteset` |
  | Tauri `identifier` | `com.texpresso.app` | `com.latteset.app` |
  | 项目内设置目录 | `.texpresso/settings.json` | `.latteset/settings.json` |
  | crate | `texpresso-core` / `-infra` / `-server` | `latteset-core` / `-infra` / `-server` |
  | 二进制 | `texpresso-cli` / `texpresso-mcp` | `latteset-cli` / `latteset-mcp` |
  | Rust use 路径 | `texpresso_core::…` | `latteset_core::…` |
  | 环境变量 | `TEXPRESSO_CONFIG_DIR` | `LATTESET_CONFIG_DIR` |
  | 前端自动打开钩子 | `VITE_TEXPRESSO_PROJECT` | `VITE_LATTESET_PROJECT` |
  | MCP `serverInfo.name` | `texpresso` | `latteset` |
- **纪律：区分「本项目名」与「上游项目名」**。文档中描述上游 TeXPresso 的内容**必须保持原拼写**，不得随更名一起替换——否则会写出不存在的 URL（`github.com/let-def/latteset`）与错误的源码符号名（`texpresso_protocol.c` / `texpresso_fork_with_channel` 是上游 C 源码里的真实符号，小写）。同理，上游方案通读文档 [`texpresso-live-rendering-roadmap.md`](./texpresso-live-rendering-roadmap.md) **保留原文件名**（它是「对上游的通读」，不是本项目文档）。
- **破坏性变更（已接受）**：`identifier` 变更导致应用配置目录从 `%APPDATA%\com.texpresso.app` 变为 `%APPDATA%\com.latteset.app`，项目内设置目录名同步变更。当前为 0.1.0、无发布用户，故不提供自动迁移；旧目录残留可手动删除或改名复用。
- **遗留项**：`docs/diagrams/*.svg` 与 `.github/assets/Latteset运行主界面.png` 是渲染产物，内部文字仍为旧名，需在有 mermaid 打包产物 + 无头浏览器的环境重跑 `node scripts/render-diagrams.mjs` 并重截主界面图。

- **状态**：已接受（2026-09）
- **备选方案**：
  - 保留 TeXPresso、仅靠描述区分——否决：同名同领域，搜索与包管理层面无法区分，属于长期品牌债；
  - 换成 `Typeccino` ——否决：字形近似拼写错误，中文用户读音不友好；
  - 换成 `Beanpress` ——否决：英国已有同名咖啡公司（[beanpress.co.uk](https://beanpress.co.uk/)），且与「排版」无语义关联；
  - 加后缀区分（如 `TexPresso-IDE`）——否决：仍以对方名称为前缀，搜索可见性无法改善。
