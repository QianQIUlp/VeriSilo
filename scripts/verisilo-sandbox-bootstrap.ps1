[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$')]
  [string]$SiloId
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# This bootstrap intentionally exposes no generic command parameter. V0.8's
# first Sandbox slice only creates a local profile and stops before claiming
# proxy, exit, DNS, health, or log evidence.
$profileRoot = Join-Path $env:LOCALAPPDATA "VeriSilo\Sandbox\$SiloId\chromium-profile"
[void](New-Item -ItemType Directory -Path $profileRoot -Force)

$status = [ordered]@{
  schemaVersion = 1
  environmentId = $SiloId
  source = 'sandbox-bootstrap'
  profile = 'configured'
  guestAgent = 'unavailable'
  proxy = 'unavailable'
  exit = 'unavailable'
  dns = 'unavailable'
}
$status | ConvertTo-Json -Compress | Write-Output
