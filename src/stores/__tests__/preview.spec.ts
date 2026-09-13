// previewStore 单测（modules.md §9.2 / docs/research/incremental-edit-x-dvi.md 的 B 功能点）：
// pdf-updated 的三种情形——无法判定 / 逐页未变 → 跳过重载 / 有变化 → 重载。
import { describe, it, expect, beforeEach } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { usePreviewStore } from "../preview";
import type { PdfUpdated } from "../../bindings";

const ev = (over: Partial<PdfUpdated> = {}): PdfUpdated => ({
  path: "E:/proj/main.pdf",
  changed_pages: [],
  pages: 0,
  ...over,
});

describe("previewStore.onPdfUpdated", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("有变化页 → 重载，并记住变化页集合", () => {
    const p = usePreviewStore();
    p.onPdfUpdated(ev({ changed_pages: [2, 5], pages: 7 }));
    expect(p.reloadKey).toBe(1);
    expect(p.changedPages).toEqual([2, 5]);
    expect(p.pagesTotal).toBe(7);
    expect(p.skippedReloads).toBe(0);
  });

  it("逐页未变（pages>0 且变化表为空）→ **跳过重载**", () => {
    const p = usePreviewStore();
    p.onPdfUpdated(ev({ changed_pages: [1], pages: 3 }));
    expect(p.reloadKey).toBe(1);
    // 第二次：内容与上次逐页相同（实测"只加一行注释"就是这个形态）
    p.onPdfUpdated(ev({ changed_pages: [], pages: 3 }));
    expect(p.reloadKey).toBe(1);
    expect(p.skippedReloads).toBe(1);
  });
  it("pages=0（无法判定）→ 照常重载，不算'未变'", () => {
    const p = usePreviewStore();
    p.onPdfUpdated(ev({ changed_pages: [], pages: 0 }));
    expect(p.reloadKey).toBe(1);
    expect(p.skippedReloads).toBe(0);
  });

  it("路径变化 → 即使变化表为空也要重载（换项目/换根文件）", () => {
    const p = usePreviewStore();
    p.onPdfUpdated(ev({ path: "E:/a/main.pdf", changed_pages: [], pages: 3 }));
    p.onPdfUpdated(ev({ path: "E:/a/main.pdf", changed_pages: [], pages: 3 }));
    expect(p.skippedReloads).toBe(1);
    p.onPdfUpdated(ev({ path: "E:/b/other.pdf", changed_pages: [], pages: 3 }));
    expect(p.reloadKey).toBe(2);
  });

  it("首次编译（无基线 → 全部页）不会误判为未变", () => {
    const p = usePreviewStore();
    p.onPdfUpdated(ev({ changed_pages: [1, 2, 3], pages: 3 }));
    expect(p.reloadKey).toBe(1);
  });
});
