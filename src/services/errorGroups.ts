// 错误分组 / 截断的**唯一口径**：错误列表展示与状态栏（折叠头「N 个问题」）共用同一份结果，
// 避免两处各算一遍导致数字对不上（同源错误去重后是"组"，原始条目是"条"）。
import type { ErrorEntry } from "../bindings";

/** 展示上限（去重后的组数）。 */
export const MAX_DISPLAY = 30;

export interface ErrorGroup {
  entry: ErrorEntry;
  count: number;
  /** 首行展示文本：有诊断用「原因」，否则用原始消息首行。 */
  headline: string;
}

export interface GroupedErrors {
  /** 实际展示的组（最多 MAX_DISPLAY 个）。 */
  items: ErrorGroup[];
  /** 因超出展示上限而未显示的组数。 */
  hidden: number;
  /** 去重后的总组数（= items.length + hidden）——界面计数一律用它。 */
  total: number;
}

/**
 * 去重：file + 展示文本（诊断原因优先，否则消息首行）作为键；保留首次出现的文件/行号。
 * `limit` 默认 `MAX_DISPLAY`（B-08「显示全部」时传 `Infinity`）。
 */
export function groupErrors(errors: ErrorEntry[], limit: number = MAX_DISPLAY): GroupedErrors {
  const seen = new Map<string, ErrorGroup>();
  for (const e of errors) {
    const headline = e.diagnosis?.cause ?? e.message.split("\n")[0] ?? "";
    const key = `${e.file ?? ""}\n${headline}`;
    const g = seen.get(key);
    if (g) g.count += 1;
    else seen.set(key, { entry: e, count: 1, headline });
  }
  const all = [...seen.values()];
  const items = all.slice(0, limit);
  return {
    items,
    hidden: all.length - items.length,
    total: all.length,
  };
}
