//! 编译 runner 的**薄壳装配点**（阶段② D1 = R3 选 A）：内嵌 Tectonic 是**唯一形态**。
//!
//! ## 为什么还要一层壳
//!
//! 保留"**资源设置改完即生效、不需要重启**"：每趟编译读一次全局设置里的 `tectonic.bundle` /
//! `tectonic.cache_dir`，据此重建 [`TectonicLibRunner`]。形态分支已**归零**——不再有形态设置位、
//! 环境变量强制档或子进程回退（ADR-0015 决策 2：引擎唯一 = 内嵌）。
//!
//! ## 优先级（显式设置 > 环境变量 > **内置精简集** > 在线兜底）
//!
//! 与 `docs/dev/modules.md` §2.7 的"运行期配置"同一口径（headless 侧无资源目录，故无内置集这一档）：
//! - **bundle**：设置里的非空值 → `LATTESET_TECTONIC_BUNDLE` → **随安装包分发的内置精简集**
//!   （`<资源目录>/tectonic-bundle.zip`，ADR-0016 候选 A：产品默认 = 离线可用）→ 上游兜底地址
//!   （联网；仅开发构建没有资源目录时出现）；
//! - **缓存目录**：设置里的非空值 → `LATTESET_TECTONIC_CACHE` → 宿主应用缓存目录。
//!   ⚠ 缓存**必须**有落点：上游 format 默认落**项目目录**，不注入会把 ~24 MB 的 `.fmt`
//!   扔进用户项目（方案 §5.2 硬约束）。
//!
//! ## 不静默回退
//!
//! 内嵌唯一后没有"另一个形态"可回退：建不来 bundle / 缺缓存目录都走 runner 的显式失败文案
//! （`latteset-tectonic` 的 `bundle.rs` / `io.rs` 已经把"缺什么、怎么配"说清），不悄悄降级。
//! 尤其：**内置精简集缺失/损坏时不会改用网络**（ADR-0016 决策 2），只给"修复安装 / 显式切在线"。**

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
    /// 随安装包分发的**内置精简宏包集**（`<资源目录>/tectonic-bundle.zip`，ADR-0016 候选 A）。
    ///
    /// `Some(path)` 时**即使文件不存在**也用它：release 构建里"该有却没有"必须显式报错
    /// （修复安装 / 显式切在线），不得静默改用网络（ADR-0016 决策 2）；`None` = 本环境没有内置集
    /// （headless、或开发构建且仓库里没有产物）⇒ 回落到在线兜底。
    shipped_bundle: Option<PathBuf>,
}

impl EmbeddedRunner {
    pub fn new(
        fs: Arc<dyn FileSystem>,
        progress: Arc<dyn CompileProgress>,
        settings: Arc<RwLock<Settings>>,
        app_cache_dir: Option<PathBuf>,
        shipped_bundle: Option<PathBuf>,
    ) -> Self {
        Self {
            fs,
            progress,
            settings,
            app_cache_dir,
            shipped_bundle,
        }
    }

    /// 按**当前**设置构造内嵌 runner（每趟编译调一次；资源设置因此改完即生效）。
    async fn build(&self) -> TectonicLibRunner {
        let t = self.settings.read().await.tectonic.clone();
        let mut lib = TectonicLibRunner::new(self.fs.clone(), self.progress.clone());
        // 缓存目录口径与首启预热 / 缓存 GC **共用同一函数**（T22 B3：显式设置 > 环境变量 > 宿主默认）。
        if let Some(cache) = latteset_tectonic::resolve_cache_dir(
            t.cache_dir.clone(),
            latteset_tectonic::cache_dir_from_env(),
            self.app_cache_dir.clone(),
        ) {
            lib = lib.with_cache_dir(cache);
        }
        let env_bundle = std::env::var_os(latteset_tectonic::BUNDLE_ENV)
            .map(|_| latteset_tectonic::bundle_from_env());
        let bundle = latteset_tectonic::resolve_source(
            explicit_bundle(t.bundle.as_deref()),
            env_bundle,
            self.shipped_bundle.as_deref(),
        );
        lib.with_bundle(bundle)
    }
}

/// 设置里的 bundle 源（空串/纯空白按"未设置"处理 ⇒ 回落到环境变量/内置集/在线兜底）。
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

    /// bundle 源：空串/纯空白 = 未设置（回落环境变量/内置集/在线兜底）；非空即显式来源。
    ///
    /// 完整优先级（显式 > 环境 > 内置集 > 在线兜底）的用例在 `latteset_tectonic::resolve_source`
    /// —— 放那边才跑得起来（`cargo test -p latteset` 在本机有 Tauri 工具链边界，见 testing.md）。
    /// 缓存目录优先级的单测同理在 `latteset_tectonic::resolve_cache_dir`（T22 B3 起三处共用）。
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
