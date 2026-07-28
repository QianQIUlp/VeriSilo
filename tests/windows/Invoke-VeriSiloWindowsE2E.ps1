[CmdletBinding()]
param(
  [ValidateSet('Chrome', 'Edge', 'Both')]
  [string]$Browser = 'Both',

  [string]$ChromePath,
  [string]$EdgePath,
  [string]$DesktopExe,
  [string]$AcceptanceDriverPath,
  [string]$CandidateDescriptorPath,
  [string]$NativeHostPath,
  [string]$ReleaseConfigPath,
  [string]$NativeHostManifestRoot,

  [ValidateSet('Windows 10', 'Windows 11')]
  [string]$ExpectedWindowsVersion,

  [switch]$RunNsis,
  [string]$NsisInstallerV1,
  [string]$NsisInstallerV2,
  [string]$InstallDirectory,
  [string[]]$RetainedDataPath = @(),

  [int]$FixturePort = 0,
  [string]$ArtifactDirectory,
  [switch]$KeepArtifacts,
  [switch]$RequireAll,
  [switch]$SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:Results = [System.Collections.Generic.List[object]]::new()
$script:ProcessIds = [System.Collections.Generic.List[int]]::new()
$script:FixtureProcess = $null
$script:FixtureOutput = $null
$script:FixturePort = 0
$script:ArtifactDirectory = $null
$script:ArtifactDirectoryWasProvided = $false

function Add-Result {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][ValidateSet('PASS', 'FAIL', 'SKIP', 'BLOCKED')][string]$Status,
    [Parameter(Mandatory = $true)][string]$Detail
  )

  $entry = [pscustomobject][ordered]@{
    name = $Name
    status = $Status
    detail = $Detail
  }
  $script:Results.Add($entry)
  Write-Host "[$Status] $Name - $Detail"
}

function Test-WindowsHost {
  return [System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT
}

function Get-FreeLoopbackPort {
  $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
  try {
    $listener.Start()
    return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
  } finally {
    $listener.Stop()
  }
}

function Test-IsAdministrator {
  if (-not (Test-WindowsHost)) {
    return $false
  }
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = [Security.Principal.WindowsPrincipal]::new($identity)
  return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Test-OperatingSystemEvidence {
  $currentVersion = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
  $productName = [string]$currentVersion.ProductName
  $displayVersion = [string]$currentVersion.DisplayVersion
  $build = [string]$currentVersion.CurrentBuild
  if (-not $ExpectedWindowsVersion) {
    Add-Result -Name 'windows_matrix_target' -Status 'SKIP' -Detail "Host is $productName $displayVersion (build $build), but no -ExpectedWindowsVersion was declared. Run once for Windows 10 and once for Windows 11 before claiming the matrix."
    return
  }
  if ($productName -notlike "*$ExpectedWindowsVersion*") {
    Add-Result -Name 'windows_matrix_target' -Status 'BLOCKED' -Detail "Expected $ExpectedWindowsVersion but the host reports $productName $displayVersion (build $build)."
    return
  }
  Add-Result -Name 'windows_matrix_target' -Status 'PASS' -Detail "Validated $productName $displayVersion (build $build) as the declared $ExpectedWindowsVersion target."
}

function Get-BrowserConfiguration {
  param([Parameter(Mandatory = $true)][ValidateSet('Chrome', 'Edge')][string]$Name)

  $providedPath = if ($Name -eq 'Chrome') { $ChromePath } else { $EdgePath }
  $candidates = [System.Collections.Generic.List[string]]::new()
  if ($providedPath) { $candidates.Add($providedPath) }
  if ($Name -eq 'Chrome') {
    $candidates.Add((Join-Path $env:ProgramFiles 'Google\Chrome\Application\chrome.exe'))
    if (${env:ProgramFiles(x86)}) {
      $candidates.Add((Join-Path ${env:ProgramFiles(x86)} 'Google\Chrome\Application\chrome.exe'))
    }
    $defaultUserData = Join-Path $env:LOCALAPPDATA 'Google\Chrome\User Data'
    $processName = 'chrome'
  } else {
    $candidates.Add((Join-Path $env:ProgramFiles 'Microsoft\Edge\Application\msedge.exe'))
    if (${env:ProgramFiles(x86)}) {
      $candidates.Add((Join-Path ${env:ProgramFiles(x86)} 'Microsoft\Edge\Application\msedge.exe'))
    }
    $defaultUserData = Join-Path $env:LOCALAPPDATA 'Microsoft\Edge\User Data'
    $processName = 'msedge'
  }

  $executable = $candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) } | Select-Object -First 1
  return [pscustomobject]@{
    Name = $Name
    Executable = $executable
    DefaultUserData = $defaultUserData
    DefaultProfile = Join-Path $defaultUserData 'Default'
    ProcessName = $processName
  }
}

function Get-NormalizedPath {
  param([Parameter(Mandatory = $true)][string]$Path)
  return [IO.Path]::GetFullPath($Path).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
}

function Get-RandomHex {
  param([ValidateRange(16, 128)][int]$Bytes = 32)
  return [Convert]::ToHexString([Security.Cryptography.RandomNumberGenerator]::GetBytes($Bytes)).ToLowerInvariant()
}

function Assert-TemporaryUserDataDirectory {
  param(
    [Parameter(Mandatory = $true)][string]$UserDataDirectory,
    [Parameter(Mandatory = $true)][string]$DefaultUserDataDirectory
  )

  $candidate = Get-NormalizedPath $UserDataDirectory
  $default = Get-NormalizedPath $DefaultUserDataDirectory
  if ([string]::Equals($candidate, $default, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Refusing to use the browser default User Data directory. This harness only launches a temporary --user-data-dir.'
  }
  $temporaryRoot = Get-NormalizedPath ([IO.Path]::GetTempPath())
  if (-not $candidate.StartsWith("$temporaryRoot$([IO.Path]::DirectorySeparatorChar)", [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing non-temporary user-data-dir: $candidate"
  }
}

function Start-ArrayProcess {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [Parameter(Mandatory = $true)][string[]]$Arguments,
    [string]$StandardOutputPath,
    [string]$StandardErrorPath
  )

  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $FilePath
  $startInfo.UseShellExecute = $false
  $startInfo.RedirectStandardOutput = [bool]$StandardOutputPath
  $startInfo.RedirectStandardError = [bool]$StandardErrorPath
  $startInfo.CreateNoWindow = $true
  foreach ($argument in $Arguments) {
    # ArgumentList avoids string-concatenated shell input and is required for every process launch in this harness.
    [void]$startInfo.ArgumentList.Add($argument)
  }
  $process = [System.Diagnostics.Process]::new()
  $process.StartInfo = $startInfo
  if (-not $process.Start()) {
    throw "Failed to start $FilePath"
  }
  if ($StandardOutputPath) {
    $process.StandardOutput.BaseStream.CopyToAsync([IO.File]::Open($StandardOutputPath, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::Read)).GetAwaiter().GetResult()
  }
  if ($StandardErrorPath) {
    $process.StandardError.BaseStream.CopyToAsync([IO.File]::Open($StandardErrorPath, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::Read)).GetAwaiter().GetResult()
  }
  return $process
}

function Start-ProcessToFiles {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [Parameter(Mandatory = $true)][string[]]$Arguments,
    [Parameter(Mandatory = $true)][string]$StandardOutputPath,
    [Parameter(Mandatory = $true)][string]$StandardErrorPath
  )

  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $FilePath
  $startInfo.UseShellExecute = $false
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  $startInfo.CreateNoWindow = $true
  foreach ($argument in $Arguments) {
    [void]$startInfo.ArgumentList.Add($argument)
  }
  $process = [System.Diagnostics.Process]::new()
  $process.StartInfo = $startInfo
  if (-not $process.Start()) { throw "Failed to start $FilePath" }
  return [pscustomobject]@{
    Process = $process
    StdoutPath = $StandardOutputPath
    StderrPath = $StandardErrorPath
    StdoutTask = $process.StandardOutput.ReadToEndAsync()
    StderrTask = $process.StandardError.ReadToEndAsync()
  }
}

function Stop-TestProcess {
  param([System.Diagnostics.Process]$Process)
  if ($null -eq $Process) { return }
  try {
    if (-not $Process.HasExited) {
      $Process.Kill($true)
      $Process.WaitForExit(10000) | Out-Null
    }
  } catch {
    Write-Warning "Could not stop test process $($Process.Id): $($_.Exception.Message)"
  }
}

function Invoke-LoopbackRequest {
  param(
    [Parameter(Mandatory = $true)][ValidateSet('GET', 'POST')][string]$Method,
    [Parameter(Mandatory = $true)][string]$Uri
  )
  $handler = [System.Net.Http.HttpClientHandler]::new()
  $handler.UseProxy = $false
  $client = [System.Net.Http.HttpClient]::new($handler)
  try {
    $httpMethod = if ($Method -eq 'GET') { [System.Net.Http.HttpMethod]::Get } else { [System.Net.Http.HttpMethod]::Post }
    $request = [System.Net.Http.HttpRequestMessage]::new($httpMethod, [Uri]$Uri)
    $response = $client.SendAsync($request).GetAwaiter().GetResult()
    $body = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
    if (-not $response.IsSuccessStatusCode) {
      throw "Loopback request failed ($([int]$response.StatusCode)): $body"
    }
    return $body
  } finally {
    $client.Dispose()
    $handler.Dispose()
  }
}

function Start-LoopbackFixture {
  $node = Get-Command node -ErrorAction SilentlyContinue
  if ($null -eq $node) { throw 'node is required to start the real loopback fixture server.' }
  $script:FixturePort = if ($FixturePort -gt 0) { $FixturePort } else { Get-FreeLoopbackPort }
  $fixture = Join-Path $PSScriptRoot 'fixtures\loopback-server.mjs'
  $stdout = Join-Path $script:ArtifactDirectory 'fixture.stdout.log'
  $stderr = Join-Path $script:ArtifactDirectory 'fixture.stderr.log'
  $started = Start-ProcessToFiles -FilePath $node.Source -Arguments @($fixture, '--host', '127.0.0.1', '--port', "$script:FixturePort") -StandardOutputPath $stdout -StandardErrorPath $stderr
  $script:FixtureProcess = $started.Process
  $script:FixtureOutput = $started
  for ($attempt = 0; $attempt -lt 40; $attempt++) {
    try {
      Invoke-LoopbackRequest -Method GET -Uri "http://127.0.0.1:$script:FixturePort/__events" | Out-Null
      return
    } catch {
      Start-Sleep -Milliseconds 125
    }
  }
  throw "The loopback fixture did not become reachable. See $stderr"
}

function Get-LoopbackEvents {
  $json = Invoke-LoopbackRequest -Method GET -Uri "http://127.0.0.1:$script:FixturePort/__events"
  return @($json | ConvertFrom-Json)
}

function Reset-LoopbackEvents {
  Invoke-LoopbackRequest -Method POST -Uri "http://127.0.0.1:$script:FixturePort/__reset" | Out-Null
}

function Get-CdpEndpoint {
  param([Parameter(Mandatory = $true)][int]$Port)
  $raw = Invoke-LoopbackRequest -Method GET -Uri "http://127.0.0.1:$Port/json/list"
  return @($raw | ConvertFrom-Json)
}

function Wait-CdpPage {
  param([Parameter(Mandatory = $true)][int]$Port)
  for ($attempt = 0; $attempt -lt 80; $attempt++) {
    try {
      $page = Get-CdpEndpoint -Port $Port | Where-Object { $_.type -eq 'page' -and $_.webSocketDebuggerUrl } | Select-Object -First 1
      if ($null -ne $page) { return $page }
    } catch {}
    Start-Sleep -Milliseconds 125
  }
  throw "No DevTools page target was exposed on loopback port $Port."
}

function Connect-Cdp {
  param([Parameter(Mandatory = $true)][string]$WebSocketDebuggerUrl)
  $socket = [System.Net.WebSockets.ClientWebSocket]::new()
  $socket.Options.KeepAliveInterval = [TimeSpan]::FromSeconds(10)
  $socket.ConnectAsync([Uri]$WebSocketDebuggerUrl, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
  return [pscustomobject]@{ Socket = $socket; NextId = 1 }
}

function Receive-CdpText {
  param([Parameter(Mandatory = $true)]$Connection)
  $buffer = New-Object byte[] 65536
  $stream = [IO.MemoryStream]::new()
  try {
    do {
      $result = $Connection.Socket.ReceiveAsync([ArraySegment[byte]]::new($buffer), [Threading.CancellationToken]::None).GetAwaiter().GetResult()
      if ($result.MessageType -eq [System.Net.WebSockets.WebSocketMessageType]::Close) {
        throw 'DevTools closed the WebSocket before it returned a response.'
      }
      $stream.Write($buffer, 0, $result.Count)
    } while (-not $result.EndOfMessage)
    return [Text.Encoding]::UTF8.GetString($stream.ToArray())
  } finally {
    $stream.Dispose()
  }
}

function Invoke-Cdp {
  param(
    [Parameter(Mandatory = $true)]$Connection,
    [Parameter(Mandatory = $true)][string]$Method,
    [hashtable]$Params = @{}
  )
  $id = $Connection.NextId
  $Connection.NextId++
  $payload = @{ id = $id; method = $Method; params = $Params } | ConvertTo-Json -Compress -Depth 20
  $bytes = [Text.Encoding]::UTF8.GetBytes($payload)
  $Connection.Socket.SendAsync([ArraySegment[byte]]::new($bytes), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
  while ($true) {
    $message = Receive-CdpText -Connection $Connection | ConvertFrom-Json
    if ($message.id -eq $id) {
      if ($message.error) { throw "DevTools $Method failed: $($message.error.message)" }
      return $message
    }
  }
}

function Invoke-CdpNavigate {
  param([Parameter(Mandatory = $true)]$Connection, [Parameter(Mandatory = $true)][string]$Uri)
  return Invoke-Cdp -Connection $Connection -Method 'Page.navigate' -Params @{ url = $Uri }
}

function Wait-FixtureTitle {
  param([Parameter(Mandatory = $true)]$Connection)
  for ($attempt = 0; $attempt -lt 80; $attempt++) {
    $response = Invoke-Cdp -Connection $Connection -Method 'Runtime.evaluate' -Params @{ expression = 'document.title'; returnByValue = $true }
    $title = [string]$response.result.result.value
    if ($title.StartsWith('VERISILO_E2E:PASS:', [StringComparison]::Ordinal)) { return $title }
    if ($title.StartsWith('VERISILO_E2E:FAIL:', [StringComparison]::Ordinal)) { throw "Fixture assertion failed: $title" }
    Start-Sleep -Milliseconds 125
  }
  throw 'Fixture page did not report a PASS/FAIL title before the timeout.'
}

function Start-TemporaryBrowser {
  param(
    [Parameter(Mandatory = $true)]$Configuration,
    [Parameter(Mandatory = $true)][string]$UserDataDirectory,
    [Parameter(Mandatory = $true)][int]$RemoteDebuggingPort,
    [string[]]$ExtraArguments = @()
  )

  Assert-TemporaryUserDataDirectory -UserDataDirectory $UserDataDirectory -DefaultUserDataDirectory $Configuration.DefaultUserData
  New-Item -ItemType Directory -Force -Path $UserDataDirectory | Out-Null
  $arguments = @(
    "--user-data-dir=$UserDataDirectory",
    "--remote-debugging-port=$RemoteDebuggingPort",
    '--remote-allow-origins=*',
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-background-networking',
    'about:blank'
  ) + $ExtraArguments
  if ($arguments -match '--profile-directory=Default') {
    throw 'The E2E harness must never select a default browser Profile.'
  }
  $process = Start-ArrayProcess -FilePath $Configuration.Executable -Arguments $arguments
  $script:ProcessIds.Add($process.Id)
  $page = Wait-CdpPage -Port $RemoteDebuggingPort
  return [pscustomobject]@{ Process = $process; Connection = (Connect-Cdp -WebSocketDebuggerUrl $page.webSocketDebuggerUrl); UserDataDirectory = $UserDataDirectory; RemoteDebuggingPort = $RemoteDebuggingPort }
}

function Stop-TemporaryBrowser {
  param($BrowserSession)
  if ($null -eq $BrowserSession) { return }
  try {
    if ($BrowserSession.Connection.Socket.State -eq [System.Net.WebSockets.WebSocketState]::Open) {
      $BrowserSession.Connection.Socket.CloseAsync([System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure, 'complete', [Threading.CancellationToken]::None).GetAwaiter().GetResult()
    }
    $BrowserSession.Connection.Socket.Dispose()
  } catch {}
  Stop-TestProcess -Process $BrowserSession.Process
}

function Get-TreeFingerprint {
  param([Parameter(Mandatory = $true)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
    throw "Path does not exist: $Path"
  }
  $records = [System.Collections.Generic.List[string]]::new()
  $root = Get-NormalizedPath $Path
  foreach ($item in Get-ChildItem -LiteralPath $root -Recurse -Force -ErrorAction Stop | Sort-Object FullName) {
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "Refusing an incomplete tree hash: reparse point found at $($item.FullName)"
    }
    $relative = $item.FullName.Substring($root.Length).TrimStart([char]'\', [char]'/' )
    if ($item.PSIsContainer) {
      $records.Add("D|$relative|$($item.LastWriteTimeUtc.Ticks)")
    } else {
      $hash = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256 -ErrorAction Stop).Hash
      $records.Add("F|$relative|$($item.Length)|$($item.LastWriteTimeUtc.Ticks)|$hash")
    }
  }
  $serialized = [Text.Encoding]::UTF8.GetBytes(($records -join "`n"))
  $aggregate = [Security.Cryptography.SHA256]::HashData($serialized)
  return [pscustomobject]@{ Hash = ([Convert]::ToHexString($aggregate)); Entries = $records.Count }
}

function Test-DefaultProfileInvariant {
  param([Parameter(Mandatory = $true)]$Configuration, [Parameter(Mandatory = $true)][scriptblock]$Exercise)
  if (-not (Test-Path -LiteralPath $Configuration.DefaultProfile -PathType Container)) {
    try {
      & $Exercise
      if (Test-Path -LiteralPath $Configuration.DefaultProfile) {
        throw 'The real cases created a browser default Profile that was absent before the run.'
      }
      Add-Result -Name "$($Configuration.Name)_default_profile_unchanged" -Status 'PASS' -Detail 'The default Profile was absent before the run and remained absent; every launch used a separate temporary user-data-dir.'
    } catch {
      Add-Result -Name "$($Configuration.Name)_default_profile_unchanged" -Status 'FAIL' -Detail $_.Exception.Message
    }
    return
  }
  if (Get-Process -Name $Configuration.ProcessName -ErrorAction SilentlyContinue) {
    Add-Result -Name "$($Configuration.Name)_default_profile_unchanged" -Status 'BLOCKED' -Detail 'A browser process is already running. It could mutate the default profile, so the harness will not claim a clean before/after file-tree hash.'
    & $Exercise
    return
  }
  try {
    $before = Get-TreeFingerprint -Path $Configuration.DefaultProfile
    & $Exercise
    $after = Get-TreeFingerprint -Path $Configuration.DefaultProfile
    if ($before.Hash -ne $after.Hash -or $before.Entries -ne $after.Entries) {
      throw "default profile file-tree hash or mtime changed ($($before.Hash) -> $($after.Hash))."
    }
    Add-Result -Name "$($Configuration.Name)_default_profile_unchanged" -Status 'PASS' -Detail "SHA-256 tree fingerprint unchanged across $($before.Entries) entries."
  } catch {
    Add-Result -Name "$($Configuration.Name)_default_profile_unchanged" -Status 'FAIL' -Detail $_.Exception.Message
  }
}

function Test-BrowserStorageIsolation {
  param([Parameter(Mandatory = $true)]$Configuration)
  $browserRoot = Join-Path $script:ArtifactDirectory $Configuration.Name.ToLowerInvariant()
  $profileA = Join-Path $browserRoot 'profile-a'
  $profileB = Join-Path $browserRoot 'profile-b'
  $session = $null
  try {
    $session = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profileA -RemoteDebuggingPort (Get-FreeLoopbackPort)
    Invoke-CdpNavigate -Connection $session.Connection -Uri "http://127.0.0.1:$script:FixturePort/?op=write&value=A" | Out-Null
    Wait-FixtureTitle -Connection $session.Connection | Out-Null
    Invoke-CdpNavigate -Connection $session.Connection -Uri "http://127.0.0.1:$script:FixturePort/?op=read&expected=A" | Out-Null
    Wait-FixtureTitle -Connection $session.Connection | Out-Null
    Stop-TemporaryBrowser -BrowserSession $session
    $session = $null

    $session = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profileB -RemoteDebuggingPort (Get-FreeLoopbackPort)
    Invoke-CdpNavigate -Connection $session.Connection -Uri "http://127.0.0.1:$script:FixturePort/?op=read&expected=" | Out-Null
    Wait-FixtureTitle -Connection $session.Connection | Out-Null
    Stop-TemporaryBrowser -BrowserSession $session
    $session = $null

    $session = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profileA -RemoteDebuggingPort (Get-FreeLoopbackPort)
    Invoke-CdpNavigate -Connection $session.Connection -Uri "http://127.0.0.1:$script:FixturePort/?op=read&expected=A" | Out-Null
    Wait-FixtureTitle -Connection $session.Connection | Out-Null
    Add-Result -Name "$($Configuration.Name)_temporary_A_B_storage_cookie_isolation" -Status 'PASS' -Detail 'Temporary A/B user-data-dir profiles isolated localStorage, sessionStorage, IndexedDB, and cookies; reopening A retained all markers.'
  } catch {
    Add-Result -Name "$($Configuration.Name)_temporary_A_B_storage_cookie_isolation" -Status 'FAIL' -Detail $_.Exception.Message
  } finally { Stop-TemporaryBrowser -BrowserSession $session }
}

function Test-BrowserLockBehavior {
  param([Parameter(Mandatory = $true)]$Configuration)
  $profile = Join-Path (Join-Path $script:ArtifactDirectory $Configuration.Name.ToLowerInvariant()) 'lock-profile'
  $first = $null
  $second = $null
  try {
    $first = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profile -RemoteDebuggingPort (Get-FreeLoopbackPort)
    $lockPaths = @((Join-Path $profile 'SingletonLock'), (Join-Path $profile 'SingletonCookie'))
    if (-not ($lockPaths | Where-Object { Test-Path -LiteralPath $_ })) {
      throw 'Chromium did not expose a managed-profile lock marker before the lock test.'
    }
    $secondPort = Get-FreeLoopbackPort
    $second = Start-ArrayProcess -FilePath $Configuration.Executable -Arguments @("--user-data-dir=$profile", "--remote-debugging-port=$secondPort", '--no-first-run', 'about:blank')
    Start-Sleep -Seconds 3
    $secondEndpoint = $null
    try { $secondEndpoint = Get-CdpEndpoint -Port $secondPort } catch {}
    if (-not $second.HasExited -and $null -ne $secondEndpoint) {
      throw 'A second browser instance accepted the already locked temporary user-data-dir.'
    }
    Add-Result -Name "$($Configuration.Name)_browser_profile_lock_refusal" -Status 'PASS' -Detail 'A live Chromium lock prevented a second process from exposing an independent DevTools session for the same temporary profile.'
  } catch {
    Add-Result -Name "$($Configuration.Name)_browser_profile_lock_refusal" -Status 'FAIL' -Detail $_.Exception.Message
  } finally {
    Stop-TestProcess -Process $second
    Stop-TemporaryBrowser -BrowserSession $first
  }
}

function Test-LoopbackProxyFailClosed {
  param([Parameter(Mandatory = $true)]$Configuration)
  $closedPort = Get-FreeLoopbackPort
  $profile = Join-Path (Join-Path $script:ArtifactDirectory $Configuration.Name.ToLowerInvariant()) 'proxy-fail-closed'
  $session = $null
  try {
    Reset-LoopbackEvents
    $session = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profile -RemoteDebuggingPort (Get-FreeLoopbackPort) -ExtraArguments @(
      "--proxy-server=http://127.0.0.1:$closedPort",
      '--proxy-bypass-list=<-loopback>'
    )
    $response = Invoke-CdpNavigate -Connection $session.Connection -Uri "http://127.0.0.1:$script:FixturePort/?op=read&expected="
    $errorText = [string]$response.result.errorText
    $events = Get-LoopbackEvents
    if ($errorText -notmatch 'ERR_PROXY_CONNECTION_FAILED') {
      throw "Expected ERR_PROXY_CONNECTION_FAILED, got '$errorText'."
    }
    if ($events.Count -ne 0) {
      throw "Fixture received $($events.Count) request(s) despite an unreachable loopback proxy."
    }
    Add-Result -Name "$($Configuration.Name)_loopback_proxy_fail_closed" -Status 'PASS' -Detail 'The browser reported ERR_PROXY_CONNECTION_FAILED and the loopback fixture saw no direct request.'
  } catch {
    Add-Result -Name "$($Configuration.Name)_loopback_proxy_fail_closed" -Status 'FAIL' -Detail $_.Exception.Message
  } finally { Stop-TemporaryBrowser -BrowserSession $session }
}

function Test-ExtensionAbsentBehavior {
  param([Parameter(Mandatory = $true)]$Configuration)
  $profile = Join-Path (Join-Path $script:ArtifactDirectory $Configuration.Name.ToLowerInvariant()) 'no-extension'
  $session = $null
  try {
    $session = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profile -RemoteDebuggingPort (Get-FreeLoopbackPort) -ExtraArguments @('--disable-extensions')
    Invoke-CdpNavigate -Connection $session.Connection -Uri "http://127.0.0.1:$script:FixturePort/?op=read&expected=" | Out-Null
    Wait-FixtureTitle -Connection $session.Connection | Out-Null
    $extensionTargets = Get-CdpEndpoint -Port $session.RemoteDebuggingPort | Where-Object { $_.url -like 'chrome-extension://*' }
    if (@($extensionTargets).Count -ne 0) { throw 'A chrome-extension target was present despite --disable-extensions.' }
    Add-Result -Name "$($Configuration.Name)_extension_absent_browser_baseline" -Status 'PASS' -Detail 'A fresh temporary browser with --disable-extensions completed the fixture; no extension target was observed.'
  } catch {
    Add-Result -Name "$($Configuration.Name)_extension_absent_browser_baseline" -Status 'FAIL' -Detail $_.Exception.Message
  } finally { Stop-TemporaryBrowser -BrowserSession $session }
}

function Add-DesktopAcceptanceUnavailable {
  param(
    [Parameter(Mandatory = $true)]$Configuration,
    [Parameter(Mandatory = $true)][ValidateSet('FAIL', 'SKIP', 'BLOCKED')][string]$Status,
    [Parameter(Mandatory = $true)][string]$Detail
  )

  foreach ($name in @(
    "$($Configuration.Name)_desktop_vault_init_unlock_silo_create",
    "$($Configuration.Name)_desktop_isolated_user_data_dir",
    'vault_locked_sensitive_operation_refusal',
    'verisilo_profile_lock_safe_refusal',
    'extension_absent_desktop_degradation',
    'desktop_recovery_after_exception'
  )) {
    Add-Result -Name $name -Status $Status -Detail $Detail
  }
}

function Test-DesktopAcceptance {
  param([Parameter(Mandatory = $true)]$Configuration)

  if (-not $DesktopExe -or -not (Test-Path -LiteralPath $DesktopExe -PathType Leaf)) {
    Add-DesktopAcceptanceUnavailable -Configuration $Configuration -Status 'SKIP' -Detail 'No verified candidate desktop executable was supplied; browser-only evidence is not desktop-core evidence.'
    return
  }
  if (-not $AcceptanceDriverPath -or -not (Test-Path -LiteralPath $AcceptanceDriverPath -PathType Leaf)) {
    Add-DesktopAcceptanceUnavailable -Configuration $Configuration -Status 'SKIP' -Detail 'No acceptance-tests feature-gated desktop-core driver was supplied; production desktop exposes no automation endpoint.'
    return
  }
  if (-not $CandidateDescriptorPath -or -not (Test-Path -LiteralPath $CandidateDescriptorPath -PathType Leaf)) {
    Add-DesktopAcceptanceUnavailable -Configuration $Configuration -Status 'BLOCKED' -Detail 'The acceptance driver was supplied without the verified exact-candidate descriptor.'
    return
  }
  if (-not $ReleaseConfigPath -or -not (Test-Path -LiteralPath $ReleaseConfigPath -PathType Leaf)) {
    Add-DesktopAcceptanceUnavailable -Configuration $Configuration -Status 'BLOCKED' -Detail 'The verified release configuration is required to prove the formal Companion ID is absent from the temporary Silo.'
    return
  }

  $acceptanceRoot = Join-Path ([IO.Path]::GetTempPath()) ("verisilo-desktop-acceptance-" + [guid]::NewGuid().ToString('N'))
  $driverSucceeded = $false
  try {
    $descriptor = Get-Content -Raw -LiteralPath $CandidateDescriptorPath | ConvertFrom-Json
    if ($descriptor.schema -cne 'urn:verisilo:windows-promotion-candidate:1' -or
        $descriptor.schemaVersion -ne 1 -or
        [string]$descriptor.repository -cne 'QianQIUlp/VeriSilo' -or
        ([string]$descriptor.artifactId) -cnotmatch '^[1-9][0-9]{0,15}$' -or
        [string]$descriptor.artifactSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$descriptor.artifactSha256 -cmatch '^0{64}$' -or
        [string]$descriptor.sourceRevision -cnotmatch '^[0-9a-f]{40}$' -or
        $descriptor.acceptanceDriver.sourceRevision -cne [string]$descriptor.sourceRevision -or
        $descriptor.acceptanceDriver.cargoFeature -cne 'acceptance-tests' -or
        $descriptor.acceptanceDriver.cargoTarget -cne 'verisilo-acceptance-driver') {
      throw 'The candidate descriptor has no strict repository/artifact/digest/revision binding.'
    }
    if (-not [string]::Equals(
      (Get-NormalizedPath ([string]$descriptor.files.desktopExe)),
      (Get-NormalizedPath $DesktopExe),
      [StringComparison]::OrdinalIgnoreCase
    )) {
      throw 'DesktopExe does not match the verified exact-candidate descriptor.'
    }
    $releaseConfig = Get-ReleaseConfig -Path $ReleaseConfigPath
    $extensionId = if ($Configuration.Name -eq 'Chrome') {
      [string]$releaseConfig.chromeExtensionId
    } else {
      [string]$releaseConfig.edgeExtensionId
    }

    New-Item -ItemType Directory -Path $acceptanceRoot | Out-Null
    $sentinel = Get-RandomHex
    [IO.File]::WriteAllText(
      (Join-Path $acceptanceRoot '.verisilo-acceptance-sentinel'),
      $sentinel,
      [Text.UTF8Encoding]::new($false)
    )
    $passphrase = "VeriSilo-E2E-$(Get-RandomHex)"
    $request = [ordered]@{
      schema = 'urn:verisilo:windows-acceptance-request:1'
      schemaVersion = 1
      root = $acceptanceRoot
      sentinel = $sentinel
      passphrase = $passphrase
      browser = [ordered]@{
        kind = $Configuration.Name
        executable = $Configuration.Executable
        extensionId = $extensionId
      }
      candidate = [ordered]@{
        repository = [string]$descriptor.repository
        artifactId = [long]$descriptor.artifactId
        artifactSha256 = [string]$descriptor.artifactSha256
        sourceRevision = [string]$descriptor.sourceRevision
      }
    }

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $AcceptanceDriverPath
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardInput = $true
    $startInfo.StandardInputEncoding = [Text.UTF8Encoding]::new($false)
    # Do not pipe stdout/stderr: the real stock browser inherits desktop
    # handles, and an inherited pipe could keep ReadToEnd open after a crash.
    $startInfo.RedirectStandardOutput = $false
    $startInfo.RedirectStandardError = $false
    $startInfo.CreateNoWindow = $true
    # The driver accepts no argv input. The random Vault passphrase exists only in this anonymous stdin pipe.
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) { throw 'Could not start the feature-gated acceptance driver.' }
    try {
      $process.StandardInput.Write(($request | ConvertTo-Json -Compress -Depth 12))
      $process.StandardInput.Close()
      $passphrase = $null
      $request.passphrase = $null
      if (-not $process.WaitForExit(300000)) {
        Stop-TestProcess -Process $process
        throw 'The desktop-core acceptance driver exceeded five minutes.'
      }
      if ($process.ExitCode -ne 0) {
        throw "The desktop-core acceptance driver failed with exit code $($process.ExitCode); see the immediately preceding driver diagnostic."
      }
    } finally {
      Stop-TestProcess -Process $process
      $process.Dispose()
    }

    $driverReceiptPath = Join-Path $acceptanceRoot 'acceptance-receipt.json'
    if (-not (Test-Path -LiteralPath $driverReceiptPath -PathType Leaf)) {
      throw 'The successful driver did not create its sentinel-root receipt.'
    }
    $receiptText = Get-Content -Raw -LiteralPath $driverReceiptPath
    $receipt = $receiptText | ConvertFrom-Json
    $expectedNames = @(
      "$($Configuration.Name)_desktop_vault_init_unlock_silo_create",
      "$($Configuration.Name)_desktop_isolated_user_data_dir",
      'vault_locked_sensitive_operation_refusal',
      'verisilo_profile_lock_safe_refusal',
      'extension_absent_desktop_degradation',
      'desktop_recovery_after_exception'
    )
    $actualNames = @($receipt.results | ForEach-Object { [string]$_.name })
    if ($receipt.schema -cne 'urn:verisilo:windows-acceptance-receipt:1' -or
        $receipt.schemaVersion -ne 1 -or
        $receipt.result -cne 'PASS' -or
        $receipt.candidate.repository -cne [string]$descriptor.repository -or
        $receipt.candidate.artifactId -ne [long]$descriptor.artifactId -or
        $receipt.candidate.artifactSha256 -cne [string]$descriptor.artifactSha256 -or
        $receipt.candidate.sourceRevision -cne [string]$descriptor.sourceRevision -or
        $receipt.driverBuild.sourceRevision -cne [string]$descriptor.sourceRevision -or
        $receipt.driverBuild.cargoFeature -cne 'acceptance-tests' -or
        $receipt.driverBuild.credentialTransport -cne 'anonymous-stdin-pipe' -or
        $receipt.browser.kind -cne $Configuration.Name -or
        -not $receipt.browser.isolatedUserDataDir -or
        $receipt.browser.companionState -cne 'not_connected_no_extension_evidence' -or
        -not $receipt.safety.osTemporaryRootValidated -or
        -not $receipt.safety.randomSentinelValidated -or
        -not $receipt.safety.productionRootsRefused -or
        -not $receipt.safety.exactRuntimeTermination -or
        -not $receipt.safety.unrelatedProcessSurvived -or
        -not $receipt.safety.profilePreserved -or
        @((Compare-Object ($expectedNames | Sort-Object) ($actualNames | Sort-Object))).Count -ne 0 -or
        @($receipt.results | Where-Object { $_.status -cne 'PASS' }).Count -ne 0) {
      throw 'The acceptance receipt is incomplete or is not bound to the exact candidate/browser execution.'
    }

    $receiptPath = Join-Path $script:ArtifactDirectory "desktop-acceptance-$($Configuration.Name).json"
    [IO.File]::WriteAllText($receiptPath, $receiptText, [Text.UTF8Encoding]::new($false))
    foreach ($result in $receipt.results) {
      Add-Result -Name ([string]$result.name) -Status 'PASS' -Detail ([string]$result.detail)
    }
    $driverSucceeded = $true
  } catch {
    Add-DesktopAcceptanceUnavailable -Configuration $Configuration -Status 'FAIL' -Detail $_.Exception.Message
  } finally {
    if (-not $driverSucceeded) {
      Write-Warning "The harness will not issue a blind PID cleanup or delete a possibly live managed Profile after driver failure. Inspect and clean the retained sentinel root manually: $acceptanceRoot"
    } else {
      Remove-Item -LiteralPath $acceptanceRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
  }
}

function Read-Exact {
  param([Parameter(Mandatory = $true)][IO.Stream]$Stream, [Parameter(Mandatory = $true)][int]$Count)
  $buffer = New-Object byte[] $Count
  $offset = 0
  while ($offset -lt $Count) {
    $read = $Stream.Read($buffer, $offset, $Count - $offset)
    if ($read -eq 0) { throw 'Native Host ended its output before a complete frame was received.' }
    $offset += $read
  }
  return $buffer
}

function Invoke-NativeHostFrame {
  param(
    [Parameter(Mandatory = $true)][string]$HostPath,
    [Parameter(Mandatory = $true)][string]$Origin,
    [Parameter(Mandatory = $true)]$Message
  )
  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $HostPath
  $startInfo.UseShellExecute = $false
  $startInfo.RedirectStandardInput = $true
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true
  $startInfo.CreateNoWindow = $true
  [void]$startInfo.ArgumentList.Add($Origin)
  $process = [Diagnostics.Process]::new()
  $process.StartInfo = $startInfo
  if (-not $process.Start()) { throw 'Could not start the Native Host.' }
  try {
    $payload = [Text.Encoding]::UTF8.GetBytes(($Message | ConvertTo-Json -Compress -Depth 20))
    $length = [BitConverter]::GetBytes([uint32]$payload.Length)
    $input = $process.StandardInput.BaseStream
    $input.Write($length, 0, $length.Length)
    $input.Write($payload, 0, $payload.Length)
    $input.Flush()
    $output = $process.StandardOutput.BaseStream
    $responseLength = [BitConverter]::ToUInt32((Read-Exact -Stream $output -Count 4), 0)
    if ($responseLength -gt 16384) { throw "Native Host emitted an oversized $responseLength-byte response." }
    $response = Read-Exact -Stream $output -Count ([int]$responseLength)
    return ([Text.Encoding]::UTF8.GetString($response) | ConvertFrom-Json)
  } finally {
    try { $process.StandardInput.Close() } catch {}
    Stop-TestProcess -Process $process
    $process.Dispose()
  }
}

function Get-ReleaseConfig {
  param([Parameter(Mandatory = $true)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Native Host release configuration is missing: $Path" }
  $config = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
  $placeholderIds = @('abcdefghijklmnopabcdefghijklmnop', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb')
  foreach ($name in @('chromeExtensionId', 'edgeExtensionId')) {
    $value = [string]$config.$name
    if ($value -notmatch '^[a-p]{32}$' -or $placeholderIds -contains $value -or @($value.ToCharArray() | Select-Object -Unique).Count -lt 4) {
      throw "Native Host release configuration has no explicit formal $name value."
    }
  }
  return $config
}

function Test-NativeHost {
  if (-not $NativeHostPath -or -not (Test-Path -LiteralPath $NativeHostPath -PathType Leaf)) {
    Add-Result -Name 'native_host_current_user_registration_and_messages' -Status 'SKIP' -Detail 'No built verisilo-native-host.exe was supplied; no Native Host behavior was simulated.'
    return
  }
  try {
    $config = Get-ReleaseConfig -Path $ReleaseConfigPath
    $verifyScript = Join-Path $PSScriptRoot '..\..\scripts\verify-native-host-install.ps1'
    $verifyArguments = @{
      HostPath = $NativeHostPath
      ReleaseConfigPath = $ReleaseConfigPath
    }
    if ($NativeHostManifestRoot) { $verifyArguments.ManifestRoot = $NativeHostManifestRoot }
    & $verifyScript @verifyArguments
    foreach ($extensionId in @($config.chromeExtensionId, $config.edgeExtensionId)) {
      $requestId = [guid]::NewGuid().ToString()
      $origin = "chrome-extension://$extensionId/"
      $positive = Invoke-NativeHostFrame -HostPath $NativeHostPath -Origin $origin -Message @{ type = 'handshake'; protocolVersion = 1; requestId = $requestId }
      if ($positive.type -ne 'handshake_ack' -or $positive.requestId -ne $requestId -or $positive.protocolVersion -ne 1) {
        throw 'Native Host did not return the expected handshake_ack response.'
      }
    }
    $negative = Invoke-NativeHostFrame -HostPath $NativeHostPath -Origin "chrome-extension://$($config.chromeExtensionId)/" -Message @{ type = 'unknown_message'; protocolVersion = 1; requestId = ([guid]::NewGuid().ToString()) }
    if ($negative.type -ne 'error' -or $negative.code -ne 'invalid_message') {
      throw 'Native Host did not reject an unknown message with invalid_message.'
    }
    Add-Result -Name 'native_host_current_user_registration_and_messages' -Status 'PASS' -Detail 'Verified the real HKCU registration, an allowlisted handshake, and rejection of an unknown framed message.'
  } catch {
    Add-Result -Name 'native_host_current_user_registration_and_messages' -Status 'BLOCKED' -Detail $_.Exception.Message
  }
}

function Invoke-WaitedProcess {
  param([Parameter(Mandatory = $true)][string]$FilePath, [Parameter(Mandatory = $true)][string[]]$Arguments)
  $process = Start-ArrayProcess -FilePath $FilePath -Arguments $Arguments
  if (-not $process.WaitForExit(120000)) {
    Stop-TestProcess -Process $process
    throw "Timed out after two minutes: $FilePath"
  }
  if ($process.ExitCode -ne 0) { throw "$FilePath exited with $($process.ExitCode)." }
}

function Test-NsisLifecycle {
  if (-not $RunNsis) {
    Write-Host '[INFO] nsis_silent_install_upgrade_uninstall_data_retention was not selected. It is a separate destructive V1/V2 lab matrix, not an implicit candidate-promotion case.'
    return
  }
  if (Test-IsAdministrator) {
    Add-Result -Name 'nsis_silent_install_upgrade_uninstall_data_retention' -Status 'BLOCKED' -Detail 'This current-user NSIS scenario must run from an unelevated standard-user session; administrator execution does not verify the intended install scope.'
    return
  }
  try {
    foreach ($artifact in @($NsisInstallerV1, $NsisInstallerV2)) {
      if (-not $artifact -or -not (Test-Path -LiteralPath $artifact -PathType Leaf)) { throw 'Both -NsisInstallerV1 and -NsisInstallerV2 must be real NSIS installer files.' }
    }
    if (-not $InstallDirectory -or -not (Test-Path -LiteralPath $InstallDirectory -PathType Container)) {
      throw 'InstallDirectory must point to the already installed V1 directory after a preflight install; the harness will not infer a default installation directory.'
    }
    if (@($RetainedDataPath).Count -eq 0) {
      throw 'Provide one or more existing real Vault/report/Silo data paths with -RetainedDataPath. The harness will not fabricate retention data.'
    }
    $before = @{}
    foreach ($path in $RetainedDataPath) { $before[$path] = Get-TreeFingerprint -Path $path }
    Invoke-WaitedProcess -FilePath $NsisInstallerV1 -Arguments @('/S')
    Invoke-WaitedProcess -FilePath $NsisInstallerV2 -Arguments @('/S')
    $uninstaller = Join-Path $InstallDirectory 'uninstall.exe'
    if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) { throw "Expected current-user NSIS uninstaller was not found: $uninstaller" }
    Invoke-WaitedProcess -FilePath $uninstaller -Arguments @('/S')
    foreach ($path in $RetainedDataPath) {
      $after = Get-TreeFingerprint -Path $path
      if ($before[$path].Hash -ne $after.Hash -or $before[$path].Entries -ne $after.Entries) {
        throw "Retained desktop data changed during NSIS lifecycle: $path"
      }
    }
    Add-Result -Name 'nsis_silent_install_upgrade_uninstall_data_retention' -Status 'PASS' -Detail 'Real NSIS /S V1 install, V2 upgrade, and uninstall completed while supplied real desktop data fingerprints stayed unchanged.'
  } catch {
    Add-Result -Name 'nsis_silent_install_upgrade_uninstall_data_retention' -Status 'BLOCKED' -Detail $_.Exception.Message
  }
}

function Invoke-SelfTest {
  $temporary = Join-Path ([IO.Path]::GetTempPath()) 'verisilo-e2e-self-test'
  Assert-TemporaryUserDataDirectory -UserDataDirectory $temporary -DefaultUserDataDirectory (Join-Path ([IO.Path]::GetTempPath()) 'not-default')
  $refused = $false
  try { Assert-TemporaryUserDataDirectory -UserDataDirectory 'C:\non-temporary-profile' -DefaultUserDataDirectory 'C:\default' } catch { $refused = $true }
  if (-not $refused) { throw 'Self-test accepted a non-temporary user-data-dir.' }
  Add-Result -Name 'harness_input_safety' -Status 'PASS' -Detail 'The runner rejects default and non-temporary user-data-dir inputs before starting a browser.'
}

function Complete-Run {
  $summaryPath = Join-Path $script:ArtifactDirectory 'summary.json'
  $script:Results | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $summaryPath -Encoding utf8
  $failed = @($script:Results | Where-Object { $_.status -eq 'FAIL' }).Count
  $blocked = @($script:Results | Where-Object { $_.status -eq 'BLOCKED' }).Count
  $skipped = @($script:Results | Where-Object { $_.status -eq 'SKIP' }).Count
  Write-Host "Summary written to $summaryPath"
  if ($failed -gt 0 -or ($RequireAll -and ($blocked -gt 0 -or $skipped -gt 0))) { exit 1 }
}

if ($SelfTest) {
  $script:ArtifactDirectory = Join-Path ([IO.Path]::GetTempPath()) ("verisilo-e2e-self-test-" + [guid]::NewGuid())
  New-Item -ItemType Directory -Force -Path $script:ArtifactDirectory | Out-Null
  try { Invoke-SelfTest; Complete-Run } finally { Remove-Item -LiteralPath $script:ArtifactDirectory -Recurse -Force -ErrorAction SilentlyContinue }
  exit $LASTEXITCODE
}

if (-not (Test-WindowsHost)) {
  Write-Error 'This runner only executes real browser/NSIS cases on Windows. Run node tests/windows/self-test.mjs for a cross-platform static safety check.'
  exit 2
}

$script:ArtifactDirectoryWasProvided = [bool]$ArtifactDirectory
$script:ArtifactDirectory = if ($ArtifactDirectory) { Get-NormalizedPath $ArtifactDirectory } else { Join-Path ([IO.Path]::GetTempPath()) ("verisilo-windows-e2e-" + [guid]::NewGuid()) }
New-Item -ItemType Directory -Force -Path $script:ArtifactDirectory | Out-Null

try {
  Test-OperatingSystemEvidence
  Start-LoopbackFixture
  $selected = if ($Browser -eq 'Both') { @('Chrome', 'Edge') } else { @($Browser) }
  foreach ($name in $selected) {
    $configuration = Get-BrowserConfiguration -Name $name
    if (-not $configuration.Executable) {
      Add-Result -Name "$name_browser_cases" -Status 'SKIP' -Detail "No $name executable was found. Supply -$($name)Path to run real browser cases."
      continue
    }
    Test-DefaultProfileInvariant -Configuration $configuration -Exercise {
      Test-BrowserStorageIsolation -Configuration $configuration
      Test-BrowserLockBehavior -Configuration $configuration
      Test-LoopbackProxyFailClosed -Configuration $configuration
      Test-ExtensionAbsentBehavior -Configuration $configuration
      Test-DesktopAcceptance -Configuration $configuration
    }
  }
  Test-NativeHost
  Test-NsisLifecycle
} finally {
  Stop-TestProcess -Process $script:FixtureProcess
  if ($script:FixtureOutput) {
    try {
      $stdout = $script:FixtureOutput.StdoutTask.GetAwaiter().GetResult()
      Set-Content -LiteralPath $script:FixtureOutput.StdoutPath -Value $stdout -Encoding utf8
      $stderr = $script:FixtureOutput.StderrTask.GetAwaiter().GetResult()
      Set-Content -LiteralPath $script:FixtureOutput.StderrPath -Value $stderr -Encoding utf8
    } catch {
      Write-Warning "Could not collect fixture logs: $($_.Exception.Message)"
    }
  }
  Complete-Run
  if ($KeepArtifacts -or $script:ArtifactDirectoryWasProvided) {
    Write-Host "Artifacts retained for inspection: $script:ArtifactDirectory"
  } else {
    Remove-Item -LiteralPath $script:ArtifactDirectory -Recurse -Force -ErrorAction SilentlyContinue
  }
}
