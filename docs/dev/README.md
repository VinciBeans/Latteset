# 开发者文档导航

改 Latteset 代码的入口：先按这份装环境、认代码地图，再按需跳模块契约 / 测试 / 设计文档。产品定位与用户文档见仓库根 [README.md](../../README.md)。

## 上手

**环境前置**：Windows 10/11（首发平台）、Node ≥18、Rust **msvc** toolchain、WebView2 Runtime（Win11 自带）、**vcpkg**（内嵌 Tectonic 的 C 链：`x64-windows-static-release` 六端口）。**所有构建（含 `npm run tauri dev` 与 CI）都需要 vcpkg 环境**——统一入口是 `scripts/with-tectonic-lib.ps1`，先用 `npm run lib:check` 自检（缺项给可执行修复命令）。**不需要** TeX Live / MiKTeX，也**不要求 PATH 上有 `tectonic`**（子进程形态已删除，ADR-0015 决策 2；`synctex.exe` 只留给复核脚本）。发行形态：**随包精简宏包集**（`src-tauri/resources/tectonic-bundle.zip`，517 文件 / 30.43 MiB）⇒ 离线开箱；许可资源 `src-tauri/resources/licenses/`（48.95 MiB）随安装包；**T19 终版安装包 NSIS 88.46 MiB / MSI 109.63 MiB（≤200 MB 门禁）**。本机若在受限沙箱会话（工作区带 Low 完整性标签）里构建，另需把 `TMP`/`TEMP` 指到工作区内目录，见 [../troubleshooting.md](../troubleshooting.md)。

```bash
npm install
npm run lib:check      # 校验 vcpkg / triplet / 端口 / 工具链
npm run tauri dev      # 构建 Rust + 起 vite + 真窗口（GUI 测试必须用它）
```

| 常用命令 | 作用 |
|---|---|
| `npm run tauri dev` | 真机窗口（改代码后验收 GUI 走这条；唯一入口会先校验 vcpkg 前置） |
| `npm run tauri build` | 打包（NSIS 冒烟，验证 ADR-0003 分发路径） |
| `npm run dev` | 只起 vite（`localhost:1420`），**不**调起窗口 |
| `npm run build` | `copy:pdf-worker` + `vue-tsc --noEmit` + `vite build`（交付前类型检查） |
| `npm run test` / `test:watch` | 前端 vitest |
| `npm run lib:check` | 构建前置自检（vcpkg / triplet / 端口 / 工具链） |
| `npm run lib:test` | 全仓 Rust 测试（core / infra / server / tectonic 四个 crate） |
| `npm run lib:cli` | release 构建 headless CLI（`latteset-cli`） |
| `node scripts/bench-embedded.mjs` | 六档性能基线（release CLI；见 [testing.md](./testing.md)「性能基线」） |
| `node scripts/make-bundle.mjs --max-zip-mb=120` + `--verify` | 打随包精简集 zip（确定性、自校验、体积门禁） |
| `node scripts/gen-bench-bundle.mjs --phase=all` | 生成/补齐测试用 bundle 源目录（联网按需取文件，不入库） |
| `node scripts/make-notices.mjs --check` | 许可资源离线一致性复核（CI 门禁；`--collect` 重采集） |
| `cargo test -p latteset-core \| -p latteset-infra` | 不依赖 C 链的两层，日常最快反馈（见 [testing.md](./testing.md)） |

## 代码地图

```
├── src-tauri/             # Tauri 壳：命令面（commands）、事件契约与装配（events / lib）
│                          #   runner.rs 是编译 runner 的薄壳：每趟读设置重建内嵌 runner
│                          #   resources/ 随安装包资源：tectonic-bundle.zip（精简宏包集）+ licenses/（许可材料）
├── crates/latteset-core   # 核心（纯逻辑，可测）：编译调度器、.log 解析与诊断、设置模型与合并、
│                          #   根文件探测、文档大纲、SyncTeX 纯逻辑、XDV 页哈希
├── crates/latteset-infra  # 基础设施（无 Tauri）：文件系统、synctex CLI、notify 监视、设置存储
├── crates/latteset-server # 无 GUI 交互层：headless Session + latteset-cli + latteset-mcp
├── crates/latteset-tectonic # 内嵌 Tectonic 引擎（唯一形态，C 链，见 tectonic-library.md）
│                          #   bundle.rs：来源解析（显式/env/随包/在线）、预热零网络判定、缓存 GC
├── src/                   # Vue 前端：stores（Pinia）/ 组件 / Monaco 语法 / IPC 服务层
├── docs/                  # 本目录（开发文档）+ design.md / adr/ / archive/
├── public/                # 图标资源与 pdf.js 产物（cmaps/worker，npm 脚本生成）
└── scripts/               # 工具脚本（pdf.js 拷贝、logo 导出、架构图渲染、bench/确定性/synctex 报告、打包/许可）
```

**关键约束**（改代码前先认这两条）：

- **`latteset-infra` 是外部依赖与文件系统的唯一落点**：上层（`src-tauri` / `latteset-server`）不得直接调 `tokio::fs` / `std::fs` / `std::process`（ADR-0010）。
- **文件系统是内容真相源**（ADR-0007），命令面做路径校验；编辑器缓冲区与磁盘不一致时以磁盘为准的那套分支有专门的冲突判据。

事件单向分发与"两条入口（GUI + headless）共用 core + infra"的架构要点见 [../design.md](../design.md) 与 [../adr/](../adr/)；内嵌引擎的构建前置、装配点与能力面见 [tectonic-library.md](./tectonic-library.md)。

## 文档入口

| 文档 | 内容 |
|---|---|
| [modules.md](./modules.md) | 模块实现契约、当前行为基线、已知债与验证入口 |
| [copy-guide.md](./copy-guide.md) | **用户文案规范**：8 条硬规则、术语唯一说法/禁用说法、可见串 grep 检查方法 |
| [testing.md](./testing.md) | 怎么跑测试、真机验收清单、真机探针手法 |
| [tectonic-library.md](./tectonic-library.md) | 内嵌 Tectonic（唯一形态）：构建/打包入口、装配点、随包精简集、能力面与验证入口 |
| [tectonic-licensing.md](./tectonic-licensing.md) | 分发许可审计：Rust/C 依赖与随包精简集的许可证、义务清单、G-L1..G-L9 门禁（全绿）+ R-L10 残余风险 |
| [../design.md](../design.md) | 设计与架构（含延迟预算实测） |
| [../adr/](../adr/) | 决策记录（ADR） |
| [../troubleshooting.md](../troubleshooting.md) | 操作与环境坑 |
| [../cli.md](../cli.md) | headless CLI / MCP 使用 |
| [../archive/](../archive/) | 已归档原文（含调研证据）与归档索引 |

AI 协作规范（工作流、验证纪律、禁止项）见 [../../AGENTS.md](../../AGENTS.md)。
