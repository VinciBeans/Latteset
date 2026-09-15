//! XDV 页表扫描（把 `test_file/research-tectonic-lib/engine-spike/src/bin/xdvscan.rs` 的算法
//! 搬进产品仓：方案 §6 的 P5 复核入口由 `scripts/tectonic-lib-xdvscan.mjs` 驱动本示例）。
//!
//! 三件事（与 t5 的口径一致）：
//! 1. **页数与流式时间线**：固定块大小（默认 16384，= C 侧 `DVI_BUF_SIZE`）下，每块到达时
//!    "已完成多少页"——这就是实时消费者能看到的粒度（t5 §4.2 实测：16 KB → 4 页）；
//! 2. **精确页表（可选，默认关）**：每页 `bop` 的字节起点。做法是先增量喂窗口定位"第 k+1 页
//!    出现在哪个窗口"，再用"前缀只有落在 opcode 边界上才解析成功"这一性质把起点收敛到精确字节；
//! 3. **退出码**：解析失败 exit 1（是结论，不是 harness 错误）。
//!
//! ## V-01（t24）：为什么精确页表**默认关**、并带时限
//!
//! 第 2 件的复杂度是 **页数 × 窗口 × 文件长**（每个候选字节都从 0 重解析前缀）。t23 复审实测
//! **large 夹具（4.59 MB / 125 页）：240 s 预算内跑不完，CPU 已耗 224.2 s**；而方案 §6 的 P5/P6
//! 复核命令在 large 上会因此挂住。处置（按复审给的两条路之二）：
//! - **默认 `--no-refine`**：只出页数 + 流式时间线（`page_ranges` 退回 `[0,0]`），P5/P6 命令不再挂住；
//! - **精确页表另设入口与时限**：`--refine [--refine-budget-ms N]`（默认 20000 ms，0 = 无上限）。
//!   超时即停在已收敛的页数上，并**如实**报 `refine_mode: "partial"` + `refine_resolved_pages`
//!   —— 不把未收敛的页当成精确值。
//!
//! 用法（由 scripts 包装）：
//!   cargo run -p latteset-tectonic --example xdvscan -- --xdv <path> [--chunk N] [--pages-json <out>]
//!        [--refine] [--no-refine] [--refine-budget-ms N]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Instant;

use tectonic_xdv::{FileType, XdvEvents, XdvParser};

#[derive(Default)]
struct Collector {
    pages_started: usize,
    chars: usize,
    glyphs: usize,
    specials: usize,
    rules: usize,
    native_fonts: BTreeMap<i32, String>,
    header: String,
}

struct Ev {
    shared: Rc<RefCell<Collector>>,
}

impl XdvEvents for Ev {
    type Error = String;

    fn handle_header(&mut self, filetype: FileType, comment: &[u8]) -> Result<(), String> {
        self.shared.borrow_mut().header = format!("{filetype:?}:{}", String::from_utf8_lossy(comment));
        Ok(())
    }

    fn handle_begin_page(&mut self, _counters: &[i32], _previous_bop: i32) -> Result<(), String> {
        self.shared.borrow_mut().pages_started += 1;
        Ok(())
    }

    fn handle_char_run(&mut self, _font_num: i32, chars: &[i32]) -> Result<(), String> {
        self.shared.borrow_mut().chars += chars.len();
        Ok(())
    }

    fn handle_text_and_glyphs(
        &mut self,
        _font_num: i32,
        text: &str,
        _width: i32,
        glyphs: &[u16],
        _x: &[i32],
        _y: &[i32],
    ) -> Result<(), String> {
        let mut c = self.shared.borrow_mut();
        c.chars += text.chars().count();
        c.glyphs += glyphs.len();
        Ok(())
    }

    fn handle_glyph_run(
        &mut self,
        _font_num: i32,
        glyphs: &[u16],
        _x: &[i32],
        _y: &[i32],
    ) -> Result<(), String> {
        self.shared.borrow_mut().glyphs += glyphs.len();
        Ok(())
    }

    fn handle_define_native_font(
        &mut self,
        name: &str,
        font_num: i32,
        _size: i32,
        _face_index: u32,
        _color_rgba: Option<u32>,
        _extend: Option<u32>,
        _slant: Option<u32>,
        _embolden: Option<u32>,
    ) -> Result<(), String> {
        self.shared.borrow_mut().native_fonts.insert(font_num, name.to_owned());
        Ok(())
    }

    fn handle_special(&mut self, _x: i32, _y: i32, _c: &[u8]) -> Result<(), String> {
        self.shared.borrow_mut().specials += 1;
        Ok(())
    }

    fn handle_rule(&mut self, _x: i32, _y: i32, _h: i32, _w: i32) -> Result<(), String> {
        self.shared.borrow_mut().rules += 1;
        Ok(())
    }
}

/// 用**全新** parser 解析前缀 `data[..len]`：只有整好落在 opcode 边界上才成功。
fn parse_prefix(data: &[u8], len: usize) -> Result<usize, String> {
    let c: Rc<RefCell<Collector>> = Rc::new(RefCell::new(Collector::default()));
    let ev = Ev { shared: c.clone() };
    let mut parser = XdvParser::new(ev);
    parser.parse(&data[..len])?;
    // 先绑定再返回：`Ok(c.borrow()...)` 的 `Ref` 临时量会活到块尾，届时 `c` 已被 drop。
    let pages = c.borrow().pages_started;
    Ok(pages)
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "'").replace('\n', " ")
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut xdv_path = String::new();
    let mut chunk: usize = 16384;
    let mut pages_json = String::new();
    // V-01：精确页表**默认关**（脚本默认 `--no-refine`）；`--refine` 显式打开，并受时限约束。
    let mut refine = false;
    let mut refine_budget_ms: u128 = 20_000;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--xdv" => {
                i += 1;
                xdv_path = argv[i].clone();
            }
            "--chunk" => {
                i += 1;
                chunk = argv[i].parse().expect("--chunk 需要整数");
            }
            "--pages-json" => {
                i += 1;
                pages_json = argv[i].clone();
            }
            "--refine" => refine = true,
            "--no-refine" => refine = false,
            "--refine-budget-ms" => {
                i += 1;
                refine_budget_ms = argv[i].parse().expect("--refine-budget-ms 需要整数");
            }
            other => panic!("unknown arg {other}"),
        }
        i += 1;
    }

    let data = std::fs::read(&xdv_path).expect("while reading XDV");
    let total = data.len();

    // ---- pass 1：增量喂窗口（拿总数 + 每页 `bop` 落在哪个窗口）----
    let collect = Rc::new(RefCell::new(Collector::default()));
    let mut parser = XdvParser::new(Ev { shared: collect.clone() });
    let t0 = Instant::now();
    let mut pos = 0usize;
    let win_base = chunk.min(128);
    let mut win = win_base;
    let mut pages_seen = 0usize;
    let mut page_windows: Vec<(usize, usize)> = Vec::new();
    let mut steps = 0usize;
    while pos < total {
        let end = (pos + win).min(total);
        match parser.parse(&data[pos..end]) {
            Ok((consumed, keep_going)) => {
                let pages_now = collect.borrow().pages_started;
                while pages_seen < pages_now {
                    page_windows.push((pos, end));
                    pages_seen += 1;
                }
                steps += 1;
                if consumed == 0 {
                    if end == total {
                        break;
                    }
                    win = (win * 2).min(1 << 20);
                    continue;
                }
                pos += consumed;
                win = win_base;
                if !keep_going {
                    break;
                }
            }
            Err(e) => {
                println!(
                    "{{\"case\":\"xdvscan\",\"ok\":false,\"mode\":\"incremental\",\"error\":\"{}\",\"offset\":{}}}",
                    json_escape(&e),
                    pos
                );
                std::process::exit(1);
            }
        }
    }
    let consumed_total = pos;
    let scan_ms = t0.elapsed().as_millis();
    let pages = collect.borrow().pages_started;

    // ---- pass 2：把每页起点收敛到精确字节（**默认关 + 带时限**，见文件头 V-01）----
    let t1 = Instant::now();
    let mut page_starts: Vec<usize> = vec![0; pages];
    let mut refined_pages = 0usize;
    let mut refine_partial = false;
    if refine {
        'pages: for k in 0..pages {
            let (a, b) = page_windows.get(k).copied().unwrap_or((0, 0));
            page_starts[k] = a; // 未收敛时的保守值（= 窗口下界）
            let mut spent = 0usize;
            for l in a..=b {
                // 时限检查放在循环里（不是逐字节 Instant::now()：那本身会把大窗口拖慢），
                // 每 4096 个候选点查一次 —— 保证"单页扫描本身"也不会无限跑。
                spent += 1;
                if spent % 4096 == 0
                    && refine_budget_ms > 0
                    && t1.elapsed().as_millis() >= refine_budget_ms
                {
                    refine_partial = true;
                    break 'pages;
                }
                if let Ok(p) = parse_prefix(&data, l) {
                    if p >= k + 1 {
                        page_starts[k] = l;
                        break;
                    }
                }
            }
            refined_pages += 1;
        }
    }
    let mut page_ranges: Vec<(usize, usize)> = Vec::with_capacity(pages);
    for k in 0..pages {
        let start = page_starts.get(k).copied().unwrap_or(0);
        let end = if k + 1 < pages { page_starts[k + 1] } else { consumed_total };
        page_ranges.push((start, end));
    }
    let refine_ms = t1.elapsed().as_millis();
    let refine_mode = if !refine {
        "off"
    } else if refine_partial {
        "partial"
    } else {
        "exact"
    };

    if !pages_json.is_empty() {
        let body = page_ranges
            .iter()
            .enumerate()
            .map(|(k, (s, e))| format!("{{\"page\":{},\"start\":{},\"end\":{},\"bytes\":{}}}", k + 1, s, e, e - s))
            .collect::<Vec<_>>()
            .join(",");
        // 注意：`"pages"` 这个键**仍然是页表数组**（与 t21/t23 记录的 JSON 形状保持兼容）；
        // 新增的三个字段只补充"这次页表是怎么得到的"（V-01 的诚实口径）。
        std::fs::write(
            &pages_json,
            format!(
                "{{\"xdv\":\"{}\",\"total_bytes\":{},\"pages\":[{}],\"refine_mode\":\"{}\",\"refine_resolved_pages\":{},\"page_ranges_note\":\"精确起点需 --refine（默认关，见 V-01）\"}}",
                json_escape(&xdv_path),
                total,
                body,
                refine_mode,
                refined_pages
            ),
        )
        .expect("while writing --pages-json");
    }

    // ---- pass 3：固定块大小的流式时间线（实时消费者能看到什么）----
    let shared2: Rc<RefCell<Collector>> = Rc::new(RefCell::new(Collector::default()));
    let mut parser2 = XdvParser::new(Ev { shared: shared2.clone() });
    let mut timeline: Vec<String> = Vec::new();
    let mut p2 = 0usize;
    let mut tl_step = 0usize;
    let mut last_pages = 0usize;
    while p2 < total {
        let end = (p2 + chunk).min(total);
        let (consumed, _keep) = match parser2.parse(&data[p2..end]) {
            Ok(v) => v,
            Err(e) => {
                timeline.push(format!("\"chunk@{p2}:ERR {}\"", json_escape(&e)));
                break;
            }
        };
        let seen = shared2.borrow().pages_started;
        if tl_step < 16 || seen != last_pages {
            timeline.push(format!(
                "\"bytes={} pages_started={} chars={}\"",
                end,
                seen,
                shared2.borrow().chars
            ));
        }
        last_pages = seen;
        tl_step += 1;
        if consumed == 0 {
            timeline.push(format!("\"need-bigger-buffer@{p2}\""));
            break;
        }
        p2 += consumed;
    }

    // 先取值再打印：`RefCell::borrow()` 的借用期不跨过 `println!`（否则借用检查器会报
    // `does not live long enough`）。
    let (chars, glyphs, specials, rules, native_fonts, header) = {
        let c = collect.borrow();
        (c.chars, c.glyphs, c.specials, c.rules, c.native_fonts.len(), c.header.clone())
    };
    println!(
        "{{\"case\":\"xdvscan\",\"ok\":true,\"xdv\":\"{}\",\"total_bytes\":{},\"pages\":{},\"chars\":{},\"glyphs\":{},\"specials\":{},\"rules\":{},\"native_fonts\":{},\"parsed_through_postamble_at\":{},\"chunk\":{},\"incremental_ms\":{},\"refine_ms\":{},\"refine_mode\":\"{}\",\"refine_budget_ms\":{},\"refine_resolved_pages\":{},\"steps\":{},\"header\":\"{}\",\"page_ranges\":[{}],\"stream_timeline\":[{}]}}",
        json_escape(&xdv_path),
        total,
        pages,
        chars,
        glyphs,
        specials,
        rules,
        native_fonts,
        consumed_total,
        chunk,
        scan_ms,
        refine_ms,
        refine_mode,
        refine_budget_ms,
        refined_pages,
        steps,
        json_escape(&header),
        page_ranges
            .iter()
            .map(|(s, e)| format!("[{s},{e}]"))
            .collect::<Vec<_>>()
            .join(","),
        timeline.join(",")
    );
}
