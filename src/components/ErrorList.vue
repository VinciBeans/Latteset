<!-- ErrorList（modules.md §9.4）：错误条目 + 点击跳转源码。
   去重/截断（modules.md §12 后置项）：同源错误（文件 + 首行消息相同）聚合为一条并显示 ×N；
   不同源最多展示 MAX_DISPLAY 组，超出提示隐藏数量（xelatex 错误雪崩时几十条同源错误不再刷屏）。
   诊断（roadmap ④）：条目带 diagnosis 时优先展示「原因 + 怎么改」两行，原始 .log 消息降到 title 提示；
   无诊断则退回原文首行（宁可不说，也不瞎说）。 -->
<script setup lang="ts">
import { computed, ref } from "vue";
import { useCompileStore } from "../stores/compile";
import { useEditorStore } from "../stores/editor";
import { useSettingsStore } from "../stores/settings";
import { ipc } from "../services/ipc";
import { groupErrors, MAX_DISPLAY } from "../services/errorGroups";
import { ONLINE_BUNDLE_URL } from "../services/bundleSource";
import type { ErrorEntry } from "../bindings";

const compile = useCompileStore();
const editor = useEditorStore();
const settings = useSettingsStore();

/** B-08：「显示全部」后不再截断（MAX_DISPLAY 不出现在文案里）。 */
const showAll = ref(false);

/** 去重 + 截断（口径在 `services/errorGroups`：与状态栏计数同源）。 */
const grouped = computed(() =>
  groupErrors(compile.errors, showAll.value ? Number.POSITIVE_INFINITY : MAX_DISPLAY),
);

/** 空态三态（A-01/A-02）：未编译 / 排版中 / 没有发现问题。 */
const emptyText = computed(() => {
  if (compile.phase === "running") return "排版中…";
  if (!compile.hasCompiled) return "还没有编译";
  return "没有发现问题";
});
const emptyIcon = computed(() => {
  if (compile.phase === "running") return "…";
  return compile.hasCompiled ? "✓" : "○";
});
const emptyIdle = computed(() => compile.phase !== "running" && !compile.hasCompiled);

function jump(file: string | null, line: number | null) {
  if (!file) return;
  editor.openFile(file, line ?? 1);
}

// ---- 超时一键重试（roadmap ㉕）----
const retrying = ref(false);

/** 把编译超时提到诊断建议值并立即重跑（建议值由后端按证据算出，见 core `diagnose_timeout`）。 */
async function raiseTimeoutAndRetry(secs: number) {
  if (retrying.value) return;
  retrying.value = true;
  try {
    await settings.update({ timeout_secs: secs });
    await ipc.compileNow();
  } catch (e) {
    console.error("提高超时并重试失败：", e);
  } finally {
    retrying.value = false;
  }
}

// ---- 缺包补全通道一键切换（T16 / ADR-0016 候选 A）----
//
// 内置精简集只覆盖代表性文档集（不是完整 TeX Live）：检出"缺宏包/类/文件/字体"诊断时，
// 给一个显式切到**在线完整宏包集**的按钮（写死默认在线地址 → 重编）。
// **不做静默回退**（ADR-0016 决策 2）：用户点之前绝不改来源；点了就是显式选择。
const switching = ref(false);

const MISSING_BUNDLE_KINDS = new Set([
  "missing_package",
  "missing_class",
  "missing_file",
  "missing_font",
]);

/** 这条错误是否属于"可能被内置精简集覆盖不全"的缺文件类。 */
function isMissingFromBundle(e: ErrorEntry): boolean {
  return e.diagnosis?.kind ? MISSING_BUNDLE_KINDS.has(e.diagnosis.kind) : false;
}

async function switchToOnlineBundleAndRetry() {
  if (switching.value) return;
  switching.value = true;
  try {
    await settings.update({ bundle: ONLINE_BUNDLE_URL });
    await ipc.compileNow();
  } catch (e) {
    console.error("切换到在线宏包集并重试失败：", e);
  } finally {
    switching.value = false;
  }
}
</script>

<template>
  <div class="error-list">
    <div class="list-head">
      <span class="head-title">问题</span>
      <span class="head-count" v-if="grouped.items.length > 0">{{ grouped.total }}</span>
    </div>
    <div class="list-body">
      <div v-if="grouped.items.length === 0" class="empty">
        <span class="empty-icon" :class="{ idle: emptyIdle }">{{ emptyIcon }}</span>
        <span>{{ emptyText }}</span>
      </div>
      <div
        v-for="(g, i) in grouped.items"
        :key="i"
        class="entry"
        :class="[g.entry.kind, { diagnosed: !!g.entry.diagnosis }]"
        :title="g.entry.message"
        @click="jump(g.entry.file, g.entry.line)"
      >
        <span class="badge">!</span>
        <span class="count" v-if="g.count > 1" :title="`同类错误共 ${g.count} 条，已折叠`">×{{ g.count }}</span>
        <div class="body">
          <div class="row-head">
            <span class="loc" v-if="g.entry.file">{{ g.entry.file }}<template v-if="g.entry.line">:{{ g.entry.line }}</template></span>
            <span class="headline">{{ g.headline }}</span>
          </div>
          <div class="hint" v-if="g.entry.diagnosis?.hint">
            <span class="hint-icon">→</span>{{ g.entry.diagnosis.hint }}
          </div>
          <!-- 超时一键重试（roadmap ㉕）：值来自后端证据化诊断，前端不自己造数字 -->
          <div class="action" v-if="g.entry.diagnosis?.suggested_timeout_secs">
            <button
              class="retry-btn"
              :disabled="retrying"
              @click.stop="raiseTimeoutAndRetry(g.entry.diagnosis!.suggested_timeout_secs!)"
            >
              {{ retrying ? "重试中…" : `提高到 ${g.entry.diagnosis.suggested_timeout_secs} 秒并重试` }}
            </button>
          </div>
          <!-- 缺包补全通道（T16 / ADR-0016 候选 A）：显式切在线完整宏包集后重编，不静默回退 -->
          <div class="action" v-if="isMissingFromBundle(g.entry)">
            <button
              class="retry-btn"
              :disabled="switching"
              title="把宏包集来源显式切到「在线完整宏包集」（需联网）后立即重编"
              @click.stop="switchToOnlineBundleAndRetry"
            >
              {{ switching ? "切换中…" : "切到在线完整宏包集并重试" }}
            </button>
          </div>
        </div>
      </div>
      <div class="more" v-if="grouped.hidden > 0">
        <span>已隐藏 {{ grouped.hidden }} 条重复问题</span>
        <button class="show-all" @click.stop="showAll = true">显示全部</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.error-list { height: 100%; overflow: hidden; display: flex; flex-direction: column; background: var(--card); font-size: 12.5px; }
.list-head {
  display: flex; align-items: center; gap: 8px;
  flex: 0 0 auto;
  height: 32px; padding: 0 14px;
  font-size: 11px; font-weight: 700;
  letter-spacing: 1px;
  color: var(--ink-dim);
  border-bottom: 1.5px solid var(--line-soft);
}
.head-count {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 17px; height: 17px; padding: 0 5px;
  border-radius: 9px;
  background: var(--tint-coral-strong);
  color: var(--danger-ink);
  font-size: 10.5px; font-weight: 700;
  letter-spacing: 0;
}
.list-body { flex: 1 1 auto; overflow: auto; }
.empty {
  display: flex; align-items: center; gap: 9px;
  padding: 16px 18px;
  color: var(--ink-faint);
}
.empty-icon {
  display: inline-flex; align-items: center; justify-content: center;
  width: 17px; height: 17px;
  border-radius: 50%;
  background: rgba(var(--mint-rgb), 0.14);
  color: var(--ok-ink);
  font-size: 10px; font-weight: 700;
}
/* 未编译（A-02）：中性灰圈，别给"已检查通过"的绿色暗示 */
.empty-icon.idle {
  background: var(--card-2);
  color: var(--ink-faint);
}
.entry {
  display: flex; align-items: flex-start; gap: 9px;
  padding: 6px 14px;
  margin: 1px 6px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  border-left: 2.5px solid transparent;
  transition: background 0.12s;
}
.entry:hover { background: var(--card-2); }
.entry.content_error, .entry.io, .entry.timeout { border-left-color: var(--coral); }
/* 有诊断的条目：左侧改用主色，视觉上区分"能解释的"与"只能给原文的" */
.entry.diagnosed { border-left-color: var(--blueberry); }
.body { flex: 1 1 auto; min-width: 0; }
.row-head { display: flex; align-items: baseline; gap: 9px; min-width: 0; }
.headline { color: var(--ink); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
/* 修复建议行：次行缩进对齐，弱化但可读 */
.hint {
  margin-top: 2px;
  color: var(--ink-dim);
  font-size: 11.5px;
  line-height: 1.5;
  /* B2：可行动出路不能被单行截断——完整换行显示（不再 nowrap/省略号） */
  white-space: normal;
  word-break: break-word;
}
.hint-icon { color: var(--blueberry); font-weight: 700; margin-right: 4px; }
/* 一键操作行（roadmap ㉕：超时"提高上限并重试"） */
.action { margin-top: 5px; }
.retry-btn {
  display: inline-flex; align-items: center;
  height: 22px; padding: 0 10px;
  background: var(--tint-blueberry);
  border: 1.5px solid var(--blueberry);
  border-radius: 6px;
  color: var(--blueberry);
  font-size: 11.5px; font-weight: 600;
  cursor: pointer;
  transition: background 0.12s;
}
.retry-btn:hover:not(:disabled) { background: rgba(var(--blueberry-rgb), 0.2); }
.retry-btn:disabled { opacity: 0.5; cursor: default; }
.badge {
  flex: 0 0 auto;
  display: inline-flex; align-items: center; justify-content: center;
  width: 16px; height: 16px;
  border-radius: 5px;
  background: var(--tint-coral-strong);
  color: var(--danger-ink);
  font-size: 11px; font-weight: 800;
}
.count {
  flex: 0 0 auto;
  display: inline-flex; align-items: center;
  padding: 0 5px; height: 15px;
  border-radius: 8px;
  background: var(--tint-mango);
  color: var(--warn-ink-deep);
  font-size: 10.5px; font-weight: 700;
}
.loc { color: var(--blueberry); font-family: var(--mono); flex: 0 0 auto; max-width: 45%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.more {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px 12px;
  color: var(--ink-faint);
  font-size: 11.5px;
}
.show-all {
  padding: 2px 8px;
  border: 1.5px solid var(--line);
  border-radius: 6px;
  background: var(--card);
  color: var(--ink-dim);
  font-size: 11px;
  font-family: inherit;
  cursor: pointer;
}
.show-all:hover { border-color: var(--blueberry); color: var(--blueberry); }
</style>
