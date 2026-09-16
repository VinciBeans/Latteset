//! `CompileRunner` 接口（modules.md §2.4）。
//!
//! 超时检测、进程树杀、PDF 拷贝全部在 runner 内（设计决策 D2）：
//! 调度器无时钟、无进程概念，单测只需喂假 `CompileOutcome`。

use crate::types::{CompileOutcome, CompileRequest, ErrorEntry};
use async_trait::async_trait;
use std::path::Path;
use tokio_util::sync::CancellationToken;

/// 编译执行抽象：core 唯一知道的"怎么编译"。
///
/// 契约：
/// - `cancel` 被取消后必须尽快终止（树杀）并返回 `Aborted`；
/// - 超时由 runner 自行检测（`req.timeout`），超时后树杀并返回 `Timeout`；
/// - 成功时 PDF 必须已拷贝到项目根（`Success.pdf_path`）。
#[async_trait]
pub trait CompileRunner: Send + Sync {
    async fn compile(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome;
}

/// 编译**进行中**的反馈通道（roadmap「阶段 2 · 流式输出」）：runner 在进程还活着时
/// 就能把"排到第几页""已经出现了哪些错误"报上来，而不必等编译结束。
///
/// 为什么是 runner 的可选依赖而不是 `Emitter` 的一部分：调度器的 `Emitter` 只在**任务完成**
/// 时被调用（合并队列/失败语义都发生在那一刻）；流式反馈属于"运行中的进程输出"，与调度语义
/// 正交。默认实现全为空，测试与 headless 不必关心。
///
/// 契约：**非权威**。编译结束时 runner 仍要返回权威的 `CompileOutcome`，前端应以最终结果覆盖
/// 流式期间的中间态（`errors` 尤其如此——半截日志可能给出不完整的条目）。
pub trait CompileProgress: Send + Sync {
    /// 已排版页数（来自引擎输出的 `[N]` 标记）；只在**数值变大**时调用。
    fn pages(&self, _pages: u32) {}

    /// 编译进行中解析到的**致命错误**条目（节流 + 去重后）。
    ///
    /// 实现可以只报"会终结本次编译的错误"、不报 `Overfull` 这类警告——完整清单由终态
    /// `CompileOutcome` 给出。调用方必须把它当作**只在编译进行中有效**的中间态：终态结果到达后
    /// 覆盖它，且晚到的流式事件不得再改写终态列表（实现在编译收尾阶段仍会补发最后一批）。
    fn errors(&self, _errors: &[ErrorEntry]) {}

    /// **编译中预览**（roadmap ㉞「流式出图」）：已完成页合成出的**部分 PDF** 落在磁盘上的路径。
    ///
    /// 契约（与 `pages`/`errors` 同级，都是**非权威**中间态）：
    /// - 它**不是**最终产物 —— 终态仍以 `CompileOutcome::Success.pdf_path` 为准，前端只能用它在
    ///   编译期间"先看上几页"；
    /// - **不得**把它当作页哈希 / A 闸门 / 收敛判断的输入（中间态会污染基线）；
    /// - `pages` **只增不减**；同一次编译里可能被调用多次（限流后的节拍）。
    ///
    /// 默认空实现：只有**库形态**有内存 XDV 可合成；子进程档没有这条通道，headless 也不需要。
    /// 调用方（runner）负责把"只在编译确实跑得久时才出图"的阈值与限流做在前面。
    fn partial_pdf(&self, _path: &Path, _pages: u32) {}
}

/// 什么都不做的反馈实现（headless / 测试用）。
pub struct NoProgress;

impl CompileProgress for NoProgress {}

