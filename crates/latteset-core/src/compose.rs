//! 组合层翻译（modules.md §7 / 设计决策 D3）。
//!
//! 文件事件 → [`CompileRequest`] 的纯逻辑部分：scheduler 只认识请求，
//! 不认识文件/项目/设置。src-tauri 的 watch 调用本函数后把请求发给调度器。
//! 放在 core 里是为了可测（信息局部性：每次构造都取快照拷贝，不持有引用）。

use crate::project::{is_compile_trigger, ProjectState};
use crate::settings::Settings;
use crate::types::{CompileKind, CompileRequest};
use std::path::Path;
use std::time::Duration;
use tracing::debug;

/// 翻译所需的上下文快照（由 src-tauri 组合层从状态中取）。
#[derive(Clone, Copy)]
pub struct ComposeContext<'a> {
    pub project: &'a ProjectState,
    pub settings: &'a Settings,
}

/// 文件变化 → 编译请求；不满足触发条件返回 None。
///
/// 触发条件：
/// - 已确定根文件；
/// - 变化路径在项目根内；
/// - 是**触发路径**（roadmap ㉜：项目内任何非忽略文件 —— `.bib`/图片/`.cls`/`.sty` 都算输入；
///   排除 tmp/、隐藏项，以及**本次编译自己写在项目根的产物**，后者不排除会自激）。
///
/// **强度 = Quick**（roadmap ㉘）：编辑期只要快速出图，引用/目录可能落后一趟；
/// 由「空闲收敛」（前端在停手后调 `compile_request_manual`）与「首编」兜底正确性。
/// 若项目尚无构建产物，runner 会把 Quick 自动升级为 Full（见 infra::runner）。
///
/// **决策日志**（roadmap ㉛）：不触发时把**原因**打出来（哪个条件没过），
/// 否则用户只能看到"改了没反应"。
pub fn compile_request_for_change(ctx: ComposeContext<'_>, changed: &Path) -> Option<CompileRequest> {
    let Some(root_file) = ctx.project.root_file.as_ref() else {
        debug!(changed = %changed.display(), "不触发编译：尚未确定根文件（root_file 为空）");
        return None;
    };
    if !changed.starts_with(&ctx.project.root) {
        debug!(changed = %changed.display(), "不触发编译：变化路径在项目根之外");
        return None;
    }
    if !is_compile_trigger(changed, &ctx.project.root, ctx.project.root_file.as_deref()) {
        debug!(
            changed = %changed.display(),
            "不触发编译：路径被排除（tmp/、隐藏项、无扩展名的目录/文件，或本次编译自己在项目根的产物如 <stem>.pdf）"
        );
        return None;
    }
    debug!(
        changed = %changed.display(),
        root = %root_file.display(),
        "触发编译（编辑触发 = Quick 单趟）"
    );
    Some(CompileRequest {
        root_file: root_file.clone(),
        project_root: ctx.project.root.clone(),
        engine: ctx.settings.compile.engine,
        timeout: Duration::from_secs(u64::from(ctx.settings.compile.timeout_secs)),
        kind: CompileKind::Quick,
    })
}

/// 手动编译请求（compile_now 命令）：与文件变化同路径，只是不校验 changed。
///
/// **强度 = Full**：用户主动点「编译」要的是正确结果；前端「空闲收敛」也复用它
/// （多趟 + bibtex/biber/索引），这是 ㉘ 正确性的兜底。
pub fn compile_request_manual(ctx: ComposeContext<'_>) -> Option<CompileRequest> {
    let root_file = ctx.project.root_file.as_ref()?;
    Some(CompileRequest {
        root_file: root_file.clone(),
        project_root: ctx.project.root.clone(),
        engine: ctx.settings.compile.engine,
        timeout: Duration::from_secs(u64::from(ctx.settings.compile.timeout_secs)),
        kind: CompileKind::Full,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectState;
    use crate::settings::{CompileSettings, SCHEMA_VERSION};
    use crate::types::{CompileMode, Engine};
    use std::path::PathBuf;

    fn ctx<'a>(project: &'a ProjectState, settings: &'a Settings) -> ComposeContext<'a> {
        ComposeContext { project, settings }
    }

    fn settings() -> Settings {
        Settings {
            schema_version: SCHEMA_VERSION,
            compile: CompileSettings {
                mode: CompileMode::Continuous,
                debounce_ms: 500,
                timeout_secs: 120,
                engine: Engine::XeLaTeX,
            },
            tectonic: Default::default(),
            ui: Default::default(),
            root_file: None,
        }
    }

    #[test]
    fn no_root_file_returns_none() {
        let project = ProjectState {
            root: PathBuf::from("proj"),
            root_file: None,
        };
        assert_eq!(
            compile_request_for_change(ctx(&project, &settings()), Path::new("proj/main.tex")),
            None
        );
    }

    /// **roadmap ㉜**：非 `.tex` 的输入也要触发（过去只认 `.tex` ⇒ 改 `.bib` 没反应）。
    #[test]
    fn non_tex_inputs_do_trigger() {
        let project = ProjectState {
            root: PathBuf::from("proj"),
            root_file: Some(PathBuf::from("proj/main.tex")),
        };
        for p in ["proj/refs.bib", "proj/figures/plot.png", "proj/custom.sty"] {
            assert!(
                compile_request_for_change(ctx(&project, &settings()), Path::new(p)).is_some(),
                "{p} 必须触发编译"
            );
        }
    }

    /// **防自激**：本次编译写在项目根的 `<stem>.pdf`（与原子替换的 `.tmp`）不得触发编译。
    #[test]
    fn own_root_pdf_does_not_trigger() {
        let project = ProjectState {
            root: PathBuf::from("proj"),
            root_file: Some(PathBuf::from("proj/main.tex")),
        };
        for p in ["proj/main.pdf", "proj/main.pdf.tmp"] {
            assert_eq!(
                compile_request_for_change(ctx(&project, &settings()), Path::new(p)),
                None,
                "{p} 是自己的产物，不能触发（否则 编译→写 PDF→再编译）"
            );
        }
    }

    #[test]
    fn tmp_change_returns_none() {
        let project = ProjectState {
            root: PathBuf::from("proj"),
            root_file: Some(PathBuf::from("proj/main.tex")),
        };
        assert_eq!(
            compile_request_for_change(ctx(&project, &settings()), Path::new("proj/tmp/main.tex")),
            None
        );
    }

    #[test]
    fn outside_project_returns_none() {
        let project = ProjectState {
            root: PathBuf::from("proj"),
            root_file: Some(PathBuf::from("proj/main.tex")),
        };
        assert_eq!(
            compile_request_for_change(ctx(&project, &settings()), Path::new("other/evil.tex")),
            None
        );
    }

    #[test]
    fn tex_change_builds_request_from_snapshot() {
        let project = ProjectState {
            root: PathBuf::from("proj"),
            root_file: Some(PathBuf::from("proj/main.tex")),
        };
        let mut s = settings();
        s.compile.engine = Engine::LuaLaTeX;
        s.compile.timeout_secs = 60;

        let req = compile_request_for_change(ctx(&project, &s), Path::new("proj/chapters/a.tex"))
            .expect("应触发");
        assert_eq!(req.root_file, PathBuf::from("proj/main.tex"));
        assert_eq!(req.project_root, PathBuf::from("proj"));
        assert_eq!(req.engine, Engine::LuaLaTeX);
        assert_eq!(req.timeout, Duration::from_secs(60));
        // roadmap ㉘：编辑触发 = 草稿（快速出图，由空闲收敛兜底正确性）
        assert_eq!(req.kind, CompileKind::Quick);
    }

    #[test]
    fn manual_request_is_full_kind() {
        // roadmap ㉘：手动「编译」与前端空闲收敛都走 Full（用户要正确结果 / 收敛兜底）
        let project = ProjectState {
            root: PathBuf::from("proj"),
            root_file: Some(PathBuf::from("proj/main.tex")),
        };
        let req = compile_request_manual(ctx(&project, &settings())).unwrap();
        assert_eq!(req.kind, CompileKind::Full);
    }

    #[test]
    fn request_is_snapshot_not_reference() {
        // 请求携带构造时值：之后设置变化不影响已构造请求（modules.md §2.4）
        let project = ProjectState {
            root: PathBuf::from("proj"),
            root_file: Some(PathBuf::from("proj/main.tex")),
        };
        let mut s = settings();
        let req = compile_request_for_change(ctx(&project, &s), Path::new("proj/a.tex")).unwrap();
        s.compile.timeout_secs = 5; // 构造后改设置
        assert_eq!(req.timeout, Duration::from_secs(120)); // 请求不受影响
    }

    #[test]
    fn manual_request_ignores_changed_path() {
        let project = ProjectState {
            root: PathBuf::from("proj"),
            root_file: Some(PathBuf::from("proj/main.tex")),
        };
        let req = compile_request_manual(ctx(&project, &settings())).unwrap();
        assert_eq!(req.root_file, PathBuf::from("proj/main.tex"));
    }

    #[test]
    fn manual_request_without_root_returns_none() {
        let project = ProjectState {
            root: PathBuf::from("proj"),
            root_file: None,
        };
        assert_eq!(compile_request_manual(ctx(&project, &settings())), None);
    }
}
