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
//! 只在 `-linux-` 上补 `icudata`）。放在这里也就不影响 `latteset-infra` 等默认构建成员。
//!
//! kernel32 已由 Rust 默认链接行提供（`kernel32.lib` + `ntdll/userenv/ws2_32/dbghelp`），
//! 其余 8 个逐条发出；重复发同一条库是无害的，缺失则会 LNK2019。

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

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
