[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$HostPath,

  [Parameter(Mandatory = $true)]
  [string]$ReleaseConfigPath,

  [string]$ManifestRoot = (Join-Path $env:LOCALAPPDATA 'VeriSilo\NativeMessaging')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# install-native-host.ps1 owns the full snapshot/write/verify/rollback
# transaction. Do not run a second unconditional uninstall here: if install
# restored a previous valid registration after a failure, such cleanup would
# destroy the state it just recovered.
& (Join-Path $PSScriptRoot 'install-native-host.ps1') `
  -HostPath $HostPath `
  -ReleaseConfigPath $ReleaseConfigPath `
  -ManifestRoot $ManifestRoot

Write-Host 'Native Messaging Host installation and current-user registration were verified.'
