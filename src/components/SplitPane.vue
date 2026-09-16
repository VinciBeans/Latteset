<!-- 自研 splitter（modules.md Q6）：水平/垂直拖拽分隔。信息局部性：拖拽状态在组件内。 -->
<script setup lang="ts">
import { computed, ref } from "vue";

const props = defineProps<{
  direction?: "horizontal" | "vertical";
  /** 主面板初始占比（0-1）。 */
  initial?: number;
}>();

const ratio = ref(props.initial ?? 0.25);
const primaryFlex = computed(() => `${ratio.value * 100}%`);
const dragging = ref(false);

function onPointerDown(e: PointerEvent) {
  dragging.value = true;
  (e.target as HTMLElement).setPointerCapture(e.pointerId);
}

function onPointerMove(e: PointerEvent) {
  if (!dragging.value) return;
  const host = (e.currentTarget as HTMLElement).parentElement!;
  const rect = host.getBoundingClientRect();
  if (props.direction === "vertical") {
    ratio.value = Math.min(0.8, Math.max(0.1, (e.clientX - rect.left) / rect.width));
  } else {
    ratio.value = Math.min(0.8, Math.max(0.1, (e.clientY - rect.top) / rect.height));
  }
}

function onPointerUp() {
  dragging.value = false;
}
</script>

<template>
  <div
    class="split-pane"
    :class="[direction ?? 'horizontal', { dragging }]"
  >
    <div class="pane primary"><slot name="primary" /></div>
    <div
      class="divider"
      :class="direction ?? 'horizontal'"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
    >
      <span class="grip" />
    </div>
    <div class="pane secondary"><slot name="secondary" /></div>
  </div>
</template>

<style scoped>
.split-pane { display: flex; width: 100%; height: 100%; overflow: hidden; }
/* direction 指分隔条朝向：vertical=左右排布（竖向分隔条），horizontal=上下排布（横向分隔条） */
.split-pane.vertical { flex-direction: row; }
.split-pane.horizontal { flex-direction: column; }
.divider {
  position: relative;
  flex: 0 0 6px;
  background: var(--line-soft);
  cursor: col-resize;
  transition: background 0.15s;
}
.divider:hover,
.split-pane.dragging .divider { background: var(--blueberry); }
.divider.horizontal { cursor: row-resize; }
.grip {
  position: absolute; top: 50%; left: 50%;
  width: 20px; height: 30px;
  transform: translate(-50%, -50%);
  border-radius: 4px;
  background:
    radial-gradient(circle at center, var(--line-strong) 0 1.5px, transparent 1.5px)
    center / 8px 11px repeat-y;
  opacity: 0.95;
  pointer-events: none;
}
.divider:hover .grip,
.split-pane.dragging .grip { background: radial-gradient(circle at center, var(--on-accent) 0 1.5px, transparent 1.5px) center / 8px 11px repeat-y; }
.divider.horizontal .grip { width: 30px; height: 20px; background: radial-gradient(circle at center, var(--line-strong) 0 1.5px, transparent 1.5px) center / 11px 8px repeat-x; }
.divider.horizontal:hover .grip,
.split-pane.dragging .divider.horizontal .grip { background: radial-gradient(circle at center, var(--on-accent) 0 1.5px, transparent 1.5px) center / 11px 8px repeat-x; }
.pane { min-width: 0; min-height: 0; overflow: hidden; }
.primary { flex: 0 0 v-bind(primaryFlex); }
.secondary { flex: 1 1 auto; }
</style>
