# ROADMAP.md

> _Last updated: 2026-10-12_

## North Star

**A CAM system from 3D model to machine-proven G-code without leaving the
app.** Everything below serves that. Each milestone must leave the app
genuinely better at cutting a real part — not just architecturally more
impressive.

### Guiding principles

- **Milestones ship value.** Each one delivers something a machinist would
  actually use. No "framework phases" whose payoff is future features.
- **The UI is the product.** A toolpath you can't set up, inspect, and trust
  is a toolpath you won't cut with. Every milestone includes its UI.
- **The output is trusted, or it doesn't post.** Verification (collision,
  gouge, stock simulation) gates NC output in every phase, starting with
  Phase 1's first post. A blocked program is better than a crashed spindle.
- **Each phase builds, no rewrites.** Architecture decisions in Phase 1 are
  chosen so Phases 2-4 extend them, not replace them.
- **Provenance is a deliverable.** This is a clean-room implementation: every
  milestone must pass a provenance review (see AGENTS.md "Clean-Room
  Protocol") before it can be marked ✅. Behavior was specced from public
  documentation and published technical literature only.

## How to read this roadmap

**Phase** → **Milestone** → concrete, checkable features. The feature list is
the contract; the Goal line is what the feature list adds up to.

## Status legend

| Icon | Meaning |
|------|---------|
| ✅ | Done and verified |
| ⚠️ | Partial (note what's missing) |
| 📋 | Not started |
| 🔄 | In progress |
| ⏸ | Deferred (note why) |

---

## Phase 1 — MVP: a CAM that opens a model and posts a real program

Everything a machinist needs to cut a 2.5D part on a vertical mill today:
setup, tools, 2D operations, drilling, simulation, and one trustworthy post.

**Current status snapshot**: M1.1 (mesh side) and the M1.2 shell, viewport and
job setup (stock + WCS) are in; toolpath display and units remain for the phase.

### M1.1 - Core Foundation & File I/O
**Goal: a workspace that opens models and saves projects, and never crashes on bad input.** — ⚠️ partial

- ✅ **Mesh import**: STL (binary + ASCII, sniffed) / OBJ, validated at command
      creation; repair (weld, holes) still open, for toolpath-supporting surfaces
- ✅ **Robustness**: degenerate and corrupt models produce clean errors, never
      crashes (Prime Directive) - corrupt-input corpus tests enforce this
- ✅ **Project save/load**: versioned single-file project (`.hsmproj` JSON, at
      v2: rigid transforms, stock and WCS; older files migrate on load;
      operations and tools attach as those entities land)
- ✅ **Command foundation**: command pattern with undo/redo driving all
      UI-visible state changes from day one
- 📋 **Units**: millimeters and inches, correct everywhere including posts
- ⏸ **IGES import**: still refused with a clean diagnostic - no IGES
      translator exists in pure Rust. STEP import shipped early, in M1.2
      (below), via the truck tessellator

### M1.2 - UI Shell & Job Setup
**Goal: real application chrome; define and display a machining setup.** — ⚠️ partial

- ⚠️ **Menu & toolbar**: menus + shortcuts shipped (File/Edit/View, Ctrl+O/S/Z/Y,
      Del, F); toolbar icons not yet (blocked on the `assets/ui/` icon set)
- ⚠️ **Setup tree pane**: models pane with viewport-synced selection, plus a
      Setup tab editing stock and work offsets; setups → operations nodes arrive with M1.3
- ✅ **Viewport**: orbit / pan / zoom, selection highlight, frame-all, dockable
      offscreen 3D target; translucent stock and WCS triads render, stock is
      pickable where nothing is behind it; operations pickables arrive with M1.3.
      The triad never doubles the dimmed world-origin axes (they are drawn only
      when no offset claims machine zero), and gizmo tips carry live X/Y/Z labels
      projected through the camera
- ✅ **STEP import**: `.step`/`.stp` tessellate to meshes via
      truck (Apache-2.0, pure Rust); feature-edge wireframe in the viewport.
      B-rep *editing* and booleans remain kernel-decision territory (M1.3)
- ✅ **Stock definition**: box, cylinder, or from-model-solid stock with a user
      transform (XY center, Z bottom, yaw) and Fit-to-Models; Fit honors the
      selected shape (a cylinder fit takes the margin-padded footprint diagonal,
      so the model's corners clear) instead of always producing a box; shapes
      are stored as parameters and re-meshed deterministically, so files stay
      small and undo is exact. Specced from published stock-shape conventions
      and general machining knowledge only
- ✅ **WCS**: numeric origin and full orientation per slot (roll/pitch/yaw plus
      90° turns about the world axes - which apply on press, so the viewport
      triad moves with the button instead of waiting for a re-apply - so an
      offset can fit any machine's axis convention; machines axis their
      coordinates differently, so the WCS
      orientation is its own setup choice - specced from published CAM setup
      documentation, behavior only) with a G54-G59 readout and viewport triads,
      plus box-point placing: an explicit stock-or-model reference (with a
      visible note when the chosen one is missing and the other stands in),
      a 3x3 XY compass grid at bottom/middle/top Z, and a viewport pick mode
      that draws the nine points of the current level as clickable markers -
      buttons and markers share one anchor helper, so what you click, what
      highlights and what places cannot drift apart. True face/vertex
      snapping on model faces is deferred (needs M1.3 selection geometry).
      Offset codes are the published RS-274 / ISO 6983 ones
- 📋 **Toolpath display**: colored toolpath preview with directional arrows

### M1.3 - 2D Machining
**Goal: a machinist can face, pocket, and contour a simple plate.**

- ⏸ **IGES import + B-rep operations**: STEP import already landed (M1.2,
      truck). Face/island recognition and boolean stock ops need the kernel
      decision - truck (already linked for import) vs an OpenCascade binding;
      benchmark truck's booleans before committing
- **Face operation**: clean stock tops
- **2D Contour**: islands, lead in/out (linear, arc, tangential), tabs, cutter compensation (G41/G42)
- **2D Pocket**: with islands, spiral and zigzag entry, stock-to-leave
- **Slot and Trace**: straight slots with corner radii; trace of 2D curves
- **Tool library**: end mills, bull-nose, ball nose; shank/holder/stub definition; a homegrown speeds-and-feeds database by material group (never a copy of the Machining Data Handbook)
- **Operation parameters**: heights (clearance/retract/feed/top/bottom), depth passes and stepover, tolerance, climb vs conventional
- **Verification**: pre-post self-intersection and gouge warning on every generated path

### M1.4 - Drilling
**Goal: a complete hole list is machined correctly.**

- **Drill cycles**: spot, drill, peck (chip-breaking), deep-hole with dwell
- **Rigid tapping** and thread milling (simple, small pitch first)
- **Boring and back-boring**: basic cycles
- **Hole recognition**: extract holes from the model into an operation automatically
- **Canned-cycle posting**: G73/G81/G83/G84 output, not retracted point-to-point moves

### M1.5 - Simulation & Post (Release 0.1.0)
**Goal: verified output - first public release.**

- **Move simulation**: rapid vs feed differentiation, feedrate readout, play/pause/scrub
- **Validation warnings**: rapid-move violations and holder-vs-stock contact, blocking the post
- **Fanuc post**: first class post - program header, tool changes, work offsets, canned cycles, coolant M-codes
- **G-code viewer**: inspect the posted program in-app, with toolpath highlighting on scroll
- **CI**: build matrix, regression test suite on golden models, all tests green
- **Release build**: signed installer + portable zip, release notes

---

## Phase 2 — 3D Machining

From prismatic parts to molds and dies. Roughing that holds constant tool
load, finishing that meets a surface tolerance, and simulation you trust
before touching metal.

### M2.1 - 3D Roughing
**Goal: high-efficiency roughing that keeps the tool loaded and cool.**

- **Adaptive clearing**: constant-engagement paths derived from published HEM literature (tool centerline offset construction, trochoidal-style slots), specced from public sources only
- **3D Pocket**: multi-level with islands from any closed region, ramp and helical entries
- **Rest material**: toolpath computed against the stock left by the previous tool, as a difference of voxelized tool sweeps
- **Roughing simulation**: verify material removal, never crash
- **Feed-rate optimization**: engagement-based feed modulation within tool limits

### M2.2 - 3D Finishing
**Goal: mold-quality surfaces at a stated tolerance.**

- **Parallel**: XY and multi-direction, steep/shallow auto-split
- **Waterline**: Z-level finishing with tolerance-driven stepover
- **Scallop**: constant cusp height
- **Contour / Ramp / Spiral / Radial / Project**: finishing family around the model
- **Surface finish parameters**: tolerance, stepover, minimum step, ramp angle
- **Tolerance verification**: deviation between toolpath envelope and model measured, not assumed

### M2.3 - Verification (Release 0.2.0)
**Goal: the toolpath is trusted before the machine sees it.**

- **Material simulation**: the machined block maintained as an adaptive voxel / dexel stock grid, subtracted per cutting move, with color-mapped deviation against the final model
- **Gouge and collision checking**: tool, holder, and collar vs stock and fixtures, with locations pinpointed in the viewer
- **Stock comparison**: color map of over/under-cut against the final geometry
- **Machine limits**: axis travel, feed, and spindle validation from a machine definition
- **Post gating**: the verification result is the gate that releases the NC program (Never Gouge, made structural)

---

## Phase 3 — Associative CAM & Production Workflow

The workflow that justified HSMWorks' name: the model changes, and the toolpaths
update. Re-MACHining, multi-setup jobs, posts for the machines people actually
own.

### M3.1 - Associativity & Regeneration
**Goal: a model revision no longer means rebuilding the job.**

- **Geometry association**: operations reference model features and curves, not stale copies
- **Model update detection**: diff a new model revision and mark affected operations
- **Re-MACHining**: automatic rest-rough against areas machined by previous operations, minimizing redundant air-cutting
- **Parameter propagation**: changed stock, tool, or WCS flows correctly through dependents

### M3.2 - Setups & Documentation
**Goal: multi-setup production jobs with paperwork.**

- **Multiple setups**: flipping strategies, per-setup stock / fixture / WCS, setup-to-setup datum transfer
- **Fixture modeling**: vises, clamps, and tables that collision checking knows about
- **Setup sheets**: printable setup documentation with tools, offsets, and stock (like the images in the [SolveCraft setup sheet reference](https://github.com/bherbruck/solvecraft/blob/main/Documentation/reference/setup-sheet/README.md) but generated from our CAM data)
- **Tool list**: per-setup tool inventory with change counts and run time

### M3.3 - Post System & Machine Library (Release 0.3.0)
**Goal: real G-code for real controls, written by the user, not by us guessing.**

- **Post template engine**: an embedded scripting runtime so users own their posts (a clean-room design: no adaptation of the original .post JavaScript files)
- **Built-in posts**: Fanuc, Haas, Siemens 840D, Heidenhain, Mazak - each with conformance tests against a golden G-code suite
- **Machine definitions**: a machine group in the Setup pane - select or edit a
      machine, its name shown in the setup, with axis travel, tool changer,
      spindle and coolant - driving simulation limits and post output. The
      user-facing workflow (pick a machine, edit it, see it on the setup) is
      specced from published CAM user documentation only; no vendor strings,
      templates, or post files are copied or adapted
- **RTCP output**: G43.4 / A-angles correctly emitted for trunnion and tilting-head machines
- **Post debugging**: block-by-block trace from toolpath to line of G-code

---

## Phase 4 — Power: Multi-Axis, Turning, Scale

The Ultimate-edition feature set on top of a mature product: continuous
multi-axis, mill-turn, and the performance and automation to run a shop
programming department.

### M4.1 - 5-Axis Milling
**Goal: simultaneous-axis and tilted machining done safely.**

- **Swarf**: bladed and ruled-surface finishing
- **5-axis contour and parallel** with tool axis control: lead/lag, tilt step, max tilt
- **Collision-aware orientation**: auto-tilt to avoid shank contact, with shortest-travel smoothing
- **Drive curves**: machine to a curve on a surface with orientation rules
- **Machine simulation**: full kinematic simulation of the post's A/B/C motion model, not just the toolpath

### M4.2 - Turning & Mill-Turn
**Goal: lathe parts and C-axis parts from the same system.**

- **2D turning**: face, OD/ID rough and finish with inserts from the tool library
- **Grooving, part-off, single-point threading**
- **Bar feeder patterns**: catch/transfer support for production turning
- **Mill-turn**: live tooling operations indexed and continuous (C-axis)

### M4.3 - Performance & Automation (Release 0.4.0)
**Goal: big models, headless jobs, scripting.**

- **Large models**: GPU-resident sparse voxel / SDF stock simulation, streaming mesh and B-rep handling, 10M+ triangle models
- **Generation performance**: threaded toolpath generation, incremental recompute after model changes
- **Scripting API**: Lua bindings for batch ops - generate, verify, post, report
- **CLI**: headless project processing and regression runs
- **Regression suite**: golden-model CI (import → generate → verify → post) with numerical tolerances

---

## Milestone Summary

| Milestone | Goal | Key Features | Status |
|-----------|------|--------------|--------|
| **M1.1** | Model in, project saved, never crash | Mesh import, save/load, undo/redo, robustness (IGES deferred; units open) | ⚠️ |
| **M1.2** | Define a job | Shell, dockable viewport, STEP import, stock + WCS setup in; toolpath display open | ⚠️ |
| **M1.3** | Cut a 2.5D plate | Face, contour, pocket, slot, tool library, parameters; B-rep ops gated on kernel | 📋 |
| **M1.4** | Machine the hole list | Drill/peck/tap/bore, hole recognition, canned cycles | 📋 |
| **M1.5** | Verified first output | Move simulation, validation, Fanuc post, 0.1.0 | 📋 |
| **M2.1** | Rough 3D at constant load | Adaptive clearing, rest material, roughing sim | 📋 |
| **M2.2** | Finish 3D to tolerance | Parallel, waterline, scallop, contour family | 📋 |
| **M2.3** | Trust before cutting | Material sim, gouge/collision, stock compare, post gate, 0.2.0 | 📋 |
| **M3.1** | Model changes, toolpaths update | Associativity, re-MACHining, parameter propagation | 📋 |
| **M3.2** | Production setups | Multi-setup, fixtures, setup sheets, tool lists | 📋 |
| **M3.3** | Every control covered | Post engine, 5 posts, machines, RTCP, 0.3.0 | 📋 |
| **M4.1** | Five axes | Swarf, tilt control, collision-aware orientation, machine sim | 📋 |
| **M4.2** | Lathes too | Turning cycles, bar feeder, mill-turn | 📋 |
| **M4.3** | Shop scale | 10M tri performance, Lua, CLI, regression CI, 0.4.0 | 📋 |

## Open questions & deferred

- **Geometry kernel**: [ricosjp/truck](https://github.com/ricosjp/truck) (Apache-2.0,
  Rust-native B-rep: truck-topology/geometry/meshalgo + truck-stepio STEP IO) is now
  LINKED and ships STEP→mesh import (M1.2) - pure Rust, no C++ toolchain risk,
  license-clean for the clean-room posture. Its boolean/offset quality for toolpath
  operations is still unbuilt here: bench truck vs an OpenCascade binding before M1.3
  commits to either for booleans; no proprietary kernel internals may be studied for
  implementation hints either way (clean-room cuts both ways)
- **Post scripting runtime**: which embedded language (QuickJS / Lua / Rust-native DSL) -
  decide before M3.3; must be sandboxed like other scripts
- **Simulation stock representation**: adaptive dexel vs. sparse voxel octree vs. signed distance field - decide before M1.5.
  The voxel grid models the *machined block for simulation only*: operation geometry and tolerance checks still evaluate against
  B-rep/mesh, so discretized stock is the verification layer, not the source of truth (an SDF is the closest cousin to a particle
  approach and keeps the exact remaining-material queries gouge checks need)
- **Cutting data**: no licensed handbook is affordable long-term - grow a homegrown
  materials/tools database under an open license, seeded from public vendor catalogs
- **Patent check (FTO)**: high-efficiency milling and some 5-axis orientation techniques
  were patented in the 2000s-2010s; run a freedom-to-operate review before M2.1 and M4.1
  to confirm the literature we're implementing from is unencumbered
- **Probing / workpiece verification**: deferred until post Phase 2 (which probing protocol
  sets first - RS238 or Renishaw-style macros?)
- **Feature recognition**: automatic hole/pocket/slot recognition beyond M1.4 basics -
  deferred; M3.1 associativity will need some of it anyway
- **Machine simulation kinematics fidelity**: full controller emulation is out of scope;
  we simulate geometry and limits, not the control loop
- **SolidWorks link**: HSMWorks was a SolidWorks add-in; this implementation is a
  standalone application - add-in/host integration deferred indefinitely (and any
  file-format interop done from published formats only)
- **Naming**: the project currently uses the HSMWorks name descriptively; pick our own
  product name before 0.1.0 to stay clear of trademark confusion (clean-room clears
  copyright, not brands)

## Updating this roadmap

Maintenance rules:

- Update whenever a feature changes status - a milestone moves to ✅ only when all its
  features are done, their tests are green, and the milestone's provenance review passed
- Record decisions in the relevant milestone, not just chat
- Prune completed rows to links + one-liners after each release
- New features must be specced from public documentation or published literature - a
  feature motivated by inspecting HSMWorks internals is not adopted (Clean-Room Protocol)
