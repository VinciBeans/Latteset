<!-- OutlinePane（modules.md §9.4）：LaTeX 文档结构树（大纲）。点击 → 揭示源码 + SyncTeX 正向跳页。
    嵌套层级由 outlineStore 按结构命令 level 建树；本组件拍平后按 depth 缩进渲染。 -->
<script setup lang="ts">
import { computed } from "vue";
import { useOutlineStore, type OutlineNode } from "../stores/outline";
import { useEditorStore } from "../stores/editor";

const outline = useOutlineStore();
const editor = useEditorStore();

interface Row {
  node: OutlineNode;
  depth: number;
}

const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  const walk = (nodes: OutlineNode[], depth: number) => {
    for (const n of nodes) {
      out.push({ node: n, depth });
      walk(n.children, depth + 1);
    }
  };
  walk(outline.items, 0);
  return out;
});

/** 层级 → 小标签（part 用 °，书类 chapter=章，其余 §/¶ 递进）。 */
const LEVEL_TAG: Record<number, string> = {
  0: "部",
  1: "章",
  2: "§",
  3: "§§",
  4: "§§§",
  5: "¶",
  6: "¶¶",
};

function isActive(node: OutlineNode): boolean {
  return editor.activePath === node.file;
}

function onClick(node: OutlineNode) {
  void outline.goTo(node);
}
</script>

<template>
  <div class="outline-pane">
    <div class="outline-head">
      <span class="head-title">大纲</span>
      <span class="head-count" v-if="rows.length > 0">{{ rows.length }}</span>
    </div>
    <div class="outline-body">
      <div v-if="outline.isEmpty" class="empty">
        <span class="empty-icon">≡</span>
        <span>{{ "编译后自动生成文档结构" }}</span>
      </div>
      <div
        v-for="row in rows"
        :key="`${row.node.file}:${row.node.line}`"
        class="item"
        :class="{ active: isActive(row.node) }"
        :style="{ paddingLeft: 10 + row.depth * 14 + 'px' }"
        :title="`${row.node.file}:${row.node.line}`"
        @click="onClick(row.node)"
      >
        <span class="tag" :class="`lv-${row.node.level}`">{{ LEVEL_TAG[row.node.level] ?? "·" }}</span>
        <span class="title">{{ row.node.title }}</span>
        <span class="loc">{{ row.node.fileBase }}:{{ row.node.line }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.outline-pane {
  height: 100%;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  background: var(--card);
  font-size: 12.5px;
}
.outline-head {
  display: flex; align-items: center; gap: 8px;
  flex: 0 0 auto;
  height: 32px; padding: 0 14px;
  font-size: 11px; font-weight: 700;
  letter-spacing: 1px;
  color: var(--ink-dim);
  border-bottom: 1.5px solid var(--line-soft);
}
.head-count {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 17px; height: 17px; padding: 0 5px;
  border-radius: 9px;
  background: var(--tint-violet);
  color: var(--violet, var(--violet));
  font-size: 10.5px; font-weight: 700;
  letter-spacing: 0;
}
.outline-body { flex: 1 1 auto; overflow: auto; }
.empty {
  display: flex; align-items: center; gap: 9px;
  padding: 16px 18px;
  color: var(--ink-faint);
}
.empty-icon {
  display: inline-flex; align-items: center; justify-content: center;
  width: 17px; height: 17px;
  border-radius: 50%;
  background: var(--tint-violet);
  color: var(--violet, var(--violet));
  font-size: 11px; font-weight: 700;
}
.item {
  display: flex; align-items: center; gap: 7px;
  padding: 4px 10px;
  margin: 1px 6px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  border-left: 2.5px solid transparent;
  transition: background 0.12s;
}
.item:hover { background: var(--card-2); }
.item.active { border-left-color: var(--blueberry); }
.item.active .title { color: var(--blueberry-deep); font-weight: 700; }
.tag {
  flex: 0 0 auto;
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 15px; height: 15px; padding: 0 3px;
  border-radius: 4px;
  font-size: 9.5px; font-weight: 800;
  background: var(--tint-blueberry-strong);
  color: var(--blueberry-deep);
}
.tag.lv-0 { background: rgba(var(--violet-rgb), 0.14); color: var(--violet); }
.tag.lv-1 { background: rgba(var(--blueberry-rgb), 0.16); color: var(--blueberry-deep); }
.tag.lv-2 { background: rgba(var(--sage-rgb), 0.16); color: var(--sage-ink); }
.tag.lv-3 { background: rgba(var(--rose-rgb), 0.14); color: var(--rose-ink); }
.tag.lv-4 { background: rgba(var(--rose-rgb), 0.12); color: var(--rose-ink); }
.tag.lv-5, .tag.lv-6 { background: var(--tint-ink); color: var(--ink-dim); }
.title {
  flex: 1 1 auto; min-width: 0;
  color: var(--ink);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.loc {
  flex: 0 0 auto;
  color: var(--ink-faint);
  font-family: var(--mono);
  font-size: 10.5px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  max-width: 40%;
}
</style>
