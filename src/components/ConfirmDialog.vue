<!-- 二次确认弹窗（roadmap ㊼ 删除用）：把"要删什么、里面有多少东西"讲清楚再动手。
     刻意做成**通用**组件（标题 + 若干行 + 一个危险动作按钮），将来别的破坏性动作可以直接复用；
     但只做这一件事——不做输入框、不做多步向导（那些各有各的组件）。 -->
<script setup lang="ts">
import { onMounted, ref } from "vue";

defineProps<{
  title: string;
  /** 每行一句话（第一行通常是名字，后面是后果说明）。 */
  lines: string[];
  /** 危险动作按钮的文案（如「删除」）。 */
  confirmLabel?: string;
  busy?: boolean;
  error?: string;
}>();

const emit = defineEmits<{ (e: "confirm"): void; (e: "cancel"): void }>();

/** 打开即把焦点收到确认按钮上：弹窗是键盘可达的（Tab 在两键之间、Esc 取消）。 */
const panel = ref<HTMLElement | null>(null);
onMounted(() => panel.value?.focus());
</script>

<template>
  <div class="cf-backdrop" @click.self="emit('cancel')">
    <div
      ref="panel"
      class="cf-panel"
      role="alertdialog"
      aria-modal="true"
      tabindex="-1"
      data-testid="confirm-dialog"
      @keydown.esc.prevent="emit('cancel')"
    >
      <header class="cf-head">
        <span class="cf-icon">⚠</span>
        <span class="cf-title">{{ title }}</span>
      </header>
      <div class="cf-body">
        <p v-for="(l, i) in lines" :key="i" :class="{ lead: i === 0 }">{{ l }}</p>
        <p class="cf-err" v-if="error">{{ error }}</p>
      </div>
      <footer class="cf-foot">
        <button class="btn" :disabled="busy" data-testid="confirm-cancel" @click="emit('cancel')">
          取消
        </button>
        <button class="btn danger" :disabled="busy" data-testid="confirm-ok" @click="emit('confirm')">
          {{ busy ? "处理中…" : (confirmLabel ?? "确认") }}
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.cf-backdrop {
  position: fixed;
  inset: 0;
  z-index: 130;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--scrim);
  backdrop-filter: blur(2px);
}
.cf-panel {
  width: 420px;
  max-width: calc(100vw - 40px);
  background: var(--card);
  border: 1.5px solid var(--line);
  border-radius: 14px;
  box-shadow: 0 24px 64px var(--scrim-weak), 4px 4px 0 var(--tint-ink-weak);
  overflow: hidden;
  outline: none;
}
.cf-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px 8px;
}
.cf-icon { color: var(--rose, #d1547e); font-size: 14px; }
.cf-title { font-size: 13.5px; font-weight: 700; }
.cf-body { padding: 0 16px 6px; }
.cf-body p {
  margin: 0 0 6px;
  font-size: 12px;
  line-height: 1.6;
  color: var(--ink-dim);
  word-break: break-all;
}
.cf-body p.lead { font-size: 12.5px; color: var(--ink); font-weight: 600; }
.cf-err {
  margin-top: 6px !important;
  padding: 7px 10px;
  color: var(--danger-ink);
  background: rgba(var(--coral-rgb), 0.12);
  border-radius: var(--radius-sm);
}
.cf-foot {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 10px 16px 14px;
}
.btn {
  height: 30px;
  padding: 0 14px;
  background: var(--card);
  border: 1.5px solid var(--line);
  border-radius: var(--radius-sm);
  color: var(--ink);
  font-size: 12.5px;
  font-weight: 550;
  cursor: pointer;
  box-shadow: var(--shadow-hard);
}
.btn:hover:not(:disabled) { transform: translate(-1px, -1px); box-shadow: var(--shadow-hard-big); }
.btn:disabled { opacity: 0.5; cursor: default; }
.btn.danger {
  background: var(--rose, #d1547e);
  border-color: var(--rose, #d1547e);
  color: #fff;
}
</style>
