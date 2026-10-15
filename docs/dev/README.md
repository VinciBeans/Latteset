# 开发者文档导航

改 Latteset 代码的入口：先按这份装环境、认代码地图，再按需跳模块契约 / 测试 / 设计文档。产品定位与用户文档见仓库根 [README.md](../../README.md)。

## 上手

**环境前置**：Windows 10/11（首发平台）、Node ≥18、Rust **msvc** toolchain、WebView2 Runtime（Win11 自带）；跑真实编译还需 **TeX Live** 或 **MiKTeX**（可选，缺了只能用 Tectonic 库形态或看错误列表）。库形态另需 vcpkg（见 [tectonic-library.md](./tectonic-library.md)）。

```bash
npm install
npm run tauri dev      # 构建 Rust + 起 vite + 真窗口（GUI 测试必须用它）
```

| 常用命令 | 作用 |
|---|---|
| `npm run tauri dev` | 真机窗口（改代码后验收 GUI 走这条） |
| `npm run dev` | 只起 vite（`localhost:1420`），**不**调起窗口 |
| `npm run build` | `copy:pdf-worker` + `vue-tsc --noEmit` + `vite build`（交付前类型检查） |
| `npm run tauri build` | 打包（NSIS 冒烟，验证 ADR-0003 分发路径） |
| `npm run test` / `test:watch` | 前端 vitest |
| `cargo test -p latteset-core \| -p latteset-infra \| -p latteset-server` | 分层 Rust 测试（见 [testing.md](./testing.md)） |
| `npm run lib:check / lib:dev / lib:test / lib:cli / lib:build` | 库形态（vcpkg 环境）的 check / 开发 / 测试 / CLI / 发布构建 |
| `cargo build -p latteset-server` | headless 两个二进制（[cli.md](../cli.md)） |

## 代码地图

```
├── src-tauri/             # Tauri 壳：命令面（commands）、事件契约与装配（events / lib）
├── crates/latteset-core   # 核心（纯逻辑，可测）：编译调度器、.log 解析与诊断、设置模型与合并、
│                          #   根文件探测、文档大纲、SyncTeX 纯逻辑、XDV 页哈希
├── crates/latteset-infra  # 基础设施（无 Tauri）：文件系统、latexmk/synctex 进程、notify 监视、设置存储
├── crates/latteset-server # 无 GUI 交互层：headless Session + latteset-cli + latteset-mcp
├── crates/latteset-tectonic # Tectonic 库内嵌形态（可选特性；见 tectonic-library.md）
├── src/                   # Vue 前端：stores（Pinia）/ 组件 / Monaco 语法 / IPC 服务层
├── docs/                  # 本目录（开发文档）+ design.md / adr/ / archive/
├── public/                # 图标资源与 pdf.js 产物（cmaps/worker，npm 脚本生成）
└── scripts/               # 工具脚本（pdf.js 拷贝、logo 导出、架构图渲染、bench/确定性/synctex 报告）
```

**关键约束**（改代码前先认这两条）：

- **`latteset-infra` 是外部依赖与文件系统的唯一落点**：上层（`src-tauri` / `latteset-server`）不得直接调 `tokio::fs` / `std::fs` / `std::process`（ADR-0010）。
- **文件系统是内容真相源**（ADR-0007），命令面做路径校验；编辑器缓冲区与磁盘不一致时以磁盘为准的那套分支有专门的冲突判据。

事件单向分发与"两条入口（GUI + headless）共用 core + infra"的架构要点见 [../design.md](../design.md) 与 [../adr/](../adr/)。

## 文档入口

| 文档 | 内容 |
|---|---|
| [modules.md](./modules.md) | 模块实现契约、当前行为基线、已知债与验证入口 |
| [testing.md](./testing.md) | 怎么跑测试、真机验收清单、真机探针手法 |
| [tectonic-library.md](./tectonic-library.md) | Tectonic 库形态：构建、bundle、两形态差异 |
| [../design.md](../design.md) | 设计与架构（含延迟预算实测） |
| [../adr/](../adr/) | 决策记录（ADR） |
| [../troubleshooting.md](../troubleshooting.md) | 操作与环境坑 |
| [../cli.md](../cli.md) | headless CLI / MCP 使用 |
| [../archive/](../archive/) | 已归档原文（含调研证据）与归档索引 |

AI 协作规范（工作流、验证纪律、禁止项）见 [../../AGENTS.md](../../AGENTS.md)。
