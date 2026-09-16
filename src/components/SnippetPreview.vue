<!--
  片段预览（草稿层真实排版实验 (A)，docs/research/snippet-preview.md）：

  编辑时把**改动所在的那一段**单独编译成一份小 PDF，用 pdf.js 渲染成一张悬浮卡。
  与现有草稿层（字符近似，30ms，叠在旧画面上）是**两个不同的东西**：

  | | 草稿层（既有） | 片段预览（本组件） |
  |---|---|---|
  | 内容 | 改动那一行，系统字体近似 | 改动那一段，**真实排版**（真字体/真断行） |
  | 位置 | 文档里那一页的原位 | 悬浮卡（**不是**文档里那一页的位置） |
  | 延迟 | 31–34 ms（纯前端） | 去抖 350 ms + 一份小文档编译（实测 ~0.1–0.3 s） |

  两条**必须自己守**的约定（后端注释里也写了）：
  1. **编译中不调**：引擎是**进程内**全局锁，片段编译会把主编译顶住 ⇒ `phase === "running"` 时直接跳过
     （那段窗口由草稿层兜着）；
  2. 结果**只用于看一眼**：不写进当前文档 / 页哈希 / A 闸门 / 权威产物（后端走的是独立项目根
     `<项目>/tmp/snippet/`，产物不会碰到 `<stem>.pdf`）。

  可回滚性：本组件与 `compile_snippet` 命令是**一整块**，删掉组件 + App.vue 里的一行挂载即回退，
  不触碰预览面板的渲染契约。
-->
<script setup lang="ts">
import { onBeforeUnmount, nextTick, ref, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import * as pdfjsLib from "pdfjs-dist";
import { ipc } from "../services/ipc";
import { snippetForChange } from "../services/snippetPatch";
import { useCompileStore } from "../stores/compile";
import { useEditorStore } from "../stores/editor";
import { usePreviewStore } from "../stores/preview";
import type { CmdError } from "../bindings";

pdfjsLib.GlobalWorkerOptions.workerSrc = "/pdfjs/pdf.worker.min.js";

/// 改动停下来多久才去编译（打字期间不该每击键都起一次编译）。
const DEBOUNCE_MS = 350;
/// 卡片自动消失（看过了就收起来，别一直挡着）。
const HIDE_AFTER_MS = 5000;
/// 片段最长行数（防止把整章搬进去编译）。
const MAX_SNIPPET_LINES = 60;
/// 卡片里渲染的目标宽度（CSS 像素）。
const CARD_WIDTH = 336;

const editor = useEditorStore();
const compile = useCompileStore();
const preview = usePreviewStore();

const visible = ref(false);
const busy = ref(false);
const error = ref<string | null>(null);
const elapsedMs = ref(0);
const canvasRef = ref<HTMLCanvasElement | null>(null);

/** 上一份缓冲快照（用于找"首个改动行"）；`null` = 还没建立基线（刚打开文件）。 */
let lastText: string | null = null;
let lastPath: string | null = null;
let timer: ReturnType<typeof setTimeout> | null = null;
let hideTimer: ReturnType<typeof setTimeout> | null = null;
/** 运行序号：晚到的结果不得覆盖新结果（与 PreviewPane 的 loadSeq 同款守卫）。 */
let runSeq = 0;
let loadingTask: pdfjsLib.PDFDocumentLoadingTask | null = null;

/** 首个不同的行号与段落定位在 `services/snippetPatch.ts`（纯函数、有单测）。 */

function dismiss() {
  visible.value = false;
  if (hideTimer) clearTimeout(hideTimer);
}

function restartHideTimer() {
  if (hideTimer) clearTimeout(hideTimer);
  hideTimer = setTimeout(() => (visible.value = false), HIDE_AFTER_MS);
}

/** 把片段 PDF 的第 1 页渲染到卡片里（裁到文字包围盒，不留整页白边）。 */
async function render(path: string) {
  const resp = await fetch(convertFileSrc(path));
  const data = await resp.arrayBuffer();
  loadingTask?.destroy().catch(() => {});
  loadingTask = pdfjsLib.getDocument({
    data,
    // 中文 PDF 的 CMap（与预览面板同款：缺失会报 cMapUrl 错、文字渲染失败）
    cMapUrl: new URL("/pdfjs/cmaps/", document.baseURI).href,
    cMapPacked: true,
  });
  const doc = await loadingTask.promise;
  const page = await doc.getPage(1);
  const viewport = page.getViewport({ scale: 1 });

  // 文字包围盒（viewport 坐标，y 向下）——片段文档带着整页版心，不裁就是一张大白纸
  const tc = await page.getTextContent();
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const raw of tc.items) {
    const it = raw as { str?: string; width?: number; height?: number; transform?: number[] };
    if (!it.str || !it.transform) continue;
    const t = it.transform;
    const x = t[4];
    const y = viewport.height - t[5];
    const h = it.height || Math.abs(t[3]) || 10;
    minX = Math.min(minX, x);
    maxX = Math.max(maxX, x + (it.width ?? 0));
    minY = Math.min(minY, y - h);
    maxY = Math.max(maxY, y + 2);
  }
  if (!Number.isFinite(minX)) {
    minX = 0;
    minY = 0;
    maxX = viewport.width;
    maxY = viewport.height;
  }
  console.log(
    `[snippet] 渲染：items=${tc.items.length} bbox=${Math.round(minX)},${Math.round(minY)} ` +
      `${Math.round(maxX - minX)}x${Math.round(maxY - minY)} page=${Math.round(viewport.width)}x${Math.round(viewport.height)}`
  );
  const pad = 6;
  const bw = Math.max(24, maxX - minX + pad * 2);
  const bh = Math.max(24, maxY - minY + pad * 2);
  // 目标宽度按"内容宽度"取，最多到卡片宽度；太窄的内容按 2 倍放大但不超过 3 倍
  const scale = Math.min(CARD_WIDTH / bw, 3);
  const canvas = canvasRef.value;
  // 缺失就**显式报错**：静默 return 会留下一张空白卡，看不出是没渲染还是排版为空
  if (!canvas) throw new Error("片段卡片的画布尚未挂载");
  canvas.width = Math.ceil(bw * scale);
  canvas.height = Math.ceil(bh * scale);
  await page.render({
    canvas,
    viewport,
    // 裁到包围盒：先按 viewport 画，再整体平移 + 缩放
    transform: [scale, 0, 0, scale, -(minX - pad) * scale, -(minY - pad) * scale],
  }).promise;
}

/** 编译 + 渲染 + 显示一个片段（不带改动判定，便于直接调用与验收）。 */
async function showSnippet(snippet: string) {
  if (!snippet.trim()) return;
  // 编译中不抢引擎（进程内全局锁）——那段窗口由草稿层兜
  if (compile.phase === "running") return;

  const seq = ++runSeq;
  busy.value = true;
  try {
    const r = await ipc.compileSnippet(snippet);
    if (seq !== runSeq) return; // 已被更新的改动取代
    // **先让卡片进 DOM 再渲染**：canvas 在 `v-if` 里，早一步 `canvasRef` 就是 null
    // （实测踩到：静默 return 会留下一张空白卡）。
    visible.value = true;
    await nextTick();
    await render(r.path);
    if (seq !== runSeq) return;
    elapsedMs.value = r.elapsed_ms;
    error.value = null;
    restartHideTimer();
  } catch (e) {
    if (seq !== runSeq) return;
    // 片段单独编译失败是常见情形（宏定义在正文里、`\cite` 没有 aux…）——如实显示，别装作有预览
    const msg = (e as CmdError)?.message ?? String(e);
    error.value = msg;
    elapsedMs.value = 0;
    visible.value = true;
    restartHideTimer();
  } finally {
    if (seq === runSeq) busy.value = false;
  }
}

/** 一次缓冲改动 → 该编译的片段 → 出卡片。 */
async function run(next: string, prev: string) {
  const snippet = snippetForChange(prev, next, MAX_SNIPPET_LINES);
  if (!snippet) return;
  await showSnippet(snippet);
}

// 调试/端到端钩子（与 `window.__previewLastReload` 同款的外部可观测约定）：MCP 的合成按键
// 驱动不了 Monaco（实测 `press`/`type` 都被忽略、也没有 EditContext），真机验收需要一条能直接
// 喂片段的入口。它是**只读式**的：不改缓冲、不写文件，只走"编译片段 → 渲染卡片"这条流水线。
(window as any).__snippetPreview = { showSnippet, run, dismiss };

watch(
  () => editor.buffers.get(editor.activePath ?? ""),
  (next) => {
    const path = editor.activePath ?? null;
    if (next === undefined) return;
    if (path !== lastPath) {
      // 换文件：重建基线，不触发（打开文件的"变化"不是用户改动）
      lastPath = path;
      lastText = next;
      return;
    }
    const prev = lastText;
    lastText = next;
    if (prev === null || prev === next) return;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void run(next, prev), DEBOUNCE_MS);
  }
);

// 权威 PDF 更新（真画面已经就位）⇒ 收起卡片，别和真结果抢注意力
watch(
  () => preview.reloadKey,
  () => dismiss()
);

onBeforeUnmount(() => {
  if (timer) clearTimeout(timer);
  if (hideTimer) clearTimeout(hideTimer);
  loadingTask?.destroy().catch(() => {});
  loadingTask = null;
});
</script>

<template>
  <transition name="snippet-fade">
    <div v-if="visible" class="snippet-card">
      <div class="snippet-head">
        <span class="snippet-title">片段预览</span>
        <span class="snippet-meta">
          <template v-if="error">无法单独编译</template>
          <template v-else-if="busy">编译中…</template>
          <template v-else>{{ elapsedMs }} ms · 真实排版</template>
        </span>
        <button class="snippet-close" title="关闭" @click="dismiss">×</button>
      </div>
      <div class="snippet-body">
        <p v-if="error" class="snippet-error" :title="error">{{ error }}</p>
        <canvas v-show="!error" ref="canvasRef" />
      </div>
    </div>
  </transition>
</template>

<style scoped>
.snippet-card {
  position: fixed;
  right: 18px;
  bottom: 46px;
  z-index: 40;
  max-width: 360px;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  background: var(--card);
  box-shadow: var(--shadow-hard-big);
  overflow: hidden;
}
.snippet-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px 5px 10px;
  border-bottom: 1px solid var(--line-soft);
  background: var(--tint-blueberry-weak);
}
.snippet-title {
  font-size: 11.5px;
  font-weight: 700;
  color: var(--blueberry);
}
.snippet-meta {
  flex: 1 1 auto;
  min-width: 0;
  font-size: 11px;
  color: var(--ink-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.snippet-close {
  border: 0;
  background: transparent;
  color: var(--ink-faint);
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
  padding: 2px 4px;
}
.snippet-close:hover {
  color: var(--coral);
}
.snippet-body {
  padding: 8px 10px 10px;
  max-height: 300px;
  overflow: auto;
}
.snippet-body canvas {
  display: block;
  max-width: 100%;
}
.snippet-error {
  margin: 0;
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--danger-ink);
  max-height: 120px;
  overflow: hidden;
}
.snippet-fade-enter-active,
.snippet-fade-leave-active {
  transition: opacity 0.14s ease;
}
.snippet-fade-enter-from,
.snippet-fade-leave-to {
  opacity: 0;
}
</style>
