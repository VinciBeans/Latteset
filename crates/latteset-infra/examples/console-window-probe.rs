//! 控制台窗口探测（Windows 专项）：**同一个父进程**分别用"裸 spawn"与 [`latteset_infra::proc`]
//! 的 spawn 起一个控制台子程序，看**有没有多出一个控制台窗口**。
//!
//! 为什么需要它：GUI 是 `windows_subsystem = "windows"`（没有控制台），此时子进程默认会**新建**一个
//! 控制台窗口 —— 用户在 release 档看到的"每次编译弹一个黑框"就是这个。本文件用
//! `#![windows_subsystem = "windows"]` **模拟 GUI 父进程**，于是 debug 档也能复现与验证，
//! 不必先出一个 release 包。
//!
//! 用法（从**有控制台**的 pwsh 里跑，计数器在外面数）：
//!   cargo run -q -p latteset-infra --example console-window-probe -- plain   # 期望：多一个控制台
//!   cargo run -q -p latteset-infra --example console-window-probe -- quiet   # 期望：不新增
//!
//! 判定不看本程序的输出（windows 子系统没有 stdout），而看外部计数：
//! `(Get-Process conhost,WindowsTerminal -ErrorAction SilentlyContinue).Count`。
#![windows_subsystem = "windows"]

use std::time::Duration;

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "quiet-std".into());
    // 子程序自己活 3 秒，给外面的计数器留出观察窗口（用系统自带的 ping 当"睡 3 秒"的载体）。
    let spawn = |cmd: &mut std::process::Command| {
        cmd.args(["/c", "ping", "-n", "4", "127.0.0.1"]);
        cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        let _ = cmd.spawn().expect("起子进程失败");
    };
    match mode.as_str() {
        // 对照档：**什么都不起** —— 用来量"背景噪声"（本机时不时会有别的进程建控制台）
        "none" => {}
        // 裸 spawn：**故意**不加 CREATE_NO_WINDOW —— 这是修复前的行为
        "plain" => spawn(&mut std::process::Command::new("cmd")),
        // 生产路径之一：`proc::command_std`（同步；`taskkill` 走这条）
        "quiet-tokio" => {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            rt.block_on(async {
                let mut c = latteset_infra::proc::command("cmd");
                c.args(["/c", "ping", "-n", "4", "127.0.0.1"]);
                c.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
                let _ = c.spawn().expect("起子进程失败");
                tokio::time::sleep(Duration::from_millis(2500)).await;
            });
            return;
        }
        // 生产路径之二：`proc::command`（异步；引擎 / latexmk / xdvipdfmx / synctex 走这条）
        _ => spawn(&mut latteset_infra::proc::command_std("cmd")),
    }
    std::thread::sleep(Duration::from_millis(2500));
}
