import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";

// 前端单测（vitest）：docs/archive/architecture.md §8「MVP 后补 vitest + @vue/test-utils」。
// 与 vite.config.ts 分离，避免污染 Tauri dev/build 的 vite 配置。
export default defineConfig({
  plugins: [vue()],
  test: {
    // 测 store/composable/组件都可能在 jsdom 环境里跑；happy-dom 轻量且开箱即用。
    environment: "happy-dom",
    globals: true,
    include: ["src/**/*.{test,spec}.ts"],
    // 纯逻辑不依赖真实计时（debounce 测试用 vi.useFakeTimers，这里不设长超时）。
    testTimeout: 10000,
  },
});
