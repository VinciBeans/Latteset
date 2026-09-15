import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: [
        "**/src-tauri/**",
        // **不参与前端的巨型目录**（2026-09 实测：dev server 起得来、端口在监听，但**所有请求都不响应**
        // ——连接被接受后迟迟不回，`vite optimize` 与 `vite build` 却都正常 ⇒ 卡在 dev server 的
        // 请求路径，而不是 esbuild）。
        // `test_file/` 是本地素材区：vcpkg 检出（1.4 万文件）、vendor（824 MB）、tectonic 上游源码、
        // 各种夹具与编译产物 —— Vite 没有任何理由去 watch 它，而 chokidar 在这种体量上会拖住 dev server。
        // `dist/` 是构建产物（`tauri build` 与 dev 同时存在时会自我触发）。
        "**/test_file/**",
        "**/dist/**",
        // 临时目录（如 logo 预览工具写入的 .logo.svg.<pid>.<uuid>.tmpdir/）：
        // 文件可能被工具锁定（EBUSY），chokidar 的未处理 error 会拖垮整个 vite。
        /\.tmpdir[\\/]/,
      ],
    },
  },
}));
