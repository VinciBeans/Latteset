<!-- 应用壳（modules.md §3）：三栏布局 + 底部错误列表 + 状态栏，自研 splitter。 -->
<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import SplitPane from "./components/SplitPane.vue";
import FileTree from "./components/FileTree.vue";
import TabBar from "./components/TabBar.vue";
import EditorPane from "./components/EditorPane.vue";
import PreviewPane from "./components/PreviewPane.vue";
import ErrorList from "./components/ErrorList.vue";
import OutlinePane from "./components/OutlinePane.vue";
import StatusBar from "./components/StatusBar.vue";
import { useProjectStore } from "./stores/project";
import { useEditorStore } from "./stores/editor";
import { useOutlineStore } from "./stores/outline";
import { useSettingsStore } from "./stores/settings";
import { useCompileStore } from "./stores/compile";
import { useAutoSave } from "./composables/useAutoSave";
import { useIdleConvergence } from "./composables/useIdleConvergence";
import { ipc } from "./services/ipc";
import { subscribeEvents } from "./services/events";
import { useTheme } from "./composables/useTheme";
import SettingsPanel from "./components/SettingsPanel.vue";
import RootFilePicker from "./components/RootFilePicker.vue";
import type { ProjectInfo } from "./bindings";

const project = useProjectStore();
const editor = useEditorStore();
const settings = useSettingsStore();
const compile = useCompileStore();
const outline = useOutlineStore();
const autoSave = useAutoSave();
// 空闲收敛（roadmap ㉘）：草稿编译后停手 2s 补一次完整 latexmk，追上目录/引用页码
const idleConvergence = useIdleConvergence();
// 主题（roadmap ⑩）：设置 → `<html data-theme>` + Monaco。首帧那份由 index.html 的内联脚本给，
// 设置读回来后再 sync 一次覆盖（首帧的来源可能过期）。
const theme = useTheme();

const cursorLine = ref(0);
const cursorCol = ref(0);

/** 底部报告面板是否折叠（默认折叠，回收竖向空间给主编辑/预览区；点报告头展开）。 */
const bottomCollapsed = ref(true);
/**
 * 折叠头部摘要：**只在编译失败时**显示错误条数。
 *
 * 就绪 / 排版中… 不在这里出现——状态栏 phase chip 读的是同一个 compile store，
 * 两处一起渲染就是重复的「就绪」。折叠细条上独有的信息只有错误条数。
 */
const bottomStatus = computed(() =>
  compile.hasError ? { text: `${compile.errors.length} 个错误` } : null,
);

let unsubscribe: (() => void) | null = null;
let removeKeydown: (() => void) | null = null;

onMounted(async () => {
  unsubscribe = subscribeEvents();
  // 设置加载失败不应阻塞应用初始化（否则会成未处理 rejection 并跳过环境自动打开）。
  try {
    await settings.init();
  } catch (e) {
    console.error("加载设置失败：", e);
  }
  // 设置到手后再落一次主题：首帧那份来自 localStorage（可能过期，或本机首次运行尚无记录）。
  // 加载失败时也调用——此时 theme 落到「跟随系统」，比停在过期值更接近用户预期。
  theme.sync();
  // Ctrl+S / Cmd+S：立即保存全部脏文件（on_save 模式的「保存触发」；连续模式也立即落盘一次）。
  const onKeyDown = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      autoSave.flush().catch((err) => console.error("保存失败：", err));
    }
  };
  window.addEventListener("keydown", onKeyDown);
  removeKeydown = () => window.removeEventListener("keydown", onKeyDown);
  // 测试/开发钩子：设置 VITE_LATTESET_PROJECT 目录则自动打开项目，绕过原生目录弹窗
  // （原生弹窗 WebDriver 无法驱动），便于端到端测试。生产不设置，行为不变。
  const envProject = import.meta.env.VITE_LATTESET_PROJECT as string | undefined;
  if (envProject) {
    try {
      const info = await openProjectInto(envProject);
      maybeOpenRootPicker(info);
    } catch (e) {
      console.error("自动打开项目失败：", e);
    }
  }
});

onBeforeUnmount(() => {
  unsubscribe?.();
  removeKeydown?.();
});

/** 打开项目公共流程：openProject → 打开根文件 → 重建大纲。返回 info 供调用方按需提示。 */
async function openProjectInto(dir: string): Promise<ProjectInfo> {
  const info = await project.openProject(dir);
  if (info.root_file) await editor.openFile(info.root_file);
  await outline.refresh();
  return info;
}

// ---- 根文件选择器（roadmap P0-②-1）----
// 打开项目后未探测到唯一根文件时，用弹窗让用户选；取代此前的 console.warn
// （用户看到的是「打开项目没反应」）。候选列表来自后端 ProjectInfo.root_candidates。
const rootPickerOpen = ref(false);
const rootPickerBusy = ref(false);
const rootPickerError = ref("");

/** 零候选时的兜底列表：项目内全部 .tex（与大纲兜底同源：文件树已过滤 tmp/ 与隐藏项）。 */
const rootPickerFallback = computed(() =>
  project.tree.filter((e) => !e.is_dir && /\.tex$/i.test(e.name)).map((e) => e.path)
);

function openRootPicker() {
  if (!project.project) return;
  rootPickerError.value = "";
  rootPickerBusy.value = false;
  rootPickerOpen.value = true;
}

/**
 * 打开项目后按需弹根文件选择器（roadmap P0-②-1 + ㊺ §6.13.1-B）。
 *
 * **项目里一个 `.tex` 都没有时不弹**：那种弹窗没有可选项（只有一句"项目内没有 .tex 文件"），
 * 而"空目录"恰恰是新用户的第一条路 —— 一进去先被一个模态挡住，正是本项要消掉的打扰。
 * 此时改由状态栏那条「未确定根文件 · 点击选择」常驻提示（点了仍能打开本选择器），
 * 用户也可以直接用文件树的 `＋` 新建第一份文档。
 */
function maybeOpenRootPicker(info: ProjectInfo) {
  if (info.root_file) return;
  if (!rootPickerFallback.value.length) return;
  openRootPicker();
}

/** 选择候选 → 写项目覆盖 → 同步内存项目状态 → 打开该文件 → 刷新大纲。
 *  顺序关键：后端 update_settings 现在会同步 `ProjectState.root_file`（此前不同步，
 *  表现为「选完仍报未确定根文件」），故选择后必须重新 get_project 再打开文件。 */
async function onRootPicked(relPath: string) {
  rootPickerBusy.value = true;
  rootPickerError.value = "";
  try {
    await settings.update({ root_file: relPath });
    const info = await project.syncProject();
    if (!info.root_file) {
      // 后端解析出的绝对路径为空 → 覆盖未生效（路径非法等），保留弹窗让用户改选
      rootPickerError.value = `无法使用该路径作为根文件：${relPath}`;
      return;
    }
    await editor.openFile(info.root_file);
    await outline.refresh();
    rootPickerOpen.value = false;
  } catch (e) {
    const msg = typeof e === "object" && e && "message" in e ? String((e as { message: unknown }).message) : String(e);
    rootPickerError.value = `设置根文件失败：${msg}`;
    console.error("设置根文件失败：", e);
  } finally {
    rootPickerBusy.value = false;
  }
}

/** 打开项目（dialog 选文件夹）。 */
async function chooseProject() {
  const dir = await open({ directory: true, title: "打开 TeX 项目文件夹" });
  if (!dir) return;
  try {
    const info = await openProjectInto(dir);
    maybeOpenRootPicker(info);
  } catch (e) {
    console.error("打开项目失败：", e);
  }
}

function onEditorChange(_path: string) {
  // 用户又动了键盘 → 取消待收敛的完整编译，把调度器让给编辑态的 Quick 单趟
  idleConvergence.cancel();
  // 连续模式：编辑触发防抖自动保存；on_save 模式不自动写盘（由 Ctrl+S / 点「编译」/ 关标签触发）。
  if (settings.settings?.compile.mode === "continuous") autoSave.schedule();
}

async function manualCompile() {
  try {
    // 先把未落盘的脏文件写盘（on_save 模式下不自动保存，点「编译」需先落盘），
    // 后端 watch 会响应写盘触发编译；compile_now 再强制入队（合并队列吸收重复）。
    await autoSave.flush();
    await ipc.compileNow();
  } catch (e) {
    console.error("手动编译失败：", e);
  }
}

async function abort() {
  await ipc.abortCompile();
}

const isRunning = () => compile.phase === "running" || compile.phase === "queued";

const settingsOpen = ref(false);
</script>

<template>
  <div class="app">
    <div class="toolbar">
      <div class="brand">
        <span class="brand-mark" aria-hidden="true"><img src="/logo.svg" width="22" height="22" alt="" /></span>
        <span class="brand-name">Latteset</span>
      </div>
      <div class="toolbar-actions">
        <button class="btn" @click="chooseProject">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none"><path d="M1.5 3.5A1.5 1.5 0 0 1 3 2h2.6l1.4 1.5H13a1.5 1.5 0 0 1 1.5 1.5v7A1.5 1.5 0 0 1 13 13.5H3A1.5 1.5 0 0 1 1.5 12v-8.5Z" stroke="currentColor" stroke-width="1.3"/></svg>
          <span>打开项目</span>
        </button>
        <button
          class="btn primary"
          :class="{ typesetting: isRunning() }"
          :disabled="isRunning()"
          @click="manualCompile"
        >
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M4.5 2.8a1 1 0 0 1 1.53-.85l8 5.2a1 1 0 0 1 0 1.7l-8 5.2a1 1 0 0 1-1.53-.85V2.8Z"/></svg>
          <span>{{ isRunning() ? "排版中…" : "编译" }}</span>
        </button>
        <button class="btn ghost" :disabled="!isRunning()" @click="abort">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><rect x="3" y="3" width="10" height="10" rx="1.5"/></svg>
          <span>终止</span>
        </button>
        <button class="btn icon" title="设置" @click="settingsOpen = true">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z"/></svg>
        </button>
      </div>
      <span class="title" :title="project.project?.root ?? ''">
        {{ project.project ? project.project.root : "" }}
      </span>
      <span class="title-fill" />
    </div>

    <div class="main">
      <SplitPane direction="vertical" :initial="0.15">
        <template #primary>
          <!-- 左栏：文件树 上 | 大纲 下（上下分布，改善观感） -->
          <SplitPane direction="horizontal" :initial="0.55">
            <template #primary>
              <FileTree />
            </template>
            <template #secondary>
              <OutlinePane />
            </template>
          </SplitPane>
        </template>
        <template #secondary>
          <!-- 编辑器 | PDF 预览：左右排布（vertical = 竖向分隔条），1:1 -->
          <SplitPane direction="vertical" :initial="0.5">
            <template #primary>
              <div class="editor-area">
                <TabBar />
                <EditorPane
                  @change="onEditorChange"
                  @cursor="(l: number, c: number) => { cursorLine = l; cursorCol = c }"
                />
              </div>
            </template>
            <template #secondary>
              <PreviewPane />
            </template>
          </SplitPane>
        </template>
      </SplitPane>
    </div>

    <div class="bottom" :class="{ collapsed: bottomCollapsed }">
      <!-- 折叠头：状态摘要 + 折叠/展开 -->
      <button
        class="bottom-toggle"
        :title="bottomCollapsed ? '展开报告' : '收起报告'"
        @click="bottomCollapsed = !bottomCollapsed"
      >
        <span class="caret">{{ bottomCollapsed ? "▸" : "▾" }}</span>
        <span class="toggle-label">报告</span>
        <span v-if="bottomStatus" class="toggle-status">
          <span class="dot" />{{ bottomStatus.text }}
        </span>
        <span class="spacer" />
        <span class="toggle-hint">{{ bottomCollapsed ? "展开" : "收起" }}</span>
      </button>
      <!-- 内容（折叠时隐藏）：错误列表（大纲已移至左侧栏） -->
      <div class="bottom-content" v-show="!bottomCollapsed">
        <div class="error-area"><ErrorList /></div>
      </div>
    </div>

    <StatusBar :cursor-line="cursorLine" :cursor-col="cursorCol" @pick-root="openRootPicker" />

    <SettingsPanel v-if="settingsOpen" @close="settingsOpen = false" />

    <RootFilePicker
      v-if="rootPickerOpen && project.project"
      :root="project.project.root"
      :candidates="project.project.root_candidates"
      :fallback-files="rootPickerFallback"
      :busy="rootPickerBusy"
      :error="rootPickerError"
      @select="onRootPicked"
      @close="rootPickerOpen = false"
    />
  </div>
</template>

<style>
/* ============ 设计系统：Candy Desk（young & lively） ============ */
:root {
  --paper: #f4f2fb;        /* 淡紫罗兰纸面 */
  --card: #ffffff;         /* 卡片 */
  --card-2: #edeaf8;       /* 悬停/次级面 */
  --ink: #2b2438;          /* 主墨色 */
  --ink-dim: #7a7490;      /* 次级 */
  --ink-faint: #b0aac6;    /* 三级 */
  --line: #ded9ee;         /* 边框 */
  --line-soft: #eae6f5;    /* 弱边框 */
  --blueberry: #5d5fef;    /* 主色 */
  --blueberry-deep: #4a4cd8;
  --coral: #ff7a6e;        /* 强调/错误 */
  --mango: #ffb54a;        /* 警告/脏 */
  --mint: #2fbf8f;         /* 成功 */
  --radius: 12px;
  --radius-sm: 8px;
  --shadow-hard: 3px 3px 0 rgba(var(--shadow-rgb), 0.09);
  --shadow-hard-big: 5px 5px 0 rgba(var(--shadow-rgb), 0.10);
  --mono: "Cascadia Mono", "JetBrains Mono", Consolas, "Courier New", monospace;

  /* ============ 语义层（2026-09-15，为深色主题引入）============
     两层：**通道变量** + **角色 token**。
     - 通道变量（`--X-rgb`）：站点自己拼 alpha（`rgba(var(--blueberry-rgb), 0.10)`）
       ⇒ 浅色下每个 alpha 逐字保留，深色只改一处通道值就整片生效。
     - 角色 token：给"强调色当文本/描边"（对表面要有对比度，深浅两态必须换值）与实色用。
     下面这些**浅色值 = 重构前的字面量，逐字未改** ⇒ 浅色渲染零变化。 */
  --blueberry-rgb: 93, 95, 239;
  --coral-rgb: 255, 122, 110;
  --mango-rgb: 255, 181, 74;
  --mint-rgb: 47, 191, 143;
  --violet-rgb: 124, 58, 237;
  --rose-rgb: 209, 84, 126;
  --sage-rgb: 127, 151, 126;
  --ink-rgb: 43, 36, 56;
  --scrim-rgb: 30, 26, 46;
  --shadow-rgb: 43, 36, 56;   /* 硬阴影专用：深色下要翻成黑，不能跟着「墨」变亮 */

  --blueberry-hover: #6a5cff;
  --violet: #7c3aed;
  --info: #4e9bff;
  --rose: #d1547e;
  --danger-ink: #e85f52;   /* 错误文字：浅色压暗、深色提亮 */
  --warn-ink: #b8791a;
  --warn-ink-alt: #e09a2e;
  --warn-ink-deep: #d98d18;
  --warn-line: #e8a72c;
  --ok-ink: #23a377;
  --rose-ink: #c84e74;
  --sage-ink: #5f7a5e;
  --sienna: #b26a2b;

  --tint-blueberry: rgba(var(--blueberry-rgb), 0.10);
  --tint-blueberry-weak: rgba(var(--blueberry-rgb), 0.08);
  --tint-blueberry-strong: rgba(var(--blueberry-rgb), 0.12);
  --tint-mango: rgba(var(--mango-rgb), 0.18);
  --tint-mango-weak: rgba(var(--mango-rgb), 0.16);
  --tint-coral: rgba(var(--coral-rgb), 0.14);
  --tint-coral-strong: rgba(var(--coral-rgb), 0.15);
  --tint-coral-weak: rgba(var(--coral-rgb), 0.13);
  --tint-violet: rgba(var(--violet-rgb), 0.12);
  --tint-ink: rgba(var(--ink-rgb), 0.10);
  --tint-ink-weak: rgba(var(--ink-rgb), 0.06);
  --scrim: rgba(var(--scrim-rgb), 0.38);
  --scrim-weak: rgba(var(--ink-rgb), 0.28);
  --select: #dcd7f6;
  --line-strong: #c8c0e8;
  --hover-row: #eeeafd;
  --danger-tint-soft: #ffe9e5;
  --scroll-thumb: #c9c2e4;
  --scroll-thumb-hover: #b3aad6;
  /* 实底强调色上的文字（主按钮 / 选中的分段按钮）。
     浅色：白字（底是深蓝莓）；深色：**翻成墨色**（底提亮成 #8a8cff，白字只有 2.4:1）——
     这是深色主题唯一必须"反转"的一对，实测见 design.md 的主题一节。 */
  --on-accent: #ffffff;
}

/* ============ Candy Desk · 夜间态（2026-09-15）============
   设计取向：**不是"黑底 + 荧光色"，而是同一张糖果桌在台灯下**。
   四条规则（改这里时别破坏它们）：
   1. **色相不能丢**：纸面用墨紫罗兰（与浅色 `#f4f2fb` 同色系），不用中性灰/纯黑；
   2. **抬升方向不变**：浅色下卡片比纸面**亮**（白 vs 淡紫）⇒ 深色下卡片仍比纸面亮一档，
      只是整体压到暗部（"纸"→"墨"，关系没变）；
   3. **糖果色提亮**：强调色在深底上要"发光"；当文本用的 `*-ink` 一律换成亮版，
      半透明底走**通道变量**（`--X-rgb`）⇒ 全站 alpha 不用逐个改；
   4. **硬阴影翻成暗投影**：浅色下 3px 硬偏移是"纸片翘起"，深色下若还取亮色会变成光晕（语义反了）
      ⇒ `--shadow-rgb` 换成黑。硬边（无模糊）这个签名保留。
   PDF 预览**不做反色**：页面保持纸白（阅读保真优先，图片/图表反色会失真），只把周边 chrome 压暗。 */
[data-theme="dark"] {
  --paper: #16131f;        /* 墨紫罗兰（纸面） */
  --card: #1f1b2c;         /* 卡片：比纸面亮一档 = 抬升 */
  --card-2: #292337;       /* 悬停/次级面 */
  --ink: #ece9f7;          /* 主墨色 → 亮紫白 */
  --ink-dim: #a49dbb;
  --ink-faint: #87809d;    /* 三级：抬到 ≥4.4:1（原 #6f6885 只有 3.2:1，提示文字偏糊） */
  --line: #372f49;
  --line-soft: #2a2438;

  /* 通道：整体提亮（深底上要发光），alpha 仍由各站点自己决定 */
  --blueberry-rgb: 138, 140, 255;
  --coral-rgb: 255, 138, 128;
  --mango-rgb: 255, 199, 107;
  --mint-rgb: 77, 214, 166;
  --violet-rgb: 167, 122, 255;
  --rose-rgb: 240, 130, 170;
  --sage-rgb: 160, 186, 158;
  --ink-rgb: 236, 233, 247;
  --scrim-rgb: 6, 4, 12;   /* 遮罩：深色下要更实才压得住 */
  --shadow-rgb: 0, 0, 0;   /* 硬阴影：翻成暗投影 */

  --blueberry: #8a8cff;
  --blueberry-deep: #a5a6ff;   /* 深色下"更深"= 更亮（它被当文本色用） */
  --blueberry-hover: #a5a6ff;
  --coral: #ff8a80;
  --mango: #ffc76b;
  --mint: #4dd6a6;
  --violet: #a77aff;
  --info: #6db2ff;
  --rose: #f082aa;
  --danger-ink: #ff9b90;
  --warn-ink: #ffc76b;
  --warn-ink-alt: #ffc76b;
  --warn-ink-deep: #f0b45a;
  --warn-line: #ffc76b;
  --ok-ink: #4dd6a6;
  --rose-ink: #f082aa;
  --sage-ink: #a0ba9e;
  --sienna: #e0a06a;

  --select: #3b3a72;             /* 选中底：蓝莓调的深色（浅色下是淡紫） */
  --line-strong: #4a4260;
  --hover-row: #262038;
  --danger-tint-soft: #3a2530;
  --scroll-thumb: #3c3550;
  --scroll-thumb-hover: #4d4566;
  /* 深色下实底强调色是**亮**的 ⇒ 上面的文字必须翻成墨色（白字只有 2.4:1，实测） */
  --on-accent: #16131f;
}

html, body, #app { height: 100%; margin: 0; }body {
  font-family: "Segoe UI", "Microsoft YaHei", "PingFang SC", sans-serif;
  background: var(--paper);
  color: var(--ink);
  font-size: 13px;
  -webkit-font-smoothing: antialiased;
}

::selection { background: var(--select); }

:focus-visible { outline: 2px solid var(--blueberry); outline-offset: 2px; border-radius: 4px; }

::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb {
  background: var(--scroll-thumb);
  border-radius: 6px;
  border: 2px solid transparent;
  background-clip: content-box;
}
::-webkit-scrollbar-thumb:hover { background-color: var(--scroll-thumb-hover); }
::-webkit-scrollbar-corner { background: transparent; }
</style>

<style scoped>
.app { display: flex; flex-direction: column; height: 100%; }

/* ---- 工具栏 ---- */
.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  height: 46px;
  padding: 0 14px;
  background: var(--card);
  border-bottom: 1.5px solid var(--line);
  flex: 0 0 auto;
}
.brand { display: flex; align-items: center; gap: 8px; flex: 0 0 auto; }
.brand-mark {
  display: inline-flex; align-items: center; justify-content: center;
  width: 22px; height: 22px;
}
.brand-mark img { display: block; }
.brand-name {
  font-weight: 800; font-size: 14.5px; letter-spacing: -0.2px;
  color: var(--ink);
}

.toolbar-actions { display: flex; align-items: center; gap: 8px; flex: 0 0 auto; }

.btn {
  display: inline-flex; align-items: center; gap: 6px;
  height: 30px; padding: 0 13px;
  background: var(--card);
  border: 1.5px solid var(--line);
  border-radius: var(--radius-sm);
  color: var(--ink);
  font-size: 12.5px; font-weight: 550;
  cursor: pointer;
  box-shadow: var(--shadow-hard);
  transition: transform 0.1s, box-shadow 0.1s, border-color 0.12s, background 0.12s;
}
.btn:hover:not(:disabled) {
  transform: translate(-1px, -1px);
  box-shadow: var(--shadow-hard-big);
  border-color: var(--line-strong);
}
.btn:active:not(:disabled) { transform: translate(1px, 1px); box-shadow: 1px 1px 0 rgba(var(--shadow-rgb), 0.08); }
.btn.primary {
  background: linear-gradient(135deg, var(--blueberry-hover) 0%, var(--blueberry) 50%, var(--info) 120%);
  border-color: transparent;
  color: var(--on-accent);
  box-shadow: 2.5px 2.5px 0 rgba(var(--blueberry-rgb), 0.28);
}
.btn.primary:hover:not(:disabled) { border-color: transparent; box-shadow: 4px 4px 0 rgba(var(--blueberry-rgb), 0.30); }
.btn.primary.typesetting {
  background: linear-gradient(120deg, var(--blueberry-hover), #5d5fef, var(--info), var(--blueberry-hover));
  background-size: 260% 100%;
  animation: typeset-flow 1.1s linear infinite;
}
@keyframes typeset-flow { to { background-position: -260% 0; } }
.btn.ghost {
  background: transparent;
  border: 1.5px dashed var(--line-strong);
  box-shadow: none;
}
.btn.icon { padding: 0 9px; }
.btn.icon:hover:not(:disabled) { border-color: var(--blueberry); color: var(--blueberry); }
.btn.ghost:hover:not(:disabled) { transform: none; box-shadow: none; border-color: var(--coral); color: var(--coral); }
.btn:disabled { opacity: 0.45; cursor: default; box-shadow: none; }

.title {
  flex: 0 1 auto; min-width: 0;
  font-size: 12px; color: var(--ink-faint);
  font-family: var(--mono);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.title-fill { flex: 1 1 auto; }

/* ---- 主体 ---- */
.main { flex: 1 1 auto; min-height: 0; }
.bottom {
  flex: 0 0 140px; min-height: 0; border-top: 1.5px solid var(--line);
  display: flex; flex-direction: column;
}
.bottom.collapsed { flex: 0 0 30px; }
.bottom-content { flex: 1 1 auto; min-height: 0; overflow: hidden; }
.bottom-toggle {
  display: flex; align-items: center; gap: 8px;
  flex: 0 0 auto; height: 30px; padding: 0 12px;
  background: var(--card); border: none; border-bottom: 1.5px solid var(--line-soft);
  color: var(--ink-dim); font-size: 11.5px; font-weight: 700; letter-spacing: 1px;
  cursor: pointer; text-align: left; width: 100%;
}
.bottom-toggle:hover { background: var(--card-2); }
.bottom-toggle .caret { font-size: 11px; color: var(--ink-faint); }
/* 折叠头部摘要：只在失败时出现（就绪/排版中… 由状态栏 phase chip 承担，不重复渲染） */
.bottom-toggle .toggle-status {
  display: inline-flex; align-items: center; gap: 5px;
  font-size: 11.5px; font-weight: 600; letter-spacing: 0;
  color: var(--danger-ink);
}
.bottom-toggle .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--coral); }
.bottom-toggle .spacer { flex: 1 1 auto; }
.bottom-toggle .toggle-hint { color: var(--ink-faint); font-size: 11px; font-weight: 500; letter-spacing: 0.5px; }
.editor-area { display: flex; flex-direction: column; height: 100%; background: var(--card); }
.editor-area > :last-child { flex: 1; min-height: 0; }
.error-area { height: 100%; }

@media (prefers-reduced-motion: reduce) {
  .btn.primary.typesetting { animation: none; }
}
</style>
