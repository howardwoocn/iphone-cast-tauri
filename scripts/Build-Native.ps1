param([string]$MsysRoot, [switch]$SkipCollect)
$ErrorActionPreference = 'Stop'
$ProjectRoot = Split-Path -Parent $PSScriptRoot
if (-not $MsysRoot) { $MsysRoot = Join-Path $ProjectRoot '.tools\msys64' }
$MsysRoot = (Resolve-Path -LiteralPath $MsysRoot).Path
$Bash = Join-Path $MsysRoot 'usr\bin\bash.exe'
if (-not (Test-Path -LiteralPath $Bash)) { throw 'Missing independent MSYS2 installation. See docs/BUILD.md.' }
$env:MSYSTEM = 'UCRT64'
$env:CHERE_INVOKING = '1'
Push-Location $ProjectRoot
try {
    & $Bash -lc 'cmake -S native -B .cache/native -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_POLICY_VERSION_MINIMUM=3.5 && cmake --build .cache/native --target cast-receiver geometry-tests --parallel 4 && .cache/native/geometry-tests.exe'
    if ($LASTEXITCODE -ne 0) { throw "Native build failed ($LASTEXITCODE)" }
    if (-not $SkipCollect) {
        & (Join-Path $PSScriptRoot 'Collect-Runtime.ps1') -MsysRoot $MsysRoot
    }
} finally { Pop-Location }
