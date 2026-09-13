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

**附：WSLg 多显示器窗口跑到外接屏**：window-state 插件会保存并恢复跨屏位置；窗口“消失”时先 `rm ~/.config/com.latteset.app/.window-state.json`。真机多显示器拔出后同样可能出现，后续可加“位置越界则居中”保护。

## GUI/端到端测试链路（WebDriver + pc-control，2026-08）

**方案**：`test_file/e2e/` 用 **WebdriverIO**（`browserName:'wry'` + `tauri:options.application`）驱动 `tauri-driver`（中介）+ `msedgedriver`（Windows，需 `msedgedriver-tool` 安装并匹配 Edge 版本，`--native-driver` 指路径）。`src/App.vue` 提供 `VITE_LATTESET_PROJECT` 钩子自动打开项目（绕过原生目录弹窗，WebDriver 无法驱动）；`PreviewPane.vue` 带重载耗时插桩。

**关键教训（均实测）**：
- **手动拉起 debug 二进制 ≠ 可用**：`npm run dev`（只起 vite）+ 手动 `target/debug/latteset.exe`，前端**不渲染**（`button.btn.primary` 找不到）。必须 **`npm run tauri dev`**（正确构建 Rust + 起 vite + 真正调起 Tauri 窗口），问题即消失。
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
| 13 | 非 UTF-8 源文件提示（㉓） | 夹具 `中文GBK工程` → 展开 `子目录` → 点 `gbk.tex` → 读 `.open-error` 与标签页 | 状态栏出现中文提示「…不是 UTF-8 编码…另存为 UTF-8…」；**不开新标签**（此前是无人接的 rejection） |
| 14 | 源码版模板提示（㉗） | 夹具 `源码版模板工程`（`\documentclass{nosuchthesis}` + `nosuchthesis.ins/.dtx`）→ 点「编译」→ 读第一条错误 | 「缺少文档类文件 nosuchthesis.cls」+「项目里有源码版模板文件 `nosuchthesis.ins`：先执行 `xelatex nosuchthesis.ins`…」（**必须是 `.ins`**，不能是 `.dtx`） |
| 15 | 外部改 settings.json 生效（㉑） | 夹具 `多候选工程`（先把 `.latteset/settings.json` 置为 `{}` 并让它生效）→ 从**外部**写入 `{"root_file":"main.tex"}` → 看状态栏；随后改回 `{}` | 第一次：日志 `设置热更新：内存 root_file 已同步 from=None to=Some(…)` 且「未确定根文件」消失；改回：`from=Some(…) to=None` 且提示**回来**。**必须用非原子写法**（PowerShell `Set-Content` 就会"截断 → 写入"，正好覆盖竞态），原子替换测不出这条 |

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

## Rust 单测：`cargo test -p latteset`（src-tauri）在 Windows 启动即失败

**现象**：`cargo test -p latteset --lib` 编译成功，但测试二进制**加载即退出**——`STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)`，任何测试都未运行。

**根因**：`latteset_lib` 链接了 tauri/wry 的 `webview2-com`。`cargo test` 的测试二进制不做 GUI 初始化，加载阶段解析 WebView2/webview2-com 失败（`STATUS_ENTRYPOINT_NOT_FOUND`，发生在任何测试运行前）。**与业务代码无关**；`cargo check -p latteset --tests` 能正常通过（测试代码编译无误）；本机 WebView2 Runtime 已安装（151.0.4129.107），排除运行时缺失。

**处置**：

- 纯逻辑单测走 `cargo test -p latteset-core`（无 Tauri 依赖，运行正常，CI 用这个验证纯逻辑）。
- 前端单测走 `npm run test`（vitest，需提权 `danger-full-access` 跑 esbuild worker）。
- 基础设施层的用例（`fs` / `runner` / `watch` / `storage`）随 ADR-0010 迁到 `latteset-infra` 后**本机可正常运行**：`cargo test -p latteset-infra`（单测）+ `cargo test -p latteset-infra -- --ignored`（真实 latexmk/synctex 集成用例：成功、内容错误、超时树杀、取消、中文路径双向）。仍留在 src-tauri 的只剩 `commands::pdf_path_for_root`（纯函数，受同一 WebView2 链接限制）。
- **已穷尽尝试仍失败**：把 `webview2-com-sys-*/out/{arch}/WebView2Loader.dll` 拷到 `target/debug` **及 `target/debug/deps`（测试 exe 同目录）** 并加入 PATH；`dumpbin /imports` 显示静态导入均为系统 DLL、延迟导入仅 `VCRUNTIME140.dll`；`danger-full-access` 提权运行——均仍 `STATUS_ENTRYPOINT_NOT_FOUND`。**非沙箱权限、非 PATH、非运行时缺失**，是 Tauri v2 shell crate 测试二进制的已知 Windows 工具链限制。把 `cargo test -p latteset -- --ignored export_bindings` 当"无 GUI 生成 bindings"的捷径同样不可行（`0xc0000139`）。
- **推论（DTO 改动后怎么刷新 `src/bindings.ts`）**：`export_bindings` 这条捷径在本机不可用 → **起一次 `npm run tauri dev`**：debug 构建启动时会自动重新导出 `src/bindings.ts`（`lib.rs` 里 `#[cfg(debug_assertions)]` 的 export）。新增 DTO 字段时实测即如此，导出结果与手写预期一致。

## Linux CI 上 `cargo test -p latteset-core` 红：Windows 路径语义被写进了跨平台用例（2026-09）

**现象**：本机（Windows）173 项全绿，GitHub Actions 的 `ubuntu-latest` 上 `synctex::classify::tests::windows_separators_and_case` 失败，172 passed / 1 failed：

```
left:  OutsideProject("E:\\proj\\chapters\\intro.tex")
right: Source("E:\\proj\\chapters\\intro.tex")
```

**根因**：`normalize_lexically` / `classify_inverse_target` 的路径比较是**词法级**的，吃的是 `std::path::Path` 的**平台原生**分隔符语义——Windows 上 `\` 是分隔符（`E:\proj\chapters\intro.tex` 能剥出项目内相对路径），Unix 上 `\` 只是普通字符，整串是**一个文件名**，`starts_with(root)` 为假 → 判 `OutsideProject`。**产品逻辑无误**（Windows 首发，synctex 返回的就是 Windows 路径），红的是把 Windows 专属结论写成跨平台契约的用例。

**处置**（`crates/latteset-core/src/synctex/classify.rs`）：

- 跨平台部分改用 `std::path::MAIN_SEPARATOR` 拼路径，「项目内源码」与「盘符大小写不 case-fold」在两端都被断言到；
- 反斜杠形态按平台分叉：`#[cfg(windows)]` 断言 `Source`，`#[cfg(not(windows))]` 断言 `OutsideProject`（锁死"Linux 上绝不误判成可打开的源码"）。

**验证**：本机 `cargo test -p latteset-core` → 173 passed；`rustup target add x86_64-unknown-linux-gnu` 后 `cargo check -p latteset-core --target x86_64-unknown-linux-gnu --tests` 通过（非 Windows 分支可编译）。沙箱内首次 `rustup target add` 与 crates 下载均被 TLS 拦（`SEC_E_NO_CREDENTIALS`），按 §4 边界提权一次后成功。

**规则**：core 单测要能在 Linux CI 上跑，**别把 Windows 路径形态（`\`、`E:\`）当跨平台契约**——要么用 `MAIN_SEPARATOR` 拼，要么 `#[cfg]` 分叉并显式写出两端期望值。（`latteset-infra` 的 `runner` / `storage` 用例含同类 Windows 字面量：读实现可判定 `root_stem` / `latexmk_input` 在 Unix 上会因"整串只当一个文件名"而走回退分支、断言必失败（**未在 Linux 实跑，仅读码判定**）；CI 目前不跑 infra，纳入前需先按此规则过一遍。）

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
| **App 真机链路** | ✅ 全通 | `npm run tauri dev` + `VITE_LATTESET_PROJECT` 打开中文工程 → 日志实见 `打开项目：…\中文测试工程`、`开始监视项目：…`（notify 注册成功）、`手动编译: root=…\中文主文件.tex` → 项目根产出 **`中文主文件.pdf` (40,648 B)**；`tmp/` 事件被正确忽略 |
| `synctex view`（正向） | ✅ | 中文绝对路径 `-i 5:1:E:/…/中文主文件.tex` → `Page:1` |
| `synctex edit`（反向） | ✅ | 回传 `Input:E:/…/中文测试工程/./中文主文件.tex`（中文完好） |
| `.log` 中文文件名 | ✅ | 日志内 `(./中文主文件.tex` 为 UTF-8；解析能带出中文文件名 + 行号（回归测试 `parse_log_keeps_chinese_file_name_and_line`） |
| `canonicalize` + `\\?\` 剥离 | ✅ | `\\?\E:\项目\…` → `E:\项目\…`（既有 `strip_verbatim` 对中文安全） |
| 中文应用配置目录 | ✅ | 模拟 `C:\Users\中文用户\AppData\…`：全局设置与项目覆盖 `.latteset/settings.json` 均原子读写通过 |

**关键编码事实（字节级实测，值得记住）**：`synctex` 的 stdout **恒为 UTF-8**（`中` = `E4 B8 AD`），**与代码页无关**——CP65001 与 CP936 下抓到的字节完全一致。故 `String::from_utf8_lossy(&out.stdout)` 的用法**正确**（曾怀疑它会按代码页输出——已证伪）。

### 真实缺陷：GBK 源文件 + pdflatex → `.log` 含非法 UTF-8 → 错误列表拿不到任何信息

**现象**：源 `.tex` 是 GBK 编码（中文用户遗留文件很常见）时，`.log` 可能**不是合法 UTF-8**，而 `runner` 用严格 `read_to_string` 读日志 → 读取失败 → 前端只拿到「编译失败且无法读取日志（…）：stream did not contain valid UTF-8」，**真正的 TeX 错误一条都看不到**——恰好击中调研里最高频的痛点（错误诊断）。

**实测差异（同一 GBK 源，两个引擎行为不同）**：

| 引擎 | `.log` 是否合法 UTF-8 | 说明 |
|---|---|---|
| `xelatex` | ✅ 合法 | 引擎自己把非法字节替换为 U+FFFD 写进日志（`Invalid UTF-8 byte or sequence at line 3 replaced by U+FFFD.`） |
| **`pdflatex`** | ❌ **非法** | 实测含 **73 个非法字节**（首个 `0xD6`，offset 1893）——原始 GBK 字节被直接回显 |

**修复**：新增 `latteset_core::log_parser::decode_log(bytes)`——能严格解就严格解，否则 lossy（非法字节 → U+FFFD）；`FileSystem` trait 增 `read_to_string_lossy`（默认退化为严格读取，`TokioFs` 覆盖为 lossy 解码），`runner` 读 `.log` 改走它。

**修复后判据（实测）**：同一个非法日志 lossy 解码后，`! LaTeX Error: Invalid UTF-8 byte sequence.` 与 `l.3 ` 等 **ASCII 骨架完好**，`parse_log` 仍能给出消息 + 行号——诊断信息"有损"远好于"没有"。回归测试 `log_parser::decode_tests::invalid_bytes_do_not_lose_error_skeleton` 锁定该行为。

**App 端到端复现与验证**：夹具 `test_file/projects/中文GBK工程/`（gitignore，需重建）——`主文件.tex`（纯 ASCII）+ `子目录/gbk.tex`（**GBK 编码**，含 `\undefinedcommandhere`）+ `.latteset/settings.json` 覆盖 `{"compile":{"engine":"pdflatex"}}`。步骤：

1. `VITE_LATTESET_PROJECT=…\中文GBK工程 npm run tauri dev`；
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
| 插件初始化 | ✅ | dev stdout：`[MCP][PLUGIN][INFO] MCP Bridge plugin initialized for 'Latteset' (com.latteset.app) on 0.0.0.0:9223` |
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
- 本机 `cargo test -p latteset`（src-tauri）无法运行（见上一节），故 src-tauri 侧的中文用例只能以 `cargo check -p latteset --tests` 编译校验；随 ADR-0010 迁移后，`fs` / `runner` / `storage` 的中文用例已在 `latteset-infra` 下实际运行通过（含 2 个需 latexmk 的 `#[ignore]` 集成用例）。

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

**2026-09 补充：latexmk 的自动恢复不是普遍成立的。** 自建夹具（10 章 ctexbook，章节用 `\include{chapters/cNN}`，`tmp/` 为空）走 **Full**（latexmk 完整收敛）时并没有恢复：日志停在"写不出中间文件 `chapters/c01.aux`" + `Emergency stop`，latexmk 反复重跑同一处失败，直到 **120s 超时**（上面 §"为什么合成多文件档没事"里的 `tmp/sub` 被创建，在这里没有发生）。两次实验的差异未定位到判据（文档类/中文宏包/页数都可能相关），但"latexmk 会自动建目录并重跑"这句话**不能当作保证**。顺带这也是一次流式反馈的实战价值演示：实时错误在该次编译的 **68s** 就报出了这条诊断，而超时终态要到 **127s** 才出现（提前 59s）。

## 引擎输出在非 TTY 下是 4KB 块缓冲：实时反馈要尾随 `.log`（2026-09 实测）

**现象**：把引擎的 stdout/stderr 接成管道（`Stdio::piped()`）做"边编译边报错"后，短文档的**错误与页标记会一起在进程快结束时才到**——看起来像流式没生效。

**实测**：一次 1471ms 的编译里，首条流式错误出现在 **1394ms**（几乎等于进程退出时刻）。原因是 C 运行时的块缓冲：**stdout/stderr 不是 TTY 时按 4KB 缓冲**，TeX 只在缓冲满（约每 20 页）或退出时才 flush。

**对策（已落地）**：实时通道以 `tmp/<stem>.log` 为主力——**`.log` 是按页 flush 的**，每 200ms 尾随一次即可稳定拿到"排到第几页""刚出现的致命错误"；管道只作补充（两条来源共用同一状态机做页码取最大 + 错误指纹去重）。结论：**"接个管道"不够**，短文档/早错场景下管道几乎给不出提前量。

**验证入口**：`cargo test -p latteset-infra -- --ignored live_errors`（断言首个流式错误距编译结束 ≥100ms；只接管道时这条会红）。

## 错误列表的文件名偶尔错一章：TeX 日志的 `)` 与 `(` 会打在同一行（2026-09 定位，未修）

**现象**：错误实际写在 `ch_05.tex` 第 164 行，列表显示 `./ch_04.tex:164`，点击跳到 `ch_04.tex`。

**根因**：TeX 日志把"关闭上一个文件 + 打开下一个文件"打在**同一行**：

```
[64]) (./ch_05.tex
```

而 `log_parser::scan` 的文件栈只认**行首** `(` 入栈、行首 `)` 出栈 → `ch_05` 没入栈、`ch_04` 被弹出，此后该文件的错误都归到 `ch_04`。终态与流式共用同一解析器，两条路径都会错。修法要按字符顺序做括号配对（独立任务，记在 [modules.md](./modules.md) §12.1 #22）。

## 外部（非原子）写 settings.json：watcher 会读到半截文件（2026-09 实测，㉑ 顺带修）

**现象**：用 PowerShell `Set-Content` / 记事本改 `.latteset/settings.json`（或全局 `settings.json`），应用**有时不响应**——日志里能看到：

```
DEBUG watch 原始事件: [".latteset\settings.json"] kind=Modify(Any)
WARN 项目设置解析失败，忽略外部修改：….latteset\settings.json      ← 读到的是"截断后、写入前"的空文件
DEBUG watch 原始事件: [".latteset\settings.json"] kind=Modify(Any)  ← 第二次事件
```

**根因**：这类写入是"**截断 → 写入 → 关闭**"三步，watcher 会收到**两个** Modify 事件，中间存在文件为空/被占用的窗口：第一次读拿到空内容（JSON 解析失败），第二次读可能撞上共享冲突（`read_to_string` 直接失败）。旧实现两种情况都只是"忽略这次修改"（后者连日志都没有），用户侧就是「改了没反应」。

**处置（已落地）**：`handle_settings_change` 改为 **3 次 × 150ms 短重试**（每次重试重新读 + 重新解析），最终仍失败才打警告。**验收要点**：必须用非原子写法复现（`Set-Content` 即可），用"先写临时文件再 rename"的原子替换测不出这条。

**另注**：应用自己写设置走的是原子写（`atomic_write` + 自写盘 hash 过滤），不受此影响。

## `cargo build -p latteset-server` 报「failed to remove file … 拒绝访问」(os error 5)

**现象**：接了 DSH 的 `mcp-latteset`（或任何常驻的 MCP 客户端）之后，重新构建 headless 会失败：

```
error: failed to remove file `E:\Works\tex-presso\src-tauri\target\debug\latteset-mcp.exe`
Caused by: 拒绝访问。 (os error 5)
```

**根因**：`latteset-mcp.exe` 正**作为常驻进程运行**（stdio MCP server 由 harness 拉起后一直活着），Windows 锁定已加载的可执行文件 → 链接器无法替换它。与代码无关，`cargo build -p latteset`（GUI 应用）不受影响，因为它不产这个二进制。

**处置（按代价排序）**：

1. **只验 lib / 不产二进制**：`cargo test -p latteset-server --lib`（⑨ 之外的所有单测都在 lib 里）；
2. **换 target 目录构建一次**（本机实测可用，代价是依赖图重编一遍）：
   ```powershell
   $env:CARGO_TARGET_DIR='<某个不在 src-tauri 下的目录>'; cargo test -p latteset-server
   ```
   ⚠️ **别把它放在 `src-tauri/` 里面**——`tauri dev` 的文件监视会看到那些 `.fingerprint` 变化并触发重建（本机踩过），放工作区外或 `%TEMP%`；
3. 在 DSH 里临时移除 `mcp-latteset` 行（或重启 DSH）再构建。

> 这也是「⑥ + 接线」的副作用之一：headless server 一旦常驻，它的二进制就不可被替换——改 `crates/latteset-server` 的代码后要记着这条。

## 更名 TeXPresso → Latteset：配置目录与项目设置目录都变了（2026-09）

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

**本项目内的配套改动**（避免真机验证时踩空）：`test_file/projects/{multifile,中文GBK工程,多候选工程}/.texpresso/` 已随更名改到 `.latteset/`；这些目录被 `.gitignore` 覆盖，**不在版本库内**，重建夹具时要按新名创建。

**验证证据（2026-09 真机）**：`npm run tauri dev` 启动日志 `MCP Bridge plugin initialized for 'Latteset' (com.latteset.app)`，运行的是 `target\debug\latteset.exe`，窗口标题 `Latteset`；启动后自动出现 `%APPDATA%\com.latteset.app\settings.json`（identifier 生效的直接证据）；`open_project` 后端日志 `打开项目：…\multifile（根文件 Some(…\main.tex)，候选 1 个）` + `开始监视项目`，即重命名后的设置/监视链路全通。

**另一个坑：文档里不要跟着改「上游项目」的名字**。文档描述的上游 [let-def/texpresso](https://github.com/let-def/texpresso) **必须保持原拼写**——批量替换会造出不存在的 URL（`github.com/let-def/latteset`）和错误的源码符号名（上游真实符号是小写 `texpresso_protocol.c` / `texpresso_fork_with_channel`，不是 `TeXpresso_protocol.c`）。同理 [texpresso-live-rendering-roadmap.md](./texpresso-live-rendering-roadmap.md) 的**文件名保留原名**（它是"对上游的通读"）。

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

## headless（CLI / MCP）怎么跑、怎么排障（2026-09，⑥ 落地记录）

**跑起来**（二进制是工作区产物，`cargo build -p latteset-server` 后位于 `src-tauri/target/debug/latteset-{cli,mcp}.exe`）：

```powershell
latteset-cli --project test_file\projects\multifile compile        # stdout 只有 JSON
latteset-cli --project <dir> compile | ConvertFrom-Json | % { $_.compile.status, $_.compile.errors }
latteset-mcp --project <dir>                                        # stdio server（harness 本地拉起）
```

**四条踩过的坑**：

1. **PowerShell 会吃掉 `$`**：直接把含公式的 LaTeX 当命令行参数传，`$E = mc^2$` 的 `$...$` 被 PowerShell 当变量展开 → 文件里剩下 `E = mc^2`（无数学模式）→ 编译报 `! Missing $ inserted.`。**处置**：用 stdin 写——
   ```powershell
   $body = @'
   \section{测试}
   能量 $E = mc^2$。
   '@
   $body | latteset-cli --project <dir> write chapters/intro.tex -
   ```
   （单引号 here-string = 不求值；`-` 表示从 stdin 读内容。）
2. **`write` 不会替你建目录**：父目录不存在 → 报 `NotFound` 并说明"父目录必须已存在"（与 GUI `save_all` 同契约）。先建目录，或先写已存在的目录下的文件。
3. **退出码语义**：`compile` 的 `0` 只表示 **status=success**；编译未通过是 `1`（**不是**命令出错）。`2` = 用法/路径类错误，`3` = 内部错误。
4. **配置目录**：默认与 GUI **同一份**（Windows `%APPDATA%\com.latteset.app`），所以 CLI 改了设置 GUI 也会看到；要隔离（测试/CI）用 `--config-dir <dir>` 或 `LATTESET_CONFIG_DIR`。

**并发禁忌**：同一项目**不要**同时用 GUI 和 CLI 编译——两路 latexmk 抢同一个 `tmp/`（先到先写、后到覆盖）。当前约定 headless 独占，没有项目锁。

**MCP 侧自检**：协议没握手成功时先手工喂一行看回什么——

```powershell
'{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}' |
  latteset-mcp --project <dir>
```

应回 `result.protocolVersion` + `capabilities.tools` + `instructions`。日志（含每一次工具调用）走 **stderr**，stdout 只有协议报文——排查时别把两者混在一个管道里。

## 怎么查「引擎到底读了什么 / 为什么找不到文件」（2026-09，G1 研究配套）

排"改了文件没生效""缺文件找不到"时，四个通道都是现成的（结论与实测见 [research/g1-read-interception-feasibility.md](./research/g1-read-interception-feasibility.md)）：

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

1. **"改了 `.bib` 没反应"是已知缺陷，不是操作问题**：`watch` 只对 `.tex` 触发（[modules.md](./modules.md) §7），被 `\bibliography` 引用的 `.bib` 改完，`tmp/` 全部产物 mtime 不变（roadmap ㉜）；反向也错——改**未被引用**的 `.tex` 会白编译一次（roadmap ㉝）。
2. **别常开 `KPATHSEA_DEBUG`**：它的输出走 **stderr**，而应用的流式错误通道也在读 stderr（[modules.md](./modules.md) §2.6.1）——1 MB 的 `kdebug:` 行会灌进解析缓冲；只在做单次诊断性编译时开。

## 用 WS 探针脚本调前端时踩到的两个坑（2026-09）

「WS 直驱真机」的探针（`node scripts/editor-report.mjs --eval-file <file>`）很好用——它自己连 MCP Bridge 的 9223，不需要 MCP 会话在场。但有两条**不是产品缺陷、却极难自查**的坑：

1. **探针脚本不能以 `//` 注释开头**：应用的执行器把脚本**包进括号**求值，行注释开头会让整段解析失败——而且失败是**静默**的：返回 `null` 而不是 error（脚本里的 `console.log` 也看不到）。**让第一行就是 `(async () => {`**；`Session.eval` 已对 `//`/`/*` 开头的脚本补前导换行兜底。
2. **别用 mtime 判"某条命令有没有跑"**：验证"跳过 `xdvipdfmx`"时用 `tmp/<stem>.pdf` 的 mtime 当判据，结果被**空闲收敛**（㉘）干扰——Quick 成功后 2s 的收敛 Full 走 latexmk，它自己会重写该文件，看起来像"Quick 没跳过"。**改用日志行判定**（`跳过 xdvipdfmx 转换与 PDF 拷贝`）才看清真相。

顺带一条：探针涉及的 dev 实例只允许一个——端口 1420/9223 被占时新实例会**静默连到旧实例**（日志里看到的项目可能不是你以为的那个）。起实例前先 `Test-NetConnection 127.0.0.1 -Port 1420`。
