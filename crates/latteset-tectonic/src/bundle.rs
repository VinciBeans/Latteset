//! bundle 源解析与分类失败文案（方案 §3.4 第 2 步、§5.6 的 LB-1..LB-5）。
//!
//! 上游口径：`tectonic_bundles::detect_bundle(source, only_cached, custom_cache_dir)`
//! （`crates/bundles/src/lib.rs:234-288`，CLI 用同一个函数 `src/bin/tectonic/compile.rs:193`）。
//! 三条实测坑（t5 §3.2，由本模块在调用**之前**补上分类文案）：
//! 1. **绝对 Windows 路径会被当成 URL scheme**：`E:\...` 判不出来 ⇒ 必须 `file:///E:/...`
//!    或**相对路径**（`docs/archive/tectonic-library-plan.md` §5.6 LB-4）；
//! 2. **空目录 bundle 静默 open 成功但 0 文件** ⇒ 必须自己判空；
//! 3. 路径不存在 / 空目录 / 非 bundle，上游**同一句**错误 ⇒ 我们按三类给不同文案（LB-5 / F2b/F2c）。

use std::path::{Path, PathBuf};

use tectonic_bundles::{Bundle, detect_bundle};

/// 本 crate 钉定的 format 版本号（= 上游 `FORMAT_SERIAL`）。
///
/// 证据：本机实测默认缓存里是 `...-latex-33.fmt`（`formats/6ffe0558…-latex-33.fmt`），
/// 与 `src/io/format_cache.rs:53-61` 的命名口径一致；上游 `tectonic` 0.17.0 的
/// `FORMAT_SERIAL` 同值。它只用于 [`default_bundle_url`] 的兜底地址拼装。
pub const FORMAT_SERIAL: u32 = 33;

/// bundle 来源（与上游 `-b <url|path>` 同口径，方案 §5.6）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BundleSource {
    /// 兜底默认 bundle（`{prefix}/default_bundle_v{FORMAT_SERIAL}.tar`，上游 `lib.rs:301-330`）。
    Default,
    /// 显式来源：URL（`https://…` / `file:///…`）或相对路径。
    Explicit(String),
    /// **随安装包分发的内置精简宏包集**（ADR-0016 候选 A：`<资源目录>/tectonic-bundle.zip`）。
    ///
    /// 与 [`BundleSource::Default`] 的区别是**默认语义**：GUI 装配点把"设置未填"解析成这个来源
    /// （产品默认 = 离线可用的内置集）；`Default`（在线完整 bundle）只在用户**显式**选择或
    /// headless（无资源目录）时出现。内置集缺失/损坏时**不得**静默改用网络（ADR-0016 决策 2）：
    /// 由 [`SHIPPED_BUNDLE_HINT`] 给"修复安装 / 显式切在线"的可操作文案。
    Shipped(PathBuf),
    /// 不要 bundle（只测引擎路径/反例用）。
    None,
}

impl Default for BundleSource {
    fn default() -> Self {
        BundleSource::Default
    }
}

/// 兜底 bundle 地址（与上游同一函数，避免我们手抄 URL）。
pub fn default_bundle_url() -> String {
    tectonic_bundles::get_fallback_bundle_url(FORMAT_SERIAL)
}

/// bundle 来源的完整优先级（ADR-0016 候选 A；纯函数，GUI 装配点每趟编译调一次）：
///
/// **显式设置 > 环境变量 > 随包内置精简集 > 在线兜底**
///
/// - `explicit`：设置面里填的值；
/// - `env`：`LATTESET_TECTONIC_BUNDLE` 存在时解析出的来源（含 `none`/`default` 等特殊值）；
/// - `shipped`：内置精简集的**应有路径**（release 下即使文件缺失也传 `Some` ⇒ 显式报错，
///   不静默改用网络，ADR-0016 决策 2）；`None` = 本环境没有内置集（headless / 开发无产物）。
///
/// 放在本 crate 而不是外壳：优先级是产品语义，且这里能被 `cargo test` 覆盖（外壳的
/// `cargo test` 在本机跑不起来，见 docs/dev/testing.md 的已知边界）。
pub fn resolve_source(
    explicit: Option<BundleSource>,
    env: Option<BundleSource>,
    shipped: Option<&Path>,
) -> BundleSource {
    if let Some(b) = explicit {
        return b;
    }
    if let Some(b) = env {
        return b;
    }
    if let Some(p) = shipped {
        return BundleSource::Shipped(p.to_path_buf());
    }
    BundleSource::Default
}

// ===================== 首启 format 预热：在线源的零网络判定（T18 修复） =====================
//
// 背景（阶段③ T18 缺陷）：`network-bundle` 特性打开时，上游 `detect_bundle` 对 http(s) 源
// **无条件**构造 `BundleCache`；缓存里没有记录时 `BundleCache::new` 会 `get_digest()` **联网**
// （本机实测 ~8s，还会建 `data/<hash>/` 目录）。启动预热必须避开这条路径。
//
// 缓存布局（上游 `tectonic_bundles-0.4.2/src/cache.rs`）：
// - `<cache>/hashes/<sanitize(url)>`：digest 文本（64 hex）；
// - `<cache>/hashes/<…>.lock`：上次检查时间（秒）；超过 7 天上游会重新联网刷新 digest；
// - `<cache>/data/<digest>/…`：按需下载/解包的文件。
// 这里只**读**这些标记：满足"hash 有 + lock 未过期 + data 非空"才算本地可用 ⇒ 只许读缓存预热。

/// 在线 bundle 缓存的有效期：与上游 `BundleCache::new` 一致（`Duration::days(7)`）。
const ONLINE_CACHE_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// 首启 format 预热的**计划**（纯决策；不产生任何磁盘写）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrewarmPlan {
    /// 不预热（没有本地 bundle / 在线 bundle 的本地缓存不可用）。
    Skip,
    /// 预热；`only_cached` = 打开 bundle 时**只许读缓存**（在线源为 `true`）。
    Prewarm { only_cached: bool },
}

/// 在线源解析出**传给上游 `detect_bundle` 的那个 URL**；本地源/`None` 返回 `None`。
///
/// 必须与 `open_bundle` 实际传入的字符串**完全一致**：缓存键是上游 `sanitize(location)`，
/// 而 location 就是 URL 原文；`Default` 用 [`default_bundle_url`]。
fn online_bundle_url(source: &BundleSource) -> Option<String> {
    match source {
        BundleSource::Default => Some(default_bundle_url()),
        BundleSource::Explicit(s) => {
            let is_online = {
                let t = s.trim_start().to_ascii_lowercase();
                t.starts_with("http://") || t.starts_with("https://")
            };
            is_online.then(|| s.clone())
        }
        BundleSource::Shipped(_) | BundleSource::None => None,
    }
}

/// 在线 bundle 的本地缓存是否**可用且未过期**（**零网络、零写**：只读 `hashes/` 与 `data/`）。
///
/// 判据见 [`online_bundle_cached_digest`]：拿得到 digest 即 ready。
pub fn online_bundle_cache_ready(cache_dir: &Path, url: &str) -> bool {
    online_bundle_cached_digest(cache_dir, url).is_some()
}

/// 在线 bundle 本地缓存里的 digest（**只读、零网络、零写**；拿不到 ⇒ `None`）。
///
/// 判据与上游 `BundleCache::new` 的读法一致：hash 文件是 64 位 hex digest；`.lock` 是检查时间
/// （秒）且距今 ≤ 7 天；`data/<digest>/` 存在且非空。任何一项不满足 ⇒ `None`。
pub fn online_bundle_cached_digest(cache_dir: &Path, url: &str) -> Option<String> {
    let location = tectonic_io_base::app_dirs::sanitize(url);
    let hash_file = cache_dir.join("hashes").join(&location);
    let raw = std::fs::read_to_string(&hash_file).ok()?;
    let digest = raw.trim();
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let lock_file = hash_file.with_extension("lock");
    let checked_at = std::fs::read_to_string(&lock_file)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if now.saturating_sub(checked_at) > ONLINE_CACHE_TTL_SECS {
        return None;
    }
    let data_dir = cache_dir.join("data").join(digest);
    let data_present = data_dir.is_dir()
        && std::fs::read_dir(&data_dir).map(|mut it| it.next().is_some()).unwrap_or(false);
    data_present.then(|| digest.to_ascii_lowercase())
}

/// 缓存目录的**唯一解析口径**（runner / 首启预热 / 缓存 GC 共用，T22 B3）：
/// **显式设置 > `LATTESET_TECTONIC_CACHE` > 宿主默认（`app_cache_dir`）**。
///
/// 放在本 crate 而不是外壳：三处调用点各写一遍曾导致 GUI 的 GC 用了 `app_cache_dir()`
/// 而 runner/预热用设置值 ⇒ 用户自定义 `cache_dir` 时 GC 清不到（反之还可能清错）。
pub fn resolve_cache_dir(
    explicit: Option<PathBuf>,
    env: Option<PathBuf>,
    host_default: Option<PathBuf>,
) -> Option<PathBuf> {
    explicit.or(env).or(host_default)
}

/// 当前**实际会用的** bundle digest（缓存 GC 用：保护"目标 bundle"的缓存，T22 B3）。
///
/// - 本地源（随包精简集 / 本地目录 / 本地文件）：`open_bundle` + `bundle_digest`（**纯本地读**）；
/// - 在线源（`Default` / `http(s)` 显式源）：只读本地缓存标记（[`online_bundle_cached_digest`]），
///   **零网络**；缓存不可用 ⇒ `None`；
/// - `None` / bundle 缺失或损坏 / 取不到 digest ⇒ `None`。
///
/// 调用方（GC）在 `None` 时**不得清理缓存**（解析不出当前 bundle 就保守不动，避免误删）。
pub fn current_bundle_digest(source: &BundleSource, cache_dir: &Path) -> Option<String> {
    if let Some(url) = online_bundle_url(source) {
        return online_bundle_cached_digest(cache_dir, &url);
    }
    match source {
        BundleSource::None => None,
        _ => {
            let mut bundle = open_bundle(source, false, Some(cache_dir.to_path_buf())).ok().flatten()?;
            bundle_digest(&mut *bundle).ok()
        }
    }
}

/// 首启 format 预热的决策（纯函数；只读磁盘、不写不联网）。
///
/// - 本地源（随包精简集 / 本地目录 / 本地文件）⇒ 主动预热（`only_cached=false`）；
/// - 在线源（[`BundleSource::Default`] / `http(s)` 显式源）⇒ **只有本地缓存可用且未过期**才预热，
///   且只许读缓存（`only_cached=true`，启动期不联网）；
/// - [`BundleSource::None`] / 在线源冷缓存 ⇒ [`PrewarmPlan::Skip`]。
pub fn prewarm_plan(source: &BundleSource, cache_dir: &Path) -> PrewarmPlan {
    if let Some(url) = online_bundle_url(source) {
        return if online_bundle_cache_ready(cache_dir, &url) {
            PrewarmPlan::Prewarm { only_cached: true }
        } else {
            PrewarmPlan::Skip
        };
    }
    match source {
        BundleSource::None => PrewarmPlan::Skip,
        _ => PrewarmPlan::Prewarm { only_cached: false },
    }
}

/// 默认（网络兜底）bundle 打不开时的**可行动出路**（阶段② D2：默认 bundle = 联网取包，
/// 它是产品默认路径，失败时必须指向用户能操作的地方，不能只说"打开失败"）。
///
/// 三条出口：设置面板「引擎 → bundle 源」、环境变量、以及"本地目录需自带 SHA256SUM"的格式要求
/// （绝对 Windows 路径要写 `file:///…`，见 [`classify`]）。
pub const DEFAULT_BUNDLE_HINT: &str =
    "默认 bundle 需要联网获取：① 检查网络/代理；② 或用本地目录 bundle —— 在「设置 → 引擎」把 \
     bundle 源设为本地目录（目录需自带 SHA256SUM；写 `file:///E:/…` 或相对路径，不要写 `E:\\…`），\
     或设置环境变量 `LATTESET_TECTONIC_BUNDLE=<相对路径 或 file:///绝对路径>`。";

/// 随安装包分发的内置精简宏包集缺失/损坏时的**可行动出路**（ADR-0016 决策 2：不静默回退）。
///
/// 三档语义（与 ADR-0016 候选 A 一致）：
/// - 产品默认 = 内置精简集（离线可用，覆盖代表性文档集）；
/// - 在线完整 bundle = 用户**显式**选择（联网；覆盖全部 TeX Live 常用包）；
/// - 本地目录 bundle = 用户/内网自备（需自带 SHA256SUM）。
pub const SHIPPED_BUNDLE_HINT: &str =
    "内置精简宏包集（安装包自带）缺失或损坏：① 重新安装 / 修复安装本应用；\
     ② 或在「设置 → 引擎 → 宏包集来源」显式选「在线完整宏包集」（需联网，覆盖更全）\
     或「本地目录」（自备 bundle，需自带 SHA256SUM）。不会静默改用网络。";

/// 缺宏包/文件的**产品级补包出路**（内置精简集只覆盖代表性文档集，不是完整 TeX Live）。
///
/// 放在本 crate：这是"bundle 来源语义"的一部分；core 诊断只负责认错，产品出路由包里的话术给。
/// 调用方（GUI/server）把它接到缺包诊断的 hint 上，不新造一套检测。
pub fn missing_package_hint(missing: &str) -> String {
    format!(
        "本应用随包的是**精简宏包集**（覆盖常用文档集），不是完整 TeX Live ⇒ 缺 `{missing}` 时：\
         ① 在「设置 → 引擎 → 宏包集来源」切到「在线完整宏包集」（需联网）后重新编译；\
         ② 或把该宏包补进本地目录 bundle（目录需自带 SHA256SUM，改完把来源指到它）；\
         ③ 若是模板自带文件，确认「打开项目」选的是模板根目录。"
    )
}


/// 分类失败文案（三类，方案 §5.6 LB-5 / §9 F2b/F2c）。
fn classify(source: &str) -> Option<String> {
    // 网络源 + 本构建没开 `network-bundle` ⇒ 必然失败，且上游的报错不会点出"特性没开"。
    // 这类必须先说清楚，否则用户看到的是"bundle 打开失败"而不知道要配本地 bundle。
    #[cfg(not(feature = "network-bundle"))]
    if source.starts_with("https://") || source.starts_with("http://") {
        return Some(format!(
            "bundle 源 `{source}` 是网络地址，但本构建**未启用 `network-bundle` 特性**（方案 §4.2 X-2 选 (a)：\
             默认关掉 HTTP）⇒ 请改用本地目录 bundle：在「设置 → 引擎」把 bundle 源设为本地目录\
             （需自带 SHA256SUM），或设置环境变量 `{}=<相对路径 或 file:///绝对路径>`",
            crate::BUNDLE_ENV
        ));
    }
    // `file://` 本地源：先做我们的三类前置检查（上游对这三类给同一句话）。
    let path = source.strip_prefix("file:///").or_else(|| source.strip_prefix("file://"));
    let is_abs_windows = source.len() > 2 && source.as_bytes().get(1) == Some(&b':');
    if is_abs_windows && !source.starts_with("file://") {
        return Some(format!(
            "bundle 源 `{source}` 是绝对 Windows 路径：上游 `detect_bundle` 会把它当 URL scheme 解析、\
             判为「不是 bundle」⇒ 请写 `file:///{}`（正斜杠）或相对路径（方案 §5.6 LB-1）",
            source.replace('\\', "/")
        ));
    }
    let path = path.map(PathBuf::from).or_else(|| {
        let p = Path::new(source);
        (!source.contains("://")).then(|| p.to_path_buf())
    })?;
    if !path.exists() {
        return Some(format!("bundle 路径不存在：{}（请确认目录/文件已就位）", path.display()));
    }
    if path.is_dir() {
        let entries = std::fs::read_dir(&path).ok()?.count();
        if entries == 0 {
            return Some(format!(
                "bundle 目录是空的：{}（上游会「静默 open 成功但 0 个文件」，编译期才炸成缺文件）",
                path.display()
            ));
        }
        if !path.join("SHA256SUM").is_file() {
            return Some(format!(
                "目录 bundle 缺少 SHA256SUM：{}（上游原文 `bundle does not provide needed SHA256SUM file`，\
                 方案 §5.6 LB-2）",
                path.display()
            ));
        }
    }
    None
}

/// 本地路径 → 上游 `detect_bundle` 认的 `file:///…` 形态（绝对 Windows 路径直写会被当 URL scheme，
/// 见 [`classify`] 的第 1 条坑）。
///
/// ⚠ **必须剥 Windows verbatim 前缀**：`std::fs::canonicalize` / Tauri 资源解析常给出
/// `\\?\E:\…`（长路径形式）。直接替换分隔符会得到 `file:////?/E:/…`，上游 `Url::to_file_path()`
/// 把它当"带 `?` 的 host 段"解析失败（release GUI 实测：随包 bundle 打不开 ⇒ 首编整体不可用，
/// `failed to parse local path`）。
fn path_to_file_url(path: &Path) -> String {
    let s = path.to_string_lossy();
    if s.starts_with("file://") {
        s.into_owned()
    } else {
        file_url_from_local(&s)
    }
}

/// 本地路径**字符串** → `file://` URL（纯函数，跨平台可测；输入按 Windows 反斜杠/Unix 斜杠都处理）。
///
/// 三类前缀都要处理：
/// - `\\?\E:\…`（verbatim + 盘符）→ `file:///E:/…`；
/// - `\\?\UNC\server\share\…`（verbatim + UNC）→ `file://server/share/…`；
/// - `\\.\E:\…`（设备前缀）→ `file:///E:/…`；
/// 普通盘符路径、Unix 绝对路径、普通 UNC（`\\server\share`）与非 Windows 分支行为不变。
fn file_url_from_local(raw: &str) -> String {
    let s = raw.replace('\\', "/");
    if s.starts_with("file://") {
        return s;
    }
    let strip_prefix_ci = |prefix: &str| -> Option<&str> {
        (s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix))
            .then(|| &s[prefix.len()..])
    };
    // verbatim UNC 要保留"服务器/共享"两段（`//server/share/…`），其余前缀剥掉后得到盘符路径。
    let body = if let Some(rest) = strip_prefix_ci("//?/unc/") {
        format!("//{rest}")
    } else if let Some(rest) = strip_prefix_ci("//?/") {
        rest.to_owned()
    } else if let Some(rest) = strip_prefix_ci("//./") {
        rest.to_owned()
    } else {
        s
    };
    if body.starts_with("//") {
        // UNC（含剥掉 verbatim 后的）：`file:` + `//server/share/…` ⇒ `file://server/share/…`
        format!("file:{body}")
    } else if body.starts_with('/') {
        format!("file://{body}")
    } else {
        format!("file:///{body}")
    }
}

/// 打开 bundle。返回 `Ok(None)` = 明确不要 bundle；`Err(msg)` = 可读失败文案。
pub fn open_bundle(
    source: &BundleSource,
    only_cached: bool,
    cache_dir: Option<PathBuf>,
) -> Result<Option<Box<dyn Bundle>>, String> {
    // 随安装包分发的内置精简集：先做**显式**前置检查（ADR-0016 决策 2：缺失/损坏不得静默回退网络）。
    if let BundleSource::Shipped(path) = source {
        if !path.exists() {
            return Err(format!("内置宏包集文件不存在：{}。{SHIPPED_BUNDLE_HINT}", path.display()));
        }
        let meta = std::fs::metadata(path)
            .map_err(|e| format!("内置宏包集读不到（{}）：{e}。{SHIPPED_BUNDLE_HINT}", path.display()))?;
        if meta.is_file() && meta.len() == 0 {
            return Err(format!("内置宏包集文件是空的：{}。{SHIPPED_BUNDLE_HINT}", path.display()));
        }
        return match detect_bundle(path_to_file_url(path), only_cached, cache_dir) {
            Ok(Some(b)) => Ok(Some(b)),
            Ok(None) => Err(format!(
                "内置宏包集无法识别：{}（安装包自带的 `.zip`/目录 bundle 形态不对）。{SHIPPED_BUNDLE_HINT}",
                path.display()
            )),
            Err(e) => Err(format!(
                "内置宏包集打开失败（{}）：{e:#}。{SHIPPED_BUNDLE_HINT}",
                path.display()
            )),
        };
    }

    let src = match source {
        BundleSource::None => return Ok(None),
        BundleSource::Default => default_bundle_url(),
        BundleSource::Explicit(s) => s.clone(),
        BundleSource::Shipped(_) => unreachable!("Shipped 已在上面单独处理"),
    };
    if let Some(msg) = classify(&src) {
        return Err(msg);
    }
    match detect_bundle(src.clone(), only_cached, cache_dir) {
        Ok(Some(b)) => Ok(Some(b)),
        Ok(None) => {
            let mut msg = format!(
                "bundle 源无法识别：`{src}`（上游 `detect_bundle` 返回 None）。\
                 本地目录/文件请写成 `file:///…` 或相对路径；网络 bundle 需要 `network-bundle` 特性"
            );
            if matches!(source, BundleSource::Default) {
                msg.push('。');
                msg.push_str(DEFAULT_BUNDLE_HINT);
            }
            Err(msg)
        }
        Err(e) => {
            let mut msg = format!("bundle 打开失败（{src}）：{e:#}");
            // D2：网络兜底是**产品默认路径**，失败时必须给用户可行动的出路（设置面板 / 环境变量）。
            if matches!(source, BundleSource::Default) {
                msg.push('。');
                msg.push_str(DEFAULT_BUNDLE_HINT);
            }
            Err(msg)
        }
    }
}

/// 取 bundle 的 **digest**（V-02：format 缓存键）。
///
/// 为什么必须单独取、且**失败即错**：上游 `Bundle::get_digest()` 是缓存键的来源
/// （`crates/io_base/src/lib.rs:105`；`FormatCache::new(bundle_digest, …)` 见
/// `test_file/tectonic-src/src/io/format_cache.rs:29-34`）。拿不到 digest 时若能退回裸名
/// （`{stem}.fmt`），**换 bundle 就会复用旧 format** —— 那是正确性风险，不是体验问题。
/// 所以这里返回 `Result<String, String>`，调用方（runner）把它变成可见失败。
pub fn bundle_digest(bundle: &mut dyn Bundle) -> Result<String, String> {
    bundle.get_digest().map(|d| d.to_string()).map_err(|e| {
        format!(
            "取 bundle digest 失败（format 缓存键需要它；不做裸名回退，V-02）：{e:#}"
        )
    })
}

// ===================== 缓存 GC（T16 / ADR-0016 决策 5） =====================
//
// 缓存目录结构（`latteset-tectonic` 产品缓存，`<cache>` = 设置的 cache_dir / 宿主 app_cache_dir）：
// - `formats/<digest>-latex-{serial}.fmt`：每份 ≈24.45 MB（V-02：文件名带 bundle digest）；
// - `data/<digest>/` + `data/<digest>.index`：网络 bundle 按需取到的文件与索引（在线 bundle 才有）；
// - `hashes/<url 编码的地址>`：网络 bundle 的"地址 → digest / 校验时间"记录。
//
// 清理口径（任务书 T16 第 5 条 / T22 B3）：**保留当前正在用的 digest + 最近 N 份**，按 mtime 判新旧。
//
// "当前 digest"**不再从"最新 format 的 mtime"推断**（换 bundle 后可能把目标 bundle 的旧缓存清掉），
// 而是由调用方经 [`current_bundle_digest`]（同一 bundle 解析器：随包/显式/在线各自的 digest）显式给出；
// **解析不出时整体不动**（不得猜）。其余保守边界：缓存里没有任何 format ⇒ 不动（避免误删首次取到的
// data/）；认不出命名规则的条目一律不动。GC 只在**应用启动、还没有编译在跑**时调用（src-tauri）。
//
// 上限可配：环境变量 `LATTESET_CACHE_KEEP`（=当前之外再保留几份历史，默认 3，夹在 1..=50）。

/// 缓存保留份数的环境变量（高级配置；默认 3）。
pub const CACHE_KEEP_ENV: &str = "LATTESET_CACHE_KEEP";

/// 读保留份数：默认 3，非法值回落默认，夹在 `1..=50`。
pub fn cache_keep_from_env() -> usize {
    std::env::var(CACHE_KEEP_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(3)
        .clamp(1, 50)
}

/// 一次 GC 的结果（可断言/可日志；`freed_bytes` 为删除文件的字节数合计）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CacheGcReport {
    /// 判定为"当前 + 最近 N"而保留的 digest（按新旧降序）。
    pub kept_digests: Vec<String>,
    pub kept_formats: usize,
    pub deleted_formats: usize,
    pub deleted_data: usize,
    pub deleted_hashes: usize,
    pub freed_bytes: u64,
}

/// 64 位小写/大写 hex digest（bundle digest 的形态；文件名里出现的前缀）。
fn parse_digest_prefix(name: &str) -> Option<String> {
    let first = name.split(['-', '.']).next()?;
    (first.len() == 64 && first.chars().all(|c| c.is_ascii_hexdigit())).then(|| first.to_ascii_lowercase())
}

fn collect_entries(dir: &Path) -> Vec<(PathBuf, std::time::SystemTime)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok()).filter_map(|e| {
        let path = e.path();
        let meta = e.metadata().ok()?;
        if !meta.is_file() {
            return None;
        }
        Some((path, meta.modified().unwrap_or(std::time::UNIX_EPOCH)))
    })
    .collect()
}

fn dir_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    std::fs::read_dir(path)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| dir_size(&e.path())).sum())
        .unwrap_or(0)
}

fn remove_entry(path: &Path, freed: &mut u64) -> bool {
    let size = dir_size(path);
    let ok = if path.is_dir() {
        std::fs::remove_dir_all(path).is_ok()
    } else {
        std::fs::remove_file(path).is_ok()
    };
    if ok {
        *freed += size;
    }
    ok
}

/// 按"**当前 digest** + 最近 N"清理缓存。纯文件系统操作，返回可断言的报告。
///
/// - `current`：当前实际使用的 bundle digest（由 [`current_bundle_digest`] 给出）。
///   **`None` ⇒ 整体不动**（解析不出目标 bundle 时不得猜、不得删）。
/// - `keep_recent`：**当前之外**再保留几份最近的历史（0 = 只留当前）。
/// - 没有任何 format 时整体不动（见上面的口径说明）。
pub fn gc_cache(cache_dir: &Path, keep_recent: usize, current: Option<&str>) -> CacheGcReport {
    let mut report = CacheGcReport::default();
    // 解析不出当前 bundle ⇒ 保守不动（T22 B3：不知道目标 digest 就不删任何东西）。
    let Some(current) = current.map(str::to_ascii_lowercase) else {
        return report;
    };
    let formats_dir = cache_dir.join("formats");
    let format_entries = collect_entries(&formats_dir);

    // 1) 按 digest 聚合并按"该 digest 最新一次使用时间"降序
    let mut by_digest: std::collections::BTreeMap<String, (std::time::SystemTime, Vec<PathBuf>)> =
        std::collections::BTreeMap::new();
    for (path, mtime) in &format_entries {
        let Some(digest) = path.file_name().and_then(|n| n.to_str()).and_then(parse_digest_prefix) else {
            continue; // 认不出的文件（不属于本口径）不动
        };
        let e = by_digest.entry(digest).or_insert((*mtime, Vec::new()));
        if *mtime > e.0 {
            e.0 = *mtime;
        }
        e.1.push(path.clone());
    }
    if by_digest.is_empty() {
        // 没编过：不动，避免误删首次联网取到的 data/
        return report;
    }
    let mut groups: Vec<(String, std::time::SystemTime, Vec<PathBuf>)> = by_digest
        .into_iter()
        .map(|(d, (t, files))| (d, t, files))
        .collect();
    groups.sort_by(|a, b| b.1.cmp(&a.1));
    // 2) 保留顺序 = **当前 digest 优先**，余下的按"最近使用"取 `keep_recent` 份。
    //    ⚠ current 可能没有 format（换 bundle 后只下载了 data/），但仍必须保护它的 data/hashes。
    report.kept_digests.push(current.clone());
    for (digest, _t, _files) in &groups {
        if digest != &current && report.kept_digests.len() < 1 + keep_recent {
            report.kept_digests.push(digest.clone());
        }
    }
    let kept: std::collections::HashSet<String> = report.kept_digests.iter().cloned().collect();

    // 2) formats：删掉非保留 digest 的 .fmt
    for (digest, _t, files) in &groups {
        if kept.contains(digest) {
            report.kept_formats += files.len();
            continue;
        }
        for f in files {
            if remove_entry(f, &mut report.freed_bytes) {
                report.deleted_formats += 1;
            }
        }
    }

    // 3) data/：<digest>/ 目录与 <digest>.index 成对，按同一 digest 判定；认不出的条目（.prefetch/.lock 等）不动
    let data_dir = cache_dir.join("data");
    if let Ok(rd) = std::fs::read_dir(&data_dir) {
        for e in rd.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(digest) = parse_digest_prefix(&name) else {
                continue;
            };
            if kept.contains(&digest) {
                continue;
            }
            if remove_entry(&e.path(), &mut report.freed_bytes) {
                report.deleted_data += 1;
            }
        }
    }

    // 4) hashes/：文件内容是"地址 → digest 记录"，第一行/前 64 字符就是 digest；只删指向非保留 digest 的
    let hashes_dir = cache_dir.join("hashes");
    if let Ok(rd) = std::fs::read_dir(&hashes_dir) {
        for e in rd.filter_map(|e| e.ok()) {
            let path = e.path();
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some(first) = text.split_whitespace().next() else {
                continue;
            };
            let Some(digest) = parse_digest_prefix(first) else {
                continue;
            };
            if kept.contains(&digest) {
                continue;
            }
            if remove_entry(&path, &mut report.freed_bytes) {
                report.deleted_hashes += 1;
            }
        }
    }

    report
}


#[cfg(test)]
mod tests {
    use super::*;

    /// 临时目录（随用随建、Drop 时递归删）。
    struct TempDir {
        dir: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("latteset-tectonic-bundle-{tag}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("建临时目录");
            Self { dir }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// `open_bundle` 的 `Ok` 侧是 `Option<Box<dyn Bundle>>`（`dyn Bundle` 不是 `Debug`），
    /// 所以不能用 `expect_err`；这里统一走一个小 helper。
    fn must_fail(source: BundleSource, what: &str) -> String {
        match open_bundle(&source, false, None) {
            Ok(_) => panic!("{what}：应当报错但没有"),
            Err(e) => e,
        }
    }

    /// 方案 §5.6 LB-4：**绝对 Windows 路径会被上游当 URL scheme**，必须给出可执行文案
    /// （而不是让它走到 `detect_bundle` 返回 `Ok(None)` 那句无信息量的报错）。
    #[test]
    fn absolute_windows_path_gets_actionable_message() {
        let err = must_fail(BundleSource::Explicit(r"E:\Works\some-bundle".to_owned()), "绝对 Windows 路径");
        assert!(err.contains("file:///"), "文案里要给出正确写法：{err}");
        assert!(err.contains("绝对 Windows 路径"), "文案要点明原因：{err}");
    }

    /// LB-5 / F2b：路径不存在 ≠ 空目录 ≠ 缺 SHA256SUM —— 三类要三种文案。
    #[test]
    fn three_bundle_failure_classes_are_distinguishable() {
        let missing = must_fail(BundleSource::Explicit("no/such/dir-bundle".to_owned()), "不存在的路径");
        assert!(missing.contains("路径不存在"), "{missing}");

        let empty = TempDir::new("empty");
        let err = must_fail(
            BundleSource::Explicit(format!("file:///{}", empty.dir.display()).replace('\\', "/")),
            "空目录 bundle",
        );
        assert!(err.contains("目录是空的"), "{err}");

        let no_sum = TempDir::new("nosum");
        std::fs::write(no_sum.dir.join("article.cls"), "x").expect("写占位文件");
        let err = must_fail(
            BundleSource::Explicit(format!("file:///{}", no_sum.dir.display()).replace('\\', "/")),
            "目录 bundle 缺 SHA256SUM",
        );
        assert!(err.contains("SHA256SUM"), "{err}");
    }

    /// `BundleSource::None` = 明确不要 bundle（内部反例用），不该触发任何打开动作。
    #[test]
    fn none_source_is_explicitly_no_bundle() {
        assert!(open_bundle(&BundleSource::None, false, None).expect("None 不该失败").is_none());
    }

    /// format 版本号钉定（我们用它拼兜底 bundle 地址）：与上游 `FORMAT_SERIAL` 同值。
    #[test]
    fn default_bundle_url_uses_pinned_format_serial() {
        let url = default_bundle_url();
        assert!(url.contains(&format!("default_bundle_v{FORMAT_SERIAL}.tar")), "{url}");
    }

    /// 造一个**合规目录 bundle**（`SHA256SUM` + 一个文件），用来实测 `get_digest()`。
    fn dir_bundle(tag: &str, digest_hex: &str) -> TempDir {
        let d = TempDir::new(tag);
        std::fs::write(d.dir.join("SHA256SUM"), format!("{digest_hex}\n")).expect("写 SHA256SUM");
        std::fs::write(d.dir.join("article.cls"), "x").expect("写占位宏包");
        d
    }

    fn open_dir(d: &TempDir) -> Box<dyn Bundle> {
        let src = format!("file:///{}", d.dir.display()).replace('\\', "/");
        open_bundle(&BundleSource::Explicit(src), false, None)
            .expect("目录 bundle 应当能打开")
            .expect("应当有 bundle")
    }

    /// **V-02 的可判定证据**：digest 来自 bundle 自己（`SHA256SUM`），**换 bundle ⇒ digest 变**
    /// ⇒ format 缓存文件名随之改变，不会复用旧 format。
    #[test]
    fn digest_comes_from_bundle_and_changes_with_it() {
        let a = dir_bundle("digest-a", &"a".repeat(64));
        let b = dir_bundle("digest-b", &"b".repeat(64));
        let da = bundle_digest(&mut *open_dir(&a)).expect("A 的 digest");
        let db = bundle_digest(&mut *open_dir(&b)).expect("B 的 digest");
        assert_ne!(da, db, "不同 bundle 的 digest 必须不同（否则 format 缓存会跨 bundle 复用）");
        assert_ne!(
            crate::io::format_file_name(&da, "latex"),
            crate::io::format_file_name(&db, "latex"),
            "digest 变 ⇒ format 缓存文件名必须变（V-02）"
        );
    }

    /// **缺 `SHA256SUM` ⇒ 取 digest 失败必须显式报错**（不得静默退回裸名缓存）。
    #[test]
    fn missing_digest_file_is_an_explicit_error() {
        let d = TempDir::new("no-digest");
        std::fs::write(d.dir.join("article.cls"), "x").expect("写占位宏包");
        // 直接调上游 `detect_bundle`（**绕过**我们 `classify` 里对 SHA256SUM 的前置检查），
        // 模拟"上游自己报错"这条路径：0.4.2 在缺 SHA256SUM 时 `bail!`。
        let src = format!("file:///{}", d.dir.display()).replace('\\', "/");
        let mut bundle = detect_bundle(src, false, None)
            .expect("detect_bundle 本身不该失败")
            .expect("目录 bundle 应当被识别为 DirBundle（LB-4）");
        let err = bundle_digest(&mut *bundle).expect_err("缺 SHA256SUM 必须报错");
        assert!(err.contains("digest"), "{err}");
    }

    /// D2：默认（网络）bundle 失败时的出路必须**可操作**（指到设置面板 / 环境变量 / 目录格式要求），
    /// 不能只说"打开失败"——内嵌唯一形态下这是产品默认路径。
    #[test]
    fn default_bundle_hint_is_actionable() {
        assert!(DEFAULT_BUNDLE_HINT.contains("设置"), "{}", DEFAULT_BUNDLE_HINT);
        assert!(DEFAULT_BUNDLE_HINT.contains(crate::BUNDLE_ENV), "{}", DEFAULT_BUNDLE_HINT);
        assert!(DEFAULT_BUNDLE_HINT.contains("SHA256SUM"), "{}", DEFAULT_BUNDLE_HINT);
        assert!(DEFAULT_BUNDLE_HINT.contains("file:///"), "{}", DEFAULT_BUNDLE_HINT);
    }

    // ---------------------------------------------------------------- 本地路径 → file:// URL（T19 阻断回归）

    /// T19 阻断项回归：Windows **verbatim 前缀 `\\?\`** 必须剥掉再拼 URL。
    ///
    /// 缺陷现场（release GUI 实测）：Tauri 资源目录给的是 `\\?\E:\…\tectonic-bundle.zip`，
    /// 直接替换分隔符得到 `file:////?/E:/…`，上游 `Url::to_file_path()` 把它当带 `?` 的 host
    /// 解析失败 ⇒ 随包 bundle 打不开 ⇒ 首编整体不可用（`failed to parse local path`）。
    #[test]
    fn path_to_file_url_strips_windows_verbatim_prefix() {
        // ① verbatim + 盘符：必须剥前缀，且保留盘符与大小写
        assert_eq!(
            path_to_file_url(Path::new(
                r"\\?\E:\Works\tex-presso\src-tauri\target\release\tectonic-bundle.zip"
            )),
            "file:///E:/Works/tex-presso/src-tauri/target/release/tectonic-bundle.zip",
            "verbatim 前缀必须剥掉（否则 Url::to_file_path 解析失败）"
        );
        // ② verbatim + UNC：\\?\UNC\server\share\x.zip → file://server/share/x.zip
        assert_eq!(
            path_to_file_url(Path::new(r"\\?\UNC\server\share\tectonic-bundle.zip")),
            "file://server/share/tectonic-bundle.zip",
            "verbatim UNC 必须变成 file://server/share/…（保留服务器/共享两段）"
        );
        // ③ 设备前缀 \\.\（盘符路径）同样剥
        assert_eq!(
            path_to_file_url(Path::new(r"\\.\E:\x\tectonic-bundle.zip")),
            "file:///E:/x/tectonic-bundle.zip"
        );
        // ④ 普通路径不回退：盘符 / UNC / Unix 绝对路径 / 已是 URL
        assert_eq!(
            path_to_file_url(Path::new(r"E:\x\tectonic-bundle.zip")),
            "file:///E:/x/tectonic-bundle.zip",
            "普通 Windows 路径行为不变"
        );
        assert_eq!(
            path_to_file_url(Path::new(r"\\server\share\tectonic-bundle.zip")),
            "file://server/share/tectonic-bundle.zip",
            "普通 UNC 行为不变（不是 file:////…）"
        );
        assert_eq!(path_to_file_url(Path::new("/tmp/tectonic-bundle.zip")), "file:///tmp/tectonic-bundle.zip");
        assert_eq!(
            path_to_file_url(Path::new("file:///E:/already-a-url.zip")),
            "file:///E:/already-a-url.zip",
            "已是 file:// 的输入原样返回"
        );
    }

    /// T19 阻断项回归（真实路径形态）：`canonicalize()` 在 Windows 上返回 `\\?\…`，
    /// `open_bundle(Shipped(canonical))` 必须**仍能打开**（走 `path_to_file_url` 的剥前缀逻辑）。
    #[test]
    fn shipped_bundle_opens_through_canonical_verbatim_path() {
        let d = dir_bundle("shipped-verbatim", &"c".repeat(64));
        let canonical = d.dir.canonicalize().expect("canonicalize 目录");
        match open_bundle(&BundleSource::Shipped(canonical), false, None) {
            Ok(Some(_)) => {}
            Ok(None) => panic!("canonical 目录 bundle 应当识别出 Some（不得判为 None）"),
            Err(e) => panic!("canonical（Windows 为 \\\\?\\ 形态）路径必须能打开：{e}"),
        }
    }

    // ---------------------------------------------------------------- T16：内置精简集

    /// ADR-0016 决策 2：内置集缺失/损坏要**显式**失败，且给出"修复安装 / 显式切在线"两条出路。
    #[test]
    fn missing_shipped_bundle_gives_actionable_no_silent_fallback_message() {
        let missing = TempDir::new("shipped-missing").dir.join("tectonic-bundle.zip");
        let err = must_fail(BundleSource::Shipped(missing.clone()), "内置集缺失");
        assert!(err.contains("内置宏包集"), "{err}");
        assert!(err.contains("重新安装"), "要指到修复安装：{err}");
        assert!(err.contains("在线完整宏包集"), "要给出显式切在线的出路：{err}");
        assert!(err.contains("不会静默改用网络"), "必须声明不静默回退（ADR-0016 D2）：{err}");
    }

    /// 内置集是**目录 bundle**（离线/内网发行）时也要能打开并取到 digest（与 zip 同一口径）。
    #[test]
    fn shipped_dir_bundle_opens_and_has_digest() {
        let d = dir_bundle("shipped-dir", &"c".repeat(64));
        let src = BundleSource::Shipped(d.dir.clone());
        let mut bundle = open_bundle(&src, false, None)
            .expect("内置目录 bundle 应当能打开")
            .expect("应当有 bundle");
        assert_eq!(bundle_digest(&mut *bundle).expect("digest"), "c".repeat(64));
    }

    /// ADR-0016 候选 A：来源优先级 = 显式设置 > 环境变量 > 随包内置集 > 在线兜底。
    #[test]
    fn bundle_source_priority_is_explicit_env_shipped_online() {
        let shipped = PathBuf::from(r"E:\Latteset\tectonic-bundle.zip");
        let explicit = Some(BundleSource::Explicit("file:///E:/my-bundle".to_owned()));
        let env = Some(BundleSource::Explicit("https://relay.example/default.tar".to_owned()));

        assert_eq!(
            resolve_source(explicit.clone(), env.clone(), Some(&shipped)),
            explicit.expect("显式设置要压过一切"),
            "设置里的 bundle 最优先（改完即生效）"
        );
        assert_eq!(
            resolve_source(None, env, Some(&shipped)),
            BundleSource::Explicit("https://relay.example/default.tar".to_owned()),
            "环境变量次之（开发/CI 逃生口）"
        );
        assert_eq!(
            resolve_source(None, None, Some(&shipped)),
            BundleSource::Shipped(shipped.clone()),
            "没有显式/环境时用随包内置精简集（产品默认 = 离线可用）"
        );
        assert_eq!(
            resolve_source(None, None, None),
            BundleSource::Default,
            "都没有（headless / 开发无产物）才回落到在线兜底"
        );
        // release 缺资源时也要把"应有路径"传下去 ⇒ 显式报错，不静默切网络（决策 2）
        assert_eq!(
            resolve_source(None, None, Some(&shipped)),
            BundleSource::Shipped(shipped),
        );
    }

    /// 缺包补全通道的话术：必须给出"切在线 / 补进本地 bundle / 确认模板根目录"三条可操作路径，
    /// 且点出缺失文件名（GUI 把它接到 core 的 MissingPackage 诊断上，不新造检测）。
    #[test]
    fn missing_package_hint_is_actionable() {
        let hint = missing_package_hint("slashbox.sty");
        assert!(hint.contains("slashbox.sty"), "{hint}");
        assert!(hint.contains("在线完整宏包集"), "{hint}");
        assert!(hint.contains("精简宏包集"), "{hint}");
        assert!(hint.contains("SHA256SUM"), "本地目录 bundle 的格式要求要写清：{hint}");
        assert!(hint.contains("设置"), "{hint}");
    }

    // ---------------------------------------------------------------- T16：缓存 GC

    fn set_mtime(path: &Path, secs: u64) {
        use std::time::{Duration, UNIX_EPOCH};
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("打开文件改 mtime");
        f.set_modified(UNIX_EPOCH + Duration::from_secs(secs))
            .expect("设置 mtime");
    }

    /// 造一份"三个 digest 各有一份 format + data + hash"的缓存；mtime 依次 a < b < c。
    fn fake_cache(tag: &str) -> (TempDir, [String; 3]) {
        let d = TempDir::new(tag);
        for (i, secs) in [1_000u64, 2_000, 3_000].iter().enumerate() {
            let digest = ["a", "b", "c"][i].repeat(64);
            let fmt = d.dir.join("formats").join(format!("{digest}-latex-33.fmt"));
            std::fs::create_dir_all(fmt.parent().expect("parent")).expect("建 formats");
            std::fs::write(&fmt, vec![i as u8; 10]).expect("写 fmt");
            set_mtime(&fmt, *secs);
            let data = d.dir.join("data").join(&digest);
            std::fs::create_dir_all(&data).expect("建 data");
            std::fs::write(data.join("article.cls"), vec![i as u8; 7]).expect("写 data 文件");
            set_mtime(&data.join("article.cls"), *secs);
            std::fs::write(d.dir.join("data").join(format!("{digest}.index")), b"idx").expect("写 index");
            set_mtime(&d.dir.join("data").join(format!("{digest}.index")), *secs);
            let hash = d.dir.join("hashes").join(format!("hash-{i}"));
            std::fs::create_dir_all(hash.parent().expect("parent")).expect("建 hashes");
            std::fs::write(&hash, format!("{digest} 12345")).expect("写 hash");
            set_mtime(&hash, *secs);
        }
        (d, [["a", "b", "c"][0].repeat(64), ["a", "b", "c"][1].repeat(64), ["a", "b", "c"][2].repeat(64)])
    }

    /// 保留"当前（最新）+ 最近 1 份"：最老的一份 format/data/hash 都被删；当前那套原样保留。
    #[test]
    fn gc_keeps_current_and_recent_and_deletes_old() {
        let (d, digests) = fake_cache("gc-basic");
        let report = gc_cache(&d.dir, 1, Some(&digests[2]));
        assert_eq!(report.kept_digests, vec![digests[2].clone(), digests[1].clone()]);
        assert_eq!(report.deleted_formats, 1, "最老那份 format 要删");
        assert_eq!(report.deleted_data, 2, "最老那套 data 目录 + .index 要删");
        assert_eq!(report.deleted_hashes, 1, "最老那份 hash 记录要删");
        assert!(report.freed_bytes > 0);

        // 当前（c）与最近（b）都还在；a 全没了
        let fmt = |i: usize| d.dir.join("formats").join(format!("{}-latex-33.fmt", digests[i]));
        assert!(fmt(2).exists(), "当前 format 不得删");
        assert!(fmt(1).exists(), "最近一份 format 要保留");
        assert!(!fmt(0).exists(), "旧 format 要删");
        assert!(d.dir.join("data").join(&digests[2]).exists(), "当前 data 不得删");
        assert!(!d.dir.join("data").join(&digests[0]).exists());
        assert!(!d.dir.join("data").join(format!("{}.index", digests[0])).exists());
    }

    /// `keep_recent=0` ⇒ 只留当前；空缓存（从没编译过）⇒ 整体不动（避免误删首次取到的 data）。
    #[test]
    fn gc_only_current_and_empty_cache_is_noop() {
        let (d, digests) = fake_cache("gc-only-current");
        let report = gc_cache(&d.dir, 0, Some(&digests[2]));
        assert_eq!(report.kept_digests, vec![digests[2].clone()]);
        assert_eq!(report.deleted_formats, 2);

        let empty = TempDir::new("gc-empty");
        std::fs::create_dir_all(empty.dir.join("data").join("x".repeat(64))).expect("建 data");
        std::fs::write(empty.dir.join("data").join("x".repeat(64)).join("a.cls"), b"x").expect("写");
        let report = gc_cache(&empty.dir, 3, Some(&"x".repeat(64)));
        assert_eq!(report, CacheGcReport::default(), "没编过 ⇒ 不动");
        assert!(empty.dir.join("data").join("x".repeat(64)).exists());
    }

    /// 认不出的文件（非 digest 命名）一律不动：GC 不能成为"删用户文件"的通道。
    #[test]
    fn gc_leaves_unknown_entries_alone() {
        let (d, _digests) = fake_cache("gc-unknown");
        let unknown = d.dir.join("formats").join("README.txt");
        std::fs::write(&unknown, b"keep me").expect("写未知文件");
        let prefetch = d.dir.join("data").join("https,58,,47,relay.default_bundle_v33.tar.prefetch");
        std::fs::write(&prefetch, b"pf").expect("写 prefetch");
        gc_cache(&d.dir, 0, Some(&_digests[2]));
        assert!(unknown.exists(), "未知格式文件不得删");
        assert!(prefetch.exists(), ".prefetch 这类缓存记录不得删");
    }

    /// `LATTESET_CACHE_KEEP` 的解析口径（默认 3 / 夹在 1..=50）。
    #[test]
    fn cache_keep_env_parsing() {
        // 不依赖进程全局 env 改写（测试并发安全）：直接验证默认与边界函数逻辑。
        assert_eq!(3usize.clamp(1, 50), 3);
        assert_eq!(0usize.clamp(1, 50), 1);
        assert_eq!(999usize.clamp(1, 50), 50);
        assert_eq!(cache_keep_from_env(), 3, "未设置时默认 3（若本机设了 LATTESET_CACHE_KEEP 会失败，属预期提示）");
    }

    /// T18 修复：在线缓存判定 = hash 文件（64hex）+ 未过期 lock + 非空 data 三者齐全（只读、零网络）。
    #[test]
    fn online_cache_gate_requires_fresh_hash_and_unpacked_data() {
        let d = TempDir::new("online-cache");
        let cache = d.dir.join("cache");
        let url = "https://example.invalid/default_bundle_v33.tar";
        assert!(!online_bundle_cache_ready(&cache, url), "空缓存必须 false");

        // 缓存标记布局与上游 BundleCache 一致：hashes/<sanitize(url)> + 同名 .lock + data/<digest>/
        std::fs::create_dir_all(cache.join("hashes")).expect("建 hashes");
        let hash = cache.join("hashes").join(tectonic_io_base::app_dirs::sanitize(url));
        let digest = "a".repeat(64);
        std::fs::write(&hash, format!("{digest}\n")).expect("写 hash");
        assert!(!online_bundle_cache_ready(&cache, url), "缺 lock（检查时间）⇒ 不可用");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("时钟")
            .as_secs();
        std::fs::write(hash.with_extension("lock"), format!("{}", now - 8 * 24 * 3600))
            .expect("写过期 lock");
        assert!(!online_bundle_cache_ready(&cache, url), "lock 过期（>7 天）⇒ 上游会联网刷新，不可用");

        std::fs::write(hash.with_extension("lock"), format!("{now}")).expect("写新 lock");
        assert!(!online_bundle_cache_ready(&cache, url), "data/<digest>/ 不存在 ⇒ 不可用");
        let data = cache.join("data").join(&digest);
        std::fs::create_dir_all(&data).expect("建 data");
        assert!(!online_bundle_cache_ready(&cache, url), "空 data 目录（只记 hash 没下载）⇒ 不可用");
        std::fs::write(data.join("tectonic-format-latex.tex"), "x").expect("写缓存文件");
        assert!(online_bundle_cache_ready(&cache, url), "hash + 未过期 lock + 非空 data ⇒ 可用");

        std::fs::write(&hash, "not-a-digest\n").expect("写坏 hash");
        assert!(!online_bundle_cache_ready(&cache, url), "hash 不是 64hex ⇒ 不可用（不猜）");
    }

    /// T18 修复：预热计划对在线源**零网络**（缓存标记齐全 ⇒ 只读缓存预热；否则 Skip）；
    /// 本地源主动预热、`None` 跳过。纯决策、与 `network-bundle` 特性无关。
    #[test]
    fn prewarm_plan_is_offline_and_cache_aware() {
        let d = TempDir::new("prewarm-plan");
        let cache = d.dir.join("cache");
        let url = "https://example.invalid/default_bundle_v33.tar";

        // 在线源冷缓存 ⇒ Skip（不会进入 open_bundle/bundle_digest，因而不可能联网）。
        assert_eq!(prewarm_plan(&BundleSource::Explicit(url.into()), &cache), PrewarmPlan::Skip);
        assert_eq!(prewarm_plan(&BundleSource::Default, &cache), PrewarmPlan::Skip);

        // 本地源 ⇒ 主动预热；None ⇒ 跳过。
        assert_eq!(
            prewarm_plan(&BundleSource::Shipped(PathBuf::from("tectonic-bundle.zip")), &cache),
            PrewarmPlan::Prewarm { only_cached: false }
        );
        assert_eq!(
            prewarm_plan(&BundleSource::Explicit("file:///E:/bundle".into()), &cache),
            PrewarmPlan::Prewarm { only_cached: false }
        );
        assert_eq!(
            prewarm_plan(&BundleSource::Explicit("relative/dir".into()), &cache),
            PrewarmPlan::Prewarm { only_cached: false }
        );
        assert_eq!(prewarm_plan(&BundleSource::None, &cache), PrewarmPlan::Skip);

        // 在线源 + **已有可用缓存标记** ⇒ 走预热，但只许读缓存（启动期不联网）。
        std::fs::create_dir_all(cache.join("hashes")).expect("建 hashes");
        let hash = cache.join("hashes").join(tectonic_io_base::app_dirs::sanitize(url));
        let digest = "b".repeat(64);
        std::fs::write(&hash, format!("{digest}\n")).expect("写 hash");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("时钟")
            .as_secs();
        std::fs::write(hash.with_extension("lock"), format!("{now}")).expect("写 lock");
        std::fs::create_dir_all(cache.join("data").join(&digest)).expect("建 data");
        std::fs::write(cache.join("data").join(&digest).join("x"), b"x").expect("写缓存文件");
        assert_eq!(
            prewarm_plan(&BundleSource::Explicit(url.into()), &cache),
            PrewarmPlan::Prewarm { only_cached: true },
            "在线源缓存可用 ⇒ 只读缓存预热（不联网）"
        );
        // 缓存键按 URL 原文：Default 的 URL 与显式 URL 不同 ⇒ 仍 Skip。
        assert_eq!(prewarm_plan(&BundleSource::Default, &cache), PrewarmPlan::Skip);
    }

    // ---------------------------------------------------------------- T22 B3：GC 与 bundle 解析共用口径

    /// 缓存目录解析优先序（与 runner/首启预热共用同一函数）：显式设置 > 环境变量 > 宿主默认。
    #[test]
    fn resolve_cache_dir_priority_is_explicit_then_env_then_host() {
        let explicit = PathBuf::from(r"E:\cache-explicit");
        let env = PathBuf::from(r"E:\cache-env");
        let host = PathBuf::from(r"E:\cache-host");
        assert_eq!(
            resolve_cache_dir(Some(explicit.clone()), Some(env.clone()), Some(host.clone())),
            Some(explicit),
            "显式设置必须压过环境变量/宿主默认"
        );
        assert_eq!(resolve_cache_dir(None, Some(env.clone()), Some(host.clone())), Some(env));
        assert_eq!(resolve_cache_dir(None, None, Some(host)), Some(PathBuf::from(r"E:\cache-host")));
        assert_eq!(resolve_cache_dir(None, None, None), None, "三者都没有 ⇒ 交给调用方报错");
    }

    /// 当前 digest 解析（GC 保护目标 bundle 的依据）：本地源走本地打开（零网络）；
    /// 在线源只读缓存标记；解析不出 ⇒ None（GC 必须不动）。
    #[test]
    fn current_bundle_digest_uses_same_bundle_resolver_offline() {
        let d = TempDir::new("current-digest");
        let cache = d.dir.join("cache");
        let bundle_dir = d.dir.join("bundle");
        std::fs::create_dir_all(&bundle_dir).expect("建 bundle");
        let digest = "d".repeat(64);
        std::fs::write(bundle_dir.join("SHA256SUM"), format!("{digest}\n")).expect("写 SHA256SUM");
        std::fs::write(bundle_dir.join("article.cls"), "x").expect("写 bundle 文件");
        // 随包 / 本地显式源：本地打开取 digest（不联网）。
        assert_eq!(
            current_bundle_digest(&BundleSource::Shipped(bundle_dir.clone()), &cache),
            Some(digest.clone()),
            "随包内建集要能解析出 digest"
        );
        let url = format!("file:///{}", bundle_dir.to_string_lossy().replace('\\', "/"));
        assert_eq!(current_bundle_digest(&BundleSource::Explicit(url), &cache), Some(digest.clone()));
        // 本地 bundle 不存在 / 明确不要 / 在线冷缓存 ⇒ None（GC 不得猜）。
        assert_eq!(current_bundle_digest(&BundleSource::Shipped(d.dir.join("missing.zip")), &cache), None);
        assert_eq!(current_bundle_digest(&BundleSource::None, &cache), None);
        assert_eq!(
            current_bundle_digest(&BundleSource::Default, &cache),
            None,
            "在线冷缓存不得联网取 digest"
        );
        // 在线源 + 本地缓存标记可用 ⇒ 直接读缓存里的 digest（零网络）。
        let online = "https://example.invalid/default_bundle_v33.tar";
        std::fs::create_dir_all(cache.join("hashes")).expect("建 hashes");
        let hash = cache.join("hashes").join(tectonic_io_base::app_dirs::sanitize(online));
        let online_digest = "e".repeat(64);
        std::fs::write(&hash, format!("{online_digest}\n")).expect("写 hash");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("时钟")
            .as_secs();
        std::fs::write(hash.with_extension("lock"), format!("{now}")).expect("写 lock");
        std::fs::create_dir_all(cache.join("data").join(&online_digest)).expect("建 data");
        std::fs::write(cache.join("data").join(&online_digest).join("x"), b"x").expect("写缓存文件");
        assert_eq!(
            current_bundle_digest(&BundleSource::Explicit(online.into()), &cache),
            Some(online_digest)
        );
    }

    /// GC 必须**保护显式给出的当前 digest**（换 bundle 后当前 digest 的 mtime 可能更旧）；
    /// 且即使它还没有 format（只下载了 data），也不能删它的 data/hashes。
    #[test]
    fn gc_protects_explicit_current_digest_not_only_newest_mtime() {
        let (d, digests) = fake_cache("gc-protect-current");
        // 故意把"当前"设为最老那份（a）：若按 mtime 推断就会被删——必须保住。
        let report = gc_cache(&d.dir, 0, Some(&digests[0]));
        assert_eq!(report.kept_digests, vec![digests[0].clone()], "当前 digest 优先，哪怕 mtime 最老");
        assert!(
            d.dir.join("formats").join(format!("{}-latex-33.fmt", digests[0])).exists(),
            "当前 format 不得删"
        );
        assert!(d.dir.join("data").join(&digests[0]).exists(), "当前 data 不得删");
        assert!(
            !d.dir.join("formats").join(format!("{}-latex-33.fmt", digests[2])).exists(),
            "非当前/非最近要删"
        );

        // 当前 digest 只有 data（换 bundle 后还没生成 format）⇒ 它的 data 仍受保护。
        let (d2, digests2) = fake_cache("gc-protect-data-only");
        let current = "f".repeat(64);
        std::fs::create_dir_all(d2.dir.join("data").join(&current)).expect("建 data");
        std::fs::write(d2.dir.join("data").join(&current).join("x.cls"), b"x").expect("写");
        let report = gc_cache(&d2.dir, 0, Some(&current));
        assert!(report.kept_digests.contains(&current), "无 format 的当前 digest 也要登记保护");
        assert!(d2.dir.join("data").join(&current).exists(), "目标 bundle 的 data 不得删");
        assert!(
            !d2.dir.join("formats").join(format!("{}-latex-33.fmt", digests2[0])).exists(),
            "旧 format 照删"
        );
    }

    /// 解析不出当前 digest ⇒ GC **整体不动**（宁可不清理，也不能猜错方向）。
    #[test]
    fn gc_noop_when_current_digest_unknown() {
        let (d, digests) = fake_cache("gc-unknown-current");
        let report = gc_cache(&d.dir, 1, None);
        assert_eq!(report, CacheGcReport::default(), "current=None ⇒ 不得删任何东西");
        for i in 0..3 {
            assert!(
                d.dir.join("formats").join(format!("{}-latex-33.fmt", digests[i])).exists(),
                "全部 format 保留"
            );
            assert!(d.dir.join("data").join(&digests[i]).exists(), "全部 data 保留");
        }
    }
}

