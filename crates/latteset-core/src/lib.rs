//! latteset-core：Latteset 领域核心。
//!
//! 纪律（ADR-0006）：本 crate **不得**依赖 tauri、不得做 IO；
//! 一切 IO 经 [`project::FileSystem`] 等 trait 注入。

pub mod compose;
pub mod log_parser;
pub mod math;
pub mod newfile;
pub mod outline;
pub mod project;
pub mod scheduler;
pub mod settings;
pub mod snippet;
pub mod synctex;
pub mod types;
pub mod xdv;

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod testutil;
#[cfg(test)]
mod unicode_path_tests;
