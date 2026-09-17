<!-- EditorPane（modules.md §9.4）：Monaco 封装。
   模块内信息：Monaco 实例、model 表；对外只发内容变更与定位事件。 -->
<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import * as monaco from "monaco-editor";
import { useEditorStore } from "../stores/editor";
import { useCompileStore } from "../stores/compile";
import { useSyncTex } from "../composables/useSyncTex";
import { ipc } from "../services/ipc";
import MathPreviewCard from "./MathPreviewCard.vue";

const emit = defineEmits<{
  change: [path: string];
  cursor: [line: number, column: number];
}>();

const editor = useEditorStore();
const { forward } = useSyncTex();

const host = ref<HTMLElement | null>(null);
let monacoEditor: monaco.editor.IStandaloneCodeEditor | null = null;
/** Monaco 事件订阅，逐条显式释放（不依赖 editor.dispose 级联，防残留监听）。 */
const monacoSubscriptions: monaco.IDisposable[] = [];

// ---- 公式预览（roadmap ㊸ 切片 3）：悬停一个公式 → 单独编译它 → 浮层里给真实排版结果 ----
// 触发是**我们自己**的 `onMouseMove` + 去抖（不走 Monaco 的 hover provider：浮层要放一张画布，
// Monaco 的 hover 内容只能放 markdown）。
const compileStore = useCompileStore();
const MATH_HOVER_DELAY_MS = 160;
const mathCard = ref({ visible: false, x: 0, y: 0, pdfPath: "", message: "", formula: "", mode: "auto" });
let mathTimer: number | undefined;
/** 序号：迟到的响应必须丢掉（鼠标扫过一行会连续触发多次） */
let mathSeq = 0;

function hideMathCard() {
  mathCard.value.visible = false;
}

async function requestMathPreview(text: string, utf16Offset: number, x: number, y: number) {
  const seq = ++mathSeq;
  try {
    const p = await ipc.compileMath(text, utf16Offset);
    if (seq !== mathSeq) return; // 过期结果
    // 真机验收的可观测钩子（与 window.__previewLastReload 同款约定：只读、不驱动 UI）
    (window as unknown as Record<string, unknown>).__mathPreview = p;
    if (!p.hit) {
      hideMathCard();
      return;
    }
    mathCard.value = {
      visible: true,
      x,
      y,
      pdfPath: p.pdfPath ?? "",
      message: p.message ?? "",
      formula: p.formula ?? "",
      mode: p.mode,
    };
  } catch (err) {
    if (seq !== mathSeq) return;
    (window as unknown as Record<string, unknown>).__mathPreview = { hit: true, ok: false, message: String(err) };
    hideMathCard();
  }
}

function onEditorMouseMove(e: monaco.editor.IEditorMouseEvent) {
  // 主编译（queued/running）期间**不发起**悬停编译：库形态的引擎是**进程内全局锁**，此刻发起
  // 只会排队等锁（图出来时鼠标早走了），还会白占一次 30 s 上限的任务（roadmap §6.11.4 决定）。
  if (compileStore.phase === "running" || compileStore.phase === "queued") {
    // 诊断计数：真机验收要能把"守卫生效"与"这次悬停位置上本来就没有公式"区分开
    // （与 window.__mathPreview 同款只读可观测约定）。
    const w = window as unknown as Record<string, number>;
    w.__mathHoverSkipped = (w.__mathHoverSkipped ?? 0) + 1;
    window.clearTimeout(mathTimer);
    hideMathCard();
    return;
  }
  const position = e.target.position;
  const model = monacoEditor?.getModel();
  if (!position || !model) {
    window.clearTimeout(mathTimer);
    hideMathCard();
    return;
  }
  const offset = model.getOffsetAt(position);
  const { posx: x, posy: y } = e.event;
  window.clearTimeout(mathTimer);
  mathTimer = window.setTimeout(() => void requestMathPreview(model.getValue(), offset, x, y), MATH_HOVER_DELAY_MS);
}

/**
 * model uri.toString() → 存储路径（反查表）。
 * Monaco 的 `uri.path` 对 `file:///E:/...` 会带前导 `/`；对 Windows 反斜杠路径
 * 会把整段解析进 authority（uri.path 变空）。因此所有“model → 存储路径”的映射
 * 一律经此表，绝不直接读 uri.path 作为存储键。
 */
const modelPaths = new Map<string, string>();

/**
 * 正在把外部重载写进 model。
 *
 * `setValue` 会同步触发 `onDidChangeModelContent`（同一调用栈内），而事件本身分不出是用户敲的
 * 还是重载写的。不区分就要付代价（真机实测 2/2）：外部改文件后 tab 被误判为脏、自动保存把同一份
 * 内容又写回磁盘，并让随后的第二条 `files-changed` 误报「外部修改」（那个提示的动作是放弃本地）。
 *
 * 用调用栈内的开关而非比对内容：开关只可能在 `setValue` 期间为真 ⇒ 结构上不可能吞掉用户输入。
 */
let applyingReload = false;

function uriOf(path: string) {
  // 统一正斜杠：file:///E:/...（Windows 盘符）或 file:///home/...（UNIX）
  const p = path.replace(/\\/g, "/");
  return monaco.Uri.parse("file://" + (p.startsWith("/") ? p : "/" + p));
}

function storePathOf(model: monaco.editor.ITextModel | null): string | undefined {
  return model ? modelPaths.get(model.uri.toString()) : undefined;
}

function reportCursor() {
  const pos = monacoEditor?.getPosition();
  if (pos) emit("cursor", pos.lineNumber, pos.column);
}

onMounted(() => {
  monacoEditor = monaco.editor.create(host.value!, {
    automaticLayout: true,
    fontSize: 14,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    language: "latex",
    readOnly: true, // 未打开项目/文件时源码区不可编辑（防"空文件"可写误输入）
    ariaLabel: "源码编辑器",
    // ---- v1.1 编辑器增强（design.md §编辑器）：折叠 / 多光标 / 代码片段 ----
    folding: true,
    foldingHighlight: true,
    showFoldingControls: "mouseover",
    tabCompletion: "on",            // Tab 展开代码片段（snippetSuggestions 需开启）
    snippetSuggestions: "inline",   // 片段内联在建议列表（可 Ctrl+Space 触发）
    quickSuggestions: { other: true, comments: false, strings: true },
    suggest: { showSnippets: true },
    // 多光标：Alt+点击/Alt+方向（默认 altKey）。避免设为 ctrlCmd——Ctrl+点击被用于
    // SyncTeX 正向（onMouseDown ctrlKey），两者会冲突。Ctrl+D 加选下一处、Ctrl+Shift+L 全选同词（默认）。
    multiCursorModifier: "alt",     // Alt+点击/Alt+方向；避免设为 ctrlCmd 与 SyncTeX Ctrl+点击冲突
    multiCursorPaste: "spread",     // 粘贴时广播到所有光标
  });

  // 内容变更 → 脏标记 + 缓冲（自动保存的数据源）
  monacoSubscriptions.push(
    monacoEditor.onDidChangeModelContent(() => {
      const model = monacoEditor!.getModel();
      const path = storePathOf(model);
      if (!path || !model) return;
      if (applyingReload) return; // 本次是外部重载，不是用户编辑
      editor.markDirty(path, model.getValue());
      emit("change", path);
    }),
  );

  monacoSubscriptions.push(monacoEditor.onDidChangeCursorPosition(reportCursor));
  monacoSubscriptions.push(monacoEditor.onMouseMove(onEditorMouseMove));

  // Ctrl+点击 → SyncTeX 正向（modules.md §5.3）
  monacoSubscriptions.push(
    monacoEditor.onMouseDown((e) => {
      if (e.event.ctrlKey && e.target.position) {
        const pos = e.target.position;
        const model = monacoEditor!.getModel();
        const path = storePathOf(model);
        if (model && path) forward(path, pos.lineNumber, pos.column);
      }
    }),
  );
});

// 打开文件切换：get-or-create model
watch(
  () => editor.activePath,
  (path) => {
    if (!path || !monacoEditor) return;
    const uri = uriOf(path);
    let model = monaco.editor.getModel(uri);
    if (!model) {
      model = monaco.editor.createModel(editor.buffers.get(path) ?? "", "latex", uri);
    }
    modelPaths.set(model.uri.toString(), path);
    monacoEditor.setModel(model);
    monacoEditor.updateOptions({ readOnly: false }); // 有活动文件即可编辑
    reportCursor();
  }
);

// 最后一个标签被关闭（activePath 变 null）→ 清空编辑器：否则旧 model 残留，
// 仍可编辑/自动保存/触发编译（BUG 修复）。
watch(
  () => editor.activePath,
  (path) => {
    if (path || !monacoEditor) return;
    monacoEditor.setModel(null);
    monacoEditor.updateOptions({ readOnly: true }); // 无活动文件 → 源码区不可编辑
  }
);

// 关闭标签 → 释放对应 model（防残留编辑；重开时按 buffer/磁盘内容重建，不读旧 model）
watch(
  () => editor.tabs.map((t) => t.path),
  (paths) => {
    for (const m of monaco.editor.getModels()) {
      const storePath = modelPaths.get(m.uri.toString());
      if (storePath !== undefined && !paths.includes(storePath)) {
        modelPaths.delete(m.uri.toString());
        m.dispose();
      }
    }
  }
);

// 外部重载（architecture.md §5.5）：buffer 变化 → model 同步；值相同跳过（防循环）
watch(
  () => editor.buffers.get(editor.activePath ?? ""),
  (content) => {
    if (content === undefined || !monacoEditor) return;
    const model = monacoEditor.getModel();
    if (model && model.getValue() !== content) {
      applyingReload = true;
      try {
        model.setValue(content);
      } finally {
        applyingReload = false;
      }
    }
  }
);

// 定位请求（错误跳转 / SyncTeX 反向）
watch(
  () => editor.pendingReveal,
  (r) => {
    if (!r || !monacoEditor) return;
    const model = monaco.editor.getModel(uriOf(r.path));
    if (model) {
      monacoEditor.setModel(model);
      monacoEditor.updateOptions({ readOnly: false }); // 揭示已打开文件应可编辑
      monacoEditor.revealLineInCenter(r.line);
      monacoEditor.setPosition({ lineNumber: r.line, column: 1 });
      monacoEditor.focus();
    }
    editor.consumeReveal();
  }
);

onBeforeUnmount(() => {
  window.clearTimeout(mathTimer);
  for (const sub of monacoSubscriptions) sub.dispose();
  monacoSubscriptions.length = 0;
  monacoEditor?.dispose();
  monacoEditor = null;
});
</script>

<template>
  <div class="editor-pane">
    <!-- Monaco 宿主始终挂载（编辑器初始化一次）；无活动文件时用占位提示覆盖 -->
    <div ref="host" class="editor-body" />
    <div v-if="!editor.activePath" class="editor-empty">
      <span class="empty-icon">📄</span>
      <span class="empty-title">还没有打开文件</span>
      <span class="empty-hint">在左侧文件树点开一个 .tex 文件开始编辑</span>
    </div>
    <MathPreviewCard
      v-if="mathCard.visible"
      :x="mathCard.x"
      :y="mathCard.y"
      :pdf-path="mathCard.pdfPath"
      :message="mathCard.message"
      :formula="mathCard.formula"
      :mode="mathCard.mode"
      @close="hideMathCard"
    />
  </div>
</template>

<style scoped>
.editor-pane { position: relative; width: 100%; height: 100%; }
.editor-body { width: 100%; height: 100%; }

/* 无活动文件时的占位提示（与文件树/PDF 空态一致的视觉语言） */
.editor-empty {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  background: var(--card);
  color: var(--ink-faint);
  pointer-events: none; /* 不拦截 Monaco 的后续交互（有文件后该节点被移除） */
}
.editor-empty .empty-icon { font-size: 38px; display: block; }
.editor-empty .empty-title { display: block; margin-top: 12px; font-size: 14px; font-weight: 700; color: var(--ink-dim); }
.editor-empty .empty-hint { display: block; margin-top: 6px; font-size: 12px; }
</style>
