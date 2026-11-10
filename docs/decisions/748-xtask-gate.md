## #748 - one cross-platform gate: `cargo xtask`

**Chosen:** the gate tiers (`quick`, `batch`, `release`, `test <name>`) and the decision collection move into a workspace member `xtask/` (not published), run as `cargo xtask gate ...` / `cargo xtask decisions --version X.Y.Z` through a `.cargo/config.toml` alias. `scripts/gate.ps1`, `scripts/gate.sh` and `scripts/collect-decisions.{ps1,sh}` stay as thin wrappers that forward their old flags, so muscle memory and fork process-sync keep working. The port reproduces `gate.ps1`, not `gate.sh`: `gate.sh`'s quick tier had drifted coarser (whole crates), and the PowerShell one is what AGENTS.md describes. `--dry-run` output for batch, release, quick (a 6-file sample diff) and single tests is line-for-line identical to the old `gate.ps1 -DryRun`, apart from the items below.

**Why:** one implementation can't drift, cargo is on every contributor's machine (no PowerShell 7 on macOS/Linux, no bash on Windows), and the plan selection gets unit tests.

**Deliberate differences from gate.ps1:**
- `-Test` on a fn in a `tests/` module under `src/` (e.g. `crates/ui/src/egui_app/tests/layout.rs`) was planned as `--test layout`, a suite that doesn't exist, so cargo failed. A file is now its own suite only when it sits directly in `crates/<c>/tests/`; everything else is a `--lib` test.
- `-Test` hits in several src files of one crate gave one identical `--lib` step per file. They now share one step.
- The quick note for skipped heavy fns names `--heavy` / `gate test <name>` instead of `-Heavy` / `-Test <name>`.
- A change under `xtask/` makes quick run xtask's own unit tests (the old script didn't know about xtask).
- The log dir timestamp is UTC (std has no local-time formatting, and pulling in a time crate for a folder name isn't worth it). gate.ps1 used local time.
- Where gate.ps1 grouped steps in hashtable order (non-deterministic), the steps are now sorted.

**Dependency:** `regex` (already in `Cargo.lock` through tracing-subscriber's env-filter, so no new crate enters the tree). The gate maps, test fn spans and diff hunks are matched with the same patterns as the .NET regexes in gate.ps1, so the port stays one-for-one. No serde_json: the gate drives `cargo test` / `cargo clippy` the way gate.ps1 did and never launches test executables itself, so it has no artifact list to parse. That also means it never globs `*.exe`.

**Release not ported here:** `scripts/release.ps1` (243 lines: 10 phases, version bump, zip, installer, signing) does not port cleanly into this PR. It stays as is, and a follow-up issue tracks the port, so this PR is `Refs #748`.

**Forks:** `xtask/` and `.cargo/config.toml` join the process files a fork syncs. A fork whose cut tag predates xtask also adds the `"xtask"` workspace member in its process-sync commit (the wrappers need it); it adds no product code.

**Considered and dropped:**
- **One PowerShell 7 script for all OSes:** every macOS/Linux contributor would have to install pwsh.
- **Keeping the twins in step by review:** that's what we had, and it had already drifted.
- **A standalone xtask workspace** (own `[workspace]`, `--manifest-path` alias), so forks would need no member line: it brings a second lock file and a second target dir, the alias breaks outside the root, and the maintainer asked for a workspace member.

**Cost to change later:** low. The wrappers isolate callers, and the gate's lists live in `xtask/src/plan.rs`.
