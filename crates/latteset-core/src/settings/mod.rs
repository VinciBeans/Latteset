//! 设置模块（modules.md §6）。
//!
//! - [`model`]：设置模型（全局 Settings / 项目 ProjectOverrides / 更新 SettingsPatch）
//! - [`merge`]：合并（项目覆盖全局，字段级）与局部更新
//! - [`validate`]：范围校验

pub mod merge;
pub mod model;
pub mod validate;

pub use merge::{apply_patch, merge};
pub use model::{
    CompileOverrides, CompileSettings, Notice, NoticeLevel, ProjectOverrides, Settings, SettingsPatch,
    TectonicSettings, UiSettings, UiTheme, ENGINE_UNIFIED_NOTICE_ID, ENGINE_UNIFIED_NOTICE_MESSAGE,
    FORM_EMBEDDED_NOTICE_ID, FORM_EMBEDDED_NOTICE_MESSAGE, SCHEMA_VERSION,
    SUBPROCESS_FORM_REMOVED_NOTICE_ID, SUBPROCESS_FORM_REMOVED_NOTICE_MESSAGE,
};
pub use validate::{sanitize_overrides, validate, validate_overrides};
