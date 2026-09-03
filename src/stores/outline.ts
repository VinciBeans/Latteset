// outlineStore（modules.md §9.2）：LaTeX 文档结构树（大纲）。
// 解析逻辑已下沉 Rust（2026-09-03：crates/texpresso-core/src/outline.rs + get_outline 命令，
// cli-mcp-plan.md P1-4）——本 store 只负责触发时机、传参（实时缓冲/兜底文件列表）
// 与结果呈现；点击大纲项 → editor.openFile + SyncTeX 正向定位见 goTo。
// 刷新时机：项目打开、编译成功、结构变化（files-changed structural）——由 App/events 触发。
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

  const isEmpty = computed(() => items.value.length === 0);

  /** 重建大纲：解析在 Rust（get_outline）；前端提交打开标签的实时缓冲（未落盘也反映，
   *  keep 旧实现「缓冲优先」语义）与无根文件时的兜底文件列表（与旧实现同源：项目文件树）。 */
  async function refresh() {
    const mySeq = ++refreshSeq;
    const project = useProjectStore();
    if (!project.project) {
      items.value = [];
      return;
    }
    const buffers = Array.from(useEditorStore().buffers.entries()).map(([path, content]) => ({
      path,
      content,
    }));
    // 无根文件（探测失败/多候选未指定）→ 解析全部 .tex（文件树已过滤+排序，旧语义保持）
    const files = project.project.root_file
      ? null
      : project.tree
          .filter((e) => !e.is_dir && /\.tex$/i.test(e.name))
          .map((e) => e.path)
          .sort();
    const res = await ipc.getOutline(buffers, files);
    // 并发守卫：期间若又触发了一次刷新（编译成功/结构变化连发），丢弃本次陈旧结果。
    // 也避免了两次 refresh 交错写 items 造成的覆盖顺序不确定性。
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
