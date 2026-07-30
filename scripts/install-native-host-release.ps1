[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$HostPath,

  [Parameter(Mandatory = $true)]
  [string]$ReleaseConfigPath
)

$ErrorActionPreference = 'Stop'

try {
  & (Join-Path $PSScriptRoot 'install-native-host.ps1') `
    -HostPath $HostPath `
    -ReleaseConfigPath $ReleaseConfigPath
  & (Join-Path $PSScriptRoot 'verify-native-host-install.ps1') `
    -HostPath $HostPath `
    -ReleaseConfigPath $ReleaseConfigPath
} catch {
  & (Join-Path $PSScriptRoot 'uninstall-native-host.ps1') -ErrorAction SilentlyContinue
  throw
}

Write-Host 'Native Messaging Host installation and current-user registration were verified.'
