//! SyncTeX 反向定位的**策略层**（roadmap ⑤/㉒/⑥-P0-3）：GUI 命令面与 headless CLI/MCP 共用一份。
//!
//! 背景（2026-09 实测）：`synctex edit` 在**生成内容**上会返回生成它的中间文件——点 `multifile`
//! 第 3 页目录区得到 `tmp/main.toc:15`，而同一屏往上 50pt 就是 `main.tex:37`。直接把生成文件当
//! 跳转目标，用户/Agent 都会拿到一屏"没人写过"的内容。
//!
//! 策略（与 [`super::classify`] 配合）：
//! 1. 先按给定点定位；命中项目内源码 → 直接返回（`note = None`，正常路径零额外开销）；
//! 2. 命中生成产物/项目外文件 → 在 y 方向按 `[0, ±40, ±80]` **就近探测**，取第一个项目内源码，
//!    并在 `note` 里说明"已回落"；
//! 3. 全落空 → `source = None` + 一句人话（生成文件名 / 项目外文件名 / 同步数据缺失）。
//!
//! 为什么放在 core：这段逻辑此前只在 `src-tauri` 命令面里，headless 侧要么复制一份、要么行为漂移；
//! 它只依赖 core 自己的 [`SyncTexProvider`] 抽象，天然属于这里（ADR-0006：core 无 IO，IO 经 trait）。

use super::classify::{classify_inverse_target, InverseTarget};
use super::model::{SourcePosition, SyncTexError, SyncTexPosition};
use super::provider::SyncTexProvider;
use std::path::{Path, PathBuf};

/// 就近探测的 y 偏移（PDF 点；正数 = 页面下方）。顺序即优先级：越靠前越"近"。
pub const FALLBACK_Y_OFFSETS: [f32; 5] = [0.0, -40.0, 40.0, -80.0, 80.0];

/// 反向定位结果（可跳转源码 + 给用户/Agent 的一句话）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InverseResolution {
    /// 可跳转的源码位置；`None` = 该处没有可打开的源码。
    pub source: Option<SourcePosition>,
    /// 失败原因 / 回落说明；`None` = 正常直连。
    pub note: Option<String>,
}

impl InverseResolution {
    fn without_source(note: impl Into<String>) -> Self {
        Self {
            source: None,
            note: Some(note.into()),
        }
    }
}

/// PDF 位置 → 源码位置，命中生成产物时就近回落（见模块文档）。
///
/// `root` = 项目根（用于分类命中目标）；`pdf` = 项目根的 PDF 副本。
pub async fn resolve_inverse(
    provider: &dyn SyncTexProvider,
    root: &Path,
    pdf: &Path,
    page: u32,
    x: f32,
    y: f32,
) -> InverseResolution {
    let mut first_target: Option<InverseTarget> = None;
    let mut last_err: Option<SyncTexError> = None;

    for (i, dy) in FALLBACK_Y_OFFSETS.iter().enumerate() {
        let pos = SyncTexPosition { page, x, y: y + dy };
        match provider.inverse(&pos, pdf).await {
            Ok(hit) => match classify_inverse_target(root, &hit.file) {
                InverseTarget::Source(file) => {
                    let note = (i > 0).then(|| {
                        format!(
                            "此处是自动生成的内容，已回落到最近的源码（{}:{}{}）",
                            file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                            hit.line,
                            if *dy < 0.0 { "，向上探测" } else { "，向下探测" }
                        )
                    });
                    return InverseResolution {
                        source: Some(SourcePosition {
                            file,
                            line: hit.line,
                            column: hit.column,
                        }),
                        note,
                    };
                }
                other => {
                    // 记录**第一次**命中的非源码目标（原点那次最有信息量，用于提示文案）
                    if first_target.is_none() {
                        first_target = Some(other);
                    }
                }
            },
            Err(e) => {
                // **数据级失败就不必再试位置候选**（roadmap ㊱，2026-09-17）：`Unavailable`（没编译过）
                // 与 `Busy`（正被重写）都是"整份同步数据用不了"—— 换 y 偏移不会变好，继续循环只会把
                // provider 的退避**乘上候选数**（实测 5 × 625 ms ≈ **3.13 s**）。位置级失败（`Parse`：
                // 这个点上没有映射）才继续回落 —— 那正是这几档偏移的用途。
                let data_level = matches!(e, SyncTexError::Unavailable(_) | SyncTexError::Busy(_));
                if last_err.is_none() {
                    last_err = Some(e);
                }
                if data_level {
                    break;
                }
            }
        }
    }

    match first_target {
        Some(InverseTarget::Generated(f)) => InverseResolution::without_source(format!(
            "此处来自自动生成的文件 {}（目录/参考文献/索引等由 LaTeX 生成），没有对应的源码行",
            base_name(&f)
        )),
        Some(InverseTarget::OutsideProject(f)) => InverseResolution::without_source(format!(
            "此处来自项目外的文件 {}（系统宏包/文档类），无法在编辑器里打开",
            base_name(&f)
        )),
        // 一次都没拿到映射：多半是"还没编译过"或同步数据正被重写
        _ => InverseResolution::without_source(match last_err {
            Some(e) => format!("同步失败：{e}"),
            None => "此处没有对应的源码位置".to_string(),
        }),
    }
}

fn base_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// 根文件对应的 PDF 路径（项目根下的同名副本，见 design.md 产物位置）。
///
/// 根文件缺失时退化为 `main.pdf`（与调度的"未确定根文件不编译"配合，不会真的用到）。
pub fn pdf_path_for_root(root: &Path, root_file: Option<&Path>) -> PathBuf {
    let stem = root_file
        .and_then(|f| f.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".into());
    root.join(format!("{stem}.pdf"))
}

/// `.synctex.gz` 所在路径（latexmk `-outdir=tmp`，见 infra runner）。
pub fn synctex_data_path(root: &Path, root_file: Option<&Path>) -> PathBuf {
    let stem = root_file
        .and_then(|f| f.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".into());
    root.join("tmp").join(format!("{stem}.synctex.gz"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synctex::SyncTexProvider;
    use async_trait::async_trait;
    use std::sync::Mutex;

    /// 按 y 值回答的假 provider：`[0]=Err`, 其余 y → 指定文件/行。
    struct FakeProvider {
        /// (y 偏移 → (文件, 行))；用 `None` 表示该点报错
        answers: Mutex<Vec<(f32, Option<(String, u32)>)>>,
        calls: Mutex<Vec<f32>>,
    }

    impl FakeProvider {
        fn new(base_y: f32, answers: Vec<(f32, Option<(&str, u32)>)>) -> Self {
            Self {
                answers: Mutex::new(
                    answers
                        .into_iter()
                        .map(|(dy, v)| (base_y + dy, v.map(|(f, l)| (f.to_string(), l))))
                        .collect(),
                ),
                calls: Mutex::new(Vec::new()),
            }
        }
        fn calls(&self) -> Vec<f32> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SyncTexProvider for FakeProvider {
        async fn forward(&self, _src: &SourcePosition, _pdf: &Path) -> Result<SyncTexPosition, SyncTexError> {
            unreachable!()
        }
        async fn inverse(&self, pos: &SyncTexPosition, _pdf: &Path) -> Result<SourcePosition, SyncTexError> {
            self.calls.lock().unwrap().push(pos.y);
            let answers = self.answers.lock().unwrap();
            match answers.iter().find(|(y, _)| (*y - pos.y).abs() < 0.01) {
                Some((_, Some((file, line)))) => Ok(SourcePosition {
                    file: PathBuf::from(file),
                    line: *line,
                    column: -1,
                }),
                _ => Err(SyncTexError::Io("文件被占用".into())),
            }
        }
    }

    const ROOT: &str = "/proj";
    const PDF: &str = "/proj/main.pdf";

    /// 固定返回同一种错误的假 provider（用来量"候选回落循环跑了几次"，roadmap ㊱）。
    struct FailingProvider {
        err: SyncTexError,
        calls: Mutex<u32>,
    }

    #[async_trait]
    impl SyncTexProvider for FailingProvider {
        async fn forward(&self, _src: &SourcePosition, _pdf: &Path) -> Result<SyncTexPosition, SyncTexError> {
            unreachable!()
        }
        async fn inverse(&self, _pos: &SyncTexPosition, _pdf: &Path) -> Result<SourcePosition, SyncTexError> {
            *self.calls.lock().unwrap() += 1;
            Err(self.err.clone())
        }
    }

    /// **数据级失败**（同步数据不存在 / 正被重写）⇒ 立刻放弃位置回落，只查一次。
    #[tokio::test]
    async fn data_level_failure_stops_the_fallback_loop() {
        for err in [
            SyncTexError::Unavailable("还没编译过".into()),
            SyncTexError::Busy("正被重写".into()),
        ] {
            let want = err.to_string();
            let p = FailingProvider { err, calls: Mutex::new(0) };
            let r = resolve_inverse(&p, Path::new(ROOT), Path::new(PDF), 3, 100.0, 600.0).await;
            assert!(r.source.is_none());
            assert_eq!(*p.calls.lock().unwrap(), 1, "数据级失败不该把 5 个 y 候选都试一遍");
            assert!(r.note.unwrap().contains(&want), "提示要带原始原因");
        }
    }

    /// **位置级失败**（这个点上没有映射）⇒ 5 档偏移照旧全试（㉒ 的回落行为不能回归）。
    #[tokio::test]
    async fn parse_miss_keeps_trying_all_candidates() {
        let p = FailingProvider {
            err: SyncTexError::Parse("此处没有可用的源码映射".into()),
            calls: Mutex::new(0),
        };
        let r = resolve_inverse(&p, Path::new(ROOT), Path::new(PDF), 3, 100.0, 600.0).await;
        assert!(r.source.is_none());
        assert_eq!(
            *p.calls.lock().unwrap() as usize,
            FALLBACK_Y_OFFSETS.len(),
            "位置级失败要继续回落（候选表有几档就试几档）"
        );
    }

    #[tokio::test]
    async fn direct_hit_has_no_note_and_single_call() {
        let p = FakeProvider::new(600.0, vec![(0.0, Some(("/proj/chapters/a.tex", 12)))]);
        let r = resolve_inverse(&p, Path::new(ROOT), Path::new(PDF), 3, 100.0, 600.0).await;
        assert_eq!(r.source.unwrap().line, 12);
        assert!(r.note.is_none(), "正常命中不该有提示");
        assert_eq!(p.calls().len(), 1, "正常路径只查一次");
    }

    #[tokio::test]
    async fn generated_hit_falls_back_to_nearest_source() {
        // 原点命中 tmp/main.toc，向上 40pt 命中 main.tex（复刻 multifile 目录区的实测形态）
        let p = FakeProvider::new(
            600.0,
            vec![
                (0.0, Some(("/proj/tmp/main.toc", 15))),
                (-40.0, Some(("/proj/main.tex", 37))),
            ],
        );
        let r = resolve_inverse(&p, Path::new(ROOT), Path::new(PDF), 3, 100.0, 600.0).await;
        let src = r.source.expect("应回落到源码");
        assert_eq!(src.file, PathBuf::from("/proj/main.tex"));
        assert_eq!(src.line, 37);
        assert!(r.note.unwrap().contains("已回落"));
    }

    #[tokio::test]
    async fn generated_only_reports_which_file_instead_of_jumping() {
        // 五个探测点全落在生成文件上 → 不给源码，只在提示里点名文件
        let p = FakeProvider::new(
            600.0,
            FALLBACK_Y_OFFSETS
                .iter()
                .map(|dy| (*dy, Some(("/proj/tmp/main.toc", 15))))
                .collect(),
        );
        let r = resolve_inverse(&p, Path::new(ROOT), Path::new(PDF), 3, 100.0, 600.0).await;
        assert!(r.source.is_none(), "生成文件不该被当源码返回");
        let note = r.note.unwrap();
        assert!(note.contains("main.toc"), "{note}");
        assert!(note.contains("自动生成"), "{note}");
    }

    #[tokio::test]
    async fn outside_project_is_reported_not_opened() {
        let p = FakeProvider::new(600.0, vec![(0.0, Some(("C:/texlive/base/article.cls", 9)))]);
        let r = resolve_inverse(&p, Path::new(ROOT), Path::new(PDF), 1, 10.0, 600.0).await;
        assert!(r.source.is_none());
        assert!(r.note.unwrap().contains("article.cls"));
    }

    #[tokio::test]
    async fn all_calls_failing_reports_sync_error() {
        let p = FakeProvider::new(600.0, vec![]); // 所有点都 Err
        let r = resolve_inverse(&p, Path::new(ROOT), Path::new(PDF), 1, 10.0, 600.0).await;
        assert!(r.source.is_none());
        assert!(r.note.unwrap().contains("同步失败"));
        assert_eq!(p.calls().len(), FALLBACK_Y_OFFSETS.len(), "失败也会按偏移扫一遍");
    }

    #[test]
    fn pdf_and_synctex_paths_follow_root_stem() {
        let root = Path::new("/proj");
        assert_eq!(
            pdf_path_for_root(root, Some(Path::new("/proj/css/thesis.tex"))),
            PathBuf::from("/proj/thesis.pdf")
        );
        assert_eq!(
            synctex_data_path(root, Some(Path::new("/proj/css/thesis.tex"))),
            PathBuf::from("/proj/tmp/thesis.synctex.gz")
        );
        // 嵌套根文件只取 stem（与 runner 的 tmp/<stem>.pdf 约定一致）
        assert_eq!(pdf_path_for_root(root, None), PathBuf::from("/proj/main.pdf"));
    }
}
