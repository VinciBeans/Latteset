# Troubleshooting

## 白屏：无 GPU 虚拟机环境的首帧呈现竞态

**现象**：`npm run tauri dev` 启动应用进程时，窗口偶发白屏（webview 页面已加载、JS 正常、devtools Console 无报错，但首帧未呈现）。**右键 → Reload 后立即正常**。

**边界**（实测）：
- 纯前端改动（Vite HMR）不触发
- Rust 改动触发的进程重启不触发
- 仅**进程冷启动**时偶发（不是每次）
- 与 `vite.svg` 的 404 无关（模板遗留文件的瞬态请求）

**已尝试无效**：`WEBKIT_DISABLE_DMABUF_RENDERER=1`、`LIBGL_ALWAYS_SOFTWARE=1`、`WEBKIT_DISABLE_COMPOSITING_MODE=1`。

**处置**：白屏时右键 → Reload。开发环境怪癖，仅影响无 GPU 虚拟机；目标平台 Windows（WebView2/DirectX 硬件路径）预计不受影响，真机验证时复查。

## 幽灵窗口：WSLg 下 WebKitGTK 完全不渲染（无 GPU 的 Linux/WSL 桌面）

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

**处置**：按上两步修复后 `npm run tauri:dev` 正常显示。若仍异常，改用 Windows 真机（WebView2）验证——目标平台不受此问题影响。

**附：WSLg 多显示器窗口跑到外接屏**：window-state 插件会保存并恢复跨屏位置；窗口“消失”时先 `rm ~/.config/com.texpresso.app/.window-state.json`。真机多显示器拔出后同样可能出现，后续可加“位置越界则居中”保护。

## GUI/端到端测试链路（WebDriver + pc-control，2026-08）

**方案**：`test_file/e2e/` 用 **WebdriverIO**（`browserName:'wry'` + `tauri:options.application`）驱动 `tauri-driver`（中介）+ `msedgedriver`（Windows，需 `msedgedriver-tool` 安装并匹配 Edge 版本，`--native-driver` 指路径）。`src/App.vue` 提供 `VITE_TEXPRESSO_PROJECT` 钩子自动打开项目（绕过原生目录弹窗，WebDriver 无法驱动）；`PreviewPane.vue` 带重载耗时插桩。

**关键教训（均实测）**：
- **手动拉起 debug 二进制 ≠ 可用**：`npm run dev`（只起 vite）+ 手动 `target/debug/texpresso.exe`，前端**不渲染**（`button.btn.primary` 找不到）。必须 **`npm run tauri dev`**（正确构建 Rust + 起 vite + 真正调起 Tauri 窗口），问题即消失。
- **tauri-driver 不打印 "listening"**：`beforeSession` 靠字符串匹配会永久卡住；改成**轮询 127.0.0.1:4444 是否可连**（`net.connect`）。
- **只读沙箱限制**：vite/esbuild 的 worker 子进程需创建命名管道，`workspace-write` 下会 `EPERM`；需 `danger-full-access` 才能跑 `vite dev`/WebDriver/调起 WebView2 窗口。npm 缓存写 `%LocalAppData%` 也被拒，需把 `NPM_CONFIG_CACHE` 指到工作区。
- **读取前端控制台**：pc-control 的 `screenshot` 返回 base64，需解码成图片再读；devtools 快捷键（F12/Ctrl+Shift+I）需窗口聚焦才生效。窗口标题不随 `document.title` 改变（Tauri 不同步），**不要**用它做观测通道。
- **`webview_keyboard` 无法向 Monaco 编辑器插入文本（2026-08-25 实测定级）**：**不是 MCP 服务 bug**（`webview_keyboard type` 能在普通 `<input>` 上写入 `hello`，实测）；**是 Monaco（EditContext 架构）对合成事件不响应的限制**。Monaco 可编辑区是 `.native-edit-context`（DIV，`role=textbox`，走 EditContext）；`press` 只派发 `keydown/keypress/keyup`（**全部 `isTrusted:false`，且不产生 `beforeinput`/`input`**），合成键不触发浏览器默认文本插入（普通 input 上 `press` 同样不插入，实测），故 Monaco 不写入。`type` 靠 set `value` + `input` 事件，仅对原生 input/textarea 有效，且对 Monaco 唯一 textarea `.ime-text-area`（**readonly** 的 IME 缓冲）无效。**凡是涉及 Monaco 文本编辑的自动化，用真实 OS 按键（pc-control，`isTrusted:true` 触发 EditContext 插入，已实测 `Q` 写入），或改用 Monaco `executeEdits` API**（需实例，MCP 触不到）。
- **e2e 操作要点（2026-08-25 完整 e2e 实测）**：① **pc-control 打字前先 `Ctrl+Space` 关闭中文输入法**——否则 IME 会截走按键转义（实测 `% E2E_MARKER` 被 IME 吞掉/乱插）；② **状态栏「外部修改：文件名」可点击**——点击即从磁盘重载该文件（`acceptExternal`），用于外部修改冲突恢复（v1 无独立「重载」按钮，此 span 即入口）；③ 原生目录对话框流程：`Ctrl+L` 聚焦地址栏 → 粘贴路径 → Enter → 再补 `Enter`（连按确认）即可选中并关闭（实测成功打开 multifile 工程）。

**注意**：`test_file/e2e/drivers/`（msedgedriver 二进制）与 `test_file/e2e/node_modules/` 已 gitignore。

## 真机验收清单（tauri server MCP 驱动）

> 用途：把"改完代码怎么确认真机没问题"固化成可重复的步骤。**性能基准的编译侧由 `scripts/bench.mjs` 自动测**，
> 本清单负责**只有真实窗口才能验的部分**（预览耗时、点击链路、目视渲染）。前置：`npm run tauri dev`（需提权）
> + `driver_session action=start`。

**第 0 步（易踩）**：应用**重新构建/重启后**必须 `driver_session action=stop` → `action=start` 重建会话——
`pluginVersion` 是建会话时抓取的元数据，不重建会一直显示旧值并误判"升级没生效"。

| # | 检查项 | 操作 | 期望 |
|---|---|---|---|
| 1 | 项目打开 | `manage_window list` 确认窗口 → `webview_screenshot` | 标题栏显示项目绝对路径；左栏文件树非空 |
| 2 | 错误列表（roadmap ④） | 点「编译」→ `webview_dom_snapshot` scoped `.error-list` | 条目两行式（原因 + `→` 建议）；头部「已诊断 N」；原始 `.log` 在 `title` |
| 3 | 错误跳转 | `webview_find_element .entry` 取几何 → `webview_interact` 点第 2 条 → 读 `.cursor-pos` | 状态栏行号变为该条目的 `:行号` |
| 4 | 根文件选择器（P0-②-1） | 打开多候选工程 → `webview_dom_snapshot` scoped `.picker-panel` → 点 `.file-row` | 弹窗列出候选（title 为绝对路径）；点选后弹窗关闭、`.needs-root` 匹配数归 0 |
| 5 | 编译出 PDF | 点 `.btn.primary` → `webview_wait_for .page-wrap canvas` → `webview_screenshot` | 预览渲染出页面；页计数 `n / N` 正常 |
| 6 | **预览耗时（基准）** | `webview_execute_js` 读 `window.__previewLastReload` | 得到 `{fetch,parse,render,total,pagesRendered}`；与 `scripts/bench.mjs` 的输出拼成端到端 |
| 7 | 反向 SyncTeX | `webview_find_element .page-wrap canvas` 取页几何 → 点击正文区 → 读状态栏 | 编辑器切到对应 `.tex` 且行号≈点击句所在行（`multifile` 实测点第 5 页正文 → `chapters/intro.tex` **Ln 3**，跨文件）。**点目录区**不再打开生成的 `.toc`（roadmap ㉒ 已修）：工具条出现「此处来自自动生成的文件 main.toc…」或「已回落到最近的源码（main.tex:37）」提示 |
| 8 | 中文路径渲染（P0-①） | 用 `中文测试工程` 夹具重复 1/5 | 中文标题/目录/正文/公式正常渲染，`fetch` 无 404 |
| 9 | 编辑期草稿 + 空闲收敛（㉘） | 点「编译」建产物 → **在编辑器里敲一行**（见下「如何在应用内输入」）→ 记录状态栏时间线 | `排版中…` → `就绪` + **「引用待更新」** → 约 2s 后再次 `排版中…`（空闲收敛的 Full）→ 提示消失；dev stdout 同 cycle 有 `Quick 编译（单趟直调引擎）` 与 `Full 编译（完整 latexmk 收敛）` |
| 10 | 超时诊断 + 一键提超时重试（㉕） | 设置面板把超时改 **5s** → 清掉 `tmp/` 制造首编 → 点「编译」→ 读 `.error-list .entry` | 状态栏「失败·超时」；列表一条：原因含「已排版到第 N 页…在推进」（或「还没出现任何页面输出」）+ 建议 + 按钮**「提高到 900s 并重试」**（首编跳档）；点按钮后 `get_settings` 超时变为 900、编译启动并转「就绪」、条目消失。**注意**：这一项会把全局 `settings.json` 的超时改掉，验完记得改回 120 |
| 11 | SyncTeX 生成产物回落（㉒） | 点 PDF **目录区**（`multifile` 第 3 页中部，x≈20% 页宽 / y≈65–85% 页高）→ 读 `.sync-note` 与标签页 | 工具条提示「此处来自自动生成的文件 main.toc（…）」或「已回落到最近的源码（main.tex:37，向下探测）」；**标签页不得出现 `main.toc`**；提示约 5s 后自动消失 |
| 12 | 正向 SyncTeX 高亮（⑳，`webview_interact` 无修饰键） | 用 Vite 预打包 URL 动态 import Monaco → `ed.setPosition(...)` → 对编辑器 DOM 派发 `mousedown/mouseup/click`（带 `ctrlKey: true`）→ 读 `.highlight` 的 `display/left/top` | 恰好一个 `.highlight` 为 `display: block` 且落在目标页；编辑器 `onMouseDown(ctrlKey)` 是真实入口，派发等价于用户 Ctrl+点击 |

**取预览耗时的一行命令**（第 6 步的具体形态）：

```js
// webview_execute_js 的 script 参数
(() => JSON.stringify(window.__previewLastReload))()
```

### 如何在应用内输入文字（Monaco）

**问题**：`webview_keyboard action=type/press` 敲不进 Monaco（返回 `Illegal invocation`，或按了 Ctrl+End/Enter 后光标纹丝不动）；Monaco 已启用 **EditContext API**（`typeof window.EditContext !== "undefined"`），`textarea` + `document.execCommand("insertText")` 也无效（返回 `false`、textarea 仍为空）。**Monaco 实例也没挂在 window 上**（无 `window.monaco`）。

**可行做法**：Vite dev 把 `monaco-editor` 预打包成一个模块，**按同一 URL 再 import 一次会拿到同一个模块实例**（模块注册表按 URL 去重），于是 `monaco.editor.getEditors()` 能取到**页面里正在用的那个编辑器**：

```js
// webview_execute_js 的 script 参数（URL 从 performance 里取，?v= 哈希必须一致）
(async () => {
  const url = performance.getEntriesByType("resource").map(r => r.name)
    .find(n => /deps\/monaco-editor\.js/.test(n));
  const monaco = await import(/* @vite-ignore */ url);
  const ed = monaco.editor.getEditors()[0];
  const m = ed.getModel();
  ed.setPosition({ lineNumber: m.getLineCount(), column: m.getLineMaxColumn(m.getLineCount()) });
  ed.trigger("mcp", "type", { text: "\n% hello" });   // 走 Monaco 自己的输入路径
  return m.getValue().slice(-40);
})()
```

这会让 `onDidChangeModelContent` 真实触发 → 应用的 `@change` / 防抖自动保存 / watch / 编译链路全部按真机路径走（比"从外部改文件"多验了编辑器这一环；外部改文件还会额外触发状态栏「外部修改」提示，属干扰）。

**另一个坑：`webview_execute_js` 的 JS 执行有 ~3s 上限**（`timeout` 参数不影响它）。要观测一段 5–15s 的 UI 时间线，别写成"轮询到超时再返回"，改为**装一个常驻探针再分段读**：

```js
// 第一次调用：装探针（立即返回）
const bar = document.querySelector(".status-bar"); const log = []; const t0 = Date.now();
const snap = () => ({ t: Date.now() - t0, phase: bar.querySelector(".phase")?.textContent?.trim(),
                      draft: bar.querySelector(".draft") ? "引用待更新" : null });
log.push({ ...snap(), note: "install" }); window.__dshProbe = log;
new MutationObserver(() => { const s = snap(); const l = log[log.length-1];
  if (l.phase !== s.phase || l.draft !== s.draft) log.push(s); })
  .observe(bar, { childList: true, subtree: true, characterData: true });
// 之后隔几秒用 () => window.__dshProbe 读一次即可
```

**编译产物的时间戳也能当证据**：`tmp/main.fdb_latexmk` 的 mtime 只在 **latexmk** 跑过时才推进（Quick 单趟直调引擎不碰它），故「Quick 确实没走 latexmk」与「空闲收敛确实跑了 latexmk」都能用它与 `tmp/main.xdv` 的 mtime 对比来核实。

### SyncTeX 精度怎么量（一条命令）

```bash
node scripts/synctex-report.mjs                    # 默认三组样本（multifile / beamer工程 / bench/large）
node scripts/synctex-report.mjs --tolerance=5      # 换个"跳到位"容差
node scripts/synctex-report.mjs --projects=<dir> --recompile
```

它按 `\section`/`\chapter`/`\frametitle`/`\begin{frame}`/`\label`/`\part` 取样本，逐点做「正向 → 反向」往返，报告成功率与行号差。**当前基线**：正向/反向/同文件 34/34、跳到位 ≤3 行 31/34（≤5 行 34/34）；分档与解释见 [design.md](./design.md) §预览。

**beamer 的 2–4 行偏移不是解析 bug**：把取块规则从「第一个 `Output` 块」换成「最小 H」「首个 H≤40」，**往返结果完全一致**（三组样本逐一相同）——偏移来自 beamer/主题的 synctex 记录粒度。同理，`\only<n>` 覆盖层的内容在非本层页面上没有对应记录，反向映射天然不可确定。

**沙箱注意**：脚本用「stdout 重定向到文件」而不是管道（Node 的 `pipe` 在受限沙箱会 EPERM），因此**无需提权**即可运行。

**失败面排查顺序**：① dev stdout 有无 `打开项目` / `触发编译` / `构造编译请求`（后端链路）；
② `read_logs(console)` 有无前端异常；③ `ipc_get_backend_state` 确认连接的是本应用。

## Rust 单测：`cargo test -p texpresso`（src-tauri）在 Windows 启动即失败

**现象**：`cargo test -p texpresso --lib` 编译成功，但测试二进制**加载即退出**——`STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)`，任何测试都未运行。

**根因**：`texpresso_lib` 链接了 tauri/wry 的 `webview2-com`。`cargo test` 的测试二进制不做 GUI 初始化，加载阶段解析 WebView2/webview2-com 失败（`STATUS_ENTRYPOINT_NOT_FOUND`，发生在任何测试运行前）。**与业务代码无关**；`cargo check -p texpresso --tests` 能正常通过（测试代码编译无误）；本机 WebView2 Runtime 已安装（151.0.4129.107），排除运行时缺失。

**处置**：

- 纯逻辑单测走 `cargo test -p texpresso-core`（无 Tauri 依赖，运行正常，CI 用这个验证纯逻辑）。
- 前端单测走 `npm run test`（vitest，需提权 `danger-full-access` 跑 esbuild worker）。
- 基础设施层的用例（`fs` / `runner` / `watch` / `storage`）随 ADR-0010 迁到 `texpresso-infra` 后**本机可正常运行**：`cargo test -p texpresso-infra`（单测）+ `cargo test -p texpresso-infra -- --ignored`（真实 latexmk/synctex 集成用例：成功、内容错误、超时树杀、取消、中文路径双向）。仍留在 src-tauri 的只剩 `commands::pdf_path_for_root`（纯函数，受同一 WebView2 链接限制）。
- **已穷尽尝试仍失败**：把 `webview2-com-sys-*/out/{arch}/WebView2Loader.dll` 拷到 `target/debug` **及 `target/debug/deps`（测试 exe 同目录）** 并加入 PATH；`dumpbin /imports` 显示静态导入均为系统 DLL、延迟导入仅 `VCRUNTIME140.dll`；`danger-full-access` 提权运行——均仍 `STATUS_ENTRYPOINT_NOT_FOUND`。**非沙箱权限、非 PATH、非运行时缺失**，是 Tauri v2 shell crate 测试二进制的已知 Windows 工具链限制。把 `cargo test -p texpresso -- --ignored export_bindings` 当"无 GUI 生成 bindings"的捷径同样不可行（`0xc0000139`）。
- **推论（DTO 改动后怎么刷新 `src/bindings.ts`）**：`export_bindings` 这条捷径在本机不可用 → **起一次 `npm run tauri dev`**：debug 构建启动时会自动重新导出 `src/bindings.ts`（`lib.rs` 里 `#[cfg(debug_assertions)]` 的 export）。新增 DTO 字段时实测即如此，导出结果与手写预期一致。

## 中文文件名/路径兼容性实测

**总结论**：**文件名/路径层面的中文全链路可用**（无阻塞缺陷，回归测试已固化）；唯一实测出的真实缺陷**不在路径，而在日志编码**（见下节）。

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

**关键编码事实（字节级实测，值得记住）**：`synctex` 的 stdout **恒为 UTF-8**（`中` = `E4 B8 AD`），**与代码页无关**——CP65001 与 CP936 下抓到的字节完全一致。故 `String::from_utf8_lossy(&out.stdout)` 的用法**正确**（曾怀疑它会按代码页输出——已证伪）。

### 真实缺陷：GBK 源文件 + pdflatex → `.log` 含非法 UTF-8 → 错误列表拿不到任何信息

**现象**：源 `.tex` 是 GBK 编码（中文用户遗留文件很常见）时，`.log` 可能**不是合法 UTF-8**，而 `runner` 用严格 `read_to_string` 读日志 → 读取失败 → 前端只拿到「编译失败且无法读取日志（…）：stream did not contain valid UTF-8」，**真正的 TeX 错误一条都看不到**——恰好击中调研里最高频的痛点（错误诊断）。

**实测差异（同一 GBK 源，两个引擎行为不同）**：

| 引擎 | `.log` 是否合法 UTF-8 | 说明 |
|---|---|---|
| `xelatex` | ✅ 合法 | 引擎自己把非法字节替换为 U+FFFD 写进日志（`Invalid UTF-8 byte or sequence at line 3 replaced by U+FFFD.`） |
| **`pdflatex`** | ❌ **非法** | 实测含 **73 个非法字节**（首个 `0xD6`，offset 1893）——原始 GBK 字节被直接回显 |

**修复**：新增 `texpresso_core::log_parser::decode_log(bytes)`——能严格解就严格解，否则 lossy（非法字节 → U+FFFD）；`FileSystem` trait 增 `read_to_string_lossy`（默认退化为严格读取，`TokioFs` 覆盖为 lossy 解码），`runner` 读 `.log` 改走它。

**修复后判据（实测）**：同一个非法日志 lossy 解码后，`! LaTeX Error: Invalid UTF-8 byte sequence.` 与 `l.3 ` 等 **ASCII 骨架完好**，`parse_log` 仍能给出消息 + 行号——诊断信息"有损"远好于"没有"。回归测试 `log_parser::decode_tests::invalid_bytes_do_not_lose_error_skeleton` 锁定该行为。

**App 端到端复现与验证**：夹具 `test_file/projects/中文GBK工程/`（gitignore，需重建）——`主文件.tex`（纯 ASCII）+ `子目录/gbk.tex`（**GBK 编码**，含 `\undefinedcommandhere`）+ `.texpresso/settings.json` 覆盖 `{"compile":{"engine":"pdflatex"}}`。步骤：

1. `VITE_TEXPRESSO_PROJECT=…\中文GBK工程 npm run tauri dev`；
2. 触碰 `主文件.tex`（改 mtime）经 watch 触发编译；
3. 观察 dev stdout。

**实测输出（修复后）**：

```
DEBUG 构造编译请求: root=…\中文GBK工程\主文件.tex engine=PdfLaTeX
DEBUG 编译失败：已从 .log 解析出错误条目 count=9 log=…\tmp\主文件.log
```

即：非法 UTF-8 日志被容错解码后**成功解析出 9 条错误送达前端**。修复前该路径会退化为 `编译失败且无法读取日志（…）：stream did not contain valid UTF-8`（`warn!` 分支），错误列表**一条都没有**；现在编译失败在 stdout 可见，便于排查。

> 注：把"文件名编码"与"内容编码"两个变量隔离——根文件与子文件名均为 ASCII，GBK 只出现在子文件**内容**里；中文**文件名/路径**的验证由 `中文测试工程` 夹具覆盖。

### GUI 目视验证（tauri server MCP 驱动真实窗口）

两项 GUI 目视项用**截图 + DOM + 真实点击**验证（夹具 `test_file/projects/中文测试工程`）：

| 项 | 结果 | 证据 |
|---|---|---|
| **pdf.js 经 asset 协议加载中文路径 PDF 的渲染** | ✅ 通过 | `webview_screenshot`：标题「中文路径兼容性测试」、作者/日期、**目录三项中文条目**、章节正文与公式 `E = mc²` 全部正常渲染；3 页连续分页（`1 / 3`）；控制台实测 `[preview] reload#1 中文主文件.pdf pages=3 bytes=40648 fetch=9ms parse=32ms render=54ms total=94ms pagesRendered=3`——**fetch 走 asset 协议无 404/编码错误** |
| **SyncTeX 反向跳转（PDF → 源码）** | ✅ 通过 | `webview_interact` 点 PDF 正文 → 编辑器切回 `中文主文件.tex` 且光标停在 **Ln 10**（`公式测试：$E = mc^2$。`），即点击的那句正文对应的源码行 |

**点 PDF 目录区会映射到生成文件**（`中文主文件.toc`）：产品行为是生成产物/项目外文件**一律不打开**——先就近回落真实源码（`y ±40/80pt` 探测），回落到就跳并提示「已回落到最近的源码」，落空则只给提示「此处来自自动生成的文件 …，没有对应的源码行」。中文文件名在两种情况下都正确解析——这也反证反向 SyncTeX 的中文链路是通的。

### MCP Bridge 插件 0.13（版本上报与窗口工具的真机复验）

`@hypothesi/tauri-mcp-server@0.13.0` 会对应用侧插件做 skew 检查；插件低于 0.13 时 `driver_session` 报
`The connected app cannot report its plugin version.`（0.12 的 `get_backend_state` 还没有 `bridge.pluginVersion` 字段）。
本项目 `src-tauri/Cargo.toml` 用 `tauri-plugin-mcp-bridge = "0.13"`，复验项如下：

| 项 | 结果 | 证据 |
|---|---|---|
| 编译 | ✅ | `cargo check` 通过（插件 0.13.0）；capability 无需改动——0.13 的 `permissions/default.toml` 与 0.12 权限项完全一致，新增的窗口命令走既有权限 | 
| 插件初始化 | ✅ | dev stdout：`[MCP][PLUGIN][INFO] MCP Bridge plugin initialized for 'TeXPresso' (com.texpresso.app) on 0.0.0.0:9223` |
| 版本上报 | ✅ | `ipc_get_backend_state` → `bridge.pluginVersion = 0.13.0`；`driver_session status` → `pluginVersion 0.13.0` / `serverVersion 0.13.0` / **`versionWarning: null`**（升级前为 `pluginVersion: null` + 警告） |
| `manage_window` | ✅ | `list` 返回 main 窗口；`resize 1440×900` → `info` 报 2182×1406 物理像素（×1.5 缩放 = 1440×900 逻辑，与 0.13「物理↔CSS 像素换算」修复一致）；新增的 `maximize` 可把最小化窗口恢复正常几何 |
| 业务链路回归 | ✅ | 打开 `中文测试工程` → 「编译」→ stdout `手动编译: root=…中文主文件.tex` + PDF 生成 → 预览渲染 3 页（控制台 `[preview] reload#1 … pages=3 fetch=14 parse=45 render=60 total=118ms`） |
| SyncTeX 反向 | ✅ | `webview_interact` 点第一页正文 → 状态栏光标跳到 `中文主文件.tex` **Ln 9**（`\section{第一章：中文标题}`） |
| JS 执行 / 日志 | ✅ | `webview_execute_js` 返回 `document.title`、预览 canvas 数；`read_logs(console)` 读到 bridge 与 vite 日志 |

**⚠️ 坑：`driver_session status` 的 `pluginVersion` 来自 session 建立时抓取的元数据**（`mcp-server-tauri` 的
`fetchAppMetadata` 只在 start 时调用一次）。应用重建后仅重启应用**不会**刷新该字段——必须
`driver_session action=stop` → `action=start` 重新建会话，否则会一直看到旧的 `pluginVersion: null` 与升级警告，
误判成"升级没生效"。

**⚠️ `focus` 的实测边界**：0.13 的 `manage_window action=focus` 调用返回成功，但**不会把最小化的窗口还原**
（Windows 下实测 `x/y = -32000` 不变），也不能从别的应用手里抢前台；还原用 `maximize`，或 `resize` 到目标尺寸。

### 仍未覆盖 / 已知未修

- **正向 SyncTeX 高亮**（源码 Ctrl+点击 → PDF 高亮）：`webview_interact` 不支持带修饰键点击，做法是动态 import Monaco 实例后派发 `ctrlKey` 鼠标事件——见「真机验收清单」第 12 项。
- **已知未修**：**编辑** GBK 源文件（`read_file` 严格 UTF-8）会失败并返回英文 IO 错误。若要支持"打开并转码显示 GBK 源文件"，需单独设计（含保存时的编码回写策略）。
- 本机 `cargo test -p texpresso`（src-tauri）无法运行（见上一节），故 src-tauri 侧的中文用例只能以 `cargo check -p texpresso --tests` 编译校验；随 ADR-0010 迁移后，`fs` / `runner` / `storage` 的中文用例已在 `texpresso-infra` 下实际运行通过（含 2 个需 latexmk 的 `#[ignore]` 集成用例）。

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

**为什么合成多文件档（`multifile`）没事**：**latexmk 会在第一趟失败后把子目录建出来并自动重跑一趟**，所以 `tmp/chapters/`、`tmp/sections/` 是 latexmk 建的、第二趟就成功了（对照实测：同一最小工程用 `latexmk -xelatex -outdir=tmp ...` → 第一趟报同样的错、但 `tmp/sub` 被创建、第二轮 `Output written ... / All targets are up-to-date`、exit 0）。hithesis 这一档没有恢复，**嫌疑指向它自带的 `latexmkrc`（覆写 `$pdflatex`、内建 `--shell-escape` 与尾部 `;cp`）与 `-outdir=tmp` 的相互作用**——这正是 roadmap **㉖** 的题目；目前只做到"确认不是超时问题"，**尚未定位到可复现的 rc 最小组合**（用一个精简 rc 复刻其关键行跑最小工程，仍能正常恢复）。

**对产品的两个直接结论**：

1. **超时诊断必须先看日志里的致命错误**：这种"报错后不退出"的运行，产品只会走到超时；若只按"慢/卡住"提示，用户永远修不好。超时诊断因此优先报日志里的那条错误，且**不给"提高超时"按钮**（提高超时救不了它）。
2. **可用绕过（已实测）**：把主文件里的 `\include{子目录/文件}` 改成 `\input{子目录/文件}`——`\input` 不写子文件 `.aux`，因此不需要 `tmp/子目录/`：

```powershell
xelatex -interaction=nonstopmode -synctex=1 -output-directory=tmp main.tex   # main.tex: \input{sub/part}
# → Output written on tmp/main.pdf (1 page). / exit 0（tmp/sub 仍不存在也没关系）
```

诊断文案里已经写上这条建议（`DiagnosisKind::aux_write_failed`）。

## 探针文档含中文时不能用 pdflatex（附一条被证伪的假设）

**现象**：用 PowerShell 生成探针 `.tex` 后 `latexmk -pdf` **exit 12**，文档内容看起来完全正常。

**真实根因**：探针里含**中文正文**，而 `-pdf`（pdflatex）不支持 CJK → 编译失败。换 `-xelatex` 即 exit 0。这与 P0-② 分析 §3.1 是同一个现象（引擎选错 = 首屏一堆不可读错误）。

**⚠️ 被证伪的假设（记录以免重走）**：一度判断是 `Set-Content -Encoding UTF8` 写入了 **UTF-8 BOM**。实测证伪——

| 写法 | 头 6 字节 | 结果 |
|---|---|---|
| `Set-Content -Encoding UTF8` | `5C 64 6F 63 75 6D`（`\docum`，**无 BOM**） | exit 0（纯 ASCII 内容） |
| `[System.IO.File]::WriteAllText(..., UTF8Encoding($false))` | `5C 64 6F 63 75 6D` | exit 0 |

本机为 **pwsh 7.6.5**：`-Encoding UTF8` 默认即为**无 BOM**（`utf8NoBOM`）。**BOM 不是原因**；把两次差异归因于 BOM 是错的，真正的差异是**内容里有中文**。

**处置**：探针文档含中文时用 `xelatex`/`lualatex`；只有在 **Windows PowerShell 5.1**（非本机 `pwsh`）上才需要担心 `Set-Content -Encoding UTF8` 的 BOM 问题。

> 与 AGENTS.md §4 的「PowerShell 转义」是同类排查场景：反斜杠过度转义、编码、**引擎选择**都会表现为 exit 12，先确认是哪一个再改。
