# CamCraft 0.6.0 (clean-room implementation)

Sixth public release: Phase 9 — the workpane remake, every operation
opens its own page. The dropdown that hid the ribbon is retired: press a
kind in the ribbon and it creates that operation and opens *that
operation's* pane; select its row in the tree and it edits in place. The
six tab groups fold into one stacked pane — geometry, heights, passes,
leads, feeds and machine limits at a glance, sections where the kind has
them, gone where it doesn't. And the pane carries a law tested across
all thirty registry kinds: every section header renders exactly when
its kind has fields for it, and no field renders twice — the law that
caught a real header/field collision on its first run and renamed it
out of existence.

This is an independent, clean-room reimplementation of user-visible
HSMWorks behavior, specced from published documentation and general
machining knowledge. "HSMWorks" is a trademark of Autodesk; this project
is unaffiliated (see AGENTS.md). Third-party notices:
`assets/ui/LICENSES.md`.

## What's new since 0.5.0

- **Click-through creation (M9.1 A)**: the create dialog and the grouped
  Type dropdown are retired — the ribbon creates or opens by kind, a
  tree selection opens the existing operation in place, and the retype
  verb rides the context menu as a command, so every open-and-edit is
  undoable like anything else.
- **One stacked pane (M9.1 B)**: the six section tabs fold into a single
  scrolling pane with collapsing headers, per the Blender-style verdict
  — `op_tab` retires (pure view state with nothing left to view), and
  the field-placement drift guard is restated at exact strength: one
  frame, every field located by label geometry, each rendered once.
- **The pane law walks the registry (M9.1 C)**: one test per law across
  all thirty kinds, headers checked against the same predicate the pane
  consumes; it caught the swarf ladder's `Passes` field echoing its own
  group header — now `Pass count` — where the old tab UI could hide the
  ambiguity.
- **Projects**: persistence format unchanged (v42) — the remake lives
  in the UI; toolpath and G-code goldens byte-identical across it.

## What works

- **Import & document**: STL/mesh (streaming, shared) and STEP models,
  `.camproj` projects with full undo/redo (everything is a command),
  mm/inch units system-wide.
- **Job setup**: multi-setup jobs with datum transfer, stock
  definitions, work offsets, origin anchoring, machine definition
  mounting and a machine editor (`.mch`), user fixtures, and the
  machine-kind switch in the Setup group.
- **2D machining**: face, contour, pocket, slot, trace — feature-driven
  selection (faces, edges, welded patches), tangential leads, G41/G42
  compensation output, arc moves; helix/ramp/plunge entries that hold
  from the first level and say so when substituted.
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
  spot/drill/peck/deep/tap/bore cycles with a fit door that refuses a
  tool that cannot enter by name, canned-cycle intent.
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
- **Interface**: token theming with user JSON themes and a contrast
  gate, a dark and a light golden, the family-door ribbon, pinned
  kinds, the grouped command combo, and one stacked operation pane per
  kind — click-through creation, no type dropdown.

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
  and `assets/machines/`; the scripter's types in `assets/scripts/`;
  theme files ride the user themes door (Setup page).

## Known limits in 0.6.0

- The 2D/3D strategy catalogs from M2.4/M2.5 are still open (2D adaptive
  and offset roughing, engrave, chamfer mills, helical thread milling,
  steep-and-shallow, pencil, ramp, morphed spiral, geodesic); flowline
  landed with M5.1; surface-finish parameters from M2.2 remain open;
  IGES import is deferred.
- The SDF stock engine (Phase 7) is the next build-out: the machined
  volume is still a height field, not yet a field you can see.
- The WASM rung is a compile-only guarantee today — a web UI is its own
  future phase.
- The download is an **unsigned portable zip** — no installer and no
  Authenticode signature yet (no signing certificate owned; see
  RECOMMENDED.md). Windows SmartScreen may warn on first run.

## Build

Rust (edition 2024, `rust-version` ≥ 1.90): `cargo build --release -p
camcraft-ui -p camcraft-app -p camcraft-post`. CI runs the golden and
numerical regression suites, all tests, the release-profile verdicts,
clippy with warnings denied, and the headless app run
(.github/workflows/ci.yml).

## 0.5.0

Fifth public release: Phases 6 and 8 — a token-themed interface with a
home for every operation, and every generator re-asked how the tool
first meets metal. See the git history (tag `v0.5.0`) for its full
notes.

## 0.4.0

Fourth public release: Phases 4 and 5 — honest crates, every platform,
five-axis milling, turning as a real machine kind, and shop-scale
performance and automation. See the git history (tag `v0.4.0`) for its
full notes.

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
