<!-- 公式预览浮层（roadmap ㊸ 切片 3）：悬停一个公式时，把**这一个公式**的真实排版结果显示出来。
     与竞品的差别在"真排版"：用的是项目自己的导言区 + Tectonic（ADR-0014），不是 MathJax 近似。
     失败时**不留白**：卡片里给一句话（`message`），这是 A 那一刀踩过的坑（画布没挂载就渲染 → 空白卡）。 -->
<script setup lang="ts">
import { onBeforeUnmount, ref, watch, nextTick } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import * as pdfjsLib from "pdfjs-dist";

pdfjsLib.GlobalWorkerOptions.workerSrc = "/pdfjs/pdf.worker.min.js";

const props = defineProps<{
  x: number;
  y: number;
  pdfPath: string;
  message: string;
  formula: string;
  mode: string;
}>();
const emit = defineEmits<{ (e: "close"): void }>();

const canvasRef = ref<HTMLCanvasElement | null>(null);
/** 渲染失败的原因（与 props.message 分开：一个是"排不出来"，一个是"渲染不出来"）。 */
const renderError = ref("");
let doc: pdfjsLib.PDFDocumentProxy | null = null;
let renderTask: pdfjsLib.RenderTask | null = null;

async function draw() {
  renderError.value = "";
  if (renderTask) {
    renderTask.cancel();
    renderTask = null;
  }
  if (doc) {
    void doc.cleanup();
    doc = null;
  }
  if (!props.pdfPath) return; // 只有 message：卡片当提示用，不画图
  await nextTick();
  const canvas = canvasRef.value;
  // 显式报错而不是静默 return —— A 当年就是因为静默 return 留了一张空白卡
  if (!canvas) {
    renderError.value = "画布未挂载（内部错误）";
    throw new Error(renderError.value);
  }
  let pdf: pdfjsLib.PDFDocumentProxy;
  try {
    const resp = await fetch(convertFileSrc(props.pdfPath));
    const bytes = new Uint8Array(await resp.arrayBuffer());
    pdf = await pdfjsLib.getDocument({
      data: bytes,
      cMapUrl: new URL("/pdfjs/cmaps/", document.baseURI).href,
      cMapPacked: true,
    }).promise;
  } catch (e) {
    renderError.value = `预览渲染失败：${String(e)}`;
    return;
  }
  doc = pdf;
  const page = await pdf.getPage(1);
  const base = page.getViewport({ scale: 1 });
  // 片段文档是**一整页**（`article` 的 A4 版心），公式只占中间一小块 ⇒ 直接画整页会留下大片空白
  // （用户实测反馈）。所以先离屏渲染一页，再按**内容包围盒**裁到公式本身。
  const SCALE = 2; // 离屏渲染倍率：够清晰，又不至于让像素扫描太贵
  const PAD = 10; // 裁剪留白（离屏像素）
  const MAX_CSS_W = 480; // 卡片里公式的最大显示宽度（CSS px；用户反馈原来 340 偏小）
  const MIN_CSS_W = 180; // 极小公式（如 \\$）放大到至少这么宽，否则字太小

  const off = document.createElement("canvas");
  off.width = Math.max(1, Math.floor(base.width * SCALE));
  off.height = Math.max(1, Math.floor(base.height * SCALE));
  const octx = off.getContext("2d", { willReadFrequently: true });
  if (!octx) {
    renderError.value = "拿不到离屏 2D 上下文";
    return;
  }
  octx.fillStyle = "#ffffff";
  octx.fillRect(0, 0, off.width, off.height);
  const offViewport = page.getViewport({ scale: SCALE });
  renderTask = page.render({ canvas: off, canvasContext: octx, viewport: offViewport } as never);
  try {
    await renderTask.promise;
  } catch (e) {
    renderError.value = `预览渲染失败：${String(e)}`;
    return;
  }

  // 找非白像素的包围盒（片段页只有公式，没有页眉页脚 —— 装配时已 `\pagestyle{empty}` + `\nofiles`）
  const pixels = octx.getImageData(0, 0, off.width, off.height).data;
  let minX = off.width;
  let minY = off.height;
  let maxX = -1;
  let maxY = -1;
  for (let y = 0; y < off.height; y++) {
    const row = y * off.width * 4;
    for (let x = 0; x < off.width; x++) {
      const i = row + x * 4;
      if (pixels[i] < 245 || pixels[i + 1] < 245 || pixels[i + 2] < 245) {
        if (x < minX) minX = x;
        if (x > maxX) maxX = x;
        if (y < minY) minY = y;
        if (y > maxY) maxY = y;
      }
    }
  }
  if (maxX < 0 || maxY < 0) {
    renderError.value = "片段页是空白的（没有排出版式）";
    return;
  }
  const cx = Math.max(0, minX - PAD);
  const cy = Math.max(0, minY - PAD);
  const cw = Math.min(off.width - cx, maxX - minX + 1 + PAD * 2);
  const ch = Math.min(off.height - cy, maxY - minY + 1 + PAD * 2);

  // 裁剪后按"最多 MAX_CSS_W"缩小显示（公式很长时也不撑爆卡片）
  const cssW = Math.min(MAX_CSS_W, Math.max(MIN_CSS_W, cw / SCALE));
  const cssH = (ch / cw) * cssW;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(cssW * dpr);
  canvas.height = Math.floor(cssH * dpr);
  canvas.style.width = `${cssW}px`;
  canvas.style.height = `${cssH}px`;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    renderError.value = "拿不到 2D 上下文";
    return;
  }
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(off, cx, cy, cw, ch, 0, 0, canvas.width, canvas.height);
}

watch(() => [props.pdfPath, props.formula], () => void draw(), { immediate: true });
onBeforeUnmount(() => {
  renderTask?.cancel();
  void doc?.cleanup();
});
</script>

<template>
  <!-- Teleport 到 body：浮层不该被编辑器的布局/裁剪影响（免去在 EditorPane 里算坐标） -->
  <Teleport to="body">
    <div
      class="math-preview-card"
      :style="{ left: `${x + 12}px`, top: `${y + 12}px` }"
      data-testid="math-preview-card"
    >
      <div class="head">
        <span class="tag">{{ mode === "button" ? "公式预览（该项目已退让为按钮式）" : "公式预览" }}</span>
        <button class="close" title="关闭" @click="emit('close')">×</button>
      </div>
      <canvas v-if="pdfPath" ref="canvasRef" class="canvas" />
      <div v-if="message" class="note" data-testid="math-preview-message">{{ message }}</div>
      <div v-if="renderError" class="note">{{ renderError }}</div>
      <div class="formula" data-testid="math-preview-formula">{{ formula }}</div>
    </div>
  </Teleport>
</template>

<style scoped>
.math-preview-card {
  position: fixed;
  z-index: 60;
  max-width: 380px;
  background: var(--surface, #fff);
  color: var(--text, #222);
  border: 1px solid var(--border, #d8d8e0);
  border-radius: 8px;
  box-shadow: 0 8px 28px rgb(0 0 0 / 22%);
  padding: 8px 10px 6px;
}
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}
.tag {
  font-size: 11px;
  opacity: 0.72;
}
.close {
  border: none;
  background: transparent;
  color: inherit;
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  opacity: 0.7;
}
.close:hover {
  opacity: 1;
}
.canvas {
  display: block;
  max-width: 100%;
  border-radius: 4px;
}
.note {
  font-size: 12px;
  color: var(--rose, #d1547e);
  max-width: 340px;
  white-space: pre-wrap;
}
.formula {
  margin-top: 6px;
  font-family: ui-monospace, Consolas, monospace;
  font-size: 11px;
  opacity: 0.6;
  max-width: 340px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
