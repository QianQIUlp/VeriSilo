[CmdletBinding()]
param(
  [string]$PfxPath = (Join-Path $env:USERPROFILE '.verisilo-signing\engine-package-rc1.pfx'),
  [string]$MetadataPath = (Join-Path $env:USERPROFILE '.verisilo-signing\engine-package-rc1.json')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path.TrimEnd('\') + '\'

function Assert-OutsideRepository {
  param([Parameter(Mandatory = $true)] [string]$Path)
  $fullPath = [IO.Path]::GetFullPath($Path)
  if ($fullPath.StartsWith($repositoryRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Signer output must be outside the repository.'
  }
  return $fullPath
}

function Set-CurrentUserOnlyAcl {
  param(
    [Parameter(Mandatory = $true)] [string]$Path,
    [switch]$Directory
  )
  $currentUser = [Security.Principal.WindowsIdentity]::GetCurrent().Name
  $acl = Get-Acl -LiteralPath $Path
  $acl.SetAccessRuleProtection($true, $false)
  $existingRules = @($acl.Access)
  foreach ($rule in $existingRules) {
    [void]$acl.RemoveAccessRule($rule)
  }
  $inheritance = if ($Directory) {
    [Security.AccessControl.InheritanceFlags]::ContainerInherit -bor
      [Security.AccessControl.InheritanceFlags]::ObjectInherit
  } else {
    [Security.AccessControl.InheritanceFlags]::None
  }
  $accessRule = [Security.AccessControl.FileSystemAccessRule]::new(
    $currentUser,
    [Security.AccessControl.FileSystemRights]::FullControl,
    $inheritance,
    [Security.AccessControl.PropagationFlags]::None,
    [Security.AccessControl.AccessControlType]::Allow
  )
  $acl.SetAccessRule($accessRule)
  Set-Acl -LiteralPath $Path -AclObject $acl
}

$pfx = Assert-OutsideRepository -Path $PfxPath
$metadata = Assert-OutsideRepository -Path $MetadataPath
$metadataParent = Split-Path -Parent $metadata
$pfxParent = Split-Path -Parent $pfx
New-Item -ItemType Directory -Force -Path $metadataParent, $pfxParent | Out-Null
foreach ($parent in @($metadataParent, $pfxParent) | Sort-Object -Unique) {
  Set-CurrentUserOnlyAcl -Path $parent -Directory
}

$certificate = New-SelfSignedCertificate `
  -Subject 'CN=VeriSilo Camoufox Engine Release Signer' `
  -CertStoreLocation 'Cert:\CurrentUser\My' `
  -Type CodeSigningCert `
  -KeyAlgorithm RSA `
  -KeyLength 3072 `
  -HashAlgorithm SHA256 `
  -KeyExportPolicy Exportable `
  -NotAfter (Get-Date).ToUniversalTime().AddYears(10)
try {
  if (-not $certificate.HasPrivateKey -or $certificate.PublicKey.Key.KeySize -ne 3072) {
    throw 'Generated signer does not have the required RSA-3072 private key.'
  }
  $ekuExtension = $certificate.Extensions | Where-Object {
    $_.Oid.Value -eq '2.5.29.37'
  }
  $codeSigningEku = if ($null -ne $ekuExtension) {
    ([Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]$ekuExtension).
      EnhancedKeyUsages | Where-Object { $_.Value -eq '1.3.6.1.5.5.7.3.3' }
  }
  if ($null -eq $codeSigningEku) {
    throw 'Generated signer is missing the Code Signing EKU.'
  }

  # The password is entered only as a SecureString and is never an argument,
  # environment value, log field, or metadata field.
  $password = Read-Host -Prompt 'PFX password (will not be echoed)' -AsSecureString
  Export-PfxCertificate `
    -Cert $certificate `
    -FilePath $pfx `
    -Password $password `
    -CryptoAlgorithmOption AES256_SHA256 | Out-Null
  Set-CurrentUserOnlyAcl -Path $pfx

  $der = $certificate.Export([Security.Cryptography.X509Certificates.X509ContentType]::Cert)
  $digest = [Security.Cryptography.SHA256]::HashData($der)
  $sha256 = -join ($digest | ForEach-Object { $_.ToString('x2') })
  $public = [ordered]@{
    schema = 'urn:verisilo:cms-signer-public:1'
    schemaVersion = 1
    certificateSha256 = $sha256
    subject = $certificate.Subject
    issuer = $certificate.Issuer
    notBeforeUtc = $certificate.NotBefore.ToUniversalTime().ToString('o')
    notAfterUtc = $certificate.NotAfter.ToUniversalTime().ToString('o')
    keyAlgorithm = 'RSA'
    keyLength = 3072
    extendedKeyUsage = @('1.3.6.1.5.5.7.3.3')
    store = 'CurrentUser\My'
  }
  [IO.File]::WriteAllText(
    $metadata,
    (($public | ConvertTo-Json -Depth 4) + [Environment]::NewLine),
    [Text.UTF8Encoding]::new($false)
  )
  Set-CurrentUserOnlyAcl -Path $metadata
  Write-Output "Camoufox engine signer public certificate SHA-256: $sha256"
  Write-Output "Public metadata written to $metadata"
}
finally {
  if ($null -ne $certificate) {
    $store = [Security.Cryptography.X509Certificates.X509Store]::new(
      [Security.Cryptography.X509Certificates.StoreName]::My,
      [Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser
    )
    try {
      $store.Open([Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
      $store.Remove($certificate)
    }
    finally {
      $store.Dispose()
      $certificate.Dispose()
    }
  }
}
