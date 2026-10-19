# HSMWorks (clean-room) 0.1.0

First public release: Milestone 1 — a CAM system that opens a 3D model, defines
a job, machines 2.5D parts, verifies itself, and posts a real program.

This is an independent, clean-room reimplementation of user-visible HSMWorks
behavior, specced from published documentation and general machining
knowledge. "HSMWorks" is a trademark of Autodesk; this project is unaffiliated
(see AGENTS.md). Third-party notices: `assets/ui/LICENSES.md`.

## What works

- **Import & document**: STL/mesh and STEP models, `.hsmproj` projects with
  full undo/redo (everything is a command), mm/inch units system-wide.
- **Job setup**: stock definitions, work offsets, origin anchoring on a
  picked point of the solid or the stock (3×3 compass + viewport crosses).
- **2D machining**: face, contour, pocket, slot, trace — feature-driven
  selection (faces, edges, welded patches), tangential leads, G41/G42
  compensation output, arc moves.
- **Drilling**: hole recognition from the model, spot/drill/peck/deep/tap/bore
  cycles, canned-cycle intent carried alongside the move chain.
- **Tools**: tool library with a graphical editor (end/ball/bull nose,
  drills, taps, boring heads) whose profile drives collision checks.
- **Verification**: gouge and shank/holder collision checks, feed/climb
  sanity, machine-limit checks — warnings block the post (Never Gouge, made
  structural).
- **Simulation**: timed move simulation with play/pause/scrub, rapids paced
  from the machine file, live feedrate and tool marker, stock comparison.
- **Post**: the CPS-architecture engine (`.cps` scripts + `.mch` machine
  files + our `.cnc` intermediate), the Fanuc post as its first script, and
  an in-app G-code viewer where scrolling the listing highlights the
  corresponding move in the toolpath.

## How to run

- `hsmworks-ui.exe` — the application (all UI assets are embedded).
- `post.exe` — the command-line post processor:
  `post [--machine <file.mch>] <config.cps> <intermediate.cnc> <output.nc>`
  (`--log <file>` writes warnings there; counted warnings are reported, not
  fatal — the in-app validation gate is what refuses a machine).
- Editable post scripts and machine definitions ship in `assets/posts/` and
  `assets/machines/`.

## Known limits in 0.1.0

- 2.5D only: Z-axis tools, planar features; no multi-axis or 3D toolpaths
  (Phase 4), no model→operation associativity yet (M3.1).
- One post shipped (Fanuc); the in-app machine-file workflow arrives with
  M3.3. Thread milling is not implemented. IGES import is deferred.
- The download is an **unsigned portable zip** — no installer and no
  Authenticode signature yet (no signing certificate owned; see
  RECOMMENDED.md). Windows SmartScreen may warn on first run.

## Build

Rust (edition 2024, `rust-version` ≥ 1.90): `cargo build --release -p
hsmworks-ui -p hsmworks-app`. CI runs the golden regression suite, all tests,
clippy with warnings denied, and the headless app run
(.github/workflows/ci.yml).
