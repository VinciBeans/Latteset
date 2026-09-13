//! 设置模块（modules.md §6）。
//!
//! - [`model`]：设置模型（全局 Settings / 项目 ProjectOverrides / 更新 SettingsPatch）
//! - [`merge`]：合并（项目覆盖全局，字段级）与局部更新
//! - [`validate`]：范围校验

pub mod merge;
pub mod model;
pub mod validate;

pub use merge::{apply_patch, merge};
pub use model::{CompileOverrides, CompileSettings, ProjectOverrides, Settings, SettingsPatch, SCHEMA_VERSION};
pub use validate::{sanitize_overrides, validate, validate_overrides};
