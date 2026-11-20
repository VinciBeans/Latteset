//! 设置存储（modules.md §6）：全局 settings.json + 项目 .latteset/settings.json。
//!
//! - 原子写：临时文件 + rename（防崩溃截断）；
//! - 自写盘过滤：写入时记录内容 hash，watch 事件比对后消费（设计决策 D6）；
//! - **v1 → v2 → v3 迁移**（ADR-0015）：旧文件读入时**逐级**改写并如实告知——
//!   * v1 → v2：`compile.engine` 不再有对应字段（旧值忽略）；
//!   * v2 → v3（ADR-0015 决策 2）：形态收成内嵌唯一，`tectonic` 里的形态位被删除。
//!   原文按**迁移源版本**留备份（`settings.json.v1.bak` / `.v2.bak`，已有不覆盖），
//!   v3 追加一次性提示（见 [`migrate_global_value`]）。

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use latteset_core::project::FileSystem;
use latteset_core::settings::{
    merge, sanitize_overrides, validate, Notice, NoticeLevel, ProjectOverrides, Settings,
    ENGINE_UNIFIED_NOTICE_ID, ENGINE_UNIFIED_NOTICE_MESSAGE, FORM_EMBEDDED_NOTICE_ID,
    FORM_EMBEDDED_NOTICE_MESSAGE, SCHEMA_VERSION, SUBPROCESS_FORM_REMOVED_NOTICE_ID,
    SUBPROCESS_FORM_REMOVED_NOTICE_MESSAGE,
};
use tracing::{error, info, warn};

pub struct SettingsStorage {
    /// 全局设置文件路径（app_config_dir/settings.json）。
    global_path: PathBuf,
    /// 最近一次写入的内容 hash（自写盘过滤，D6）。
    last_write: Mutex<HashMap<PathBuf, u64>>,
}

/// v1 → v2 的版本门槛（v2 步只处理 `version < 2` 或残留 `compile.engine`）。
const V2: u32 = 2;

/// 一次迁移的结果：要追加的一次性提示 + 迁移源版本（决定备份名）。
///
/// 为什么需要多条提示：v2 → v3 有两条（形态固定的通用告知 + 子进程旧用户的专属警告），
/// 而一次迁移只落一次盘，所以提示必须成批返回。`backup_from` 取**最早一级**的源版本：
/// v1 文件一次升到 v3 时留的是那份 v1 原文（`.v1.bak`），不会凭空生出一份 `.v2.bak`。
#[derive(Debug)]
pub struct Migration {
    /// 要追加的提示（调用方按 `id` 去重后落盘）。
    pub notices: Vec<Notice>,
    /// 迁移源版本：`settings.json` → `settings.json.v{from}.bak`。
    pub backup_from: u32,
}

/// v1 → v2 → v3 迁移判定（ADR-0015）：**原地**改写 JSON，返回要追加的一次性提示与备份版本。
///
/// 两级判据（任一即迁移，可同时触发）：
/// - **v1 → v2**：`schema_version < 2`，或还存在 `compile.engine` 键；
/// - **v2 → v3**：`schema_version < 3`，或还存在 `tectonic` 的旧形态位键。
///
/// 迁移动作：版本写成 3、删掉 `compile.engine`、删掉旧形态位（旧值**静默忽略**，不作为错误
/// ——ADR-0015 第 3 条）。v3 提示：`form-embedded-v3`（info，恒加）+ `subprocess-form-removed-v3`
/// （warning，仅当旧形态位显式为"子进程档"= `false`——那是唯一可能依赖"引擎调本机 biber"的人，
/// ADR-0015 第 4 条不静默降级）。
///
/// **幂等**：版本已是 3 且没有旧键时返回 `None`，一个字节都不改。
///
/// 为什么是 `pub`：`watch.rs` 的热更新解析路径也要走同一份判定。两处各写一份会漂移——一处迁移、
/// 一处不迁移 ⇒ 用户手改 `settings.json` 后提示又冒出来，或者版本永远停在旧值。
pub fn migrate_global_value(value: &mut serde_json::Value) -> Option<Migration> {
    let version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
        .min(u64::from(u32::MAX)) as u32;
    let has_legacy_engine = value
        .get("compile")
        .and_then(|c| c.get("engine"))
        .is_some();
    let legacy_form = value
        .get("tectonic")
        .and_then(|t| t.get("lib_form"))
        .and_then(serde_json::Value::as_bool);

    let needs_v2 = version < V2 || has_legacy_engine;
    let needs_v3 = version < SCHEMA_VERSION || legacy_form.is_some();
    if !needs_v2 && !needs_v3 {
        return None;
    }

    let mut notices = Vec::new();
    if needs_v2 {
        notices.push(Notice {
            id: ENGINE_UNIFIED_NOTICE_ID.to_owned(),
            level: NoticeLevel::Info,
            message: ENGINE_UNIFIED_NOTICE_MESSAGE.to_owned(),
        });
    }
    if needs_v3 {
        notices.push(Notice {
            id: FORM_EMBEDDED_NOTICE_ID.to_owned(),
            level: NoticeLevel::Info,
            message: FORM_EMBEDDED_NOTICE_MESSAGE.to_owned(),
        });
        if legacy_form == Some(false) {
            notices.push(Notice {
                id: SUBPROCESS_FORM_REMOVED_NOTICE_ID.to_owned(),
                level: NoticeLevel::Warning,
                message: SUBPROCESS_FORM_REMOVED_NOTICE_MESSAGE.to_owned(),
            });
        }
    }

    // 备份名取**最早一级**的源版本：v1 一次升到 v3 也只留一份 v1 原文
    // （缺版本号的极老文件按 v1 处理，与既有口径一致）。
    let backup_from = if needs_v2 { version.max(1) } else { version };

    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "schema_version".to_owned(),
            serde_json::Value::from(SCHEMA_VERSION),
        );
        if let Some(compile) = obj.get_mut("compile").and_then(serde_json::Value::as_object_mut) {
            compile.remove("engine");
        }
        if let Some(tectonic) = obj.get_mut("tectonic").and_then(serde_json::Value::as_object_mut) {
            tectonic.remove("lib_form");
        }
    }
    Some(Migration { notices, backup_from })
}

/// 迁移原文备份路径：`settings.json` → `settings.json.v{from_version}.bak`。
///
/// 按**迁移源版本**命名：v1 原文留 `.v1.bak`、v2 原文留 `.v2.bak`（契约的一部分，排障要看
/// "升级前是什么"）。同一文件逐级迁移只留最早那份原文——它就是升级前的磁盘形态。
pub fn backup_path(global_path: &Path, from_version: u32) -> PathBuf {
    global_path.with_extension(format!("json.v{from_version}.bak"))
}

/// 解析失败（文件损坏）时的原文备份路径：`settings.json` → `settings.json.corrupt.bak`。
pub fn corrupt_backup_path(global_path: &Path) -> PathBuf {
    global_path.with_extension("json.corrupt.bak")
}

impl SettingsStorage {
    pub fn new(global_path: PathBuf) -> Self {
        // 启动时唯一需要检查的地方：已删除的环境变量（`LATTESET_TEX_ENGINES` /
        // `LATTESET_TECTONIC_LIB`）。放在这里是因为 `SettingsStorage::new` 在两个入口的装配期
        // 都会走到，而 infra 没有更早的启动钩子。
        crate::warn_removed_env_overrides();
        Self {
            global_path,
            last_write: Mutex::new(HashMap::new()),
        }
    }

    pub fn global_path(&self) -> &Path {
        &self.global_path
    }

    fn hash(content: &str) -> u64 {
        let mut h = DefaultHasher::new();
        content.hash(&mut h);
        h.finish()
    }

    /// 读取全局设置；缺失 → 默认值并落盘；损坏 → **先备份原文**再回退默认值
    /// （错误记录，不阻塞启动）；解析成功但范围非法（如手工/陈旧 timeout_secs=1）→ 回退默认，
    /// 避免越界值被直接使用。
    ///
    /// v1 → v2 → v3 的迁移在 [`Self::parse_global`] 里完成（含备份与落盘）。
    pub async fn load_global(&self, fs: &dyn FileSystem) -> Settings {
        let text = match fs.read_to_string(&self.global_path).await {
            Ok(t) => t,
            Err(_) => {
                let d = Settings::default();
                self.save_global(&d).await;
                return d;
            }
        };
        match self.parse_global(&text).await {
            Ok(s) if validate(&s).is_ok() => s,
            Ok(_) => {
                error!("全局设置范围非法，使用默认值");
                let d = Settings::default();
                self.save_global(&d).await;
                d
            }
            Err(e) => {
                error!("全局设置损坏，使用默认值：{e}");
                // 回退默认前把原文留一份（review E5 ②）：不备份就等于把用户手写的设置
                // （root_file、超时等）连同损坏一起静默丢掉。已有备份则不覆盖。
                self.backup_corrupt().await;
                let d = Settings::default();
                self.save_global(&d).await;
                d
            }
        }
    }

    /// 解析全局设置文本，需要时执行 v1 → v2 → v3 迁移（备份原文 + 落盘 + 追加一次性提示）。
    ///
    /// `Err` = 文本不可解析（调用方决定回退默认 / watch 重试）。
    /// 抽成方法是为了让 `load_global`（启动）与 `watch`（热更新）共用同一条迁移路径。
    ///
    /// 三个边界（review E5）：
    /// - **BOM**：Windows 记事本"UTF-8"另存默认带 BOM，直接 `serde_json` 会解析失败 ⇒
    ///   先把 BOM 剥掉再解析（剥的是文本副本，落盘/备份不受影响）。
    /// - **已有备份不覆盖**：迁移可能被走两次（只读文件、崩溃回滚），无条件 `copy` 会把**最早那份
    ///   原始备份**换成本次（可能已被别的工具改过的）内容 ⇒ 最早原文永久丢失。备份名按迁移源版本
    ///   分开（`.v1.bak` / `.v2.bak`），各自"已有不覆盖"。
    /// - **写盘失败**（文件只读/权限）：仍按迁移后的值在内存生效（配置不丢），但**不追加一次性
    ///   提示**——落不了盘 ⇒ 下次启动还会重新迁移，提示就会每次启动弹一次，纯骚扰；
    ///   只打一条 `warn`（可见、每次启动一条，不刷屏）。
    pub async fn parse_global(&self, text: &str) -> Result<Settings, String> {
        // 剥 BOM（`U+FEFF`）：只影响解析视角，不影响备份/落盘的原始字节
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("解析失败：{e}"))?;
        let migration = migrate_global_value(&mut value);
        let mut settings: Settings =
            serde_json::from_value(value).map_err(|e| format!("解析失败：{e}"))?;

        let Some(migration) = migration else {
            return Ok(settings);
        };

        // 迁移后的落盘对象：当前版本 + 一次性提示（按 id 去重，别因重复迁移堆多条）。
        settings.schema_version = SCHEMA_VERSION;
        let mut added_ids = Vec::new();
        for notice in migration.notices {
            if settings.notices.iter().any(|n| n.id == notice.id) {
                continue;
            }
            added_ids.push(notice.id.clone());
            settings.notices.push(notice);
        }
        // 原文件复制一份：用户/排障要能看到"升级前是什么"。失败只警告，不阻塞启动。
        self.backup_migrated(migration.backup_from).await;
        warn!(
            path = %self.global_path.display(),
            backup_from = migration.backup_from,
            "全局设置已迁移到 v{SCHEMA_VERSION}：旧引擎/形态字段已删除，原值被忽略（ADR-0015）"
        );
        let persisted = self.save_global(&settings).await;
        if !persisted {
            // 迁移落不了盘 ⇒ 下轮启动会重走迁移；此时把**本次新增**的提示收回，避免每次启动弹一次。
            // 设置本身不丢：本次会话按迁移后的值生效（旧字段已忽略），磁盘原文件保持原样。
            // 只收本次新增的：文件里原本就有的提示（用户还没 dismiss 的旧提示）不能被连带清掉。
            settings.notices.retain(|n| !added_ids.contains(&n.id));
            warn!(
                path = %self.global_path.display(),
                "全局设置迁移无法落盘（文件只读或权限不足）：本次按迁移后的值在内存生效，磁盘仍是旧版本；\
                 不追加一次性提示以免每次启动重复弹出"
            );
        }
        Ok(settings)
    }

    /// 迁移原文备份（**已有则不覆盖**，review E5 ①）。
    async fn backup_migrated(&self, from_version: u32) {
        let backup = backup_path(&self.global_path, from_version);
        if backup.exists() {
            info!(
                path = %backup.display(),
                "v{from_version} 迁移备份已存在，保留最早那份（不覆盖）"
            );
            return;
        }
        match tokio::fs::copy(&self.global_path, &backup).await {
            Ok(_) => info!(path = %backup.display(), "已备份 v{from_version} 全局设置"),
            Err(e) => warn!(
                path = %backup.display(),
                "备份 v{from_version} 全局设置失败（迁移继续）：{e}"
            ),
        }
    }

    /// 损坏原文备份（**已有则不覆盖**，review E5 ②）。
    async fn backup_corrupt(&self) {
        let backup = corrupt_backup_path(&self.global_path);
        if backup.exists() {
            warn!(path = %backup.display(), "损坏设置的备份已存在，保留最早那份（不覆盖）");
            return;
        }
        match tokio::fs::copy(&self.global_path, &backup).await {
            Ok(_) => warn!(
                path = %backup.display(),
                "全局设置无法解析，已把原文备份为该文件后回退默认值"
            ),
            Err(e) => warn!(path = %backup.display(), "备份损坏的全局设置失败：{e}"),
        }
    }

    /// 原子写全局设置，并记录内容 hash 供自写盘过滤。
    ///
    /// 返回**是否真的落盘**：调用方（迁移路径）据此决定要不要把一次性提示留在内存里
    /// （写不进去就别提示，见 [`Self::parse_global`]）。
    pub async fn save_global(&self, s: &Settings) -> bool {
        let text = match serde_json::to_string_pretty(s) {
            Ok(t) => t,
            Err(e) => {
                error!("全局设置序列化失败：{e}");
                return false;
            }
        };
        let hash = Self::hash(&text);
        if let Err(e) = atomic_write(&self.global_path, &text).await {
            error!(
                path = %self.global_path.display(),
                "全局设置写盘失败（文件可能只读/权限不足）：{e}"
            );
            return false;
        }
        self.last_write
            .lock()
            .unwrap()
            .insert(self.global_path.clone(), hash);
        true
    }

    /// 读取项目覆盖；缺失 → 空覆盖（全继承全局）；损坏 → 忽略覆盖；
    /// 个别字段非法（越界/.. root_file）→ 逐字段清除（sanitize），保留其它合法覆盖（避免连带丢弃）。
    ///
    /// 旧项目文件里的 `compile.engine` **被忽略即可**：不报错、也**不重写用户项目文件**
    /// （只打一条 info，让排障知道这个键已失效）。
    pub async fn load_overrides(&self, fs: &dyn FileSystem, project_root: &Path) -> ProjectOverrides {
        let path = project_overrides_path(project_root);
        match fs.read_to_string(&path).await {
            Ok(text) => {
                if has_legacy_engine_key(&text) {
                    info!(
                        path = %path.display(),
                        "项目设置里的 `compile.engine` 已废弃（引擎已统一为 Tectonic），忽略该键；不重写项目文件"
                    );
                }
                match serde_json::from_str::<ProjectOverrides>(&text) {
                    Ok(mut o) => {
                        sanitize_overrides(&mut o);
                        o
                    }
                    Err(e) => {
                        error!("项目设置损坏，忽略覆盖：{e}");
                        ProjectOverrides::default()
                    }
                }
            }
            Err(_) => ProjectOverrides::default(),
        }
    }

    /// 原子写项目覆盖 + 记录 hash。
    pub async fn save_overrides(&self, project_root: &Path, o: &ProjectOverrides) {
        let path = project_overrides_path(project_root);
        let text = match serde_json::to_string_pretty(o) {
            Ok(t) => t,
            Err(e) => {
                error!("项目设置序列化失败：{e}");
                return;
            }
        };
        let hash = Self::hash(&text);
        if let Err(e) = atomic_write(&path, &text).await {
            error!("项目设置写盘失败：{e}");
            return;
        }
        self.last_write.lock().unwrap().insert(path, hash);
    }

    /// 自写盘过滤（D6）：watch 事件到达时，若内容 hash 与上次写入一致则消费并跳过。
    pub fn is_self_write(&self, path: &Path, content: &str) -> bool {
        let mut map = self.last_write.lock().unwrap();
        let expected = map.remove(path);
        match expected {
            Some(h) => h == Self::hash(content),
            None => false,
        }
    }

    /// 计算有效设置：全局 + 项目覆盖（modules.md §6 merge）。
    pub fn effective(global: &Settings, overrides: &ProjectOverrides) -> Settings {
        merge(global, overrides)
    }
}

/// 项目覆盖文件路径（.latteset/settings.json）。
pub fn project_overrides_path(project_root: &Path) -> PathBuf {
    project_root.join(".latteset").join("settings.json")
}

/// 文本里是否还有已废弃的 `compile.engine` 键（只用于打一条 info，不影响解析）。
fn has_legacy_engine_key(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| v.get("compile").and_then(|c| c.get("engine")).cloned())
        .is_some()
}

/// 原子写：临时文件 + rename（防崩溃截断；modules.md §6）。
/// 原子写是基础设施层的专属原语（create_dir_all + rename 组合）：core 的 FileSystem::write
/// 是给上层命令面用的通用写路径，不含"临时文件 + rename"语义，故在此独立实现。
async fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    // 父目录可能不存在（如项目覆盖 .latteset/）：先创建，否则 write 直接失败，
    // 覆盖只留在内存（重启即丢），表现为“设置/清除不持久”。
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, content).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::TokioFs;
    use latteset_core::settings::model::CompileOverrides;
    use latteset_core::settings::TectonicSettings;

    /// 每个用例一个独立临时目录（含 `<pid>`，避免并行测试互相踩）。
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("latteset-storage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// v1 设置（带 `engine` 与旧形态位）——升级前磁盘上的真实形态。
    const V1_JSON: &str = r#"{
  "schema_version": 1,
  "compile": {
    "mode": "on_save",
    "debounce_ms": 400,
    "timeout_secs": 77,
    "engine": "xelatex",
    "new_file_wizard": false
  },
  "tectonic": {
    "lib_form": true
  },
  "root_file": "main.tex"
}"#;

    /// v2 设置且形态位 `true`（库内嵌档）：阶段① 会落盘的形状，含资源设置。
    const V2_JSON_LIB_TRUE: &str = r#"{
  "schema_version": 2,
  "compile": {
    "mode": "on_save",
    "debounce_ms": 400,
    "timeout_secs": 77,
    "new_file_wizard": false
  },
  "tectonic": {
    "lib_form": true,
    "bundle": "file:///E:/Works/bundle",
    "cache_dir": null
  },
  "root_file": "main.tex"
}"#;

    /// v2 设置且形态位 `false`（子进程档）：唯一需要额外"子进程形态已删除"警告的老用户。
    const V2_JSON_LIB_FALSE: &str = r#"{
  "schema_version": 2,
  "compile": {
    "mode": "continuous",
    "debounce_ms": 500,
    "timeout_secs": 120,
    "new_file_wizard": true
  },
  "tectonic": {
    "lib_form": false,
    "bundle": null,
    "cache_dir": null
  },
  "root_file": null
}"#;

    /// v3 设置（当前形状）：不该被迁移触碰。
    const V3_JSON: &str = r#"{
  "schema_version": 3,
  "compile": {
    "mode": "continuous",
    "debounce_ms": 500,
    "timeout_secs": 120,
    "new_file_wizard": true
  },
  "tectonic": {
    "bundle": null,
    "cache_dir": null
  },
  "root_file": null
}"#;

    #[test]
    fn effective_merges_overrides() {
        let g = Settings::default();
        let mut o = ProjectOverrides::default();
        o.compile = Some(CompileOverrides {
            timeout_secs: Some(60),
            ..Default::default()
        });
        let s = SettingsStorage::effective(&g, &o);
        assert_eq!(s.compile.timeout_secs, 60);
        // 未覆盖字段继承全局
        assert_eq!(s.compile.debounce_ms, g.compile.debounce_ms);
    }

    #[test]
    fn project_overrides_path_is_dot_latteset_settings() {
        assert_eq!(
            project_overrides_path(Path::new(r"C:\proj")),
            PathBuf::from(r"C:\proj\.latteset\settings.json")
        );
    }

    // ---- v1 → v2 → v3 迁移（ADR-0015）----

    /// v1 一次升到 v3：版本 3、旧 engine 与旧形态位被忽略、v1 原文备份、追加 v3 提示、其余设置不变。
    ///
    /// V1_JSON 的形态位是 `true`（库内嵌档）⇒ **不该**收到"子进程形态已删除"警告。
    #[tokio::test]
    async fn migrates_v1_with_engine_and_keeps_other_settings() {
        let dir = temp_dir("migrate");
        let path = dir.join("settings.json");
        std::fs::write(&path, V1_JSON).unwrap();
        let s = SettingsStorage::new(path.clone());

        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded.schema_version, SCHEMA_VERSION);
        assert_eq!(SCHEMA_VERSION, 3, "阶段②的当前版本");
        assert_eq!(loaded.compile.mode, latteset_core::types::CompileMode::OnSave);
        assert_eq!(loaded.compile.debounce_ms, 400, "其余字段必须原样保留");
        assert_eq!(loaded.compile.timeout_secs, 77);
        assert!(!loaded.compile.new_file_wizard);
        assert_eq!(loaded.tectonic, TectonicSettings::default(), "旧文件没给资源，迁移后仍是默认");
        assert_eq!(
            loaded.root_file.as_deref(),
            Some(Path::new("main.tex")),
            "root_file 覆盖要保持"
        );

        // 一次性提示：v1 升 v3 恰好两条（引擎统一 + 形态固定），id/级别/文案固定
        assert_eq!(loaded.notices.len(), 2, "{:?}", loaded.notices);
        assert_eq!(loaded.notices[0].id, ENGINE_UNIFIED_NOTICE_ID);
        assert_eq!(loaded.notices[0].level, NoticeLevel::Info);
        assert_eq!(loaded.notices[0].message, ENGINE_UNIFIED_NOTICE_MESSAGE);
        assert_eq!(loaded.notices[1].id, FORM_EMBEDDED_NOTICE_ID);
        assert_eq!(loaded.notices[1].level, NoticeLevel::Info);
        assert_eq!(loaded.notices[1].message, FORM_EMBEDDED_NOTICE_MESSAGE);
        assert!(
            loaded.notices.iter().all(|n| n.id != SUBPROCESS_FORM_REMOVED_NOTICE_ID),
            "形态位 true（库内嵌档）不该收到子进程警告：{:?}",
            loaded.notices
        );

        // 原文件已备份（内容 = 迁移前的 v1 原文，文件名按迁移源版本 `settings.json.v1.bak`）
        let backup = backup_path(&path, 1);
        assert_eq!(
            backup.file_name().and_then(|n| n.to_str()),
            Some("settings.json.v1.bak"),
            "备份文件名是契约的一部分"
        );
        assert!(backup.exists(), "必须留 settings.json.v1.bak");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), V1_JSON);
        assert!(
            !backup_path(&path, 2).exists(),
            "v1 一次升到 v3 只留一份原文备份（不凭空生出 .v2.bak）"
        );

        // 落盘文件已是 v3、没有 engine / 旧形态位、带两条提示
        let persisted = std::fs::read_to_string(&path).unwrap();
        assert!(!persisted.contains("\"engine\""), "落盘后不应再有 engine 键：{persisted}");
        assert!(!persisted.contains("\"lib_form\""), "落盘后不应再有旧形态位：{persisted}");
        let rewritten: Settings = serde_json::from_str(&persisted).unwrap();
        assert_eq!(rewritten.schema_version, 3);
        assert_eq!(rewritten.notices.len(), 2);
        assert_eq!(rewritten.compile.timeout_secs, 77);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 迁移**幂等**：再读一次不重复加提示、不再改文件（备份保持原文）。
    #[tokio::test]
    async fn migration_is_idempotent() {
        let dir = temp_dir("migrate-idempotent");
        let path = dir.join("settings.json");
        std::fs::write(&path, V1_JSON).unwrap();
        let s = SettingsStorage::new(path.clone());

        let first = s.load_global(&TokioFs).await;
        let after_first = std::fs::read_to_string(&path).unwrap();
        let second = s.load_global(&TokioFs).await;
        let after_second = std::fs::read_to_string(&path).unwrap();

        assert_eq!(first.notices, second.notices, "第二次读不得追加提示");
        assert_eq!(second.notices.len(), 2);
        assert_eq!(after_first, after_second, "v3 已是幂等，不该再改写文件");
        assert_eq!(std::fs::read_to_string(backup_path(&path, 1)).unwrap(), V1_JSON);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v3 且无旧键：**一个字节都不改**、不备份、不加提示。
    #[tokio::test]
    async fn v3_settings_are_untouched() {
        let dir = temp_dir("v3-clean");
        let path = dir.join("settings.json");
        std::fs::write(&path, V3_JSON).unwrap();
        let s = SettingsStorage::new(path.clone());

        let loaded = s.load_global(&TokioFs).await;
        assert!(loaded.notices.is_empty(), "v3 文件不该冒提示：{:?}", loaded.notices);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), V3_JSON, "v3 文件不该被重写");
        assert!(!backup_path(&path, 1).exists(), "不需要迁移就不备份 v1");
        assert!(!backup_path(&path, 2).exists(), "不需要迁移就不备份 v2");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v2（形态位 `true` = 库内嵌档）→ v3：只加"形态已固定"一条 info，资源设置原样保留。
    #[tokio::test]
    async fn migrates_v2_lib_true_without_subprocess_warning() {
        let dir = temp_dir("migrate-v2-lib-true");
        let path = dir.join("settings.json");
        std::fs::write(&path, V2_JSON_LIB_TRUE).unwrap();
        let s = SettingsStorage::new(path.clone());

        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded.schema_version, 3);
        assert_eq!(loaded.compile.timeout_secs, 77, "其余字段必须原样保留");
        assert_eq!(
            loaded.tectonic.bundle.as_deref(),
            Some("file:///E:/Works/bundle"),
            "资源设置要保留"
        );
        assert_eq!(loaded.notices.len(), 1, "{:?}", loaded.notices);
        assert_eq!(loaded.notices[0].id, FORM_EMBEDDED_NOTICE_ID);
        assert!(
            loaded.notices.iter().all(|n| n.id != SUBPROCESS_FORM_REMOVED_NOTICE_ID),
            "库内嵌档不该收到子进程警告：{:?}",
            loaded.notices
        );

        // 备份按迁移源版本命名：v2 原文 → settings.json.v2.bak
        let backup = backup_path(&path, 2);
        assert_eq!(
            backup.file_name().and_then(|n| n.to_str()),
            Some("settings.json.v2.bak"),
            "v2 → v3 的备份名是契约的一部分"
        );
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), V2_JSON_LIB_TRUE);
        assert!(!backup_path(&path, 1).exists(), "v2 迁移不该留 v1 备份");

        let persisted = std::fs::read_to_string(&path).unwrap();
        assert!(!persisted.contains("\"lib_form\""), "落盘后不应再有旧形态位：{persisted}");
        let rewritten: Settings = serde_json::from_str(&persisted).unwrap();
        assert_eq!(rewritten.schema_version, 3);
        assert_eq!(rewritten.notices.len(), 1);

        // 幂等：再读一次不追加、不改写
        let again = s.load_global(&TokioFs).await;
        assert_eq!(again.notices, loaded.notices);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), persisted);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v2（形态位 `false` = 子进程档）→ v3：额外追加 **warning**（不静默降级，ADR-0015 第 4 条）。
    #[tokio::test]
    async fn migrates_v2_subprocess_user_with_warning() {
        let dir = temp_dir("migrate-v2-subprocess");
        let path = dir.join("settings.json");
        std::fs::write(&path, V2_JSON_LIB_FALSE).unwrap();
        let s = SettingsStorage::new(path.clone());

        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded.schema_version, 3);
        assert_eq!(loaded.notices.len(), 2, "{:?}", loaded.notices);
        assert_eq!(loaded.notices[0].id, FORM_EMBEDDED_NOTICE_ID);
        assert_eq!(loaded.notices[0].level, NoticeLevel::Info);
        assert_eq!(loaded.notices[1].id, SUBPROCESS_FORM_REMOVED_NOTICE_ID);
        assert_eq!(loaded.notices[1].level, NoticeLevel::Warning);
        assert_eq!(
            loaded.notices[1].message, SUBPROCESS_FORM_REMOVED_NOTICE_MESSAGE,
            "子进程老用户要看到「不再调用本机 biber」这句"
        );
        assert_eq!(
            std::fs::read_to_string(backup_path(&path, 2)).unwrap(),
            V2_JSON_LIB_FALSE
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `schema_version` 缺失（极老文件）：按 v1 处理——能读、不丢字段、升到 3。
    #[tokio::test]
    async fn missing_schema_version_is_treated_as_v1() {
        let dir = temp_dir("no-version");
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"compile":{"mode":"on_save","debounce_ms":300,"timeout_secs":60},"root_file":null}"#,
        )
        .unwrap();
        let s = SettingsStorage::new(path.clone());

        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded.schema_version, 3, "缺键按 v1 迁移到当前版本");
        assert_eq!(loaded.compile.debounce_ms, 300, "缺版本号不影响其余字段");
        // 旧文件没有形态位 ⇒ 只有引擎统一 + 形态固定两条，不追加子进程警告
        assert_eq!(loaded.notices.len(), 2, "{:?}", loaded.notices);
        assert!(backup_path(&path, 1).exists(), "缺版本号按 v1 命名备份");
        let rewritten: Settings = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(rewritten.schema_version, 3);
        assert_eq!(rewritten.compile.timeout_secs, 60);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// v1 且**越界**：先迁移（留备份），再按既有语义回退默认并落盘。
    #[tokio::test]
    async fn out_of_range_v1_migrates_then_falls_back_to_default() {
        let dir = temp_dir("migrate-range");
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"schema_version":1,"compile":{"mode":"continuous","debounce_ms":50,"timeout_secs":1,"engine":"xelatex"},"root_file":null}"#,
        )
        .unwrap();
        let s = SettingsStorage::new(path.clone());

        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded, Settings::default(), "越界应回退默认");
        let rewritten: Settings = serde_json::from_str(&std::fs::read_to_string(&path).unwrap())
            .expect("回退后落盘内容应可解析");
        assert_eq!(rewritten, Settings::default(), "回退后应落盘默认值");
        assert!(backup_path(&path, 1).exists(), "迁移过的文件仍应留备份");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// review E5 ①：**已有 `settings.json.v1.bak` 时不得覆盖**。
    ///
    /// 场景：迁移走两次（文件只读导致第一次没落盘 → 下次启动重走；或用户手工改回旧版本）。
    /// 无条件 `copy` 会把最早那份原始备份换成本次内容 ⇒ 最早原文永久丢失。
    #[tokio::test]
    async fn existing_v1_backup_is_not_overwritten() {
        let dir = temp_dir("bak-keep");
        let path = dir.join("settings.json");
        let backup = backup_path(&path, 1);
        // 用户已有的原始备份（第一次迁移留下的最早原文）
        let earliest = r#"{"schema_version":1,"compile":{"timeout_secs":11,"engine":"xelatex"},"root_file":"最早的根文件.tex"}"#;
        std::fs::write(&backup, earliest).unwrap();
        // 现在的 settings.json 又是一份 v1（内容不同）
        std::fs::write(&path, V1_JSON).unwrap();
        let s = SettingsStorage::new(path.clone());

        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded.schema_version, SCHEMA_VERSION, "仍应完成迁移");
        assert_eq!(loaded.compile.timeout_secs, 77);
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            earliest,
            "已有的 v1 备份是最早原文，不能被本次迁移覆盖"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// review E5 ①：**已有 `settings.json.v2.bak` 时同样不得覆盖**（v2 → v3 这一级）。
    ///
    /// v1 备份与 v2 备份是两份独立的原文（版本不同、命名不同），互不影响。
    #[tokio::test]
    async fn existing_v2_backup_is_not_overwritten() {
        let dir = temp_dir("bak-v2-keep");
        let path = dir.join("settings.json");
        let backup = backup_path(&path, 2);
        let earliest =
            r#"{"schema_version":2,"compile":{"timeout_secs":11},"tectonic":{"lib_form":false},"root_file":"最早的根文件.tex"}"#;
        std::fs::write(&backup, earliest).unwrap();
        std::fs::write(&path, V2_JSON_LIB_FALSE).unwrap();
        let s = SettingsStorage::new(path.clone());

        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded.schema_version, 3, "仍应完成迁移");
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            earliest,
            "已有的 v2 备份是最早原文，不能被本次迁移覆盖"
        );
        assert!(!backup_path(&path, 1).exists(), "v2 → v3 不该顺手造 v1 备份");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// review E5 ②：**UTF-8 BOM** 的 v1 文件（Windows 记事本"UTF-8"另存默认带 BOM）。
    ///
    /// 修前：`serde_json` 见到 BOM 直接解析失败 ⇒ 静默重置为默认值、不留备份，
    /// 用户手写的 root_file / 超时全部丢失。修后：先剥 BOM 再解析（备份保留原始字节）。
    #[tokio::test]
    async fn bom_prefixed_v1_migrates_without_losing_settings() {
        let dir = temp_dir("bom");
        let path = dir.join("settings.json");
        let original = format!("\u{feff}{V1_JSON}");
        std::fs::write(&path, &original).unwrap();
        let s = SettingsStorage::new(path.clone());

        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded.schema_version, SCHEMA_VERSION, "带 BOM 也必须能迁移");
        assert_eq!(loaded.compile.timeout_secs, 77, "用户设置不许被重置：{loaded:?}");
        assert_eq!(
            loaded.root_file.as_deref(),
            Some(Path::new("main.tex")),
            "root_file 覆盖不许丢"
        );
        assert_eq!(loaded.notices.len(), 2, "{:?}", loaded.notices);
        assert_eq!(loaded.notices[0].id, ENGINE_UNIFIED_NOTICE_ID);
        assert_eq!(loaded.notices[1].id, FORM_EMBEDDED_NOTICE_ID);
        // 备份保留**原始字节**（含 BOM）：排障要看的就是磁盘上原本的样子
        assert_eq!(std::fs::read_to_string(backup_path(&path, 1)).unwrap(), original);
        // 落盘文件是合法 v3（可以被严格解析，且不再带 BOM / 旧形态位）
        let persisted = std::fs::read_to_string(&path).unwrap();
        assert!(!persisted.starts_with('\u{feff}'), "落盘文件不该带 BOM");
        assert!(!persisted.contains("\"lib_form\""), "落盘后不应再有旧形态位：{persisted}");
        assert_eq!(serde_json::from_str::<Settings>(&persisted).unwrap().schema_version, 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// review E5 ②：真解析失败（旧枚举值 `mode: "watch"`）⇒ **先备份原文再回退默认**。
    ///
    /// 修前：静默重置默认、无备份 ⇒ 用户手写的 root_file 被覆盖且无从找回。
    /// 备份文件固定 `settings.json.corrupt.bak`，且**不覆盖已有**（同 ①）。
    #[tokio::test]
    async fn corrupt_settings_are_backed_up_before_reset() {
        let dir = temp_dir("corrupt");
        let path = dir.join("settings.json");
        let corrupt = r#"{"schema_version":1,"compile":{"mode":"watch","debounce_ms":400,"timeout_secs":77,"engine":"xelatex"},"root_file":"珍贵的根文件.tex"}"#;
        std::fs::write(&path, corrupt).unwrap();
        let s = SettingsStorage::new(path.clone());

        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded, Settings::default(), "解析失败仍按既有语义回退默认");
        assert_eq!(
            std::fs::read_to_string(corrupt_backup_path(&path)).unwrap(),
            corrupt,
            "损坏原文必须留档（含 root_file）"
        );
        let rewritten: Settings = serde_json::from_str(&std::fs::read_to_string(&path).unwrap())
            .expect("回退后应落盘可解析的默认值");
        assert_eq!(rewritten, Settings::default());

        // 再损坏一次（内容不同）：已有 corrupt 备份不被覆盖
        let corrupt2 = r#"{"schema_version":1,"compile":{"mode":"watch"},"root_file":"另一份.tex"}"#;
        std::fs::write(&path, corrupt2).unwrap();
        let _ = s.load_global(&TokioFs).await;
        assert_eq!(
            std::fs::read_to_string(corrupt_backup_path(&path)).unwrap(),
            corrupt,
            "corrupt 备份同样保留最早那份"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 损坏的 **v2** 文件（带旧形态位 + 非法枚举值）：v3 迁移不改变损坏边界——先 `.corrupt.bak`
    /// 留档再回退默认；迁移备份（`.v2.bak`）不产生（文本都没解析成功，不能拿它当"原文"）。
    #[tokio::test]
    async fn corrupt_v2_settings_are_backed_up_before_reset() {
        let dir = temp_dir("corrupt-v2");
        let path = dir.join("settings.json");
        let corrupt = r#"{"schema_version":2,"compile":{"mode":"watch"},"tectonic":{"lib_form":false},"root_file":"珍贵的根文件.tex"}"#;
        std::fs::write(&path, corrupt).unwrap();
        let s = SettingsStorage::new(path.clone());

        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded, Settings::default(), "解析失败仍按既有语义回退默认");
        assert_eq!(
            std::fs::read_to_string(corrupt_backup_path(&path)).unwrap(),
            corrupt,
            "损坏原文必须留档"
        );
        assert!(!backup_path(&path, 2).exists(), "没解析成功就不该产生迁移备份");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// review E5 ③：**迁移落不了盘**（文件只读 / 权限不足）。
    ///
    /// 处置：迁移后的值在**内存**生效（配置不丢）、磁盘原文件保持 v1（用户配置不丢）、
    /// 但**不追加一次性提示**——落不了盘就每次启动都会重走迁移，提示会每次弹一次，纯骚扰。
    /// 失败本身走 `error!/warn!` 日志（可见），不靠 UI 提示。
    ///
    /// 这里用「`settings.json.tmp` 是个目录」让原子写的临时文件写不进去——跨平台可复现，
    /// 且正好打在 `atomic_write` 的真实失败点上。
    #[tokio::test]
    async fn unwritable_settings_keeps_notice_out_and_does_not_lose_config() {
        let dir = temp_dir("readonly");
        let path = dir.join("settings.json");
        std::fs::write(&path, V1_JSON).unwrap();
        // 原子写的临时文件路径被目录占住 → write 必失败（与只读文件的失败路径相同）
        std::fs::create_dir_all(path.with_extension("json.tmp")).unwrap();
        let s = SettingsStorage::new(path.clone());

        let first = s.load_global(&TokioFs).await;
        assert_eq!(first.schema_version, SCHEMA_VERSION, "迁移后的值要在内存生效");
        assert_eq!(first.compile.timeout_secs, 77, "配置不许丢：{first:?}");
        assert!(
            first.notices.is_empty(),
            "落不了盘就不该弹一次性提示（否则每次启动弹一次）：{:?}",
            first.notices
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            V1_JSON,
            "写不进去时磁盘原文件保持 v1（不丢用户配置）"
        );

        // 第二次启动：行为一致（不重复提示、不再覆盖备份）
        let second = s.load_global(&TokioFs).await;
        assert!(second.notices.is_empty(), "{:?}", second.notices);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), V1_JSON);
        assert_eq!(
            std::fs::read_to_string(backup_path(&path, 1)).unwrap(),
            V1_JSON,
            "备份保留 v1 原文，且不被第二次迁移覆盖"
        );
        assert!(
            !backup_path(&path, 2).exists(),
            "v1 一次升到 v3 只留一份原文备份"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 只读的 **v2** 文件（v3 步同一条失败路径）：内存按迁移后值生效、不弹提示、磁盘不动。
    #[tokio::test]
    async fn unwritable_v2_settings_keeps_notice_out() {
        let dir = temp_dir("readonly-v2");
        let path = dir.join("settings.json");
        std::fs::write(&path, V2_JSON_LIB_FALSE).unwrap();
        // 原子写的临时文件路径被目录占住 → write 必失败（跨平台可复现）
        std::fs::create_dir_all(path.with_extension("json.tmp")).unwrap();
        let s = SettingsStorage::new(path.clone());

        let first = s.load_global(&TokioFs).await;
        assert_eq!(first.schema_version, 3, "迁移后的值要在内存生效");
        assert_eq!(first.compile.timeout_secs, 120, "配置不许丢：{first:?}");
        assert!(
            first.notices.is_empty(),
            "落不了盘就不该弹一次性提示（否则每次启动弹一次）：{:?}",
            first.notices
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            V2_JSON_LIB_FALSE,
            "写不进去时磁盘原文件保持 v2"
        );
        // 备份仍留（它是迁移的证据，与能否落盘无关）
        assert_eq!(
            std::fs::read_to_string(backup_path(&path, 2)).unwrap(),
            V2_JSON_LIB_FALSE,
            "备份保留 v2 原文"
        );

        // 第二次启动：行为一致（不重复提示、不再覆盖备份）
        let second = s.load_global(&TokioFs).await;
        assert!(second.notices.is_empty(), "{:?}", second.notices);
        assert_eq!(
            std::fs::read_to_string(backup_path(&path, 2)).unwrap(),
            V2_JSON_LIB_FALSE
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// review E5 ③（Windows 真只读文件的同一行为，端到端形态）。
    ///
    /// 只在 Windows 断言：Unix 的 rename 由**目录**权限决定，只读目标文件仍可被原子替换，
    /// 造不出"写失败"。
    #[cfg(windows)]
    #[tokio::test]
    async fn read_only_settings_file_falls_back_without_nagging() {
        let dir = temp_dir("readonly-file");
        let path = dir.join("settings.json");
        std::fs::write(&path, V1_JSON).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms).unwrap();
        let s = SettingsStorage::new(path.clone());

        let first = s.load_global(&TokioFs).await;
        assert_eq!(first.compile.timeout_secs, 77, "只读也要按用户配置在内存生效");
        assert!(first.notices.is_empty(), "只读：不弹一次性提示：{:?}", first.notices);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), V1_JSON, "只读文件不被改写");

        let second = s.load_global(&TokioFs).await;
        assert!(second.notices.is_empty(), "第二次启动同样不弹：{:?}", second.notices);
        assert_eq!(std::fs::read_to_string(backup_path(&path, 1)).unwrap(), V1_JSON);

        // 收尾：恢复可写，否则 remove_dir_all 在 Windows 上删不掉
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(&path, perms).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 纯函数层面的迁移判定（watch 热更新复用同一份）：两级一起走、只留最早一级的备份名。
    #[test]
    fn migrate_global_value_is_idempotent_and_drops_legacy_keys() {
        let mut v: serde_json::Value = serde_json::from_str(V1_JSON).unwrap();
        let m = migrate_global_value(&mut v).expect("v1 应判定为需要迁移");
        assert_eq!(m.backup_from, 1, "v1 ⇒ 备份名 .v1.bak");
        assert_eq!(m.notices.len(), 2, "{:?}", m.notices);
        assert_eq!(m.notices[0].id, ENGINE_UNIFIED_NOTICE_ID);
        assert_eq!(m.notices[1].id, FORM_EMBEDDED_NOTICE_ID);
        assert_eq!(v.get("schema_version").and_then(|x| x.as_u64()), Some(3));
        assert!(v["compile"].get("engine").is_none(), "engine 键要删掉：{v}");
        assert!(v["tectonic"].get("lib_form").is_none(), "旧形态位要删掉：{v}");

        assert!(migrate_global_value(&mut v).is_none(), "已迁移的值再判一次应为 None");
    }

    /// 纯函数层面：v2 的两个形态位取值 → 提示矩阵与备份版本（`false` 才是子进程老用户）。
    #[test]
    fn migrate_global_value_v2_notice_matrix() {
        // 库内嵌档：只有"形态已固定"一条 info
        let mut v: serde_json::Value = serde_json::from_str(V2_JSON_LIB_TRUE).unwrap();
        let m = migrate_global_value(&mut v).expect("v2 应判定为需要迁移");
        assert_eq!(m.backup_from, 2, "v2 ⇒ 备份名 .v2.bak");
        assert_eq!(m.notices.len(), 1);
        assert_eq!(m.notices[0].id, FORM_EMBEDDED_NOTICE_ID);
        assert_eq!(m.notices[0].level, NoticeLevel::Info);
        assert_eq!(v["tectonic"].get("lib_form"), None, "旧形态位要删掉");

        // 子进程档：再加一条 warning（不静默降级）
        let mut v: serde_json::Value = serde_json::from_str(V2_JSON_LIB_FALSE).unwrap();
        let m = migrate_global_value(&mut v).expect("v2 应判定为需要迁移");
        assert_eq!(m.notices.len(), 2, "{:?}", m.notices);
        assert_eq!(m.notices[1].id, SUBPROCESS_FORM_REMOVED_NOTICE_ID);
        assert_eq!(m.notices[1].level, NoticeLevel::Warning);
        assert_eq!(m.notices[1].message, SUBPROCESS_FORM_REMOVED_NOTICE_MESSAGE);
        assert_eq!(v.get("schema_version").and_then(|x| x.as_u64()), Some(3));
        assert!(v["tectonic"].get("lib_form").is_none());

        // v3 且干净：None（幂等）
        let mut v: serde_json::Value = serde_json::from_str(V3_JSON).unwrap();
        assert!(migrate_global_value(&mut v).is_none(), "v3 干净文件不该判定为需要迁移");
    }

    // ---- 既有行为 ----

    #[tokio::test]
    async fn chinese_config_and_project_paths_round_trip() {
        // 模拟中文 Windows 用户名：app_config_dir 落在 `C:\Users\中文用户\AppData\...`，
        // 项目根也可能含中文。全局设置 + 项目覆盖都必须能原子读写。
        let base = std::env::temp_dir().join(format!("latteset-配置-{}", std::process::id()));
        let config_dir = base.join("用户配置");
        let project_root = base.join("项目").join("中文测试工程");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::create_dir_all(&project_root).unwrap();

        let s = SettingsStorage::new(config_dir.join("settings.json"));

        // 全局：写 → 读回一致（并验证原子写落盘）
        let mut custom = Settings::default();
        custom.compile.timeout_secs = 77;
        s.save_global(&custom).await;
        assert!(s.global_path().exists(), "中文配置目录应落盘 settings.json");
        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded.compile.timeout_secs, 77);

        // 项目覆盖：写 → 读回一致（中文项目根下的 .latteset/settings.json）
        let mut o = ProjectOverrides::default();
        o.root_file = Some("中文主文件.tex".into());
        s.save_overrides(&project_root, &o).await;
        let override_path = project_overrides_path(&project_root);
        assert!(override_path.exists(), "中文项目根应落盘 .latteset/settings.json");
        let loaded_o = s.load_overrides(&TokioFs, &project_root).await;
        assert_eq!(
            loaded_o.root_file.as_deref(),
            Some(Path::new("中文主文件.tex"))
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// 旧项目覆盖里的 `compile.engine`：忽略它、保留同文件的合法覆盖、**不重写项目文件**。
    #[tokio::test]
    async fn load_overrides_ignores_legacy_engine_key() {
        let root = temp_dir("overrides-legacy");
        let path = project_overrides_path(&root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let raw = r#"{"compile":{"timeout_secs":60,"engine":"lualatex"},"root_file":"main.tex"}"#;
        std::fs::write(&path, raw).unwrap();
        let s = SettingsStorage::new(root.join("settings.json"));

        let o = s.load_overrides(&TokioFs, &root).await;
        assert_eq!(o.compile.as_ref().unwrap().timeout_secs, Some(60), "合法覆盖要保留");
        assert_eq!(o.root_file.as_deref(), Some(Path::new("main.tex")));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            raw,
            "读覆盖不得重写用户项目文件"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn save_then_self_write_filter_consumes() {
        // 覆盖：写盘（原子写）→ 自写盘 hash 过滤消费一次 → 再次/不同内容为 false
        let dir = temp_dir("self-write");
        let path = dir.join("settings.json");
        let s = SettingsStorage::new(path.clone());
        s.save_global(&Settings::default()).await;
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(s.is_self_write(&path, &text), "首次应命中自写盘 hash");
        assert!(!s.is_self_write(&path, &text), "hash 应被消费一次");
        assert!(!s.is_self_write(&path, "other"), "不同内容不应命中");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn load_global_missing_returns_default_and_persists() {
        let dir = temp_dir("load-missing");
        let path = dir.join("settings.json");
        let s = SettingsStorage::new(path.clone());
        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded, Settings::default());
        assert!(path.exists(), "缺失时应落盘默认值");
        assert!(!backup_path(&path, 1).exists(), "全新装不该有 v1 备份");
        assert!(!backup_path(&path, 2).exists(), "全新装不该有 v2 备份");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
