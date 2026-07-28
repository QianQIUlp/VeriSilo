[CmdletBinding()]
param(
  [string]$ManifestRoot = (Join-Path $env:LOCALAPPDATA 'VeriSilo\NativeMessaging')
)

$ErrorActionPreference = 'Stop'
$hostName = 'io.verisilo.host'
$browserSettings = @(
  [pscustomobject]@{
    Name = 'chrome'
    RegistryPath = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
  },
  [pscustomobject]@{
    Name = 'edge'
    RegistryPath = "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
  }
)

foreach ($browser in $browserSettings) {
  $manifestPath = Join-Path $ManifestRoot "native-host-$($browser.Name).json"
  if (Test-Path -LiteralPath $browser.RegistryPath) {
    $registeredPath = (Get-Item -LiteralPath $browser.RegistryPath).GetValue('')
    if ([string]::Equals($registeredPath, $manifestPath, [StringComparison]::OrdinalIgnoreCase)) {
      Remove-Item -LiteralPath $browser.RegistryPath -Force
    } else {
      Write-Warning "Left $($browser.RegistryPath) unchanged because another manifest is registered."
    }
  }
  Remove-Item -LiteralPath $manifestPath -Force -ErrorAction SilentlyContinue
}

Remove-Item -LiteralPath (Join-Path $ManifestRoot 'install-record.json') -Force -ErrorAction SilentlyContinue
if ((Test-Path -LiteralPath $ManifestRoot -PathType Container) -and
    @((Get-ChildItem -LiteralPath $ManifestRoot -Force)).Count -eq 0) {
  Remove-Item -LiteralPath $ManifestRoot -Force
}

Write-Host "Unregistered $hostName from Chrome and Edge for the current user."
Write-Host 'Vault, Silo metadata, browser Profile directories, and reports were not deleted.'
