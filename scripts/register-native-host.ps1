[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateSet('chrome', 'edge')]
  [string]$Browser,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[a-p]{32}$')]
  [string]$ExtensionId,

  [Parameter(Mandatory = $true)]
  [ValidateScript({ Test-Path $_ -PathType Leaf })]
  [string]$HostPath
)

$ErrorActionPreference = 'Stop'
$root = Join-Path $env:LOCALAPPDATA 'VeriSilo'
New-Item -ItemType Directory -Force -Path $root | Out-Null

$hostName = 'io.verisilo.host'
$manifestPath = Join-Path $root "native-host-$Browser.json"
$manifest = [ordered]@{
  name = $hostName
  description = 'VeriSilo Native Messaging Host'
  path = (Resolve-Path $HostPath).Path
  type = 'stdio'
  allowed_origins = @("chrome-extension://$ExtensionId/")
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -Path $manifestPath -Encoding utf8

$allowlistPath = Join-Path $root 'native-host-allowlist.json'
$allowlist = if (Test-Path $allowlistPath) {
  Get-Content -Raw -Path $allowlistPath | ConvertFrom-Json
} else {
  [pscustomobject]@{ allowedExtensionIds = @() }
}
if ($allowlist.allowedExtensionIds -notcontains $ExtensionId) {
  $allowlist.allowedExtensionIds = @($allowlist.allowedExtensionIds) + $ExtensionId
}
$allowlist | ConvertTo-Json -Depth 4 | Set-Content -Path $allowlistPath -Encoding utf8

$registryPath = if ($Browser -eq 'chrome') {
  "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
} else {
  "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
}
New-Item -Path $registryPath -Force | Out-Null
Set-Item -Path $registryPath -Value $manifestPath

Write-Host "Registered $hostName for $Browser at the current-user scope."
Write-Host 'This script does not install, force-install, or enable an extension.'
