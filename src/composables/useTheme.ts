// useTheme（roadmap ⑩）：把「界面主题」设置落到 DOM 上。
//
// 三层取值：设置里的 `light | dark | system` → 解析成**实际**的 `light | dark` → 写到
// `<html data-theme="…">`。CSS 侧只认 `[data-theme="dark"]`（App.vue 的 token 覆盖块），
// 所以"跟随系统"这种动态语义只在这个文件里，样式表不需要知道。
//
// 为什么要**同时**写 localStorage：`index.html` 里的内联脚本在首帧前跑（那时设置还没从后端读回来），
// 只能拿到上次解析出的主题 ⇒ 用它避免"深色用户先闪一下白屏"。真正的权威仍是后端设置。
import { onBeforeUnmount, watch } from "vue";
import type { UiTheme } from "../bindings";
import { applyMonacoTheme } from "../monacoTheme";
import { ipc } from "../services/ipc";
import { useSettingsStore } from "../stores/settings";

/** localStorage 键（内联脚本里是同一个字符串，改一处要两处一起改）。 */
export const THEME_STORAGE_KEY = "latteset.theme";

/** 系统是否偏好深色（`matchMedia` 不可用时按浅色处理）。 */
export function systemPrefersDark(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.("(prefers-color-scheme: dark)").matches;
}

/** 设置 + 系统偏好 → 实际主题（纯函数，单测直接打）。 */
export function resolveTheme(theme: UiTheme | undefined, prefersDark: boolean): "light" | "dark" {
  if (theme === "dark") return "dark";
  if (theme === "light") return "light";
  // `system` 与"设置还没读回来"（undefined）都跟随系统：宁可跟系统，也不要先亮后暗
  return prefersDark ? "dark" : "light";
}

/** 写到 DOM + localStorage + Monaco（一处入口，免得三边各自为政）。 */
export function applyTheme(resolved: "light" | "dark") {
  const root = document.documentElement;
  if (resolved === "dark") root.dataset.theme = "dark";
  else delete root.dataset.theme; // 浅色是默认态：不写属性，省得和 CSS 的默认块两处打架
  try {
    localStorage.setItem(THEME_STORAGE_KEY, resolved);
  } catch {
    // 隐私模式/存储被禁：只影响"首帧不闪"，不影响功能
  }
  applyMonacoTheme(resolved);
}

export function useTheme() {
  const settings = useSettingsStore();
  const media = typeof window !== "undefined" ? window.matchMedia?.("(prefers-color-scheme: dark)") : undefined;

  /** 原生标题栏跟随**设置值**（不是解析后的深浅）：`system` 要让系统自己跟。 */
  function pushWindowTheme(theme: UiTheme | undefined) {
    ipc.setWindowTheme(theme ?? "light").catch((e) => {
      // 平台不支持 / 没有标题栏：只影响外观，不该打断任何流程
      console.debug("设置窗口主题失败（标题栏可能没跟上）：", e);
    });
  }

  const sync = () => {
    const theme = settings.settings?.ui?.theme;
    applyTheme(resolveTheme(theme, systemPrefersDark()));
    pushWindowTheme(theme);
  };

  // 设置变化（含"跟随系统"→ 切到 dark/light）
  watch(() => settings.settings?.ui?.theme, sync);
  // 系统偏好变化：只有"跟随系统"时才该动（多算一次 resolveTheme 即可，不必分支）
  media?.addEventListener?.("change", sync);
  onBeforeUnmount(() => media?.removeEventListener?.("change", sync));

  return { sync };
}
