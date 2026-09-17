# ㊹ 侦察（1/2）：把项目导言区固化成 format —— 用真工具实测（TeX Live 2026 + mylatexformat 3.4）。
#
# 四个问题（== roadmap §4 的 ㊹ 出口条件）：
#   ① 字节一致：用 format 编译 vs 逐字读导言区编译，.xdv/.pdf 是否**逐字节相同**（判据核心）
#   ② 提速：每次编译省多少、format 一次性构建多久
#   ③ 宏存活：导言区定义的宏进 format 后在正文里还能用吗
#   ④ 失败面：哪些宏包进不了 format（`hyperref` / `biblatex` 是经典嫌疑）
#
# 踩过的三处（写在这里免得下次再踩）：
#   · `xetex -initialize` 是 **MiKTeX** 写法；TeX Live 用 `-ini`；
#   · `mylatexformat.ltx` 要传**裸文件名**（让 kpathsea 找），给绝对路径会 "Please type another
#     input file name"，什么都 dump 不出来；
#   · PowerShell 里带 `&` 的参数走 `cmd /c` 最稳（`&` 是 PS 的调用运算符）。
#
# 用法（工作区产物，未提交）：.\test_file\e2e\format-recon.ps1
param(
    [string]$Out = "test_file\e2e\format-recon.json",
    [int]$Samples = 3
)
$ErrorActionPreference = 'Stop'
$env:SOURCE_DATE_EPOCH = '1700000000'
$repo = (Get-Location).Path
$lab = Join-Path $repo 'test_file\e2e\format-recon'
if (Test-Path $lab) { Remove-Item $lab -Recurse -Force }
New-Item -ItemType Directory -Force -Path $lab | Out-Null

$cases = [ordered]@{
    'L 轻（article+amsmath）' = "\documentclass{article}`n\usepackage{amsmath,amssymb}`n\newcommand{\R}{\mathbb{R}}`n"
    'H 重（ctexbook+tikz）'   = "\documentclass[UTF8]{ctexbook}`n\usepackage{tikz}`n\usepackage{amsmath}`n\newcommand{\R}{\mathbb{R}}`n"
    'X 含 hyperref'           = "\documentclass{article}`n\usepackage{amsmath}`n\usepackage{hyperref}`n\newcommand{\R}{\mathbb{R}}`n"
    'B 含 biblatex'           = "\documentclass{article}`n\usepackage{amsmath}`n\usepackage{biblatex}`n\newcommand{\R}{\mathbb{R}}`n"
}
$body = "\begin{document}`n行内 `$\R^n`$ 与 `$\int_0^1 x^2\,\mathrm{d}x`$，行间：`n\[`n  \sum_{k=1}^{n} k = \frac{n(n+1)}{2}`n\]`n\end{document}`n"

function Sha([string]$Path) { if (Test-Path $Path) { (Get-FileHash $Path -Algorithm SHA256).Hash } else { $null } }

function Measure-Run([string]$Dir, [string[]]$LatexArgs) {
    $times = @(); $codes = @()
    for ($i = 1; $i -le $Samples; $i++) {
        foreach ($stale in @('main.xdv','main.pdf','main.aux','main.toc','main.out','main.log')) {
            $p = Join-Path $Dir $stale; if (Test-Path $p) { Remove-Item $p -Force }
        }
        Push-Location $Dir
        try {
            $sw = [Diagnostics.Stopwatch]::StartNew()
            $null = & xelatex @LatexArgs 2>&1 | Out-String
            $sw.Stop()
            $codes += $LASTEXITCODE
        } finally { Pop-Location }
        $times += [int]$sw.Elapsed.TotalMilliseconds
    }
    return @{ ms = $times; codes = $codes }
}

$report = [ordered]@{}
$i = 0
foreach ($name in $cases.Keys) {
    $i++
    $dir = Join-Path $lab "case$i"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    [IO.File]::WriteAllText((Join-Path $dir 'main.tex'), ($cases[$name] + $body), (New-Object Text.UTF8Encoding($false)))
    $fmtName = "pre$i"
    Write-Host ""
    Write-Host "=== $name ===" -ForegroundColor Cyan

    $baseXdv = Measure-Run $dir @('-interaction=nonstopmode', '-no-pdf', 'main.tex')
    if (Test-Path (Join-Path $dir 'main.xdv')) { Copy-Item (Join-Path $dir 'main.xdv') (Join-Path $dir 'base.xdv') -Force }
    $basePdf = Measure-Run $dir @('-interaction=nonstopmode', 'main.tex')
    if (Test-Path (Join-Path $dir 'main.pdf')) { Copy-Item (Join-Path $dir 'main.pdf') (Join-Path $dir 'base.pdf') -Force }
    Write-Host ("  基线      xdv {0} ms [{1}] | pdf {2} ms [{3}]" -f ($baseXdv.ms -join '/'), ($baseXdv.codes -join '/'), ($basePdf.ms -join '/'), ($basePdf.codes -join '/'))

    Push-Location $dir
    try {
        $swB = [Diagnostics.Stopwatch]::StartNew()
        $null = & cmd /c "xetex -ini -interaction=nonstopmode -jobname=$fmtName `"&xelatex`" mylatexformat.ltx main.tex" 2>&1 | Out-String
        $swB.Stop()
    } finally { Pop-Location }
    $fmtPath = Join-Path $dir "$fmtName.fmt"
    if (-not (Test-Path $fmtPath)) {
        $tail = ((Get-Content (Join-Path $dir "$fmtName.log") -Tail 8 -ErrorAction SilentlyContinue) -join ' | ')
        Write-Host ("  ✗ 造 format 失败：{0}" -f $tail.Substring(0, [Math]::Min(150, $tail.Length))) -ForegroundColor Red
        $report[$name] = [ordered]@{ fmt_built = $false; base_xdv_ms = $baseXdv.ms; base_pdf_ms = $basePdf.ms; log_tail = $tail }
        continue
    }
    Write-Host ("  造 format {0} ms，{1} KB" -f [int]$swB.Elapsed.TotalMilliseconds, [int]((Get-Item $fmtPath).Length / 1024))

    $fmtXdv = Measure-Run $dir @('-interaction=nonstopmode', '-no-pdf', "-fmt=$fmtName", 'main.tex')
    if (Test-Path (Join-Path $dir 'main.xdv')) { Copy-Item (Join-Path $dir 'main.xdv') (Join-Path $dir 'fmt.xdv') -Force }
    $fmtPdf = Measure-Run $dir @('-interaction=nonstopmode', "-fmt=$fmtName", 'main.tex')
    if (Test-Path (Join-Path $dir 'main.pdf')) { Copy-Item (Join-Path $dir 'main.pdf') (Join-Path $dir 'fmt.pdf') -Force }
    $sameXdv = ((Sha (Join-Path $dir 'base.xdv')) -ne $null) -and ((Sha (Join-Path $dir 'base.xdv')) -eq (Sha (Join-Path $dir 'fmt.xdv')))
    $samePdf = ((Sha (Join-Path $dir 'base.pdf')) -ne $null) -and ((Sha (Join-Path $dir 'base.pdf')) -eq (Sha (Join-Path $dir 'fmt.pdf')))
    $log = Join-Path $dir 'main.log'
    $undef = if (Test-Path $log) { (Select-String -Path $log -Pattern 'Undefined control sequence' | Measure-Object).Count } else { -1 }
    $clsLoaded = if (Test-Path $log) { @(Select-String -Path $log -Pattern 'Document Class:').Count } else { -1 }
    Write-Host ("  用 format xdv {0} ms [{1}] | pdf {2} ms [{3}]" -f ($fmtXdv.ms -join '/'), ($fmtXdv.codes -join '/'), ($fmtPdf.ms -join '/'), ($fmtPdf.codes -join '/'))
    Write-Host ("  ⇒ 逐字节相同？ xdv={0} pdf={1}（Undefined={2}，类加载行={3}）" -f $sameXdv, $samePdf, $undef, $clsLoaded) -ForegroundColor $(if ($sameXdv -and $samePdf) { 'Green' } else { 'Red' })
    $avgBase = ($baseXdv.ms | Measure-Object -Average).Average
    $avgFmt = ($fmtXdv.ms | Measure-Object -Average).Average
    $speed = [math]::Round($avgBase / [math]::Max($avgFmt, 1), 2)

    $report[$name] = [ordered]@{
        fmt_built = $true; fmt_ms = [int]$swB.Elapsed.TotalMilliseconds; fmt_kb = [int]((Get-Item $fmtPath).Length / 1024)
        base_xdv_ms = $baseXdv.ms; fmt_xdv_ms = $fmtXdv.ms; base_pdf_ms = $basePdf.ms; fmt_pdf_ms = $fmtPdf.ms
        base_xdv_sha = Sha (Join-Path $dir 'base.xdv'); fmt_xdv_sha = Sha (Join-Path $dir 'fmt.xdv')
        base_pdf_sha = Sha (Join-Path $dir 'base.pdf'); fmt_pdf_sha = Sha (Join-Path $dir 'fmt.pdf')
        same_xdv = $sameXdv; same_pdf = $samePdf; undefined = $undef; class_lines = $clsLoaded; speedup = $speed
    }
}

$report['when'] = (Get-Date -Format s)
$report['note'] = 'TeX Live 2026 xelatex + mylatexformat 3.4；SOURCE_DATE_EPOCH=1700000000；每轮清 aux'
$report | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 $Out
Write-Host "`n证据已落：$Out"
