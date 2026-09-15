// compileStore（modules.md §9.2）：编译状态/队列/错误列表，与后端事件对齐。
import { defineStore } from "pinia";
import { ref } from "vue";
import type { CompilePhase, ErrorEntry, FailureKind } from "../bindings";

export const useCompileStore = defineStore("compile", () => {
  const phase = ref<CompilePhase>("success"); // 初始无编译
  const kind = ref<FailureKind | null>(null);
  const errors = ref<ErrorEntry[]>([]);
  const hasError = ref(false);
  /**
   * **当前屏幕上的 PDF 是否为草稿**（roadmap ㉘）：编辑触发的 Quick 单趟会让目录/交叉引用
   * 页码落后一趟，直到空闲收敛的 Full 成功才追上。
   *
   * 只在 success 时按事件里的 `draft` 更新：
   * - queued/running：PDF 还是上一次的，状态未变，不清提示；
   * - failed：PDF 同样是上一次的（失败不产出新 PDF），故保持原值。
   */
  const draft = ref(false);
  /**
   * **编译进行中已排版的页数**（roadmap「阶段 2 · 流式输出」）：来自引擎输出的 `[N]` 标记，
   * 由 `compile-progress` 事件更新（只在数值变大时发）。
   *
   * **非权威**：只在 `running` 期间有意义——编译结束由 `compile-status` 给终态，这里清零。
   * 事件可能晚于终态到达（进程刚退出时的最后一批输出），故 `setProgress` 在非 running 时忽略。
   */
  const pages = ref(0);

  function setStatus(p: CompilePhase, k: FailureKind | null, isDraft: boolean) {
    phase.value = p;
    kind.value = k;
    if (p === "running") {
      // 编译中清空错误（design.md 错误列表时机）+ 进度归零（新的一趟）
      errors.value = [];
      hasError.value = false;
      pages.value = 0;
    }
    if (p === "failed") {
      hasError.value = true;
      pages.value = 0;
    }
    if (p === "success") {
      // 成功态清空错误：若无 running 前置（直接 success），上一次失败的错误会残留到「就绪」。
      errors.value = [];
      hasError.value = false;
      draft.value = isDraft;
      pages.value = 0;
    }
  }

  /** 流式进度：只在"排版中"接受（终态之后到达的旧进度一律丢弃）。 */
  function setProgress(next: number) {
    if (phase.value !== "running") return;
    if (next > pages.value) pages.value = next;
  }

  /**
   * 流式错误（roadmap「阶段 2」）：编译进行中解析到的**致命错误**（不含 Overfull 这类警告）。
   *
   * 只在"排版中"接受：runner 收尾时会补发最后一批中间态，那批可能**晚于**终态
   * `errors-updated` 抵达；照单全收就会把权威列表顶掉——真机实测到过（超时诊断 1 条被 30 条
   * 实时条目覆盖）。终态一到（phase 变化）这里就自动闭嘴，权威结果只由 `setErrors` 写入。
   *
   * 不动 `hasError`：它表示"本次编译以失败告终"，实时错误不代表结论（此刻状态栏 phase chip 显示"排版中…"）。
   */
  function setLiveErrors(list: ErrorEntry[]) {
    if (phase.value !== "running") return;
    errors.value = list;
  }

  function setErrors(list: ErrorEntry[]) {
    errors.value = list;
  }

  return {
    phase,
    kind,
    errors,
    hasError,
    draft,
    pages,
    setStatus,
    setProgress,
    setLiveErrors,
    setErrors,
  };
});
