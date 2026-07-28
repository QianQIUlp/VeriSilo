[CmdletBinding(DefaultParameterSetName = 'Run')]
param(
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [string]$RequestPath,
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [string]$StateRoot,
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [string]$ApprovedImageRoot,
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

function Read-StrictRequest {
  param([string]$Path)
  $file = Assert-RegularFile $Path 16384 'Request file'
  $request = Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
  $common = @('schemaVersion', 'action', 'environmentId', 'confirmDestroy')
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
  $parsedId = [Guid]::Empty
  $rawId = [string]$request.environmentId
  if (-not [Guid]::TryParseExact($rawId, 'D', [ref]$parsedId) -or
      $parsedId.ToString('D') -cne $rawId) {
    throw 'environmentId must be a canonical lowercase UUID.'
  }
  if ($request.action -notin @('create', 'start', 'stop', 'pause', 'checkpoint', 'remove', 'health', 'logs')) {
    throw 'Unknown Hyper-V action.'
  }
  return $request
}

function Read-SiloBinding {
  param([string]$EnvironmentRoot, [string]$EnvironmentId, [string]$VmName)
  $path = Assert-DescendantPath $EnvironmentRoot (Join-Path $EnvironmentRoot 'binding.json')
  $file = Assert-RegularFile $path 8192 'Persistent Silo binding'
  $binding = Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
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
  if ($bytes.Length -gt $MaximumBytes) { throw 'Hyper-V receipt exceeded its fixed byte limit.' }
  $temporary = "$Path.$([Guid]::NewGuid().ToString('N')).tmp"
  $stream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try {
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
  } finally {
    $stream.Dispose()
  }
  if (Test-Path -LiteralPath $Path) {
    [void](Assert-RegularFile $Path $MaximumBytes 'Existing Hyper-V receipt')
    Remove-Item -LiteralPath $Path -Force
  }
  Move-Item -LiteralPath $temporary -Destination $Path
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
  $receipt = Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
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
  Write-Host 'Hyper-V request validation self-test passed.'
  exit 0
}

$stateRootPath = Resolve-SafeDirectory $StateRoot 'StateRoot'
$requestPathResolved = Assert-DescendantPath $stateRootPath $RequestPath
$request = Read-StrictRequest $requestPathResolved
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

$environmentId = ([Guid]$request.environmentId).ToString('D')
$vmName = "VeriSilo-$environmentId"
$switchName = $vmName
$bindingNote = "VeriSilo:v1:$environmentId"
$environmentRoot = Resolve-SafeDirectory (Assert-DescendantPath $stateRootPath (Join-Path $stateRootPath $environmentId)) 'Hyper-V environment root'
$diskPath = Assert-DescendantPath $environmentRoot (Join-Path $environmentRoot 'system-diff.vhdx')
$logPath = Assert-DescendantPath $environmentRoot (Join-Path $environmentRoot 'hyperv-status.json')
$providerReceiptPath = Assert-DescendantPath $environmentRoot (Join-Path $environmentRoot 'hyperv-receipt.json')
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

try {
  $providerReceipt = $null
  if ($request.action -ne 'create') {
    $providerReceipt = Read-ProviderReceipt $providerReceiptPath $environmentId $vmName $switchName $diskPath
    [void](Assert-DescendantPath $imageRootPath ([string]$providerReceipt.baseImagePath))
    Assert-ReceiptImageBinding $providerReceipt
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
    $actualHash = (Get-FileHash -LiteralPath $image.FullName -Algorithm SHA256).Hash
    if ($actualHash -cne ([string]$request.manifestImageSha256).ToUpperInvariant()) {
      throw 'Base image hash does not match the signed build manifest.'
    }

    $switch = Assert-BoundSwitch $false
    if ($null -eq $switch) {
      $switch = New-VMSwitch -Name $switchName -SwitchType Internal -Notes $bindingNote
    }
    $disk = Assert-BoundDisk $image.FullName $false
    if ($null -eq $disk) {
      $disk = New-VHD -Path $diskPath -ParentPath $image.FullName -Differencing
    }
    $vm = Get-VM -Name $vmName -ErrorAction SilentlyContinue
    if ($null -eq $vm) {
      $vm = New-VM -Name $vmName -Path $environmentRoot -Generation 2 -MemoryStartupBytes 4GB -VHDPath $diskPath -SwitchName $switchName
    }
    Initialize-BoundVm
    [void](Assert-BoundSwitch $true)
    [void](Assert-BoundDisk $image.FullName $true)
    $vm = Assert-BoundVm $true
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
  }
  'health' {
    [void](Assert-BoundVm $true ([string]$providerReceipt.vmId))
    [void](Assert-BoundSwitch $true)
  }
  'logs' {
    $vm = Assert-BoundVm $true ([string]$providerReceipt.vmId)
  }
  }

  $result = [ordered]@{
    schemaVersion = 1
    action = $request.action
    environmentId = $environmentId
    success = $true
    source = 'hyperv-controller'
    observedAt = [DateTime]::UtcNow.ToString('o')
    vmName = [string]$providerReceipt.vmName
    vmId = [string]$providerReceipt.vmId
    generation = [int]$providerReceipt.generation
    baseImageSha256 = [string]$providerReceipt.baseImageSha256
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
  Write-BoundedJson $logPath $result 16384
  $result | ConvertTo-Json -Compress
} catch {
  $errorMessage = [string]$_.Exception.Message
  $failure = [ordered]@{
    schemaVersion = 1
    action = [string]$request.action
    environmentId = $environmentId
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
}
