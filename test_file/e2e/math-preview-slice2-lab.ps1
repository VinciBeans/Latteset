# ㊸ 切片 2 的定量前置（2026-09-17）：命令面与隔离设计要用的三块数据。
#
# M1 **每公式目录的档位**：设计是"一个公式一个 snippet 目录"（才吃得到 A 闸门与热 aux）。
#    要确认：新公式 = 冷（清空重建）、同一公式第二次 = A 闸门（convert 归零）、三档导言区各自多少。
# M2 **导言区指纹的成本**：缓存键要含"导言区哈希"（256 KB–1 MB 的根文件）⇒ 每次悬停算一次贵不贵。
# M3 **零污染**：片段编译写进 `<项目>/tmp/snippet/<键>/` 之后，权威 `<stem>.pdf` 与 `tmp/main.*`
#    必须**逐字节不变**（切片 2 的验收判据之一）。
#
# 用法（工作区产物，未提交）：.\test_file\e2e\math-preview-slice2-lab.ps1
param(
    [string]$Cli = "src-tauri\target\release\latteset-cli.exe",
    [string]$Bundle = "file:///E:/Works/tex-presso/test_file/tectonic-bundle",
    [string]$Cache = "",
    [string]$Out = "test_file\e2e\math-preview-slice2-lab.json"
)
$ErrorActionPreference = 'Stop'
if (-not $Cache) { $Cache = Join-Path (Get-Location) 'test_file\e2e\fmt-cache-lab' }
New-Item -ItemType Directory -Force -Path $Cache | Out-Null
$env:LATTESET_TECTONIC_BUNDLE = $Bundle
$env:LATTESET_TECTONIC_CACHE = $Cache
$env:LATTESET_TECTONIC_LIB = '1'
$repo = (Get-Location).Path
$projRoot = (Resolve-Path 'test_file\projects\bench\large').Path

function Get-Preamble([string]$RootFile) {
    $src = [IO.File]::ReadAllText($RootFile)
    $at = $src.IndexOf('\begin{document}')
    return $src.Substring(0, $at)
}
# 与 core::snippet::build_snippet_document 同口径 + 切片 2 的两处新决定：注入 `\nofiles`、按公式分目录
function New-Doc([string]$Pre, [string]$Body) {
    $d = $projRoot.Replace('\','/'); if (-not $d.EndsWith('/')) { $d += '/' }
    "\makeatletter`n\def\input@path{{$d}}`n\makeatother`n" + $Pre +
    "\nofiles`n\begin{document}`n\makeatletter`n\@ifundefined{graphicspath}{}{\graphicspath{{$d}}}`n\makeatother`n\pagestyle{empty}`n" +
    $Body + "`n\end{document}`n"
}
function Compile([string]$Dir) {
    if (-not (Test-Path $Dir)) { New-Item -ItemType Directory -Force -Path $Dir | Out-Null }
    $t = [Diagnostics.Stopwatch]::StartNew()
    $raw = & $Cli --project $Dir compile --quick 2>&1 | Out-String
    $t.Stop()
    $i = $raw.IndexOf('{'); $j = $raw.LastIndexOf('}')
    $o = if ($i -ge 0) { ($raw.Substring($i,$j-$i+1) | ConvertFrom-Json).compile } else { $null }
    $kv = ([regex]::Match($raw,'分阶段耗时(?<kv>.*)')).Groups['kv'].Value
    return [ordered]@{
        status  = if ($o) { $o.status } else { '(无JSON)' }
        wall    = [int]$t.Elapsed.TotalMilliseconds
        typeset = ([regex]::Match($kv,'typeset_ms=(\d+)')).Groups[1].Value
        convert = ([regex]::Match($kv,'convert_ms=(\d+)')).Groups[1].Value
        pages   = ([regex]::Match($kv,'pages=(\d+)')).Groups[1].Value
        reused  = ([regex]::Match($kv,'reused_pdf=(\w+)')).Groups[1].Value
        passes  = ([regex]::Match($raw,'排版趟数 passes=(\d+)')).Groups[1].Value
    }
}

$report = [ordered]@{}
# ---------- M1：三档导言区 × {新公式冷、同公式二跑} ----------
$preambles = [ordered]@{
    'P0 article（24 B）'   = (Get-Preamble (Join-Path $repo 'test_file\projects\bench\large\main.tex'))
    'P2 ctexbook+tikz'     = (Get-Preamble (Join-Path $repo 'test_file\projects\bench\multifile\main.tex'))
}
$fA = '$\int_0^1 x^2\,\mathrm{d}x$'
$fB = '$a^2+b^2=c^2$'
$m1 = [ordered]@{}
foreach ($pk in $preambles.Keys) {
    $labBase = Join-Path $projRoot ("tmp\slice2-lab\" + ($pk.Split(' ')[0]))
    $dA = Join-Path $labBase 'fA'
    $dB = Join-Path $labBase 'fB'
    foreach ($d in @($dA, $dB)) { if (Test-Path $d) { Remove-Item $d -Recurse -Force } ; New-Item -ItemType Directory -Force -Path $d | Out-Null }
    [IO.File]::WriteAllText((Join-Path $dA 'main.tex'), (New-Doc $preambles[$pk] $fA), (New-Object Text.UTF8Encoding($false)))
    [IO.File]::WriteAllText((Join-Path $dB 'main.tex'), (New-Doc $preambles[$pk] $fB), (New-Object Text.UTF8Encoding($false)))
    $r = [ordered]@{}
    $r['公式A 首次（冷）']    = Compile $dA
    $r['公式A 二次（A 闸门）'] = Compile $dA
    $r['公式B 首次（另一个公式 ⇒ 另一个目录）'] = Compile $dB
    Write-Host ("[{0}] A 首 {1} / A 二 {2}（reused={3}） / B 首 {4} ms" -f $pk.Split(' ')[0],
        $r['公式A 首次（冷）'].wall, $r['公式A 二次（A 闸门）'].wall, $r['公式A 二次（A 闸门）'].reused, $r['公式B 首次（另一个公式 ⇒ 另一个目录）'].wall)
    $m1[$pk] = $r
}
$report['M1_per_formula_dir'] = $m1

# ---------- M2：导言区指纹成本（读根文件 + 抽导言区 + SHA256，分别计时） ----------
$m2 = [ordered]@{}
foreach ($f in @('test_file\projects\bench\large\main.tex','test_file\projects\bench\multifile\main.tex')) {
    $abs = Join-Path $repo $f
    $bytes = [IO.File]::ReadAllBytes($abs)
    $sw = [Diagnostics.Stopwatch]::StartNew()
    1..20 | ForEach-Object { $null = [IO.File]::ReadAllBytes($abs) } | Out-Null
    $sw.Stop(); $readMs = $sw.Elapsed.TotalMilliseconds / 20
    $sw2 = [Diagnostics.Stopwatch]::StartNew()
    1..20 | ForEach-Object { $null = (Get-Preamble $abs) } | Out-Null
    $sw2.Stop(); $preMs = $sw2.Elapsed.TotalMilliseconds / 20
    $sha = [Security.Cryptography.SHA256]::Create()
    $sw3 = [Diagnostics.Stopwatch]::StartNew()
    1..20 | ForEach-Object { $null = $sha.ComputeHash($bytes) } | Out-Null
    $sw3.Stop(); $hashMs = $sw3.Elapsed.TotalMilliseconds / 20
    $sha.Dispose()
    $m2[$f] = [ordered]@{ bytes = $bytes.Length; read_ms = [math]::Round($readMs,2); extract_ms = [math]::Round($preMs,2); sha256_ms = [math]::Round($hashMs,2) }
    Write-Host ("[M2] {0}：{1} B ⇒ 读 {2} ms / 抽导言区 {3} ms / SHA256 {4} ms（各 20 次均值）" -f $f, $bytes.Length, $m2[$f].read_ms, $m2[$f].extract_ms, $m2[$f].sha256_ms)
}
$report['M2_preamble_fingerprint'] = $m2

# ---------- M3：零污染（片段编译前后，权威产物逐字节不变） ----------
$m3 = [ordered]@{}
$auth = @('main.pdf','tmp\main.log','tmp\main.synctex.gz','tmp\main.tectonic.pages')
$before = [ordered]@{}
foreach ($a in $auth) { $p = Join-Path $projRoot $a; $before[$a] = if (Test-Path $p) { (Get-FileHash $p -Algorithm SHA256).Hash } else { $null } }
# 在 `<项目>/tmp/snippet/<公式键>/` 里编一次（切片 2 的落点）
$snipDir = Join-Path $projRoot 'tmp\snippet\int_0^1_x2'
if (Test-Path $snipDir) { Remove-Item $snipDir -Recurse -Force }
New-Item -ItemType Directory -Force -Path $snipDir | Out-Null
[IO.File]::WriteAllText((Join-Path $snipDir 'main.tex'), (New-Doc (Get-Preamble (Join-Path $projRoot 'main.tex')) $fA), (New-Object Text.UTF8Encoding($false)))
$snip = Compile $snipDir
$after = [ordered]@{}
foreach ($a in $auth) { $p = Join-Path $projRoot $a; $after[$a] = if (Test-Path $p) { (Get-FileHash $p -Algorithm SHA256).Hash } else { $null } }
$unchanged = $true
foreach ($a in $auth) { if ($before[$a] -ne $after[$a]) { $unchanged = $false; Write-Host ("  ✗ 权威产物被改了：{0}" -f $a) -ForegroundColor Red } }
Write-Host ("[M3] 片段编译（{0} ms，{1}）后权威产物逐字节不变？ {2}" -f $snip.wall, $snip.status, $unchanged) -ForegroundColor $(if ($unchanged) { 'Green' } else { 'Red' })
$m3['snip'] = $snip; $m3['authoritative_unchanged'] = $unchanged
$m3['before'] = $before; $m3['after'] = $after
$report['M3_zero_pollution'] = $m3

$report['when'] = (Get-Date -Format s)
$report['note'] = 'Tectonic 库形态；--quick；片段文档注入 \nofiles；每公式一个目录；LATTESET_TECTONIC_CACHE 指向工作区实验缓存'
$report | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 $Out
Write-Host "`n证据已落：$Out"