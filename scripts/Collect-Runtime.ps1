param([Parameter(Mandatory=$true)][string]$MsysRoot)
$ErrorActionPreference = 'Stop'
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$Prefix = Join-Path (Resolve-Path -LiteralPath $MsysRoot).Path 'ucrt64'
$Destination = Join-Path $ProjectRoot 'src-tauri\resources\runtime'
New-Item -ItemType Directory -Path $Destination -Force | Out-Null
function Copy-Binary([string]$Source,[string]$Target) {
    $Original=Get-Item -LiteralPath $Source
    if(Test-Path -LiteralPath $Target -PathType Container){$Target=Join-Path $Target $Original.Name}
    if(Test-Path -LiteralPath $Target){
        $Existing=Get-Item -LiteralPath $Target
        if($Existing.Length -eq $Original.Length -and $Existing.LastWriteTimeUtc -eq $Original.LastWriteTimeUtc){return}
    }
    Copy-Item -LiteralPath $Source -Destination $Target -Force
}
Copy-Binary (Join-Path $ProjectRoot '.cache\native\cast-receiver.exe') $Destination
# Deliberately select the protocol/decoder/render/audio plugins and recursively
# collect their DLL imports, rather than shipping an entire MSYS installation.
$PluginNames = @('coreelements','app','playback','autodetect','typefindfunctions','videoconvertscale','videoparsersbad','videofilter','d3d11','libav','wasapi','wasapi2','audioconvert','audioresample','volume','level','audioparsers','isomp4','tcp','udp','rtp','rtpmanager','videotestsrc','audiotestsrc','imagefreeze','jpeg','pango','compositor')
$PluginsOut = Join-Path $Destination 'gstreamer-1.0'
New-Item -ItemType Directory -Path $PluginsOut -Force | Out-Null
foreach ($Name in $PluginNames) {
    $Source = Join-Path $Prefix "lib\gstreamer-1.0\libgst$Name.dll"
    if (-not (Test-Path -LiteralPath $Source)) { throw "Required plugin missing: $Source" }
    $Target = Join-Path $PluginsOut "libgst$Name.dll"
    Copy-Binary $Source $Target
}
$Scanner = Join-Path $Prefix 'lib\gstreamer-1.0\gst-plugin-scanner.exe'
if (-not (Test-Path -LiteralPath $Scanner)) { $Scanner = Join-Path $Prefix 'libexec\gstreamer-1.0\gst-plugin-scanner.exe' }
Copy-Binary $Scanner (Join-Path $Destination 'gst-plugin-scanner.exe')
& python (Join-Path $PSScriptRoot 'collect-dlls.py') --prefix $Prefix --destination $Destination --manifest (Join-Path $ProjectRoot 'dependencies\runtime-lock.json')
if ($LASTEXITCODE -ne 0) { throw 'Runtime DLL collection failed' }
# DNS-SD is dynamically loaded by upstream. Bonjour is a separately installed
# Apple system service; its signed DLL is not redistributed in this package.
$LicensesOut = Join-Path $ProjectRoot 'licenses\runtime'
New-Item -ItemType Directory -Path $LicensesOut -Force | Out-Null
Copy-Item -Path (Join-Path $Prefix 'share\licenses\*') -Destination $LicensesOut -Recurse -Force
$env:MSYSTEM='UCRT64'
& (Join-Path $MsysRoot 'usr\bin\bash.exe') -lc 'pacman -Q' | Set-Content -LiteralPath (Join-Path $ProjectRoot 'dependencies\msys2-packages.txt') -Encoding UTF8
Write-Host "Collected runtime into $Destination"

