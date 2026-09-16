// useTheme（roadmap ⑩）：纯逻辑 + DOM 落地 + 原生标题栏。
// Monaco 与 ipc 都 mock 掉——单测不该加载整个编辑器，也不该碰 Tauri 内部。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { defineComponent } from "vue";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

vi.mock("../../monacoTheme", () => ({ applyMonacoTheme: vi.fn() }));
vi.mock("../../services/ipc", () => ({ ipc: { setWindowTheme: vi.fn(async () => {}) } }));

import { resolveTheme, applyTheme, useTheme, THEME_STORAGE_KEY } from "../useTheme";
import { applyMonacoTheme } from "../../monacoTheme";
import { ipc } from "../../services/ipc";
import { useSettingsStore } from "../../stores/settings";

describe("useTheme", () => {
  beforeEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
    vi.mocked(applyMonacoTheme).mockClear();
    vi.mocked(ipc.setWindowTheme).mockClear();
  });

  it("设置 + 系统偏好 → 实际主题", () => {
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });

  // 设置还没从后端读回来（首帧）时跟系统走：否则深色用户会先看到一帧浅色。
  it("设置缺省时跟随系统", () => {
    expect(resolveTheme(undefined, true)).toBe("dark");
    expect(resolveTheme(undefined, false)).toBe("light");
  });

  it("applyTheme 同时落 data-theme / localStorage / Monaco", () => {
    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");
    expect(applyMonacoTheme).toHaveBeenLastCalledWith("dark");

    // 浅色是默认态：**不写属性**（删掉），免得和 CSS 默认块两处打架
    applyTheme("light");
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("light");
    expect(applyMonacoTheme).toHaveBeenLastCalledWith("light");
  });

  // 原生标题栏要吃**设置值**：`system` 必须原样传下去（后端翻成 set_theme(None) 让系统自己跟）。
  // 若把解析后的 dark/light 推过去，用户切系统主题后标题栏会卡在旧值上。
  it("sync 把设置值（含 system）推给窗口主题，并落 CSS", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const store = useSettingsStore();
    const Comp = defineComponent({
      setup() {
        return useTheme();
      },
      template: "<div />",
    });
    const w = mount(Comp, { global: { plugins: [pinia] } });
    const { sync } = w.vm as unknown as { sync: () => void };

    store.setSettings({ ui: { theme: "system" } } as never);
    sync();
    expect(ipc.setWindowTheme).toHaveBeenLastCalledWith("system");

    store.setSettings({ ui: { theme: "dark" } } as never);
    sync();
    expect(ipc.setWindowTheme).toHaveBeenLastCalledWith("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");

    store.setSettings({ ui: { theme: "light" } } as never);
    sync();
    expect(ipc.setWindowTheme).toHaveBeenLastCalledWith("light");
    expect(document.documentElement.dataset.theme).toBeUndefined();
    w.unmount();
  });
});
