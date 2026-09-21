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

// ---- ㊺ 新建文件 / 新建文件夹：软件内建，避免"打开空目录必须跳出去建" ----
// 两个入口（工具栏与节点右键菜单）共用同一条流程：**就地输入名称** → 落盘 → 刷新树 →
// （文件）在编辑器打开 + 空项目重跑根探测。落点默认项目根，右键菜单可指定落在**某个目录**里。
//
// 名称规则：工具栏那条路允许 `chapters/ch1.tex` 这样的相对路径（落到**已存在**的子目录，
// 不代建目录——与 `write` 契约一致）；右键菜单那条路已经锁定了目标目录，名称里再带 `/` 就报错。
const createKind = ref<"file" | "dir" | null>(null);
/** 目标目录（**项目内绝对路径**，空串 = 项目根）。 */
const createDirPath = ref("");
const newName = ref("");
const createError = ref("");
const createInput = ref<HTMLInputElement | null>(null);
/** 建在某个目录里之后要把它展开（否则新内容藏在收起的目录里，看起来像没建成）。 */
const expandPath = ref("");

/**
 * 待创建的目标；非空 = **新建向导**开着（roadmap ㊺ §6.13.1-A）。
 * 向导只负责问（语言/类型/标题），落盘仍走下面那条共用的路。
 */
const wizard = ref<{ target: string; name: string; title: string } | null>(null);
const wizardBusy = ref(false);
const wizardError = ref("");

/** 右键菜单（位置 + 目标节点）；菜单项由本组件渲染 —— 只有它知道项目根与创建流程。 */
const menu = ref<{ x: number; y: number; dirPath: string; label: string } | null>(null);

/** 右键：目录 ⇒ 建在它里面；文件 ⇒ 建在它所在的目录里。 */
function openMenu(payload: { x: number; y: number; path: string; isDir: boolean; name: string }) {
  if (!project.project) return;
  createError.value = "";
  const dirPath = payload.isDir ? payload.path : payload.path.slice(0, Math.max(0, payload.path.lastIndexOf("/")));
  menu.value = {
    // 贴着视口右下边时往回收一点，菜单不出屏
    x: Math.min(payload.x, window.innerWidth - 190),
    y: Math.min(payload.y, window.innerHeight - 110),
    dirPath,
    label: payload.isDir ? payload.name : `${payload.name} 所在目录`,
  };
}

/** 打开创建行：`kind` 决定建文件还是文件夹，`dirPath` 决定落点（空 = 项目根）。 */
async function startCreate(kind: "file" | "dir", dirPath = "") {
  if (!project.project) return;
  menu.value = null;
  createKind.value = kind;
  createDirPath.value = dirPath;
  newName.value = "";
  createError.value = "";
  await nextTick();
  createInput.value?.focus();
}

function cancelCreate() {
  // 向导开着时不要收掉这一行：焦点被向导抢走会让输入框 blur，而此刻用户还没做完决定
  if (wizard.value) return;
  createKind.value = null;
  newName.value = "";
  createError.value = "";
}

/** 输入框占位文案：一眼看出这一次建的是文件还是文件夹。 */
const createPlaceholder = computed(() =>
  createKind.value === "dir" ? "文件夹名，如 chapters" : "文件名，如 main.tex"
);

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
  const kind = createKind.value;
  if (!kind) return;
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
  // 右键菜单那条路已经锁定了目标目录 ⇒ 名称里再带 `/` 是误操作（工具栏那条路允许，
  // 因为"chapters/ch1.tex 落到已有子目录"是写明了的用法）
  if (createDirPath.value && rel.includes("/")) {
    createError.value = "在某个目录里新建时只需填名称（要建子目录请先建那个目录）";
    return;
  }
  const base = createDirPath.value || root.replace(/[\\/]+$/, "");
  const target = `${base.replace(/[\\/]+$/, "")}/${rel}`;
  try {
    if (kind === "dir") {
      await createDirectory(target);
      return;
    }
    if (needsWizard(rel)) {
      createError.value = "";
      wizardError.value = "";
      wizard.value = { target, name: rel, title: stemOf(rel) };
      return;
    }
    await createFile(target, "");
  } catch (e) {
    // 不静默失败：把后端那句人话留在输入框位置上
    createError.value = errorText(e);
    // 目录没建成 ⇒ 不要假装"建完了还展开了它"（展开请求只跟着成功路径走）
    expandPath.value = "";
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

/** 新建目录（roadmap ㊺ 待办②）：落盘 → 刷新树 → 展开**它的父目录**（否则新目录藏在收起的目录里）。 */
async function createDirectory(target: string) {
  if (!project.project) return;
  const root = project.project.root.replace(/[\\/]+$/, "");
  await ipc.createDir(target);
  resetCreateState();
  await project.refreshTree();
  const parent = target.slice(0, Math.max(0, target.lastIndexOf("/")));
  if (parent && parent !== root) {
    expandPath.value = "";
    await nextTick();
    expandPath.value = parent;
  }
}

/** 落盘 + 刷新树 + 打开 + 空项目重探测（向导与"建空文件"两条路共用）。 */
async function createFile(target: string, content: string) {
  if (!project.project) return;
  await ipc.saveAll([{ path: target, content }]);
  resetCreateState();
  await project.refreshTree();
  await editor.openFile(target);
  // 建在某个目录里 ⇒ 展开那个目录（否则新文件藏在收起的目录里）
  const parent = target.slice(0, Math.max(0, target.lastIndexOf("/")));
  if (parent && parent !== project.project.root.replace(/[\\/]+$/, "")) {
    expandPath.value = "";
    await nextTick();
    expandPath.value = parent;
  }
  // 空目录：还没有根文件 ⇒ 重跑根探测（探到即补一次首编），让"建完就能编"
  if (project.project && !project.project.root_file) {
    await project.rescanRoot();
  }
}

/** 收掉创建行/向导（成功之后；失败时保留输入与提示）。 */
function resetCreateState() {
  createKind.value = null;
  createDirPath.value = "";
  newName.value = "";
  createError.value = "";
  wizard.value = null;
}

/** 向导里的"先建个文件夹"（㊺）：关掉向导、回到树里的文件夹命名流程，落点仍是项目根。 */
async function onWizardFolder() {
  wizard.value = null;
  await startCreate("dir");
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
        <!-- **一个**新建入口贴这一列右缘（roadmap ㊺）：建文件还是建文件夹在下面那行的
             小切换里选（默认文件）—— 两个"＋"并排既占地方又让人先做一次无谓的选择。
             右键菜单那条路仍然直接给两项（菜单里罗列本来就自然，而且能指定落在哪个目录）。 -->
        <button
          class="new-file"
          title="新建（文件 / 文件夹；也可在树节点上右键建到某个目录里）"
          data-testid="new-file-button"
          @click="startCreate('file')"
        >＋</button>
      </div>
      <div v-if="createKind" class="new-row">
        <input
          ref="createInput"
          v-model="newName"
          class="new-input"
          :placeholder="createPlaceholder"
          data-testid="new-file-input"
          spellcheck="false"
          @keydown.enter.prevent="submitCreate"
          @keydown.esc.prevent="cancelCreate"
          @blur="cancelCreate"
        />
        <!-- 类型切换（同一个入口里的"建什么"）：挨着输入框，占位文案跟着变 -->
        <div class="new-kind" role="group" aria-label="要建的类型">
          <button
            class="kind-btn"
            :class="{ on: createKind === 'file' }"
            title="新建文件"
            data-testid="new-kind-file"
            @mousedown.prevent
            @click="createKind = 'file'"
          >📄</button>
          <button
            class="kind-btn"
            :class="{ on: createKind === 'dir' }"
            title="新建文件夹"
            data-testid="new-kind-dir"
            @mousedown.prevent
            @click="createKind = 'dir'"
          >📁</button>
        </div>
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
        <FileTreeItem
          v-for="n in tree"
          :key="n.entry.path"
          :node="n"
          :expand-path="expandPath"
          @menu="openMenu"
        />
      </div>
    </div>

    <!-- 节点右键菜单（roadmap ㊺）：目录 ⇒ 建在它里面；文件 ⇒ 建在它所在的目录里 -->
    <div
      v-if="menu"
      class="ctx-backdrop"
      data-testid="tree-context-menu"
      @click="menu = null"
      @contextmenu.prevent="menu = null"
    >
      <div class="ctx-menu" :style="{ left: `${menu.x}px`, top: `${menu.y}px` }" @click.stop>
        <div class="ctx-title" :title="menu.dirPath">建在：{{ menu.label }}</div>
        <button class="ctx-item" data-testid="ctx-new-file" @click="startCreate('file', menu.dirPath)">
          📄 新建文件
        </button>
        <button class="ctx-item" data-testid="ctx-new-dir" @click="startCreate('dir', menu.dirPath)">
          📁 新建文件夹
        </button>
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
      @folder="onWizardFolder"
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
   —— `margin-left:auto` 把它推到面板右侧（`.panel-title` 是 flex 行）。
   只有**一个**入口：建文件还是建文件夹在下面那行的小切换里选。 */
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
.new-row { display: flex; align-items: center; gap: 6px; padding: 4px 8px; }
.new-input {
  flex: 1 1 auto;
  min-width: 0;
  box-sizing: border-box;
  font: inherit;
  font-size: 12px;
  padding: 3px 6px;
  border: 1px solid var(--border, #d8d8e0);
  border-radius: 4px;
  background: var(--surface, #fff);
  color: inherit;
}
/* 类型切换：两个小图标按钮（`mousedown.prevent` 保住输入框焦点，点它不会当成"失焦取消"） */
.new-kind { display: flex; flex: 0 0 auto; gap: 2px; }
.kind-btn {
  width: 24px;
  height: 23px;
  padding: 0;
  font-size: 11px;
  line-height: 1;
  border: 1px solid var(--border, #d8d8e0);
  border-radius: 4px;
  background: var(--surface, #fff);
  cursor: pointer;
  opacity: 0.55;
}
.kind-btn:hover { opacity: 0.85; }
.kind-btn.on {
  opacity: 1;
  border-color: var(--blueberry);
  background: var(--card-2);
  box-shadow: inset 0 0 0 1px var(--blueberry);
}
.new-error {
  padding: 2px 8px 6px;
  font-size: 11px;
  color: var(--rose, #d1547e);
  white-space: pre-wrap;
}

/* 右键菜单（㊺）：透明蒙层接住"点别处关掉"，菜单本体固定定位在光标处 */
.ctx-backdrop { position: fixed; inset: 0; z-index: 100; }
.ctx-menu {
  position: fixed;
  min-width: 168px;
  padding: 4px;
  background: var(--card);
  border: 1.5px solid var(--line);
  border-radius: var(--radius-sm);
  box-shadow: 0 12px 32px var(--scrim-weak), 3px 3px 0 var(--tint-ink-weak);
}
.ctx-title {
  padding: 5px 8px 6px;
  font-size: 11px;
  color: var(--ink-faint);
  border-bottom: 1px solid var(--line-soft);
  margin-bottom: 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ctx-item {
  display: block;
  width: 100%;
  padding: 6px 8px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--ink);
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
}
.ctx-item:hover { background: var(--card-2); }
</style>