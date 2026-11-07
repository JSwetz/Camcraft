# CamCraft

A CAM system that turns 3D models into NC programs (G-code) for CNC milling
machines and lathes. Written in Rust on a Flecs ECS core with an egui
interface.

CamCraft is a clean-room implementation of HSMWorks' user-visible behavior:
its operation types, parameters and workflow, specced from published
documentation and general machining knowledge. "HSMWorks" is a trademark of
Autodesk; this project is independent and unaffiliated. No HSMWorks code,
text, assets or post processors were used.

> **Status: pre-1.0, for testing.** Every toolpath is verified against the
> part, the stock and the fixtures before it can be posted, and anything that
> cannot be proven safe is refused by name. Still, check every program in
> your machine's own simulation and dry-run it before cutting metal.

## What it does

- **Import** STL, OBJ and STEP models; repair meshes (weld vertices, fill
  holes).
- **Set up** stock, work offsets (G54–G59), fixtures and machines (mill,
  5-axis, lathe, mill-turn).
- **2D milling:** face, contour (with G41/G42 compensation), pocket,
  adaptive clearing, slot, trace, engrave, chamfer, circular, bore and
  thread milling.
- **3D milling:** adaptive roughing per level, waterline, parallel,
  scallop, radial, spiral, morph, flow, pencil and steep/shallow finishing.
- **Drilling:** spot, drill, peck, deep, tap and bore cycles, with hole
  recognition.
- **Multi-axis:** 3+2 indexing, rotary contour/parallel/offset, 5-axis
  contour and swarf.
- **Turning:** facing, profile roughing and finishing, grooving, parting,
  threading, boring and axial drilling.
- **Area control:** pick faces, edges or loops, then set containment,
  offset, extension and extra or open boundaries on every operation that
  has an area.
- **Verify and simulate:** a solid-law verifier (gouge, rest, rapid and
  fixture checks), stock simulation with playback, and a deviation view.
- **Post:** Fanuc, Haas, Mazak, Sinumerik, Heidenhain and Fanuc Lathe,
  written as scriptable `.cps` post processors. The setup sheet and tool
  list print alongside.

## Download

Windows builds are attached to each
[GitHub release](https://github.com/NeoTech/Camcraft/releases). Unzip and run
`camcraft-ui.exe`. `TESTING.md` in the zip is a short hand-test checklist
for what changed in that release.

## Build from source

Requires Rust 1.90 or newer (edition 2024).

```sh
cargo build --release -p camcraft-ui        # the desktop app
cargo run   --release -p camcraft-ui        # build and start it
cargo build --release -p camcraft-app --bin camcraft-cli   # headless CLI
cargo build --release -p camcraft-post --bin post          # standalone post processor
```

Windows is the primary platform. The core crates also build and test on
Linux and macOS, and the post engine compiles for `wasm32-unknown-unknown`.

## Workspace layout

| Crate | Role |
|---|---|
| `crates/geometry` | Meshes, slicing, 2D regions, mesh repair |
| `crates/cl` | The cutter-location (CL) move stream |
| `crates/tools` | Tool library and tool laws |
| `crates/motion` | Toolpath generators (2D, 3D, drilling, multi-axis, turning) |
| `crates/sim` | Stock simulation and the verifier |
| `crates/post` | The `.cps` post-processor engine |
| `crates/app` | Document model, commands (undo/redo), operations, posting |
| `crates/ui` | The egui desktop interface |
| `crates/kernel` | Shared kernel types |

`assets/` holds the post processors, machine definitions, UI icons, fonts
and themes. `testdata/` holds the test models.

## Testing

```sh
cargo test -p camcraft-app -p camcraft-motion -p camcraft-sim \
           -p camcraft-geometry -p camcraft-cl -p camcraft-tools -p camcraft-post
cargo test -p camcraft-ui                        # run from crates/ui
cargo test --release -p camcraft-app -p camcraft-motion -- --ignored   # slow verdicts
```

Golden toolpath and G-code outputs live under each crate's `tests/golden/`.
Gates run locally through `scripts/gate.ps1` (see Contributing). GitHub
Actions runs only on a pushed release tag; the CI and nightly workflows in
`.github/workflows/` are manual.

## Project documents

- `ROADMAP.md`: the frozen milestone archive, kept for history and design
  context. Work is tracked as GitHub issues.
- `RECOMMENDED.md`: the decision ledger, with what was chosen, why, and
  what was dropped.
- `AGENTS.md`: engineering rules (clean-room protocol, the never-crash /
  never-gouge directive, layering, testing).
- `RELEASE_NOTES.md`, `TESTING.md`: the current release.

## Reporting issues

Use the issue templates on GitHub: **Toolpath / path planning bug** for
anything about the machine's motion, **UI / UX bug** for the interface, and
the general bug and feature templates for the rest. If a path would gouge or
collide and the app did not warn you, please say so. Those reports matter
most.

## Contributing

CamCraft is built largely by AI coding agents working under
[`AGENTS.md`](AGENTS.md), with a human maintainer approving what lands.
Outside contributors follow the same flow. The full rules live in
`AGENTS.md` ("Issue Law" and "Commits and Releases"); this is the short
version.

### Tools

- **`git`** for branches and commits. Never force-push `main` or a branch
  with an open PR.
- **`gh`** (the GitHub CLI) for everything on GitHub: reading, filing,
  claiming and commenting on issues, and opening pull requests. Agents load
  the `gh-issues` skill for issue work.
- **`graphify`** keeps a knowledge graph of the repo in `graphify-out/`
  (generated, never committed or hand-edited). Agents query it before
  grepping (`graphify query "<question>"`) and refresh it after code changes
  (`graphify update`).
- **`scripts/gate.ps1`** runs the test gates: `-Tier quick` per change,
  `-Tier batch` before a batch of commits, `-Tier release` at a release cut,
  and `-Test <name>` for a single test.
- Project agents and skills for Claude Code live in `.claude/agents/` and
  `.claude/skills/` (gate runner, graph updater, PR reviewer).

### Issues

Work is the open GitHub issues, picked by priority: `P0` (wrong output,
gouge, crash, data loss), then `P1` (wrong behaviour a user hits), then `P2`
(owed polish), oldest first. Each issue carries one `P*` label and one or
more `area:*` labels. Issues opened by an agent also carry `source:agent`.

1. **Claim before you code.** Comment
   `claimed: <name> · <UTC time> · branch <name> · plan: <one line>`. The
   maintainer adds the `claimed` label and assignee for contributors who
   cannot set labels. A claim with no word from its claimant for 4 hours is
   stale and may be taken over.
2. **Report on the issue.** Findings, blockers and decisions go in issue
   comments. New work you discover becomes a new issue.
3. **Finishing** means a pushed branch and an open pull request, not a merge.
   The issue moves from `claimed` to `in-review`.

### Branches and pull requests

- Start from the latest `main`: `git fetch origin`, then branch
  `issue/<N>-<short-name>` (agents) or any branch on your fork.
- Commit subjects follow [Conventional Commits](https://github.com/qoomon/conventional-commits):
  `feat:`, `fix:`, `docs:`, `test:` and so on. Mark breaking changes with `!`
  or a `BREAKING CHANGE:` footer. An incompatible project-file or post-output
  change counts as breaking.
- The PR description says what changed, how it was tested, and names the
  tests that pin it. It carries a `Changelog:` line with the one-line entry
  (or `none` for docs and tests). It ends with `Fixes #N`, or `Refs #N` for
  partial work. Do not edit `CHANGELOG.md` in the PR; rows are written at
  release time.
- Gate it locally before asking for review:
  `powershell -File scripts/gate.ps1 -Tier quick`.
- Follow the engineering rules in `AGENTS.md`: the clean-room protocol, never
  crash and never gouge (no `unwrap`, `expect` or `panic!` outside tests;
  refuse bad input by name), every UI change as an undoable command, files
  under about 400 lines, and a provenance line on new modules.

### Review and merge: a manual gate

`main` is protected. Every change reaches it through a pull request, admins
included; force-pushes and deletions are blocked; review conversations must
be resolved; only merge commits are allowed.

- **Review of fork PRs.** The `PR-reviewer` agent (run by `/review-prs`)
  trial-merges the PR with current `main`, builds it, runs the suites it
  touches plus clippy, checks it against `AGENTS.md`, and posts its verdict
  as a comment. It never approves and never merges. A new push to the PR
  gets a fresh review. A PR that touches build scripts, CI workflows,
  scripts or dependencies is not built by the reviewer; it is flagged for
  the maintainer instead.
- **Review of the maintainers' own PRs.** Issue PRs are reviewed at the
  release cut by a subagent that did not write them; findings are fixed on
  the branch before it merges.
- **Approval is manual.** GitHub requires 0 approvals (the agents and the
  maintainer share one account, which cannot approve its own PR), so the
  gate is the maintainer's manual merge: the maintainer reads the review and
  approves and merges pull requests from forks by hand. Address the
  review's requested changes and push again to get there.
- **Workflows on forks** need the maintainer's approval to run, so a fork PR
  never spends the Actions budget on its own.
- **Docs-, process- and CI-only PRs** merge once reviewed, without
  waiting for a cut.
- **Releases.** Issue PRs merge at a release cut: reviewed, merged to `main`
  with merge commits, gated on a `release/vX.Y.Z` branch, merged through a
  release PR, then tagged. Versions follow semantic versioning from the
  commits (`fix` = patch, `feat` = minor, breaking = major; while the major
  is 0, breaking bumps the minor). Each issue gets a `shipped in vX.Y.Z`
  comment when its release is published. Only `vX.Y.0` tags (and manual
  runs) build the release artifact on GitHub.

## License

MIT. See [`LICENSE`](LICENSE). Third-party notices are in
`assets/ui/LICENSES.md`.
