//! 链接期补丁：把 ICU 静态库所需的 Windows 系统库接上。
//!
//! 症状（可复现）：`cargo check` 通过，但 `cargo test` / `cargo build --examples` 在链接期失败：
//!
//! ```text
//! icuuc.lib(wintz.ao) : error LNK2019: 无法解析的外部符号 __imp_RegCloseKey
//!     … __imp_RegEnumKeyExW / __imp_RegOpenKeyExW / __imp_RegQueryInfoKeyW / __imp_RegQueryValueExW
//! error: linking with `link.exe` failed: exit code 1120
//! ```
//!
//! 原因：vcpkg 的 `icu` 端口是**静态**库（triplet `x64-windows-static-release`），静态 ICU 在
//! Windows/MSVC 下要依赖 C++ 的隐式系统库，vcpkg 把它写进了 `lib/pkgconfig/icu-uc.pc` 的
//! `baselibs`（来自 `ports/icu/portfile.cmake:145-155` 注入的 `CXX_IMPLICIT_LINK_LIBRARIES`）：
//!
//! ```text
//! baselibs = -lkernel32 -luser32 -lgdi32 -lwinspool -lshell32 -lole32 -loleaut32 -lcomdlg32 -ladvapi32
//! ```
//!
//! 而 Rust 走的是 vcpkg **动态探测**路径（`tectonic_dep_support` + `vcpkg-rs`），只发 ICU 自己的
//! `icuuc/icuin/icudt/icuio`，**不会**发这些系统库；`cargo:rustc-link-lib` 也不会沿着
//! `icuuc.lib` 的 `#pragma comment` 传递（静态库没有这条链）。故由本 crate 补发。
//!
//! 为什么落在本 crate：`crates/latteset-tectonic` 是 C 链的**唯一落点**（ADR-0012 / 方案 §4.2 的
//! X-5），上游 bridge crate 的 build script 明确不管这件事（`tectonic_bridge_icu-0.2.4/build.rs:44-52`
//! 只在 `-linux-` 上补 `icudata`）。
//!
//! kernel32 已由 Rust 默认链接行提供（`kernel32.lib` + `ntdll/userenv/ws2_32/dbghelp`），
//! 其余 8 个逐条发出；重复发同一条库是无害的，缺失则会 LNK2019。
//!
//! **Windows 前置自检（阶段② §3.2 第 4 条，尽力提示）**：自阶段②起所有构建都需要 vcpkg
//! （ADR-0015 决策 2）。缺 `VCPKG_ROOT`/`VCPKGRS_TRIPLET` 时，上游 vcpkg-rs 的报错**不指向环境**
//! （实测：会炸成 "you need to install a pkg-config package for your OS" 之类），所以在链接补丁之前
//! 先给一句可读的 panic + 可复制的修复命令。build script 执行顺序不保证 ⇒ 这只是尽力提示，
//! 不是保证（真正缺库时上游仍会报错）。

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    check_vcpkg_env();

    for lib in [
        "advapi32", // 注册表（__imp_RegCloseKey 等就缺在这里）
        "user32",   // 窗口/消息
        "gdi32",    // GDI
        "winspool", // 打印子系统
        "shell32",  // Shell API
        "ole32",    // COM
        "oleaut32", // COM 自动化
        "comdlg32", // 通用对话框
    ] {
        println!("cargo:rustc-link-lib={lib}");
    }
}

/// Windows 下 vcpkg 前置缺失 → **可读 panic**（含可复制命令），而不是让上游 build script 炸一句
/// 与环境无关的报错。显式选择 system 后端（`TECTONIC_DEP_BACKEND=system`）时不拦。
fn check_vcpkg_env() {
    let backend = std::env::var("TECTONIC_DEP_BACKEND").unwrap_or_default();
    if backend.eq_ignore_ascii_case("system") {
        return;
    }
    let missing: Vec<&str> =
        ["VCPKG_ROOT", "VCPKGRS_TRIPLET"].into_iter().filter(|k| std::env::var_os(k).is_none()).collect();
    if missing.is_empty() {
        return;
    }
    panic!(
        "\n\
         latteset-tectonic 构建前置缺失：Windows 下内嵌 Tectonic 必须走 vcpkg（ADR-0015 决策 2），\n\
         但环境里没有：{}\n\
         \n\
         怎么修（复制即可跑，任选其一）：\n\
         ① 只自检并打印环境（不编译）：\n\
            pwsh -NoProfile -File scripts/with-tectonic-lib.ps1 -Mode check\n\
         ② dot-source 同一脚本后再跑任意 cargo 命令（本 shell 内生效）：\n\
            . ./scripts/with-tectonic-lib.ps1; cargo check -p latteset-tectonic\n\
         ③ CI 见 .github/workflows/ci.yml 的 vcpkg setup 步骤（clone pin rev → bootstrap → install 六个端口）。\n\
         \n\
         说明：本检查只是尽力提示（build script 执行顺序不保证）；真正的 C 链来自 vcpkg triplet \n\
         `x64-windows-static-release` 的六个端口：freetype harfbuzz graphite2 icu fontconfig libpng。\n",
        missing.join(", ")
    );
}
