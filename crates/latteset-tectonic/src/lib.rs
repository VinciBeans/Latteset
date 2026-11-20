//! Tectonic **内嵌引擎**（阶段②起是**唯一形态**；ADR-0015 决策 2 的落地）。
//!
//! 权威依据：`docs/adr/0015-tectonic-only-embedded-engine.md`、
//! [`docs/archive/tectonic-library-plan.md`](../../../docs/archive/tectonic-library-plan.md) §3.2
//! （落点裁决）、§3.4（路径 B 的九步序列）、§4.2 的 X-5（本 crate 是外部依赖/C 链的**第二个落点**，
//! 对 ADR-0010 的结构性偏离）与 `docs/adr/0012-tectonic-library-form-engine.md`。
//!
//! 为什么单独一个 crate：本 crate 的依赖闭包**必然**把 C/C++ 链拉进来
//! （freetype2 / graphite2 / harfbuzz / ICU 走外部探测，探测失败即 build script panic ⇒ 需要
//! vcpkg）。放进 `latteset-infra` 会让 core/infra 的测试也被迫拖上 C 链，所以它保持独立 workspace 成员；
//! 但自阶段②起它**不再被排除在默认构建之外**（所有产品构建都需要 vcpkg，见
//! `scripts/with-tectonic-lib.ps1`）。
//!
//! 模块：
//! - [`io`]：`TectonicIo`（`IoProvider`）——内存直喂输入 + 项目磁盘 + bundle 兜底 + 输出捕获；
//! - [`status`]：`ProgressStatus`（`StatusBackend`）——把引擎状态映射到 `CompileProgress` 与日志；
//! - [`bundle`]：bundle 源解析（`detect_bundle`）与分类失败文案；
//! - [`runner`]：`TectonicLibRunner`（实现 `CompileRunner`）；
//! - [`synctex`]：同步数据的**主输入记录修补**（roadmap ㊷ —— 引擎在库形态下把主文件记成 `texput`）。
//!
//! **线程模型**（方案 §3.4 末段）：引擎有进程内全局互斥（`ENGINE_LOCK`），编译必须在
//! `spawn_blocking` 里自建自跑；同一进程内不并发跑两个引擎。

pub mod bundle;
pub mod io;
pub mod preview;
pub mod runner;
pub mod status;
pub mod synctex;

pub use bundle::{BundleSource, bundle_digest, open_bundle};
pub use io::{CANCEL_MESSAGE, IoCapture, TectonicIo, format_file_name};
pub use runner::TectonicLibRunner;
pub use status::ProgressStatus;

/// TeX format 名（上游 `src/config.rs:140-164`；本引擎没有"子进程档"，改由本 crate 自管 format）。
///
/// io 层（format 缓存文件名）与 runner（`TexEngine::process` 的 format 参数）共用同一个常量。
pub const FORMAT_NAME: &str = "latex";

/// **会被回读的中间产物后缀**（两个用途共用一张表，避免两处口径漂移）：
///
/// 1. **重跑判据**（`runner::rerun_snapshot`）：这些文件在下一趟会被 LaTeX 读回来
///    （`.aux` 的引用/标签编号、`.toc` 的页码、`.bbl` 的条目……），它们变了就说明还没稳定；
/// 2. **上一趟产物的输入资格**（`io::TectonicIo::disk_path`）：只有这几类允许从 `tmp/` 读
///    **上一次编译**留下的副本 —— "单趟编译不倒退"正是靠这一点（与 latexmk 读旧 `.aux` 同一思路）。
///    刻意不把 `.pdf`/`.xdv` 放进来：它们不该被当成输入（否则 `\includegraphics{main.pdf}`
///    这类写法会读到我们自己的产物）。
pub const RERUN_EXTENSIONS: &[&str] = &[
    ".aux", ".toc", ".lof", ".lot", ".out", ".bbl", ".nav", ".snm", ".idx", ".ind", ".glo", ".gls",
];

/// bundle 来源的**运行期**配置（设置面「引擎 → bundle 源」是产品口径；环境变量用于复核/CI）。
///
/// 取值：
/// - **未设 / 空串** → [`BundleSource::Default`]（上游兜底地址，需要已启用的 `network-bundle`
///   特性 = 联网取包；打不开时 [`bundle::open_bundle`] 会给出指向设置面板/本变量的可行动文案）；
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

/// 页哈希支持位（方案 §5.3 / 任务判据 6）：**已支持**。
///
/// XDV 就在捕获表里（`<stem>.xdv`），所以**连盘都不用读**：直接把那份字节喂给
/// [`latteset_core::xdv::page_hashes`] —— 与 headless/GUI 两侧**同一个函数**（口径定义在 core）。
/// 由此本引擎也能吃 **A/B/C** 三个功能点：**A** = 页逐页相同 + 项目根已有 PDF ⇒ 跳过
/// `XdvipdfmxEngine` 转换与拷贝（只对 Quick）；**B/C** 由前端按 `changed_pages` 决定是否重载/只重绘变化页。
/// 判据缓存 = `tmp/<stem>.tectonic.pages`（文件名段恒为 [`latteset_core::types::ENGINE_ID`] 的值，
/// 已知债 #25）。
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
/// 时不需要 BibTeX。**biber 仍不支持**（biblatex 需要外部 biber，而本引擎不运行外部工具；
/// `backend=bibtex` 走内建 BibtexEngine），见 [`BIBER_PASS_SUPPORTED`]。
pub const BIB_PASS_SUPPORTED: bool = true;

/// **biber 支持位**：本阶段 `false`。
///
/// biblatex 的检出信号是 `<主文件名>.run.xml` 里**真的**写 `<binary>biber</binary>`
/// （上游 `check_biber_requirement`；`backend=bibtex` 也写该文件但请求的是 bibtex，不算），
/// 而 biber 是**外部程序**——**内嵌引擎不运行外部工具**（没有上游子进程驱动的
/// `external_tool_pass`），所以 biber 在 PATH 上也不会被调用、不产 `.bbl`（阶段② T9 真机实测）。
/// 检出时 runner 会显式 `warn!` 并通过 `CompileProgress::notice` 给用户可行动出路
/// （[`BIBER_PASS_MISSING_NOTE`]），不假装引用解析了。
pub const BIBER_PASS_SUPPORTED: bool = false;

/// 检出 biblatex/biber 但本引擎不运行 biber 时的警告文案（如实收窄声明，不静默）。
pub const BIBER_PASS_MISSING_NOTE: &str =
    "内嵌引擎不运行外部 biber（biblatex 检出信号 = `<stem>.run.xml` 里的 `<binary>biber</binary>`）\
     ⇒ 该文档的引用/参考文献表**可能未解析**（不会生成 `.bbl`）；改用 \
     `\\usepackage[backend=bibtex]{biblatex}` 或传统 `\\bibliography` + 内建 BibTeX 的文档不受影响";


/// **收敛支持位**：**有条件支持**（2026-09-15）。
///
/// 实现的是上游 `default_pass` 的重跑循环：跑一趟 TeX，比较 rerun 相关中间产物（`.aux`/`.toc`/
/// `.bbl`…）与上一趟是否相同，不同就再跑，上限 6 趟（上游 `DEFAULT_MAX_TEX_PASSES`）。
///
/// **编辑触发的 `Quick` 请求也走这条判定**（2026-09-15）：原先它无条件单趟即停，把"引用/目录落后
/// 一趟"当成草稿的语义；但内嵌引擎里重跑循环与中间产物指纹本来就在，"这一趟落没落后"是**可判定**的。
/// 现在它有一个预算（2000 ms，与前端空闲收敛的 `DELAY_MS` 同值；上限 3 趟）：预算内跑到稳定
/// 就按**实际结果**报 `Full` ⇒ 前端不亮「引用待更新」、也不再排一次空闲收敛的 Full；超预算则停在
/// 草稿态，行为与 ㉘ 原先完全一致（大文档单趟即超预算 ⇒ 第 1 趟之后即停）。
///
/// **仍会退回 `Quick` 的情形**（不得虚报 —— 报 `Full` 就等于告诉用户"引用/目录已就绪"）：
/// 1. 预算内没跑到稳定（含跑满 `MAX_TEX_PASSES`）；
/// 2. 文档**要我们跑不了的外部工具** —— biber（`.run.xml` 里的 `<binary>biber</binary>`）/
///    makeindex（`.idx`）/ glossaries（`.glo`）。
///
/// 第 2 条是关键：带 biber 的文档我们根本没解析引用 ⇒ 必须退回 `Quick`，让 ㉘ 的提示亮着。
pub const CONVERGENCE_SUPPORTED: bool = true;

