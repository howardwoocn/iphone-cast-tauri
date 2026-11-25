$ErrorActionPreference='Stop'
$ProjectRoot=Split-Path -Parent $PSScriptRoot
Push-Location $ProjectRoot
try {
    $env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
    & (Join-Path $PSScriptRoot 'Build-Native.ps1')
    & npm.cmd run build
    if($LASTEXITCODE -ne 0){throw 'Frontend build failed'}
    & node scripts/collect-js-licenses.mjs
    if($LASTEXITCODE -ne 0){throw 'License collection failed'}
    & python scripts/collect-rust-licenses.py
    if($LASTEXITCODE -ne 0){throw 'Rust license collection failed'}
    $SavedTauriConfig=$env:TAURI_CONFIG
    try {
        # Unit tests do not start the receiver. Avoid copying DLLs over a running development receiver.
        $env:TAURI_CONFIG='{"bundle":{"resources":[]}}'
        & cargo test --manifest-path src-tauri\Cargo.toml
        if($LASTEXITCODE -ne 0){throw 'Rust tests failed'}
    } finally {$env:TAURI_CONFIG=$SavedTauriConfig}
    & npm.cmd run tauri -- build
    if($LASTEXITCODE -ne 0){throw 'Tauri package failed'}
    $Output=Join-Path $ProjectRoot 'artifacts'
    New-Item -ItemType Directory -Path $Output -Force | Out-Null
    Copy-Item -Path 'src-tauri\target\release\bundle\nsis\*.exe' -Destination $Output -Force
    & python scripts/package-source.py
    if($LASTEXITCODE -ne 0){throw 'Source archive failed'}
    Write-Host "Installer saved to $Output"
} finally {Pop-Location}
