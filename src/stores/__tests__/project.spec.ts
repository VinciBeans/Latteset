// projectStore 单测：normalizePath / resolvePath 路径归一化 + 文件树增量刷新（modules.md §9.2 / §12）。
// 覆盖 2026-08-25 修复：反向定位返回正斜杠+`./` 的绝对路径 → 须归一为已开标签路径。
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { setActivePinia, createPinia } from "pinia";
import { normalizePath, relativizePath, useProjectStore } from "../project";
import { ipc } from "../../services/ipc";

vi.mock("../../services/ipc", () => ({
  ipc: { listDir: vi.fn(async () => [{ name: "main.tex", path: "E:/proj/main.tex", is_dir: false }]) },
}));

describe("normalizePath", () => {
  it("剥离磁盘绝对路径中的 `./`", () => {
    expect(normalizePath("E:/Works/tex-presso/test_file/projects/multifile/./main.tex")).toBe(
      "E:/Works/tex-presso/test_file/projects/multifile/main.tex"
    );
  });

  it("无 `./` 的绝对路径原样保留", () => {
    expect(normalizePath("E:/Works/tex-presso/test_file/projects/multifile/main.tex")).toBe(
      "E:/Works/tex-presso/test_file/projects/multifile/main.tex"
    );
  });

  it("合并 `..` 与折叠连续斜杠", () => {
    expect(normalizePath("/home/u/proj/a/../chapters/./b.tex")).toBe("/home/u/proj/chapters/b.tex");
  });

  it("相对路径的 `./` 剥离（不变绝对）", () => {
    expect(normalizePath("./main.tex")).toBe("main.tex");
  });

  it("根绝对路径保留前导 `/`", () => {
    expect(normalizePath("/home/u/proj/main.tex")).toBe("/home/u/proj/main.tex");
  });
});

describe("resolvePath", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  function withProject(): ReturnType<typeof useProjectStore> {
    const store = useProjectStore();
    store.project = { root: "E:/Works/tex-presso/test_file/projects/multifile" } as never;
    return store;
  }

  it("绝对路径（含 `./`）归一为项目内正斜杠路径", () => {
    const store = withProject();
    expect(store.resolvePath("E:/Works/tex-presso/test_file/projects/multifile/./main.tex")).toBe(
      "E:/Works/tex-presso/test_file/projects/multifile/main.tex"
    );
  });

  it("相对 `./main.tex` 拼接项目根", () => {
    const store = withProject();
    expect(store.resolvePath("./main.tex")).toBe("E:/Works/tex-presso/test_file/projects/multifile/main.tex");
  });

  it("`\\\\?\\` verbatim 防御直通", () => {
    const store = withProject();
    expect(store.resolvePath("\\\\?\\C:\\x\\y.tex")).toBe("\\\\?\\C:\\x\\y.tex");
  });
});

describe("relativizePath（roadmap P0-②-1：候选绝对路径 → 项目覆盖用的相对路径）", () => {
  const root = "E:/Works/tex-presso/test_file/projects/multifile";

  it("项目内子路径 → 相对路径", () => {
    expect(relativizePath(`${root}/chapters/intro.tex`, root)).toBe("chapters/intro.tex");
  });

  it("根目录直接子文件 → 文件名", () => {
    expect(relativizePath(`${root}/main.tex`, root)).toBe("main.tex");
  });

  it("后端的反斜杠绝对路径同样可转（Windows canonicalize 形态）", () => {
    expect(relativizePath("E:\\Works\\tex-presso\\test_file\\projects\\multifile\\main.tex", root)).toBe(
      "main.tex"
    );
  });

  it("含 `./` 的路径先归一化", () => {
    expect(relativizePath(`${root}/./main.tex`, root)).toBe("main.tex");
  });

  it("中文路径正确转换（不按字节截断）", () => {
    const cn = "E:/项目/中文测试工程";
    expect(relativizePath(`${cn}/章节/第一章.tex`, cn)).toBe("章节/第一章.tex");
    expect(relativizePath(`${cn}/中文主文件.tex`, cn)).toBe("中文主文件.tex");
  });

  it("项目外路径 → 空串（调用方据此拒绝发起更新）", () => {
    expect(relativizePath("E:/Works/other/main.tex", root)).toBe("");
  });

  it("根自身 / 空前缀 → 空串", () => {
    expect(relativizePath(root, root)).toBe("");
    expect(relativizePath("", root)).toBe("");
    expect(relativizePath(`${root}/main.tex`, "")).toBe("");
  });

  it("同名前缀但不构成路径边界的项目外路径 → 空串（防 `multifile2` 误判）", () => {
    expect(relativizePath("E:/Works/tex-presso/test_file/projects/multifile2/main.tex", root)).toBe("");
  });
});

describe("refreshTreeDebounced（文件树增量刷新）", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    const store = useProjectStore();
    store.project = { root: "E:/Works/tex-presso/test_file/projects/multifile" } as never;
    vi.useFakeTimers();
    vi.mocked(ipc.listDir).mockClear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("内容修改（structural=false）→ 不重建文件树（增量：跳过）", async () => {
    const store = useProjectStore();
    store.refreshTreeDebounced(false); // 如自动保存
    await vi.advanceTimersByTimeAsync(500);
    expect(vi.mocked(ipc.listDir)).not.toHaveBeenCalled();
    expect(store.tree).toEqual([]); // 树未被重建
  });

  it("结构变化（structural=true）→ 防抖后重建文件树", async () => {
    const store = useProjectStore();
    store.refreshTreeDebounced(true); // 如新建/删除文件
    await vi.advanceTimersByTimeAsync(500);
    expect(vi.mocked(ipc.listDir)).toHaveBeenCalledTimes(1);
    expect(store.tree.length).toBe(1); // 树已重建（拿到目录项）
  });

  it("先结构变化后内容修改 → 仍重建一次（保留了结构变化意图）", async () => {
    const store = useProjectStore();
    store.refreshTreeDebounced(true);
    store.refreshTreeDebounced(false); // 防抖窗口内：不覆盖结构变化意图
    await vi.advanceTimersByTimeAsync(500);
    expect(vi.mocked(ipc.listDir)).toHaveBeenCalledTimes(1);
    expect(store.tree.length).toBe(1);
  });
});
