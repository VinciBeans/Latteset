<!-- 文件树递归项（modules.md §9.4）：展开状态只在组件内（信息局部性）。
     三个来自父级的"一次性请求"原样向下透传，命中本项即生效：
     - `expandPath`：刚在某个目录里建了东西，要把它展开（否则新内容藏在收起的目录里）；
     - `renamingPath`：这一项进入改名态（行内输入框，roadmap ㊽）；
     - `forceExpand`：搜索中（只显示命中项与其祖先，祖先必须全展开，否则看不到结果，roadmap ㊾）。
     不把展开状态提升到父级——递归里每层透传一个值，比维护一棵状态树简单得多。 -->
<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { useEditorStore } from "../stores/editor";
import type { DirEntryInfo } from "../bindings";

interface Node {
  entry: DirEntryInfo;
  children: Node[];
}

const props = defineProps<{
  node: Node;
  expandPath?: string;
  renamingPath?: string;
  forceExpand?: boolean;
}>();
const emit = defineEmits<{
  /** 右键节点（坐标 + 目标节点）：菜单由 FileTree 渲染（它是唯一知道项目根与创建流程的地方）。 */
  (e: "menu", value: { x: number; y: number; path: string; isDir: boolean; name: string }): void;
  /** 垃圾桶按钮（悬停出现）：只上报，确认弹窗与删除动作在 FileTree。 */
  (e: "remove", value: { path: string; isDir: boolean; name: string }): void;
  /** 改名提交（只给新名字，路径由 FileTree 拼）。 */
  (e: "rename", value: { path: string; name: string }): void;
  (e: "renameCancel"): void;
}>();

const editor = useEditorStore();
const expanded = ref(false);

const isDir = computed(() => props.node.entry.is_dir);
const isActive = computed(() => editor.activePath === props.node.entry.path);
const isTex = computed(() => props.node.entry.name.endsWith(".tex"));
const renaming = computed(() => props.renamingPath === props.node.entry.path);
const open = computed(() => expanded.value || !!props.forceExpand);

// 父级的展开请求（见文件头注释）：命中本项即展开；`immediate` 覆盖"挂载时请求已在"
watch(
  () => props.expandPath,
  (p) => {
    if (p && p === props.node.entry.path) expanded.value = true;
  },
  { immediate: true }
);

function click() {
  if (isDir.value) {
    expanded.value = !expanded.value;
  } else if (isTex.value) {
    editor.openFile(props.node.entry.path);
  }
}

function openMenu(e: MouseEvent) {
  emit("menu", {
    x: e.clientX,
    y: e.clientY,
    path: props.node.entry.path,
    isDir: isDir.value,
    name: props.node.entry.name,
  });
}

// ---- 改名（roadmap ㊽）：行内输入框 ----
const renameInput = ref<HTMLInputElement | null>(null);
const draft = ref("");

watch(
  renaming,
  async (on) => {
    if (!on) return;
    draft.value = props.node.entry.name;
    await nextTick();
    renameInput.value?.focus();
    // 只选中**主名**（不含扩展名）：改名的常见意图是改名字，不是改后缀
    const dot = draft.value.lastIndexOf(".");
    renameInput.value?.setSelectionRange(0, dot > 0 ? dot : draft.value.length);
  },
  { immediate: true }
);

function submitRename() {
  const name = draft.value.trim();
  if (!name || name === props.node.entry.name) {
    emit("renameCancel");
    return;
  }
  emit("rename", { path: props.node.entry.path, name });
}
</script>

<template>
  <div class="node">
    <div
      class="row"
      :class="{ active: isActive, dir: isDir, renaming }"
      @click="renaming ? undefined : click()"
      @contextmenu.prevent.stop="renaming ? undefined : openMenu($event)"
    >
      <span class="twist">{{ isDir && !renaming ? (open ? "▾" : "▸") : "" }}</span>
      <span class="icon" :class="{ 'icon-dir': isDir, 'icon-tex': isTex }">{{ isDir ? (open ? "📂" : "📁") : "📄" }}</span>
      <input
        v-if="renaming"
        ref="renameInput"
        v-model="draft"
        class="rename-input"
        spellcheck="false"
        data-testid="rename-input"
        @click.stop
        @keydown.enter.prevent="submitRename"
        @keydown.esc.prevent="emit('renameCancel')"
        @blur="emit('renameCancel')"
      />
      <template v-else>
        <span class="name">{{ node.entry.name }}</span>
        <!-- 悬停才出现的垃圾桶（roadmap ㊼）：平时不占视觉，鼠标到这一行才露出来 -->
        <button
          class="trash"
          :title="`删除${isDir ? '文件夹' : '文件'}：${node.entry.name}`"
          data-testid="tree-trash"
          @click.stop="emit('remove', { path: node.entry.path, isDir, name: node.entry.name })"
        >🗑</button>
      </template>
    </div>
    <div v-if="isDir && open" class="children">
      <FileTreeItem
        v-for="c in node.children"
        :key="c.entry.path"
        :node="c"
        :expand-path="expandPath"
        :renaming-path="renamingPath"
        :force-expand="forceExpand"
        @menu="emit('menu', $event)"
        @remove="emit('remove', $event)"
        @rename="emit('rename', $event)"
        @rename-cancel="emit('renameCancel')"
      />
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
.row.renaming { background: var(--card-2); }
.twist { width: 12px; flex: 0 0 12px; font-size: 9px; color: var(--ink-faint); text-align: center; }
.icon { flex: 0 0 auto; font-size: 12px; }
.icon-tex { color: var(--blueberry); }
.name { overflow: hidden; text-overflow: ellipsis; }
.children { padding-left: 13px; }

/* 垃圾桶：默认透明**但仍占位**（不能 display:none —— 那会让名字的省略号宽度在悬停时跳一下），
   悬停整行才显形；键盘聚焦到它时也可见。 */
.trash {
  margin-left: auto;
  flex: 0 0 auto;
  border: none;
  background: transparent;
  padding: 0 2px;
  font-size: 11px;
  line-height: 1;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.1s;
}
.row:hover .trash, .trash:focus-visible { opacity: 0.7; }
.trash:hover { opacity: 1; }

.rename-input {
  flex: 1 1 auto;
  min-width: 0;
  font: inherit;
  font-size: 12.5px;
  padding: 1px 4px;
  border: 1px solid var(--blueberry);
  border-radius: 3px;
  background: var(--surface, #fff);
  color: var(--ink);
}
</style>
