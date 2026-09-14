// 草案补丁（v1）：把"编辑器里刚改动的那一行"近似地画回预览的对应位置。
//
// 目标与口径（docs/research/realtime-preview-cost.md §6、docs/design.md §延迟预算）：
// 用户在 ~200ms 内看到自己的输入被反映，**允许不精确**；真正的正确画面由下一次编译的
// `pdf-updated` 覆盖掉草案层。这里只放**纯函数**（可单测）：文本行索引 / 锚点匹配 / 改动行定位。

export interface PdfTextItem {
  str: string;
  /** pdf.js 的 transform：[a,b,c,d,e,f]，e = x、f = y（PDF 用户空间，原点在左下）。 */
  transform: number[];
  width: number;
  height: number;
}

/** 一"视觉行"：同一基线上按 x 排开的条目拼起来。 */
export interface PdfLine {
  page: number;
  /** PDF 用户空间：左边界 x、基线 y。 */
  x: number;
  y: number;
  /** 行宽与字高（均 PDF 单位）。 */
  w: number;
  size: number;
  text: string;
}

/** 同一行的 y 容差（PDF 单位）：行距通常 ≥ 10，1.5 足够区分上下行。 */
const EPS_Y = 1.5;
/** 锚点匹配的最少字符数：太短（如"的"）会满篇误匹配。 */
const MIN_ANCHOR_CHARS = 4;

/** 把一页的 textContent 条目按基线聚成"视觉行"（按 x 排序后拼接文本）。 */
export function linesOfPage(page: number, items: PdfTextItem[]): PdfLine[] {
  const rows = new Map<number, PdfTextItem[]>();
  for (const it of items) {
    if (!it.str || !it.str.trim()) continue;
    const y = it.transform[5];
    let key: number | undefined;
    for (const k of rows.keys()) {
      if (Math.abs(k - y) <= EPS_Y) {
        key = k;
        break;
      }
    }
    if (key === undefined) {
      key = y;
      rows.set(key, []);
    }
    rows.get(key)!.push(it);
  }
  const out: PdfLine[] = [];
  for (const [y, list] of rows) {
    list.sort((a, b) => a.transform[4] - b.transform[4]);
    const text = list
      .map((i) => i.str)
      .join("")
      .replace(/\s+/g, " ")
      .trim();
    if (!text) continue;
    const x = Math.min(...list.map((i) => i.transform[4]));
    const right = Math.max(...list.map((i) => i.transform[4] + (i.width || 0)));
    const size = Math.max(...list.map((i) => i.height || 0)) || 10;
    out.push({ page, x, y, w: Math.max(right - x, 1), size, text });
  }
  return out;
}

/** 归一化：去掉空白与花括号（LaTeX 命令噪声），用于"编辑器行 ↔ PDF 行"匹配。 */
export function normalizeForMatch(s: string): string {
  return s.replace(/\s+/g, "").replace(/[{}]/g, "");
}

/**
 * 找"这一行现在在 PDF 的哪里"。策略由严到宽：
 * ① 整行相等（1000 分档）；② 索引行**包含**目标串（500 分档）；③ 最长公共前缀 ≥ MIN_ANCHOR_CHARS。
 * **同分取先出现的候选**（索引按页序构建 ⇒ 靠前的页优先）。找不到返回 `null`（调用方静默不画草案）。
 */
export function findAnchor(lines: PdfLine[], oldLineText: string): PdfLine | null {
  const target = normalizeForMatch(oldLineText);
  if (target.length < MIN_ANCHOR_CHARS) return null;
  let best: PdfLine | null = null;
  let bestScore = 0;
  for (const ln of lines) {
    const cand = normalizeForMatch(ln.text);
    if (!cand) continue;
    let score = 0;
    if (cand === target) score = 1000 + target.length;
    else if (cand.includes(target)) score = 500 + target.length;
    else {
      let i = 0;
      while (i < cand.length && i < target.length && cand[i] === target[i]) i++;
      if (i >= MIN_ANCHOR_CHARS) score = i;
    }
    if (score > bestScore) {
      bestScore = score;
      best = ln;
    }
  }
  return best;
}

/** 逐行找出**第一处**改动（返回旧/新行文本）；无改动返回 `null`。 */
export function firstChangedLine(
  oldText: string,
  newText: string
): { lineNo: number; oldLine: string; newLine: string } | null {
  const a = oldText.split(/\r?\n/);
  const b = newText.split(/\r?\n/);
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) {
    const oa = a[i] ?? "";
    const ob = b[i] ?? "";
    if (oa !== ob) return { lineNo: i + 1, oldLine: oa, newLine: ob };
  }
  return null;
}

/**
 * 绘制参数：把"新行文本"放到锚点行上。
 *
 * - `coverW`：要盖住的原行宽度（PDF 单位）——取锚点行宽与新行估算宽的**较大者**，
 *   避免旧字从右边露出来；
 * - `size`：字号取锚点行的字高（近似：PDF 行高≈字号）；
 * - `overflow`：新行估算宽度 / 可用宽度（>1 表示画不下，调用方据此淡出或截断）。
 */
export function draftGeometry(
  anchor: PdfLine,
  newLineText: string,
  avgCharWidth: number
): { coverW: number; size: number; estW: number; overflow: number } {
  const size = anchor.size > 0 ? anchor.size : 10;
  const estW = Math.max(newLineText.length * avgCharWidth, 1);
  return {
    coverW: Math.max(anchor.w, estW) + size * 0.3,
    size,
    estW,
    overflow: estW / Math.max(anchor.w, 1),
  };
}
