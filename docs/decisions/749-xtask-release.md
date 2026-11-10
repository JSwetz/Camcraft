## #749 - `cargo xtask release` replaces release.ps1's body

**Decided.** The release cut is `cargo xtask release --version X.Y.Z --no-tag
[--title T] [--skip-gate] [--skip-build] [--installer] [--dry-run]`
(`xtask/src/release.rs`, `release_bump.rs`, `release_pack.rs`), with the same
ten phases and refusals as the old `scripts/release.ps1`, which is now a
wrapper forwarding its switches. Deliberate differences from the old script:

- **Gate in process.** Phase 2 runs the current `gate release` plan
  (`plan::release`) inside the running xtask, not the old script's own,
  older command list, and not a nested `cargo xtask gate release`: that
  would `cargo run -p xtask` again, i.e. ask cargo to relink the xtask exe
  that is running, which Windows locks.
- **Build the product only.** Phase 5 builds `-p camcraft-ui -p camcraft-app
  -p camcraft-post` (what release.yml builds), not the whole workspace, so
  the xtask package is never compiled during a cut either.
- **Cargo.lock bumped in Rust.** Phase 3 moves every `[[package]]` without a
  `source` line (the workspace's path packages) to the new version, then
  still runs `cargo update -w -q` and the old "version lines only" diff
  guard. Unit-testable on fixture text; cargo stays the judge.
- **NOTICE in the zip.** The staged tree mirrors release.yml, which ships
  NOTICE; the old local script left it out. `assets/*` dirs are copied
  recursively.
- **Other OSes.** Linux, macOS and FreeBSD run the matching
  `scripts/package-*.sh` in phase 8 (macOS builds both architectures in
  phase 5, as release.yml does); signing is skipped by name off Windows
  (sign.ps1 is Authenticode; package-macos.sh codesigns its bundle), and
  `--installer` is refused by name (Inno Setup is Windows-only). AppImage,
  snap and flatpak stay CI-only (they need tooling the cut does not
  install).
- **Tagging stays refused.** The old script never tagged any more (no
  `-NoTag` fails as "main is branch-protected"); the port keeps exactly
  that and has no tag code path. The commit trailer is kept verbatim.
- **AGENTS.md's hand-test line** said to run `release.ps1 -NoTag` "on main",
  which the script refuses; it now says a scratch branch off main.

**Considered and dropped.** A `zip` crate for the archive (a new dependency;
Compress-Archive is what both the old script and release.yml use, and only
the Windows path makes a zip). Spawning `cargo xtask gate release` as a
child (the exe lock above). Porting sign.ps1 / installer.ps1 (signtool and
ISCC are Windows tools; the scripts stay the one place they are driven).

**Cost to change.** Low: each phase is one method of `Cut`; the dry-run
phase list is pinned by `dry_run_walks_the_ten_phases_and_changes_nothing`.
