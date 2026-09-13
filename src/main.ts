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
// 与全局设计系统一致的浅色 Monaco 主题（Candy Desk，App.vue :root 色板）
monaco.editor.defineTheme("latteset", {
  base: "vs",
  inherit: true,
  rules: [
    // ---- LaTeX 关键词高亮配色（Candy Desk）----
    { token: "comment", foreground: "7f977e" },
    { token: "keyword", foreground: "4a4cd8" },
    { token: "keyword.command", foreground: "4a4cd8" },
    { token: "keyword.env", foreground: "d1547e" },
    { token: "string.env", foreground: "d1547e" },
    { token: "keyword.math", foreground: "7c3aed" },
    { token: "math", foreground: "8b5cf6" },
    { token: "number", foreground: "2f9e8f" },
    { token: "string.url", foreground: "2979b5" },
    { token: "operator", foreground: "7a7490" },
    { token: "delimiter", foreground: "a49dc0" },
    { token: "string", foreground: "b0731c" },
    { token: "text", foreground: "2b2438" },
  ],
  colors: {
    "editor.background": "#ffffff",
    "editor.foreground": "#2b2438",
    "editor.lineHighlightBackground": "#f7f5fd",
    "editorLineNumber.foreground": "#b8b2ce",
    "editorLineNumber.activeForeground": "#7a7490",
    "editorCursor.foreground": "#5d5fef",
    "editor.selectionBackground": "#dcd7f6",
    "editor.inactiveSelectionBackground": "#ece9fa",
    "editorIndentGuide.background1": "#e7e3f4",
    "editorWidget.background": "#ffffff",
    "editorWidget.border": "#ded9ee",
    "editorSuggestWidget.background": "#ffffff",
    "editorSuggestWidget.border": "#ded9ee",
    "scrollbarSlider.background": "#c9c2e480",
    "scrollbarSlider.hoverBackground": "#b3aad6a0",
    "editorGutter.background": "#fbfaff",
  },
});
monaco.editor.setTheme("latteset");

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
