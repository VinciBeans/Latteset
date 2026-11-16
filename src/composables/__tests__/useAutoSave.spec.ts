// useAutoSave 单测（modules.md §9.3）：防抖保存算法。
// mock services/ipc；用 fake timers 验证「反复 schedule 只在停顿后保存一次、成功才清脏」。
// 用 withSetup 提供组件实例（useAutoSave 内部用 onBeforeUnmount 清计时器，需组件上下文）。
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { createApp, h } from "vue";
import { setActivePinia, createPinia } from "pinia";
import { useAutoSave } from "../useAutoSave";
import { useEditorStore } from "../../stores/editor";
import { useSettingsStore } from "../../stores/settings";
import { ipc } from "../../services/ipc";

vi.mock("../../services/ipc", () => ({
  ipc: {
    saveAll: vi.fn(async (files: { path: string; content: string }[]) => files),
  },
}));

/** 在组件 setup 里调用 composable，使 onBeforeUnmount 等生命周期钩子正常注册。 */
function withSetup<T>(composable: () => T): T {
  let result!: T;
  const app = createApp({ setup() { result = composable(); return () => h("i"); } });
  app.mount(document.createElement("div"));
  return result;
}

const MAIN = "E:/Works/tex-presso/test_file/projects/multifile/main.tex";

describe("useAutoSave", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    const settings = useSettingsStore();
    settings.settings = {
      compile: { mode: "continuous", debounce_ms: 500, timeout_secs: 120 },
    } as never;
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("schedule 后到防抖点保存一次，并清脏", async () => {
    const editor = useEditorStore();
    editor.markDirty(MAIN, "v1");
    const auto = withSetup(() => useAutoSave());
    auto.schedule();
    await vi.advanceTimersByTimeAsync(500);
    expect(vi.mocked(ipc.saveAll)).toHaveBeenCalledTimes(1);
    expect(editor.dirty.has(MAIN)).toBe(false);
  });

  it("反复 schedule 重置计时器：只在最后一次停顿后保存一次", async () => {
    const editor = useEditorStore();
    editor.markDirty(MAIN, "v1");
    const auto = withSetup(() => useAutoSave());
    auto.schedule();
    await vi.advanceTimersByTimeAsync(200);
    auto.schedule(); // 重置
    await vi.advanceTimersByTimeAsync(300); // 累计 500，但 timer 已重置 → 未触发
    expect(vi.mocked(ipc.saveAll)).toHaveBeenCalledTimes(0);
    await vi.advanceTimersByTimeAsync(200); // 再 200 → 距第二次 schedule 500
    expect(vi.mocked(ipc.saveAll)).toHaveBeenCalledTimes(1);
    expect(editor.dirty.has(MAIN)).toBe(false);
  });

  it("无脏文件时 schedule 到点不调用 saveAll", async () => {
    const auto = withSetup(() => useAutoSave());
    auto.schedule();
    await vi.advanceTimersByTimeAsync(500);
    expect(vi.mocked(ipc.saveAll)).toHaveBeenCalledTimes(0);
  });

  it("flush 立即保存（跳过防抖）", async () => {
    const editor = useEditorStore();
    editor.markDirty(MAIN, "v1");
    const auto = withSetup(() => useAutoSave());
    auto.schedule();
    await auto.flush();
    expect(vi.mocked(ipc.saveAll)).toHaveBeenCalledTimes(1);
    expect(editor.dirty.has(MAIN)).toBe(false);
  });

  it("保存期间内容变化：不清脏，重排保存最新内容（陈旧保存竞态回归）", async () => {
    const editor = useEditorStore();
    editor.markDirty(MAIN, "v1");
    const auto = withSetup(() => useAutoSave());

    // 让本次 saveAll 挂起，直到测试手动放行（模拟磁盘写盘期间用户仍在输入）
    let resolve!: () => void;
    vi.mocked(ipc.saveAll).mockImplementationOnce(
      () => new Promise<null>((res) => { resolve = () => res(null); }),
    );

    auto.schedule();
    await vi.advanceTimersByTimeAsync(500); // 触发 run → saveAll 挂起
    expect(vi.mocked(ipc.saveAll)).toHaveBeenCalledTimes(1);

    // 保存期间用户继续输入 → buffer 变 v2，dirty 重新加入
    editor.markDirty(MAIN, "v2");

    // 放行本次保存（写盘的仍是 v1；保存成功，返回值不影响判定）
    resolve!();
    await vi.advanceTimersByTimeAsync(0);

    // 关键断言：buffer(v2) ≠ 已保存内容(v1)，不应清脏
    expect(editor.dirty.has(MAIN)).toBe(true);

    // 且重排了下次保存：防抖后再次 saveAll，写入最新 v2
    await vi.advanceTimersByTimeAsync(500);
    expect(vi.mocked(ipc.saveAll)).toHaveBeenCalledTimes(2);
    expect(vi.mocked(ipc.saveAll)).toHaveBeenLastCalledWith([{ path: MAIN, content: "v2" }]);
    expect(editor.dirty.has(MAIN)).toBe(false);
  });
});
