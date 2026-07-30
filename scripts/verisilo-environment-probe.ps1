[CmdletBinding(DefaultParameterSetName = 'Probe')]
param(
  [Parameter(ParameterSetName = 'Probe', Mandatory = $true)]
  [ValidatePattern('^[0-9a-f]{64}$')]
  [string]$ExpectedSignerCertificateSha256,

  [Parameter(ParameterSetName = 'SelfTest', Mandatory = $true)]
  [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'

function Test-Administrator {
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = [Security.Principal.WindowsPrincipal]::new($identity)
  return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Test-OptionalFeatureEnabled {
  param([Parameter(Mandatory = $true)] [string]$FeatureName)
  try {
    $feature = Get-WindowsOptionalFeature -Online -FeatureName $FeatureName -ErrorAction Stop
    return $feature.State -eq 'Enabled'
  } catch {
    return $false
  }
}

function Test-RebootPending {
  $paths = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending',
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired'
  )
  foreach ($path in $paths) {
    if (Test-Path -LiteralPath $path) { return $true }
  }
  return $false
}

function Test-ReleaseScriptBoundary {
  param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{64}$')]
    [string]$ExpectedSignerCertificateSha256
  )
  $paths = @(
    $PSCommandPath,
    (Join-Path $PSScriptRoot 'verisilo-hyperv.ps1'),
    (Join-Path $PSScriptRoot 'verisilo-sandbox.ps1'),
    (Join-Path $PSScriptRoot 'verisilo-sandbox-bootstrap.ps1')
  )
  foreach ($path in $paths) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $false }
    $item = Get-Item -LiteralPath $path -Force
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return $false }
    $signature = Get-AuthenticodeSignature -LiteralPath $item.FullName
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or
        $null -eq $signature.SignerCertificate -or
        $null -eq $signature.TimeStamperCertificate) {
      return $false
    }
    $actualSignerCertificateSha256 = $signature.SignerCertificate.GetCertHashString(
      [Security.Cryptography.HashAlgorithmName]::SHA256
    ).ToLowerInvariant()
    if ($actualSignerCertificateSha256 -cne $ExpectedSignerCertificateSha256) {
      return $false
    }
  }
  return $true
}

if ($SelfTest) {
  $sample = [ordered]@{
    schemaVersion = 1
    supportedSku = $false
    administrator = $false
    virtualizationEnabled = $false
    hypervEnabled = $false
    rebootRequired = $false
    sandboxAvailable = $false
    releaseScriptsTrusted = $false
  }
  $roundTrip = ($sample | ConvertTo-Json -Compress | ConvertFrom-Json)
  if ($roundTrip.schemaVersion -ne 1 -or $null -eq $roundTrip.sandboxAvailable -or
      $null -eq $roundTrip.releaseScriptsTrusted) {
    throw 'Environment probe self-test failed.'
  }
  Write-Host 'Environment probe self-test passed.'
  exit 0
}

$caption = ''
$virtualizationEnabled = $false
try {
  $os = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop
  $caption = [string]$os.Caption
  $computer = Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction Stop
  $processors = @(Get-CimInstance -ClassName Win32_Processor -ErrorAction Stop)
  $virtualizationEnabled = [bool]$computer.HypervisorPresent -or
    ($processors.Count -gt 0 -and @($processors | Where-Object { $_.VirtualizationFirmwareEnabled }).Count -eq $processors.Count)
} catch {
  $virtualizationEnabled = $false
}

$hypervEnabled = Test-OptionalFeatureEnabled -FeatureName 'Microsoft-Hyper-V-All'
$sandboxFeatureEnabled = Test-OptionalFeatureEnabled -FeatureName 'Containers-DisposableClientVM'
$sandboxExecutable = Join-Path $env:WINDIR 'System32\WindowsSandbox.exe'
$supportedSku = -not [string]::IsNullOrWhiteSpace($caption) -and $caption -notmatch '\bHome\b'

$result = [ordered]@{
  schemaVersion = 1
  supportedSku = $supportedSku
  administrator = Test-Administrator
  virtualizationEnabled = $virtualizationEnabled
  hypervEnabled = $hypervEnabled
  rebootRequired = Test-RebootPending
  sandboxAvailable = $sandboxFeatureEnabled -and (Test-Path -LiteralPath $sandboxExecutable -PathType Leaf)
  releaseScriptsTrusted = Test-ReleaseScriptBoundary -ExpectedSignerCertificateSha256 $ExpectedSignerCertificateSha256
}

$result | ConvertTo-Json -Compress
