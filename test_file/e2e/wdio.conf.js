// WebdriverIO 配置（Windows 目标）
// 后端协议：tauri-driver（127.0.0.1:4444）→ 底层 Edge WebDriver（msedgedriver）。
// 前置条件（见 README.md）：
//   1) 已安装 tauri-driver 与 msedgedriver（cargo install --git https://github.com/chippers/msedgedriver-tool 后运行 msedgedriver-tool.exe）
//   2) Rust debug 二进制已构建：`cargo build --manifest-path ../../src-tauri/Cargo.toml`
//   3) vite dev 已启动（app 的 debug 二进制从 devUrl 加载前端）：`npm run dev`（若搭配自动打开项目，见 README）
import path from "path";
import net from "net";
import { spawn } from "child_process";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// msedgedriver 由 msedgedriver-tool 解压到 test_file/e2e/drivers/（已 gitignore）
const nativeDriver = path.resolve(__dirname, "drivers/msedgedriver.exe");
let tauriDriver;

// tauri-driver 可能不向 stdout 打印 "listening"（不同版本行为不一），
// 直接轮询中间端口是否可连，避免 beforeSession 永远等待字符串。
function waitForPort(host, port, timeoutMs) {
  return new Promise((resolve, reject) => {
    const start = Date.now();
    const tryConnect = () => {
      const s = net.connect({ host, port });
      s.once("connect", () => {
        s.destroy();
        resolve();
      });
      s.once("error", () => {
        s.destroy();
        if (Date.now() - start > timeoutMs) reject(new Error(`tauri-driver 未在 ${host}:${port} 就绪`));
        else setTimeout(tryConnect, 500);
      });
    };
    tryConnect();
  });
}

export const config = {
  hostname: "127.0.0.1",
  port: 4444,
  specs: ["./specs/**/*.js"],
  maxInstances: 1,

  capabilities: [
    {
      browserName: "wry",
      "tauri:options": {
        application: path.resolve(__dirname, "../../src-tauri/target/debug/latteset.exe"),
      },
    },
  ],

  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { ui: "bdd", timeout: 120000 },

  // 启动 tauri-driver 中介（连接底层 Edge WebDriver）
  beforeSession: async () => {
    tauriDriver = spawn("tauri-driver", ["--native-driver", nativeDriver], {
      stdio: ["ignore", "pipe", "pipe"],
    });
    tauriDriver.stderr.on("data", (d) => process.stderr.write(d));
    await waitForPort("127.0.0.1", 4444, 20000);
  },

  afterSession: () => tauriDriver?.kill(),
};
