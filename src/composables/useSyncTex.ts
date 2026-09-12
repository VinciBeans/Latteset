// useSyncTex（modules.md §9.3）：双向定位编排（modules.md §5.3）。
//
// roadmap ⑤ 加固点：
// - 反向定位的返回值是 `{ source, note }`（后端 `synctex_inverse`）：命中生成产物（`tmp/main.toc`）
//   或项目外文件时 `source` 为空、`note` 说明原因——**不再打开生成文件，也不静默失败**；
//   能回落到附近真实源码时 `source` 有值且 `note` 提示"已回落"。
// - 正向定位失败同样给可见提示（此前只有 console.error，用户点了没反应）。
import { ipc } from "../services/ipc";
import { useEditorStore } from "../stores/editor";
import { usePreviewStore } from "../stores/preview";

export function useSyncTex() {
  const preview = usePreviewStore();
  const editor = useEditorStore();

  /** 正向：源码 Ctrl+点击 → PDF 高亮。 */
  async function forward(file: string, line: number, column: number) {
    try {
      const target = await ipc.synctexForward(file, line, column);
      if (target.x !== null && target.y !== null) {
        preview.setHighlight({ page: target.page, x: target.x, y: target.y });
      }
    } catch (e) {
      // 常见原因：还没编译过（无 .synctex.gz）或正在编译（数据被重写，已由后端重试兜过）
      console.error("SyncTeX 正向定位失败：", e);
      preview.setSyncNote("正向定位失败：先编译一次再试（同步数据来自上一次编译）");
    }
  }

  /** 反向：PDF 点击 → 源码跳转；无可跳转源码时给出提示。 */
  async function inverse(page: number, x: number, y: number) {
    try {
      const result = await ipc.synctexInverse(page, x, y);
      if (result.source) {
        await editor.openFile(result.source.file, result.source.line);
        // 回落提示（"此处是自动生成的内容，已回落到最近的源码"）保留给用户看
        preview.setSyncNote(result.note ?? null);
      } else {
        preview.setSyncNote(result.note ?? "此处没有对应的源码位置");
      }
    } catch (e) {
      console.error("SyncTeX 反向定位失败：", e);
      preview.setSyncNote("同步失败：先编译一次再试");
    }
  }

  return { forward, inverse };
}
