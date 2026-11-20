//! 设置模型（modules.md §6）。
//!
//! 存储分层：全局 `settings.json` 存完整 [`Settings`]；
//! 项目 `.latteset/settings.json` 存 [`ProjectOverrides`]（只含要覆盖的键，缺省继承全局）。

use crate::types::CompileMode;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;

/// 设置文件 schema 版本（modules.md §6：schema_version 字段，未来迁移用）。
///
/// **v2**（ADR-0015）：引擎统一为 Tectonic，删除 `compile.engine`（全局与项目覆盖两层）。
/// **v3**（ADR-0015 决策 2）：形态收成**内嵌 Tectonic 唯一**，删除 `tectonic` 里的形态位
/// （子进程形态已删除）。
///
/// v1 → v2 → v3 的迁移在 infra（`latteset_infra::storage`）：旧文件被读入时自动逐级改写；
/// 原文按**迁移源版本**各留一份（`settings.json.v1.bak` / `settings.json.v2.bak`）。
pub const SCHEMA_VERSION: u32 = 3;

/// 一次性提示的级别（前端据此选样式：info = 中性告知，warning = 需要注意）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    Info,
    Warning,
}

/// 设置层的一次性提示（状态栏 / 设置面板展示，用户可 dismiss）。
///
/// 存在的理由：有些调整**没有可迁移的等价字段**（例如 v2 删掉了引擎设置），只能如实告知
/// "原设置已忽略"。做成数据而不是一次性弹窗，是因为它要跨重启存活到用户看见为止——
/// dismiss 后按 `id` 从列表里移除（[`super::merge::apply_patch`]）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Notice {
    /// 稳定标识（dismiss 按它移除；同一 id 不会重复出现）。
    pub id: String,
    pub level: NoticeLevel,
    /// 给用户看的一句话。
    pub message: String,
}

/// 引擎统一为 Tectonic 的迁移提示 id（v1 → v2 迁移只追加这一条）。
pub const ENGINE_UNIFIED_NOTICE_ID: &str = "engine-unified-v2";

/// v1 → v2 迁移时追加的提示文案（迁移与前端展示共用同一句）。
pub const ENGINE_UNIFIED_NOTICE_MESSAGE: &str =
    "引擎已统一为 Tectonic（v2 起不再有引擎设置），原设置已忽略。";

/// 形态固定为内嵌 Tectonic 的迁移提示 id（v2 → v3 恒加）。
pub const FORM_EMBEDDED_NOTICE_ID: &str = "form-embedded-v3";

/// v2 → v3 迁移恒加的提示文案：形态设置位从这一版起不复存在。
pub const FORM_EMBEDDED_NOTICE_MESSAGE: &str =
    "引擎形态已固定为内嵌 Tectonic（v3 起不再有形态设置），原形态设置已忽略。";

/// 子进程形态被删除的迁移提示 id（v2 → v3，**仅当**旧值表示"子进程档"时追加）。
pub const SUBPROCESS_FORM_REMOVED_NOTICE_ID: &str = "subprocess-form-removed-v3";

/// 子进程形态被删除的提示文案（ADR-0015 第 4 条"不静默降级"的落点）。
///
/// 只对**显式选过子进程**的老用户追加：他们是唯一可能依赖"引擎调本机 biber"的人，必须
/// 白纸黑字告知这条能力没了——而不是让编译行为悄悄变了。
pub const SUBPROCESS_FORM_REMOVED_NOTICE_MESSAGE: &str =
    "你原来使用子进程形态（tectonic.exe）。该形态已删除；从本版本起编译始终在内嵌引擎中进行——\
     不再调用本机 biber，makeindex/glossaries 也不会自动运行。需要外部工具的文档请先确认替代做法。";

/// Tectonic 资源配置（bundle / 缓存目录）；形态位在 v3 随子进程形态一并删除。
///
/// **只在全局设置里**（不进 [`ProjectOverrides`]）：装配点**每趟编译读一次全局设置**（见
/// `src-tauri` 的 runner 装配）；项目级覆盖要走"资源随 `CompileRequest` 下发"那条路，属后续项
/// ——现在放一个项目级字段只会是**看着能覆盖、实际不生效**的假开关。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TectonicSettings {
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
            bundle: None,
            cache_dir: None,
        }
    }
}

impl TectonicSettings {
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
    /// schema 版本。
    ///
    /// `#[serde(default)]` 是**兜底**：极老的 `settings.json` 可能没有这个键，缺键不该让整份
    /// 配置解析失败（用户设置全丢）。真正的 v1 → v2 迁移在 infra 的存储入口完成（先读 Value 判版本，
    /// 不依赖这个字段）；这里只保证"缺键也能读出一个可用对象"。
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub compile: CompileSettings,
    /// Tectonic **资源**（全局：bundle 源 / 缓存目录；形态自 v3 起唯一 = 内嵌，见 [`TectonicSettings`]）。
    #[serde(default)]
    pub tectonic: TectonicSettings,
    /// 界面（全局；`#[serde(default)]` ⇒ 旧 settings.json 没有 `ui` 也能读）。
    #[serde(default)]
    pub ui: UiSettings,
    /// 一次性提示（见 [`Notice`]）；空表不落盘。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<Notice>,
    /// 根文件手动覆盖（探测结果的逃生门，ADR-0009）。
    pub root_file: Option<PathBuf>,
}

/// 缺 `schema_version` 键时的兜底值（迁移判定不依赖它，见字段文档）。
fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            schema_version: SCHEMA_VERSION,
            compile: CompileSettings {
                mode: CompileMode::Continuous,
                debounce_ms: 500,
                timeout_secs: 120,
                // 默认开：新建第一个 .tex 时给新手一份能直接编译的骨架（roadmap ㊺ §6.13.1-A）
                new_file_wizard: true,
            },
            tectonic: TectonicSettings::default(),
            ui: UiSettings::default(),
            notices: Vec::new(),
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
    /// 新建向导开关（roadmap ㊺ §6.13.1-A）：`None` = 不动。**全局**设置，无项目覆盖
    /// （它是"界面要不要拦一下"，与具体项目无关）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_file_wizard: Option<bool>,
    /// 关掉一条一次性提示（[`Notice::id`]）：`None` = 不动；空串 / 未知 id 无害。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismiss_notice: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_root_file_patch"
    )]
    #[specta(type = Option<Option<PathBuf>>)]
    pub root_file: Option<Option<PathBuf>>,
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
        assert_eq!(SCHEMA_VERSION, 3, "v3 = 形态收成内嵌唯一（ADR-0015 决策 2）");
        assert_eq!(s.compile.mode, CompileMode::Continuous);
        assert_eq!(s.compile.debounce_ms, 500);
        assert_eq!(s.compile.timeout_secs, 120);
        assert_eq!(s.tectonic, TectonicSettings::default(), "默认没有资源覆盖");
        assert!(s.notices.is_empty(), "默认不该带迁移提示");
        assert_eq!(s.root_file, None);
    }

    /// `schema_version` 缺失（极老文件）要能读，且不因缺键而整份丢失。
    #[test]
    fn settings_without_schema_version_still_deserializes() {
        let json = r#"{"compile":{"mode":"continuous","debounce_ms":500,"timeout_secs":120}}"#;
        let s: Settings = serde_json::from_str(json).expect("缺 schema_version 必须能读");
        assert_eq!(s.schema_version, SCHEMA_VERSION, "缺键按当前版本兜底");
        assert_eq!(s.compile.timeout_secs, 120);
    }

    /// 旧 JSON 里的 `compile.engine` 必须**静默忽略**（不加 `deny_unknown_fields`）：
    /// ADR-0015 要求"旧值不作为错误"。
    #[test]
    fn legacy_engine_key_is_ignored_not_rejected() {
        let json = r#"{"schema_version":1,"compile":{"mode":"continuous","debounce_ms":500,
            "timeout_secs":120,"engine":"xelatex"},"root_file":null}"#;
        let s: Settings = serde_json::from_str(json).expect("旧 engine 键必须被忽略");
        assert_eq!(s.compile.timeout_secs, 120);
        // 项目覆盖同理：旧 `compile.engine` 不报错、也不影响合并
        let o: ProjectOverrides = serde_json::from_str(r#"{"compile":{"engine":"lualatex"}}"#)
            .expect("项目覆盖里的旧 engine 必须被忽略");
        assert_eq!(o.compile.unwrap().timeout_secs, None);
    }

    /// v3 删掉了形态位：旧文件里残留的形态位键必须**静默忽略**
    /// （同 `compile.engine`：不加 `deny_unknown_fields`），同段资源照常读出，
    /// 且新写出的 JSON 不再带这个键。
    #[test]
    fn legacy_form_key_is_ignored_not_rejected() {
        let json = r#"{"schema_version":2,"compile":{"mode":"continuous","debounce_ms":500,
            "timeout_secs":120},"tectonic":{"lib_form":true,
            "bundle":"file:///E:/Works/bundle","cache_dir":null},"root_file":null}"#;
        let s: Settings = serde_json::from_str(json).expect("旧形态位必须被忽略");
        assert_eq!(
            s.tectonic.bundle.as_deref(),
            Some("file:///E:/Works/bundle"),
            "同一段里的资源设置要保留"
        );
        let reserialized = serde_json::to_value(&s.tectonic).unwrap();
        assert!(
            reserialized.get("lib_form").is_none(),
            "v3 的 tectonic 不再有形态位：{reserialized}"
        );
    }

    /// `notices` 空表不落盘（老前端不会看到多出来的键），有值时正常往返。
    #[test]
    fn notices_serde_and_skip_when_empty() {
        let json = serde_json::to_string(&Settings::default()).unwrap();
        assert!(!json.contains("notices"), "空表不该出现在磁盘上：{json}");

        let n = Notice {
            id: ENGINE_UNIFIED_NOTICE_ID.to_owned(),
            level: NoticeLevel::Info,
            message: ENGINE_UNIFIED_NOTICE_MESSAGE.to_owned(),
        };
        let s = Settings { notices: vec![n.clone()], ..Settings::default() };
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.notices, vec![n]);
        // 级别用 snake_case（DTO 口径）
        assert!(json.contains("\"level\":\"info\""), "{json}");
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
        // 旧前端仍可能发旧形态位键：按未知字段忽略、不报错（v3 已无这个设置）
        let legacy: SettingsPatch =
            serde_json::from_str(r#"{"lib_form":true,"timeout_secs":60}"#).unwrap();
        assert_eq!(legacy.timeout_secs, Some(60), "同批合法键仍要生效");
        assert_eq!(legacy, SettingsPatch { timeout_secs: Some(60), ..Default::default() });
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
}
