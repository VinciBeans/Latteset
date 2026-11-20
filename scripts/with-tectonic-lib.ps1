# 内嵌 Tectonic（唯一形态）构建/运行的**唯一入口**：脚本、npm scripts 与 CI 共用这一份。
#
# 为什么需要它：内嵌引擎要拉 21 个 native 标记的 C 链（freetype2 / harfbuzz / graphite2 / ICU /
# fontconfig / libpng），构建前置是 5 个环境变量 + 一套 vcpkg triplet。自阶段②起（ADR-0015 决策 2）
# **所有构建都需要 vcpkg** —— 缺任何一项时，失败信息来自 vcpkg-rs 或 build script 的 panic，
# **不指向环境**（实测过：`pkg-config` 缺失会以 "you need to install a pkg-config package for your
# OS" 的形式炸在 tectonic_bridge_png 上）。所以这里先**校验**、缺项就给可执行修复命令，再跑真正的命令。
#
# 用法：
#   pwsh -NoProfile -File scripts/with-tectonic-lib.ps1 -Mode check        # 只校验前置
#   pwsh -NoProfile -File scripts/with-tectonic-lib.ps1 -Mode tauri dev    # 透传 `npx tauri dev`
#   pwsh -NoProfile -File scripts/with-tectonic-lib.ps1 -Mode tauri build  # 透传 `npx tauri build`
#   pwsh -NoProfile -File scripts/with-tectonic-lib.ps1 -Mode test         # 全仓 Rust 测试
#   pwsh -NoProfile -File scripts/with-tectonic-lib.ps1 -Mode cli          # release CLI 构建
# npm 侧对应 `npm run lib:check` / `npm run tauri dev` / `npm run tauri build` /
# `npm run lib:test` / `npm run lib:cli`。
#
# 被 dot-source 时（`. scripts/with-tectonic-lib.ps1`）只设置并校验环境、不跑命令，
# 供需要自己拼命令的场合（CI、手工 cargo）复用同一份前置口径 —— 别在别处再抄一遍。
#
# vcpkg 检出位置：默认 `$env:VCPKG_ROOT`，再退回 <repo>/test_file/vcpkg（pin rev 见 .gitignore）；
# 也可用 -VcpkgRoot <dir> 显式指定（其它机器/CI 上指向已有检出）。

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet('check', 'tauri', 'test', 'cli')]
    [string]$Mode = 'check',

    # vcpkg 检出位置；默认 $env:VCPKG_ROOT，再退回 <repo>/test_file/vcpkg
    [string]$VcpkgRoot = $env:VCPKG_ROOT,

    # 产品缓存目录（跑起来才用得上，属运行期；构建阶段用不到）
    [string]$CacheDir = $env:LATTESET_TECTONIC_CACHE,

    # 透传给 `npx tauri <args>`（仅 -Mode tauri 用）：`-Mode tauri dev` ⇒ `npx tauri dev`
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs = @()
)

$ErrorActionPreference = 'Stop'

# dot-source 时不执行命令（调用方只要环境）。
$dotSourced = $MyInvocation.InvocationName -eq '.'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $VcpkgRoot) { $VcpkgRoot = Join-Path $repo 'test_file/vcpkg' }

$triplet = 'x64-windows-static-release'
# vcpkg pin rev：与 `test_file/tectonic-src/Cargo.toml` 的 [package.metadata.vcpkg] rev 一致
# （`.gitignore` 的复现命令同步）。本机/CI 钉同一个 rev，不写"最新版"。
$vcpkgPinRev = 'a62ce77d56ee07513b4b67de1ec2daeaebfae51a'
$ports = 'freetype harfbuzz graphite2 icu fontconfig libpng'
$problems = [System.Collections.Generic.List[string]]::new()
$warnings = [System.Collections.Generic.List[string]]::new()

# ① vcpkg 检出本身
if (-not (Test-Path $VcpkgRoot)) {
    $problems.Add(@"
找不到 vcpkg 检出：$VcpkgRoot
    修（复制即可跑；也可用 -VcpkgRoot 或 `$env:VCPKG_ROOT 指向已有检出）：
      git clone https://github.com/microsoft/vcpkg "$VcpkgRoot"
      git -C "$VcpkgRoot" checkout $vcpkgPinRev
      & "$VcpkgRoot\bootstrap-vcpkg.bat" -disableMetrics
"@)
} elseif (-not (Test-Path (Join-Path $VcpkgRoot 'vcpkg.exe'))) {
    $problems.Add(@"
vcpkg 未 bootstrap（缺 vcpkg.exe）：$VcpkgRoot
    修（复制即可跑）：
      & "$VcpkgRoot\bootstrap-vcpkg.bat" -disableMetrics
"@)
}

# ② 该 triplet 的依赖是否**已装**（vcpkg 把 ports 落到 <root>/installed/<triplet>）。
#    这一步是硬要求：cargo 只是链接这些预编译静态库，缺了它连链接参数都给不出来。
$installed = Join-Path $VcpkgRoot "installed/$triplet"
$portsPresent = (Test-Path (Join-Path $installed 'lib')) -and
                ((Get-ChildItem (Join-Path $installed 'lib') -Filter '*.lib' -ErrorAction SilentlyContinue | Measure-Object).Count -gt 0)
if ((Test-Path $VcpkgRoot) -and -not $portsPresent) {
    $problems.Add(@"
triplet 的依赖未安装（$installed 下没有 .lib）：自阶段②起所有构建都需要这六个端口。
    修（复制即可跑；传递依赖由 vcpkg 自己拉：brotli / bzip2 / dirent / expat / zlib）：
      `$env:VCPKG_ROOT = "$VcpkgRoot"
      & "$VcpkgRoot\vcpkg.exe" install $ports --triplet $triplet
"@)
}

# ③ 构建工具链：**只有要（重）建 ports 时才需要**。ports 已装好时 cargo 直接链接，
#    实测这些工具不在 PATH 上照样构建成功 —— 所以这里是**警告**，不是硬失败
#    （把它们判成硬失败会让一个本来能跑的构建被拦住）。
$toolFixes = [ordered]@{
    'cmake'      = 'winget install -e --id Kitware.CMake'
    'nasm'       = 'winget install -e --id NASM.NASM'
    'pkg-config' = 'choco install pkgconfiglite'
}
if (-not $portsPresent) {
    foreach ($tool in $toolFixes.Keys) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            $problems.Add("PATH 上找不到 $tool —— 要现场构建 ports 就必须有它`n    修：$($toolFixes[$tool])")
        }
    }
} else {
    foreach ($tool in $toolFixes.Keys) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            $warnings.Add("PATH 上没有 $tool —— 当前 ports 已装好，构建不受影响；只有要重建 ports 时才需要（$($toolFixes[$tool])）")
        }
    }
}

if ($problems.Count -gt 0) {
    Write-Host "✗ 内嵌 Tectonic 的构建前置不齐（$($problems.Count) 项）：" -ForegroundColor Red
    $problems | ForEach-Object { Write-Host "  · $_" -ForegroundColor Red }
    throw "构建前置不齐；按上面的『修』处理后重试（自阶段②起所有构建都需要 vcpkg：ADR-0015 决策 2）。"
}
$warnings | ForEach-Object { Write-Host "  ⚠ $_" -ForegroundColor Yellow }

# ---- 设置环境（全仓唯一落点：别在别处再抄一遍）----
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

Write-Host "内嵌 Tectonic 构建前置就绪：" -ForegroundColor Green
Write-Host "  VCPKG_ROOT          = $env:VCPKG_ROOT"
Write-Host "  triplet             = $triplet"
Write-Host "  RUSTFLAGS           = $env:RUSTFLAGS"
if ($env:LATTESET_TECTONIC_CACHE) { Write-Host "  LATTESET_TECTONIC_CACHE = $env:LATTESET_TECTONIC_CACHE" }

if ($dotSourced -or $Mode -eq 'check') { return }

# ---- 命令面（唯一入口）：tauri 透传 / 全仓 Rust 测试 / CLI release ----
switch ($Mode) {
    'tauri' {
        # `npm run tauri dev` ⇒ RemainingArgs=@('dev')；`npm run tauri build` ⇒ @('build')。
        $program = 'npx'
        $argv = @('tauri') + $RemainingArgs
    }
    'test' {
        # 全量 Rust 测试：内嵌 crate 从"可选档"变成必跑档，四个 crate 一起跑（不再只测 tectonic）。
        $program = 'cargo'
        $argv = @('test', '-p', 'latteset-core', '-p', 'latteset-infra', '-p', 'latteset-server', '-p', 'latteset-tectonic')
    }
    'cli' {
        $program = 'cargo'
        $argv = @('build', '--release', '-p', 'latteset-server', '--bin', 'latteset-cli')
    }
    default { throw "未知 -Mode：$Mode" }
}
Write-Host "→ $program $($argv -join ' ')" -ForegroundColor Cyan
& $program @argv
exit $LASTEXITCODE
