// Latteset 端到端 GUI 测试（Windows）
// 依赖：应用 debug 二进制 + vite dev + tauri-driver + msedgedriver（见 README.md）。
// 若想测量 pdf.js 重载耗时，请以 VITE_LATTESET_PROJECT 启动 vite dev 自动打开项目，
// 再点「编译」触发重载，本测试会从 window.__previewLastReload 读取并打印每次重载耗时。
import { $, browser, expect } from "@wdio/globals";

describe("Latteset GUI（Windows）", () => {
  it("启动并渲染核心 UI（工具条 + 预览面板）", async () => {
    await browser.waitUntil(() => $("button.btn.primary").isExisting(), {
      timeout: 30000,
    });
    const toolbarText = (await $$(".toolbar-actions button").getText()).join(" ");
    expect(toolbarText).toMatch(/打开项目/);
    expect(toolbarText).toMatch(/编译/);
    expect($(".preview-root")).toBeExisting();
  });

  it("预览空态提示可见（未加载 PDF 时）", async () => {
    const empty = await $(".empty-title");
    await empty.waitForExist({ timeout: 10000 });
    expect(await empty.getText()).toContain("PDF 在这里等你");
  });

  it("触发手动编译并读取 pdf.js 重载耗时", async () => {
    const btn = await $("button.btn.primary");
    await btn.waitForExist({ timeout: 20000 });
    await btn.click();
    // 等待出现初次重载耗时（编译器后台运行，可能数秒）
    await browser.waitUntil(
      () =>
        browser.execute(() => {
          const t = window.__previewLastReload;
          return t != null && typeof t.total === "number";
        }),
      { timeout: 90000 }
    );
    const t = await browser.execute(() => window.__previewLastReload);
    // 打印 fetch/parse/render/total/pages/pagesRendered，用于判断 pin 的瓶颈
    console.log("PDF_RELOAD " + JSON.stringify(t));
    expect(t).toHaveProperty("total");
    expect(t).toHaveProperty("pages");
    expect(t).toHaveProperty("render");
  });
});
