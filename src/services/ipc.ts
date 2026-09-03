// 服务层（modules.md §9.1）：唯一碰 IPC 的层。
// 命令类型由 tauri-specta 自动生成（src/bindings.ts），本文件只做结果解包。

import { commands, type CmdError, type FileContent } from "../bindings";

type Result<T> = Promise<{ status: "ok"; data: T } | { status: "error"; error: CmdError }>;

/** 解包 specta 的 typedError 结果：错误直接抛出（调用方按 CmdError 处理）。 */
async function unwrap<T>(r: Result<T>): Promise<T> {
  const res = await r;
  if (res.status === "error") throw res.error;
  return res.data;
}

export const ipc = {
  openProject: (folder: string) => unwrap(commands.openProject(folder)),
  listDir: (path: string) => unwrap(commands.listDir(path)),
  readFile: (path: string) => unwrap(commands.readFile(path)),
  saveAll: (files: { path: string; content: string }[]) => unwrap(commands.saveAll(files)),
  getOutline: (buffers: FileContent[], files: string[] | null) =>
    unwrap(commands.getOutline(buffers, files)),
  compileNow: () => unwrap(commands.compileNow()),
  abortCompile: () => unwrap(commands.abortCompile()),
  synctexForward: (file: string, line: number, column: number) =>
    unwrap(commands.synctexForward(file, line, column)),
  synctexInverse: (page: number, x: number, y: number) =>
    unwrap(commands.synctexInverse(page, x, y)),
  getSettings: () => unwrap(commands.getSettings()),
  updateSettings: (patch: Parameters<typeof commands.updateSettings>[0]) =>
    unwrap(commands.updateSettings(patch)),
};
