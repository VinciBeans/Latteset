//! 公式预览的片段落点（roadmap ㊸ 切片 2）：把「独立项目根 + 片段文档」准备好。
//!
//! 为什么在 infra：建目录/写文件是文件系统操作，按 ADR-0010 只有 infra 可以碰 `std::fs`
//! （core 是纯逻辑、src-tauri 与 latteset-server 只经注入的 `FileSystem`）。
//!
//! **隔离契约**（切片 2 的硬要求，别在调用方各写一套）：
//! - 落点固定在 `<项目>/tmp/snippet/<键>/` —— `tmp/` 已在 watch 与文件树忽略清单里（不触发编译）；
//! - **不碰**权威 `<stem>.pdf` 与主编译的 `tmp/main.*`（片段自带独立的 `project_root`）；
//! - **目录已存在就原地复用**：复用的目录才能吃到 runner 已有的 **A 闸门**（页哈希相同 ⇒ 跳过
//!   XDV→PDF 转换）与热 aux —— 实测同一公式二次悬停 89 ms / 350 ms，而清空重建要 177 / 430 ms；
//! - 目录名 = [`latteset_core::snippet::snippet_key`]（`导言区 + 公式` 的短哈希）⇒ 换公式或改导言区
//!   都会换目录，不会把别的公式的产物当缓存复用。

use std::path::{Path, PathBuf};

/// 片段落点的相对目录（与 GUI 的中间产物约定一致：都住在 `tmp/` 下）。
pub const SNIPPET_DIR: &str = "tmp/snippet";

/// 准备 `<project_root>/tmp/snippet/<key>/main.tex`，返回该目录。
///
/// 已存在则**复用**（只重写 `main.tex`），因为 A 闸门与热 aux 都挂在目录的持久状态上。
pub fn prepare_snippet_dir(project_root: &Path, key: &str, document: &str) -> std::io::Result<PathBuf> {
    let dir = project_root.join(SNIPPET_DIR).join(key);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("main.tex"), document)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 片段目录按公式键分家且可复用() {
        let root = std::env::temp_dir().join("latteset-infra-snippet-lab");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建临时项目根");

        let a = prepare_snippet_dir(&root, "aaa111", "\\documentclass{article}\n").expect("准备 A");
        assert_eq!(a, root.join("tmp/snippet/aaa111"));
        assert!(a.join("main.tex").is_file());
        // 复用：同一个键第二次调用不该清掉目录（清掉就吃不到 A 闸门与热 aux）
        std::fs::write(a.join("marker"), b"keep").expect("放个标记");
        let a2 = prepare_snippet_dir(&root, "aaa111", "\\documentclass{article}\n% v2\n").expect("再准备 A");
        assert_eq!(a2, a);
        assert!(a.join("marker").is_file(), "复用不得清空目录");
        assert!(std::fs::read_to_string(a.join("main.tex")).expect("读文档").contains("% v2"), "main.tex 要重写成最新内容");
        // 不同键 ⇒ 不同目录
        let b = prepare_snippet_dir(&root, "bbb222", "x").expect("准备 B");
        assert_ne!(a, b);
        let _ = std::fs::remove_dir_all(&root);
    }
}
