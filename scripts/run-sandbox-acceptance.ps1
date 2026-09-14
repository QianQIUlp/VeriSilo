# Orchestrator to launch and monitor Windows Sandbox acceptance
[CmdletBinding()]
param(
  [int]$BootTimeoutSeconds = 240,
  [int]$ExecutionTimeoutSeconds = 900
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Stop-AllSandboxProcesses {
  $procs = @(Get-Process -Name "ManagedWindowsVM","WindowsSandbox","*SandboxServer*","*SandboxClient*","*SandboxRemoteSession*" -ErrorAction SilentlyContinue)
  if ($procs.Count -gt 0) {
    Write-Host "[Host] Terminating $($procs.Count) existing Sandbox process(es)..."
    $procs | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 4
  }
}

# 1. Stop all sandbox processes FIRST
Stop-AllSandboxProcesses

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$sourcePackage = (Resolve-Path (Join-Path $repoRoot "artifacts/qa/dev-engine-package-521ae24")).Path
$sourceDriver = (Resolve-Path (Join-Path $repoRoot "artifacts/qa/sandbox-driver")).Path
$worktreeEvidence = Join-Path $repoRoot "artifacts/qa/sandbox-evidence"
$docsQaEvidence = Join-Path $repoRoot "docs/qa/packaged-host-sandbox-runtime-acceptance"

if (-not (Test-Path $sourcePackage)) { throw "Source package not found: $sourcePackage" }
if (-not (Test-Path $sourceDriver)) { throw "Source driver not found: $sourceDriver" }

# Clean staging directory with proper permissions
$qaStageRoot = "C:\Users\qiu\AppData\Local\Temp\verisilo-qa"
$stagePackage = Join-Path $qaStageRoot "package"
$stageDriver = Join-Path $qaStageRoot "driver"
$stageEvidence = Join-Path $qaStageRoot "evidence"

Write-Host "[Host] Synchronizing sandbox inputs to: $qaStageRoot"
New-Item -ItemType Directory -Path $stagePackage, $stageDriver, $stageEvidence -Force | Out-Null

Copy-Item "$sourcePackage\*" $stagePackage -Recurse -Force
Copy-Item "$sourceDriver\*" $stageDriver -Recurse -Force
Remove-Item "$stageEvidence\*" -Recurse -Force -ErrorAction SilentlyContinue

# Ensure ACL permissions for Windows Sandbox container
icacls "$qaStageRoot" /grant "Users:(OI)(CI)F" /grant "*S-1-15-2-1:(OI)(CI)F" /T /Q | Out-Null
Write-Host "[Host] ACL permissions applied to staging root."

if (Test-Path $worktreeEvidence) {
  Remove-Item -Recurse -Force $worktreeEvidence -ErrorAction SilentlyContinue
}
New-Item -ItemType Directory -Path $worktreeEvidence -Force | Out-Null

# 2. Generate WSB configuration
$wsbPath = Join-Path $qaStageRoot "sandbox-acceptance.wsb"
$wsbContent = @"
<Configuration>
  <VGpu>Default</VGpu>
  <Networking>Enable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>$stagePackage</HostFolder>
      <SandboxFolder>C:\Package</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
    <MappedFolder>
      <HostFolder>$stageDriver</HostFolder>
      <SandboxFolder>C:\Driver</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
    <MappedFolder>
      <HostFolder>$stageEvidence</HostFolder>
      <SandboxFolder>C:\Evidence</SandboxFolder>
      <ReadOnly>false</ReadOnly>
    </MappedFolder>
  </MappedFolders>
  <LogonCommand>
    <Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "for (`$i=0; `$i -lt 90; `$i++) { if (Test-Path C:\Driver\bootstrap.ps1) { powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\Driver\bootstrap.ps1; break }; Start-Sleep -Seconds 1 }"</Command>
  </LogonCommand>
</Configuration>
"@

[System.IO.File]::WriteAllText($wsbPath, $wsbContent, [System.Text.UTF8Encoding]::new($false))
Write-Host "[Host] Generated WSB descriptor: $wsbPath"

# 3. Launch Windows Sandbox
$sandboxExe = Join-Path $env:WINDIR "System32\WindowsSandbox.exe"
if (-not (Test-Path $sandboxExe)) { throw "WindowsSandbox.exe not found in System32" }

Write-Host "[Host] Launching Windows Sandbox..."
$proc = Start-Process -FilePath $sandboxExe -ArgumentList "`"$wsbPath`"" -PassThru

# 4. Wait for started.sentinel or bootstrap-started.txt (Boot phase)
$bootstrapStarted = Join-Path $stageEvidence "bootstrap-started.txt"
$startedSentinel = Join-Path $stageEvidence "started.sentinel"
$completedSentinel = Join-Path $stageEvidence "completed.sentinel"
$passSentinel = Join-Path $stageEvidence "pass.sentinel"
$failSentinel = Join-Path $stageEvidence "fail.sentinel"
$driverLog = Join-Path $stageEvidence "driver.log"

Write-Host "[Host] Awaiting Sandbox logon & driver start (timeout: ${BootTimeoutSeconds}s)..."
$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
while ($stopwatch.Elapsed.TotalSeconds -lt $BootTimeoutSeconds) {
  if (Test-Path $bootstrapStarted) {
    Write-Host "[Host] Bootstrap started sentinel detected after $($stopwatch.Elapsed.TotalSeconds.ToString('F1'))s"
  }
  if (Test-Path $startedSentinel) {
    Write-Host "[Host] Sandbox driver started after $($stopwatch.Elapsed.TotalSeconds.ToString('F1'))s"
    break
  }
  if (Test-Path $driverLog) {
    Write-Host "[Host] Driver log detected during boot phase..."
  }
  Start-Sleep -Seconds 2
}
if (-not (Test-Path $startedSentinel)) {
  if (Test-Path $driverLog) {
    Write-Host "[Host] Driver log content:"
    Get-Content $driverLog
  }
  Stop-AllSandboxProcesses
  throw "[Host] Timed out waiting for Sandbox driver to start (${BootTimeoutSeconds}s)"
}

# 5. Wait for completed.sentinel (Execution phase)
Write-Host "[Host] Awaiting Sandbox acceptance execution completion (timeout: ${ExecutionTimeoutSeconds}s)..."
$execStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
$lastLogLineCount = 0
while ($execStopwatch.Elapsed.TotalSeconds -lt $ExecutionTimeoutSeconds) {
  if (Test-Path $completedSentinel) {
    Write-Host "[Host] Sandbox acceptance execution completed after $($execStopwatch.Elapsed.TotalSeconds.ToString('F1'))s"
    break
  }
  if (Test-Path $driverLog) {
    $lines = @(Get-Content $driverLog -ErrorAction SilentlyContinue)
    if ($lines.Count -gt $lastLogLineCount) {
      for ($i = $lastLogLineCount; $i -lt $lines.Count; $i++) {
        Write-Host "  [Guest] $($lines[$i])"
      }
      $lastLogLineCount = $lines.Count
    }
  }
  Start-Sleep -Seconds 3
}

# Print any remaining log lines
if (Test-Path $driverLog) {
  $lines = @(Get-Content $driverLog -ErrorAction SilentlyContinue)
  if ($lines.Count -gt $lastLogLineCount) {
    for ($i = $lastLogLineCount; $i -lt $lines.Count; $i++) {
      Write-Host "  [Guest] $($lines[$i])"
    }
  }
}

if (-not (Test-Path $completedSentinel)) {
  Stop-AllSandboxProcesses
  throw "[Host] Timed out waiting for Sandbox acceptance execution (${ExecutionTimeoutSeconds}s)"
}

# 6. Check Pass/Fail
Start-Sleep -Seconds 2
$isPass = Test-Path $passSentinel
$isFail = Test-Path $failSentinel

Write-Host "[Host] Inspection: PassSentinel=$isPass, FailSentinel=$isFail"
$resultsFile = Join-Path $stageEvidence "results.json"
if (Test-Path $resultsFile) {
  $results = Get-Content $resultsFile -Raw -Encoding UTF8 | ConvertFrom-Json
  Write-Host "[Host] Acceptance Verdict: $($results.verdict)"
  Write-Host "[Host] Classification: $($results.classification)"
}

# Copy evidence to worktree artifacts/qa and docs/qa
Copy-Item "$stageEvidence\*" $worktreeEvidence -Recurse -Force
Write-Host "[Host] Copied safe evidence artifacts to: $worktreeEvidence"

New-Item -ItemType Directory -Path $docsQaEvidence -Force | Out-Null
Get-ChildItem -Path $docsQaEvidence -Exclude "STATUS.md" | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
Copy-Item "$stageEvidence\*" $docsQaEvidence -Recurse -Force
Write-Host "[Host] Copied safe evidence artifacts to: $docsQaEvidence"

# 7. Terminate Windows Sandbox processes cleanly
Write-Host "[Host] Cleaning up Windows Sandbox..."
Stop-AllSandboxProcesses

if ($isPass) {
  Write-Host "[Host] Windows Sandbox Acceptance PASSED SUCCESSFULLY!"
  exit 0
} else {
  Write-Host "[Host] Windows Sandbox Acceptance FAILED."
  exit 1
}
