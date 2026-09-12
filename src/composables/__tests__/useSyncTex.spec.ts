// useSyncTex 单测（roadmap ⑤ 加固）：反向定位的三种结果 + 失败可见提示。
// 关键回归点：**绝不能把生成文件（tmp/main.toc）当源码打开**——以前会开出一个用户没写过的标签页。
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { createApp, h } from "vue";
import { setActivePinia, createPinia } from "pinia";
import { useSyncTex } from "../useSyncTex";
import { useEditorStore } from "../../stores/editor";
import { usePreviewStore } from "../../stores/preview";
import { ipc } from "../../services/ipc";

vi.mock("../../services/ipc", () => ({
  ipc: {
    synctexInverse: vi.fn(),
    synctexForward: vi.fn(),
  },
}));

function withSetup<T>(composable: () => T): T {
  let result!: T;
  const app = createApp({ setup() { result = composable(); return () => h("i"); } });
  app.mount(document.createElement("div"));
  return result;
}

describe("useSyncTex（⑤ 加固）", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });
  afterEach(() => vi.clearAllMocks());

  it("正常命中源码：打开文件并跳行，不产生提示", async () => {
    vi.mocked(ipc.synctexInverse).mockResolvedValue({
      source: { file: "E:/proj/chapters/intro.tex", line: 22, column: -1 },
      note: null,
    });
    const editor = useEditorStore();
    const openSpy = vi.spyOn(editor, "openFile").mockResolvedValue(undefined);
    const { inverse } = withSetup(() => useSyncTex());
    await inverse(5, 100, 600);
    expect(openSpy).toHaveBeenCalledWith("E:/proj/chapters/intro.tex", 22);
    expect(usePreviewStore().syncNote).toBeNull();
  });

  it("命中生成文件：不打开任何文件，只给提示", async () => {
    vi.mocked(ipc.synctexInverse).mockResolvedValue({
      source: null,
      note: "此处来自自动生成的文件 main.toc（目录/参考文献/索引等由 LaTeX 生成），没有对应的源码行",
    });
    const editor = useEditorStore();
    const openSpy = vi.spyOn(editor, "openFile").mockResolvedValue(undefined);
    const { inverse } = withSetup(() => useSyncTex());
    await inverse(3, 100, 560);
    expect(openSpy).not.toHaveBeenCalled();
    expect(usePreviewStore().syncNote).toContain("main.toc");
  });

  it("回落到最近源码：跳转 + 保留「已回落」提示", async () => {
    vi.mocked(ipc.synctexInverse).mockResolvedValue({
      source: { file: "E:/proj/main.tex", line: 37, column: -1 },
      note: "此处是自动生成的内容，已回落到最近的源码（main.tex:37，向下探测）",
    });
    const editor = useEditorStore();
    const openSpy = vi.spyOn(editor, "openFile").mockResolvedValue(undefined);
    const { inverse } = withSetup(() => useSyncTex());
    await inverse(3, 100, 640);
    expect(openSpy).toHaveBeenCalledWith("E:/proj/main.tex", 37);
    expect(usePreviewStore().syncNote).toContain("已回落");
  });

  it("反向失败（未编译/无同步数据）：不抛异常、给出提示", async () => {
    vi.mocked(ipc.synctexInverse).mockRejectedValue({ code: "Internal", message: "同步失败" });
    const editor = useEditorStore();
    const openSpy = vi.spyOn(editor, "openFile").mockResolvedValue(undefined);
    const { inverse } = withSetup(() => useSyncTex());
    await expect(inverse(1, 10, 10)).resolves.toBeUndefined();
    expect(openSpy).not.toHaveBeenCalled();
    expect(usePreviewStore().syncNote).toContain("先编译一次");
  });

  it("正向失败：给出可见提示（此前只有 console.error）", async () => {
    vi.mocked(ipc.synctexForward).mockRejectedValue({ code: "Internal", message: "同步失败" });
    const { forward } = withSetup(() => useSyncTex());
    await forward("E:/proj/main.tex", 12, 3);
    expect(usePreviewStore().syncNote).toContain("正向定位失败");
  });

  it("正向成功：设置高亮、不清提示逻辑（不写 note）", async () => {
    vi.mocked(ipc.synctexForward).mockResolvedValue({ page: 2, x: 79.37, y: 387.56 });
    const preview = usePreviewStore();
    const { forward } = withSetup(() => useSyncTex());
    await forward("E:/proj/main.tex", 12, 3);
    expect(preview.highlight).toEqual({ page: 2, x: 79.37, y: 387.56 });
    expect(preview.syncNote).toBeNull();
  });
});
