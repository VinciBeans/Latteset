//! 设置合并与局部更新（modules.md §6）。

use super::model::{ProjectOverrides, Settings, SettingsPatch};

/// 项目覆盖全局：字段级 Option 语义，缺失继承全局（modules.md §6 merge 算法）。
///
/// 纯函数：不修改入参，返回新值。
pub fn merge(global: &Settings, overrides: &ProjectOverrides) -> Settings {
    let mut s = global.clone();
    if let Some(compile) = &overrides.compile {
        if let Some(v) = compile.mode {
            s.compile.mode = v;
        }
        if let Some(v) = compile.debounce_ms {
            s.compile.debounce_ms = v;
        }
        if let Some(v) = compile.timeout_secs {
            s.compile.timeout_secs = v;
        }
        if let Some(v) = compile.engine {
            s.compile.engine = v;
        }
    }
    if let Some(v) = &overrides.root_file {
        s.root_file = Some(v.clone());
    }
    s
}

/// 局部更新：只改 patch 里的键，其余不动；更新后整体校验（modules.md §6）。
///
/// 失败时**不修改** `settings`（先应用到副本，校验通过才写回）。
pub fn apply_patch(settings: &mut Settings, patch: &SettingsPatch) -> Result<(), Vec<String>> {
    let mut next = settings.clone();
    if let Some(v) = patch.mode {
        next.compile.mode = v;
    }
    if let Some(v) = patch.debounce_ms {
        next.compile.debounce_ms = v;
    }
    if let Some(v) = patch.timeout_secs {
        next.compile.timeout_secs = v;
    }
    if let Some(v) = patch.engine {
        next.compile.engine = v;
    }
    if let Some(v) = &patch.root_file {
        next.root_file = v.clone();
    }
    super::validate::validate(&next)?;
    *settings = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::model::{CompileOverrides, CompileSettings, SCHEMA_VERSION};
    use crate::types::{CompileMode, Engine};
    use std::path::PathBuf;

    fn global() -> Settings {
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

    #[test]
    fn merge_empty_overrides_is_identity() {
        let g = global();
        let merged = merge(&g, &ProjectOverrides::default());
        assert_eq!(merged, g);
    }

    #[test]
    fn merge_partial_override_inherits_rest() {
        let g = global();
        let o = ProjectOverrides {
            compile: Some(CompileOverrides {
                mode: Some(CompileMode::OnSave),
                ..CompileOverrides::default()
            }),
            root_file: Some(PathBuf::from("thesis.tex")),
        };
        let merged = merge(&g, &o);
        assert_eq!(merged.compile.mode, CompileMode::OnSave);
        // 未覆盖的键继承全局
        assert_eq!(merged.compile.debounce_ms, 500);
        assert_eq!(merged.compile.timeout_secs, 120);
        assert_eq!(merged.compile.engine, Engine::XeLaTeX);
        assert_eq!(merged.root_file.as_deref(), Some(std::path::Path::new("thesis.tex")));
        // 纯函数：全局未被修改
        assert_eq!(g.compile.mode, CompileMode::Continuous);
        assert_eq!(g.root_file, None);
    }

    #[test]
    fn merge_overrides_all_compile_fields() {
        let o = ProjectOverrides {
            compile: Some(CompileOverrides {
                mode: Some(CompileMode::OnSave),
                debounce_ms: Some(800),
                timeout_secs: Some(60),
                engine: Some(Engine::PdfLaTeX),
            }),
            ..ProjectOverrides::default()
        };
        let merged = merge(&global(), &o);
        assert_eq!(merged.compile.mode, CompileMode::OnSave);
        assert_eq!(merged.compile.debounce_ms, 800);
        assert_eq!(merged.compile.timeout_secs, 60);
        assert_eq!(merged.compile.engine, Engine::PdfLaTeX);
    }

    #[test]
    fn apply_patch_sets_only_given_keys() {
        let mut s = global();
        let patch = SettingsPatch {
            timeout_secs: Some(60),
            ..SettingsPatch::default()
        };
        apply_patch(&mut s, &patch).unwrap();
        assert_eq!(s.compile.timeout_secs, 60);
        assert_eq!(s.compile.mode, CompileMode::Continuous); // 未动
        assert_eq!(s.compile.debounce_ms, 500); // 未动
    }

    #[test]
    fn apply_patch_root_file_set_and_clear() {
        let mut s = global();
        apply_patch(
            &mut s,
            &SettingsPatch {
                root_file: Some(Some(PathBuf::from("main.tex"))),
                ..SettingsPatch::default()
            },
        )
        .unwrap();
        assert_eq!(s.root_file.as_deref(), Some(std::path::Path::new("main.tex")));
        apply_patch(
            &mut s,
            &SettingsPatch {
                root_file: Some(None),
                ..SettingsPatch::default()
            },
        )
        .unwrap();
        assert_eq!(s.root_file, None);
    }

    #[test]
    fn apply_patch_invalid_value_leaves_settings_untouched() {
        let mut s = global();
        let patch = SettingsPatch {
            timeout_secs: Some(9999), // 超界
            ..SettingsPatch::default()
        };
        assert!(apply_patch(&mut s, &patch).is_err());
        assert_eq!(s.compile.timeout_secs, 120); // 原值未动
    }
}
