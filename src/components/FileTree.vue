<!-- 文件树（modules.md §9.4）：从扁平 list_dir 结果构建嵌套。展开状态只在组件内。 -->
<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { useProjectStore } from "../stores/project";
import { useEditorStore } from "../stores/editor";
import { useSettingsStore } from "../stores/settings";
import { ipc } from "../services/ipc";
import type { DirEntryInfo, DocKind, DocLanguage } from "../bindings";
import FileTreeItem from "./FileTreeItem.vue";
import NewFileWizard from "./NewFileWizard.vue";

const project = useProjectStore();
const editor = useEditorStore();
const settings = useSettingsStore();

// ---- ㊺ 新建文件：软件内建文件，避免"打开空目录必须跳出去建" ----
// 口径：在**项目根**建，名称里带 `/` 时可落到**已存在**的子目录（不代建目录 —— 与 write 契约一致）；
// 落盘走 `saveAll`（允许目标不存在，D8 校验在项目内）；成功后刷新树 + 在编辑器打开；
// **空目录特例**：建完第一个 `.tex` 后若项目还没有根文件，重跑一次 `openProject`（根探测），
// 否则会出现"建了文件却编不了"（候选 0 个）。
const creating = ref(false);
const newName = ref("");
const createError = ref("");
const createInput = ref<HTMLInputElement | null>(null);

/**
 * 待创建的目标；非空 = **新建向导**开着（roadmap ㊺ §6.13.1-A）。
 * 向导只负责问（语言/类型/标题），落盘仍走下面那条共用的路。
 */
const wizard = ref<{ target: string; name: string; title: string } | null>(null);
const wizardBusy = ref(false);
const wizardError = ref("");

async function startCreate() {
  if (!project.project) return;
  creating.value = true;
  newName.value = "";
  createError.value = "";
  await nextTick();
  createInput.value?.focus();
}

function cancelCreate() {
  // 向导开着时不要收掉这一行：焦点被向导抢走会让输入框 blur，而此刻用户还没做完决定
  if (wizard.value) return;
  creating.value = false;
  newName.value = "";
  createError.value = "";
}

/**
 * 要不要弹向导：**空项目里的第一个 `.tex`** 才弹（roadmap ㊺ §6.13.1-A）。
 *
 * - 设置里关掉开关 ⇒ 不弹（回到"建空文件"，判据 2）；
 * - 已有根文档 ⇒ 不弹（判据 3，日常使用不该被打断）；
 * - **已有候选**（项目里本来就有 `\documentclass`，只是没定下来是哪一个）⇒ 也不弹：
 *   那不是"新手的空项目"，给模板用户弹"文档语言/类型"只会挡路（他们会走根文件选择器那条路）。
 */
function needsWizard(rel: string): boolean {
  if (!settings.settings?.compile.new_file_wizard) return false;
  if (!/\.tex$/i.test(rel)) return false;
  const p = project.project;
  return !!p && !p.root_file && p.root_candidates.length === 0;
}

/** 文件名（去目录、去 `.tex`）—— 预填成标题。 */
function stemOf(rel: string): string {
  const base = rel.slice(rel.lastIndexOf("/") + 1);
  return base.replace(/\.tex$/i, "");
}

async function submitCreate() {
  if (wizard.value || wizardBusy.value) return;
  const name = newName.value.trim();
  if (!name) {
    cancelCreate();
    return;
  }
  const root = project.project?.root;
  if (!root) return;
  const rel = name.replace(/\\/g, "/");
  if (rel.startsWith("/") || rel.includes("..")) {
    createError.value = "只能建在项目内（不要以 / 开头、不要出现 ..）";
    return;
  }
  const target = `${root.replace(/[\\/]+$/, "")}/${rel}`;
  if (needsWizard(rel)) {
    createError.value = "";
    wizardError.value = "";
    wizard.value = { target, name: rel, title: stemOf(rel) };
    return;
  }
  try {
    await createFile(target, "");
  } catch (e) {
    // 不静默失败：把后端那句人话留在输入框位置上
    createError.value = errorText(e);
  }
}

/** 向导确认：内容由后端现算（core 那张表的唯一副本），再走同一条落盘路径。 */
async function onWizardConfirm(payload: { lang: DocLanguage; kind: DocKind; title: string }) {
  const w = wizard.value;
  if (!w) return;
  wizardBusy.value = true;
  wizardError.value = "";
  try {
    const content = await ipc.newFileSkeleton(payload.lang, payload.kind, payload.title.trim() || null);
    await createFile(w.target, content);
  } catch (e) {
    // 留着向导与用户的选择，只把失败原因摆进面板（重试不用重新选一遍）
    wizardError.value = errorText(e);
  } finally {
    wizardBusy.value = false;
  }
}

/** 落盘 + 刷新树 + 打开 + 空项目重探测（向导与"建空文件"两条路共用）。 */
async function createFile(target: string, content: string) {
  if (!project.project) return;
  await ipc.saveAll([{ path: target, content }]);
  creating.value = false;
  newName.value = "";
  createError.value = "";
  wizard.value = null;
  await project.refreshTree();
  await editor.openFile(target);
  // 空目录：还没有根文件 ⇒ 重跑根探测（探到即补一次首编），让"建完就能编"
  if (project.project && !project.project.root_file) {
    await project.rescanRoot();
  }
}

function errorText(e: unknown): string {
  return typeof e === "string" ? e : ((e as { message?: string })?.message ?? String(e));
}

interface Node {
  entry: DirEntryInfo;
  children: Node[];
}

function buildTree(): Node[] {
  const norm = (p: string) => p.replace(/\\/g, "/");
  const map = new Map<string, Node>();
  const roots: Node[] = [];
  for (const e of project.tree) {
    const path = norm(e.path);
    map.set(path, { entry: { ...e, path }, children: [] });
  }
  for (const e of project.tree) {
    const path = norm(e.path);
    const node = map.get(path)!;
    // 分隔符无关：Windows 反斜杠 / WSL 正斜杠都能切
    const pos = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    const parentPath = pos < 0 ? "" : path.slice(0, pos);
    const parent = map.get(parentPath);
    if (parent && parent.entry.is_dir) parent.children.push(node);
    else roots.push(node);
  }
  const sort = (ns: Node[]) => {
    ns.sort((a, b) =>
      a.entry.is_dir === b.entry.is_dir
        ? a.entry.name.localeCompare(b.entry.name)
        : a.entry.is_dir
          ? -1
          : 1
    );
    for (const n of ns) sort(n.children);
  };
  sort(roots);
  return roots;
}

const tree = computed(buildTree);
</script>

<template>
  <div class="file-tree">
    <div class="tree-panel">
      <div class="panel-title">
        <span class="panel-icon">🗂</span>
        <span>资源管理器</span>
        <button class="new-file" title="新建文件（项目根；可写 chapters/ch1.tex 落到已有子目录）" data-testid="new-file-button" @click="startCreate">＋</button>
      </div>
      <div v-if="creating" class="new-row">
        <input
          ref="createInput"
          v-model="newName"
          class="new-input"
          placeholder="文件名，如 main.tex"
          data-testid="new-file-input"
          spellcheck="false"
          @keydown.enter.prevent="submitCreate"
          @keydown.esc.prevent="cancelCreate"
          @blur="cancelCreate"
        />
      </div>
      <div v-if="createError" class="new-error" data-testid="new-file-error">{{ createError }}</div>
      <div class="tree-scroll">
        <div v-if="!project.project" class="empty">
          <span class="empty-card">
            <span class="empty-icon">📂</span>
            <span class="empty-title">还没打开项目</span>
            <span class="empty-hint">点左上角「打开项目」开始</span>
          </span>
        </div>
        <FileTreeItem v-for="n in tree" :key="n.entry.path" :node="n" />
      </div>
    </div>
    <!-- 新建向导（roadmap ㊺ §6.13.1-A）：只在"空项目里的第一个 .tex + 开关为开"时出现 -->
    <NewFileWizard
      v-if="wizard"
      :file-name="wizard.name"
      :initial-title="wizard.title"
      :busy="wizardBusy"
      :error="wizardError"
      @confirm="onWizardConfirm"
      @cancel="wizard = null"
    />
  </div>
</template>

<style scoped>
.file-tree {
  height: 100%;
  background: var(--card);
  user-select: none;
}
.tree-panel { display: flex; flex-direction: column; height: 100%; }
.panel-title {
  display: flex; align-items: center; gap: 7px;
  flex: 0 0 auto;
  height: 32px; padding: 0 14px;
  font-size: 11px; font-weight: 700;
  letter-spacing: 1px;
  color: var(--ink-dim);
  border-bottom: 1.5px solid var(--line-soft);
}
.panel-icon { font-size: 12px; }
.tree-scroll { flex: 1 1 auto; overflow: auto; padding: 6px 0 14px; font-size: 13px; }
.empty { display: flex; justify-content: center; padding: 36px 10px 0; }
.empty-card {
  display: flex; flex-direction: column; align-items: center; gap: 6px;
  max-width: 100%;
  padding: 20px 12px;
  border: 1.5px dashed var(--line);
  border-radius: var(--radius);
  color: var(--ink-faint);
  white-space: nowrap;
}
.empty-icon { font-size: 26px; }
.empty-title { font-size: 12.5px; font-weight: 600; color: var(--ink-dim); }
.empty-hint { font-size: 11.5px; }

/* 新建入口（㊺）：**贴资源管理器这一列的右缘**，不跟着标题文字走
   —— `margin-left:auto` 把它推到面板右侧（`.panel-title` 是 flex 行）。 */
.new-file {
  margin-left: auto;
  border: none;
  background: transparent;
  color: inherit;
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
  padding: 0 4px;
  opacity: 0.75;
}
.new-file:hover { opacity: 1; }
.new-row { padding: 4px 8px; }
.new-input {
  width: 100%;
  box-sizing: border-box;
  font: inherit;
  font-size: 12px;
  padding: 3px 6px;
  border: 1px solid var(--border, #d8d8e0);
  border-radius: 4px;
  background: var(--surface, #fff);
  color: inherit;
}
.new-error {
  padding: 2px 8px 6px;
  font-size: 11px;
  color: var(--rose, #d1547e);
  white-space: pre-wrap;
}
</style>