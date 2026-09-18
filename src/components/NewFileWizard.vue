<!-- 新建向导（roadmap ㊺ §6.13.1-A）：空项目里建**第一个** `.tex` 时问一句"写什么"，
     然后生成一份**能直接编译**的最小骨架（而不是空文件 —— 空文件探测不出根文件，
     建完反而编译不了，那正是 ㊺ 第一轮验收卡住的地方）。

     触发条件是"空项目 ∧ 第一个 .tex ∧ 设置里开关为开"，判定在 FileTree（本组件只负责问与答）。
     刻意只放三个选项（语言 / 类型 / 标题）：这是新手的第一条路，不是模板库（模板库属 ⑫ 的账）。 -->
<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { ipc } from "../services/ipc";
import type { DocKind, DocLanguage } from "../bindings";

const props = defineProps<{
  /** 正在建的文件（项目内相对路径，展示用）。 */
  fileName: string;
  /** 标题预填值（调用方给文件名去扩展名）。 */
  initialTitle: string;
  /** 提交中：禁用交互，防重复提交。 */
  busy?: boolean;
  /** 提交失败信息（留在面板里，不静默失败也不关窗）。 */
  error?: string;
}>();

const emit = defineEmits<{
  (e: "confirm", value: { lang: DocLanguage; kind: DocKind; title: string }): void;
  (e: "cancel"): void;
  /** 「先建个文件夹」：关掉向导、回到树里的文件夹命名流程（roadmap ㊺ 的目录入口之一）。 */
  (e: "folder"): void;
}>();

/** 面向新手只放两种语言 / 四种类型：`ctex*` 与标准类一一对应，多一个选项就多一次犹豫。 */
const LANGS: { value: DocLanguage; label: string; hint: string }[] = [
  { value: "chinese", label: "中文", hint: "用 ctex 文档类，中文开箱可用" },
  { value: "english", label: "English", hint: "用标准文档类" },
];

const KINDS: { value: DocKind; label: string; hint: string }[] = [
  { value: "article", label: "短文 / 论文", hint: "单篇文档，最常用" },
  { value: "report", label: "报告", hint: "带章（chapter）的长报告" },
  { value: "book", label: "书", hint: "书籍：章 + 双面排版" },
  { value: "beamer", label: "幻灯片", hint: "演示文稿" },
];

const lang = ref<DocLanguage>("chinese");
const kind = ref<DocKind>("article");
const title = ref(props.initialTitle);

/**
 * 打开后把焦点收进面板：一来 `Esc` 能直接取消（键盘事件从输入框冒泡到面板），
 * 二来文件树里那个行内输入框会**失焦**——它的 `blur` 处理器据此知道"向导接管了"，
 * 不会把正在新建的这一行收掉。
 */
const titleInput = ref<HTMLInputElement | null>(null);
onMounted(() => titleInput.value?.focus());

/**
 * 骨架**预览**：内容由后端 `new_file_skeleton` 现算（core 那张表的唯一副本），
 * 本组件不另写一份"哪种语言对应哪个类"——预览与落盘产物因此不可能不一致。
 */
const preview = ref("");
const previewFailed = ref(false);

async function refreshPreview() {
  try {
    preview.value = await ipc.newFileSkeleton(lang.value, kind.value, title.value.trim() || null);
    previewFailed.value = false;
  } catch {
    previewFailed.value = true; // 只是预览拿不到：不挡创建（创建失败才是真错误）
  }
}

let timer: ReturnType<typeof setTimeout> | undefined;
/** 标题每敲一个字都发一次 IPC 没必要（内容就那么几行）；选项变化则立刻刷。 */
function schedulePreview(delay = 0) {
  clearTimeout(timer);
  timer = setTimeout(refreshPreview, delay);
}
watch([lang, kind], () => schedulePreview());
watch(title, () => schedulePreview(150));
refreshPreview();

function confirm() {
  if (props.busy) return;
  emit("confirm", { lang: lang.value, kind: kind.value, title: title.value });
}
</script>

<template>
  <div class="wizard-backdrop" @click.self="emit('cancel')">
    <div
      class="wizard-panel"
      role="dialog"
      aria-label="新建 LaTeX 文档"
      tabindex="-1"
      @keydown.esc.prevent="emit('cancel')"
    >
      <header class="panel-head">
        <span class="head-icon">📄</span>
        <span class="head-title">新建 LaTeX 文档</span>
        <button class="head-close" title="取消（不会创建文件）" @click="emit('cancel')">×</button>
      </header>

      <form class="panel-body" @submit.prevent="confirm">
        <p class="lead">
          这是项目里的第一份文档：选一下要写什么，我们生成一份<b>可以直接编译</b>的骨架。
          <code>{{ fileName }}</code>
        </p>

        <div class="field">
          <span class="field-label">文档语言</span>
          <div class="opt-row">
            <button
              v-for="l in LANGS"
              :key="l.value"
              type="button"
              class="opt"
              :class="{ on: lang === l.value }"
              :title="l.hint"
              data-testid="wizard-lang"
              @click="lang = l.value"
            >
              <span class="opt-label">{{ l.label }}</span>
              <span class="opt-hint">{{ l.hint }}</span>
            </button>
          </div>
        </div>

        <div class="field">
          <span class="field-label">文档类型</span>
          <div class="opt-row wrap">
            <button
              v-for="k in KINDS"
              :key="k.value"
              type="button"
              class="opt"
              :class="{ on: kind === k.value }"
              :title="k.hint"
              data-testid="wizard-kind"
              @click="kind = k.value"
            >
              <span class="opt-label">{{ k.label }}</span>
              <span class="opt-hint">{{ k.hint }}</span>
            </button>
          </div>
        </div>

        <div class="field">
          <label class="field-label" for="wizard-title">标题（可不填）</label>
          <input
            id="wizard-title"
            ref="titleInput"
            class="input"
            type="text"
            spellcheck="false"
            placeholder="留空就不写 \title"
            v-model="title"
            data-testid="wizard-title"
          />
        </div>

        <div class="field">
          <span class="field-label">将写入的内容</span>
          <pre class="preview" data-testid="wizard-preview">{{ previewFailed ? "（预览不可用，创建时仍会生成骨架）" : preview }}</pre>
        </div>

        <p class="err" v-if="error">{{ error }}</p>
      </form>

      <footer class="panel-foot">
        <span class="foot-hint">
          不想要这一步？在「设置 → 编译」里关掉「新建文件向导」即可。
          <button class="link" :disabled="busy" data-testid="wizard-folder" @click="emit('folder')">
            或者先建个文件夹
          </button>
        </span>
        <button class="btn" :disabled="busy" @click="emit('cancel')">取消</button>
        <button class="btn primary" :disabled="busy" data-testid="wizard-create" @click="confirm">
          {{ busy ? "创建中…" : "创建" }}
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.wizard-backdrop {
  position: fixed;
  inset: 0;
  z-index: 120;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--scrim);
  backdrop-filter: blur(2px);
}
.wizard-panel {
  width: 560px;
  max-width: calc(100vw - 40px);
  max-height: calc(100vh - 80px);
  display: flex;
  flex-direction: column;
  background: var(--card);
  border: 1.5px solid var(--line);
  border-radius: 14px;
  box-shadow: 0 24px 64px var(--scrim-weak), 4px 4px 0 var(--tint-ink-weak);
  overflow: hidden;
}
.panel-head {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 44px;
  padding: 0 14px 0 16px;
  border-bottom: 1.5px solid var(--line-soft);
  flex: 0 0 auto;
}
.head-icon { font-size: 14px; }
.head-title { font-weight: 700; font-size: 13.5px; letter-spacing: 0.5px; }
.head-close {
  margin-left: auto;
  width: 24px; height: 24px;
  border: none; background: transparent;
  color: var(--ink-faint); font-size: 17px;
  border-radius: 6px; cursor: pointer;
}
.head-close:hover { background: var(--card-2); color: var(--ink); }

.panel-body { flex: 1 1 auto; overflow: auto; padding: 12px 16px 8px; }
.lead { margin: 0 0 12px; font-size: 12px; color: var(--ink-dim); line-height: 1.6; }
.lead code {
  font-family: var(--mono);
  background: var(--card-2);
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 11px;
}

.field { margin-bottom: 12px; }
.field-label {
  display: block;
  margin-bottom: 5px;
  font-size: 11.5px;
  font-weight: 600;
  color: var(--ink-dim);
}
.opt-row { display: flex; gap: 8px; }
.opt-row.wrap { flex-wrap: wrap; }
.opt {
  flex: 1 1 0;
  min-width: 118px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 7px 10px;
  background: var(--card);
  border: 1.5px solid var(--line-soft);
  border-radius: var(--radius-sm);
  text-align: left;
  cursor: pointer;
  transition: border-color 0.12s, background 0.12s, transform 0.08s;
}
.opt:hover { border-color: var(--blueberry); background: var(--card-2); transform: translate(-1px, -1px); }
.opt.on { border-color: var(--blueberry); background: var(--card-2); box-shadow: inset 0 0 0 1px var(--blueberry); }
.opt-label { font-size: 12.5px; font-weight: 650; color: var(--ink); }
.opt-hint { font-size: 11px; color: var(--ink-faint); }

.input {
  width: 100%;
  box-sizing: border-box;
  height: 30px;
  padding: 0 9px;
  font: inherit;
  font-size: 12.5px;
  color: var(--ink);
  background: var(--surface);
  border: 1.5px solid var(--line);
  border-radius: var(--radius-sm);
}
.input:focus { outline: none; border-color: var(--blueberry); }

.preview {
  margin: 0;
  max-height: 132px;
  overflow: auto;
  padding: 8px 10px;
  font-family: var(--mono);
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--ink-dim);
  background: var(--card-2);
  border: 1.5px solid var(--line-soft);
  border-radius: var(--radius-sm);
  white-space: pre-wrap;
}

.err {
  margin: 4px 0;
  padding: 7px 10px;
  font-size: 11.5px;
  color: var(--danger-ink);
  background: rgba(var(--coral-rgb), 0.12);
  border-radius: var(--radius-sm);
}

.panel-foot {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 16px;
  border-top: 1.5px solid var(--line-soft);
  flex: 0 0 auto;
}
.foot-hint { flex: 1 1 auto; font-size: 11px; color: var(--ink-faint); }
/* 行内文字按钮（"或者先建个文件夹"）：跟着 hint 的排版走，不做成第三个按钮 */
.link {
  border: none;
  background: transparent;
  padding: 0;
  margin-left: 4px;
  color: var(--blueberry);
  font: inherit;
  text-decoration: underline;
  cursor: pointer;
}
.link:disabled { opacity: 0.5; cursor: default; }
.btn {
  height: 30px; padding: 0 13px;
  background: var(--card);
  border: 1.5px solid var(--line);
  border-radius: var(--radius-sm);
  color: var(--ink);
  font-size: 12.5px; font-weight: 550;
  cursor: pointer;
  box-shadow: var(--shadow-hard);
}
.btn:hover:not(:disabled) { transform: translate(-1px, -1px); box-shadow: var(--shadow-hard-big); border-color: var(--line-strong); }
.btn:disabled { opacity: 0.5; cursor: default; }
.btn.primary { background: var(--blueberry); border-color: var(--blueberry); color: #fff; }
</style>
