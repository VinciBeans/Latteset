// 错误取人话（roadmap ㊿：前端**唯一一份**）。
//
// 后端命令的错误契约是 `CmdError = { code, message }`（modules.md §4），但 rejection 也可能是
// 字符串、`Error`、甚至任意形状。此前有三份不等价实现（stores/editor、FileTree、App.vue 内联），
// 结果同一类失败在不同位置显示成 `[object Object]` 或空串 —— 归一到这一个入口。

/** 任意 rejection → 可显示的一句话。 */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && e && "message" in e) {
    const m = (e as { message: unknown }).message;
    return typeof m === "string" ? m : String(m);
  }
  return String(e);
}
