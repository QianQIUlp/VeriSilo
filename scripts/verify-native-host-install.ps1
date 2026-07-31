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

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$hostName = 'io.verisilo.host'

function ConvertFrom-JsonPreservingDateStrings {
  param([Parameter(Mandatory = $true)][string]$Json)
  $convertFromJson = Get-Command ConvertFrom-Json
  if ($convertFromJson.Parameters.ContainsKey('DateKind')) {
    # PowerShell 7.5+ otherwise converts ISO timestamps into DateTime values
    # using the local time zone before this script can validate the wire text.
    return $Json | ConvertFrom-Json -DateKind String
  }
  return $Json | ConvertFrom-Json
}

$resolvedHostPath = (Resolve-Path -LiteralPath $HostPath).Path
$resolvedDesktopPath = Join-Path (Split-Path -Parent $resolvedHostPath) 'verisilo.exe'
$resolvedManifestRoot = [IO.Path]::GetFullPath($ManifestRoot)
if ((Split-Path -Leaf $resolvedHostPath) -ine 'verisilo-native-host.exe') {
  throw 'HostPath must point to verisilo-native-host.exe.'
}
if (-not (Test-Path -LiteralPath $resolvedDesktopPath -PathType Leaf)) {
  throw 'verisilo.exe must be installed beside verisilo-native-host.exe.'
}
if (-not (Test-Path -LiteralPath $resolvedManifestRoot -PathType Container)) {
  throw 'Native Host manifest root is missing.'
}
$manifestRootItem = Get-Item -LiteralPath $resolvedManifestRoot -Force
if (($manifestRootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
  throw 'Native Host manifest root must not be a reparse point.'
}
$releaseConfig = Get-Content -Raw -LiteralPath $ReleaseConfigPath | ConvertFrom-Json
$expectedConfigProperties = @('schemaVersion', 'chromeExtensionId', 'edgeExtensionId')
$actualConfigProperties = @($releaseConfig.PSObject.Properties.Name | Sort-Object)
if (@(Compare-Object ($expectedConfigProperties | Sort-Object) $actualConfigProperties).Count -ne 0) {
  throw 'Native Host release configuration has unknown or missing fields.'
}
if ($releaseConfig.schemaVersion -ne 1) {
  throw 'Unsupported Native Host release configuration version.'
}
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
  $manifestPath = Join-Path $resolvedManifestRoot "native-host-$($check.Name).json"
  $manifestItem = Get-Item -LiteralPath $manifestPath -Force
  if (($manifestItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "$($check.Name) manifest must not be a reparse point."
  }
  $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
  $expectedManifestProperties = @('name', 'description', 'path', 'type', 'allowed_origins')
  $actualManifestProperties = @($manifest.PSObject.Properties.Name | Sort-Object)
  if (@(Compare-Object ($expectedManifestProperties | Sort-Object) $actualManifestProperties).Count -ne 0) {
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

$installRecordPath = Join-Path $resolvedManifestRoot 'install-record.json'
$installRecordItem = Get-Item -LiteralPath $installRecordPath -Force
if (($installRecordItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
  throw 'Native Host install record must not be a reparse point.'
}
$installRecordJson = Get-Content -Raw -LiteralPath $installRecordPath
$installRecord = ConvertFrom-JsonPreservingDateStrings -Json $installRecordJson
$expectedRecordProperties = @(
  'schemaVersion', 'hostPath', 'chromeExtensionId', 'edgeExtensionId', 'installedAt'
)
$actualRecordProperties = @($installRecord.PSObject.Properties.Name | Sort-Object)
if (@(Compare-Object ($expectedRecordProperties | Sort-Object) $actualRecordProperties).Count -ne 0 -or
    $installRecord.schemaVersion -ne 1 -or
    -not [string]::Equals(
      [string]$installRecord.hostPath,
      $resolvedHostPath,
      [StringComparison]::OrdinalIgnoreCase
    ) -or
    [string]$installRecord.chromeExtensionId -cne [string]$releaseConfig.chromeExtensionId -or
    [string]$installRecord.edgeExtensionId -cne [string]$releaseConfig.edgeExtensionId) {
  throw 'Native Host install record does not match the release configuration and executable.'
}
$installedAt = [DateTimeOffset]::MinValue
if (-not [DateTimeOffset]::TryParseExact(
  [string]$installRecord.installedAt,
  'o',
  [Globalization.CultureInfo]::InvariantCulture,
  [Globalization.DateTimeStyles]::RoundtripKind,
  [ref]$installedAt
)) {
  throw 'Native Host install record has an invalid installedAt timestamp.'
}

Write-Host 'Native Host manifests, install record, production origins, executable paths, and HKCU registrations are consistent.'
