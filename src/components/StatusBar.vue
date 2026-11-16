<!-- StatusBar（modules.md §9.4）：编译状态 / 引擎与形态 / 提示 chip / 行列号 / 编译模式 / 冲突提示。
     另含「未确定根文件」入口（roadmap P0-②-1）：弹窗被关掉后仍可从这里重新打开选择器。 -->
<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useCompileStore } from "../stores/compile";
import { useEditorStore } from "../stores/editor";
import { useNoticesStore } from "../stores/notices";
import { useSettingsStore } from "../stores/settings";
import { useProjectStore } from "../stores/project";
import { ipc } from "../services/ipc";
import type { CompilePhase, EngineFormDto } from "../bindings";

defineProps<{ cursorLine: number; cursorCol: number }>();
defineEmits<{ (e: "pick-root"): void }>();

const compile = useCompileStore();
const editor = useEditorStore();
const settings = useSettingsStore();
const notices = useNoticesStore();
const project = useProjectStore();

/** 唯一引擎（ADR-0015）：不读设置、不拉清单——没有"当前是哪个引擎"这回事。 */
const ENGINE_NAME = "Tectonic";

/** 项目已打开但未确定根文件 → 显示可点击提示（否则不会触发任何编译）。 */
const needsRootFile = computed(() => !!project.project && !project.project.root_file);

const phaseText = computed(() => {
  const map: Record<CompilePhase, string> = {
    queued: "排队中…",
    running: "排版中…",
    success: "就绪",
    failed: "失败",
  };
  return map[compile.phase] ?? "";
});

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
 * 页面切换与编译成功都不清（它解释的往往正是"这一趟为什么这样"），只有用户点击才清空。
 */
const runtimeNotices = computed(() => notices.runtime);

async function dismissNotice(id: string) {
  try {
    await settings.update({ dismiss_notice: id });
  } catch (e) {
    console.error("关闭提示失败：", e);
  }
}

/**
 * **形态位**（库形态方案 §6 P7 验收判据 ①）：子进程 / 库内嵌。
 *
 * 判定**只在后端做**（`engine_form` 命令直接复用 runner 的纯函数 `use_library_form`），这里只渲染。
 * 不能从设置自己推：`LATTESET_TECTONIC_LIB` 能压过设置，只看设置会报出与实际不符的形态。
 */
const form = ref<EngineFormDto | null>(null);
async function refreshForm() {
  try {
    form.value = await ipc.engineForm();
  } catch {
    form.value = null; // 拿不到就不显示，绝不猜
  }
}
onMounted(refreshForm);
// 形态随设置变（形态位会改判定）；settings-changed 落到 store 上之后再取一次。
watch(() => settings.settings, refreshForm);

/** **拿到 form 数据就显示**：唯一引擎下形态永远值得说一句（不再与"哪个引擎"绑定）。 */
const showForm = computed(() => form.value !== null);

const formText = computed(() => {
  switch (form.value?.form) {
    case "lib": return "库内嵌";
    case "lib_unavailable": return "库形态不可用";
    case "subprocess": return "子进程（tectonic.exe）";
    default: return "";
  }
});

const formTitle = computed(() => {
  const f = form.value;
  if (!f) return "";
  const env = f.env_forced ? "（由环境变量 LATTESET_TECTONIC_LIB 强制，压过设置）" : "";
  switch (f.form) {
    case "lib": return `Tectonic 库内嵌：引擎嵌在产品进程里${env}`;
    case "lib_unavailable":
      return `设置要库内嵌，但本次构建没编入 tectonic-lib ⇒ 下一趟编译会显式失败（D1：不静默回退到子进程）${env}`;
    default: return `子进程档：直调官方 tectonic.exe 编译${env}`;
  }
});

async function toggleMode() {
  await settings.update({ mode: isContinuous.value ? "on_save" : "continuous" });
}
</script>

<template>
  <div class="status-bar">
    <span class="phase" :class="compile.phase">
      <span class="phase-dot" />
      {{ phaseText }}{{ kindText }}
    </span>
    <!-- D3（2026-09-14）：当前引擎常驻可见。唯一引擎（ADR-0015）后名字是常量，但"用的是哪个引擎"仍要讲清楚 -->
    <span class="engine" :title="`当前引擎：${ENGINE_NAME}${compile.draft ? '（本次为 Quick 草稿编译）' : ''}`">
      {{ ENGINE_NAME }}
    </span>
    <!-- P7 验收判据 ①：形态位（子进程 / 库内嵌）。判定来自后端 engine_form，与编译实际行为同源 -->
    <span
      v-if="showForm"
      class="form"
      :class="{ warn: form?.form === 'lib_unavailable' }"
      :title="formTitle"
    >{{ formText }}</span>
    <!-- 流式进度（阶段 2）：只在"排版中"且已有页输出时显示，避免闪烁 -->
    <span v-if="compile.phase === 'running' && compile.pages > 0" class="pages">
      已排版 {{ compile.pages }} 页
    </span>
    <span
      v-if="compile.draft"
      class="draft"
      title="当前 PDF 由编辑态单趟编译产出：目录/交叉引用页码可能落后一趟，停手后会自动收敛"
    >
      引用待更新
    </span>
    <span
      v-if="needsRootFile"
      class="needs-root"
      title="尚未确定根文件，点击选择要编译的主文件"
      @click="$emit('pick-root')"
    >
      未确定根文件 · 点击选择
    </span>
    <span
      v-for="p in editor.externalConflict"
      :key="p"
      class="conflict"
      title="文件已被外部修改，点击采用磁盘版本"
      @click="editor.acceptExternal(p)"
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
    <!-- 编译期运行时提示（compile-notice 事件）：非持久、不落盘；点一下清空 -->
    <span
      v-for="(text, i) in runtimeNotices"
      :key="`${i}-${text}`"
      class="notice info runtime"
      :title="`${text}（点击清空运行时提示）`"
      @click="notices.clear()"
    >
      {{ text }}
    </span>
    <span class="spacer" />
    <span class="cursor">
      <span class="cursor-file">{{ editor.activeTab?.name ?? "" }}</span>
      <span class="cursor-pos">Ln {{ cursorLine }}, Col {{ cursorCol }}</span>
    </span>
    <button class="mode" :class="{ on: isContinuous }" @click="toggleMode">
      {{ isContinuous ? "连续编译" : "保存触发" }}
    </button>
  </div>
</template>

<style scoped>
.status-bar {
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
/* 形态位（P7 判据 ①）：与引擎名同处，不打开设置面板也看得见"子进程 / 库内嵌" */
.form {
  padding: 1px 7px;
  border-radius: 999px;
  background: var(--card-2, rgba(0, 0, 0, 0.05));
  color: var(--ink-dim);
  font-size: 11.5px;
  font-weight: 600;
}
/* 库形态不可用 = 下一趟编译会显式失败：必须显眼（D1 要求失败可见） */
.form.warn {
  background: var(--tint-mango);
  color: var(--warn-ink);
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

@media (prefers-reduced-motion: reduce) {
  .phase.running .phase-dot { animation: none; }
}
</style>
