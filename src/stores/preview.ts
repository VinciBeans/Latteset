// previewStore（modules.md §9.2）：PDF 文档、滚动位置、SyncTeX 高亮、同步提示。
import { defineStore } from "pinia";
import { ref } from "vue";

export interface Highlight {
  page: number;
  x: number;
  y: number;
}

export const usePreviewStore = defineStore("preview", () => {
  const pdfPath = ref<string | null>(null);
  /** 每次 pdf-updated 递增，触发 PreviewPane 重载（modules.md §5.2）。 */
  const reloadKey = ref(0);
  const highlight = ref<Highlight | null>(null);
  /**
   * SyncTeX 提示（roadmap ⑤）：反向定位落到生成文件、同步数据缺失、正向失败等情况的**可见**反馈。
   * 短句、自动消失；`null` = 无提示。此前这些情况只有 `console.error`，用户看到的是"点了没反应"。
   */
  const syncNote = ref<string | null>(null);

  function onPdfUpdated(path: string) {
    pdfPath.value = path;
    reloadKey.value++;
  }

  function setHighlight(h: Highlight | null) {
    highlight.value = h;
  }

  function setSyncNote(note: string | null) {
    syncNote.value = note;
  }

  return { pdfPath, reloadKey, highlight, syncNote, onPdfUpdated, setHighlight, setSyncNote };
});
