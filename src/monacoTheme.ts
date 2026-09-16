// Monaco 主题（roadmap ⑩）：与 App.vue 的 Candy Desk token 一一对应，深浅各一套。
//
// 为什么要单独一份而不是吃 CSS 变量：Monaco 的 `defineTheme` 只认**具体色值**（它把颜色算进
// token 样式表，不参与 CSS 变量求值）。所以这里是"token 的第二落点"—— 改 App.vue 的糖果色时，
// 两处要一起改；两套主题的色相必须同源，否则编辑器会和外壳脱节。
import * as monaco from "monaco-editor";

/** 浅色：Candy Desk（原 `main.ts` 里的内联定义，逐字搬过来）。 */
const LIGHT: monaco.editor.IStandaloneThemeData = {
  base: "vs",
  inherit: true,
  rules: [
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
};

/**
 * 深色：Candy Desk 夜间态。
 * 规则与外壳一致：**强调色提亮**（深底上要发光）、行号/次要文字降一档、
 * 选中底换成蓝莓调的深色、控件面比编辑面亮一档（`card-2` > `card`）。
 */
const DARK: monaco.editor.IStandaloneThemeData = {
  base: "vs-dark",
  inherit: true,
  rules: [
    { token: "comment", foreground: "9db39b" },
    { token: "keyword", foreground: "a5a6ff" },
    { token: "keyword.command", foreground: "a5a6ff" },
    { token: "keyword.env", foreground: "f082aa" },
    { token: "string.env", foreground: "f082aa" },
    { token: "keyword.math", foreground: "a77aff" },
    { token: "math", foreground: "c0a3ff" },
    { token: "number", foreground: "4dd6a6" },
    { token: "string.url", foreground: "6db2ff" },
    { token: "operator", foreground: "a49dbb" },
    { token: "delimiter", foreground: "8a83a8" },
    { token: "string", foreground: "ffc76b" },
    { token: "text", foreground: "ece9f7" },
  ],
  colors: {
    "editor.background": "#1f1b2c",
    "editor.foreground": "#ece9f7",
    "editor.lineHighlightBackground": "#262038",
    "editorLineNumber.foreground": "#6f6885",
    "editorLineNumber.activeForeground": "#a49dbb",
    "editorCursor.foreground": "#8a8cff",
    "editor.selectionBackground": "#3b3a72",
    "editor.inactiveSelectionBackground": "#2e2b52",
    "editorIndentGuide.background1": "#2f293f",
    "editorWidget.background": "#292337",
    "editorWidget.border": "#372f49",
    "editorSuggestWidget.background": "#292337",
    "editorSuggestWidget.border": "#372f49",
    "scrollbarSlider.background": "#4d4566a0",
    "scrollbarSlider.hoverBackground": "#5f5680c0",
    "editorGutter.background": "#1b1727",
  },
};

let defined = false;
/** 定义两套主题并切到 `mode`（幂等：重复调用只切不重复定义）。 */
export function applyMonacoTheme(mode: "light" | "dark") {
  if (!defined) {
    monaco.editor.defineTheme("latteset", LIGHT);
    monaco.editor.defineTheme("latteset-dark", DARK);
    defined = true;
  }
  monaco.editor.setTheme(mode === "dark" ? "latteset-dark" : "latteset");
}
