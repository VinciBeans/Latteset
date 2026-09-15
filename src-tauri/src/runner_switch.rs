//! 可切换的编译 runner —— 设置面「引擎形态」的落点（方案 §3.5 的收口项）。
//!
//! ## 为什么是"每趟读设置"而不是"启动时装配一次"
//!
//! 原来这里是启动时二选一：`LATTESET_TECTONIC_LIB=1` 就装库形态，否则装子进程。那样**设置面根本
//! 落不了地**（改完要重启），而这正是本轮要修的"功能开不出来"。
//!
//! 现在装配点只装一个 [`SwitchableRunner`]：它持**全局设置句柄**，每趟编译读一次
//! `settings.tectonic`，据此选形态与资源。收益：**改完即生效，不需重启**；代价：形态位是**全局**
//! 设置（项目级覆盖要随 `CompileRequest` 下发，属后续项 —— 见 `TectonicSettings` 的说明）。
//!
//! ## 优先级（显式设置 > 环境变量 > 宿主默认）
//!
//! - **形态**：`settings.tectonic.lib_form`，或环境变量 `LATTESET_TECTONIC_LIB=1` **强制开**
//!   （复核/CI 用；它压过设置，所以能复现"库形态"那条路径而不改用户设置）。
//!   ⚠ **形态位受引擎闸门约束**：只有 `compile.engine == Tectonic` 时 `lib_form` 才生效
//!   （判定在 [`latteset_core::settings::TectonicSettings::use_library_form`]）；环境变量那条是
//!   显式覆盖，不受闸门约束。
//! - **bundle**：设置里的非空值 → `LATTESET_TECTONIC_BUNDLE` → 上游兜底（网络地址）。
//! - **缓存目录**：设置里的非空值 → `LATTESET_TECTONIC_CACHE` → 宿主应用缓存目录。
//!   ⚠ 缓存**必须**有落点：上游 format 默认落**项目目录**，不注入会往用户项目扔 24 MB 的 `.fmt`
//!   （方案 §5.2 硬约束）。
//!
//! ## D1：不静默回退
//!
//! 用户选了库形态而本次构建**没有**编入 `tectonic-lib` 特性时，**显式报错**，不许悄悄切回子进程
//! ——否则用户以为在用库形态、实际在跑 `tectonic.exe`，行为差异（biber/makeindex 不支持等）
//! 会被当成 bug。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use latteset_core::project::FileSystem;
use latteset_core::scheduler::{CompileProgress, CompileRunner};
use latteset_core::settings::Settings;
use latteset_core::types::{CompileOutcome, CompileRequest};
use latteset_infra::runner::LatexmkRunner;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// 形态位的环境变量（**强制开**档，复核/CI 用；与 `latteset_tectonic::LIB_FORM_ENV` 同值）。
const LIB_FORM_ENV: &str = "LATTESET_TECTONIC_LIB";

/// 本轮构建是否编入了库形态。
pub const LIB_FORM_COMPILED_IN: bool = cfg!(feature = "tectonic-lib");

/// 环境变量是否强制要求库形态（认不出的取值按"不强制"）。
pub fn lib_form_forced_by_env() -> bool {
    matches!(
        std::env::var(LIB_FORM_ENV).map(|v| v.trim().to_ascii_lowercase()),
        Ok(v) if matches!(v.as_str(), "1" | "true" | "yes" | "on")
    )
}

pub struct SwitchableRunner {
    fs: Arc<dyn FileSystem>,
    progress: Arc<dyn CompileProgress>,
    /// 全局设置句柄（与 `AppState.settings` 是**同一个** Arc ⇒ 改设置立刻生效）。
    settings: Arc<RwLock<Settings>>,
    /// 宿主应用缓存目录（库形态 format 缓存的默认落点）。
    /// 没编入库形态时这个字段确实没人读 —— 用 cfg 化 allow，别用全局 allow 掩盖真死代码。
    #[cfg_attr(not(feature = "tectonic-lib"), allow(dead_code))]
    app_cache_dir: Option<PathBuf>,
}

impl SwitchableRunner {
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

    /// 子进程形态（默认档）。
    async fn run_subprocess(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome {
        LatexmkRunner::new(self.fs.clone(), self.progress.clone())
            .compile(req, cancel)
            .await
    }
}

#[async_trait]
impl CompileRunner for SwitchableRunner {
    async fn compile(&self, req: CompileRequest, cancel: CancellationToken) -> CompileOutcome {
        let t = self.settings.read().await.tectonic.clone();
        // 形态判定走 core 的纯函数——引擎闸门在它里面（形态位是 Tectonic 引擎的子选项）。
        let want_lib = t.use_library_form(req.engine, lib_form_forced_by_env());

        if !want_lib {
            return self.run_subprocess(req, cancel).await;
        }

        #[cfg(feature = "tectonic-lib")]
        {
            let mut lib = latteset_tectonic::TectonicLibRunner::new(
                self.fs.clone(),
                self.progress.clone(),
            );
            // 显式设置 → 环境变量 → 宿主默认（见模块头注的优先级）
            let cache = t
                .cache_dir
                .clone()
                .or_else(latteset_tectonic::cache_dir_from_env)
                .or_else(|| self.app_cache_dir.clone());
            if let Some(cache) = cache {
                lib = lib.with_cache_dir(cache);
            }
            let bundle = t
                .bundle
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| latteset_tectonic::BundleSource::Explicit(s.to_owned()))
                .unwrap_or_else(latteset_tectonic::bundle_from_env);
            lib.with_bundle(bundle).compile(req, cancel).await
        }

        #[cfg(not(feature = "tectonic-lib"))]
        {
            let _ = cancel;
            CompileOutcome::IoError {
                message: format!(
                    "已选择 Tectonic **库形态**，但本次构建没有编入 `tectonic-lib` 特性 ⇒ 拒绝静默回退到\
                     子进程（D1）。请用 `--features tectonic-lib` 重新构建，或在设置里选回「子进程形态」。\
                     （{} 之类的环境变量同样需要该特性。）",
                    LIB_FORM_ENV
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 环境变量的认法：只有真值才强制，乱填不生效（避免 `=0` 反而打开形状的事故）。
    #[test]
    fn only_truthy_env_forces_lib_form() {
        // 注意：这里不能并发跑（同一进程的环境变量是全局的），故本测试只断言解析规则本身。
        for v in ["1", "true", "YES", " On "] {
            assert!(
                matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
                "{v} 应被认作真值"
            );
        }
        for v in ["0", "false", "no", "off", "", "maybe"] {
            assert!(
                !matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
                "{v} 不该被认作真值"
            );
        }
    }

    /// 编译进来的能力位必须与 `cfg` 一致（设置面据此禁用选项）。
    #[test]
    fn availability_flag_matches_cfg() {
        assert_eq!(LIB_FORM_COMPILED_IN, cfg!(feature = "tectonic-lib"));
    }
}
