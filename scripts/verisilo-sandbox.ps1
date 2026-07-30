[CmdletBinding(DefaultParameterSetName = 'Run')]
param(
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [string]$RequestPath,
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [string]$StateRoot,
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [string]$SandboxExecutable,
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

function Remove-RegularFileIfPresent {
  param([string]$Path, [long]$MaximumBytes, [string]$Label)
  if (Test-Path -LiteralPath $Path) {
    [void](Assert-RegularFile $Path $MaximumBytes $Label)
    Remove-Item -LiteralPath $Path -Force
  }
}

function Write-BoundedJson {
  param([string]$Path, [object]$Value, [int]$MaximumBytes)
  $json = ($Value | ConvertTo-Json -Depth 6 -Compress) + [Environment]::NewLine
  $bytes = [Text.Encoding]::UTF8.GetBytes($json)
  if ($bytes.Length -gt $MaximumBytes) { throw 'Sandbox receipt exceeded its fixed byte limit.' }
  $temporary = "$Path.$([Guid]::NewGuid().ToString('N')).tmp"
  $stream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try {
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
  } finally {
    $stream.Dispose()
  }
  Remove-RegularFileIfPresent $Path $MaximumBytes 'Existing Sandbox receipt'
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

function Read-Request {
  param([string]$Path)
  $file = Assert-RegularFile $Path 16384 'Sandbox request'
  $request = Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
  $fields = @('schemaVersion', 'action', 'environmentId', 'confirmDestroy')
  Assert-ExactFields $request $fields $fields
  if (($request.schemaVersion -isnot [int] -and $request.schemaVersion -isnot [long]) -or
      $request.schemaVersion -ne 1 -or $request.action -isnot [string] -or
      $request.environmentId -isnot [string] -or $request.confirmDestroy -isnot [bool]) {
    throw 'Sandbox request field types are invalid.'
  }
  $parsedId = [Guid]::Empty
  $rawId = [string]$request.environmentId
  if (-not [Guid]::TryParseExact($rawId, 'D', [ref]$parsedId) -or
      $parsedId.ToString('D') -cne $rawId) {
    throw 'environmentId must be a canonical lowercase UUID.'
  }
  if ($request.action -notin @('start', 'stop', 'health', 'logs', 'assert-exited')) {
    throw 'Unknown Sandbox controller action.'
  }
  return $request
}

function Read-Binding {
  param([string]$EnvironmentRoot, [string]$EnvironmentId)
  $path = Assert-DescendantPath $EnvironmentRoot (Join-Path $EnvironmentRoot 'binding.json')
  $file = Assert-RegularFile $path 8192 'Persistent Silo binding'
  $binding = Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
  Assert-ExactFields $binding @('schemaVersion', 'environmentId', 'backend', 'providerKey') @('schemaVersion', 'environmentId', 'backend', 'providerKey')
  if (($binding.schemaVersion -isnot [int] -and $binding.schemaVersion -isnot [long]) -or
      $binding.schemaVersion -ne 1 -or [string]$binding.environmentId -cne $EnvironmentId -or
      [string]$binding.backend -cne 'windows-sandbox' -or
      [string]$binding.providerKey -cne 'windows-sandbox-v0.8-ephemeral') {
    throw 'Persistent Silo binding does not match the Sandbox controller.'
  }
}

function Read-ProcessReceipt {
  param([string]$Path, [string]$EnvironmentId, [string]$DescriptorPath, [string]$ExecutablePath)
  if (-not (Test-Path -LiteralPath $Path)) { return $null }
  $file = Assert-RegularFile $Path 8192 'Sandbox process receipt'
  $receipt = Get-Content -LiteralPath $file.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
  $fields = @('schemaVersion', 'environmentId', 'processId', 'startTimeUtcTicks', 'executablePath', 'descriptorPath', 'descriptorSha256', 'startedAt')
  Assert-ExactFields $receipt $fields $fields
  if (($receipt.schemaVersion -isnot [int] -and $receipt.schemaVersion -isnot [long]) -or
      $receipt.schemaVersion -ne 1 -or [string]$receipt.environmentId -cne $EnvironmentId -or
      ($receipt.processId -isnot [int] -and $receipt.processId -isnot [long]) -or [long]$receipt.processId -le 0 -or
      ($receipt.startTimeUtcTicks -isnot [long] -and $receipt.startTimeUtcTicks -isnot [int]) -or
      $receipt.executablePath -isnot [string] -or $receipt.descriptorPath -isnot [string] -or
      $receipt.descriptorSha256 -isnot [string] -or
      -not [IO.Path]::GetFullPath([string]$receipt.executablePath).Equals($ExecutablePath, [StringComparison]::OrdinalIgnoreCase) -or
      -not [IO.Path]::GetFullPath([string]$receipt.descriptorPath).Equals($DescriptorPath, [StringComparison]::OrdinalIgnoreCase) -or
      [string]$receipt.descriptorSha256 -notmatch '^[A-F0-9]{64}$' -or
      (Get-FileHash -LiteralPath $DescriptorPath -Algorithm SHA256).Hash -cne [string]$receipt.descriptorSha256) {
    throw 'Sandbox process receipt failed its exact binding.'
  }
  [void](Convert-StrictUtcTimestamp $receipt.startedAt 'Sandbox receipt startedAt')
  return $receipt
}

function Get-ExactTrackedProcess {
  param([object]$Receipt, [string]$ExecutablePath)
  if ($null -eq $Receipt) { return $null }
  $process = Get-Process -Id ([int]$Receipt.processId) -ErrorAction SilentlyContinue
  if ($null -eq $process) { return $null }
  try {
    $startTicks = $process.StartTime.ToUniversalTime().Ticks
    $actualPath = [IO.Path]::GetFullPath([string]$process.Path)
  } catch {
    throw 'The tracked Sandbox process identity could not be revalidated.'
  }
  if ($startTicks -ne [long]$Receipt.startTimeUtcTicks -or
      -not $actualPath.Equals($ExecutablePath, [StringComparison]::OrdinalIgnoreCase)) {
    # PID reuse or executable drift proves this is not the owned process. It is
    # never closed or terminated by this controller.
    return $null
  }
  $receiptStartedAt = Convert-StrictUtcTimestamp $Receipt.startedAt 'Sandbox receipt startedAt'
  $processStartedAt = [DateTimeOffset]$process.StartTime.ToUniversalTime()
  if ($receiptStartedAt -lt $processStartedAt -or $receiptStartedAt -gt $processStartedAt.AddMinutes(1)) {
    throw 'Sandbox process receipt timestamp does not match the exact process start.'
  }
  return $process
}

function Get-AnySandboxProcess {
  param([string]$ExecutablePath)
  foreach ($process in @(Get-Process -Name 'WindowsSandbox' -ErrorAction SilentlyContinue)) {
    try {
      if ([IO.Path]::GetFullPath([string]$process.Path).Equals($ExecutablePath, [StringComparison]::OrdinalIgnoreCase)) {
        return $process
      }
    } catch {
      # An uninspectable same-name process cannot be adopted or terminated.
      return $process
    }
  }
  return $null
}

if ($SelfTest) {
  $sample = [pscustomobject]@{
    schemaVersion = 1
    action = 'start'
    environmentId = [Guid]::NewGuid().ToString('D')
    confirmDestroy = $false
  }
  Assert-ExactFields $sample @('schemaVersion', 'action', 'environmentId', 'confirmDestroy') @('schemaVersion', 'action', 'environmentId', 'confirmDestroy')
  $rejected = $false
  try { Assert-ExactFields ([pscustomobject]@{ action = 'stop'; processId = 1 }) @('action') @('action') } catch { $rejected = $true }
  if (-not $rejected) { throw 'Sandbox controller self-test did not reject a caller-supplied PID.' }
  Write-Host 'Sandbox controller request validation self-test passed.'
  exit 0
}

$stateRootPath = Resolve-SafeDirectory $StateRoot 'StateRoot'
$requestPathResolved = Assert-DescendantPath $stateRootPath $RequestPath
$request = Read-Request $requestPathResolved
$environmentId = ([Guid]$request.environmentId).ToString('D')
$environmentRoot = Resolve-SafeDirectory (Assert-DescendantPath $stateRootPath (Join-Path $stateRootPath $environmentId)) 'Sandbox environment root'
$descriptorPath = Assert-DescendantPath $environmentRoot (Join-Path $environmentRoot 'environment.wsb')
$bootstrapRootPath = Assert-DescendantPath $environmentRoot (Join-Path $environmentRoot 'bootstrap')
$processReceiptPath = Assert-DescendantPath $environmentRoot (Join-Path $environmentRoot 'sandbox-process.json')
$statusPath = Assert-DescendantPath $environmentRoot (Join-Path $environmentRoot 'sandbox-status.json')
$expectedExecutable = [IO.Path]::GetFullPath((Join-Path $env:WINDIR 'System32\WindowsSandbox.exe'))
$actualExecutable = [IO.Path]::GetFullPath($SandboxExecutable)
if (-not $actualExecutable.Equals($expectedExecutable, [StringComparison]::OrdinalIgnoreCase)) {
  throw 'SandboxExecutable must be the fixed Windows System32 binary.'
}
[void](Assert-RegularFile $actualExecutable ([long]::MaxValue) 'Windows Sandbox executable')
[void](Assert-RegularFile $descriptorPath 262144 'Sandbox descriptor')
Read-Binding $environmentRoot $environmentId

$receipt = Read-ProcessReceipt $processReceiptPath $environmentId $descriptorPath $actualExecutable
$trackedProcess = Get-ExactTrackedProcess $receipt $actualExecutable
$state = 'exited'
$processId = $null

try {
  switch ([string]$request.action) {
    'start' {
      $bootstrapRoot = Resolve-SafeDirectory $bootstrapRootPath 'Sandbox bootstrap root'
      $bootstrapPath = Assert-DescendantPath $bootstrapRoot (Join-Path $bootstrapRoot 'verisilo-sandbox-bootstrap.ps1')
      [void](Assert-RegularFile $bootstrapPath 262144 'Sandbox bootstrap')
      if ($null -ne $receipt -and $null -eq $trackedProcess) {
        Remove-RegularFileIfPresent $processReceiptPath 8192 'Stale Sandbox process receipt'
        $receipt = $null
      }
      if ($null -eq $trackedProcess) {
        if ($null -ne (Get-AnySandboxProcess $actualExecutable)) {
          throw 'Another Windows Sandbox process is active and cannot be adopted.'
        }
        $trackedProcess = Start-Process -FilePath $actualExecutable -ArgumentList @($descriptorPath) -PassThru
        Start-Sleep -Seconds 2
        $trackedProcess.Refresh()
        if ($trackedProcess.HasExited) { throw 'Windows Sandbox exited during the bounded launch check.' }
        $receipt = [ordered]@{
          schemaVersion = 1
          environmentId = $environmentId
          processId = [int]$trackedProcess.Id
          startTimeUtcTicks = [long]$trackedProcess.StartTime.ToUniversalTime().Ticks
          executablePath = $actualExecutable
          descriptorPath = $descriptorPath
          descriptorSha256 = (Get-FileHash -LiteralPath $descriptorPath -Algorithm SHA256).Hash
          startedAt = [DateTime]::UtcNow.ToString('o')
        }
        Write-BoundedJson $processReceiptPath $receipt 8192
      }
      $state = 'running'
      $processId = [int]$trackedProcess.Id
    }
    'stop' {
      if ($null -ne $trackedProcess) {
        if (-not $trackedProcess.CloseMainWindow()) {
          throw 'The exact tracked Sandbox process did not expose a graceful close action.'
        }
        if (-not $trackedProcess.WaitForExit(20000)) {
          throw 'The exact tracked Sandbox process did not exit within 20 seconds; it was not force-killed.'
        }
      } elseif ($null -ne (Get-AnySandboxProcess $actualExecutable)) {
        throw 'An untracked Windows Sandbox process is active and will not be terminated or reported stopped.'
      }
      Remove-RegularFileIfPresent $processReceiptPath 8192 'Sandbox process receipt'
      $state = 'stopped'
    }
    'health' {
      if ($null -eq $trackedProcess) { throw 'The exact tracked Sandbox process is not running.' }
      $state = 'running'
      $processId = [int]$trackedProcess.Id
    }
    'logs' {
      if ($null -eq $trackedProcess -and $null -ne (Get-AnySandboxProcess $actualExecutable)) {
        throw 'An untracked Windows Sandbox process is active; exact-process logs fail closed.'
      }
      $state = if ($null -eq $trackedProcess) { 'exited' } else { 'running' }
      if ($null -ne $trackedProcess) { $processId = [int]$trackedProcess.Id }
    }
    'assert-exited' {
      if ($request.confirmDestroy -ne $true) { throw 'assert-exited requires confirmDestroy=true.' }
      if ($null -ne $trackedProcess) { throw 'The exact tracked Sandbox process is still running.' }
      if ($null -ne (Get-AnySandboxProcess $actualExecutable)) {
        throw 'An untracked Windows Sandbox process is active; descriptor cleanup fails closed.'
      }
      Remove-RegularFileIfPresent $processReceiptPath 8192 'Exited Sandbox process receipt'
      $state = 'exited'
    }
  }

  $result = [ordered]@{
    schemaVersion = 1
    action = [string]$request.action
    environmentId = $environmentId
    success = $true
    state = $state
    processId = $processId
    observedAt = [DateTime]::UtcNow.ToString('o')
    source = 'sandbox-controller'
    guestHealth = 'unavailable'
    proxy = 'unavailable'
    exit = 'unavailable'
    proxyDns = 'unavailable'
    guestResolver = 'unavailable'
    browserReady = 'unavailable'
  }
  Write-BoundedJson $statusPath $result 8192
  $result | ConvertTo-Json -Compress
} catch {
  $failure = [ordered]@{
    schemaVersion = 1
    action = [string]$request.action
    environmentId = $environmentId
    success = $false
    state = 'failed'
    observedAt = [DateTime]::UtcNow.ToString('o')
    source = 'sandbox-controller'
    error = ([string]$_.Exception.Message).Substring(0, [Math]::Min(1024, ([string]$_.Exception.Message).Length))
    guestHealth = 'unavailable'
    proxy = 'unavailable'
    exit = 'unavailable'
    proxyDns = 'unavailable'
    guestResolver = 'unavailable'
    browserReady = 'unavailable'
  }
  Write-BoundedJson $statusPath $failure 8192
  throw
}
