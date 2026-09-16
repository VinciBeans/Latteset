// useTheme（roadmap ⑩）：纯逻辑 + DOM 落地。Monaco 侧 mock 掉——单测不该去加载整个编辑器。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../monacoTheme", () => ({ applyMonacoTheme: vi.fn() }));

import { resolveTheme, applyTheme, THEME_STORAGE_KEY } from "../useTheme";
import { applyMonacoTheme } from "../../monacoTheme";

describe("useTheme", () => {
  beforeEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
    vi.mocked(applyMonacoTheme).mockClear();
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
});
