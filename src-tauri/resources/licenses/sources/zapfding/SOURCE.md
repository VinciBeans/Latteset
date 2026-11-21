# zapfding（pzdr.tfm）Corresponding Source

随包分发的 `pzdr.tfm`（TL zapfding 包，GPL 许可）由 `pzdr.afm` 经 dvips 的 `afm2tfm` 生成。
本目录附 AFM（度量源数据，取自 TeX Live 2026 `fonts/afm/adobe/zapfding/pzdr.afm`）。

- 生成命令：`afm2tfm pzdr.afm -v pzdr.vpl -o pzdr.tfm`（dvips 构建链，参数以 TL 2024-03-12 的 zapfding 包为准）
- 生成器（GPL）源码：https://github.com/TeX-Live/texlive-source 的 `texk/dvips/`（TeX Live 2024 分支）
- 注意：`pzdr.tfm` 只含 TFM 度量，不含字体轮廓；ZapfDingbats 轮廓由 PDF 阅读器/系统字体提供。
