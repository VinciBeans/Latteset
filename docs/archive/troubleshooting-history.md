# 历史排障记录（已修条目，原文保留）

> 来源：原 `docs/troubleshooting.md` 中"问题**已在当前版本修复**"的条目。按「移动不删改」纪律**原文保留**
> （含原有的日期与定位叙述——归档语境里它们是史实），一行按语说明现役对应物。
> 现役排障请回 [../troubleshooting.md](../troubleshooting.md)；测试与真机验收见 [../dev/testing.md](../dev/testing.md)。

## `npm run tauri dev` 前端全白：Vite dev server 被 15.8 万个被监视文件拖死（2026-09 已修）

**现象**：`npm run tauri dev` 起来后窗口是**彻底空白**（不是首帧竞态——右键 Reload 也白）。日志看起来一切正常：

```
VITE v6.4.3  ready in 258 ms
  ➜  Local:   http://localhost:1420/
     Running DevCommand (`cargo run …`)
```

**根因（实测定位）**：vite 的 chokidar 把**整个工作区**递归纳入监视，而 `test_file/` 下有 **158,541 个文件**（vcpkg 检出、`vendor/` 824 MB、tectonic 上游源码、各类夹具与编译产物）。这种体量下 dev server **端口在监听、TCP 连接能被接受，但请求路径永远不出响应**：

| 探针 | 结果 |
|---|---|
| `netstat -ano \| Select-String ":1420"` | `[::1]:1420 LISTENING`（正常） |
| `fetch("http://[::1]:1420/")` | 连接被接受 → 无响应 → 客户端超时；服务端侧留 `CLOSE_WAIT` |
| `npx vite optimize --debug` | **正常**（`Dependencies bundled in 386.12ms`） |
| `npx vite build` | **正常** |
| `127.0.0.1:1420` | `ECONNREFUSED`（vite 只绑 `[::1]`，属正常形态，不是根因） |

即 **`vite build` / `optimize` 全好、只有 dev server 的请求路径卡住** ⇒ 排除 esbuild、排除业务代码、排除端口冲突，指向 watcher。

**处置**（`vite.config.ts` → `server.watch.ignored`）：排除不参与前端的巨型目录——

```ts
ignored: [
  "**/src-tauri/**",   // Rust 侧，vite 不参与
  "**/test_file/**",   // 本地素材区（vcpkg / vendor / 上游源码 / 夹具）
  "**/dist/**",        // 构建产物（tauri build 与 dev 并存时会自我触发）
  /\.tmpdir[\\/]/,     // 被锁定的临时目录（chokidar 未处理 error 会拖垮 vite）
]
```

**复测**：`/` → 200 / 434 B / **57 ms**；`/src/main.ts` → 200 / 8549 B / **81 ms**；`localhost:1420/src/main.ts` → 200 / **157 ms**。

**真机复验（`npm run tauri dev`）**：窗口渲染出完整三栏布局（工具条 / 资源管理器 / 编辑器 / PDF 预览 / 状态栏），dev stdout 出现 3 条 `Server handshake done.`；再经 IPC 走完整链路——`open_project`（`中文测试工程`）→ `compile_now` → `编译成功（无等待请求） draft=false` + `PDF 就绪：… pages=3 changed=3`，预览渲染出 3 页中文 PDF（40,519 B）。

**判据速查**：dev server "起得来但不响应"时，先量被监视的文件数，别急着怀疑 esbuild/端口/业务代码——

```powershell
(Get-ChildItem <repo> -Recurse -File -Force -ErrorAction SilentlyContinue).Count
```

**回归自查（加 watch ignore 时的必做项）**：ignore 加多了会**悄悄废掉 HMR**，所以两边都验一次——
改 `src/` 下的文件，dev stdout 应出现 `[vite] (client) hmr update /src/…`，且**真机窗口内容随之更新**；
在 `test_file/` 下建/删文件则**不应**产生任何 `[vite]` 行。注意：**vite 只在有客户端连着时才打 `hmr update`**，
所以"只起 `npm run dev`、不开窗口"时看不到这行，不代表 HMR 失效。

> 按语：现役 `vite.config.ts` 已带这组 `server.watch.ignored`；HMR 回归自查仍是改 watch 配置时的必做项。

## 子进程 Tectonic 的 Quick 产物**整段没有目录**：`-o tmp` 只是输出目录（2026-09 实测，已修）

**现象**：`engine=tectonic`（子进程档）下每编辑一次，预览会先跳成 **26 页**（正文少了目录）、约 4 s 后才收敛回 **28 页**——页数在 26/28 之间反复抖。xelatex / lualatex 档没有这个现象。

**根因**：Tectonic 的 `-o tmp` **只作输出目录、不作输入目录** ⇒ 单趟档**读不回**上一趟留在 `tmp/` 的 `.toc`/`.aux`，于是这一趟从零重建目录——**目录整段排不进去**。（对照：latexmk 档靠 `-output-directory` 读得回；库形态档靠 `seed_previous_intermediates` 预热内存层。三条路只有 Tectonic 子进程档没有回读机制。）

**判据（可复算，release `tectonic.exe` 0.17.0，同一份 28 页中文夹具、同一条命令形态）**：

| 档 | 耗时 | 页数 | 文本层 | 目录 |
|---|---|---|---|---|
| `-r 0`（单趟） | **657 ms** | **26** | 12,624 字 | **没有**（第 2 页只剩页码标记 `i`，正文从第 3 页起） |
| `-r 1`（两趟） | **1057 ms** | 28 | 13,570 字 | 有（占第 2–3 页） |
| 默认（收敛，Full 用） | 1495 ms | 28 | 13,570 字 | 有 |

差的 **2 页 / 946 字就是目录**。所以这不是 roadmap ㉘ 说的"目录/交叉引用**页码**落后一趟"——那是**内容缺失**，与承诺不符。

**处置**：`tectonic_command` 的 Quick 档用 `-r 1`（两趟）。代价 +400 ms，Quick 仍明显快于 Full（1057 vs 1495）。真机复验：Quick 出 **28 页 / 13,570 字**，与 Full 逐项一致，预览不再抖。

> **量这个的坑**：`scripts/validate-pdf.mjs` 本身是确定性的（同一文件连跑 5 次同值），但**别在 `tectonic` 刚退出时立刻量**——会读到明显偏小的文本层计数。只采用**可复算**的那一组。

> 按语：Quick 档现为 `-r 1`；「别在引擎刚退出时立刻量 PDF」仍是 `validate-pdf.mjs` 的使用注意。

## 外部改文件后「既重载成功、又报外部修改」：重载被当成了用户编辑（2026-09-15 已修）

**症状**（真机 2/2 复现）：文件在编辑器里开着、人没碰键盘，用外部工具写它 → 编辑器**静默重载成功**（新内容进缓冲区），状态栏却同时挂出「外部修改」。该提示的动作是 `acceptExternal`（**放弃本地**）⇒ 误报后一点击就是丢输入。

**定位**：外部重载走 `EditorPane.vue` 的 `model.setValue()`，而 Monaco 的 `setValue` **同步**触发 `onDidChangeModelContent`，应用据此 `markDirty` + `emit("change")`——重载被当成了用户编辑。于是 `editor.ts` 干净分支里「读盘 await 之后的复检」看到 `dirty` 已为真，直接 `externalConflict.add(path)`：脏分支有「再比一次磁盘内容」的判据，这一条没有。连带两个副作用：tab 误判为脏；连续模式下自动保存把**同一份内容**写回磁盘（mtime 被改）→ 磁盘又变一次 → 再触发一轮编译。

**修法**（两条都要）：
1. `EditorPane.vue`：重载写 model 时用调用栈内的开关（`applyingReload`）屏蔽这次内容变更事件——开关只可能在 `setValue` 期间为真，**结构上不可能吞掉用户输入**；
2. `editor.ts`：干净分支的复检只认内容（磁盘 == 缓冲 ⇒ 不算冲突），与脏分支对齐。

**验证**（真机，scratch 项目 `test_file/e2e/dedup-status`）：外部写两次 → 无提示、tab 无脏点、**文件 mtime 不变**（没有回写）、编辑器内容照常更新；用 `ed.trigger("mcp","type",…)` 造真实编辑 → tab 标脏 + 自动保存落盘（证明开关没吞掉用户输入）；保持脏缓冲再外部写 → 提示出现、本地输入保住、点提示后采用磁盘版。单测：`editor.spec.ts` 新增两条（见 [modules.md](../dev/modules.md) §5.5），改回旧写法时「不得误报」那条会红。

> 按语：现役回归由 `src/stores/__tests__/editor.spec.ts` 的两条用例锁定。

## `tauri dev` 下编辑触发档不肯收敛、提示还亮着：预算按发布档标定（2026-09-15）

**现象**：库形态（`npm run lib:dev`）里编辑一个 28 页项目，状态栏仍亮「引用待更新」、2 s 后还补一次 Full；同一个项目在 release 下（`cargo run --release -p latteset-tectonic --example bench -- --quick`）**1 趟就收敛、报 `Full`、无提示**。

**原因**：**不是缺陷，是标定口径**。编辑触发档的收敛预算里有一道"第 1 趟必须 <1 s"的闸门，阈值按**发布档**定；而 `dev` profile 下 C 引擎（xetex/xdvipdfmx）按 `-O0` 编译，同一趟慢 **≈4.6×**（实测同项目同改动：release 475 ms / debug 2174 ms）⇒ 闸门先于收敛被触发，退化成 ㉘ 的旧行为。

**怎么确认是这一条**（决策日志一行点名）：

```
DEBUG 编辑触发档不再追收敛：停在草稿态（交给 ㉘ 的空闲收敛） passes=1 first_pass_ms=2174 elapsed_ms=2174 first_pass_max_ms=1000 max_passes=3 budget_ms=2000
```

`first_pass_ms > first_pass_max_ms` 即命中（`RUST_LOG=debug`）。**要量真实行为必须 `--release`** —— 同 §6.4 的测量纪律（debug 下库形态数字大 6–8×）。判据与实测见 [tectonic-library-plan.md](./tectonic-library-plan.md) §6.5。

> 按语：现役形态即"按发布档标定"；在 debug 档观察到不收敛属预期，测量结论一律取 `--release`。

## release 档「点编译弹出一个终端黑框」（2026-09-16 修）

**症状**：装出来的包（release）里点「编译」会弹一个控制台黑框；`npm run tauri dev` 下**完全看不到**。

**原因**：`src-tauri/src/main.rs` 带 `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`
—— release 档是 **GUI 子系统、没有控制台**，而 Windows 默认会给"控制台子程序"**新建一个**控制台窗口
（`latexmk` / `xelatex` / `tectonic.exe` / `xdvipdfmx` / `taskkill` / `synctex` 全是控制台程序）。
debug 档父进程本身就是控制台程序（从终端起），子进程直接**继承**它的控制台 ⇒ 同一个 bug 被藏住，
这也是它只在 release 暴露的原因。

**修法**：`latteset_infra::proc::command` / `command_std`（Windows 上加 `CREATE_NO_WINDOW`），
**所有生产路径**改走它们（引擎 / latexmk / xdvipdfmx / taskkill / synctex）。它只影响"要不要分配
控制台"，stdout/stderr 仍是管道 ⇒ 日志解析、退出码、页哈希、树杀都不变。**别在别处直接
`Command::new`** —— 漏一处就漏一个黑框。

**复现/验证**（不必先打 release 包，用探针模拟"没有控制台的父进程"）：

```powershell
cargo build --release -p latteset-infra --example console-window-probe
$probe = "src-tauri\target\release\examples\console_window_probe.exe"

# ⚠ 量具必须是**可见窗口数**，不是 conhost 进程数（见下）
Add-Type @"
using System;using System.Text;using System.Collections.Generic;using System.Runtime.InteropServices;
public class WinProbe {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] static extern int GetClassName(IntPtr h, StringBuilder sb, int n);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  public static List<string> VisibleConsoles() {
    var res = new List<string>();
    EnumWindows((h, l) => {
      if (!IsWindowVisible(h)) return true;
      var sb = new StringBuilder(256); GetClassName(h, sb, 256);
      var c = sb.ToString();
      if (c == "ConsoleWindowClass" || c == "CASCADIA_HOSTING_WINDOW_CLASS") res.Add(c);
      return true;
    }, IntPtr.Zero);
    return res;
  }
}
"@

foreach ($mode in 'plain','quiet-std','quiet-tokio') {   # 建议再加一个 'none' 当噪声对照
  Start-Sleep -Seconds 4
  $before = [WinProbe]::VisibleConsoles().Count; $max = $before
  $p = Start-Process $probe -ArgumentList $mode -PassThru     # ⚠ 不能加 -NoNewWindow
  for ($i = 0; $i -lt 14; $i++) { Start-Sleep -Milliseconds 250; $c = [WinProbe]::VisibleConsoles().Count; if ($c -gt $max) { $max = $c } }
  $p.WaitForExit(); "$mode 可见控制台窗口增量 = $($max - $before)"
}
```

实测（本机，release 版探针，每档 4 轮）：`none`（对照）= **0**；`plain` = **+1 可见窗口**；
`quiet-std` / `quiet-tokio`（产品路径）= **0** ✓。

⚠ 两个量具坑（第一版就量错过，别重蹈）：
1. **别拿 `conhost` 进程数当判据**：`CREATE_NO_WINDOW` 的语义是"**分配控制台但不给窗口**"，所以进程数
   仍会 **+1**，但那是个不可见宿主、用户什么也看不到 —— 判据只能是**可见窗口**（上面那段 EnumWindows）。
2. **别把正在退出的进程算进基线**：`ping` 子进程活 3 秒，两档之间不睡够就会把上一档的残余算进 `before`，
   `max` 因此偏低（第一版 debug 探针就这样得出过假的"quiet=0"）。每档之间 `Start-Sleep 4`，
   并且**加一个 `none` 对照档**量背景噪声。

⚠ **探针必须以 `Start-Process`（不加 `-NoNewWindow`）启动**：那样它自己没有控制台、才等价于 GUI 父进程；
用 `-NoNewWindow` 会继承当前终端的控制台，把问题又藏起来（探针是 `windows_subsystem = "windows"`，
所以它的输出要用外部计数看，别指望 stdout）。

⚠ **别拿裸 `cargo build --release` 出的 exe 当"release 版"验证**：那样打出来的应用**没有内嵌前端资源**
（正规路径是 `npx tauri build`，它先跑 `beforeBuildCommand` 再嵌 `dist/`），启动后是个空白窗口、
不打开项目、也不编译 —— 用它会得出"编译没触发"的假结论（本次就绕了这一圈）。

> 按语：现役生产路径一律走 `latteset_infra::proc::command` / `command_std`；验 release 行为用 `npm run tauri build`。
> 归档原因是该条目主体是"修前定位 procedure"，产品侧已无对应现场。

## 库内嵌档下 SyncTeX 定位不可用：同步数据里没有真实源文件名（2026-09-17 定位 → **同日修复**）

**症状**：引擎切到「库内嵌」后，Ctrl+点击源码不再跳到 PDF、点 PDF 也不再回到源码 —— 用户视角就是"点了没反应"。CLI 侧的直接读数（这一条是**唯一**能把三种原因区分开的证据）：

```
latteset-cli --project <项目> forward <项目>\main.tex 100
{ "error": { "code": "Internal", "message": "SyncTeX：输出解析失败：同步数据里没有这个源文件（…\main.tex）—— 库形态产出的文件可能只有空 Input（…）" } }
```

**一眼确认**（不必编译两遍）：把 `tmp/<stem>.synctex.gz` 解开看 `Input:` 记录 ——

```powershell
$gz = "<项目>\tmp\main.synctex.gz"
$in = [IO.File]::OpenRead($gz)
$g  = New-Object IO.Compression.GZipStream($in, [IO.Compression.CompressionMode]::Decompress)
$sr = New-Object IO.StreamReader($g)
($sr.ReadToEnd() -split "`n") | Where-Object { $_ -like 'Input:*' } | Select-Object -First 8
$sr.Close(); $in.Close()
```

| 引擎形态 | `Input:1` 的样子 | `forward` 结果 |
|---|---|---|
| **子进程档**（`LATTESET_TECTONIC_LIB=0`） | `Input:1:E:\…\main.tex` ✓ | `{"ok":true,"page":8,…}` ✓ |
| **库形态**（`LATTESET_TECTONIC_LIB=1`，**修复前**） | `Input:1:texput`、`Input:2..8` 空 ✗ | ✗ "同步数据里没有这个源文件" |
| **库形态**（**修复后**，2026-09-17 起） | `Input:1:E:\…\main.tex` ✓（其余可为空，解析器跳过空名） | `{"ok":true,"page":8,…}` ✓ 与子进程档**逐字段相同** |

**原因**：库形态把**主文件内存直喂**引擎，而引擎 C 侧在 `start_input()` **之前**就调了 `open_log_file()`（后者把 jobname 定成默认的 `texput`，`xetex-xetex0.c:10874`），于是 `xetex-xetex0.c:11014` 那句"jobname 为空才用主输入名"永不命中；Rust 侧也没有补救口子（`tt_xetex_set_string_variable` 在 0.5.3 是空实现）。
**修法**（roadmap ㊷ 已落地）：收尾处把同步数据里的主输入记录补成**真实绝对路径**（`crates/latteset-tectonic/src/synctex.rs`，纯数据补丁，捕获表与 `tmp/` 镜像两处都改）——**不改引擎、不动文档结构**，PDF/`.log`/错误归属实测逐字节不变。详见 [tex-ide-roadmap-priority.md](./research/tex-ide-roadmap-priority.md) §6.10.1。
**若它又坏了**（症状复发 = 补丁没生效）：看库形态编译日志有没有 `已修补同步数据的主输入记录（㊷…）`；没有的话按上表解 gz 看 `Input:1` —— 若又变回 `texput`，说明引擎换了写法（`patch_outputs` 会打 `warn!`："找不到主输入记录"）。
**一处仍然存在的形态差异**（**不是故障**）：**目录区域**的反向点击，子进程档回到源码行 `\tableofcontents`，库形态回到生成物 `main.toc`（前端按 ㉒ 显示"此处来自自动生成的文件"）。把主输入名改回 `texput` 结果一字不变 ⇒ 与本补丁无关，是两引擎对目录盒子的记录归属不同。

**别把三种"点了没反应"混起来**（它们的修法完全不同）：

| 原因 | 判别 | 现状 |
|---|---|---|
| 同步数据里**没有真实文件名**（本条） | `Input:1:texput` / 空 | **已修**（㊷，2026-09-17）：库形态收尾补上真实路径；复发时看日志有没有那条 `已修补…` |
| 编译**正把同步数据重写** | `tmp/` 里出现 `*.synctex(busy)` 或查询偶发失败后自愈 | 退避重试（100/200/300 ms），实测能救回来 |
| **根本没编译过** | `tmp/<stem>.synctex.gz` 不存在 | 现在**立刻**报"同步数据不存在"（roadmap ㊱：此前要白等 645 ms，inverse 更差 3.13 s） |

> 按语：两档 `Input:1` 逐字段相同的实测，以及「一条真实文件名 + 同名 source 的回归」由
> `crates/latteset-tectonic` 的 synctex 单测锁定（8 条）；本条其余内容已不再是现役现场。
