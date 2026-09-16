<!-- 标签页条（modules.md §9.4）：打开文件 + 脏标记 + 关闭。 -->
<script setup lang="ts">
import { useEditorStore } from "../stores/editor";
import { useAutoSave } from "../composables/useAutoSave";

const editor = useEditorStore();
const autoSave = useAutoSave();

/** 关标签：先 flush 未落盘的自动保存（防丢内容），再移除标签。 */
async function closeTab(path: string) {
  await autoSave.flush();
  editor.closeTab(path);
}
</script>

<template>
  <div class="tab-bar">
    <div
      v-for="t in editor.tabs"
      :key="t.path"
      class="tab"
      :class="{ active: editor.activePath === t.path }"
      @click="editor.activePath = t.path"
      @auxclick="closeTab(t.path)"
    >
      <span class="dot" v-if="editor.dirty.has(t.path)">●</span>
      <span class="icon">📄</span>
      <span class="name">{{ t.name }}</span>
      <span class="close" @click.stop="closeTab(t.path)" title="关闭">×</span>
    </div>
    <div v-if="editor.tabs.length === 0" class="tab-bar-empty" />
  </div>
</template>

<style scoped>
.tab-bar {
  display: flex;
  height: 37px;
  background: var(--card);
  border-bottom: 1.5px solid var(--line);
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: none;
}
.tab-bar::-webkit-scrollbar { display: none; }
.tab {
  position: relative;
  display: flex; align-items: center; gap: 7px;
  padding: 0 10px 0 13px;
  min-width: 0;
  font-size: 12.5px;
  color: var(--ink-dim);
  cursor: pointer;
  border-right: 1px solid var(--line-soft);
  transition: background 0.12s, color 0.12s;
}
.tab:hover { background: var(--card-2); color: var(--ink); }
.tab.active {
  background: var(--paper);
  color: var(--ink);
  font-weight: 600;
}
/* 活动标签顶部主色条 */
.tab.active::before {
  content: " ";
  position: absolute; left: 8px; right: 8px; top: 0;
  height: 2.5px;
  border-radius: 0 0 3px 3px;
  background: var(--blueberry);
}
.icon { font-size: 11px; flex: 0 0 auto; }
.dot { color: var(--mango); font-size: 9px; flex: 0 0 auto; }
.name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 220px; }
.close {
  flex: 0 0 auto;
  display: inline-flex; align-items: center; justify-content: center;
  width: 17px; height: 17px;
  border-radius: 5px;
  font-size: 13px; line-height: 1;
  color: var(--ink-faint);
  transition: background 0.12s, color 0.12s;
}
.close:hover { background: var(--danger-tint-soft); color: var(--coral); }
.tab-bar-empty { flex: 1 1 auto; }
</style>
