//! 子进程的**窗口行为**（Windows 专项，2026-09-16）。
//!
//! **为什么要集中在这里**：GUI 是 `windows_subsystem = "windows"` —— 它**没有控制台**，而 Windows
//! 默认会给"控制台子程序"**新建一个**控制台窗口。表现就是**每次编译弹一个黑框**。
//! debug 档看不见：那时父进程本身是控制台程序（`npm run tauri dev` 里带的那个），子进程直接继承它
//! ⇒ 这个 bug **只在 release 档出现**（用户实测报的就是这个）。
//!
//! 修法是给每个子进程加 `CREATE_NO_WINDOW`。它只影响"要不要分配控制台"，**不影响 I/O**：
//! stdout/stderr 仍走我们建的管道 ⇒ 编译日志、退出码、页哈希、树杀一切不变。
//!
//! **所有生产路径都必须经 [`command`] / [`command_std`] 起进程**（别在别处直接 `Command::new`，
//! 漏一处就漏一个黑框）。非 Windows 上是纯转发，没有额外行为。

/// `CREATE_NO_WINDOW`（winbase.h）：不为子进程创建控制台窗口。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 起一个**不弹控制台**的子进程（异步路径：引擎 / latexmk / xdvipdfmx / synctex）。
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(program);
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// 同上，同步路径（`taskkill` 这类"发一条命令就返回"的调用）。
pub fn command_std(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}
