//! Tectonic **库形态**引擎（路径 B：引擎 crate + 自持输出层）。
//!
//! 权威依据：[`docs/tectonic-library-plan.md`](../../../docs/tectonic-library-plan.md) §3.2（落点裁决）、
//! §3.4（选定路径 B 的九步序列）、§4.2 的 X-5（本 crate 是外部依赖/C 链的**第二个落点**，
//! 对 ADR-0010 的结构性偏离）与 `docs/adr/0012-tectonic-library-form-engine.md`。
//!
//! 为什么单独一个 crate（方案 §3.2 的候选①）：本 crate 的依赖闭包**必然**把 C/C++ 链拉进来
//! （freetype2 / graphite2 / harfbuzz / ICU 走外部探测，探测失败即 build script panic ⇒ 需要
//! vcpkg）。放进 `latteset-infra` 会让**整仓默认构建**永远需要 vcpkg，所以它必须是独立 workspace 成员，
//! 并由根 `Cargo.toml` 的 `default-members` 排除在默认构建之外。
//!
//! 模块：
//! - [`io`]：`TectonicIo`（`IoProvider`）——内存直喂输入 + 项目磁盘 + bundle 兜底 + 输出捕获；
//! - [`status`]：`ProgressStatus`（`StatusBackend`）——把引擎状态映射到 `CompileProgress` 与日志；
//! - [`bundle`]：bundle 源解析（`detect_bundle`）与分类失败文案；
//! - [`runner`]：`TectonicLibRunner`（与 `LatexmkRunner` 同一个 `CompileRunner` trait）。
//!
//! **线程模型**（方案 §3.4 末段）：引擎有进程内全局互斥（`ENGINE_LOCK`），编译必须在
//! `spawn_blocking` 里自建自跑；同一进程内不并发跑两个引擎。

pub mod bundle;
pub mod io;
pub mod runner;
pub mod status;

pub use bundle::{BundleSource, bundle_digest, open_bundle};
pub use io::{CANCEL_MESSAGE, IoCapture, TectonicIo, format_file_name};
pub use runner::{BIB_PASS_MISSING_NOTE, TectonicLibRunner, needs_bib_pass};
pub use status::ProgressStatus;

/// TeX format 名（子进程档用 Tectonic 默认的 `latex`；上游 `src/config.rs:140-164`）。
///
/// io 层（format 缓存文件名）与 runner（`TexEngine::process` 的 format 参数）共用同一个常量。
pub const FORMAT_NAME: &str = "latex";

/// 选形态的**运行期**开关（默认关闭 ⇒ 子进程形态，方案 §3.5「默认」行）。
///
/// 设置面（全局设置 + 项目覆盖的形态位字段）属后续阶段（方案 §3.5 / T13 收口），本阶段先用
/// 环境变量做运行期选择：`LATTESET_TECTONIC_LIB=1`（`0/false/no/off` 强制关闭）。
/// **不回退**（D1）：本开关只决定装配哪个 runner，运行中失败不自动切回子进程。
pub const LIB_FORM_ENV: &str = "LATTESET_TECTONIC_LIB";

/// 读 [`LIB_FORM_ENV`]，返回「是否启用库形态」。认不出的取值按**未启用**处理。
pub fn lib_form_enabled() -> bool {
    match std::env::var(LIB_FORM_ENV) {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => false,
    }
}

/// bundle 来源的**运行期**配置（与 [`LIB_FORM_ENV`] 同一层；设置面的形态位/bundle 位属后续收口，
/// 方案 §3.5）。
///
/// 取值：
/// - **未设 / 空串** → [`BundleSource::Default`]（上游兜底地址，**需要 `network-bundle` 特性**，
///   否则会在打开阶段显式报错——不静默、不回退，D1）；
/// - `none` / `off` / `0` / `false` → [`BundleSource::None`]（只测引擎路径的反例口径）；
/// - 其它 → [`BundleSource::Explicit`]。**必须写 `file:///…` 绝对 URL 或相对路径**：
///   `E:\…` / `E:/…` 会被上游当 URL scheme 解析成 `Ok(None)`（方案 §5.6 LB-4 实测）。
pub const BUNDLE_ENV: &str = "LATTESET_TECTONIC_BUNDLE";

/// 读 [`BUNDLE_ENV`]，解析成 [`BundleSource`]（见该常量的取值说明）。
pub fn bundle_from_env() -> BundleSource {
    match std::env::var(BUNDLE_ENV) {
        Ok(v) => {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                BundleSource::Default
            } else if matches!(trimmed.to_ascii_lowercase().as_str(), "none" | "off" | "0" | "false") {
                BundleSource::None
            } else {
                BundleSource::Explicit(trimmed.to_owned())
            }
        }
        Err(_) => BundleSource::Default,
    }
}

/// 产品缓存目录的**运行期覆盖**（`bundles/` 与 `formats/` 的父目录）。
///
/// 为什么要这个开关：方案 §5.2 / HL-11 是条**硬约束**——上游 `format_cache_path` 默认落
/// **项目目录**，不注入就会把 ~24 MB 的 `.fmt` 扔进用户项目。默认值取宿主自己的应用缓存目录
/// （GUI = `app_cache_dir()`，headless 由调用方给）；本变量用于**验证与放缓存目录隔离**：
/// 把它指向一个空目录，即可实测"冷缓存 → 生成 format → 热缓存复用"且项目目录里不出现 `.fmt`。
pub const CACHE_ENV: &str = "LATTESET_TECTONIC_CACHE";

/// 读 [`CACHE_ENV`]（未设/空 ⇒ `None`，由调用方给宿主默认值）。
pub fn cache_dir_from_env() -> Option<std::path::PathBuf> {
    match std::env::var(CACHE_ENV) {
        Ok(v) if !v.trim().is_empty() => Some(std::path::PathBuf::from(v.trim())),
        _ => None,
    }
}

/// 页哈希支持位（方案 §5.3 / 任务判据 6）：**库形态本阶段不产出页哈希**。
///
/// 理由（可核）：路径 B 把 XDV 拿在我们自己的 I/O 层里（`main.xdv` 在内存文件表中），
/// 但它**不落盘**、也不含子进程档 `tmp/<stem>.xdv` 的"最后一次成功编译产物"语义 ⇒
/// `CompileOutcome::Success::page_hashes` 必须显式给空表（= 「无法判定」，下游保守全量刷新），
/// 而不是"静默跳过"。等 P5 的自建输出层探针接上 `XdvParser` 后，这里改为按块解析产出页表。
/// 依据：t5 §4.1/§4.2（`XdvParser::parse` 逐块可行）、方案 §5.3、`crates/latteset-core/src/types.rs:288-293`。
pub const PAGE_HASH_SUPPORTED: bool = false;

/// 页哈希不可得时的**登记文案**（写进日志/文档，便于复核者一眼看到这是显式决定，不是漏做）。
pub const PAGE_HASH_NOTE: &str =
    "Tectonic 库形态（路径 B）本阶段不产出页哈希：XDV 只在自持 I/O 层的内存文件表里 → \
     Success.page_hashes 显式给空表（= 无法判定，下游按保守全量刷新处理）";

/// **bib 趟支持位**（t24 / V-03）：本阶段 `false`。
///
/// 本 crate 没有 `tectonic_engine_bibtex` 依赖（本机 registry 与 vendor 都没有它 ⇒ 加依赖会让
/// 本模块失去离线可复现性）⇒ 带引用的文档**不等价**于子进程档 Full。检出引用时 runner 会
/// 显式 `warn!`（[`BIB_PASS_MISSING_NOTE`]），`Success.kind` 报 `Quick`（[`CONVERGENCE_SUPPORTED`]）。
/// 真做 bib 趟的入口 = 加 `tectonic_engine_bibtex` + 按 `.aux` 存在且文档有引用时跑一趟。
pub const BIB_PASS_SUPPORTED: bool = false;

/// **收敛支持位**（t24 / V-04）：本阶段 `false`（只跑一趟 TeX，不重跑、不收敛）。
///
/// 因此 `CompileOutcome::Success.kind` 必须报 `CompileKind::Quick`：子进程档的 `Quick` 语义
/// 就是"引用/目录可能落后一趟"，而库形态单趟**更弱**（连 bib 都没跑）⇒ 由 ㉘ 的 `draft` /
/// 「引用待更新」提示兜底，不能让用户以为已经收敛。
pub const CONVERGENCE_SUPPORTED: bool = false;
