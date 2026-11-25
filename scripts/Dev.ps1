$ErrorActionPreference='Stop'
$ProjectRoot=Split-Path -Parent $PSScriptRoot
$env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
Push-Location $ProjectRoot
try {
    if(-not(Test-Path -LiteralPath 'src-tauri\resources\runtime\cast-receiver.exe')){& (Join-Path $PSScriptRoot 'Build-Native.ps1')}
    & npm.cmd run tauri -- dev
    if($LASTEXITCODE -ne 0){throw 'Desktop development process failed'}
} finally {Pop-Location}
