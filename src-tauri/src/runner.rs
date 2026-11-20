//! 编译 runner 的**薄壳装配点**（阶段② D1 = R3 选 A）：内嵌 Tectonic 是**唯一形态**。
//!
//! ## 为什么还要一层壳
//!
//! 保留"**资源设置改完即生效、不需要重启**"：每趟编译读一次全局设置里的 `tectonic.bundle` /
//! `tectonic.cache_dir`，据此重建 [`TectonicLibRunner`]。形态分支已**归零**——不再有形态设置位、
//! 环境变量强制档或子进程回退（ADR-0015 决策 2：引擎唯一 = 内嵌）。
//!
//! ## 优先级（显式设置 > 环境变量 > 宿主默认）
//!
//! 与 `docs/dev/modules.md` §2.7 的"运行期配置"同一口径（headless 侧同序，见 `latteset-server`）：
//! - **bundle**：设置里的非空值 → `LATTESET_TECTONIC_BUNDLE` → 上游兜底地址（联网取包，
//!   `network-bundle` 特性由 manifest 打开）；
//! - **缓存目录**：设置里的非空值 → `LATTESET_TECTONIC_CACHE` → 宿主应用缓存目录。
//!   ⚠ 缓存**必须**有落点：上游 format 默认落**项目目录**，不注入会把 ~24 MB 的 `.fmt`
//!   扔进用户项目（方案 §5.2 硬约束）。
//!
//! ## 不静默回退
//!
//! 内嵌唯一后没有"另一个形态"可回退：建不来 bundle / 缺缓存目录都走 runner 的显式失败文案
//! （`latteset-tectonic` 的 `bundle.rs` / `io.rs` 已经把"缺什么、怎么配"说清），不悄悄降级。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use latteset_core::project::FileSystem;
use latteset_core::scheduler::{CompileProgress, CompileRunner};
use latteset_core::settings::Settings;
use latteset_core::types::{CompileOutcome, CompileRequest};
use latteset_tectonic::{BundleSource, TectonicLibRunner};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// 内嵌引擎的薄壳 runner：只持装配材料，每趟编译按**当前设置**重建真正的 runner。
pub struct EmbeddedRunner {
    fs: Arc<dyn FileSystem>,
    progress: Arc<dyn CompileProgress>,
    /// 全局设置句柄（与 `AppState.settings` 是**同一个** Arc ⇒ 改设置立刻生效）。
    settings: Arc<RwLock<Settings>>,
    /// 宿主应用缓存目录（format 缓存的默认落点）。
    app_cache_dir: Option<PathBuf>,
}

impl EmbeddedRunner {
    pub fn new(
        fs: Arc<dyn FileSystem>,
        progress: Arc<dyn CompileProgress>,
        settings: Arc<RwLock<Settings>>,
        app_cache_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            fs,
            progress,
            settings,
            app_cache_dir,
        }
    }

    /// 按**当前**设置构造内嵌 runner（每趟编译调一次；资源设置因此改完即生效）。
    async fn build(&self) -> TectonicLibRunner {
        let t = self.settings.read().await.tectonic.clone();
        let mut lib = TectonicLibRunner::new(self.fs.clone(), self.progress.clone());
        if let Some(cache) = pick_cache(
            t.cache_dir.clone(),
            latteset_tectonic::cache_dir_from_env(),
            self.app_cache_dir.clone(),
        ) {
            lib = lib.with_cache_dir(cache);
        }
        let bundle = explicit_bundle(t.bundle.as_deref())
            .unwrap_or_else(latteset_tectonic::bundle_from_env);
        lib.with_bundle(bundle)
    }
}

/// 缓存目录的优先级：显式设置 > 环境变量 > 宿主默认（纯函数，单测直接打）。
fn pick_cache(
    explicit: Option<PathBuf>,
    env: Option<PathBuf>,
    host: Option<PathBuf>,
) -> Option<PathBuf> {
    explicit.or(env).or(host)
}

/// 设置里的 bundle 源（空串/纯空白按"未设置"处理 ⇒ 回落到环境变量/上游默认）。
fn explicit_bundle(raw: Option<&str>) -> Option<BundleSource> {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| BundleSource::Explicit(s.to_owned()))
}

#[async_trait]
impl CompileRunner for EmbeddedRunner {
    async fn compile(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome {
        self.build().await.compile(req, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 资源设置热生效的判据：显式设置压过环境变量与宿主默认（改完即生效、无需重启）。
    #[test]
    fn cache_priority_is_explicit_then_env_then_host() {
        let explicit = PathBuf::from(r"E:\cache-explicit");
        let env = PathBuf::from(r"E:\cache-env");
        let host = PathBuf::from(r"E:\cache-host");

        assert_eq!(
            pick_cache(Some(explicit.clone()), Some(env.clone()), Some(host.clone())),
            Some(explicit.clone()),
            "设置里的缓存目录必须压过环境变量/宿主默认"
        );
        assert_eq!(
            pick_cache(None, Some(env.clone()), Some(host.clone())),
            Some(env),
            "设置未填 ⇒ 环境变量次之"
        );
        assert_eq!(pick_cache(None, None, Some(host.clone())), Some(host), "最后才用宿主默认");
        assert_eq!(pick_cache(None, None, None), None, "三者都没有 ⇒ 交给 runner 显式报错");
    }

    /// bundle 源：空串/纯空白 = 未设置（回落环境变量/上游默认）；非空即显式来源。
    #[test]
    fn bundle_comes_from_non_empty_setting_only() {
        assert_eq!(
            explicit_bundle(Some(r"  file:///E:/bundle  ")),
            Some(BundleSource::Explicit("file:///E:/bundle".to_owned())),
            "设置值要 trim 后原样当显式来源"
        );
        assert_eq!(explicit_bundle(Some("")), None);
        assert_eq!(explicit_bundle(Some("   ")), None);
        assert_eq!(explicit_bundle(None), None);
    }
}
