[CmdletBinding(DefaultParameterSetName = 'Run')]
param(
  [string]$ProviderDirectory = (Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'scripts'),
  [string]$ArtifactDirectory,
  [Parameter(ParameterSetName = 'Run', Mandatory = $true)]
  [ValidatePattern('^[0-9a-f]{64}$')]
  [string]$ExpectedSignerCertificateSha256,
  [switch]$RunWslIdentity,
  [string]$WslDistribution,
  [switch]$RunSandboxLaunch,
  [switch]$RunHyperVLifecycle,
  [string]$HyperVApprovedImageRoot,
  [string]$HyperVImageFile,
  [string]$HyperVImageSha256,
  [switch]$ConfirmHyperVDestroy,
  [ValidateRange(5, 120)]
  [int]$TimeoutSeconds = 45,
  [ValidateRange(30, 600)]
  [int]$HyperVTimeoutSeconds = 300,
  [Parameter(ParameterSetName = 'SelfTest', Mandatory = $true)]
  [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:Results = [System.Collections.Generic.List[object]]::new()
$script:TemporaryArtifactDirectory = $false
$script:VerifiedProviderDirectory = $null
$script:VerifiedProviderDigests = @{}

function Add-Result {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][ValidateSet('PASS', 'FAIL', 'SKIP', 'BLOCKED')][string]$Status,
    [Parameter(Mandatory = $true)][string]$Detail
  )
  $script:Results.Add([pscustomobject][ordered]@{ name = $Name; status = $Status; detail = $Detail })
  Write-Host "[$Status] $Name - $Detail"
}

function Assert-PowerShellRuntime {
  if ($PSVersionTable.PSVersion.Major -lt 7) {
    throw 'The environment acceptance harness requires PowerShell 7 so ProcessStartInfo.ArgumentList can preserve argument boundaries.'
  }
}

function Invoke-FixedProcess {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [Parameter(Mandatory = $true)][string[]]$Arguments,
    [Parameter(Mandatory = $true)][int]$MaximumSeconds,
    [ValidateRange(1024, 65536)][int]$MaximumOutputBytes = 16384
  )
  $startInfo = [Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $FilePath
  $startInfo.UseShellExecute = $false
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  $startInfo.CreateNoWindow = $true
  foreach ($argument in $Arguments) { [void]$startInfo.ArgumentList.Add($argument) }
  $process = [Diagnostics.Process]::new()
  $process.StartInfo = $startInfo
  if (-not $process.Start()) { throw "Failed to start fixed executable: $FilePath" }
  $stdoutTask = $process.StandardOutput.ReadToEndAsync()
  $stderrTask = $process.StandardError.ReadToEndAsync()
  if (-not $process.WaitForExit($MaximumSeconds * 1000)) {
    $process.Kill($true)
    $process.WaitForExit()
    throw "Fixed process exceeded the $MaximumSeconds-second timeout: $FilePath"
  }
  $stdout = $stdoutTask.GetAwaiter().GetResult()
  $stderr = $stderrTask.GetAwaiter().GetResult()
  if ([Text.Encoding]::UTF8.GetByteCount($stdout) -gt $MaximumOutputBytes -or
      [Text.Encoding]::UTF8.GetByteCount($stderr) -gt $MaximumOutputBytes) {
    throw 'Fixed process output exceeded its acceptance-harness bound.'
  }
  return [pscustomobject]@{ ExitCode = $process.ExitCode; Stdout = $stdout; Stderr = $stderr }
}

function Assert-CanonicalUuid {
  param([string]$Value)
  $parsed = [Guid]::Empty
  if (-not [Guid]::TryParseExact($Value, 'D', [ref]$parsed) -or $parsed.ToString('D') -cne $Value) {
    throw 'Expected a canonical lowercase UUID.'
  }
}

function Get-ProviderLeafName {
  param([ValidateSet('Probe', 'HyperV', 'SandboxController', 'SandboxBootstrap', 'WslAgent')][string]$Name)
  $leaf = switch ($Name) {
    'Probe' { 'verisilo-environment-probe.ps1' }
    'HyperV' { 'verisilo-hyperv.ps1' }
    'SandboxController' { 'verisilo-sandbox.ps1' }
    'SandboxBootstrap' { 'verisilo-sandbox-bootstrap.ps1' }
    'WslAgent' { 'verisilo-wsl-guest-agent.sh' }
  }
  return $leaf
}

function Get-ProviderPath {
  param([ValidateSet('Probe', 'HyperV', 'SandboxController', 'SandboxBootstrap', 'WslAgent')][string]$Name)
  $leaf = Get-ProviderLeafName $Name
  $rootItem = Get-Item -LiteralPath ([IO.Path]::GetFullPath($ProviderDirectory)) -Force
  if (-not $rootItem.PSIsContainer -or ($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'ProviderDirectory must be a real directory, not a reparse point.'
  }
  $root = $rootItem.FullName
  $path = [IO.Path]::GetFullPath((Join-Path $root $leaf))
  if (-not $path.StartsWith($root.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
      -not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw "Fixed provider resource is missing or escaped ProviderDirectory: $leaf"
  }
  $item = Get-Item -LiteralPath $path -Force
  if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "Fixed provider resource must not be a reparse point: $leaf"
  }
  return $item.FullName
}

function Assert-PinnedProviderSignature {
  param([Parameter(Mandatory = $true)][string]$Path)
  $signature = Get-AuthenticodeSignature -LiteralPath $Path
  if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or
      $null -eq $signature.SignerCertificate -or $null -eq $signature.TimeStamperCertificate) {
    throw "Provider is not validly Authenticode-signed and timestamped: $Path"
  }
  $actualSignerCertificateSha256 = $signature.SignerCertificate.GetCertHashString(
    [Security.Cryptography.HashAlgorithmName]::SHA256
  ).ToLowerInvariant()
  if ($actualSignerCertificateSha256 -cne $ExpectedSignerCertificateSha256) {
    throw "Provider signer certificate SHA-256 does not match the pinned release signer: $Path"
  }
}

function Initialize-VerifiedProviderStage {
  $artifactRoot = Get-Item -LiteralPath $script:ArtifactDirectory -Force
  if (-not $artifactRoot.PSIsContainer -or ($artifactRoot.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'ArtifactDirectory must be a real directory before provider verification.'
  }
  $stage = Join-Path $artifactRoot.FullName "verified-providers-$([Guid]::NewGuid().ToString('N'))"
  [void](New-Item -ItemType Directory -Path $stage)
  $script:VerifiedProviderDirectory = (Get-Item -LiteralPath $stage -Force).FullName
  foreach ($name in @('Probe', 'HyperV', 'SandboxController', 'SandboxBootstrap')) {
    $source = Get-ProviderPath $name
    Assert-PinnedProviderSignature $source
    $sourceDigest = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
    $destination = Join-Path $script:VerifiedProviderDirectory ([IO.Path]::GetFileName($source))
    Copy-Item -LiteralPath $source -Destination $destination
    $stagedItem = Get-Item -LiteralPath $destination -Force
    if (($stagedItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "Staged provider unexpectedly became a reparse point: $destination"
    }
    Assert-PinnedProviderSignature $stagedItem.FullName
    $stagedDigest = (Get-FileHash -LiteralPath $stagedItem.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($stagedDigest -cne $sourceDigest) {
      throw "Staged provider digest differs from the signed source: $destination"
    }
    $script:VerifiedProviderDigests[$name] = $stagedDigest
  }
  Add-Result 'provider_signature_digest_stage' 'PASS' 'Pinned SHA-256 signer verification and exact file-digest staging completed before any provider script was executed.'
}

function Get-VerifiedProviderPath {
  param([ValidateSet('Probe', 'HyperV', 'SandboxController', 'SandboxBootstrap')][string]$Name)
  if (-not $script:VerifiedProviderDirectory -or -not $script:VerifiedProviderDigests.ContainsKey($Name)) {
    throw "Provider $Name was requested before signature/digest staging."
  }
  $leaf = Get-ProviderLeafName $Name
  $path = [IO.Path]::GetFullPath((Join-Path $script:VerifiedProviderDirectory $leaf))
  if (-not $path.StartsWith($script:VerifiedProviderDirectory.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Verified provider escaped its private stage: $Name"
  }
  $item = Get-Item -LiteralPath $path -Force
  if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "Verified provider became a reparse point: $Name"
  }
  Assert-PinnedProviderSignature $item.FullName
  $actualDigest = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actualDigest -cne [string]$script:VerifiedProviderDigests[$Name]) {
    throw "Verified provider digest changed after staging: $Name"
  }
  return $item.FullName
}

function Get-StreamSha256 {
  param([Parameter(Mandatory = $true)][IO.Stream]$Stream)
  $originalPosition = $Stream.Position
  $Stream.Position = 0
  $algorithm = [Security.Cryptography.SHA256]::Create()
  try {
    return [Convert]::ToHexString($algorithm.ComputeHash($Stream)).ToLowerInvariant()
  } finally {
    $algorithm.Dispose()
    $Stream.Position = $originalPosition
  }
}

function Initialize-DirectoryLeaseNative {
  if ('VeriSilo.Acceptance.DirectoryLeaseNative' -as [type]) { return }
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace VeriSilo.Acceptance {
  [StructLayout(LayoutKind.Sequential)]
  public struct ByHandleFileInformation {
    public uint FileAttributes;
    public System.Runtime.InteropServices.ComTypes.FILETIME CreationTime;
    public System.Runtime.InteropServices.ComTypes.FILETIME LastAccessTime;
    public System.Runtime.InteropServices.ComTypes.FILETIME LastWriteTime;
    public uint VolumeSerialNumber;
    public uint FileSizeHigh;
    public uint FileSizeLow;
    public uint NumberOfLinks;
    public uint FileIndexHigh;
    public uint FileIndexLow;
  }

  public static class DirectoryLeaseNative {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern SafeFileHandle CreateFileW(
      string fileName,
      uint desiredAccess,
      uint shareMode,
      IntPtr securityAttributes,
      uint creationDisposition,
      uint flagsAndAttributes,
      IntPtr templateFile
    );

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetFileInformationByHandle(
      SafeFileHandle file,
      out ByHandleFileInformation information
    );
  }
}
'@
}

function Open-DirectoryChainLease {
  param([Parameter(Mandatory = $true)][string]$Path)
  Initialize-DirectoryLeaseNative
  $fullPath = [IO.Path]::GetFullPath($Path)
  $ancestors = [System.Collections.Generic.List[string]]::new()
  $current = [IO.DirectoryInfo]::new($fullPath)
  while ($null -ne $current) {
    $ancestors.Add($current.FullName)
    $current = $current.Parent
  }
  $ancestors.Reverse()
  $handles = [System.Collections.Generic.List[Microsoft.Win32.SafeHandles.SafeFileHandle]]::new()
  try {
    foreach ($ancestor in $ancestors) {
      $handle = [VeriSilo.Acceptance.DirectoryLeaseNative]::CreateFileW(
        $ancestor,
        0x80,
        0x00000001 -bor 0x00000002,
        [IntPtr]::Zero,
        3,
        0x02000000 -bor 0x00200000,
        [IntPtr]::Zero
      )
      if ($handle.IsInvalid) {
        $handle.Dispose()
        throw "Could not lock directory ancestor: $ancestor (Win32 $([Runtime.InteropServices.Marshal]::GetLastWin32Error()))."
      }
      $information = [VeriSilo.Acceptance.ByHandleFileInformation]::new()
      if (-not [VeriSilo.Acceptance.DirectoryLeaseNative]::GetFileInformationByHandle($handle, [ref]$information) -or
          ($information.FileAttributes -band 0x10) -eq 0 -or
          ($information.FileAttributes -band 0x400) -ne 0) {
        $handle.Dispose()
        throw "Directory ancestor lease resolved to a non-directory or reparse point: $ancestor"
      }
      $handles.Add($handle)
    }
    return $handles
  } catch {
    foreach ($handle in $handles) { $handle.Dispose() }
    throw
  }
}

function Open-HyperVImageLease {
  param([string]$ApprovedRoot, [string]$ImageFile, [string]$ExpectedSha256)
  $root = [IO.Path]::GetFullPath($ApprovedRoot)
  if ([IO.Path]::GetFileName($ImageFile) -cne $ImageFile) {
    throw 'Hyper-V image lease requires an exact leaf filename.'
  }
  $imagePath = [IO.Path]::GetFullPath((Join-Path $root $ImageFile))
  if (-not [IO.Path]::GetDirectoryName($imagePath).Equals($root, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Hyper-V image lease escaped ApprovedImageRoot.'
  }
  $directoryHandles = Open-DirectoryChainLease $root
  $stream = $null
  try {
    $item = Get-Item -LiteralPath $imagePath -Force
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or $item.Length -eq 0) {
      throw 'Hyper-V image lease requires a non-empty regular non-reparse file.'
    }
    $stream = [IO.File]::Open($imagePath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    if ((Get-StreamSha256 $stream) -cne $ExpectedSha256.ToLowerInvariant()) {
      throw 'Locked Hyper-V image handle did not match the declared SHA-256.'
    }
    return [pscustomobject]@{
      Path = $imagePath
      Stream = $stream
      DirectoryHandles = $directoryHandles
    }
  } catch {
    if ($null -ne $stream) { $stream.Dispose() }
    foreach ($handle in $directoryHandles) { $handle.Dispose() }
    throw
  }
}

function Close-HyperVImageLease {
  param([Parameter(Mandatory = $true)]$Lease)
  $Lease.Stream.Dispose()
  foreach ($handle in $Lease.DirectoryHandles) { $handle.Dispose() }
}

function Open-VerifiedProviderLease {
  param([ValidateSet('Probe', 'HyperV', 'SandboxController', 'SandboxBootstrap')][string]$Name)
  $path = Get-VerifiedProviderPath $Name
  $stream = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
  try {
    if ((Get-StreamSha256 $stream) -cne [string]$script:VerifiedProviderDigests[$Name]) {
      throw "Provider changed before its non-writable execution lease was acquired: $Name"
    }
    return [pscustomobject]@{ Path = $path; Stream = $stream }
  } catch {
    $stream.Dispose()
    throw
  }
}

function Test-VerifiedProviderSet {
  try {
    foreach ($name in @('Probe', 'HyperV', 'SandboxController', 'SandboxBootstrap')) {
      $lease = Open-VerifiedProviderLease $name
      $lease.Stream.Dispose()
    }
    return $true
  } catch {
    return $false
  }
}

function Invoke-ProviderSelfTests {
  $pwsh = [Environment]::ProcessPath
  foreach ($name in @('Probe', 'HyperV', 'SandboxController')) {
    $lease = Open-VerifiedProviderLease $name
    try {
      $output = Invoke-FixedProcess $pwsh @('-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'AllSigned', '-File', $lease.Path, '-SelfTest') $TimeoutSeconds 4096
      if ($output.ExitCode -ne 0) { throw "$name provider self-test failed: $($output.Stderr)" }
    } finally {
      $lease.Stream.Dispose()
    }
  }
  Add-Result 'provider_source_self_tests' 'PASS' 'Probe, Hyper-V, and Sandbox controller strict request self-tests completed; no virtualization operation was requested.'
}

function Invoke-WslIdentityAcceptance {
  if ([string]::IsNullOrWhiteSpace($WslDistribution)) {
    throw '-RunWslIdentity requires -WslDistribution from the current wsl.exe discovery result.'
  }
  if ($WslDistribution.Trim() -cne $WslDistribution -or $WslDistribution.Length -gt 128 -or
      $WslDistribution.IndexOfAny([char[]]@(0, 10, 13)) -ge 0) {
    throw 'WSL distribution contains unsupported characters.'
  }
  $discovery = Invoke-FixedProcess 'wsl.exe' @('--list', '--quiet') $TimeoutSeconds
  if ($discovery.ExitCode -ne 0) { throw "wsl.exe discovery failed: $($discovery.Stderr)" }
  $distributions = @($discovery.Stdout -split "`r?`n" | ForEach-Object { $_.Trim([char]0).Trim() } | Where-Object { $_ })
  if ($WslDistribution -cnotin $distributions) { throw 'Requested WSL distribution was not returned by current discovery.' }
  $nilId = [Guid]::Empty.ToString('D')
  $identityOutput = Invoke-FixedProcess 'wsl.exe' @(
    '-d', $WslDistribution, '--user', 'root', '--exec',
    '/opt/verisilo/bin/verisilo-guest-agent', 'identity', '--silo-id', $nilId
  ) $TimeoutSeconds 4096
  if ($identityOutput.ExitCode -ne 0) { throw "WSL identity failed: $($identityOutput.Stderr)" }
  $identity = $identityOutput.Stdout | ConvertFrom-Json
  $expectedHash = (Get-FileHash -LiteralPath (Get-ProviderPath 'WslAgent') -Algorithm SHA256).Hash.ToLowerInvariant()
  $fields = @($identity.PSObject.Properties.Name | Sort-Object)
  $expectedFields = @('agentVersion', 'browserUid', 'browserUser', 'mode', 'ownerUid', 'path', 'schemaVersion', 'sha256')
  if (($fields -join ',') -cne ($expectedFields -join ',') -or $identity.schemaVersion -ne 1 -or
      [string]$identity.agentVersion -cne '0.8.0' -or [string]$identity.sha256 -cne $expectedHash -or
      $identity.ownerUid -ne 0 -or [string]$identity.mode -cne '755' -or
      [string]$identity.path -cne '/opt/verisilo/bin/verisilo-guest-agent' -or
      [string]$identity.browserUser -cne 'verisilo-browser' -or
      $identity.browserUid -lt 1000 -or $identity.browserUid -ge 65534) {
    throw 'WSL guest agent did not match the exact path/hash/owner/mode/version contract.'
  }
  Add-Result 'wsl_exact_guest_agent_identity' 'PASS' "Validated exact identity in discovered distribution $WslDistribution; no guest state was changed."
}

function Start-SandboxAcceptance {
  $probeLease = Open-VerifiedProviderLease 'Probe'
  try {
    $probeOutput = Invoke-FixedProcess ([Environment]::ProcessPath) @(
      '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'AllSigned',
      '-File', $probeLease.Path, '-ExpectedSignerCertificateSha256', $ExpectedSignerCertificateSha256
    ) $TimeoutSeconds 4096
  } finally {
    $probeLease.Stream.Dispose()
  }
  if ($probeOutput.ExitCode -ne 0) { throw "Environment probe failed: $($probeOutput.Stderr)" }
  $probe = $probeOutput.Stdout | ConvertFrom-Json
  if (-not $probe.sandboxAvailable) { throw 'Windows Sandbox is unavailable according to the fixed host probe.' }
  if (-not $probe.releaseScriptsTrusted) { throw 'Sandbox acceptance requires the same-signer release-script boundary.' }
  $siloId = [Guid]::NewGuid().ToString('D')
  $stateRoot = Join-Path $script:ArtifactDirectory "sandbox-state-$siloId"
  $environmentRoot = Join-Path $stateRoot $siloId
  $bootstrapStage = Join-Path $environmentRoot 'bootstrap'
  [void](New-Item -ItemType Directory -Path $bootstrapStage -Force)
  $verifiedBootstrapLease = Open-VerifiedProviderLease 'SandboxBootstrap'
  $stagedBootstrap = Join-Path $bootstrapStage 'verisilo-sandbox-bootstrap.ps1'
  try {
    Copy-Item -LiteralPath $verifiedBootstrapLease.Path -Destination $stagedBootstrap
  } finally {
    $verifiedBootstrapLease.Stream.Dispose()
  }
  Assert-PinnedProviderSignature $stagedBootstrap
  if ((Get-FileHash -LiteralPath $stagedBootstrap -Algorithm SHA256).Hash.ToLowerInvariant() -cne
      [string]$script:VerifiedProviderDigests['SandboxBootstrap']) {
    throw 'Sandbox bootstrap digest changed while copying it into the read-only guest stage.'
  }
  $bootstrapExecutionLease = [IO.File]::Open(
    $stagedBootstrap,
    [IO.FileMode]::Open,
    [IO.FileAccess]::Read,
    [IO.FileShare]::Read
  )
  if ((Get-StreamSha256 $bootstrapExecutionLease) -cne [string]$script:VerifiedProviderDigests['SandboxBootstrap']) {
    $bootstrapExecutionLease.Dispose()
    throw 'Sandbox bootstrap changed before its guest-execution lease was acquired.'
  }
  try {
  $binding = [ordered]@{
    schemaVersion = 1
    environmentId = $siloId
    backend = 'windows-sandbox'
    providerKey = 'windows-sandbox-v0.8-ephemeral'
  }
  [IO.File]::WriteAllText((Join-Path $environmentRoot 'binding.json'), (($binding | ConvertTo-Json) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
  $bootstrapRoot = [Security.SecurityElement]::Escape([IO.Path]::GetFullPath($bootstrapStage))
  $wsbPath = Join-Path $environmentRoot 'environment.wsb'
  $xml = "<Configuration>`n  <VGpu>Disable</VGpu>`n  <Networking>Disable</Networking>`n  <AudioInput>Disable</AudioInput>`n  <VideoInput>Disable</VideoInput>`n  <PrinterRedirection>Disable</PrinterRedirection>`n  <ClipboardRedirection>Disable</ClipboardRedirection>`n  <ProtectedClient>Enable</ProtectedClient>`n  <MemoryInMB>4096</MemoryInMB>`n  <MappedFolders>`n    <MappedFolder>`n      <HostFolder>$bootstrapRoot</HostFolder>`n      <SandboxFolder>C:\VeriSilo\Bootstrap</SandboxFolder>`n      <ReadOnly>true</ReadOnly>`n    </MappedFolder>`n  </MappedFolders>`n  <LogonCommand>`n    <Command>powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy AllSigned -File C:\VeriSilo\Bootstrap\verisilo-sandbox-bootstrap.ps1 -SiloId $siloId</Command>`n  </LogonCommand>`n</Configuration>`n"
  [IO.File]::WriteAllText($wsbPath, $xml, [Text.UTF8Encoding]::new($false))
  $sandboxExecutable = Join-Path $env:WINDIR 'System32\WindowsSandbox.exe'
  $started = $false
  function Invoke-SandboxControllerAcceptance {
    param([string]$Action, [bool]$ConfirmDestroy)
    $request = [ordered]@{
      schemaVersion = 1
      action = $Action
      environmentId = $siloId
      confirmDestroy = $ConfirmDestroy
    }
    $requestPath = Join-Path $environmentRoot "$([Guid]::NewGuid().ToString('D')).sandbox-request.json"
    [IO.File]::WriteAllText($requestPath, (($request | ConvertTo-Json) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
    $controllerLease = $null
    try {
      $controllerLease = Open-VerifiedProviderLease 'SandboxController'
      $output = Invoke-FixedProcess ([Environment]::ProcessPath) @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'AllSigned',
        '-File', $controllerLease.Path, '-RequestPath', $requestPath, '-StateRoot', $stateRoot,
        '-SandboxExecutable', $sandboxExecutable
      ) ([Math]::Max($TimeoutSeconds, 30)) 16384
      if ($output.ExitCode -ne 0) { throw "Sandbox controller $Action failed: $($output.Stderr)" }
      $response = $output.Stdout | ConvertFrom-Json
      if ($response.schemaVersion -ne 1 -or [string]$response.action -cne $Action -or
          [string]$response.environmentId -cne $siloId -or $response.success -ne $true -or
          [string]$response.source -cne 'sandbox-controller' -or
          @('guestHealth', 'proxy', 'exit', 'proxyDns', 'guestResolver', 'browserReady').Where({ [string]$response.$_ -cne 'unavailable' }).Count -ne 0) {
        throw "Sandbox controller $Action returned a mismatched or overstated receipt."
      }
      return $response
    } finally {
      if ($null -ne $controllerLease) { $controllerLease.Stream.Dispose() }
      Remove-Item -LiteralPath $requestPath -Force -ErrorAction SilentlyContinue
    }
  }
  try {
    $start = Invoke-SandboxControllerAcceptance 'start' $false
    $started = $true
    if ([string]$start.state -cne 'running' -or [int]$start.processId -le 0) {
      throw 'Sandbox start did not return an exact running process receipt.'
    }
    [void](Invoke-SandboxControllerAcceptance 'health' $false)
    [void](Invoke-SandboxControllerAcceptance 'logs' $false)
    [void](Invoke-SandboxControllerAcceptance 'stop' $false)
    $started = $false
    [void](Invoke-SandboxControllerAcceptance 'assert-exited' $true)
  } finally {
    if ($started) {
      try { [void](Invoke-SandboxControllerAcceptance 'stop' $false) } catch { }
    }
  }
  Add-Result 'sandbox_exact_process_lifecycle' 'PASS' 'Started, health-checked, logged, gracefully stopped, and exit-confirmed the exact Sandbox process. Guest network, DNS, health, and browser readiness remained unavailable.'
  } finally {
    $bootstrapExecutionLease.Dispose()
  }
}

function Write-HyperVRequest {
  param([string]$StateRoot, [string]$EnvironmentId, [string]$Action, [bool]$ConfirmDestroy)
  Assert-CanonicalUuid $EnvironmentId
  $directory = Join-Path $StateRoot $EnvironmentId
  [void](New-Item -ItemType Directory -Path $directory -Force)
  $requestNonce = [Guid]::NewGuid().ToString('D')
  $request = [ordered]@{
    schemaVersion = 1
    action = $Action
    environmentId = $EnvironmentId
    requestNonce = $requestNonce
    confirmDestroy = $ConfirmDestroy
  }
  if ($Action -eq 'create') {
    $request.manifestSchemaVersion = 1
    $request.manifestImageFile = $HyperVImageFile
    $request.manifestImageSha256 = $HyperVImageSha256.ToLowerInvariant()
    $request.manifestTrusted = $true
  }
  $path = Join-Path $directory "$requestNonce.request.json"
  $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($request | ConvertTo-Json -Depth 4) + [Environment]::NewLine)
  $directoryHandles = Open-DirectoryChainLease $directory
  $stream = $null
  try {
    $stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::Read)
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
  } catch {
    if ($null -ne $stream) { $stream.Dispose() }
    foreach ($handle in $directoryHandles) { $handle.Dispose() }
    throw
  }
  return [pscustomobject]@{
    Path = $path
    Nonce = $requestNonce
    Stream = $stream
    DirectoryHandles = $directoryHandles
  }
}

function Invoke-HyperVAction {
  param([string]$StateRoot, [string]$EnvironmentId, [string]$Action, [bool]$ConfirmDestroy)
  $requestLease = Write-HyperVRequest $StateRoot $EnvironmentId $Action $ConfirmDestroy
  $providerLease = $null
  try {
    $providerLease = Open-VerifiedProviderLease 'HyperV'
    $output = Invoke-FixedProcess 'powershell.exe' @(
      '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'AllSigned', '-File',
      $providerLease.Path, '-RequestPath', $requestLease.Path, '-StateRoot', $StateRoot,
      '-ApprovedImageRoot', $HyperVApprovedImageRoot,
      '-ExpectedEnvironmentId', $EnvironmentId, '-ExpectedAction', $Action,
      '-ExpectedRequestNonce', $requestLease.Nonce
    ) $HyperVTimeoutSeconds 16384
    if ($output.ExitCode -ne 0) { throw "Hyper-V $Action failed: $($output.Stderr)" }
    $response = $output.Stdout | ConvertFrom-Json
    $vmId = [Guid]::Empty
    $hasVmId = $null -ne $response.vmId
    $vmIdValid = $hasVmId -and
      [Guid]::TryParseExact([string]$response.vmId, 'D', [ref]$vmId) -and
      $vmId -ne [Guid]::Empty
    $cleanupProperty = $response.PSObject.Properties['cleanupState']
    $cleanupStateValid = if ($Action -eq 'remove') {
      $null -ne $cleanupProperty -and
        [string]$response.cleanupState -cin @('removed_from_receipt', 'rolled_back_from_journal') -and
        ([string]$response.cleanupState -cne 'removed_from_receipt' -or $vmIdValid) -and
        ([string]$response.cleanupState -cne 'rolled_back_from_journal' -or $null -eq $response.vmId -or $vmIdValid)
    } else {
      $null -eq $cleanupProperty -and $vmIdValid
    }
    if ($response.schemaVersion -ne 1 -or [string]$response.action -cne $Action -or
        [string]$response.environmentId -cne $EnvironmentId -or $response.success -ne $true -or
        [string]$response.requestNonce -cne $requestLease.Nonce -or
        [string]$response.source -cne 'hyperv-controller' -or
        [string]$response.vmName -cne "VeriSilo-$EnvironmentId" -or
        -not $cleanupStateValid -or
        $response.generation -ne 2 -or [string]$response.baseImageSha256 -cne $HyperVImageSha256.ToLowerInvariant() -or
        $null -ne $response.guestAgentVersion -or $null -ne $response.guestAgentSha256 -or
        @('guestProfile', 'guestHealth', 'proxy', 'exit', 'proxyDns', 'guestResolver', 'browserReady').Where({ [string]$response.$_ -cne 'unavailable' }).Count -ne 0) {
      throw "Hyper-V $Action returned a mismatched receipt."
    }
  } finally {
    if ($null -ne $providerLease) { $providerLease.Stream.Dispose() }
    if ($null -ne $requestLease.Stream) { $requestLease.Stream.Dispose() }
    foreach ($handle in $requestLease.DirectoryHandles) { $handle.Dispose() }
    Remove-Item -LiteralPath $requestLease.Path -Force -ErrorAction SilentlyContinue
  }
}

function Invoke-HyperVLifecycleAcceptance {
  if (-not $ConfirmHyperVDestroy) { throw '-RunHyperVLifecycle requires -ConfirmHyperVDestroy so the harness cannot strand its test VM by design.' }
  if ([string]::IsNullOrWhiteSpace($HyperVApprovedImageRoot) -or [string]::IsNullOrWhiteSpace($HyperVImageFile) -or
      [string]::IsNullOrWhiteSpace($HyperVImageSha256)) {
    throw 'Hyper-V lifecycle acceptance requires the approved image root, leaf filename, and SHA-256.'
  }
  if ([IO.Path]::GetFileName($HyperVImageFile) -cne $HyperVImageFile -or
      $HyperVImageSha256 -notmatch '^[A-Fa-f0-9]{64}$') { throw 'Hyper-V image manifest inputs are invalid.' }
  if (-not (Test-VerifiedProviderSet)) { throw 'Hyper-V acceptance requires the intact pinned-signer and digest-verified provider stage.' }
  $imageLease = Open-HyperVImageLease $HyperVApprovedImageRoot $HyperVImageFile $HyperVImageSha256
  try {
    $stateRoot = Join-Path $script:ArtifactDirectory 'hyperv-state'
    [void](New-Item -ItemType Directory -Path $stateRoot -Force)
    $environmentId = [Guid]::NewGuid().ToString('D')
    $environmentRoot = Join-Path $stateRoot $environmentId
    [void](New-Item -ItemType Directory -Path $environmentRoot -Force)
    $binding = [ordered]@{ schemaVersion = 1; environmentId = $environmentId; backend = 'hyper-v'; providerKey = "VeriSilo-$environmentId" }
    [IO.File]::WriteAllText((Join-Path $environmentRoot 'binding.json'), (($binding | ConvertTo-Json) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
    $created = $false
    try {
      $created = $true
      Invoke-HyperVAction $stateRoot $environmentId 'create' $false
      foreach ($action in @('create', 'start', 'health', 'pause', 'pause', 'start', 'checkpoint', 'checkpoint', 'stop', 'logs')) {
        Invoke-HyperVAction $stateRoot $environmentId $action $false
      }
      Add-Result 'hyperv_fixed_lifecycle' 'PASS' 'Create/retry/start/health/pause/retry/checkpoint/retry/stop/logs completed with exact receipts.'
    } finally {
      $cleanupAuthorized = $created -and (
        (Test-Path -LiteralPath (Join-Path $environmentRoot 'hyperv-receipt.json') -PathType Leaf) -or
        (Test-Path -LiteralPath (Join-Path $environmentRoot 'hyperv-create-journal.json') -PathType Leaf)
      )
      if ($cleanupAuthorized) {
        Invoke-HyperVAction $stateRoot $environmentId 'remove' $true
        Invoke-HyperVAction $stateRoot $environmentId 'remove' $true
        Add-Result 'hyperv_confirmed_cleanup' 'PASS' 'Confirmed destroy and idempotent destroy retry completed from either the final receipt or the persistent pre-mutation rollback journal.'
      }
    }
  } finally {
    Close-HyperVImageLease $imageLease
  }
}

Assert-PowerShellRuntime
if (-not $ArtifactDirectory) {
  $ArtifactDirectory = Join-Path ([IO.Path]::GetTempPath()) "verisilo-environment-acceptance-$([Guid]::NewGuid().ToString('N'))"
  $script:TemporaryArtifactDirectory = $true
}
$script:ArtifactDirectory = [IO.Path]::GetFullPath($ArtifactDirectory)
[void](New-Item -ItemType Directory -Path $script:ArtifactDirectory -Force)

try {
  if ($SelfTest) {
    Assert-CanonicalUuid ([Guid]::NewGuid().ToString('D'))
    if ('command' -in @('create', 'start', 'stop', 'pause', 'checkpoint', 'remove', 'health', 'logs')) {
      throw 'Harness allowlist self-test failed.'
    }
    $rejectedUnverifiedProvider = $false
    try { [void](Get-VerifiedProviderPath 'Probe') } catch { $rejectedUnverifiedProvider = $true }
    if (-not $rejectedUnverifiedProvider) {
      throw 'Harness allowed a provider path before pinned signature/digest staging.'
    }
    $imageLeaseRoot = Join-Path $script:ArtifactDirectory "image-lease-self-test-$([Guid]::NewGuid().ToString('N'))"
    [void](New-Item -ItemType Directory -Path $imageLeaseRoot)
    $imageLeasePath = Join-Path $imageLeaseRoot 'base.vhdx'
    [IO.File]::WriteAllBytes($imageLeasePath, [Text.Encoding]::ASCII.GetBytes('abc'))
    $imageLease = Open-HyperVImageLease $imageLeaseRoot 'base.vhdx' 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'
    try {
      $secondWriteDenied = $false
      try {
        $attacker = [IO.File]::Open($imageLeasePath, [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
        $attacker.Dispose()
      } catch {
        $secondWriteDenied = $true
      }
      if (-not $secondWriteDenied) { throw 'Harness image lease allowed a second writer.' }
      $rootRenameDenied = $false
      try { [IO.Directory]::Move($imageLeaseRoot, "$imageLeaseRoot.replaced") } catch { $rootRenameDenied = $true }
      if (-not $rootRenameDenied) { throw 'Harness image lease allowed its approved root to be renamed.' }
    } finally {
      Close-HyperVImageLease $imageLease
      Remove-Item -LiteralPath $imageLeasePath -Force
      Remove-Item -LiteralPath $imageLeaseRoot -Force
    }
    Add-Result 'provider_source_self_tests' 'SKIP' 'Harness self-test intentionally did not execute unsigned provider source; release provider self-tests run only after pinned signature/digest staging.'
    Add-Result 'harness_self_test' 'PASS' 'Argument-array, UUID, output-bound, fixed-action, request binding, and locked-image pre-verification guards loaded successfully.'
  } else {
    Initialize-VerifiedProviderStage
    Invoke-ProviderSelfTests
  }
  if ($RunWslIdentity) { Invoke-WslIdentityAcceptance } else { Add-Result 'wsl_exact_guest_agent_identity' 'SKIP' 'Use -RunWslIdentity with an exact discovered distribution.' }
  if ($RunSandboxLaunch) { Start-SandboxAcceptance } else { Add-Result 'sandbox_exact_process_lifecycle' 'SKIP' 'Use -RunSandboxLaunch on an eligible signed Windows host.' }
  if ($RunHyperVLifecycle) { Invoke-HyperVLifecycleAcceptance } else { Add-Result 'hyperv_fixed_lifecycle' 'SKIP' 'Use -RunHyperVLifecycle with signed providers, a pinned image, elevation, and explicit destroy confirmation.' }
} catch {
  Add-Result 'acceptance_harness' 'FAIL' $_.Exception.Message
} finally {
  $reportPath = Join-Path $script:ArtifactDirectory 'environment-acceptance-results.json'
  [IO.File]::WriteAllText($reportPath, (($script:Results | ConvertTo-Json -Depth 6) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
  Write-Host "Acceptance report: $reportPath"
}

if (@($script:Results | Where-Object { $_.status -in @('FAIL', 'BLOCKED') }).Count -gt 0) { exit 1 }
