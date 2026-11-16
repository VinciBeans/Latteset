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
//! 模块：fs（tokio::fs 实现 FileSystem + verbatim 前缀剥离）、runner（Tectonic 子进程执行器）、
//! synctex（synctex CLI）、snippet（公式预览的片段落点，roadmap ㊸ 切片 2）、storage（设置存储：
//! 原子写 + 自写盘过滤 + v1→v2 迁移）、watch（notify 文件监视）、tectonic（Tectonic 的 bundle
//! 获取策略与缓存判定）。

pub mod fs;
pub mod proc;
pub mod runner;
pub mod snippet;
pub mod storage;
pub mod synctex;
pub mod tectonic;
pub mod watch;

/// 已删除的环境变量：`LATTESET_TEX_ENGINES`（引擎清单覆盖）。
///
/// ADR-0015 把引擎矩阵（含清单覆盖）整体删除。旧文档/脚本里仍可能有人设着它，所以启动时**如实
/// 告知一次**"已删除、被忽略"，而不是让用户以为它还在生效。用 [`std::sync::Once`] 保证只打一条。
pub const REMOVED_ENGINES_ENV: &str = "LATTESET_TEX_ENGINES";

/// 启动时调用一次：检测已删除的环境变量并打警告（幂等，重复调用只打一条）。
pub fn warn_removed_env_overrides() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if std::env::var_os(REMOVED_ENGINES_ENV).is_some() {
            tracing::warn!(
                "{REMOVED_ENGINES_ENV} 已删除，忽略（引擎已统一为 Tectonic，见 ADR-0015）"
            );
        }
    });
}
