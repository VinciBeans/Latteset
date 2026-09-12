// useIdleConvergence 单测（roadmap ㉘）：草稿编译后停手补一次完整收敛。
// mock services/ipc；用 fake timers 验证「成功草稿才排定、敲键盘/新编译取消、只收敛一次」。
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { createApp, h, nextTick } from "vue";
import { setActivePinia, createPinia } from "pinia";
import { useIdleConvergence, DELAY_MS } from "../useIdleConvergence";
import { useCompileStore } from "../../stores/compile";
import { ipc } from "../../services/ipc";

vi.mock("../../services/ipc", () => ({
  ipc: { compileNow: vi.fn(async () => null) },
}));

function withSetup<T>(composable: () => T): T {
  let result!: T;
  const app = createApp({ setup() { result = composable(); return () => h("i"); } });
  app.mount(document.createElement("div"));
  return result;
}

/** 推进到防抖点并 flush 微任务（watch 回调是异步的）。 */
async function idle(ms: number) {
  await vi.advanceTimersByTimeAsync(ms);
  await nextTick();
}

describe("useIdleConvergence", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("成功草稿 → 停手后收敛一次（走 Full 的手动编译命令）", async () => {
    const compile = useCompileStore();
    withSetup(() => useIdleConvergence());
    compile.setStatus("running", null, true);
    await nextTick();
    compile.setStatus("success", null, true);
    await idle(DELAY_MS - 1);
    expect(vi.mocked(ipc.compileNow)).toHaveBeenCalledTimes(0);
    await idle(1);
    expect(vi.mocked(ipc.compileNow)).toHaveBeenCalledTimes(1);
  });

  it("已收敛（成功且非草稿）不排定收敛", async () => {
    const compile = useCompileStore();
    withSetup(() => useIdleConvergence());
    compile.setStatus("running", null, false);
    await nextTick();
    compile.setStatus("success", null, false);
    await idle(DELAY_MS * 2);
    expect(vi.mocked(ipc.compileNow)).toHaveBeenCalledTimes(0);
  });

  it("失败不排定收敛：PDF 未变化，收敛同样会失败（不白烧 CPU）", async () => {
    const compile = useCompileStore();
    withSetup(() => useIdleConvergence());
    compile.setStatus("running", null, true);
    await nextTick();
    // 先成功一次草稿 → 已排定
    compile.setStatus("success", null, true);
    await nextTick();
    // 排定期间又来一次编译，且失败
    compile.setStatus("queued", null, true);
    await nextTick();
    compile.setStatus("failed", "content_error", true);
    await idle(DELAY_MS * 2);
    expect(vi.mocked(ipc.compileNow)).toHaveBeenCalledTimes(0);
  });

  it("排定后用户又敲键盘 → cancel 取消（把调度器让给编辑态 Quick）", async () => {
    const compile = useCompileStore();
    const conv = withSetup(() => useIdleConvergence());
    compile.setStatus("running", null, true);
    await nextTick();
    compile.setStatus("success", null, true);
    await idle(DELAY_MS - 100);
    conv.cancel(); // App.vue 在 @change 时调用
    await idle(DELAY_MS * 2);
    expect(vi.mocked(ipc.compileNow)).toHaveBeenCalledTimes(0);
  });

  it("排定后新的编译入队 → 取消旧排定，由新终态重新决定（不叠加）", async () => {
    const compile = useCompileStore();
    withSetup(() => useIdleConvergence());
    compile.setStatus("running", null, true);
    await nextTick();
    compile.setStatus("success", null, true);
    await idle(DELAY_MS - 100);
    compile.setStatus("queued", null, true); // 新编辑触发的编译
    await nextTick();
    await idle(DELAY_MS); // 旧排定若没取消，此时已开火
    expect(vi.mocked(ipc.compileNow)).toHaveBeenCalledTimes(0);
    // 新编译成功且仍是草稿 → 重新排定一次
    compile.setStatus("running", null, true);
    await nextTick();
    compile.setStatus("success", null, true);
    await idle(DELAY_MS);
    expect(vi.mocked(ipc.compileNow)).toHaveBeenCalledTimes(1);
  });
});
