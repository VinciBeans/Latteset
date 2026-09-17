//! 外部可执行文件的**可用性探测**（roadmap ㊻：引擎清单要如实说"这台机器上能不能用"）。
//!
//! 只做 PATH 上的**文件存在性**查找，**不启动任何进程**：
//! - 便宜（一次面板打开只走几趟目录查询，不是几次进程启动）；
//! - 无副作用（不会像 `--version` 那样在坏安装上卡住、也不会弹任何窗口）；
//! - 只需回答"有没有"，能不能跑起来由编译时的错误来回答（那条路已有可行动文案）。
//!
//! 宁可**漏判也不误判**：找不到 ⇒ 报"没有"（用户看到的是"装好 TeX Live 后重启应用"，
//! 而真实原因也确实是没装/不在 PATH 上）。反过来的误判（说"有"却跑不起来）才会浪费时间。

use std::path::{Path, PathBuf};

/// `PATH` 上找可执行文件（Windows 上还会试 `.exe`）。
///
/// 不做 `PATHEXT` 全展开：我们找的都是原生 exe 或 TeX Live 的 exe 包装器（`latexmk.exe`）。
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if let Some(hit) = find_in_dir(&dir, name) {
            return Some(hit);
        }
    }
    None
}

/// 在单个目录里找一个可执行文件（拆出来便于单测，不依赖真实 PATH）。
fn find_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    if dir.as_os_str().is_empty() {
        return None;
    }
    let bare = dir.join(name);
    if is_file(&bare) {
        return Some(bare);
    }
    #[cfg(windows)]
    if !name.to_ascii_lowercase().ends_with(".exe") {
        let exe = dir.join(format!("{name}.exe"));
        if is_file(&exe) {
            return Some(exe);
        }
    }
    None
}

fn is_file(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)
}

/// `LATTESET_TEX_ENGINES` 的原始值（清单覆盖，见 `core::engine`）。
///
/// 读环境变量算"外部依赖"，按 ADR-0010 落在 infra：上层只拿字符串，不直接碰 `std::env`。
pub fn env_tex_engines() -> Option<String> {
    std::env::var(latteset_core::engine::ENGINES_ENV).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 目录里没有就是没有（含空目录、不存在的目录）。
    #[test]
    fn find_in_dir_misses_cleanly() {
        let tmp = std::env::temp_dir().join("latteset-probe-none");
        let _ = std::fs::create_dir_all(&tmp);
        assert_eq!(find_in_dir(&tmp, "definitely-not-here-xyz"), None);
        assert_eq!(find_in_dir(Path::new(""), "anything"), None);
        assert_eq!(
            find_in_dir(&tmp.join("no-such-dir-at-all"), "anything"),
            None
        );
    }

    /// 找得到真实文件；`.exe` 后缀在 Windows 上要自动补。
    #[cfg(windows)]
    #[test]
    fn find_in_dir_finds_exe_with_and_without_suffix() {
        let dir = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .map(|p| p.join("System32"))
            .expect("Windows 上 SystemRoot 必须存在");
        assert!(find_in_dir(&dir, "cmd").is_some(), "cmd.exe 该能找到");
        assert!(find_in_dir(&dir, "cmd.exe").is_some(), "带后缀也要能找到");
    }

    /// 真机探针：本机装了 TeX Live，`latexmk` 应当在 PATH 上（不在则跳过，不误报失败）。
    #[test]
    fn latexmk_is_findable_on_a_texlive_machine() {
        match find_in_path("latexmk") {
            Some(p) => assert!(p.is_file(), "命中的路径得是真文件：{}", p.display()),
            None => eprintln!("本机 PATH 上没有 latexmk —— 跳过（这条用例不假设环境）"),
        }
    }

    /// 探针不启动进程：拿一个**必然存在但不可执行**的东西也要报"在"（纯文件存在性）。
    #[test]
    fn probe_is_purely_file_existence() {
        let tmp = std::env::temp_dir().join("latteset-probe-exists");
        let _ = std::fs::create_dir_all(&tmp);
        let fake = tmp.join("not-really-a-program");
        std::fs::write(&fake, b"x").unwrap();
        assert_eq!(find_in_dir(&tmp, "not-really-a-program"), Some(fake.clone()));
        let _ = std::fs::remove_file(&fake);
    }
}
