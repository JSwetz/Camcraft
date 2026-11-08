# Authenticode signing (M13.3): signs the given exe/installer files with
# signtool when a certificate is configured through the environment, and
# does nothing - printing "unsigned: no certificate configured" - when it is
# not. Unsigned is today's state and stays a legitimate release.
#
# Configuration (never committed, never embedded):
#   CAMCRAFT_SIGN_THUMBPRINT      SHA-1 thumbprint of a cert in the user's
#                                 certificate store (local machines), or
#   CAMCRAFT_SIGN_CERT_PATH +     a PFX file and its password (CI materialises
#   CAMCRAFT_SIGN_CERT_PASSWORD   the PFX from a base64 secret, see release.yml)
#   CAMCRAFT_SIGN_TIMESTAMP_URL   optional RFC-3161 server override
#
# Usage: powershell -File scripts/sign.ps1 -Path a.exe, b.exe
#        powershell -File scripts/sign.ps1 -Status   (configured? see below)
#
# -Status answers "is a certificate configured?" without signing anything:
# exit 0 configured, exit 10 not configured (prints the unsigned line), exit
# 1 half-configured. installer.ps1 asks it before wiring Inno Setup's
# SignTool, so this file stays the one place that reads CAMCRAFT_SIGN_*.
# Inno Setup calls this script as its sign tool for the setup exe AND the
# uninstaller it embeds (installer.ps1, /Scamcraft=...), so the signtool
# command and the timestamp server below are the only ones in the repo.
# Exit 0 when unsigned-by-configuration or signed and verified; exit 1 when a
# certificate IS configured but signing cannot proceed (missing signtool,
# missing file, signtool failure) - a configured-but-broken setup is an error.
param(
    [string[]]$Path = @(),
    [switch]$Status,
    [string]$Description = "camcraft",
    [string]$TimestampUrl = ""
)
$ErrorActionPreference = "Stop"

function Fail([string]$msg) {
    Write-Host "SIGN FAILED: $msg"
    exit 1
}

# ---- 1. Is a certificate configured? ------------------------------------
$thumb = $env:CAMCRAFT_SIGN_THUMBPRINT
$pfx = $env:CAMCRAFT_SIGN_CERT_PATH
$pfxPass = $env:CAMCRAFT_SIGN_CERT_PASSWORD
$certArgs = @()
if ($thumb) {
    $certArgs = @("/sha1", $thumb)
    Write-Host "signing with certificate store thumbprint $thumb"
} elseif ($pfx -and $pfxPass) {
    if (-not (Test-Path $pfx)) { Fail "CAMCRAFT_SIGN_CERT_PATH '$pfx' does not exist" }
    $certArgs = @("/f", $pfx, "/p", $pfxPass)
    Write-Host "signing with PFX $pfx (password from CAMCRAFT_SIGN_CERT_PASSWORD, not shown)"
} elseif ($pfx -or $pfxPass) {
    Fail "CAMCRAFT_SIGN_CERT_PATH and CAMCRAFT_SIGN_CERT_PASSWORD must both be set"
} else {
    Write-Host "unsigned: no certificate configured"
    Write-Host "  (set CAMCRAFT_SIGN_THUMBPRINT, or CAMCRAFT_SIGN_CERT_PATH + CAMCRAFT_SIGN_CERT_PASSWORD; see scripts/README.md)"
    if ($Status) { exit 10 }
    exit 0
}
if ($Status) { exit 0 }
if ($Path.Count -eq 0) { Fail "-Path is required (the files to sign)" }

# ---- 2. Find signtool ---------------------------------------------------
# PATH first (the GitHub runner and a Developer Command Prompt have it),
# then the newest x64 Windows SDK under Program Files (x86)\Windows Kits.
$signtool = $null
$cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
if ($cmd) { $signtool = $cmd.Source }
if (-not $signtool) {
    $kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
    if (Test-Path $kits) {
        $found = Get-ChildItem $kits -Directory |
            Where-Object { $_.Name -match '^\d+(\.\d+)+$' } |
            Sort-Object { [version]$_.Name } -Descending |
            ForEach-Object { Join-Path $_.FullName "x64\signtool.exe" } |
            Where-Object { Test-Path $_ } | Select-Object -First 1
        if ($found) { $signtool = $found }
    }
}
if (-not $signtool) { Fail "signtool.exe not found (install the Windows SDK or add it to PATH)" }
Write-Host "signtool: $signtool"

# ---- 3. Sign, then verify -----------------------------------------------
if (-not $TimestampUrl) { $TimestampUrl = $env:CAMCRAFT_SIGN_TIMESTAMP_URL }
if (-not $TimestampUrl) { $TimestampUrl = "http://timestamp.digicert.com" }
$files = @()
foreach ($p in $Path) {
    if (-not (Test-Path $p)) { Fail "file to sign not found: $p" }
    $files += (Resolve-Path $p).Path
}
# SHA-256 file digest, RFC-3161 timestamp with a SHA-256 digest: the
# signature outlives the certificate's validity.
$signArgs = @("sign", "/fd", "sha256", "/td", "sha256", "/tr", $TimestampUrl, "/d", $Description) + $certArgs + $files
# Echo the command with the password masked; never print the real one.
$shown = ($signArgs | ForEach-Object { if ($_ -eq $pfxPass) { "****" } else { $_ } }) -join ' '
Write-Host "> signtool $shown"
& $signtool @signArgs
if ($LASTEXITCODE -ne 0) { Fail "signtool sign exited with $LASTEXITCODE" }
foreach ($f in $files) {
    & $signtool verify /pa /q $f
    if ($LASTEXITCODE -ne 0) { Fail "signature on $f does not verify" }
    Write-Host "signed and verified: $f"
}
exit 0
