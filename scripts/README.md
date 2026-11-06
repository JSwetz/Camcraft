# scripts

`release.ps1` cuts a release: preflight, gate, version bump, commits and tag,
release build, signing (if configured), the portable zip (same layout as
`.github/workflows/release.yml`) and, on request, the installer.

    powershell -ExecutionPolicy Bypass -File scripts/release.ps1 -Version 0.8.0 -Title "..."

Switches: `-SkipGate`, `-SkipBuild`, `-Installer`, `-DryRun` (prints every
command, changes nothing).
Update RELEASE_NOTES.md / TESTING.md first; the script commits them.
It never pushes; it prints the push command for you to run.

## Installer (M13.3)

The portable zip stays the release and the primary test vehicle. The
installer is additive: `installer.ps1` compiles `installer/camcraft.iss`
(Inno Setup 6) against the SAME staged tree the zip was made from and
writes `dist/camcraft-<ver>-win-x64-setup.exe` beside the zip.

    powershell -File scripts/release.ps1 -Version 0.8.0 -Installer   # as a phase of the cut
    powershell -File scripts/installer.ps1 -Version 0.8.0            # on an existing dist/ stage

- Per-user by default: no administrator prompt, installs under
  `%LOCALAPPDATA%\Programs\camcraft`, Start Menu shortcuts (app, hand-test
  checklist, release notes, uninstaller), optional desktop shortcut. The
  first page offers an all-users install (Program Files, elevated).
- Upgrades install over the previous version in place (fixed AppId); the
  uninstaller removes what was installed and leaves user files alone.
- Locally, a missing `ISCC.exe` is a named skip ("installer skipped: ...")
  and the cut still succeeds with the zip. In CI (`release.yml`) Inno Setup
  is preinstalled on `windows-latest` and the step is required.
- Why Inno Setup: one script file, no extra toolchain on the runner, free
  for any use; WiX needs its own CLI and XML authoring, MSIX cannot install
  at all unsigned.

## Signing (M13.3)

`sign.ps1` signs exes and the installer with `signtool` (SHA-256 digest,
RFC-3161 timestamp so signatures outlive the certificate). It is called by
`release.ps1` after the build and before the zip, and again on the
installer; `release.yml` does the same.

With no certificate configured it prints `unsigned: no certificate
configured` and exits 0: the unsigned release is a legitimate release, as
every cut so far has been (SmartScreen will warn on first run). A
certificate that IS configured but cannot be used (missing signtool, bad
path, signtool failure) fails the cut - a half-configured signing setup is
an error, silence is not.

Configure through the environment only; nothing is committed:

| Variable | Meaning |
|---|---|
| `CAMCRAFT_SIGN_THUMBPRINT` | SHA-1 thumbprint of a certificate in your Windows certificate store (local cuts) |
| `CAMCRAFT_SIGN_CERT_PATH` + `CAMCRAFT_SIGN_CERT_PASSWORD` | a PFX file and its password |
| `CAMCRAFT_SIGN_TIMESTAMP_URL` | optional; overrides the default public RFC-3161 server |

In CI the optional repository secrets are `CAMCRAFT_SIGN_CERT_BASE64` (the
PFX, base64-encoded) and `CAMCRAFT_SIGN_CERT_PASSWORD`; the workflow writes
the PFX to the runner's temp dir, signs, and deletes it. Unset secrets mean
an unsigned release, not a failed one.

Obtaining a certificate: buy an OV or EV code-signing certificate from any
public CA (EV ones live on hardware tokens or a cloud signing service, which
is the local-thumbprint route). Known follow-on: the uninstaller embedded by
Inno Setup is not signed yet (Inno's `SignTool` directive is the hook).
