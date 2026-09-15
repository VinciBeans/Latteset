//! Tectonic 子进程形态的 bundle 获取策略与缓存判定（P0，2026-09）。
//!
//! 要修的现状缺陷（`docs/tectonic-library-plan.md` §6 P0 / 表 E-1）：`tectonic_command` 原来
//! **无条件**给命令加 `-C`（== `--only-cached`），于是全新机器 + 空缓存时它拒绝联网取 bundle、
//! 首编必失败，上游原文 `this bundle isn't cached, and we couldn't get it from the internet`
//! （`test_file/tectonic-src/crates/bundles/src/cache.rs:181`）。
//!
//! 本模块负责两件事：
//! 1. [`probe`] —— 给「这一次能不能加 `-C`」一个**只认正向证据**的判定（不用猜的启发式）；
//! 2. [`bundle_failure_message`] —— 把上游的 bundle/网络失败换成可执行文案。
//!
//! 路由：`runner.rs` 在构造 Tectonic 命令前调 [`probe`]，失败收尾后调 [`bundle_failure_message`]。

use std::path::{Path, PathBuf};

/// 取 bundle 的策略（命令构造参数；**非 Tectonic 引擎不看它**）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BundlePolicy {
    /// 给命令加 `-C`：只用本地缓存，不联网、结果稳定。**只有拿到正向证据**才选它。
    CachedOnly,
    /// 不加 `-C`：允许按需联网取 bundle；取到的文件落缓存，下次即可离线。
    AllowFetch,
}

/// 离线开关：`1/true/yes/on` 强制 [`BundlePolicy::CachedOnly`]，`0/false/no/off` 强制
/// [`BundlePolicy::AllowFetch`]。覆盖自动判定，供两种反向场景使用——
/// 缓存不完整但坚持离线；缓存就绪但这次要用缓存里没有的新宏包。
pub const OFFLINE_ENV: &str = "LATTESET_TECTONIC_OFFLINE";

/// 探测结果：策略 + 判定依据（日志与复核用）。
#[derive(Clone, Debug)]
pub struct Probe {
    pub policy: BundlePolicy,
    /// [`OFFLINE_ENV`] 的显式取值（`None` = 未设，走自动判定）。
    pub forced: Option<bool>,
    /// 上游缓存根下的 `bundles` 目录（判定的对象）。
    pub cache_dir: Option<PathBuf>,
    /// `cache_dir` 里是否有可用的默认 bundle（[`cache_ready`] 的证据结论）。
    pub cache_ready: bool,
}

/// 按当前环境探测策略（读 [`OFFLINE_ENV`] + 查缓存证据）。
pub fn probe() -> Probe {
    let forced = force_offline(std::env::var(OFFLINE_ENV).ok().as_deref());
    let candidates = bundles_dir_candidates(host_os(), &CacheEnv::from_env());
    // 第一个「有证据」的候选胜出；都无证据时仍返回首选候选（日志里要能看到查的是哪个目录）。
    let hit = candidates.iter().find(|d| cache_ready(d)).cloned();
    let cache_ready = hit.is_some();
    let cache_dir = hit.or_else(|| candidates.into_iter().next());
    Probe { policy: decide(forced, cache_ready), forced, cache_dir, cache_ready }
}

/// 策略判定（纯函数，便于单测）：显式开关 > 缓存证据。
///
/// 缓存没有正向证据 ⇒ [`BundlePolicy::AllowFetch`]（首编能拿到 bundle）；反过来猜"缓存就绪"
/// 会让首编硬失败，所以这里只认 [`cache_ready`] 的证据。
pub fn decide(forced: Option<bool>, cache_ready: bool) -> BundlePolicy {
    match forced {
        Some(true) => BundlePolicy::CachedOnly,
        Some(false) => BundlePolicy::AllowFetch,
        None if cache_ready => BundlePolicy::CachedOnly,
        None => BundlePolicy::AllowFetch,
    }
}

/// 解析 [`OFFLINE_ENV`] 的取值；认不出或为空按"未设"处理（不猜用户意图）。
pub fn force_offline(raw: Option<&str>) -> Option<bool> {
    match raw?.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// 平台分支（显式枚举以便单测覆盖非宿主平台）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Os {
    Windows,
    MacOs,
    Other,
}

pub fn host_os() -> Os {
    if cfg!(target_os = "windows") {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::MacOs
    } else {
        Os::Other
    }
}

/// 上游缓存目录的来源。显式传参而不是就地读环境，平台分支才可单测。
#[derive(Clone, Debug, Default)]
pub struct CacheEnv {
    /// `TECTONIC_CACHE_DIR`：设了就**整个替换**平台缓存根
    /// （`.../io_base/src/app_dirs.rs:82-96`）。
    pub tectonic_cache_dir: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub xdg_cache_home: Option<PathBuf>,
}

impl CacheEnv {
    pub fn from_env() -> Self {
        let read = |k: &str| std::env::var_os(k).map(PathBuf::from);
        Self {
            tectonic_cache_dir: read("TECTONIC_CACHE_DIR"),
            local_app_data: read("LOCALAPPDATA"),
            home: read("HOME"),
            xdg_cache_home: read("XDG_CACHE_HOME"),
        }
    }
}

/// Tectonic 的 `<cache>/bundles` 目录候选，与上游 `app_dirs::get_user_cache_dir("bundles")`
/// 同口径（`.../io_base/src/app_dirs.rs:19-20,82-96`）：Windows 是
/// `%LOCALAPPDATA%\TectonicProject\Tectonic\cache\bundles`（本机实测一致）。
///
/// 非 Windows 多给一个小写目录候选：`directories` 对 project path 的大小写处理随版本变动，
/// 而上游文档写的是 `$XDG_CACHE_HOME/Tectonic`。候选**必须过 [`cache_ready`]** 才算命中，
/// 所以猜错只会退回"允许联网"，不会误判成离线。
pub fn bundles_dir_candidates(os: Os, env: &CacheEnv) -> Vec<PathBuf> {
    if let Some(root) = &env.tectonic_cache_dir {
        return vec![root.join("bundles")];
    }
    let bases: Vec<PathBuf> = match os {
        Os::Windows => env
            .local_app_data
            .iter()
            .map(|d| d.join("TectonicProject").join("Tectonic").join("cache"))
            .collect(),
        Os::MacOs => env.home.iter().map(|h| h.join("Library").join("Caches").join("Tectonic")).collect(),
        Os::Other => env
            .xdg_cache_home
            .clone()
            .or_else(|| env.home.as_ref().map(|h| h.join(".cache")))
            .into_iter()
            .flat_map(|r| [r.join("Tectonic"), r.join("tectonic")])
            .collect(),
    };
    bases.into_iter().map(|b| b.join("bundles")).collect()
}

/// 「这个 `<cache>/bundles` 里有可用的默认 bundle」的**正向证据**，三条全中才算：
///
/// 1. `hashes/<sanitized location>` 是上游写的 bundle 哈希标记（`.../bundles/src/cache.rs:133-145`），
///    文件名含 `default_bundle` —— 默认/兜底 bundle 家族的 location 是
///    `{prefix}/default_bundle_v{format_version}.tar`（`.../bundles/src/lib.rs:301-330`，本机实测
///    `https,58,...relay.fullyjustified.net,47,default_bundle_v33.tar`）；`*.lock` 是 7 天检查时间戳，
///    不算标记；
/// 2. 该文件内容是 64 位十六进制摘要（上游 `DigestData::from_str` 的格式），且
///    `data/<摘要>/` 解包目录里至少有 1 个文件（`cache.rs:238-248`）；
/// 3. **latex 格式已经在这台机器上生成过**：`<cache>/formats/<摘要>-latex-*.fmt` 存在
///    （命名见 `src/io/format_cache.rs:42-61`，摘要取自同一 bundle 的 `get_digest()`）。
///
/// 第 3 条的取舍（实测，本机 2026-09-15）：只要求"解包目录非空"会把**半途中断过的缓存**
/// （首编联网取 bundle 到一半就被超时杀掉：9.9 MB / 74 个文件、无 `.fmt`）判成就绪，随后
/// `-C` 编译死在 `error: failed to open input file "loadhyph-be.tex"`（连 `.log` 都没有）；
/// 而要求 `.fmt` 存在则退回联网档 —— 那次编译能继续补齐缓存，且**缺 `.fmt` 也可以离线现场生成**
/// （格式输入文件本就在解包目录里）。反过来，缓存里已有 `.fmt` 但少了这次文档新用的宏包时，
/// 仍会走离线档并在收尾给出"允许联网一次"的可执行文案（见 [`bundle_failure_message`]）——
/// 这是"保持离线"换来的已知取舍，产品侧的缓存/本地 bundle 管理归后续阶段。
///
/// 按家族名（而不是钉 `v33`）匹配，是为了容忍上游升级 format 版本；`-b` 指定的其它 bundle
/// 不参与判定——产品命令从不传 `-b`，把别的 bundle 当默认 bundle 会误判成可离线。
///
/// 漏判只会让这一次编译允许联网（安全方向），误判会让首编硬失败。
pub fn cache_ready(bundles_dir: &Path) -> bool {
    let cache_root = match bundles_dir.parent() {
        Some(root) => root,
        None => return false,
    };
    let Ok(entries) = std::fs::read_dir(bundles_dir.join("hashes")) else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.contains("default_bundle") || name.ends_with(".lock") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let digest = text.trim();
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let payload = bundles_dir.join("data").join(digest);
        let non_empty =
            std::fs::read_dir(&payload).map(|mut it| it.next().is_some()).unwrap_or(false);
        if non_empty && format_already_built(cache_root, digest) {
            return true;
        }
    }
    false
}

/// 证据③：`<cache>/formats/<bundle 摘要>-latex-<serial>.fmt` 已生成。
fn format_already_built(cache_root: &Path, digest: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(cache_root.join("formats")) else {
        return false;
    };
    entries.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        name.starts_with(digest) && name.contains("-latex-") && name.ends_with(".fmt")
    })
}

/// 上游 bundle/格式获取失败的原文标记：
/// - `crates/bundles/src/cache.rs:181`：冷缓存且取不到 bundle（`-C` 下的失败原文）；
/// - `crates/bundles/src/lib.rs:342`：兜底 bundle 打不开；
/// - 空缓存 + `-C` 的实测原文（本机 2026-09-15）：格式生成需要的输入文件不在缓存里。
const BUNDLE_FAILURE_MARKERS: [&str; 3] = [
    "isn't cached, and we couldn't get it from the internet",
    "could not open default bundle",
    "failed to open input file \"tectonic-format-",
];

/// 离线档下"缓存里缺这次要用的文件"的原文（实测：半填充缓存 + `-C` ⇒
/// `error: failed to open input file "loadhyph-be.tex"`，连 `.log` 都不落）。
/// 只在 `CachedOnly` 下认它：联网档出现同一句是"bundle 真的没有这个文件"，与缓存无关。
const OFFLINE_FILE_MISS_MARKER: &str = "failed to open input file \"";

/// 把上游的 bundle/网络失败换成可执行文案；没命中标记时返回 `None`（保持原有判定）。
///
/// 为什么要换：Tectonic 这类失败**只写 stderr、连 `tmp/<stem>.log` 都不落**，runner 现有路径
/// 会报「编译失败且无法读取日志（…）」，对用户没有任何可操作信息。
///
/// 文案里必须给出**当前策略下的有效出路**：离线档不能说"重新编译一次即可"——判定依据没变，
/// 重编还是离线档。有效出路是 [`OFFLINE_ENV`] 设为 `0` 允许联网一次，补齐缓存后自动回到离线档。
pub fn bundle_failure_message(policy: BundlePolicy, output_tail: &str) -> Option<String> {
    let line = BUNDLE_FAILURE_MARKERS
        .iter()
        .find_map(|marker| output_tail.lines().find(|l| l.contains(marker)))
        .or_else(|| match policy {
            BundlePolicy::CachedOnly => output_tail.lines().find(|l| l.contains(OFFLINE_FILE_MISS_MARKER)),
            BundlePolicy::AllowFetch => None,
        })?;
    let line = one_line(line);
    Some(match policy {
        BundlePolicy::CachedOnly => format!(
            "Tectonic 按离线缓存编译（-C），但本地缓存里缺少这次编译需要的文件。\
             处置：设 {OFFLINE_ENV}=0 允许联网后重新编译一次——补齐缓存后会自动回到离线档。\
             上游原文：{line}"
        ),
        BundlePolicy::AllowFetch => format!(
            "Tectonic 需要联网获取宏包集合（bundle），但这次没取到（网络不可用或被拦）。\
             确认网络（能访问 bundle 地址）后重新编译即可（首次下载约 60 MB，之后可离线）；\
             也可以在能联网的机器上预热缓存目录（TECTONIC_CACHE_DIR 指定位置）后整体拷过来。\
             上游原文：{line}"
        ),
    })
}

/// 压成一行并限长：上游原文含换行与路径，直接塞进错误列表很难读。
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(300).collect()
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
            let dir = std::env::temp_dir().join(format!(
                "latteset-tectonic-probe-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("建临时目录");
            Self { dir }
        }

        fn put(&self, rel: &str, text: &str) {
            let path = self.dir.join(rel);
            std::fs::create_dir_all(path.parent().expect("有父目录")).expect("建父目录");
            std::fs::write(path, text).expect("写文件");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    const DIGEST: &str = "6ffe055852f8faf66c0acbe1a7fb27f87b869a90bad1204f3bf4d9683f597c7c";
    const MARKER: &str = "https,58,,47,,47,relay.fullyjustified.net,47,default_bundle_v33.tar";

    #[test]
    fn policy_follows_cache_evidence_and_explicit_switch() {
        // 自动档：有证据才离线
        assert_eq!(decide(None, true), BundlePolicy::CachedOnly);
        assert_eq!(decide(None, false), BundlePolicy::AllowFetch);
        // 显式档压过证据（两个方向都要能压过）
        assert_eq!(decide(Some(true), false), BundlePolicy::CachedOnly);
        assert_eq!(decide(Some(false), true), BundlePolicy::AllowFetch);
    }

    #[test]
    fn offline_env_parsing_is_conservative() {
        for v in ["1", "true", "TRUE", " yes ", "on"] {
            assert_eq!(force_offline(Some(v)), Some(true), "{v} 应判为强制离线");
        }
        for v in ["0", "false", "No", "off"] {
            assert_eq!(force_offline(Some(v)), Some(false), "{v} 应判为强制联网");
        }
        for v in ["", "  ", "maybe", "2"] {
            assert_eq!(force_offline(Some(v)), None, "{v:?} 认不出 ⇒ 按未设");
        }
        assert_eq!(force_offline(None), None);
    }

    #[test]
    fn bundles_dir_matches_upstream_layout_per_platform() {
        let env = CacheEnv {
            tectonic_cache_dir: None,
            local_app_data: Some(PathBuf::from("C:/Users/knuth/AppData/Local")),
            home: Some(PathBuf::from("/home/knuth")),
            xdg_cache_home: Some(PathBuf::from("/home/knuth/.xdg-cache")),
        };
        assert_eq!(
            bundles_dir_candidates(Os::Windows, &env),
            vec![PathBuf::from("C:/Users/knuth/AppData/Local/TectonicProject/Tectonic/cache/bundles")]
        );
        assert_eq!(
            bundles_dir_candidates(Os::MacOs, &env),
            vec![PathBuf::from("/home/knuth/Library/Caches/Tectonic/bundles")]
        );
        assert_eq!(
            bundles_dir_candidates(Os::Other, &env),
            vec![
                PathBuf::from("/home/knuth/.xdg-cache/Tectonic/bundles"),
                PathBuf::from("/home/knuth/.xdg-cache/tectonic/bundles"),
            ]
        );

        // TECTONIC_CACHE_DIR 是整根替换（上游 app_dirs.rs:82-96），且对三个平台一视同仁
        let overridden = CacheEnv { tectonic_cache_dir: Some(PathBuf::from("D:/tectonic-cache")), ..env };
        for os in [Os::Windows, Os::MacOs, Os::Other] {
            assert_eq!(
                bundles_dir_candidates(os, &overridden),
                vec![PathBuf::from("D:/tectonic-cache/bundles")],
                "{os:?} 下 TECTONIC_CACHE_DIR 应压过平台默认"
            );
        }
    }

    #[test]
    fn cache_ready_needs_marker_digest_payload_and_format() {
        let t = TempDir::new("ready");
        let bundles = t.dir.join("cache").join("bundles");
        std::fs::create_dir_all(&bundles).expect("建 bundles 目录");
        assert!(!cache_ready(&bundles), "空目录不是就绪缓存");

        // 只有哈希标记、没有解包目录 ⇒ 不算（首编会缺文件）
        t.put(&format!("cache/bundles/hashes/{MARKER}"), &format!("{DIGEST}\n"));
        assert!(!cache_ready(&bundles), "有标记但无 data/<摘要>/ 必须判未就绪");

        // 解包目录是空的 ⇒ 不算
        std::fs::create_dir_all(bundles.join("data").join(DIGEST)).expect("建空解包目录");
        assert!(!cache_ready(&bundles), "空的解包目录不算就绪");

        // 解包目录非空、但格式没生成过 ⇒ 仍不算。实测依据：首编联网取 bundle 到一半被超时杀掉
        // （9.9 MB / 74 文件、无 .fmt）时若判成就绪，`-C` 会死在缺文件的硬错误上。
        t.put(&format!("cache/bundles/data/{DIGEST}/ctexart.cls"), "x");
        t.put(&format!("cache/bundles/data/{DIGEST}/tectonic-format-latex.tex"), "x");
        assert!(!cache_ready(&bundles), "没有 .fmt 时必须判未就绪（半填充缓存要留在联网档）");

        // 格式已生成 ⇒ 就绪
        t.put(&format!("cache/formats/{DIGEST}-latex-33.fmt"), "x");
        assert!(cache_ready(&bundles), "标记 + 摘要 + 非空解包目录 + 已生成 .fmt = 就绪");
    }

    #[test]
    fn cache_ready_rejects_foreign_format_file() {
        let t = TempDir::new("format");
        let bundles = t.dir.join("cache").join("bundles");
        t.put(&format!("cache/bundles/hashes/{MARKER}"), &format!("{DIGEST}\n"));
        t.put(&format!("cache/bundles/data/{DIGEST}/ctexart.cls"), "x");
        // 命名口径见 src/io/format_cache.rs:53-61：{bundle_digest}-{stem}-{FORMAT_SERIAL}.fmt
        t.put("cache/formats/00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff-latex-33.fmt", "x");
        assert!(!cache_ready(&bundles), "别的 bundle 摘要的 .fmt 不算");
        t.put(&format!("cache/formats/{DIGEST}-plain-33.fmt"), "x");
        assert!(!cache_ready(&bundles), "非 latex 的 .fmt 不算（我们固定用 latex 格式）");
    }

    #[test]
    fn cache_ready_rejects_lock_timestamp_and_other_bundles() {
        let t = TempDir::new("negative");

        // `.lock` 是 7 天检查时间戳，不是 bundle 标记（其余证据都给足，失败原因只能是没有标记）
        t.put(
            "cache/bundles/hashes/https,58,,47,,47,relay.fullyjustified.net,47,default_bundle_v33.lock",
            "1757000000",
        );
        t.put(&format!("cache/bundles/data/{DIGEST}/ctexart.cls"), "x");
        t.put(&format!("cache/bundles/data/{DIGEST}/tectonic-format-latex.tex"), "x");
        assert!(!cache_ready(&t.dir.join("cache/bundles")), "只有 .lock 时不能判就绪");

        // 非默认 bundle（`-b` 指定过别的 bundle）不参与判定
        let other = TempDir::new("other-bundle");
        other.put("cache/bundles/hashes/https,58,,47,,47,example.com,47,mine.tar", &format!("{DIGEST}\n"));
        other.put(&format!("cache/bundles/data/{DIGEST}/ctexart.cls"), "x");
        other.put(&format!("cache/bundles/data/{DIGEST}/tectonic-format-latex.tex"), "x");
        assert!(!cache_ready(&other.dir.join("cache/bundles")), "别的 bundle 不能当默认 bundle 用");

        // 摘要格式不对（上游 DigestData 是 64 位十六进制）
        let bad = TempDir::new("bad-digest");
        bad.put(&format!("cache/bundles/hashes/{MARKER}"), "not-a-digest\n");
        bad.put(&format!("cache/bundles/data/{DIGEST}/tectonic-format-latex.tex"), "x");
        assert!(!cache_ready(&bad.dir.join("cache/bundles")), "摘要格式不对时必须保守判未就绪");
    }

    #[test]
    fn bundle_failure_text_is_actionable_per_policy() {
        let upstream = "error: this bundle isn't cached, and we couldn't get it from the internet. \
                        Error: index.gz download failed";
        let offline = bundle_failure_message(BundlePolicy::CachedOnly, upstream).expect("命中标记");
        assert!(offline.contains("只用了本地缓存") || offline.contains("离线缓存"), "离线档要说清是 `-C` 下失败：{offline}");
        assert!(
            offline.contains(&format!("{OFFLINE_ENV}=0")),
            "离线档必须给**当前策略下有效**的出路（重编还是离线档）：{offline}"
        );

        let online = bundle_failure_message(BundlePolicy::AllowFetch, upstream).expect("命中标记");
        assert!(online.contains("需要联网获取"), "联网档要指对方向：{online}");
        assert!(online.contains("TECTONIC_CACHE_DIR"), "联网档要给预热缓存的做法：{online}");

        // 兜底 bundle 打不开（lib.rs:342）同样要认
        assert!(bundle_failure_message(BundlePolicy::AllowFetch, "error: could not open default bundle").is_some());

        // 空缓存 + `-C` 的实测原文（格式生成需要的输入文件不在缓存里，见本模块 cache_ready 的证据③）
        let fmt = bundle_failure_message(
            BundlePolicy::CachedOnly,
            "note: using only cached resource files\nnote: generating format \"latex\"\nerror: failed to open input file \"tectonic-format-latex.tex\"",
        )
        .expect("命中标记");
        assert!(fmt.contains(&format!("{OFFLINE_ENV}=0")), "格式文件缺失也要给同一套可执行文案：{fmt}");

        // 半填充缓存 + `-C` 的实测原文（缺的是普通宏包文件，不是格式输入文件）
        let miss = bundle_failure_message(
            BundlePolicy::CachedOnly,
            "error: failed to open input file \"loadhyph-be.tex\"",
        )
        .expect("离线档要认这一类缺文件");
        assert!(miss.contains(&format!("{OFFLINE_ENV}=0")), "缺文件档也要给可执行出路：{miss}");
        // 同一句话出现在**联网档**时不接管：那是 bundle 里真没有这个文件，与缓存无关
        assert!(
            bundle_failure_message(BundlePolicy::AllowFetch, "error: failed to open input file \"loadhyph-be.tex\"")
                .is_none()
        );

        // 没有标记 ⇒ 不接管（保持 .log 判定）
        assert!(bundle_failure_message(BundlePolicy::AllowFetch, "! Undefined control sequence.").is_none());
    }
}
