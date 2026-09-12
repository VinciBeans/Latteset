//! 设置存储（modules.md §6）：全局 settings.json + 项目 .texpresso/settings.json。
//!
//! - 原子写：临时文件 + rename（防崩溃截断）；
//! - 自写盘过滤：写入时记录内容 hash，watch 事件比对后消费（设计决策 D6）。

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use texpresso_core::project::FileSystem;
use texpresso_core::settings::{merge, sanitize_overrides, validate, ProjectOverrides, Settings};
use tracing::error;

pub struct SettingsStorage {
    /// 全局设置文件路径（app_config_dir/settings.json）。
    global_path: PathBuf,
    /// 最近一次写入的内容 hash（自写盘过滤，D6）。
    last_write: Mutex<HashMap<PathBuf, u64>>,
}

impl SettingsStorage {
    pub fn new(global_path: PathBuf) -> Self {
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

    /// 读取全局设置；缺失 → 默认值并落盘；损坏 → 默认值（错误记录，不阻塞启动）；
    /// 解析成功但范围非法（如手工/陈旧 timeout_secs=1）→ 回退默认，避免越界值被直接使用。
    pub async fn load_global(&self, fs: &dyn FileSystem) -> Settings {
        match fs.read_to_string(&self.global_path).await {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) => {
                    if validate(&s).is_ok() {
                        s
                    } else {
                        error!("全局设置范围非法，使用默认值");
                        let d = Settings::default();
                        self.save_global(&d).await;
                        d
                    }
                }
                Err(e) => {
                    error!("全局设置损坏，使用默认值：{e}");
                    let d = Settings::default();
                    self.save_global(&d).await;
                    d
                }
            },
            Err(_) => {
                let d = Settings::default();
                self.save_global(&d).await;
                d
            }
        }
    }

    /// 原子写全局设置，并记录内容 hash 供自写盘过滤。
    pub async fn save_global(&self, s: &Settings) {
        let text = match serde_json::to_string_pretty(s) {
            Ok(t) => t,
            Err(e) => {
                error!("全局设置序列化失败：{e}");
                return;
            }
        };
        let hash = Self::hash(&text);
        if let Err(e) = atomic_write(&self.global_path, &text).await {
            error!("全局设置写盘失败：{e}");
            return;
        }
        self.last_write
            .lock()
            .unwrap()
            .insert(self.global_path.clone(), hash);
    }

    /// 读取项目覆盖；缺失 → 空覆盖（全继承全局）；损坏 → 忽略覆盖；
    /// 个别字段非法（越界/.. root_file）→ 逐字段清除（sanitize），保留其它合法覆盖（避免连带丢弃）。
    pub async fn load_overrides(&self, fs: &dyn FileSystem, project_root: &Path) -> ProjectOverrides {
        let path = project_overrides_path(project_root);
        match fs.read_to_string(&path).await {
            Ok(text) => match serde_json::from_str::<ProjectOverrides>(&text) {
                Ok(mut o) => {
                    sanitize_overrides(&mut o);
                    o
                }
                Err(e) => {
                    error!("项目设置损坏，忽略覆盖：{e}");
                    ProjectOverrides::default()
                }
            },
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

/// 项目覆盖文件路径（.texpresso/settings.json）。
pub fn project_overrides_path(project_root: &Path) -> PathBuf {
    project_root.join(".texpresso").join("settings.json")
}

/// 原子写：临时文件 + rename（防崩溃截断；modules.md §6）。
/// 写入属于 src-tauri 基础设施（FileSystem trait 只有读接口，core 纯度不受影响）。
async fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    // 父目录可能不存在（如项目覆盖 .texpresso/）：先创建，否则 write 直接失败，
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
    use crate::fs_impl::TokioFs;
    use texpresso_core::settings::model::CompileOverrides;

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
    fn project_overrides_path_is_dot_texpresso_settings() {
        assert_eq!(
            project_overrides_path(Path::new(r"C:\proj")),
            PathBuf::from(r"C:\proj\.texpresso\settings.json")
        );
    }

    #[tokio::test]
    async fn chinese_config_and_project_paths_round_trip() {
        // 模拟中文 Windows 用户名：app_config_dir 落在 `C:\Users\中文用户\AppData\...`，
        // 项目根也可能含中文。全局设置 + 项目覆盖都必须能原子读写。
        let base = std::env::temp_dir().join(format!("texpresso-配置-{}", std::process::id()));
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

        // 项目覆盖：写 → 读回一致（中文项目根下的 .texpresso/settings.json）
        let mut o = ProjectOverrides::default();
        o.root_file = Some("中文主文件.tex".into());
        s.save_overrides(&project_root, &o).await;
        let override_path = project_overrides_path(&project_root);
        assert!(override_path.exists(), "中文项目根应落盘 .texpresso/settings.json");
        let loaded_o = s.load_overrides(&TokioFs, &project_root).await;
        assert_eq!(
            loaded_o.root_file.as_deref(),
            Some(Path::new("中文主文件.tex"))
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn save_then_self_write_filter_consumes() {
        // 覆盖：写盘（原子写）→ 自写盘 hash 过滤消费一次 → 再次/不同内容为 false
        let dir = std::env::temp_dir().join(format!("texpresso-storage-it-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
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
        let dir = std::env::temp_dir().join(format!("texpresso-storage-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let s = SettingsStorage::new(path.clone());
        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded, Settings::default());
        assert!(path.exists(), "缺失时应落盘默认值");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn load_global_out_of_range_falls_back_to_default() {
        // 手工/陈旧越界值（debounce 50<100、timeout 1<5）不应被使用，回退默认并落盘
        let dir = std::env::temp_dir().join(format!("texpresso-storage-range-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"schema_version":1,"compile":{"mode":"continuous","debounce_ms":50,"timeout_secs":1,"engine":"xelatex"},"root_file":null}"#,
        )
        .unwrap();
        let s = SettingsStorage::new(path.clone());
        let loaded = s.load_global(&TokioFs).await;
        assert_eq!(loaded, Settings::default(), "越界应回退默认");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("\"timeout_secs\": 1"), "回退后应落盘默认值");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
