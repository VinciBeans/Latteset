// editorStore（modules.md §9.2）：打开文件、脏标志、活动标签、外部修改处理。
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { ipc } from "../services/ipc";
import { useProjectStore } from "./project";

export interface OpenFile {
  path: string; // 项目内绝对路径
  name: string;
}

export const useEditorStore = defineStore("editor", () => {
  const tabs = ref<OpenFile[]>([]);
  const activePath = ref<string | null>(null);
  /** 脏缓冲（内容已在内存，未落盘）。 */
  const dirty = ref<Set<string>>(new Set());
  /** 最新内容（EditorPane 变更时写入；自动保存时读出）。 */
  const buffers = ref<Map<string, string>>(new Map());
  /** 自保存过滤（modules.md §5.5）：保存时刻记录，files-changed 里近期的路径视为自己写的。 */
  const lastSaved = ref<Map<string, number>>(new Map());
  /** 外部修改冲突提示（打开且脏 → 保留本地）。 */
  const externalConflict = ref<Set<string>>(new Set());
  /**
   * 打开文件的失败提示（roadmap ㉓）：非 UTF-8 源文件、路径不可读等。
   * 此前 openFile 的 rejection 无人接（文件树点击没有 await/catch）→ 用户看到的是"点了没反应"。
   */
  const openError = ref<string | null>(null);

  const project = useProjectStore();
  const activeTab = computed(() => tabs.value.find((t) => t.path === activePath.value) ?? null);

  async function openFile(rawPath: string, revealLine?: number) {
    const path = project.resolvePath(rawPath);
    if (!tabs.value.some((t) => t.path === path)) {
      let content: string;
      try {
        content = await ipc.readFile(path);
      } catch (e) {
        openError.value = cmdErrorMessage(e);
        console.error("打开文件失败：", e);
        return;
      }
      // 去重需在 await 后复检：并发的 openFile（如树节点快速双击）都在 await 前通过了 some 判断，
      // 此处复检避免重复开标签（复检后的 push 是同步的，不会再有并发窗口）。
      if (!tabs.value.some((t) => t.path === path)) {
        tabs.value.push({ path, name: path.split(/[/\\]/).pop() ?? path });
        buffers.value.set(path, content);
      }
      openError.value = null;
    }
    activePath.value = path;
    if (revealLine) {
      // 让 EditorPane 感知定位请求
      pendingReveal.value = { path, line: revealLine };
    }
  }

  /** CmdError（`{code, message}`）取人话；未知形状退化为文本。 */
  function cmdErrorMessage(e: unknown): string {
    if (typeof e === "object" && e && "message" in e) return String((e as { message: unknown }).message);
    return String(e);
  }

  function closeTab(path: string) {
    const i = tabs.value.findIndex((t) => t.path === path);
    if (i < 0) return;
    tabs.value.splice(i, 1);
    dirty.value.delete(path);
    buffers.value.delete(path);
    if (activePath.value === path) {
      activePath.value = tabs.value[i]?.path ?? tabs.value[i - 1]?.path ?? null;
    }
  }

  function markDirty(path: string, content: string) {
    buffers.value.set(path, content);
    dirty.value.add(path);
    externalConflict.value.delete(path);
  }

  /** 自动保存成功后调用：清脏 + 记录时间（自保存过滤）。 */
  function markSaved(paths: string[]) {
    const now = Date.now();
    for (const p of paths) {
      dirty.value.delete(p);
      lastSaved.value.set(p, now);
    }
  }

  /** 乐观记录自保存（修竞态：后端写盘 → notify → files-changed 可能先于 saveAll 返回到达，
   *  此时若 lastSaved 未记录会误判外部修改）。失败时回滚。 */
  function markSaving(paths: string[]) {
    const now = Date.now();
    for (const p of paths) lastSaved.value.set(p, now);
  }

  function rollbackSaving(paths: string[]) {
    for (const p of paths) lastSaved.value.delete(p);
  }

  /** 读盘内容（读不到返回 null —— 文件可能已被删除）。 */
  async function readDisk(path: string): Promise<string | null> {
    try {
      return await ipc.readFile(path);
    } catch {
      return null;
    }
  }

  /** files-changed 处理（modules.md §5.5 算法）：
   * 1. 自己刚保存的（<2s）→ 忽略；
   * 2. 打开且不脏 → 静默重载；
   * 3. 打开且脏 → 保留本地 + 冲突标记；
   * 4. 未打开 → 忽略（文件树自己刷新）。
   *
   * ⚠ **脏分支的自保存窗口判据不能"消费一次"**：Windows notify 对**同一次**写盘会投递**多条**事件
   * （实测 2 条、同一毫秒），而它们都早于 `saveAll` 的 promise 续体（`markSaved`）——消费一次会让
   * 第二条撞上"仍脏"并误报「外部修改」，而该提示的动作是 `acceptExternal`（**放弃本地**），
   * 误报后一点击就丢输入。脏分支因此：窗口内忽略，窗口外**再比一次磁盘内容**。 */
  async function onFilesChanged(paths: string[]) {
    const now = Date.now();
    for (const raw of paths) {
      const path = project.resolvePath(raw);
      if (!tabs.value.some((t) => t.path === path)) continue;
      const savedAt = lastSaved.value.get(path);
      const withinSelfSave = savedAt !== undefined && now - savedAt < 2000;
      if (dirty.value.has(path)) {
        // 自己刚写盘（含同一次写盘的第 2..n 条事件）：交给下一次保存收敛，不是"外部修改"。
        if (withinSelfSave) continue;
        // 窗口之外再比对一次：内容一致 ⇒ 不是冲突（不靠时间判）。
        const disk = await readDisk(path);
        if (disk !== null && disk === buffers.value.get(path)) continue;
        externalConflict.value.add(path);
        continue;
      }
      if (withinSelfSave) {
        lastSaved.value.delete(path);
        continue;
      }
      try {
        const content = await ipc.readFile(path);
        // 复检：await 期间用户可能已输入（markDirty 重写 buffer + dirty），此时磁盘快照会覆盖最新输入。
        // 脏则保留本地缓冲并发冲突标记。
        if (!dirty.value.has(path)) {
          buffers.value.set(path, content);
        } else {
          externalConflict.value.add(path);
        }
      } catch {
        // 文件可能被删除
      }
    }
  }

  /** 外部修改冲突确认：放弃本地、采用磁盘（v1 状态栏提供按钮）。 */
  async function acceptExternal(path: string) {
    const content = await ipc.readFile(path);
    buffers.value.set(path, content);
    dirty.value.delete(path);
    externalConflict.value.delete(path);
  }

  const pendingReveal = ref<{ path: string; line: number } | null>(null);
  function consumeReveal() {
    const r = pendingReveal.value;
    pendingReveal.value = null;
    return r;
  }

  return {
    tabs, activePath, dirty, buffers, lastSaved, externalConflict, openError, activeTab,
    openFile, closeTab, markDirty, markSaved, markSaving, rollbackSaving,
    onFilesChanged, acceptExternal,
    pendingReveal, consumeReveal,
  };
});
