<!-- 文件树（modules.md §9.4）：从扁平 list_dir 结果构建嵌套。展开状态只在组件内。 -->
<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { useProjectStore } from "../stores/project";
import { useEditorStore } from "../stores/editor";
import { useSettingsStore } from "../stores/settings";
import { ipc } from "../services/ipc";
import { fuzzyMatch } from "../services/fuzzy";
import type { DirEntryInfo, DocKind, DocLanguage } from "../bindings";
import FileTreeItem from "./FileTreeItem.vue";
import NewFileWizard from "./NewFileWizard.vue";
import ConfirmDialog from "./ConfirmDialog.vue";

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

/** 右键菜单（位置 + 落点 + 目标节点）；菜单项由本组件渲染 —— 只有它知道项目根与创建流程。 */
const menu = ref<{
  x: number;
  y: number;
  dirPath: string;
  label: string;
  /** 被右键的节点本身（改名用；"落点目录"是另一回事）。 */
  nodePath: string;
} | null>(null);

/** 右键：目录 ⇒ 建在它里面；文件 ⇒ 建在它所在的目录里。 */
function openMenu(payload: { x: number; y: number; path: string; isDir: boolean; name: string }) {
  if (!project.project) return;
  createError.value = "";
  const dirPath = payload.isDir ? payload.path : payload.path.slice(0, Math.max(0, payload.path.lastIndexOf("/")));
  menu.value = {
    // 贴着视口右下边时往回收一点，菜单不出屏
    x: Math.min(payload.x, window.innerWidth - 190),
    y: Math.min(payload.y, window.innerHeight - 140),
    dirPath,
    label: payload.isDir ? payload.name : `${payload.name} 所在目录`,
    nodePath: payload.path,
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

// ---- ㊼ 删除：悬停垃圾桶 → 二次确认 → 递归删除 + 编辑器收口 ----

/** 待确认的删除目标（非空 = 弹窗开着）。 */
const pendingDelete = ref<{ path: string; isDir: boolean; name: string; count: number } | null>(null);
const deleteBusy = ref(false);
const deleteError = ref("");

/** 目录里有多少个文件（从**已有的树**里数，不额外读盘）：确认弹窗要把后果讲清楚。 */
function countFilesUnder(path: string): number {
  const base = path.replace(/\\/g, "/");
  return project.tree.filter((e) => {
    const p = e.path.replace(/\\/g, "/");
    return !e.is_dir && p.startsWith(`${base}/`);
  }).length;
}

function askDelete(payload: { path: string; isDir: boolean; name: string }) {
  if (!project.project) return;
  deleteError.value = "";
  pendingDelete.value = {
    ...payload,
    count: payload.isDir ? countFilesUnder(payload.path) : 0,
  };
}

/** 确认弹窗的行文案：**先说要删什么，再说后果**（目录要报里面有多少个文件）。 */
const deleteLines = computed(() => {
  const d = pendingDelete.value;
  if (!d) return [];
  const lines = [`${d.isDir ? "文件夹" : "文件"}：${d.name}`];
  if (d.isDir) {
    lines.push(
      d.count > 0
        ? `它里面还有 ${d.count} 个文件，会一起删除（不可撤销）。`
        : "该文件夹是空的（删除不可撤销）。"
    );
  } else {
    lines.push("删除后不可撤销（项目里没有回收站）。");
  }
  return lines;
});

async function confirmDelete() {
  const d = pendingDelete.value;
  const root = project.project?.root;
  if (!d || !root) return;
  deleteBusy.value = true;
  deleteError.value = "";
  try {
    await ipc.deletePath(d.path);
    // 编辑器收口：先关标签（目录要连子文件一起关）——文件已经没了，留着标签点进去只会报错
    editor.closeTabsUnder(d.path);
    pendingDelete.value = null;
    await project.refreshTree();
    await afterTreeMutation(d.path, null);
  } catch (e) {
    // 留窗给重试：把后端那句人话摆在弹窗里（路径被占用、权限不足等）
    deleteError.value = errorText(e);
  } finally {
    deleteBusy.value = false;
  }
}

// ---- ㊽ 改名：右键 → 行内输入 → 同目录改名 + 路径重映射 ----

const renamingPath = ref("");
const renameError = ref("");

function startRename(path: string) {
  menu.value = null;
  createError.value = "";
  renameError.value = "";
  renamingPath.value = path;
}

async function submitRename(payload: { path: string; name: string }) {
  const root = project.project?.root;
  if (!root) return;
  renamingPath.value = "";
  const parent = payload.path.slice(0, Math.max(0, payload.path.lastIndexOf("/")));
  const target = `${parent}/${payload.name}`;
  try {
    const newPath = await ipc.renamePath(payload.path, target);
    // 打开着的标签/缓冲/脏标记整体搬到新路径（漏掉的话：改名后编辑被当成外部修改、保存写回旧路径）
    editor.remapPaths(payload.path, newPath);
    await project.refreshTree();
    await afterTreeMutation(payload.path, newPath);
  } catch (e) {
    createError.value = errorText(e);
    renameError.value = errorText(e);
  }
}

/**
 * 删 / 改名之后的项目状态收口（两条路共用）。
 *
 * 只有一件事要做，但**漏了就静默坏掉**：被动过的如果正是**根文件**，内存里的 `root_file`
 * 会指向一个不存在（或改名前）的路径 ⇒ 之后每次编译都失败，而且用户看不出为什么。
 * - 有手动覆盖（`settings.root_file` 非空）⇒ 走 `update_settings`：删 ⇒ 清覆盖（回到自动探测）；
 *   改名 ⇒ 覆盖改成新名字。后端在这一支里本来就会重算并同步内存 root_file。
 * - 没有覆盖（自动探测出来的根）⇒ 重开项目重探一次。
 *
 * 最后**补一次编译**：watch 在改名/删除那一刻已经触发过一次编译，而那次用的还是旧根（必然失败）
 * ⇒ 不补的话状态栏会一直停在「失败」，直到用户再敲一个字。
 *
 * ⚠ 比对用的是 `oldPath`（被动过之前的路径）：`root_file` 此刻还指向它。用新路径比会永远不成立
 * —— 真机验过，症状就是"改了根文件名之后编译一直失败"。
 */
async function afterTreeMutation(oldPath: string, newPath: string | null) {
  const p = project.project;
  if (!p || !p.root_file) return;
  const same = (a: string, b: string) => a.replace(/\\/g, "/") === b.replace(/\\/g, "/");
  if (!same(p.root_file, oldPath)) return;
  if (settings.settings?.root_file) {
    await settings.update({ root_file: newPath ? relativeToRoot(newPath) : null });
    await project.syncProject();
  } else {
    await project.openProject(p.root);
  }
  try {
    await ipc.compileNow();
  } catch (e) {
    // 删掉的若是唯一的根候选，此刻已经没有根文档了 —— 那是正常状态（状态栏会提示），不是错误
    console.debug("根文件变动后的补编译没跑起来：", e);
  }
}

/** 项目内绝对路径 → 相对路径（`update_settings` 只接受相对形态）。 */
function relativeToRoot(abs: string): string {
  const root = (project.project?.root ?? "").replace(/\\/g, "/").replace(/\/+$/, "");
  const a = abs.replace(/\\/g, "/");
  return a.startsWith(`${root}/`) ? a.slice(root.length + 1) : a;
}

// ---- ㊾ 搜索：树上方一个输入框，模糊匹配后只留命中项与它们的祖先 ----

const query = ref("");
const searching = computed(() => query.value.trim().length > 0);

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

/**
 * 搜索过滤（roadmap ㊾）：只留**命中项**与它们的**祖先目录**（祖先靠 structure 保住上下文——
 * 只给一串扁平结果，用户不知道文件在哪）。
 *
 * 目录本身命中时，把它的整棵子树留下：搜 `chapters` 想看的是"这个目录里有什么"，
 * 而不是一个空壳目录。
 */
function filterTree(nodes: Node[], q: string): Node[] {
  const out: Node[] = [];
  for (const n of nodes) {
    const hit = fuzzyMatch(relativeToRoot(n.entry.path), q);
    const kids = filterTree(n.children, q);
    if (hit) {
      out.push(n); // 命中 ⇒ 整棵子树留着（不动 children）
    } else if (kids.length > 0) {
      out.push({ entry: n.entry, children: kids }); // 只是"路过" ⇒ 只留下有命中的分支
    }
  }
  return out;
}

const tree = computed(() => {
  const base = buildTree();
  const q = query.value.trim();
  return q ? filterTree(base, q) : base;
});

/** 搜索结果计数（给搜索栏右侧显示"命中 N 项"，空结果要能一眼看出来）。 */
const hitCount = computed(() => {
  if (!searching.value) return project.tree.length;
  let n = 0;
  const walk = (ns: Node[]) => {
    for (const x of ns) {
      if (!x.entry.is_dir) n++;
      walk(x.children);
    }
  };
  walk(tree.value);
  return n;
});

function clearSearch() {
  query.value = "";
}
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

      <!-- 搜索栏（roadmap ㊾）：树上方一个输入框，模糊匹配文件名/路径 -->
      <div class="search-row">
        <span class="search-icon">🔍</span>
        <input
          v-model="query"
          class="search-input"
          type="text"
          spellcheck="false"
          placeholder="搜索文件（支持模糊，如 cit → chapters/intro.tex）"
          data-testid="tree-search"
          @keydown.esc.prevent="clearSearch"
        />
        <button v-if="searching" class="search-clear" title="清空搜索" data-testid="tree-search-clear" @click="clearSearch">×</button>
      </div>
      <div v-if="searching" class="search-count" data-testid="tree-search-count">
        命中 {{ hitCount }} 个文件{{ hitCount === 0 ? "（换个词试试）" : "" }}
      </div>

      <div class="tree-scroll">
        <div v-if="!project.project" class="empty">
          <span class="empty-card">
            <span class="empty-icon">📂</span>
            <span class="empty-title">还没打开项目</span>
            <span class="empty-hint">点左上角「打开项目」开始</span>
          </span>
        </div>
        <div v-else-if="searching && hitCount === 0" class="empty">
          <span class="empty-card">
            <span class="empty-icon">🔍</span>
            <span class="empty-title">没有匹配的文件</span>
            <span class="empty-hint">搜索按名字或路径模糊匹配</span>
          </span>
        </div>
        <FileTreeItem
          v-for="n in tree"
          :key="n.entry.path"
          :node="n"
          :expand-path="expandPath"
          :renaming-path="renamingPath"
          :force-expand="searching"
          @menu="openMenu"
          @remove="askDelete"
          @rename="submitRename"
          @rename-cancel="renamingPath = ''"
        />
      </div>
    </div>

    <!-- 节点右键菜单（roadmap ㊺/㊽）：目录 ⇒ 建在它里面；文件 ⇒ 建在它所在的目录里；改名对两者都可用 -->
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
        <button class="ctx-item" data-testid="ctx-rename" @click="startRename(menu.nodePath)">
          ✏️ 重命名
        </button>
      </div>
    </div>

    <!-- 删除二次确认（roadmap ㊼）：把"删什么、里面有多少东西"讲清楚再动手 -->
    <ConfirmDialog
      v-if="pendingDelete"
      title="删除确认"
      :lines="deleteLines"
      confirm-label="删除"
      :busy="deleteBusy"
      :error="deleteError"
      @confirm="confirmDelete"
      @cancel="pendingDelete = null"
    />

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

/* 搜索栏（㊾）：树上方一条窄输入框；命中数只在搜索时出现（不占平时的地方） */
.search-row {
  display: flex;
  align-items: center;
  gap: 5px;
  flex: 0 0 auto;
  margin: 6px 8px 0;
  padding: 0 6px;
  height: 25px;
  border: 1px solid var(--line-soft);
  border-radius: var(--radius-sm);
  background: var(--surface, #fff);
}
.search-row:focus-within { border-color: var(--blueberry); }
.search-icon { font-size: 10px; opacity: 0.55; }
.search-input {
  flex: 1 1 auto;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  font: inherit;
  font-size: 11.5px;
  color: var(--ink);
}
.search-clear {
  border: none;
  background: transparent;
  color: var(--ink-faint);
  font-size: 14px;
  line-height: 1;
  padding: 0 2px;
  cursor: pointer;
}
.search-clear:hover { color: var(--ink); }
.search-count {
  flex: 0 0 auto;
  padding: 4px 10px 0;
  font-size: 10.5px;
  color: var(--ink-faint);
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