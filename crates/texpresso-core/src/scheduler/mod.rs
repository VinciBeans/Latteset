//! 编译调度器（modules.md §2，ADR-0001 合并队列调度）。
//!
//! - [`queue`]：合并队列（最多一个、总是最新）
//! - [`policy`]：失败语义决策表（纯函数）
//! - [`runner`]：`CompileRunner` 接口 + `CompileProgress` 流式反馈（执行经 trait 注入，D2）
//! - [`actor`]：主循环（唯一写者，状态全在 task 内，D1）

pub mod actor;
pub mod policy;
pub mod queue;
pub mod runner;

pub use actor::{Emitter, Scheduler, SchedulerHandle};
pub use runner::{CompileProgress, CompileRunner, NoProgress};
