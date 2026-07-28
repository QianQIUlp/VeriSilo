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
$resolvedDesktopPath = Join-Path (Split-Path -Parent $resolvedHostPath) 'verisilo.exe'
if ((Split-Path -Leaf $resolvedHostPath) -ine 'verisilo-native-host.exe') {
  throw 'HostPath must point to verisilo-native-host.exe.'
}
if (-not (Test-Path -LiteralPath $resolvedDesktopPath -PathType Leaf)) {
  throw 'verisilo.exe must be installed beside verisilo-native-host.exe.'
}

$releaseConfig = Get-Content -Raw -LiteralPath $ReleaseConfigPath | ConvertFrom-Json
$expectedConfigProperties = @('schemaVersion', 'chromeExtensionId', 'edgeExtensionId')
$actualConfigProperties = @($releaseConfig.PSObject.Properties.Name | Sort-Object)
if ((Compare-Object ($expectedConfigProperties | Sort-Object) $actualConfigProperties).Count -ne 0) {
  throw 'Native Host release configuration has unknown or missing fields.'
}
if ($releaseConfig.schemaVersion -ne 1) {
  throw 'Unsupported Native Host release configuration version.'
}
foreach ($property in @('chromeExtensionId', 'edgeExtensionId')) {
  if ($releaseConfig.$property -notmatch '^[a-p]{32}$') {
    throw "$property must contain a published 32-character extension ID."
  }
}

function Write-Utf8JsonAtomically {
  param(
    [Parameter(Mandatory = $true)] [string]$Path,
    [Parameter(Mandatory = $true)] [object]$Value
  )
  $temporaryPath = "$Path.$([Guid]::NewGuid().ToString('N')).tmp"
  $json = $Value | ConvertTo-Json -Depth 8
  [System.IO.File]::WriteAllText(
    $temporaryPath,
    $json,
    [System.Text.UTF8Encoding]::new($false)
  )
  Move-Item -LiteralPath $temporaryPath -Destination $Path -Force
}

New-Item -ItemType Directory -Force -Path $ManifestRoot | Out-Null
$browserSettings = @(
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

foreach ($browser in $browserSettings) {
  $manifestPath = Join-Path $ManifestRoot "native-host-$($browser.Name).json"
  $manifest = [ordered]@{
    name = $hostName
    description = 'VeriSilo Native Messaging Host'
    path = $resolvedHostPath
    type = 'stdio'
    allowed_origins = @("chrome-extension://$($browser.ExtensionId)/")
  }
  Write-Utf8JsonAtomically -Path $manifestPath -Value $manifest
  New-Item -Path $browser.RegistryPath -Force | Out-Null
  Set-Item -Path $browser.RegistryPath -Value $manifestPath
}

$installRecord = [ordered]@{
  schemaVersion = 1
  hostPath = $resolvedHostPath
  chromeExtensionId = $releaseConfig.chromeExtensionId
  edgeExtensionId = $releaseConfig.edgeExtensionId
  installedAt = [DateTimeOffset]::UtcNow.ToString('o')
}
Write-Utf8JsonAtomically -Path (Join-Path $ManifestRoot 'install-record.json') -Value $installRecord

Write-Host "Registered $hostName for Chrome and Edge at the current-user scope."
Write-Host 'No extension policy was written and no browser extension was installed.'
