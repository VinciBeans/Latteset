# Fandol 字体 Corresponding Source

随本安装包分发的 6 个 Fandol OTF 的 **Corresponding Source**（GPLv3 §6）由以下上游源码提供：

- 仓库：https://github.com/Yixf-Self/fandol-fonts
- 提交（pin）：`b93300821373d3092e378e57ae22ecb8e9082c62`
- 上游项目页：https://fandol.github.io/（作者：Clerk Ma & Jie Su）
- 字体二进制来源：CTAN `fonts/fandol`（与上游 OTF 同源；本包只改分包方式，不改字体内容）

每个 `<font>/` 目录含 `cidfont.ps`（CID Type1 轮廓）、`AFM`（度量）、`features`（OpenType 特性）、`fontinfo`、`cidfontinfo`、`FontMenuNameDB`（AFDKO 构建输入）。构建工具链为 Adobe AFDKO（`makeotf`），对应二进制与源码映射见 `data/sources.json`。

许可见 `COPYING`（GPLv3 + 字体嵌入例外：把本字体嵌入文档不会使文档本身受 GPL 约束）。
复现：用 AFDKO 的 `makeotf` 以各目录为工作目录生成 `<font>.otf`；产物哈希随包记录在 manifest.json。
