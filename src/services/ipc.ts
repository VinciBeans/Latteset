// 服务层（modules.md §9.1）：唯一碰 IPC 的层。
// 命令类型由 tauri-specta 自动生成（src/bindings.ts），本文件只做结果解包。

import { commands, type CmdError, type FileContent, type UiTheme } from "../bindings";

type Result<T> = Promise<{ status: "ok"; data: T } | { status: "error"; error: CmdError }>;

/** 解包 specta 的 typedError 结果：错误直接抛出（调用方按 CmdError 处理）。 */
async function unwrap<T>(r: Result<T>): Promise<T> {
  const res = await r;
  if (res.status === "error") throw res.error;
  return res.data;
}

/**
 * UTF-16 码元偏移 → UTF-8 字节偏移（公式预览用；见 `compileMath` 的口径说明）。
 *
 * 用 `TextEncoder` 把光标之前的那一段按 UTF-8 编一遍取长度 —— 452 KB 的文档约几毫秒，
 * 相对一次片段编译（170 ms 起）可以忽略。
 */
function utf16ToByteOffset(text: string, utf16Offset: number): number {
  const clamped = Math.max(0, Math.min(utf16Offset, text.length));
  return new TextEncoder().encode(text.slice(0, clamped)).length;
}

export const ipc = {
  openProject: (folder: string) => unwrap(commands.openProject(folder)),
  /**
   * 公式预览（roadmap ㊸ 切片 3）：把光标所在的公式单独编译成单页 PDF。
   *
   * ⚠ **偏移口径**：Monaco 给的是 **UTF-16 码元**偏移，而后端 `math_at` 按 **UTF-8 字节**偏移解释；
   * 一个汉字是 1 个码元 / 3 个字节 ⇒ 公式**前面**有中文时两者不等（中文文档里必然发生）。
   * 所以在这里统一换算，调用方只管把 `model.getOffsetAt(position)` 原样传进来。
   */
  compileMath: (text: string, utf16Offset: number) =>
    unwrap(commands.compileMath(text, utf16ToByteOffset(text, utf16Offset))),
  getProject: () => unwrap(commands.getProject()),
  listDir: (path: string) => unwrap(commands.listDir(path)),
  readFile: (path: string) => unwrap(commands.readFile(path)),
  saveAll: (files: { path: string; content: string }[]) => unwrap(commands.saveAll(files)),
  // 大纲（roadmap ⑦a 起为增量语义）：buffers 只传**变化过**的缓冲，openPaths 传当前打开的标签
  // （后端据此淘汰已关闭文件的缓冲；未变化的文件按内容指纹复用扫描结果）。
  getOutline: (buffers: FileContent[], files: string[] | null, openPaths: string[]) =>
    unwrap(commands.getOutline(buffers, files, openPaths)),
  compileNow: () => unwrap(commands.compileNow()),
  abortCompile: () => unwrap(commands.abortCompile()),
  synctexForward: (file: string, line: number, column: number) =>
    unwrap(commands.synctexForward(file, line, column)),
  synctexInverse: (page: number, x: number, y: number) =>
    unwrap(commands.synctexInverse(page, x, y)),
  getSettings: () => unwrap(commands.getSettings()),
  updateSettings: (patch: Parameters<typeof commands.updateSettings>[0]) =>
    unwrap(commands.updateSettings(patch)),
  /**
   * 本次构建是否编入了 Tectonic 库形态（设置面据此禁用选项，而不是让用户选了才炸）。
   *
   * 注意：该命令直接返回 `bool`（不是 `Result`），所以**不走 `unwrap`** —— specta 只为
   * `Result<_, CmdError>` 的命令生成 `typedError` 包装。
   */
  libFormAvailable: () => commands.libFormAvailable(),
  /**
   * 当前**实际**会用的引擎形态（状态栏显示用）。判定在后端做一次（与 runner 共用同一个纯函数）。
   * 前端只渲染：形态不能从 settings 推，环境变量能压过它。
   */
  engineForm: () => unwrap(commands.engineForm()),
  /**
   * 把**原生标题栏**也纳入主题（Windows 走 DWM 沉浸式深色）。
   *
   * 传**设置值**（`light|dark|system`）而不是解析后的深浅：`system` 交给系统跟。
   * 窗口首次出现那一下由后端在 `setup` 里直接从磁盘设置定色，所以这里失败不影响可用性。
   */
  setWindowTheme: (theme: UiTheme) => unwrap(commands.setWindowTheme(theme)),
};
