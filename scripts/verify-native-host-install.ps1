[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateScript({ Test-Path $_ -PathType Leaf })]
  [string]$HostPath,

  [Parameter(Mandatory = $true)]
  [ValidateScript({ Test-Path $_ -PathType Leaf })]
  [string]$ReleaseConfigPath,

  [string]$ManifestRoot = (Join-Path $env:LOCALAPPDATA 'VeriSilo\NativeMessaging')
)

$ErrorActionPreference = 'Stop'
$hostName = 'io.verisilo.host'
$resolvedHostPath = (Resolve-Path -LiteralPath $HostPath).Path
$releaseConfig = Get-Content -Raw -LiteralPath $ReleaseConfigPath | ConvertFrom-Json
$checks = @(
  [pscustomobject]@{
    Name = 'chrome'
    ExtensionId = $releaseConfig.chromeExtensionId
    RegistryPath = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
  },
  [pscustomobject]@{
    Name = 'edge'
    ExtensionId = $releaseConfig.edgeExtensionId
    RegistryPath = "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
  }
)

foreach ($check in $checks) {
  if ($check.ExtensionId -notmatch '^[a-p]{32}$') {
    throw "Invalid release extension ID for $($check.Name)."
  }
  $manifestPath = Join-Path $ManifestRoot "native-host-$($check.Name).json"
  $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
  $expectedManifestProperties = @('name', 'description', 'path', 'type', 'allowed_origins')
  $actualManifestProperties = @($manifest.PSObject.Properties.Name | Sort-Object)
  if ((Compare-Object ($expectedManifestProperties | Sort-Object) $actualManifestProperties).Count -ne 0) {
    throw "$($check.Name) manifest has unknown or missing fields."
  }
  if ($manifest.name -ne $hostName -or $manifest.type -ne 'stdio') {
    throw "$($check.Name) manifest identifies an unexpected Native Host."
  }
  if (-not [string]::Equals($manifest.path, $resolvedHostPath, [StringComparison]::OrdinalIgnoreCase)) {
    throw "$($check.Name) manifest points to a different executable."
  }
  $origins = @($manifest.allowed_origins)
  if ($origins.Count -ne 1 -or $origins[0] -ne "chrome-extension://$($check.ExtensionId)/") {
    throw "$($check.Name) manifest origin allowlist does not match the release configuration."
  }
  if (-not (Test-Path -LiteralPath $check.RegistryPath)) {
    throw "$($check.Name) current-user registry entry is missing."
  }
  $registeredPath = (Get-Item -LiteralPath $check.RegistryPath).GetValue('')
  if (-not [string]::Equals($registeredPath, $manifestPath, [StringComparison]::OrdinalIgnoreCase)) {
    throw "$($check.Name) registry entry points to a different manifest."
  }
}

Write-Host 'Native Host manifests, production origins, executable paths, and HKCU registrations are consistent.'
