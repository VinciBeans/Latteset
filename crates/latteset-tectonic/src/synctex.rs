//! 同步数据的**主输入记录修补**（roadmap ㊷）。
//!
//! ## 现象与根因（实测 2026-09-17）
//!
//! 库形态产出的 `tmp/<stem>.synctex.gz` 里主输入记录是 **`Input:1:texput`**（`Input:2..N:` 为空），
//! 而子进程档在同一点写的是 `Input:1:<项目根绝对路径>`。自解析（`latteset-infra/src/synctex.rs`）
//! 按文件找 tag 时跳过空名（`core::synctex::parse::tag_for_file`），于是**主文件永远找不到 tag**
//! ⇒ 单文件工程的 `forward`/`inverse` 全废；多文件工程只有被 `\include`/`\input` 进来的章节能定位
//! （它们的 `Input:` 记录是对的，实测 `chapters/ch01.tex` 两形态完全一致）。
//!
//! 根因在引擎的 C 侧：`tt_run_engine()` 先 `open_log_file()`（`xetex-ini.c:2945`，其中把 `job_name`
//! 置成 `"texput"`，`xetex-xetex0.c:10874`），之后才 `start_input(input_file_name)`
//! （`xetex-ini.c:3872`），于是 `xetex-xetex0.c:11014` 的 `if (job_name == 0) job_name = cur_name;`
//! 永不命中。Rust 侧也没有补救口子：`tectonic_engine_xetex` 0.5.3 的
//! `tt_xetex_set_string_variable` 是**空实现**（`xetex-engine-interface.c:47` 的 "Currently unused"）。
//!
//! ## 为什么补数据是正解
//!
//! 要改的只是同步数据里的**一个字符串**，而这份字节本来就在我们手里（[`crate::io::IoCapture`]
//! 的捕获表 + `tmp/` 镜像）。实测：把既有产物里的 `Input:1:texput` 就地换成真实绝对路径后，
//! 正反向结果与子进程档**逐字段一致**（同一点：`page 8 / x 133.76837158203125 / y 204.46743774414062`）。
//! 备选方案是包一层合成 `\input{<root>}` 让引擎自己记对名字 —— 那会多一层文件栈，
//! 行号与页归属都要重新标定（㉒/⑤ 的全部精度结论都得重做），风险远大于收益。
//!
//! 补丁是**纯数据**变换：不参与排版、不动行号、不动 `.log`，因此对错误归属零影响。
//!
//! ## 为什么落在本 crate（而不是 core / infra）
//!
//! - 缺陷只存在于**库形态**：子进程档的记录本来就是对的，补丁在这里是"就地修自己的产物"；
//! - 形态边界（ADR-0012）：本 crate 不依赖 `latteset-infra`，而 gzip 编解码只在 infra（读侧）
//!   与本 crate（写侧）需要 —— `latteset-core` 保持**不碰压缩**（`core::synctex` 只吃已解压文本）。

use std::io::{Read, Write};
use std::path::Path;

use tracing::{debug, info, warn};

use crate::io::SharedCapture;

/// gzip 魔数（判容器形态；与 `latteset-infra/src/synctex.rs` 读侧同一判据）。
const GZIP_MAGIC: &[u8] = &[0x1f, 0x8b];

/// 主输入记录的**键**：Synctex 里 tag 1 恒为主文件。
const PRIMARY_KEY: &[u8] = b"Input:1:";

/// 修补结果。
///
/// 分成三态而不是 `Option`：产物存在却**改不了**（引擎换了写法、或字节解不开）是"缺陷回来了"
/// 的信号，收尾处要能把它和"本来就不用改"分开记（见 [`patch_outputs`] 的日志）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOutcome {
    /// 已改：新字节（容器形态与原字节一致）。
    Patched(Vec<u8>),
    /// 不用改：主输入记录已经是真实路径（补丁幂等，重跑不会反复重压）。
    AlreadyCorrect,
    /// 改不了：没有主输入记录 / 不是可解的 gzip / 不是文本。
    NotApplicable,
}

/// 把同步数据里的**主输入记录**（`Input:1:<name>`）改成 `real_name`。
///
/// 只动 tag 1 那一行 —— 其余记录（`\include` 进来的章节、`{<tag>` 段标记、`Content:`、`Postamble:`
/// 与计数）逐字节原样搬运；行尾（`\n` / `\r\n` / 末行无换行）也照原样保留。
///
/// 容器：gzip 进 gzip 出；未压缩的 `.synctex` 文本原样进出（引擎当前固定写 `.gz`，
/// 未压缩形态照读侧同一口径容忍）。
pub fn patch_primary_input(sync: &[u8], real_name: &str) -> PatchOutcome {
    let gz = sync.starts_with(GZIP_MAGIC);
    let raw = if gz {
        match gunzip(sync) {
            Ok(raw) => raw,
            Err(_) => return PatchOutcome::NotApplicable,
        }
    } else {
        sync.to_vec()
    };
    let Some((start, body_end)) = primary_input_line(&raw) else {
        return PatchOutcome::NotApplicable;
    };
    if &raw[start + PRIMARY_KEY.len()..body_end] == real_name.as_bytes() {
        return PatchOutcome::AlreadyCorrect;
    }
    let mut patched = Vec::with_capacity(raw.len() + real_name.len());
    patched.extend_from_slice(&raw[..start]);
    patched.extend_from_slice(PRIMARY_KEY);
    patched.extend_from_slice(real_name.as_bytes());
    patched.extend_from_slice(&raw[body_end..]);
    if !gz {
        return PatchOutcome::Patched(patched);
    }
    match gzip(&patched) {
        Ok(bytes) => PatchOutcome::Patched(bytes),
        Err(_) => PatchOutcome::NotApplicable,
    }
}

/// 把补丁落到**两处**：捕获表（内存真相）与 `tmp/` 镜像（自解析定位读的就是它）。
///
/// 两种文件名都试（`.synctex.gz` 优先）—— 引擎写哪个我们就修哪个。
/// **失败不致命**：补丁只影响定位，不该让一次编译失败；写盘失败记 `warn!`（那是用户可见退化的唯一线索）。
pub fn patch_outputs(
    capture: &SharedCapture,
    stem: &str,
    mirror_dir: &Path,
    real_name: &str,
) -> bool {
    let gz = format!("{stem}.synctex.gz");
    let plain = format!("{stem}.synctex");
    for name in [&gz, &plain] {
        let Some(bytes) = ({
            let c = capture.lock().unwrap_or_else(|e| e.into_inner());
            c.files.get(name).cloned()
        }) else {
            continue;
        };
        let patched = match patch_primary_input(&bytes, real_name) {
            PatchOutcome::Patched(patched) => patched,
            PatchOutcome::AlreadyCorrect => {
                debug!(sync = %name, "同步数据的主输入记录已是真实路径，无需修补");
                continue;
            }
            PatchOutcome::NotApplicable => {
                warn!(
                    sync = %name,
                    bytes = bytes.len(),
                    "同步数据里找不到主输入记录（`Input:1:`）—— 引擎可能换了写法，\
                     库形态的正反向定位会退回「找不到主文件」（roadmap ㊷）"
                );
                continue;
            }
        };
        let mirror = mirror_dir.join(name);
        if let Err(e) = std::fs::write(&mirror, &patched) {
            warn!(
                path = %mirror.display(),
                "同步数据补丁写回 tmp/ 失败：内存副本已修，磁盘定位仍会找不到主文件：{e}"
            );
            return false;
        }
        {
            let mut c = capture.lock().unwrap_or_else(|e| e.into_inner());
            // 账目与捕获长度对齐（补丁改了长度）。
            c.written.insert(name.clone(), patched.len());
            c.files.insert(name.clone(), patched);
        }
        info!(
            sync = %name,
            primary_input = real_name,
            "已修补同步数据的主输入记录（㊷：库形态引擎写的是 texput）"
        );
        return true;
    }
    false
}

/// 找 `Input:1:` 行的 `(行首, 行尾不含换行)`。
///
/// 必须**按行**匹配：`Input:` 记录在第一列，而正文里可能有别的 `Input:` 字样（宏包名、路径片段）。
/// 顺带挡住 `Input:10:` 这类别的 tag（前缀要求第 8 字节是 `:`）。
fn primary_input_line(raw: &[u8]) -> Option<(usize, usize)> {
    let mut start = 0usize;
    while start < raw.len() {
        let end = match raw[start..].iter().position(|b| *b == b'\n') {
            Some(i) => start + i,
            None => raw.len(),
        };
        let mut body_end = end;
        if body_end > start && raw[body_end - 1] == b'\r' {
            body_end -= 1;
        }
        if raw[start..body_end].starts_with(PRIMARY_KEY) {
            return Some((start, body_end));
        }
        start = end + 1;
    }
    None
}

fn gunzip(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes).read_to_end(&mut out)?;
    Ok(out)
}

fn gzip(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(bytes)?;
    enc.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一份"像真的"的同步数据：主输入是引擎写的 `texput`，另有一个 `\include` 进来的章节
    /// （它的记录本来就对，补丁不许碰），外加空的 `Input:2..3:` 与 `Content:` 段。
    const SAMPLE: &str = "SyncTeX Version:1\n\
        Input:1:texput\n\
        Input:2:\n\
        Input:3:\n\
        Output:pdf\n\
        Magnification:1000\n\
        Unit:1\n\
        X Offset:0\n\
        Y Offset:0\n\
        Content:\n\
        !139\n\
        {1\n\
        [1,100:4736286,4443115:26673152,3276800,0\n\
        ]\n\
        }1\n\
        Input:9:E:/proj/chapters/ch01.tex\n\
        {9\n\
        [9,5:1234567,7654321:12345678,0,0\n\
        ]\n\
        }9\n\
        Postamble:\n\
        Count:2\n\
        !140\n";

    const REAL: &str = "E:\\proj\\main.tex";

    fn gz(text: &str) -> Vec<u8> {
        gzip(text.as_bytes()).expect("压测试数据")
    }

    fn text_of(bytes: &[u8]) -> String {
        String::from_utf8(gunzip(bytes).expect("解回来")).expect("UTF-8")
    }

    /// 主输入记录换成真实路径；**其余字节逐字节不动**（章节记录、空 Input、Content 段、计数）。
    #[test]
    fn patches_only_the_primary_record() {
        let PatchOutcome::Patched(out) = patch_primary_input(&gz(SAMPLE), REAL) else {
            panic!("应当修补");
        };
        let text = text_of(&out);
        assert!(text.contains("Input:1:E:\\proj\\main.tex\n"), "{text}");
        assert!(!text.contains("texput"), "占位名必须消失：{text}");
        assert_eq!(text, SAMPLE.replace("Input:1:texput", "Input:1:E:\\proj\\main.tex"));
    }

    /// 幂等：已经对了就不再产出新字节（重跑不会反复重压同一份产物）。
    #[test]
    fn is_idempotent_once_patched() {
        let PatchOutcome::Patched(out) = patch_primary_input(&gz(SAMPLE), REAL) else {
            panic!("第一次应当修补");
        };
        assert_eq!(patch_primary_input(&out, REAL), PatchOutcome::AlreadyCorrect);
    }

    /// 未被压缩的 `.synctex` 文本：原样进出（不额外压一层）。
    #[test]
    fn handles_uncompressed_synctex() {
        let PatchOutcome::Patched(out) = patch_primary_input(SAMPLE.as_bytes(), REAL) else {
            panic!("应当修补");
        };
        assert!(!out.starts_with(GZIP_MAGIC), "未压缩进就得未压缩出");
        assert_eq!(
            String::from_utf8(out).expect("UTF-8"),
            SAMPLE.replace("Input:1:texput", "Input:1:E:\\proj\\main.tex")
        );
    }

    /// CRLF 与"末行无换行"都要保住（同步数据是引擎按平台写的）。
    #[test]
    fn keeps_line_endings() {
        let crlf = SAMPLE.replace('\n', "\r\n");
        let PatchOutcome::Patched(out) = patch_primary_input(crlf.as_bytes(), REAL) else {
            panic!("应当修补");
        };
        let text = String::from_utf8(out).expect("UTF-8");
        assert!(text.contains("Input:1:E:\\proj\\main.tex\r\n"), "{text}");

        let no_tail = "Input:1:texput";
        let PatchOutcome::Patched(out) = patch_primary_input(no_tail.as_bytes(), REAL) else {
            panic!("末行无换行也要修");
        };
        assert_eq!(String::from_utf8(out).expect("UTF-8"), "Input:1:E:\\proj\\main.tex");
    }

    /// 空的主输入记录（`Input:1:`）同样是"引擎没记对名字"，照修。
    #[test]
    fn patches_empty_primary_record() {
        let PatchOutcome::Patched(out) =
            patch_primary_input("Input:1:\nContent:\n".as_bytes(), REAL)
        else {
            panic!("应当修补");
        };
        assert_eq!(
            String::from_utf8(out).expect("UTF-8"),
            "Input:1:E:\\proj\\main.tex\nContent:\n"
        );
    }

    /// 没有 tag 1 的同步数据不动（不伪造记录）；`Input:10:` 不算 tag 1。
    #[test]
    fn does_not_invent_a_primary_record() {
        assert_eq!(
            patch_primary_input(b"Input:10:x\nInput:2:y\n", REAL),
            PatchOutcome::NotApplicable
        );
    }

    /// 坏 gzip / 空字节：给 `NotApplicable`，**不 panic**（产物可能只写了一半）。
    #[test]
    fn tolerates_corrupt_input() {
        assert_eq!(patch_primary_input(&[], REAL), PatchOutcome::NotApplicable);
        assert_eq!(
            patch_primary_input(&[0x1f, 0x8b, 0x00, 0x01, 0x02], REAL),
            PatchOutcome::NotApplicable
        );
    }

    /// 落点：捕获表与 `tmp/` 镜像**都要**拿到补丁后的字节（定位读的是磁盘那份）。
    #[test]
    fn patch_outputs_updates_capture_and_mirror() {
        let mirror = std::env::temp_dir().join("latteset-tectonic-synctex-patch-test");
        let _ = std::fs::remove_dir_all(&mirror);
        std::fs::create_dir_all(&mirror).expect("建镜像目录");
        let capture = crate::io::new_capture();
        let sync = gz(SAMPLE);
        {
            let mut c = capture.lock().expect("锁");
            c.files.insert("main.synctex.gz".to_owned(), sync.clone());
            c.written.insert("main.synctex.gz".to_owned(), sync.len());
        }

        assert!(patch_outputs(&capture, "main", &mirror, REAL));

        let in_memory = {
            let c = capture.lock().expect("锁");
            c.files.get("main.synctex.gz").cloned().expect("捕获表里还在")
        };
        let on_disk = std::fs::read(mirror.join("main.synctex.gz")).expect("镜像已落盘");
        assert_eq!(in_memory, on_disk, "两处必须是同一份字节");
        assert!(text_of(&on_disk).contains("Input:1:E:\\proj\\main.tex\n"));
        let written = {
            let c = capture.lock().expect("锁");
            c.written.get("main.synctex.gz").copied().expect("账目在")
        };
        assert_eq!(written, on_disk.len(), "账目与捕获长度对齐");
        // 没有产物时不许凭空造文件（`main.synctex` 不存在 ⇒ 一个字都不写）。
        assert!(!mirror.join("main.synctex").exists());
        let _ = std::fs::remove_dir_all(&mirror);
    }
}
