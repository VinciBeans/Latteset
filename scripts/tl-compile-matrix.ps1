<#
.SYNOPSIS
  模板语料「引擎兼容矩阵」实测（roadmap ⑲ 模板样本调研）。

.DESCRIPTION
  对一组**真实用户模板**（中文硕博 / 期刊 / 日文控制组）的本机 TeX Live 样例，
  分别用产品默认引擎（XeLaTeX）与对照引擎（pdfLaTeX）各编译一次，记录退出码并
  从 .log 归类失败原因，用来回答：

    「默认 XeLaTeX 的开箱即用率是多少？失败的能不能靠 pdflatex 救回来？」
    —— 这决定 roadmap ②-2（引擎推断）该不该做、做多大。

  设计要点：
  - 每个样本**整目录复制**到临时工作区后再编译（样例常 \input 同目录文件）；
    两种引擎各自一份干净副本，避免 aux 互相污染。
  - `-interaction=nonstopmode`，不加 `-shell-escape`（与产品默认一致）。
  - 失败原因从 .log 文本归类，区分「引擎不兼容」与「缺文件/需 shell-escape」——
    只有前者才是引擎推断能救的。

.PARAMETER Samples
  「文档类=样例文件绝对路径」列表；默认用脚本内置的真实模板清单。

.PARAMETER WorkRoot
  临时工作区（默认 test_file/research/tl-compile，已 gitignore）。

.PARAMETER TimeoutSec
  单次编译超时（默认 90s）。

.EXAMPLE
  pwsh -File scripts/tl-compile-matrix.ps1
  pwsh -File scripts/tl-compile-matrix.ps1 -Samples @{ 'elsarticle' = 'C:\texlive\2026\texmf-dist\doc\latex\elsarticle\elsarticle-template-harv.tex' }
#>
[CmdletBinding()]
param(
  [hashtable]$Samples,
  [string]$WorkRoot = "$PSScriptRoot\..\test_file\research\tl-compile",
  [int]$TimeoutSec = 90
)

$ErrorActionPreference = 'Continue'
$TL = 'C:\texlive\2026\texmf-dist\doc'

# 真实用户模板清单：中文硕博（产品核心客群）+ 期刊/会议 + 日文（CJK 控制组，预期需 platex）
if (-not $Samples) {
  $Samples = [ordered]@{
    # —— 中文：学位论文 / 中文文档类（产品核心客群）——
    'bithesis(北理工)'   = "$TL\latex\bithesis\bithesis-doc.tex"
    'hitszthesis(哈工深)' = "$TL\latex\hitszthesis\hitszthesis-example.tex"
    'bjfuthesis(北林)'   = "$TL\latex\bjfuthesis\example\thesis.tex"
    'mcmthesis(数模)'    = "$TL\latex\mcmthesis\mcmthesis-demo.tex"
    'xtuthesis(湘大/ctexbook)' = "$TL\latex\xtuthesis\xtuthesis.tex"
    'cquthesis(重庆大学)' = "$TL\latex\cquthesis\main.tex"
    'seuthesis(东南)'    = "$TL\latex\seuthesis\sample.tex"
    'shtthesis(上科大)'  = "$TL\latex\shtthesis\shtthesis-user-guide.tex"
    'fduthesis(复旦)'    = "$TL\latex\fduthesis\fduthesis-en.tex"
    'hithesis(哈工大)'   = "$TL\xelatex\hithesis\main.tex"
    # —— 期刊 / 会议 ——
    'elsarticle(Elsevier)' = "$TL\latex\elsarticle\elsarticle-template-harv.tex"
    'aastex701(AAS)'       = "$TL\latex\aastex\aastex701-sample.tex"
    'nature'               = "$TL\latex\nature\nature-template.tex"
    'spie'                 = "$TL\latex\spie\article.tex"
    'acmart(ACM)'          = "$TL\latex\acmart\samples\sample-acmcp.tex"
    'revtex4-2(APS)'       = "$TL\latex\hep-paper\hep-paper-test-revtex.tex"
    'llncs(Springer)'      = "$TL\latex\authorarchive\examples\brucker-authorarchive-2016-llncs-a4.tex"
    'IEEEtran'             = "$TL\latex\authorarchive\examples\brucker-authorarchive-2016-IEEEtran-nourl.tex"
    # —— 日文（CJK 控制组：预期两种引擎都不行，需 platex/uptex）——
    'jsarticle(日文)'      = "$TL\bibtex\ieejtran\ieejtran.tex"
  }
}

$Rows = New-Object System.Collections.Generic.List[object]

function Get-FailureKind {
  param([string]$LogPath)
  if (-not (Test-Path $LogPath)) { return 'no-log' }
  $t = ''
  try { $t = [System.IO.File]::ReadAllText($LogPath) } catch { return 'unreadable-log' }
  # 归类顺序即优先级：先判引擎不兼容，再判外部依赖。
  # 注意：匹配**报错句式**而非关键词——日志正文里提到 "minted"/"XeTeX" 很常见，
  # 用裸关键词会把「文档恰好提到」误判成「失败原因」（首版脚本就踩了这个坑）。
  if ($t -match '(?m)^!.*(?:requires|needs)\s+(?:XeTeX|LuaTeX)') { return 'engine-needs-xetex' }
  if ($t -match '(?m)^!.*only (?:supported|works) (?:by|with) (?:pdfTeX|XeTeX|LuaTeX)') { return 'engine-exclusive' }
  if ($t -match 'This is (e-)?(p|up)TeX|pLaTeX|upLaTeX') { return 'engine-needs-platex' }
  # 注意：`restricted \write18 enabled` 是**每次运行都有的正常行**，不可作为判据
  # （第二版脚本把它写进模式，结果几乎所有失败都被误判成 needs-shell-escape）。
  if ($t -match 'Package minted Error|You must invoke LaTeX with the -shell-escape flag|shell escape is disabled|runsystem\([^)]*\)\.\.\. disabled') { return 'needs-shell-escape' }
  if ($t -match '(?m)^!.*(?:File|file)\s+`[^'']+''\s+not found') { return 'missing-file' }
  if ($t -match '(?m)^!.*[Ff]ont.*not (?:found|loadable)') { return 'missing-font' }
  if ($t -match '(?m)^!.*(?:Font shape|fontspec)') { return 'font-or-fontspec' }
  if ($t -match '(?m)^!') { return 'latex-error' }
  return 'other'
}

function Invoke-Compile {
  param([string]$Name, [string]$SamplePath, [string]$EngineFlag, [string]$EngineLabel)
  $dest = Join-Path $WorkRoot ("{0}__{1}" -f ($Name -replace '[^\w\u4e00-\u9fa5-]', '_'), $EngineLabel)
  if (Test-Path $dest) { Remove-Item $dest -Recurse -Force -ErrorAction SilentlyContinue }
  New-Item -ItemType Directory -Path $dest -Force | Out-Null
  $srcDir = Split-Path $SamplePath -Parent
  # 整目录复制（样例常 \input 同目录文件）；用 robocopy 静默且快
  $null = robocopy $srcDir $dest /E /NFL /NDL /NJH /NJS /NP 2>&1
  $base = Split-Path $SamplePath -Leaf
  $target = Join-Path $dest $base
  if (-not (Test-Path $target)) { return [pscustomobject]@{ exit = -1; kind = 'copy-failed' } }

  $outLog = Join-Path $dest "_stdout.txt"
  $job = Start-Job -ScriptBlock {
    param($d, $b, $flag, $logFile)
    Set-Location $d
    # 输出重定向到文件：只把退出码回传，否则 Receive-Job 会把 latexmk 的全部 stdout
    # 与退出码一起返回成数组（首版脚本因此把 $exit 取成 Object[]）。
    & latexmk $flag -interaction=nonstopmode -outdir=tmp $b *> $logFile
    return $LASTEXITCODE
  } -ArgumentList $dest, $base, $EngineFlag, $outLog
  $done = Wait-Job $job -Timeout $TimeoutSec
  if (-not $done) {
    Stop-Job $job -ErrorAction SilentlyContinue
    Get-Process latexmk, xelatex, pdflatex, perl -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Remove-Job $job -Force -ErrorAction SilentlyContinue
    return [pscustomobject]@{ exit = -2; kind = 'timeout' }
  }
  $exit = @(Receive-Job $job)[-1]
  Remove-Job $job -Force -ErrorAction SilentlyContinue
  if ($exit -isnot [int]) { $exit = -1 }

  $logName = [System.IO.Path]::GetFileNameWithoutExtension($base) + '.log'
  $logPath = Join-Path $dest "tmp\$logName"
  $kind = if ($exit -eq 0) { 'ok' } else { Get-FailureKind $logPath }
  # 首条 `!` 错误原文：让归类可被人工复核（报告里直接引用，避免"黑箱结论"）
  $firstError = ''
  if ($exit -ne 0 -and (Test-Path $logPath)) {
    try {
      $lm = [regex]::Match([System.IO.File]::ReadAllText($logPath), '(?m)^!.*$')
      if ($lm.Success) { $firstError = $lm.Value.Trim() }
    } catch { }
  }
  return [pscustomobject]@{ exit = $exit; kind = $kind; firstError = $firstError }
}

Write-Host "工作区: $WorkRoot"
Write-Host ("样本数: {0}`n" -f $Samples.Count)

foreach ($name in $Samples.Keys) {
  $sample = $Samples[$name]
  if (-not (Test-Path $sample)) {
    Write-Host ("{0,-24} 样例不存在，跳过" -f $name)
    $Rows.Add([pscustomobject]@{ sample = $name; xelatex = 'missing'; xelatex_kind = 'sample-absent'; pdflatex = 'missing'; pdflatex_kind = 'sample-absent' })
    continue
  }
  $xe = Invoke-Compile -Name $name -SamplePath $sample -EngineFlag '-xelatex' -EngineLabel 'xe'
  $pd = Invoke-Compile -Name $name -SamplePath $sample -EngineFlag '-pdf' -EngineLabel 'pdf'
  Write-Host ("{0,-24} xelatex={1,-4}({2,-20}) pdflatex={3,-4}({4})" -f $name, $xe.exit, $xe.kind, $pd.exit, $pd.kind)
  if ($xe.firstError) { Write-Host ("    xe! {0}" -f $xe.firstError) }
  if ($pd.firstError) { Write-Host ("    pd! {0}" -f $pd.firstError) }
  $Rows.Add([pscustomobject]@{
    sample = $name; xelatex = $xe.exit; xelatex_kind = $xe.kind; xelatex_err = $xe.firstError
    pdflatex = $pd.exit; pdflatex_kind = $pd.kind; pdflatex_err = $pd.firstError
  })
}

$out = Join-Path $WorkRoot 'matrix.csv'
$Rows | Export-Csv -Path $out -NoTypeInformation -Encoding UTF8
Write-Host "`n矩阵已写入: $out"
$xOk = @($Rows | Where-Object { $_.xelatex -eq 0 }).Count
$pOk = @($Rows | Where-Object { $_.pdflatex -eq 0 }).Count
Write-Host ("xelatex 通过 {0}/{1}；pdflatex 通过 {2}/{1}" -f $xOk, $Rows.Count, $pOk)
