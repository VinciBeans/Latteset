# 带 Tectonic 库形态（feature `tectonic-lib`）构建/运行的**唯一入口**。
#
# 为什么需要它：库形态要拉 21 个 native 标记的 C 链（freetype2 / harfbuzz / graphite2 / ICU /
# fontconfig / libpng），构建前置是 5 个环境变量 + 一套 vcpkg triplet。缺任何一项时，失败信息
# 来自 vcpkg-rs 或 build script 的 panic，**不指向环境**（实测过：`pkg-config` 缺失会以
# "you need to install a pkg-config package for your OS" 的形式炸在 tectonic_bridge_png 上）。
# 所以这里先**校验**、缺项就给可执行文案，再跑真正的命令。
#
# 用法（两种）：
#   pwsh -NoProfile -File scripts/with-tectonic-lib.ps1 -Mode check      # 只校验前置
#   pwsh -NoProfile -File scripts/with-tectonic-lib.ps1 -Mode dev|build|test|cli
# npm 侧对应 `npm run lib:check` / `lib:dev` / `lib:build` / `lib:test` / `lib:cli`。
#
# 被 dot-source 时（`. scripts/with-tectonic-lib.ps1`）只设置并校验环境、不跑命令，
# 供需要自己拼命令的场合（CI、手工 cargo）复用同一份前置口径 —— 别在别处再抄一遍。

[CmdletBinding()]
param(
    [ValidateSet('check', 'dev', 'build', 'test', 'cli')]
    [string]$Mode = 'check',

    # vcpkg 检出位置；默认 <repo>/test_file/vcpkg（pin rev 见 .gitignore 的说明）
    [string]$VcpkgRoot = $env:VCPKG_ROOT,

    # 产品缓存目录（跑起来才用得上，属运行期；构建阶段用不到）
    [string]$CacheDir = $env:LATTESET_TECTONIC_CACHE
)

$ErrorActionPreference = 'Stop'

# dot-source 时不执行命令（调用方只要环境）。
$dotSourced = $MyInvocation.InvocationName -eq '.'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $VcpkgRoot) { $VcpkgRoot = Join-Path $repo 'test_file/vcpkg' }

$triplet = 'x64-windows-static-release'
$problems = [System.Collections.Generic.List[string]]::new()
$warnings = [System.Collections.Generic.List[string]]::new()

# ① vcpkg 检出本身
if (-not (Test-Path $VcpkgRoot)) {
    $problems.Add("找不到 vcpkg 检出：$VcpkgRoot`n    修：git clone https://github.com/microsoft/vcpkg `"$VcpkgRoot`" ; & `"$VcpkgRoot\bootstrap-vcpkg.bat`" -disableMetrics")
} elseif (-not (Test-Path (Join-Path $VcpkgRoot 'vcpkg.exe'))) {
    $problems.Add("vcpkg 未 bootstrap（缺 vcpkg.exe）：$VcpkgRoot`n    修：& `"$VcpkgRoot\bootstrap-vcpkg.bat`" -disableMetrics")
}

# ② 该 triplet 的依赖是否**已装**（vcpkg 把 ports 落到 <root>/installed/<triplet>）。
#    这一步是硬要求：cargo 只是链接这些预编译静态库，缺了它连链接参数都给不出来。
$installed = Join-Path $VcpkgRoot "installed/$triplet"
$portsPresent = (Test-Path (Join-Path $installed 'lib')) -and
                ((Get-ChildItem (Join-Path $installed 'lib') -Filter '*.lib' -ErrorAction SilentlyContinue | Measure-Object).Count -gt 0)
if ((Test-Path $VcpkgRoot) -and -not $portsPresent) {
    $problems.Add("triplet 的依赖未安装（$installed 下没有 .lib）：`n    修：装 freetype / harfbuzz / graphite2 / icu / fontconfig / libpng 的 $triplet 端口；清单与命令见 docs/tectonic-library-plan.md §3.1.3")
}

# ③ 构建工具链：**只有要（重）建 ports 时才需要**。ports 已装好时 cargo 直接链接，
#    实测这些工具不在 PATH 上照样构建成功 —— 所以这里是**警告**，不是硬失败
#    （把它们判成硬失败会让一个本来能跑的构建被拦住）。
if (-not $portsPresent) {
    foreach ($tool in @('cmake', 'pkg-config', 'nasm')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            $problems.Add("PATH 上找不到 $tool —— 要现场构建 ports 就必须有它`n    修：装 vcpkg 的这套前置（cmake / nasm / pkg-config 或等价物）")
        }
    }
} else {
    foreach ($tool in @('cmake', 'pkg-config', 'nasm')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            $warnings.Add("PATH 上没有 $tool —— 当前 ports 已装好，构建不受影响；只有要重建 ports 时才需要")
        }
    }
}

if ($problems.Count -gt 0) {
    Write-Host "✗ 库形态的构建前置不齐（$($problems.Count) 项）：" -ForegroundColor Red
    $problems | ForEach-Object { Write-Host "  · $_" -ForegroundColor Red }
    throw "库形态构建前置不齐；按上面的『修』处理，或改用不带库形态的默认构建。"
}
$warnings | ForEach-Object { Write-Host "  ⚠ $_" -ForegroundColor Yellow }

# ---- 设置环境（与 docs/tectonic-library-plan.md §6.1 的命令行逐项一致）----
$env:TECTONIC_DEP_BACKEND = 'vcpkg'
$env:VCPKG_ROOT = (Resolve-Path $VcpkgRoot).Path
$env:VCPKG_DEFAULT_TRIPLET = $triplet
$env:VCPKG_DEFAULT_HOST_TRIPLET = $triplet
# ⚠ 非它不可：release profile 下 vcpkg-rs 会自己算成 `x64-windows-static`（未装）⇒ build script panic
$env:VCPKGRS_TRIPLET = $triplet
# triplet 的 VCPKG_CRT_LINKAGE=static ⇒ Rust 侧必须同用静态 CRT，否则链接冲突。
if ($env:RUSTFLAGS -notlike '*+crt-static*') {
    $env:RUSTFLAGS = ("$env:RUSTFLAGS -Ctarget-feature=+crt-static").Trim()
}
if ($CacheDir) { $env:LATTESET_TECTONIC_CACHE = $CacheDir }

Write-Host "库形态构建前置就绪：" -ForegroundColor Green
Write-Host "  VCPKG_ROOT          = $env:VCPKG_ROOT"
Write-Host "  triplet             = $triplet"
Write-Host "  RUSTFLAGS           = $env:RUSTFLAGS"
if ($env:LATTESET_TECTONIC_CACHE) { Write-Host "  LATTESET_TECTONIC_CACHE = $env:LATTESET_TECTONIC_CACHE" }

if ($dotSourced -or $Mode -eq 'check') { return }

# ---- 命令面：库形态的构建/运行/测试各一条 ----
$commands = @{
    dev   = @('npx', @('tauri', 'dev', '--features', 'tectonic-lib'))
    build = @('npx', @('tauri', 'build', '--features', 'tectonic-lib'))
    test  = @('cargo', @('test', '-p', 'latteset-tectonic'))
    cli   = @('cargo', @('build', '--release', '-p', 'latteset-server', '--features', 'tectonic-lib', '--bin', 'latteset-cli'))
}
$program, $argv = $commands[$Mode]
Write-Host "→ $program $($argv -join ' ')" -ForegroundColor Cyan
& $program @argv
exit $LASTEXITCODE
