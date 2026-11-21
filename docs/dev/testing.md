# 测试与验证

怎么跑测试、怎么在真机上验收一次改动。读者是改代码的人；**只有真实窗口才能验的部分**（预览耗时、点击链路、目视渲染）都收在这一份里。

## 怎么跑，每层覆盖什么

| 想验什么 | 命令 | 覆盖 |
|---|---|---|
| 纯逻辑 | `cargo test -p latteset-core` | 调度器语义（队列合并 / 超时 / 重试 / 终止）、`.log` 解析与诊断、设置模型与合并（含 v3 默认值 / 旧 `lib_form` 键忽略）、根文件探测、大纲、SyncTeX 纯逻辑（路径与回落分类）、XDV 页哈希、公式扫描与片段装配 |
| 文件系统 / 存储 / watch | `cargo test -p latteset-infra` | 文件系统实现、notify 监视、设置存储（**v1→v2→v3 迁移**：备份名 / notice 矩阵 / 幂等 / 只读 / 损坏 / BOM）、manifest 解析；子进程 runner 的 32 个用例已随整文件删除 |
| 内嵌 Tectonic（单测） | `cargo test -p latteset-tectonic`（需 vcpkg env；或 `npm run lib:test`） | format 键 / bundle 三类失败文案 / **bundle 来源优先级（显式 > env > 随包 > 在线）与不静默回退** / **在线源预热零网络判定** / 预热门闩与失败静默 / **缓存 GC** / 页哈希接线 + **A 闸门（Full 复用 + `.pdfid` 身份，含反例）** / D3① 外部工具提示 / D3③ 页进度（XDV 完整页）/ D3② 超时证据 / D7 模板提示 / Quick 预算闸门 / SyncTeX 主输入补丁 |
| 内嵌 Tectonic（真编） | `cargo test -p latteset-tectonic -- --ignored` | **闸门 = 编进来 + 有可用 bundle**（不再探测 PATH 上的 `tectonic`）：真编 / **预热后首编（`format_ms=0` + 页哈希一致）** / **随包 `Shipped` verbatim 真编**。T22 复核 **3 个用例全过（17.07 s）**；crate 全集 **65 passed / 3 ignored** |
| headless（CLI / MCP） | `cargo test -p latteset-server`（`-- --ignored` 真编译） | `Session`（打开 / 读写 / 树 / 大纲 / 非 UTF-8 / 编译拒绝 / 配置目录 / D4 资源优先级）；lib **12 passed / 4 ignored** + cli **8 passed**（T22 复核） |
| MCP 协议链路 | `cargo test -p latteset-server --test mcp_stdio` | **真实二进制 + stdio 管道**：握手 / `tools/list` / 工具往返 / 错误语义 / 逐调用切项目（**5 passed + 1 个 `#[ignore]`** 编译用例，闸门同上） |
| 前端 | `npm run test`（`test:watch` 监视） | stores（project/editor/compile/preview/outline）、composables（useAutoSave / useIdleConvergence / useSyncTex / useTheme）、组件（ErrorList / RootFilePicker / SettingsPanel / StatusBar）、services（paths / fuzzy / draftPatch）；T22 复核 **17 files / 161 tests** |
| 全仓 Rust（唯一入口） | `npm run lib:test` | 四个 crate 一起跑：core 269 + infra 40 + server 12+8+5（mcp_stdio）+ tectonic 65；T22 复核 **399 passed / 8 ignored**（8 个真编在各自 `--ignored` 下全过） |
| **性能基线（六档，现行）** | `node scripts/bench-embedded.mjs`（先 `npm run lib:cli`、`node scripts/gen-bench-projects.mjs`） | 内嵌 release CLI 的六档 × `cold-format` / `cold-project` / `noop` / `noop2` / `edit`（A 闸门命中）/ `edit2`（真实改动），跨项目 format 命中、进程地板；每档 3 次中位 + min/max + 原始值；输出 JSON + Markdown；超预算/复现差>10%/档失败退出码 1。口径、预算与未覆盖项见 §性能基线 |
| 交付前 | `npm run build` | `copy:pdf-worker` + `vue-tsc --noEmit` + `vite build` |
| 绑定生成物 | 启动一次 debug 构建自动导出 `src/bindings.ts`（Tauri 测试二进制在本机起不来，`export_bindings` 捷径不可用）；T22 复核 **13/13 PASS**（命令 **19 条**，含 `openLicenses` / 事件 9 个与 Rust 一致） |

模块分工与各用例的判据见 [modules.md](./modules.md)；**现行**性能基线用 `node scripts/bench-embedded.mjs`（内嵌 release CLI，见下「性能基线（六档）」），`node scripts/bench.mjs` 是 latexmk/xelatex 时代的历史脚本（见「脚本失效说明」）。

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

## 性能基线（六档，内嵌 release CLI）

阶段③起的**性能事实来源**。测量仪器 = `scripts/bench-embedded.mjs`（夹具由 `scripts/gen-bench-projects.mjs` 生成、不入库；脚本入库）。

```powershell
$env:TMP = $env:TEMP = '<repo>\src-tauri\target\.tmp'   # 受限沙箱会话才需要（Low 完整性标签）
npm run lib:cli                                        # release CLI（先 npm run lib:check）
node scripts/gen-bench-projects.mjs                    # 六档夹具，首次/重建时
node scripts/bench-embedded.mjs                        # 六档 × 3 次 → JSON + Markdown（约 2–3 分钟）
node scripts/bench-embedded.mjs --compare=test_file/phase3/baseline.json \
  --out=test_file/phase3/baseline-run2.json --md=test_file/phase3/BASELINE-run2.md   # 第二遍做复现比较
node scripts/bench-embedded.mjs --tiers=tiny,thesis    # 只跑指定档（跨项目 format 命中按给定顺序）
```

**口径**（每个采样内部按顺序跑；每档 3 次取中位，另记 min/max 与全部原始值）：

| 口径 | 做法 | 预期 |
|---|---|---|
| `cold-format` | 清 `<cache>/formats` + 项目 `tmp/` 与主 PDF → Full | 真首编，含 ≈1 s format 生成 |
| `cold-project` | 保留 formats、只清项目产物 → Full | format 命中（`format_ms=0`） |
| `noop` / `noop2` | 紧接再 Full 两次（无改动） | 热路径稳态；`noop` 与 `noop2` 中位差应 ≤3% |
| `edit` | **注释级编辑**（不改排版）→ `--quick` | A 闸门命中：`reused_pdf=true`，跳过 XDV→PDF 转换 |
| `edit2` | **真实改动**（正文插一句话）→ `--quick` | A 不命中、转换照跑；多文件/目录档通常 2 趟 |
| `cross-project` | 每档第一次编译时 format 由上一档留下 | 跨项目 format 命中（首档除外） |
| 进程地板 | `help` / `settings` / `open` 各 3 次 | release CLI 启动地板 ≈10 ms |

- **测量字段**：`elapsed_ms` 取 CLI JSON（产品口径）；墙钟由 Node `spawnSync` 前后 hrtime 得到（进程开销 = 墙钟 − elapsed）；分阶段取 stderr 的 `库形态分阶段耗时` 行，其中 `format_ms` / `typeset_ms` / `convert_ms` / `engine_total_ms` 是引擎段，`prep_ms`（bundle 打开 + 主文件读）/ `post_ms`（PDF 原子替换 + 页哈希缓存写）/ `preview_ms`（编译中流式出图累计，计入 typeset 窗口）是 T14 新增的**归因打点**（只加日志，不改产品语义）。
- **预算（[design.md](../design.md) §2.6，**分类口径已由产品负责人采纳**）**：按**实测页数**分类——≤8 页 = 小文档（1 s 优秀 / 2 s 及格）、>8 页 = 大文档（3 s 优秀 / 5 s 及格）。用于两处判定：`cold-format` 的 `elapsed_ms` 中位、以及`编辑→出图 e2e = debounce(500) + edit2 中位 + preview 估算(100)`。**T22 环境税清零版**：`tiny`/`small-article` cold PASS + e2e EXCELLENT；`graphics` 小文档 cold 1.50 PASS / e2e 0.90 EXCELLENT；`thesis` EXCELLENT；`multifile`（74 页）归大文档后 cold 2.94 s / e2e 2.01 s 均 EXCELLENT（cold 距 3 s 仅 64 ms，**临界**）。**已知硬缺口**：`large`（125 页）cold 6.41 s 超 5 s（保留 5 s 线、不放宽，登记于 [design.md](../design.md) §2.8）；T19 绝对值含 ~244 ms/进程 fontconfig 税，**不得跨环境比较绝对值**。
- **退出码**：`0` 通过；`1` 有档超预算 / 两轮复现中位差 >10%（`--compare=`）/ 有档编译失败；`2` 前置缺失（manifest 或 release CLI）。`--allow-over-budget` 只记录不判红（用于「先取基线数字」）。
- **可复现要求**：两轮独立运行**同档同口径中位差 <10%**；单轮内 3 次离群（>20%）**保留并标注**，不剔除。
- **证据**：`test_file/phase3/BASELINE.md`（报告，可直接摘进 design.md §2.8）+ `baseline.json`（原始数据）+ `logs-embedded/<tier>/`（每次编译的 CLI JSON 与 stderr）。**这些是工作区证据，不入库。**
- **bundle 覆盖**：默认本地目录 bundle `test_file/tectonic-bundle`（`--bundle=` 可换）；bundle 必须覆盖六档夹具（`small-article` 需要 `lmroman9-regular` 等字体，见 T14b 的 bundle 补全）。

## e2e / 工具脚本什么时候用

索引与完整清单见 [scripts/README.md](../../scripts/README.md)；验收常用的几个：

- **`bench-embedded.mjs`**——**现行**性能基线（内嵌 Tectonic release CLI，六档 × cold-format/cold-project/noop/noop2/edit/edit2 + 跨项目 format + 进程地板）；超预算/复现差>10% 退出码 1。见上节。
- `bench.mjs`——⚠ **历史**：latexmk/xelatex 时代的冷编译 / 空跑 / 编辑触发 / 端到端报告（数据只能当历史基线复现）。
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
- **build-windows**（`windows-latest`）：checkout 钉 rev 的 vcpkg → bootstrap → 缓存 `installed/`+`downloads/` → `vcpkg install` 六端口 → `with-tectonic-lib.ps1 -Mode check` → **无条件 `Build release CLI`（`-Mode cli`，T22 B2：它是代表集 bundle 生成与离线首编的共同前置，不得塞进缓存分支）** → **代表集缓存/重建（`test_file/tectonic-bundle`，digest `9e8c63ca…`；未命中则 `gen-bench-bundle.mjs` 重建）** → `make-bundle.mjs` 打包产品 zip + 自校验 → **离线首编（不可用代理 + 随包 zip + validate-pdf）** → **许可资源门禁 `make-notices.mjs --check`** → `-Mode test`（全仓 Rust 测试）→ `-Mode tauri build`（NSIS/MSI）→ **安装包体积门禁（≤200 MB，记录进 `installer-size.json`）**。阶段②起没有独立的 `build-windows-tectonic-lib` 档。内嵌构建细节见 [tectonic-library.md](./tectonic-library.md) §2。

## ③段 T16：离线首编与产品精简宏包集（随安装包）

**产品默认 = 随包精简集**（ADR-0016 候选 A）。下面三条是本机/CI 的验收命令；**前置**：`npm run lib:cli`（release CLI）+ 产品 zip 已生成（`node scripts/make-bundle.mjs`，源目录 `test_file/tectonic-bundle` 由 `scripts/gen-bench-bundle.mjs` 生成）。

```powershell
# ① 产品 zip：确定性打包 + 自校验 + 体积上限（CI 门禁，raw zip ≤120 MiB）
node scripts/make-bundle.mjs --max-zip-mb=120 ; node scripts/make-bundle.mjs --verify

# ② 断网首编（内置集 + 空缓存 + 不可用代理 ⇒ 能编过就证明全程没走网络）
$proj = "$env:TEMP\offline-first"; New-Item -ItemType Directory -Force $proj | Out-Null
Set-Content "$proj\main.tex" -Encoding utf8 -Value @'
\documentclass[UTF8]{ctexart}
\begin{document}
断网首编：你好，Latteset。\par
{\heiti 黑体} {\kaishu 楷体} {\fangsong 仿宋} {\bfseries 加粗}
\end{document}
'@
$env:LATTESET_TECTONIC_BUNDLE = "file:///$((Resolve-Path 'src-tauri/resources/tectonic-bundle.zip').Path.Replace('\','/'))"
$env:LATTESET_TECTONIC_CACHE  = "$env:TEMP\offline-cache"
$env:HTTPS_PROXY = 'http://127.0.0.1:9'; $env:HTTP_PROXY = 'http://127.0.0.1:9'
src-tauri/target/release/latteset-cli.exe --project $proj compile
node scripts/validate-pdf.mjs --pdf "$proj/main.pdf" --expect-pages 1 --expect-text '你好' --expect-text '黑体'

# ③ 二次编译（预热后应明显更快）与安装包体积（NSIS 冒烟，≤200 MB 产品上限）
src-tauri/target/release/latteset-cli.exe --project $proj compile
Get-Item src-tauri/target/release/bundle/nsis/*.exe | Select-Object Name,Length
```

- **验收判据（T16 首验 + T19 终版）**：断网首编 **exit 0 / 4.4 s**（T19；中文 Fandol 四款 + `validate-pdf` 全 PASS，0 U+FFFD）；二次 1.6 s（T16）/ 1.2 s（T19）；缺包文档给可操作提示且**不自动换源**，显式切「在线完整」后 exit 0（首编含联网取包 344 s）；**最终安装包（T19）NSIS 88.46 MiB / MSI 109.63 MiB（≤200 MB 门禁）**。证据 `test_file/phase3/t16-verify/`、`test_file/phase3/t19-tauri-build.log`。
- **不在本机验的**：`cargo test -p latteset`（外壳测试二进制在本机启动即失败，§已知边界）⇒ bundle 来源优先级/GC/预热的单测放在 `latteset-tectonic`（`cargo test -p latteset-tectonic`，T22 复核 **65 passed / 3 ignored**），外壳侧只做 `cargo check -p latteset --tests` 编译校验。
- **bundle 内容/覆盖/体积**：见 `test_file/phase3/BUNDLE.md`；**上线前许可门禁**见 [tectonic-licensing.md](./tectonic-licensing.md) §8。

## ③段 T21：第三方声明与 GPL 字体源码（G-L3 验收）

**产物落点**：`src-tauri/resources/licenses/`（随安装包，`bundle.resources` 映射到安装目录 `licenses/`）。**入库清单**：`THIRD-PARTY-NOTICES.md`、`manifest.json`、`LICENSES/`（规范全文 + 去重后的 Rust/C 依赖许可文本）、`sources/`（Fandol 六款字体源码 + zapfding AFM）、`patches/`（Tectonic 对 TL 的 patch）、`data/`（可复核清单：`meta.json` / `bundle-packages.json` / `rust-deps.json` / `c-deps.json` / `patched-tl-files.json` / `sources.json`）。

```powershell
# ① 离线确定性复核：NOTICE/manifest 必须与 data+LICENSES+sources 逐字节一致（CI 门禁）
node scripts/make-notices.mjs --check

# ② 关键完整性（--check 已内置，手工抽查等价）
Get-ChildItem src-tauri/resources/licenses/sources/fandol -Recurse -File -Filter cidfont.ps   # 6 个
Get-ChildItem src-tauri/resources/licenses/sources/zapfding; Get-Item src-tauri/resources/licenses/sources/zapfding/pzdr.afm
Get-Content src-tauri/resources/licenses/THIRD-PARTY-NOTICES.md -TotalCount 20

# ③ 安装包内可见（G-L3 验收 = 解包可见；About 入口由 T21b frontend 接，资源路径 licenses/）
Get-Item src-tauri/target/release/bundle/nsis/*.exe | Select-Object Name,Length
# 解包抽查（7z/WinRAR 任意）确认 licenses/THIRD-PARTY-NOTICES.md 与 licenses/sources/fandol/*/cidfont.ps 存在

# ④ 换 bundle / 换依赖后的重采集（开发机；需 TeX Live 2026 + cargo registry + vcpkg installed + tectonic-src）
node scripts/make-notices.mjs --collect
git status --short src-tauri/resources/licenses   # 采集结果必须整体入库、review diff
```

- **验收判据（本机实测 2026-11-20）**：`--collect` 生成 308 文件 / 48.95 MiB（NOTICE 16.2 KB；Rust 389 crate 许可去重 261 份文本；C 依赖 11 port 全文；Fandol 6 套源码 47.77 MiB）；`--check` 重算逐字节一致；安装包内 `licenses/` 可见；NSIS 体积见下方新数字；G-L1..L9 状态见 [tectonic-licensing.md](./tectonic-licensing.md) §5。
- **残余风险（已登记，不谎报）**：Fandol 源码取自社区镜像 `Yixf-Self/fandol-fonts@b9330082…`（上游团队未单独发布源码包）；**未做「源码 → OTF」重建验证**（需 Adobe AFDKO）；Knuth 许可正文取自 TL 的 CM 源文件头声明（该许可本身是短声明）。见许可审计 §8.6。

## ③段 T17/T18/T19 + T22 受控重测：六档基线、预热与门禁（2026-11-21）

**六档基线**（命令见上「性能基线（六档）」）：**T22 环境税清零版**——release CLI、同夹具/bundle，六档 × 每口径 3 次 × **三轮独立运行**取"中位的中位"；测量期用最小合法 fontconfig 配置 + 可写缓存目录清除了 ~0.24 s/进程宿主字体扫描税（仅测量环境变量、产品不变）；同档同口径三轮极差除 tiny 两条热路径（绝对 21–24 ms）外 ≤7.2%。完整数字进 [design.md](../design.md) §2.8，原始数据 `test_file/phase3/BASELINE.md` §0.1（`baseline-fc-run1.json` / `baseline-fc.json` / `baseline-fc-run3.json`）。**预算分类按实测页数**（≤8 页 small / >8 页 large，产品负责人已采纳）：`multifile`（74 页）归 large 后 cold 2.94 s / e2e 2.01 s 均 EXCELLENT（cold 距 3 s 仅 64 ms，**临界**）；**唯一硬缺口 `large` cold 6.41 s 超 5 s（保留 5 s 线、不放宽）**。**T14→T19 的 +200–240 ms/进程差已归因：fontconfig 宿主字体扫描环境税（2026-11-21）**——T19 绝对值含税、跨版本绝对比较作废；T17 相对收益仍有效：L4b（A 闸门扩 Full）`noop` −7.9%…−51.6%；L5（预览默认 1 帧）`preview_ms` 311→150 / 266→131 ms。

**首启预热 + 启动分段**（T18 实现、T19 复测）：

```powershell
# release GUI 直启（隔离 cache/config），RUST_LOG=info 读分段日志
$env:TMP = $env:TEMP = '<repo>\src-tauri\target\.tmp'
$env:LATTESET_TECTONIC_CACHE = '<repo>\test_file\phase3\tcache-gui'
$env:RUST_LOG = 'info'
& '<repo>\src-tauri\target\release\latteset.exe'
# 期望日志：startup 分段：watcher 已启动 → setup 完成 → setup 末尾（后台预热已派发）→ webview Ready
#           首启 format 预热完成 prewarm_ms=…；<cache>/formats/<digest>-latex-33.fmt 出现
```

- 验收判据（T19）：进程→setup/Ready **126 ms**、后台预热 **992 ms**（不挡窗口）、`.fmt` **24,451,466 B** 落盘、首编 **`format_cached=true / format_ms=0`**；温启动 **144–171 ms**（`AlreadyCached`）；预热前后页哈希逐页相等（`--ignored` 真编用例 `prewarm_then_first_compile_hits_format_cache`）。
- **两个已修缺陷（T19）**：① `lib:test` 特性耦合（`network-bundle` 打开后，在线源 + 冷缓存时预热会联网）→ `bundle::prewarm_plan` 零网络零写判定；② 随包 zip 的 Windows verbatim 路径（`\\?\…`）⇒ `path_to_file_url` 未剥前缀、`Url::to_file_path()` 失败 → 修复后预热/默认离线编译双绿（单测 + `Shipped` 真编）。**T22 B2**：CI 的 release CLI 构建改为**无条件前置**（代表集缓存命中但 rust-cache 无 release 产物时，离线首编曾必红）。**T22 B3**：GC / runner / 预热共用 `resolve_cache_dir`（设置 > env > 宿主），当前 digest 由同一 bundle 解析器给出、解析不出时 GC 整体不动。
- **门禁数字（T22 复核）**：core 269 / infra 40 / tectonic **65+3ign** / server **12+4ign+8** / mcp_stdio **5+1ign** / `lib:test` **399 passed / 8 ignored**；`npm run build` 0 error；vitest **17 files / 161 tests**；bindings **13/13**（命令 19 条）；安装包体积 **NSIS 88.46 MiB / MSI 109.63 MiB ≤200 MB**。
- **未覆盖**（详见 `test_file/phase3/REPORT-gates.md` §8）：NSIS 安装版冷/温启动与窗口内可编辑/About 点击 `[未测-会话环境限制]`；在线 → 随包**反向**切换未单独重测；真实超时真编、makeindex/glossaries 真编 `[未测]`；GC 与预热/编译的**并发对抗样例**未构造（单测已覆盖保留当前/解析不出不动）。**每进程 +200–240 ms 已归因：fontconfig 环境税（2026-11-21），不是代码回归。**
