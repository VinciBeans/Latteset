<!-- SettingsPanel（modules.md §6）：设置面板。
   编译四项（mode/debounce/timeout/engine）→ 全局设置；root_file → 项目覆盖。
   即改即存：store.update(patch) 后端写盘 + 广播 settings-changed，面板从 store 同步回显。 -->
<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useSettingsStore } from "../stores/settings";
import type { CompileMode, Engine } from "../bindings";

const emit = defineEmits<{ close: [] }>();

const store = useSettingsStore();
const settings = computed(() => store.settings);

const MODES: { value: CompileMode; label: string; hint: string }[] = [
  { value: "continuous", label: "连续编译", hint: "编辑后 500ms 自动编译" },
  { value: "on_save", label: "保存触发", hint: "手动点「编译」或保存时触发" },
];
const ENGINES: { value: Engine; label: string; hint: string }[] = [
  { value: "xelatex", label: "XeLaTeX", hint: "默认，中文支持最佳" },
  { value: "lualatex", label: "LuaLaTeX", hint: "Lua 脚本、最新特性；比 XeLaTeX 慢，首次编译要建字体缓存" },
  { value: "pdflatex", label: "pdfLaTeX", hint: "传统引擎，中文需额外配置" },
  {
    value: "tectonic",
    label: "Tectonic",
    hint: "免装 TeX Live（自带宏包）；首次编译要联网下载 bundle，可能数十秒；该引擎下不启用页级增量复用",
  },
];

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

/** 恢复默认（编译四项 → 默认值；root_file 不动）。 */
async function resetDefaults() {
  await store.update({ mode: "continuous", debounce_ms: 500, timeout_secs: 120, engine: "xelatex" });
  snapshotInputs();
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

      <div class="panel-body" v-if="settings">
        <!-- 编译 -->
        <section class="sec">
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

          <div class="field">
            <label class="field-label" for="set-engine">TeX 引擎</label>
            <select id="set-engine" class="input select" :value="settings.compile.engine" @change="setEngine(($event.target as HTMLSelectElement).value as Engine)">
              <option v-for="e in ENGINES" :key="e.value" :value="e.value">{{ e.label }} — {{ e.hint }}</option>
            </select>
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

        <!-- 项目 -->
        <section class="sec">
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
  background: rgba(30, 26, 46, 0.38);
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
  box-shadow: 0 24px 64px rgba(43, 36, 56, 0.28), 4px 4px 0 rgba(43, 36, 56, 0.06);
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

.panel-body { flex: 1 1 auto; overflow: auto; padding: 4px 18px 12px; }
.sec { padding: 12px 0 4px; }
.sec + .sec { border-top: 1px solid var(--line-soft); margin-top: 8px; padding-top: 14px; }
.sec-title {
  margin: 0 0 12px;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 1px;
  text-transform: uppercase;
  color: var(--ink-dim);
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
.field-hint.err { color: #e85f52; }

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
.input:focus { outline: none; border-color: var(--blueberry); box-shadow: 0 0 0 3px rgba(93, 95, 239, 0.14); }
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
.seg-btn.on { background: var(--blueberry); color: #fff; box-shadow: 0 2px 6px rgba(93, 95, 239, 0.35); }

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
  background: linear-gradient(135deg, #6a5cff 0%, var(--blueberry) 60%, #4e9bff 130%);
  border-color: transparent;
  color: #fff;
}
.btn.primary:hover:not(:disabled) { color: #fff; border-color: transparent; }
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
