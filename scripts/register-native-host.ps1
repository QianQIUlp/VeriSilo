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
$manifestRoot = Join-Path $root 'NativeMessaging\Development'
New-Item -ItemType Directory -Force -Path $root | Out-Null
New-Item -ItemType Directory -Force -Path $manifestRoot | Out-Null

$hostName = 'io.verisilo.host'
$resolvedHostPath = (Resolve-Path -LiteralPath $HostPath).Path
if ((Split-Path -Leaf $resolvedHostPath) -ine 'verisilo-native-host.exe') {
  throw 'HostPath must point to verisilo-native-host.exe.'
}
$manifestPath = Join-Path $manifestRoot "native-host-development-$Browser.json"
$manifest = [ordered]@{
  name = $hostName
  description = 'VeriSilo Native Messaging Host'
  path = $resolvedHostPath
  type = 'stdio'
  allowed_origins = @("chrome-extension://$ExtensionId/")
}
$manifestJson = $manifest | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText(
  $manifestPath,
  $manifestJson,
  [System.Text.UTF8Encoding]::new($false)
)

$allowlistPath = Join-Path $root 'native-host-development-allowlist.json'
$allowlist = if (Test-Path $allowlistPath) {
  Get-Content -Raw -Path $allowlistPath | ConvertFrom-Json
} else {
  [pscustomobject]@{ allowedExtensionIds = @() }
}
if ($allowlist.allowedExtensionIds -notcontains $ExtensionId) {
  $allowlist.allowedExtensionIds = @($allowlist.allowedExtensionIds) + $ExtensionId
}
$allowlistJson = $allowlist | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText(
  $allowlistPath,
  $allowlistJson,
  [System.Text.UTF8Encoding]::new($false)
)

$registryPath = if ($Browser -eq 'chrome') {
  "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
} else {
  "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
}
New-Item -Path $registryPath -Force | Out-Null
Set-Item -Path $registryPath -Value $manifestPath

Write-Host "Registered $hostName for $Browser at the current-user scope."
Write-Host 'Development IDs are read only by a debug Native Host build.'
Write-Host 'This script does not install, force-install, or enable an extension.'
