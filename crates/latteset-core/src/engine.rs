//! TeX 引擎**清单**（roadmap ㊻）：谁可选、什么顺序、本机能不能用。
//!
//! 为什么在 core：这张表要**同时**满足三件事，写在任何一层前端都做不到 ——
//! ① 顺序与文案只有一份（前端不再各写一份 `ENGINES` 数组）；
//! ② "本机可用性"由后端探测（进程/PATH 是 infra 的事，core 只收一个判据闭包 ⇒ 可单测）；
//! ③ 允许用环境变量**覆盖清单本身**（CI/headless/裁剪发布）。
//!
//! 边界：**支持的引擎集合仍是 [`Engine`] 枚举**（编译命令按引擎分支：`-xelatex` / `-C` 分档 /
//! 是否经 latexmk），环境变量只能"筛选与重排"，不能凭空造一个新引擎 —— 那需要扩 runner 的
//! 命令模板与产物契约，属另一个功能点。

use crate::types::Engine;
use serde::{Deserialize, Serialize};
use specta::Type;

/// 覆盖清单的环境变量名（值 = 逗号分隔的引擎 id，如 `xelatex,tectonic`）。
pub const ENGINES_ENV: &str = "LATTESET_TEX_ENGINES";

/// 推荐顺序（**Tectonic 第一**，ADR-0014 以它为基准形态）。
///
/// 这与"默认值"是两件事：默认值由 `Settings::default()` 决定，这里是下拉里的先后。
pub const ENGINE_ORDER: [Engine; 4] = [
    Engine::Tectonic,
    Engine::XeLaTeX,
    Engine::LuaLaTeX,
    Engine::PdfLaTeX,
];

/// 界面名（选项栏里显示的那一个词）。**别把说明写进这里**：原生下拉的弹层按最长选项撑宽
/// （真机反馈过溢出），说明走 [`hint`]，显示在选项栏下方。
pub fn label(engine: Engine) -> &'static str {
    match engine {
        Engine::Tectonic => "Tectonic",
        Engine::XeLaTeX => "XeLaTeX",
        Engine::LuaLaTeX => "LuaLaTeX",
        Engine::PdfLaTeX => "pdfLaTeX",
    }
}

/// 一句话说明（选中项显示在选项栏下方）。
pub fn hint(engine: Engine) -> &'static str {
    match engine {
        // 文案必须与实现一致（runner.rs 的 `-C` 分档）：首次编译（缓存里没有可用 bundle）**会联网**
        // 下载宏包集；缓存就绪后离线复用。写成"总是联网"或"总是离线"都与实际相反。
        Engine::Tectonic => {
            "免装 TeX Live（自带宏包）；首次编译需联网下载宏包集（约 60 MB，可能数十秒到数分钟），\
             之后离线复用缓存；该引擎下不启用页级增量复用"
        }
        Engine::XeLaTeX => "需要本机装好 TeX Live；中文支持最佳，页级增量复用只在这个引擎下可用",
        Engine::LuaLaTeX => "Lua 脚本、最新特性；比 XeLaTeX 慢，首次编译要建字体缓存",
        Engine::PdfLaTeX => "传统引擎，中文需额外配置",
    }
}

/// 跑一趟**完整编译**需要哪些可执行文件在 PATH 上（**判据的唯一副本**，探测侧照它查）。
///
/// - 三个 latexmk 引擎：`latexmk` + 引擎自己（Quick 路径是直调引擎的，见 roadmap ㉘）；
/// - Tectonic：不经 latexmk，只要 `tectonic`（库形态则一个都不需要，由调用方给 `lib_form` 事实）。
pub fn required_binaries(engine: Engine) -> &'static [&'static str] {
    match engine {
        Engine::XeLaTeX => &["latexmk", "xelatex"],
        Engine::LuaLaTeX => &["latexmk", "lualatex"],
        Engine::PdfLaTeX => &["latexmk", "pdflatex"],
        Engine::Tectonic => &["tectonic"],
    }
}

/// 清单里的一项（命令面 DTO，前端只渲染）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct EngineInfo {
    /// 引擎 id（与设置里存的值同一个字面量）。
    pub id: Engine,
    /// 选项栏里显示的名字。
    pub label: String,
    /// 一句话说明。
    pub hint: String,
    /// 本机/本次构建能不能用它编译。
    pub available: bool,
    /// **不可用时**给一句可行动的原因（可用时为 `None`）。
    pub reason: Option<String>,
}

/// 组装清单。
///
/// - `has_binary`：`name` 这个可执行文件在不在（真机走 infra 的 PATH 探测；测试给假闭包）；
/// - `lib_form_compiled_in`：本次构建是否编入了 Tectonic 库形态（`tectonic-lib` 特性）——
///   库形态**不需要** `tectonic.exe`，所以它能让 Tectonic 变可用；
/// - `env_override`：`LATTESET_TEX_ENGINES` 的原始值（`None`/空 = 不干预）。
///
/// **覆盖语义**：给了就是"清单就是这些、按这个顺序"（可以藏掉不想要的引擎）；无法识别的名字
/// 忽略；**一个都不认识时整体忽略覆盖**（回到完整清单）—— 否则一个笔误会让选项栏变空。
/// 覆盖**不能**让不可用的引擎变可用（可用性是探测出来的事实，不是配置）。
pub fn engine_list(
    has_binary: &dyn Fn(&str) -> bool,
    lib_form_compiled_in: bool,
    env_override: Option<&str>,
) -> Vec<EngineInfo> {
    let order: Vec<Engine> = match env_override.and_then(parse_override) {
        Some(list) => list,
        None => ENGINE_ORDER.to_vec(),
    };
    order
        .into_iter()
        .map(|e| {
            let (available, reason) = availability(e, has_binary, lib_form_compiled_in);
            EngineInfo {
                id: e,
                label: label(e).to_owned(),
                hint: hint(e).to_owned(),
                available,
                reason,
            }
        })
        .collect()
}

/// 覆盖串 → 引擎列表；认不出任何引擎时返回 `None`（调用方回落到完整清单）。
fn parse_override(raw: &str) -> Option<Vec<Engine>> {
    let mut out = Vec::new();
    for token in raw.split(',') {
        let t = token.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(e) = engine_from_id(t) {
            // 重复写同一个引擎不重复出条目（覆盖串是人手写的，容错优先）
            if !out.contains(&e) {
                out.push(e);
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// 宽松认 id：忽略大小写与常见别名（`xe`/`xetex`/`tectonic`…），
/// 但**只认枚举里的四个**（见模块文档的边界说明）。
fn engine_from_id(raw: &str) -> Option<Engine> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "tectonic" => Some(Engine::Tectonic),
        "xelatex" | "xe" | "xetex" => Some(Engine::XeLaTeX),
        "lualatex" | "lua" | "luatex" => Some(Engine::LuaLaTeX),
        "pdflatex" | "pdf" | "pdftex" => Some(Engine::PdfLaTeX),
        _ => None,
    }
}

/// 可用性判据（纯函数：事实由 `has_binary` / `lib_form_compiled_in` 给）。
fn availability(
    engine: Engine,
    has_binary: &dyn Fn(&str) -> bool,
    lib_form_compiled_in: bool,
) -> (bool, Option<String>) {
    if engine == Engine::Tectonic {
        if has_binary("tectonic") {
            return (true, None);
        }
        if lib_form_compiled_in {
            // 库形态不需要外部 tectonic.exe
            return (true, None);
        }
        return (
            false,
            Some(
                "本机 PATH 上没有 tectonic.exe，且本次构建未编入 Tectonic 库形态（`--features tectonic-lib`）\
                 —— 装一个 Tectonic 或换用 TeX Live 引擎"
                    .to_owned(),
            ),
        );
    }
    let missing: Vec<&str> = required_binaries(engine)
        .iter()
        .copied()
        .filter(|b| !has_binary(b))
        .collect();
    if missing.is_empty() {
        (true, None)
    } else {
        (
            false,
            Some(format!(
                "本机 PATH 上没有 {} —— 装好 TeX Live（或把它的 bin 目录加进 PATH）后重启应用",
                missing.join(" 和 ")
            )),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_present(_: &str) -> bool {
        true
    }
    fn nothing_present(_: &str) -> bool {
        false
    }
    /// 只有 TeX Live（没有 tectonic.exe）——本机实验室环境就是这个形态。
    fn texlive_only(name: &str) -> bool {
        name != "tectonic"
    }

    /// 顺序：Tectonic 第一（ADR-0014）；清单覆盖全部四个引擎，且 id 不重复。
    #[test]
    fn order_puts_tectonic_first() {
        let list = engine_list(&all_present, false, None);
        let ids: Vec<Engine> = list.iter().map(|e| e.id).collect();
        assert_eq!(ids, ENGINE_ORDER.to_vec());
        assert_eq!(ids[0], Engine::Tectonic);
        assert_eq!(list.len(), 4, "四个引擎一个不少");
    }

    /// 名字与说明都非空，且**名字里不含说明**（原生下拉的弹层按最长选项撑宽，塞说明会溢出）。
    #[test]
    fn labels_stay_short_and_hints_carry_the_detail() {
        for e in ENGINE_ORDER {
            let l = label(e);
            let h = hint(e);
            assert!(!l.is_empty() && !h.is_empty(), "{e:?}");
            assert!(l.chars().count() <= 10, "{e:?} 的名字要短：{l}");
            assert!(!l.contains('；') && !l.contains("需要"), "名字里不该有句子：{l}");
        }
    }

    /// 三个 latexmk 引擎缺 `latexmk` 或引擎自己都算不可用；Tectonic 只认 `tectonic`。
    #[test]
    fn availability_follows_the_required_binaries() {
        let none = engine_list(&nothing_present, false, None);
        assert!(none.iter().all(|e| !e.available), "什么都没有 ⇒ 四个全不可用");
        for e in &none {
            assert!(e.reason.is_some(), "{:?} 不可用就必须给原因", e.id);
        }

        let texlive = engine_list(&texlive_only, false, None);
        let by = |id: Engine| texlive.iter().find(|e| e.id == id).unwrap().clone();
        assert!(by(Engine::XeLaTeX).available);
        assert!(by(Engine::LuaLaTeX).available);
        assert!(by(Engine::PdfLaTeX).available);
        assert!(!by(Engine::Tectonic).available, "没装 tectonic.exe ⇒ Tectonic 不可用");
        assert!(by(Engine::Tectonic).reason.unwrap().contains("tectonic.exe"));

        // 只缺 latexmk：三个 latexmk 引擎全挂，Tectonic 反而可用
        let no_latexmk = engine_list(&|n| n != "latexmk", true, None);
        assert!(!no_latexmk.iter().find(|e| e.id == Engine::XeLaTeX).unwrap().available);
        assert!(no_latexmk.iter().find(|e| e.id == Engine::Tectonic).unwrap().available);
    }

    /// 库形态编进来时，没有 `tectonic.exe` 也算可用（库内嵌不调外部程序）。
    #[test]
    fn lib_form_makes_tectonic_available_without_the_binary() {
        let list = engine_list(&texlive_only, true, None);
        let t = list.iter().find(|e| e.id == Engine::Tectonic).unwrap();
        assert!(t.available && t.reason.is_none());
    }

    /// 环境变量 = 清单与顺序（可藏掉引擎）；认不出任何名字时整体忽略，绝不返回空清单。
    #[test]
    fn env_override_selects_and_reorders() {
        let only_two = engine_list(&all_present, false, Some("xelatex, tectonic"));
        let ids: Vec<Engine> = only_two.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![Engine::XeLaTeX, Engine::Tectonic], "顺序照覆盖串");
        assert_eq!(only_two.len(), 2, "没点名的引擎不出现");

        // 大小写/别名/空白/重复都要容错
        let messy = engine_list(&all_present, false, Some("  XeLaTeX , xe , pdf "));
        assert_eq!(messy.len(), 2, "别名与重复项要归并：{messy:?}");

        // 全不认识 ⇒ 忽略覆盖（不能把选项栏变空）
        let ignored = engine_list(&all_present, false, Some("nonsense,,"));
        assert_eq!(ignored.len(), 4);
        // 空串同理
        assert_eq!(engine_list(&all_present, false, Some("   ")).len(), 4);
    }

    /// 覆盖**不能**把不可用的引擎变可用（可用性是探测出来的事实，不是配置）。
    #[test]
    fn env_override_cannot_fake_availability() {
        let list = engine_list(&nothing_present, false, Some("tectonic"));
        assert_eq!(list.len(), 1);
        assert!(!list[0].available);
        assert!(list[0].reason.is_some());
    }
}
