# scripts

## Gate tiers (`gate.ps1`)

`gate.ps1` runs the test gate in three tiers so a change never costs hours.
Every step is one cargo command (`-j 6`, `--test-threads=6`), logged to
`gate-logs/<tier>-<timestamp>/`; the run ends with a per-step pass/fail
summary (failing binaries named) and a non-zero exit on any failure.
`-DryRun` prints the plan without running anything.

| Tier | When | What |
|---|---|---|
| `quick` | every patch (agent / gate-runner), ~minutes | what the diff touches, in debug |
| `batch` | before committing a batch; push/PR CI | every debug suite minus `SLOW_BINARIES`, ui (CWD `crates/ui`), clippy `-D warnings` |
| `release` | at a cut; nightly CI (`nightly.yml`) | everything: all debug suites incl. slow and kernel, ui, clippy, then `--release -- --ignored` for motion and app |

    powershell -File scripts/gate.ps1 -Tier quick [-Base main] [-Heavy]
    powershell -File scripts/gate.ps1 -Tier batch
    powershell -File scripts/gate.ps1 -Tier release

**quick** diffs the working tree against `merge-base(<Base>, HEAD)`
(committed + uncommitted + untracked) and prints what it chose before
running it:

- a changed suite file (`crates/<c>/tests/<b>.rs`) runs only the `#[test]`
  fns whose lines the diff hunks touch (`--exact`); a change outside any test
  fn (header, helpers, `use`) or a new suite runs the whole binary. Changed
  `#[ignore]`d fns are heavy-lane: skipped (the binary is still compiled)
  unless `-Heavy`, which runs them `--release -- --ignored`.
- a changed source file (`crates/<c>/src/<mod>/<sub>.rs`) runs the crate's
  lib tests plus the suites whose `Gate map` header names the module -
  narrowed to the suites naming `mod::sub` / `mod (.., sub, ..)` when any
  do. Suites of other crates match when they name `<c>::<mod>`.
- `tests/common`, `tests/golden`, snapshots or a crate `Cargo.toml` run the
  crate's lib and all of its suites.
- a touch to a shared crate (`geometry`, `cl`) or the workspace manifest /
  lock falls back to `batch`.

`SLOW_BINARIES` (top of the script) lists the debug binaries over ~40 s;
batch skips them, release runs them. A test that takes more than ~60 s in
debug belongs in the heavy lane (`#[ignore = "heavy lane: ..."]`), not here.

### Single tests

    powershell -File scripts/gate.ps1 -Test <filter> [-Crate app] [-Binary operations] [-Exact]

Runs exactly the matching `#[test]` fn(s) and nothing else. Without
`-Binary` the binary is found by searching `fn <filter>` under
`crates/*/tests` and `crates/*/src` (`-Crate` narrows it); a substring
filter by default, the exact fn name with `-Exact`. A match that is
`#[ignore]`d runs in release with `--ignored`; lib unit tests run via
`--lib`; ui tests run from `crates/ui`. Example:

    powershell -File scripts/gate.ps1 -Test an_open_level_holds_the_planner_laws -Exact

## Release

`release.ps1` cuts a release: preflight, gate, version bump, commits and tag
(`-NoTag` on a `release/vX.Y.Z` branch: no tag - main is tagged after the
release PR merges, AGENTS.md "A cut goes PR review"),
release build, signing (if configured), the portable zip (same layout as
`.github/workflows/release.yml`) and, on request, the installer.

    powershell -ExecutionPolicy Bypass -File scripts/release.ps1 -Version 0.8.0 -Title "..."

Switches: `-SkipGate`, `-SkipBuild`, `-Installer`, `-DryRun` (prints every
command, changes nothing).
Update RELEASE_NOTES.md / TESTING.md first; the script commits them.
It never pushes; it prints the push command for you to run.

The Linux packages (`package-linux.sh`, `package-appimage.sh`) can also be
built in a container, with no Linux box or runner: `docker/build.sh`
(`docker/README.md` lists which targets build in Docker and why the rest
stay on their runners).

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
