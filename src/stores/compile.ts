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

  function setStatus(p: CompilePhase, k: FailureKind | null, isDraft: boolean) {
    phase.value = p;
    kind.value = k;
    if (p === "running") {
      // 编译中清空错误（design.md 错误列表时机）
      errors.value = [];
      hasError.value = false;
    }
    if (p === "failed") {
      hasError.value = true;
    }
    if (p === "success") {
      // 成功态清空错误：若无 running 前置（直接 success），上一次失败的错误会残留到「就绪」。
      errors.value = [];
      hasError.value = false;
      draft.value = isDraft;
    }
  }

  function setErrors(list: ErrorEntry[]) {
    errors.value = list;
  }

  return { phase, kind, errors, hasError, draft, setStatus, setErrors };
});
