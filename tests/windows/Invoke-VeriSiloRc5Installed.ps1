[CmdletBinding()]
param(
  [string]$ReleaseDirectory,
  [string]$ExpectedSourceSha,
  [string]$EvidenceDirectory,
  [int]$TimeoutSeconds = 1200,
  [switch]$Guest,
  [switch]$Fixture,
  [string]$PortFile,
  [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-True([bool]$Condition, [string]$Message) {
  if (-not $Condition) { throw $Message }
}

function New-Token([int]$Bytes = 12) {
  $raw = New-Object byte[] $Bytes
  $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
  try { $rng.GetBytes($raw) } finally { $rng.Dispose() }
  return ([BitConverter]::ToString($raw)).Replace('-', '').ToLowerInvariant()
}

function Write-JsonFile([string]$Path, $Value) {
  $json = $Value | ConvertTo-Json -Depth 12
  [IO.File]::WriteAllText($Path, "$json`n", [Text.UTF8Encoding]::new($false))
}

function Invoke-Fixture([string]$ReadyFile) {
  $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
  $listener.Start()
  try {
    [IO.File]::WriteAllText($ReadyFile, [string]$listener.LocalEndpoint.Port)
    $html = @'
<!doctype html><title>RC5:WAIT</title><script>
const q = new URLSearchParams(location.search), v = q.get('v'), op = q.get('op');
const stored = localStorage.getItem('rc5-smoke');
const cookie = document.cookie.split('; ').includes('rc5-smoke=' + v);
let ok = false;
if (op === 'write') {
  localStorage.setItem('rc5-smoke', v);
  document.cookie = 'rc5-smoke=' + v + '; SameSite=Lax; path=/';
  ok = true;
} else if (op === 'empty') {
  ok = stored === null && !document.cookie.includes('rc5-smoke=');
} else if (op === 'read') {
  ok = stored === v && cookie;
}
document.title = 'RC5:' + (ok ? 'PASS' : 'FAIL') + ':' + op;
</script>
'@
    $body = [Text.Encoding]::UTF8.GetBytes($html)
    $head = [Text.Encoding]::ASCII.GetBytes("HTTP/1.1 200 OK`r`nContent-Type: text/html; charset=utf-8`r`nContent-Length: $($body.Length)`r`nConnection: close`r`n`r`n")
    while ($true) {
      $client = $listener.AcceptTcpClient()
      try {
        $client.ReceiveTimeout = 5000
        $stream = $client.GetStream()
        $buffer = New-Object byte[] 4096
        $null = $stream.Read($buffer, 0, $buffer.Length)
        $stream.Write($head, 0, $head.Length)
        $stream.Write($body, 0, $body.Length)
      } catch { } finally { $client.Dispose() }
    }
  } finally { $listener.Stop() }
}

function Start-Fixture([string]$ReadyFile) {
  $child = Start-Process -FilePath 'powershell.exe' -WindowStyle Hidden -PassThru `
    -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', ('"' + $PSCommandPath + '"'), '-Fixture', '-PortFile', ('"' + $ReadyFile + '"'))
  for ($i = 0; $i -lt 50; $i++) {
    if (Test-Path -LiteralPath $ReadyFile) {
      $text = [IO.File]::ReadAllText($ReadyFile)
      if ($text -match '^[0-9]{1,5}$') {
        $port = [int]$text
        Assert-True ($port -gt 0 -and $port -le 65535) 'Fixture port is invalid.'
        return @{ Process = $child; Port = $port }
      }
    }
    if ($child.HasExited) { break }
    Start-Sleep -Milliseconds 100
  }
  if (-not $child.HasExited) { Stop-Process -Id $child.Id -Force }
  throw 'Loopback fixture did not start.'
}

function Invoke-Cli([string]$Operation, [string[]]$CliArgs, [string]$StdinText = '', [switch]$ExpectFailure) {
  foreach ($arg in $CliArgs) {
    Assert-True ($arg -notmatch '[\s"]') "CLI argument for $Operation contains whitespace or a quote."
  }
  $psi = [Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = $script:CliPath
  $psi.Arguments = (@('--vault', $script:VaultName, '--json') + $CliArgs) -join ' '
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $process = [Diagnostics.Process]::new()
  $process.StartInfo = $psi
  Assert-True ($process.Start()) "CLI $Operation could not start."
  try {
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if ($StdinText.Length -gt 0) { $process.StandardInput.Write($StdinText) }
    $process.StandardInput.Close()
    if (-not $process.WaitForExit(180000)) {
      $process.Kill()
      throw "CLI $Operation timed out; check its exact state before any retry."
    }
    $null = $stderr.Result # Never put CLI stderr or a Vault secret in the evidence.
    if ($ExpectFailure) {
      Assert-True ($process.ExitCode -ne 0) "CLI $Operation unexpectedly succeeded."
      return $null
    }
    Assert-True ($process.ExitCode -eq 0) "CLI $Operation failed with exit $($process.ExitCode)."
    return ($stdout.Result | ConvertFrom-Json)
  } finally { $process.Dispose() }
}

function Invoke-Installer([string]$Path) {
  $process = Start-Process -FilePath $Path -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
  Assert-True ($process.ExitCode -eq 0) "Installer or uninstaller exited $($process.ExitCode)."
}

function Assert-NoProductProcesses {
  for ($i = 0; $i -lt 30; $i++) {
    $owned = @(Get-Process -Name 'verisilo', 'camoufox', 'camoufox-host', 'verisilo-camoufox-supervisor' -ErrorAction SilentlyContinue)
    if ($owned.Count -eq 0) { return }
    Start-Sleep -Milliseconds 500
  }
  throw 'Product processes remained after service stop.'
}

function Assert-Session([string]$Id, [string]$State) {
  $status = Invoke-Cli 'status' @('status', $Id)
  Assert-True ($status.activation.state -ceq $State) "Silo $Id did not reach $State."
  return $status
}

function Assert-Page([string]$Id, [string]$Action, [string]$Marker) {
  $url = "http://127.0.0.1:$($script:FixturePort)/?op=$Action&v=$Marker"
  $null = Invoke-Cli 'page goto' @('page', $Id, 'goto', $url)
  for ($i = 0; $i -lt 20; $i++) {
    $page = Invoke-Cli 'page snapshot' @('page', $Id, 'snapshot')
    if ($page.title -ceq "RC5:PASS:$Action") { return }
    if ($page.title -ceq "RC5:FAIL:$Action") { break }
    Start-Sleep -Milliseconds 500
  }
  throw "Silo $Id did not pass synthetic $Action page check."
}

function Invoke-Guest {
  $inputRoot = 'C:\Rc5Input'
  $outRoot = 'C:\Rc5Evidence'
  Assert-True (Test-Path -LiteralPath $outRoot -PathType Container) 'Mapped evidence directory is unavailable.'
  $binding = $null
  $installer = Join-Path $inputRoot 'VeriSilo-Managed-Browser-v0.1.0-rc5-x64-setup.exe'
  $installRoot = Join-Path $env:LOCALAPPDATA 'VeriSilo'
  $desktop = Join-Path $installRoot 'verisilo.exe'
  $script:CliPath = Join-Path $installRoot 'verisilo-cli.exe'
  $script:VaultName = "rc5-$(New-Token 6)"
  $vaultRoot = Join-Path $env:LOCALAPPDATA "io.verisilo.app\vaults\$($script:VaultName)"
  $steps = [ordered]@{}
  $stage = 'preflight'
  $result = [ordered]@{
    schema = 'verisilo-rc5-installed-smoke/v1'; verdict = 'inconclusive'
    candidate = $null; environment = [ordered]@{
      os = [Environment]::OSVersion.Version.ToString()
      architecture = $env:PROCESSOR_ARCHITECTURE
      accountIsAdministrator = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    }
    steps = $steps; startedAtUtc = [DateTime]::UtcNow.ToString('o')
  }
  $fixture = $null
  $serviceStarted = $false
  $passphrase = ''
  try {
    $binding = Get-Content -LiteralPath (Join-Path $inputRoot 'binding.json') -Raw | ConvertFrom-Json
    $result.candidate = $binding
    Assert-True ($binding.releaseVersion -ceq 'v0.1.0-rc5') 'Candidate version mismatch.'
    Assert-True ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $binding.installerSha256) 'Mapped installer hash mismatch.'
    Assert-True (-not (Test-Path -LiteralPath $installRoot)) 'Sandbox is not pristine: product installation exists.'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'io.verisilo.app'))) 'Sandbox is not pristine: product data exists.'
    $steps.preflight = 'PASS'

    $stage = 'install'
    Invoke-Installer $installer
    Assert-True ((Get-FileHash -LiteralPath $script:CliPath -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $binding.cliSha256) 'Installed CLI differs from candidate.'
    Assert-True ((Get-FileHash -LiteralPath $desktop -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $binding.desktopSha256) 'Installed Desktop differs from candidate.'
    $manifest = Join-Path $installRoot 'managed-browser\engine-package\engine-package.json'
    Assert-True ((Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $binding.packageManifestSha256) 'Installed Engine manifest differs from candidate.'
    $steps.install = 'PASS'

    $stage = 'two-managed-sessions'
    $passphrase = New-Token 32
    $null = Invoke-Cli 'status' @('status')
    $serviceStarted = $true
    $null = Invoke-Cli 'vault init' @('vault', 'init') "$passphrase`n$passphrase`n"
    $readyFile = Join-Path $env:TEMP "rc5-fixture-$(New-Token 5).port"
    $fixture = Start-Fixture $readyFile
    $script:FixturePort = $fixture.Port
    $a = Invoke-Cli 'create A' @('create', '--name', 'rc5-A', '--network', 'direct')
    $b = Invoke-Cli 'create B' @('create', '--name', 'rc5-B', '--network', 'direct')
    $c = Invoke-Cli 'create C' @('create', '--name', 'rc5-C', '--network', 'direct')
    Assert-True (@(@($a.id, $b.id, $c.id) | Select-Object -Unique).Count -eq 3) 'Created Silo IDs are not distinct.'
    Assert-True ($a.profileDirectory -cne $b.profileDirectory) 'A and B share a Profile.'
    $aFirst = Invoke-Cli 'start A' @('start', $a.id)
    Assert-True ($aFirst.state -ceq 'running' -and $aFirst.identityEvidence.state -ceq 'matched') 'A did not start with matched identity.'
    $markerA = New-Token 8
    $markerB = New-Token 8
    Assert-Page $a.id 'write' $markerA
    $bFirst = Invoke-Cli 'start B' @('start', $b.id)
    Assert-True ($bFirst.state -ceq 'running' -and $bFirst.identityEvidence.state -ceq 'matched') 'B did not start with matched identity.'
    Assert-Page $b.id 'empty' $markerA
    Assert-Page $b.id 'write' $markerB
    $sessions = Invoke-Cli 'sessions' @('sessions')
    Assert-True (@($sessions | Where-Object { $_.activation.state -ceq 'running' }).Count -eq 2) 'Two Managed sessions were not running.'
    $null = Invoke-Cli 'third start rejection' @('start', $c.id) '' -ExpectFailure
    $null = Assert-Session $a.id 'running'
    $null = Assert-Session $b.id 'running'
    Assert-Page $a.id 'read' $markerA
    Assert-Page $b.id 'read' $markerB
    $steps.twoManagedSessions = 'PASS'

    $stage = 'restart'
    $null = Invoke-Cli 'stop A' @('stop', $a.id)
    $null = Assert-Session $b.id 'running'
    Assert-Page $b.id 'read' $markerB
    $null = Invoke-Cli 'stop B' @('stop', $b.id)
    $null = Invoke-Cli 'service stop' @('service', 'stop')
    $serviceStarted = $false
    Assert-NoProductProcesses
    $vaultFile = Join-Path $vaultRoot 'vault.json'
    $vaultHash = (Get-FileHash -LiteralPath $vaultFile -Algorithm SHA256).Hash.ToLowerInvariant()
    $null = Invoke-Cli 'status after restart' @('status')
    $serviceStarted = $true
    $null = Invoke-Cli 'vault unlock' @('vault', 'unlock') "$passphrase`n"
    $null = Assert-Session $a.id 'stopped'
    $null = Assert-Session $b.id 'stopped'
    $steps.restart = 'PASS'

    $stage = 'repair-reinstall'
    $null = Invoke-Cli 'service stop before repair' @('service', 'stop')
    $serviceStarted = $false
    Assert-NoProductProcesses
    Invoke-Installer $installer
    Assert-True ((Get-FileHash -LiteralPath $vaultFile -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $vaultHash) 'Repair changed Vault bytes.'
    $null = Invoke-Cli 'status after repair' @('status')
    $serviceStarted = $true
    $null = Invoke-Cli 'unlock after repair' @('vault', 'unlock') "$passphrase`n"
    $null = Invoke-Cli 'start A after repair' @('start', $a.id)
    $null = Invoke-Cli 'start B after repair' @('start', $b.id)
    Assert-Page $a.id 'read' $markerA
    Assert-Page $b.id 'read' $markerB
    $steps.repairReinstall = 'PASS'

    $stage = 'uninstall-preserve-reinstall'
    $null = Invoke-Cli 'stop A before uninstall' @('stop', $a.id)
    $null = Invoke-Cli 'stop B before uninstall' @('stop', $b.id)
    $null = Invoke-Cli 'service stop before uninstall' @('service', 'stop')
    $serviceStarted = $false
    Assert-NoProductProcesses
    $vaultHash = (Get-FileHash -LiteralPath $vaultFile -Algorithm SHA256).Hash.ToLowerInvariant()
    Invoke-Installer (Join-Path $installRoot 'uninstall.exe')
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $installRoot 'verisilo.exe'))) 'Uninstall left Desktop binary.'
    Assert-True ((Get-FileHash -LiteralPath $vaultFile -Algorithm SHA256).Hash.ToLowerInvariant() -ceq $vaultHash) 'Uninstall changed Vault bytes.'
    $steps.uninstallPreservesData = 'PASS'
    Invoke-Installer $installer
    Assert-True (Test-Path -LiteralPath $script:CliPath -PathType Leaf) 'Reinstall did not restore CLI.'
    $null = Invoke-Cli 'status after reinstall' @('status')
    $serviceStarted = $true
    $null = Invoke-Cli 'unlock after reinstall' @('vault', 'unlock') "$passphrase`n"
    $null = Invoke-Cli 'start A after reinstall' @('start', $a.id)
    $null = Invoke-Cli 'start B after reinstall' @('start', $b.id)
    Assert-Page $a.id 'read' $markerA
    Assert-Page $b.id 'read' $markerB
    $steps.reinstallReopens = 'PASS'
    $result.verdict = 'passed'
  } catch {
    $result.verdict = 'failed'
    $result.failedStage = $stage
    $message = $_.Exception.Message
    if ($passphrase.Length -gt 0) { $message = $message.Replace($passphrase, '[redacted]') }
    $result.failure = $message
  } finally {
    if ($serviceStarted) {
      try {
        foreach ($session in @(Invoke-Cli 'cleanup sessions' @('sessions'))) {
          if ($session.activation.state -ceq 'running') { $null = Invoke-Cli 'cleanup stop' @('stop', $session.siloId) }
        }
        $null = Invoke-Cli 'cleanup service stop' @('service', 'stop')
      } catch {
        $result.cleanupFailure = 'Service cleanup failed.'
        $result.verdict = 'failed'
      }
    }
    if ($fixture -and -not $fixture.Process.HasExited) { Stop-Process -Id $fixture.Process.Id -Force }
    try { Assert-NoProductProcesses; $steps.processCleanup = 'PASS' } catch { $steps.processCleanup = 'FAIL'; $result.verdict = 'failed' }
    $result.finishedAtUtc = [DateTime]::UtcNow.ToString('o')
    $temporary = Join-Path $outRoot 'result.json.tmp'
    Write-JsonFile $temporary $result
    Move-Item -LiteralPath $temporary -Destination (Join-Path $outRoot 'result.json')
  }
}

function Invoke-Host {
  Assert-True ($ExpectedSourceSha -match '^[0-9a-f]{40}$') 'Pass the full frozen source SHA.'
  Assert-True (Test-Path -LiteralPath $ReleaseDirectory -PathType Container) 'Release directory is missing.'
  $release = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
  $provenance = Get-Content -LiteralPath (Join-Path $release 'provenance.json') -Raw | ConvertFrom-Json
  $acceptance = Get-Content -LiteralPath (Join-Path $release 'windows-acceptance-report.json') -Raw | ConvertFrom-Json
  Assert-True ($provenance.source.revision -ceq $ExpectedSourceSha -and $provenance.source.dirty -eq $false) 'Release provenance does not bind clean frozen source.'
  Assert-True ($acceptance.release -ceq 'v0.1.0-rc5' -and $acceptance.status -ceq 'Pending') 'Full legacy acceptance report must remain Pending for this focused smoke.'
  $installerName = 'VeriSilo-Managed-Browser-v0.1.0-rc5-x64-setup.exe'
  $installer = Join-Path $release $installerName
  $desktop = Join-Path $release 'verisilo.exe'
  $cli = Join-Path $release 'verisilo-cli.exe'
  $manifest = Join-Path $release 'engine-package\engine-package.json'
  $installerHash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
  $desktopHash = (Get-FileHash -LiteralPath $desktop -Algorithm SHA256).Hash.ToLowerInvariant()
  $cliHash = (Get-FileHash -LiteralPath $cli -Algorithm SHA256).Hash.ToLowerInvariant()
  $manifestHash = (Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant()
  $sums = Get-Content -LiteralPath (Join-Path $release 'SHA256SUMS')
  Assert-True ($sums -ccontains "$installerHash  $installerName") 'Installer hash is absent from SHA256SUMS.'
  Assert-True ($sums -ccontains "$desktopHash  verisilo.exe") 'Desktop hash is absent from SHA256SUMS.'
  Assert-True ($sums -ccontains "$cliHash  verisilo-cli.exe") 'CLI hash is absent from SHA256SUMS.'
  Assert-True ($sums -ccontains "$manifestHash  engine-package/engine-package.json") 'Engine manifest hash is absent from SHA256SUMS.'
  $sandboxExe = Join-Path $env:WINDIR 'System32\WindowsSandbox.exe'
  Assert-True (Test-Path -LiteralPath $sandboxExe -PathType Leaf) 'Windows Sandbox is unavailable.'
  $existing = @(Get-Process -Name 'WindowsSandbox', 'ManagedWindowsVM', '*SandboxServer*', '*SandboxClient*', '*SandboxRemoteSession*' -ErrorAction SilentlyContinue)
  Assert-True ($existing.Count -eq 0) 'An existing Windows Sandbox is running; this check will not disturb it.'
  $runId = New-Token 6
  $stageRoot = Join-Path $env:TEMP "verisilo-rc5-installed-$runId"
  $inputRoot = Join-Path $stageRoot 'input'
  $mappedEvidence = Join-Path $stageRoot 'evidence'
  New-Item -ItemType Directory -Path $inputRoot, $mappedEvidence -Force | Out-Null
  Copy-Item -LiteralPath $installer -Destination $inputRoot
  Copy-Item -LiteralPath $PSCommandPath -Destination $inputRoot
  $binding = [ordered]@{
    releaseVersion = 'v0.1.0-rc5'; sourceSha = $ExpectedSourceSha
    installerSha256 = $installerHash; desktopSha256 = $desktopHash; cliSha256 = $cliHash
    packageManifestSha256 = $manifestHash
  }
  Write-JsonFile (Join-Path $inputRoot 'binding.json') $binding
  if (-not $EvidenceDirectory) {
    $EvidenceDirectory = Join-Path (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path "artifacts\qa\rc5-installed-$runId"
  }
  Assert-True (-not (Test-Path -LiteralPath $EvidenceDirectory)) 'Evidence directory already exists.'
  New-Item -ItemType Directory -Path $EvidenceDirectory | Out-Null
  $inputXml = [Security.SecurityElement]::Escape($inputRoot)
  $evidenceXml = [Security.SecurityElement]::Escape($mappedEvidence)
  $wsb = Join-Path $stageRoot 'rc5-installed.wsb'
  [IO.File]::WriteAllText($wsb, @"
<Configuration>
  <Networking>Enable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MappedFolders>
    <MappedFolder><HostFolder>$inputXml</HostFolder><SandboxFolder>C:\Rc5Input</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$evidenceXml</HostFolder><SandboxFolder>C:\Rc5Evidence</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
  <LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\Rc5Input\Invoke-VeriSiloRc5Installed.ps1 -Guest</Command></LogonCommand>
</Configuration>
"@, [Text.UTF8Encoding]::new($false))
  $sandbox = $null
  try {
    $sandbox = Start-Process -FilePath $sandboxExe -ArgumentList ('"' + $wsb + '"') -PassThru
    $resultFile = Join-Path $mappedEvidence 'result.json'
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline -and -not (Test-Path -LiteralPath $resultFile)) {
      Start-Sleep -Seconds 2
    }
    if (Test-Path -LiteralPath $resultFile) {
      Copy-Item -LiteralPath $resultFile -Destination (Join-Path $EvidenceDirectory 'result.json')
      $result = Get-Content -LiteralPath $resultFile -Raw | ConvertFrom-Json
      Assert-True ($result.verdict -ceq 'passed') "RC5 installed smoke $($result.verdict); see $EvidenceDirectory."
      Write-Output "RC5 installed smoke passed: $EvidenceDirectory"
    } else {
      Write-JsonFile (Join-Path $EvidenceDirectory 'host-result.json') ([ordered]@{
        schema = 'verisilo-rc5-installed-host/v1'; verdict = 'blocked'
        reason = 'Sandbox did not return result.json within the deadline.'
        candidate = $binding; stagedAt = $stageRoot
      })
      throw "Sandbox did not return evidence; see $EvidenceDirectory."
    }
  } finally {
    if ($sandbox -and -not $sandbox.HasExited) { Stop-Process -Id $sandbox.Id -Force }
  }
}

if ($Fixture) {
  Assert-True ([bool]$PortFile) 'Fixture requires -PortFile.'
  Invoke-Fixture $PortFile
} elseif ($SelfTest) {
  $token = New-Token 8
  Assert-True ($token -match '^[0-9a-f]{16}$') 'Random token self-test failed.'
  $portFile = Join-Path $env:TEMP "rc5-selftest-$(New-Token 4).port"
  $fixtureInfo = Start-Fixture $portFile
  try {
    $html = (Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$($fixtureInfo.Port)/?op=empty&v=test").Content
    Assert-True ($html.Contains("document.title = 'RC5:'") -and $html.Contains("localStorage.getItem('rc5-smoke')")) 'Loopback fixture self-test failed.'
  } finally {
    if (-not $fixtureInfo.Process.HasExited) { Stop-Process -Id $fixtureInfo.Process.Id -Force }
    Remove-Item -LiteralPath $portFile -ErrorAction SilentlyContinue
  }
  Write-Output 'RC5 installed harness self-test passed (no Sandbox or installer launched).'
} elseif ($Guest) {
  Invoke-Guest
} else {
  Invoke-Host
}
