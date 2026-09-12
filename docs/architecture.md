# TeXPresso 架构设计（分层、接口、模块、技术栈）

> 项目状态：**已实现并迭代中**（Windows 首发 MVP）。
> 上层设计见 [design.md](./design.md)，术语见根目录 [CONTEXT.md](../CONTEXT.md)，决策记录见 [adr/](./adr/)。

## 1. 分层总览

> 图示版（Mermaid，含编译链路 / 调度语义 / SyncTeX 三张附图）见 [architecture-diagram.md](./architecture-diagram.md)。

非对称分层，依赖方向严格单向：

```
┌─ 前端（Vue 3 + TS）───────────────────────────────┐
│  views / components（EditorPane、PreviewPane、…） │
│  stores（Pinia ×6）                               │
│  services（ipc.ts / events.ts）← 唯一碰 IPC 的层   │
└──────────────┬────────────────────────────────────┘
               │ IPC 契约（tauri-specta 生成 TS 类型）
┌──────────────▼────────────────────────────────────┐
│  src-tauri：commands（DTO 转换、发事件、无业务逻辑）│
│            + 装配注入（AppState / watch 接线）      │
└──────────────┬────────────────────────────────────┘
               │ 经 core trait 调用：FileSystem / CompileRunner / SyncTexProvider
               │ 监视结果经 WatchSink 回调（事件形态在 src-tauri 定型）
┌──────────────▼────────────────────────────────────┐
│  texpresso-infra：进程、文件、监视、设置存储        │
│            （外部依赖与文件系统唯一落点，无 Tauri） │
└──────────────┬────────────────────────────────────┘
               │ core 定义接口与策略（core 不依赖 infra）
┌──────────────▼────────────────────────────────────┐
│  texpresso-core：队列语义、探测规则、解析 — 纯逻辑  │
└───────────────────────────────────────────────────┘
```

原则：

- 依赖单向：`前端 → IPC 契约 → src-tauri → texpresso-infra → texpresso-core`（接口在 core、实现在 infra、注入在 src-tauri；ADR-0010）
- 上层（src-tauri）不直接调用 OS API：文件与进程一律经 core trait，路径安全策略（D8）在 core `project::paths`
- 领域层不依赖 Tauri、不做 IO；IO 经基础设施，测试经 trait 注入
- 不做 ports & adapters 式 trait 泛滥；只有真正需要替换/注入的边界才抽象（CompileRunner、SyncTexProvider）

## 2. Rust 侧：workspace 与模块

Cargo workspace 三 crate：ADR-0006 拆出 core，ADR-0010 再拆出 infra（基础设施层与 core 同级）。

**texpresso-core**（无 Tauri 依赖、无 IO，全量可单测）

| 模块 | 职责 |
|---|---|
| `scheduler` | 合并队列 + 超时/重试/终止语义状态机。**只有事件输入与指令输出，不 spawn 进程**；经 `CompileRunner` trait 执行编译（单测用 fake runner） |
| `project` | 项目模型、根文件探测规则（见 §5.4）、文件集合过滤 |
| `log_parser` | .log 解析 → ErrorEntry（快照测试） |
| `synctex` | `SyncTexProvider` 接口 + 输出解析（进程调用在 texpresso-infra） |
| `settings` | 设置模型、默认值、全局/项目合并、校验 |
| `compose` | 组合层翻译：文件事件/手动编译 → `CompileRequest`（D3，infra watch 调用） |
| `paths` | 项目内路径解析（D8）：canonicalize + 根内校验 + `PathError`（IO 经 FileSystem） |
| `types` | 跨边界 DTO（CompileStatus / ErrorEntry / ProjectInfo / Settings…，specta 导出） |

**texpresso-infra**（基础设施层：外部依赖与文件系统的唯一落点，ADR-0010）

| 模块 | 职责 |
|---|---|
| `fs` | `tokio::fs` 实现 `FileSystem`（含 canonicalize / is_dir / write）；Windows `\\?\` verbatim 前缀剥离 |
| `runner` | latexmk 执行（tokio::process、超时、Windows `taskkill /T /F` 树杀、PDF 原子拷贝、.log 容错解码） |
| `synctex` | synctex CLI 调用（实现 SyncTexProvider，见 ADR-0008） |
| `storage` | 设置存储：全局/项目文件原子写 + 自写盘 hash 过滤 |
| `watch` | notify 8.2 直连（不用 tauri-plugin-fs 的 JS watch）：事件分类 → 调度器输入 / 设置热更新；结果经 `WatchSink` 回调（本层不认识 Tauri） |

**src-tauri**（接线与契约薄壳）

| 模块 | 职责 |
|---|---|
| `commands` | invoke 处理器：DTO 转换、路径策略调用、错误契约 `{ code, message }`；无业务逻辑 |
| `events` | 事件契约（specta 生成）+ `TauriSink`（监视结果 → tauri 事件）+ 调度器 Emitter 适配 |
| `lib` | 装配：构造 infra 实现注入 core trait 位；持有 AppState（调度器、设置、项目状态） |

## 3. 前端侧：模块划分

- **services**：`ipc.ts`（specta 生成的类型化 invoke 封装）、`events.ts`（事件订阅 → store 分发）
- **stores（Pinia ×6）**：projectStore（项目/根文件/文件树）｜editorStore（打开文件、脏标志、活动标签）｜compileStore（编译状态/队列/错误列表）｜previewStore（PDF 文档、滚动位置、SyncTeX 高亮）｜outlineStore（源码结构树）｜settingsStore
- **组件**：EditorPane（Monaco 封装）、PreviewPane（pdf.js 封装）、FileTree、TabBar、OutlinePane（大纲）、ErrorList、StatusBar、布局壳、**自研 splitter**（不引入 vue-code-layout，多面板布局后置）
- **composables**：useAutoSave（防抖保存）、useSyncTex

## 4. 层间接口契约（IPC）

类型安全：**tauri-specta**（锁定 RC 版本，如 rc.25），命令签名与事件载荷从 Rust 自动生成 TS 类型。

### 命令面（invoke）

| 命令 | 输入 | 输出 |
|---|---|---|
| open_project | folder | ProjectInfo（含根文件探测结果） |
| list_dir | path | Vec\<DirEntryInfo\>（递归文件树，含目录；前端防抖） |
| read_file | path | content |
| save_all | files: Vec\<FileContent\> | 空（写盘：`save_content`；自动保存批量写盘用，唯一保存路径） |
| get_outline | buffers, files | Vec\<OutlineNode\>（结构树，详见 modules.md） |
| compile_now | 空 | 空 |
| abort_compile | 空 | 空 |
| synctex_forward | file, line, col | { page, x, y } |
| synctex_inverse | page, x, y | { file, line, column } |
| get_settings / update_settings | [patch] | Settings |

文件读写走自建命令，不用 tauri-plugin-fs（"打开任意文件夹"的动态 scope 配置绕）。

### 事件面（emit）

| 事件 | 载荷 |
|---|---|
| compile-status | { phase: queued\|running\|success\|failed, kind: timeout\|content_error\|aborted\|null } |
| errors-updated | ErrorEntry[]（失败时携带；编译中清空） |
| pdf-updated | { path }（编译成功、PDF 新版本就绪） |
| files-changed | { paths, structural }（监视旁路：文件树刷新/重载判定；`structural`=true 仅增/删/重命名，内容修改为 false——见 modules.md §7） |
| settings-changed | Settings |

不做编译输出流式推送——.log 文件是权威，避免 IPC 风暴。

### 错误模型

- 命令错误：`{ code, message }`（thiserror 枚举在命令边界序列化）
- ErrorEntry：`{ message, file, line, kind }`

## 5. 关键数据流

### 5.1 编译触发链

```
连续模式：输入 → Monaco 变更 → 前端防抖 500ms → save_all（自动保存全部脏文件，仅 continuous 模式）
       → 磁盘变化 → notify 事件 → 调度器入队（合并队列吸收风暴）→ 编译（编辑触发 = Quick 单趟）
保存模式：编辑不自动写盘；Ctrl+S / 点「编译」/ 关标签 → flush()（save_all 全部脏文件）→ 同上
手动编译：compile_now → 直接入队（无论有无变化；on_save 下点「编译」会先 flush 落盘）
外部修改：notify → 入队（不依赖前端）
```

- **防抖只在前端"保存时机"层**（500ms 是产品语义）；后端不做时间防抖——合并队列（最多一个等待条目）本身吸收事件风暴，后端再加防抖会把延迟撑爆预算
- 若实测 notify 事件风暴成问题，再引入 notify-debouncer-mini（低延迟合并），不预装

### 5.2 PDF 刷新

`pdf-updated` → previewStore 记录页码+滚动位置 → pdf.js 重新加载 → 恢复位置。SyncTeX 高亮为前端 overlay。滚动保持用"重载+恢复"而非局部更新（pdf.js 无增量渲染 API）。

### 5.3 SyncTeX 双向

Ctrl+点击 → `synctex_forward` → { page, x, y } → PDF 高亮；PDF 点击 → `synctex_inverse` → { file, line } → 编辑器跳转。CLI 指向 `tmp/<根名>.synctex.gz`。

### 5.4 根文件探测

扫描项目内全部 .tex（排除 tmp/），剔除被 `\input`/`\include` 引用的文件，余者含 `\documentclass` 的为候选：

- 唯一候选 → 自动采用
- 多候选 → 弹窗选择（Naive UI）
- 零候选 → 提示手动指定

手动覆盖写入项目 settings.json 的 `root_file` 键。

### 5.5 外部修改处理

前端过滤自己刚自动保存的文件（editorStore 比对 path，避免重载光标跳动）；打开且不脏 → 静默重载；打开且脏 → 保留本地缓冲 + 状态栏提示（冲突对话框后置）。

## 6. 技术栈冻结

**Rust**

| 依赖 | 版本约束 | 用途 |
|---|---|---|
| tauri 2 | — | 应用框架 |
| tauri-specta | 锁定 RC（如 rc.25） | 类型化 IPC |
| notify | 8.2.x（9.x 是 RC 不碰） | 文件监视（texpresso-infra） |
| tokio | — | 进程/超时（infra）、异步锁与通道（core / src-tauri） |
| thiserror | — | 领域错误枚举 |
| serde | — | 序列化 |
| tracing + tracing-subscriber | — | 结构化日志 |
| tauri-plugin-log | skip_logger 模式 | 日志通道 |
| tauri-plugin-window-state | — | 窗口状态持久化 |
| insta（dev） | — | 快照测试 |

**前端**

| 依赖 | 版本约束 | 用途 |
|---|---|---|
| Vue 3 + TS | — | 框架 |
| Pinia | — | 状态管理 |
| Naive UI | — | 基础组件 |
| monaco-editor | 直接 ESM + Vite `?worker` | 编辑器（不用 @guolao 包装：CDN-first 与离线冲突；AMD 已弃用） |
| pdfjs-dist | worker 拷 `public/pdfjs/pdf.worker.min.js`（`.js` 扩展名规避 asset 协议 MIME 拒载；不用 `?worker&inline`——workerPort 在组件卸载重建时会撕毁） | PDF 预览 |
| Vite | 锁 6.x（7.1+ 有 `?url` 回归） | 构建 |

**明确不引入**：tauri-plugin-shell（PATH 二进制作用域模型别扭；sidecar 只用于捆绑二进制）、tauri-plugin-fs（自建命令替代）、tauri-plugin-store（用户可编辑设置用手写 JSON）、vue-router（无独立页面）、@guolao/vue-monaco-editor、vue-code-layout（自研 splitter）。

## 7. 安全

- **CSP v1 即开**（Tauri 默认 csp:null 不强制，我们不接受）：`script-src 'self'`；`worker-src 'self' blob:`（Monaco + pdf.js worker）；`connect-src 'self' blob: ipc: http://ipc.localhost asset:`（pdf.js 读 blob、Tauri IPC、asset 协议读 PDF）；`style-src 'self' 'unsafe-inline'`（Naive UI css-render 注入）。起步指令，打包实测后定稿
- **capabilities 最小集**：core:window 默认 + tauri-plugin-window-state + opener（外部查看器预留）；无 fs/shell/http 权限
- **配置写入原子化**：临时文件 + rename

## 8. 工程基建

- **Rust 测试**：cargo test——scheduler 用 fake CompileRunner 单测（队列合并/超时/重试/终止语义）；insta 快照——log_parser 用真实 latexmk 日志固化为用例；texpresso-infra 另有 `#[ignore]` 集成用例（真实 latexmk/synctex：成功、内容错误、超时树杀、取消、中文路径），跑法 `cargo test -p texpresso-infra -- --ignored`
- **前端**：vitest（+ @vue/test-utils）——stores 与 composables（project 路径归一化、editor 自保存过滤/冲突、useAutoSave 防抖、useIdleConvergence、useSyncTex）与 `RootFilePicker` 组件单测，`npm run test`；其余组件测后续补
- **CI（GitHub Actions，MVP 前即搭）**：cargo test + `vue-tsc --noEmit` + `npm run test`（前端单测）+ Windows runner `tauri build` 冒烟（顺带验证 NSIS 打包链路，覆盖 ADR-3）

## 9. 与上层设计的关系

- 本文件是 design.md 之下的第二层：design.md 定产品语义（延迟预算、失败语义、MVP 边界），本文件定实现结构（分层、模块、接口、技术栈）
- 后置未决清单以 design.md 为准；函数级契约、已知债与验证入口见 [modules.md](./modules.md)（§12）。
