# scripts/ —— 工具脚本索引

> **这里不按主题分子目录**：约 50 处文档逐字引用 `node scripts/<name>.mjs`（`docs/modules.md` §12.3、
> `docs/design.md` 的基准节、各份 research 报告），搬迁只会造成大面积链接失效而换不到实际收益
> （2026-09 审计结论）。**新增脚本请同时在本表登记一行。**

## 构建与前置（package.json 直接调用）

| 脚本 | 用途 | 调用处 |
|---|---|---|
| `copy-pdf-worker.mjs` | 把 pdfjs 的 worker/cmaps 拷进 `public/pdfjs/`（`?worker&inline` 在 Tauri 下有 MIME/生命周期坑） | `npm run dev/build`（自动） |
| `with-tectonic-lib.ps1` | **库形态的唯一入口**：vcpkg 环境（`RUSTFLAGS`/triplet）+ `-Mode check\|dev\|build\|test\|cli` | `npm run lib:*`、CI 的 tag 档 |

## 基准与门禁（超预算/不确定即退出码 1）

| 脚本 | 用途 |
|---|---|
| `gen-bench-projects.mjs` | 生成六档基准夹具（夹具不入库，脚本入库）→ `test_file/projects/bench/*` |
| `bench.mjs` | 延迟预算报告：冷编译 / 空跑 / 编辑触发 / 端到端 |
| `bench-single-pass.mjs` | 编辑期"单趟 xelatex" vs 完整 latexmk 的 A/B（roadmap ㉘ 的收益评估） |
| `check-determinism.mjs` | 构建确定性：同一份源码连跑两次，PDF 是否**逐字节**相同（`--without-epoch` 可复现非确定性） |
| `validate-pdf.mjs` | PDF 页数 / 文本层断言（库形态复核用） |

## 编辑器与预览（真机探针）

| 脚本 | 用途 |
|---|---|
| `gen-large-project.mjs` | 生成"多文件大项目"编辑器侧夹具（三档） |
| `editor-report.mjs` | **WS 直驱真机的 8 探针**（每击键 / 折叠 / 大纲往返 / 内存…），超门槛退出码 1 |
| `gen-stream-fixture.mjs` | 生成"流式输出"验证用的长文档夹具（400 KB / 162 页，`--error` 造错误流） |

## SyncTeX 与 DVI/XDV 研究

| 脚本 | 用途 |
|---|---|
| `synctex-report.mjs` | 双向定位精度基线（三组样本 × 正反向 × 往返"跳到位"） |
| `synctex-selfcheck.mjs` | 进程内自解析 vs `synctex` CLI 逐点对拍 |
| `xdv-report.mjs` | XDV 页索引 / 页级差分 / 截断可读性（`--diff=` / `--truncate-at=` / `--watch=`） |
| `xdv-inc.mjs` | 追加式（增量）页索引：`--selftest` 与全量逐字段对拍、`--cost=` 成本对比 |
| `xdv-partial.mjs` | 半成品 XDV 合成器（流式出图的前置件，roadmap ㉞） |
| `tectonic-lib-xdvscan.mjs` | 库形态下的 XDV 扫描复核 |
| `bench-tectonic-lib.mjs` | 库形态的**常驻 / 冷启**性能对照（tag 档发布构建的配套测量） |

## 依赖与其它

| 脚本 | 用途 | 备注 |
|---|---|---|
| `fls-report.mjs` | `.fls` / `.fdb_latexmk` 依赖差集（G1 研究） | 研究工具，**未接线**到产品 |
| `render-diagrams.mjs` | 架构图渲染（Mermaid → `docs/diagrams/`） | ⚠ **本机跑不通**（缺 mermaid + 无头浏览器，ADR-0011 记） |
| `export-logo.mjs` | Logo PNG 导出 | ⚠ 依赖 `@resvg/resvg-js`（**不在 package.json 里**，需手动装） |
| `gen-log-error-corpus.ps1` | 生成 `crates/latteset-core/src/log_parser/real_error_corpus.rs`（真实错误语料） | 测试生成器，改语料时跑 |
| `tl-compile-matrix.ps1` | TeX Live 模板编译矩阵（语料调研用） | 一次性调研，结论已进 `docs/research/` |
