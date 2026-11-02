# CamCraft 0.5.0 (clean-room implementation)

Fifth public release: Phases 6 and 8 — the interface learns manners and
the generators review the order of their cuts. The UI is no longer one
hard-coded dark skin: every color, radius and spacing rides a named
token table, user themes load from JSON and are refused by name when
unreadable, and the ribbon gives every operation kind one honest home —
family doors, pinning, and a machine-kind switch that rides the undo
history. Then every motion generator was visited and asked the same
question again: how does the tool first meet metal, and in what order?
The answers changed real laws — the entry family now reaches the first
level, a cert proves the ramp before it is trusted, the bore's door
refuses what will not fit, and a five-axis dive leads with the cut it
lands into, not the plunge that brought it there.

This is an independent, clean-room reimplementation of user-visible
HSMWorks behavior, specced from published documentation and general
machining knowledge. "HSMWorks" is a trademark of Autodesk; this project
is unaffiliated (see AGENTS.md). Third-party notices:
`assets/ui/LICENSES.md`.

## What's new since 0.4.0

- **Themeable UI (M6.1)**: one named `Tokens` table drives every widget
  and painted surface; a `camcraft-theme/1` JSON format loads user
  themes from disk with caps and named refusals, a WCAG contrast module
  derives unreadable states instead of shipping them, and switching is
  a command — the light theme undoes like anything else.
- **A home for every operation (M6.2)**: the ribbon's family sections
  come from the kind registry — hiding, not darkening, per mounted
  machine; every family behind one labeled door with pins, and the
  Setup group switches Milling/Turning/Mill-turning by mounting that
  kind's reference machine, through the undo history.
- **Cutting order (Phase 8)**: a generator-by-generator review —
  roughing (M8.1), finishing and planar (M8.2), canned, posed and
  revolved (M8.3). Selected entries now reach metal as selected (true
  helix or ramp into the first level, no silent plunge fallback);
  contour, waterline and slot entries are certified against the air
  above their destination before the ramp is trusted; the adaptive
  ladder's trochoid and substitution law was held to account and
  diagnosed; drilling grew a bore-fit door that refuses an oversize
  tool or a stepped hole by name; the multi-axis stamp learned that a
  dive's lead belongs to the cut it lands into; turning's order law
  stands where the catalog rows already promised it. The whole registry
  table carries its order-of-operations verdict.
- **Projects**: persistence v40→v42 (theme reference, ribbon pins);
  old files step up silently.

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
    kinds, and the grouped command combo.

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

## Known limits in 0.5.0

- The 2D/3D strategy catalogs from M2.4/M2.5 are still open (2D adaptive
  and offset roughing, engrave, chamfer mills, helical thread milling,
  steep-and-shallow, pencil, ramp, morphed spiral, geodesic); flowline
  landed with M5.1; surface-finish parameters from M2.2 remain open;
  IGES import is deferred.
- Every kind has a ribbon home now, but every home still doors into one
  generic property page: the workpane remake (M9.1) — one pane per
  operation kind, click-through creation — is the next task.
- The SDF stock engine (Phase 7) is queued behind it: the machined
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
