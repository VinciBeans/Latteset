// outlineStore（modules.md §9.2）：LaTeX 文档结构树（大纲）。
// 解析逻辑已下沉 Rust（2026-09-03：crates/latteset-core/src/outline.rs + get_outline 命令，
// cli-mcp-plan.md P1-4）——本 store 只负责触发时机、传参（实时缓冲/兜底文件列表）
// 与结果呈现；点击大纲项 → editor.openFile + SyncTeX 正向定位见 goTo。
// 刷新时机：项目打开、编译成功、结构变化（files-changed structural）——由 App/events 触发。
//
// 增量（roadmap ⑦a，2026-09）：只提交**改动过**的缓冲（`lastSent` 记录上次上报的内容），
// 并上报当前打开的标签集合（后端据此淘汰已关闭文件的缓冲）。未变化的文件由后端按内容指纹
// 复用扫描结果——实测这条路径把「每次编译成功都全量重扫」降为读盘 + 指纹比对。
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { useEditorStore } from "./editor";
import { useProjectStore } from "./project";
import { useSyncTex } from "../composables/useSyncTex";
import { ipc } from "../services/ipc";
import type { OutlineNode } from "../bindings";

export type { OutlineNode };

export const useOutlineStore = defineStore("outline", () => {
  const items = ref<OutlineNode[]>([]);
  /** 刷新序号：并发 refresh 时只有最新一次写结果，旧刷新完成即放弃（防陈旧覆盖）。 */
  let refreshSeq = 0;
  /**
   * 上次**已上报**给后端的缓冲内容（path → content）。
   * 与 `editor.buffers` 的值比较（字符串按值比较）即可判定"这个文件变了吗"；
   * `projectRoot` 变化（切项目/重开项目）时必须清空——后端的缓冲缓存是按项目根作废的，
   * 否则切回旧项目时未变化的脏缓冲不会被重发，后端会读到磁盘上的旧内容。
   */
  let lastSent = new Map<string, string>();
  let lastRoot: string | null = null;
  /** 合并进行中的刷新：结构变化/编译成功可能连发（实测一次结构事件风暴里连发了 11 次），
   *  并发刷新会让"只发改动缓冲"的差分失效（每次都在上一次写回 `lastSent` 之前发出），
   *  且后端缓冲缓存的写入顺序不再确定。故：进行中就不再发，只记一个"待补一次"的意图。 */
  let inFlight = false;
  let refreshAgain = false;

  const isEmpty = computed(() => items.value.length === 0);

  /** 重建大纲：解析在 Rust（get_outline）；前端只提交**变化过的**缓冲与打开的标签集合。
   *  未变化的打开文件由后端沿用上次上报的缓冲内容（不会回落到磁盘，见 core `OutlineCache`）。 */
  async function refresh() {
    if (inFlight) {
      refreshAgain = true;
      return;
    }
    inFlight = true;
    try {
      do {
        refreshAgain = false;
        await refreshOnce();
      } while (refreshAgain);
    } finally {
      inFlight = false;
    }
  }

  async function refreshOnce() {
    const mySeq = ++refreshSeq;
    const project = useProjectStore();
    if (!project.project) {
      items.value = [];
      lastSent.clear();
      lastRoot = null;
      return;
    }
    // 项目根变了 → 后端缓存已作废，本次必须全量重发
    if (lastRoot !== project.root) {
      lastSent.clear();
      lastRoot = project.root;
    }
    const editor = useEditorStore();
    const openPaths: string[] = [];
    const changed: { path: string; content: string }[] = [];
    for (const [path, content] of editor.buffers) {
      openPaths.push(path);
      if (lastSent.get(path) !== content) changed.push({ path, content });
    }
    // 无根文件（探测失败/多候选未指定）→ 解析全部 .tex（文件树已过滤+排序，旧语义保持）
    const files = project.project.root_file
      ? null
      : project.tree
          .filter((e) => !e.is_dir && /\.tex$/i.test(e.name))
          .map((e) => e.path)
          .sort();
    const res = await ipc.getOutline(changed, files, openPaths);
    // 这次提交已到达后端（拿到了响应），无论结果是否被采用都要记账——否则下一轮会把同一批
    // 缓冲再推一遍（"只发改动"的差分就白做了）。
    for (const b of changed) lastSent.set(b.path, b.content);
    for (const p of [...lastSent.keys()]) {
      if (!openPaths.includes(p)) lastSent.delete(p); // 标签已关 → 下轮不需要再记
    }
    // 并发守卫（现在由 refresh 的合并保证，保留作最后一道）：更晚的一次已经写了结果就别覆盖。
    if (mySeq !== refreshSeq) return;
    items.value = res;
  }

  /** 点击大纲项：揭示源码 + SyncTeX 正向高亮/居中 PDF 对应页（尽力而为）。 */
  async function goTo(node: OutlineNode) {
    void useEditorStore().openFile(node.file, node.line);
    try {
      await useSyncTex().forward(node.file, node.line, 0);
    } catch (e) {
      console.warn("大纲点击：SyncTeX 正向定位失败（仅跳源码）：", e);
    }
  }

  return { items, isEmpty, refresh, goTo };
});
