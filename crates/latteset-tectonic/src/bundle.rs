//! bundle 源解析与分类失败文案（方案 §3.4 第 2 步、§5.6 的 LB-1..LB-5）。
//!
//! 上游口径：`tectonic_bundles::detect_bundle(source, only_cached, custom_cache_dir)`
//! （`crates/bundles/src/lib.rs:234-288`，CLI 用同一个函数 `src/bin/tectonic/compile.rs:193`）。
//! 三条实测坑（t5 §3.2，由本模块在调用**之前**补上分类文案）：
//! 1. **绝对 Windows 路径会被当成 URL scheme**：`E:\...` 判不出来 ⇒ 必须 `file:///E:/...`
//!    或**相对路径**（`docs/tectonic-library-plan.md` §5.6 LB-4）；
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

/// 分类失败文案（三类，方案 §5.6 LB-5 / §9 F2b/F2c）。
fn classify(source: &str) -> Option<String> {
    // 网络源 + 本构建没开 `network-bundle` ⇒ 必然失败，且上游的报错不会点出"特性没开"。
    // 这类必须先说清楚，否则用户看到的是"bundle 打开失败"而不知道要配本地 bundle。
    #[cfg(not(feature = "network-bundle"))]
    if source.starts_with("https://") || source.starts_with("http://") {
        return Some(format!(
            "bundle 源 `{source}` 是网络地址，但本构建**未启用 `network-bundle` 特性**（方案 §4.2 X-2 选 (a)：\
             默认关掉 HTTP）⇒ 请改用本地目录 bundle（`{}=<相对路径 或 file:///绝对路径>`，目录需自带 SHA256SUM），\
             或按需重新构建时打开该特性",
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

/// 打开 bundle。返回 `Ok(None)` = 明确不要 bundle；`Err(msg)` = 可读失败文案。
pub fn open_bundle(
    source: &BundleSource,
    only_cached: bool,
    cache_dir: Option<PathBuf>,
) -> Result<Option<Box<dyn Bundle>>, String> {
    let src = match source {
        BundleSource::None => return Ok(None),
        BundleSource::Default => default_bundle_url(),
        BundleSource::Explicit(s) => s.clone(),
    };
    if let Some(msg) = classify(&src) {
        return Err(msg);
    }
    match detect_bundle(src.clone(), only_cached, cache_dir) {
        Ok(Some(b)) => Ok(Some(b)),
        Ok(None) => Err(format!(
            "bundle 源无法识别：`{src}`（上游 `detect_bundle` 返回 None）。\
             本地目录/文件请写成 `file:///…` 或相对路径；网络 bundle 需要 `network-bundle` 特性"
        )),
        Err(e) => Err(format!("bundle 打开失败（{src}）：{e:#}")),
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
}

