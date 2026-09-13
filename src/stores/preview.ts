// previewStore（modules.md §9.2）：PDF 文档、滚动位置、SyncTeX 高亮、同步提示。
import { defineStore } from "pinia";
import { ref } from "vue";
import type { PdfUpdated } from "../bindings";

export interface Highlight {
  page: number;
  x: number;
  y: number;
}

export const usePreviewStore = defineStore("preview", () => {
  const pdfPath = ref<string | null>(null);
  /** 每次**需要重载**时递增，触发 PreviewPane 重载（modules.md §5.2）。 */
  const reloadKey = ref(0);
  const highlight = ref<Highlight | null>(null);
  /**
   * SyncTeX 提示（roadmap ⑤）：反向定位落到生成文件、同步数据缺失、正向失败等情况的**可见**反馈。
   * 短句、自动消失；`null` = 无提示。此前这些情况只有 `console.error`，用户看到的是"点了没反应"。
   */
  const syncNote = ref<string | null>(null);

  /**
   * 本次编译**变化的页号**（1-based，升序）——roadmap「增量编辑 × DVI」的 B/C 两个功能点
   * （docs/research/incremental-edit-x-dvi.md）。
   */
  const changedPages = ref<number[]>([]);
  /** 本次编译的页数；`0` = **页信息不可判定**（XDV 读不到）——与"零页变化"是两回事。 */
  const pagesTotal = ref(0);
  /** 因"排版结果逐页未变"而**跳过**重载的累计次数（诊断与单测用）。 */
  const skippedReloads = ref(0);
  /** 上一次通知过的 PDF 路径：换文件必须重载（哪怕是首轮无差异信息）。 */
  let lastNotifiedPath: string | null = null;

  /**
   * PDF 就绪（`pdf-updated`）。三种情形：
   *
   * 1. `pages === 0` → **无法判定**（XDV 缺失/损坏）→ 照常重载（保守）；
   * 2. `changedPages` 为空且 `pages > 0` → **逐页字节相同** → **跳过重载**：新旧 PDF 在页级别
   *    等价，刷新只会白重绘一遍并可能让滚动位置跳动（实测"只加一行注释"就属于这种，125 页 0 变化）；
   * 3. 其余 → 重载，并把变化页集合交给 PreviewPane 做"只重绘变化页"。
   */
  function onPdfUpdated(payload: PdfUpdated) {
    const pathChanged = lastNotifiedPath !== payload.path;
    lastNotifiedPath = payload.path;
    pdfPath.value = payload.path;
    changedPages.value = payload.changed_pages ?? [];
    pagesTotal.value = payload.pages ?? 0;

    const pages = pagesTotal.value;
    const unchanged = pages > 0 && changedPages.value.length === 0;
    if (unchanged && !pathChanged) {
      skippedReloads.value++;
      console.debug(`[preview] 跳过重载：${pages} 页逐页未变（第 ${skippedReloads.value} 次）`);
      return;
    }
    reloadKey.value++;
  }

  function setHighlight(h: Highlight | null) {
    highlight.value = h;
  }

  function setSyncNote(note: string | null) {
    syncNote.value = note;
  }

  return {
    pdfPath,
    reloadKey,
    highlight,
    syncNote,
    changedPages,
    pagesTotal,
    skippedReloads,
    onPdfUpdated,
    setHighlight,
    setSyncNote,
  };
});
