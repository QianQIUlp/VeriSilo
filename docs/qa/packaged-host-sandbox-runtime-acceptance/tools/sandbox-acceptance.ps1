# Standalone PowerShell Acceptance Driver for Packaged Camoufox Host in Windows Sandbox
[CmdletBinding()]
param(
  [string]$PackageRoot,
  [string]$OutputDir,
  [string]$DriverDir,
  [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Enable interactive launch mode to trigger window geometry coherence and stale coordinate clamping
$env:VERISILO_INTERACTIVE = '1'
$global:AcceptanceLogFile = $null

function Write-Step {
  param([string]$Message)
  $timestamp = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ssZ")
  $formatted = "[$timestamp] $Message"
  Write-Host $formatted
  if ($global:AcceptanceLogFile) {
    try {
      [System.IO.File]::AppendAllText($global:AcceptanceLogFile, "$formatted`r`n", [System.Text.Encoding]::UTF8)
    } catch {}
  }
}

function Invoke-HostProvision {
  param(
    [string]$HostExe,
    [string]$PackageRoot,
    [string]$ArtifactRoot,
    [string]$ProfileRoot,
    [string]$StateRoot,
    [hashtable]$RequestPayload
  )
  $json = ($RequestPayload | ConvertTo-Json -Compress)
  $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
  $reqBytes = $utf8NoBom.GetBytes($json)
  $lenBytes = [System.BitConverter]::GetBytes([int]$reqBytes.Length)
  if ([System.BitConverter]::IsLittleEndian) {
    [System.Array]::Reverse($lenBytes)
  }

  $frameBytes = [byte[]]::new(4 + $reqBytes.Length)
  [System.Array]::Copy($lenBytes, 0, $frameBytes, 0, 4)
  [System.Array]::Copy($reqBytes, 0, $frameBytes, 4, $reqBytes.Length)

  $stageTemp = Join-Path $StateRoot ("prov-stage-" + [Guid]::NewGuid().ToString("n"))
  New-Item -ItemType Directory -Path $stageTemp -Force | Out-Null
  $inBin = Join-Path $stageTemp "frame-in.bin"
  $outBin = Join-Path $stageTemp "frame-out.bin"
  $errLog = Join-Path $stageTemp "frame-err.log"

  [System.IO.File]::WriteAllBytes($inBin, $frameBytes)
  Write-Step "Invoke-HostProvision: staged payload ($($reqBytes.Length) bytes, lenBytes: $([BitConverter]::ToString($lenBytes)))"

  $argsStr = "--package-root `"$PackageRoot`" --artifact-root `"$ArtifactRoot`" --profile-root `"$ProfileRoot`" --state-root `"$StateRoot`" --provision-artifact"
  $cmdLine = "`"$HostExe`" $argsStr < `"$inBin`" > `"$outBin`" 2> `"$errLog`""
  cmd.exe /c $cmdLine
  $exitCode = $LASTEXITCODE

  $stderr = ""
  if (Test-Path $errLog) {
    $stderr = [System.IO.File]::ReadAllText($errLog, [System.Text.Encoding]::UTF8)
  }

  if (-not (Test-Path $outBin)) {
    throw "Provision host produced no output file. ExitCode: $exitCode. Stderr: $stderr"
  }

  $resRaw = [System.IO.File]::ReadAllBytes($outBin)
  if ($resRaw.Length -lt 4) {
    throw "Provision host output is less than 4 bytes ($($resRaw.Length) bytes). ExitCode: $exitCode. Stderr: $stderr"
  }

  $resLenBytes = [byte[]]::new(4)
  [System.Array]::Copy($resRaw, 0, $resLenBytes, 0, 4)
  if ([System.BitConverter]::IsLittleEndian) {
    [System.Array]::Reverse($resLenBytes)
  }
  $resLen = [System.BitConverter]::ToInt32($resLenBytes, 0)

  if ($resRaw.Length -lt (4 + $resLen)) {
    throw "Provision host output ($($resRaw.Length) bytes) is less than expected (4 + $resLen bytes). ExitCode: $exitCode. Stderr: $stderr"
  }

  $resJson = [System.Text.Encoding]::UTF8.GetString($resRaw, 4, $resLen)
  if ($exitCode -ne 0) {
    throw "Provision host exited with code $exitCode. Stderr: $stderr. Response: $resJson"
  }

  Remove-Item -Recurse -Force $stageTemp -ErrorAction SilentlyContinue

  return ($resJson | ConvertFrom-Json)
}

class HostSession : System.IDisposable {
  [System.Diagnostics.Process]$Process
  [System.IO.Pipes.NamedPipeServerStream]$PipeIn
  [System.IO.Pipes.NamedPipeServerStream]$PipeOut
  [System.IO.StreamReader]$Reader
  [string]$Label
  [int]$MsgId = 0

  HostSession([string]$hostExe, [string]$packageRoot, [string]$artifactRoot, [string]$profileRoot, [string]$stateRoot, [string]$label) {
    $this.Label = $label
    $guid = [Guid]::NewGuid().ToString("n")
    $pipeNameIn = "vs_host_in_${guid}"
    $pipeNameOut = "vs_host_out_${guid}"

    $this.PipeIn = [System.IO.Pipes.NamedPipeServerStream]::new($pipeNameIn, [System.IO.Pipes.PipeDirection]::Out, 1, [System.IO.Pipes.PipeTransmissionMode]::Byte, [System.IO.Pipes.PipeOptions]::Asynchronous)
    $this.PipeOut = [System.IO.Pipes.NamedPipeServerStream]::new($pipeNameOut, [System.IO.Pipes.PipeDirection]::In, 1, [System.IO.Pipes.PipeTransmissionMode]::Byte, [System.IO.Pipes.PipeOptions]::Asynchronous)

    $errLog = Join-Path $stateRoot "host-stderr.log"
    $argsStr = "--package-root `"$packageRoot`" --artifact-root `"$artifactRoot`" --profile-root `"$profileRoot`" --state-root `"$stateRoot`""
    $cmdLine = "`"$hostExe`" $argsStr < `\\.\pipe\$pipeNameIn > `\\.\pipe\$pipeNameOut"

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cmd.exe"
    $psi.Arguments = "/c `"$cmdLine`""
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.EnvironmentVariables["VERISILO_INTERACTIVE"] = "1"

    $initialLogLen = [long]0
    if (Test-Path $errLog) {
      try { $initialLogLen = (Get-Item $errLog).Length } catch {}
    }

    $this.Process = [System.Diagnostics.Process]::Start($psi)

    $this.PipeIn.WaitForConnection()
    $this.PipeOut.WaitForConnection()
    $this.Reader = [System.IO.StreamReader]::new($this.PipeOut, [System.Text.UTF8Encoding]::new($false))

    # Wait for Host to initialize and reader thread to start
    $swWatch = [System.Diagnostics.Stopwatch]::StartNew()
    $ready = $false
    while ($swWatch.Elapsed.TotalSeconds -lt 45) {
      if ($this.Process.HasExited) {
        throw "Host process exited prematurely with code $($this.Process.ExitCode)."
      }
      if (Test-Path $errLog) {
        try {
          $fs = [System.IO.FileStream]::new($errLog, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
          if ($fs.Length -gt $initialLogLen) {
            $fs.Seek($initialLogLen, [System.IO.SeekOrigin]::Begin) | Out-Null
            $sr = [System.IO.StreamReader]::new($fs, [System.Text.Encoding]::UTF8)
            $newContent = $sr.ReadToEnd()
            $fs.Close()
            if ($newContent -like "*host: reader thread started*") {
              $ready = $true
              break
            }
          } else {
            $fs.Close()
          }
        } catch {}
      }
      Start-Sleep -Milliseconds 200
    }
    if (-not $ready) {
      Write-Step "HostSession [$($this.Label)] Warning: reader thread started not detected within $($swWatch.Elapsed.TotalSeconds.ToString('F1'))s, proceeding anyway..."
    } else {
      Write-Step "HostSession [$($this.Label)] Host ready in $($swWatch.Elapsed.TotalSeconds.ToString('F1'))s (reader thread active)"
    }
  }

  [object]Request([string]$command, [hashtable]$params) {
    $this.MsgId++
    $reqId = "msg-$($this.Label)-$($this.MsgId)"
    $reqObj = [ordered]@{
      id = $reqId
      command = $command
      params = if ($null -eq $params) { @{} } else { $params }
    }
    $line = (ConvertTo-Json -Compress -InputObject $reqObj)
    Write-Step "HostSession [$($this.Label)] send: $line"
    $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
    $bytes = $utf8NoBom.GetBytes("$line`n")
    $this.PipeIn.Write($bytes, 0, $bytes.Length)
    $this.PipeIn.Flush()

    $respLine = $this.Reader.ReadLine()
    if ($null -eq $respLine) {
      throw "Host process unexpectedly closed stdout while awaiting response for $command."
    }
    Write-Step "HostSession [$($this.Label)] recv: $respLine"
    $resp = ($respLine | ConvertFrom-Json)
    if ($resp.id -ne $reqId) {
      throw "Mismatched response id. Expected $reqId, got $($resp.id). Full response: $respLine"
    }
    return $resp
  }

  [void]Dispose() {
    try {
      if (-not $this.Process.HasExited) {
        try { $this.PipeIn.Close() } catch {}
        $this.Process.WaitForExit(10000)
        if (-not $this.Process.HasExited) {
          $this.Process.Kill()
        }
      }
    } catch {}
    try { $this.PipeIn.Dispose() } catch {}
    try { $this.PipeOut.Dispose() } catch {}
    try { $this.Reader.Dispose() } catch {}
    try { $this.Process.Dispose() } catch {}
  }
}

$GEOMETRY_EVAL_SCRIPT = @"
() => ({
  outerWidth: window.outerWidth, outerHeight: window.outerHeight,
  innerWidth: window.innerWidth, innerHeight: window.innerHeight,
  screenX: window.screenX, screenY: window.screenY,
  screenLeft: window.screenLeft, screenTop: window.screenTop,
  devicePixelRatio: window.devicePixelRatio,
  screen: {
    width: screen.width, height: screen.height,
    availWidth: screen.availWidth, availHeight: screen.availHeight,
    availLeft: screen.availLeft, availTop: screen.availTop,
    colorDepth: screen.colorDepth, pixelDepth: screen.pixelDepth
  },
  url: location.href
})
"@

function Test-GeometryInvariants {
  param([object]$obs)
  $screen = $obs.screen
  $invariants = [ordered]@{
    screenX_equals_screenLeft = ($obs.screenX -eq $obs.screenLeft)
    screenY_equals_screenTop = ($obs.screenY -eq $obs.screenTop)
    availLeft_le_screenX = ($screen.availLeft -le $obs.screenX)
    screenX_outerWidth_le_avail = (($obs.screenX + $obs.outerWidth) -le ($screen.availLeft + $screen.availWidth))
    availTop_le_screenY = ($screen.availTop -le $obs.screenY)
    screenY_outerHeight_le_avail = (($obs.screenY + $obs.outerHeight) -le ($screen.availTop + $screen.availHeight))
  }
  $allPass = $true
  foreach ($k in $invariants.Keys) {
    if (-not $invariants[$k]) { $allPass = $false }
  }
  return [pscustomobject]@{
    allPass = $allPass
    invariants = $invariants
  }
}

function Find-Signal {
  param([object]$evidence, [string]$signalName)
  if ($null -eq $evidence -or $null -eq $evidence.signals) { return $null }
  foreach ($s in $evidence.signals) {
    if ($s.signal -eq $signalName) { return $s }
  }
  return $null
}

if ($SelfTest) {
  Write-Step "Running PowerShell Driver Framing & Stdio Self-Test..."
  $hostExe = Join-Path $PackageRoot "host\camoufox-host.exe"
  if (-not (Test-Path $hostExe)) { throw "Host executable not found: $hostExe" }

  $tempSelf = Join-Path $env:TEMP "verisilo-selftest-$([Guid]::NewGuid().ToString('n'))"
  $artRoot = Join-Path $tempSelf "identity"
  $profRoot = Join-Path $tempSelf "profiles"
  $stateRoot = Join-Path $tempSelf "state"
  New-Item -ItemType Directory -Path $artRoot, $profRoot, $stateRoot -Force | Out-Null

  try {
    # 1. Test binary provision
    Write-Step "Testing binary length-prefixed provision-artifact..."
    $seedBytes = [byte[]]::new(32)
    for ($i = 0; $i -lt 32; $i++) { $seedBytes[$i] = [byte]$i }
    $seedB64 = [Convert]::ToBase64String($seedBytes)

    $req = @{
      seed = $seedB64
      preset = "balanced-en-us"
      window = @(1280, 800)
    }
    $provRes = Invoke-HostProvision -HostExe $hostExe -PackageRoot $PackageRoot -ArtifactRoot $artRoot -ProfileRoot $profRoot -StateRoot $stateRoot -RequestPayload $req
    if ($provRes.ok -ne $true) { throw "Provision self-test failed: $($provRes | ConvertTo-Json)" }
    $artId = $provRes.result.artifactId
    Write-Step "Provision self-test passed: $artId"

    # 2. Test JSONL Host stdio hello & shutdown
    Write-Step "Testing JSONL Host stdio hello & shutdown..."
    $session = [HostSession]::new($hostExe, $PackageRoot, $artRoot, $profRoot, $stateRoot, "selftest")
    try {
      $hello = $session.Request("hello", @{})
      if ($hello.ok -ne $true -or $hello.result.protocol -ne "verisilo-camoufox-host/v1") {
        throw "Hello self-test failed: $($hello | ConvertTo-Json)"
      }
      Write-Step "Hello self-test passed (protocol: $($hello.result.protocol), release: $($hello.result.browserRelease))"

      $shutdown = $session.Request("shutdown", @{})
      if ($shutdown.ok -ne $true -or $shutdown.result.state -ne "shutdown") {
        throw "Shutdown self-test failed: $($shutdown | ConvertTo-Json)"
      }
      Write-Step "Shutdown self-test passed (state: $($shutdown.result.state))"
    } finally {
      $session.Dispose()
    }
    Write-Step "All Driver Self-Tests PASSED!"
    exit 0
  } finally {
    Remove-Item -Recurse -Force $tempSelf -ErrorAction SilentlyContinue
  }
}

# --- Full Acceptance Mode ---
if ([string]::IsNullOrWhiteSpace($PackageRoot) -or [string]::IsNullOrWhiteSpace($OutputDir) -or [string]::IsNullOrWhiteSpace($DriverDir)) {
  throw "Missing required arguments: -PackageRoot, -OutputDir, -DriverDir"
}

$hostExe = Join-Path $PackageRoot "host\camoufox-host.exe"
if (-not (Test-Path $hostExe)) { throw "Host executable not found: $hostExe" }

# Ensure OutputDir exists and clean sentinel files to guarantee bounded fresh run
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
$global:AcceptanceLogFile = Join-Path $OutputDir "driver.log"
Remove-Item (Join-Path $OutputDir "*.sentinel") -Force -ErrorAction SilentlyContinue
Set-Content -Path (Join-Path $OutputDir "started.sentinel") -Value ([DateTime]::UtcNow.ToString("o")) -Encoding UTF8

Write-Step "Packaged-Host Windows Sandbox Runtime Acceptance Starting..."
Write-Step "PackageRoot: $PackageRoot"
Write-Step "OutputDir: $OutputDir"
Write-Step "DriverDir: $DriverDir"

# 1. Environment Info
$os = Get-CimInstance Win32_OperatingSystem
$envInfo = [ordered]@{
  caption = $os.Caption
  version = $os.Version
  buildNumber = $os.BuildNumber
  osArchitecture = $os.OSArchitecture
  csName = $os.CSName
  windowsDirectory = $os.WindowsDirectory
  recordedAtUtc = [DateTime]::UtcNow.ToString("o")
}
# Also query UBR from registry
try {
  $reg = Get-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion" -ErrorAction Stop
  $envInfo["ubr"] = $reg.UBR
  $envInfo["displayVersion"] = $reg.DisplayVersion
  $envInfo["currentBuild"] = $reg.CurrentBuild
} catch {
  $envInfo["ubr"] = $null
}
$envJson = ($envInfo | ConvertTo-Json -Depth 4)
[System.IO.File]::WriteAllText((Join-Path $OutputDir "environment.json"), $envJson, [System.Text.UTF8Encoding]::new($false))
Write-Step "Environment recorded: $($envInfo.caption) Build $($envInfo.buildNumber) UBR $($envInfo.ubr)"

# Copy package provenance if present in DriverDir
$provSource = Join-Path $DriverDir "package-provenance.json"
if (Test-Path $provSource) {
  Copy-Item $provSource (Join-Path $OutputDir "package-provenance.json") -Force
}

# 2. Setup Sandbox-Local Temporary Directories
$sandboxTemp = Join-Path $env:TEMP "verisilo-acceptance-$([Guid]::NewGuid().ToString('n'))"
$localArtRoot = Join-Path $sandboxTemp "identity"
$localProfRoot = Join-Path $sandboxTemp "profiles"
$localStateRoot = Join-Path $sandboxTemp "state"
New-Item -ItemType Directory -Path $localArtRoot, $localProfRoot, $localStateRoot -Force | Out-Null
Write-Step "Sandbox-local temp directory created: $sandboxTemp"

$testResults = [ordered]@{
  verdict = "INCONCLUSIVE"
  evidenceCoverageII = [ordered]@{}
  freshRecheck = [ordered]@{}
  windowGeometry = [ordered]@{}
  legacyCompatibility = [ordered]@{}
  coldRestart = [ordered]@{}
  cleanup = [ordered]@{}
}

try {
  # 3. Provision Production Artifact: 1280x800
  Write-Step "Provisioning Production Artifact (1280x800, balanced-en-us, fontMode=managed)..."
  $seed1280 = [byte[]]::new(32)
  [System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($seed1280)
  $provReq1280 = @{
    seed = [Convert]::ToBase64String($seed1280)
    preset = "balanced-en-us"
    window = @(1280, 800)
  }
  $res1280 = Invoke-HostProvision -HostExe $hostExe -PackageRoot $PackageRoot -ArtifactRoot $localArtRoot -ProfileRoot $localProfRoot -StateRoot $localStateRoot -RequestPayload $provReq1280
  if ($res1280.ok -ne $true) { throw "Provisioning 1280x800 failed: $($res1280 | ConvertTo-Json)" }
  $artId1280 = $res1280.result.artifactId
  $artSha1280 = $res1280.result.artifactFileSha256
  $artFile1280 = Join-Path $localArtRoot "$artId1280.json"
  $artData1280 = (Get-Content $artFile1280 -Raw -Encoding UTF8 | ConvertFrom-Json)
  if ($artData1280.policy.fontMode -ne "managed") {
    throw "Provisioned artifact fontMode is not 'managed': $($artData1280.policy.fontMode)"
  }
  Write-Step "Provisioned 1280x800 artifact: $artId1280 (fontMode=managed)"

  # 4. Provision Second Size Artifact: 1024x768
  Write-Step "Provisioning Second Size Artifact (1024x768, balanced-en-us)..."
  $seed1024 = [byte[]]::new(32)
  [System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($seed1024)
  $provReq1024 = @{
    seed = [Convert]::ToBase64String($seed1024)
    preset = "balanced-en-us"
    window = @(1024, 768)
  }
  $res1024 = Invoke-HostProvision -HostExe $hostExe -PackageRoot $PackageRoot -ArtifactRoot $localArtRoot -ProfileRoot $localProfRoot -StateRoot $localStateRoot -RequestPayload $provReq1024
  if ($res1024.ok -ne $true) { throw "Provisioning 1024x768 failed: $($res1024 | ConvertTo-Json)" }
  $artId1024 = $res1024.result.artifactId
  $artSha1024 = $res1024.result.artifactFileSha256
  Write-Step "Provisioned 1024x768 artifact: $artId1024"

  # 5. Stage Pre-generated Legacy Artifact
  Write-Step "Staging Pre-generated Legacy Artifact from DriverDir..."
  $legacyMetaFile = Join-Path $DriverDir "legacy\legacy-meta.json"
  if (-not (Test-Path $legacyMetaFile)) { throw "Legacy meta file missing: $legacyMetaFile" }
  $legacyMeta = (Get-Content $legacyMetaFile -Raw -Encoding UTF8 | ConvertFrom-Json)
  $legacyArtId = $legacyMeta.artifactId
  $legacySrcJson = Join-Path $DriverDir "legacy\$legacyArtId.json"
  $legacySrcSha = Join-Path $DriverDir "legacy\$legacyArtId.json.sha256"

  $legacyDstJson = Join-Path $localArtRoot "$legacyArtId.json"
  $legacyDstSha = Join-Path $localArtRoot "$legacyArtId.json.sha256"
  Copy-Item $legacySrcJson $legacyDstJson -Force
  Copy-Item $legacySrcSha $legacyDstSha -Force

  $legacyShaBefore = (Get-FileHash $legacyDstJson -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($legacyShaBefore -ne $legacyMeta.expectedSha256.ToLowerInvariant()) {
    throw "Staged legacy artifact hash ($legacyShaBefore) does not match expected ($($legacyMeta.expectedSha256))"
  }
  Write-Step "Staged legacy artifact $legacyArtId (SHA256: $legacyShaBefore, injectedScreenX: $($legacyMeta.injectedScreenX), injectedScreenY: $($legacyMeta.injectedScreenY))"

  # --- Session 1: 1280x800 Primary Acceptance (Evidence Coverage II + Fresh Recheck + Geometry) ---
  Write-Step "=== SESSION 1: 1280x800 Launch & Acceptance ==="
  $session1 = [HostSession]::new($hostExe, $PackageRoot, $localArtRoot, $localProfRoot, $localStateRoot, "sess1-1280")
  try {
    $hello = $session1.Request("hello", @{})
    if ($hello.ok -ne $true) { throw "Hello failed in Session 1" }

    Write-Step "Launching Session 1 with artifact $artId1280..."
    $launch = $session1.Request("launch", @{
      artifactId = $artId1280
      profileId = "profile-1280-a"
      expectedArtifactFileSha256 = $artSha1280
    })
    if ($launch.ok -ne $true) { throw "Launch failed in Session 1: $($launch | ConvertTo-Json)" }
    $sessionId1 = $launch.result.sessionId
    $launchState1 = $launch.result.state
    $evidence1 = $launch.result.identityEvidence

    Write-Step "Session 1 launched successfully: sessionId=$sessionId1, state=$launchState1"

    # Evidence Coverage II Validations
    $sigVendor = Find-Signal $evidence1 "webgl2Vendor"
    $sigRenderer = Find-Signal $evidence1 "webgl2Renderer"
    $sigEncoding = Find-Signal $evidence1 "acceptEncoding"

    if ($null -eq $sigVendor -or $null -eq $sigRenderer -or $null -eq $sigEncoding) {
      throw "Evidence Coverage II signals missing in launch identityEvidence"
    }

    $rendererUnavailableCorrect = (($sigRenderer.observed -like "*, or similar" -or $sigRenderer.expected -like "*, or similar") -and $sigRenderer.state -eq "unavailable") -or ($sigRenderer.state -eq "matched")

    $testResults.evidenceCoverageII = [ordered]@{
      sessionId = $sessionId1
      state = $launchState1
      fontMode = $artData1280.policy.fontMode
      managedFontMaskingGatePassed = ($launchState1 -eq "running")
      webgl2Vendor = [ordered]@{
        state = $sigVendor.state
        expected = $sigVendor.expected
        observed = $sigVendor.observed
        pass = ($sigVendor.state -eq "matched")
      }
      webgl2Renderer = [ordered]@{
        state = $sigRenderer.state
        expected = $sigRenderer.expected
        observed = $sigRenderer.observed
        unavailableCorrect = $rendererUnavailableCorrect
        pass = $rendererUnavailableCorrect
      }
      acceptEncoding = [ordered]@{
        state = $sigEncoding.state
        expected = $sigEncoding.expected
        observed = $sigEncoding.observed
        pass = ($sigEncoding.state -eq "matched")
      }
      pass = ($launchState1 -eq "running" -and $sigVendor.state -eq "matched" -and $rendererUnavailableCorrect -and $sigEncoding.state -eq "matched")
    }
    Write-Step "Evidence Coverage II Checks: WebGL2Vendor=$($sigVendor.state), WebGL2Renderer=$($sigRenderer.state), AcceptEncoding=$($sigEncoding.state), Pass=$($testResults.evidenceCoverageII.pass)"

    # Window Geometry 1280x800 Evaluation
    Write-Step "Evaluating Window Geometry on 1280x800 session..."
    $geomPage1 = $session1.Request("page", @{
      sessionId = $sessionId1
      action = "evaluate"
      script = $GEOMETRY_EVAL_SCRIPT
    })
    if ($geomPage1.ok -ne $true) { throw "Page evaluate failed in Session 1: $($geomPage1 | ConvertTo-Json)" }
    $geomObs1 = $geomPage1.result.value
    $geomInv1 = Test-GeometryInvariants $geomObs1

    $testResults.windowGeometry["1280x800"] = [ordered]@{
      observation = $geomObs1
      invariants = $geomInv1.invariants
      pass = $geomInv1.allPass
    }
    Write-Step "Window Geometry 1280x800 Invariants: Pass=$($geomInv1.allPass)"

    # Fresh Recheck with Page Sentinel
    Write-Step "Awaiting initial start page navigation settlement before setting Sentinel..."
    $settleSw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($settleSw.Elapsed.TotalSeconds -lt 15) {
      $curUrlRes = $session1.Request("page", @{
        sessionId = $sessionId1
        action = "evaluate"
        script = "() => ({ url: location.href, readyState: document.readyState })"
      })
      if ($curUrlRes.ok -eq $true) {
        $curVal = $curUrlRes.result.value
        if ($curVal.url -notlike "*probe.html*" -and $curVal.readyState -eq "complete") {
          Write-Step "Initial navigation settled on: $($curVal.url)"
          break
        }
      }
      Start-Sleep -Milliseconds 500
    }

    Write-Step "Setting Page Sentinel before reobserve_identity..."
    $sentinelToken = "sentinel-" + [Guid]::NewGuid().ToString("n")
    $setSentinelScript = "() => { window.__verisilo_sentinel = '$sentinelToken'; return window.__verisilo_sentinel; }"
    $setSentinelRes = $session1.Request("page", @{
      sessionId = $sessionId1
      action = "evaluate"
      script = $setSentinelScript
    })
    if ($setSentinelRes.ok -ne $true -or $setSentinelRes.result.value -ne $sentinelToken) {
      throw "Failed to set page sentinel before reobserve_identity"
    }
    $initialObservedAt = $evidence1.observedAt
    $initialPageUrl = $setSentinelRes.result.url
    Write-Step "Page Sentinel set: $sentinelToken on URL: $initialPageUrl (initial observedAt: $initialObservedAt)"

    Write-Step "Sending reobserve_identity command..."
    $reobserveRes = $session1.Request("reobserve_identity", @{
      sessionId = $sessionId1
    })
    if ($reobserveRes.ok -ne $true) { throw "reobserve_identity command failed: $($reobserveRes | ConvertTo-Json)" }
    $reobservedAt = $reobserveRes.result.reobservedAt
    $reobsEvidence = $reobserveRes.result.identityEvidence
    $reobsVendor = Find-Signal $reobsEvidence "webgl2Vendor"
    $reobsRenderer = Find-Signal $reobsEvidence "webgl2Renderer"
    $reobsEncoding = Find-Signal $reobsEvidence "acceptEncoding"

    Write-Step "Checking Page Sentinel after reobserve_identity..."
    $checkSentinelScript = "() => ({ sentinel: window.__verisilo_sentinel, url: location.href })"
    $checkSentinelRes = $session1.Request("page", @{
      sessionId = $sessionId1
      action = "evaluate"
      script = $checkSentinelScript
    })
    if ($checkSentinelRes.ok -ne $true) { throw "Failed to evaluate page sentinel after reobserve_identity" }
    $postSentinel = $checkSentinelRes.result.value.sentinel
    $postUrl = $checkSentinelRes.result.value.url

    $sentinelIntact = ($postSentinel -eq $sentinelToken)
    $urlIntact = ($postUrl -eq $initialPageUrl)
    $timestampAdvanced = ($reobservedAt -gt $initialObservedAt)

    $testResults.freshRecheck = [ordered]@{
      sessionId = $sessionId1
      initialObservedAt = $initialObservedAt
      reobservedAt = $reobservedAt
      timestampAdvanced = $timestampAdvanced
      sentinelIntact = $sentinelIntact
      urlIntact = $urlIntact
      webgl2VendorReobserved = ($null -ne $reobsVendor)
      webgl2RendererReobserved = ($null -ne $reobsRenderer)
      acceptEncodingReobserved = ($null -ne $reobsEncoding)
      pass = ($timestampAdvanced -and $sentinelIntact -and $urlIntact -and $null -ne $reobsVendor -and $null -ne $reobsRenderer -and $null -ne $reobsEncoding)
    }
    Write-Step "Fresh Recheck Checks: TimestampAdvanced=$timestampAdvanced, SentinelIntact=$sentinelIntact, UrlIntact=$urlIntact, Pass=$($testResults.freshRecheck.pass)"

    # Close & Shutdown Session 1
    Write-Step "Closing Session 1..."
    $close1 = $session1.Request("close", @{ sessionId = $sessionId1 })
    if ($close1.ok -ne $true) { throw "Close failed in Session 1" }
    $shut1 = $session1.Request("shutdown", @{})
    if ($shut1.ok -ne $true) { throw "Shutdown failed in Session 1" }
  } finally {
    $session1.Dispose()
  }

  # --- Session 2: Cold Restart with Same 1280x800 Artifact ---
  Write-Step "=== SESSION 2: Cold Restart with Same 1280x800 Artifact ==="
  $session2 = [HostSession]::new($hostExe, $PackageRoot, $localArtRoot, $localProfRoot, $localStateRoot, "sess2-restart")
  try {
    [void]$session2.Request("hello", @{})
    Write-Step "Launching Session 2 (Cold Restart)..."
    $launch2 = $session2.Request("launch", @{
      artifactId = $artId1280
      profileId = "profile-1280-a"
      expectedArtifactFileSha256 = $artSha1280
    })
    if ($launch2.ok -ne $true) { throw "Launch failed in Session 2: $($launch2 | ConvertTo-Json)" }
    $sessionId2 = $launch2.result.sessionId

    $geomPage2 = $session2.Request("page", @{
      sessionId = $sessionId2
      action = "evaluate"
      script = $GEOMETRY_EVAL_SCRIPT
    })
    if ($geomPage2.ok -ne $true) { throw "Page evaluate failed in Session 2" }
    $geomObs2 = $geomPage2.result.value
    $geomInv2 = Test-GeometryInvariants $geomObs2

    $testResults.coldRestart = [ordered]@{
      sessionId = $sessionId2
      observation = $geomObs2
      invariants = $geomInv2.invariants
      pass = $geomInv2.allPass
    }
    Write-Step "Cold Restart Geometry Invariants: Pass=$($geomInv2.allPass)"

    [void]$session2.Request("close", @{ sessionId = $sessionId2 })
    [void]$session2.Request("shutdown", @{})
  } finally {
    $session2.Dispose()
  }

  # --- Session 3: Second Size (1024x768 Artifact) ---
  Write-Step "=== SESSION 3: Second Size (1024x768) ==="
  $session3 = [HostSession]::new($hostExe, $PackageRoot, $localArtRoot, $localProfRoot, $localStateRoot, "sess3-1024")
  try {
    [void]$session3.Request("hello", @{})
    Write-Step "Launching Session 3 (1024x768)..."
    $launch3 = $session3.Request("launch", @{
      artifactId = $artId1024
      profileId = "profile-1024"
      expectedArtifactFileSha256 = $artSha1024
    })
    if ($launch3.ok -ne $true) { throw "Launch failed in Session 3: $($launch3 | ConvertTo-Json)" }
    $sessionId3 = $launch3.result.sessionId

    $geomPage3 = $session3.Request("page", @{
      sessionId = $sessionId3
      action = "evaluate"
      script = $GEOMETRY_EVAL_SCRIPT
    })
    if ($geomPage3.ok -ne $true) { throw "Page evaluate failed in Session 3" }
    $geomObs3 = $geomPage3.result.value
    $geomInv3 = Test-GeometryInvariants $geomObs3

    $testResults.windowGeometry["1024x768"] = [ordered]@{
      observation = $geomObs3
      invariants = $geomInv3.invariants
      pass = $geomInv3.allPass
    }
    Write-Step "Window Geometry 1024x768 Invariants: Pass=$($geomInv3.allPass)"

    [void]$session3.Request("close", @{ sessionId = $sessionId3 })
    [void]$session3.Request("shutdown", @{})
  } finally {
    $session3.Dispose()
  }

  # --- Session 4: Legacy Artifact Clamping & Compatibility ---
  Write-Step "=== SESSION 4: Legacy Artifact Clamping & File Immutability ==="
  $session4 = [HostSession]::new($hostExe, $PackageRoot, $localArtRoot, $localProfRoot, $localStateRoot, "sess4-legacy")
  try {
    [void]$session4.Request("hello", @{})
    Write-Step "Launching Session 4 with Legacy Artifact ($legacyArtId)..."
    $launch4 = $session4.Request("launch", @{
      artifactId = $legacyArtId
      profileId = "profile-legacy"
      expectedArtifactFileSha256 = $legacyShaBefore
    })
    if ($launch4.ok -ne $true) { throw "Launch failed in Session 4: $($launch4 | ConvertTo-Json)" }
    $sessionId4 = $launch4.result.sessionId

    $geomPage4 = $session4.Request("page", @{
      sessionId = $sessionId4
      action = "evaluate"
      script = $GEOMETRY_EVAL_SCRIPT
    })
    if ($geomPage4.ok -ne $true) { throw "Page evaluate failed in Session 4" }
    $geomObs4 = $geomPage4.result.value
    $geomInv4 = Test-GeometryInvariants $geomObs4

    # Verify that screenX and screenY were clamped within virtual screen bounds (not equal to the 2232 / 140 injected)
    $screen = $geomObs4.screen
    $maxX = $screen.availLeft + [Math]::Max(0, ($screen.availWidth - $geomObs4.outerWidth))
    $maxY = $screen.availTop + [Math]::Max(0, ($screen.availHeight - $geomObs4.outerHeight))
    $clampedX = ($geomObs4.screenX -le $maxX -and $geomObs4.screenX -ge $screen.availLeft)
    $clampedY = ($geomObs4.screenY -le $maxY -and $geomObs4.screenY -ge $screen.availTop)
    $clamped = ($clampedX -and $clampedY -and ($geomObs4.screenX -ne $legacyMeta.injectedScreenX))

    [void]$session4.Request("close", @{ sessionId = $sessionId4 })
    [void]$session4.Request("shutdown", @{})

    # Disk file SHA check
    $legacyShaAfter = (Get-FileHash $legacyDstJson -Algorithm SHA256).Hash.ToLowerInvariant()
    $fileUnchanged = ($legacyShaBefore -eq $legacyShaAfter)

    $testResults.legacyCompatibility = [ordered]@{
      artifactId = $legacyArtId
      injectedScreenX = $legacyMeta.injectedScreenX
      injectedScreenY = $legacyMeta.injectedScreenY
      observedScreenX = $geomObs4.screenX
      observedScreenY = $geomObs4.screenY
      clampedWithinBounds = $clamped
      invariantsPass = $geomInv4.allPass
      artifactFileShaBefore = $legacyShaBefore
      artifactFileShaAfter = $legacyShaAfter
      artifactFileUnchanged = $fileUnchanged
      pass = ($clamped -and $geomInv4.allPass -and $fileUnchanged)
    }
    Write-Step "Legacy Artifact Clamping: Clamped=$clamped, InvariantsPass=$($geomInv4.allPass), FileUnchanged=$fileUnchanged, Pass=$($testResults.legacyCompatibility.pass)"
  } finally {
    $session4.Dispose()
  }

  # Cleanup & Process Inspection
  Write-Step "Performing Post-Test Process & Cleanup Check..."
  Start-Sleep -Seconds 2
  $camoufoxProcs = @(Get-Process -Name "camoufox" -ErrorAction SilentlyContinue)
  $hostProcs = @(Get-Process -Name "camoufox-host" -ErrorAction SilentlyContinue)
  $supervisorProcs = @(Get-Process -Name "verisilo-camoufox-supervisor" -ErrorAction SilentlyContinue)

  $testResults.cleanup = [ordered]@{
    camoufoxResidualProcessCount = $camoufoxProcs.Count
    hostResidualProcessCount = $hostProcs.Count
    supervisorResidualProcessCount = $supervisorProcs.Count
    pass = ($camoufoxProcs.Count -eq 0 -and $hostProcs.Count -eq 0 -and $supervisorProcs.Count -eq 0)
  }
  Write-Step "Cleanup Check: Camoufox residuals=$($camoufoxProcs.Count), Host residuals=$($hostProcs.Count), Supervisor residuals=$($supervisorProcs.Count)"

  # Overall Verdict
  $allTestsPassed = (
    $testResults.evidenceCoverageII.pass -and
    $testResults.freshRecheck.pass -and
    $testResults.windowGeometry["1280x800"].pass -and
    $testResults.windowGeometry["1024x768"].pass -and
    $testResults.coldRestart.pass -and
    $testResults.legacyCompatibility.pass -and
    $testResults.cleanup.pass
  )

  if ($allTestsPassed) {
    $testResults.verdict = "PASS"
    $testResults["classification"] = "CURRENT_SOURCE_PACKAGED_HOST_RUNTIME_ACCEPTED_IN_WINDOWS_SANDBOX"
  } else {
    $testResults.verdict = "FAIL"
    $testResults["classification"] = "ACCEPTANCE_TEST_FAILED"
  }

  Write-Step "=== FINAL ACCEPTANCE VERDICT: $($testResults.verdict) ($($testResults.classification)) ==="

} catch {
  Write-Step "ERROR during acceptance: $_"
  $testResults.verdict = "FAIL"
  $testResults["error"] = "$_"
  $testResults["classification"] = "ACCEPTANCE_EXECUTION_ERROR"
} finally {
  # Save Safe Evidence Summaries to OutputDir
  $resultsJson = ($testResults | ConvertTo-Json -Depth 6)
  [System.IO.File]::WriteAllText((Join-Path $OutputDir "results.json"), $resultsJson, [System.Text.UTF8Encoding]::new($false))

  # geometry-observation.json
  $geometryObservations = [ordered]@{
    "1280x800" = $testResults.windowGeometry["1280x800"]
    "1024x768" = $testResults.windowGeometry["1024x768"]
    "coldRestart" = $testResults.coldRestart
    "legacyCompatibility" = $testResults.legacyCompatibility
  }
  $geomJson = ($geometryObservations | ConvertTo-Json -Depth 6)
  [System.IO.File]::WriteAllText((Join-Path $OutputDir "geometry-observation.json"), $geomJson, [System.Text.UTF8Encoding]::new($false))

  # identity-evidence-summary.json
  $identitySummary = [ordered]@{
    evidenceCoverageII = $testResults.evidenceCoverageII
    freshRecheck = $testResults.freshRecheck
  }
  $idJson = ($identitySummary | ConvertTo-Json -Depth 6)
  [System.IO.File]::WriteAllText((Join-Path $OutputDir "identity-evidence-summary.json"), $idJson, [System.Text.UTF8Encoding]::new($false))

  # Copy host-stderr.log if it exists
  $hostStderr = Join-Path $localStateRoot "host-stderr.log"
  if (Test-Path $hostStderr) {
    Copy-Item $hostStderr (Join-Path $OutputDir "host-stderr.log") -Force
  }

  # Clean up local sandbox temporary directory
  Remove-Item -Recurse -Force $sandboxTemp -ErrorAction SilentlyContinue

  # Write Final Sentinel
  if ($testResults.verdict -eq "PASS") {
    Set-Content -Path (Join-Path $OutputDir "pass.sentinel") -Value ([DateTime]::UtcNow.ToString("o")) -Encoding UTF8
  } else {
    Set-Content -Path (Join-Path $OutputDir "fail.sentinel") -Value ([DateTime]::UtcNow.ToString("o")) -Encoding UTF8
  }
  Set-Content -Path (Join-Path $OutputDir "completed.sentinel") -Value ([DateTime]::UtcNow.ToString("o")) -Encoding UTF8
  Write-Step "Packaged-Host Windows Sandbox Runtime Acceptance Finished."
}
