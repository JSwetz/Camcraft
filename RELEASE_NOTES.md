# CamCraft 0.3.0 (clean-room implementation)

Third public release: Milestone 3 — the production job. A model revision
updates the toolpaths that depend on it instead of stranding them, a part can
be a multi-setup job with fixtures and per-setup tool lists, and the post
speaks for real controls: five built-in posts, machine definitions you own,
RTCP output, and a block-by-block trace to show the work.

This is an independent, clean-room reimplementation of user-visible HSMWorks
behavior, specced from published documentation and general machining
knowledge. "HSMWorks" is a trademark of Autodesk; this project is unaffiliated
(see AGENTS.md). Third-party notices: `assets/ui/LICENSES.md`.

## What's new since 0.2.0

- **Associativity (M3.1)**: models carry revisions and features carry stable
  identity, so `UpdateModel` re-attaches old toolpaths to new geometry. Ops
  are stamped with the tool, WCS and stock they were built from — change one
  and the op goes stale with a reason you can read, and Regenerate fixes the
  whole tree in one undo unit. Rest material is re-MACHINED from predecessor
  paths, not re-guessed: a re-rough converges in two rounds.
- **Production setups (M3.2)**:
  - *Multi-setup jobs*: per-setup stock and work offset, datum transfer from
    a previous setup's machined stock, one program in setup order.
  - *Fixtures*: mount your own vise/chuck STEP solids; they wear collision
    markers and the spindle/table bands come from the mounted `.mch`. A
    fixture hit is a live gate — it blocks the post, not just the log.
  - *Paperwork*: a per-setup tool list in change order, and a printable setup
    sheet per setup (no timestamps — the sheet is documentation, not a log).
- **Post system & machine library (M3.3)**:
  - *Users own their posts*: the mounted `.mch` now actually shapes the
    output (spindle limits, coolant M-codes, the machine's name), post
    selection rides the undo history, and a missing or broken user `.cps`
    refuses by name — it never silently falls back.
  - *Five built-in posts*: Fanuc, Haas, Siemens 840D, Heidenhain, Mazak —
    all original scripts, addressed by name, each covered by conformance
    goldens.
  - *Machine definitions*: a machine declares its kind (mill / lathe /
    mill-turn) and that gates which operation families a setup offers; the
    editor mirrors travel, rates, spindle, coolant and changer numbers
    honestly (draft law — an invalid draft never reaches the document).
  - *RTCP & the trace*: rotary-axis output as G43.4 (pivots delegated to the
    controller) or as machine geometry, angle words modal both directions —
    and a block-by-block post trace that shows which toolpath block wrote
    which G-code block.
- **Projects**: persistence v19→v23 (revisions and feature identity; setups,
  fixtures, tool lists and sheets; post selection); old files step up
  silently.

## What works

- **Import & document**: STL/mesh and STEP models, `.hsmproj` projects with
  full undo/redo (everything is a command), mm/inch units system-wide.
- **Job setup**: multi-setup jobs with datum transfer, stock definitions,
  work offsets, origin anchoring, machine definition mounting and a machine
  editor (`.mch`), user fixtures.
- **2D machining**: face, contour, pocket, slot, trace — feature-driven
  selection (faces, edges, welded patches), tangential leads, G41/G42
  compensation output, arc moves.
- **3D machining**: 3D pocket with rest-aware adaptive roughing; waterline,
  parallel, scallop, radial and spiral finishing to a stated tolerance;
  re-machining against a predecessor's recorded path.
- **Drilling**: hole recognition from the model, spot/drill/peck/deep/tap/bore
  cycles, canned-cycle intent carried alongside the move chain.
- **Tools**: tool library with a graphical editor (end/ball/bull nose,
  drills, taps, boring heads) whose profile drives collision checks;
  per-setup tool lists in change order.
- **Verification**: per-op gouge and shank/holder collision checks,
  fixture-aware, plus program-level material simulation against the final
  model and machine-limit checks — warnings block the post. Never crash,
  never gouge.
- **Simulation**: timed move simulation with play/pause/scrub, rapids paced
  from the machine file, live feedrate and tool marker, Deviate's judged
  stock surface.
- **Post**: the CPS-architecture engine (`.cps` scripts + `.mch` machine
  files + our `.cnc` intermediate), five built-in posts and user scripts,
  machine-gated output including RTCP words, an in-app G-code viewer where
  scrolling the listing highlights the corresponding move, and the post
  trace pane that links G-code blocks back to the toolpath.

## How to run

- `camcraft-ui.exe` — the application (all UI assets are embedded).
- `post.exe` — the command-line post processor:
  `post [--machine <file.mch>] [--trace <file>] <config.cps>
  <intermediate.cnc> <output.nc>`
  (`--log <file>` writes warnings there instead of stderr; counted warnings
  are reported, not fatal — the in-app validation gate is what refuses a
  machine).
- Editable post scripts and machine definitions ship in `assets/posts/` and
  `assets/machines/`.

## Known limits in 0.3.0

- Still 2.5D kinematics on the toolpath side: posts can emit A/B/C words and
  RTCP output, but the generators are 3-axis paths — swarf, tilt control and
  collision-aware orientation are M5.1, and the lathe/mill-turn kinds shape
  output and gate operations before turning cycles arrive (M5.2).
- The 2D and 3-axis operation catalogs are not complete (M2.4/M2.5: 2D
  adaptive roughing, engrave, chamfer, circular, helical thread milling;
  steep-and-shallow, pencil, ramp, flow and the rest); surface-finish
  parameters from M2.2 remain open; IGES import is deferred.
- The download is an **unsigned portable zip** — no installer and no
  Authenticode signature yet (no signing certificate owned; see
  RECOMMENDED.md). Windows SmartScreen may warn on first run.

## Build

Rust (edition 2024, `rust-version` ≥ 1.90): `cargo build --release -p
camcraft-ui -p camcraft-app`. CI runs the golden regression suite, all tests,
clippy with warnings denied, and the headless app run
(.github/workflows/ci.yml).

## 0.2.0

Second public release: Milestone 2 — 3D machining you can trust before
touching metal. See the git history (tag `v0.2.0`) for its full notes.

## 0.1.0

First public release: Milestone 1 — a CAM system that opens a 3D model,
defines a job, machines 2.5D parts, verifies itself, and posts a real
program. See the git history (tag `v0.1.0`) for its full notes.
