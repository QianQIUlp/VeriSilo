[CmdletBinding(DefaultParameterSetName = 'Download')]
param(
  [Parameter(ParameterSetName = 'Download')]
  [string]$Repository,

  [Parameter(ParameterSetName = 'Download')]
  [long]$ArtifactId,

  [Parameter(ParameterSetName = 'Download')]
  [string]$ArtifactSha256,

  [Parameter(ParameterSetName = 'Download')]
  [string]$SourceRevision,

  [Parameter(ParameterSetName = 'Download')]
  [string]$DestinationDirectory,

  [Parameter(ParameterSetName = 'Download')]
  [string]$MetadataPath,

  [Parameter(ParameterSetName = 'Download')]
  [string]$GitHubToken = $env:GITHUB_TOKEN,

  [Parameter(Mandatory = $true, ParameterSetName = 'SelfTest')]
  [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:MaximumEntries = 20000
$script:MaximumUncompressedBytes = 274877906944L

function Test-ReparsePoint {
  param([Parameter(Mandatory = $true)][IO.FileSystemInfo]$Item)
  return ($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0
}

function Assert-Inputs {
  if ($Repository -cnotmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
    throw 'Repository must be the current owner/repository slug.'
  }
  if ($ArtifactId -lt 1 -or $ArtifactId.ToString([Globalization.CultureInfo]::InvariantCulture) -cnotmatch '^[1-9][0-9]{0,15}$') {
    throw 'ArtifactId must be a positive GitHub Actions artifact ID.'
  }
  if ($ArtifactSha256 -cnotmatch '^[0-9a-f]{64}$' -or $ArtifactSha256 -cmatch '^0{64}$') {
    throw 'ArtifactSha256 must be the non-zero lowercase digest emitted by upload-artifact.'
  }
  if ($SourceRevision -cnotmatch '^[0-9a-f]{40}$') {
    throw 'SourceRevision must be the full lowercase candidate source revision.'
  }
  if ([string]::IsNullOrWhiteSpace($DestinationDirectory) -or [string]::IsNullOrWhiteSpace($MetadataPath)) {
    throw 'DestinationDirectory and MetadataPath are required fixed runner-temporary paths.'
  }
  if ([string]::IsNullOrWhiteSpace($GitHubToken)) {
    throw 'GITHUB_TOKEN with actions:read is required.'
  }
}

function Assert-SafeEntryName {
  param([Parameter(Mandatory = $true)][string]$Value)
  if ($Value.Length -gt 512 -or $Value.Length -eq 0 -or $Value.StartsWith('/', [StringComparison]::Ordinal) -or
      $Value -cnotmatch '^[A-Za-z0-9._+/\-]+$' -or $Value.Contains('\') -or $Value.Contains(':') -or $Value.IndexOf([char]0) -ge 0 -or
      $Value.IndexOf([char]10) -ge 0 -or $Value.IndexOf([char]13) -ge 0) {
    throw "Candidate archive contains an unsafe path: $Value"
  }
  $trimmed = $Value.TrimEnd('/')
  $segments = @($trimmed.Split('/'))
  if ($trimmed.Length -eq 0 -or $segments.Count -eq 0 -or
      @($segments | Where-Object {
        $baseName = $_.Split('.')[0]
        $_.Length -eq 0 -or $_ -ceq '.' -or $_ -ceq '..' -or $_.EndsWith('.') -or $_.EndsWith(' ') -or
        $baseName -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$'
      }).Count -gt 0) {
    throw "Candidate archive contains traversal or an empty path segment: $Value"
  }
}

function Assert-ArchiveEntries {
  param([Parameter(Mandatory = $true)][IO.Compression.ZipArchive]$Archive)
  $entries = @($Archive.Entries)
  if ($entries.Count -lt 1 -or $entries.Count -gt $script:MaximumEntries) {
    throw 'Candidate archive has an invalid entry count.'
  }
  $names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
  [long]$totalBytes = 0
  foreach ($entry in $entries) {
    Assert-SafeEntryName $entry.FullName
    if (-not $names.Add($entry.FullName.TrimEnd('/'))) {
      throw "Candidate archive contains a duplicate Windows path: $($entry.FullName)"
    }
    $unixMode = ($entry.ExternalAttributes -shr 16) -band 0xF000
    $dosAttributes = $entry.ExternalAttributes -band 0xFFFF
    if ($unixMode -eq 0xA000 -or ($dosAttributes -band [int][IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "Candidate archive contains a link or reparse point: $($entry.FullName)"
    }
    if (-not $entry.FullName.EndsWith('/', [StringComparison]::Ordinal)) {
      if ($entry.Name.Length -eq 0) { throw 'Candidate file entry has no leaf name.' }
      $totalBytes += $entry.Length
      if ($entry.Length -lt 0 -or $totalBytes -gt $script:MaximumUncompressedBytes) {
        throw 'Candidate archive exceeds the uncompressed byte ceiling.'
      }
    }
  }
  return $entries
}

function Expand-SafeArchive {
  param(
    [Parameter(Mandatory = $true)][string]$ArchivePath,
    [Parameter(Mandatory = $true)][string]$Destination
  )
  if (Test-Path -LiteralPath $Destination) {
    throw 'Candidate destination must not already exist.'
  }
  [void](New-Item -ItemType Directory -Path $Destination)
  $root = [IO.Path]::GetFullPath($Destination).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
  $archive = [IO.Compression.ZipFile]::OpenRead($ArchivePath)
  try {
    $entries = Assert-ArchiveEntries $archive
    foreach ($entry in $entries) {
      $relative = $entry.FullName.Replace([char]'/', [IO.Path]::DirectorySeparatorChar)
      $target = [IO.Path]::GetFullPath((Join-Path $Destination $relative))
      if (-not $target.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Candidate archive escaped the extraction root: $($entry.FullName)"
      }
      if ($entry.FullName.EndsWith('/', [StringComparison]::Ordinal)) {
        [void](New-Item -ItemType Directory -Path $target -Force)
        continue
      }
      [void](New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force)
      $source = $entry.Open()
      $destinationStream = [IO.File]::Open($target, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
      try { $source.CopyTo($destinationStream) } finally { $destinationStream.Dispose(); $source.Dispose() }
    }
  } catch {
    Remove-Item -LiteralPath $Destination -Recurse -Force -ErrorAction SilentlyContinue
    throw
  } finally {
    $archive.Dispose()
  }
  foreach ($item in Get-ChildItem -LiteralPath $Destination -Recurse -Force) {
    if (Test-ReparsePoint $item) { throw "Extracted candidate contains a reparse point: $($item.FullName)" }
  }
}

function Get-Artifact {
  param([Parameter(Mandatory = $true)][string]$TemporaryDirectory)
  $apiRoot = "https://api.github.com/repos/$Repository/actions/artifacts/$ArtifactId"
  $headers = @{
    Accept = 'application/vnd.github+json'
    Authorization = "Bearer $GitHubToken"
    'X-GitHub-Api-Version' = '2026-03-10'
    'User-Agent' = 'VeriSilo-windows-promotion-gate'
  }
  $metadata = Invoke-RestMethod -Method Get -Uri $apiRoot -Headers $headers
  if ([long]$metadata.id -ne $ArtifactId -or [bool]$metadata.expired -or
      [string]$metadata.archive_download_url -cne "$apiRoot/zip" -or
      [string]$metadata.digest -cne "sha256:$ArtifactSha256" -or
      [long]$metadata.workflow_run.repository_id -lt 1 -or
      [long]$metadata.workflow_run.repository_id -ne [long]$metadata.workflow_run.head_repository_id -or
      [string]$metadata.workflow_run.head_sha -cne $SourceRevision) {
    throw 'GitHub returned a mismatched, expired, cross-boundary, or wrong-digest artifact record.'
  }
  $expiresAt = [DateTimeOffset]::Parse(
    [string]$metadata.expires_at,
    [Globalization.CultureInfo]::InvariantCulture,
    [Globalization.DateTimeStyles]::AssumeUniversal
  )
  if ($expiresAt -le [DateTimeOffset]::UtcNow) { throw 'The Windows candidate Actions artifact is expired.' }
  $archivePath = Join-Path $TemporaryDirectory 'candidate.zip'
  Invoke-WebRequest -Method Get -Uri "$apiRoot/zip" -Headers $headers -OutFile $archivePath -MaximumRedirection 5
  $downloadedSha256 = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($downloadedSha256 -cne $ArtifactSha256) {
    throw 'Downloaded Windows candidate archive does not match the required artifact digest.'
  }
  return [pscustomobject]@{ ArchivePath = $archivePath; ExpiresAt = $expiresAt.ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ') }
}

function Write-Receipt {
  param([Parameter(Mandatory = $true)][string]$ExpiresAt)
  $receipt = [ordered]@{
    schema = 'urn:verisilo:actions-artifact-receipt:1'
    schemaVersion = 1
    repository = $Repository
    artifactId = $ArtifactId
    artifactSha256 = $ArtifactSha256
    sourceRevision = $SourceRevision
    expiresAt = $ExpiresAt
  }
  $parent = Split-Path -Parent ([IO.Path]::GetFullPath($MetadataPath))
  [void](New-Item -ItemType Directory -Path $parent -Force)
  [IO.File]::WriteAllText(
    [IO.Path]::GetFullPath($MetadataPath),
    (($receipt | ConvertTo-Json -Depth 4) + [Environment]::NewLine),
    [Text.UTF8Encoding]::new($false)
  )
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

function Assert-Rejected {
  param([scriptblock]$Action, [string]$Label)
  $rejected = $false
  try { & $Action } catch { $rejected = $true }
  if (-not $rejected) { throw "Candidate extraction self-test accepted $Label." }
}

function Invoke-SelfTest {
  $temporary = Join-Path ([IO.Path]::GetTempPath()) "verisilo-candidate-extract-$([Guid]::NewGuid().ToString('N'))"
  [void](New-Item -ItemType Directory -Path $temporary)
  try {
    $good = Join-Path $temporary 'good.zip'
    $traversal = Join-Path $temporary 'traversal.zip'
    $collision = Join-Path $temporary 'collision.zip'
    New-TestArchive $good @{ 'SHA256SUMS' = 'fixture'; 'nested/provenance.json' = '{}' }
    New-TestArchive $traversal @{ '../escape.exe' = 'fixture' }
    New-TestArchive $collision @{ 'A/file.exe' = 'a'; 'a/file.exe' = 'b' }
    Expand-SafeArchive $good (Join-Path $temporary 'good-output')
    Assert-Rejected { Expand-SafeArchive $traversal (Join-Path $temporary 'traversal-output') } 'path traversal'
    Assert-Rejected { Expand-SafeArchive $collision (Join-Path $temporary 'collision-output') } 'a case-insensitive path collision'
  } finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
  }
  Write-Host 'Windows candidate extraction self-test passed; no network or real candidate was used.'
}

if ($SelfTest) {
  Invoke-SelfTest
  exit 0
}

Assert-Inputs
$temporary = Join-Path ([IO.Path]::GetTempPath()) "verisilo-candidate-download-$([Guid]::NewGuid().ToString('N'))"
[void](New-Item -ItemType Directory -Path $temporary)
try {
  $artifact = Get-Artifact $temporary
  Expand-SafeArchive $artifact.ArchivePath ([IO.Path]::GetFullPath($DestinationDirectory))
  Write-Receipt $artifact.ExpiresAt
} finally {
  Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Host "Downloaded and safely extracted exact same-repository Windows candidate artifact $ArtifactId."
