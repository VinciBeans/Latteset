# 排障（Troubleshooting）

操作与环境坑速查：每条按「现象 → 原因 → 处理」写，读者是遇到问题的人（使用者与开发者同一套现场）。
真机验收与测试链路见 [dev/testing.md](./dev/testing.md)；**已修**条目的原文保留在 [archive/troubleshooting-history.md](./archive/troubleshooting-history.md)。

## 现象 → 小节（速查）

| 现象 | 小节 |
|---|---|
| 白屏 / 界面不渲染（无 GPU、WSLg、改 `App.vue` 结构后 HMR） | 无 GPU 虚拟机 / WSLg / HMR 白屏 |
| 编译报错但看不到任何信息；中文文件名 / 路径 / 编码 | 中文文件名与路径 / GBK 源 |
| `\include{子目录}` 报 `I can't write on file` | `\include{子目录/文件}` + `-output-directory` |
| XeLaTeX 每次编译慢 4–5 秒 | fontconfig 字体缓存失效 |
| LuaLaTeX 切过去失败 / 第一次特别慢 | LuaLaTeX 的两个环境前提 |
| 编译期实时反馈不「实时」 | 非 TTY 4KB 块缓冲 |
| 错误列表文件名偶尔错一章 | TeX 日志的 `)` 与 `(` 同行 |
| 外部改 `settings.json` 有时不生效 | 非原子写 settings.json |
| `cargo build -p latteset-server` 报拒绝访问 | 常驻 `latteset-mcp.exe` 锁二进制 |
| Rust 单测 `cargo test -p latteset` 加载即退出 | src-tauri 的 WebView2 链接限制 |
| Linux CI 上路径语义用例红 | Windows 路径语义别当跨平台契约 |
| 更名后设置回到默认 / 项目根文件覆盖没了 | 更名后的配置目录迁移 |
| 探针文档含中文 `exit 12` | 探针含中文时不能用 pdflatex |
| 引擎「找不到文件 / 改了没生效」 | 引擎读了什么：四个通道 |
| headless（CLI / MCP） | [cli.md](./cli.md) + 常驻二进制锁 |
| 前端探针 / 脚本的坑（WS 探针、PowerShell 自动变量、format、`<select>`、死 CSS） | 前端与脚本杂项 |
| 精简 bundle 不含 `beamer.cls` | 实验室 bundle 缺 beamer |

## 无 GPU 虚拟机白屏：首帧呈现竞态

**现象**：`npm run tauri dev` 启动应用进程时，窗口偶发白屏（webview 页面已加载、JS 正常、devtools Console 无报错，但首帧未呈现）。**右键 → Reload 后立即正常**。

**边界**（实测）：

- 纯前端改动（Vite HMR）不触发
- Rust 改动触发的进程重启不触发
- 仅**进程冷启动**时偶发（不是每次）
- 与 `vite.svg` 的 404 无关（模板遗留文件的瞬态请求）

**已尝试无效**：`WEBKIT_DISABLE_DMABUF_RENDERER=1`、`LIBGL_ALWAYS_SOFTWARE=1`、`WEBKIT_DISABLE_COMPOSITING_MODE=1`。

**处理**：白屏时右键 → Reload。这是无 GPU 虚拟机环境的怪癖；目标平台 Windows（WebView2/DirectX 硬件路径）不受影响。

## WSLg 幽灵窗口：WebKitGTK 完全不渲染

**现象**：`npm run tauri:dev` 进程正常启动（无 panic），窗口创建（window-state 插件有记录、任务栏有缩略图）但内容完全不渲染——静态 HTML 页面、前端代码全部排除后依旧。

**根因**：WSLg（无 GPU）与 WebKitGTK 渲染栈的版本兼容问题。特征为 `libEGL warning` + `ZINK: vkCreateInstance failed`（无 GPU 驱动时 ZINK/Vulkan 初始化失败，软件渲染兜底也未生效），且**与项目代码无关**——全新 Tauri 模板（vanilla/Vue，零业务代码）同样幽灵。

**诊断链路**（全部实测过）：

- 进程存活、`WebKitWebProcess`/`WebKitNetworkProcess` 正常；`WEBKIT_FORCE_SANDBOX=0` 无效
- 网络层正常：`ss -tnp` 显示 webview 与 vite 已建立 `[::1]:1420` 连接，无代理
- **决定性实验**：脱离 Tauri 的最小 WebKitGTK 程序（python-gobject）同样不渲染，仅 `libEGL` 警告
- 环境特征：Arch WSL + WSLg + webkit2gtk 2.52.x + mesa 26

**已尝试无效**：`WEBKIT_DISABLE_DMABUF_RENDERER=1`、`WEBKIT_DISABLE_COMPOSITING_MODE=1`、`WEBKIT_FORCE_SANDBOX=0`、`LIBGL_ALWAYS_SOFTWARE=1`、`GDK_BACKEND=x11`、窗口位置钉主屏、CSP 置 null、静态 HTML 页面。

**处理（实测有效）**：

1. **升级 WSL**（Windows 侧 PowerShell/CMD）：`wsl --update`
2. **重装 WebKitGTK**（WSL 内）：`sudo pacman -S webkit2gtk-4.1`（若版本未变可先 `sudo pacman -Rns webkit2gtk-4.1` 再装）

按上两步修复后 `npm run tauri:dev` 正常显示。若仍异常，改用 Windows 真机（WebView2）验证——目标平台不受此问题影响。

**附：WSLg 多显示器窗口跑到外接屏**：window-state 插件会保存并恢复跨屏位置；窗口"消失"时先 `rm ~/.config/com.latteset.app/.window-state.json`。真机多显示器拔出后同样可能出现，后续可加"位置越界则居中"保护。

## 改 `App.vue` 的 `<script setup>` 结构后 HMR 白屏：刷新一下就好

**现象**：给 `App.vue` 加一个 import + 在 setup 里调一次新 composable，Vite 只报 `hmr update /src/App.vue`，随后**窗口整个空白**（只剩 body 底色，工具栏/编辑器都不在），console 只有两条空载荷 `ERROR {}`。

**原因**：Vue 的 HMR 对 `<script setup>` 的结构性变化（新增顶层 import/调用）只能做"重新挂载"，而这次中间态没有回到可用状态；`main.ts` 里被搬走的 Monaco 主题定义同理。

**处理**：`location.reload()`（或右键 → Reload）重新引导即可，**不是代码缺陷**——刷新后一切正常（含 `index.html` 的内联首帧脚本）。排查时先看 dev stdout 的 `hmr update` 行确认改的是哪个文件，别急着怀疑自己的改动。

## src-tauri 的 `cargo test` 在 Windows 启动即失败

**现象**：`cargo test -p latteset --lib` 编译成功，但测试二进制**加载即退出**——`STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)`，任何测试都未运行。

**根因**：`latteset_lib` 链接了 tauri/wry 的 `webview2-com`。`cargo test` 的测试二进制不做 GUI 初始化，加载阶段解析 WebView2/webview2-com 失败（`STATUS_ENTRYPOINT_NOT_FOUND`，发生在任何测试运行前）。**与业务代码无关**；`cargo check -p latteset --tests` 能正常通过（测试代码编译无误）；本机 WebView2 Runtime 已安装（151.0.4129.107），排除运行时缺失。

**处理**：

- 纯逻辑单测走 `cargo test -p latteset-core`（无 Tauri 依赖，运行正常；CI 用这个验证纯逻辑）。
- 前端单测走 `npm run test`（vitest）。
- 基础设施层用例（`fs` / `runner` / `watch` / `storage`）随 ADR-0010 迁到 `latteset-infra` 后**本机可正常运行**：`cargo test -p latteset-infra`（单测）+ `cargo test -p latteset-infra -- --ignored`（真实 latexmk/synctex 集成用例：成功、内容错误、超时树杀、取消、中文路径双向）。仍留在 src-tauri 的只剩 `commands::pdf_path_for_root`（纯函数，受同一 WebView2 链接限制）。
- **已穷尽尝试仍失败**：把 `webview2-com-sys-*/out/{arch}/WebView2Loader.dll` 拷到 `target/debug` **及 `target/debug/deps`（测试 exe 同目录）** 并加入 PATH；`dumpbin /imports` 显示静态导入均为系统 DLL、延迟导入仅 `VCRUNTIME140.dll`；提权运行——均仍 `STATUS_ENTRYPOINT_NOT_FOUND`。**非沙箱权限、非 PATH、非运行时缺失**，是 Tauri v2 shell crate 测试二进制的已知 Windows 工具链限制。
- **推论（DTO 改动后怎么刷新 `src/bindings.ts`）**：`export_bindings` 这条捷径在本机不可用（同一个 `0xc0000139`）→ **起一次 `npm run tauri dev`**：debug 构建启动时会自动重新导出 `src/bindings.ts`（`lib.rs` 里 `#[cfg(debug_assertions)]` 的 export）。

完整测试分工见 [dev/testing.md](./dev/testing.md)。

## Linux CI 上路径语义用例红：Windows 路径语义被写进跨平台用例

**现象**：本机（Windows）173 项全绿，GitHub Actions 的 `ubuntu-latest` 上 `synctex::classify::tests::windows_separators_and_case` 失败，172 passed / 1 failed：

```
left:  OutsideProject("E:\\proj\\chapters\\intro.tex")
right: Source("E:\\proj\\chapters\\intro.tex")
```

**根因**：`normalize_lexically` / `classify_inverse_target` 的路径比较是**词法级**的，吃的是 `std::path::Path` 的**平台原生**分隔符语义——Windows 上 `\` 是分隔符（`E:\proj\chapters\intro.tex` 能剥出项目内相对路径），Unix 上 `\` 只是普通字符，整串是**一个文件名**，`starts_with(root)` 为假 → 判 `OutsideProject`。**产品逻辑无误**（Windows 首发，synctex 返回的就是 Windows 路径），红的是把 Windows 专属结论写成跨平台契约的用例。

**处理**（`crates/latteset-core/src/synctex/classify.rs`）：

- 跨平台部分改用 `std::path::MAIN_SEPARATOR` 拼路径，「项目内源码」与「盘符大小写不 case-fold」在两端都被断言到；
- 反斜杠形态按平台分叉：`#[cfg(windows)]` 断言 `Source`，`#[cfg(not(windows))]` 断言 `OutsideProject`（锁死"Linux 上绝不误判成可打开的源码"）。

**验证**：本机 `cargo test -p latteset-core` → 173 passed；`rustup target add x86_64-unknown-linux-gnu` 后 `cargo check -p latteset-core --target x86_64-unknown-linux-gnu --tests` 通过（非 Windows 分支可编译）。

**规则**：core 单测要能在 Linux CI 上跑，**别把 Windows 路径形态（`\`、`E:\`）当跨平台契约**——要么用 `MAIN_SEPARATOR` 拼，要么 `#[cfg]` 分叉并显式写出两端期望值。（`latteset-infra` 的 `runner` / `storage` 用例含同类 Windows 字面量：如 `root_stem(Path::new(r"C:\proj\main.tex"))` 断言 `"main"`——Unix 上 `file_stem()` 得到 `C:\proj\main`，断言必失败（**未在 Linux 实跑，仅读码判定**）；纳入 CI 的 infra 档之前需先过一遍。）

## 中文文件名与路径：全链路可用

**总结论**：**文件名/路径层面的中文全链路可用**（无阻塞缺陷，回归测试已固化）；唯一实测出的真实缺陷**不在路径，而在日志编码**（见下一节）。

**实测环境与样本**：TeX Live 2026（`latexmk` / `xelatex` / `pdflatex` / `synctex` 1.5），Windows，活动代码页 **65001**；并在 **`chcp 936`** 下逐项重复——**控制台代码页对结论无影响**。样本工程 `test_file/projects/中文测试工程/`（**gitignore**，需手动重建）：中文目录 + 中文文件名 `中文主文件.tex` + 中文子目录 `章节/第一章.tex`（`\include` 引入）+ `ctexart` 中文正文。

| 环节 | 结果 | 证据 |
|---|---|---|
| `latexmk` 编译 | ✅ exit 0 | `latexmk -xelatex -outdir=tmp -synctex=1 -interaction=nonstopmode 中文主文件.tex`，CP65001 与 CP936 均成功 |
| 中间产物 | ✅ 中文名正确 | `tmp/中文主文件.{log,aux,fls,toc,xdv,synctex.gz}` 全部产出 |
| **App 真机链路** | ✅ 全通 | `npm run tauri dev` + `VITE_LATTESET_PROJECT` 打开中文工程 → 日志实见 `打开项目：…\中文测试工程`、`开始监视项目：…`（notify 注册成功）、`手动编译: root=…\中文主文件.tex` → 项目根产出 **`中文主文件.pdf` (40,648 B)**；`tmp/` 事件被正确忽略 |
| `synctex view`（正向） | ✅ | 中文绝对路径 `-i 5:1:E:/…/中文主文件.tex` → `Page:1` |
| `synctex edit`（反向） | ✅ | 回传 `Input:E:/…/中文测试工程/./中文主文件.tex`（中文完好） |
| `.log` 中文文件名 | ✅ | 日志内 `(./中文主文件.tex` 为 UTF-8；解析能带出中文文件名 + 行号（回归测试 `parse_log_keeps_chinese_file_name_and_line`） |
| `canonicalize` + `\\?\` 剥离 | ✅ | `\\?\E:\项目\…` → `E:\项目\…`（既有 `strip_verbatim` 对中文安全） |
| 中文应用配置目录 | ✅ | 模拟 `C:\Users\中文用户\AppData\…`：全局设置与项目覆盖 `.latteset/settings.json` 均原子读写通过 |

**关键编码事实（字节级实测）**：`synctex` 的 stdout **恒为 UTF-8**（`中` = `E4 B8 AD`），**与代码页无关**——CP65001 与 CP936 下抓到的字节完全一致。故 `String::from_utf8_lossy(&out.stdout)` 的用法**正确**（曾怀疑它会按代码页输出——已证伪）。

**GUI 目视复验**（tauri server MCP 驱动真实窗口，夹具 `test_file/projects/中文测试工程`）：

| 项 | 结果 | 证据 |
|---|---|---|
| **pdf.js 经 asset 协议加载中文路径 PDF 的渲染** | ✅ | `webview_screenshot`：标题「中文路径兼容性测试」、作者/日期、**目录三项中文条目**、章节正文与公式 `E = mc²` 全部正常渲染；3 页连续分页（`1 / 3`）；控制台 `[preview] reload#1 中文主文件.pdf pages=3 bytes=40648 fetch=9ms parse=32ms render=54ms total=94ms pagesRendered=3`——**fetch 走 asset 协议无 404/编码错误** |
| **SyncTeX 反向跳转（PDF → 源码）** | ✅ | `webview_interact` 点 PDF 正文 → 编辑器切回 `中文主文件.tex` 且光标停在 **Ln 10**（`公式测试：$E = mc^2$。`） |

**点 PDF 目录区会映射到生成文件**（`中文主文件.toc`）：产品行为是生成产物/项目外文件**一律不打开**——先就近回落真实源码（`y ±40/80pt` 探测），回落到就跳并提示「已回落到最近的源码」，落空则只给提示「此处来自自动生成的文件 …，没有对应的源码行」。中文文件名在两种情况下都正确解析——这也反证反向 SyncTeX 的中文链路是通的。

**已知未修**：**编辑** GBK 源文件（`read_file` 严格 UTF-8）会失败并返回英文 IO 错误。若要支持"打开并转码显示 GBK 源文件"，需单独设计（含保存时的编码回写策略）。

## GBK 源文件 + pdflatex → `.log` 非法 UTF-8 → 错误列表拿不到信息

**现象**：源 `.tex` 是 GBK 编码（中文用户遗留文件很常见）时，`.log` 可能**不是合法 UTF-8**；若按严格 `read_to_string` 读日志 → 读取失败 → 前端只拿到「编译失败且无法读取日志（…）：stream did not contain valid UTF-8」，**真正的 TeX 错误一条都看不到**——恰好击中最高频的痛点（错误诊断）。

**实测差异（同一 GBK 源，两个引擎行为不同）**：

| 引擎 | `.log` 是否合法 UTF-8 | 说明 |
|---|---|---|
| `xelatex` | ✅ 合法 | 引擎自己把非法字节替换为 U+FFFD 写进日志（`Invalid UTF-8 byte or sequence at line 3 replaced by U+FFFD.`） |
| **`pdflatex`** | ❌ **非法** | 实测含 **73 个非法字节**（首个 `0xD6`，offset 1893）——原始 GBK 字节被直接回显 |

**现役行为**：`latteset_core::log_parser::decode_log(bytes)`——能严格解就严格解，否则 lossy（非法字节 → U+FFFD）；`FileSystem` trait 的 `read_to_string_lossy`（`TokioFs` 覆盖为 lossy 解码）由 `runner` 读 `.log` 时使用。

**判据（实测）**：同一个非法日志 lossy 解码后，`! LaTeX Error: Invalid UTF-8 byte sequence.` 与 `l.3 ` 等 **ASCII 骨架完好**，`parse_log` 仍能给出消息 + 行号——诊断信息"有损"远好于"没有"。回归测试 `log_parser::decode_tests::invalid_bytes_do_not_lose_error_skeleton` 锁定该行为。App 端到端复现：夹具 `test_file/projects/中文GBK工程/`（gitignore，需重建）——`主文件.tex`（纯 ASCII）+ `子目录/gbk.tex`（**GBK 编码**，含 `\undefinedcommandhere`）+ `.latteset/settings.json` 覆盖 `{"compile":{"engine":"pdflatex"}}`；`VITE_LATTESET_PROJECT=…\中文GBK工程 npm run tauri dev` 后触碰 `主文件.tex` 经 watch 触发编译，dev stdout 给出：

```
DEBUG 构造编译请求: root=…\中文GBK工程\主文件.tex engine=PdfLaTeX
DEBUG 编译失败：已从 .log 解析出错误条目 count=9 log=…\tmp\主文件.log
```

即非法 UTF-8 日志被容错解码后**成功解析出 9 条错误送达前端**。

> 注：把"文件名编码"与"内容编码"两个变量隔离——根文件与子文件名均为 ASCII，GBK 只出现在子文件**内容**里；中文**文件名/路径**的验证由 `中文测试工程` 夹具覆盖。

## `\include{子目录/文件}` + `-output-directory`：中间目录里必须先有同名子目录

**现象**：真实学位论文模板 `thesis-real-hithesis`（TeX Live 自带样例，已复制进 bench fixture）用产品完全相同的命令冷编译，跑约 **4 分钟**后在主文件第 106 行报错：

```
! I can't write on file `body/introduction.aux'.
\@include ...mmediate \openout \@partaux "#1.aux"
l.106 \include{body/introduction}
(Press Enter to retry, or Control-Z to exit; ...)
Please type another output file name
! Emergency stop.
```

随后 **`latexmk`/`perl` 进程并没有退出**（实测挂了 10 分钟、CPU 累计仅 0.5s/7.7s），于是产品侧看到的是「超时」而不是「内容错误」。

**根因（已用最小工程复现）**：`\include{body/introduction}` 要写 `tmp/body/introduction.aux`，而 **xelatex 不会创建输出目录的子目录**——`tmp/body/` 不存在 → `\openout` 失败。

```powershell
# 最小复现（tmp 存在但 tmp/sub 不存在）
xelatex -interaction=nonstopmode -synctex=1 -output-directory=tmp main.tex   # main.tex: \include{sub/part}
# → ! I can't write on file `sub/part.aux'.  / ! Emergency stop.  / exit 1
```

**为什么合成多文件档（`multifile`）没事**：**latexmk 会在第一趟失败后把子目录建出来并自动重跑一趟**，所以 `tmp/chapters/`、`tmp/sections/` 是 latexmk 建的、第二趟就成功了（对照实测：同一最小工程用 `latexmk -xelatex -outdir=tmp ...` → 第一趟报同样的错、但 `tmp/sub` 被创建、第二轮 `Output written ... / All targets are up-to-date`、exit 0）。hithesis 这一档没有恢复，**嫌疑指向它自带的 `latexmkrc`（覆写 `$pdflatex`、内建 `--shell-escape` 与尾部 `;cp`）与 `-outdir=tmp` 的相互作用**——目前只做到"确认不是超时问题"，**尚未定位到可复现的 rc 最小组合**（用一个精简 rc 复刻其关键行跑最小工程，仍能正常恢复）。

**对产品的两个直接结论**：

1. **超时诊断必须先看日志里的致命错误**：这种"报错后不退出"的运行，产品只会走到超时；若只按"慢/卡住"提示，用户永远修不好。超时诊断因此优先报日志里的那条错误，且**不给"提高超时"按钮**（提高超时救不了它）。
2. **可用绕过（已实测）**：把主文件里的 `\include{子目录/文件}` 改成 `\input{子目录/文件}`——`\input` 不写子文件 `.aux`，因此不需要 `tmp/子目录/`：

```powershell
xelatex -interaction=nonstopmode -synctex=1 -output-directory=tmp main.tex   # main.tex: \input{sub/part}
# → Output written on tmp/main.pdf (1 page). / exit 0（tmp/sub 仍不存在也没关系）
```

诊断文案里已写这条建议（`DiagnosisKind::AuxWriteFailed`）。

**latexmk 的自动恢复不是普遍成立的。** 自建夹具（10 章 ctexbook，章节用 `\include{chapters/cNN}`，`tmp/` 为空）走 **Full**（latexmk 完整收敛）时并没有恢复：日志停在"写不出中间文件 `chapters/c01.aux`" + `Emergency stop`，latexmk 反复重跑同一处失败，直到 **120s 超时**（上面"为什么合成多文件档没事"里的 `tmp/sub` 被创建，在这里没有发生）。两次实验的差异未定位到判据（文档类/中文宏包/页数都可能相关），但"latexmk 会自动建目录并重跑"这句话**不能当作保证**。顺带这也是一次流式反馈的实战价值演示：实时错误在该次编译的 **68s** 就报出了这条诊断，而超时终态要到 **127s** 才出现（提前 59s）。

## XeLaTeX 每次编译慢 4–5 秒：fontconfig 字体缓存失效

**现象**：连 `\documentclass{article}` + 一个字符的极简文档，`xelatex` 也要 **4.8 s**（正常 ~0.8 s）；`xelatex --version` 单是启动就 0.5–0.7 s。**同期 `lualatex` 完全正常**（同一份文档 1.2 s），Lua 层微基准与 `cmd /c exit` 也都在基线 ⇒ **不是 CPU 降频、不是磁盘、不是我们的管线**。

**根因**：XeTeX 在 Windows 上用 **fontconfig** 解析 native font（连它基座 format 里的默认字体都走这条路；见 [fmt 研究](./archive/research/no-fork-alternatives.md) §3.2），而 fontconfig 的缓存文件**坏了或丢了** → 每次运行都**重新扫描 3,640 个系统字体**。

**判据（两条，几秒钟就能验）**：

- `fc-list` 计时：正常 ~0.7 s；故障时 **4.2–4.7 s**
- `fc-cache -v`：直接报 `invalid cache file: C:/texlive/<年>/texmf-var/fonts/cache/<hash>-x64.cache-9`

**修法**：跑一次 `fc-cache -f`（TeX Live 自带 `fc-cache.exe`）。修好后本机实测：极简文档 **4799 → 790 ms**、`min-zh` 夹具 **5380–5765 → 1371 ms**，`fc-list` **4.2 s → 0.7 s**。

**缓存为什么会坏**：断电/强杀把缓存文件写了一半、磁盘满、杀软隔离、或手工清了 temp。**只有 XeLaTeX 慢、LuaLaTeX 正常**是本问题的特征（LuaLaTeX 不走 fontconfig）。

> 对产品的影响：默认引擎是 **Tectonic**（不走 fontconfig，见 [ADR-0014 修订 1](./adr/0014-tectonic-first.md)），所以只有**切到 XeLaTeX** 的用户会在这方面每次编译白等 4 s。诊断入口就上面两条命令。

**沙箱注意**：缓存写在 `C:/texlive/<年>/texmf-var/fonts/cache`，**在工作区之外** ⇒ 受限沙箱下 `fc-cache` 会 `Permission denied`，缓存永远建不起来、**每次编译都慢**。需要在正常权限（或提权）下跑一次 `fc-cache -f`，或让应用在正常权限下自建。

## LuaLaTeX：切过去之后编译失败 / 第一次特别慢

引擎是设置面板里可选项（`tectonic` / `xelatex` / `lualatex` / `pdflatex`），但 LuaLaTeX 有两个**环境前提**，不满足时的报错都长得不像"引擎问题"：

**① `luaotfload` 需要一个可写的字体缓存目录**（不满足 → 引擎第一句就退出）：

```
luaotfload | load : FATAL ERROR
luaotfload | load :   × Failed to load "fontloader" module "basics-gen".
luaotfload | load :     × ".../luaotfload-init.lua:301: system : no writeable cache path, quiting"
!  ==> Fatal error occurred, no output PDF file produced!
```

- 缓存落点是 **`TEXMFVAR`**（不是 `TEXMFCACHE`——实测只设后者不够），默认 `~/.texlive<年>/texmf-var`。TeX Live 装在只读位置、或用户配置目录不可写（沙箱/受限账户）时会踩到。
- 处理：把 `TEXMFVAR` 指到一个可写目录再启动应用（应用会把它继承给引擎）；或给该目录写权限。

**② 第一次编译要建字体库**：全新缓存下第一次编译实测 **25.8 s**（产出 23.6 MB / 35 个文件，`luatex-cache/generic/{names,otl,luatexja}`），之后回落到正常耗时。设置面板的引擎提示已写明这一点。

**③ 速度**：同一份 28 页中文论文单趟 **1870 ms（XeLaTeX）vs 6468 ms（LuaLaTeX）**，约 3.2–3.5×；且中文 LuaLaTeX **不产 DVI/XDV**（`luatexja`：`DVI output is not supported in LuaTeX-ja`），所以页级复用的"跳过转换/只重绘变化页"在它下面**不发生**（下游按"无法判定"保守全量刷新，功能正常、只是没有那层优化）。完整数据见 [现代引擎实测](./archive/research/modern-engines-zh.md)。

**④ 探针别忘 `-halt-on-error`**：LuaLaTeX 的启动期 Lua 错误在 `nonstopmode` 下会**无限刷屏**——实测把一份日志写到 **226 MB** 才被掐掉。所有非交互探针都要带 `-halt-on-error`。

## 引擎输出在非 TTY 下是 4KB 块缓冲：实时反馈要尾随 `.log`

**现象**：把引擎的 stdout/stderr 接成管道（`Stdio::piped()`）做"边编译边报错"后，短文档的**错误与页标记会一起在进程快结束时才到**——看起来像流式没生效。

**实测**：一次 1471ms 的编译里，首条流式错误出现在 **1394ms**（几乎等于进程退出时刻）。原因是 C 运行时的块缓冲：**stdout/stderr 不是 TTY 时按 4KB 缓冲**，TeX 只在缓冲满（约每 20 页）或退出时才 flush。

**对策（现役行为）**：实时通道以 `tmp/<stem>.log` 为主力——**`.log` 是按页 flush 的**，每 200ms 尾随一次即可稳定拿到"排到第几页""刚出现的致命错误"；管道只作补充（两条来源共用同一状态机做页码取最大 + 错误指纹去重）。结论：**"接个管道"不够**，短文档/早错场景下管道几乎给不出提前量。

**验证入口**：`cargo test -p latteset-infra -- --ignored live_errors`（断言首个流式错误距编译结束 ≥100ms；只接管道时这条会红）。

## 错误列表的文件名偶尔错一章：TeX 日志的 `)` 与 `(` 会打在同一行（未修）

**现象**：错误实际写在 `ch_05.tex` 第 164 行，列表显示 `./ch_04.tex:164`，点击跳到 `ch_04.tex`。

**根因**：TeX 日志把"关闭上一个文件 + 打开下一个文件"打在**同一行**：

```
[64]) (./ch_05.tex
```

而 `log_parser::scan` 的文件栈只认**行首** `(` 入栈、行首 `)` 出栈 → `ch_05` 没入栈、`ch_04` 被弹出，此后该文件的错误都归到 `ch_04`。终态与流式共用同一解析器，两条路径都会错。修法要按字符顺序做括号配对（独立任务，记在 [dev/modules.md](./dev/modules.md) §12.1）。

## 外部（非原子）写 settings.json：watcher 会读到半截文件

**现象**：用 PowerShell `Set-Content` / 记事本改 `.latteset/settings.json`（或全局 `settings.json`），应用**有时不响应**——日志里能看到：

```
DEBUG watch 原始事件: [".latteset\settings.json"] kind=Modify(Any)
WARN 项目设置解析失败，忽略外部修改：….latteset\settings.json      ← 读到的是"截断后、写入前"的空文件
DEBUG watch 原始事件: [".latteset\settings.json"] kind=Modify(Any)  ← 第二次事件
```

**根因**：这类写入是"**截断 → 写入 → 关闭**"三步，watcher 会收到**两个** Modify 事件，中间存在文件为空/被占用的窗口：第一次读拿到空内容（JSON 解析失败），第二次读可能撞上共享冲突（`read_to_string` 直接失败）。旧实现两种情况都只是"忽略这次修改"（后者连日志都没有），用户侧就是「改了没反应」。

**现役行为**：`handle_settings_change` 为 **3 次 × 150ms 短重试**（每次重试重新读 + 重新解析），最终仍失败才打警告。**验收要点**：必须用非原子写法复现（`Set-Content` 即可），用"先写临时文件再 rename"的原子替换测不出这条。

**另注**：应用自己写设置走的是原子写（`atomic_write` + 自写盘 hash 过滤），不受此影响。

## 常驻 `latteset-mcp.exe` 锁二进制：`cargo build -p latteset-server` 报拒绝访问

**现象**：接了 DSH 的 `mcp-latteset`（或任何常驻的 MCP 客户端）之后，重新构建 headless 会失败：

```
error: failed to remove file `E:\Works\tex-presso\src-tauri\target\debug\latteset-mcp.exe`
Caused by: 拒绝访问。 (os error 5)
```

**根因**：`latteset-mcp.exe` 正**作为常驻进程运行**（stdio MCP server 由 harness 拉起后一直活着），Windows 锁定已加载的可执行文件 → 链接器无法替换它。与代码无关，`cargo build -p latteset`（GUI 应用）不受影响，因为它不产这个二进制。

**处理（按代价排序）**：

1. **只验 lib / 不产二进制**：`cargo test -p latteset-server --lib`（协议之外的所有单测都在 lib 里）；
2. **换 target 目录构建一次**（本机实测可用，代价是依赖图重编一遍）：
   ```powershell
   $env:CARGO_TARGET_DIR='<某个不在 src-tauri 下的目录>'; cargo test -p latteset-server
   ```
   ⚠️ **别把它放在 `src-tauri/` 里面**——`tauri dev` 的文件监视会看到那些 `.fingerprint` 变化并触发重建，放工作区外或 `%TEMP%`；
3. 在 DSH 里临时移除 `mcp-latteset` 行（或重启 DSH）再构建。

## 更名 TeXPresso → Latteset：配置目录与项目设置目录都变了

**背景与决策**：原因、命名核查与完整标识符映射见 [ADR-0011](./adr/0011-rename-to-latteset.md)。这里只记操作层面的坑。

**受影响的持久化位置**（改名**不**做自动迁移，见 ADR-0011「破坏性变更」）：

| 位置 | 旧 | 新 |
|---|---|---|
| 应用配置目录（Windows） | `%APPDATA%\com.texpresso.app` | `%APPDATA%\com.latteset.app` |
| 项目内设置 | `.texpresso/settings.json` | `.latteset/settings.json` |
| CLI/MCP 配置目录环境变量 | `TEXPRESSO_CONFIG_DIR` | `LATTESET_CONFIG_DIR` |
| 前端自动打开项目钩子 | `VITE_TEXPRESSO_PROJECT` | `VITE_LATTESET_PROJECT` |

**现象**：更名后启动应用，设置回到默认值、窗口位置丢失；旧项目里的 `.texpresso/settings.json` 不再被读取（相当于"根文件覆盖没了"）。

**处理**：旧配置目录仍在磁盘上，可手动迁移（在项目/应用都停掉后执行）：

```powershell
# 全局设置与窗口位置
Move-Item "$env:APPDATA\com.texpresso.app\*" "$env:APPDATA\com.latteset.app\" -Force
# 某个项目的设置（在项目目录内）
Rename-Item .texpresso .latteset
```

**本项目内的配套事实**：`test_file/projects/{multifile,中文GBK工程,多候选工程}/.latteset/` 是现役位置；这些目录被 `.gitignore` 覆盖，**不在版本库内**，重建夹具时要按新名创建。真机证据：启动日志 `MCP Bridge plugin initialized for 'Latteset' (com.latteset.app)`、运行的是 `target\debug\latteset.exe`、启动后自动出现 `%APPDATA%\com.latteset.app\settings.json`（identifier 生效的直接证据）、`open_project` 日志 `打开项目：…\multifile（根文件 Some(…\main.tex)，候选 1 个）` + `开始监视项目`。

**另一个坑：文档里不要跟着改「上游项目」的名字**。文档描述的上游 [let-def/texpresso](https://github.com/let-def/texpresso) **必须保持原拼写**——批量替换会造出不存在的 URL（`github.com/let-def/latteset`）和错误的源码符号名（上游真实符号是小写 `texpresso_protocol.c` / `texpresso_fork_with_channel`，不是 `TeXpresso_protocol.c`）。同理 [texpresso-live-rendering-roadmap.md](./archive/texpresso-live-rendering-roadmap.md) 的**文件名保留原名**（它是"对上游的通读"）。

## 探针文档含中文时不能用 pdflatex（附一条被证伪的假设）

**现象**：用 PowerShell 生成探针 `.tex` 后 `latexmk -pdf` **exit 12**，文档内容看起来完全正常。

**真实根因**：探针里含**中文正文**，而 `-pdf`（pdflatex）不支持 CJK → 编译失败。换 `-xelatex` 即 exit 0。这与"引擎选错 = 首屏一堆不可读错误"是同一个现象。

**⚠️ 被证伪的假设（记录以免重走）**：一度判断是 `Set-Content -Encoding UTF8` 写入了 **UTF-8 BOM**。实测证伪——

| 写法 | 头 6 字节 | 结果 |
|---|---|---|
| `Set-Content -Encoding UTF8` | `5C 64 6F 63 75 6D`（`\docum`，**无 BOM**） | exit 0（纯 ASCII 内容） |
| `[System.IO.File]::WriteAllText(..., UTF8Encoding($false))` | `5C 64 6F 63 75 6D` | exit 0 |

本机为 **pwsh 7.6.5**：`-Encoding UTF8` 默认即为**无 BOM**（`utf8NoBOM`）。**BOM 不是原因**；把两次差异归因于 BOM 是错的，真正的差异是**内容里有中文**。

**处理**：探针文档含中文时用 `xelatex`/`lualatex`；只有在 **Windows PowerShell 5.1**（非本机 `pwsh`）上才需要担心 `Set-Content -Encoding UTF8` 的 BOM 问题。

> 与 AGENTS.md §4 的「PowerShell 转义」是同类排查场景：反斜杠过度转义、编码、**引擎选择**都会表现为 exit 12，先确认是哪一个再改。

## headless（CLI / MCP）怎么排障

用法、命令表、退出码语义与 harness 接线见 [cli.md](./cli.md)。这里只留现场坑：

1. **PowerShell 会吃掉 `$`**：直接把含公式的 LaTeX 当命令行参数传，`$E = mc^2$` 的 `$...$` 被 PowerShell 当变量展开 → 文件里剩下 `E = mc^2`（无数学模式）→ 编译报 `! Missing $ inserted.`。**处理**：用 stdin 写——
   ```powershell
   $body = @'
   \section{测试}
   能量 $E = mc^2$。
   '@
   $body | latteset-cli --project <dir> write chapters/intro.tex -
   ```
   （单引号 here-string = 不求值；`-` 表示从 stdin 读内容。）
2. **`write` 不会替你建目录**：父目录不存在 → 报 `NotFound` 并说明"父目录必须已存在"（与 GUI `save_all` 同契约）。先建目录，或先写已存在的目录下的文件。
3. **配置目录默认与 GUI 同一份**（Windows `%APPDATA%\com.latteset.app`），CLI 改了设置 GUI 也会看到；要隔离（测试/CI）用 `--config-dir <dir>` 或 `LATTESET_CONFIG_DIR`。
4. **同一项目不要同时用 GUI 和 CLI 编译**：两路 latexmk 抢同一个 `tmp/`。约定 headless 独占。
5. **MCP 侧自检**：协议没握手成功时先手工喂一行看回什么——
   ```powershell
   '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}' |
     latteset-mcp --project <dir>
   ```
   应回 `result.protocolVersion` + `capabilities.tools` + `instructions`。日志（含每一次工具调用）走 **stderr**，stdout 只有协议报文——排查时别把两者混在一个管道里。
6. **`latteset-mcp.exe` 常驻则不可替换**：见「常驻 `latteset-mcp.exe` 锁二进制」一节。

## 怎么查「引擎到底读了什么 / 为什么找不到文件」

排"改了文件没生效""缺文件找不到"时，四个通道都是现成的（结论与实测见 [g1-read-interception-feasibility.md](./archive/research/g1-read-interception-feasibility.md)）：

| 想知道 | 用什么 | 开关 | 坑 |
|---|---|---|---|
| 引擎这次**实际打开了**哪些文件 | `tmp/<stem>.fls` | latexmk **默认**开 `-recorder`（**Quick 直调 `xelatex` 不带**，要用得补参数） | 只记**成功打开**的；**不含 `.bib`**（bibtex 是独立进程） |
| 产物**依赖**哪些文件（含 mtime/size/**md5**、含 bibtex 步骤） | `tmp/<stem>.fdb_latexmk` | latexmk 默认写 | 只有 **Full** 才更新；首编之前不存在；依赖里**含系统字体**（如 `msyh.ttc` 19.7 MB） |
| kpathsea **成功打开**了什么（**带秒级时间戳**） | `TEXMFLOG` | `$env:TEXMFLOG="$PWD\texmf.log"` 后编译 | 与 `.fls` 一样只记成功；覆盖到 `ls-r`/`texmf.cnf`/`.fmt` 层 |
| **失败的查找尝试**（在哪些目录找过 X、结论是什么） | `KPATHSEA_DEBUG`（stderr） | `$env:KPATHSEA_DEBUG="32"`（实测：`32` 是"查找轨迹"的最小集合，147 行；`-1` 全开 798 行） | 输出进 **stderr**、**≈1 MB/趟**（真实论文档）；见下面那条耦合 |
| 一条命令出依赖报告 | — | `node scripts/fls-report.mjs <tmp/main.fls> --fdb <tmp/main.fdb_latexmk>` | 会单独列出"只在 `.fdb_latexmk`、不在 `.fls` 的源依赖"（`.bib` 就在这类里） |

```powershell
# 缺包/缺文件探针：看它到底在哪些目录找过（$env:KPATHSEA_DEBUG 只对本次进程生效）
$env:KPATHSEA_DEBUG = "32"
xelatex -interaction=nonstopmode -output-directory=tmp a-missing-pkg.tex 2> kpse.err
Select-String -Path kpse.err -Pattern 'searching for|returning from generic search' | Select-Object -First 6
```

**两条与产品行为直接相关的结论**（都实测过）：

1. **触发面 = 项目内任何带扩展名的非忽略文件**（roadmap ㉜ 已落地，判据在 core `is_compile_trigger`，见 [dev/modules.md](./dev/modules.md) §7 / §3.3）：改被 `\bibliography` 引用的 `.bib`、图片、`.cls` 等真输入都会触发编译。**仍未做的是精确失效**（roadmap ㉝）：改**未被引用**的 `.tex`（以及 `notes.md` 这类非输入文件）会白编译一次——判据 `.fls` / `.fdb_latexmk` 已在磁盘、`scripts/fls-report.mjs` 已就绪但未接线。
2. **别常开 `KPATHSEA_DEBUG`**：它的输出走 **stderr**，而应用的流式错误通道也在读 stderr——1 MB 的 `kdebug:` 行会灌进解析缓冲；只在做单次诊断性编译时开。

## 前端与脚本杂项（WS 探针 / PowerShell / `<select>` / 死 CSS）

### WS 探针脚本的两条自查

「WS 直驱真机」的探针（`node scripts/editor-report.mjs --eval-file <file>`）很好用——它自己连 MCP Bridge 的 9223，不需要 MCP 会话在场。但有两条**不是产品缺陷、却极难自查**的坑：

1. **探针脚本不能以 `//` 注释开头**：应用的执行器把脚本**包进括号**求值，行注释开头会让整段解析失败——而且失败是**静默**的：返回 `null` 而不是 error（脚本里的 `console.log` 也看不到）。**让第一行就是 `(async () => {`**；`Session.eval` 已对 `//`/`/*` 开头的脚本补前导换行兜底。
2. **别用 mtime 判"某条命令有没有跑"**：验证"跳过 `xdvipdfmx`"时用 `tmp/<stem>.pdf` 的 mtime 当判据，结果被**空闲收敛**干扰——Quick 成功后 2s 的收敛 Full 走 latexmk，它自己会重写该文件，看起来像"Quick 没跳过"。**改用日志行判定**（`跳过 xdvipdfmx 转换与 PDF 拷贝`）才看清真相。

**一条实例只允许一个**：端口 1420/9223 被占时新实例会**静默连到旧实例**（日志里看到的项目可能不是你以为的那个）。起实例前先确认端口空闲。

**⚠ 别用 `Test-NetConnection 127.0.0.1 -Port 1420` 判**：vite **只绑 IPv6 回环**（`[::1]:1420`），IPv4 回环上是 `ECONNREFUSED`——而 `Test-NetConnection` 走的是 IPv4，于是在 dev server **完全健康**时也一律报失败。用 `netstat` 看监听：

```powershell
netstat -ano | Select-String ":1420"     # 期望一行 [::1]:1420 LISTENING；无 LISTENING 才是真没起
```

`Get-NetTCPConnection` 同样不可靠（会给出假阴性）。`[::1]:1420` 的 `TIME_WAIT` 行是正常的客户端残留，不代表有人在监听。

### PowerShell 自动变量撞名 + TeX Live 造 format 的三个口令

**① 变量名撞自动变量**（PowerShell 变量名**大小写不敏感**；实测同一天踩了三次，报错都不指向真因）：

| 写法 | 症状 |
|---|---|
| 函数参数 `$Args`（撞 `$args`） | `& xelatex @Args` 变成"**一个参数都不传**"地跑 xelatex ⇒ 工具报用法错、产物为空 |
| param `$Samples = 3` + 局部 `$samples = @()` | `无法将类型"System.Object[]"的值转换为类型"System.Int32"` |
| param `$Out = "..."` + 局部 `$out = [ordered]@{}` | `无法对类型为"System.String"的对象进行索引` |

**处理**：脚本里避开 `args / out / samples / input / error / matches / pid / host / home` 这类名字；函数内局部变量加前缀（`$LatexArgs` / `$report` / `$runs`）。

**② 在 TeX Live 2026 上造自定义 format（`mylatexformat`）的正确口令**：

- `xetex -initialize …` 是 **MiKTeX** 写法；TeX Live 用 **`xetex -ini`**，并且**必须带 `-etex`**（否则 `! LaTeX requires e-TeX`）；
- `mylatexformat.ltx` 要传**裸文件名**（让 kpathsea 找）；给绝对路径会 `Please type another input file name` 然后什么都 dump 不出来；
- PowerShell 里带 `&` 的参数（`"&xelatex"`）走 `cmd /c "…"` 最稳（`&` 是 PS 的调用运算符）；
- ⚠ **但即便口令全对，本机造出来的 format 加载即崩**（`xelatex -fmt=<name>` 退出码 `-1073741819` = `0xC0000005` 访问违例，日志 0 字节；**最小 `article` 同样崩**）⇒ 别在这条路上投入。完整侦察（两条路线、四个导言区、字节对拍）见 [tex-ide-roadmap-priority.md](./archive/research/tex-ide-roadmap-priority.md) §6.12。

### 原生 `<select>` 的弹层宽度 = 最长选项的宽度

**症状**：设置 → 引擎页的「TeX 引擎」下拉，弹层**横向冲出设置面板**（选项文本写成 `引擎名 — 整句说明`，Tectonic 那条约 100 字，弹层就按它撑宽）。

**根因**：`<select>` 自身的宽度由 CSS 定（这里是 424 px），但**弹层是原生渲染的**，宽度按最长 `<option>` 的文本算 —— CSS 管不到它。

**处理**：**别把说明写进 `<option>` 文本**。选项只留名字（实测：最长 8 字符 ⇒ 弹层不可能超过 select 自身宽度），说明放到 select 下方的 `.field-hint`（与「主题」「编译模式」两组同一范式）。面板里的 `<select>` 都按这条写。

**验证口径**：`webview_screenshot` **拍不到原生弹层**（WebView2 的 select popup 是独立窗口，截图与被点前逐像素相同 ⇒ 别拿截图当"弹层正常"的证据）。用几何证明：读 `Array.from(sel.options).map(o => o.textContent)` 取最长项字符数 + `sel.getBoundingClientRect().width`。

### Vue SFC：写在 `</style>` 之后的规则是死 CSS，构建不会报错

**症状**：`FileTree.vue` 的新建入口 `＋` 紧贴标题文字，`margin-left: auto` 看着"没生效"；行内输入框呈浏览器默认样式。

**根因**：`.new-file / .new-row / .new-input / .new-error` 这一整段被写在了 **`</style>` 之后**（文件顶层、任何 block 之外）。SFC 编译期**不报错**、`vue-tsc` 与 `vite build` 全过 —— 那 30 行只是被丢弃，元素自然一条规则都没吃到。`margin-left:auto` 本来是对的。

**怎么自查**（一条命令，扫全仓）：

```powershell
Get-ChildItem src -Recurse -Filter *.vue | ForEach-Object {
  $lines = Get-Content $_.FullName
  $idx = ($lines | Select-String -Pattern '^</style>' | Select-Object -Last 1).LineNumber
  if ($idx -and $idx -lt $lines.Count) {
    $tail = ($lines[$idx..($lines.Count-1)] | Where-Object { $_.Trim() -ne '' })
    if ($tail.Count -gt 0) { "⚠ $($_.Name)：</style> 之后还有 $($tail.Count) 行：$($tail[0])" }
  }
}
```

**处理**：把规则挪回 `<style scoped>` 内。判据不看截图看几何（`webview_execute_js` 读 `getBoundingClientRect` 与 `getComputedStyle(btn).marginLeft`）：按钮右缘应等于面板右缘减内边距、`marginLeft` 应是**算出来的 px 值**（`auto` 被解析掉）而不是 `auto`。

## 实验室的精简 bundle 不含 `beamer.cls`：选「幻灯片」会编不过（非缺陷）

**症状**：向导里选「幻灯片」建出的骨架，在本机（引擎 = Tectonic 库内嵌 + `LATTESET_TECTONIC_BUNDLE=test_file/tectonic-bundle`）编译**失败得很快（0.35 s）**，错误列表一条：

```
! LaTeX Error: File `beamer.cls' not found.
```

**原因**：`test_file/tectonic-bundle` 是**为离线复现手工裁出来的最小 bundle**（424 个文件）——`ctexart.cls`/`book.cls`/`article.cls`/`amsmath.sty` 都在，**`beamer.cls` 不在**。骨架本身没问题：同一份 `slides.tex` 用 **TeX Live 2026 的 `xelatex`** 编译得到 `Output written on slides.pdf (1 page)`、exit 0；上游标准（网络）bundle 也含 beamer。

**处理**：

- **不要去改骨架**（`core::newfile` 的 beamer 分支是对的），也**不要**为了这个把 beamer 塞进精简 bundle —— 那会把"最小离线 fixture"变成又一个宏包仓库；
- 要在本机验幻灯片这条路，就把引擎临时切到 **XeLaTeX**（TeX Live 自带 beamer），或在 `settings.json` 里把 `tectonic.bundle` 指向上游/完整 bundle；
- 顺带记住操作顺序：`LATTESET_TECTONIC_LIB=1` 是**显式覆盖**（不受"只有 engine=tectonic 才走库形态"那道闸门约束）⇒ **设了它，在设置面板里换成 XeLaTeX 也仍然是 Tectonic 库形态**（实测：日志照旧 `库形态失败…`、`.log` 里还是 Tectonic 的 `LaTeX2e <2021-11-15>`）。要让引擎选择真正生效，得去掉该环境变量再起 dev。
