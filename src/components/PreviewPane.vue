<!-- PreviewPane（modules.md §9.4）：pdf.js 封装。
   连续分页展示：**分页 DOM 虚拟化**——只挂载视口窗口内的页（前后各 PAGE_WINDOW 页），
   用顶部/底部占位撑住总高度（滚动条稳定），视口感知按需渲染（近处渲染、远处释放）。
   滚动保持：重载前记录滚动，加载后恢复；同文件内容重载保留 pageH1（scale=1 页高）→ 恢复精确。
   canvas 复用：`structuralEpoch` 仅在**缩放/换文档**时 +1 强制重建 canvas DOM（全新 2D context，
   物理排除被取消渲染残留状态）；**同文件内容重载不再重建**，复用 DOM（doRenderPage 每次
   canvas.width= 重置即获全新 context，且 renderPage 串行链已 cancel+await —— 排除黑屏/翻转回归）。
   SyncTeX：高亮 overlay 绘制（跟随对应页）；点击反向定位。
   缩放：工具条 − / % / + / 适应宽度 + Ctrl+滚轮；高亮与反向定位跟随当前缩放。 -->
<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import * as pdfjsLib from "pdfjs-dist";
import { usePreviewStore } from "../stores/preview";
import { useSyncTex } from "../composables/useSyncTex";

const preview = usePreviewStore();
const { inverse } = useSyncTex();

// SyncTeX 提示（roadmap ⑤）自动消失：长时间挂着会变成"常驻噪音"，5s 足够读到
let syncNoteTimer: ReturnType<typeof setTimeout> | undefined;
watch(
  () => preview.syncNote,
  (note) => {
    clearTimeout(syncNoteTimer);
    if (note) syncNoteTimer = setTimeout(() => preview.setSyncNote(null), 5000);
  }
);

pdfjsLib.GlobalWorkerOptions.workerSrc = "/pdfjs/pdf.worker.min.js";

const container = ref<HTMLElement | null>(null);

const scale = ref(1.5);
const SCALE_MIN = 0.5;
const SCALE_MAX = 4;
const RENDER_MARGIN = 800; // 视口外预渲染/释放的边距（px）

/** 分页虚拟化窗口：当前挂载的页码范围（前后各 PAGE_WINDOW 页）。 */
const PAGE_WINDOW = 6;
const PAGE_GAP = 18;   // 页间距（.page-wrap margin-bottom，px）
const PAD_TOP = 18;    // .pages 顶部留白（px，移入顶部占位）
const PAD_BOTTOM = 40; // .pages 底部留白（px，移入底部占位）

let doc: pdfjsLib.PDFDocumentProxy | null = null;
let loadingTask: pdfjsLib.PDFDocumentLoadingTask | null = null;
/** 总页数与当前页（响应式：页码指示器展示用）。 */
const numPages = ref(0);
const currentPageIdx = ref(1);
let scrollTop = 0;
/** 加载序号：并发 pdf-updated 时旧加载被取代（destroy 引发的 aborted 忽略）。 */
let loadSeq = 0;
/** 组件是否已卸载：在途 load/渲染/点击不再写全局或操作 doc。 */
let unmounted = false;
/** 是否已做过首次“适应宽度”：只有首次加载自动 fit，之后保留用户手动缩放。 */
let fittedOnce = false;

// ---------- 分页 DOM 引用（模板 ref 回调，仅挂载窗口内的页有值） ----------
const pageEls: (HTMLElement | null)[] = [];
const pageCanvases: (HTMLCanvasElement | null)[] = [];
const pageHighlights: (HTMLElement | null)[] = [];
/** 已渲染页：页码 → 渲染时的 scale（换缩放后需重绘）。 */
const renderedScale = new Map<number, number>();
/** 在途渲染任务：页码 → RenderTask。pdf.js 不允许同一 canvas 并发渲染——
 *  编译重载/缩放/IO+预渲染双触发时，必须先取消在途任务再启动新的，
 *  否则两个 RenderTask 交错写同一个 2D context 会把页面画黑/花屏。 */
const renderTaskByPage = new Map<number, pdfjsLib.RenderTask>();
/** 每页串行渲染链：同页渲染严格 FIFO，杜绝同 canvas 交叉。 */
const renderChainByPage = new Map<number, Promise<void>>();
/**
 * canvas 代次（结构性重建）：**仅缩放 / 换文档（文件路径变化）时 +1**，强制 v-for 重建
 * canvas DOM（全新 2D context，物理排除被取消渲染残留 transform 状态的“黑底/文字反转”）。
 * 同文件内容重载不 +1 → 复用现有 DOM（doRenderPage 每次 canvas.width= 即重置 context）。
 */
const structuralEpoch = ref(0);
/**
 * 布局版本号：每次 pageH1 变化（setHeight 记录/更新页高）时 +1。pageH1/prefixH1 是普通数组，
 * 并非响应式；所有依赖页高的 computed（上/下占位、.page-wrap 高度）引用本 ref 以获得响应式，
 * 使 warmHeights/逐页渲染填入页高时，布局与滚动条能即时重算（否则缩小后释放页高度流失、
 * scrollHeight 变短、滚动条拖不到底）。
 */
const layoutRev = ref(0);
/** 上次加载的 PDF 路径：判断“同文件内容重载”（复用 DOM/页高） vs “换文档”（重建）。 */
let lastDocPath = "";
/** 本次 reload 实际完成绘制的页数（插桩统计：诊断重载渲染成本）。 */
let pagesRenderedThisLoad = 0;

// ---------- 页高缓存（scale=1）与前缀和：虚拟化占位 + 窗口计算 + 滚动保持 ----------
/** 各页 scale=1 高度（下标 1..N，0 占位）。同文件内容重载保留 → 布局稳定。 */
const pageH1: number[] = [0];
/** 前缀和：prefixH1[i] = 前 i 页（1..i）scale=1 高度和。 */
const prefixH1: number[] = [0];
/** 高度预热任务序号（防重入/过期）。 */
let heightWarmId = 0;
/** 虚拟化窗口：当前需挂载的页码范围。 */
const mountStart = ref(1);
const mountEnd = ref(1);

/** 取前缀和（缺省 0）。 */
function pf(i: number): number {
  return prefixH1[i] || 0;
}
/** 重建前缀和（O(N)，页高变化时调用；N 数百时开销可忽略）。 */
function recomputePrefix() {
  const N = numPages.value;
  if (prefixH1.length < N + 1) prefixH1.length = N + 1;
  prefixH1[0] = 0;
  for (let i = 1; i <= N; i++) prefixH1[i] = prefixH1[i - 1] + (pageH1[i] || 0);
}
/** 记录第 n 页 scale=1 高度并更新前缀和（幂等：同值跳过）。 */
function setHeight(n: number, h1: number) {
  if (pageH1[n] === h1) return;
  pageH1[n] = h1;
  recomputePrefix();
  layoutRev.value++; // 页高变化 → 占位 .page-wrap 高度随响应式重算
}
/** 第 n 页顶部相对内容区顶部（含顶部留白）的偏移（当前 scale）。 */
function pageTop(n: number): number {
  return PAD_TOP + pf(n - 1) * scale.value + (n - 1) * PAGE_GAP;
}
/** 内容坐标 y 所在页：返回上边界 ≤ y 的最大页（闭区间二分，O(log N)）。 */
function pageAtY(y: number): number {
  const N = numPages.value;
  let lo = 1;
  let hi = N;
  let ans = 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (pageTop(mid) <= y) {
      ans = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return ans;
}
/** 顶部占位高度：PAD_TOP + 挂载窗口之前所有页（高度 + 间距）。 */
const topSpacerH = computed(() => {
  void layoutRev.value; // pageH1/prefixH1 变化时重算（布局由页高驱动）
  const last = mountStart.value - 1;
  if (last <= 0) return PAD_TOP;
  return PAD_TOP + pf(last) * scale.value + last * PAGE_GAP;
});
/** 底部占位高度：挂载窗口之后所有页（高度 + 间距）+ PAD_BOTTOM。 */
const bottomSpacerH = computed(() => {
  void layoutRev.value;
  const N = numPages.value;
  const end = mountEnd.value;
  if (end >= N) return PAD_BOTTOM;
  const cnt = N - end;
  return (pf(N) - pf(end)) * scale.value + cnt * PAGE_GAP + PAD_BOTTOM;
});
/** .page-wrap 保留高度：= pageH1[n] × scale。即使 canvas 已释放（releasePage 置 0×0）或
 *  re-mount 后尚未重绘，也保留真实页面高度，避免在窗口内的页高度流失 → scrollHeight 变短、
 *  滚动条拖不到底、renderNearViewport 因 0 高矩形误判 near 而把近页 release（PDF 消失）。 */
function pageWrapHeight(n: number): number {
  void layoutRev.value; // 响应式：页高变化时随之重算
  return Math.round((pageH1[n] || 0) * scale.value);
}
/** 需挂载的页码列表（模板 v-for）。 */
const mountedPages = computed(() => {
  const arr: number[] = [];
  for (let i = mountStart.value; i <= mountEnd.value; i++) arr.push(i);
  return arr;
});

/** 由当前滚动位置计算挂载窗口。`scroll` 可指定（加载恢复时用 keepScroll）。 */
function updateWindow(scroll?: number) {
  const N = numPages.value;
  const c = container.value;
  if (!N || !c) {
    mountStart.value = 1;
    mountEnd.value = N;
    return;
  }
  const st = scroll ?? c.scrollTop;
  const ch = c.clientHeight || 1;
  const first = pageAtY(st);
  const last = Math.min(N, pageAtY(st + ch));
  mountStart.value = Math.max(1, first - PAGE_WINDOW);
  mountEnd.value = Math.min(N, last + PAGE_WINDOW);
}

/** 取消某页在途渲染并等其完全结束（pdf.js 要求：cancel 后必须等 promise settle
 *  才能开始新的 render，否则两个任务仍会交错写同一个 2D context —— 页面画黑）。 */
async function cancelRender(pageNum: number) {
  const t = renderTaskByPage.get(pageNum);
  if (!t) return;
  renderTaskByPage.delete(pageNum);
  try {
    t.cancel();
  } catch {
    /* 已完成/已取消 */
  }
  try {
    await t.promise;
  } catch {
    /* 被取消：正常路径 */
  }
}

/** 取消全部在途渲染并等待结束（文档重载时调用，避免旧 doc 的渲染继续写画布）。 */
async function cancelAllRenders() {
  for (const pageNum of [...renderTaskByPage.keys()]) {
    await cancelRender(pageNum);
  }
}

/** synctex 坐标（顶部起算）→ pdf.js PDF 坐标（底部起算）。 */
async function toPdfPoint(page: pdfjsLib.PDFPageProxy, x: number, yTop: number): Promise<[number, number]> {
  const vp1 = page.getViewport({ scale: 1 });
  return [x, vp1.height - yTop];
}

/** 渲染指定页：入队到该页串行链（严格 FIFO），避免同 canvas 并发渲染。
 *  返回该页渲染链，供 caller await（如 SyncTeX 跳转需在渲染完成、页高落定后再定位）。 */
function renderPage(pageNum: number): Promise<void> | undefined {
  if (!doc || !pageCanvases[pageNum]) return;
  if (renderedScale.get(pageNum) === scale.value) return; // 已是最新
  const prev = renderChainByPage.get(pageNum) ?? Promise.resolve();
  const next = prev
    .then(() => doRenderPage(pageNum))
    .catch(() => {
      /* 上一环失败不影响后续 */
    });
  renderChainByPage.set(pageNum, next);
  return next;
}

/** 实际渲染（串行链内执行）：取消并等待旧任务结束后再画。 */
async function doRenderPage(pageNum: number) {
  if (!doc || !pageCanvases[pageNum]) return;
  if (renderedScale.get(pageNum) === scale.value) return; // 已是最新
  await cancelRender(pageNum);
  const page = await doc.getPage(pageNum);
  const canvas = pageCanvases[pageNum]!;
  const viewport = page.getViewport({ scale: scale.value });
  const dpr = window.devicePixelRatio || 1;
  setHeight(pageNum, viewport.height / scale.value); // 记录 scale=1 页高（布局/占位用）
  canvas.width = Math.floor(viewport.width * dpr);
  canvas.height = Math.floor(viewport.height * dpr);
  canvas.style.width = `${viewport.width}px`;
  canvas.style.height = `${viewport.height}px`;
  // 关键：canvas 物理像素 = viewport × dpr，必须传 dpr 变换，否则内容按 1x 绘制，
  // 只占 canvas 左上角 1/dpr（高 DPI 下内容偏左上、缩水）。
  const transform = dpr !== 1 ? [dpr, 0, 0, dpr, 0, 0] : undefined;
  const task = page.render({ canvas, viewport, transform });
  renderTaskByPage.set(pageNum, task);
  try {
    await task.promise;
    // 完成时若未被更新任务替换，记录已完成
    if (renderTaskByPage.get(pageNum) === task) {
      renderTaskByPage.delete(pageNum);
      renderedScale.set(pageNum, scale.value);
      pagesRenderedThisLoad++; // 插桩
    }
  } catch {
    // 被取消或被取代：不留置 renderedScale（等待下次渲染）
    if (renderTaskByPage.get(pageNum) === task) renderTaskByPage.delete(pageNum);
  }
}

/** 释放远处页面：清空 canvas 尺寸，等待滚回时重绘。 */
function releasePage(pageNum: number) {
  if (!pageCanvases[pageNum]) return;
  void cancelRender(pageNum);
  const canvas = pageCanvases[pageNum]!;
  canvas.width = 0;
  canvas.height = 0;
  renderedScale.delete(pageNum);
}

/** 渲染挂载窗口内视口附近的所有页（含边距），远离视口的释放。 */
function renderNearViewport() {
  if (!doc || !numPages.value || !container.value) return;
  const rect = container.value.getBoundingClientRect();
  for (let n = mountStart.value; n <= mountEnd.value; n++) {
    const el = pageEls[n];
    if (!el) continue;
    const r = el.getBoundingClientRect();
    const near = r.bottom >= rect.top - RENDER_MARGIN && r.top <= rect.bottom + RENDER_MARGIN;
    if (near) {
      void renderPage(n);
    } else {
      releasePage(n); // 含未渲染页（释放为幂等 no-op），确保重载后远处不残留旧内容
    }
  }
}

/** 预热各页 scale=1 高度（后台/分片，不阻塞首屏），使占位 & 窗口 & 滚动恢复尽早精确。 */
async function warmHeights() {
  if (!doc) return;
  const N = numPages.value;
  const myId = ++heightWarmId;
  // 先当前窗口，再全量
  const windowPages: number[] = [];
  for (let n = mountStart.value; n <= mountEnd.value; n++) if (!pageH1[n]) windowPages.push(n);
  for (const n of windowPages) {
    if (myId !== heightWarmId) return;
    await knowHeight(n);
  }
  for (let n = 1; n <= N; n++) {
    if (myId !== heightWarmId) return;
    if (!pageH1[n]) await knowHeight(n);
  }
}
async function knowHeight(n: number) {
  if (!doc || pageH1[n]) return;
  try {
    const page = await doc.getPage(n);
    const vp = page.getViewport({ scale: 1 });
    setHeight(n, vp.height);
  } catch {
    /* 单页获取失败不影响其他 */
  }
}

/** 更新当前页（视口中心所在页），用于 fitWidth/高亮等。 */
function updateCurrentPage() {
  if (!doc || !numPages.value || !container.value) return;
  const rect = container.value.getBoundingClientRect();
  const mid = rect.top + rect.height / 2;
  let best = 1;
  let bestDist = Infinity;
  for (let n = mountStart.value; n <= mountEnd.value; n++) {
    const el = pageEls[n];
    if (!el) continue;
    const r = el.getBoundingClientRect();
    const d = Math.abs((r.top + r.bottom) / 2 - mid);
    if (d < bestDist) {
      bestDist = d;
      best = n;
    }
  }
  currentPageIdx.value = best;
}

/** 跳转到指定页（页码输入 / SyncTeX 正向的公共导航）：展开窗口 + 预热页高 + 渲染 + 居中滚动。
 *  关键：等待目标页（及窗口内）页高落定后再**瞬间定位**，一次到位（避免 smooth 中途布局变化跳不到位）。 */
async function goToPage(n: number) {
  if (!doc || !numPages.value) return;
  n = Math.max(1, Math.min(n, numPages.value));
  // 1. 挂载窗口包含目标页
  if (n < mountStart.value || n > mountEnd.value) {
    mountStart.value = Math.max(1, n - PAGE_WINDOW);
    mountEnd.value = Math.min(numPages.value, n + PAGE_WINDOW);
    await nextTick();
  }
  // 2. 预热窗口内页高（虚拟化下布局/滚动定位准确）
  for (let p = mountStart.value; p <= mountEnd.value; p++) if (!pageH1[p]) await knowHeight(p);
  // 3. 渲染目标页（renderPage 现返回 promise，await 等渲染完成、页高已记录）
  if (renderedScale.get(n) !== scale.value) await renderPage(n);
  await nextTick();
  // 4. 居中滚动（瞬间定位）
  const pageEl = pageEls[n];
  const c = container.value;
  if (pageEl && c) {
    const pr = pageEl.getBoundingClientRect();
    const cr = c.getBoundingClientRect();
    c.scrollTop = c.scrollTop + (pr.top + pr.height / 2 - cr.top) - cr.height / 2;
    c.scrollLeft = c.scrollLeft + (pr.left + pr.width / 2 - cr.left) - cr.width / 2;
  }
  updateCurrentPage();
}

/** 设置缩放：0.5–4，重绘视口页并尽量保持滚动位置。换缩放 → structuralEpoch++ 重建 canvas。 */
async function setScale(next: number) {
  const s = Math.min(SCALE_MAX, Math.max(SCALE_MIN, Math.round(next * 100) / 100));
  if (Math.abs(s - scale.value) < 0.005) return;
  const keep = container.value?.scrollTop ?? 0;
  scale.value = s;
  structuralEpoch.value++; // 缩放改变布局 → 重建 canvas（全新 2D context）
  renderedScale.clear();
  await cancelAllRenders();
  if (doc) {
    updateWindow(keep);
    await nextTick();
    renderNearViewport();
    if (container.value) container.value.scrollTop = keep;
    updateCurrentPage();
  }
}

/** 适应宽度：按容器宽度计算缩放。 */
async function fitWidth() {
  if (!doc || !container.value) return;
  const page = await doc.getPage(currentPageIdx.value);
  const vp1 = page.getViewport({ scale: 1 });
  const avail = Math.max(200, container.value.clientWidth - 40);
  await setScale(avail / vp1.width);
}

/** 页码输入（Enter/change）：跳转到指定 PDF 页。 */
function onPageInput(e: Event) {
  const v = parseInt((e.target as HTMLInputElement).value, 10);
  if (!v || isNaN(v)) return;
  void goToPage(v);
}

/** Ctrl+滚轮缩放（拦截 webview 页面缩放）。 */
function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return;
  e.preventDefault();
  void setScale(scale.value + (e.deltaY < 0 ? 0.25 : -0.25));
}

/** 加载 PDF；带滚动恢复。同文件内容重载复用 DOM/页高，换文档才重建。 */
async function load() {
  const mySeq = ++loadSeq;
  const path = preview.pdfPath;
  if (!path) return;
  const keepScroll = scrollTop;
  pagesRenderedThisLoad = 0;
  try {
    const t0 = performance.now();
    const resp = await fetch(convertFileSrc(path));
    const data = await resp.arrayBuffer();
    const tFetch = performance.now();
    const byteLen = data.byteLength;
    loadingTask?.destroy().catch(() => {});
    // 中文 PDF 的 CMap 字体映射（缺失报 cMapUrl 错误，文字渲染失败）；
    // 必须绝对 URL：worker 内相对路径会基于 worker 脚本 URL（asset 协议）解析导致 404
    loadingTask = pdfjsLib.getDocument({
      data,
      cMapUrl: new URL("/pdfjs/cmaps/", document.baseURI).href,
      cMapPacked: true,
    });
    doc = await loadingTask.promise;
    const tParse = performance.now();
    if (unmounted || mySeq !== loadSeq) return; // 组件已卸载或已被更新的加载取代
    // 切换文档：取消并等待所有在途渲染结束（旧 doc 的 RenderTask 不允许继续写画布）
    await cancelAllRenders();
    const isSameFile = path === lastDocPath;
    if (!isSameFile) {
      // 换文档：重建 canvas DOM（全新 2D context）并清空页高缓存
      structuralEpoch.value++;
      lastDocPath = path;
      pageH1.length = 0;
      pageH1[0] = 0;
      prefixH1.length = 0;
      prefixH1[0] = 0;
    }
    numPages.value = doc.numPages;
    // 页高数组补足到新总页数（同文件重载保留已有页高 → 布局/滚动稳定）
    if (pageH1.length < doc.numPages + 1) pageH1.length = doc.numPages + 1;
    // 内容已变 → 全部标记为需重绘；DOM 复用与否由 structuralEpoch 决定
    renderedScale.clear();
    currentPageIdx.value = Math.min(currentPageIdx.value, numPages.value);
    if (!fittedOnce) {
      fittedOnce = true;
      await fitWidth();
    }
    updateWindow(keepScroll);
    await nextTick(); // 等 v-for 按窗口挂载/卸载
    renderNearViewport();
    // 后台预热高度（不阻塞首屏）
    void warmHeights();
    if (container.value) container.value.scrollTop = keepScroll;
    updateCurrentPage();
    // 恢复后再渲染一次（字体加载可能改变布局）
    await nextTick();
    renderNearViewport();
    // 等本次挂载窗口的渲染链全部落盘（真实 canvas 绘制耗时）。
    // 原实现 render=setup 时间、pagesRendered 恒为 0，无法反映渲染瓶颈（modules.md §12）。
    await Promise.allSettled([...renderChainByPage.values()]);
    if (unmounted) return; // 卸载后不再写全局/标题
    const tDone = performance.now();
    const timing = {
      reload: mySeq,
      file: path.split(/[\\/]/).pop() || path,
      pages: numPages.value,
      bytes: byteLen,
      fetch: Math.round(tFetch - t0),
      parse: Math.round(tParse - tFetch),
      render: Math.round(tDone - tParse),
      total: Math.round(tDone - t0),
      pagesRendered: pagesRenderedThisLoad,
    };
    (window as any).__previewLastReload = timing; // 端到端测试读取
    // 无 Rust 变更的观测通道：把耗时放进窗口标题，便于外部(如 pc-control list_windows)读取
    document.title = `Latteset | reload ${timing.total}ms (fetch ${timing.fetch} parse ${timing.parse} render ${timing.render}) pages ${timing.pages} rendered ${timing.pagesRendered}`;
    console.log(
      `[preview] reload#${mySeq} ${timing.file} pages=${timing.pages} bytes=${timing.bytes} ` +
        `fetch=${timing.fetch}ms parse=${timing.parse}ms render=${timing.render}ms ` +
        `total=${timing.total}ms pagesRendered=${timing.pagesRendered}`
    );
  } catch (e) {
    if (unmounted || mySeq !== loadSeq) return; // 已卸载或被取代的加载忽略
    console.error("PDF 加载失败：", e);
  }
}

// pdf-updated → 重载
watch(
  () => preview.reloadKey,
  () => {
    if (container.value) scrollTop = container.value.scrollTop;
    load();
  }
);

// 挂载窗口变化 → 更新 DOM 后按需渲染
watch(mountedPages, async () => {
  await nextTick();
  renderNearViewport();
});

// 高亮 overlay（SyncTeX 正向）：放到对应页的 wrap 内。若目标页不在挂载窗口则临时扩展窗口。
watch(
  () => preview.highlight,
  async (h) => {
    for (const box of pageHighlights) if (box) box.style.display = "none";
    if (!h || !doc) return;
    if (h.page < 1 || h.page > numPages.value) return;
    if (h.page < mountStart.value || h.page > mountEnd.value) {
      const mid = Math.max(1, Math.min(h.page, numPages.value));
      mountStart.value = Math.max(1, mid - PAGE_WINDOW);
      mountEnd.value = Math.min(numPages.value, mid + PAGE_WINDOW);
      await nextTick();
    }
    const box = pageHighlights[h.page];
    const canvas = pageCanvases[h.page];
    if (!box || !canvas) return;
    // 保证该页已渲染（若远离视口则临时渲染；renderPage 现返回 promise，await 等渲染完成 + 页高落定）
    if (renderedScale.get(h.page) !== scale.value) {
      await renderPage(h.page);
      await nextTick(); // 等布局稳定（canvas 尺寸/页高已记录）
    }
    const page = await doc.getPage(h.page);
    // synctex 的 y 从页面顶部起算，pdf.js 的坐标从底部起算——翻转（2026-08 实测）
    const pdfPt = await toPdfPoint(page, h.x, h.y);
    const viewport = page.getViewport({ scale: scale.value });
    const pt = viewport.convertToViewportPoint(pdfPt[0], pdfPt[1]);
    box.style.display = "block";
    box.style.left = `${pt[0] - 25}px`;
    box.style.top = `${pt[1] - 10}px`;
    box.style.width = "50px";
    box.style.height = "20px";
    // 滚动阅读区，使高亮点居中（一次到位：确保目标页及之前页高已知 + 瞬间定位，
    // 避免 smooth 动画中途布局变化导致跳不到位）
    const pageEl = pageEls[h.page];
    const c = container.value;
    if (pageEl && c) {
      for (let p = mountStart.value; p <= h.page; p++) if (!pageH1[p]) await knowHeight(p);
      const pageRect = pageEl.getBoundingClientRect();
      const cRect = c.getBoundingClientRect();
      const targetY = pageRect.top + pt[1]; // 高亮中心（页面坐标系，viewTop 起算）
      const targetX = pageRect.left + pt[0];
      c.scrollTo({
        top: c.scrollTop + (targetY - cRect.top) - cRect.height / 2,
        left: c.scrollLeft + (targetX - cRect.left) - cRect.width / 2,
        behavior: "auto",
      });
    }
  }
);

// 点击 → SyncTeX 反向（modules.md §5.3）
async function onCanvasClick(pageNum: number, e: MouseEvent) {
  if (!doc) return;
  const canvas = pageCanvases[pageNum];
  if (!canvas) return;
  // 卸载期间点击：getPage/inverse 可能拒绝或操作已销毁的 doc → 吞掉并退出（不产生未处理 rejection）。
  try {
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const page = await doc.getPage(pageNum);
    const viewport = page.getViewport({ scale: scale.value });
    const clickPt = viewport.convertToPdfPoint(x, y);
    // pdf.js 坐标底部起算 → synctex 顶部起算（翻转）
    const vp1 = page.getViewport({ scale: 1 });
    await inverse(pageNum, clickPt[0], vp1.height - clickPt[1]);
  } catch (err) {
    if (!unmounted) console.error("SyncTeX 反向定位失败：", err);
  }
}

function onScroll(e: Event) {
  scrollTop = (e.target as HTMLElement).scrollTop;
  updateWindow();
  renderNearViewport();
  updateCurrentPage();
}

function setPageEl(n: number, el: HTMLElement | null) {
  pageEls[n] = el;
}
function setCanvasEl(n: number, el: HTMLCanvasElement | null) {
  pageCanvases[n] = el;
}
function setHighlightEl(n: number, el: HTMLElement | null) {
  pageHighlights[n] = el;
}

/** 容器尺寸变化（面板/窗口缩放）：重算窗口并按需渲染。 */
let resizeObs: ResizeObserver | null = null;
function setupResizeObserver() {
  resizeObs?.disconnect();
  if (!container.value) return;
  resizeObs = new ResizeObserver(() => {
    updateWindow();
    renderNearViewport();
    updateCurrentPage();
  });
  resizeObs.observe(container.value);
}

onMounted(() => {
  setupResizeObserver();
  if (preview.pdfPath) load();
});

onBeforeUnmount(() => {
  unmounted = true;
  clearTimeout(syncNoteTimer);
  resizeObs?.disconnect();
  resizeObs = null;
  // 取消在途渲染任务（避免卸载后仍向已卸载的 canvas 写像素），并显式销毁加载任务
  cancelAllRenders().catch(() => {});
  loadingTask?.destroy().catch(() => {});
  loadingTask = null;
  doc = null;
});
</script>

<template>
  <div class="preview-root">
    <div class="preview-toolbar">
      <button
        class="zoom-btn"
        title="缩小（Ctrl+滚轮）"
        :disabled="scale <= SCALE_MIN"
        @click="setScale(scale - 0.25)"
      >−</button>
      <span class="zoom-pct" title="当前缩放比">{{ Math.round(scale * 100) }}%</span>
      <button
        class="zoom-btn"
        title="放大（Ctrl+滚轮）"
        :disabled="scale >= SCALE_MAX"
        @click="setScale(scale + 0.25)"
      >+</button>
      <span class="toolbar-sep" />
      <button class="zoom-btn fit" title="适应宽度" @click="fitWidth">⤢ 适应宽度</button>
      <span class="page-indicator" v-if="numPages > 0">
        <input
          class="page-input"
          type="number"
          :min="1"
          :max="numPages"
          :value="currentPageIdx"
          :title="`跳转到页（1-${numPages}），回车或失焦跳转`"
          @keydown.enter="onPageInput($event)"
          @change="onPageInput($event)"
        />
        <span class="page-total">/ {{ numPages }}</span>
      </span>
      <!-- SyncTeX 提示（roadmap ⑤）：落到生成文件/同步不可用时的可见反馈，几秒后自动消失 -->
      <span class="sync-note" v-if="preview.syncNote" :title="preview.syncNote">{{ preview.syncNote }}</span>
    </div>
    <div ref="container" class="preview-pane" @scroll.passive="onScroll" @wheel="onWheel">
      <div v-if="!preview.pdfPath" class="empty">
        <span class="empty-icon">📕</span>
        <span class="empty-title">PDF 在这里等你</span>
        <span class="empty-hint">写好 main.tex，点「编译」就能提前看到成品</span>
      </div>
      <div class="pages" v-if="numPages > 0">
        <!-- 顶部占位：撑住窗口之前页面的高度（滚动条稳定） -->
        <div class="spacer" :style="{ height: topSpacerH + 'px' }" />
        <div
          v-for="n in mountedPages"
          :key="`${n}-${structuralEpoch}`"
          class="page-wrap"
          :data-page="n"
          :style="{ height: pageWrapHeight(n) + 'px' }"
          :ref="(el) => setPageEl(n, el as HTMLElement)"
        >
          <canvas :ref="(el) => setCanvasEl(n, el as HTMLCanvasElement)" @click="onCanvasClick(n, $event)" />
          <div class="highlight" :ref="(el) => setHighlightEl(n, el as HTMLElement)" />
        </div>
        <!-- 底部占位：撑住窗口之后页面的高度 -->
        <div class="spacer" :style="{ height: bottomSpacerH + 'px' }" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.preview-root { display: flex; flex-direction: column; height: 100%; }
.preview-toolbar {
  display: flex; align-items: center; gap: 6px;
  flex: 0 0 auto;
  height: 32px; padding: 0 10px;
  background: var(--card);
  border-bottom: 1.5px solid var(--line);
}
.zoom-btn {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 26px; height: 22px;
  padding: 0 8px;
  background: var(--card);
  border: 1.5px solid var(--line);
  border-radius: 6px;
  color: var(--ink);
  font-size: 13px; font-weight: 600;
  cursor: pointer;
  transition: background 0.12s, border-color 0.12s, color 0.12s;
}
.zoom-btn:hover:not(:disabled) { border-color: var(--blueberry); color: var(--blueberry); background: rgba(93, 95, 239, 0.06); }
.zoom-btn:disabled { opacity: 0.4; cursor: default; }
.zoom-btn.fit { font-size: 11.5px; font-weight: 550; }
.zoom-pct {
  min-width: 44px;
  text-align: center;
  font-family: var(--mono);
  font-size: 11.5px;
  color: var(--ink-dim);
}
.toolbar-sep { width: 1.5px; height: 14px; background: var(--line-soft); margin: 0 2px; }
/* SyncTeX 提示：靠右、单行省略，避免顶掉分页指示器 */
.sync-note {
  margin-left: 10px;
  flex: 1 1 auto;
  min-width: 0;
  padding: 2px 9px;
  border-radius: 5px;
  background: rgba(255, 181, 74, 0.16);
  color: #b8791a;
  font-size: 11.5px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.page-indicator {
  margin-left: auto;
  display: inline-flex; align-items: center; gap: 4px;
  font-family: var(--mono);
  font-size: 11.5px;
  color: var(--ink-faint);
}
.page-input {
  width: 40px; height: 22px;
  padding: 0 4px;
  border: 1.5px solid var(--line);
  border-radius: 6px;
  background: var(--card);
  color: var(--ink);
  font-family: var(--mono);
  font-size: 11.5px;
  text-align: center;
  outline: none;
  transition: border-color 0.12s;
}
.page-input:focus { border-color: var(--blueberry); }
.page-input::-webkit-inner-spin-button,
.page-input::-webkit-outer-spin-button { -webkit-appearance: none; margin: 0; }
.page-total { color: var(--ink-faint); }

/* 点阵网格纸：呼应“写作的方格纸” */
.preview-pane {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  background:
    radial-gradient(rgba(93, 95, 239, 0.10) 1.2px, transparent 1.2px) 0 0 / 20px 20px,
    var(--paper);
}
.empty { padding: 56px 24px; color: var(--ink-faint); text-align: center; }
.empty-icon { font-size: 38px; display: block; }
.empty-title { display: block; margin-top: 12px; font-size: 14px; font-weight: 700; color: var(--ink-dim); }
.empty-hint { display: block; margin-top: 6px; font-size: 12px; }
/* 垂直 padding 移入上/下占位（虚拟化），仅保留水平 padding */
.pages { padding: 0 16px; }
.spacer { width: 100%; }
.page-wrap {
  position: relative;
  margin: 0 auto 18px;
  width: fit-content;
  border-radius: 3px;
  box-shadow: 0 6px 28px rgba(43, 36, 56, 0.18), 0 1px 4px rgba(43, 36, 56, 0.12);
  outline: 1px solid var(--line);
}
/* canvas 块级、充满 .page-wrap 保留高度；避免内联基线缝隙（releasePage 置 0×0 时仅影响宽高，布局仍由 wrap 高度驱动） */
.page-wrap canvas { display: block; }
.highlight {
  display: none;
  position: absolute;
  background: rgba(255, 181, 74, 0.4);
  border: 1.5px solid #e8a72c;
  pointer-events: none;
}
</style>
