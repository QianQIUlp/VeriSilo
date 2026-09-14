# Bootstrap script invoked by Windows Sandbox LogonCommand
$ErrorActionPreference = 'Continue'
$driverDir = $PSScriptRoot
$evidenceDir = 'C:\Evidence'
$packageRoot = 'C:\Package'
$logFile = Join-Path $evidenceDir 'driver.log'

# Wait until all mapped volumes are mounted
for ($i = 0; $i -lt 90; $i++) {
  if ((Test-Path "$packageRoot\host\camoufox-host.exe") -and (Test-Path $evidenceDir)) {
    break
  }
  Start-Sleep -Seconds 1
}

try {
  Set-Content -Path (Join-Path $evidenceDir 'bootstrap-started.txt') -Value ([DateTime]::UtcNow.ToString('o')) -Encoding utf8
  [Console]::WriteLine("Bootstrap started at $([DateTime]::UtcNow.ToString('o'))")
} catch {}

try {
  & "$driverDir\sandbox-acceptance.ps1" -PackageRoot $packageRoot -OutputDir $evidenceDir -DriverDir $driverDir
} catch {
  $errMsg = "[Bootstrap Exception] $_"
  [Console]::WriteLine($errMsg)
  try { [System.IO.File]::AppendAllText($logFile, "$errMsg`r`n", [System.Text.Encoding]::UTF8) } catch {}
}
