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

/// Tectonic 形态与资源设置（⑫ 里程碑的设置面收口项；方案 §3.5）。
///
/// **只在全局设置里**（不进 [`ProjectOverrides`]）：引擎形态由装配点的 runner 决定，而 GUI 侧
/// 的 runner **每趟编译读一次全局设置**（见 `src-tauri` 的 `SwitchableRunner`）；项目级覆盖要走
/// "形态位随 `CompileRequest` 下发"那条路，属后续项 —— 现在放一个项目级字段只会是**看着能覆盖、
/// 实际不生效**的假开关。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TectonicSettings {
    /// 引擎形态：`false`（默认）= **子进程**（`tectonic.exe`）；`true` = **库内嵌**（路径 B，ADR-0012）。
    ///
    /// 需要构建时打开 `tectonic-lib` 特性；没编进去而用户选了库形态时 runner **显式失败**
    /// （D1：不静默回退到子进程）。
    pub lib_form: bool,
    /// bundle 来源：`None`/空 = 上游兜底**网络地址**；否则 `file:///…` 或**相对路径**的目录 bundle。
    ///
    /// ⚠ **存储形态**不能是 `E:\…`：上游 `detect_bundle` 会把它当 URL scheme 解析成 `Ok(None)`
    /// （方案 §5.6 **LB-1**）。入口是宽容的——`E:\…` / `E:/…` / 带引号的「复制为路径」都会被
    /// [`TectonicSettings::normalize_bundle`] 补成 `file:///…`（`apply_patch` 里归一化在前、
    /// 校验在后），所以 [`super::validate`] 那条只兜底绕过 patch 的写入（手改 `settings.json`）。
    pub bundle: Option<String>,
    /// 产品缓存目录（`formats/` + `bundles/` 的父目录）：`None`/空 = 宿主的应用缓存目录。
    ///
    /// **必须显式给**（或由宿主注入）：上游 format 默认落**项目目录**，不注入就会往用户项目扔
    /// 24 MB 的 `.fmt`（方案 §5.2 硬约束）。
    pub cache_dir: Option<PathBuf>,
}

impl Default for TectonicSettings {
    fn default() -> Self {
        Self {
            lib_form: false,
            bundle: None,
            cache_dir: None,
        }
    }
}

impl TectonicSettings {
    /// 本趟编译是否走**库内嵌**形态（`false` = 子进程）。
    ///
    /// **引擎闸门不可省**：形态位是 Tectonic 引擎的**子选项**，只有 `engine == Tectonic` 时才谈得上。
    /// 少了 `engine == Tectonic` 这一半，`engine=xelatex` + `lib_form=true` 会**静默跑 Tectonic 库形态**
    /// ——状态栏报 XeLaTeX、实际引擎是 Tectonic。判据见 `docs/research/tectonic-test-plan.md` 的 INT-92：
    /// 切回 XeLaTeX 必须产出 `tmp/<stem>.xelatex.pages`，且首轮 `changed_pages` 为全部页。
    ///
    /// `env_forced`（`LATTESET_TECTONIC_LIB=1`）是**显式覆盖**：复核/CI 要在不改用户设置的前提下复现
    /// 库形态那条路径，所以它不受引擎闸门约束。
    pub fn use_library_form(&self, engine: Engine, env_forced: bool) -> bool {
        (engine == Engine::Tectonic && self.lib_form) || env_forced
    }

    /// 用户给的 bundle 路径 → **存储形态**（上游 `detect_bundle` 认的 URL 或相对路径）。
    ///
    /// 设置面与命令行拿到的是普通 Windows 路径（资源管理器地址栏、`复制为路径` 还带一对引号），
    /// 而上游先做 `Url::parse`：`E:\…` / `E:/…` 会被当成 scheme `e` 解析成功并返回 `Ok(None)`，
    /// 最终只报一句 `doesn't specify a valid bundle`（方案 §5.6 **LB-1** 实测）。
    ///
    /// 只改"看着就是绝对 Windows 路径"（`<盘符>:<分隔符>`）的那一类；`file://…`、`https://…`、
    /// 相对路径一律原样透传（相对路径以上游进程 cwd 为基准，是可行的给法之一）。
    pub fn normalize_bundle(raw: &str) -> String {
        let s = raw.trim().trim_matches('"').trim();
        let b = s.as_bytes();
        let drive_absolute = b.len() > 2
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && matches!(b[2], b'\\' | b'/');
        if drive_absolute {
            format!("file:///{}", s.replace('\\', "/"))
        } else {
            s.to_owned()
        }
    }
}

/// 界面主题（roadmap ⑩）。`System` = 跟随操作系统的深/浅色偏好。
///
/// **默认 `Light`**（不是 `System`）：深色主题是新能力，默认值必须让既有用户升级后**外观不变**
/// —— 若默认 `System`，系统偏好深色的用户会在升级那一刻被静默换掉整套配色。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UiTheme {
    #[default]
    Light,
    Dark,
    /// 跟随系统（前端读 `prefers-color-scheme`）。
    System,
}

/// 界面设置（全局，与 [`TectonicSettings`] 同层：不进项目覆盖）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct UiSettings {
    pub theme: UiTheme,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self { theme: UiTheme::default() }
    }
}

/// 编译相关设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CompileSettings {
    pub mode: CompileMode,
    pub debounce_ms: u32,
    pub timeout_secs: u32,
    pub engine: Engine,
    /// 新建**第一个** `.tex` 时弹「新手向导」（roadmap ㊺ §6.13.1-A）：选文档语言/类型/标题，
    /// 直接生成一份能编译的最小骨架。
    ///
    /// **默认开**（新手第一条路要有人领），可在设置里关掉 ⇒ 关掉后回到"建空文件"。
    /// `#[serde(default = ...)]`：旧 `settings.json` 没有这个键也要能读，
    /// 否则升级即报错、用户配置整个丢失（同 `ui` / `tectonic` 的理由）。
    #[serde(default = "default_true")]
    pub new_file_wizard: bool,
}

/// `bool` 字段的缺省值（serde 的 `default` 只认函数路径，不能写 `true` 字面量）。
fn default_true() -> bool {
    true
}

/// 合并后的有效设置（全局 + 项目覆盖）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Settings {
    pub schema_version: u32,
    pub compile: CompileSettings,
    /// Tectonic 形态与资源（全局；见 [`TectonicSettings`] 的说明）。
    #[serde(default)]
    pub tectonic: TectonicSettings,
    /// 界面（全局；`#[serde(default)]` ⇒ 旧 settings.json 没有 `ui` 也能读）。
    #[serde(default)]
    pub ui: UiSettings,
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
                // 默认引擎 = **Tectonic**（roadmap ㊻，2026-09-17 产品负责人裁决：引擎以 Tectonic
                // 为优先，新装就该走它）。**只影响"没有 settings.json"的新装/新用户**：老配置里
                // 写着 `"engine":"xelatex"` 的人升级后一字节都不变（不静默换引擎）。
                // 形态位 `lib_form` 仍是 false ⇒ 默认走**子进程**形态（需要 PATH 上有 tectonic.exe；
                // 库形态要 `--features tectonic-lib` 构建，见 ADR-0012）。
                engine: Engine::Tectonic,
                // 默认开：新建第一个 .tex 时给新手一份能直接编译的骨架（roadmap ㊺ §6.13.1-A）
                new_file_wizard: true,
            },
            tectonic: TectonicSettings::default(),
            ui: UiSettings::default(),
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
    /// 新建向导开关（roadmap ㊺ §6.13.1-A）：`None` = 不动。**全局**设置，无项目覆盖
    /// （它是"界面要不要拦一下"，与具体项目无关）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_file_wizard: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_root_file_patch"
    )]
    #[specta(type = Option<Option<PathBuf>>)]
    pub root_file: Option<Option<PathBuf>>,
    /// 引擎形态（Tectonic 库内嵌）：`None` = 不动。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lib_form: Option<bool>,
    /// bundle 来源：`None` = 不动；**空串 = 清除**（回到上游兜底）；其它 = 设置。
    ///
    /// 用"空串 = 清除"而不是 `Option<Option<..>>`：设置面总是发具体值，用户清空输入框就是清除，
    /// 语义够用且少一层 `null` 歧义（对比 `root_file` 的历史包袱）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<String>,
    /// 缓存目录：同上，**空串 = 清除**（回到宿主应用缓存目录）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<String>,
    /// 界面主题（roadmap ⑩）：`None` = 不动。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<UiTheme>,
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
        // 默认引擎 = Tectonic（roadmap ㊻）。形态位仍是子进程档：库形态要构建时打开特性。
        assert_eq!(s.compile.engine, Engine::Tectonic);
        assert!(!s.tectonic.lib_form);
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

    /// 绝对 Windows 路径要补成上游认的 `file:///` URL（LB-1：否则被当成 scheme `e` 吃掉）。
    #[test]
    fn normalize_bundle_fixes_absolute_windows_paths() {
        for (input, want) in [
            (r"E:\Works\bundle", "file:///E:/Works/bundle"),
            ("E:/Works/bundle", "file:///E:/Works/bundle"),
            // 「复制为路径」会给整串包上引号，且可能带首尾空白
            (r#"  "E:\Works\bundle"  "#, "file:///E:/Works/bundle"),
        ] {
            assert_eq!(TectonicSettings::normalize_bundle(input), want, "输入 {input:?}");
        }
    }

    /// 已经是可行写法的输入必须**原样透传**（不能把 `file:///…` 变成 `file:///file:///…`）。
    #[test]
    fn normalize_bundle_passes_through_viable_forms() {
        for s in [
            "file:///E:/Works/bundle",
            "file:///home/u/bundle",
            "bundles/local",     // 相对路径：上游认的第二种给法
            "https://example.com/bundle.zip",
            "",                  // 空串 = 清除，调用方按空串处理
        ] {
            assert_eq!(TectonicSettings::normalize_bundle(s), s, "{s:?} 不该被改写");
        }
        // 只有盘符、没有分隔符的（`E:`）不算绝对路径，交给 validate/上游报错，不要猜
        assert_eq!(TectonicSettings::normalize_bundle("E:"), "E:");
    }

    /// **库形态只在选中 Tectonic 引擎时才允许**（INT-92 的回归锁）。
    ///
    /// 没有引擎闸门时 `engine=xelatex` + `lib_form=true` 会静默跑 Tectonic 库形态，而状态栏
    /// 报的是 XeLaTeX——真机上就是这样骗过一轮验证的。
    #[test]
    fn library_form_requires_the_tectonic_engine() {
        let lib_on = TectonicSettings { lib_form: true, ..Default::default() };
        let lib_off = TectonicSettings::default();

        // 形态位开着，但引擎不是 Tectonic ⇒ 一律走子进程（否则引擎选择被静默无视）
        for e in [Engine::XeLaTeX, Engine::LuaLaTeX, Engine::PdfLaTeX] {
            assert!(!lib_on.use_library_form(e, false), "{e:?} 不该走库形态");
        }
        // 引擎是 Tectonic 时才由形态位决定
        assert!(lib_on.use_library_form(Engine::Tectonic, false));
        assert!(!lib_off.use_library_form(Engine::Tectonic, false));

        // 环境变量是显式覆盖：按文档"压过设置"，不受引擎闸门约束（复核/CI 的逃生门）
        assert!(lib_off.use_library_form(Engine::XeLaTeX, true));
        assert!(lib_on.use_library_form(Engine::Tectonic, true));
    }
}
