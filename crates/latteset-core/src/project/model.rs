//! 项目状态类型（modules.md §3.1）。

use std::path::PathBuf;

/// 当前打开项目的状态。存放在 src-tauri 的组合层（modules.md §7），
/// core 内只作为纯函数的输入参数传递。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectState {
    /// 项目根目录（打开文件夹即项目）。
    pub root: PathBuf,
    /// 当前根文件（探测结果或手动覆盖）。
    pub root_file: Option<PathBuf>,
}

/// 根文件候选（含 `\documentclass` 且未被引用的 .tex）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootCandidate {
    pub path: PathBuf,
}

/// 探测结果（modules.md §5.4）：
/// 唯一 → 自动采用；多候选 → 前端弹窗；零候选 → 提示手动指定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootResolution {
    Unique(PathBuf),
    Multiple(Vec<PathBuf>),
    None,
}

impl RootResolution {
    /// 候选列表（升序，与命令面 `ProjectInfo::root_candidates` 语义一致）：
    /// `Unique` → 单元素；`Multiple` → 全部；`None` → 空。
    ///
    /// 存在的意义：`Multiple` 的候选此前在命令层被丢弃（roadmap P0-②-1 的根因），
    /// 收敛到本方法后「解析结果 → 契约字段」的映射只有一处、可单测。
    pub fn candidates(&self) -> Vec<PathBuf> {
        match self {
            RootResolution::Unique(p) => vec![p.clone()],
            RootResolution::Multiple(list) => list.clone(),
            RootResolution::None => Vec::new(),
        }
    }

    /// 探测出的唯一根文件（`Multiple`/`None` 无唯一解 → None）。
    pub fn unique(&self) -> Option<PathBuf> {
        match self {
            RootResolution::Unique(p) => Some(p.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn candidates_maps_each_variant() {
        // roadmap P0-②-1：契约字段 root_candidates 的唯一映射来源
        assert_eq!(
            RootResolution::Unique(p("proj/main.tex")).candidates(),
            vec![p("proj/main.tex")]
        );
        assert_eq!(
            RootResolution::Multiple(vec![p("proj/a.tex"), p("proj/b.tex")]).candidates(),
            vec![p("proj/a.tex"), p("proj/b.tex")]
        );
        assert_eq!(RootResolution::None.candidates(), Vec::<PathBuf>::new());
    }

    #[test]
    fn unique_only_for_unique_variant() {
        assert_eq!(
            RootResolution::Unique(p("proj/main.tex")).unique(),
            Some(p("proj/main.tex"))
        );
        // 多候选/零候选都无唯一解 → 必须为 None（由用户在前端选择）
        assert_eq!(RootResolution::Multiple(vec![p("a.tex")]).unique(), None);
        assert_eq!(RootResolution::None.unique(), None);
    }

    #[test]
    fn candidates_are_stable_for_chinese_paths() {
        let list = vec![p("E:/项目/章节/第一章.tex"), p("E:/项目/中文主文件.tex")];
        assert_eq!(RootResolution::Multiple(list.clone()).candidates(), list);
    }
}
