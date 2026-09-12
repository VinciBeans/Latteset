// useIdleConvergence（roadmap ㉘）：草稿编译（Quick 单趟）之后，停手就补一次完整收敛。
//
// 背景：编辑触发只跑单趟直调引擎（比完整 latexmk 快 40% 中位，design.md §延迟预算实测附节），
// 代价是目录/交叉引用页码落后一趟。停手后必须跑一次完整 latexmk 把它追上，否则 PDF 会一直
// 显示旧页码。
//
// 何时开火（三个条件同时成立）：
// 1. 最近一次编译**成功且是草稿**（失败不补：PDF 没变，收敛大概率同样失败，白烧 CPU）；
// 2. 没有排队/运行中的编译（新的编辑已经在路上，等它的结果再决定）；
// 3. 用户在 `DELAY_MS` 内没有敲键盘（见 `cancel()` —— App 在编辑器 change 时调用）。
//
// 为什么延时取 2s 而不是沿用 500ms 防抖值：调度器的合并队列只留一个待办条目，
// 正在跑的收敛 Full 会把紧随其后的编辑态 Quick 堵在队列里（大项目 Full ≈ 4s），
// 反而拖慢「编辑→出图」。所以延时要明显长于防抖，只在用户真的停下时才占住调度器。
// 期间 UI 显示「引用待更新」（StatusBar），不影响正确性表达。
import { onBeforeUnmount, watch } from "vue";
import { ipc } from "../services/ipc";
import { useCompileStore } from "../stores/compile";

/** 停手多久后收敛（毫秒）。 */
export const DELAY_MS = 2000;

export function useIdleConvergence(delayMs: number = DELAY_MS) {
  const compile = useCompileStore();
  let timer: ReturnType<typeof setTimeout> | undefined;

  /** 取消待收敛（用户又敲键盘、或已有编译在跑时调用）。 */
  function cancel() {
    if (timer !== undefined) {
      clearTimeout(timer);
      timer = undefined;
    }
  }

  watch(
    () => [compile.phase, compile.draft] as const,
    ([phase, isDraft]) => {
      cancel();
      // 新的编译在排队/运行 → 等它的终态；成功草稿会重新排定收敛
      if (phase === "queued" || phase === "running") return;
      if (phase !== "success" || !isDraft) return;
      timer = setTimeout(() => {
        timer = undefined;
        // 手动编译命令 = Full（compose::compile_request_manual），正是收敛所需强度
        ipc.compileNow().catch((e) => console.error("空闲收敛编译失败：", e));
      }, delayMs);
    },
  );

  onBeforeUnmount(cancel);

  return { cancel };
}
