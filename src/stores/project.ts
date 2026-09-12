// projectStore（modules.md §9.2）：项目、根文件、文件树。
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { ipc } from "../services/ipc";
import type { DirEntryInfo, ProjectInfo } from "../bindings";

export const useProjectStore = defineStore("project", () => {
  const project = ref<ProjectInfo | null>(null);
  const tree = ref<DirEntryInfo[]>([]);

  const root = computed(() => project.value?.root ?? "");

  async function openProject(folder: string) {
    project.value = await ipc.openProject(folder);
    await refreshTree();
    return project.value;
  }

  /** 全量重建文件树（打开项目 / 结构变化时调用）。 */
  async function refreshTree() {
    if (!project.value) return;
    tree.value = await ipc.listDir(project.value.root);
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  /** 是否有待处理的结构变化（防抖窗口内被内容修改覆盖也不丢失）。 */
  let pendingStructural = false;
  /** 300ms 防抖重建（modules.md §5.5）。`structural=true`（增/删/重命名）才重建；
   *  内容修改（自动保存等）传 false → 跳过，避免每次编辑都全量重扫（增量刷新）。
   *  结构变化一旦 pending，后续内容修改不清除该意图。 */
  function refreshTreeDebounced(structural = false) {
    if (structural) pendingStructural = true;
    clearTimeout(timer);
    if (!pendingStructural) return; // 内容修改且无结构变化 pending → 跳过
    timer = setTimeout(() => {
      refreshTree().catch(() => {});
      pendingStructural = false;
    }, 300);
  }

  /** 相对路径（如 ./main.tex、chapters/a.tex）解析为项目内绝对路径。
   *  统一输出正斜杠（存储键/后端请求一致）；Windows 接受盘符开头（\\?\ 防御直通）；WSL：/ 开头。
   *  归一化 `.`/`..`/连续斜杠——如 synctex 反向返回的 `E:/proj/./main.tex`，
   *  与已打开的 `E:/proj/main.tex` 必须归一为同一存储键（否则重复开标签）。 */
  function resolvePath(p: string): string {
    if (p.startsWith("\\\\?\\")) return p; // 防御：verbatim 直通（后端应已剥离）
    const n = p.replace(/\\/g, "/");
    const abs = n.startsWith("/") || /^[A-Za-z]:/.test(n);
    const base = abs ? n : root.value.replace(/\\/g, "/") + "/" + n.replace(/^\.\//, "");
    return normalizePath(base);
  }

  /** 重新读取当前项目信息（roadmap P0-②-1）：root_file 变化后同步内存状态用。
   *  后端 `get_project` 会按当前覆盖重新探测候选，语义与 `open_project` 一致。 */
  async function syncProject() {
    project.value = await ipc.getProject();
    return project.value;
  }

  return {
    project,
    tree,
    root,
    openProject,
    syncProject,
    refreshTree,
    refreshTreeDebounced,
    resolvePath,
  };
});

/** 归一化文件路径：折叠连续斜杠、剥 `.`、合并 `..`（浏览器环境手写，不依赖 node:path）。
 *  磁盘绝对路径保留 `E:/...`；根绝对路径保留 `/...`。 */
export function normalizePath(p: string): string {
  const abs = p.startsWith("/") || /^[A-Za-z]:\//.test(p);
  const out: string[] = [];
  for (const seg of p.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") {
      const prev = out[out.length - 1];
      if (prev && prev !== ".." && !/^[A-Za-z]:$/.test(prev)) out.pop();
      else if (!abs && prev !== "..") out.push("..");
      continue; // 绝对路径下越界的 `..` 丢弃
    }
    out.push(seg);
  }
  if (abs) {
    const head = out[0] ?? "";
    return /^[A-Za-z]:$/.test(head) ? head + "/" + out.slice(1).join("/") : "/" + out.join("/");
  }
  return out.join("/");
}

/** 项目内绝对路径 → 项目根相对路径（正斜杠）。
 *
 *  用途（roadmap P0-②-1）：根文件选择器拿到的是后端返回的**绝对**候选路径，而
 *  `update_settings({ root_file })` 只接受**项目内相对路径**（`validate_overrides` 拒 `..`
 *  与空串），故必须在此转换。两侧都来自后端（canonicalize 后），统一归一化后做前缀比较。
 *
 *  不在项目根内 → 返回空串（调用方据此判为不可用，不应发起更新）。 */
export function relativizePath(abs: string, root: string): string {
  if (!abs || !root) return "";
  const a = normalizePath(abs.replace(/\\/g, "/"));
  const r = normalizePath(root.replace(/\\/g, "/"));
  if (!a || !r || a === r) return "";
  const prefix = r.endsWith("/") ? r : `${r}/`;
  return a.startsWith(prefix) ? a.slice(prefix.length) : "";
}
