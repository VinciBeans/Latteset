// 宏包集（bundle）来源口径（ADR-0016 候选 A：随包精简集 + 缺包补全通道）。
//
// 存储形态（`settings.tectonic.bundle`，core `TectonicSettings::normalize_bundle`）：
// - `null` / `""`             ⇒ **内置精简宏包集**（随安装包，离线可用）= 产品默认；
// - `http(s)://…`             ⇒ **在线完整宏包集**（用户显式选择，需联网，覆盖更全）；
// - `file:///…` / 相对路径    ⇒ **本地目录 bundle**（用户/内网自备，需自带 SHA256SUM）。
//
// ⚠ 不静默回退（ADR-0016 决策 2）：内置集缺失/损坏只报错 + 引导修复安装，不会自动改网络。
// 在线地址与后端 `crates/latteset-tectonic/src/bundle.rs` 的 `FORMAT_SERIAL = 33` 兜底地址一致；
// 改格式版本时两处一起改（前端只在"用户显式点在线"时把这个地址写进设置，不做网络请求）。
export const ONLINE_BUNDLE_URL = "https://relay.fullyjustified.net/default_bundle_v33.tar";

export type BundleSourceKind = "shipped" | "online" | "local";

/** 存储值 → 来源类别（界面展示 / 一键切换用；空值 = 内置精简集）。 */
export function bundleSourceKind(stored: string | null | undefined): BundleSourceKind {
  const s = (stored ?? "").trim();
  if (s === "") return "shipped";
  if (/^https?:\/\//i.test(s)) return "online";
  return "local";
}

/** 来源类别的用户可读名称（设置面/错误面板共用）。 */
export function bundleSourceLabel(kind: BundleSourceKind): string {
  switch (kind) {
    case "shipped":
      return "内置精简宏包集（随安装包，离线可用）";
    case "online":
      return "在线完整宏包集（需联网）";
    case "local":
      return "本地目录宏包集";
  }
}
