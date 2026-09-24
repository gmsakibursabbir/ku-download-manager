<#
.SYNOPSIS
  Creates a self-signed code-signing certificate for KuDownloader builds.

.DESCRIPTION
  - Creates the certificate in your personal store (Cert:\CurrentUser\My).
  - Exports it to .secrets\ (git-ignored): a password-protected .pfx for CI,
    a .cer (public part), and windows-signing.json with the thumbprint that
    `pnpm build:signed` uses.
  - With -Trust, also adds the certificate to *your* Trusted Root and Trusted
    Publishers stores so signed builds are trusted on this computer. Windows
    asks you to confirm that.

  A self-signed certificate is trusted only where it is installed: other
  people's Windows still shows SmartScreen. For public releases use a
  certificate from a trusted provider (e.g. Azure Trusted Signing, or an OV/EV
  certificate); `pnpm build:signed` and CI work with those the same way.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\windows\new-signing-cert.ps1 -Trust
#>
param(
  [string]$Subject = "CN=KuDownloader (self-signed), O=KUDUY",
  [int]$Years = 3,
  [switch]$Trust
)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$secrets = Join-Path $root ".secrets"
New-Item -ItemType Directory -Force -Path $secrets | Out-Null

$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $Subject `
  -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 `
  -KeyExportPolicy Exportable -CertStoreLocation Cert:\CurrentUser\My `
  -NotAfter (Get-Date).AddYears($Years)

$password = Read-Host -AsSecureString "Password for the exported .pfx (needed for CI)"
$pfx = Join-Path $secrets "windows-signing.pfx"
$cer = Join-Path $secrets "windows-signing.cer"
Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $password | Out-Null
Export-Certificate -Cert $cert -FilePath $cer | Out-Null
@{ thumbprint = $cert.Thumbprint; subject = $cert.Subject; expires = $cert.NotAfter.ToString("yyyy-MM-dd") } |
  ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $secrets "windows-signing.json")

if ($Trust) {
  Import-Certificate -FilePath $cer -CertStoreLocation Cert:\CurrentUser\Root | Out-Null
  Import-Certificate -FilePath $cer -CertStoreLocation Cert:\CurrentUser\TrustedPublisher | Out-Null
  Write-Host "Trusted on this computer (current user)."
}

Write-Host ""
Write-Host "Certificate: $($cert.Subject)"
Write-Host "Thumbprint:  $($cert.Thumbprint)"
Write-Host "Expires:     $($cert.NotAfter.ToString('yyyy-MM-dd'))"
Write-Host "Saved to:    $secrets (git-ignored)"
Write-Host ""
Write-Host "Signed build:   cd app; pnpm build:signed"
Write-Host "Sign in CI too (optional):"
Write-Host "  `$b = [Convert]::ToBase64String([IO.File]::ReadAllBytes('$pfx')); gh secret set WINDOWS_CERTIFICATE --body `$b"
Write-Host "  gh secret set WINDOWS_CERTIFICATE_PASSWORD   (paste the .pfx password when asked)"
