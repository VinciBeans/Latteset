<#
.SYNOPSIS
  生成「真实错误日志语料」Rust 数据文件（roadmap ④ 错误诊断升级）。

.DESCRIPTION
  两类来源，全部是**真实编译产物**，不是手写样例：
    A. 收割：从 ⑲ 模板编译矩阵的工作区（test_file/research/tl-compile）里提取失败样本的
       .log 片段——那批日志来自 19 个真实模板的实际编译；
    B. 生成：编译若干**故意写错**的最小文档（缺包/括号/数学模式/对齐等），取真实报错。

  对每条只截取「错误上下文窗口」：向前扩到最近的 `(` 文件标记（让 parse_log 能带出 file），
  向后扩到 `l.<n>` 之后两行——既保留真实结构，又不至于把 20–50KB 的整份 log 入库。

.OUTPUT
  crates/latteset-core/src/log_parser/real_error_corpus.rs
  （**自动生成，勿手改**；期望值由手写的 diagnosis_tests.rs 提供，避免自证循环）

.EXAMPLE
  pwsh -File scripts/gen-log-error-corpus.ps1
#>
[CmdletBinding()]
param(
  [string]$SurveyWork = "$PSScriptRoot\..\test_file\research\tl-compile",
  [string]$ScratchDir = "$PSScriptRoot\..\test_file\research\error-probes",
  [string]$OutFile = "$PSScriptRoot\..\crates\latteset-core\src\log_parser\real_error_corpus.rs"
)

$ErrorActionPreference = 'Continue'

# ---------------------------------------------------------------- 截取错误上下文窗口
function Get-ErrorWindow {
  param([string]$Text)
  $lines = $Text -split "`r?`n"
  $idx = -1
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^!') { $idx = $i; break }
  }
  if ($idx -lt 0) { return $null }
  # 向前：最近的 `(` 文件标记（最多 40 行）
  $start = [Math]::Max(0, $idx - 40)
  for ($i = $idx; $i -ge $start; $i--) {
    if ($lines[$i] -match '^\(') { $start = $i; break }
  }
  # 向后：`l.<n>` 之后两行（最多 25 行）
  $end = [Math]::Min($lines.Count - 1, $idx + 25)
  for ($i = $idx; $i -le $end; $i++) {
    if ($lines[$i] -match '^l\.\d+') { $end = [Math]::Min($lines.Count - 1, $i + 2); break }
  }
  return (($lines[$start..$end]) -join "`n")
}

$cases = New-Object System.Collections.Generic.List[object]

# ---------------------------------------------------------------- A. 收割 ⑲ 的真实日志
if (Test-Path $SurveyWork) {
  Write-Host "=== A. 从 ⑲ 编译矩阵收割 ==="
  $map = [ordered]@{
    'aastex-missing-logo'      = 'acmart_ACM___xe'
    'undefined-cs-dtx'         = 'bithesis_北理工___xe'
    'engine-fontspec-under-pdftex' = 'cquthesis_重庆大学___pdf'
    'siunitx-invalid-number'   = 'cquthesis_重庆大学___xe'
    'engine-unicode-math-under-pdftex' = 'fduthesis_复旦___pdf'
    'missing-font-source-han'  = 'fduthesis_复旦___xe'
    'emergency-stop'           = 'hithesis_哈工大___pdf'
    'missing-ctex-fontset'     = 'hithesis_哈工大___xe'
    'option-clash-natbib'      = 'hitszthesis_哈工深___xe'
    'missing-package-relative' = 'llncs_Springer___xe'
    'needs-platex-format'      = 'jsarticle_日文___xe'
    'missing-class-seuthesis'  = 'seuthesis_东南___xe'
    'engine-shtthesis-only'    = 'shtthesis_上科大___pdf'
    'missing-package-slashbox' = 'xtuthesis_湘大_ctexbook___xe'
  }
  foreach ($name in $map.Keys) {
    $dir = Join-Path $SurveyWork $map[$name]
    $log = Get-ChildItem (Join-Path $dir 'tmp') -Filter *.log -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $log) { Write-Host ("  [跳过] {0}（无 log）" -f $name); continue }
    $win = Get-ErrorWindow ([System.IO.File]::ReadAllText($log.FullName))
    if ($win) { $cases.Add([pscustomobject]@{ name = $name; src = "⑲:$($map[$name])"; log = $win }); Write-Host ("  [OK]   {0}" -f $name) }
    else { Write-Host ("  [跳过] {0}（无 ! 行）" -f $name) }
  }
} else {
  Write-Host "=== A. 跳过：⑲ 工作区不存在（$SurveyWork）==="
}

# ---------------------------------------------------------------- B. 编译故意写错的探针
Write-Host "`n=== B. 编译故意写错的探针 ==="
$probes = [ordered]@{
  'unclosed-brace'      = "\documentclass{article}`n\begin{document}`n\textbf{unclosed`n\end{document}`n"
  'runaway-argument'    = "\documentclass{article}`n\newcommand{\foo}[1]{#1}`n\begin{document}`n\foo{unclosed`n\end{document}`n"
  'missing-math-mode'   = "\documentclass{article}`n\begin{document}`nThe value is x_1 and y^2.`n\end{document}`n"
  'too-many-braces'     = "\documentclass{article}`n\begin{document}`nText} more`n\end{document}`n"
  'include-missing-file'= "\documentclass{article}`n\begin{document}`n\include{nope}`n\end{document}`n"
  'end-without-begin'   = "\documentclass{article}`nHello`n\end{document}`n"
  'double-subscript'    = "\documentclass{article}`n\begin{document}`n`$x_a_b`$`n\end{document}`n"
  'misplaced-alignment' = "\documentclass{article}`n\begin{document}`n\begin{tabular}{cc}`na & b ``\``n\end{tabular}`noutside & here`n\end{document}`n"
  'file-ended-scanning' = "\documentclass{article}`n\begin{document}`nHello`n"
}

New-Item -ItemType Directory -Path $ScratchDir -Force | Out-Null
foreach ($name in $probes.Keys) {
  $d = Join-Path $ScratchDir $name
  if (Test-Path $d) { Remove-Item $d -Recurse -Force }
  New-Item -ItemType Directory -Path $d -Force | Out-Null
  [System.IO.File]::WriteAllText((Join-Path $d 'main.tex'), $probes[$name], (New-Object System.Text.UTF8Encoding($false)))
  Push-Location $d
  & latexmk -xelatex -interaction=nonstopmode -outdir=tmp main.tex *> (Join-Path $d '_stdout.txt')
  Pop-Location
  $log = Join-Path $d 'tmp\main.log'
  if (-not (Test-Path $log)) { Write-Host ("  [跳过] {0}（无 log）" -f $name); continue }
  $win = Get-ErrorWindow ([System.IO.File]::ReadAllText($log))
  if ($win) { $cases.Add([pscustomobject]@{ name = $name; src = "probe:$name"; log = $win }); Write-Host ("  [OK]   {0}" -f $name) }
  else { Write-Host ("  [跳过] {0}（无 ! 行）" -f $name) }
}

# GBK 探针（非 UTF-8 源）：必须显式用 936 编码写入，且用 pdflatex——xelatex 会自行替换非法字节，
# 只有 pdflatex 会把原始 GBK 字节回显进日志（P0-① 实测结论）。
$gbkDir = Join-Path $ScratchDir 'non-utf8-source'
if (Test-Path $gbkDir) { Remove-Item $gbkDir -Recurse -Force }
New-Item -ItemType Directory -Path $gbkDir -Force | Out-Null
$gbkText = "\documentclass{article}`n\begin{document}`n中文GBK编码内容`n\end{document}`n"
[System.IO.File]::WriteAllText((Join-Path $gbkDir 'main.tex'), $gbkText, [System.Text.Encoding]::GetEncoding(936))
Push-Location $gbkDir
& latexmk -pdf -interaction=nonstopmode -outdir=tmp main.tex *> (Join-Path $gbkDir '_stdout.txt')
Pop-Location
$gbkLog = Join-Path $gbkDir 'tmp\main.log'
if (Test-Path $gbkLog) {
  $win = Get-ErrorWindow ([System.IO.File]::ReadAllText($gbkLog))
  if ($win) { $cases.Add([pscustomobject]@{ name = 'non-utf8-source'; src = 'probe:non-utf8-source(gbk+pdflatex)'; log = $win }); Write-Host "  [OK]   non-utf8-source" }
  else { Write-Host "  [跳过] non-utf8-source（无 ! 行）" }
}

# ---------------------------------------------------------------- 输出 Rust 数据文件
Write-Host ("`n=== 共 {0} 例，写入 {1} ===" -f $cases.Count, $OutFile)
$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine('//! 真实错误日志语料（roadmap ④ 错误诊断升级）——**自动生成，勿手改**。')
[void]$sb.AppendLine('//!')
[void]$sb.AppendLine('//! 生成脚本：`scripts/gen-log-error-corpus.ps1`；期望值在 `diagnosis_tests.rs` 手写，')
[void]$sb.AppendLine('//! 避免"实现自己生成期望值"的自证循环。')
[void]$sb.AppendLine('//!')
[void]$sb.AppendLine('//! 来源全部是**真实编译产物**：`survey:` 前缀来自 ⑲ 模板编译矩阵（19 个真实模板），')
[void]$sb.AppendLine('//! `probe:` 前缀来自故意写错的最小文档。每例只截取错误上下文窗口。')
[void]$sb.AppendLine('')
[void]$sb.AppendLine('/// 一条真实日志片段。')
[void]$sb.AppendLine('pub struct RealLogCase {')
[void]$sb.AppendLine('    /// 用例名（与 `diagnosis_tests.rs` 的期望表按名对应）。')
[void]$sb.AppendLine('    pub name: &''static str,')
[void]$sb.AppendLine('    /// 来源（便于复核：⑲ 矩阵目录名 / 探针名）。')
[void]$sb.AppendLine('    pub source: &''static str,')
[void]$sb.AppendLine('    /// 真实 `.log` 片段。')
[void]$sb.AppendLine('    pub log: &''static str,')
[void]$sb.AppendLine('}')
[void]$sb.AppendLine('')
[void]$sb.AppendLine('/// 真实错误日志语料。')
[void]$sb.AppendLine('pub static REAL_LOGS: &[RealLogCase] = &[')
foreach ($c in $cases) {
  [void]$sb.AppendLine('    RealLogCase {')
  [void]$sb.AppendLine(("        name: `"{0}`"," -f $c.name))
  [void]$sb.AppendLine(("        source: `"{0}`"," -f ($c.src -replace '\\','/')))
  [void]$sb.AppendLine('        log: r##"')
  [void]$sb.AppendLine($c.log)
  [void]$sb.AppendLine('"##,')
  [void]$sb.AppendLine('    },')
}
[void]$sb.AppendLine('];')
[System.IO.File]::WriteAllText($OutFile, $sb.ToString(), (New-Object System.Text.UTF8Encoding($false)))
Write-Host "完成。"
