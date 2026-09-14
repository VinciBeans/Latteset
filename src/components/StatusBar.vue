<!-- StatusBar（modules.md §9.4）：编译状态 / 行列号 / 编译模式 / 冲突提示。
     另含「未确定根文件」入口（roadmap P0-②-1）：弹窗被关掉后仍可从这里重新打开选择器。 -->
<script setup lang="ts">
import { computed } from "vue";
import { useCompileStore } from "../stores/compile";
import { useEditorStore } from "../stores/editor";
import { useSettingsStore } from "../stores/settings";
import { useProjectStore } from "../stores/project";
import type { CompilePhase } from "../bindings";

defineProps<{ cursorLine: number; cursorCol: number }>();
defineEmits<{ (e: "pick-root"): void }>();

const compile = useCompileStore();
const editor = useEditorStore();
const settings = useSettingsStore();
const project = useProjectStore();

/** 项目已打开但未确定根文件 → 显示可点击提示（否则不会触发任何编译）。 */
const needsRootFile = computed(() => !!project.project && !project.project.root_file);

const phaseText = computed(() => {
  const map: Record<CompilePhase, string> = {
    queued: "排队中…",
    running: "排版中…",
    success: "就绪",
    failed: "失败",
  };
  return map[compile.phase] ?? "";
});

const kindText = computed(() => {
  switch (compile.kind) {
    case "timeout": return "· 超时";
    case "content_error": return "· 内容错误";
    case "aborted": return "· 已终止";
    default: return "";
  }
});

const isContinuous = computed(() => settings.settings?.compile.mode === "continuous");

/**
 * 当前引擎（D3 裁决，2026-09-14）：**不打开设置面板也要能看见用的哪个引擎**。
 * 同时把编译强度体现在同一个位置——`compile.draft` 为真即本次是 Quick 草稿编译。
 */
const engineText = computed(() => {
  const e = settings.settings?.compile.engine;
  switch (e) {
    case "xelatex": return "XeLaTeX";
    case "lualatex": return "LuaLaTeX";
    case "pdflatex": return "pdfLaTeX";
    case "tectonic": return "Tectonic";
    default: return e ?? "";
  }
});

async function toggleMode() {
  await settings.update({ mode: isContinuous.value ? "on_save" : "continuous" });
}
</script>

<template>
  <div class="status-bar">
    <span class="phase" :class="compile.phase">
      <span class="phase-dot" />
      {{ phaseText }}{{ kindText }}
    </span>
    <!-- D3（2026-09-14）：当前引擎常驻可见——不打开设置面板也知道在用什么引擎 -->
    <span v-if="engineText" class="engine" :title="`当前引擎：${engineText}${compile.draft ? '（本次为 Quick 草稿编译）' : ''}`">
      {{ engineText }}
    </span>
    <!-- 流式进度（阶段 2）：只在"排版中"且已有页输出时显示，避免闪烁 -->
    <span v-if="compile.phase === 'running' && compile.pages > 0" class="pages">
      已排版 {{ compile.pages }} 页
    </span>
    <span
      v-if="compile.draft"
      class="draft"
      title="当前 PDF 由编辑态单趟编译产出：目录/交叉引用页码可能落后一趟，停手后会自动收敛"
    >
      引用待更新
    </span>
    <span
      v-if="needsRootFile"
      class="needs-root"
      title="尚未确定根文件，点击选择要编译的主文件"
      @click="$emit('pick-root')"
    >
      未确定根文件 · 点击选择
    </span>
    <span
      v-for="p in editor.externalConflict"
      :key="p"
      class="conflict"
      title="文件已被外部修改，点击采用磁盘版本"
      @click="editor.acceptExternal(p)"
    >
      外部修改：{{ p.split("/").pop() }}
    </span>
    <!-- 打开文件失败（roadmap ㉓：非 UTF-8 源文件等）——此前只有 console.error，点了没反应 -->
    <span
      v-if="editor.openError"
      class="open-error"
      :title="editor.openError"
      @click="editor.openError = null"
    >
      {{ editor.openError }}
    </span>
    <span class="spacer" />
    <span class="cursor">
      <span class="cursor-file">{{ editor.activeTab?.name ?? "" }}</span>
      <span class="cursor-pos">Ln {{ cursorLine }}, Col {{ cursorCol }}</span>
    </span>
    <button class="mode" :class="{ on: isContinuous }" @click="toggleMode">
      {{ isContinuous ? "连续编译" : "保存触发" }}
    </button>
  </div>
</template>

<style scoped>
.status-bar {
  display: flex; align-items: center; gap: 12px;
  height: 27px; padding: 0 14px;
  background: var(--card);
  border-top: 1.5px solid var(--line);
  color: var(--ink-dim);
  font-size: 11.5px;
  flex: 0 0 auto;
}
.phase {
  display: inline-flex; align-items: center; gap: 7px;
  padding: 0 11px; height: 19px;
  border-radius: 10px;
  background: rgba(93, 95, 239, 0.10);
  color: var(--blueberry);
  font-weight: 600;
}
.phase-dot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; }
.phase.success { background: rgba(47, 191, 143, 0.12); color: #23a377; }
.phase.failed { background: rgba(255, 122, 110, 0.13); color: #e85f52; }
.phase.queued { background: rgba(255, 181, 74, 0.16); color: #e09a2e; }
.phase.running .phase-dot { animation: bounce 0.9s ease-in-out infinite; }
@keyframes bounce { 50% { transform: translateY(-2px); } }
/* 流式进度（阶段 2）：编译中已排版页数（非权威中间态，只在排版中显示） */
.pages {
  background: rgba(93, 95, 239, 0.08);
  color: var(--ink-dim);
  padding: 2px 9px;
  border-radius: 5px;
  font-variant-numeric: tabular-nums;
}
.conflict {
  cursor: pointer;
  background: rgba(255, 122, 110, 0.14);
  color: #e85f52;
  padding: 2px 9px;
  border-radius: 5px;
}
/* 草稿编译提示（roadmap ㉘）：编辑期单趟出图，目录/引用页码可能落后一趟 */
.engine {
  padding: 1px 7px;
  border-radius: 999px;
  background: rgba(93, 95, 239, 0.10);
  color: var(--blueberry);
  font-size: 11.5px;
  font-weight: 600;
}
.draft {  background: rgba(255, 181, 74, 0.18);
  color: #b8791a;
  padding: 2px 9px;
  border-radius: 5px;
  font-weight: 600;
}
/* 未确定根文件：可点击入口（点击打开根文件选择器） */
.needs-root {
  cursor: pointer;
  background: rgba(255, 181, 74, 0.18);
  color: #b8791a;
  padding: 2px 9px;
  border-radius: 5px;
  font-weight: 600;
}
.needs-root:hover { background: rgba(255, 181, 74, 0.3); }
/* 打开文件失败：一条可读的错误（点一下关掉） */
.open-error {
  cursor: pointer;
  max-width: 60%;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  background: rgba(255, 122, 110, 0.14);
  color: #e85f52;
  padding: 2px 9px;
  border-radius: 5px;
  font-weight: 600;
}
.spacer { flex: 1; }
.cursor { display: inline-flex; align-items: center; gap: 9px; }
.cursor-file { color: var(--ink); font-family: var(--mono); max-width: 320px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.cursor-pos { font-family: var(--mono); color: var(--ink-faint); }
.mode {
  display: inline-flex; align-items: center;
  padding: 2px 11px;
  background: transparent;
  border: 1.5px solid var(--line);
  border-radius: 6px;
  color: var(--ink-dim);
  font-size: 11px; font-weight: 550;
  cursor: pointer;
  transition: all 0.15s;
}
.mode:hover { border-color: var(--blueberry); color: var(--ink); }
.mode.on { border-color: var(--blueberry); color: var(--blueberry); background: rgba(93, 95, 239, 0.08); }

@media (prefers-reduced-motion: reduce) {
  .phase.running .phase-dot { animation: none; }
}
</style>
