//! latteset-infra：**基础设施层**（ADR-0010）。
//!
//! 位置：位于 latteset-core（纯领域）与 src-tauri（接线薄壳）之间，与 core 同级；
//! 是**外部依赖与文件系统的唯一落点**。
//!
//! 纪律：
//! - 上层（src-tauri 的 commands 编排）不得直接调用 OS / 外部进程 API——
//!   文件读写走 [latteset_core::project::FileSystem]（本 crate 提供 TokioFs 实现），
//!   进程走 [latteset_core::scheduler::CompileRunner] /
//!   [latteset_core::synctex::SyncTexProvider]（本 crate 实现）；
//! - 本 crate **不得**依赖 tauri：监视结果经 [watch::WatchSink] 由上层注入发出，
//!   事件与命令契约留在 src-tauri；
//! - 本 crate 不含业务策略：触发条件（compose）、失败语义（scheduler/policy）仍在 core。
//!
//! 模块：fs（tokio::fs 实现 FileSystem + verbatim 前缀剥离）、runner（latexmk 执行器）、
//! synctex（synctex CLI）、storage（设置存储：原子写 + 自写盘过滤）、watch（notify 文件监视）、
//! tectonic（Tectonic 的 bundle 获取策略与缓存判定）。

pub mod fs;
pub mod runner;
pub mod storage;
pub mod synctex;
pub mod tectonic;
pub mod watch;
