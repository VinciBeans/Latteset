<!-- 根文件选择器（roadmap P0-②-1）：
     打开项目后若未探测到唯一根文件，用本弹窗让用户在界面上选，取代此前的 console.warn
     （症状是「打开项目没反应」）。候选来自后端探测（含 \documentclass 且未被引用）；
     零候选时退回列出项目内全部 .tex，避免用户无处下手。 -->
<script setup lang="ts">
import { computed } from "vue";
import { relativizePath } from "../stores/project";

const props = defineProps<{
  /** 项目根（绝对路径，用于把候选转成相对路径展示与提交）。 */
  root: string;
  /** 探测到的候选（项目内绝对路径）；非空 = 多候选。 */
  candidates: string[];
  /** 零候选时的兜底列表：项目内全部 .tex（项目内绝对路径）。 */
  fallbackFiles: string[];
  /** 提交中：禁用交互，防重复提交。 */
  busy?: boolean;
  /** 提交失败信息（如路径校验不通过）。 */
  error?: string;
}>();

const emit = defineEmits<{
  (e: "select", relPath: string): void;
  (e: "close"): void;
}>();

/** 是否为「多候选」场景（决定文案：让用户挑 vs 让用户兜底指定）。 */
const isMulti = computed(() => props.candidates.length > 0);

/** 列表项：绝对路径 + 相对路径（相对路径用于展示与提交）。 */
const items = computed(() => {
  const src = isMulti.value ? props.candidates : props.fallbackFiles;
  return src
    .map((abs) => ({ abs, rel: relativizePath(abs, props.root) }))
    .filter((it) => it.rel !== "")
    .sort((a, b) => a.rel.localeCompare(b.rel));
});

/** 拆成「目录 / 文件名」两段展示（文件名加粗，便于扫读）。 */
function split(rel: string): { dir: string; name: string } {
  const i = rel.lastIndexOf("/");
  return i < 0 ? { dir: "", name: rel } : { dir: rel.slice(0, i + 1), name: rel.slice(i + 1) };
}
</script>

<template>
  <div class="picker-backdrop" @click.self="emit('close')">
    <div class="picker-panel" role="dialog" aria-label="选择根文件">
      <header class="panel-head">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
          <path d="M6 2h7l5 5v13a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2zm7 1.5V8h4.5L13 3.5z"/>
        </svg>
        <span class="head-title">选择根文件</span>
        <button class="head-close" title="关闭" @click="emit('close')">×</button>
      </header>

      <div class="panel-body">
        <p class="lead" v-if="isMulti">
          检测到 <b>{{ items.length }}</b> 个可能的根文件（都含 <code>\documentclass</code>）。
          请选择要编译的主文件——选定后写入项目设置 <code>.latteset/settings.json</code>。
        </p>
        <p class="lead" v-else>
          没有找到含 <code>\documentclass</code> 的文件。请从项目内的 <code>.tex</code>
          中选择要编译的主文件（例如使用了自定义文档类的模板）。
        </p>

        <ul class="file-list" v-if="items.length">
          <li v-for="it in items" :key="it.abs">
            <button class="file-row" :disabled="busy" :title="it.abs" @click="emit('select', it.rel)">
              <span class="file-dir">{{ split(it.rel).dir }}</span>
              <span class="file-name">{{ split(it.rel).name }}</span>
            </button>
          </li>
        </ul>
        <p class="empty" v-else>
          项目内没有 <code>.tex</code> 文件。请确认打开的是模板所在目录，或在设置里手动指定根文件。
        </p>

        <p class="err" v-if="error">{{ error }}</p>
      </div>

      <footer class="panel-foot">
        <span class="foot-hint">也可以在「设置 → 项目 → 根文件」手动填写相对路径</span>
        <button class="btn" @click="emit('close')">取消</button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.picker-backdrop {
  position: fixed;
  inset: 0;
  z-index: 110;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--scrim);
  backdrop-filter: blur(2px);
}
.picker-panel {
  width: 520px;
  max-width: calc(100vw - 40px);
  max-height: calc(100vh - 120px);
  display: flex;
  flex-direction: column;
  background: var(--card);
  border: 1.5px solid var(--line);
  border-radius: 14px;
  box-shadow: 0 24px 64px var(--scrim-weak), 4px 4px 0 var(--tint-ink-weak);
  overflow: hidden;
}
.panel-head {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 44px;
  padding: 0 14px 0 16px;
  border-bottom: 1.5px solid var(--line-soft);
  flex: 0 0 auto;
}
.panel-head svg { color: var(--blueberry); flex: 0 0 auto; }
.head-title { font-weight: 700; font-size: 13.5px; letter-spacing: 0.5px; }
.head-close {
  margin-left: auto;
  width: 24px; height: 24px;
  border: none; background: transparent;
  color: var(--ink-faint); font-size: 17px;
  border-radius: 6px; cursor: pointer;
}
.head-close:hover { background: var(--card-2); color: var(--ink); }

.panel-body { flex: 1 1 auto; overflow: auto; padding: 12px 16px 4px; }
.lead { margin: 0 0 10px; font-size: 12px; color: var(--ink-dim); line-height: 1.6; }
.lead code {
  font-family: var(--mono);
  background: var(--card-2);
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 11px;
}

.file-list { list-style: none; margin: 0; padding: 0; }
.file-row {
  display: flex;
  align-items: baseline;
  gap: 2px;
  width: 100%;
  padding: 7px 10px;
  margin-bottom: 4px;
  background: var(--card);
  border: 1.5px solid var(--line-soft);
  border-radius: var(--radius-sm);
  font-family: var(--mono);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  transition: border-color 0.12s, background 0.12s, transform 0.08s;
}
.file-row:hover:not(:disabled) {
  border-color: var(--blueberry);
  background: var(--card-2);
  transform: translate(-1px, -1px);
}
.file-row:disabled { opacity: 0.5; cursor: default; }
.file-dir { color: var(--ink-faint); }
.file-name { color: var(--ink); font-weight: 700; }

.empty { margin: 4px 0 12px; font-size: 12px; color: var(--ink-dim); line-height: 1.6; }
.empty code { font-family: var(--mono); }

.err {
  margin: 8px 0 4px;
  padding: 7px 10px;
  font-size: 11.5px;
  color: var(--danger-ink);
  background: rgba(var(--coral-rgb), 0.12);
  border-radius: var(--radius-sm);
}

.panel-foot {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 16px;
  border-top: 1.5px solid var(--line-soft);
  flex: 0 0 auto;
}
.foot-hint { flex: 1 1 auto; font-size: 11px; color: var(--ink-faint); }
.btn {
  height: 30px; padding: 0 13px;
  background: var(--card);
  border: 1.5px solid var(--line);
  border-radius: var(--radius-sm);
  color: var(--ink);
  font-size: 12.5px; font-weight: 550;
  cursor: pointer;
  box-shadow: var(--shadow-hard);
}
.btn:hover { transform: translate(-1px, -1px); box-shadow: var(--shadow-hard-big); border-color: var(--line-strong); }
</style>
