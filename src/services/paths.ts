// 路径工具（roadmap ㊿：前端**唯一一份**）。
//
// 为什么独立出来：分隔符归一化（`\` ↔ `/`）在前端被手写了十几处，而"漏一处"的代价是
// **静默错**——标签指向不存在的文件、改名后的编辑被当成外部修改、根文件覆盖写成绝对路径。
// 全部收敛到这里，纯函数、可单测。
//
// 口径（与后端 `core::project::paths` 保持一致）：
// - **对外一律正斜杠**：存储键、IPC 参数、比对都用 `/`；
// - 判"在项目内"用**归一分隔符后的前缀**比较（`startsWith` 之前必须先归一，否则 `a\b` 与 `a/b` 永不相等）；
// - `relativize` 失败返回**空串**（不是原样返回绝对路径——那会让调用方把绝对路径当相对路径发出去）。

/** `\` → `/`（其余不动）。 */
export function toSlashes(p: string): string {
  return p.replace(/\\/g, "/");
}

/** 折叠连续斜杠、剥 `.`、合并 `..`（浏览器环境手写，不依赖 node:path）。
 *  磁盘绝对路径保留 `E:/...`；根绝对路径保留 `/...`。 */
export function normalizePath(p: string): string {
  const abs = p.startsWith("/") || /^[A-Za-z]:\//.test(p);
  const out: string[] = [];
  for (const seg of p.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") {
      const prev = out[out.length - 1];
      if (prev && prev !== ".." && !/^[A-Za-z]:$/.test(prev)) out.pop();
      else if (!abs && prev !== "..") out.push("..");
      continue; // 绝对路径下越界的 `..` 丢弃
    }
    out.push(seg);
  }
  if (abs) {
    const head = out[0] ?? "";
    return /^[A-Za-z]:$/.test(head) ? head + "/" + out.slice(1).join("/") : "/" + out.join("/");
  }
  return out.join("/");
}

/** 是否等于 `base` 或落在 `base` **下面**（分隔符无关）。删除/改名/搜索的判定都用它。 */
export function isUnder(path: string, base: string): boolean {
  const p = toSlashes(path);
  const b = toSlashes(base).replace(/\/+$/, "");
  return p === b || p.startsWith(`${b}/`);
}

/** 同一路径的判定（分隔符与尾斜杠无关）—— 比较两个**绝对路径**是否指向同一个东西。 */
export function samePath(a: string, b: string): boolean {
  const norm = (p: string) => toSlashes(p).replace(/\/+$/, "");
  return norm(a) === norm(b);
}

/** 最后一段（文件名/目录名）；`a/b/` 这类带尾斜杠的也给 `b`。 */
export function basename(p: string): string {
  const n = toSlashes(p).replace(/\/+$/, "");
  const i = n.lastIndexOf("/");
  return i < 0 ? n : n.slice(i + 1);
}

/** 项目内绝对路径 → 项目根相对路径（正斜杠）；**不在根内返回空串**。
 *
 *  调用方必须处理空串（当作"路径不合法"并给出可见失败），不要悄悄发出去。 */
export function relativize(abs: string, root: string): string {
  if (!abs || !root) return "";
  const a = normalizePath(toSlashes(abs));
  const r = normalizePath(toSlashes(root));
  if (!a || !r || a === r) return "";
  const prefix = r.endsWith("/") ? r : `${r}/`;
  return a.startsWith(prefix) ? a.slice(prefix.length) : "";
}

/** 把 `from`（文件或目录）整体换成 `to`：路径在 `from` 之下时按前缀替换，否则原样返回。
 *
 *  用途：改名后的标签/缓冲/脏集合**整体重映射**（`editor.remapPaths` 的唯一判据）。 */
export function remapUnder(path: string, from: string, to: string): string {
  const p = toSlashes(path);
  const f = toSlashes(from).replace(/\/+$/, "");
  const t = toSlashes(to).replace(/\/+$/, "");
  if (p === f) return t;
  return p.startsWith(`${f}/`) ? t + p.slice(f.length) : path;
}
