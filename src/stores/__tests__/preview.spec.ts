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

// 编译中预览（roadmap ㉞「流式出图」）：部分 PDF 是**另一个文件**，但属于**同一篇文档**。
describe("previewStore.onCompilePreview / clearLivePreview", () => {
  beforeEach(() => setActivePinia(createPinia()));

  const live = (pages: number, path = "E:/proj/tmp/main.preview.pdf") => ({ path, pages });

  it("有权威产物时：身份不动，字节来源切到部分 PDF", () => {
    const p = usePreviewStore();
    p.onPdfUpdated(ev({ changed_pages: [1], pages: 125 }));
    expect(p.reloadKey).toBe(1);
    p.onCompilePreview(live(121));
    expect(p.reloadKey).toBe(2); // 触发一次加载
    expect(p.docIdentity).toBe("E:/proj/main.pdf"); // 身份仍是权威路径 ⇒ 不重建 DOM
    expect(p.sourcePath).toBe("E:/proj/tmp/main.preview.pdf");
    expect(p.displayPath).toBe("E:/proj/tmp/main.preview.pdf");
  });

  it("首编（还没有权威产物）：部分 PDF 兜底当身份，否则空态会一直挡着", () => {
    const p = usePreviewStore();
    expect(p.displayPath).toBeNull();
    p.onCompilePreview(live(121));
    expect(p.docIdentity).toBe("E:/proj/tmp/main.preview.pdf");
    expect(p.displayPath).toBe("E:/proj/tmp/main.preview.pdf");
    // 权威产物到达：身份换成权威路径（这一次会走"换文档"重建，与今天首次出 PDF 同款）
    p.onPdfUpdated(ev({ changed_pages: [1], pages: 125 }));
    expect(p.docIdentity).toBe("E:/proj/main.pdf");
    expect(p.livePreview).toBeNull();
  });

  it("晚到的旧帧（页数没涨）直接丢，不触发重载", () => {
    const p = usePreviewStore();
    p.onCompilePreview(live(125));
    expect(p.reloadKey).toBe(1);
    p.onCompilePreview(live(121));
    expect(p.reloadKey).toBe(1);
    expect(p.livePreview?.pages).toBe(125);
  });

  it("权威产物到达时**即使「逐页未变」也要重载**：屏幕上是中间趟的字节，必须换成权威文件", () => {
    const p = usePreviewStore();
    p.onPdfUpdated(ev({ changed_pages: [1], pages: 125 }));
    p.onCompilePreview(live(125));
    expect(p.reloadKey).toBe(2);
    // 权威产物与上一份逐页未变（正常路径会跳过重载），但此刻屏幕上放的是部分 PDF
    p.onPdfUpdated(ev({ changed_pages: [], pages: 125 }));
    expect(p.skippedReloads).toBe(0);
    expect(p.reloadKey).toBe(3);
    expect(p.livePreview).toBeNull();
  });

  it("clearLivePreview：预览态才重载；失败收口后屏幕回到权威视图", () => {
    const p = usePreviewStore();
    p.onPdfUpdated(ev({ changed_pages: [1], pages: 125 }));
    expect(p.clearLivePreview()).toBe(false); // 不在预览态 ⇒ 空操作（避免多余重载）
    expect(p.reloadKey).toBe(1);
    p.onCompilePreview(live(121));
    expect(p.clearLivePreview()).toBe(true);
    expect(p.reloadKey).toBe(3);
    expect(p.livePreview).toBeNull();
    expect(p.docIdentity).toBe("E:/proj/main.pdf");
  });
});
