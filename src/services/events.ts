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
    }),
  );

  return () => {
    for (const u of unlisteners) {
      u.then((fn) => fn()).catch(() => {});
    }
  };
}
