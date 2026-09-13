// 事件订阅（modules.md §9.1）：订阅一次，分发到各 store；单向：事件 → store 动作。
// 返回取消函数（组件卸载时调用）。

import { events } from "../bindings";
import { useCompileStore } from "../stores/compile";
import { useEditorStore } from "../stores/editor";
import { useOutlineStore } from "../stores/outline";
import { usePreviewStore } from "../stores/preview";
import { useProjectStore } from "../stores/project";
import { useSettingsStore } from "../stores/settings";

export function subscribeEvents(): () => void {
  const unlisteners: Promise<() => void>[] = [];

  unlisteners.push(
    events.compileStatus.listen((e) => {
      const dto = e.payload;
      useCompileStore().setStatus(dto.phase, dto.kind, dto.draft);
      // 编译成功 = 文档结构已确立 → 重建大纲（source 结构变化后保持同步）
      if (dto.phase === "success") useOutlineStore().refresh().catch(() => {});
    }),
    // 编译进行中的流式反馈（阶段 2）：已排版页数 + 编译中的致命错误。
    // 两者都是**非权威中间态**，只走各自的新事件；终态由 compile-status / errors-updated 给。
    events.compileProgress.listen((e) => {
      useCompileStore().setProgress(e.payload.pages);
    }),
    events.compileErrors.listen((e) => {
      useCompileStore().setLiveErrors(e.payload);
    }),
    events.errorsUpdated.listen((e) => {
      useCompileStore().setErrors(e.payload);
    }),
    events.pdfUpdated.listen((e) => {
      usePreviewStore().onPdfUpdated(e.payload.path);
    }),
    events.filesChanged.listen((e) => {
      const p = e.payload;
      useEditorStore().onFilesChanged(p.paths);
      useProjectStore().refreshTreeDebounced(p.structural);
      // 结构变化（增/删/重命名）→ 文件集合变了，重建大纲
      if (p.structural) useOutlineStore().refresh().catch(() => {});
    }),
    events.settingsChanged.listen((e) => {
      useSettingsStore().setSettings(e.payload);
      // 外部直接改 settings.json 也会走到这里（watch 热更新）：root_file 可能变了，
      // 前端项目状态要跟上，否则状态栏还显示「未确定根文件」而编译其实已经能跑（roadmap ㉑）。
      const project = useProjectStore();
      const next = e.payload.root_file ?? null;
      if (project.project && (project.project.root_file ?? null) !== next) {
        project.syncProject().catch(() => {});
      }
    }),
  );

  return () => {
    for (const u of unlisteners) {
      u.then((fn) => fn()).catch(() => {});
    }
  };
}
