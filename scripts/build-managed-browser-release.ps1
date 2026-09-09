[CmdletBinding()]
param(
  [string]$EnginePackage,
  [string]$ReleaseVersion,
  [string]$OutputDirectory,
  [string]$Python = 'python',
  [ValidateSet('x86_64-pc-windows-msvc')]
  [string]$TargetTriple = 'x86_64-pc-windows-msvc',
  [switch]$Check,
  [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$releaseVersionPattern = '^v[0-9]+\.[0-9]+\.[0-9]+-rc[0-9]+$'
$installerName = $null
$targetRoot = Join-Path $root 'apps/desktop/src-tauri/target'
$stagedPackage = Join-Path $targetRoot 'verisilo-managed-browser-resources/engine-package'
$desktopBinary = Join-Path $targetRoot "$TargetTriple/release/verisilo.exe"
$nsisDirectory = Join-Path $targetRoot "$TargetTriple/release/bundle/nsis"
$verifier = Join-Path $root 'scripts/verify-managed-browser-release.mjs'
$packageBuilder = Join-Path $root 'scripts/build-camoufox-host-package.py'

function Invoke-Checked {
  param(
    [Parameter(Mandatory = $true)] [string]$File,
    [Parameter()] [string[]]$Arguments = @()
  )
  & $File @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "$File failed with exit code $LASTEXITCODE."
  }
}

function Invoke-JsonChecked {
  param(
    [Parameter(Mandatory = $true)] [string]$File,
    [Parameter()] [string[]]$Arguments = @()
  )
  $output = & $File @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "$File failed with exit code $LASTEXITCODE."
  }
  try {
    return ($output -join "`n" | ConvertFrom-Json)
  } catch {
    throw "$File did not return JSON."
  }
}

function Write-Utf8NoBom {
  param(
    [Parameter(Mandatory = $true)] [string]$Path,
    [Parameter(Mandatory = $true)] [string]$Content
  )
  [IO.File]::WriteAllText($Path, $Content, [Text.UTF8Encoding]::new($false))
}

function Assert-ReleaseVersion {
  param([Parameter(Mandatory = $true)] [string]$Value)
  if ($Value -cnotmatch $releaseVersionPattern) {
    throw "ReleaseVersion '$Value' is not a strict candidate version (expected a lowercase version such as v0.1.0-rc2)."
  }
  return $Value
}

function Set-ReleaseEnvironment {
  $revision = (& git -C $root rev-parse HEAD).Trim()
  if ($revision -notmatch '^[0-9a-f]{40}$') {
    throw 'The release source revision is not a full lowercase Git commit.'
  }
  $epoch = (& git -C $root show -s --format=%ct HEAD).Trim()
  if ($epoch -notmatch '^[0-9]+$') {
    throw 'The release source timestamp is invalid.'
  }
  $dirty = (& git -C $root status --porcelain=v1 --untracked-files=all | Out-String).Trim()
  $env:VERISILO_SOURCE_REVISION = $revision
  $env:SOURCE_DATE_EPOCH = $epoch
  $env:VERISILO_SOURCE_DIRTY = if ([string]::IsNullOrEmpty($dirty)) { 'false' } else { 'true' }
  $env:VERISILO_SIGNING_STATE = 'unsigned'
}

function Assert-EnginePackage {
  param([Parameter(Mandatory = $true)] [string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
    throw "Engine package directory does not exist: $Path"
  }
  $result = Invoke-JsonChecked $Python @($packageBuilder, '--check', (Resolve-Path -LiteralPath $Path).Path, '--require-signed')
  $manifest = Get-Content -LiteralPath (Join-Path $Path 'engine-package.json') -Raw | ConvertFrom-Json
  $pins = @(
    @($env:VERISILO_ENGINE_SIGNER_SHA256 -split ',') |
      ForEach-Object { $_.Trim() } |
      Where-Object { $_ -ne '' }
  )
  $invalidPin = @($pins | Where-Object { $_ -notmatch '^[0-9a-f]{64}$' }).Count -gt 0
  if ($pins.Count -eq 0 -or $invalidPin) {
    throw 'VERISILO_ENGINE_SIGNER_SHA256 must contain at least one lowercase public certificate SHA-256 pin.'
  }
  if (-not $result.signed -or $manifest.signature.keyId -notin $pins) {
    throw 'The signed engine package certificate is not one of the release-embedded signer pins.'
  }
  return $result
}

function Assert-ManagedConfig {
  param([switch]$RequireSigner)
  $configPath = Join-Path $root 'apps/desktop/src-tauri/tauri.managed-browser.conf.json'
  $resetPath = Join-Path $root 'apps/desktop/src-tauri/tauri.release-reset.conf.json'
  $config = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
  $reset = Get-Content -LiteralPath $resetPath -Raw | ConvertFrom-Json
  $resourceProperties = @($config.bundle.resources.PSObject.Properties)
  if (
    $config.bundle.active -ne $true -or
    @($config.bundle.targets).Count -ne 1 -or
    $config.bundle.targets[0] -cne 'nsis' -or
    $config.bundle.externalBin -ne $null -or
    $resourceProperties.Count -ne 1 -or
    $config.bundle.windows.nsis.installMode -cne 'currentUser' -or
    $config.bundle.windows.nsis.installerHooks -ne $null -or
    $resourceProperties[0].Name -cne 'target/verisilo-managed-browser-resources/engine-package/' -or
    $resourceProperties[0].Value -cne 'managed-browser/engine-package/' -or
    -not ($reset.bundle.resources -is [array]) -or
    $reset.bundle.resources.Count -ne 0
  ) {
    throw 'Managed-browser Tauri config is not the bounded current-user profile.'
  }
  if ($RequireSigner) {
    $pins = @(
      @($env:VERISILO_ENGINE_SIGNER_SHA256 -split ',') |
        ForEach-Object { $_.Trim() } |
        Where-Object { $_ -ne '' }
    )
    $invalidPin = @($pins | Where-Object { $_ -notmatch '^[0-9a-f]{64}$' }).Count -gt 0
    if ($pins.Count -eq 0 -or $invalidPin) {
      throw 'VERISILO_ENGINE_SIGNER_SHA256 is required to build the production managed-browser verifier.'
    }
  }
}

function Self-Test {
  Set-Location -LiteralPath $root
  $null = Assert-ReleaseVersion 'v0.1.0-rc2'
  $null = Assert-ReleaseVersion 'v9.8.7-rc12'
  foreach ($bad in @(
    '0.1.0-rc2',
    'v0.1.0-rc2-x',
    'v0.1.0',
    'v0.1.0-beta1',
    'V0.1.0-rc2',
    'v0.1.0-RC2',
    'v0.1.0-rc2 ',
    'v.1.0-rc2',
    ''
  )) {
    $rejected = $false
    try {
      $null = Assert-ReleaseVersion $bad
    } catch {
      $rejected = $true
    }
    if (-not $rejected) {
      throw "ReleaseVersion self-test accepted the malformed candidate version '$bad'."
    }
  }
  $previousSignerPins = $env:VERISILO_ENGINE_SIGNER_SHA256
  try {
    $env:VERISILO_ENGINE_SIGNER_SHA256 = 'a' * 64
    Assert-ManagedConfig -RequireSigner
  } finally {
    $env:VERISILO_ENGINE_SIGNER_SHA256 = $previousSignerPins
  }
  Invoke-Checked $Python @($packageBuilder, '--self-test')
  Invoke-Checked 'node' @($verifier, '--self-test')
  Write-Output 'Managed-browser release orchestrator self-test passed.'
}

if ($SelfTest) {
  Self-Test
  exit 0
}

if ([string]::IsNullOrWhiteSpace($EnginePackage)) {
  if (-not $Check) {
    throw '-EnginePackage is required; pass the signed output directory from build-camoufox-host-package.py.'
  }
}

if (-not $Check) {
  if ([string]::IsNullOrWhiteSpace($ReleaseVersion)) {
    throw '-ReleaseVersion is required to build a candidate; pass the explicit source-bound candidate version (e.g. -ReleaseVersion v0.1.0-rc2).'
  }
}
if (-not [string]::IsNullOrWhiteSpace($ReleaseVersion)) {
  $ReleaseVersion = Assert-ReleaseVersion $ReleaseVersion.Trim()
  $installerName = "VeriSilo-Managed-Browser-$ReleaseVersion-x64-setup.exe"
}

Set-Location -LiteralPath $root
Set-ReleaseEnvironment
if (-not $Check -and $env:VERISILO_SOURCE_DIRTY -eq 'true') {
  throw 'The release source tree is dirty; a candidate build requires a clean canonical checkout.'
}
if (-not [string]::IsNullOrWhiteSpace($ReleaseVersion)) {
  $defaultOutput = "artifacts/release/managed-browser/$ReleaseVersion"
  if ($PSBoundParameters.ContainsKey('OutputDirectory')) {
    if (-not $OutputDirectory.TrimEnd('/\').EndsWith($ReleaseVersion, [StringComparison]::Ordinal)) {
      throw "OutputDirectory '$OutputDirectory' does not end with candidate version '$ReleaseVersion'; the output path must come from the same ReleaseVersion."
    }
  } else {
    $OutputDirectory = $defaultOutput
  }
} elseif ($Check -and -not $PSBoundParameters.ContainsKey('OutputDirectory')) {
  throw '-Check requires either -OutputDirectory or -ReleaseVersion to locate the release directory.'
}
$releasePath = if ([IO.Path]::IsPathRooted($OutputDirectory)) {
  [IO.Path]::GetFullPath($OutputDirectory)
} else {
  [IO.Path]::GetFullPath((Join-Path $root $OutputDirectory))
}

if ($Check) {
  if (-not (Test-Path -LiteralPath $releasePath -PathType Container)) {
    throw "Release output does not exist: $releasePath"
  }
  $finalPackage = Join-Path $releasePath 'engine-package'
  $null = Assert-EnginePackage $finalPackage
  Invoke-Checked 'node' @($verifier, '--check', '--release', $releasePath, '--engine-package', $finalPackage, '--python', $Python)
  Invoke-Checked 'node' @('scripts/generate-sbom.mjs', '--out', (Join-Path $releasePath 'sbom'), '--profile', 'managed-browser-windows', '--check')
  Invoke-Checked 'pnpm' @('run', 'managed-browser:licenses', '--', '--out', (Join-Path $releasePath 'dependency-licenses.json'), '--check')
  Invoke-Checked 'node' @('scripts/generate-release-metadata.mjs', '--dir', $releasePath, '--profile', 'managed-browser-windows', '--check')
  Write-Output "Managed-browser release verified: $releasePath"
  exit 0
}

Assert-ManagedConfig -RequireSigner
$packagePath = (Resolve-Path -LiteralPath $EnginePackage).Path
$null = Assert-EnginePackage $packagePath
if (Test-Path -LiteralPath $releasePath) {
  throw "Release output already exists; remove this exact generated directory before rebuilding: $releasePath"
}
if (Test-Path -LiteralPath $stagedPackage) {
  throw "Managed-browser engine staging already exists; clean this exact target directory before rebuilding: $stagedPackage"
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $stagedPackage) | Out-Null
New-Item -ItemType Directory -Force -Path $stagedPackage | Out-Null
foreach ($member in Get-ChildItem -LiteralPath $packagePath -Force) {
  Copy-Item -LiteralPath $member.FullName -Destination $stagedPackage -Recurse -Force
}

Invoke-Checked 'pnpm' @(
  '--filter', '@verisilo/desktop', 'exec', 'tauri', 'build', '--ci', '--no-sign',
  '--target', $TargetTriple,
  '--config', 'src-tauri/tauri.release-reset.conf.json',
  '--config', 'src-tauri/tauri.managed-browser.conf.json'
)

if (-not (Test-Path -LiteralPath $desktopBinary -PathType Leaf)) {
  throw "Tauri did not produce $desktopBinary"
}
$installers = @(Get-ChildItem -LiteralPath $nsisDirectory -Filter '*.exe' -File)
if ($installers.Count -ne 1) {
  throw "Expected exactly one managed-browser NSIS installer, found $($installers.Count)."
}

New-Item -ItemType Directory -Force -Path $releasePath | Out-Null
Copy-Item -LiteralPath $desktopBinary -Destination (Join-Path $releasePath 'verisilo.exe')
Copy-Item -LiteralPath $installers[0].FullName -Destination (Join-Path $releasePath $installerName)
Write-Utf8NoBom -Path (Join-Path $releasePath 'README.txt') -Content @"
VeriSilo Managed Browser candidate $releaseVersion
==================================================

This candidate is a self-contained Windows x64 build for local evaluation. It
bundles the current CMS-signed Camoufox engine package; the target computer
does not need Git, Python, uv, Node, Rust, or a separately installed Camoufox.

The release folder contains verisilo.exe, the exact current-user NSIS installer
$installerName, the signed engine-package/ tree, SHA256SUMS, provenance.json,
authenticode-status.json, pending windows-acceptance-report.json/.md, SBOM and
license evidence, LICENSE, and THIRD_PARTY_NOTICES.md. It contains no Hyper-V
or environment files, extension files, Native Host binaries, or installer
hooks.

The installer and verisilo.exe are intentionally outer-unsigned for this local
candidate; authenticode-status.json records that unsigned boundary. The engine
package carries a mandatory detached CMS signature and is rejected before
launch if its signer, manifest, Host, or browser tree does not match the pin
embedded in the Desktop build.

Install and start: run the installer as the current user, then create or unlock
the local Vault and choose Create Silo. Reinstalling the same version repairs
the application while preserving the Vault and Silo data under
%LOCALAPPDATA%\io.verisilo.app. Uninstall removes the application but never
silently deletes that data.

The Windows acceptance report is Pending; it changes only when a real packaged
runtime acceptance run supplies evidence. This generated guide contains no
runtime result and claims no public release.
"@
Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $releasePath
Copy-Item -LiteralPath (Join-Path $root 'THIRD_PARTY_NOTICES.md') -Destination $releasePath

$finalPackage = Join-Path $releasePath 'engine-package'
New-Item -ItemType Directory -Force -Path $finalPackage | Out-Null
foreach ($member in Get-ChildItem -LiteralPath $packagePath -Force) {
  Copy-Item -LiteralPath $member.FullName -Destination $finalPackage -Recurse -Force
}

$manifestPath = Join-Path $finalPackage 'engine-package.json'
$manifestRaw = Get-Content -LiteralPath $manifestPath -Raw
$manifest = $manifestRaw | ConvertFrom-Json
$manifestDigest = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
$treeDigest = (Get-FileHash -LiteralPath (Join-Path $finalPackage 'package-tree.json') -Algorithm SHA256).Hash.ToLowerInvariant()
$browserTreeDigest = (Get-FileHash -LiteralPath (Join-Path $finalPackage 'browser-tree-manifest.json') -Algorithm SHA256).Hash.ToLowerInvariant()
$acceptance = [ordered]@{
  schema = 'urn:verisilo:managed-browser-windows-acceptance:1'
  schemaVersion = 1
  profile = 'managed-browser-windows'
  release = $releaseVersion
  status = 'Pending'
  verified = $false
  basis = 'Build-only artifact; clean Windows 11 runtime acceptance has not been run.'
  runtimeAcceptance = $null
  desktopExecutable = 'verisilo.exe'
  installer = $installerName
  outerAuthenticode = 'unsigned'
  enginePackageRoot = 'engine-package'
  enginePackage = [ordered]@{
    signed = $true
    signatureAlgorithm = [string]$manifest.signature.algorithm
    signatureKeyId = [string]$manifest.signature.keyId
    manifestSha256 = $manifestDigest
    packageTreeSha256 = $treeDigest
    browserTreeSha256 = $browserTreeDigest
  }
  dataRoot = '%LOCALAPPDATA%\io.verisilo.app'
  uninstaller = [ordered]@{ dataPolicy = 'preserve' }
}
Write-Utf8NoBom -Path (Join-Path $releasePath 'windows-acceptance-report.json') -Content (($acceptance | ConvertTo-Json -Depth 10) + "`n")
Write-Utf8NoBom -Path (Join-Path $releasePath 'windows-acceptance-report.md') -Content @"
# Managed Browser Windows acceptance report

Status: Pending
Release: $releaseVersion
Basis: Build-only artifact; clean Windows 11 runtime acceptance has not been run.
Installer: $installerName
Desktop executable: verisilo.exe
Engine package: engine-package
Outer Authenticode: unsigned
Uninstaller data policy: preserve `%LOCALAPPDATA%\io.verisilo.app`.

Attach clean Windows 11 runtime evidence before changing this report to any
runtime verdict. This generated report intentionally contains no runtime result.
"@

Invoke-Checked 'node' @('scripts/generate-sbom.mjs', '--out', (Join-Path $releasePath 'sbom'), '--profile', 'managed-browser-windows')
Invoke-Checked 'pnpm' @('run', 'managed-browser:licenses', '--', '--out', (Join-Path $releasePath 'dependency-licenses.json'))
& (Join-Path $root 'scripts/authenticode-gate.ps1') -Check -Mode Unsigned -ReleaseDirectory $releasePath -IncludeRelativePath @('verisilo.exe', $installerName) -ReportPath (Join-Path $releasePath 'authenticode-status.json')
Invoke-Checked 'node' @('scripts/generate-release-metadata.mjs', '--dir', $releasePath, '--profile', 'managed-browser-windows')
Invoke-Checked 'node' @('scripts/generate-release-metadata.mjs', '--dir', $releasePath, '--profile', 'managed-browser-windows', '--check')
Invoke-Checked 'node' @($verifier, '--check', '--release', $releasePath, '--engine-package', $finalPackage, '--python', $Python)
Write-Output "Managed-browser release candidate $releaseVersion staged at $releasePath"
