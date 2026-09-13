#!/usr/bin/env node
// 编辑器侧性能口径（roadmap ⑦c 的 DoD）：一条命令在**真实窗口**里跑完探针并判门槛。
//
// 用法（前置：`npm run tauri dev` 已起窗口，且项目 = 对应档夹具）：
//   VITE_LATTESET_PROJECT=<...>/editor/multi20 npm run tauri dev
//   node scripts/editor-report.mjs --tier multi20 [--port 9223] [--json out.json] [--quiet]
// 退出码：0 = 全部门槛通过；1 = 有门槛未过（与 bench.mjs 同风格）。
//
// 为什么脚本能自己驱动真机（零第三方依赖）：MCP Bridge 插件在应用侧监听
// `ws://0.0.0.0:9223`，协议是 `{id, command:'execute_js', args:{script, windowLabel}}` →
// `{id, success, data}`（见 @hypothesi/tauri-mcp-server dist/driver/webview-executor.js）。
// Node 22+ 自带 WHATWG `WebSocket`，因此不需要 npm 依赖、也不需要 MCP 会话在场。
//
// 探针**不改产品源码**：用 `performance.getEntriesByType('resource')` 找到 Vite 已加载模块的 URL，
// `await import(url)` 拿到同一模块实例，从而直接调 store / provider / ipc（方法同 p1 分析 §2）。
//
// ⚠️ 模块 URL 的两个实测坑（2026-09）：
//   1. **Vite 预打包的依赖没有源码路径**：Monaco 在 dev 下是 `/node_modules/.vite/deps/monaco-editor.js?v=…`
//      （不是 `/monaco-editor/esm/vs/editor/editor.api.js`）——needle 写错会匹配到 `.css`/`worker`，
//      导入后 `monaco.editor` 为 undefined。诊断用 `--eval-file`。
//   2. **只能用它加载时的那个 URL**：Monaco 的 model 注册表是模块级单例，换一个 query 会拿到新实例、
//      看不见已存在的 model。所以取的是 resource 里实际出现过的 URL，不做归一化。
//
// ⚠️ 改探针时注意：探针体是 **String.raw 模板字符串**，里面的注释/字符串**不能出现反引号**
//    （会提前终止模板，报 SyntaxError: Unexpected identifier）。
//
// 口径（沿用 p1 分析 §2 的方法论）：
//   - A/B 顺序受控交替 3 轮、丢弃第 1 轮（首轮 JIT 预热会把差高估 5–10 倍）；
//   - 不用 `slice()` 代理 `getValue()`；
//   - 击键走 `editor.trigger('keyboard','type')`（MCP 的 webview_keyboard 打不进 Monaco 的 EditContext）。

import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, "..");
const MANIFEST = join(REPO, "test_file", "projects", "editor", "editor-manifest.json");

// ---------------------------------------------------------------- 参数

function parseArgs(argv) {
  const out = { tier: "multi20", port: 9223, host: "127.0.0.1", json: null, quiet: false, keys: 200, evalFile: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--tier") out.tier = argv[++i];
    else if (a === "--port") out.port = Number(argv[++i]);
    else if (a === "--host") out.host = argv[++i];
    else if (a === "--json") out.json = argv[++i];
    else if (a === "--keys") out.keys = Number(argv[++i]);
    else if (a === "--quiet") out.quiet = true;
    // 调试入口：把文件内容当探针发过去，打印返回值（排"模块 URL/字段名变了"这类问题）
    else if (a === "--eval-file") out.evalFile = argv[++i];
  }
  return out;
}

// ---------------------------------------------------------------- WS 会话

class Session {
  constructor(ws) {
    this.ws = ws;
    this.seq = 0;
    this.pending = new Map();
    ws.addEventListener("message", (ev) => {
      let msg;
      try {
        msg = JSON.parse(typeof ev.data === "string" ? ev.data : String(ev.data));
      } catch {
        return;
      }
      const p = this.pending.get(msg.id);
      if (!p) return;
      this.pending.delete(msg.id);
      clearTimeout(p.timer);
      p.resolve(msg);
    });
  }

  static connect(host, port, timeoutMs = 5000) {
    return new Promise((ok, fail) => {
      let ws;
      try {
        ws = new WebSocket(`ws://${host}:${port}`);
      } catch (e) {
        fail(new Error(`无法创建 WebSocket：${e.message}`));
        return;
      }
      const timer = setTimeout(() => fail(new Error(`连接 ${host}:${port} 超时——应用没在跑？（先 npm run tauri dev）`)), timeoutMs);
      ws.addEventListener("open", () => {
        clearTimeout(timer);
        ok(new Session(ws));
      });
      ws.addEventListener("error", () => {
        clearTimeout(timer);
        fail(new Error(`连接 ${host}:${port} 失败——应用没在跑？（先 npm run tauri dev）`));
      });
    });
  }

  /** 执行一段 JS（IIFE 或表达式），返回可序列化结果。 */
  async eval(script, timeoutMs = 120000) {
    const id = `er_${++this.seq}_${Date.now()}`;
    const res = await new Promise((ok, fail) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        fail(new Error(`探针超时（${timeoutMs}ms）：${script.slice(0, 60)}…`));
      }, timeoutMs);
      this.pending.set(id, { resolve: ok, timer });
      this.ws.send(JSON.stringify({ id, command: "execute_js", args: { script, windowLabel: "main" } }));
    });
    if (!res.success) throw new Error(`探针失败：${res.error ?? JSON.stringify(res).slice(0, 200)}`);
    return res.data;
  }

  close() {
    try {
      this.ws.close();
    } catch {
      /* ignore */
    }
  }
}

// ---------------------------------------------------------------- 探针

/** 引导：模块加载器 + 内存读数（注入一次，后续探针都用它）。 */
const BOOTSTRAP = String.raw`
window.__perf = (() => {
  const cache = new Map();
  const urls = () => performance.getEntriesByType("resource").map((e) => e.name);
  async function mod(...needles) {
    for (const needle of needles) {
      if (cache.has(needle)) return cache.get(needle);
      const hit = urls().filter((n) => n.includes(needle));
      if (!hit.length) continue;
      // 优先无 query 的 URL：带 ?t= 的 HMR 版本会被当成另一个模块，重复执行会重建 store
      const url = hit.find((n) => !n.includes("?")) || hit[0];
      const m = await import(url);
      cache.set(needle, m);
      return m;
    }
    throw new Error("模块未加载：" + needles.join(" | "));
  }
  const mem = () => (performance.memory ? performance.memory.usedJSHeapSize : 0);
  const memTotal = () => (performance.memory ? performance.memory.totalJSHeapSize : 0);
  // 堆读数会被 GC 抖动（实测出现过"打开 41 个标签后堆反而少了 11MB"）——静置后取 3 次中位
  const memStable = async (waitMs = 600) => {
    await new Promise((r) => setTimeout(r, waitMs));
    const s = [mem(), mem(), mem()].sort((a, b) => a - b);
    return s[1];
  };
  const mb = (b) => Math.round((b / 1048576) * 100) / 100;
  // JS 字符串的 .length 是 UTF-16 码元数：中文文本下会比 UTF-8 字节数小约 3 倍 —— 规模口径一律用它
  const utf8 = (s) => new Blob([s]).size;
  return { mod, mem, memTotal, memStable, mb, utf8, urls };
})();
"ready"`;

/** P0 让窗口切到本档夹具（不依赖 VITE_LATTESET_PROJECT，一个会话里可连跑多档）。 */
const P0_SWITCH = String.raw`
(async () => {
  const { mod } = window.__perf;
  const { useProjectStore } = await mod("/src/stores/project.ts");
  const ps = useProjectStore();
  const want = "__TIER_DIR__";
  const norm = (p) => (p || "").replace(/\\/g, "/").toLowerCase();
  const texCount = () => (ps.tree || []).filter((e) => !e.is_dir && /\.tex$/i.test(e.name)).length;
  if (norm(ps.root) === norm(want) && texCount() >= __TIER_TEX_COUNT__) {
    return { switched: false, root: ps.root, texFiles: texCount() };
  }
  await ps.openProject(want);
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline && !(norm(ps.root) === norm(want) && texCount() >= __TIER_TEX_COUNT__)) {
    await new Promise((r) => setTimeout(r, 100));
  }
  await new Promise((r) => setTimeout(r, 400)); // 等 watch/大纲稳定
  return { switched: true, root: ps.root, texFiles: texCount() };
})()`;

/** P1 环境与项目：文件数、树规模、已打开标签、常驻内存。 */
const P1_ENV = String.raw`
(async () => {
  const { mod, mem, mb } = window.__perf;
  const { useProjectStore } = await mod("/src/stores/project.ts");
  const { useEditorStore } = await mod("/src/stores/editor.ts");
  const ps = useProjectStore();
  const es = useEditorStore();
  const walk = (nodes) => (nodes || []).reduce((a, n) => a + 1 + walk(n.children), 0);
  const tree = ps.tree || [];
  const tex = tree.filter((e) => !e.is_dir && /\.tex$/i.test(e.name));
  return {
    root: ps.root,
    rootFile: ps.project?.root_file ?? null,
    treeNodes: walk(tree),
    texFiles: tex.length,
    openTabs: es.tabs.length,
    memMB: mb(mem()),
  };
})()`;

/** P2 打开全部 .tex 标签（真实路径：IPC 读盘 + store + Monaco model）。 */
const P2_OPEN_ALL = String.raw`
(async () => {
  const { mod, memTotal, memStable, mb, utf8 } = window.__perf;
  const monaco = await mod("deps/monaco-editor.js", "/monaco-editor/esm/vs/editor/editor.api.js", "/monaco-editor/esm/vs/editor/editor.main.js");
  const { useProjectStore } = await mod("/src/stores/project.ts");
  const { useEditorStore } = await mod("/src/stores/editor.ts");
  const ps = useProjectStore();
  const es = useEditorStore();
  // 可重复运行：先关掉所有已打开标签，保证测到的是"从零打开全部文件"（否则第二次跑只剩切换成本）
  for (const t of [...es.tabs]) {
    try { es.closeTab(t.path); } catch (e) { /* 忽略单个失败 */ }
  }
  await new Promise((r) => requestAnimationFrame(() => r()));
  const files = (ps.tree || [])
    .filter((e) => !e.is_dir && /\.tex$/i.test(e.name))
    .map((e) => e.path)
    .sort();
  const modelsBefore = monaco.editor.getModels().length;
  const before = await memStable(300);
  const perFile = [];
  const t0 = performance.now();
  for (const f of files) {
    const t = performance.now();
    try { await es.openFile(f, 1); } catch (e) { /* 单个失败不打断整体 */ }
    perFile.push(Math.round((performance.now() - t) * 100) / 100);
  }
  const storeMs = Math.round(performance.now() - t0);
  // openFile 只做「IPC 读盘 + store.buffers」；Monaco model 由 EditorPane 在 activePath 变化时
  // 按需创建（getModel → createModel → setModel）——等它建完，才是"打开 N 个标签"的端到端成本。
  const t1 = performance.now();
  const deadline = Date.now() + 20000;
  while (monaco.editor.getModels().length - modelsBefore < files.length && Date.now() < deadline) {
    await new Promise((r) => requestAnimationFrame(() => r()));
  }
  const modelsReadyMs = Math.round(performance.now() - t1);
  const models = monaco.editor.getModels();
  let bufBytes = 0;
  for (const [, content] of es.buffers) bufBytes += utf8(content);
  const after = await memStable(600);
  return {
    count: files.length,
    storeMs,
    modelsReadyMs,
    e2eMs: storeMs + modelsReadyMs,
    perFileMs: perFile,
    medianPerFileMs: perFile.slice().sort((a, b) => a - b)[Math.floor(perFile.length / 2)] ?? 0,
    modelsBefore,
    modelsAfter: models.length,
    modelsCreated: models.length - modelsBefore,
    bufferMB: mb(bufBytes),
    memBeforeMB: mb(before),
    memAfterMB: mb(after),
    memDeltaMB: mb(after - before),
    memTotalMB: mb(memTotal()),
  };
})()`;

/**
 * P3 每击键 A/B（顺序受控交替 3 轮，丢弃首轮）。
 * A = 真实编辑器实例（含我们的 onDidChangeModelContent → getValue + markDirty），
 * B = 同内容、同语言的裸 Monaco。
 * 两个模型都是**临时模型**（内容 = 活动文件），测完还原 —— 不污染用户文档。
 */
const P3_PER_KEY = String.raw`
(async () => {
  const { mod, utf8 } = window.__perf;
  const monaco = await mod("deps/monaco-editor.js", "/monaco-editor/esm/vs/editor/editor.api.js", "/monaco-editor/esm/vs/editor/editor.main.js");
  // 先把活动文件切到 fixture 的 editTarget（288KB 的章）：打开全部标签后停在的是排序最后的
  // main.tex（几百字节），在它上面测每击键/折叠只会得到无意义的数字（2026-09 实测踩过）。
  const { useEditorStore } = await mod("/src/stores/editor.ts");
  const es = useEditorStore();
  if ("__EDIT_TARGET__") {
    try { await es.openFile("__EDIT_TARGET__", 1); } catch (e) { /* 切不过去就按当前文件测 */ }
    await new Promise((r) => requestAnimationFrame(() => r()));
    await new Promise((r) => setTimeout(r, 50));
  }
  const host = document.createElement("div");
  host.style.cssText = "position:fixed;left:-9999px;top:0;width:900px;height:600px";
  document.body.appendChild(host);

  const editors = monaco.editor.getEditors ? monaco.editor.getEditors() : [];
  const real = editors.find((e) => e.getModel() && /\.tex$/.test(e.getModel().uri.path)) || editors[0] || null;
  if (!real) throw new Error("找不到真实编辑器实例（monaco.editor.getEditors() 为空）");

  const original = real.getModel();
  const text = original.getValue();
  const lang = original.getLanguageId ? original.getLanguageId() : "latex";
  const tmpA = monaco.editor.createModel(text, lang);
  // 裸编辑器的 model 要单独存引用：editor.dispose() 不会 dispose 它，漏掉会污染后面的 model 计数
  const tmpB = monaco.editor.createModel(text, lang);
  const bare = monaco.editor.create(host, { model: tmpB, language: lang, minimap: { enabled: false } });

  const N = ${"__KEYS__"};
  const measure = (ed) => {
    const t0 = performance.now();
    for (let i = 0; i < N; i++) ed.trigger("keyboard", "type", { text: "x" });
    return (performance.now() - t0) / N;
  };

  const longtasks = [];
  const po = new PerformanceObserver((list) => { for (const e of list.getEntries()) longtasks.push(Math.round(e.duration * 100) / 100); });
  try { po.observe({ entryTypes: ["longtask"] }); } catch (e) { /* 不支持则跳过 */ }

  const roundsA = [], roundsB = [];
  real.setModel(tmpA);
  try {
    for (let r = 0; r < 5; r++) {           // 交替 5 轮：A,B,A,B…（首轮整体丢弃；实测轮间漂移明显）
      roundsA.push(measure(real));
      roundsB.push(measure(bare));
    }
  } finally {
    real.setModel(original);
    po.disconnect();
    bare.dispose();
    tmpA.dispose();
    tmpB.dispose();
    host.remove();
  }
  const med = (a) => a.slice().sort((x, y) => x - y)[Math.floor(a.length / 2)];
  const warm = (a) => a.slice(1); // 丢弃第 1 轮（JIT 预热）
  const A = med(warm(roundsA)), B = med(warm(roundsB));
  return {
    keys: N,
    roundsA: roundsA.map((v) => Math.round(v * 1000) / 1000),
    roundsB: roundsB.map((v) => Math.round(v * 1000) / 1000),
    aMs: Math.round(A * 1000) / 1000,
    bMs: Math.round(B * 1000) / 1000,
    netMs: Math.round((A - B) * 1000) / 1000,
    modelBytes: utf8(text),
    longtasks,
    longtaskMaxMs: longtasks.length ? Math.max(...longtasks) : 0,
  };
})()`;

/** P4 折叠提供者（我们自己的全量扫描）。 */
const P4_FOLDING = String.raw`
(async () => {
  const { mod, utf8 } = window.__perf;
  const monaco = await mod("deps/monaco-editor.js", "/monaco-editor/esm/vs/editor/editor.api.js", "/monaco-editor/esm/vs/editor/editor.main.js");
  const { latexFoldingProvider } = await mod("/src/latexSuggest.ts");
  const editors = monaco.editor.getEditors ? monaco.editor.getEditors() : [];
  const real = editors.find((e) => e.getModel() && /\.tex$/.test(e.getModel().uri.path)) || editors[0];
  if (!real) throw new Error("找不到真实编辑器实例");
  // 依赖 P3 已把活动文件切到 editTarget（288KB 章）：模型太小会让这个数字失去意义。
  const model = real.getModel();
  const runs = [];
  for (let i = 0; i < 5; i++) {
    const t = performance.now();
    await latexFoldingProvider.provideFoldingRanges(model);
    runs.push(Math.round((performance.now() - t) * 100) / 100);
  }
  return { runsMs: runs, medianMs: runs.slice().sort((a, b) => a - b)[2], modelBytes: utf8(model.getValue()) };
})()`;

/** P5 大纲往返（真实 IPC + Rust 解析；① 全量首扫 ② 无变化增量）。 */
const P5_OUTLINE = String.raw`
(async () => {
  const { mod, utf8 } = window.__perf;
  const { useEditorStore } = await mod("/src/stores/editor.ts");
  const { ipc } = await mod("/src/services/ipc.ts");
  const es = useEditorStore();
  const openPaths = [];
  const all = [];
  for (const [path, content] of es.buffers) { openPaths.push(path); all.push({ path, content }); }
  let totalBytes = 0;
  for (const b of all) totalBytes += utf8(b.content);
  // 「编辑了一个文件」的稳态样本要取那个大章（不是 main.tex）
  const targetName = "__EDIT_TARGET__".split("/").pop() || "";
  const picked = all.find((b) => b.path.endsWith("/" + targetName) || b.path.endsWith("\\" + targetName)) || all[all.length - 1];
  const one = picked ? [picked] : [];

  const timeIt = async (fn) => { const t = performance.now(); await fn(); return Math.round((performance.now() - t) * 100) / 100; };
  const med = (a) => a.slice().sort((x, y) => x - y)[Math.floor(a.length / 2)];

  // 三条路径必须分开报（否则会把冷路径当成稳态）：
  //   full     = 全部缓冲 → 项目打开 / 根文件切换那一次
  //   incrOne  = 1 个变化缓冲 → **⑦a 的稳态场景**（编辑一个文件后编译成功）
  //   incrZero = 0 个变化缓冲 → 编译成功但没编辑
  const full = [], incrOne = [], incrZero = [];
  for (let i = 0; i < 3; i++) full.push(await timeIt(() => ipc.getOutline(all, null, openPaths)));
  for (let i = 0; i < 3; i++) incrOne.push(await timeIt(() => ipc.getOutline(one, null, openPaths)));
  for (let i = 0; i < 3; i++) incrZero.push(await timeIt(() => ipc.getOutline([], null, openPaths)));
  return {
    openPaths: openPaths.length,
    bufferMB: Math.round((totalBytes / 1048576) * 100) / 100,
    editedFileBytes: picked ? utf8(picked.content) : 0,
    fullMs: full, fullMedianMs: med(full),
    incrOneMs: incrOne, incrOneMedianMs: med(incrOne),
    incrZeroMs: incrZero, incrZeroMedianMs: med(incrZero),
  };
})()`;

/** P6 纯模型创建：20 个 1 MB model（门槛的原意：模型创建成本，不含读盘）。 */
const P6_MODELS = String.raw`
(async () => {
  const { mod } = window.__perf;
  const monaco = await mod("deps/monaco-editor.js", "/monaco-editor/esm/vs/editor/editor.api.js", "/monaco-editor/esm/vs/editor/editor.main.js");
  const oneMB = "x".repeat(1048576);
  const models = [];
  const t0 = performance.now();
  for (let i = 0; i < 20; i++) models.push(monaco.editor.createModel(oneMB, "latex"));
  const totalMs = Math.round((performance.now() - t0) * 100) / 100;
  for (const m of models) m.dispose();
  return { count: 20, sizeMBEach: 1, totalMs, perModelMs: Math.round((totalMs / 20) * 1000) / 1000 };
})()`;

/** P7 关闭全部标签 → 内存回收（多文件长会话的真实风险：关掉标签后 model/缓冲是否释放）。 */
const P7_CLOSE_ALL = String.raw`
(async () => {
  const { mod, memTotal, memStable, mb } = window.__perf;
  const monaco = await mod("deps/monaco-editor.js", "/monaco-editor/esm/vs/editor/editor.api.js", "/monaco-editor/esm/vs/editor/editor.main.js");
  const { useEditorStore } = await mod("/src/stores/editor.ts");
  const es = useEditorStore();
  const modelsBefore = monaco.editor.getModels().length;
  const before = await memStable(300);
  const tabs = es.tabs.length;
  const buffersBefore = es.buffers.size;
  for (const t of [...es.tabs]) {
    try { es.closeTab(t.path); } catch (e) { /* 忽略单个失败 */ }
  }
  await new Promise((r) => setTimeout(r, 250)); // 等 EditorPane 的 tabs watcher flush（dispose model）
  const after = await memStable(600);
  return {
    closedTabs: tabs,
    tabsLeft: es.tabs.length,
    buffersBefore,
    buffersAfter: es.buffers.size,
    modelsBefore,
    modelsAfter: monaco.editor.getModels().length,
    memBeforeMB: mb(before),
    memAfterMB: mb(after),
    memDeltaMB: mb(after - before),
    memTotalMB: mb(memTotal()),
  };
})()`;

// ---------------------------------------------------------------- 门槛

/** 门槛（p1 分析 §7 / roadmap §6.2 的建议值）＋ 归一口径。 */
function judge(r, tier) {
  const activeMB = (r.perKey?.modelBytes ?? 1) / 1048576;
  const bufMB = r.outline?.bufferMB ?? 1;
  const rows = [];
  const push = (label, value, limit, unit, note) => {
    const ok = value <= limit;
    rows.push({ label, value, limit, unit, ok, note });
    return ok;
  };

  if (r.perKey) {
    // 每击键成本**不随规模增长**（Monaco 编辑 O(log n)；我们的处理是 O(1)：getValue 0.01ms + 一次
    // Map.set + 一个 watcher）→ 用**绝对毫秒**判定。按 MB 归一只会在小文件档虚高（实测 141KB 档被
    // 放大 7 倍、把 0.08ms 判成"超 0.5ms/MB"），那是否决了门槛本身而不是产品。
    push("每击键净开销", r.perKey.netMs, 0.5, "ms", `实测 ${r.perKey.netMs}ms @ ${(activeMB * 1024).toFixed(0)}KB（绝对值口径；p1 门槛 0.5ms 的本意是 1MB 文档下的每次击键开销）`);
    push("最长 longtask", r.perKey.longtaskMaxMs, 50, "ms", `全程 ${r.perKey.longtasks.length} 条 >50ms`);
  }
  if (r.folding) {
    // 折叠提供者是全量扫行（latexSuggest.ts 的 getValue().split）→ 随行数增长，按活动文件规模归一
    const foldPerMB = r.folding.medianMs / Math.max(activeMB, 0.05);
    push("折叠重算", Math.round(foldPerMB * 100) / 100, 2, "ms/MB", `实测 ${r.folding.medianMs}ms @ ${(activeMB * 1024).toFixed(0)}KB（按活动文件线性归一；成本随行数增长）`);
  }
  if (r.outline) {
    // 归一基准用**文档总量**而不是被编辑文件的大小：这条路径的成本主要在"后端读盘 + 全量文件指纹
    // 比对 + 结果序列化"，与被编辑文件只有几百 KB 无关（2026-09 实测踩过基准选错导致误判）。
    const perMB = r.outline.incrOneMedianMs / Math.max(bufMB, 0.05);
    push(
      "大纲往返（编辑 1 个文件后）",
      Math.round(perMB * 100) / 100,
      20,
      "ms/MB",
      `实测 ${r.outline.incrOneMedianMs}ms，文档共 ${bufMB}MB（被编辑文件 ${(r.outline.editedFileBytes / 1024).toFixed(0)}KB）；无变化 ${r.outline.incrZeroMedianMs}ms；全量（冷路径）${r.outline.fullMedianMs}ms`
    );
  }
  if (r.models) {
    push("20 个 1MB model 创建", r.models.totalMs, 100, "ms", `每模型 ${r.models.perModelMs}ms`);
  }
  return rows;
}

// ---------------------------------------------------------------- 主流程

const args = parseArgs(process.argv.slice(2));
if (!existsSync(MANIFEST)) {
  console.error(`缺少夹具清单 ${MANIFEST}\n先跑：node scripts/gen-large-project.mjs`);
  process.exit(2);
}
const manifest = JSON.parse(readFileSync(MANIFEST, "utf8"));
const tier = manifest.tiers.find((t) => t.name === args.tier);
if (!tier) {
  console.error(`清单里没有档位 ${args.tier}；可选：${manifest.tiers.map((t) => t.name).join(", ")}`);
  process.exit(2);
}

console.log(`编辑器侧性能口径（roadmap ⑦c）`);
console.log(`档位 ${tier.name}：${tier.chapters} 章 / ${(tier.bytes / 1048576).toFixed(2)} MB / 目录 ${tier.dir}`);
console.log(`门槛来源：p1-large-doc-editor-analysis.md §7（本脚本按规模做线性归一，见各行注）\n`);

const session = await Session.connect(args.host, args.port);
const results = { tier: tier.name, at: new Date().toISOString(), fixture: { chapters: tier.chapters, bytes: tier.bytes } };
try {
  await session.eval(BOOTSTRAP, 20000);
  if (args.evalFile) {
    const out = await session.eval(readFileSync(resolve(REPO, args.evalFile), "utf8"), 120000);
    console.log(JSON.stringify(out, null, 2));
    session.close();
    process.exit(0);
  }
  const tierDir = tier.dir.replace(/\\/g, "/");
  const editTargetAbs = `${tierDir}/${tier.editTarget}`;
  const fill = (probe) =>
    probe
      .replace("__KEYS__", String(args.keys))
      .replaceAll("__EDIT_TARGET__", editTargetAbs)
      .replaceAll("__TIER_DIR__", tierDir)
      .replaceAll("__TIER_TEX_COUNT__", String((tier.chapters ?? 0) + 1));
  const step = async (label, probe, timeout = 180000) => {
    process.stdout.write(`  · ${label} … `);
    const t = Date.now();
    const r = await session.eval(fill(probe), timeout);
    console.log(`完成（${Date.now() - t}ms）`);
    return r;
  };
  console.log("探针：");
  results.switch = await step("切换 / 确认项目", P0_SWITCH, 60000);
  results.env = await step("环境 / 项目规模", P1_ENV, 30000);
  results.openTabs = await step("打开全部 .tex 标签", P2_OPEN_ALL);
  results.perKey = await step(`每击键 A/B（${args.keys} 键 × 5 轮交替，弃首轮）`, P3_PER_KEY);
  results.folding = await step("折叠提供者", P4_FOLDING, 60000);
  results.outline = await step("大纲往返（全量 / 编辑 1 文件 / 无变化）", P5_OUTLINE);
  results.closeAll = await step("关闭全部标签（内存回收）", P7_CLOSE_ALL, 60000);
  results.models = await step("20 个 1MB model 创建", P6_MODELS, 120000);

  const expectedTex = (tier.chapters ?? 0) + 1;
  if (results.env.texFiles < expectedTex) {
    console.log(`\n⚠️ 窗口里的项目只有 ${results.env.texFiles} 个 .tex（预期 ${expectedTex}）——项目没切过来，数字不可比。`);
  }

  const rows = judge(results, tier);
  results.thresholds = rows;

  console.log(`\n结果：`);
  console.log(`| 项 | 实测 | 门槛 | 判定 | 说明 |`);
  console.log(`|---|---|---|---|---|`);
  for (const r of rows) {
    console.log(`| ${r.label} | ${r.value} ${r.unit} | ≤ ${r.limit} ${r.unit} | ${r.ok ? "✅" : "❌"} | ${r.note} |`);
  }
  console.log(`\n明细：`);
  console.log(`  项目：${results.env.texFiles} 个 .tex / 树 ${results.env.treeNodes} 节点 / 根文件 ${results.env.rootFile}`);
  console.log(`  打开标签：${results.openTabs.count} 个，端到端 ${results.openTabs.e2eMs}ms（读盘+store ${results.openTabs.storeMs}ms + model 就绪 ${results.openTabs.modelsReadyMs}ms；中位 ${results.openTabs.medianPerFileMs}ms/文件）`);
  console.log(`    内存：缓冲 ${results.openTabs.bufferMB}MB / model +${results.openTabs.modelsCreated}（${results.openTabs.modelsBefore}→${results.openTabs.modelsAfter}）/ 堆 +${results.openTabs.memDeltaMB}MB（→ ${results.openTabs.memAfterMB}MB，总量 ${results.openTabs.memTotalMB}MB）`);
  console.log(`  每击键：A ${results.perKey.aMs}ms / B ${results.perKey.bMs}ms → 净 ${results.perKey.netMs}ms（各轮 A=[${results.perKey.roundsA}] B=[${results.perKey.roundsB}]）`);
  console.log(`  折叠：${results.folding.medianMs}ms（5 次 [${results.folding.runsMs}]）`);
  console.log(`  关闭全部标签：${results.closeAll.closedTabs} 个 → 剩 ${results.closeAll.tabsLeft} 标签 / ${results.closeAll.buffersAfter} 缓冲；model ${results.closeAll.modelsBefore}→${results.closeAll.modelsAfter}；堆 ${results.closeAll.memDeltaMB}MB（总量 ${results.closeAll.memTotalMB}MB）`);
  console.log(`  大纲：编辑 1 文件 ${results.outline.incrOneMedianMs}ms / 无变化 ${results.outline.incrZeroMedianMs}ms / 全量 ${results.outline.fullMedianMs}ms（${results.outline.openPaths} 个缓冲 / ${results.outline.bufferMB}MB）`);
  console.log(`  模型创建：20×1MB 共 ${results.models.totalMs}ms`);

  if (args.json) {
    writeFileSync(resolve(REPO, args.json), JSON.stringify(results, null, 2), "utf8");
    console.log(`\nJSON: ${resolve(REPO, args.json)}`);
  }
  const failed = rows.filter((r) => !r.ok);
  console.log(`\n${failed.length === 0 ? "✅ 全部门槛通过" : `❌ ${failed.length} 项未过：${failed.map((f) => f.label).join("、")}`}`);
  session.close();
  process.exit(failed.length === 0 ? 0 : 1);
} catch (e) {
  session.close();
  console.error(`\n❌ ${e.message}`);
  process.exit(2);
}
