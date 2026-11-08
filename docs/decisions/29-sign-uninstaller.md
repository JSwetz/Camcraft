## #29 - Sign the Inno-embedded uninstaller (2026-11-08)

- **Chosen:** ISCC signs both the setup exe and the embedded uninstaller
  through Inno Setup's named sign tool. `installer.ps1` asks
  `sign.ps1 -Status` whether a certificate is configured; if so it passes
  `/DSignToolName=camcraft` (the `.iss` turns on `SignTool=` and
  `SignedUninstaller=yes` only under that define) and
  `/Scamcraft=powershell.exe -NoProfile -ExecutionPolicy Bypass -File
  $q<sign.ps1>$q -Path $f`. `sign.ps1` stays the only reader of
  `CAMCRAFT_SIGN_*` and the only place holding the signtool command and
  the timestamp URL; secrets reach the ISCC child through the inherited
  environment, never its command line.
- **Without a certificate:** no `/D`, no `/S`; the compile is the same
  command as before plus a named log line ("installer signing: off").
- **Dropped:** the separate `sign.ps1 -Path <setup.exe>` pass after
  ISCC in `release.ps1` and `release.yml`. ISCC now signs the setup exe
  itself; a second pass would re-sign (a second timestamp request) for
  nothing. `installer.ps1` checks the setup exe carries a signature when
  signing is on, so an ISCC that skipped the tool fails the build.
- **Also fixed in passing:** `release.yml`'s installer step had no
  `CAMCRAFT_SIGN_CERT_PASSWORD` in its env (only the earlier sign step
  had it), so with the secret set its `sign.ps1` call would have failed
  as half-configured. The step now carries it.
- **Considered and dropped:** a sign tool defined in the `.iss`
  (`SignTool=signtool ...`): it would duplicate the signtool command and
  the certificate selection in a second file. A `$p`-parameterised tool
  passing thumbprint/PFX args from installer.ps1: same duplication, and
  it would put the PFX password on ISCC's command line.
