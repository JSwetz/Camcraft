# HSMWorks (clean-room) 0.2.0

Second public release: Milestone 2 — 3D machining you can trust before
touching metal. Roughing holds a constant tool load, finishing meets a stated
tolerance, and the program is verified against the model and the machine
before the NC file is ever written.

This is an independent, clean-room reimplementation of user-visible HSMWorks
behavior, specced from published documentation and general machining
knowledge. "HSMWorks" is a trademark of Autodesk; this project is unaffiliated
(see AGENTS.md). Third-party notices: `assets/ui/LICENSES.md`.

## What's new since 0.1.0

- **3D roughing (M2.1)**: 3D Pocket with rest material and adaptive clearing -
  level-by-level staircases with computed islands, entries that never plunge
  at full width, engagement-modulated feed (median-anchor law), and a dexel
  material simulation that closes the half-step ladder honestly.
- **3D finishing (M2.2)**: waterline, parallel raster and scallop (with the
  diagonal-crossing pair and certified chains), radial and spiral polar
  placements, the solid-truth hidden-rest contour, and a tolerance-
  verification kernel whose verdicts are measured, not estimated.
- **Verification, made structural (M2.3)**:
  - *Deviate*: the whole program is simulated as one chained material removal
    over an axis-aligned stock; the judged surface replaces the stock in the
    viewer (steel clean, amber rest, red gouge, gray unjudged), and the worst
    rest/gouge cell is LOCATED - one click flies the camera to it.
  - *Machine limits*: mount a `.mch` machine definition and the checker
    validates spindle window, tool table, feed rates against the slowest
    stated driven axis, and axis travel in program coordinates (WCS-aware).
  - *Post gate*: the same two judgments now GATE the post. A program that
    gouges the model or outruns the mounted machine refuses to write - the
    file and viewer on disk stay exactly as they were. Never crash, never
    gouge.
- **Projects**: persistence v17→v19 (ramp spelling, scallop minimum-step, the
  machine key); old files step up silently.

## What works

- **Import & document**: STL/mesh and STEP models, `.hsmproj` projects with
  full undo/redo (everything is a command), mm/inch units system-wide.
- **Job setup**: stock definitions, work offsets, origin anchoring on a
  picked point of the solid or the stock (3×3 compass + viewport crosses),
  machine definition mounting (`.mch`).
- **2D machining**: face, contour, pocket, slot, trace — feature-driven
  selection (faces, edges, welded patches), tangential leads, G41/G42
  compensation output, arc moves.
- **3D machining**: 3D pocket with rest-aware adaptive roughing; waterline,
  parallel, scallop, radial and spiral finishing to a stated tolerance.
- **Drilling**: hole recognition from the model, spot/drill/peck/deep/tap/bore
  cycles, canned-cycle intent carried alongside the move chain.
- **Tools**: tool library with a graphical editor (end/ball/bull nose,
  drills, taps, boring heads) whose profile drives collision checks.
- **Verification**: per-op gouge and shank/holder collision checks, plus
  program-level material simulation against the final model and machine-limit
  checks - warnings block the post (Never Gouge, made structural).
- **Simulation**: timed move simulation with play/pause/scrub, rapids paced
  from the machine file, live feedrate and tool marker, Deviate's judged
  stock surface.
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

## Known limits in 0.2.0

- Still 2.5D kinematics: Z-axis tools, planar placements. Multi-axis tool
  paths and tool-axis tilting are Phase 4 (M4.1); lathe work is M4.2.
- No model→operation associativity yet (M3.1): a model revision means
  regenerating the ops it feeds. Fixture-aware collision checking waits for
  fixture modeling (M3.2); the program sim judges an axis-aligned box stock.
- Surface-finish parameters from M2.2 remain open; thread milling is not
  implemented; IGES import is deferred.
- One post shipped (Fanuc). The download is an **unsigned portable zip** —
  no installer and no Authenticode signature yet (no signing certificate
  owned; see RECOMMENDED.md). Windows SmartScreen may warn on first run.

## Build

Rust (edition 2024, `rust-version` ≥ 1.90): `cargo build --release -p
hsmworks-ui -p hsmworks-app`. CI runs the golden regression suite, all tests,
clippy with warnings denied, and the headless app run
(.github/workflows/ci.yml).

## 0.1.0

First public release: Milestone 1 — a CAM system that opens a 3D model,
defines a job, machines 2.5D parts, verifies itself, and posts a real
program. See the git history (tag `v0.1.0`) for its full notes.
