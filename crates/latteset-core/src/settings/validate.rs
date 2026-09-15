//! 设置范围校验（modules.md §6）。

use std::path::Path;

use super::model::{ProjectOverrides, Settings};

// 超时上限 1800s（30 分钟）：真实学位论文模板首编可达数分钟（roadmap ㉕ 实测
// `thesis-real-hithesis` 冷编译 >240s，调高到 600s 上限仍可能不够——上限本身
// 就是"无法通过调高自救"的死路），故上限取"够跑完真实大文档"而非"够跑完合成档"。
const TIMEOUT_SECS_RANGE: std::ops::RangeInclusive<u32> = 5..=1800;
const DEBOUNCE_MS_RANGE: std::ops::RangeInclusive<u32> = 100..=2000;

/// 校验合并后的有效设置；返回全部违规项（空 = 通过）。
pub fn validate(s: &Settings) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    if !TIMEOUT_SECS_RANGE.contains(&s.compile.timeout_secs) {
        errs.push(format!(
            "compile.timeout_secs 必须在 {}..={} 之间（当前 {}）",
            TIMEOUT_SECS_RANGE.start(),
            TIMEOUT_SECS_RANGE.end(),
            s.compile.timeout_secs
        ));
    }
    if !DEBOUNCE_MS_RANGE.contains(&s.compile.debounce_ms) {
        errs.push(format!(
            "compile.debounce_ms 必须在 {}..={} 之间（当前 {}）",
            DEBOUNCE_MS_RANGE.start(),
            DEBOUNCE_MS_RANGE.end(),
            s.compile.debounce_ms
        ));
    }
    errs.extend(validate_tectonic(&s.tectonic));
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Tectonic 形态与资源的校验（**在设置面就拦住**，而不是等到编译才报错）。
///
/// 两条都是实测过的坑，在这里拦能给出可执行的文案：
/// - **bundle 写绝对 Windows 路径**：上游 `detect_bundle` 先 `Url::parse`，`E:\…`/`E:/…` 会被当成
///   scheme `e` 解析成功 ⇒ 返回 `Ok(None)` ⇒ 报一句"不是有效 bundle"（方案 §5.6 LB-4 实测）。
/// - **缓存目录写相对路径**：它以进程 cwd 为基准，而 cwd 会随宿主不同（GUI/headless/测试）⇒ 同一份
///   设置指向不同目录（format 缓存会"莫名重建"）。要求绝对路径。
pub fn validate_tectonic(t: &super::model::TectonicSettings) -> Vec<String> {
    let mut errs = Vec::new();
    if let Some(raw) = t.bundle.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let looks_absolute_windows = raw.len() > 2 && raw.as_bytes().get(1) == Some(&b':');
        if looks_absolute_windows && !raw.starts_with("file://") {
            errs.push(format!(
                "bundle 不能写绝对 Windows 路径（{raw}）：上游会把它当 URL scheme 解析、判为「不是 bundle」\
                 ⇒ 请写 `file:///{}` 或相对路径",
                raw.replace('\\', "/")
            ));
        }
    }
    if let Some(raw) = t.cache_dir.as_deref() {
        let p = raw.to_string_lossy();
        let s = p.trim();
        if !s.is_empty() && !Path::new(s).is_absolute() {
            errs.push(format!(
                "缓存目录必须是绝对路径（当前 {s}）：相对路径以进程 cwd 为基准，\
                 换宿主（GUI/headless）会指向不同目录，format 缓存会反复重建"
            ));
        }
    }
    errs
}

/// root_file 基本形式校验：非空、不含跨过项目根的 `..` 组件。
/// 完整包含关系（相对具体项目根）放在 commands::open_project 层（那里才有项目根）。
fn validate_root_file_form(root_file: &Path) -> Option<String> {
    let s = root_file.to_string_lossy();
    if s.is_empty() {
        return Some("root_file 不能为空".into());
    }
    let has_parent_component = s.split(['/', '\\']).any(|c| c == "..");
    if has_parent_component {
        return Some("root_file 不允许包含 .. 组件（疑似越权路径）".into());
    }
    None
}

/// 逐字段清洗非法覆盖值：越界字段置 None（回退全局），合法字段保留。
/// 用于加载（load_overrides 容忍手改/陈旧值）；update_settings 仍用 validate_overrides 拒绝整批非法输入。
pub fn sanitize_overrides(o: &mut ProjectOverrides) {
    if let Some(rf) = &o.root_file {
        if validate_root_file_form(rf).is_some() {
            o.root_file = None;
        }
    }
    if let Some(c) = &mut o.compile {
        if let Some(t) = c.timeout_secs {
            if !TIMEOUT_SECS_RANGE.contains(&t) {
                c.timeout_secs = None;
            }
        }
        if let Some(d) = c.debounce_ms {
            if !DEBOUNCE_MS_RANGE.contains(&d) {
                c.debounce_ms = None;
            }
        }
    }
}

/// 校验项目覆盖（只查 Some 的键）。
pub fn validate_overrides(o: &ProjectOverrides) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    if let Some(root_file) = &o.root_file {
        if let Some(msg) = validate_root_file_form(root_file) {
            errs.push(msg);
        }
    }
    if let Some(compile) = &o.compile {
        if let Some(v) = compile.timeout_secs {
            if !TIMEOUT_SECS_RANGE.contains(&v) {
                errs.push(format!(
                    "compile.timeout_secs 必须在 {}..={} 之间（当前 {}）",
                    TIMEOUT_SECS_RANGE.start(),
                    TIMEOUT_SECS_RANGE.end(),
                    v
                ));
            }
        }
        if let Some(v) = compile.debounce_ms {
            if !DEBOUNCE_MS_RANGE.contains(&v) {
                errs.push(format!(
                    "compile.debounce_ms 必须在 {}..={} 之间（当前 {}）",
                    DEBOUNCE_MS_RANGE.start(),
                    DEBOUNCE_MS_RANGE.end(),
                    v
                ));
            }
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::model::{Settings, TectonicSettings};

    /// bundle 写绝对 Windows 路径要在设置面就被拦住（LB-4），并给出正确写法。
    #[test]
    fn absolute_windows_bundle_is_rejected_with_the_fix() {
        let t = TectonicSettings {
            bundle: Some(r"E:\Works\bundle".to_owned()),
            ..Default::default()
        };
        let errs = validate_tectonic(&t);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].contains("file:///"), "要点明正确写法：{}", errs[0]);
    }

    /// `file:///…` 与相对路径都必须放行（它们是可行的两种给法）。
    #[test]
    fn url_and_relative_bundle_pass() {
        for ok in ["file:///E:/Works/bundle", "bundles/local", ""] {
            let t = TectonicSettings {
                bundle: Some(ok.to_owned()),
                ..Default::default()
            };
            assert!(validate_tectonic(&t).is_empty(), "{ok} 不该被拒");
        }
    }

    /// 缓存目录必须绝对（相对路径换宿主就指向别处 ⇒ format 缓存反复重建）。
    #[test]
    fn relative_cache_dir_is_rejected() {
        let t = TectonicSettings {
            cache_dir: Some(std::path::PathBuf::from("cache")),
            ..Default::default()
        };
        let errs = validate_tectonic(&t);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].contains("绝对路径"), "{}", errs[0]);
    }

    /// 默认形态（子进程、无 bundle/缓存覆盖）必须通过校验。
    #[test]
    fn defaults_pass_validation() {
        assert!(validate(&Settings::default()).is_ok());
    }

    #[test]
    fn default_passes() {
        assert!(validate(&Settings::default()).is_ok());
    }

    #[test]
    fn rejects_out_of_range_values() {
        let mut s = Settings::default();
        s.compile.timeout_secs = 9999;
        s.compile.debounce_ms = 1;
        let errs = validate(&s).unwrap_err();
        assert_eq!(errs.len(), 2); // 全部违规项都收集
        assert!(errs[0].contains("timeout_secs"));
        assert!(errs[1].contains("debounce_ms"));
    }

    #[test]
    fn boundary_values_accepted() {
        let mut s = Settings::default();
        s.compile.timeout_secs = 5;
        assert!(validate(&s).is_ok());
        s.compile.timeout_secs = 600;
        assert!(validate(&s).is_ok());
        // 上限 1800（roadmap ㉕：真实论文档在 600s 内未必跑得完，上限必须留出空间）
        s.compile.timeout_secs = 1800;
        assert!(validate(&s).is_ok());
        s.compile.timeout_secs = 1801;
        assert!(validate(&s).is_err(), "1801 应越界");
        s.compile.timeout_secs = 120; // 复位，避免影响下面的 debounce 断言
        s.compile.debounce_ms = 100;
        assert!(validate(&s).is_ok());
        s.compile.debounce_ms = 2000;
        assert!(validate(&s).is_ok());
    }

    #[test]
    fn validate_overrides_checks_some_only() {
        let o = ProjectOverrides {
            compile: Some(crate::settings::model::CompileOverrides {
                timeout_secs: Some(9999),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate_overrides(&o).is_err());

        let o2 = ProjectOverrides::default();
        assert!(validate_overrides(&o2).is_ok());
    }

    #[test]
    fn sanitize_overrides_clears_invalid_keeps_valid() {
        let mut o = ProjectOverrides {
            root_file: Some("../outside.tex".into()),
            compile: Some(crate::settings::model::CompileOverrides {
                timeout_secs: Some(9999),
                debounce_ms: Some(150),
                ..Default::default()
            }),
            ..Default::default()
        };
        sanitize_overrides(&mut o);
        assert_eq!(o.root_file, None, "非法 root_file 应清空（回退探测）");
        let c = o.compile.unwrap();
        assert_eq!(c.timeout_secs, None, "越界 timeout 置 None（回退全局）");
        assert_eq!(c.debounce_ms, Some(150), "合法 debounce 保留");

        // 全默认 → 不变
        let mut d = ProjectOverrides::default();
        sanitize_overrides(&mut d);
        assert_eq!(d, ProjectOverrides::default());
    }

    #[test]
    fn validate_overrides_rejects_unsafe_root_file() {
        // 拒绝 .. 组件（越权路径）与空串
        for bad in ["../outside.tex", "sub/../../x.tex", ""] {
            let o = ProjectOverrides {
                root_file: Some(bad.into()),
                ..Default::default()
            };
            assert!(validate_overrides(&o).is_err(), "应拒绝 {bad:?}");
        }
        // 项目内相对路径合法
        for good in ["main.tex", "css/thesis.tex"] {
            let o = ProjectOverrides {
                root_file: Some(good.into()),
                ..Default::default()
            };
            assert!(validate_overrides(&o).is_ok(), "应接受 {good:?}");
        }
    }
}
