//! `.synctex.gz` **自解析**（roadmap ⑤ 的底座替换：不再依赖系统 `synctex` 二进制）。
//!
//! 为什么需要它：ADR-0008 选了"走系统 synctex CLI"，而那是 **TeX Live 的工具**。⑫ 里程碑的业务
//! 前提是"干净 Windows 机器零预装可用"——只装 Tectonic 时，`synctex.exe` 不存在，正反向定位
//! 全废。Tectonic 只**生成** `.synctex.gz`（`--synctex`），没有查询子命令 ⇒ 查询只能我们自己解。
//!
//! 本模块**只收已解压的文本**（gzip 解压在 infra，见 ADR-0006：core 无 IO、无 IO 依赖）。
//!
//! ## 文件格式（本机实测标定，TeX Live 2026 / Tectonic 0.17 产出的文件同构）
//!
//! ```text
//! SyncTeX Version:1
//! Input:<tag>:<path>          ← tag 1 起；**可以为空**（见下面"空 Input"的坑）
//! ...
//! Output:pdf
//! Magnification:1000          ← 1000 = 1.0
//! Unit:1
//! X Offset:0
//! Y Offset:0
//! Content:
//! {1                          ← 页码 = 1（`{<page>` 开、`}` 闭）
//! [1,7:4736287,45851111:26673152,41114824,0      ← vbox 开：`[<tag>,<line>:<x>,<y>[:<w>[,<h>[,<d>]]]`
//! (1,7:8799519,22018392:22609920,947257,236813   ← hbox 开：`(`
//! h…/v…                       ← 无子节点的 hbox/vbox（同字段）
//! x<tag>,<line>:<x>,<y>       ← 当前位置（无尺寸）
//! k…,<line>:<x>,<y>:<w>       ← kern
//! g<tag>,<line>:<x>,<y>       ← glue
//! !<offset>                   ← 锚点（`synctex edit` 的 Offset 字段用），本模块跳过
//! ) ]                         ← hbox / vbox 闭
//! }
//! ```
//!
//! ## 坐标换算（**逐位标定过**，不是抄文档）
//!
//! 原始值是 TeX 的 **sp**（scaled point）：`PDF 点 = raw / 65781.76`（= 65536 × 72.27/72）。
//! 标定方法：对同一份 `main.synctex.gz`，`synctex view -i "7:0:main.tex"` 给出
//! `x:133.768372 y:334.718781 W:343.711060 H:17.999973`，而文件里对应记录是
//! `(1,7:8799519,22018392:22609920,947257,236813` ⇒
//! `8799519/65781.76 = 133.7684`、`22018392/65781.76 = 334.7188`、`22609920/65781.76 = 343.7111`、
//! `(947257+236813)/65781.76 = 18.0000` —— 四项**逐位吻合**，且 `H = height + depth`。
//! `Unit`/`Magnification`/`X,Y Offset` 按 `raw/65781.76 × mag/1000 + offset` 参与（真实文件里
//! 恒为 1/1000/0，故退化式是被实测覆盖的那一支；带偏移的形态本机没有样本，标注为未实测）。
//!
//! ## 已知坑：**空 `Input:`**
//!
//! 库形态（路径 B，自持 I/O 层）产出的文件里，除主输入外**几乎全是空路径**（实测 141 条里只有 29
//! 条非空，且非空的那些是 `.aux` 等生成物）。原因是引擎经 `input_open_name_with_abspath` 询问
//! 真实路径，而那一层只对"项目根内能找到的文件"给得出路径。后果：反向命中落在空 tag 上 ⇒ 解析
//! 器返回 `None`，调用方按"此处没有对应源码"处理（**不伪造**、不静默给错文件）。

use std::collections::BTreeMap;
use std::path::Path;

use super::model::{SourcePosition, SyncTexPosition};

/// sp → PDF 点的除数（Synctex 的 `SYNCTEX_UNIT_FACTOR`）。
pub const UNIT_FACTOR: f64 = 65781.76;

/// 记录类型（只区分检索用得上的几类；其余一律 [`NodeKind::Other`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// `[` 有子节点的 vbox
    VBoxOpen,
    /// `(` 有子节点的 hbox
    HBoxOpen,
    /// `v` 无子节点的 vbox
    VBox,
    /// `h` 无子节点的 hbox
    HBox,
    /// `k` kern
    Kern,
    /// `g` glue
    Glue,
    /// `$` 数学
    Math,
    /// `x` 当前位置
    Current,
    /// 其它（`f`/`<`/`>` 等，本模块不用）
    Other,
}

/// 一条记录（坐标是文件里的原始 sp）。
#[derive(Debug, Clone, Copy)]
pub struct Node {
    pub kind: NodeKind,
    pub tag: u32,
    pub line: u32,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    pub d: i64,
}

impl Node {
    /// 该记录是否有**可视尺寸**（`x`/`k`/`g` 这类没有宽高，不适合当"行位置"）。
    fn has_extent(&self) -> bool {
        self.w > 0 && (self.h > 0 || self.d > 0)
    }
}

/// 解析后的 SyncTeX 文档。
#[derive(Debug, Default)]
pub struct SyncTexDoc {
    /// `tag`（1 起）→ 路径；越界/空串留给调用方判断。
    inputs: Vec<String>,
    unit: f64,
    magnification: f64,
    x_offset: f64,
    y_offset: f64,
    /// 页号 → 该页记录（**文件顺序**：就是排版顺序，forward 的"第一个"依据）。
    pages: BTreeMap<u32, Vec<Node>>,
}

impl SyncTexDoc {
    /// 解析已解压的 `.synctex` 文本。**不报错**：解析器对损坏输入尽量多解（与 synctex 自身的容错一致），
    /// 真的一页都没有时 [`Self::is_empty`] 为真，调用方据此报"同步数据缺失/损坏"。
    pub fn parse(text: &str) -> Self {
        let mut doc = SyncTexDoc {
            unit: 1.0,
            magnification: 1000.0,
            ..Default::default()
        };
        let mut page: u32 = 0;
        let mut in_content = false;
        for raw in text.lines() {
            let line = raw.trim_end_matches(['\r', '\n']);
            // ── `Input:` **可能出现在任何位置**（实测：多文件工程 96 条里有 12 条在 `Content:`
            //    之后 —— 文件是**首次被打开时**才登记进 Input 表的）。只在头部读会漏掉**全部章节
            //    源码**，而它们恰恰是用户要跳转的目标：`scripts/synctex-selfcheck.mjs` 对拍时
            //    multifile 只有 1/12 命中，就是这条引起的。
            if let Some(v) = line.strip_prefix("Input:") {
                // `Input:<tag>:<path>`；路径可能含 `:`（Windows 盘符）⇒ 只切第一个冒号。
                if let Some((tag, path)) = v.split_once(':') {
                    if let Ok(tag) = tag.trim().parse::<usize>() {
                        if tag > 0 {
                            if doc.inputs.len() < tag {
                                doc.inputs.resize(tag, String::new());
                            }
                            doc.inputs[tag - 1] = path.to_owned();
                        }
                    }
                }
                continue;
            }
            if !in_content {
                if let Some(v) = line.strip_prefix("Magnification:") {
                    doc.magnification = v.trim().parse().unwrap_or(1000.0);
                    continue;
                }
                if let Some(v) = line.strip_prefix("Unit:") {
                    doc.unit = v.trim().parse().unwrap_or(1.0);
                    continue;
                }
                if let Some(v) = line.strip_prefix("X Offset:") {
                    doc.x_offset = v.trim().parse().unwrap_or(0.0);
                    continue;
                }
                if let Some(v) = line.strip_prefix("Y Offset:") {
                    doc.y_offset = v.trim().parse().unwrap_or(0.0);
                    continue;
                }
                if line == "Content:" {
                    in_content = true;
                }
                continue;
            }
            // ── Content 段 ──
            let first = match line.as_bytes().first() {
                Some(b) => *b,
                None => continue,
            };
            match first {
                b'{' => {
                    page = line[1..].trim().parse().unwrap_or(0);
                    continue;
                }
                b'}' => {
                    page = 0;
                    continue;
                }
                b'!' => continue, // 锚点：`edit` 的 Offset 用，本模块不需要
                _ => {}
            }
            if page == 0 {
                continue; // 不在任何页里（postamble 等）
            }
            let kind = match first {
                b'[' => NodeKind::VBoxOpen,
                b'(' => NodeKind::HBoxOpen,
                b'v' => NodeKind::VBox,
                b'h' => NodeKind::HBox,
                b'k' => NodeKind::Kern,
                b'g' => NodeKind::Glue,
                b'$' => NodeKind::Math,
                b'x' => NodeKind::Current,
                _ => NodeKind::Other,
            };
            if let Some(node) = parse_record(kind, &line[1..]) {
                doc.pages.entry(page).or_default().push(node);
            }
        }
        doc
    }

    pub fn is_empty(&self) -> bool {
        self.pages.values().all(|v| v.is_empty())
    }

    /// 页数（解析出来的，不是 PDF 的页数）。
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// `tag` → 路径（空白串按 `None` 处理：空 Input 是已知的库形态缺陷，不伪造）。
    pub fn input_path(&self, tag: u32) -> Option<&str> {
        let s = self.inputs.get(tag.checked_sub(1)? as usize)?.trim();
        (!s.is_empty()).then_some(s)
    }

    /// 原始 sp → PDF 点。
    ///
    /// ⚠ **偏移量也是 raw sp，必须在缩放之前相加**（实测教训）：有的工具链会写
    /// `X Offset:4736287`（= 72 pt），把"点"当偏移加在换算之后会整整错 72 pt。标定见模块头注。
    fn to_points(&self, raw: i64) -> f64 {
        raw as f64 / UNIT_FACTOR * (self.magnification / 1000.0)
    }
    fn x_pt(&self, raw: i64) -> f64 {
        (raw as f64 + self.x_offset) / UNIT_FACTOR * (self.magnification / 1000.0)
    }
    fn y_pt(&self, raw: i64) -> f64 {
        (raw as f64 + self.y_offset) / UNIT_FACTOR * (self.magnification / 1000.0)
    }

    /// 源码文件 → `tag`。先按**归一化全等**，再按**尾部路径段**兜底（synctex 里可能是相对路径、
    /// 正反斜杠混用、带 `./`）。
    pub fn tag_for_file(&self, file: &Path) -> Option<u32> {
        let want = normalize(file.to_string_lossy().as_ref());
        let mut suffix: Option<u32> = None;
        for (i, p) in self.inputs.iter().enumerate() {
            if p.trim().is_empty() {
                continue;
            }
            let have = normalize(p);
            if have == want {
                return Some(i as u32 + 1);
            }
            if suffix.is_none() && (have.ends_with(&want) || want.ends_with(&have)) {
                suffix = Some(i as u32 + 1);
            }
        }
        suffix
    }

    /// **正向**：源码 (tag, line) → PDF 页/坐标。
    ///
    /// 两条规则都是**对着 `synctex view` 标定出来的**（见 `scripts/synctex-selfcheck.mjs` 的对拍）：
    ///
    /// 1. **行匹配不是精确相等**：取**最接近请求行的记录行**（平手取 ≥ 的那条）。
    ///    依据：beamer 夹具里 `\frametitle` 在第 12 行，而该文件的记录**从第 19 行起**，
    ///    `synctex view -i "12:0:main.tex"` 给的仍是第 19 行那个盒子的位置 (10.909, 95.208)。
    ///    只按 `line == 12` 匹配会**一条都找不到**（这正是第一版 0 命中的原因）；
    ///    而"≥ 的最小行"在 beamer 上会把**所有**早期 frame 都送到第 19 行（同一页），
    ///    实测往返跳到位只有 2/10（CLI 7/10）⇒ 改成最接近。
    /// 2. **同一行内优先"有可视尺寸的 hbox"**：`(`/`h`（正文水平材料）> `[`/`v`（版面容器）>
    ///    有宽度的 > 任意。依据：外形 vbox 的 (x,y) 是整块版面的左上角（实测 `x=72, y=697`），
    ///    拿它当"第 N 行的位置"会跳到页面左上角，而 CLI 给的是正文行首。
    ///
    /// 页取升序里最先出现的那页。
    pub fn forward_line(&self, tag: u32, line: u32) -> Option<SyncTexPosition> {
        // ① 定行：取**最接近**请求行的记录行（平手时取 ≥ 的那条）
        let mut want_line: Option<u32> = None;
        let mut best_gap: u32 = u32::MAX;
        for nodes in self.pages.values() {
            for n in nodes {
                if n.tag != tag {
                    continue;
                }
                let gap = n.line.abs_diff(line);
                let better = match want_line {
                    None => true,
                    Some(cur) => gap < best_gap || (gap == best_gap && n.line > cur),
                };
                if better {
                    best_gap = gap;
                    want_line = Some(n.line);
                }
            }
        }
        let want_line = want_line?;

        // ② 在该行的记录里按"hbox > vbox > 有宽度 > 任意"取第一条（文件顺序）
        let mut ring: [Option<(u32, &Node)>; 4] = [None; 4];
        for (page, nodes) in &self.pages {
            for n in nodes {
                if n.tag != tag || n.line != want_line {
                    continue;
                }
                let slot = if matches!(n.kind, NodeKind::HBoxOpen | NodeKind::HBox) && n.has_extent() {
                    0
                } else if matches!(n.kind, NodeKind::VBoxOpen | NodeKind::VBox) && n.has_extent() {
                    1
                } else if n.w > 0 {
                    2
                } else {
                    3
                };
                if ring[slot].is_none() {
                    ring[slot] = Some((*page, n));
                }
            }
            if ring[0].is_some() {
                break; // 页升序 ⇒ 第一个命中的页就是最先出现的
            }
        }
        let (page, n) = ring.into_iter().flatten().next()?;
        Some(SyncTexPosition {
            page,
            x: self.x_pt(n.x) as f32,
            y: self.y_pt(n.y) as f32,
        })
    }

    /// **反向**：PDF (page, x, y) → 源码。
    ///
    /// 规则：在该页所有"有可视尺寸"的记录里找**包含该点**的；包含多个时取**面积最小**的
    /// （最深的盒子 = 最贴近那一处的文本）；都不包含时取**中心最近**的一条（synctex 自己也会
    /// 做邻近回落，否则点空白处就完全没反应）。
    ///
    /// 命中空 `Input` 的 tag ⇒ `None`（不伪造文件）。
    pub fn inverse_point(&self, page: u32, x: f32, y: f32) -> Option<SourcePosition> {
        let nodes = self.pages.get(&page)?;
        let px = x as f64;
        let py = y as f64;
        let mut best: Option<(f64, &Node)> = None;
        let mut nearest: Option<(f64, &Node)> = None;
        for n in nodes {
            if !n.has_extent() || self.input_path(n.tag).is_none() {
                continue;
            }
            let nx = self.x_pt(n.x);
            let ny = self.y_pt(n.y);
            let nw = self.to_points(n.w);
            let nh = self.to_points(n.h);
            let nd = self.to_points(n.d);
            // 纵向：baseline 之下 depth、之上 height（与 synctex 的 `[y-depth, y+height]` 一致）
            let inside = px >= nx && px <= nx + nw && py >= ny - nd && py <= ny + nh;
            if inside {
                let area = nw * (nh + nd);
                if best.as_ref().is_none_or(|(a, _)| area < *a) {
                    best = Some((area, n));
                }
            } else {
                // 邻近回落：用盒子中心到点的距离
                let cx = nx + nw / 2.0;
                let cy = ny + (nh - nd) / 2.0;
                let dist = (cx - px).powi(2) + (cy - py).powi(2);
                if nearest.as_ref().is_none_or(|(d, _)| dist < *d) {
                    nearest = Some((dist, n));
                }
            }
        }
        let n = best.map(|(_, n)| n).or(nearest.map(|(_, n)| n))?;
        Some(SourcePosition {
            file: self.input_path(n.tag)?.into(),
            line: n.line,
            // synctex CLI 实测恒为 -1（未知列）；文件格式里 hbox/vbox 记录不带列号（只有
            // `synctex edit` 的 `Column:` 字段才可能给出），故这里如实给 -1。
            column: -1,
        })
    }
}

/// 归一化路径用于比较：反斜杠→正斜杠、去 `./`、小写（Windows 大小写不敏感）。
fn normalize(s: &str) -> String {
    s.replace('\\', "/")
        .replace("/./", "/")
        .to_lowercase()
}

/// 解析 `,<line>:<x>,<y>[:<w>[,<h>[,<d>]]]`（**tag 已由调用方从首字符后切掉**？不——tag 在串首）。
///
/// 形如 `1,7:4736287,45851111:26673152,41114824,0`：`tag,line : x,y : w,h,d`。
fn parse_record(kind: NodeKind, body: &str) -> Option<Node> {
    let (tag_s, rest) = body.split_once(',')?;
    let tag: u32 = tag_s.trim().parse().ok()?;
    let (line_s, rest) = rest.split_once(':')?;
    let line: u32 = line_s.trim().parse().ok()?;
    let mut dims = rest.split(':');
    let xy = dims.next()?;
    let (x_s, y_s) = xy.split_once(',')?;
    let x: i64 = x_s.trim().parse().ok()?;
    let y: i64 = y_s.trim().parse().ok()?;
    let (mut w, mut h, mut d) = (0i64, 0i64, 0i64);
    if let Some(whd) = dims.next() {
        let parts: Vec<&str> = whd.split(',').collect();
        if let Some(v) = parts.first() {
            w = v.trim().parse().unwrap_or(0);
        }
        if let Some(v) = parts.get(1) {
            h = v.trim().parse().unwrap_or(0);
        }
        if let Some(v) = parts.get(2) {
            d = v.trim().parse().unwrap_or(0);
        }
    }
    Some(Node { kind, tag, line, x, y, w, h, d })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小可解析文件：一页、主输入是 `main.tex`（tag 1）、一条 vbox + 一条 hbox。
    const MINI: &str = "SyncTeX Version:1\n\
        Input:1:E:/proj/main.tex\n\
        Input:2:\n\
        Output:pdf\n\
        Magnification:1000\n\
        Unit:1\n\
        X Offset:0\n\
        Y Offset:0\n\
        Content:\n\
        {1\n\
        [1,7:4736287,45851111:26673152,41114824,0\n\
        (1,7:8799519,22018392:22609920,947257,236813\n\
        )\n\
        ]\n\
        }\n";

    #[test]
    fn parses_header_and_converts_units_like_the_cli() {
        let doc = SyncTexDoc::parse(MINI);
        assert!(!doc.is_empty());
        assert_eq!(doc.page_count(), 1);
        assert_eq!(doc.input_path(1), Some("E:/proj/main.tex"));
        assert_eq!(doc.input_path(2), None, "空 Input 必须按 None（库形态已知缺陷，不伪造）");

        // 逐位对齐 synctex view 的实测输出（见模块头注的标定）
        let pos = doc.forward_line(1, 7).expect("应当能正向定位");
        assert_eq!(pos.page, 1);
        assert!((pos.x - 133.768_37).abs() < 0.01, "x={}", pos.x);
        assert!((pos.y - 334.718_78).abs() < 0.01, "y={}", pos.y);
    }

    #[test]
    fn forward_prefers_a_node_with_visible_extent() {
        // 第一条 vbox 有尺寸但**就是**外层盒子；hbox 也有尺寸。规则取文件顺序的第一条有尺寸者。
        // 这里给"第一条没有尺寸"的形态：`x` 记录在前，应被跳过。
        let text = MINI.replace(
            "[1,7:4736287,45851111:26673152,41114824,0",
            "x1,7:1,1\n[1,7:4736287,45851111:26673152,41114824,0",
        );
        let doc = SyncTexDoc::parse(&text);
        let pos = doc.forward_line(1, 7).expect("应当能定位");
        assert!(pos.x > 70.0, "不该选中无尺寸的 x 记录：x={}", pos.x);
    }

    /// **行匹配不是精确相等**：请求 5（记录只有 7）⇒ 取 ≥5 的最小记录行 7。
    /// 这是 beamer 夹具（记录从第 19 行起、而样本在第 12 行）暴露出来的：精确匹配会 0 命中。
    #[test]
    fn forward_falls_forward_to_the_next_recorded_line() {
        let doc = SyncTexDoc::parse(MINI);
        let pos = doc.forward_line(1, 5).expect("请求行 5 应落到记录行 7");
        assert_eq!(pos.page, 1);
        assert!((pos.y - 334.718_78).abs() < 0.01, "应当是第 7 行那个 hbox：y={}", pos.y);
    }

    /// 请求行**晚于**所有记录 ⇒ 退到最近的前一条（不能没反应）。
    #[test]
    fn forward_falls_back_to_the_previous_recorded_line() {
        let doc = SyncTexDoc::parse(MINI);
        let pos = doc.forward_line(1, 99).expect("请求行 99 应落到记录行 7");
        assert_eq!(pos.page, 1);
    }

    /// **偏移量是 raw sp，必须在缩放前相加**：`X Offset:4736287`（= 72 pt）不能被当成 72 pt。
    #[test]
    fn offsets_are_raw_units_added_before_scaling() {
        let text = MINI
            .replace("X Offset:0", "X Offset:4736287")
            .replace("Y Offset:0", "Y Offset:4736287");
        let doc = SyncTexDoc::parse(&text);
        let pos = doc.forward_line(1, 7).expect("应当能定位");
        // x = (8799519 + 4736287)/65781.76 = 205.77 pt（若把偏移当"点"加，会得到 205.77 之外的数）
        assert!((pos.x - 205.769).abs() < 0.01, "x={}", pos.x);
    }

    #[test]
    fn inverse_finds_the_point_and_maps_tag_to_file() {
        let doc = SyncTexDoc::parse(MINI);
        // hbox 覆盖 x∈[133.77,477.48]、y∈[331.12,349.12]（baseline 334.72 ± depth/height）
        let hit = doc.inverse_point(1, 200.0, 340.0).expect("点应落在 hbox 里");
        assert_eq!(hit.line, 7);
        assert_eq!(hit.file.to_string_lossy(), "E:/proj/main.tex");
        assert_eq!(hit.column, -1, "文件格式里没有列号 ⇒ 如实 -1");
    }

    #[test]
    fn inverse_does_not_invent_a_file_for_empty_input() {
        // 只有空 Input 的 tag ⇒ 一律 None（库形态产出的文件就是这种形态）
        let text = MINI.replace("Input:1:E:/proj/main.tex", "Input:1:");
        let doc = SyncTexDoc::parse(&text);
        assert!(doc.inverse_point(1, 200.0, 340.0).is_none());
    }

    #[test]
    fn inverse_falls_back_to_the_nearest_box_when_the_point_is_in_empty_space() {
        let doc = SyncTexDoc::parse(MINI);
        // 页面右上角空白：不包含在任何盒子里 ⇒ 邻近回落，仍给得出源码（否则点空白毫无反应）
        let hit = doc.inverse_point(1, 560.0, 800.0);
        assert!(hit.is_some(), "空白处应当邻近回落而不是彻底没反应");
    }

    #[test]
    fn tag_for_file_tolerates_slashes_dots_and_case() {
        let doc = SyncTexDoc::parse(MINI);
        assert_eq!(doc.tag_for_file(Path::new("E:\\proj\\main.tex")), Some(1));
        assert_eq!(doc.tag_for_file(Path::new("e:/proj/./main.tex")), Some(1));
        assert_eq!(doc.tag_for_file(Path::new("E:/other/nope.tex")), None);
    }

    /// **`Input:` 可能出现在 `Content:` 之后**（文件首次被打开时才登记）。
    ///
    /// 实测来源：multifile 工程 96 条 Input 里有 12 条在内容段里，而它们正是**章节源码**。
    /// 只在头部读会让多文件工程的反向/正向定位几乎全废（当时对拍 1/12 命中）。
    #[test]
    fn input_records_are_read_after_content_too() {
        let text = MINI.replace(
            "}\n",
            "}\nInput:9:E:/proj/chapters/intro.tex\n",
        );
        let doc = SyncTexDoc::parse(&text);
        assert_eq!(doc.input_path(9), Some("E:/proj/chapters/intro.tex"));
        assert_eq!(doc.tag_for_file(Path::new("E:/proj/chapters/intro.tex")), Some(9));
    }

    #[test]
    fn damaged_input_yields_no_pages_but_does_not_panic() {
        let doc = SyncTexDoc::parse("垃圾\nInput:1:x\nContent:\n{1\n[乱码\n");
        assert!(doc.is_empty(), "解不出页就如实为空（调用方报'同步数据缺失/损坏'）");
        assert!(doc.forward_line(1, 1).is_none());
        assert!(doc.inverse_point(1, 0.0, 0.0).is_none());
    }
}
