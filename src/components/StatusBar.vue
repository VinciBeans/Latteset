<!-- StatusBar（modules.md §9.4）：编译状态 / 引擎 / 提示 chip / 行列号 / 编译模式 / 冲突提示。
     另含「未确定根文件」入口（roadmap P0-②-1）：弹窗被关掉后仍可从这里重新打开选择器。 -->
<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { useCompileStore } from "../stores/compile";
import { useEditorStore } from "../stores/editor";
import { useNoticesStore } from "../stores/notices";
import { useSettingsStore } from "../stores/settings";
import { useProjectStore } from "../stores/project";
import { usePreviewStore } from "../stores/preview";
import ConfirmDialog from "./ConfirmDialog.vue";
import type { CompileMode, CompilePhase } from "../bindings";

defineProps<{ cursorLine: number; cursorCol: number }>();
const emit = defineEmits<{ (e: "pick-root"): void; (e: "expand-problems"): void }>();

const compile = useCompileStore();
const editor = useEditorStore();
const settings = useSettingsStore();
const notices = useNoticesStore();
const project = useProjectStore();
const preview = usePreviewStore();

/** 唯一引擎（ADR-0015）：不读设置、不拉清单——没有"当前是哪个引擎"这回事。 */
const ENGINE_NAME = "Tectonic";

/** 项目已打开但未确定根文件 → 显示可点击提示（否则不会触发任何编译）。 */
const needsRootFile = computed(() => !!project.project && !project.project.root_file);
/** 没打开项目时 phase 没有意义（后端相位没有 idle）⇒ 不渲染 phase chip。 */
const projectOpen = computed(() => !!project.project);

const PHASE_TEXT: Record<CompilePhase, string> = {
  queued: "排队中…",
  running: "排版中…",
  success: "就绪",
  failed: "失败",
};

/**
 * 未编译态（A-01）：`phase` 初值是 `success`，未编译时不能显示"就绪"。
 * 编译进行中（queued/running）优先显示进行态；其余按 `hasCompiled` 区分「未编译」与终态。
 */
const phaseText = computed(() => {
  if (compile.phase === "queued" || compile.phase === "running") return PHASE_TEXT[compile.phase];
  return compile.hasCompiled ? PHASE_TEXT[compile.phase] : "未编译";
});
const phaseClass = computed(() =>
  compile.hasCompiled || compile.phase === "queued" || compile.phase === "running"
    ? compile.phase
    : "idle",
);

const kindText = computed(() => {
  switch (compile.kind) {
    case "timeout": return "· 超时";
    case "content_error": return "· 内容错误";
    case "aborted": return "· 已终止";
    default: return "";
  }
});

const isContinuous = computed(() => settings.settings?.compile.mode === "continuous");

/**
 * 设置里的一次性提示（后端 `Settings.notices`，如"引擎已统一为 Tectonic，原引擎设置不再生效"）。
 *
 * 与下面的运行时提示**分开**：这些是落盘的，点击 = `dismiss_notice` 写盘，点掉就再也不回来。
 */
const persistentNotices = computed(() => settings.settings?.notices ?? []);

/**
 * 编译期的**运行时**提示（`compile-notice` 事件 → noticesStore）：非持久、不落盘。
 * 点击**不再清空**（C-02）：长提示会被 chip 截断，点开看全文并展开「问题」面板；清空交给编译期结束或后续交互。
 */
const runtimeNotices = computed(() => notices.runtime);

async function dismissNotice(id: string) {
  try {
    await settings.update({ dismiss_notice: id });
  } catch (e) {
    console.error("关闭提示失败：", e);
  }
}

// ---- A-12 编译模式：可见说明的菜单（title 只作补充）----
const MODE_OPTIONS: { value: CompileMode; label: string; hint: string }[] = [
  { value: "continuous", label: "连续编译", hint: "编辑停顿后自动编译（防抖时长在设置里）" },
  { value: "on_save", label: "保存时编译", hint: "点「编译」或保存文件时编译" },
];
const currentMode = computed<CompileMode>(() => settings.settings?.compile.mode ?? "continuous");
const modeLabel = computed(
  () => MODE_OPTIONS.find((m) => m.value === currentMode.value)?.label ?? "连续编译",
);
const modeMenuOpen = ref(false);
const modeWrap = ref<HTMLElement | null>(null);
const modeButton = ref<HTMLButtonElement | null>(null);
const modeMenu = ref<HTMLElement | null>(null);

/** 打开菜单并把焦点交给第一项（A-12：键盘可达）。 */
async function openModeMenu() {
  modeMenuOpen.value = true;
  await nextTick();
  modeMenu.value?.querySelector<HTMLButtonElement>(".mode-item")?.focus();
}

function closeModeMenu(refocus = false) {
  modeMenuOpen.value = false;
  if (refocus) modeButton.value?.focus();
}

/** 菜单内键盘导航：上下/Home/End 移动焦点，Esc 关闭并把焦点还给按钮。 */
function onModeMenuKeydown(e: KeyboardEvent) {
  const items = Array.from(
    modeMenu.value?.querySelectorAll<HTMLButtonElement>(".mode-item") ?? [],
  );
  if (items.length === 0) return;
  const idx = items.indexOf(document.activeElement as HTMLButtonElement);
  if (e.key === "Escape") {
    e.preventDefault();
    closeModeMenu(true);
  } else if (e.key === "ArrowDown") {
    e.preventDefault();
    items[(Math.max(idx, 0) + 1) % items.length].focus();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    items[(idx - 1 + items.length) % items.length].focus();
  } else if (e.key === "Home") {
    e.preventDefault();
    items[0].focus();
  } else if (e.key === "End") {
    e.preventDefault();
    items[items.length - 1].focus();
  }
}

/** 点击菜单外部即关闭（不吞掉点击本身，交给原目标处理）。 */
function onDocPointerDown(e: Event) {
  if (!modeMenuOpen.value) return;
  const target = e.target as Node | null;
  if (modeWrap.value && target && modeWrap.value.contains(target)) return;
  closeModeMenu();
}

onMounted(() => document.addEventListener("pointerdown", onDocPointerDown));
onBeforeUnmount(() => document.removeEventListener("pointerdown", onDocPointerDown));

async function chooseMode(mode: CompileMode) {
  closeModeMenu(true);
  if (currentMode.value === mode) return;
  await settings.update({ mode });
}

// ---- A-11 外部修改：采用磁盘版本前二次确认（会覆盖未保存的本地修改）----
const conflictPath = ref<string | null>(null);
const conflictBusy = ref(false);
const conflictName = computed(() => conflictPath.value?.split("/").pop() ?? "");

function askAcceptExternal(path: string) {
  conflictPath.value = path;
}

async function confirmAcceptExternal() {
  const path = conflictPath.value;
  if (!path || conflictBusy.value) return;
  conflictBusy.value = true;
  try {
    await editor.acceptExternal(path);
    conflictPath.value = null;
  } catch (e) {
    console.error("采用磁盘版本失败：", e);
  } finally {
    conflictBusy.value = false;
  }
}

// ---- C-02 运行时提示：点击展开全文 + 展开「问题」面板，不销毁 ----
const expandedNotice = ref<number | null>(null);
function toggleRuntimeNotice(i: number) {
  expandedNotice.value = expandedNotice.value === i ? null : i;
  if (expandedNotice.value !== null) emit("expand-problems");
}
</script>

<template>
  <div class="status-bar">
    <span v-if="projectOpen" class="phase" :class="phaseClass">
      <span class="phase-dot" />
      {{ phaseText }}{{ kindText }}
    </span>
    <!-- D3（2026-09-14）：当前引擎常驻可见。唯一引擎（ADR-0015）后名字是常量，但"用的是哪个引擎"仍要讲清楚 -->
    <span class="engine" :title="`当前引擎：${ENGINE_NAME}${compile.draft ? '（当前 PDF 为草稿编译）' : ''}`">
      {{ ENGINE_NAME }}
    </span>
    <!-- 流式进度（A-04）：编译中预览已显示「N 页」时这里不再重复 -->
    <span v-if="compile.phase === 'running' && compile.pages > 0 && !preview.livePreview" class="pages">
      已排版 {{ compile.pages }} 页
    </span>
    <span
      v-if="compile.draft"
      class="draft"
      title="草稿编译：引用可能落后，停手后自动补全"
    >
      引用待更新
    </span>
    <span
      v-if="needsRootFile"
      class="needs-root"
      title="尚未指定根文件，点击选择要编译的 .tex 文件"
      @click="$emit('pick-root')"
    >
      尚未指定根文件 · 点击选择
    </span>
    <span
      v-for="p in editor.externalConflict"
      :key="p"
      class="conflict"
      title="文件已被外部修改，点击查看并选择是否采用磁盘版本"
      @click="askAcceptExternal(p)"
    >
      外部修改：{{ p.split("/").pop() }}
    </span>
    <!-- 打开文件失败（roadmap ㉓：非 UTF-8 源文件等）——此前只有 console.error，点了没反应 -->
    <span
      v-if="editor.openError"
      class="open-error"
      :title="editor.openError"
      @click="editor.openError = null"
    >
      {{ editor.openError }}
    </span>
    <!-- 设置里的一次性提示（后端 Settings.notices）：点击 = dismiss_notice 写盘，点掉后不再出现。
         与错误列表分开：它不是错误（例如"引擎已统一为 Tectonic"），不该进 ErrorList。 -->
    <span
      v-for="n in persistentNotices"
      :key="n.id"
      class="notice"
      :class="n.level === 'warning' ? 'warn' : 'info'"
      :title="`${n.message}（点击关闭）`"
      @click="dismissNotice(n.id)"
    >
      {{ n.message }}
    </span>
    <!-- 编译期运行时提示（compile-notice 事件）：非持久、不落盘；点击展开全文，不销毁（C-02） -->
    <span
      v-for="(text, i) in runtimeNotices"
      :key="`${i}-${text}`"
      class="notice info runtime"
      :class="{ expanded: expandedNotice === i }"
      :aria-expanded="expandedNotice === i"
      :title="`${text}（点击展开全文）`"
      @click="toggleRuntimeNotice(i)"
    >
      {{ text }}
    </span>
    <span class="spacer" />
    <span class="cursor">
      <span class="cursor-file">{{ editor.activeTab?.name ?? "" }}</span>
      <span class="cursor-pos">Ln {{ cursorLine }}, Col {{ cursorCol }}</span>
    </span>
    <!-- A-12：编译模式入口 = 带可见说明的菜单（title 只作补充） -->
    <div ref="modeWrap" class="mode-wrap">
      <button
        ref="modeButton"
        class="mode"
        :class="{ on: isContinuous }"
        aria-haspopup="menu"
        :aria-expanded="modeMenuOpen"
        :title="`当前：${modeLabel}（点击查看其它编译模式）`"
        @click="modeMenuOpen ? closeModeMenu() : openModeMenu()"
      >
        {{ modeLabel }}
      </button>
      <div
        v-if="modeMenuOpen"
        ref="modeMenu"
        class="mode-menu"
        role="menu"
        aria-label="编译模式"
        @keydown="onModeMenuKeydown"
      >
        <button
          v-for="m in MODE_OPTIONS"
          :key="m.value"
          class="mode-item"
          role="menuitemradio"
          :aria-checked="currentMode === m.value"
          :class="{ on: currentMode === m.value }"
          @click="chooseMode(m.value)"
        >
          <span class="mode-item-label">{{ m.label }}</span>
          <span class="mode-item-hint">{{ m.hint }}</span>
        </button>
      </div>
    </div>
    <!-- C-02：点开的运行时提示全文（chip 本身被截断，这里完整显示） -->
    <div
      v-if="expandedNotice !== null && runtimeNotices[expandedNotice]"
      class="notice-pop"
      data-testid="notice-popover"
      @click="expandedNotice = null"
    >
      {{ runtimeNotices[expandedNotice] }}
      <span class="notice-pop-hint">（点击关掉）</span>
    </div>
    <!-- A-11：采用磁盘版本 = 放弃本地修改，必须二次确认 -->
    <ConfirmDialog
      v-if="conflictPath"
      title="采用磁盘上的版本？"
      :lines="[
        `磁盘上的 ${conflictName} 比编辑器里的新。`,
        '采用磁盘版本会覆盖你未保存的修改，且无法撤销。',
      ]"
      confirm-label="采用磁盘版本"
      :busy="conflictBusy"
      @confirm="confirmAcceptExternal"
      @cancel="conflictPath = null"
    />
  </div>
</template>

<style scoped>
.status-bar {
  position: relative;
  display: flex; align-items: center; gap: 12px;
  height: 27px; padding: 0 14px;
  background: var(--card);
  border-top: 1.5px solid var(--line);
  color: var(--ink-dim);
  font-size: 11.5px;
  flex: 0 0 auto;
}
.phase {
  display: inline-flex; align-items: center; gap: 7px;
  padding: 0 11px; height: 19px;
  border-radius: 10px;
  background: var(--tint-blueberry);
  color: var(--blueberry);
  font-weight: 600;
}
.phase-dot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; }
.phase.success { background: rgba(var(--mint-rgb), 0.12); color: var(--ok-ink); }
/* 未编译（A-01）：中性灰，不能给"已通过"的绿色暗示 */
.phase.idle { background: var(--card-2); color: var(--ink-faint); }
.phase.failed { background: var(--tint-coral-weak); color: var(--danger-ink); }
.phase.queued { background: var(--tint-mango-weak); color: var(--warn-ink-alt); }
.phase.running .phase-dot { animation: bounce 0.9s ease-in-out infinite; }
@keyframes bounce { 50% { transform: translateY(-2px); } }
/* 流式进度（阶段 2）：编译中已排版页数（非权威中间态，只在排版中显示） */
.pages {
  background: var(--tint-blueberry-weak);
  color: var(--ink-dim);
  padding: 2px 9px;
  border-radius: 5px;
  font-variant-numeric: tabular-nums;
}
.conflict {
  cursor: pointer;
  background: var(--tint-coral);
  color: var(--danger-ink);
  padding: 2px 9px;
  border-radius: 5px;
}
/* 草稿编译提示（roadmap ㉘）：编辑期单趟出图，目录/引用页码可能落后一趟 */
.engine {
  padding: 1px 7px;
  border-radius: 999px;
  background: var(--tint-blueberry);
  color: var(--blueberry);
  font-size: 11.5px;
  font-weight: 600;
}
.draft {  background: var(--tint-mango);
  color: var(--warn-ink);
  padding: 2px 9px;
  border-radius: 5px;
  font-weight: 600;
}
/* 未确定根文件：可点击入口（点击打开根文件选择器） */
.needs-root {
  cursor: pointer;
  background: var(--tint-mango);
  color: var(--warn-ink);
  padding: 2px 9px;
  border-radius: 5px;
  font-weight: 600;
}
.needs-root:hover { background: rgba(var(--mango-rgb), 0.3); }
/* 打开文件失败：一条可读的错误（点一下关掉） */
.open-error {
  cursor: pointer;
  max-width: 60%;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  background: var(--tint-coral);
  color: var(--danger-ink);
  padding: 2px 9px;
  border-radius: 5px;
  font-weight: 600;
}
/* 提示 chip（形状照 .open-error）：info 中性、warning 琥珀。
   设置里的一次性提示点击 = dismiss_notice 写盘；运行时提示点击 = 清空（虚线框提示"不落盘"）。 */
.notice {
  cursor: pointer;
  max-width: 30%;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  padding: 2px 9px;
  border-radius: 5px;
  font-weight: 600;
}
.notice.info { background: var(--card-2, rgba(0, 0, 0, 0.05)); color: var(--ink-dim); }
.notice.warn { background: var(--tint-mango); color: var(--warn-ink); }
.notice.runtime { border: 1px dashed var(--line); }
.notice:hover { filter: brightness(0.97); }
/* 运行时提示被截断：点开看全文（C-02）；chip 本身保持单行 */
.notice-pop {
  position: absolute;
  right: 120px;
  bottom: 31px;
  z-index: 80;
  max-width: 60%;
  padding: 8px 11px;
  background: var(--card);
  border: 1.5px solid var(--line);
  border-radius: var(--radius-sm);
  box-shadow: var(--shadow-hard-big);
  color: var(--ink);
  font-weight: 550;
  line-height: 1.5;
  white-space: normal;
  cursor: pointer;
}
.notice-pop-hint { color: var(--ink-faint); font-weight: 400; }
.spacer { flex: 1; }
.cursor { display: inline-flex; align-items: center; gap: 9px; }
.cursor-file { color: var(--ink); font-family: var(--mono); max-width: 320px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.cursor-pos { font-family: var(--mono); color: var(--ink-faint); }
.mode {
  display: inline-flex; align-items: center;
  padding: 2px 11px;
  background: transparent;
  border: 1.5px solid var(--line);
  border-radius: 6px;
  color: var(--ink-dim);
  font-size: 11px; font-weight: 550;
  cursor: pointer;
  transition: all 0.15s;
}
.mode:hover { border-color: var(--blueberry); color: var(--ink); }
.mode.on { border-color: var(--blueberry); color: var(--blueberry); background: var(--tint-blueberry-weak); }
/* 编译模式菜单（A-12）：两个选项各自的说明可见，title 只作补充 */
.mode-wrap { position: relative; flex: 0 0 auto; }
.mode-menu {
  position: absolute;
  right: 0;
  bottom: 31px;
  z-index: 90;
  width: 268px;
  padding: 5px;
  background: var(--card);
  border: 1.5px solid var(--line);
  border-radius: var(--radius-sm);
  box-shadow: var(--shadow-hard-big);
}
.mode-item {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  width: 100%;
  padding: 6px 9px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--ink-dim);
  text-align: left;
  cursor: pointer;
  font-family: inherit;
}
.mode-item:hover { background: var(--card-2); }
.mode-item.on { background: var(--tint-blueberry-weak); color: var(--blueberry); }
.mode-item-label { font-size: 12px; font-weight: 650; }
.mode-item-hint { font-size: 11px; color: var(--ink-faint); }
.mode-item.on .mode-item-hint { color: var(--ink-dim); }

@media (prefers-reduced-motion: reduce) {
  .phase.running .phase-dot { animation: none; }
}
</style>
