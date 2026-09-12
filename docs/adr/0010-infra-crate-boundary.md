# 基础设施层独立成 crate：texpresso-infra

texpresso-core 只定义接口（FileSystem / CompileRunner / SyncTexProvider），但实现它们要碰 OS API 与外部进程。这些实现原先散在 src-tauri 内，"接线薄壳"因此同时也是"文件系统 + 进程 + 监视 + 设置存储"的落点：命令面直接 `tokio::fs::canonicalize` / `std::fs::read_to_string`，notify 与 tauri::AppHandle 混在同一模块。上层逻辑与外部依赖强耦合，换平台或换实现要动命令层，测试也只能靠真实文件系统。

因此新增第三个 crate **texpresso-infra**（与 texpresso-core 同级），把外部依赖与文件系统的落点全部收进来。

- **位置**：`前端 → IPC 契约 → src-tauri（接线） → texpresso-infra（基础设施实现） → texpresso-core（领域接口）`。core 仍**不**依赖 infra：接口在 core、实现在 infra、装配注入在 src-tauri（依赖单向，无环）。
- **内容**：fs（TokioFs 实现 FileSystem，含 Windows verbatim 前缀剥离）、runner（latexmk 执行器：超时树杀、PDF 原子拷贝）、synctex（synctex CLI）、storage（设置存储：原子写 + 自写盘 hash 过滤）、watch（notify 监视与事件分类）。
- **纪律**：
  - infra **不得**依赖 tauri：监视结果经 `WatchSink` trait 回调，事件形态（compile-status 等）在 src-tauri 定型；
  - infra 不含业务策略：触发条件在 core `compose`、失败语义在 core `scheduler/policy`；
  - 上层（src-tauri）不出现 `tokio::fs` / `std::fs` / `std::process`：文件与进程一律经 core trait。
- **上层契约变化**：FileSystem trait 增 `canonicalize` / `is_dir` / `write`（"core 唯一的文件访问入口"名副其实）；路径安全策略（设计决策 D8）下沉为 core `project::paths`（`resolve_project_root` / `resolve_in_project` / `resolve_creatable_in_project` + `PathError`），命令面只把失败原因翻译成 `{ code, message }`。

- **状态**：已接受（2026-09）
- **备选方案**：
  - 保持 src-tauri 单 crate、只做模块划分——否决：外部依赖仍在上层，`use tokio::fs` 这类调用没有结构性约束，本次要解决的问题依旧存在；
  - 把实现并入 texpresso-core 并加 feature 开关——否决：破坏 core "无 IO、无 Tauri、全量可单测" 的纪律（ADR-0006）。
- **影响**：
  - 依赖搬迁：notify / tokio-util / async-trait / serde_json 从 src-tauri 移到 infra；src-tauri 的依赖面收窄到 tauri 系 + serde + tokio(sync) + tracing + 两个本地 crate；
  - 集成测试入口变化：真实 latexmk/synctex 用例从 `cargo test -p texpresso -- --ignored` 改为 `cargo test -p texpresso-infra -- --ignored`；
  - 架构图（architecture-diagram.md 图 1）与 modules.md 的模块归属同步为三层 Rust 结构。
