# CamCraft 0.4.0 (clean-room implementation)

Fourth public release: Phases 4 and 5 — the system grows up and the
machine list grows out. The code split into honest crates and builds
green on Windows, Linux and macOS from one source; the product is
CamCraft everywhere; five-axis milling arrived (swarf, tilt, RTCP,
collision-aware orientation, full kinematic simulation); so did turning
— a lathe is a real machine kind now, with facing, profiling, grooving,
threading and its own revolved-verdict simulation; and at shop scale the
generators thread, models stream in by the tens of millions of facets,
batch jobs run from a script or a command line, and the regression CI
judges numbers, not just bytes.

This is an independent, clean-room reimplementation of user-visible
HSMWorks behavior, specced from published documentation and general
machining knowledge. "HSMWorks" is a trademark of Autodesk; this project
is unaffiliated (see AGENTS.md). Third-party notices:
`assets/ui/LICENSES.md`.

## What's new since 0.3.0

- **Structure (M4.1)**: geometry, CL, tools, post, sim and motion are
  their own crates — the layering ui → app → flecs now has real seams
  between the domains, and the operation files split to the 400-line law.
- **Every platform (M4.2)**: Windows, Linux and macOS build and test
  green from one source; cross-platform float quantization makes the
  goldens byte-identical on all three (CI's parity job proves it), and
  the post engine compiles for wasm32. The gate ladder got shaped (M4.4):
  fast targeted debug runs, slow measurement verdicts in a release lane.
- **CamCraft (M4.3)**: the product is renamed everywhere — packages,
  binaries, `.camproj` files, window title, the cps vendor line.
- **5-axis milling (M5.1)**: tool axis rides every move; swarf and
  multi-axis contour on drive curves; 3+2 tilted-plane indexing and
  collision-aware auto-tilt; flowline finishing; RTCP output to real
  A/B/C words; and full kinematic machine simulation — the spindle,
  trunnion and columns move on screen, and the shank check reads the
  machine, not a guess.
- **Turning & mill-turn (M5.2)**: a lathe-kind machine offers turning
  operations on its own X/Z axis model — facing, profile rough/finish,
  grooving, parting, chamfers, tapers, threading (wave-verdict
  simulation), form tools, boring, radial drilling with hole
  recognition on turned parts, and a bar feeder that loads and unloads
  through the post. Mill-turn machines offer both families; the turned
  stock is a revolved solid, and the blade verdict measures the groove
  the program actually cut.
- **Shop scale (M5.3)**: model updates re-machine only what moved
  (per-entity revision memos); batch generation runs on all cores and
  proves threaded-equals-serial bit-for-bit; import streams 30M-facet
  STLs through a 72-byte shared-mesh, and the stock grid is tiled-sparse
  — grids pay for what they cut; part transfer and in-process probe
  inspection ride the stream as real intents with a deviation verdict
  that drives the re-run; and automation got two doors — a `cam.*`
  scripting API (TypeScript bindings generated from the verb table) and
  `camcraft-cli` for headless project processing.
- **Projects**: persistence v23→v40 (the machine's axes and post-emitted
  intents, revisions everywhere, the shared mesh); old files step up
  silently.

## What works

- **Import & document**: STL/mesh (streaming, shared) and STEP models,
  `.camproj` projects with full undo/redo (everything is a command),
  mm/inch units system-wide.
- **Job setup**: multi-setup jobs with datum transfer, stock
  definitions, work offsets, origin anchoring, machine definition
  mounting and a machine editor (`.mch`), user fixtures.
- **2D machining**: face, contour, pocket, slot, trace — feature-driven
  selection (faces, edges, welded patches), tangential leads, G41/G42
  compensation output, arc moves.
- **3D machining**: 3D pocket with rest-aware adaptive roughing;
  waterline, parallel, scallop, radial and spiral finishing to a stated
  tolerance; re-machining against a predecessor's recorded path.
- **5-axis machining**: swarf and multi-axis contour on drive curves,
  tilted-plane (3+2) machining with collision-aware auto-tilt, flowline
  finishing, RTCP (G43.4) or geometric post output.
- **Turning**: lathe and mill-turn machines; facing, profile
  rough/finish, grooving, parting, threading, chamfer/taper, form and
  boring passes, radial drilling with hole recognition, bar feeder
  choreography through the post.
- **Drilling**: hole recognition from the model (milled and turned),
  spot/drill/peck/deep/tap/bore cycles, canned-cycle intent.
- **Tools**: tool library with a graphical editor (end/ball/bull nose,
  drills, taps, boring heads, turning blades) whose profile drives
  collision checks; per-setup tool lists in change order.
- **Verification**: per-op gouge and shank/holder collision checks,
  fixture-aware, kinematic on 5-axis machines; program-level material
  simulation against the final model (height-field or revolved verdict);
  in-process inspection asks measured to; machine-limit checks.
  Warnings block the post. Never crash, never gouge.
- **Simulation**: timed move simulation with play/pause/scrub, rapids
  paced from the machine file, live feedrate and tool marker, the
  machine's own kinematics for rotary heads, Deviate's judged surface.
- **Post**: the CPS-architecture engine (`.cps` scripts + `.mch` machine
  files + our `.cnc` intermediate), five built-in posts and user
  scripts, machine-gated output including RTCP words and part-handling
  intents, an in-app G-code viewer where scrolling the listing
  highlights the corresponding move, and the post trace pane that links
  G-code blocks back to the toolpath.
- **Automation**: `camcraft-cli` runs a project or a job script
  headless (exit code is the verdict); the `cam.*` verbs — load, save,
  import, generate, verify, post, report — are scriptable with
  TypeScript bindings that cannot drift from the runtime (a CI test
  refuses it); the golden-model regression suite judges measured
  numbers (micron tolerances, exact counts) in CI, all platforms alike.

## How to run

- `camcraft-ui.exe` — the application (all UI assets are embedded).
- `camcraft-cli.exe` — the headless batch lane:
  `camcraft-cli script <job.js>` runs a job script;
  `camcraft-cli process <job.camproj> [-o <out.nc>] [--cps <name|file>]`
  loads, generates and posts a project — exit 0 stood, 1 refused,
  2 wrong command line.
- `post.exe` — the command-line post processor:
  `post [--machine <file.mch>] [--trace <file>] <config.cps>
  <intermediate.cnc> <output.nc>`
  (`--log <file>` writes warnings there instead of stderr).
- Editable post scripts and machine definitions ship in `assets/posts/`
  and `assets/machines/`; the scripter's types in `assets/scripts/`.

## Known limits in 0.4.0

- The 2D/3D strategy catalogs from M2.4/M2.5 are still open (2D adaptive
  and offset roughing, engrave, chamfer mills, helical thread milling,
  steep-and-shallow, pencil, ramp, morphed spiral, geodesic); flowline
  landed with M5.1; surface-finish parameters from M2.2 remain open;
  IGES import is deferred.
- The WASM rung is a compile-only guarantee today — a web UI is its own
  future phase.
- Not every command has a visible home yet: the ribbon rework (M6.2)
  is the next phase's first task.
- The download is an **unsigned portable zip** — no installer and no
  Authenticode signature yet (no signing certificate owned; see
  RECOMMENDED.md). Windows SmartScreen may warn on first run.

## Build

Rust (edition 2024, `rust-version` ≥ 1.90): `cargo build --release -p
camcraft-ui -p camcraft-app -p camcraft-post`. CI runs the golden and
numerical regression suites, all tests, the release-profile verdicts,
clippy with warnings denied, and the headless app run
(.github/workflows/ci.yml).

## 0.3.0

Third public release: Milestone 3 — the production job (associativity,
multi-setup production workflow, the CPS post system and machine
library). See the git history (tag `v0.3.0`) for its full notes.

## 0.2.0

Second public release: Milestone 2 — 3D machining you can trust before
touching metal. See the git history (tag `v0.2.0`) for its full notes.

## 0.1.0

First public release: Milestone 1 — a CAM system that opens a 3D model,
defines a job, machines 2.5D parts, verifies itself, and posts a real
program. See the git history (tag `v0.1.0`) for its full notes.
