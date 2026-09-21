// projectStore（modules.md §9.2）：项目、根文件、文件树。
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { ipc } from "../services/ipc";
import { normalizePath, toSlashes } from "../services/paths";
import type { DirEntryInfo, ProjectInfo } from "../bindings";

// 路径工具的真身在 `services/paths.ts`（roadmap ㊿ 统一）；这里**原样再导出**，
// 让既有调用点（`RootFilePicker` 等）不必改 import 路径。
export { normalizePath, relativize as relativizePath } from "../services/paths";

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
    const n = toSlashes(p);
    const abs = n.startsWith("/") || /^[A-Za-z]:/.test(n);
    const base = abs ? n : toSlashes(root.value) + "/" + n.replace(/^\.\//, "");
    return normalizePath(base);
  }

  /** 重新读取当前项目信息（roadmap P0-②-1）：root_file 变化后同步内存状态用。
   *  后端 `get_project` 会按当前覆盖重新探测候选，语义与 `open_project` 一致。 */
  async function syncProject() {
    project.value = await ipc.getProject();
    return project.value;
  }

  /**
   * 无根文档时的**静默重探测**（roadmap ㊺ §6.13.1-B）：保存/新建后调用。
   *
   * 复用 `openProject` 那条路 ⇒ 与初次打开项目的探测是**同一套口径**（不新增判定）；
   * 已经有根文件时直接返回（`project.root_file` 非空 ⇒ 不再打扰）。
   *
   * 探到根文件就顺手补一次编译：这次落盘正是"第一份能编译的文档"写下去的时刻，而 watch
   * 那条触发链在探测到根文件**之前**已经把这个变化丢掉了（当时它只能看到 `root_file = None`）
   * ⇒ 不补的话用户得再敲一个字、或手动点「编译」才会出 PDF。
   *
   * **静默**：不弹任何提示；失败只记 console（下次保存/新建再试）。
   */
  async function rescanRoot(): Promise<boolean> {
    const cur = project.value;
    if (!cur || cur.root_file) return false;
    let info: ProjectInfo | null = null;
    try {
      info = await openProject(cur.root);
    } catch (e) {
      console.debug("静默重探测根文件失败（下次保存再试）：", e);
      return false;
    }
    if (!info?.root_file) return false;
    try {
      await ipc.compileNow();
    } catch (e) {
      // 探测本身成功了：编译没起来不该让调用方以为"还没探到"（标签该撤还是撤）
      console.debug("探测到根文件后的首次编译失败：", e);
    }
    return true;
  }

  return {
    project,
    tree,
    root,
    openProject,
    syncProject,
    rescanRoot,
    refreshTree,
    refreshTreeDebounced,
    resolvePath,
  };
});
