//! 设置模型（modules.md §6）。
//!
//! 存储分层：全局 `settings.json` 存完整 [`Settings`]；
//! 项目 `.latteset/settings.json` 存 [`ProjectOverrides`]（只含要覆盖的键，缺省继承全局）。

use crate::types::{CompileMode, Engine};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;

/// 设置文件 schema 版本（modules.md §6：schema_version 字段，未来迁移用）。
pub const SCHEMA_VERSION: u32 = 1;

/// 编译相关设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CompileSettings {
    pub mode: CompileMode,
    pub debounce_ms: u32,
    pub timeout_secs: u32,
    pub engine: Engine,
}

/// 合并后的有效设置（全局 + 项目覆盖）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Settings {
    pub schema_version: u32,
    pub compile: CompileSettings,
    /// 根文件手动覆盖（探测结果的逃生门，ADR-0009）。
    pub root_file: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema_version: SCHEMA_VERSION,
            compile: CompileSettings {
                mode: CompileMode::Continuous,
                debounce_ms: 500,
                timeout_secs: 120,
                engine: Engine::XeLaTeX,
            },
            root_file: None,
        }
    }
}

/// 项目级覆盖文件（.latteset/settings.json）。
///
/// 所有字段可选：缺失 = 继承全局（modules.md §6 merge 语义）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compile: Option<CompileOverrides>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_file: Option<PathBuf>,
}

/// 项目级编译覆盖。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompileOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<CompileMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debounce_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<Engine>,
}

/// 设置局部更新（update_settings 命令载荷，modules.md §6）。
///
/// 只改 Some 的键；`root_file` 语义：缺键 = 不动，`null` = 清除覆盖，字符串 = 设置覆盖。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<CompileMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debounce_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<Engine>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_root_file_patch"
    )]
    #[specta(type = Option<Option<PathBuf>>)]
    pub root_file: Option<Option<PathBuf>>,
}

/// `Option<Option<PathBuf>>` 的 null 歧义处理：
/// serde 默认把 `null` 反序列化为外层 `None`（= 缺键），但我们约定 `null` = 显式清除。
fn deserialize_root_file_patch<'de, D>(d: D) -> Result<Option<Option<PathBuf>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<Option<PathBuf>>::deserialize(d)?;
    Ok(v.or(Some(None)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_design() {
        let s = Settings::default();
        assert_eq!(s.schema_version, SCHEMA_VERSION);
        assert_eq!(s.compile.mode, CompileMode::Continuous);
        assert_eq!(s.compile.debounce_ms, 500);
        assert_eq!(s.compile.timeout_secs, 120);
        assert_eq!(s.compile.engine, Engine::XeLaTeX);
        assert_eq!(s.root_file, None);
    }

    #[test]
    fn settings_serde_roundtrip() {
        let s = Settings {
            root_file: Some(PathBuf::from("main.tex")),
            ..Settings::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn overrides_deserialize_partial_json() {
        // 项目文件只写覆盖的键（modules.md §6）
        let json = r#"{"compile": {"mode": "on_save"}}"#;
        let o: ProjectOverrides = serde_json::from_str(json).unwrap();
        assert_eq!(o.compile.as_ref().unwrap().mode, Some(CompileMode::OnSave));
        assert_eq!(o.compile.as_ref().unwrap().debounce_ms, None);
        assert_eq!(o.root_file, None);
    }

    #[test]
    fn overrides_serde_omits_none_fields() {
        let o = ProjectOverrides::default();
        let json = serde_json::to_string(&o).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn patch_serde_roundtrip() {
        let p = SettingsPatch {
            timeout_secs: Some(60),
            root_file: Some(Some(PathBuf::from("thesis.tex"))),
            ..SettingsPatch::default()
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: SettingsPatch = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
        // 显式清空：root_file = null
        let clear: SettingsPatch = serde_json::from_str(r#"{"root_file": null}"#).unwrap();
        assert_eq!(clear.root_file, Some(None));
    }
}
