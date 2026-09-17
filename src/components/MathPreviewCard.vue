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
  // 片段卡按"够看清公式"定宽（不跟随主预览的缩放）
  const scale = Math.min(2.5, Math.max(1, 320 / Math.max(base.width, 1)));
  const viewport = page.getViewport({ scale });
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(viewport.width * dpr);
  canvas.height = Math.floor(viewport.height * dpr);
  canvas.style.width = `${viewport.width}px`;
  canvas.style.height = `${viewport.height}px`;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    renderError.value = "拿不到 2D 上下文";
    return;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, viewport.width, viewport.height);
  renderTask = page.render({ canvas, canvasContext: ctx, viewport, transform: dpr === 1 ? undefined : [dpr, 0, 0, dpr, 0, 0] } as never);
  try {
    await renderTask.promise;
  } catch (e) {
    renderError.value = `预览渲染失败：${String(e)}`;
  }
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
