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
$script:FixtureProcess = $null
$script:FixtureOutput = $null
$script:ActiveFixturePort = 0
$script:FixtureToken = $null
$script:ResolvedArtifactDirectory = $null
$script:ArtifactDirectoryWasProvided = $false
$script:ArtifactSentinel = $null
$script:RequestedFixturePort = $FixturePort
$script:RequestedArtifactDirectory = $ArtifactDirectory
$script:NativeHostProtocolVersion = 2

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
  $operatingSystem = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop
  $build = 0
  if (-not [int]::TryParse([string]$operatingSystem.BuildNumber, [ref]$build) -or $build -lt 10240) {
    throw "Win32_OperatingSystem returned an unsupported BuildNumber '$($operatingSystem.BuildNumber)'."
  }
  if ([int]$operatingSystem.ProductType -ne 1) {
    Add-Result -Name 'windows_matrix_target' -Status 'BLOCKED' -Detail "Win32_OperatingSystem reports ProductType $($operatingSystem.ProductType); the desktop acceptance matrix requires a Windows client workstation, not Windows Server."
    return
  }
  $actualWindowsVersion = Get-WindowsFamilyFromBuild -Build $build
  $caption = [string]$operatingSystem.Caption
  $version = [string]$operatingSystem.Version
  $detail = "$caption version $version (build $build; classified as $actualWindowsVersion)"
  if (-not $ExpectedWindowsVersion) {
    Add-Result -Name 'windows_matrix_target' -Status 'SKIP' -Detail "Host is $detail, but no -ExpectedWindowsVersion was declared. Run once for Windows 10 and once for Windows 11 before claiming the matrix."
    return
  }
  if ($actualWindowsVersion -cne $ExpectedWindowsVersion) {
    Add-Result -Name 'windows_matrix_target' -Status 'BLOCKED' -Detail "Expected $ExpectedWindowsVersion but Win32_OperatingSystem reports $detail."
    return
  }
  Add-Result -Name 'windows_matrix_target' -Status 'PASS' -Detail "Validated $detail as the declared $ExpectedWindowsVersion target."
}

function Get-WindowsFamilyFromBuild {
  param([ValidateRange(10240, [int]::MaxValue)][int]$Build)
  if ($Build -ge 22000) { return 'Windows 11' }
  return 'Windows 10'
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

function Test-StrictPathDescendant {
  param(
    [Parameter(Mandatory = $true)][string]$Candidate,
    [Parameter(Mandatory = $true)][string]$Ancestor
  )
  $normalizedCandidate = Get-NormalizedPath $Candidate
  $normalizedAncestor = Get-NormalizedPath $Ancestor
  return -not [string]::Equals($normalizedCandidate, $normalizedAncestor, [StringComparison]::OrdinalIgnoreCase) -and
    $normalizedCandidate.StartsWith(
      "$normalizedAncestor$([IO.Path]::DirectorySeparatorChar)",
      [StringComparison]::OrdinalIgnoreCase
    )
}

function Assert-NoReparsePointPath {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$TrustedAncestor
  )
  $candidate = Get-NormalizedPath $Path
  $ancestor = Get-NormalizedPath $TrustedAncestor
  $ancestorItem = Get-Item -LiteralPath $ancestor -Force
  if (($ancestorItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "Trusted temporary root must not be a reparse point: $ancestor"
  }
  if ([string]::Equals($candidate, $ancestor, [StringComparison]::OrdinalIgnoreCase)) {
    return
  }
  if (-not (Test-StrictPathDescendant -Candidate $candidate -Ancestor $ancestor)) {
    throw "Path is not a strict descendant of the trusted root: $candidate"
  }
  $relative = $candidate.Substring($ancestor.Length).TrimStart([char]'\', [char]'/')
  $current = $ancestor
  foreach ($segment in $relative -split '[\\/]') {
    $current = Join-Path $current $segment
    if (-not (Test-Path -LiteralPath $current)) { continue }
    $item = Get-Item -LiteralPath $current -Force
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "Refusing a reparse-point path inside the acceptance root: $($item.FullName)"
    }
  }
}

function New-TemporaryArtifactDirectory {
  param([Parameter(Mandatory = $true)][string]$Path)
  $temporaryRoot = Get-NormalizedPath ([IO.Path]::GetTempPath())
  $candidate = Get-NormalizedPath $Path
  if (-not (Test-StrictPathDescendant -Candidate $candidate -Ancestor $temporaryRoot)) {
    throw "ArtifactDirectory must be a strict descendant of the OS temporary root: $candidate"
  }
  if (Test-Path -LiteralPath $candidate) {
    throw "Refusing to reuse an existing acceptance ArtifactDirectory: $candidate"
  }
  $parent = Split-Path -Parent $candidate
  Assert-NoReparsePointPath -Path $parent -TrustedAncestor $temporaryRoot
  [void](New-Item -ItemType Directory -Path $candidate)
  Assert-NoReparsePointPath -Path $candidate -TrustedAncestor $temporaryRoot
  $script:ArtifactSentinel = Get-RandomHex
  [IO.File]::WriteAllText(
    (Join-Path $candidate '.verisilo-e2e-sentinel'),
    $script:ArtifactSentinel,
    [Text.UTF8Encoding]::new($false)
  )
  return $candidate
}

function Remove-TemporaryArtifactDirectory {
  param([Parameter(Mandatory = $true)][string]$Path)
  $temporaryRoot = Get-NormalizedPath ([IO.Path]::GetTempPath())
  $candidate = Get-NormalizedPath $Path
  Assert-NoReparsePointPath -Path $candidate -TrustedAncestor $temporaryRoot
  $sentinelPath = Join-Path $candidate '.verisilo-e2e-sentinel'
  if (-not (Test-Path -LiteralPath $sentinelPath -PathType Leaf) -or
      [IO.File]::ReadAllText($sentinelPath) -cne $script:ArtifactSentinel) {
    throw "Refusing to remove an artifact directory without its exact run sentinel: $candidate"
  }
  Remove-Item -LiteralPath $candidate -Recurse -Force
}

function Get-RandomHex {
  param([ValidateRange(16, 128)][int]$Bytes = 32)
  $buffer = New-Object byte[] $Bytes
  $generator = [Security.Cryptography.RandomNumberGenerator]::Create()
  try {
    $generator.GetBytes($buffer)
  } finally {
    $generator.Dispose()
  }
  return [BitConverter]::ToString($buffer).Replace('-', '').ToLowerInvariant()
}

function New-ShortAcceptanceLeaf {
  # The separate 256-bit sentinel authenticates the root. This 64-bit token
  # only keeps concurrent local runs distinct while preserving Chromium's
  # remaining path budget for deeply nested extension/cache entries.
  $token = (Get-RandomHex -Bytes 16).Substring(0, 16)
  return "vda-$token"
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
  Assert-NoReparsePointPath -Path $candidate -TrustedAncestor $temporaryRoot
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
  $client.Timeout = [TimeSpan]::FromSeconds(2)
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
  $script:ActiveFixturePort = if ($FixturePort -gt 0) { $FixturePort } else { Get-FreeLoopbackPort }
  if ($script:ActiveFixturePort -lt 1 -or $script:ActiveFixturePort -gt 65535) {
    throw 'FixturePort must be a TCP port between 1 and 65535.'
  }
  $script:FixtureToken = Get-RandomHex
  $fixture = Join-Path $PSScriptRoot 'fixtures\loopback-server.mjs'
  $stdout = Join-Path $script:ResolvedArtifactDirectory 'fixture.stdout.log'
  $stderr = Join-Path $script:ResolvedArtifactDirectory 'fixture.stderr.log'
  $started = Start-ProcessToFiles -FilePath $node.Source -Arguments @(
    $fixture,
    '--host', '127.0.0.1',
    '--port', "$script:ActiveFixturePort",
    '--token', $script:FixtureToken
  ) -StandardOutputPath $stdout -StandardErrorPath $stderr
  $script:FixtureProcess = $started.Process
  $script:FixtureOutput = $started
  for ($attempt = 0; $attempt -lt 40; $attempt++) {
    try {
      if ($script:FixtureProcess.HasExited) {
        throw "The loopback fixture exited with code $($script:FixtureProcess.ExitCode)."
      }
      $healthJson = Invoke-LoopbackRequest -Method GET -Uri (
        "http://127.0.0.1:$script:ActiveFixturePort/__health?harnessToken=$script:FixtureToken"
      )
      $health = $healthJson | ConvertFrom-Json
      if ($health.schema -cne 'urn:verisilo:windows-e2e-fixture-health:1' -or
          $health.token -cne $script:FixtureToken) {
        throw 'The loopback port answered without the exact per-run fixture identity.'
      }
      return
    } catch {
      Start-Sleep -Milliseconds 125
    }
  }
  throw "The loopback fixture did not become reachable. See $stderr"
}

function Get-LoopbackEvents {
  $json = Invoke-LoopbackRequest -Method GET -Uri (
    "http://127.0.0.1:$script:ActiveFixturePort/__events?harnessToken=$script:FixtureToken"
  )
  return ,(ConvertFrom-JsonArray -Json $json)
}

function Reset-LoopbackEvents {
  Invoke-LoopbackRequest -Method POST -Uri (
    "http://127.0.0.1:$script:ActiveFixturePort/__reset?harnessToken=$script:FixtureToken"
  ) | Out-Null
}

function ConvertFrom-JsonArray {
  param([Parameter(Mandatory = $true)][string]$Json)
  if ($Json -match '^\s*\[\s*\]\s*$') {
    Write-Output -NoEnumerate ([object[]]@())
    return
  }
  return ,@($Json | ConvertFrom-Json)
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
  $cancellation = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
  try {
    [void]$socket.ConnectAsync([Uri]$WebSocketDebuggerUrl, $cancellation.Token).GetAwaiter().GetResult()
    return [pscustomobject]@{ Socket = $socket; NextId = 1 }
  } catch {
    $socket.Dispose()
    throw
  } finally {
    $cancellation.Dispose()
  }
}

function Receive-CdpText {
  param([Parameter(Mandatory = $true)]$Connection)
  $buffer = New-Object byte[] 65536
  $stream = [IO.MemoryStream]::new()
  $cancellation = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
  try {
    do {
      $result = $Connection.Socket.ReceiveAsync([ArraySegment[byte]]::new($buffer), $cancellation.Token).GetAwaiter().GetResult()
      if ($result.MessageType -eq [System.Net.WebSockets.WebSocketMessageType]::Close) {
        throw 'DevTools closed the WebSocket before it returned a response.'
      }
      $stream.Write($buffer, 0, $result.Count)
      if ($stream.Length -gt 1048576) {
        throw 'DevTools emitted an oversized response.'
      }
    } while (-not $result.EndOfMessage)
    return [Text.Encoding]::UTF8.GetString($stream.ToArray())
  } catch [OperationCanceledException] {
    throw 'DevTools did not return a response within ten seconds.'
  } finally {
    $cancellation.Dispose()
    $stream.Dispose()
  }
}

function Get-CdpErrorMessage {
  param([Parameter(Mandatory = $true)]$Message)
  if ($null -eq $Message.PSObject.Properties['error']) { return $null }
  if ($null -ne $Message.error -and $null -ne $Message.error.PSObject.Properties['message']) {
    return [string]$Message.error.message
  }
  return ($Message.error | ConvertTo-Json -Compress -Depth 5)
}

function Get-NextCdpId {
  param([Parameter(Mandatory = $true)]$Connection)
  $id = $Connection.NextId
  [void]($Connection.NextId++)
  return $id
}

function Invoke-Cdp {
  param(
    [Parameter(Mandatory = $true)]$Connection,
    [Parameter(Mandatory = $true)][string]$Method,
    [hashtable]$Params = @{}
  )
  $id = Get-NextCdpId -Connection $Connection
  $payload = @{ id = $id; method = $Method; params = $Params } | ConvertTo-Json -Compress -Depth 20
  $bytes = [Text.Encoding]::UTF8.GetBytes($payload)
  $cancellation = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
  try {
    [void]$Connection.Socket.SendAsync(
      [ArraySegment[byte]]::new($bytes),
      [System.Net.WebSockets.WebSocketMessageType]::Text,
      $true,
      $cancellation.Token
    ).GetAwaiter().GetResult()
  } catch [OperationCanceledException] {
    throw "DevTools $Method could not be sent within ten seconds."
  } finally {
    $cancellation.Dispose()
  }
  while ($true) {
    $message = Receive-CdpText -Connection $Connection | ConvertFrom-Json
    $messageId = $message.PSObject.Properties['id']
    if ($null -ne $messageId -and [long]$message.id -eq $id) {
      $errorMessage = Get-CdpErrorMessage -Message $message
      if ($null -ne $errorMessage) { throw "DevTools $Method failed: $errorMessage" }
      return $message
    }
  }
}

function Invoke-CdpNavigate {
  param([Parameter(Mandatory = $true)]$Connection, [Parameter(Mandatory = $true)][string]$Uri)
  return Invoke-Cdp -Connection $Connection -Method 'Page.navigate' -Params @{ url = $Uri }
}

function Wait-FixtureTitle {
  param(
    [Parameter(Mandatory = $true)]$Connection,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-f]{32}$')][string]$ExpectedToken
  )
  $passPrefix = "VERISILO_E2E:PASS:$ExpectedToken`:"
  $failPrefix = "VERISILO_E2E:FAIL:$ExpectedToken`:"
  for ($attempt = 0; $attempt -lt 80; $attempt++) {
    $response = Invoke-Cdp -Connection $Connection -Method 'Runtime.evaluate' -Params @{ expression = 'document.title'; returnByValue = $true }
    $title = [string]$response.result.result.value
    if ($title.StartsWith($passPrefix, [StringComparison]::Ordinal)) { return $title }
    if ($title.StartsWith($failPrefix, [StringComparison]::Ordinal)) { throw "Fixture assertion failed: $title" }
    Start-Sleep -Milliseconds 125
  }
  throw "Fixture page did not report a PASS/FAIL title for operation token $ExpectedToken before the timeout."
}

function Invoke-FixtureCase {
  param(
    [Parameter(Mandatory = $true)]$Connection,
    [Parameter(Mandatory = $true)][string]$Query
  )
  $operationToken = Get-RandomHex -Bytes 16
  $uri = "http://127.0.0.1:$script:ActiveFixturePort/?$Query&harnessToken=$script:FixtureToken&operationToken=$operationToken"
  Invoke-CdpNavigate -Connection $Connection -Uri $uri | Out-Null
  return Wait-FixtureTitle -Connection $Connection -ExpectedToken $operationToken
}

function Start-TemporaryBrowser {
  param(
    [Parameter(Mandatory = $true)]$Configuration,
    [Parameter(Mandatory = $true)][string]$UserDataDirectory,
    [Parameter(Mandatory = $true)][int]$RemoteDebuggingPort,
    [string[]]$ExtraArguments = @()
  )

  $userDataExists = Test-Path -LiteralPath $UserDataDirectory
  if (-not $userDataExists) {
    [void](New-Item -ItemType Directory -Path $UserDataDirectory)
  }
  Assert-TemporaryUserDataDirectory -UserDataDirectory $UserDataDirectory -DefaultUserDataDirectory $Configuration.DefaultUserData
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
  $process = $null
  $connection = $null
  try {
    $process = Start-ArrayProcess -FilePath $Configuration.Executable -Arguments $arguments
    $page = Wait-CdpPage -Port $RemoteDebuggingPort
    $connection = Connect-Cdp -WebSocketDebuggerUrl $page.webSocketDebuggerUrl
    return [pscustomobject]@{
      Process = $process
      Connection = $connection
      UserDataDirectory = $UserDataDirectory
      RemoteDebuggingPort = $RemoteDebuggingPort
    }
  } catch {
    if ($null -ne $connection) {
      try { $connection.Socket.Dispose() } catch {}
    }
    Stop-TestProcess -Process $process
    throw
  }
}

function Stop-TemporaryBrowser {
  param($BrowserSession)
  if ($null -eq $BrowserSession) { return $true }
  $graceful = $false
  try {
    if ($BrowserSession.Connection.Socket.State -eq [System.Net.WebSockets.WebSocketState]::Open) {
      try {
        [void](Invoke-Cdp -Connection $BrowserSession.Connection -Method 'Browser.close')
      } catch {
        # Browser.close may tear down the DevTools socket before its acknowledgement.
      }

      # The bootstrap process and the DevTools endpoint can disappear a few moments apart.
      # Poll both signals within one fixed deadline instead of treating that interval as a leak.
      $deadline = [DateTime]::UtcNow.AddSeconds(15)
      do {
        $processExited = $false
        try { $processExited = $BrowserSession.Process.HasExited } catch {}

        $endpointClosed = $false
        try {
          $endpointClosed = @(Get-CdpEndpoint -Port $BrowserSession.RemoteDebuggingPort).Count -eq 0
        } catch {
          # A refused DevTools connection is the expected post-close state.
          $endpointClosed = $true
        }

        if ($processExited -and $endpointClosed) {
          $graceful = $true
          break
        }
        Start-Sleep -Milliseconds 125
      } while ([DateTime]::UtcNow -lt $deadline)
    }
  } catch {}
  try { $BrowserSession.Connection.Socket.Dispose() } catch {}
  if (-not $graceful) {
    Stop-TestProcess -Process $BrowserSession.Process
  }
  return $graceful
}

function Wait-CdpEndpointStable {
  param([Parameter(Mandatory = $true)][int]$Port)
  for ($attempt = 0; $attempt -lt 4; $attempt++) {
    if (@(Get-CdpEndpoint -Port $Port | Where-Object { $_.type -eq 'page' }).Count -eq 0) {
      throw "The original browser DevTools endpoint was not stable on port $Port."
    }
    Start-Sleep -Milliseconds 125
  }
}

function Get-Sha256HexFromBytes {
  param([Parameter(Mandatory = $true)][byte[]]$Bytes)
  $sha256 = [Security.Cryptography.SHA256]::Create()
  try {
    $digest = $sha256.ComputeHash($Bytes)
  } finally {
    $sha256.Dispose()
  }
  return ([BitConverter]::ToString($digest)).Replace('-', '')
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
  return [pscustomobject]@{ Hash = (Get-Sha256HexFromBytes -Bytes $serialized); Entries = $records.Count }
}

function Get-TreeMetadataFingerprint {
  param([Parameter(Mandatory = $true)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
    throw "Path does not exist: $Path"
  }
  $records = [System.Collections.Generic.List[string]]::new()
  $root = Get-NormalizedPath $Path
  $rootItem = Get-Item -LiteralPath $root -Force -ErrorAction Stop
  if (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "Refusing an incomplete metadata fingerprint: reparse point found at $root"
  }
  $records.Add("D|.|$($rootItem.CreationTimeUtc.Ticks)|$($rootItem.LastWriteTimeUtc.Ticks)|$([int]$rootItem.Attributes)")
  foreach ($item in Get-ChildItem -LiteralPath $root -Recurse -Force -ErrorAction Stop | Sort-Object FullName) {
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "Refusing an incomplete metadata fingerprint: reparse point found at $($item.FullName)"
    }
    $relative = $item.FullName.Substring($root.Length).TrimStart([char]'\', [char]'/' )
    if ($item.PSIsContainer) {
      $records.Add("D|$relative|$($item.CreationTimeUtc.Ticks)|$($item.LastWriteTimeUtc.Ticks)|$([int]$item.Attributes)")
    } else {
      $records.Add("F|$relative|$($item.Length)|$($item.CreationTimeUtc.Ticks)|$($item.LastWriteTimeUtc.Ticks)|$([int]$item.Attributes)")
    }
  }
  $serialized = [Text.Encoding]::UTF8.GetBytes(($records -join "`n"))
  return [pscustomobject]@{ Hash = (Get-Sha256HexFromBytes -Bytes $serialized); Entries = $records.Count }
}

function Test-DefaultProfileInvariant {
  param([Parameter(Mandatory = $true)]$Configuration, [Parameter(Mandatory = $true)][scriptblock]$Exercise)
  if (Get-Process -Name $Configuration.ProcessName -ErrorAction SilentlyContinue) {
    Add-Result -Name "$($Configuration.Name)_default_profile_unchanged" -Status 'BLOCKED' -Detail 'A browser process is already running. The harness stopped this browser configuration before any real acceptance case and did not close or alter that process.'
    return
  }
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
  try {
    $before = Get-TreeMetadataFingerprint -Path $Configuration.DefaultProfile
    & $Exercise
    if (Get-Process -Name $Configuration.ProcessName -ErrorAction SilentlyContinue) {
      Add-Result -Name "$($Configuration.Name)_default_profile_unchanged" -Status 'BLOCKED' -Detail 'A browser process appeared during the run and could have mutated the default profile; no clean before/after claim was made.'
      return
    }
    $after = Get-TreeMetadataFingerprint -Path $Configuration.DefaultProfile
    if ($before.Hash -ne $after.Hash -or $before.Entries -ne $after.Entries) {
      throw "default profile metadata changed ($($before.Hash) -> $($after.Hash))."
    }
    Add-Result -Name "$($Configuration.Name)_default_profile_unchanged" -Status 'PASS' -Detail "Metadata-only tree fingerprint unchanged across $($before.Entries) entries; no default Profile file contents were read."
  } catch {
    Add-Result -Name "$($Configuration.Name)_default_profile_unchanged" -Status 'FAIL' -Detail $_.Exception.Message
  }
}

function Test-BrowserStorageIsolation {
  param([Parameter(Mandatory = $true)]$Configuration)
  $browserRoot = Join-Path $script:ResolvedArtifactDirectory $Configuration.Name.ToLowerInvariant()
  $profileA = Join-Path $browserRoot 'profile-a'
  $profileB = Join-Path $browserRoot 'profile-b'
  $session = $null
  try {
    $session = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profileA -RemoteDebuggingPort (Get-FreeLoopbackPort)
    Invoke-FixtureCase -Connection $session.Connection -Query 'op=write&value=A' | Out-Null
    Invoke-FixtureCase -Connection $session.Connection -Query 'op=read&expected=A' | Out-Null
    if (-not (Stop-TemporaryBrowser -BrowserSession $session)) {
      throw 'The first A browser did not close gracefully, so persistence/session-lifecycle evidence is invalid.'
    }
    $session = $null

    $session = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profileB -RemoteDebuggingPort (Get-FreeLoopbackPort)
    Invoke-FixtureCase -Connection $session.Connection -Query 'op=read&expected=' | Out-Null
    if (-not (Stop-TemporaryBrowser -BrowserSession $session)) {
      throw 'The B browser did not close gracefully, so profile-isolation evidence is invalid.'
    }
    $session = $null

    $session = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profileA -RemoteDebuggingPort (Get-FreeLoopbackPort)
    Invoke-FixtureCase -Connection $session.Connection -Query 'op=read-lifecycle&expectedPersistent=A&expectedEphemeral=' | Out-Null
    if (-not (Stop-TemporaryBrowser -BrowserSession $session)) {
      throw 'The restarted A browser did not close gracefully after lifecycle verification.'
    }
    $session = $null
    Add-Result -Name "$($Configuration.Name)_temporary_A_B_storage_cookie_isolation" -Status 'PASS' -Detail 'Temporary A/B user-data-dir profiles isolated all five markers; after a full browser restart, A retained localStorage, the persistent cookie, and IndexedDB while sessionStorage and the session cookie were cleared.'
  } catch {
    Add-Result -Name "$($Configuration.Name)_temporary_A_B_storage_cookie_isolation" -Status 'FAIL' -Detail $_.Exception.Message
  } finally { [void](Stop-TemporaryBrowser -BrowserSession $session) }
}

function Test-BrowserLockBehavior {
  param([Parameter(Mandatory = $true)]$Configuration)
  $profile = Join-Path (Join-Path $script:ResolvedArtifactDirectory $Configuration.Name.ToLowerInvariant()) 'lock-profile'
  $first = $null
  $second = $null
  try {
    $first = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profile -RemoteDebuggingPort (Get-FreeLoopbackPort)
    Wait-CdpEndpointStable -Port $first.RemoteDebuggingPort
    $secondPort = Get-FreeLoopbackPort
    $second = Start-ArrayProcess -FilePath $Configuration.Executable -Arguments @(
      "--user-data-dir=$profile",
      "--remote-debugging-port=$secondPort",
      '--remote-allow-origins=*',
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-background-networking',
      'about:blank'
    )
    Start-Sleep -Seconds 3
    # Keep the expected connection-refused result as an actual empty array.
    # In PowerShell, @($null).Count is 1 and would falsely report contention
    # failure whenever the second DevTools connection was correctly refused.
    $secondEndpoint = @()
    try { $secondEndpoint = Get-CdpEndpoint -Port $secondPort } catch {}
    if ($secondEndpoint.Count -ne 0) {
      $endpointDetail = $secondEndpoint | ConvertTo-Json -Compress -Depth 5
      throw "A second browser instance accepted the already locked temporary user-data-dir: $endpointDetail"
    }
    if (@(Get-CdpEndpoint -Port $first.RemoteDebuggingPort).Count -eq 0) {
      throw 'The original browser lost its DevTools endpoint during the same-profile contention test.'
    }
    Invoke-FixtureCase -Connection $first.Connection -Query 'op=read&expected=' | Out-Null
    Add-Result -Name "$($Configuration.Name)_browser_profile_lock_refusal" -Status 'PASS' -Detail 'The live Windows Chromium profile refused an independent second DevTools endpoint while the original profile owner remained usable; no Unix-only Singleton marker was assumed.'
  } catch {
    Add-Result -Name "$($Configuration.Name)_browser_profile_lock_refusal" -Status 'FAIL' -Detail $_.Exception.Message
  } finally {
    Stop-TestProcess -Process $second
    [void](Stop-TemporaryBrowser -BrowserSession $first)
  }
}

function Test-LoopbackProxyFailClosed {
  param([Parameter(Mandatory = $true)]$Configuration)
  $closedPort = Get-FreeLoopbackPort
  $profile = Join-Path (Join-Path $script:ResolvedArtifactDirectory $Configuration.Name.ToLowerInvariant()) 'proxy-fail-closed'
  $session = $null
  try {
    Reset-LoopbackEvents
    $session = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profile -RemoteDebuggingPort (Get-FreeLoopbackPort) -ExtraArguments @(
      "--proxy-server=http://127.0.0.1:$closedPort",
      '--proxy-bypass-list=<-loopback>'
    )
    $response = Invoke-CdpNavigate -Connection $session.Connection -Uri (
      "http://127.0.0.1:$script:ActiveFixturePort/?op=read&expected=&harnessToken=$script:FixtureToken&operationToken=$(Get-RandomHex -Bytes 16)"
    )
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
  } finally { [void](Stop-TemporaryBrowser -BrowserSession $session) }
}

function Test-ExtensionAbsentBehavior {
  param([Parameter(Mandatory = $true)]$Configuration)
  if (-not $ReleaseConfigPath) {
    Add-Result -Name "$($Configuration.Name)_extension_absent_browser_baseline" -Status 'SKIP' -Detail 'Supply the verified -ReleaseConfigPath so absence can be evaluated against the formal VeriSilo extension ID instead of unrelated Chromium extension targets.'
    return
  }
  $profile = Join-Path (Join-Path $script:ResolvedArtifactDirectory $Configuration.Name.ToLowerInvariant()) 'no-extension'
  $session = $null
  try {
    $releaseConfig = Get-ReleaseConfig -Path $ReleaseConfigPath
    $extensionId = if ($Configuration.Name -eq 'Chrome') {
      [string]$releaseConfig.chromeExtensionId
    } else {
      [string]$releaseConfig.edgeExtensionId
    }
    $originPrefix = "chrome-extension://$extensionId/"
    $session = Start-TemporaryBrowser -Configuration $Configuration -UserDataDirectory $profile -RemoteDebuggingPort (Get-FreeLoopbackPort) -ExtraArguments @('--disable-extensions')
    Invoke-FixtureCase -Connection $session.Connection -Query 'op=read&expected=' | Out-Null
    $veriSiloTargets = Get-CdpEndpoint -Port $session.RemoteDebuggingPort | Where-Object {
      Test-VeriSiloExtensionTarget -Target $_ -OriginPrefix $originPrefix
    }
    if (@($veriSiloTargets).Count -ne 0) { throw "The formal VeriSilo extension target $originPrefix was present despite --disable-extensions." }
    Add-Result -Name "$($Configuration.Name)_extension_absent_browser_baseline" -Status 'PASS' -Detail "A fresh temporary browser with --disable-extensions completed the fixture; the formal VeriSilo extension ID $extensionId was absent. Unrelated extension targets were ignored."
  } catch {
    Add-Result -Name "$($Configuration.Name)_extension_absent_browser_baseline" -Status 'FAIL' -Detail $_.Exception.Message
  } finally { [void](Stop-TemporaryBrowser -BrowserSession $session) }
}

function Test-VeriSiloExtensionTarget {
  param(
    [Parameter(Mandatory = $true)]$Target,
    [Parameter(Mandatory = $true)][string]$OriginPrefix
  )
  $urlProperty = $Target.PSObject.Properties['url']
  return $null -ne $urlProperty -and
    $Target.url -is [string] -and
    $Target.url.StartsWith($OriginPrefix, [StringComparison]::Ordinal)
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

  # Keep the managed browser Profile below legacy Windows path limits even when
  # the caller deliberately places TEMP under a long, isolated evidence root.
  $acceptanceRoot = Join-Path ([IO.Path]::GetTempPath()) (New-ShortAcceptanceLeaf)
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

    $receiptPath = Join-Path $script:ResolvedArtifactDirectory "desktop-acceptance-$($Configuration.Name).json"
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
  param(
    [Parameter(Mandatory = $true)][IO.Stream]$Stream,
    [Parameter(Mandatory = $true)][int]$Count,
    [Parameter(Mandatory = $true)][Threading.CancellationToken]$CancellationToken
  )
  $buffer = New-Object byte[] $Count
  $offset = 0
  while ($offset -lt $Count) {
    try {
      $read = $Stream.ReadAsync(
        $buffer,
        $offset,
        $Count - $offset,
        $CancellationToken
      ).GetAwaiter().GetResult()
    } catch [OperationCanceledException] {
      throw 'Native Host did not return a complete frame within fifteen seconds.'
    }
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
  # Do not create an unread stderr pipe: a noisy failed Host must not deadlock
  # the bounded framed stdout exchange.
  $startInfo.RedirectStandardError = $false
  $startInfo.CreateNoWindow = $true
  [void]$startInfo.ArgumentList.Add($Origin)
  $process = [Diagnostics.Process]::new()
  $process.StartInfo = $startInfo
  if (-not $process.Start()) { throw 'Could not start the Native Host.' }
  $cancellation = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(15))
  $parsedResponse = $null
  try {
    $payload = [Text.Encoding]::UTF8.GetBytes(($Message | ConvertTo-Json -Compress -Depth 20))
    $length = [BitConverter]::GetBytes([uint32]$payload.Length)
    $input = $process.StandardInput.BaseStream
    $input.Write($length, 0, $length.Length)
    $input.Write($payload, 0, $payload.Length)
    $input.Flush()
    $output = $process.StandardOutput.BaseStream
    $responseLength = [BitConverter]::ToUInt32(
      (Read-Exact -Stream $output -Count 4 -CancellationToken $cancellation.Token),
      0
    )
    if ($responseLength -gt 16384) { throw "Native Host emitted an oversized $responseLength-byte response." }
    $response = Read-Exact -Stream $output -Count ([int]$responseLength) -CancellationToken $cancellation.Token
    $parsedResponse = [Text.Encoding]::UTF8.GetString($response) | ConvertFrom-Json
  } finally {
    try { $process.StandardInput.Close() } catch {}
    Stop-TestProcess -Process $process
    $cancellation.Dispose()
    $process.Dispose()
  }
  return $parsedResponse
}

function Get-NonAllowlistedNativeHostExtensionId {
  param([Parameter(Mandatory = $true)][string[]]$AllowedExtensionIds)

  $candidatePrefix = ('a' * 28) + 'bcd'
  foreach ($suffix in [char[]]'abcdefghijklmnop') {
    $candidate = $candidatePrefix + [string]$suffix
    if (-not ($AllowedExtensionIds -ccontains $candidate)) {
      return $candidate
    }
  }
  throw 'Could not construct a syntactically valid Native Host extension ID outside the release allowlist.'
}

function Assert-NativeHostRejectsUnauthorizedOrigin {
  param(
    [Parameter(Mandatory = $true)][string]$HostPath,
    [Parameter(Mandatory = $true)][string]$Origin
  )

  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $HostPath
  $startInfo.UseShellExecute = $false
  $startInfo.RedirectStandardInput = $true
  $startInfo.RedirectStandardOutput = $true
  # Keep stderr inherited so a noisy failure cannot fill an unread redirected pipe.
  $startInfo.RedirectStandardError = $false
  $startInfo.CreateNoWindow = $true
  [void]$startInfo.ArgumentList.Add($Origin)

  $process = [Diagnostics.Process]::new()
  $process.StartInfo = $startInfo
  $stdout = [IO.MemoryStream]::new()
  $cancellation = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(15))
  $copyTask = $null
  $started = $false
  try {
    if (-not $process.Start()) { throw 'Could not start the Native Host for the unauthorized-origin check.' }
    $started = $true
    $copyTask = $process.StandardOutput.BaseStream.CopyToAsync(
      $stdout,
      81920,
      $cancellation.Token
    )
    # The Host must reject the origin before it attempts to read a framed message.
    if (-not $process.WaitForExit(15000)) {
      throw 'Native Host did not reject a non-allowlisted origin within fifteen seconds.'
    }
    try {
      $copyTask.GetAwaiter().GetResult()
    } catch [OperationCanceledException] {
      throw 'Native Host stdout did not close after the non-allowlisted origin was rejected.'
    }
    if ($process.ExitCode -eq 0) {
      throw 'Native Host accepted a syntactically valid non-allowlisted origin.'
    }
    if ($stdout.Length -ne 0) {
      throw "Native Host emitted $($stdout.Length) stdout bytes before rejecting a non-allowlisted origin."
    }
  } finally {
    if ($started) {
      try { $process.StandardInput.Close() } catch {}
      Stop-TestProcess -Process $process
    }
    $cancellation.Cancel()
    if ($null -ne $copyTask -and -not $copyTask.IsCompleted) {
      try { $copyTask.GetAwaiter().GetResult() } catch {}
    }
    $cancellation.Dispose()
    $stdout.Dispose()
    $process.Dispose()
  }
}

function Assert-ExactObjectProperties {
  param(
    [Parameter(Mandatory = $true)]$Value,
    [Parameter(Mandatory = $true)][string[]]$Expected,
    [Parameter(Mandatory = $true)][string]$Label
  )
  $actual = @($Value.PSObject.Properties.Name | Sort-Object)
  if (@(Compare-Object ($Expected | Sort-Object) $actual).Count -ne 0) {
    throw "$Label has unknown or missing properties."
  }
}

function Get-ReleaseConfig {
  param([Parameter(Mandatory = $true)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Native Host release configuration is missing: $Path" }
  $config = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
  Assert-ExactObjectProperties -Value $config -Expected @(
    'schemaVersion', 'chromeExtensionId', 'edgeExtensionId'
  ) -Label 'Native Host release configuration'
  if ($config.schemaVersion -ne 1) {
    throw 'Native Host release configuration has an unsupported schemaVersion.'
  }
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
  if (-not $ReleaseConfigPath -or -not (Test-Path -LiteralPath $ReleaseConfigPath -PathType Leaf)) {
    Add-Result -Name 'native_host_current_user_registration_and_messages' -Status 'BLOCKED' -Detail 'A Native Host executable was supplied without its release configuration.'
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
    $unauthorizedExtensionId = Get-NonAllowlistedNativeHostExtensionId -AllowedExtensionIds @(
      [string]$config.chromeExtensionId,
      [string]$config.edgeExtensionId
    )
    Assert-NativeHostRejectsUnauthorizedOrigin `
      -HostPath $NativeHostPath `
      -Origin "chrome-extension://$unauthorizedExtensionId/"
    foreach ($extensionId in @($config.chromeExtensionId, $config.edgeExtensionId)) {
      $requestId = [guid]::NewGuid().ToString()
      $origin = "chrome-extension://$extensionId/"
      $positive = Invoke-NativeHostFrame -HostPath $NativeHostPath -Origin $origin -Message @{ type = 'handshake'; protocolVersion = $script:NativeHostProtocolVersion; requestId = $requestId }
      Assert-ExactObjectProperties -Value $positive -Expected @(
        'type', 'protocolVersion', 'requestId', 'product'
      ) -Label 'Native Host handshake response'
      if ($positive.type -cne 'handshake_ack' -or
          $positive.requestId -cne $requestId -or
          $positive.protocolVersion -ne $script:NativeHostProtocolVersion -or
          $positive.product -cne 'VeriSilo') {
        throw 'Native Host did not return the expected handshake_ack response.'
      }
    }
    $negative = Invoke-NativeHostFrame -HostPath $NativeHostPath -Origin "chrome-extension://$($config.chromeExtensionId)/" -Message @{ type = 'unknown_message'; protocolVersion = $script:NativeHostProtocolVersion; requestId = ([guid]::NewGuid().ToString()) }
    Assert-ExactObjectProperties -Value $negative -Expected @(
      'type', 'protocolVersion', 'code', 'message'
    ) -Label 'Native Host malformed-message response'
    if ($negative.type -cne 'error' -or
        $negative.protocolVersion -ne $script:NativeHostProtocolVersion -or
        $negative.code -cne 'invalid_message' -or
        [string]::IsNullOrWhiteSpace([string]$negative.message) -or
        ([string]$negative.message).Length -gt 200) {
      throw 'Native Host did not reject an unknown message with invalid_message.'
    }

    $mismatchRequestId = [guid]::NewGuid().ToString()
    $protocolMismatch = Invoke-NativeHostFrame -HostPath $NativeHostPath -Origin "chrome-extension://$($config.chromeExtensionId)/" -Message @{
      type = 'handshake'
      protocolVersion = $script:NativeHostProtocolVersion + 1
      requestId = $mismatchRequestId
    }
    Assert-ExactObjectProperties -Value $protocolMismatch -Expected @(
      'type', 'protocolVersion', 'requestId', 'code', 'message'
    ) -Label 'Native Host protocol-mismatch response'
    if ($protocolMismatch.type -cne 'error' -or
        $protocolMismatch.protocolVersion -ne $script:NativeHostProtocolVersion -or
        $protocolMismatch.requestId -cne $mismatchRequestId -or
        $protocolMismatch.code -cne 'unsupported_protocol' -or
        [string]::IsNullOrWhiteSpace([string]$protocolMismatch.message) -or
        ([string]$protocolMismatch.message).Length -gt 200) {
      throw 'Native Host did not bind an unsupported_protocol response to the mismatched request.'
    }
    Add-Result -Name 'native_host_current_user_registration_and_messages' -Status 'PASS' -Detail 'Verified the real HKCU registration, zero-output/nonzero-exit rejection of a syntactically valid non-allowlisted origin, exact v2 response schemas for both allowlisted handshakes, rejection of an unknown framed message, and explicit rejection of a future protocol version.'
  } catch {
    Add-Result -Name 'native_host_current_user_registration_and_messages' -Status 'FAIL' -Detail $_.Exception.Message
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
  if (Test-StrictPathDescendant -Candidate 'C:\non-temporary-artifacts' -Ancestor ([IO.Path]::GetTempPath())) {
    throw 'Self-test treated a non-temporary artifact path as temporary.'
  }
  if ($FixturePort -ne $script:RequestedFixturePort -or $ArtifactDirectory -cne $script:RequestedArtifactDirectory) {
    throw 'Script-scope initialization overwrote a caller-supplied FixturePort or ArtifactDirectory parameter.'
  }
  if ((Get-WindowsFamilyFromBuild -Build 19045) -cne 'Windows 10' -or
      (Get-WindowsFamilyFromBuild -Build 22000) -cne 'Windows 11') {
    throw 'Build-number Windows family classification self-test failed.'
  }
  $emptyEvents = ConvertFrom-JsonArray -Json '[]'
  if ($null -eq $emptyEvents -or $emptyEvents.Count -ne 0) {
    throw 'Empty fixture event JSON did not remain a non-null zero-length array.'
  }
  $responseWithoutError = [pscustomobject]@{ id = 1; result = [pscustomobject]@{} }
  if ($null -ne (Get-CdpErrorMessage -Message $responseWithoutError)) {
    throw 'A CDP response without an error property was treated as an error under strict mode.'
  }
  $counter = [pscustomobject]@{ NextId = 7 }
  $nextIdOutput = @(Get-NextCdpId -Connection $counter)
  if ($nextIdOutput.Count -ne 1 -or $nextIdOutput[0] -ne 7 -or $counter.NextId -ne 8) {
    throw 'CDP request-ID increment leaked an extra pipeline value.'
  }
  $formalOrigin = 'chrome-extension://abcdefghijklmnopabcdefghijklmnop/'
  if (-not (Test-VeriSiloExtensionTarget -Target ([pscustomobject]@{ url = "$formalOrigin`worker.js" }) -OriginPrefix $formalOrigin) -or
      (Test-VeriSiloExtensionTarget -Target ([pscustomobject]@{ url = 'chrome-extension://ponmlkjihgfedcbaponmlkjihgfedcba/worker.js' }) -OriginPrefix $formalOrigin)) {
    throw 'Formal extension-ID target filtering self-test failed.'
  }
  $strictResponseRejected = $false
  try {
    Assert-ExactObjectProperties -Value ([pscustomobject]@{ type = 'handshake_ack'; extra = $true }) -Expected @('type') -Label 'self-test response'
  } catch {
    $strictResponseRejected = $true
  }
  if (-not $strictResponseRejected) {
    throw 'Strict Native Host response-property validation accepted an unknown field.'
  }
  $sampleAllowedIds = @(
    'abcdeabcdeabcdeabcdeabcdeabcdeab',
    'ponmlkjihgfedcbaponmlkjihgfedcba'
  )
  $sampleUnauthorizedId = Get-NonAllowlistedNativeHostExtensionId -AllowedExtensionIds $sampleAllowedIds
  if ($sampleUnauthorizedId -notmatch '^[a-p]{32}$' -or
      $sampleAllowedIds -ccontains $sampleUnauthorizedId) {
    throw 'Non-allowlisted Native Host origin generator self-test failed.'
  }
  $shortAcceptanceLeaf = New-ShortAcceptanceLeaf
  if ($shortAcceptanceLeaf -cnotmatch '^vda-[0-9a-f]{16}$') {
    throw 'Short desktop acceptance root generation self-test failed.'
  }
  $metadataRootA = Join-Path $script:ResolvedArtifactDirectory 'metadata-a'
  $metadataRootB = Join-Path $script:ResolvedArtifactDirectory 'metadata-b'
  [void](New-Item -ItemType Directory -Path $metadataRootA)
  [void](New-Item -ItemType Directory -Path $metadataRootB)
  $metadataFileA = Join-Path $metadataRootA 'marker.bin'
  $metadataFileB = Join-Path $metadataRootB 'marker.bin'
  Set-Content -LiteralPath $metadataFileA -Value 'alpha' -Encoding ascii -NoNewline
  Set-Content -LiteralPath $metadataFileB -Value 'omega' -Encoding ascii -NoNewline
  $metadataTimestamp = [DateTime]::SpecifyKind([DateTime]'2026-01-02T03:04:05', [DateTimeKind]::Utc)
  foreach ($path in @($metadataFileA, $metadataFileB, $metadataRootA, $metadataRootB)) {
    $item = Get-Item -LiteralPath $path -Force
    $item.CreationTimeUtc = $metadataTimestamp
    $item.LastWriteTimeUtc = $metadataTimestamp
  }
  $metadataA = Get-TreeMetadataFingerprint -Path $metadataRootA
  $metadataB = Get-TreeMetadataFingerprint -Path $metadataRootB
  if ($metadataA.Hash -cne $metadataB.Hash -or $metadataA.Entries -ne $metadataB.Entries) {
    throw 'Metadata-only tree fingerprint depended on equal-length file contents.'
  }
  (Get-Item -LiteralPath $metadataFileB -Force).LastWriteTimeUtc = $metadataTimestamp.AddSeconds(1)
  $metadataChanged = Get-TreeMetadataFingerprint -Path $metadataRootB
  if ($metadataA.Hash -ceq $metadataChanged.Hash) {
    throw 'Metadata-only tree fingerprint ignored a file mtime change.'
  }
  Add-Result -Name 'harness_input_safety' -Status 'PASS' -Detail 'The runner rejects default and non-temporary user-data-dir inputs before starting a browser.'
  Add-Result -Name 'harness_regression_guards' -Status 'PASS' -Detail 'Caller parameters, temporary-root boundaries, short acceptance-root generation, Windows build classification, empty events, strict CDP/Native Host response parsing, non-allowlisted origin generation, and formal extension-ID filtering passed pure self-tests.'
}

function Complete-Run {
  $summaryPath = Join-Path $script:ResolvedArtifactDirectory 'summary.json'
  $script:Results | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $summaryPath -Encoding utf8
  $failed = @($script:Results | Where-Object { $_.status -eq 'FAIL' }).Count
  $blocked = @($script:Results | Where-Object { $_.status -eq 'BLOCKED' }).Count
  $skipped = @($script:Results | Where-Object { $_.status -eq 'SKIP' }).Count
  Write-Host "Summary written to $summaryPath"
  if ($failed -gt 0 -or ($RequireAll -and ($blocked -gt 0 -or $skipped -gt 0))) { return 1 }
  return 0
}

if ($SelfTest) {
  $selfTestExitCode = 1
  $selfTestRoot = Join-Path ([IO.Path]::GetTempPath()) ("verisilo-e2e-self-test-" + [guid]::NewGuid())
  $script:ResolvedArtifactDirectory = New-TemporaryArtifactDirectory -Path $selfTestRoot
  try {
    Invoke-SelfTest
    $selfTestExitCode = Complete-Run
  } finally {
    Remove-TemporaryArtifactDirectory -Path $script:ResolvedArtifactDirectory
  }
  exit $selfTestExitCode
}

if (-not (Test-WindowsHost)) {
  Write-Error 'This runner only executes real browser/NSIS cases on Windows. Run node tests/windows/self-test.mjs for a cross-platform static safety check.'
  exit 2
}

$script:ArtifactDirectoryWasProvided = [bool]$ArtifactDirectory
$requestedArtifactDirectory = if ($ArtifactDirectory) {
  Get-NormalizedPath $ArtifactDirectory
} else {
  Join-Path ([IO.Path]::GetTempPath()) ("verisilo-windows-e2e-" + [guid]::NewGuid())
}
$script:ResolvedArtifactDirectory = New-TemporaryArtifactDirectory -Path $requestedArtifactDirectory
$runExitCode = 1

try {
  Test-OperatingSystemEvidence
  Start-LoopbackFixture
  $selected = if ($Browser -eq 'Both') { @('Chrome', 'Edge') } else { @($Browser) }
  foreach ($name in $selected) {
    $configuration = Get-BrowserConfiguration -Name $name
    if (-not $configuration.Executable) {
      Add-Result -Name "${name}_browser_cases" -Status 'SKIP' -Detail "No $name executable was found. Supply -$($name)Path to run real browser cases."
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
  $runExitCode = Complete-Run
  if ($KeepArtifacts -or $script:ArtifactDirectoryWasProvided) {
    Write-Host "Artifacts retained for inspection: $script:ResolvedArtifactDirectory"
  } else {
    Remove-TemporaryArtifactDirectory -Path $script:ResolvedArtifactDirectory
  }
}
exit $runExitCode
