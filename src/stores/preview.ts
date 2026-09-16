// previewStore（modules.md §9.2）：PDF 文档、滚动位置、SyncTeX 高亮、同步提示。
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import type { PdfUpdated } from "../bindings";

export interface Highlight {
  page: number;
  x: number;
  y: number;
}

/** 编译中的部分 PDF（`compile-preview` 事件载荷；roadmap ㉞）。 */
export interface LivePreview {
  path: string;
  /** 该部分 PDF 的页数（只增不减 ⇒ 它同时是帧的版本号）。 */
  pages: number;
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
   * 编译中的**部分 PDF**（roadmap ㉞「流式出图」）：`null` = 不在预览态。
   *
   * 它与权威产物**刻意分成两条数据**（事件也是两个）：权威产物到达即退出预览态。
   */
  const livePreview = ref<LivePreview | null>(null);

  /**
   * **文档身份**＝PreviewPane 判断"要不要重建 canvas DOM / 重置页高"的依据。
   *
   * 权威路径优先；**首编**还没有权威产物时用部分 PDF 的路径兜底（那期间屏幕上只有它）。
   * 身份不变 ⇒ 换帧只重绘、不重建 DOM、滚动位置不动——这正是 ㉞ 的"滚动零帧偏移"判据要的。
   */
  const docIdentity = computed(() => pdfPath.value ?? livePreview.value?.path ?? "");
  /**
   * **字节来源**：编译中优先取部分 PDF。与 [`docIdentity`] 分开是刻意的——部分 PDF 是**另一个
   * 文件**（`tmp/<stem>.preview.pdf`），若让身份跟着它走，每换一帧都会渲染成"换文档"。
   */
  const sourcePath = computed(() => livePreview.value?.path ?? pdfPath.value);
  /** 屏幕上有没有东西可显示（空态判据）。 */
  const displayPath = computed(() => sourcePath.value);

  /**
   * 部分 PDF 就绪（`compile-preview`，只**库形态**会发）。
   *
   * `pages` 只增不减（出图闸门要求页数有涨才发）⇒ 拿它当版本号，晚到的旧帧直接丢。
   */
  function onCompilePreview(payload: LivePreview) {
    const cur = livePreview.value;
    if (cur && payload.pages <= cur.pages) return;
    livePreview.value = { path: payload.path, pages: payload.pages };
    reloadKey.value++;
  }

  /**
   * 退出预览态（编译终态，或权威产物到达）。返回"刚才是否在预览"——调用方据此判断要不要
   * 再重载一次：还在显示部分 PDF 而编译失败/中止时，屏幕必须回到权威视图（绝不能让中间态
   * 留在屏幕上冒充结果）。
   */
  function clearLivePreview(): boolean {
    if (!livePreview.value) return false;
    livePreview.value = null;
    reloadKey.value++;
    return true;
  }

  /**
   * PDF 就绪（`pdf-updated`）。三种情形：
   *
   * 1. `pages === 0` → **无法判定**（XDV 缺失/损坏）→ 照常重载（保守）；
   * 2. `changedPages` 为空且 `pages > 0` → **逐页字节相同** → **跳过重载**：新旧 PDF 在页级别
   *    等价，刷新只会白重绘一遍并可能让滚动位置跳动（实测"只加一行注释"就属于这种，125 页 0 变化）；
   * 3. 其余 → 重载，并把变化页集合交给 PreviewPane 做"只重绘变化页"。
   *
   * ⚠ 若此刻屏幕上放的是**部分 PDF**，"逐页未变"不能用来跳过重载：那份字节来自中间趟，
   * 与权威产物不是一回事 ⇒ 必须重载一次换成权威文件（`wasLive`）。
   */
  function onPdfUpdated(payload: PdfUpdated) {
    const wasLive = livePreview.value !== null;
    livePreview.value = null; // 权威产物到达 ⇒ 退出预览态（无论下面跳不跳过重载）
    const pathChanged = lastNotifiedPath !== payload.path;
    lastNotifiedPath = payload.path;
    pdfPath.value = payload.path;
    changedPages.value = payload.changed_pages ?? [];
    pagesTotal.value = payload.pages ?? 0;

    const pages = pagesTotal.value;
    const unchanged = pages > 0 && changedPages.value.length === 0;
    if (unchanged && !pathChanged && !wasLive) {
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
    livePreview,
    docIdentity,
    sourcePath,
    displayPath,
    onPdfUpdated,
    onCompilePreview,
    clearLivePreview,
    setHighlight,
    setSyncNote,
  };
});
