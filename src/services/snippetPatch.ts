// 片段定位（草稿层真实排版实验 (A)，docs/research/snippet-preview.md）：从"改动前/改动后的缓冲"
// 找出**改动所在的那一段**，交给后端单独编译。
//
// 只放**纯函数**（可单测）：改动行定位 + 段落切分。触发时机（去抖、编译中跳过）与 IPC 在组件里。

/** 首个不同的行号（0-based）；两份完全一致时返回 -1。 */
export function firstChangedLine(prev: string, next: string): number {
  const a = prev.split("\n");
  const b = next.split("\n");
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) if (a[i] !== b[i]) return i;
  return a.length === b.length ? -1 : n;
}

/**
 * 改动行所在的**段落**：LaTeX 的段落以空行分隔 ⇒ 上下各撑到空行边界。
 *
 * 锚点本身是空行时返回 `null`：用户刚敲回车、还没输入内容，此刻没有"新的段落"可排
 * （若不返回 null 就会向上穿过空行、把整块前文甚至导言区当成一段）。
 *
 * 超过 `maxLines` 时取改动行居中的一段（免得把整章塞进去编译）。
 */
export function paragraphAt(text: string, line: number, maxLines = 60): string | null {
  const lines = text.split("\n");
  if (line < 0 || line >= lines.length) return null;
  if (lines[line].trim() === "") return null;
  let start = line;
  let end = line;
  while (start > 0 && lines[start - 1].trim() !== "") start--;
  while (end < lines.length - 1 && lines[end + 1].trim() !== "") end++;
  if (end - start + 1 > maxLines) {
    const half = Math.floor(maxLines / 2);
    start = Math.max(start, line - half);
    end = Math.min(end, start + maxLines - 1);
  }
  const body = lines.slice(start, end + 1).join("\n").trim();
  return body.length > 0 ? body : null;
}

/** 一次改动 → 该编译的片段（`null` = 没改动 / 空段落 / 越界）。 */
export function snippetForChange(prev: string, next: string, maxLines = 60): string | null {
  const line = firstChangedLine(prev, next);
  if (line < 0) return null;
  return paragraphAt(next, line, maxLines);
}
