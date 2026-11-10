## #752 - one zip layout: `cargo xtask package`

**Decided.** The Windows portable tree and zip are staged in one place,
`cargo xtask package` (xtask/src/package.rs over release_pack.rs's layout).
`cargo xtask release`'s zip phase and release.yml's Windows job both call
it; the workflow's inline PowerShell staging block is gone. ci.yml and
nightly.yml call `cargo xtask gate batch|release` directly (still
`workflow_dispatch` only).

- **Version.** `--version` is optional: without it xtask reads
  `<bin-dir>/post.exe --version`, which is what the old workflow block did
  ("the binary we ship is the version's single truth"). The workflow passes
  no `--version`, so a tag build still names its zip after the binary it
  built. A dry run needs `--version` (it reads no binary).
- **Outputs.** Under Actions, xtask appends `version=`, `stage=`, `zip=` to
  `$GITHUB_OUTPUT` and `CAMCRAFT_VERSION=` to `$GITHUB_ENV`. The job output
  `version` (read by publish) keeps its name; the installer step now gets
  `-Stage ${{ steps.zip.outputs.stage }}` explicitly instead of relying on
  installer.ps1's default path - same directory, named once.
- **Zip tool.** Kept PowerShell's `Compress-Archive`: no new crate (a `zip`
  crate would pull a dependency tree into xtask). It prefers `pwsh` when on
  PATH and falls back to Windows PowerShell 5.1: the workflow step always ran
  under pwsh 7, whose Archive module writes '/' entry names; 5.1's older
  module is the one known to write '\'. On the runner pwsh is present, so
  the published zip is made by the same tool as before.
- **Non-Windows.** A real `package` run off Windows is refused by name (the
  zip is the win-x64 package; macOS/Linux/FreeBSD ship from
  `package-*.sh`). `--dry-run` lists the tree on any host.
- **Recursion.** release_pack copies asset dirs recursively; the old
  `Copy-Item assets/x/*` (no `-Recurse`) would have staged a sub-folder
  empty. No asset dir has a sub-folder today, so the trees are identical
  (diffed, PR body); the recursive copy is the intended behaviour if one
  ever appears (e.g. `assets/ui/fonts/`).
- **Process lists.** The fork-sync list and review-prs 2d gain
  `scripts/release.ps1` and `scripts/collect-decisions.ps1`/`.sh` (wrappers
  around xtask, so they are process files like gate.ps1/.sh);
  PROCESS_VERSION 8 -> 9.

**Dropped.** A `zip` crate dependency (heavier, and a second zip writer to
trust); keeping the workflow's inline block next to a "keep in step"
comment (the double code the maintainer asked to remove); building the zip
with `tar -a` (bsdtar on Windows/macOS only, GNU tar can't write zip).

**Cost to change later.** Swapping the zip writer is one function
(`package::zip_args` / the `Command` in `package::package`).
