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
import { useEditorStore } from "../stores/editor";
import { useSyncTex } from "../composables/useSyncTex";
import {
  draftGeometry,
  findAnchor,
  firstChangedLine,
  latexToDraftText,
  linesOfPage,
  type PdfLine,
} from "../services/draftPatch";

const preview = usePreviewStore();
const editor = useEditorStore();
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
/**
 * 本次 reload **复用旧 canvas、跳过重绘**的页数（插桩）。
 *
 * 依据（docs/research/incremental-edit-x-dvi.md 的功能点 C）：XDV 的页是自包含字节区间，
 * 页哈希相同 ⇔ 该页的排版结果逐字节未变；后端把"变化页集合"随 pdf-updated 一起发来，
 * 未变化的页其 canvas 位图仍然有效 → 不必重画。
 */
let pagesReusedThisLoad = 0;

// ---------- 草案层（v1）：把刚改动的那一行近似画回预览 ----------
//
// 口径（docs/research/realtime-preview-cost.md §6、design.md §延迟预算）：**允许不精确**。
// 用户改动 → 30ms 去抖 → 在"编译成功后建立的 PDF 文本行索引"里找到**改动前那一行**的位置 →
// 在该页 overlay 上白底重绘这一行（带草稿下划线）。下一次 `pdf-updated` 清草案、换真画面。
// **找不到锚点就静默不画**：这条路径绝不阻塞输入、绝不报错、绝不改真实画面。
let compiledText = ""; // 上次成功编译时活动缓冲的快照（逐行 diff 的基线）
const lineIndex: PdfLine[] = []; // 各页文本行索引（PDF 用户空间坐标）
let indexDocPath = ""; // 索引对应的 PDF 路径
let indexBuilding = false;
let draft: { page: number; x: number; y: number; w: number; size: number; text: string } | null = null;
const draftEpoch = ref(0); // 草案变化 → 触发重画（overlay 是命令式 canvas）
const pageDraftCanvases: (HTMLCanvasElement | null)[] = [];
let draftTimer: ReturnType<typeof setTimeout> | undefined;
/** 最近一次"缓冲变更 → 草案可见"的毫秒数（插桩：验收与诊断用）。 */
let lastDraftLatencyMs = 0;

function setDraftCanvasEl(n: number, el: HTMLCanvasElement | null) {
  pageDraftCanvases[n] = el;
}

/** 建立文本行索引（后台做；换文档或内容重载后重建）。失败静默 ⇒ 草案层自动停用。 */
async function buildLineIndex(force = false) {
  if (!doc || indexBuilding) return;
  if (!force && indexDocPath === lastDocPath) return;
  indexBuilding = true;
  const target = lastDocPath;
  const total = numPages.value;
  try {
    lineIndex.length = 0;
    const t0 = performance.now();
    for (let i = 1; i <= total; i++) {
      const page = await doc.getPage(i);
      const tc = await page.getTextContent();
      const items = tc.items as unknown as Parameters<typeof linesOfPage>[1];
      lineIndex.push(...linesOfPage(i, items));
    }
    indexDocPath = target;
    console.debug(
      `[draft] 文本行索引就绪：${lineIndex.length} 行 / ${total} 页，${Math.round(performance.now() - t0)}ms`
    );
  } catch (e) {
    console.debug("[draft] 文本行索引构建失败（草案层停用）：", e);
  } finally {
    indexBuilding = false;
  }
}

/** 清掉草案（不动真实画面）。 */
function clearDraft() {
  draft = null;
  for (const c of pageDraftCanvases) {
    if (!c) continue;
    const ctx = c.getContext("2d");
    if (ctx) ctx.clearRect(0, 0, c.width, c.height);
  }
  draftEpoch.value++;
}

/** 把草案画到对应页的 overlay 上（该页未挂载 ⇒ 什么都不做，等挂载再画）。 */
async function paintDraft() {
  if (!draft || !doc) return;
  const canvas = pageDraftCanvases[draft.page];
  if (!canvas) return;
  const page = await doc.getPage(draft.page);
  const viewport = page.getViewport({ scale: scale.value });
  const dpr = window.devicePixelRatio || 1;
  const pw = Math.floor(viewport.width * dpr);
  const ph = Math.floor(viewport.height * dpr);
  if (canvas.width !== pw || canvas.height !== ph) {
    canvas.width = pw;
    canvas.height = ph;
    canvas.style.width = `${viewport.width}px`;
    canvas.style.height = `${viewport.height}px`;
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, viewport.width, viewport.height);
  const [cx, cy] = viewport.convertToViewportPoint(draft.x, draft.y);
  const s = viewport.scale;
  const sizePx = draft.size * s;
  // 白底盖住原行（纸是白的；上下各留 ~30% 字高）
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(cx - 2, cy - sizePx * 1.05, draft.w * s + 4, sizePx * 1.45);
  // 画新行（草稿允许字形不同：找不到 PDF 内嵌字体就用系统字体栈）
  ctx.fillStyle = "#1c1a24";
  ctx.font = `${sizePx}px "Microsoft YaHei", "Noto Sans CJK SC", "Songti SC", serif`;
  ctx.textBaseline = "alphabetic";
  const maxW = Math.max(viewport.width - cx - 4, 20);
  let text = draft.text.replace(/\s+/g, " ").trim();
  if (ctx.measureText(text).width > maxW) {
    while (text.length > 1 && ctx.measureText(`${text}…`).width > maxW) text = text.slice(0, -1);
    text = `${text}…`;
  }
  ctx.fillText(text, cx, cy);
  // 草稿标记：细下划线（明确"这一行是近似预览"，非最终排版）
  const lineW = Math.min(Math.max(ctx.measureText(text).width, 8), maxW);
  ctx.strokeStyle = "rgba(var(--blueberry-rgb), 0.55)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(cx, cy + sizePx * 0.2);
  ctx.lineTo(cx + lineW, cy + sizePx * 0.2);
  ctx.stroke();
}

/** 缓冲变更 → 计算并绘制草案（去抖 30ms；任何一步失败都静默）。 */
function scheduleDraft() {
  clearTimeout(draftTimer);
  const t0 = performance.now();
  draftTimer = setTimeout(() => {
    const path = editor.activePath;
    // 诊断插桩（静默失败必须可观测；口径见 docs/research/realtime-preview-cost.md）
    const dbg = {
      fired: true,
      path: path ?? null,
      hasDoc: !!doc,
      idx: lineIndex.length,
      compiledLen: compiledText.length,
      curLen: 0,
      changed: null as number | null,
      anchor: null as number | null,
      reason: "" as string,
    };
    (window as unknown as Record<string, unknown>).__lattesetDraftDbg = dbg;
    if (!path || !doc || lineIndex.length === 0) {
      dbg.reason = `bail:path=${!!path},doc=${!!doc},idx=${lineIndex.length}`;
      return;
    }
    const cur = editor.buffers.get(path) ?? "";
    dbg.curLen = cur.length;
    const changed = firstChangedLine(compiledText, cur);
    if (!changed) {
      dbg.reason = "bail:no-diff";
      if (draft) clearDraft();
      return;
    }
    dbg.changed = changed.lineNo;
    const anchor = findAnchor(lineIndex, changed.oldLine);
    if (!anchor) {
      dbg.reason = `bail:no-anchor(oldLine=${changed.oldLine.slice(0, 24)})`;
      if (draft) clearDraft();
      return;
    }
    dbg.anchor = anchor.page;
    // **近似成排版结果的样子**再画：直接画源码会把公式显示成 `$E = mc^2$` 这种代码状态
    // （真机反馈的毛病）。全无可近似内容（纯标记行/纯注释）⇒ 不画，而不是把源码糊上去。
    const draftText = latexToDraftText(changed.newLine);
    if (!draftText) {
      dbg.reason = "bail:no-drawable-text";
      if (draft) clearDraft();
      return;
    }
    const avgCharW = anchor.text.length > 0 ? anchor.w / anchor.text.length : anchor.size * 0.5;
    const g = draftGeometry(anchor, draftText, avgCharW);
    draft = {
      page: anchor.page,
      x: anchor.x,
      y: anchor.y,
      w: g.coverW,
      size: g.size,
      text: draftText,
    };
    draftEpoch.value++;
    void paintDraft();
    lastDraftLatencyMs = performance.now() - t0;
    (window as unknown as Record<string, unknown>).__lattesetDraftMs = Math.round(lastDraftLatencyMs);
    dbg.reason = "ok";
    console.debug(`[draft] 改动→草案可见 ${Math.round(lastDraftLatencyMs)}ms（第 ${anchor.page} 页）`);
  }, 30);
}

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
  pagesReusedThisLoad = 0;
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
    const totalPages = doc.numPages;
    numPages.value = totalPages;
    // 草案层：本次 PDF 就是"当前真值" ⇒ 清草案、把缓冲快照设为 diff 基线、后台重建文本行索引
    clearDraft();
    compiledText = editor.buffers.get(editor.activePath ?? "") ?? "";
    void buildLineIndex(true);
    // 页高数组补足到新总页数（同文件重载保留已有页高 → 布局/滚动稳定）
    if (pageH1.length < totalPages + 1) pageH1.length = totalPages + 1;
    // 哪些页需要重绘？
    //   - 换文档 / 页信息不可判定（pages == 0）/ 页数与上次不一致 → **全部重绘**（现状，保守）；
    //   - 同文件 + 页数一致 + 有变化页集合 → **只让变化页失效**，其余页沿用旧 canvas 位图。
    //     安全依据：页哈希相同 ⇒ 页字节等价 ⇒ 页尺寸与内容都不变（页数也一致时不可能错位）。
    const changed = new Set(preview.changedPages ?? []);
    const canReuse = isSameFile && preview.pagesTotal > 0 && preview.pagesTotal === totalPages;
    if (canReuse) {
      for (const n of changed) renderedScale.delete(n);
      // 插桩：本次能复用的页数 = 原本已渲染、且不在变化集合里的页
      pagesReusedThisLoad = [...renderedScale.keys()].filter((n) => n <= totalPages).length;
    } else {
      renderedScale.clear();
    }
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
      pagesReused: pagesReusedThisLoad,
    };
    (window as any).__previewLastReload = timing; // 端到端测试读取
    // 无 Rust 变更的观测通道：把耗时放进窗口标题，便于外部(如 pc-control list_windows)读取
    document.title = `Latteset | reload ${timing.total}ms (fetch ${timing.fetch} parse ${timing.parse} render ${timing.render}) pages ${timing.pages} rendered ${timing.pagesRendered} reused ${timing.pagesReused}`;
    console.log(
      `[preview] reload#${mySeq} ${timing.file} pages=${timing.pages} bytes=${timing.bytes} ` +
        `fetch=${timing.fetch}ms parse=${timing.parse}ms render=${timing.render}ms ` +
        `total=${timing.total}ms pagesRendered=${timing.pagesRendered} pagesReused=${timing.pagesReused}`
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
// ---------- 草案层接线（放在脚本末尾：依赖 mountedPages / scale 等，避免 TDZ） ----------
/** 缓冲变更（活动文件）→ 计划草案。 */
watch(
  () => editor.buffers.get(editor.activePath ?? ""),
  (v) => {
    if (v !== undefined) scheduleDraft();
  }
);
/** 编译结束（重载或跳过重载）⇒ 草案让位给真画面，并刷新 diff 基线。 */
watch([() => preview.reloadKey, () => preview.skippedReloads], () => {
  clearDraft();
  compiledText = editor.buffers.get(editor.activePath ?? "") ?? "";
});
/** 页挂载窗口 / 缩放变化 ⇒ 草案按新坐标重画（未挂载时 paintDraft 自己跳过）。 */
watch([mountedPages, () => scale.value, draftEpoch], () => {
  if (draft) void paintDraft();
});
onBeforeUnmount(() => clearTimeout(draftTimer));
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
          <!-- 草案层（v1）：改动行的近似预览，压在真画面之上；pointer-events: none，不挡点击定位 -->
          <canvas class="draft-layer" :ref="(el) => setDraftCanvasEl(n, el as HTMLCanvasElement)" />
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
.zoom-btn:hover:not(:disabled) { border-color: var(--blueberry); color: var(--blueberry); background: rgba(var(--blueberry-rgb), 0.06); }
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
  background: var(--tint-mango-weak);
  color: var(--warn-ink);
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
    radial-gradient(var(--tint-blueberry) 1.2px, transparent 1.2px) 0 0 / 20px 20px,
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
  box-shadow: 0 6px 28px rgba(var(--shadow-rgb), 0.18), 0 1px 4px rgba(var(--shadow-rgb), 0.12);
  outline: 1px solid var(--line);
}
/* canvas 块级、充满 .page-wrap 保留高度；避免内联基线缝隙（releasePage 置 0×0 时仅影响宽高，布局仍由 wrap 高度驱动） */
.page-wrap canvas { display: block; }
/* 草案层：改动行的近似预览（白底重绘 + 草稿下划线）。absolute 叠在页 canvas 上；
   pointer-events: none ⇒ 不挡 canvas 的点击反向定位。 */
.draft-layer {
  position: absolute;
  left: 0;
  top: 0;
  pointer-events: none;
}
.highlight {
  display: none;
  position: absolute;
  background: rgba(var(--mango-rgb), 0.4);
  border: 1.5px solid var(--warn-line);
  pointer-events: none;
}
</style>
