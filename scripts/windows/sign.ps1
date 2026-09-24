<#
.SYNOPSIS
  Signs files with Authenticode (SHA-256 + RFC 3161 timestamp) using signtool
  from the Windows SDK, then verifies the signature.

.DESCRIPTION
  The certificate comes from, in order:
    -Thumbprint, $env:KU_SIGN_THUMBPRINT, or .secrets\windows-signing.json
      (a certificate in Cert:\CurrentUser\My, e.g. made by new-signing-cert.ps1)
    -Pfx / $env:KU_SIGN_PFX with $env:KU_SIGN_PASSWORD

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\windows\sign.ps1 target\release\bundle\nsis\*.exe
#>
param(
  [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)][string[]]$Files,
  [string]$Thumbprint = $env:KU_SIGN_THUMBPRINT,
  [string]$Pfx = $env:KU_SIGN_PFX,
  [string]$Timestamp = "http://timestamp.digicert.com"
)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")

function Find-SignTool {
  $onPath = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($onPath) { return $onPath.Source }
  $kits = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
  $arch = if ([Environment]::Is64BitOperatingSystem) { "x64" } else { "x86" }
  $found = Get-ChildItem $kits -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^\d+\.' } | Sort-Object { [version]$_.Name } -Descending |
    ForEach-Object { Join-Path $_.FullName "$arch\signtool.exe" } | Where-Object { Test-Path $_ } | Select-Object -First 1
  if (-not $found) { throw "signtool.exe not found. Install the Windows SDK (Signing Tools for Desktop Apps)." }
  return $found
}

if (-not $Thumbprint -and -not $Pfx) {
  $json = Join-Path $root ".secrets\windows-signing.json"
  if (Test-Path $json) { $Thumbprint = (Get-Content $json -Raw | ConvertFrom-Json).thumbprint }
}
if (-not $Thumbprint -and -not $Pfx) { throw "No certificate: run scripts\windows\new-signing-cert.ps1, or set KU_SIGN_THUMBPRINT / KU_SIGN_PFX." }

$signtool = Find-SignTool
$paths = $Files | ForEach-Object { Get-ChildItem $_ -File } | ForEach-Object { $_.FullName }
if (-not $paths) { throw "Nothing to sign: $Files" }

$signArgs = @("sign", "/fd", "sha256", "/tr", $Timestamp, "/td", "sha256", "/d", "KuDownloader")
if ($Thumbprint) { $signArgs += @("/sha1", $Thumbprint.Replace(" ", "")) }
else { $signArgs += @("/f", $Pfx); if ($env:KU_SIGN_PASSWORD) { $signArgs += @("/p", $env:KU_SIGN_PASSWORD) } }

foreach ($f in $paths) {
  & $signtool @signArgs $f
  if ($LASTEXITCODE -ne 0) { throw "Signing failed: $f" }
  # /pa: Authenticode policy. A self-signed certificate verifies only where it is trusted.
  & $signtool verify /pa /q $f
  if ($LASTEXITCODE -ne 0) { Write-Warning "Signed, but not trusted on this computer (normal for a self-signed certificate without -Trust): $f" }
  else { Write-Host "Signed and verified: $f" }
}
