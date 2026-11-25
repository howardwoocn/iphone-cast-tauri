param([switch]$NativeOnly)
$ErrorActionPreference='Stop'
$ProjectRoot=Split-Path -Parent $PSScriptRoot
Push-Location $ProjectRoot
try {
    if(-not $NativeOnly){
        $env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
        & npm.cmd run build
        if($LASTEXITCODE -ne 0){throw 'Frontend build failed'}
        $SavedTauriConfig=$env:TAURI_CONFIG
        try {
            $env:TAURI_CONFIG='{"bundle":{"resources":[]}}'
            & cargo test --manifest-path src-tauri\Cargo.toml
            if($LASTEXITCODE -ne 0){throw 'Rust tests failed'}
        } finally {$env:TAURI_CONFIG=$SavedTauriConfig}
    }
    & .\.cache\native\geometry-tests.exe
    if($LASTEXITCODE -ne 0){throw 'Native geometry tests failed'}
} finally {Pop-Location}
