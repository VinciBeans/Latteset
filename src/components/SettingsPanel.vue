<!-- SettingsPanel（modules.md §6）：设置面板。
   编译四项（mode/debounce/timeout/engine）→ 全局设置；root_file → 项目覆盖。
   即改即存：store.update(patch) 后端写盘 + 广播 settings-changed，面板从 store 同步回显。 -->
<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useSettingsStore } from "../stores/settings";
import { ipc } from "../services/ipc";
import type { CompileMode, Engine, UiTheme } from "../bindings";

const emit = defineEmits<{ close: [] }>();

const store = useSettingsStore();
const settings = computed(() => store.settings);
/**
 * 形态与资源。`tectonic` 在绑定里是**可选**字段（后端 `#[serde(default)]` 是为了让旧
 * settings.json 没有这个键时仍能加载升级）⇒ 这里补默认值，模板与逻辑里就不必到处写 `?.`。
 */
const tectonic = computed(
  () => settings.value?.tectonic ?? { lib_form: false, bundle: null, cache_dir: null },
);
/** 用户是否存过任何 Tectonic 专属设置——用于在引擎不是 Tectonic 时提示"存了但不生效"。 */
const hasTectonicSettings = computed(
  () => tectonic.value.lib_form || !!tectonic.value.bundle || !!tectonic.value.cache_dir,
);
const tectonicSettingsSummary = computed(() =>
  [
    tectonic.value.lib_form ? "库内嵌" : null,
    tectonic.value.bundle ? "宏包集" : null,
    tectonic.value.cache_dir ? "缓存目录" : null,
  ]
    .filter(Boolean)
    .join(" / "),
);

const MODES: { value: CompileMode; label: string; hint: string }[] = [
  { value: "continuous", label: "连续编译", hint: "编辑后 500ms 自动编译" },
  { value: "on_save", label: "保存触发", hint: "手动点「编译」或保存时触发" },
];

/**
 * 设置分页（标签栏）。四类按"用户要解决的问题"分，而不是按字段所属模块：
 * - 外观：与文档无关的个人偏好；
 * - 编译：什么时候编、编多久（模式/防抖/超时）；
 * - 引擎：用谁编（**引擎选择与 Tectonic 的形态/资源同页** —— 后者只在选中 Tectonic 时才有意义，
 *   分到两页会让那条"存了但不生效"的提示失去落点）；
 * - 项目：只影响当前项目的覆盖项（根文件）。
 */
const TABS = [
  { id: "appearance", label: "外观" },
  { id: "compile", label: "编译" },
  { id: "engine", label: "引擎" },
  { id: "project", label: "项目" },
] as const;
type TabId = (typeof TABS)[number]["id"];

/**
 * 当前分页。放在**模块作用域**（不是 setup 内）：`v-if` 每次开关面板都会重建组件，
 * 组件内的 ref 会回到第一页——而"我刚改完引擎，再打开还想看引擎"是常态。
 * 模块级变量只为"记住上次看的那页"，不进设置、不落盘（换次启动回到第一页可以接受）。
 */
const activeTab = ref<TabId>("appearance");

/** 左右方向键切页（tablist 的键盘约定）。 */
function onTabKey(e: KeyboardEvent) {
  const i = TABS.findIndex((t) => t.id === activeTab.value);
  if (e.key === "ArrowRight") activeTab.value = TABS[(i + 1) % TABS.length].id;
  else if (e.key === "ArrowLeft") activeTab.value = TABS[(i - 1 + TABS.length) % TABS.length].id;
  else if (e.key === "Home") activeTab.value = TABS[0].id;
  else if (e.key === "End") activeTab.value = TABS[TABS.length - 1].id;
  else return;
  e.preventDefault();
}

/** 主题（roadmap ⑩）：全局设置，即点即存。"跟随系统"由前端监听 prefers-color-scheme 落实。 */
const THEMES: { value: UiTheme; label: string; hint: string }[] = [
  { value: "light", label: "浅色", hint: "Candy Desk（默认）" },
  { value: "dark", label: "深色", hint: "夜间态：墨紫罗兰纸面 + 提亮的糖果色" },
  { value: "system", label: "跟随系统", hint: "按操作系统的深浅色偏好切换" },
];
/**
 * 引擎清单。**顺序 = 推荐顺序**：Tectonic 排第一 —— ADR-0014 把 Tectonic 定为新功能的基准形态，
 * 选项栏里它就该是第一眼看到的那个（注意这与"默认值"是两件事：`Settings::default` 仍是 XeLaTeX
 * + 子进程，改这里是改推荐顺序，不是改默认档）。
 *
 * ⚠ **`hint` 不进 `<option>` 的文本**：原生下拉的弹层按最长选项撑宽，把整句说明塞进选项会让弹层
 * 冲出面板（真机反馈）。选项只留名字，说明显示在 select 下方 —— 与「主题」「编译模式」两组同一范式。
 */
const ENGINES: { value: Engine; label: string; hint: string }[] = [
  {
    value: "tectonic",
    label: "Tectonic",
    // 文案必须与实现一致（runner.rs 的 `-C` 分档）：首次编译（本地缓存没有可用 bundle）**会联网**
    // 下载宏包集；之后缓存就绪即离线复用。写成"总是联网"或"总是离线"都会与实际行为相反。
    hint: "免装 TeX Live（自带宏包）；首次编译需联网下载宏包集（约 60 MB，可能数十秒到数分钟），之后离线复用缓存；该引擎下不启用页级增量复用",
  },
  { value: "xelatex", label: "XeLaTeX", hint: "默认档：中文支持最佳，需要本机装好 TeX Live" },
  { value: "lualatex", label: "LuaLaTeX", hint: "Lua 脚本、最新特性；比 XeLaTeX 慢，首次编译要建字体缓存" },
  { value: "pdflatex", label: "pdfLaTeX", hint: "传统引擎，中文需额外配置" },
];

/** 当前选中引擎的那句说明（不在 `<option>` 里，见 `ENGINES` 的注释）。 */
const engineHint = computed(() => ENGINES.find((e) => e.value === settings.value?.compile.engine)?.hint ?? "");

const DEBOUNCE_MIN = 100;
const DEBOUNCE_MAX = 5000;
// 与 core `TIMEOUT_SECS_RANGE` 保持一致（5..=1800）：上限取 1800s 是因为真实学位论文
// 首编可达数分钟（roadmap ㉕），上限太低会让用户"调到顶也编不过"。
const TIMEOUT_MIN = 5;
const TIMEOUT_MAX = 1800;

const debounceInput = ref("");
const timeoutInput = ref("");
const rootFileInput = ref("");
const rootFileError = ref("");
const lastAppliedRoot = ref("");

// 打开时快照输入框（失焦提交后失焦时可能正在编辑）
function snapshotInputs() {
  const s = settings.value;
  if (!s) return;
  debounceInput.value = String(s.compile.debounce_ms);
  timeoutInput.value = String(s.compile.timeout_secs);
  rootFileInput.value = s.root_file ?? "";
  lastAppliedRoot.value = s.root_file ?? "";
  rootFileError.value = "";
}
onMounted(snapshotInputs);

/** 防抖提交：clamp + 仅变化才发 patch。 */
async function submitDebounce() {
  const s = settings.value;
  if (!s) return;
  const n = Math.min(DEBOUNCE_MAX, Math.max(DEBOUNCE_MIN, Number(debounceInput.value)));
  const v = Number.isFinite(n) ? Math.round(n) : s.compile.debounce_ms;
  debounceInput.value = String(v);
  if (v !== s.compile.debounce_ms) await store.update({ debounce_ms: v });
}

/** 超时提交：clamp + 仅变化才发 patch。 */
async function submitTimeout() {
  const s = settings.value;
  if (!s) return;
  const n = Math.min(TIMEOUT_MAX, Math.max(TIMEOUT_MIN, Number(timeoutInput.value)));
  const v = Number.isFinite(n) ? Math.round(n) : s.compile.timeout_secs;
  timeoutInput.value = String(v);
  if (v !== s.compile.timeout_secs) await store.update({ timeout_secs: v });
}

/** 模式切换：即点即存。 */
async function setMode(mode: CompileMode) {
  if (settings.value?.compile.mode === mode) return;
  await store.update({ mode });
}

/** 引擎切换：即点即存。 */
async function setEngine(engine: Engine) {
  if (settings.value?.compile.engine === engine) return;
  await store.update({ engine });
}

/** 主题切换：即点即存；落地（DOM/Monaco）由 useTheme 的 watcher 统一做。 */
async function setTheme(theme: UiTheme) {
  if (themeOf(settings.value) === theme) return;
  await store.update({ theme });
}

/**
 * 新建向导开关（roadmap ㊺ §6.13.1-A）：**默认开**（新手第一条路要有人领），
 * 关掉后新建 `.tex` 回到"建空文件"。`compile.new_file_wizard` 在绑定里是必填 `boolean`，
 * 但旧 settings.json 可能没有这个键 ⇒ 后端已按默认开补齐，这里再兜一次底。
 */
const wizardOn = computed(() => settings.value?.compile.new_file_wizard ?? true);

async function setWizard(on: boolean) {
  if (wizardOn.value === on) return;
  await store.update({ new_file_wizard: on });
}

/** 主题的当前值（`ui` 在绑定里可选 ⇒ 缺省按浅色，与后端 `Default for UiSettings` 同口径）。 */
function themeOf(s: { ui?: { theme: UiTheme } | null } | null | undefined): UiTheme {
  return s?.ui?.theme ?? "light";
}

/**
 * 引擎页上是否有**需要知道但当前页看不见**的事：引擎不是 Tectonic、却存着 Tectonic 专属设置。
 * 不给标记的话，用户切到 XeLaTeX 后那几项就成了隐形开关（既看不到、也没提示）。
 */
const engineTabNotice = computed(
  () => settings.value?.compile.engine !== "tectonic" && hasTectonicSettings.value,
);

/** 根文件覆盖：仅相对路径；输入为空 = 清除覆盖（自动探测，幂等发送 null patch）。 */
async function applyRootFile() {
  const raw = rootFileInput.value.trim();
  if (raw === "") {
    rootFileInput.value = "";
    lastAppliedRoot.value = "";
    rootFileError.value = "";
    // 无条件清除：即使当前无记录也幂等（保证“清除”永远移除磁盘覆盖）
    try {
      await store.update({ root_file: null });
    } catch (e) {
      console.error("清除根文件覆盖失败：", e);
    }
    return;
  }
  if (/^(\/|[A-Za-z]:)/.test(raw) || raw.includes("\\")) {
    rootFileError.value = "请填写项目内相对路径（如 main.tex、chapters/main.tex）";
    return;
  }
  rootFileError.value = "";
  const target = raw.replace(/^\.\//, "");
  if (target !== (settings.value?.root_file ?? "")) {
    try {
      await store.update({ root_file: target });
    } catch (e) {
      console.error("设置根文件覆盖失败：", e);
    }
  }
  lastAppliedRoot.value = target;
}

/** 恢复默认（编译各项 → 默认值；root_file 不动）。 */
async function resetDefaults() {
  await store.update({
    mode: "continuous",
    debounce_ms: 500,
    timeout_secs: 120,
    engine: "xelatex",
    // 向导也是"编译"页的项 ⇒ 一并回默认开（按钮的字面意思就是"恢复默认"）
    new_file_wizard: true,
  });
  snapshotInputs();
}

// ---------------------------------------------------------------- Tectonic 形态（⑫ 里程碑）

/**
 * 存储形态 → 展示形态。磁盘上 bundle 存的是上游认的 `file:///…`（见 core
 * `TectonicSettings::normalize_bundle`），但用户眼里它就是一条目录路径 ⇒ 界面上不摆 URL 前缀。
 */
function bundleDisplay(stored: string | null | undefined): string {
  const s = (stored ?? "").trim();
  if (!s.startsWith("file://")) return s;
  const rest = s.slice("file://".length); // `/E:/x`（Windows）或 `/home/u`（POSIX）
  return /^\/[A-Za-z]:/.test(rest) ? rest.slice(1) : rest;
}

/** 本次构建是否编入库形态：没编进来就禁用该选项（而不是让用户选了到编译时才炸）。 */
const libAvailable = ref<boolean | null>(null);
const bundleInput = ref("");
/** 留空 = 用上游兜底网络地址（`None`）；`true` = 用本地目录 bundle。 */
const bundleLocal = ref(false);
const cacheInput = ref("");
const formError = ref("");

onMounted(async () => {
  try {
    libAvailable.value = await ipc.libFormAvailable();
  } catch {
    libAvailable.value = false;
  }
  const t = tectonic.value;
  if (t) {
    bundleLocal.value = !!t.bundle;
    bundleInput.value = bundleDisplay(t.bundle);
    cacheInput.value = t.cache_dir ?? "";
  }
});

/** 形态切换：即点即存，**下一趟编译就生效**（runner 每趟读设置，不需要重启）。 */
async function setLibForm(useLib: boolean) {
  if (tectonic.value.lib_form === useLib) return;
  formError.value = "";
  try {
    await store.update({ lib_form: useLib });
  } catch (e) {
    formError.value = String((e as { message?: string })?.message ?? e);
  }
}

/**
 * bundle 来源：留空 = 上游兜底（网络）。
 *
 * 这里**不管路径形态**：`E:\…` 与 `file:///E:/…` 都照原样发出去，由 core 统一归一化成存储形态
 * （`TectonicSettings::normalize_bundle`，LB-1）。应用后回填**展示形态**，保证输入框与磁盘一致
 * ——否则用户粘一条 `E:\…`，输入框留着反斜杠、磁盘上却是 URL，看起来像"没生效"。
 */
async function applyBundle() {
  formError.value = "";
  const raw = bundleInput.value.trim();
  if (!bundleLocal.value || raw === "") {
    // 切回"上游兜底"：空串 = 清除
    bundleInput.value = "";
    if (tectonic.value.bundle) {
      try {
        await store.update({ bundle: "" });
      } catch (e) {
        formError.value = String((e as { message?: string })?.message ?? e);
      }
    }
    return;
  }
  if (raw === bundleDisplay(tectonic.value.bundle)) return;
  try {
    await store.update({ bundle: raw });
    bundleInput.value = bundleDisplay(tectonic.value.bundle);
  } catch (e) {
    formError.value = String((e as { message?: string })?.message ?? e);
  }
}

/** 缓存目录：留空 = 宿主的应用缓存目录（必须绝对路径，相对路径会被后端拒绝）。 */
async function applyCacheDir() {
  formError.value = "";
  const raw = cacheInput.value.trim();
  const cur = tectonic.value.cache_dir ?? "";
  if (raw === cur) return;
  try {
    await store.update({ cache_dir: raw });
  } catch (e) {
    formError.value = String((e as { message?: string })?.message ?? e);
  }
}
</script>

<template>
  <div class="settings-backdrop" @click.self="emit('close')">
    <div class="settings-panel" role="dialog" aria-label="设置">
      <header class="panel-head">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z"/></svg>
        <span class="head-title">设置</span>
        <button class="head-close" title="关闭" @click="emit('close')">×</button>
      </header>

      <!-- 标签栏：**下划线式**（而不是面板里选值用的胶囊式）——两级控件要一眼分得开：
           这里是"翻页"，面板里那些 .seg-btn 是"选一个值"。 -->
      <div class="settings-tabs" role="tablist" aria-label="设置分类" @keydown="onTabKey">
        <button
          v-for="t in TABS"
          :key="t.id"
          class="tab-btn"
          :class="{ on: activeTab === t.id }"
          role="tab"
          :aria-selected="activeTab === t.id"
          :aria-controls="`tabpanel-${t.id}`"
          :tabindex="activeTab === t.id ? 0 : -1"
          @click="activeTab = t.id"
        >
          {{ t.label }}
          <!-- 引擎页有"存了但不生效"的设置时点一颗：否则切到别的页就再没有线索 -->
          <span
            v-if="t.id === 'engine' && engineTabNotice"
            class="tab-dot"
            title="有已保存的 Tectonic 形态设置在当前引擎下不生效"
          />
        </button>
      </div>

      <div class="panel-body" v-if="settings">
        <!-- 外观（roadmap ⑩） -->
        <section class="sec" v-if="activeTab === 'appearance'" id="tabpanel-appearance" role="tabpanel">
          <h3 class="sec-title">外观</h3>
          <div class="field">
            <label class="field-label" for="set-theme">主题</label>
            <div class="mode-seg" id="set-theme">
              <button
                v-for="t in THEMES"
                :key="t.value"
                class="seg-btn"
                :class="{ on: themeOf(settings) === t.value }"
                :title="t.hint"
                @click="setTheme(t.value)"
              >{{ t.label }}</button>
            </div>
            <p class="field-hint">{{ THEMES.find(t => t.value === themeOf(settings))?.hint }}</p>
          </div>
        </section>

        <!-- 编译：什么时候编、编多久 -->
        <section class="sec" v-if="activeTab === 'compile'" id="tabpanel-compile" role="tabpanel">
          <h3 class="sec-title">编译</h3>

          <div class="field">
            <label class="field-label" for="set-mode">编译模式</label>
            <div class="mode-seg" id="set-mode">
              <button
                v-for="m in MODES"
                :key="m.value"
                class="seg-btn"
                :class="{ on: settings.compile.mode === m.value }"
                :title="m.hint"
                @click="setMode(m.value)"
              >{{ m.label }}</button>
            </div>
            <p class="field-hint">{{ MODES.find(m => m.value === settings?.compile.mode)?.hint }}</p>
          </div>

          <!-- 新建向导（roadmap ㊺ §6.13.1-A）：只影响"空项目里建第一个 .tex"那一次 -->
          <div class="field">
            <label class="field-label" for="set-wizard">新建文件向导</label>
            <div class="mode-seg" id="set-wizard">
              <button
                class="seg-btn"
                :class="{ on: wizardOn }"
                title="在空项目里新建第一个 .tex 时问一次「文档语言 / 类型 / 标题」，直接生成能编译的骨架"
                @click="setWizard(true)"
              >开</button>
              <button
                class="seg-btn"
                :class="{ on: !wizardOn }"
                title="新建 .tex 一律得到空文件（已有根文档的项目本来就不会弹）"
                @click="setWizard(false)"
              >关</button>
            </div>
            <p class="field-hint">
              {{ wizardOn
                ? "仅在「项目里还没有根文件」时、新建第一个 .tex 时弹一次；已有主文件的项目不受影响"
                : "关闭：新建 .tex 得到空文件（需要自己写 \\documentclass）" }}
            </p>
          </div>

          <div class="field-row">
            <div class="field">
              <label class="field-label" for="set-debounce">防抖（毫秒）</label>
              <input
                id="set-debounce"
                class="input number"
                type="number"
                :min="DEBOUNCE_MIN"
                :max="DEBOUNCE_MAX"
                step="100"
                v-model="debounceInput"
                @blur="submitDebounce"
                @keydown.enter="($event.target as HTMLInputElement).blur()"
              />
            </div>
            <div class="field">
              <label class="field-label" for="set-timeout">超时（秒）</label>
              <input
                id="set-timeout"
                class="input number"
                type="number"
                :min="TIMEOUT_MIN"
                :max="TIMEOUT_MAX"
                step="10"
                v-model="timeoutInput"
                @blur="submitTimeout"
                @keydown.enter="($event.target as HTMLInputElement).blur()"
              />
            </div>
          </div>
        </section>

        <!-- 项目：只影响当前项目的覆盖项 -->
        <section class="sec" v-if="activeTab === 'project'" id="tabpanel-project" role="tabpanel">
          <h3 class="sec-title">项目</h3>
          <div class="field">
            <label class="field-label" for="set-root">根文件（覆盖自动探测）</label>
            <div class="root-row">
              <input
                id="set-root"
                class="input"
                type="text"
                spellcheck="false"
                placeholder="main.tex / chapters/main.tex（留空 = 自动探测）"
                v-model="rootFileInput"
                @keydown.enter="applyRootFile"
              />
              <button class="btn small" @click="applyRootFile">应用</button>
              <button
                class="btn small ghost"
                :disabled="!settings.root_file && !rootFileInput"
                title="清除项目覆盖，回到自动探测"
                @click="rootFileInput = ''; applyRootFile()"
              >清除</button>
            </div>
            <p class="field-hint" :class="{ err: rootFileError }">
              {{ rootFileError || (settings.root_file ? `当前覆盖：${settings.root_file}` : "留空时自动探测根文件") }}
            </p>
          </div>
        </section>

        <!-- 引擎：用谁编。**引擎选择与 Tectonic 的形态/资源同页** —— 后者只在选中 Tectonic 时
             才有意义，分到两页会让那条"存了但不生效"的提示失去落点（见 TABS 的注释）。 -->
        <section class="sec" v-if="activeTab === 'engine'" id="tabpanel-engine" role="tabpanel">
          <h3 class="sec-title">引擎</h3>

          <div class="field">
            <label class="field-label" for="set-engine">TeX 引擎</label>
            <select id="set-engine" class="input select" :value="settings.compile.engine" @change="setEngine(($event.target as HTMLSelectElement).value as Engine)">
              <option v-for="e in ENGINES" :key="e.value" :value="e.value">{{ e.label }}</option>
            </select>
            <!-- 说明放在 select **下方**（不进 option 文本）：原生弹层按最长选项撑宽，塞进去会冲出面板 -->
            <p class="field-hint">{{ engineHint }}</p>
            <!-- Tectonic 段只在选中 Tectonic 时显示（下面那条 template 的 v-if），所以这里必须
                 把"已经存了 Tectonic 设置、但当前不生效"讲出来——否则用户切到 XeLaTeX 后
                 那几项连看都看不到，成了隐形开关。 -->
            <p class="field-hint warn" v-if="engineTabNotice">
              已保存 Tectonic 形态设置（{{ tectonicSettingsSummary }}）；<b>仅在选择 Tectonic 引擎时生效</b>，当前引擎不受影响。
            </p>
          </div>

          <!-- Tectonic 形态（⑫ 里程碑的设置面）：只在引擎真的选了 Tectonic 时出现——
               引擎不是它的时候，这几项对用户没有任何作用，摆在面板上只会让人以为"改了没生效" -->
          <template v-if="settings.compile.engine === 'tectonic'">
            <h3 class="sec-title sub">Tectonic 形态与资源</h3>

          <div class="field">
            <label class="field-label" for="set-form">驱动形态</label>
            <div class="mode-seg" id="set-form">
              <button
                class="seg-btn"
                :class="{ on: !tectonic.lib_form }"
                title="运行官方 tectonic.exe：稳定、与上游行为一致；需要 PATH 上有 tectonic"
                @click="setLibForm(false)"
              >子进程</button>
              <button
                class="seg-btn"
                :class="{ on: tectonic.lib_form }"
                :disabled="libAvailable === false"
                :title="libAvailable === false
                  ? '本次构建未编入 tectonic-lib 特性，无法使用库形态'
                  : '把 Tectonic 引擎嵌进产品进程：免装 TeX Live（自带宏包）、支持 BibTeX 与多趟收敛、可做页级增量复用；不支持 biber/makeindex 这类外部工具'"
                @click="setLibForm(true)"
              >库内嵌</button>
            </div>
            <p class="field-hint" :class="{ err: formError }">
              {{ formError || (libAvailable === false
                ? "库形态在本次构建中不可用（需以 --features tectonic-lib 构建）"
                : tectonic.lib_form
                  ? "库形态：切过去后，下一趟编译即生效（无需重启）；不支持 biber/makeindex（外部工具），检出时会提示"
                  : "子进程形态：与上游 tectonic.exe 行为一致（默认）") }}
            </p>
          </div>

          <div class="field">
            <label class="field-label" for="set-bundle-path">宏包集（bundle）来源</label>
            <div class="mode-seg" id="set-bundle-source">
              <button
                class="seg-btn"
                :class="{ on: !bundleLocal }"
                title="用上游兜底地址首次联网下载（约 60 MB），之后离线复用缓存"
                @click="bundleLocal = false; bundleInput = ''; applyBundle()"
              >自动（联网）</button>
              <button
                class="seg-btn"
                :class="{ on: bundleLocal }"
                title="指向本地目录 bundle：离线/内网部署必须用这个（目录需自带 SHA256SUM）"
                @click="bundleLocal = true"
              >本地目录</button>
            </div>
            <div class="root-row" v-if="bundleLocal" style="margin-top: 6px">
              <input
                id="set-bundle-path"
                class="input"
                type="text"
                spellcheck="false"
                placeholder="E:/bundles/tex  或  bundles/tex"
                v-model="bundleInput"
                @keydown.enter="applyBundle"
              />
              <button class="btn small" @click="applyBundle">应用</button>
            </div>
            <p class="field-hint">
              {{ tectonic.bundle
                ? `当前：${bundleDisplay(tectonic.bundle)}`
                : "当前：上游兜底地址（首次编译需联网下载宏包集）" }}
            </p>
          </div>

          <div class="field">
            <label class="field-label" for="set-cache">缓存目录（format / bundle）</label>
            <div class="root-row">
              <input
                id="set-cache"
                class="input"
                type="text"
                spellcheck="false"
                placeholder="留空 = 应用缓存目录"
                v-model="cacheInput"
                @keydown.enter="applyCacheDir"
              />
              <button class="btn small" @click="applyCacheDir">应用</button>
              <button
                class="btn small ghost"
                :disabled="!tectonic.cache_dir"
                title="回到应用缓存目录"
                @click="cacheInput = ''; applyCacheDir()"
              >清除</button>
            </div>
            <p class="field-hint">
              必填绝对路径：留空时用应用缓存目录。格式文件（约 24 MB）<b>不会</b>落进你的项目目录。
            </p>
          </div>

          <p class="field-hint" v-if="!tectonic.lib_form">
            以上两项只在<b>库内嵌</b>形态下生效（子进程档由 `tectonic.exe` 自己管缓存）。
          </p>
          </template>
        </section>
      </div>

      <footer class="panel-foot">
        <button class="btn small ghost" @click="resetDefaults">恢复默认</button>
        <span class="foot-spacer" />
        <button class="btn small primary" @click="emit('close')">完成</button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.settings-backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--scrim);
  backdrop-filter: blur(2px);
}
.settings-panel {
  width: 460px;
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
.head-title { font-weight: 700; font-size: 13.5px; letter-spacing: 0.5px; }
.panel-head svg { color: var(--blueberry); flex: 0 0 auto; }
.head-close {
  margin-left: auto;
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  color: var(--ink-faint);
  font-size: 17px;
  border-radius: 6px;
  cursor: pointer;
}
.head-close:hover { background: var(--card-2); color: var(--ink); }

/* ---- 标签栏（下划线式）----
   与面板内 `.seg-btn`（胶囊＝选一个值）**故意不同形**：这里是翻页，不是选值。
   下划线用 `--blueberry`，未选中项压在 `--ink-dim`，整条底边就是分页与内容的自然分界。 */
.settings-tabs {
  display: flex;
  gap: 2px;
  padding: 0 12px;
  border-bottom: 1.5px solid var(--line-soft);
  flex: 0 0 auto;
}
.tab-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 9px 12px 8px;
  border: none;
  background: transparent;
  color: var(--ink-dim);
  font-size: 12.5px;
  font-weight: 600;
  font-family: inherit;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  margin-bottom: -1.5px;   /* 压住容器底边，选中时下划线才是"连着的" */
  transition: color 0.13s, border-color 0.13s;
}
.tab-btn:hover { color: var(--ink); }
.tab-btn.on { color: var(--blueberry); border-bottom-color: var(--blueberry); }
.tab-btn:focus-visible { outline: 2px solid var(--blueberry); outline-offset: -2px; border-radius: 4px 4px 0 0; }
/* "有存了但不生效的设置"的提示点（琥珀）：标签栏是唯一能跨页传达这件事的地方 */
.tab-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--mango);
  flex: 0 0 auto;
}

.panel-body { flex: 1 1 auto; overflow: auto; padding: 4px 18px 12px; }
.sec { padding: 12px 0 4px; }
/* 分页后同一页里只可能有一条 .sec，所以 `.sec + .sec` 那条分隔线改为**同页内的子标题**分界 */
.sec + .sec { border-top: 1px solid var(--line-soft); margin-top: 8px; padding-top: 14px; }
.sec-title {
  margin: 0 0 12px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 1px;
  text-transform: uppercase;
  color: var(--ink-dim);
}
/* 同页第二段（引擎页的 Tectonic 形态与资源）：与上面的"引擎"拉开层次 */
.sec-title.sub {
  margin-top: 22px;
  padding-top: 14px;
  border-top: 1px solid var(--line-soft);
  letter-spacing: 0.6px;
}

.field { margin-bottom: 14px; }
.field-row { display: flex; gap: 12px; }
.field-row .field { flex: 1; }
.field-label {
  display: block;
  margin-bottom: 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--ink);
}
.field-hint { margin: 6px 0 0; font-size: 11px; color: var(--ink-faint); }
.field-hint.err { color: var(--danger-ink); }
/* 「存了但当前不生效」——不能用 err（不是错误），但也不能与普通灰字同权重 */
.field-hint.warn { color: var(--sienna, var(--sienna)); }

.input {
  width: 100%;
  height: 32px;
  padding: 0 10px;
  background: var(--paper);
  border: 1.5px solid var(--line);
  border-radius: 8px;
  color: var(--ink);
  font-size: 12.5px;
  font-family: var(--mono);
  box-sizing: border-box;
}
.input:focus { outline: none; border-color: var(--blueberry); box-shadow: 0 0 0 3px rgba(var(--blueberry-rgb), 0.14); }
.input.number { font-family: var(--mono); }
.select { cursor: pointer; }

.mode-seg {
  display: flex;
  gap: 6px;
  padding: 4px;
  background: var(--paper);
  border: 1.5px solid var(--line);
  border-radius: 9px;
}
.seg-btn {
  flex: 1;
  height: 26px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--ink-dim);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.13s;
}
.seg-btn:hover { color: var(--ink); }
.seg-btn.on { background: var(--blueberry); color: var(--on-accent); box-shadow: 0 2px 6px rgba(var(--blueberry-rgb), 0.35); }

.root-row { display: flex; gap: 6px; }
.root-row .input { flex: 1 1 auto; }
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 32px;
  padding: 0 12px;
  border: 1.5px solid var(--line);
  border-radius: 8px;
  background: var(--card);
  color: var(--ink);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
  transition: all 0.13s;
}
.btn:hover:not(:disabled) { border-color: var(--blueberry); color: var(--blueberry); }
.btn.small { height: 32px; padding: 0 12px; }
.btn.primary {
  background: linear-gradient(135deg, var(--blueberry-hover) 0%, var(--blueberry) 60%, var(--info) 130%);
  border-color: transparent;
  color: var(--on-accent);
}
.btn.primary:hover:not(:disabled) { color: var(--on-accent); border-color: transparent; }
.btn.ghost { background: transparent; box-shadow: none; }
.btn:disabled { opacity: 0.45; cursor: default; }

.panel-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1.5px solid var(--line-soft);
  flex: 0 0 auto;
}
.foot-spacer { flex: 1; }
</style>
