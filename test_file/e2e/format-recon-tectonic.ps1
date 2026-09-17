# ㊹ 侦察（2/2，**Tectonic 库形态**路线）：把项目导言区固化成 format，到底行不行。
#
# 为什么重做：第一轮（test_file/e2e/format-recon.ps1）全部跑在 `xelatex + mylatexformat` 上，
# 按 ADR-0014（以 Tectonic 为准）那是**非目标引擎**的证据 ⇒ 不适用。Tectonic 的机制不同：
#   · format 趟由 `format_primary_source()` 合成的 `\input tectonic-format-latex.tex` 驱动
#     （`crates/latteset-tectonic/src/io.rs:478`），源文件从 **bundle** 读 ⇒ 改 bundle 里那个 19 B 的
#     文件就等于换掉"format 里装什么"，**一行 Rust 都不用改**；
#   · 产物落在产品缓存的 `formats/<digest>-latex-33.fmt`（24.4 MB），`input_open_format` 直接供引擎读；
#   · bundle 里 `xelatex.ini`（486 B）与 `latex.ltx`（540 KB）都在，`mylatexformat.ltx` **不在**
#     （而且它在 TeX Live 上加载即崩，见第一轮）。
#
# 三个问题：
#   ① 能不能造出**装着项目导言区**的 format（判据：正文里用导言区定义的宏，不用带导言区就能编过）
#   ② 字节一致：用该 format 编译 vs 逐字读导言区编译，页哈希与 PDF 是否相同
#   ③ 提速：`typeset_ms` 降多少（重导言区当前每趟白付 ≈260 ms）
#
# ⚠ 本脚本会**临时改写 bundle 里的 `tectonic-format-latex.tex`、删除产品缓存里的 `.fmt`**，
#   结束后**自动还原**（bundle 文件按备份写回；缓存 `.fmt` 删掉让下次编译重建标准 format）。
#
# 用法（工作区产物，未提交）：.\test_file\e2e\format-recon-tectonic.ps1
param(
    [string]$Cli = "src-tauri\target\release\latteset-cli.exe",
    [string]$Bundle = "file:///E:/Works/tex-presso/test_file/tectonic-bundle",
    [string]$Cache = "",   # 省略 ⇒ 工作区内的实验缓存目录（沙箱不许写 %LOCALAPPDATA%）
    [string]$Out = "test_file\e2e\format-recon-tectonic.json"
)
$ErrorActionPreference = 'Stop'
if (-not $Cache) { $Cache = Join-Path (Get-Location) 'test_file\e2e\fmt-cache-lab' }
New-Item -ItemType Directory -Force -Path $Cache | Out-Null
$env:LATTESET_TECTONIC_BUNDLE = $Bundle
$env:LATTESET_TECTONIC_CACHE = $Cache
$env:LATTESET_TECTONIC_LIB = '1'
$repo = (Get-Location).Path
$bundleDir = Join-Path $repo 'test_file\tectonic-bundle'
$fmtSource = Join-Path $bundleDir 'tectonic-format-latex.tex'
$fmtCacheDir = Join-Path $Cache 'formats'
$projRoot = (Resolve-Path 'test_file\projects\bench\large').Path
$lab = Join-Path $projRoot 'tmp\fmt-recon-tectonic'

# ---- 被测的项目导言区（"项目"= 一个宏 + 一个宏包）与两种文档 ----
$preamble = "\documentclass{article}`n\usepackage{amsmath}`n\newcommand{\Half}{\frac{1}{2}}`n"
$bodyOnly = "\begin{document}`n`$\Half + \int_0^1 x^2\,\mathrm{d}x`$`n\end{document}`n"
$withPreamble = "\documentclass{article}`n\nofiles`n\usepackage{amsmath}`n\newcommand{\Half}{\frac{1}{2}}`n" + $bodyOnly
$bodyOnlyNoFiles = "\nofiles`n" + $bodyOnly

# 自定义 format 源：抑制内核自带的 \dump，插进项目导言区，再 dump。
# （`latex.ltx` 结尾是**无条件 `\dump`**，上游没有注入钩子 ⇒ 只能这样绕；见 roadmap §6.12。）
function New-FormatSource([string]$Preamble) {
    "\let\oridump\dump`n\let\dump\relax`n\input xelatex.ini`n\let\dump\oridump`n" + $Preamble + "\dump`n"
}

function Invoke-LibCompile([string]$Dir, [string]$Tag) {
    if (Test-Path $Dir) { Remove-Item $Dir -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    $raw = & $Cli --project $Dir compile --quick 2>&1 | Out-String
    $i = $raw.IndexOf('{'); $j = $raw.LastIndexOf('}')
    $json = $raw.Substring($i, $j - $i + 1) | ConvertFrom-Json
    $kv = ([regex]::Match($raw, '分阶段耗时(?<kv>.*)')).Groups['kv'].Value
    $pagesFile = Join-Path $Dir 'tmp\main.tectonic.pages'
    $hash = if (Test-Path $pagesFile) { ((Get-Content $pagesFile | Select-Object -Skip 1) -join '|') } else { $null }
    $pdf = Join-Path $Dir 'main.pdf'
    return [ordered]@{
        tag       = $Tag
        status    = $json.compile.status
        error     = @($json.compile.errors | ForEach-Object { ($_.message -replace "`n", ' ') } | Select-Object -First 1)
        elapsed   = ([regex]::Match($raw, '"elapsed_ms":\s*(\d+)')).Groups[1].Value
        format_ms = ([regex]::Match($kv, 'format_ms=(\d+)')).Groups[1].Value
        typeset   = ([regex]::Match($kv, 'typeset_ms=(\d+)')).Groups[1].Value
        convert   = ([regex]::Match($kv, 'convert_ms=(\d+)')).Groups[1].Value
        pages     = ([regex]::Match($kv, 'pages=(\d+)')).Groups[1].Value
        cached    = ([regex]::Match($kv, 'format_cached=(\w+)')).Groups[1].Value
        page_hash = $hash
        pdf_sha   = if (Test-Path $pdf) { (Get-FileHash $pdf -Algorithm SHA256).Hash } else { $null }
    }
}
function Write-Doc([string]$Dir, [string]$Doc) {
    if (Test-Path $Dir) { Remove-Item $Dir -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    [IO.File]::WriteAllText((Join-Path $Dir 'main.tex'), $Doc, (New-Object Text.UTF8Encoding($false)))
}

$report = [ordered]@{}
$backup = [IO.File]::ReadAllText($fmtSource)
Write-Host "bundle 原 format 源（备份）：$($backup.Trim())"
try {
    # ================= ① 基线：标准 format + 逐字读导言区 =================
    $baseDir = Join-Path $lab 'base'
    Write-Doc $baseDir $withPreamble
    $base = Invoke-LibCompile $baseDir '标准 format + 带导言区'
    Write-Host ("`n[基线] status={0} elapsed={1} typeset={2} convert={3} pages={4} cached={5}" -f $base.status, $base.elapsed, $base.typeset, $base.convert, $base.pages, $base.cached)
    Write-Host ("       页哈希 {0}" -f $base.page_hash)
    $report['baseline'] = $base
    if ($base.status -ne 'success') { Write-Host ("✗ 基线都编不过，侦察到此为止：{0}" -f $base.error) -ForegroundColor Red; throw 'baseline failed' }

    # ================= ② 换 format 源 ⇒ 删缓存 ⇒ 让 Tectonic 重建 =================
    [IO.File]::WriteAllText($fmtSource, (New-FormatSource $preamble), (New-Object Text.UTF8Encoding($false)))
    $removed = @(Get-ChildItem $fmtCacheDir -Filter '*.fmt' -ErrorAction SilentlyContinue)
    foreach ($f in $removed) { Remove-Item $f.FullName -Force }
    Write-Host ("`n已改写 format 源（{0} B）并删除缓存 format：{1}" -f (Get-Item $fmtSource).Length, ($removed.Name -join ', '))

    # 自定义 format + **只有文档体**（导言区应从 format 里来）
    $customDir = Join-Path $lab 'custom'
    Write-Doc $customDir $bodyOnlyNoFiles
    $custom = Invoke-LibCompile $customDir '自定义 format + 仅文档体'
    Write-Host ("[自定义] status={0} elapsed={1} format_ms={2} typeset={3} convert={4} pages={5} cached={6}" -f $custom.status, $custom.elapsed, $custom.format_ms, $custom.typeset, $custom.convert, $custom.pages, $custom.cached)
    if ($custom.status -ne 'success') { Write-Host ("       错误：{0}" -f $custom.error) -ForegroundColor Red }
    Write-Host ("       页哈希 {0}" -f $custom.page_hash)
    # 判据①：`\R` 是导言区定义的宏 ⇒ 不带导言区还能编过，就说明 format 里确实装了导言区
    $macroFromFormat = ($custom.status -eq 'success')
    # 判据②：页哈希与基线相同 ⇒ 输出一致
    $sameHash = ($custom.page_hash -and $custom.page_hash -eq $base.page_hash)
    $samePdf = ($custom.pdf_sha -and $custom.pdf_sha -eq $base.pdf_sha)
    Write-Host ("  ⇒ 宏来自 format？ {0}   页哈希相同？ {1}   PDF 相同？ {2}" -f $macroFromFormat, $sameHash, $samePdf) -ForegroundColor $(if ($sameHash -or $samePdf) { 'Green' } else { 'Yellow' })
    $report['custom_format'] = $custom
    $report['verdict'] = [ordered]@{ macro_from_format = $macroFromFormat; same_page_hash = $sameHash; same_pdf = $samePdf }

    # ================= ③ 重导言区（ctexbook+tikz）能不能进 format =================
    $heavy = "\documentclass[UTF8]{ctexbook}`n\usepackage{tikz}`n\usepackage{amsmath}`n"
    [IO.File]::WriteAllText($fmtSource, (New-FormatSource $heavy), (New-Object Text.UTF8Encoding($false)))
    foreach ($f in @(Get-ChildItem $fmtCacheDir -Filter '*.fmt' -ErrorAction SilentlyContinue)) { Remove-Item $f.FullName -Force }
    $heavyDir = Join-Path $lab 'heavy'
    Write-Doc $heavyDir ("\nofiles`n\begin{document}`n`$E=mc^2`$`n\end{document}`n")
    $heavyRun = Invoke-LibCompile $heavyDir '重导言区 format + 仅文档体'
    Write-Host ("`n[重导言区] status={0} elapsed={1} format_ms={2} typeset={3} pages={4}" -f $heavyRun.status, $heavyRun.elapsed, $heavyRun.format_ms, $heavyRun.typeset, $heavyRun.pages)
    if ($heavyRun.status -ne 'success') { Write-Host ("       错误：{0}" -f $heavyRun.error) -ForegroundColor Yellow }
    $report['heavy_format'] = $heavyRun
} finally {
    # ================= 还原 =================
    [IO.File]::WriteAllText($fmtSource, $backup, (New-Object Text.UTF8Encoding($false)))
    foreach ($f in @(Get-ChildItem $fmtCacheDir -Filter '*.fmt' -ErrorAction SilentlyContinue)) { Remove-Item $f.FullName -Force }
    Write-Host "`n已还原 bundle 的 format 源并清掉缓存 .fmt（下次编译自动重建标准 format）"
}

$report['when'] = (Get-Date -Format s)
$report['note'] = 'Tectonic 库形态；--quick + \nofiles；format 源 = 抑制 latex.ltx 的 \dump → 插导言区 → \dump'
$report | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 $Out
Write-Host "证据已落：$Out"
