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
$resolvedHostPath = (Resolve-Path -LiteralPath $HostPath).Path
$resolvedDesktopPath = Join-Path (Split-Path -Parent $resolvedHostPath) 'verisilo.exe'
$resolvedManifestRoot = [IO.Path]::GetFullPath($ManifestRoot)
if ((Split-Path -Leaf $resolvedHostPath) -ine 'verisilo-native-host.exe') {
  throw 'HostPath must point to verisilo-native-host.exe.'
}
if (-not (Test-Path -LiteralPath $resolvedDesktopPath -PathType Leaf)) {
  throw 'verisilo.exe must be installed beside verisilo-native-host.exe.'
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
  try {
    $json = $Value | ConvertTo-Json -Depth 8
    [System.IO.File]::WriteAllText(
      $temporaryPath,
      $json,
      [System.Text.UTF8Encoding]::new($false)
    )
    Move-Item -LiteralPath $temporaryPath -Destination $Path -Force
  } finally {
    Remove-Item -LiteralPath $temporaryPath -Force -ErrorAction SilentlyContinue
  }
}

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

if (Test-Path -LiteralPath $resolvedManifestRoot) {
  $manifestRootItem = Get-Item -LiteralPath $resolvedManifestRoot -Force
  if (-not $manifestRootItem.PSIsContainer -or
      ($manifestRootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'ManifestRoot must be a real directory, not a reparse point.'
  }
}

$manifestRootExisted = Test-Path -LiteralPath $resolvedManifestRoot -PathType Container
$installRecordPath = Join-Path $resolvedManifestRoot 'install-record.json'
$managedFilePaths = @(
  (Join-Path $resolvedManifestRoot 'native-host-chrome.json'),
  (Join-Path $resolvedManifestRoot 'native-host-edge.json'),
  $installRecordPath
)
$fileSnapshots = foreach ($path in $managedFilePaths) {
  if (Test-Path -LiteralPath $path) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
      throw "Native Host managed path is not a file: $path"
    }
    [pscustomobject]@{ Path = $path; Existed = $true; Bytes = [IO.File]::ReadAllBytes($path) }
  } else {
    [pscustomobject]@{ Path = $path; Existed = $false; Bytes = $null }
  }
}

$registrySnapshots = foreach ($browser in $browserSettings) {
  $manifestPath = Join-Path $resolvedManifestRoot "native-host-$($browser.Name).json"
  $existed = Test-Path -LiteralPath $browser.RegistryPath
  $value = if ($existed) { (Get-Item -LiteralPath $browser.RegistryPath).GetValue('') } else { $null }
  if ($existed -and
      -not [string]::Equals([string]$value, $manifestPath, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to overwrite a pre-existing $($browser.Name) Native Host registration owned by another manifest."
  }
  [pscustomobject]@{
    RegistryPath = $browser.RegistryPath
    ManifestPath = $manifestPath
    Existed = $existed
    Value = $value
  }
}

try {
  New-Item -ItemType Directory -Force -Path $resolvedManifestRoot | Out-Null
  foreach ($browser in $browserSettings) {
    $manifestPath = Join-Path $resolvedManifestRoot "native-host-$($browser.Name).json"
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
  Write-Utf8JsonAtomically -Path $installRecordPath -Value $installRecord

  $verifyScript = Join-Path $PSScriptRoot 'verify-native-host-install.ps1'
  & $verifyScript `
    -HostPath $resolvedHostPath `
    -ReleaseConfigPath $ReleaseConfigPath `
    -ManifestRoot $resolvedManifestRoot
} catch {
  $installationError = $_.Exception
  $rollbackErrors = [System.Collections.Generic.List[string]]::new()

  foreach ($snapshot in $registrySnapshots) {
    try {
      if ($snapshot.Existed) {
        New-Item -Path $snapshot.RegistryPath -Force | Out-Null
        Set-Item -Path $snapshot.RegistryPath -Value $snapshot.Value
      } elseif (Test-Path -LiteralPath $snapshot.RegistryPath) {
        $currentValue = (Get-Item -LiteralPath $snapshot.RegistryPath).GetValue('')
        if ([string]::Equals(
          [string]$currentValue,
          [string]$snapshot.ManifestPath,
          [StringComparison]::OrdinalIgnoreCase
        )) {
          Remove-Item -LiteralPath $snapshot.RegistryPath -Force
        }
      }
    } catch {
      $rollbackErrors.Add("registry $($snapshot.RegistryPath): $($_.Exception.Message)")
    }
  }
  foreach ($snapshot in $fileSnapshots) {
    try {
      if ($snapshot.Existed) {
        [IO.File]::WriteAllBytes([string]$snapshot.Path, [byte[]]$snapshot.Bytes)
      } else {
        Remove-Item -LiteralPath $snapshot.Path -Force -ErrorAction SilentlyContinue
      }
    } catch {
      $rollbackErrors.Add("file $($snapshot.Path): $($_.Exception.Message)")
    }
  }
  if (-not $manifestRootExisted -and
      (Test-Path -LiteralPath $resolvedManifestRoot -PathType Container) -and
      @((Get-ChildItem -LiteralPath $resolvedManifestRoot -Force)).Count -eq 0) {
    try { Remove-Item -LiteralPath $resolvedManifestRoot -Force } catch {
      $rollbackErrors.Add("manifest root $resolvedManifestRoot`: $($_.Exception.Message)")
    }
  }
  if ($rollbackErrors.Count -ne 0) {
    throw "Native Host installation failed: $($installationError.Message) Rollback also failed: $($rollbackErrors -join '; ')"
  }
  throw $installationError
}

Write-Host "Registered $hostName for Chrome and Edge at the current-user scope."
Write-Host 'No extension policy was written and no browser extension was installed.'
