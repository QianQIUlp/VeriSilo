[CmdletBinding(DefaultParameterSetName = 'Check')]
param(
  [Parameter(ParameterSetName = 'Check', Mandatory = $true)]
  [switch]$Check,

  [Parameter(ParameterSetName = 'Check', Mandatory = $true)]
  [ValidateSet('Unsigned', 'DryRunSigning', 'SignAndVerify', 'VerifySigned')]
  [string]$Mode,

  [Parameter(ParameterSetName = 'Check', Mandatory = $true)]
  [ValidateScript({ Test-Path $_ -PathType Container })]
  [string]$ReleaseDirectory,

  [Parameter(ParameterSetName = 'Check')]
  [string[]]$IncludeRelativePath,

  [Parameter(ParameterSetName = 'Check')]
  [string]$ReportPath,

  [Parameter(ParameterSetName = 'Check')]
  [string]$CertificatePath,

  [Parameter(ParameterSetName = 'Check')]
  [string]$ExpectedSignerCertificateSha256,

  [Parameter(ParameterSetName = 'Check')]
  [string]$TimestampUrl = 'https://timestamp.digicert.com',

  [Parameter(ParameterSetName = 'SelfTest', Mandatory = $true)]
  [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-HttpsTimestampUrl {
  param([Parameter(Mandatory = $true)] [string]$Value)
  $uri = [Uri]$Value
  if ($uri.Scheme -ne 'https' -or -not $uri.IsAbsoluteUri) {
    throw 'Authenticode timestamp URL must be an absolute HTTPS URL.'
  }
}

function Assert-SignableExtension {
  param([Parameter(Mandatory = $true)] [string]$Path)
  $extension = [IO.Path]::GetExtension($Path).ToLowerInvariant()
  if ($extension -notin @('.exe', '.ps1')) {
    throw "Authenticode gate only accepts EXE and PS1 inputs: $Path"
  }
}

function Assert-Sha256Hex {
  param([Parameter(Mandatory = $true)] [string]$Value)
  if ($Value -cnotmatch '^[0-9a-f]{64}$' -or $Value -match '^0{64}$') {
    throw 'Expected signer certificate SHA-256 must be 64 lowercase, non-zero hex characters.'
  }
}

function Get-CertificateSha256 {
  param(
    [Parameter(Mandatory = $true)]
    [Security.Cryptography.X509Certificates.X509Certificate2]$Certificate
  )
  return $Certificate.GetCertHashString(
    [Security.Cryptography.HashAlgorithmName]::SHA256
  ).ToLowerInvariant()
}

function Assert-ExpectedSignerCertificate {
  param(
    [Parameter(Mandatory = $true)]
    [Security.Cryptography.X509Certificates.X509Certificate2]$Certificate,
    [Parameter(Mandatory = $true)]
    [string]$ExpectedSha256
  )
  Assert-Sha256Hex -Value $ExpectedSha256
  $actual = Get-CertificateSha256 -Certificate $Certificate
  if ($actual -cne $ExpectedSha256) {
    throw "Authenticode signer certificate SHA-256 does not match the release-pinned signer."
  }
}

function Get-SignToolPath {
  $command = Get-Command 'signtool.exe' -ErrorAction SilentlyContinue
  if ($null -ne $command) {
    return $command.Source
  }
  $programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
  if ([string]::IsNullOrWhiteSpace($programFilesX86)) {
    return $null
  }
  $kitsBin = Join-Path $programFilesX86 'Windows Kits\10\bin'
  if (-not (Test-Path -LiteralPath $kitsBin -PathType Container)) {
    return $null
  }
  $candidate = @(
    Get-ChildItem -LiteralPath $kitsBin -Filter 'signtool.exe' -File -Recurse |
      Where-Object { $_.FullName -match '[\\/]x64[\\/]signtool\.exe$' } |
      Sort-Object FullName -Descending
  ) | Select-Object -First 1
  if ($null -eq $candidate) {
    return $null
  }
  return $candidate.FullName
}

function Get-SignatureReportEntry {
  param(
    [Parameter(Mandatory = $true)] [string]$Root,
    [Parameter(Mandatory = $true)] [IO.FileInfo]$File,
    [Parameter(Mandatory = $true)] $Signature
  )
  $entry = [ordered]@{
    path = [IO.Path]::GetRelativePath($Root, $File.FullName).Replace('\', '/')
    status = [string]$Signature.Status
  }
  if ($null -ne $Signature.SignerCertificate) {
    $entry.signerThumbprint = $Signature.SignerCertificate.Thumbprint
    $entry.signerCertificateSha256 = Get-CertificateSha256 -Certificate $Signature.SignerCertificate
    $entry.signerSubject = $Signature.SignerCertificate.Subject
  }
  if ($null -ne $Signature.TimeStamperCertificate) {
    $entry.timestampThumbprint = $Signature.TimeStamperCertificate.Thumbprint
    $entry.timestampSubject = $Signature.TimeStamperCertificate.Subject
  }
  return $entry
}

if ($SelfTest) {
  Assert-HttpsTimestampUrl -Value 'https://timestamp.example.invalid'
  $rejected = $false
  try {
    Assert-HttpsTimestampUrl -Value 'http://timestamp.example.invalid'
  } catch {
    $rejected = $true
  }
  if (-not $rejected) {
    throw 'Authenticode gate self-test failed to reject an insecure timestamp URL.'
  }
  Assert-SignableExtension -Path 'fixture.ps1'
  $rejected = $false
  try {
    Assert-SignableExtension -Path 'fixture.txt'
  } catch {
    $rejected = $true
  }
  if (-not $rejected) {
    throw 'Authenticode gate self-test failed to reject a non-signable input.'
  }
  Assert-Sha256Hex -Value ('a' * 64)
  $rejected = $false
  try {
    Assert-Sha256Hex -Value ('A' * 64)
  } catch {
    $rejected = $true
  }
  if (-not $rejected) {
    throw 'Authenticode gate self-test failed to reject a non-canonical signer pin.'
  }
  Write-Host 'Authenticode input gate self-test passed.'
  exit 0
}

if ($Mode -in @('DryRunSigning', 'SignAndVerify', 'VerifySigned')) {
  if ([string]::IsNullOrWhiteSpace($ExpectedSignerCertificateSha256)) {
    throw "$Mode requires an expected signer certificate SHA-256 pin."
  }
  Assert-Sha256Hex -Value $ExpectedSignerCertificateSha256
}

$resolvedReleaseDirectory = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
$releasePrefix = $resolvedReleaseDirectory.TrimEnd(
  [IO.Path]::DirectorySeparatorChar,
  [IO.Path]::AltDirectorySeparatorChar
) + [IO.Path]::DirectorySeparatorChar

if ($null -eq $IncludeRelativePath -or $IncludeRelativePath.Count -eq 0) {
  $signableFiles = @(
    Get-ChildItem -LiteralPath $resolvedReleaseDirectory -File -Recurse |
      Where-Object { $_.Extension.ToLowerInvariant() -in @('.exe', '.ps1') } |
      Sort-Object FullName
  )
} else {
  $seenPaths = [Collections.Generic.HashSet[string]]::new(
    [StringComparer]::OrdinalIgnoreCase
  )
  $signableFiles = @(@(
    foreach ($relativePath in $IncludeRelativePath) {
      if ([IO.Path]::IsPathRooted($relativePath)) {
        throw "Authenticode include paths must be relative: $relativePath"
      }
      Assert-SignableExtension -Path $relativePath
      $candidate = [IO.Path]::GetFullPath(
        (Join-Path $resolvedReleaseDirectory $relativePath)
      )
      if (-not $candidate.StartsWith($releasePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Authenticode include path escapes the release directory: $relativePath"
      }
      $resolvedCandidate = (Resolve-Path -LiteralPath $candidate).Path
      if (-not $resolvedCandidate.StartsWith($releasePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Resolved Authenticode include path escapes the release directory: $relativePath"
      }
      if (-not (Test-Path -LiteralPath $resolvedCandidate -PathType Leaf)) {
        throw "Authenticode include path is not a file: $relativePath"
      }
      if (-not $seenPaths.Add($resolvedCandidate)) {
        throw "Authenticode include path is duplicated: $relativePath"
      }
      Get-Item -LiteralPath $resolvedCandidate
    }
  ) | Sort-Object FullName)
}

if ($signableFiles.Count -eq 0) {
  throw 'Release directory does not contain any selected Authenticode-signable EXE or PS1 files.'
}

$report = [ordered]@{
  schemaVersion = 1
  mode = $Mode
  signingState = 'unverified'
  expectedSignerCertificateSha256 = if ([string]::IsNullOrWhiteSpace($ExpectedSignerCertificateSha256)) { $null } else { $ExpectedSignerCertificateSha256 }
  files = @()
}

switch ($Mode) {
  'Unsigned' {
    foreach ($file in $signableFiles) {
      $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
      if ($signature.Status -ne 'NotSigned') {
        throw "$($file.Name) was expected to be unsigned, but its status is $($signature.Status)."
      }
      $report.files += Get-SignatureReportEntry -Root $resolvedReleaseDirectory -File $file -Signature $signature
    }
    $report.signingState = 'unsigned'
  }
  'DryRunSigning' {
    Assert-HttpsTimestampUrl -Value $TimestampUrl
    if ([string]::IsNullOrWhiteSpace($CertificatePath) -or
        -not (Test-Path -LiteralPath $CertificatePath -PathType Leaf)) {
      throw 'DryRunSigning requires an existing certificate input. No signing was attempted.'
    }
    if ([string]::IsNullOrWhiteSpace($env:VERISILO_AUTHENTICODE_PASSWORD)) {
      throw 'DryRunSigning requires VERISILO_AUTHENTICODE_PASSWORD. Its value is never printed.'
    }
    $flags = [Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet
    $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new(
      $CertificatePath,
      $env:VERISILO_AUTHENTICODE_PASSWORD,
      $flags
    )
    try {
      Assert-ExpectedSignerCertificate -Certificate $certificate -ExpectedSha256 $ExpectedSignerCertificateSha256
      if (-not $certificate.HasPrivateKey) {
        throw 'The supplied Authenticode certificate has no accessible private key.'
      }
    } finally {
      $certificate.Dispose()
    }
    $signtool = Get-SignToolPath
    if ($null -eq $signtool -and @($signableFiles | Where-Object Extension -eq '.exe').Count -gt 0) {
      throw 'signtool.exe is required to sign Windows executables. No signing was attempted.'
    }
    $report.signingState = 'dry-run-inputs-validated-not-signed'
    $report.timestampUrl = $TimestampUrl
    foreach ($file in $signableFiles) {
      $report.files += [ordered]@{
        path = [IO.Path]::GetRelativePath($resolvedReleaseDirectory, $file.FullName).Replace('\', '/')
        status = 'SigningNotAttempted'
      }
    }
  }
  'SignAndVerify' {
    Assert-HttpsTimestampUrl -Value $TimestampUrl
    if ([string]::IsNullOrWhiteSpace($CertificatePath) -or
        -not (Test-Path -LiteralPath $CertificatePath -PathType Leaf)) {
      throw 'SignAndVerify requires an existing PFX certificate input.'
    }
    if ([string]::IsNullOrWhiteSpace($env:VERISILO_AUTHENTICODE_PASSWORD)) {
      throw 'SignAndVerify requires VERISILO_AUTHENTICODE_PASSWORD. Its value is never printed.'
    }
    $signtool = Get-SignToolPath
    if ($null -eq $signtool -and @($signableFiles | Where-Object Extension -eq '.exe').Count -gt 0) {
      throw 'signtool.exe is required to sign Windows executables.'
    }
    $flags = [Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet
    $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new(
      $CertificatePath,
      $env:VERISILO_AUTHENTICODE_PASSWORD,
      $flags
    )
    try {
      Assert-ExpectedSignerCertificate -Certificate $certificate -ExpectedSha256 $ExpectedSignerCertificateSha256
      if (-not $certificate.HasPrivateKey) {
        throw 'The supplied Authenticode certificate has no accessible private key.'
      }
      foreach ($file in $signableFiles) {
        if ($file.Extension.ToLowerInvariant() -eq '.exe') {
          & $signtool sign /fd SHA256 /td SHA256 /tr $TimestampUrl /f $CertificatePath /p $env:VERISILO_AUTHENTICODE_PASSWORD $file.FullName
          if ($LASTEXITCODE -ne 0) {
            throw "signtool failed for $($file.Name)."
          }
        } else {
          $signature = Set-AuthenticodeSignature -LiteralPath $file.FullName -Certificate $certificate -HashAlgorithm SHA256 -TimestampServer $TimestampUrl
          if ($signature.Status -ne 'Valid') {
            throw "PowerShell Authenticode signing failed for $($file.Name): $($signature.Status)."
          }
        }
        $verified = Get-AuthenticodeSignature -LiteralPath $file.FullName
        if ($verified.Status -ne 'Valid' -or
            $null -eq $verified.SignerCertificate -or
            $null -eq $verified.TimeStamperCertificate) {
          throw "$($file.Name) does not have a valid signer and timestamp immediately after signing."
        }
        Assert-ExpectedSignerCertificate -Certificate $verified.SignerCertificate -ExpectedSha256 $ExpectedSignerCertificateSha256
        $report.files += Get-SignatureReportEntry -Root $resolvedReleaseDirectory -File $file -Signature $verified
      }
    } finally {
      $certificate.Dispose()
    }
    $report.signingState = 'signed-and-verified'
    $report.timestampUrl = $TimestampUrl
  }
  'VerifySigned' {
    foreach ($file in $signableFiles) {
      $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
      if ($signature.Status -ne 'Valid' -or
          $null -eq $signature.SignerCertificate -or
          $null -eq $signature.TimeStamperCertificate) {
        throw "$($file.Name) does not have a valid Authenticode signer and timestamp."
      }
      Assert-ExpectedSignerCertificate -Certificate $signature.SignerCertificate -ExpectedSha256 $ExpectedSignerCertificateSha256
      $report.files += Get-SignatureReportEntry -Root $resolvedReleaseDirectory -File $file -Signature $signature
    }
    $report.signingState = 'signed-and-verified'
  }
}

$resolvedReportPath = if ([string]::IsNullOrWhiteSpace($ReportPath)) {
  Join-Path $resolvedReleaseDirectory 'authenticode-status.json'
} else {
  [IO.Path]::GetFullPath($ReportPath)
}
$reportParent = Split-Path -Parent $resolvedReportPath
if (-not (Test-Path -LiteralPath $reportParent -PathType Container)) {
  [void](New-Item -ItemType Directory -Force -Path $reportParent)
}
[IO.File]::WriteAllText(
  $resolvedReportPath,
  (($report | ConvertTo-Json -Depth 8) + [Environment]::NewLine),
  [Text.UTF8Encoding]::new($false)
)

Write-Host "Authenticode gate completed in $Mode mode. Signing state: $($report.signingState)."
