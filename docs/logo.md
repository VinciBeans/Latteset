# 品牌标识（Logo）

> 概念：**热浪杯**（Hot Steam Cup）。风格：年轻、简约、扁平几何 · 不对称动势。
> 术语见 [CONTEXT.md](../CONTEXT.md)，产品设计见 [design.md](./design.md)。

## 概念

Latteset = **Latte + Typeset**，本义是"一杯拿铁的时间，排版已经跟上"，与核心设计因素**实时编译**绑定（见 CONTEXT.md）。

Logo 是一只**刚冲好、还冒着热气的拿铁杯**：

- **翻边杯口 + 收窄杯身 + 加宽杯碟**：干净的实心几何，上宽下窄的矮杯剪影；
- **四粒渐细的蒸汽**：从杯口向右上漂散、越来越小——不对称构图带来动势，尖端消失在"出杯的瞬间"。

一句话：*刚冲好的拿铁，还冒着热气。* 与产品文案"编辑即编译，如同现萃即饮"同构。

> 迭代记录：经 logo-generator 工作流从 8 个方向（[logo-showcase.html](./logo-showcase.html)）选 3 强
> （[logo-finalists.html](./logo-finalists.html)）后定案为 **V4 Hot Steam Cup**；融合版「Steam T」因线条质感
> 不如实心版而放弃，保留在 `docs/logo-variants/` 供参考。

## 色板

沿用应用内 "Candy Desk" 设计系统（见 `src/App.vue` 的 `:root` 变量），不另造色板：

| 名称 | 色值 | 用途 |
|---|---|---|
| Blueberry | `#5D5FEF` | 杯口、杯身、杯碟（主色，稳定） |
| Coral | `#FF7A6E` | 蒸汽四粒（强调，热/即时） |
| Ink | `#2B2438` | 字标、深色背景上的 mark |
| Paper | `#F4F2FB` | 应用图标瓦片上的杯身 |
| Tile 渐变 | `#6A5CFF → #5D5FEF → #4E9BFF` | 应用图标背景（同主按钮渐变） |

## 文件

| 文件 | 用途 |
|---|---|
| `public/logo.svg` | **mark**（100×100 viewBox，三色，透明底），任意浅/深底色、工具栏、页眉 |
| `public/logo-full.svg` | 全称标志：mark + 字标 "Latteset"（横向 lockup） |
| `public/app-icon.svg` | 应用图标瓦片（圆角方块 + mark，1024×1024） |
| `public/logo-mono.svg` | 单色版（`currentColor`），用于单色限制场景（托盘图标、水印） |
| `docs/logo-export/*.png` | 已渲染的 PNG 导出（mark 512/1024、tile 1024、lockup、单色） |
| `src-tauri/icons/` | 由 `tauri icon public/app-icon.svg -o src-tauri/icons` 生成的全平台图标组 |
| `scripts/export-logo.mjs` | PNG 导出脚本（需 `npm i -D @resvg/resvg-js`） |

## 字标

"Latteset" 统一无衬线 800（`Segoe UI` / `Helvetica Neue` / `Arial`），墨色 Ink，字距 -0.5。
不混排衬线——TeX 的暗号由蒸汽承担，字标只管干净利落。

## 使用规则

- **留白**：mark 四周至少留出杯碟高度（`8/100`）的净空，勿贴边切割。
- **最小尺寸**：mark ≥ 16px（再小蒸汽粒并入杯身）；lockup ≥ 96px 高。
- **不变量**：不拉伸变形、不加阴影/描边、不换色（单色场景用 `logo-mono.svg`）、不旋转。
- **深色背景**：mark 三色版可直接使用（Coral 与 Blueberry 在 Ink 上都成立）；纯色场景用 `app-icon.svg` 瓦片。
- **改版**：改 `public/logo.svg` 后需同步 `logo-full.svg`、`app-icon.svg`、`logo-mono.svg`，再依次
  `tauri icon` 与 `node scripts/export-logo.mjs`。
