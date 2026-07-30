[CmdletBinding(DefaultParameterSetName = 'Stage')]
param(
  [Parameter(ParameterSetName = 'Stage')]
  [Parameter(ParameterSetName = 'Check')]
  [string]$Repository,

  [Parameter(ParameterSetName = 'Stage')]
  [Parameter(ParameterSetName = 'Check')]
  [long]$ArtifactId,

  [Parameter(ParameterSetName = 'Stage')]
  [Parameter(ParameterSetName = 'Check')]
  [string]$ImageFile,

  [Parameter(ParameterSetName = 'Stage')]
  [Parameter(ParameterSetName = 'Check')]
  [string]$ImageSha256,

  [Parameter(ParameterSetName = 'Stage')]
  [Parameter(ParameterSetName = 'Check')]
  [string]$RedistributionAcknowledgement,

  [Parameter(ParameterSetName = 'Stage')]
  [Parameter(ParameterSetName = 'Check')]
  [string]$DestinationRoot = (Join-Path (Split-Path -Parent $PSScriptRoot) 'apps/desktop/src-tauri/target/verisilo-release-resources/environment'),

  [Parameter(ParameterSetName = 'Stage')]
  [string]$GitHubToken = $env:GITHUB_TOKEN,

  [Parameter(Mandatory = $true, ParameterSetName = 'Check')]
  [switch]$Check,

  [Parameter(Mandatory = $true, ParameterSetName = 'SelfTest')]
  [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ManifestSchema = 'urn:verisilo:hyperv-image-source:1'
$script:RedistributionStatement = 'I_HAVE_VERIFIED_REDISTRIBUTION_RIGHTS'
$script:MaximumImageBytes = 137438953472L

function Assert-ReleaseInputs {
  if ($Repository -cnotmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
    throw 'Repository must be the current owner/repository slug.'
  }
  if ($ArtifactId -lt 1 -or $ArtifactId.ToString([Globalization.CultureInfo]::InvariantCulture) -cnotmatch '^[1-9][0-9]{0,15}$') {
    throw 'ArtifactId must be a positive GitHub Actions artifact ID.'
  }
  Assert-ImageLeafName $ImageFile
  if ($ImageSha256 -cnotmatch '^[0-9a-f]{64}$' -or $ImageSha256 -cmatch '^0{64}$') {
    throw 'ImageSha256 must be a non-zero lowercase SHA-256.'
  }
  if ($RedistributionAcknowledgement -cne $script:RedistributionStatement) {
    throw "RedistributionAcknowledgement must exactly equal $($script:RedistributionStatement)."
  }
}

function Assert-ImageLeafName {
  param([Parameter(Mandatory = $true)][string]$Value)
  $baseName = $Value.Split('.')[0]
  if ($Value -cnotmatch '^[a-z0-9][a-z0-9._-]{0,119}\.vhdx$' -or
      $Value.Contains('..') -or
      $baseName -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$' -or
      [IO.Path]::GetFileName($Value) -cne $Value -or
      $Value.IndexOfAny([char[]]@('/', '\', [char]0, [char]10, [char]13)) -ge 0) {
    throw 'ImageFile must be one lowercase VHDX leaf filename without traversal.'
  }
}

function Test-ReparsePoint {
  param([Parameter(Mandatory = $true)][IO.FileSystemInfo]$Item)
  return ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0
}

function Assert-ArchiveEntry {
  param(
    [Parameter(Mandatory = $true)][IO.Compression.ZipArchive]$Archive,
    [Parameter(Mandatory = $true)][string]$ExpectedLeaf
  )
  $entries = @($Archive.Entries)
  if ($entries.Count -ne 1) {
    throw "The Hyper-V image artifact must contain exactly one entry; found $($entries.Count)."
  }
  $entry = $entries[0]
  if ($entry.FullName -cne $ExpectedLeaf -or $entry.Name -cne $ExpectedLeaf) {
    throw 'The Hyper-V image archive entry must be exactly the declared leaf filename.'
  }
  Assert-ImageLeafName $entry.FullName
  if ($entry.Length -lt 1 -or $entry.Length -gt $script:MaximumImageBytes) {
    throw 'The Hyper-V VHDX entry has an unsupported uncompressed size.'
  }
  $unixMode = ($entry.ExternalAttributes -shr 16) -band 0xF000
  $dosAttributes = $entry.ExternalAttributes -band 0xFFFF
  if ($unixMode -eq 0xA000 -or ($dosAttributes -band [int][IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'The Hyper-V image archive entry must not be a link or reparse point.'
  }
  return $entry
}

function Expand-VerifiedImageEntry {
  param(
    [Parameter(Mandatory = $true)][string]$ArchivePath,
    [Parameter(Mandatory = $true)][string]$ExpectedLeaf,
    [Parameter(Mandatory = $true)][string]$ExpectedSha256,
    [Parameter(Mandatory = $true)][string]$OutputPath
  )
  $archiveItem = Get-Item -LiteralPath $ArchivePath -Force
  if (Test-ReparsePoint $archiveItem) { throw 'Downloaded artifact archive must not be a reparse point.' }
  $archive = [IO.Compression.ZipFile]::OpenRead($archiveItem.FullName)
  try {
    $entry = Assert-ArchiveEntry $archive $ExpectedLeaf
    $outputDirectory = Split-Path -Parent $OutputPath
    [void](New-Item -ItemType Directory -Path $outputDirectory -Force)
    $source = $entry.Open()
    $destination = [IO.File]::Open($OutputPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $source.CopyTo($destination) } finally { $destination.Dispose(); $source.Dispose() }
  } finally {
    $archive.Dispose()
  }
  $actualSha256 = (Get-FileHash -LiteralPath $OutputPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actualSha256 -cne $ExpectedSha256) {
    Remove-Item -LiteralPath $OutputPath -Force -ErrorAction SilentlyContinue
    throw 'Downloaded Hyper-V VHDX does not match ImageSha256.'
  }
}

function Get-SameRepositoryArtifact {
  param(
    [Parameter(Mandatory = $true)][string]$TemporaryDirectory,
    [Parameter(Mandatory = $true)][string]$Token
  )
  if ([string]::IsNullOrWhiteSpace($Token)) {
    throw 'GITHUB_TOKEN with actions:read is required to download the same-repository image artifact.'
  }
  $apiRoot = "https://api.github.com/repos/$Repository/actions/artifacts/$ArtifactId"
  $headers = @{
    Accept = 'application/vnd.github+json'
    Authorization = "Bearer $Token"
    'X-GitHub-Api-Version' = '2026-03-10'
    'User-Agent' = 'VeriSilo-release-image-stager'
  }
  $metadata = Invoke-RestMethod -Method Get -Uri $apiRoot -Headers $headers
  if ([long]$metadata.id -ne $ArtifactId -or [bool]$metadata.expired -or
      [string]$metadata.archive_download_url -cne "$apiRoot/zip" -or
      [long]$metadata.workflow_run.repository_id -lt 1 -or
      [long]$metadata.workflow_run.repository_id -ne [long]$metadata.workflow_run.head_repository_id) {
    throw 'GitHub returned a mismatched, expired, or cross-boundary artifact record.'
  }
  $expiresAt = [DateTimeOffset]::Parse(
    [string]$metadata.expires_at,
    [Globalization.CultureInfo]::InvariantCulture,
    [Globalization.DateTimeStyles]::AssumeUniversal
  )
  if ($expiresAt -le [DateTimeOffset]::UtcNow) {
    throw 'The Hyper-V image Actions artifact is expired.'
  }
  $archivePath = Join-Path $TemporaryDirectory 'artifact.zip'
  Invoke-WebRequest -Method Get -Uri "$apiRoot/zip" -Headers $headers -OutFile $archivePath -MaximumRedirection 5
  return $archivePath
}

function New-Manifest {
  return [ordered]@{
    schema = $script:ManifestSchema
    schemaVersion = 1
    repository = $Repository
    artifactId = $ArtifactId
    imageFile = $ImageFile
    imageSha256 = $ImageSha256
    redistributionAcknowledged = $true
  }
}

function Write-CanonicalJson {
  param([Parameter(Mandatory = $true)]$Value, [Parameter(Mandatory = $true)][string]$Path)
  $json = ($Value | ConvertTo-Json -Depth 8) + [Environment]::NewLine
  [IO.File]::WriteAllText($Path, $json, [Text.UTF8Encoding]::new($false))
}

function Assert-StagedImage {
  $root = [IO.Path]::GetFullPath($DestinationRoot)
  $manifestPath = Join-Path $root 'hyperv-image-manifest.json'
  $imagesRoot = Join-Path $root 'images'
  foreach ($itemPath in @($manifestPath, $imagesRoot)) {
    if (-not (Test-Path -LiteralPath $itemPath)) { throw "Staged Hyper-V image input is missing: $itemPath" }
    if (Test-ReparsePoint (Get-Item -LiteralPath $itemPath -Force)) {
      throw 'Staged Hyper-V image paths must not be reparse points.'
    }
  }
  $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
  $fields = @($manifest.PSObject.Properties.Name | Sort-Object)
  $expectedFields = @('artifactId', 'imageFile', 'imageSha256', 'redistributionAcknowledged', 'repository', 'schema', 'schemaVersion')
  if (($fields -join ',') -cne ($expectedFields -join ',') -or
      [string]$manifest.schema -cne $script:ManifestSchema -or $manifest.schemaVersion -ne 1 -or
      [string]$manifest.repository -cne $Repository -or [long]$manifest.artifactId -ne $ArtifactId -or
      [string]$manifest.imageFile -cne $ImageFile -or [string]$manifest.imageSha256 -cne $ImageSha256 -or
      $manifest.redistributionAcknowledged -ne $true) {
    throw 'Staged Hyper-V image manifest is stale or has unknown fields.'
  }
  $entries = @(Get-ChildItem -LiteralPath $imagesRoot -Force)
  if ($entries.Count -ne 1 -or $entries[0].Name -cne $ImageFile -or $entries[0].PSIsContainer -or
      (Test-ReparsePoint $entries[0])) {
    throw 'Staged Hyper-V images root must contain exactly the declared regular VHDX.'
  }
  $actualSha256 = (Get-FileHash -LiteralPath $entries[0].FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actualSha256 -cne $ImageSha256) { throw 'Staged Hyper-V VHDX SHA-256 is stale or invalid.' }
}

function Install-StagedImage {
  param([Parameter(Mandatory = $true)][string]$VerifiedImagePath)
  $root = [IO.Path]::GetFullPath($DestinationRoot)
  [void](New-Item -ItemType Directory -Path $root -Force)
  $rootItem = Get-Item -LiteralPath $root -Force
  if (Test-ReparsePoint $rootItem) { throw 'Hyper-V release resource root must not be a reparse point.' }
  $imagesRoot = Join-Path $root 'images'
  if (Test-Path -LiteralPath $imagesRoot) {
    $imagesItem = Get-Item -LiteralPath $imagesRoot -Force
    if (-not $imagesItem.PSIsContainer -or (Test-ReparsePoint $imagesItem)) {
      throw 'Existing Hyper-V images staging path is not a safe directory.'
    }
    Remove-Item -LiteralPath $imagesRoot -Recurse -Force
  }
  [void](New-Item -ItemType Directory -Path $imagesRoot)
  Copy-Item -LiteralPath $VerifiedImagePath -Destination (Join-Path $imagesRoot $ImageFile)
  Write-CanonicalJson (New-Manifest) (Join-Path $root 'hyperv-image-manifest.json')
  Assert-StagedImage
}

function New-TestArchive {
  param([string]$Path, [hashtable]$Entries)
  $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
  $archive = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create, $false)
  try {
    foreach ($name in $Entries.Keys) {
      $entry = $archive.CreateEntry($name)
      $writer = [IO.StreamWriter]::new($entry.Open(), [Text.UTF8Encoding]::new($false))
      try { $writer.Write([string]$Entries[$name]) } finally { $writer.Dispose() }
    }
  } finally { $archive.Dispose() }
}

function Assert-SelfTestRejected {
  param([scriptblock]$Action, [string]$Label)
  $rejected = $false
  try { & $Action } catch { $rejected = $true }
  if (-not $rejected) { throw "Hyper-V image staging self-test accepted $Label." }
}

function Invoke-SelfTest {
  $temporary = Join-Path ([IO.Path]::GetTempPath()) "verisilo-image-stage-$([Guid]::NewGuid().ToString('N'))"
  [void](New-Item -ItemType Directory -Path $temporary)
  try {
    $good = Join-Path $temporary 'good.zip'
    $traversal = Join-Path $temporary 'traversal.zip'
    $multiple = Join-Path $temporary 'multiple.zip'
    New-TestArchive $good @{ 'licensed-base.vhdx' = 'fixture' }
    New-TestArchive $traversal @{ '../escape.vhdx' = 'fixture' }
    New-TestArchive $multiple @{ 'licensed-base.vhdx' = 'fixture'; 'extra.vhdx' = 'fixture' }
    $output = Join-Path $temporary 'output.vhdx'
    $fixtureBytes = [Text.Encoding]::UTF8.GetBytes('fixture')
    $expected = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($fixtureBytes)).ToLowerInvariant()
    $archive = [IO.Compression.ZipFile]::OpenRead($good)
    try { [void](Assert-ArchiveEntry $archive 'licensed-base.vhdx') } finally { $archive.Dispose() }
    Expand-VerifiedImageEntry $good 'licensed-base.vhdx' $expected $output
    if ((Get-Content -Raw -LiteralPath $output) -cne 'fixture') { throw 'Valid extraction self-test failed.' }
    Assert-SelfTestRejected {
      $candidate = [IO.Compression.ZipFile]::OpenRead($traversal)
      try { [void](Assert-ArchiveEntry $candidate 'licensed-base.vhdx') } finally { $candidate.Dispose() }
    } 'path traversal'
    Assert-SelfTestRejected {
      $candidate = [IO.Compression.ZipFile]::OpenRead($multiple)
      try { [void](Assert-ArchiveEntry $candidate 'licensed-base.vhdx') } finally { $candidate.Dispose() }
    } 'multiple files'
    Assert-SelfTestRejected {
      Expand-VerifiedImageEntry $good 'licensed-base.vhdx' ('f' * 64) (Join-Path $temporary 'wrong-output.vhdx')
    } 'a wrong image hash'
    if ($expected -cnotmatch '^[0-9a-f]{64}$') { throw 'SHA-256 runtime self-test failed.' }
  } finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
  }
  Write-Host 'Hyper-V image staging self-test passed; no image or network operation was performed.'
}

if ($SelfTest) {
  Invoke-SelfTest
  exit 0
}

Assert-ReleaseInputs
if ($Check) {
  Assert-StagedImage
  Write-Host "Verified staged Hyper-V image $ImageFile from Actions artifact $ArtifactId."
  exit 0
}

$temporary = Join-Path ([IO.Path]::GetTempPath()) "verisilo-image-download-$([Guid]::NewGuid().ToString('N'))"
[void](New-Item -ItemType Directory -Path $temporary)
try {
  $archivePath = Get-SameRepositoryArtifact $temporary $GitHubToken
  $verifiedImagePath = Join-Path $temporary 'verified.vhdx'
  Expand-VerifiedImageEntry $archivePath $ImageFile $ImageSha256 $verifiedImagePath
  Install-StagedImage $verifiedImagePath
} finally {
  Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}

if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
  @(
    "VERISILO_HYPERV_ARTIFACT_ID=$ArtifactId",
    "VERISILO_HYPERV_IMAGE_FILE=$ImageFile",
    "VERISILO_HYPERV_IMAGE_SHA256=$ImageSha256",
    'VERISILO_HYPERV_REDISTRIBUTION_ACKNOWLEDGED=true'
  ) | Out-File -LiteralPath $env:GITHUB_ENV -Encoding utf8 -Append
}
Write-Host "Staged verified Hyper-V VHDX from same-repository Actions artifact $ArtifactId."
