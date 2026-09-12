# Troubleshooting

## 白屏：无 GPU 虚拟机环境的首帧呈现竞态

**现象**：`npm run tauri dev` 启动应用进程时，窗口偶发白屏（webview 页面已加载、JS 正常、devtools Console 无报错，但首帧未呈现）。**右键 → Reload 后立即正常**。

**边界**（实测）：
- 纯前端改动（Vite HMR）不触发
- Rust 改动触发的进程重启不触发
- 仅**进程冷启动**时偶发（不是每次）
- 与 `vite.svg` 的 404 无关（模板遗留文件，品牌化后已移除；当时属重启瞬间的瞬态请求）

**已尝试无效**：`WEBKIT_DISABLE_DMABUF_RENDERER=1`、`LIBGL_ALWAYS_SOFTWARE=1`、`WEBKIT_DISABLE_COMPOSITING_MODE=1`。

**处置**：白屏时右键 → Reload。开发环境怪癖，仅影响无 GPU 虚拟机；目标平台 Windows（WebView2/DirectX 硬件路径）预计不受影响，真机验证时复查。

## 幽灵窗口：WSLg 下 WebKitGTK 完全不渲染（2026-08 实测，已修复）

**现象**：`npm run tauri:dev` 进程正常启动（无 panic），窗口创建（window-state 插件有记录、任务栏有缩略图）但内容完全不渲染——静态 HTML 页面、前端代码全部排除后依旧。

**根因**：WSLg（无 GPU）与 WebKitGTK 渲染栈的版本兼容问题。特征为 `libEGL warning` + `ZINK: vkCreateInstance failed`（无 GPU 驱动时 ZINK/Vulkan 初始化失败，软件渲染兜底也未生效），且**与项目代码无关**——全新 Tauri 模板（vanilla/Vue，零业务代码）同样幽灵。

**诊断链路**（遇到同样现象时按此排查，全部实测过）：
- 进程存活、`WebKitWebProcess`/`WebKitNetworkProcess` 正常；`WEBKIT_FORCE_SANDBOX=0` 无效
- 网络层正常：`ss -tnp` 显示 webview 与 vite 已建立 `[::1]:1420` 连接，无代理
- **决定性实验**：脱离 Tauri 的最小 WebKitGTK 程序（python-gobject）同样不渲染，仅 `libEGL` 警告
- 环境特征：Arch WSL + WSLg + webkit2gtk 2.52.x + mesa 26

**已尝试无效**：`WEBKIT_DISABLE_DMABUF_RENDERER=1`、`WEBKIT_DISABLE_COMPOSITING_MODE=1`、`WEBKIT_FORCE_SANDBOX=0`、`LIBGL_ALWAYS_SOFTWARE=1`、`GDK_BACKEND=x11`、窗口位置钉主屏、CSP 置 null、静态 HTML 页面。

**修复方法（实测有效）**：
1. **升级 WSL**（Windows 侧 PowerShell/CMD）：`wsl --update`
2. **重装 WebKitGTK**（WSL 内）：`sudo pacman -S webkit2gtk-4.1`（若版本未变可先 `sudo pacman -Rns webkit2gtk-4.1` 再装）

**处置**：修复后 `npm run tauri:dev` 正常显示。若仍异常，再走 Windows 真机（WebView2）验证——目标平台不受此问题影响。

**附：WSLg 多显示器窗口跑到外接屏**：window-state 插件会保存并恢复跨屏位置；窗口“消失”时先 `rm ~/.config/com.texpresso.app/.window-state.json`。真机多显示器拔出后同样可能出现，后续可加“位置越界则居中”保护。

## GUI/端到端测试链路（WebDriver + pc-control，2026-08）

**方案**：`test_file/e2e/` 用 **WebdriverIO**（`browserName:'wry'` + `tauri:options.application`）驱动 `tauri-driver`（中介）+ `msedgedriver`（Windows，需 `msedgedriver-tool` 安装并匹配 Edge 版本，`--native-driver` 指路径）。`src/App.vue` 加了 `VITE_TEXPRESSO_PROJECT` 钩子自动打开项目（绕过原生目录弹窗，WebDriver 无法驱动）；`src/components/PreviewPane.vue` 加了重载耗时插桩。

**关键教训（均实测）**：
- **手动拉起 debug 二进制 ≠ 可用**：`npm run dev`（只起 vite）+ 手动 `target/debug/texpresso.exe`，前端**不渲染**（`button.btn.primary` 找不到）。必须 **`npm run tauri dev`**（正确构建 Rust + 起 vite + 真正调起 Tauri 窗口），问题即消失。
- **tauri-driver 不打印 "listening"**：`beforeSession` 靠字符串匹配会永久卡住；改成**轮询 127.0.0.1:4444 是否可连**（`net.connect`）。
- **只读沙箱限制**：vite/esbuild 的 worker 子进程需创建命名管道，`workspace-write` 下会 `EPERM`；需 `danger-full-access` 才能跑 `vite dev`/WebDriver/调起 WebView2 窗口。npm 缓存写 `%LocalAppData%` 也被拒，需把 `NPM_CONFIG_CACHE` 指到工作区。
- **读取前端控制台**：pc-control 的 `screenshot` 返回 base64，需解码成图片再读；devtools 快捷键（F12/Ctrl+Shift+I）需窗口聚焦才生效。窗口标题不随 `document.title` 改变（Tauri 不同步），**不要**用它做观测通道。
- **`webview_keyboard` 无法向 Monaco 编辑器插入文本（2026-08-25 实测定级）**：**不是 MCP 服务 bug**（`webview_keyboard type` 能在普通 `<input>` 上写入 `hello`，实测）；**是 Monaco（EditContext 架构）对合成事件不响应的限制**。Monaco 可编辑区是 `.native-edit-context`（DIV，`role=textbox`，走 EditContext）；`press` 只派发 `keydown/keypress/keyup`（**全部 `isTrusted:false`，且不产生 `beforeinput`/`input`**），合成键不触发浏览器默认文本插入（普通 input 上 `press` 同样不插入，实测），故 Monaco 不写入。`type` 靠 set `value` + `input` 事件，仅对原生 input/textarea 有效，且对 Monaco 唯一 textarea `.ime-text-area`（**readonly** 的 IME 缓冲）无效。**凡是涉及 Monaco 文本编辑的自动化，用真实 OS 按键（pc-control，`isTrusted:true` 触发 EditContext 插入，已实测 `Q` 写入），或改用 Monaco `executeEdits` API**（需实例，MCP 触不到）。
- **e2e 操作要点（2026-08-25 完整 e2e 实测）**：① **pc-control 打字前先 `Ctrl+Space` 关闭中文输入法**——否则 IME 会截走按键转义（实测 `% E2E_MARKER` 被 IME 吞掉/乱插）；② **状态栏「外部修改：文件名」可点击**——点击即从磁盘重载该文件（`acceptExternal`），用于外部修改冲突恢复（v1 无独立「重载」按钮，此 span 即入口）；③ 原生目录对话框流程：`Ctrl+L` 聚焦地址栏 → 粘贴路径 → Enter → 再补 `Enter`（连按确认）即可选中并关闭（实测成功打开 multifile 工程）。

**注意**：`test_file/e2e/drivers/`（msedgedriver 二进制）与 `test_file/e2e/node_modules/` 已 gitignore。

## Rust 单测：`cargo test -p texpresso`（src-tauri）在 Windows 启动即失败（2026-08）

**现象**：`cargo test -p texpresso --lib` 编译成功，但测试二进制**加载即退出**——`STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)`，任何测试都未运行。

**根因**：`texpresso_lib` 链接了 tauri/wry 的 `webview2-com`。`cargo test` 的测试二进制不做 GUI 初始化，加载阶段解析 WebView2/webview2-com 失败（`STATUS_ENTRYPOINT_NOT_FOUND`，发生在任何测试运行前）。**与业务代码无关**；`cargo check -p texpresso --tests` 能正常通过（测试代码编译无误）；本机 WebView2 Runtime 已安装（151.0.4129.107），排除运行时缺失。

**处置**：
- 核心 crate 单测走 `cargo test -p texpresso-core`（无 Tauri 依赖，运行正常，CI 用这个验证纯逻辑；本轮 97 pass）。
- 前端单测走 `npm run test`（vitest，需提权 `danger-full-access` 跑 esbuild worker；本轮 28 pass）。
- src-tauri 接线层的纯逻辑单测（`fs_impl::strip_verbatim`、`runner::root_stem/latexmk_input`、`watch::should_process/is_structural_event/normalize_event_paths`、`storage::effective/is_self_write/project_overrides_path`、`commands::pdf_path_for_root`）**可编译、逻辑已验证**，但本机无法直接 `cargo test` 执行；在能解析 WebView2 的 Windows 环境（真机宿主）再运行。
- **已穷尽尝试仍失败**：把 `webview2-com-sys-*/out/{arch}/WebView2Loader.dll` 拷到 `target/debug` **及 `target/debug/deps`（测试 exe 同目录）** 并加入 PATH；`dumpbin /imports` 显示静态导入均为系统 DLL、延迟导入仅 `VCRUNTIME140.dll`；`danger-full-access` 提权运行——均仍 `STATUS_ENTRYPOINT_NOT_FOUND`。**非沙箱权限、非 PATH、非运行时缺失**，是 Tauri v2 shell crate 测试二进制的已知 Windows 工具链限制。

## 中文文件名/路径兼容性实测（2026-09，roadmap P0-①）

**总结论**：**文件名/路径层面的中文全链路可用**（无阻塞缺陷，回归测试已固化）；唯一实测出的真实缺陷**不在路径，而在日志编码**（见下节，已修复）。

### 实测环境与样本

- TeX Live 2026（`latexmk` / `xelatex` / `pdflatex` / `synctex` 1.5），Windows，活动代码页 **65001**；并在 **`chcp 936`** 下逐项重复——**控制台代码页对结论无影响**（见下 synctex 字节实测）。
- 样本工程 `test_file/projects/中文测试工程/`（**gitignore**，需手动重建）：中文目录 + 中文文件名 `中文主文件.tex` + 中文子目录 `章节/第一章.tex`（`\include` 引入）+ `ctexart` 中文正文。

### 逐项实测结果（全部通过）

| 环节 | 结果 | 证据 |
|---|---|---|
| `latexmk` 编译 | ✅ exit 0 | `latexmk -xelatex -outdir=tmp -synctex=1 -interaction=nonstopmode 中文主文件.tex`，CP65001 与 CP936 均成功 |
| 中间产物 | ✅ 中文名正确 | `tmp/中文主文件.{log,aux,fls,toc,xdv,synctex.gz}` 全部产出 |
| **App 真机链路** | ✅ 全通 | `npm run tauri dev` + `VITE_TEXPRESSO_PROJECT` 打开中文工程 → 日志实见 `打开项目：…\中文测试工程`、`开始监视项目：…`（notify 注册成功）、`手动编译: root=…\中文主文件.tex` → 项目根产出 **`中文主文件.pdf` (40,648 B)**；`tmp/` 事件被正确忽略 |
| `synctex view`（正向） | ✅ | 中文绝对路径 `-i 5:1:E:/…/中文主文件.tex` → `Page:1` |
| `synctex edit`（反向） | ✅ | 回传 `Input:E:/…/中文测试工程/./中文主文件.tex`（中文完好） |
| `.log` 中文文件名 | ✅ | 日志内 `(./中文主文件.tex` 为 UTF-8；解析能带出中文文件名 + 行号（回归测试 `parse_log_keeps_chinese_file_name_and_line`） |
| `canonicalize` + `\\?\` 剥离 | ✅ | `\\?\E:\项目\…` → `E:\项目\…`（既有 `strip_verbatim` 对中文安全） |
| 中文应用配置目录 | ✅ | 模拟 `C:\Users\中文用户\AppData\…`：全局设置与项目覆盖 `.texpresso/settings.json` 均原子读写通过 |

**关键编码事实（字节级实测，值得记住）**：`synctex` 的 stdout **恒为 UTF-8**（`中` = `E4 B8 AD`），**与代码页无关**——CP65001 与 CP936 下抓到的字节完全一致。故现有 `String::from_utf8_lossy(&out.stdout)` 的用法**正确**（此前怀疑它会按代码页输出，已证伪）。

### 真实缺陷（已修复）：GBK 源文件 + pdflatex → `.log` 含非法 UTF-8 → 错误列表拿不到任何信息

**现象**：源 `.tex` 是 GBK 编码（中文用户遗留文件很常见）时，`.log` 可能**不是合法 UTF-8**，而 `runner` 用严格 `read_to_string` 读日志 → 读取失败 → 前端只拿到「编译失败且无法读取日志（…）：stream did not contain valid UTF-8」，**真正的 TeX 错误一条都看不到**——恰好击中调研里最高频的痛点（错误诊断）。

**实测差异（同一 GBK 源，两个引擎行为不同）**：

| 引擎 | `.log` 是否合法 UTF-8 | 说明 |
|---|---|---|
| `xelatex` | ✅ 合法 | 引擎自己把非法字节替换为 U+FFFD 写进日志（`Invalid UTF-8 byte or sequence at line 3 replaced by U+FFFD.`） |
| **`pdflatex`** | ❌ **非法** | 实测含 **73 个非法字节**（首个 `0xD6`，offset 1893）——原始 GBK 字节被直接回显 |

**修复**：新增 `texpresso_core::log_parser::decode_log(bytes)`——能严格解就严格解，否则 lossy（非法字节 → U+FFFD）；`FileSystem` trait 增 `read_to_string_lossy`（默认退化为严格读取，`TokioFs` 覆盖为 lossy 解码），`runner` 读 `.log` 改走它。

**修复后判据（实测）**：同一个非法日志 lossy 解码后，`! LaTeX Error: Invalid UTF-8 byte sequence.` 与 `l.3 ` 等 **ASCII 骨架完好**，`parse_log` 仍能给出消息 + 行号——诊断信息"有损"远好于"没有"。回归测试 `log_parser::decode_tests::invalid_bytes_do_not_lose_error_skeleton` 锁定该行为。

**App 端到端复现与验证（已完成）**：夹具 `test_file/projects/中文GBK工程/`（gitignore，需重建）——`主文件.tex`（纯 ASCII）+ `子目录/gbk.tex`（**GBK 编码**，含 `\undefinedcommandhere`）+ `.texpresso/settings.json` 覆盖 `{"compile":{"engine":"pdflatex"}}`。步骤：

1. `VITE_TEXPRESSO_PROJECT=…\中文GBK工程 npm run tauri dev`；
2. 触碰 `主文件.tex`（改 mtime）经 watch 触发编译；
3. 观察 dev stdout。

**实测输出（修复后）**：

```
DEBUG 构造编译请求: root=…\中文GBK工程\主文件.tex engine=PdfLaTeX
DEBUG 编译失败：已从 .log 解析出错误条目 count=9 log=…\tmp\主文件.log
```

即：非法 UTF-8 日志被容错解码后**成功解析出 9 条错误送达前端**。修复前该路径会退化为 `编译失败且无法读取日志（…）：stream did not contain valid UTF-8`（`warn!` 分支），错误列表**一条都没有**。该日志行为本次一并补上（此前编译失败在 stdout 完全不可见）。

> 注：把"文件名编码"与"内容编码"两个变量隔离——根文件与子文件名均为 ASCII，GBK 只出现在子文件**内容**里；中文**文件名/路径**的验证由 `中文测试工程` 夹具覆盖。

### 未覆盖 / 下一步

- **GUI 目视项（需 tauri server MCP 会话补做）**：pdf.js 经 asset 协议加载中文路径 PDF 的渲染、SyncTeX 高亮/跳转的可视确认。机制上 Tauri 的 `convertFileSrc` 会做 percent-encoding、asset scope 为 `**`，但**本次未目视确认**，不计入已验证。
- **已知未修**：**编辑** GBK 源文件（`read_file` 严格 UTF-8）会失败并返回英文 IO 错误。本次范围是文件名/路径，未改该行为；若要支持"打开并转码显示 GBK 源文件"，需单独设计（含保存时的编码回写策略）。
- 本机 `cargo test -p texpresso`（src-tauri）无法运行（见上一节），故 src-tauri 侧新增的中文用例（`fs_impl` / `runner` / `storage` / `commands`）**仅编译校验通过**（`cargo check -p texpresso --tests`），待真机宿主执行；core 侧 10 条中文用例已实际运行通过。
