import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";

// Monaco worker（modules.md §6：直接 ESM + Vite ?worker，不用 CDN 包装）
import * as monaco from "monaco-editor";
import editorWorker from "monaco-editor/editor/editor.worker?worker";

self.MonacoEnvironment = {
  getWorker() {
    return new editorWorker();
  },
};
// 与全局设计系统一致的 Monaco 主题（Candy Desk 深浅两套，见 src/monacoTheme.ts）。
// 首帧按 index.html 已经定好的 `data-theme` 选——不走这里再判一次系统偏好，避免两处口径。
import { applyMonacoTheme } from "./monacoTheme";
applyMonacoTheme(document.documentElement.dataset.theme === "dark" ? "dark" : "light");

// ---- LaTeX 语法注册：关键字色彩高亮（自研 Monarch 语法，替代内置 grammar）----
import { latexLanguage, latexConfiguration } from "./latexSyntax";
// 本构建未内置 latex 语言，需先注册（未注册时 setLanguageConfiguration 会抛错 → 白屏）
if (!monaco.languages.getLanguages().some((l) => l.id === "latex")) {
  monaco.languages.register({ id: "latex" });
}
monaco.languages.setMonarchTokensProvider("latex", latexLanguage);
monaco.languages.setLanguageConfiguration("latex", latexConfiguration);

// ---- LaTeX 语言扩展（v1.1）：代码片段补全 + 环境块折叠 ----
import { registerLatexProvider } from "./latexSuggest";
registerLatexProvider();

const app = createApp(App);
app.use(createPinia());
app.mount("#app");
