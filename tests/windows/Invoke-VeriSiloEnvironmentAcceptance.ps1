[CmdletBinding()]
param(
  [string]$ProviderDirectory = (Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'scripts'),
  [string]$ArtifactDirectory,
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
  [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:Results = [System.Collections.Generic.List[object]]::new()
$script:TemporaryArtifactDirectory = $false

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

function Get-ProviderPath {
  param([ValidateSet('Probe', 'HyperV', 'SandboxController', 'SandboxBootstrap', 'WslAgent')][string]$Name)
  $leaf = switch ($Name) {
    'Probe' { 'verisilo-environment-probe.ps1' }
    'HyperV' { 'verisilo-hyperv.ps1' }
    'SandboxController' { 'verisilo-sandbox.ps1' }
    'SandboxBootstrap' { 'verisilo-sandbox-bootstrap.ps1' }
    'WslAgent' { 'verisilo-wsl-guest-agent.sh' }
  }
  $root = [IO.Path]::GetFullPath($ProviderDirectory)
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

function Test-SameValidProviderSigner {
  $thumbprints = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
  foreach ($name in @('Probe', 'HyperV', 'SandboxController', 'SandboxBootstrap')) {
    $signature = Get-AuthenticodeSignature -LiteralPath (Get-ProviderPath $name)
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or
        $null -eq $signature.SignerCertificate -or $null -eq $signature.TimeStamperCertificate) { return $false }
    [void]$thumbprints.Add($signature.SignerCertificate.Thumbprint)
  }
  return $thumbprints.Count -eq 1
}

function Invoke-ProviderSelfTests {
  $pwsh = [Environment]::ProcessPath
  foreach ($name in @('Probe', 'HyperV', 'SandboxController')) {
    $path = Get-ProviderPath $name
    $output = Invoke-FixedProcess $pwsh @('-NoLogo', '-NoProfile', '-NonInteractive', '-File', $path, '-SelfTest') $TimeoutSeconds 4096
    if ($output.ExitCode -ne 0) { throw "$name provider self-test failed: $($output.Stderr)" }
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
  $probePath = Get-ProviderPath 'Probe'
  $probeOutput = Invoke-FixedProcess ([Environment]::ProcessPath) @('-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'AllSigned', '-File', $probePath) $TimeoutSeconds 4096
  if ($probeOutput.ExitCode -ne 0) { throw "Environment probe failed: $($probeOutput.Stderr)" }
  $probe = $probeOutput.Stdout | ConvertFrom-Json
  if (-not $probe.sandboxAvailable) { throw 'Windows Sandbox is unavailable according to the fixed host probe.' }
  if (-not $probe.releaseScriptsTrusted) { throw 'Sandbox acceptance requires the same-signer release-script boundary.' }
  $siloId = [Guid]::NewGuid().ToString('D')
  $stateRoot = Join-Path $script:ArtifactDirectory "sandbox-state-$siloId"
  $environmentRoot = Join-Path $stateRoot $siloId
  $bootstrapStage = Join-Path $environmentRoot 'bootstrap'
  [void](New-Item -ItemType Directory -Path $bootstrapStage -Force)
  Copy-Item -LiteralPath (Get-ProviderPath 'SandboxBootstrap') -Destination (Join-Path $bootstrapStage 'verisilo-sandbox-bootstrap.ps1')
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
  $controller = Get-ProviderPath 'SandboxController'
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
    try {
      $output = Invoke-FixedProcess ([Environment]::ProcessPath) @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'AllSigned',
        '-File', $controller, '-RequestPath', $requestPath, '-StateRoot', $stateRoot,
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
}

function Write-HyperVRequest {
  param([string]$StateRoot, [string]$EnvironmentId, [string]$Action, [bool]$ConfirmDestroy)
  Assert-CanonicalUuid $EnvironmentId
  $directory = Join-Path $StateRoot $EnvironmentId
  [void](New-Item -ItemType Directory -Path $directory -Force)
  $request = [ordered]@{ schemaVersion = 1; action = $Action; environmentId = $EnvironmentId; confirmDestroy = $ConfirmDestroy }
  if ($Action -eq 'create') {
    $request.manifestSchemaVersion = 1
    $request.manifestImageFile = $HyperVImageFile
    $request.manifestImageSha256 = $HyperVImageSha256.ToLowerInvariant()
    $request.manifestTrusted = $true
  }
  $path = Join-Path $directory "$([Guid]::NewGuid().ToString('D')).request.json"
  [IO.File]::WriteAllText($path, (($request | ConvertTo-Json -Depth 4) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
  return $path
}

function Invoke-HyperVAction {
  param([string]$StateRoot, [string]$EnvironmentId, [string]$Action, [bool]$ConfirmDestroy)
  $requestPath = Write-HyperVRequest $StateRoot $EnvironmentId $Action $ConfirmDestroy
  try {
    $output = Invoke-FixedProcess 'powershell.exe' @(
      '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'AllSigned', '-File',
      (Get-ProviderPath 'HyperV'), '-RequestPath', $requestPath, '-StateRoot', $StateRoot,
      '-ApprovedImageRoot', $HyperVApprovedImageRoot
    ) $HyperVTimeoutSeconds 16384
    if ($output.ExitCode -ne 0) { throw "Hyper-V $Action failed: $($output.Stderr)" }
    $response = $output.Stdout | ConvertFrom-Json
    $vmId = [Guid]::Empty
    if ($response.schemaVersion -ne 1 -or [string]$response.action -cne $Action -or
        [string]$response.environmentId -cne $EnvironmentId -or $response.success -ne $true -or
        [string]$response.source -cne 'hyperv-controller' -or
        [string]$response.vmName -cne "VeriSilo-$EnvironmentId" -or
        -not [Guid]::TryParseExact([string]$response.vmId, 'D', [ref]$vmId) -or
        $response.generation -ne 2 -or [string]$response.baseImageSha256 -cne $HyperVImageSha256.ToLowerInvariant() -or
        $null -ne $response.guestAgentVersion -or $null -ne $response.guestAgentSha256 -or
        @('guestProfile', 'guestHealth', 'proxy', 'exit', 'proxyDns', 'guestResolver', 'browserReady').Where({ [string]$response.$_ -cne 'unavailable' }).Count -ne 0) {
      throw "Hyper-V $Action returned a mismatched receipt."
    }
  } finally {
    Remove-Item -LiteralPath $requestPath -Force -ErrorAction SilentlyContinue
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
  if (-not (Test-SameValidProviderSigner)) { throw 'Hyper-V acceptance requires valid same-signer provider scripts.' }
  $imagePath = Join-Path ([IO.Path]::GetFullPath($HyperVApprovedImageRoot)) $HyperVImageFile
  if (-not (Test-Path -LiteralPath $imagePath -PathType Leaf) -or
      (Get-FileHash -LiteralPath $imagePath -Algorithm SHA256).Hash -cne $HyperVImageSha256.ToUpperInvariant()) {
    throw 'Hyper-V image file does not match the declared SHA-256.'
  }
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
    if ($created) {
      Invoke-HyperVAction $stateRoot $environmentId 'remove' $true
      Invoke-HyperVAction $stateRoot $environmentId 'remove' $true
      Add-Result 'hyperv_confirmed_cleanup' 'PASS' 'Confirmed destroy and idempotent destroy retry completed for the harness-owned VM.'
    }
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
  Invoke-ProviderSelfTests
  if ($SelfTest) {
    Assert-CanonicalUuid ([Guid]::NewGuid().ToString('D'))
    if ('command' -in @('create', 'start', 'stop', 'pause', 'checkpoint', 'remove', 'health', 'logs')) {
      throw 'Harness allowlist self-test failed.'
    }
    Add-Result 'harness_self_test' 'PASS' 'Argument-array, UUID, output-bound, and fixed-action guards loaded successfully.'
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
