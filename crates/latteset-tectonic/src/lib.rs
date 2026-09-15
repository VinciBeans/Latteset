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
pub use runner::TectonicLibRunner;
pub use status::ProgressStatus;

/// TeX format 名（子进程档用 Tectonic 默认的 `latex`；上游 `src/config.rs:140-164`）。
///
/// io 层（format 缓存文件名）与 runner（`TexEngine::process` 的 format 参数）共用同一个常量。
pub const FORMAT_NAME: &str = "latex";

/// **会被回读的中间产物后缀**（两个用途共用一张表，避免两处口径漂移）：
///
/// 1. **重跑判据**（`runner::rerun_snapshot`）：这些文件在下一趟会被 LaTeX 读回来
///    （`.aux` 的引用/标签编号、`.toc` 的页码、`.bbl` 的条目……），它们变了就说明还没稳定；
/// 2. **上一趟产物的输入资格**（`io::TectonicIo::disk_path`）：只有这几类允许从 `tmp/` 读
///    **上一次编译**留下的副本 —— latexmk 正是靠这一点让 Quick 档的目录/引用**不倒退**。
///    刻意不把 `.pdf`/`.xdv` 放进来：它们不该被当成输入（否则 `\includegraphics{main.pdf}`
///    这类写法会读到我们自己的产物）。
pub const RERUN_EXTENSIONS: &[&str] = &[
    ".aux", ".toc", ".lof", ".lot", ".out", ".bbl", ".nav", ".snm", ".idx", ".ind", ".glo", ".gls",
];

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

/// 页哈希支持位（方案 §5.3 / 任务判据 6）：**已支持**（2026-09-15，任务 3）。
///
/// 路径 B 的 XDV 本来就在我们的捕获表里（`main.xdv`），所以**连盘都不用读**：直接把那份字节喂给
/// [`latteset_core::xdv::page_hashes`] —— 与子进程档**同一个函数、同一份数据** ⇒ 两个形态产出的
/// 页哈希**逐页可比**。这一点很重要：若两套口径不同，用户换形态时前端会把整篇判成"变了"（全量重绘）。
///
/// 由此库形态档也能吃 **A/B/C** 三个功能点：**A** = 页逐页相同 + 项目根已有 PDF ⇒ 跳过
/// `XdvipdfmxEngine` 转换与拷贝（只对 Quick，与子进程档同闸门）；**B/C** 由前端按
/// `changed_pages` 决定是否重载/只重绘变化页。判据缓存 = `tmp/<stem>.tectonic.pages`
/// （`<engine>` 带引擎名，已知债 #25；口径定义在 core，两档共用）。
///
/// 空表仍然只表示**无法判定**（XDV 缺失/损坏）——调用方必须保守全量刷新，不是"零页"。
pub const PAGE_HASH_SUPPORTED: bool = true;

/// **bib 趟支持位**（t24 / V-03 的收口）：**已支持**（2026-09-15）。
///
/// 序列照上游 `default_pass`：排版趟 → 若 `.aux` 出现 `\bibdata`（= 用了 `\bibliography{...}`）
/// → `BibtexEngine` 处理该 aux（读 `.bib` 走项目磁盘、`plain.bst` 走 bundle、`.bbl` 写内存层）
/// → 回到重跑循环。
///
/// ⚠ **bib 趟必须配重跑循环才有意义**：`latex → bibtex → latex` 之后 `\cite` **仍是未解析**
/// （实测第二趟读到 `.bbl` 后仍报 `Citation ... undefined` + `Label(s) may have changed. Rerun`）
/// —— 因为 `\bibcite`（编号）是上一趟才写进 `.aux` 的，`\begin{document}` 读的是旧值。
/// 所以 bib 趟在**预算内**的编辑触发档同样会跑（2026-09-15；先前它只在 `Full` 下跑，
/// 于是"加了 `\cite`"这类编辑要等到空闲收敛才解析出来）。
///
/// 检出信号是 **`.aux` 里的 `\bibdata`**，不是扫源码找 `\cite`：`\cite` 配 `thebibliography`
/// 时不需要 BibTeX。**biber 仍不支持**（biblatex 的外部工具），见 [`BIBER_PASS_SUPPORTED`]。
pub const BIB_PASS_SUPPORTED: bool = true;

/// **biber 支持位**：本阶段 `false`。
///
/// biblatex 的检出信号是 `<主文件名>.run.xml`（上游 `check_biber_requirement`），而它要跑
/// **外部 `biber` 二进制**——库形态不跑外部工具（与"不重造引擎驱动"同一条取向）。
/// 检出时 runner 会显式 `warn!`（[`BIBER_PASS_MISSING_NOTE`]），不假装引用解析了。
pub const BIBER_PASS_SUPPORTED: bool = false;

/// 检出 biblatex/biber 但本轮不跑 biber 时的警告文案（如实收窄声明，不静默）。
pub const BIBER_PASS_MISSING_NOTE: &str =
    "库形态本轮不跑 biber（需要外部 `biber` 二进制，biblatex 检出信号 = `<stem>.run.xml`）\
     ⇒ 该文档的引用/参考文献表**可能未解析**；用 `\\bibliography` + BibTeX 的文档不受影响";


/// **收敛支持位**：**有条件支持**（2026-09-15）。
///
/// 实现的是上游 `default_pass` 的重跑循环：跑一趟 TeX，比较 rerun 相关中间产物（`.aux`/`.toc`/
/// `.bbl`…）与上一趟是否相同，不同就再跑，上限 6 趟（上游 `DEFAULT_MAX_TEX_PASSES`）。
///
/// **编辑触发的 `Quick` 请求也走这条判定**（2026-09-15）：原先它无条件单趟即停，把"引用/目录落后
/// 一趟"当成草稿的语义；但库形态里重跑循环与中间产物指纹本来就在，"这一趟落没落后"是**可判定**的。
/// 现在它有一个预算（2000 ms，与前端空闲收敛的 `DELAY_MS` 同值；上限 3 趟）：预算内跑到稳定
/// 就按**实际结果**报 `Full` ⇒ 前端不亮「引用待更新」、也不再排一次空闲收敛的 Full；超预算则停在
/// 草稿态，行为与 ㉘ 原先完全一致（大文档单趟即超预算 ⇒ 第 1 趟之后即停）。
///
/// **仍会退回 `Quick` 的两种情形**（不得虚报 —— 报 `Full` 就等于告诉用户"引用/目录已就绪"）：
/// 1. 预算内没跑到稳定（含跑满 `MAX_TEX_PASSES`）；
/// 2. 文档**要我们跑不了的外部工具** —— biber（`<stem>.run.xml`）/ makeindex（`.idx`）。
///
/// 第 2 条是关键：带 biber 的文档我们根本没解析引用 ⇒ 必须退回 `Quick`，让 ㉘ 的提示亮着。
pub const CONVERGENCE_SUPPORTED: bool = true;

