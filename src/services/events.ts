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
      // 编译终态 ⇒ 退出"编译中预览"（roadmap ㉞）。成功时 `pdf-updated` 通常已经先退出过一次
      // （那时这里是空操作）；**失败/中止**时不会有 `pdf-updated`，必须在这里收口，否则屏幕上
      // 会留着中间态的部分 PDF 冒充结果。
      if (dto.phase !== "running") usePreviewStore().clearLivePreview();
      // 编译成功 = 文档结构已确立 → 重建大纲（source 结构变化后保持同步）
      if (dto.phase === "success") useOutlineStore().refresh().catch(() => {});
    }),
    // 编译进行中的流式反馈（阶段 2）：已排版页数 + 编译中的致命错误。
    // 两者都是**非权威中间态**，只走各自的新事件；终态由 compile-status / errors-updated 给。
    events.compileProgress.listen((e) => {
      useCompileStore().setProgress(e.payload.pages);
    }),
    // 编译中的**部分 PDF**（roadmap ㉞，只库形态会发）：让长编译在跑的时候就能看上几页。
    // 与上面两个中间态同一守卫口径：只在"排版中"采纳（终态之后到达的中间态一律丢弃）。
    events.compilePreview.listen((e) => {
      if (useCompileStore().phase !== "running") return;
      usePreviewStore().onCompilePreview(e.payload);
    }),
    events.compileErrors.listen((e) => {
      useCompileStore().setLiveErrors(e.payload);
    }),
    events.errorsUpdated.listen((e) => {
      useCompileStore().setErrors(e.payload);
    }),
    events.pdfUpdated.listen((e) => {
      // 载荷含变化页集合（roadmap「增量编辑 × DVI」B/C）：store 决定是否真的重载。
      usePreviewStore().onPdfUpdated(e.payload);
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
