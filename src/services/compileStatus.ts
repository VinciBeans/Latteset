// 编译进行态文案（纯前端派生）：工具栏按钮与状态栏 phase 必须同口径。
//
// 后端的 `CompilePhase` 有 `queued` 与 `running` 两种进行态，而排队中的笔记/预览并没有在排版；
// 把两者都说成「排版中…」会让用户以为引擎已在跑（也就解释了为什么"点了没反应"）。
import type { CompilePhase } from "../bindings";

/** 工具栏「编译」按钮的进行态文案：queued → 排队中…；running → 排版中…；其余 → 编译。 */
export function compileButtonText(phase: CompilePhase): string {
  if (phase === "queued") return "排队中…";
  if (phase === "running") return "排版中…";
  return "编译";
}
