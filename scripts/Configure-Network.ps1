# Run explicitly in an elevated PowerShell; never disables the firewall.
$ErrorActionPreference='Stop'
$ProjectRoot=Split-Path -Parent $PSScriptRoot
$Receiver=(Resolve-Path -LiteralPath (Join-Path $ProjectRoot 'src-tauri\resources\runtime\cast-receiver.exe')).Path
foreach($Protocol in @('TCP','UDP')){
    $Name="ChengJing-iPhoneCast-$Protocol"
    if(-not(Get-NetFirewallRule -Name $Name -ErrorAction SilentlyContinue)){
        New-NetFirewallRule -Name $Name -DisplayName "澄镜 iPhone Cast ($Protocol)" -Direction Inbound -Action Allow -Program $Receiver -Profile Private -Protocol $Protocol | Out-Null
    }
}
Write-Host 'Allowed the development receiver on private networks only. Installed builds use their own Windows firewall prompt.'
