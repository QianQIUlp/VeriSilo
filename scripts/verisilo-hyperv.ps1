[CmdletBinding(DefaultParameterSetName = 'Run')]
param(
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [string]$RequestPath,
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [string]$StateRoot,
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [string]$ApprovedImageRoot,
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [string]$ExpectedEnvironmentId,
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [ValidateSet('create', 'start', 'stop', 'pause', 'checkpoint', 'remove', 'health', 'logs')]
  [string]$ExpectedAction,
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [string]$ExpectedRequestNonce,
  [Parameter(ParameterSetName = 'SelfTest', Mandatory = $true)]
  [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-ExactFields {
  param([object]$Value, [string[]]$Allowed, [string[]]$Required)
  if ($null -eq $Value -or $Value -isnot [pscustomobject]) { throw 'JSON value must be an object.' }
  $names = @($Value.PSObject.Properties.Name)
  foreach ($name in $names) {
    if ($name -notin $Allowed) { throw "Unknown request field: $name" }
  }
  foreach ($name in $Required) {
    if ($name -notin $names) { throw "Missing request field: $name" }
  }
}

function ConvertFrom-StrictJson {
  param([Parameter(Mandatory = $true)][string]$Json)
  $command = Get-Command ConvertFrom-Json -CommandType Cmdlet
  if ($command.Parameters.ContainsKey('DateKind')) {
    return $Json | ConvertFrom-Json -DateKind String
  }
  return $Json | ConvertFrom-Json
}

function Assert-LeafName {
  param([string]$Value)
  if ([string]::IsNullOrWhiteSpace($Value) -or
      $Value -cnotmatch '^[a-z0-9][a-z0-9._-]{0,119}\.vhdx$' -or
      $Value.Contains('..') -or
      $Value -match '^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)' -or
      $Value -cne [IO.Path]::GetFileName($Value)) {
    throw 'manifestImageFile must be a strict lowercase VHDX leaf filename under ApprovedImageRoot.'
  }
}

function Assert-DescendantPath {
  param([string]$Root, [string]$Candidate)
  $rootPath = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
  $candidatePath = [IO.Path]::GetFullPath($Candidate)
  if (-not $candidatePath.StartsWith($rootPath, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Resolved path escapes its approved root.'
  }
  return $candidatePath
}

function Resolve-SafeDirectory {
  param([string]$Path, [string]$Label)
  $resolved = (Resolve-Path -LiteralPath $Path -ErrorAction Stop).Path
  $item = Get-Item -LiteralPath $resolved -Force
  if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "$Label must be a real directory, not a reparse point."
  }
  return $item.FullName
}

function Assert-RegularFile {
  param([string]$Path, [long]$MaximumBytes, [string]$Label)
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "$Label is missing." }
  $item = Get-Item -LiteralPath $Path -Force
  if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $item.Length -gt $MaximumBytes) {
    throw "$Label must be a bounded regular file, not a reparse point."
  }
  return $item
}

function Convert-CanonicalUuid {
  param([object]$Value, [string]$Label)
  if ($Value -isnot [string]) { throw "$Label must be a canonical lowercase UUID." }
  $parsed = [Guid]::Empty
  $raw = [string]$Value
  if (-not [Guid]::TryParseExact($raw, 'D', [ref]$parsed) -or
      $parsed -eq [Guid]::Empty -or $parsed.ToString('D') -cne $raw) {
    throw "$Label must be a canonical lowercase non-zero UUID."
  }
  return $parsed.ToString('D')
}

function Get-LockedFileSha256 {
  param([Parameter(Mandatory = $true)][IO.Stream]$Stream)
  $originalPosition = $Stream.Position
  $Stream.Position = 0
  $algorithm = [Security.Cryptography.SHA256]::Create()
  try {
    return [BitConverter]::ToString($algorithm.ComputeHash($Stream)).Replace('-', '').ToLowerInvariant()
  } finally {
    $algorithm.Dispose()
    $Stream.Position = $originalPosition
  }
}

function Read-StrictRequest {
  param([string]$Path)
  $file = Assert-RegularFile $Path 16384 'Request file'
  $request = ConvertFrom-StrictJson (Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8)
  $common = @('schemaVersion', 'action', 'environmentId', 'requestNonce', 'confirmDestroy')
  $manifest = @('manifestSchemaVersion', 'manifestImageFile', 'manifestImageSha256', 'manifestTrusted')
  if ($request.action -eq 'create') {
    Assert-ExactFields $request ($common + $manifest) ($common + $manifest)
  } else {
    Assert-ExactFields $request $common $common
  }
  if (($request.schemaVersion -isnot [int] -and $request.schemaVersion -isnot [long]) -or
      $request.schemaVersion -ne 1) { throw 'Unsupported request schemaVersion.' }
  if ($request.action -isnot [string] -or $request.environmentId -isnot [string]) {
    throw 'action and environmentId must be strings.'
  }
  if ($request.confirmDestroy -isnot [bool]) { throw 'confirmDestroy must be a Boolean.' }
  [void](Convert-CanonicalUuid $request.environmentId 'environmentId')
  [void](Convert-CanonicalUuid $request.requestNonce 'requestNonce')
  if ($request.action -notin @('create', 'start', 'stop', 'pause', 'checkpoint', 'remove', 'health', 'logs')) {
    throw 'Unknown Hyper-V action.'
  }
  return $request
}

function Read-SiloBinding {
  param([string]$EnvironmentRoot, [string]$EnvironmentId, [string]$VmName)
  $path = Assert-DescendantPath $EnvironmentRoot (Join-Path $EnvironmentRoot 'binding.json')
  $file = Assert-RegularFile $path 8192 'Persistent Silo binding'
  $binding = ConvertFrom-StrictJson (Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8)
  Assert-ExactFields $binding @('schemaVersion', 'environmentId', 'backend', 'providerKey') @('schemaVersion', 'environmentId', 'backend', 'providerKey')
  if (($binding.schemaVersion -isnot [int] -and $binding.schemaVersion -isnot [long]) -or
      $binding.environmentId -isnot [string] -or $binding.backend -isnot [string] -or
      $binding.providerKey -isnot [string] -or $binding.schemaVersion -ne 1 -or
      [string]$binding.environmentId -cne $EnvironmentId -or [string]$binding.backend -cne 'hyper-v' -or
      [string]$binding.providerKey -cne $VmName) {
    throw 'Persistent Silo binding does not match the requested Hyper-V VM.'
  }
}

function Write-BoundedJson {
  param([string]$Path, [object]$Value, [int]$MaximumBytes)
  $json = ($Value | ConvertTo-Json -Depth 8 -Compress) + [Environment]::NewLine
  $bytes = [Text.Encoding]::UTF8.GetBytes($json)
  if ($bytes.Length -gt $MaximumBytes) { throw 'Hyper-V state artifact exceeded its fixed byte limit.' }
  $temporary = "$Path.$([Guid]::NewGuid().ToString('N')).tmp"
  $stream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try {
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
  } finally {
    $stream.Dispose()
  }
  if (Test-Path -LiteralPath $Path) {
    [void](Assert-RegularFile $Path $MaximumBytes 'Existing Hyper-V state artifact')
    $backup = "$Path.$([Guid]::NewGuid().ToString('N')).bak"
    [IO.File]::Replace($temporary, $Path, $backup, $true)
    if (Test-Path -LiteralPath $backup -PathType Leaf) {
      [void](Assert-RegularFile $backup $MaximumBytes 'Replaced Hyper-V state artifact backup')
      Remove-Item -LiteralPath $backup -Force
    }
  } else {
    Move-Item -LiteralPath $temporary -Destination $Path
  }
}

function Convert-StrictUtcTimestamp {
  param([object]$Value, [string]$Label)
  if ($Value -isnot [string]) { throw "$Label must be an RFC3339 string." }
  try {
    $timestamp = [DateTimeOffset]::ParseExact(
      [string]$Value,
      'o',
      [Globalization.CultureInfo]::InvariantCulture,
      [Globalization.DateTimeStyles]::RoundtripKind
    )
  } catch {
    throw "$Label must be a strict round-trip timestamp."
  }
  if ($timestamp.Offset -ne [TimeSpan]::Zero -or $timestamp -gt [DateTimeOffset]::UtcNow.AddSeconds(30)) {
    throw "$Label must be UTC and not in the future."
  }
  return $timestamp
}

function Read-ProviderReceipt {
  param(
    [string]$Path,
    [string]$EnvironmentId,
    [string]$VmName,
    [string]$SwitchName,
    [string]$DiskPath
  )
  $file = Assert-RegularFile $Path 16384 'Hyper-V provider receipt'
  $receipt = ConvertFrom-StrictJson (Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8)
  $fields = @(
    'schemaVersion', 'environmentId', 'vmName', 'vmId', 'generation',
    'switchName', 'diskPath', 'baseImagePath', 'baseImageFile', 'baseImageSha256',
    'guestAgentVersion', 'guestAgentSha256', 'guestProfile', 'networkEvidence', 'createdAt'
  )
  Assert-ExactFields $receipt $fields $fields
  Assert-ExactFields $receipt.networkEvidence @('proxy', 'exit', 'proxyDns', 'guestResolver', 'browserReady') @('proxy', 'exit', 'proxyDns', 'guestResolver', 'browserReady')
  $parsedVmId = [Guid]::Empty
  if (($receipt.schemaVersion -isnot [int] -and $receipt.schemaVersion -isnot [long]) -or
      $receipt.schemaVersion -ne 1 -or $receipt.environmentId -isnot [string] -or
      $receipt.vmName -isnot [string] -or $receipt.vmId -isnot [string] -or
      ($receipt.generation -isnot [int] -and $receipt.generation -isnot [long]) -or
      $receipt.switchName -isnot [string] -or $receipt.diskPath -isnot [string] -or
      $receipt.baseImagePath -isnot [string] -or $receipt.baseImageFile -isnot [string] -or
      $receipt.baseImageSha256 -isnot [string] -or
      [string]$receipt.environmentId -cne $EnvironmentId -or [string]$receipt.vmName -cne $VmName -or
      -not [Guid]::TryParseExact([string]$receipt.vmId, 'D', [ref]$parsedVmId) -or
      $parsedVmId -eq [Guid]::Empty -or $parsedVmId.ToString('D') -cne [string]$receipt.vmId -or
      [int]$receipt.generation -ne 2 -or [string]$receipt.switchName -cne $SwitchName -or
      -not [IO.Path]::GetFullPath([string]$receipt.diskPath).Equals($DiskPath, [StringComparison]::OrdinalIgnoreCase) -or
      [string]$receipt.baseImageFile -cne [IO.Path]::GetFileName([string]$receipt.baseImagePath) -or
      [string]$receipt.baseImageSha256 -notmatch '^[a-f0-9]{64}$' -or
      $null -ne $receipt.guestAgentVersion -or $null -ne $receipt.guestAgentSha256 -or
      [string]$receipt.guestProfile -cne 'unavailable' -or
      @($receipt.networkEvidence.PSObject.Properties.Value | Where-Object { $_ -isnot [string] -or [string]$_ -cne 'unavailable' }).Count -ne 0) {
    throw 'Hyper-V provider receipt failed its VM, image, guest-agent, profile, or network binding.'
  }
  Assert-LeafName ([string]$receipt.baseImageFile)
  [void](Convert-StrictUtcTimestamp $receipt.createdAt 'Hyper-V receipt createdAt')
  return $receipt
}

function New-CreateJournal {
  param(
    [string]$EnvironmentId,
    [string]$VmName,
    [string]$SwitchName,
    [string]$DiskPath,
    [string]$BaseImagePath,
    [string]$BaseImageFile,
    [string]$BaseImageSha256
  )
  $now = [DateTime]::UtcNow.ToString('o')
  return [ordered]@{
    schemaVersion = 1
    environmentId = $EnvironmentId
    vmName = $VmName
    switchName = $SwitchName
    diskPath = [IO.Path]::GetFullPath($DiskPath)
    baseImagePath = [IO.Path]::GetFullPath($BaseImagePath)
    baseImageFile = $BaseImageFile
    baseImageSha256 = $BaseImageSha256.ToLowerInvariant()
    vmId = $null
    switchOwned = $false
    diskOwned = $false
    vmOwned = $false
    phase = 'prepared'
    createdAt = $now
    updatedAt = $now
  }
}

function Read-CreateJournal {
  param(
    [string]$Path,
    [string]$EnvironmentId,
    [string]$VmName,
    [string]$SwitchName,
    [string]$DiskPath
  )
  $file = Assert-RegularFile $Path 16384 'Hyper-V create rollback journal'
  $journal = ConvertFrom-StrictJson (Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8)
  $fields = @(
    'schemaVersion', 'environmentId', 'vmName', 'switchName', 'diskPath',
    'baseImagePath', 'baseImageFile', 'baseImageSha256', 'vmId',
    'switchOwned', 'diskOwned', 'vmOwned', 'phase', 'createdAt', 'updatedAt'
  )
  Assert-ExactFields $journal $fields $fields
  $validPhases = @(
    'prepared', 'switch_pending', 'switch_ready', 'disk_pending', 'disk_ready',
    'vm_pending', 'vm_ready', 'configuring', 'configured', 'cleanup_pending',
    'cleanup_complete'
  )
  $parsedVmId = [Guid]::Empty
  $vmIdValid = $null -eq $journal.vmId -or
    ($journal.vmId -is [string] -and
      [Guid]::TryParseExact([string]$journal.vmId, 'D', [ref]$parsedVmId) -and
      $parsedVmId -ne [Guid]::Empty -and
      $parsedVmId.ToString('D') -ceq [string]$journal.vmId)
  if (($journal.schemaVersion -isnot [int] -and $journal.schemaVersion -isnot [long]) -or
      $journal.schemaVersion -ne 1 -or $journal.environmentId -isnot [string] -or
      $journal.vmName -isnot [string] -or $journal.switchName -isnot [string] -or
      $journal.diskPath -isnot [string] -or $journal.baseImagePath -isnot [string] -or
      $journal.baseImageFile -isnot [string] -or $journal.baseImageSha256 -isnot [string] -or
      $journal.switchOwned -isnot [bool] -or $journal.diskOwned -isnot [bool] -or
      $journal.vmOwned -isnot [bool] -or $journal.phase -isnot [string] -or
      [string]$journal.environmentId -cne $EnvironmentId -or
      [string]$journal.vmName -cne $VmName -or [string]$journal.switchName -cne $SwitchName -or
      -not [IO.Path]::GetFullPath([string]$journal.diskPath).Equals($DiskPath, [StringComparison]::OrdinalIgnoreCase) -or
      [string]$journal.baseImageFile -cne [IO.Path]::GetFileName([string]$journal.baseImagePath) -or
      [string]$journal.baseImageSha256 -cnotmatch '^[a-f0-9]{64}$' -or
      [string]$journal.phase -cnotin $validPhases -or -not $vmIdValid) {
    throw 'Hyper-V create rollback journal failed its exact resource and phase binding.'
  }
  Assert-LeafName ([string]$journal.baseImageFile)
  [void](Convert-StrictUtcTimestamp $journal.createdAt 'Hyper-V create journal createdAt')
  [void](Convert-StrictUtcTimestamp $journal.updatedAt 'Hyper-V create journal updatedAt')
  return $journal
}

function Write-CreateJournalPhase {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)]$Journal,
    [Parameter(Mandatory = $true)]
    [ValidateSet(
      'prepared', 'switch_pending', 'switch_ready', 'disk_pending', 'disk_ready',
      'vm_pending', 'vm_ready', 'configuring', 'configured', 'cleanup_pending',
      'cleanup_complete'
    )]
    [string]$Phase,
    [string]$VmId
  )
  $Journal.phase = $Phase
  if ($PSBoundParameters.ContainsKey('VmId')) {
    $Journal.vmId = $VmId
  }
  $Journal.updatedAt = [DateTime]::UtcNow.ToString('o')
  Write-BoundedJson $Path $Journal 16384
}

if ($SelfTest) {
  Assert-LeafName 'windows-11-base.vhdx'
  $rejected = $false
  try { Assert-LeafName '..\outside.vhdx' } catch { $rejected = $true }
  if (-not $rejected) { throw 'Self-test did not reject traversal.' }
  $rejected = $false
  try { Assert-ExactFields ([pscustomobject]@{ action = 'start'; command = 'Remove-VM' }) @('action') @('action') } catch { $rejected = $true }
  if (-not $rejected) { throw 'Self-test did not reject an unknown field.' }
  $id = [Guid]::NewGuid().ToString('D')
  if ("VeriSilo-$id".Length -gt 64) { throw 'Full UUID resource name exceeds the supported bound.' }
  $selfTestId = [Guid]::NewGuid().ToString('N').Substring(0, 12)
  $selfTestRoot = Join-Path ([IO.Path]::GetTempPath()) "vhv-$selfTestId"
  [void](New-Item -ItemType Directory -Path $selfTestRoot)
  try {
    $selfTestRequestNonce = [Guid]::NewGuid().ToString('D')
    $selfTestRequestPath = Join-Path $selfTestRoot "$selfTestRequestNonce.request.json"
    $selfTestRequest = [ordered]@{
      schemaVersion = 1
      action = 'health'
      environmentId = $id
      requestNonce = $selfTestRequestNonce
      confirmDestroy = $false
    }
    Write-BoundedJson $selfTestRequestPath $selfTestRequest 16384
    $requestRoundTrip = Read-StrictRequest $selfTestRequestPath
    if ([string]$requestRoundTrip.environmentId -cne $id -or
        [string]$requestRoundTrip.action -cne 'health' -or
        [string]$requestRoundTrip.requestNonce -cne $selfTestRequestNonce) {
      throw 'Strict request environment, action, and nonce binding did not round-trip.'
    }
    $selfTestDisk = Join-Path $selfTestRoot 'system-diff.vhdx'
    $selfTestImage = Join-Path $selfTestRoot 'windows-11-base.vhdx'
    $selfTestJournalPath = Join-Path $selfTestRoot 'hyperv-create-journal.json'
    $journal = New-CreateJournal $id "VeriSilo-$id" "VeriSilo-$id" $selfTestDisk $selfTestImage 'windows-11-base.vhdx' ('a' * 64)
    Write-BoundedJson $selfTestJournalPath $journal 16384
    $journal = Read-CreateJournal $selfTestJournalPath $id "VeriSilo-$id" "VeriSilo-$id" $selfTestDisk
    foreach ($phase in @(
      'switch_pending', 'switch_ready', 'disk_pending', 'disk_ready', 'vm_pending',
      'vm_ready', 'configuring', 'configured', 'cleanup_pending', 'cleanup_complete'
    )) {
      if ($phase -eq 'switch_pending') { $journal.switchOwned = $true }
      if ($phase -eq 'disk_pending') { $journal.diskOwned = $true }
      if ($phase -eq 'vm_pending') { $journal.vmOwned = $true }
      if ($phase -eq 'vm_ready') {
        Write-CreateJournalPhase $selfTestJournalPath $journal $phase ([Guid]::NewGuid().ToString('D'))
      } else {
        Write-CreateJournalPhase $selfTestJournalPath $journal $phase
      }
      $journal = Read-CreateJournal $selfTestJournalPath $id "VeriSilo-$id" "VeriSilo-$id" $selfTestDisk
      if ([string]$journal.phase -cne $phase) { throw "Journal phase did not persist: $phase" }
    }
    if (-not $journal.switchOwned -or -not $journal.diskOwned -or -not $journal.vmOwned) {
      throw 'Journal ownership flags did not persist across phase replacements.'
    }
    $journal.vmId = $null
    $cleanupResponse = [ordered]@{
      action = 'remove'
      vmId = $null
      cleanupState = 'rolled_back_from_journal'
    }
    $cleanupRoundTrip = ConvertFrom-StrictJson ($cleanupResponse | ConvertTo-Json -Compress)
    if ([string]$cleanupRoundTrip.action -cne 'remove' -or $null -ne $cleanupRoundTrip.vmId -or
        [string]$cleanupRoundTrip.cleanupState -cne 'rolled_back_from_journal') {
      throw 'Remove-only nullable VM identity cleanup response did not round-trip.'
    }
    $journal.phase = 'untrusted_phase'
    Write-BoundedJson $selfTestJournalPath $journal 16384
    $rejected = $false
    try { [void](Read-CreateJournal $selfTestJournalPath $id "VeriSilo-$id" "VeriSilo-$id" $selfTestDisk) } catch { $rejected = $true }
    if (-not $rejected) { throw 'Self-test accepted an unknown rollback-journal phase.' }
  } finally {
    Remove-Item -LiteralPath $selfTestRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
  Write-Host 'Hyper-V request and persistent rollback-journal self-test passed.'
  exit 0
}

$environmentId = Convert-CanonicalUuid $ExpectedEnvironmentId 'ExpectedEnvironmentId'
$requestNonce = Convert-CanonicalUuid $ExpectedRequestNonce 'ExpectedRequestNonce'
if ([string]$ExpectedAction -cnotin @('create', 'start', 'stop', 'pause', 'checkpoint', 'remove', 'health', 'logs')) {
  throw 'ExpectedAction must use the exact lowercase Hyper-V action spelling.'
}
$stateRootPath = Resolve-SafeDirectory $StateRoot 'StateRoot'
$environmentRoot = Resolve-SafeDirectory (Assert-DescendantPath $stateRootPath (Join-Path $stateRootPath $environmentId)) 'Hyper-V environment root'
$requestPathResolved = Assert-DescendantPath $stateRootPath $RequestPath
$requestParent = [IO.Path]::GetFullPath([IO.Path]::GetDirectoryName($requestPathResolved))
if (-not $requestParent.Equals($environmentRoot, [StringComparison]::OrdinalIgnoreCase) -or
    [IO.Path]::GetFileName($requestPathResolved) -cne "$requestNonce.request.json") {
  throw 'RequestPath must be the nonce-named direct child of the exact bound environment directory.'
}
$request = Read-StrictRequest $requestPathResolved
if ([string]$request.environmentId -cne $environmentId -or
    [string]$request.action -cne [string]$ExpectedAction -or
    [string]$request.requestNonce -cne $requestNonce) {
  throw 'Request file did not match the expected environmentId, action, and requestNonce.'
}
$imageRootPath = if ($request.action -eq 'create' -or (Test-Path -LiteralPath $ApprovedImageRoot)) {
  Resolve-SafeDirectory $ApprovedImageRoot 'ApprovedImageRoot'
} else {
  [IO.Path]::GetFullPath($ApprovedImageRoot)
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  throw 'Hyper-V actions require an elevated administrator token.'
}
if ($null -eq (Get-Command Get-VM -ErrorAction SilentlyContinue)) {
  throw 'Hyper-V PowerShell commands are unavailable on this SKU or feature state.'
}

$vmName = "VeriSilo-$environmentId"
$switchName = $vmName
$bindingNote = "VeriSilo:v1:$environmentId"
$diskPath = Assert-DescendantPath $environmentRoot (Join-Path $environmentRoot 'system-diff.vhdx')
$logPath = Assert-DescendantPath $environmentRoot (Join-Path $environmentRoot 'hyperv-status.json')
$providerReceiptPath = Assert-DescendantPath $environmentRoot (Join-Path $environmentRoot 'hyperv-receipt.json')
$createJournalPath = Assert-DescendantPath $environmentRoot (Join-Path $environmentRoot 'hyperv-create-journal.json')
Read-SiloBinding $environmentRoot $environmentId $vmName

function Assert-BoundSwitch {
  param([bool]$Required)
  $switch = Get-VMSwitch -Name $switchName -ErrorAction SilentlyContinue
  if ($null -eq $switch) {
    if ($Required) { throw 'Bound internal switch is missing.' }
    return $null
  }
  if ([string]$switch.SwitchType -cne 'Internal' -or [string]$switch.Notes -cne $bindingNote) {
    throw 'A same-name virtual switch exists without the exact Silo binding.'
  }
  return $switch
}

function Assert-BoundDisk {
  param([string]$ExpectedParent, [bool]$Required)
  if (-not (Test-Path -LiteralPath $diskPath -PathType Leaf)) {
    if ($Required) { throw 'Bound differencing disk is missing.' }
    return $null
  }
  $item = Assert-RegularFile $diskPath ([long]::MaxValue) 'Bound differencing disk'
  $vhd = Get-VHD -Path $item.FullName -ErrorAction Stop
  if ([string]$vhd.VhdType -cne 'Differencing' -or
      -not [IO.Path]::GetFullPath([string]$vhd.ParentPath).Equals([IO.Path]::GetFullPath($ExpectedParent), [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Existing Silo disk is not a differencing child of the signed-manifest image.'
  }
  return $vhd
}

function Assert-ReceiptImageBinding {
  param([object]$Receipt)
  Assert-LeafName ([string]$Receipt.baseImageFile)
  $expectedPath = Assert-DescendantPath $imageRootPath (Join-Path $imageRootPath ([string]$Receipt.baseImageFile))
  if (-not [IO.Path]::GetFullPath([string]$Receipt.baseImagePath).Equals($expectedPath, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Hyper-V receipt base-image path does not match its exact approved leaf.'
  }
  if (Test-Path -LiteralPath $expectedPath) {
    $image = Assert-RegularFile $expectedPath ([long]::MaxValue) 'Receipt-bound base image'
    $actualHash = (Get-FileHash -LiteralPath $image.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -cne [string]$Receipt.baseImageSha256) {
      throw 'Receipt-bound base image no longer matches its exact SHA-256.'
    }
  }
}

function Assert-BoundVm {
  param([bool]$Required, [string]$ExpectedVmId = '')
  $vm = Get-VM -Name $vmName -ErrorAction SilentlyContinue
  if ($null -eq $vm) {
    if ($Required) { throw 'Bound Hyper-V VM is missing.' }
    return $null
  }
  $actualVmId = ([Guid]$vm.Id).ToString('D')
  if ([string]$vm.Notes -cne $bindingNote -or [int]$vm.Generation -ne 2 -or
      (-not [string]::IsNullOrEmpty($ExpectedVmId) -and $actualVmId -cne $ExpectedVmId)) {
    throw 'A same-name VM exists without the exact Silo binding and generation.'
  }
  $drives = @(Get-VMHardDiskDrive -VMName $vmName -ErrorAction Stop)
  if ($drives.Count -ne 1 -or
      -not [IO.Path]::GetFullPath([string]$drives[0].Path).Equals([IO.Path]::GetFullPath($diskPath), [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Bound VM disk attachment differs from the deterministic Silo disk.'
  }
  $adapters = @(Get-VMNetworkAdapter -VMName $vmName -ErrorAction Stop)
  if ($adapters.Count -ne 1 -or [string]$adapters[0].SwitchName -cne $switchName -or
      [string]$adapters[0].DhcpGuard -cne 'On' -or [string]$adapters[0].RouterGuard -cne 'On' -or
      [string]$adapters[0].MacAddressSpoofing -cne 'Off') {
    throw 'Bound VM must have exactly one adapter on its private internal switch.'
  }
  if (@(Get-VMDvdDrive -VMName $vmName -ErrorAction Stop).Count -ne 0) {
    throw 'Bound VM unexpectedly exposes a virtual DVD device.'
  }
  $guestService = @(Get-VMIntegrationService -VMName $vmName -Name 'Guest Service Interface' -ErrorAction Stop)
  if ($guestService.Count -ne 1 -or $guestService[0].Enabled) {
    throw 'Hyper-V Guest Service Interface must exist and remain disabled.'
  }
  if ($null -ne (Get-Command Get-VMAssignableDevice -ErrorAction SilentlyContinue) -and
      @(Get-VMAssignableDevice -VMName $vmName -ErrorAction Stop).Count -ne 0) {
    throw 'Discrete device assignment is forbidden for a VeriSilo VM.'
  }
  if ($null -ne (Get-Command Get-VMGpuPartitionAdapter -ErrorAction SilentlyContinue) -and
      @(Get-VMGpuPartitionAdapter -VMName $vmName -ErrorAction SilentlyContinue).Count -ne 0) {
    throw 'GPU partition assignment is forbidden for a VeriSilo VM.'
  }
  if ($null -ne (Get-Command Get-VMFibreChannelHba -ErrorAction SilentlyContinue) -and
      @(Get-VMFibreChannelHba -VMName $vmName -ErrorAction SilentlyContinue).Count -ne 0) {
    throw 'Virtual Fibre Channel host storage mapping is forbidden for a VeriSilo VM.'
  }
  if ($null -ne (Get-Command Get-VMComPort -ErrorAction SilentlyContinue) -and
      @(Get-VMComPort -VMName $vmName -ErrorAction Stop | Where-Object { -not [string]::IsNullOrEmpty([string]$_.Path) }).Count -ne 0) {
    throw 'Named-pipe COM host mappings are forbidden for a VeriSilo VM.'
  }
  $firmware = Get-VMFirmware -VMName $vmName -ErrorAction Stop
  $memory = Get-VMMemory -VMName $vmName -ErrorAction Stop
  if ([string]$firmware.SecureBoot -cne 'On' -or $memory.DynamicMemoryEnabled -or
      [long]$memory.Startup -ne 4GB -or $vm.AutomaticCheckpointsEnabled -or
      [string]$vm.CheckpointType -cne 'Production') {
    throw 'Bound VM security, memory, or checkpoint policy drifted from the V0.8 baseline.'
  }
  return $vm
}

function Initialize-BoundVm {
  $vm = Get-VM -Name $vmName -ErrorAction Stop
  if ([int]$vm.Generation -ne 2 -or
      (-not [string]::IsNullOrEmpty([string]$vm.Notes) -and [string]$vm.Notes -cne $bindingNote)) {
    throw 'Existing VM cannot be adopted into the exact Silo binding.'
  }
  [void](Assert-DescendantPath $environmentRoot ([string]$vm.Path))
  $drives = @(Get-VMHardDiskDrive -VMName $vmName -ErrorAction Stop)
  $adapters = @(Get-VMNetworkAdapter -VMName $vmName -ErrorAction Stop)
  if ($drives.Count -ne 1 -or
      -not [IO.Path]::GetFullPath([string]$drives[0].Path).Equals([IO.Path]::GetFullPath($diskPath), [StringComparison]::OrdinalIgnoreCase) -or
      $adapters.Count -ne 1 -or [string]$adapters[0].SwitchName -cne $switchName) {
    throw 'Existing VM attachments cannot be adopted into the Silo binding.'
  }
  if ([string]$vm.State -eq 'Off') {
    Set-VM -Name $vmName -Notes $bindingNote -AutomaticCheckpointsEnabled $false -CheckpointType Production -SnapshotFileLocation $environmentRoot -SmartPagingFilePath $environmentRoot
    Set-VMMemory -VMName $vmName -DynamicMemoryEnabled $false -StartupBytes 4GB
    Set-VMFirmware -VMName $vmName -EnableSecureBoot On -SecureBootTemplate 'MicrosoftWindows'
    Get-VMDvdDrive -VMName $vmName -ErrorAction Stop | Remove-VMDvdDrive
    $guestService = @(Get-VMIntegrationService -VMName $vmName -Name 'Guest Service Interface' -ErrorAction Stop)
    if ($guestService.Count -ne 1) { throw 'Hyper-V Guest Service Interface is unavailable.' }
    if ($guestService[0].Enabled) {
      Disable-VMIntegrationService -VMName $vmName -Name 'Guest Service Interface'
    }
    Set-VMNetworkAdapter -VMName $vmName -DhcpGuard On -RouterGuard On -MacAddressSpoofing Off
  } elseif ([string]$vm.Notes -cne $bindingNote) {
    throw 'A partially created VM can be recovered only while it is powered off.'
  }
}

function Wait-VmState {
  param([string[]]$AllowedStates, [int]$TimeoutSeconds)
  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  do {
    $vm = Get-VM -Name $vmName -ErrorAction Stop
    if ([string]$vm.State -in $AllowedStates) { return $vm }
    Start-Sleep -Milliseconds 250
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "VM did not reach a bounded expected state: $($AllowedStates -join ', ')."
}

function Assert-NoOtherRunningSilo {
  $other = @(Get-VM -ErrorAction Stop | Where-Object {
    $_.Name -match '^VeriSilo-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' -and
    $_.Name -cne $vmName -and [string]$_.State -eq 'Running'
  })
  if ($other.Count -ne 0) { throw 'Concurrent multi-Silo Hyper-V execution is gated in V0.8.' }
}

function Invoke-CreateJournalRollback {
  param([Parameter(Mandatory = $true)]$Journal)

  $vm = Get-VM -Name $vmName -ErrorAction SilentlyContinue
  $switch = Get-VMSwitch -Name $switchName -ErrorAction SilentlyContinue
  $diskExists = Test-Path -LiteralPath $diskPath -PathType Leaf
  if ([string]$Journal.phase -ceq 'cleanup_complete') {
    if (($Journal.vmOwned -and $null -ne $vm) -or
        ($Journal.diskOwned -and $diskExists) -or
        ($Journal.switchOwned -and $null -ne $switch)) {
      throw 'A journal-owned resource reappeared after rollback completed; refusing to delete an unjournaled replacement.'
    }
    return $Journal
  }

  $ownedVmId = if ($null -ne $Journal.vmId) { [string]$Journal.vmId } else { $null }
  if ($Journal.vmOwned -and $null -ne $vm) {
    $actualVmId = ([Guid]$vm.Id).ToString('D')
    if (($null -ne $ownedVmId -and $actualVmId -cne $ownedVmId) -or
        [int]$vm.Generation -ne 2 -or
        ([string]$vm.Notes -notin @('', $bindingNote))) {
      throw 'Journal-owned partial VM failed its exact identity, generation, or binding check.'
    }
    [void](Assert-DescendantPath $environmentRoot ([string]$vm.Path))
    $drives = @(Get-VMHardDiskDrive -VMName $vmName -ErrorAction Stop)
    if ($drives.Count -gt 1 -or
        ($drives.Count -eq 1 -and
          -not [IO.Path]::GetFullPath([string]$drives[0].Path).Equals($diskPath, [StringComparison]::OrdinalIgnoreCase))) {
      throw 'Journal-owned partial VM has an unexpected disk attachment.'
    }
    $adapters = @(Get-VMNetworkAdapter -VMName $vmName -ErrorAction Stop)
    if ($adapters.Count -gt 1 -or
        ($adapters.Count -eq 1 -and [string]$adapters[0].SwitchName -cne $switchName)) {
      throw 'Journal-owned partial VM has an unexpected network attachment.'
    }
    if ([string]$vm.State -notin @('Off', 'Saved')) {
      throw 'Journal rollback refuses to force-power off a partial VM.'
    }
    $ownedVmId = $actualVmId
    if ($null -eq $Journal.vmId) {
      Write-CreateJournalPhase $createJournalPath $Journal 'cleanup_pending' $ownedVmId
    }
  } elseif ($Journal.vmOwned -and $null -ne $ownedVmId) {
    $renamedVm = Get-VM -Id ([Guid]$ownedVmId) -ErrorAction SilentlyContinue
    if ($null -ne $renamedVm) {
      throw 'Journal-owned VM identity exists under an unexpected name; refusing cleanup.'
    }
  }

  if ($Journal.switchOwned) {
    $foreignSwitchAdapters = @(Get-VMNetworkAdapter -All -ErrorAction Stop | Where-Object {
      [string]$_.SwitchName -ceq $switchName -and
      ($null -eq $ownedVmId -or ([string]$_.VMId).ToLowerInvariant() -cne $ownedVmId)
    })
    if ($foreignSwitchAdapters.Count -ne 0) {
      throw 'The journal-owned switch has a foreign active VM adapter; rollback will not mutate any resource.'
    }
  }
  if ($Journal.diskOwned -and $diskExists) {
    $foreignDiskUsers = @(Get-VM -ErrorAction Stop | Get-VMHardDiskDrive -ErrorAction Stop | Where-Object {
      [IO.Path]::GetFullPath([string]$_.Path).Equals($diskPath, [StringComparison]::OrdinalIgnoreCase) -and
      ($null -eq $ownedVmId -or ([string]$_.VMId).ToLowerInvariant() -cne $ownedVmId)
    })
    if ($foreignDiskUsers.Count -ne 0) {
      throw 'The journal-owned disk is attached to a foreign VM; rollback will not mutate any resource.'
    }
    [void](Assert-RegularFile $diskPath ([long]::MaxValue) 'Journal-owned differencing disk')
    $journalDisk = Get-VHD -Path $diskPath -ErrorAction Stop
    if ([string]$journalDisk.VhdType -cne 'Differencing' -or
        -not [IO.Path]::GetFullPath([string]$journalDisk.ParentPath).Equals(
          [IO.Path]::GetFullPath([string]$Journal.baseImagePath),
          [StringComparison]::OrdinalIgnoreCase
        )) {
      throw 'Journal-owned disk is not the expected differencing child of the recorded base image.'
    }
  }
  if ($Journal.switchOwned -and $null -ne $switch -and
      ([string]$switch.SwitchType -cne 'Internal' -or [string]$switch.Notes -cne $bindingNote)) {
    throw 'Journal-owned switch failed its exact internal-switch binding.'
  }

  Write-CreateJournalPhase $createJournalPath $Journal 'cleanup_pending'
  if ($Journal.vmOwned -and $null -ne $vm) {
    Remove-VM -VM $vm -Force
    if ($null -ne (Get-VM -Id ([Guid]$ownedVmId) -ErrorAction SilentlyContinue)) {
      throw 'The exact journal-owned VM still exists after rollback removal.'
    }
  }
  if ($Journal.diskOwned -and (Test-Path -LiteralPath $diskPath -PathType Leaf)) {
    Write-CreateJournalPhase $createJournalPath $Journal 'cleanup_pending'
    Remove-Item -LiteralPath $diskPath -Force
  }
  if ($Journal.switchOwned) {
    $switch = Get-VMSwitch -Name $switchName -ErrorAction SilentlyContinue
    if ($null -ne $switch) {
      if (@(Get-VMNetworkAdapter -All -ErrorAction Stop | Where-Object { [string]$_.SwitchName -ceq $switchName }).Count -ne 0) {
        throw 'The journal-owned switch still has active VM adapters after VM rollback.'
      }
      Write-CreateJournalPhase $createJournalPath $Journal 'cleanup_pending'
      Remove-VMSwitch -VMSwitch $switch -Force
    }
  }
  Write-CreateJournalPhase $createJournalPath $Journal 'cleanup_complete'
  return $Journal
}

$baseImageLease = $null
try {
  $providerReceipt = $null
  $createJournal = $null
  $cleanupState = $null
  if ($request.action -ne 'create') {
    if (Test-Path -LiteralPath $providerReceiptPath -PathType Leaf) {
      $providerReceipt = Read-ProviderReceipt $providerReceiptPath $environmentId $vmName $switchName $diskPath
      [void](Assert-DescendantPath $imageRootPath ([string]$providerReceipt.baseImagePath))
      Assert-ReceiptImageBinding $providerReceipt
    } elseif ($request.action -eq 'remove' -and (Test-Path -LiteralPath $createJournalPath -PathType Leaf)) {
      $createJournal = Read-CreateJournal $createJournalPath $environmentId $vmName $switchName $diskPath
      [void](Assert-DescendantPath $imageRootPath ([string]$createJournal.baseImagePath))
    } else {
      throw 'Hyper-V provider receipt is missing and no rollback journal authorizes this action.'
    }
  }

  switch ($request.action) {
  'create' {
    if (($request.manifestSchemaVersion -isnot [int] -and $request.manifestSchemaVersion -isnot [long]) -or
        $request.manifestSchemaVersion -ne 1 -or $request.manifestTrusted -isnot [bool] -or
        $request.manifestTrusted -ne $true -or $request.manifestImageFile -isnot [string] -or
        $request.manifestImageSha256 -isnot [string]) {
      throw 'The signed base-image manifest boundary was not verified.'
    }
    Assert-LeafName ([string]$request.manifestImageFile)
    if ([string]$request.manifestImageSha256 -cnotmatch '^[a-f0-9]{64}$') {
      throw 'Invalid signed-manifest image SHA-256.'
    }
    $imagePath = Assert-DescendantPath $imageRootPath (Join-Path $imageRootPath ([string]$request.manifestImageFile))
    $image = Assert-RegularFile $imagePath ([long]::MaxValue) 'Signed-manifest base image'
    $baseImageLease = [IO.File]::Open($image.FullName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $actualHash = Get-LockedFileSha256 $baseImageLease
    if ($actualHash -cne ([string]$request.manifestImageSha256).ToLowerInvariant()) {
      throw 'Base image hash does not match the signed build manifest.'
    }

    if (Test-Path -LiteralPath $createJournalPath -PathType Leaf) {
      $createJournal = Read-CreateJournal $createJournalPath $environmentId $vmName $switchName $diskPath
      if (-not [IO.Path]::GetFullPath([string]$createJournal.baseImagePath).Equals($image.FullName, [StringComparison]::OrdinalIgnoreCase) -or
          [string]$createJournal.baseImageFile -cne [string]$request.manifestImageFile -or
          [string]$createJournal.baseImageSha256 -cne ([string]$request.manifestImageSha256).ToLowerInvariant()) {
        throw 'Existing rollback journal is bound to a different signed base image.'
      }
      if ([string]$createJournal.phase -ceq 'cleanup_complete') {
        if (($createJournal.vmOwned -and $null -ne (Get-VM -Name $vmName -ErrorAction SilentlyContinue)) -or
            ($createJournal.diskOwned -and (Test-Path -LiteralPath $diskPath -PathType Leaf)) -or
            ($createJournal.switchOwned -and $null -ne (Get-VMSwitch -Name $switchName -ErrorAction SilentlyContinue))) {
          throw 'Completed rollback journal still has an owned resource; refusing a new create.'
        }
        $createJournal = New-CreateJournal $environmentId $vmName $switchName $diskPath $image.FullName ([string]$request.manifestImageFile) ([string]$request.manifestImageSha256)
        Write-BoundedJson $createJournalPath $createJournal 16384
      }
    } else {
      $createJournal = New-CreateJournal $environmentId $vmName $switchName $diskPath $image.FullName ([string]$request.manifestImageFile) ([string]$request.manifestImageSha256)
      Write-BoundedJson $createJournalPath $createJournal 16384
    }

    $switch = Assert-BoundSwitch $false
    if ($null -eq $switch) {
      $createJournal.switchOwned = $true
      Write-CreateJournalPhase $createJournalPath $createJournal 'switch_pending'
      $switch = New-VMSwitch -Name $switchName -SwitchType Internal -Notes $bindingNote
      Write-CreateJournalPhase $createJournalPath $createJournal 'switch_ready'
    }
    $disk = Assert-BoundDisk $image.FullName $false
    if ($null -eq $disk) {
      $createJournal.diskOwned = $true
      Write-CreateJournalPhase $createJournalPath $createJournal 'disk_pending'
      $disk = New-VHD -Path $diskPath -ParentPath $image.FullName -Differencing
      Write-CreateJournalPhase $createJournalPath $createJournal 'disk_ready'
    }
    $vm = Get-VM -Name $vmName -ErrorAction SilentlyContinue
    if ($null -eq $vm) {
      $createJournal.vmOwned = $true
      Write-CreateJournalPhase $createJournalPath $createJournal 'vm_pending'
      $vm = New-VM -Name $vmName -Path $environmentRoot -Generation 2 -MemoryStartupBytes 4GB -VHDPath $diskPath -SwitchName $switchName
      Write-CreateJournalPhase $createJournalPath $createJournal 'vm_ready' (([Guid]$vm.Id).ToString('D'))
    } elseif ($createJournal.vmOwned) {
      $existingVmId = ([Guid]$vm.Id).ToString('D')
      if ($null -ne $createJournal.vmId -and [string]$createJournal.vmId -cne $existingVmId) {
        throw 'Existing partial VM does not match the rollback journal VM identity.'
      }
      Write-CreateJournalPhase $createJournalPath $createJournal 'vm_ready' $existingVmId
    }
    Write-CreateJournalPhase $createJournalPath $createJournal 'configuring'
    Initialize-BoundVm
    [void](Assert-BoundSwitch $true)
    [void](Assert-BoundDisk $image.FullName $true)
    $vm = Assert-BoundVm $true
    Write-CreateJournalPhase $createJournalPath $createJournal 'configured' (([Guid]$vm.Id).ToString('D'))
    $providerReceipt = [ordered]@{
      schemaVersion = 1
      environmentId = $environmentId
      vmName = $vmName
      vmId = ([Guid]$vm.Id).ToString('D')
      generation = 2
      switchName = $switchName
      diskPath = $diskPath
      baseImagePath = $image.FullName
      baseImageFile = [string]$request.manifestImageFile
      baseImageSha256 = ([string]$request.manifestImageSha256).ToLowerInvariant()
      guestAgentVersion = $null
      guestAgentSha256 = $null
      guestProfile = 'unavailable'
      networkEvidence = [ordered]@{
        proxy = 'unavailable'
        exit = 'unavailable'
        proxyDns = 'unavailable'
        guestResolver = 'unavailable'
        browserReady = 'unavailable'
      }
      createdAt = [DateTime]::UtcNow.ToString('o')
    }
    Write-BoundedJson $providerReceiptPath $providerReceipt 16384
    [void](Assert-RegularFile $createJournalPath 16384 'Completed Hyper-V create rollback journal')
    Remove-Item -LiteralPath $createJournalPath -Force
  }
  'start' {
    [void](Assert-BoundDisk ([string]$providerReceipt.baseImagePath) $true)
    $vm = Assert-BoundVm $true ([string]$providerReceipt.vmId)
    Assert-NoOtherRunningSilo
    if ([string]$vm.State -notin @('Running')) {
      if ([string]$vm.State -notin @('Off', 'Saved')) { throw "VM cannot start safely from state $($vm.State)." }
      Start-VM -Name $vmName
      [void](Wait-VmState @('Running') 20)
    }
  }
  'stop' {
    $vm = Assert-BoundVm $true ([string]$providerReceipt.vmId)
    if ([string]$vm.State -eq 'Running') {
      Stop-VM -Name $vmName -Shutdown
      [void](Wait-VmState @('Off') 20)
    } elseif ([string]$vm.State -notin @('Off', 'Saved')) {
      throw "VM cannot stop safely from state $($vm.State)."
    }
  }
  'pause' {
    $vm = Assert-BoundVm $true ([string]$providerReceipt.vmId)
    if ([string]$vm.State -eq 'Running') {
      Save-VM -Name $vmName
      [void](Wait-VmState @('Saved') 20)
    } elseif ([string]$vm.State -ne 'Saved') {
      throw "VM cannot be saved safely from state $($vm.State)."
    }
  }
  'checkpoint' {
    [void](Assert-BoundVm $true ([string]$providerReceipt.vmId))
    $checkpointName = "VeriSilo-v0.8-$environmentId"
    if ($null -eq (Get-VMSnapshot -VMName $vmName -Name $checkpointName -ErrorAction SilentlyContinue)) {
      [void](Checkpoint-VM -Name $vmName -SnapshotName $checkpointName)
    }
  }
  'remove' {
    if ($request.confirmDestroy -ne $true) { throw 'VM removal requires confirmDestroy=true.' }
    if ($null -ne $createJournal) {
      $createJournal = Invoke-CreateJournalRollback $createJournal
      $cleanupState = 'rolled_back_from_journal'
    } else {
      $vm = Assert-BoundVm $false ([string]$providerReceipt.vmId)
      $ownedDisks = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
      if ($null -ne $vm) {
        $foreignSwitchAdapters = @(Get-VMNetworkAdapter -All -ErrorAction Stop | Where-Object {
          [string]$_.SwitchName -ceq $switchName -and
          ([string]$_.VMId).ToLowerInvariant() -cne ([string]$providerReceipt.vmId).ToLowerInvariant()
        })
        if ($foreignSwitchAdapters.Count -ne 0) {
          throw 'The bound switch has a foreign active VM adapter; no VM, disk, or switch will be removed.'
        }
        foreach ($drive in @(Get-VMHardDiskDrive -VMName $vmName -ErrorAction Stop)) {
          [void]$ownedDisks.Add((Assert-DescendantPath $environmentRoot ([string]$drive.Path)))
        }
        foreach ($snapshot in @(Get-VMSnapshot -VMName $vmName -ErrorAction SilentlyContinue)) {
          foreach ($drive in @($snapshot | Get-VMHardDiskDrive -ErrorAction Stop)) {
            [void]$ownedDisks.Add((Assert-DescendantPath $environmentRoot ([string]$drive.Path)))
          }
        }
        if ([string]$vm.State -notin @('Off', 'Saved')) {
          throw 'Stop the exact bound VM before destroy; removal never force-powers it off.'
        }
        Remove-VM -Name $vmName -Force
        if ($null -ne (Get-VM -Id ([Guid]$providerReceipt.vmId) -ErrorAction SilentlyContinue)) {
          throw 'The exact bound VM still exists after removal.'
        }
      }
      if (Test-Path -LiteralPath $diskPath -PathType Leaf) {
        $orphan = Get-VHD -Path $diskPath -ErrorAction Stop
        if ([string]$orphan.VhdType -cne 'Differencing') {
          throw 'Orphaned Silo disk is not a differencing disk.'
        }
      }
      $activeSwitchAdapters = @(Get-VMNetworkAdapter -All -ErrorAction Stop | Where-Object { [string]$_.SwitchName -ceq $switchName })
      if ($activeSwitchAdapters.Count -ne 0) {
        throw 'The bound switch still has active VM adapters and no disk or switch will be removed.'
      }
      [void]$ownedDisks.Add($diskPath)
      foreach ($ownedDisk in $ownedDisks) {
        if (Test-Path -LiteralPath $ownedDisk -PathType Leaf) {
          [void](Assert-RegularFile $ownedDisk ([long]::MaxValue) 'Owned VM disk')
          Remove-Item -LiteralPath $ownedDisk -Force
        }
      }
      $switch = Assert-BoundSwitch $false
      if ($null -ne $switch) { Remove-VMSwitch -Name $switchName -Force }
      if (Test-Path -LiteralPath $createJournalPath -PathType Leaf) {
        [void](Read-CreateJournal $createJournalPath $environmentId $vmName $switchName $diskPath)
        Remove-Item -LiteralPath $createJournalPath -Force
      }
      $cleanupState = 'removed_from_receipt'
    }
  }
  'health' {
    [void](Assert-BoundVm $true ([string]$providerReceipt.vmId))
    [void](Assert-BoundSwitch $true)
  }
  'logs' {
    $vm = Assert-BoundVm $true ([string]$providerReceipt.vmId)
  }
  }

  $identitySource = if ($null -ne $providerReceipt) { $providerReceipt } else { $createJournal }
  $result = [ordered]@{
    schemaVersion = 1
    action = $request.action
    environmentId = $environmentId
    requestNonce = $requestNonce
    success = $true
    source = 'hyperv-controller'
    observedAt = [DateTime]::UtcNow.ToString('o')
    vmName = [string]$identitySource.vmName
    vmId = if ($null -eq $identitySource.vmId) { $null } else { [string]$identitySource.vmId }
    generation = 2
    baseImageSha256 = [string]$identitySource.baseImageSha256
    guestAgentVersion = $null
    guestAgentSha256 = $null
    guestProfile = 'unavailable'
    guestHealth = 'unavailable'
    proxy = 'unavailable'
    exit = 'unavailable'
    proxyDns = 'unavailable'
    guestResolver = 'unavailable'
    browserReady = 'unavailable'
  }
  if ($request.action -eq 'remove') {
    $result.cleanupState = $cleanupState
  }
  Write-BoundedJson $logPath $result 16384
  $result | ConvertTo-Json -Compress
} catch {
  $errorMessage = [string]$_.Exception.Message
  $failure = [ordered]@{
    schemaVersion = 1
    action = [string]$request.action
    environmentId = $environmentId
    requestNonce = $requestNonce
    success = $false
    source = 'hyperv-controller'
    observedAt = [DateTime]::UtcNow.ToString('o')
    error = $errorMessage.Substring(0, [Math]::Min(1024, $errorMessage.Length))
    guestAgentVersion = $null
    guestAgentSha256 = $null
    guestProfile = 'unavailable'
    guestHealth = 'unavailable'
    proxy = 'unavailable'
    exit = 'unavailable'
    proxyDns = 'unavailable'
    guestResolver = 'unavailable'
    browserReady = 'unavailable'
  }
  try { Write-BoundedJson $logPath $failure 16384 } catch { }
  throw
} finally {
  if ($null -ne $baseImageLease) { $baseImageLease.Dispose() }
}
