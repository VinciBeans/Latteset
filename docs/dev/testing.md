# 测试与验证

怎么跑测试、怎么在真机上验收一次改动。读者是改代码的人；**只有真实窗口才能验的部分**（预览耗时、点击链路、目视渲染）都收在这一份里。

## 怎么跑，每层覆盖什么

| 想验什么 | 命令 | 覆盖 |
|---|---|---|
| 纯逻辑 | `cargo test -p latteset-core` | 调度器语义（队列合并 / 超时 / 重试 / 终止）、`.log` 解析与诊断、设置模型与合并（含 v3 默认值 / 旧 `lib_form` 键忽略）、根文件探测、大纲、SyncTeX 纯逻辑（路径与回落分类）、XDV 页哈希、公式扫描与片段装配 |
| 文件系统 / 存储 / watch | `cargo test -p latteset-infra` | 文件系统实现、notify 监视、设置存储（**v1→v2→v3 迁移**：备份名 / notice 矩阵 / 幂等 / 只读 / 损坏 / BOM）、manifest 解析；子进程 runner 的 32 个用例已随整文件删除 |
| 内嵌 Tectonic（单测） | `cargo test -p latteset-tectonic`（需 vcpkg env；或 `npm run lib:test`） | format 键 / bundle 三类失败文案 / 页哈希接线 / D3① 外部工具提示 / D3③ 页进度（XDV 完整页）/ D3② 超时证据 / D7 模板提示 / Quick 预算闸门 / SyncTeX 主输入补丁 |
| 内嵌 Tectonic（真编） | `cargo test -p latteset-tectonic -- --ignored` | **闸门 = 编进来 + 有可用 bundle**（不再探测 PATH 上的 `tectonic`）：format 生成 + 真 PDF 出图 + 页进度 + `compile-notice` 断言。T9 实测 1 个用例通过（5.33 s） |
| headless（CLI / MCP） | `cargo test -p latteset-server`（`-- --ignored` 真编译） | `Session`（打开 / 读写 / 树 / 大纲 / 非 UTF-8 / 编译拒绝 / 配置目录 / D4 资源优先级）；lib **12 passed / 4 ignored** + cli **8 passed**（T9 实测） |
| MCP 协议链路 | `cargo test -p latteset-server --test mcp_stdio` | **真实二进制 + stdio 管道**：握手 / `tools/list` / 工具往返 / 错误语义 / 逐调用切项目（**5 passed + 1 个 `#[ignore]`** 编译用例，闸门同上） |
| 前端 | `npm run test`（`test:watch` 监视） | stores（project/editor/compile/preview/outline）、composables（useAutoSave / useIdleConvergence / useSyncTex / useTheme）、组件（ErrorList / RootFilePicker / SettingsPanel / StatusBar）、services（paths / fuzzy / draftPatch）；T9 实测 **17 files / 151 tests** |
| 全仓 Rust（唯一入口） | `npm run lib:test` | 四个 crate 一起跑；T9 实测 **377 passed / 6 ignored**（6 个真编在 `--ignored` 下全过） |
| 交付前 | `npm run build` | `copy:pdf-worker` + `vue-tsc --noEmit` + `vite build` |
| 绑定生成物 | 启动一次 debug 构建自动导出 `src/bindings.ts`（Tauri 测试二进制在本机起不来，`export_bindings` 捷径不可用）；T9 核对 13/13 PASS（命令 18 条 / 事件 9 个与 Rust 一致） |

模块分工与各用例的判据见 [modules.md](./modules.md)；延迟预算报告用 `node scripts/bench.mjs`（**历史脚本，仍驱动 latexmk/xelatex**，见下「脚本失效说明」）。

> **构建前置（内嵌唯一后必读）**：任何 `cargo` / `tauri` 命令都需要 vcpkg 环境——统一走 `scripts/with-tectonic-lib.ps1`（先 `npm run lib:check` 自检），**不要裸跑 `cargo test`**（build script 会因缺 5 个环境变量 panic）。本机若在受限沙箱会话（工作区带 Low 完整性标签，报 `cl D8050` / `LNK1104`），还要先把 `TMP`/`TEMP` 指到工作区内目录，见 [troubleshooting.md](../troubleshooting.md)。正常终端/CI 不需要这一步。

**哪些算「真机」**：只有 `npm run tauri dev`（正确构建 Rust + 起 vite + 真正调起 Tauri 窗口）能验 GUI。`npm run dev` 只起 vite（`localhost:1420`），配手动拉起的 debug 二进制**前端不渲染**（`button.btn.primary` 找不到）——不要用这条路做 GUI 测试。

**三个已知边界**：

- **`cargo test -p latteset`（src-tauri）在本机跑不起来**（测试二进制加载即退出，`STATUS_ENTRYPOINT_NOT_FOUND`）——Tauri v2 shell crate 的 Windows 工具链限制，不是代码问题；src-tauri 侧只能用 `cargo check -p latteset --tests` 做编译校验。改动 DTO 后 `src/bindings.ts` 的刷新方式也记在 [troubleshooting.md](../troubleshooting.md) 那一条里。
- **受限沙箱跑不了前端**：vite/esbuild 的 worker 子进程需要创建命名管道，`npm run test` / `npm run build` / 起窗口会 `EPERM`，需要放宽权限。
- **平台可移植性**：core 单测要能在 Linux CI 上跑——**别把 Windows 路径形态（`\`、`E:\`）当跨平台契约**：要么用 `std::path::MAIN_SEPARATOR` 拼，要么 `#[cfg]` 分叉并显式写出两端期望值。反例与修法见 [troubleshooting.md](../troubleshooting.md) 的「Linux CI 上路径语义用例红」。

## 真机验收（GUI）

前置：`npm run tauri dev`（真实窗口）+ tauri server MCP `driver_session action=start`（默认端口 9223）。

**第 0 步（易踩）**：应用**重新构建/重启后**必须 `driver_session action=stop` → `action=start` 重建会话——`pluginVersion` 是建会话时抓取的元数据，不重建会一直显示旧值并误判"升级没生效"。

| # | 检查项 | 操作 | 期望 |
|---|---|---|---|
| 1 | 项目打开 | `manage_window list` 确认窗口 → `webview_screenshot` | 标题栏显示项目绝对路径；左栏文件树非空 |
| 2 | 错误列表 | 点「编译」→ `webview_dom_snapshot` scoped `.error-list` | 条目两行式（原因 + `→` 建议）；头部「已诊断 N」；原始 `.log` 在 `title` |
| 3 | 错误跳转 | `webview_find_element .entry` 取几何 → `webview_interact` 点第 2 条 → 读 `.cursor-pos` | 状态栏行号变为该条目的 `:行号` |
| 4 | 根文件选择器 | 打开多候选工程 → `webview_dom_snapshot` scoped `.picker-panel` → 点 `.file-row` | 弹窗列出候选（title 为绝对路径）；点选后弹窗关闭、`.needs-root` 匹配数归 0 |
| 5 | 编译出 PDF | 点 `.btn.primary` → `webview_wait_for .page-wrap canvas` → `webview_screenshot` | 预览渲染出页面；页计数 `n / N` 正常 |
| 6 | **预览耗时（基准）** | `webview_execute_js` 读 `window.__previewLastReload` | 得到 `{fetch,parse,render,total,pagesRendered}`；与 `scripts/bench.mjs` 的输出拼成端到端 |
| 7 | 反向 SyncTeX | `webview_find_element .page-wrap canvas` 取页几何 → 点击正文区 → 读状态栏 | 编辑器切到对应 `.tex` 且行号≈点击句所在行（`multifile` 实测点第 5 页正文 → `chapters/intro.tex` **Ln 3**，跨文件）。**点目录区**不打开生成的 `.toc`：工具条出现「此处来自自动生成的文件 main.toc…」或「已回落到最近的源码（main.tex:37）」提示 |
| 8 | 中文路径渲染 | 用 `中文测试工程` 夹具重复 1/5 | 中文标题/目录/正文/公式正常渲染，`fetch` 无 404 |
| 9 | 编辑期草稿 + 空闲收敛 | 点「编译」建产物 → **在编辑器里敲一行**（见下「如何在应用内输入文字」）→ 记录状态栏时间线 | `排版中…` → `就绪` + **「引用待更新」** → 约 2s 后再次 `排版中…`（空闲收敛的 Full）→ 提示消失；同 cycle 的 dev stdout / 日志与 `kind` 字段可区分实际强度（内嵌形态的 Quick 是请求强度：预算内收敛会如实报 Full，此时**不该**亮「引用待更新」） |
| 10 | 超时诊断 + 一键提超时重试 | 设置面板把超时改 **5s** → 清掉 `tmp/` 制造首编 → 点「编译」→ 读 `.error-list .entry` | 状态栏「失败·超时」；列表一条：原因含「**引擎输出显示**已经排版到第 N 页——是在推进」（或「**引擎输出里**还没出现任何页面输出」）+ 建议 + 按钮**「提高到 900s 并重试」**（首编跳档）；点按钮后 `get_settings` 超时变为 900、编译启动并转「就绪」、条目消失。**证据口径**：证据 = 捕获的 `.log`（内嵌形态边写边在内存里）+ 引擎状态环尾部 + 已上报最大页，合并后判定。⚠ 该路径**真实超时未在阶段②真机跑过**（单测覆盖），跑之前先按 `test_file/phase2/REPORT-gates.md` 的记录补「真实超时真编 `[未测]`」状态。**注意**：这一项会把全局 `settings.json` 的超时改掉，验完记得改回 120 |
| 11 | SyncTeX 生成产物回落 | 点 PDF **目录区**（`multifile` 第 3 页中部，x≈20% 页宽 / y≈65–85% 页高）→ 读 `.sync-note` 与标签页 | 工具条提示「此处来自自动生成的文件 main.toc（…）」或「已回落到最近的源码（main.tex:37，向下探测）」；**标签页不得出现 `main.toc`**；提示约 5s 后自动消失 |
| 12 | 正向 SyncTeX 高亮（`webview_interact` 无修饰键） | 用 Vite 预打包 URL 动态 import Monaco → `ed.setPosition(...)` → 对编辑器 DOM 派发 `mousedown/mouseup/click`（带 `ctrlKey: true`）→ 读 `.highlight` 的 `display/left/top` | 恰好一个 `.highlight` 为 `display: block` 且落在目标页；编辑器 `onMouseDown(ctrlKey)` 是真实入口，派发等价于用户 Ctrl+点击 |
| 13 | 非 UTF-8 源文件提示 | 夹具 `中文GBK工程` → 展开 `子目录` → 点 `gbk.tex` → 读 `.open-error` 与标签页 | 状态栏出现中文提示「…不是 UTF-8 编码…另存为 UTF-8…」；**不开新标签**（此前是无人接的 rejection） |
| 14 | 源码版模板提示 | 夹具 `源码版模板工程`（`\documentclass{nosuchthesis}` + `nosuchthesis.ins/.dtx`）→ 点「编译」→ 读第一条错误 | 「缺少文档类文件 nosuchthesis.cls」+「项目里有源码版模板文件 `nosuchthesis.ins`：先执行 `xelatex nosuchthesis.ins`…」（**必须是 `.ins`**，不能是 `.dtx`） |
| 15 | 外部改 settings.json 生效 | 夹具 `多候选工程`（先把 `.latteset/settings.json` 置为 `{}` 并让它生效）→ 从**外部**写入 `{"root_file":"main.tex"}` → 看状态栏；随后改回 `{}` | 第一次：日志 `设置热更新：内存 root_file 已同步 from=None to=Some(…)` 且「未确定根文件」消失；改回：`from=Some(…) to=None` 且提示**回来**。**必须用非原子写法**（PowerShell `Set-Content` 就会"截断 → 写入"，正好覆盖竞态），原子替换测不出这条 |
| 16 | 空目录第一条路：新建向导 | **空目录**起 dev（`VITE_LATTESET_PROJECT=<空目录>`）→ 点 `.new-file` `＋` → 输 `main.tex` + Enter → 向导里选语言/类型 → 点 `[data-testid=wizard-create]` | **打开空目录时不弹根文件选择器**（只有状态栏那条标签）；向导出现且**预览**随选项变；创建后文件落盘（含 `\documentclass`）、树里出现、编辑器打开、状态栏标签**消失**、日志出现 `打开项目…候选 1 个` → `手动编译` → `编译成功`，预览**自动**出图（不用点「编译」）。关掉设置里「新建文件向导」后重来一次：**不弹向导**、得到 0 B 空文件 |
| 17 | 无根文档的静默重探测 | 有 `.tex` 但**没有 `\documentclass`** 的目录 → 确认状态栏出现「未确定根文件 · 点击选择」→ 在编辑器里补上 `\documentclass{article}` + 文档体（输入法见下节）→ 等自动保存（或 Ctrl+S） | 标签**消失**；磁盘上文件已写全；`main.pdf` **自动产出**；日志三段：`不触发编译：尚未确定根文件` → `打开项目…（根文件 Some(…)，候选 1 个）` → `手动编译` → `编译成功`。全程**无弹窗** |

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

这会让 `onDidChangeModelContent` 真实触发 → 应用的 `@change` / 防抖自动保存 / watch / 编译链路全部按真机路径走。

**前置条件：窗口必须在前台**。`document.hasFocus()` 为 false 时，`webview_keyboard` 的 type/press、`document.execCommand("insertText")`、手工构造的 `InputEvent` **三条路都无效**（textarea 的 value 被写上又立刻被 Monaco 的状态管理器收回）：先 `manage_window action=focus`，再确认 `document.hasFocus() === true`。

**外部改文件不等于用户编辑**：外部写盘走的是「静默重载」分支，不触发 `@change`（真机核对：文件 mtime 不变 ⇒ 没有自动保存回写）。要验编辑器这一环就得用上面的 `ed.trigger("mcp","type",…)`。

**备选入口：直接给 EditContext 发 `textupdate`**。当 `ed.trigger` 那条路不合适（或只想"像打字一样"灌一段多行文本）时，注意**真正的输入 sink 不是那个 textarea**：

- Monaco 0.56 用的是 **EditContext**：`.monaco-editor .overflow-guard > div.native-edit-context`，该 div 的 `editContext` 才是它监听的对象；`textarea.ime-text-area` 只是 IME 备份（`webview_keyboard action=type` 会把文本写进它，但 **Monaco 不会消费**——value 一直留在那儿、屏幕无变化，这是最容易误判成"敲进去了"的坑）。
- `ec.updateText(0, len, "…")` **不会**触发 Monaco：它只改 EditContext 自己的文本，不派发 `textupdate`。
- 可行做法：先**真实点击编辑器**（`webview_interact click` 命中 `.view-lines`），确认 `.monaco-editor.focused` 存在，再

```js
const ec = document.querySelector('.native-edit-context').editContext;
ec.dispatchEvent(new TextUpdateEvent('textupdate', {
  updateRangeStart: 0, updateRangeEnd: 0,          // 插入点（替换范围同理）
  text: "\\documentclass{article}\n…",             // 多行可以（Monaco 的 type() 认 \n）
  selectionStart: text.length, selectionEnd: text.length,
}));
```

这会走 Monaco 的 `NativeEditContext → viewController.type()` ⇒ `onDidChangeModelContent` **真实触发**（脏标记 → 自动保存 → watch → 编译全链路，与手打完全同路）。判据不要看 `.view-line`（可能还没重渲染）：直接 `(await import(monacoUrl)).editor.getModels()[0].getValue()` 读模型。

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

**编译产物的时间戳也能当证据（仅旧驱动）**：`tmp/main.fdb_latexmk` 的 mtime 只在 **latexmk** 跑过时才推进——它曾用来区分 Quick 与 Full。⚠ **该文件不再生成**（latexmk 已删除）：现在改用编译结果里的 `kind` 字段与状态栏 `draft` / 「引用待更新」判定两档；`.fls` 同此（两者都是 latexmk 产物，见 [troubleshooting.md](../troubleshooting.md) 的相关小节标注）。

### SyncTeX 精度怎么量（一条命令）

```bash
node scripts/synctex-report.mjs                    # 默认三组样本（multifile / beamer工程 / bench/large）
node scripts/synctex-report.mjs --tolerance=5      # 换个"跳到位"容差
node scripts/synctex-report.mjs --projects=<dir> --recompile
```

它按 `\section`/`\chapter`/`\frametitle`/`\begin{frame}`/`\label`/`\part` 取样本，逐点做「正向 → 反向」往返，报告成功率与行号差。**当前基线**：正向/反向/同文件 34/34、跳到位 ≤3 行 31/34（≤5 行 34/34）；分档与解释见 [design.md](../design.md)。

> ⚠ **这套指标只覆盖"往返"口径**：反向的查询点是**正向给出的坐标**，不是用户在 PDF 上随手点的点。2026-11 验收发现任意点反向在「页尾命令行 / 跨文件页」会退化到页尾行号、跨文件页还会指错文件（`LATTESET_SYNCTEX=cli` 在同数据上正确；`xelatex` 产物同样复现 ⇒ 预存缺陷）。已登记 [modules.md](./modules.md) §12.1 #29；补测任意点反向前，**别把这组数字当"反向定位正确"的证据**。

**beamer 的 2–4 行偏移不是解析 bug**：把取块规则从「第一个 `Output` 块」换成「最小 H」「首个 H≤40」，**往返结果完全一致**（三组样本逐一相同）——偏移来自 beamer/主题的 synctex 记录粒度。同理，`\only<n>` 覆盖层的内容在非本层页面上没有对应记录，反向映射天然不可确定。

**沙箱注意**：脚本用「stdout 重定向到文件」而不是管道（Node 的 `pipe` 在受限沙箱会 EPERM），因此**无需提权**即可运行。

**失败面排查顺序**：① dev stdout 有无 `打开项目` / `触发编译` / `构造编译请求`（后端链路）；② `read_logs(console)` 有无前端异常；③ `ipc_get_backend_state` 确认连接的是本应用。

### 备选：WebDriver 端到端链路

`test_file/e2e/` 用 **WebdriverIO**（`browserName:'wry'` + `tauri:options.application`）驱动 `tauri-driver`（中介）+ `msedgedriver`（Windows，需 `msedgedriver-tool` 安装并匹配 Edge 版本，`--native-driver` 指路径）。`src/App.vue` 提供 `VITE_LATTESET_PROJECT` 钩子自动打开项目（绕过原生目录弹窗，WebDriver 无法驱动）；`PreviewPane.vue` 带重载耗时插桩。**这条链路是半配置状态的备选**——日常验收走上面的 tauri server MCP。

**关键教训（均实测）**：

- **tauri-driver 不打印 "listening"**：`beforeSession` 靠字符串匹配会永久卡住；改成**轮询 127.0.0.1:4444 是否可连**（`net.connect`）。
- **受限沙箱跑不起来**：见上「两个已知边界」；npm 缓存写 `%LocalAppData%` 也被拒，需把 `NPM_CONFIG_CACHE` 指到工作区。
- **读取前端控制台**：pc-control 的 `screenshot` 返回 base64，需解码成图片再读；devtools 快捷键（F12/Ctrl+Shift+I）需窗口聚焦才生效。窗口标题不随 `document.title` 改变（Tauri 不同步），**不要**用它做观测通道。
- **`webview_keyboard` 无法向 Monaco 插入文本**：**不是 MCP 服务 bug**（`type` 能在普通 `<input>` 上写入 `hello`，实测）；**是 Monaco（EditContext 架构）对合成事件不响应的限制**。Monaco 可编辑区是 `.native-edit-context`（DIV，`role=textbox`，走 EditContext）；`press` 只派发 `keydown/keypress/keyup`（**全部 `isTrusted:false`，且不产生 `beforeinput`/`input`**），合成键不触发浏览器默认文本插入，故 Monaco 不写入。`type` 靠 set `value` + `input` 事件，仅对原生 input/textarea 有效，且对 Monaco 唯一 textarea `.ime-text-area`（**readonly** 的 IME 缓冲）无效。**凡是涉及 Monaco 文本编辑的自动化，用真实 OS 按键（pc-control，`isTrusted:true` 触发 EditContext 插入，已实测 `Q` 写入），或改用 Monaco `executeEdits` API**（需实例，MCP 触不到）。
- **e2e 操作要点**：① **pc-control 打字前先 `Ctrl+Space` 关闭中文输入法**——否则 IME 会截走按键转义（实测 `% E2E_MARKER` 被 IME 吞掉/乱插）；② **状态栏「外部修改：文件名」可点击**——点击即从磁盘重载该文件（`acceptExternal`），用于外部修改冲突恢复（没有独立「重载」按钮时，此 span 即入口）；③ 原生目录对话框流程：`Ctrl+L` 聚焦地址栏 → 粘贴路径 → Enter → 再补 `Enter`（连按确认）即可选中并关闭。

**注意**：`test_file/e2e/drivers/`（msedgedriver 二进制）与 `test_file/e2e/node_modules/` 已 gitignore。

## e2e / 工具脚本什么时候用

索引与完整清单见 [scripts/README.md](../../scripts/README.md)；验收常用的几个：

- `bench.mjs`——冷编译 / 空跑 / 编辑触发 / 端到端的延迟预算报告（超预算退出码 1）；真机第 6 步与它拼成端到端。
- `synctex-report.mjs`——SyncTeX 双向精度基线（见上）。
- `validate-pdf.mjs`——PDF 页数 / 文本层断言，内嵌引擎真编复核用（**别在引擎刚退出时立刻量**）。
- `check-determinism.mjs`——同一份源码连跑两次 PDF 是否逐字节相同。
- `editor-report.mjs`——WS 直驱真机的 8 探针（每击键 / 折叠 / 大纲往返 / 内存…），超门槛退出码 1。
- `fls-report.mjs`——`.fls` / `.fdb_latexmk` 依赖差集，排"改了文件没生效"时用。
- `gen-*` 系列——生成夹具（bench 六档 / 大项目 / 流式长文档 / 多文件大项目），夹具不入库、脚本入库。
- `render-diagrams.mjs`——架构图渲染（Mermaid → `docs/diagrams/`）；⚠ 本机跑不通（缺 mermaid + 无头浏览器，ADR-0011 记）。

**脚本失效说明（ADR-0015 之后）**：下列脚本仍直接调用 **latexmk / xelatex**，而产品已不再经过那条路径 ⇒ 它们的数据只能当**历史基线复现**，不能用来判定当前实现：

| 脚本 | 失效点 |
|---|---|
| `bench.mjs` | 命令 `latexmk -xelatex …`；`noop` 靠 latexmk 判定 up-to-date |
| `check-determinism.mjs` | 命令 latexmk / 直调 xelatex，并向子进程注入 `SOURCE_DATE_EPOCH`（与产品现行口径相反） |
| `bench-single-pass.mjs` | A/B 的两条命令都是旧驱动 |
| `synctex-report.mjs` | 内部 `latexmk -xelatex` 重编样本 |
| `fls-report.mjs` / `tl-compile-matrix.ps1` / `gen-log-error-corpus.ps1` | 只处理 latexmk/TeX Live 产物（`.fls`、`.fdb_latexmk`、模板矩阵、GBK 日志语料） |

这些脚本在本机是否还能跑 `[未测]`（需要 TeX Live 2026 + latexmk）；要按 Tectonic 重做需另立改动。

## CI 概览

[.github/workflows/ci.yml](../../.github/workflows/ci.yml) 两个档：

- **check**（`ubuntu-latest`，push / PR）：`npm ci` → `npm run build`（含 `vue-tsc --noEmit`）→ `npm run test` → `cargo test -p latteset-core` → `cargo test -p latteset-infra`。**不跑 `latteset-server`**（server 已非可选依赖内嵌 Tectonic，Linux 上会去探测 pkg-config 的 C 链；R11 决策：挪到 Windows 档）。真实编译的 `#[ignore]` 用例**不在**这里跑。
- **build-windows**（`windows-latest`）：checkout 钉 rev 的 vcpkg → bootstrap → 缓存 `installed/`+`downloads/` → `vcpkg install` 六端口 → `with-tectonic-lib.ps1 -Mode check` → `-Mode test`（**全仓 Rust 测试，含 server 与内嵌 crate**）→ `-Mode tauri build`（含 NSIS 安装包冒烟，ADR-0003 分发路径）。**阶段②起没有独立的 `build-windows-tectonic-lib` 档**——内嵌档就是默认档，代价是每次 push/PR 都要装六端口（缓存命中时只剩校验开销）。内嵌构建细节见 [tectonic-library.md](./tectonic-library.md) §2。
