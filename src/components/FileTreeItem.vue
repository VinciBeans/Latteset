<!-- 文件树递归项（modules.md §9.4）：展开状态只在组件内（信息局部性）。 -->
<script setup lang="ts">
import { computed, ref } from "vue";
import { useEditorStore } from "../stores/editor";
import type { DirEntryInfo } from "../bindings";

interface Node {
  entry: DirEntryInfo;
  children: Node[];
}

const props = defineProps<{ node: Node }>();

const editor = useEditorStore();
const expanded = ref(false);

const isDir = computed(() => props.node.entry.is_dir);
const isActive = computed(() => editor.activePath === props.node.entry.path);
const isTex = computed(() => props.node.entry.name.endsWith(".tex"));

function click() {
  if (isDir.value) {
    expanded.value = !expanded.value;
  } else if (isTex.value) {
    editor.openFile(props.node.entry.path);
  }
}
</script>

<template>
  <div class="node">
    <div class="row" :class="{ active: isActive, dir: isDir }" @click="click">
      <span class="twist">{{ isDir ? (expanded ? "▾" : "▸") : "" }}</span>
      <span class="icon" :class="{ 'icon-dir': isDir, 'icon-tex': isTex }">{{ isDir ? (expanded ? "📂" : "📁") : "📄" }}</span>
      <span class="name">{{ node.entry.name }}</span>
    </div>
    <div v-if="isDir && expanded" class="children">
      <FileTreeItem v-for="c in node.children" :key="c.entry.path" :node="c" />
    </div>
  </div>
</template>

<script lang="ts">
// script setup 里递归引用自身：通过 defineOptions 命名
export default { name: "FileTreeItem" };
</script>

<style scoped>
.node { position: relative; }
.children { position: relative; }
/* 层级缩进引导线 */
.children::before {
  content: " ";
  position: absolute; left: 12px; top: 0; bottom: 0;
  width: 1.5px;
  background: var(--line-soft);
}
.row {
  position: relative;
  display: flex; align-items: center; gap: 5px;
  height: 25px; margin: 1px 6px;
  padding: 0 8px 0 4px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  color: var(--ink-dim);
  white-space: nowrap;
  transition: background 0.12s, color 0.12s;
}
.row:hover { background: var(--card-2); color: var(--ink); }
.row.active {
  background: linear-gradient(90deg, var(--hover-row) 0%, rgba(var(--blueberry-rgb), 0.07) 100%);
  color: var(--ink);
}
.row.active::before {
  content: " ";
  position: absolute; left: 0; top: 4px; bottom: 4px;
  width: 3px; border-radius: 0 3px 3px 0;
  background: var(--blueberry);
}
.twist { width: 12px; flex: 0 0 12px; font-size: 9px; color: var(--ink-faint); text-align: center; }
.icon { flex: 0 0 auto; font-size: 12px; }
.icon-tex { color: var(--blueberry); }
.name { overflow: hidden; text-overflow: ellipsis; }
.children { padding-left: 13px; }
</style>
