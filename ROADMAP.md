# ROADMAP.md

> _Last updated: 2026-11-04_

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
the contract; the Goal line is what the feature list adds up to. Phases past
all green gates live in `DONE.md` (see AGENTS.md's Done ledger law); this file
keeps the open work, the registry table, and a pointer.

## Status legend

| Icon | Meaning |
|------|---------|
| ✅ | Done and verified |
| ⚠️ | Partial (note what's missing) |
| 📋 | Not started |
| 🔄 | In progress |
| ⏸ | Deferred (note why) |

---

## Operation Catalog — the source of truth

The complete list of operations the product must offer, grouped the way the
ribbon shows them (M1.4.3). Status is current (kept in step with the registry); Home is the
milestone that owns each name. A new operation is added here first — if it
isn't on this list it isn't on the roadmap, and if it is, it has a ribbon
home - and, from M10.1, a declared set of tool kinds that can cut it
(the tool law).

### Milling — 2D

| Operation | Status | Home |
|-----------|--------|------|
| Face | ✅ | M1.3 |
| 2D Contour | ✅ | M1.3, M1.5 |
| Slot | ✅ | M1.3 |
| Trace | ✅ | M1.3 |
| 2D Pocket (zig / winding) | ✅ | M1.3 |
| 2D Adaptive roughing | ⚠️ | M2.4 -> M11.4 C (shipped as engagement-stepped rings; trochoidal/constant-load owed) |
| 2D Offset pocket roughing | ✅ | M2.4 |
| Engrave | ✅ | M2.4 |
| 2D Chamfer | ✅ | M2.4 |
| Circular (pocket / vault) | ✅ | M2.4 |
| Thread (helical milling; tapping is M1.6) | ✅ | M2.4 |
| Bore (standalone op; the cycle is M1.6) | ✅ | M2.4 |

### Milling — 3D

| Operation | Status | Home |
|-----------|--------|------|
| 3D Adaptive roughing | ⚠️ | M2.1 -> M11.4 C (trochoidal slot paths and fallbacks owed) |
| 3D Offset pocket roughing | ✅ | M2.5 |
| 3D Contour | ✅ | M2.2 |
| Horizontal (waterline) | ✅ | M2.2 |
| Parallel | ✅ | M2.2 |
| Scallop | ✅ | M2.2 |
| Spiral | ✅ | M2.2 |
| Radial | ✅ | M2.2 |
| Project | ✅ | M2.2 |
| Steep and shallow | ✅ | M2.5 |
| Flat (flat-area finishing) | ✅ | M2.5 |
| Pencil | ✅ | M2.5 |
| Ramp | ✅ | M2.5 |
| Morphed spiral | ✅ | M2.5 |
| Geodesic | ✅ | M2.5 |
| Blend | ✅ | M2.5 |
| Morph | ✅ | M2.5 |
| Corner | ✅ | M2.5 |
| Flow (flowline; ruled-surface flow waits for M5.1 - done since: M5.1 D swarf off the ruling fit) | ✅ | M2.5 |
| Deburr | ✅ | M2.5 |
| 3+2 roughing (tilted plane, indexed) | ✅ | M5.1 |

### Milling — Drilling

| Operation | Status | Home |
|-----------|--------|------|
| Drill (spot / drill / peck / deep / tap / bore cycles) | ✅ | M1.6 |
| Hole recognition | ✅ | M1.6 |

### Milling — Multi-axis

| Operation | Status | Home |
|-----------|--------|------|
| 5-Axis Contour (waterline ladder, posed axis) | ✅ | M5.1 |
| 5-Axis Parallel (raster ladder, posed axis) | ✅ | M5.1 |
| Rotary offset roughing | ✅ | M5.1 |
| Multi-axis roughing | ✅ | M5.1 |
| Swarf | ✅ | M5.1 |
| Advanced swarf | ✅ | M5.1 |
| Multi-axis contour | ✅ | M5.1 |
| Rotary parallel | ✅ | M5.1 |
| Rotary contour | ✅ | M5.1 |
| Multi-axis finishing | ✅ | M5.1 |

### Turning — Operations

| Operation | Status | Home |
|-----------|--------|------|
| Facing | ✅ | M5.2 A |
| Profile roughing | ✅ | M5.2 B |
| Profile finishing | ✅ | M5.2 B |
| Roughing groove | ✅ | M5.2 C |
| Finishing groove | ✅ | M5.2 C |
| Adaptive roughing (turning) | ✅ | M5.2 E (the multi-pass ladder; see RECOMMENDED) |
| Turning groove | ✅ | M5.2 C |
| Turning single groove | ✅ | M5.2 C |
| Turning threading | ✅ | M5.2 D |
| Turning boring | ✅ | M5.2 F |
| Turning parting | ✅ | M5.2 C |
| Turning chamfering | ✅ | M5.2 E |
| Taper turning | ✅ | M5.2 E |
| Form turning | ✅ | M5.2 E2 (the crown floor of the revolve verdict) |
| Multi-pass turning | ✅ | M5.2 E |
| Drill (radial/axial, live tooling) | ✅ | M5.2 F2 (see RECOMMENDED) |
| Hole recognition (on the turned solid) | ✅ | M5.2 F2 (the cylinder primitive) |

### Turning — Part handling

| Operation | Status | Home |
|-----------|--------|------|
| Part orientation | ✅ | M5.2 G.1 (the flip grew full rotation; see RECOMMENDED) |
| Part clamping | ✅ | M3.2 (fixtures) |
| Part loading | ✅ | M5.2 G.2 (bar feeder events on the stream; see RECOMMENDED) |
| Part unloading | ✅ | M5.2 G.2 (bar feeder events on the stream; see RECOMMENDED) |
| Part transfer | ✅ | M5.3 C (the handoff intent on the stream; see RECOMMENDED) |
| Part inspection | ✅ | M5.3 D (the PROBE ask on the stream, the composed verdict at the post; see RECOMMENDED) |

---

## Phase 1 — MVP: a CAM that opens a model and posts a real program

Everything a machinist needs to cut a 2.5D part on a vertical mill today:
setup, tools, 2D operations, drilling, simulation, and one trustworthy post.

**Current status snapshot**: M1.1 (mesh side and units), the M1.2 shell, viewport, job
setup (stock + WCS) and toolpath display, M1.3 2D machining (face, contour,
pocket, slot, trace over mesh-sliced geometry, with the tool library,
operation parameters and verification warnings), and the M1.4 ribbon/browser
UI/UX rework are in; M1.5 feature selection has shipped its selection
commands, chips and 3D overlay, and its four hand-test reworks (M1.5.1:
readable selection rows, height-plane guides, and selection chains that
actually close; M1.5.2: welded-wall picks, arc moves, staged facing, retract
hops, per-level planes and translucent cut-region fills; M1.5.3: browser-row
ids that stop stealing clicks, and From/Offset height references riding
stock, model and work-offset sources; M1.5.4: a bore's wall contoured from
inside by default (Side), collision warnings folded to one line per kind,
height offsets back in view in docked panes) - the ⏸ items below remain
open. The M1.4 look-and-feel verdict prioritized the Dear ImGui → egui
stack rework, and that rework has now shipped: M1.4.1 (the decision and
GO spike) and M1.4.2 (the per-panel graduation, imgui retired) are both
done - egui is the shell. The remaining M1.4 surface polish landed right
after the graduation - viewport display modes, the tabbed-panels UX and
the graphical tool editor window; M1.6 drilling has shipped cycles,
recognition and the canned-cycle representation. What's left of the
early phase is the first post (M1.7).

### M1.1 - Core Foundation & File I/O
**Goal: a workspace that opens models and saves projects, and never crashes on bad input.** — ⚠️ partial

- ✅ **Mesh import**: STL (binary + ASCII, sniffed) / OBJ, validated at command
      creation; repair (weld, holes) still open, for toolpath-supporting surfaces
- ✅ **Robustness**: degenerate and corrupt models produce clean errors, never
      crashes (Prime Directive) - corrupt-input corpus tests enforce this
- ✅ **Project save/load**: versioned single-file project (`.hsmproj` JSON, at
      v7: tools, operations with height references and contour side, their
      cached, verified toolpaths (including arc moves) and work-offset
      placements; older files migrate on load through chained steps,
      v1→2→3→4→5→6→7, never skipping one)
- ✅ **Command foundation**: command pattern with undo/redo driving all
      UI-visible state changes from day one
- ✅ **Units**: millimeters and inches as a document-level lens - storage
      stays millimeters (geometry, the micron lattice, toolpaths, goldens,
      entity parameters) and the unit converts only at the widget and
      string edges, so flipping it changes the screen and nothing else;
      SetUnits is an undoable command (no-ops refused), the unit control
      is a radio pair on the stock page, inch input snaps onto the micron
      lattice, and the project file grew to v8 (an absent key reads mm,
      so the v7 corpus is its own migration). The post's half of the
      claim - converting at output time and declaring G20/G21 (RS-274) -
      is specced in `app::units` and lands with the first post (M1.7)
- ⏸ **IGES import**: still refused with a clean diagnostic - no IGES
      translator exists in pure Rust. STEP import shipped early, in M1.2
      (below), via the truck tessellator

### M1.2 - UI Shell & Job Setup
**Goal: real application chrome; define and display a machining setup.** — ✅

- ✅ **Menu & toolbar**: menus + shortcuts shipped (File/Edit/View/CAM,
      Ctrl+O/S/Z/Y, Del, F); the toolbar-icons item closed in M1.4 - the
      ribbon renders Tabler Icons (MIT) from `assets/ui/` through `icons.rs`
- ✅ **Setup tree pane**: models pane with viewport-synced selection plus an
      Operations section (name, type, derived never/OK/STALE status, warning
      counts), and a Setup tab editing stock and work offsets (re-surfaced as
      the M1.4 Browser tree and contextual Operations pane; same facts, same
      commands underneath)
- ✅ **Viewport**: orbit / pan / zoom, selection highlight, frame-all, dockable
      offscreen 3D target; translucent stock and WCS triads render, stock is
      pickable where nothing is behind it; operations pickables arrive with M1.3.
      The triad never doubles the dimmed world-origin axes (they are drawn only
      when no offset claims machine zero), and gizmo tips carry live X/Y/Z labels
      projected through the camera. The world-origin axes are also hideable - a
      Models-pane view toggle for when a G5x offset sits away from machine zero
      and two gizmos read as clutter; offset triads are setup, not furniture,
      and always show
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
      snapping waits on M1.5's vertex pick mode (see its ⏸ bullet);
      box-point placement is the shipped mechanism.
      Offset codes are the published RS-274 / ISO 6983 ones
- ✅ **Toolpath display**: generated paths render in the viewport - green
      feed, blue rapid, red plunge, with directional chevrons along the feed
      moves; the selected operation draws bright, others dim under a
      Models-pane "Show all toolpaths" toggle (view state, like the camera)

### M1.3 - 2D Machining
**Goal: a machinist can face, pocket, and contour a simple plate.** — ⚠️ partial

Operation geometry comes from slicing meshes at Z (the same construction the
feature-edge wireframe uses): STL, OBJ and STEP-derived models all take the
identical path, and planar-face vertices are exact in XY. Feature-edge-driven
operations wait on the kernel decision below. Even-odd region algebra and
round-join offsetting run through clipper2-rust (BSL-1.0, a pure-Rust port of
published Clipper2) behind our own `geo2d` wrapper. Staleness is derived on
read from a generation stamp (stock params + transforms + geometry digests),
never stored, so a pane can't show a stale OK about a stale path; every
command - including generation, which runs in the command's constructor -
undoes and redoes exactly, replaying byte-identical paths.

- ⏸ **IGES import + B-rep operations**: STEP import already landed (M1.2,
      truck). Face/island recognition and boolean stock ops need the kernel
      decision - truck (already linked for import) vs an OpenCascade binding;
      benchmark truck's booleans before committing. The 2D ops above work
      without it, from meshes
- ✅ **Face operation**: serpentine rows over the stock or model footprint,
      staged down from an optional Top Z by a Stepdown through the cut levels
      (M1.5.2; the default stays the single level of M1.3, so pre-stepdown
      paths are byte-stable), with rapids only at the ends
- ⚠️ **2D Contour**: islands (even-odd region sets), climb vs conventional,
      linear, arc and tangential lead in/out, tabs as skipped segments, per-level
      stepdowns with same-level hops to the retract plane between loops
      (M1.5.2), tolerance-driven arc tessellation - and since M1.5.2 the arc
      leads (the tangential half-arc since M1.3: it plunges 2*radius off the
      wall and lands moving parallel to it) and fitted rims travel as single
      arc moves, not chord fans (G2/G3 text emission waits for the post,
      M2.3). Missing: G41/G42 output - the compensation side is recorded in the
      parameters but the path is cutter-center until the post emits it (M1.7)
- ⚠️ **2D Pocket**: zigzag (scanline parity walk; islands fall out of the
      even-odd set) and waterline (inward buffers), with an Auto choice
      between them from the region's circularity - decided through `fit_arcs`
      and reported in the status, never silent (M1.5.2; rectangular fixtures
      stay zigzag), stock-to-leave as a region shrink, per-level stepdowns,
      same-level between-ring hops to the retract plane with full clearance
      only at level changes (M1.5.2).
      Missing: spiral entry and ramp/helical plunges (M2.1 has the entry
      family); plunge entries are vertical today
- ✅ **Slot and Trace**: slots as stadium regions between two points (corner
      radii from width and tool); trace runs the region's loops themselves,
      level by level
- ✅ **Tool library**: end/ball/bull-nose entities with shank, holder and
      stickout, a starter catalog of standard metric sizes, and our own
      speeds-and-feeds numbers by material group (never a copy of the
      Machining Data Handbook); referenced operations read STALE after an
      edit - no silent cascade
- ✅ **Operation parameters**: heights (clearance/retract/feed/top/bottom -
      the depth ones From-reference + Offset pairs since M1.5.3),
      stepdowns and stepover, tolerance, climb vs conventional, feeds and
      plunge feeds - full property page; parameter edits ride the history
      like everything else
- ✅ **Verification**: every generated path is checked before it caches -
      self-intersection, gouge against the region boundary at tool radius,
      shank/holder vs live stock above the cut level, rapids below the
      clearance plane - and the warnings surface in the tree and property
      page. The blocking post-gate lands with the post itself (M1.7/M2.3)

### M1.4 - UI/UX Rework: Ribbon & Browser
**Goal: the workflow sits on the surface - one persistent pane per kind of
work, and no button that fails silently.** — ⚠️ partial

Prompted by the M1.3 hand-test verdict. Layout and behavior patterns were
specced from public CAM shell conventions only (clean-room: our own strings,
our own glyphs); icons are Tabler Icons (MIT) bundled under `assets/ui/`.

- ✅ **Ribbon bar**: a persistent command strip over the viewport, grouped
      File / Setup / Create / Modify / Inspect, with per-button Tabler icons
      and contextual disabling whose tooltip states the reason (closes the
      M1.2 toolbar-icons item)
- ✅ **Browser tree**: the left dock pane nests Setup (stock, G54-G59),
      Models, Tools and Operations; per-node type icons, status badges
      (never/OK/STALE plus warning counts), and eye toggles for model and
      path visibility - view state like the camera, never a command; hidden
      models are transparent to picking, so a click selects what you SEE
- ✅ **Contextual Operations pane**: one right-hand pane pages by target -
      operation parameters and status, the tool page (the standalone Tool
      Library window is gone), stock and WCS pages, model info - replacing
      the four pop-out-looking tabs
- ✅ **No silent refusals**: every command refusal renders inline under the
      action that caused it, and risky buttons pre-validate (disabled with
      the reason on hover) - the three M1.3 hand-test bugs, fixed at the
      plumbing level, not just those three widgets
- ✅ **What-gets-cut preview**: selecting an operation draws its region
      loops in the viewport at cut Z, straight from the generators' own
      region builder (`operation::region_of`), and hovering a model
      highlights the facet under the cursor (`pick::first_hit_f`) - the
      facet-index hook M1.5 feature picking builds on
- ✅ **View cube + viewport navigation bar**: the viewport's corner
      furniture - a top-right cube and a nav column beneath it. The cube is
      drawn through the camera basis and picked by the same projection (an
      orthographic slab raycast classifies the click's cube-space entry as a
      face, edge or corner and commands `Camera::toward`), so what lights
      under the cursor is what the click turns to; a press on the furniture
      arms no gesture, exactly like a ribbon button's. The nav bar's Fit
      queues the same `FrameAll` the F key does, Iso is the house
      orientation, and Wire/Paths/Axes are direct view-state writes as
      panes.rs does them (the milestone the display modes opened)
- ✅ **Graphical tool editor**: tool creation and editing move from the
      property page's field list to its own window with a live preview -
      the dock keeps the LIBRARY (a composed world summary, Edit/Delete,
      the catalog) and the editor is the shell's first floating surface:
      fields left, tool-shank-holder side view right, drawn to scale
      from `tools::Tool::profile` - the same band stack `verify`
      collides - and updating as the numbers change, the draft law
      keeping the world untouched until Apply. ✕ closing plus the ghost
      law in the drain keep it honest; the open subject is ephemeral
      view state, never a command. The window form is the considered
      revisit of the "standalone tool library window is gone" decision,
      as specced - builds on the M1.4.1 stack (this commit)
- ✅ **Viewport display modes**: wireframe-only and shaded-with-edges,
      switched by the browser pane's Solids radios - view state under the
      same law as the axes and paths checkboxes, never a command.
      Wireframe sends faces to the depth buffer only (hidden lines stay
      hidden) and brightens the feature-edge wire for the dark background;
      the stock, a volume rather than a surface, keeps its full wire.
      Generalizes the STEP feature-edge wireframe from M1.2 to every
      model - the pixel law is tested in `tests/gpu_render.rs` (this commit)
- ✅ **Panel UX overhaul**: the operation page is five named sections
      behind a tab bar - Geometry, Heights, Passes, Leads & Links, Feeds -
      instead of one long stack, so a parameter reads as part of a page,
      not another row. A kind shows only the sections it has fields for
      (Trace has no Passes, non-Contour has no Leads), Status lives above
      the bar so warnings never hide behind a tab, and the open tab is
      ephemeral view state like the drafts it reveals - never a command,
      never saved. `every_field_lives_on_exactly_one_section` is the
      drift guard (this commit; the M1.4.2 migration it rode on is below)

### M1.4.1 - UI Framework Rework: Dear ImGui → egui
**Goal: make the look-and-feel complaint structural instead of another
restyle - rebuild the shell on a toolkit we fully control, keep every
behavior the app layer has earned, and ship the Fusion 360-caliber dark
UI that ImGui styling has not reached.** — ✅ decision + spike done 2026-10-17
(GO verdict; the graduation itself shipped as M1.4.2 below)

Prompted by the M1.4 hand-test verdict: testers' single biggest complaint
is the Dear ImGui look and feel, and restyling inside ImGui has not closed
it. The target - general modern dark-CAD conventions, our own strings and
glyphs (clean-room) - is buildable on egui. Decisions recorded after a
three-part investigation (UI inventory, egui, clay), October 2026:

- ✅ **Decision - egui replaces Dear ImGui**: the current egui line pins
      wgpu 30 and winit 0.30 - exactly what this shell already runs, so
      no forced-upgrade ripple; egui_dock gives the docked pane layout;
      the viewport composites into a docked panel as a registered
      texture, the pattern Rerun runs in production; egui_kittest is a
      stronger headless harness than the current one - only two tests
      execute an immediate-mode frame today. The dock crate is
      single-maintainer and trails egui's quarterly releases by a minor
      (0.21 targets egui 0.36); our layout is built in code and ini
      persistence is already disabled, so no user layouts exist to port -
      worst case the small crate gets vendored.
- ✅ **Decision - clay considered, not now**: a layout engine only - no
      widgets, no docking, no accessibility; the Rust binding trails the
      C master and carries an open unresolved use-after-free report,
      disqualifying against the never-crash directive. A full UI on clay
      means writing a toolkit from scratch (~50-100k LOC); grafting clay
      under egui for layout is an anti-pattern with a bad precedent. If
      finer layout control ever matters, a pure-Rust taffy-class engine
      (kaolin, cobogo, kui-core) is the FFI-free way there - clay's
      ideas yes, the dependency no.
- ✅ **Decision - no dual-stack hybrid**: an imgui/egui bridge means two
      toolkits fighting over hit-testing and focus in one frame.
      Migrate per-panel, switch each panel wholly, never split one.

Spike first - a branch, the go/no-go evidence (**done** 2026-10-17,
verdict **GO** in `docs/spike-egui-go-no-go.md`; `ui-egui-spike`
`8219f62..644713a`, restore tag `pre-egui-spike`):

- ✅ **Shell** @ `482a54b`: eframe-less egui-winit + egui-wgpu over the
      existing winit 0.30 event loop; View3D's offscreen texture docked
      as an egui native texture, rebound on resize; orbit/pan/zoom
      through egui pointer responses; headless readback proves the 3D
      view under the new shell (caught the TexturesDelta drain bug).
- ✅ **One real page plus the painted chrome** @ `1930f76`: the Contour
      page on the same commands as imgui, painted ribbon/browser/status
      (the draw_list port), the dark theme module; 100%/150% PNG dumps
      judged by eye - the small-text risk from the investigation is
      cleared (egui's CPU atlas rasterizes per pixel ratio).
- ✅ **Harness** @ `644713a`: egui_kittest frame tests driving the same
      `shell_layout` the window runs - pointer-event command round trip
      and a committed snapshot as the migration churn gate; the golden
      CLFile/G-code gate was untouched throughout.

### M1.4.2 - egui graduation - the migration itself
**Goal: every surface runs on egui, imgui exits the tree, nothing the app
layer earned changes behavior.** — ✅ done 2026-10-17

Ran the spike report's "Migration order" as one commit per step, full
gate ladder green at each: ~3.4k LOC and ~350 imgui-facing call sites,
all inside the ui crates; crates/app was already 100% imgui-free, so the
layering (ui → app → flecs), commands, systems, persistence and every
generator stayed untouched (the golden CLFile/G-code suite never moved).

- ✅ **M-a** shell graduation @ `b7527c7` - the egui loop/dock/View3D-texture
      law moved from the spike bin into the lib as the `--ui egui` entry;
      `test_harness` learned to spawn it.
- ✅ **M-b** ribbon + browser @ `b687407` - painted strips graduated, tree
      column model per browser.rs (the spike's eye/warn overlap is gone).
- ✅ **M-c** property pages @ `8e5a988` - the five 2D operation pages +
      heights + warning fold, then setup pages; pages arrived as one
      scroll (parity with imgui) - the tabbed redesign was named a
      post-migration carry-over and landed after M-g, not here.
- ✅ **M-d** dialogs + file ops @ `44c6e3a` - rfd wiring, new/open/save,
      error surfaces; the window opens on a real empty document, the demo
      seed retired to the harnesses and the headless proof.
- ✅ **M-e** viewport interaction @ `431a32f` - pick, hover chips, anchor
      grid, overlay handles through egui pointer events; the law moved to
      `crate::interact`, shared, not mirrored.
- ✅ **M-f** actions + paths panels @ `0bacce2` - every surface fires
      through `crate::actions`; tips name keys that actually work (both
      shells gained Ctrl+N/O/Shift+O/S arms); the painted ribbon became
      hit-testable through `ribbon_layout`.
- ✅ **M-g** retire imgui @ this commit - the three dear-imgui deps and
      `--ui imgui` left the tree (named refusal, rc 2), the nine imgui
      surface modules deleted, the Tabler glyphs now ride egui textures
      through `egui_app/icons.rs` with the painted vectors as fallback,
      and the graphify rebuild re-baselined the knowledge graph.

Parity held as promised: same features, same command wiring, undo/redo
gated by tests, not hand-test. The M1.4 carry-over (graphical tool
editor window) rode the new stack to done; viewport display modes and
the tabbed panel-UX redesign landed right after the graduation as their
own M1.4 items. Version coupling came
true the cheap way: egui 0.36.2 + egui_dock 0.21.1 held the pin through
the whole migration, snapshot diffs caught every paint change.

Honest risks (from the spike, now measured): focus/IME edge cases stayed
Latin-first fine; no built-in animation easing; egui-minor churn is
bounded by the kittest snapshots.

### M1.4.3 - Structured Ribbon - a home for every operation
**Goal: the whole catalog is reachable without drowning the ribbon.** — ✅ (landed as
M6.2 in Phase 6 - the home law below was overtaken by events, see there)

- **Sections by family**: the ribbon divides into labeled sections - Milling
      2D, Milling 3D, Drilling, Multi-axis, Turning - mirroring the
      Operation Catalog; a setup's machine kind (M3.3's mill/lathe/mill-turn)
      decides which sections it offers, so a lathe setup never shows waterline.
- **Overflow dropdowns**: operations beyond a section's width fold into a
      dropdown that carries the section's label - nothing is ever hidden
      behind an iconless chevron.
- **Pin to ribbon**: any catalog item can be pinned (and unpinned) by the
      user; pins are a document setting - persisted with the project like
      panel docking, not an undoable command (undo replays cuts, not layout).
- **The home law**: M1.4.3 lands before the first operation of any new
      family (the M2.5 strategies, the M5.1 multi-axis ops, the M5.2
      turning ops) - an operation with no ribbon home is a bug, not
      a backlog item.

### M1.5 - Feature Selection
**Goal: pick a face or an edge chain and machine that - not just whole
models.** — ⚠️ partial

The first customer of M1.4's `first_hit_f` facet indices shipped:
planar-face clustering and tangent-continuous edge chains run off the
feature-edge weld, lifted into `app::geometry` (mesh-based - no
geometry-kernel commitment; that stays the M1.3 ⏸ open question), with
ids stable per model revision and enforced by the generation stamp.
Picks are commands (undoable, validated at construction), show as
readable rows in the Geometry section and as 3D overlay lines that hold
their place under orbiting, and persist in the project file
(`.hsmproj` v4 then; v5 since the M1.5.2 rework, v6 since the M1.5.3 one
and v7 since the M1.5.4 one below). Contour machines both halves of a
selection: the picked
loops as boundary passes, the picked open runs as a centerline pass -
preview and generation read one chain construction, so they cannot
disagree. The
M1.5.1 hand-test rework (off `Milling-test.step`) made the
selection itself legible, honest and geometrically complete; the
M1.5.2 rework closed the seven issues the second session on the same
model brought back, the M1.5.3 rework closed the third session's three,
and the M1.5.4 rework the fourth's - heights now say where they come
from and ride it, and a bore's wall is contoured from the inside by
default; all 2D, per the standing rule to finish the 2D story before
the 2.5D/3D ones.

- ✅ **Facet → feature**: weld mesh facets into planar faces and edge
      chains, exposed as stable ids per model revision (`app::geometry`,
      lifted out of `viewport.rs`)
- ✅ **Pick modes**: whole model, face (raycast through the facet map)
      and edge (nearest feature-edge search; Ctrl/Shift multi-add);
      vertex picking landed 2026-11-06 under M13.4 (feature corners,
      screen-space nearest with a depth test - RECOMMENDED "M13.4:
      vertex picking and Set origin")
- ✅ **Boundary-driven operations**: contour a selected face or edge
      chain instead of a stock section (`SrcRef::Selection` - the
      shipped name for `SrcRef::Faces` above); islands come from the
      picked loops, picked open runs get the centerline pass, and an
      open run on a Face op extends to the stock boundary and bounds a
      real region (M1.5.1, below) - nothing is silently dropped
      anymore; what cannot be used is refused by name
- ✅ **M1.5.1 hand-test rework** (the four issues from the first
      feature-selection session, commits `ddb559a` → `a785eca`):
  - the Geometry section is a readable list - "Top face · z=22.00 ·
        142.6 mm²", "Closed chain · 199.8 mm · 63.5 × 63.5", "Open run
        · 122.4 mm · 4 of 6 runs" - and hovering a row highlights that
        feature in the viewport (amber, immediate-mode);
  - Apply always reports what it did (status line or queued command);
  - tessellated rims chain closed (smooth-band merging: shared face +
        shared crease patch across the step < 40°) and picked open runs
        stitch into closed rings by quantized endpoints;
  - the open operation's heights draw as labeled planes in the
        viewport: Clearance (blue), Retract (sky, Contour only), Cut
        (red), beside the existing orange Top; planes track draft
        edits without a document rebuild
- ✅ **M1.5.2 hand-test rework** (the seven issues from the second
      feature-selection session, commits `e6b4749` → `e805c64`):
  - the op list lists: operation Type locks at creation - the combo renders
        only on the new-op form, committed ops show a static kind label, and
        `EditOperationParams` refuses a foreign variant at the command layer;
        Generate flushes a visible dirty draft through the history first (one
        click, two honest commands), and the `OperationN` prefill skips taken
        names;
  - a raycast onto a curved wall answers the weld, not the strip:
        `feature::patches` groups planar strips sharing a facet-patch root
        under a `PatchId` (boundary segments on the position lattice plus
        strip-plane signatures - order- and winding-free, stable across
        re-imports), and `FeatureSel::Patch` bounds its rims through the same
        material-side rule, so a bore-wall pick machines the annulus and a
        boss-wall pick the disk; flat faces keep plain `Face` ids, so no v4
        pick re-keys;
  - arcs move: `Move` carries a planar arc (`PathBuilder::push_arc` refuses
        helicals and zero-sweep), lead-in/out post one arc move, and
        `fit_arcs` replaces tessellation chords on a tri-state window (fit /
        not-yet-proven / refuse) - contour_plate_with_hole's golden went
        330 → 69 moves and the waterline 228 → 51 with non-arc goldens
        byte-identical; goldens log v2 arc lines, centers stand on the µm
        lattice, and the verifier flattens through `Move::chords`, the same
        sagitta law the overlay and dump use, so warning indices stay stable;
  - facing steps down: Top Z + Stepdown stage the same band law through
        `cut_levels`, levels snapping to the finish exactly - the unstaged
        default is the old single level byte-for-byte, and a staged face whose
        first level would plunge zero depth refuses;
  - the retract plane earns its keep: same-level lifts between passes travel
        `min(retract_z, clearance_z)` - contour's loop-to-loop transitions and
        waterline's island splits hop there, level changes still cross at full
        clearance, and a retract under top_z is refused by name (a hop plane
        inside material is a gouge with a plane named after it). Pocket, Slot
        and Trace carry the plane in data; Slot shows no control for it - one
        serpentine per level has no same-level lift, and a dead knob is a lie.
        Waterline rings hop only where an island splits them, so island-free
        spirals still chain at depth;
  - height planes are one truth: `shown_regions` gates region outlines, plane
        stand-ins and fills level by level (a staged Face's Cut rectangle
        stands exactly where no outline paints its level - the invisible-
        Face-Cut bug), labels name their operation ("Operation1 · Cut 22.00")
        and clamp into the viewport; Retract widened from Contour-only to
        every depth kind;
  - circularity decides the pocket: `PocketStrategy::Auto` - the new default
        for new pockets, never retyped over an explicit choice - runs
        Waterline where `fit_arcs` covers nine tenths of a rim around centers
        a quarter-stepover apart, Zigzag otherwise, and the panel quotes the
        generator's own `resolve()`, so the decision reads in the status
        instead of guessing silently;
  - cutting is visible: `geo2d::Region::triangles()` (bridge-merge +
        ear-clip, our construction from textbook literature - clipper2 does
        not triangulate) lays a translucent film (α 0.22, depth-write off,
        its own pipeline) over the stock at each shown region's level - holes
        stay void, nested islands refill, and any degeneracy refuses to
        wireframe-only, never a partial sheet; the status line counts moves,
        plunges, levels and arcs from the cached toolpath, and stitch chips
        annotate `· → ring k` through the stitcher's own map.
  Deferred by decision: G2/G3 text emission rides the post (M2.3),
  helical/ramp entries stay M2.1's family, rest-material stock shading is
  simulation work (M2.3/M4).
- ✅ **M1.5.3 height references** (the three issues from the third
      hand-test session, commits `7e95c31` → `ea2628b`):
  - the browser row stops stealing its own clicks: row and eye carry
        distinct ImGui ids, so the "2 visible items with conflicting ID"
        warning is gone and clicking between operations switches the
        property pane again;
  - every height is From + Offset: `operation::Height` pairs a
        `HeightReference` (Fixed value, Top/Bottom of stock, Top/Bottom of
        model, Work offset G54..G59) with an offset in mm, resolved against
        the live world wherever a z is needed - the pane's combo and slot
        picker with a `(= Z …)` echo, the viewport chips that name their
        source ("Operation1 · Bottom 2.00 (G54 -1.00)" - Fixed heights
        stay bare labels), verify and generation through `resolve_params`,
        which refuses by field and by reason. Move a G54 anchor and every
        plane, echo and stamp that rides it moves with it;
  - new operations default part-aware - Top of model + 0, Bottom G54 + 0,
        rapids 25 above the stock top - so the bottom height is the
        selected work offset, never an arbitrary five under the stock top;
        a fresh Face skims the stock top, because a Bottom default would
        cut through the whole block;
  - `.hsmproj` v6: saved numeric heights migrate to `Fixed` bit-for-bit
        (goldens unchanged), work-offset placements finally persist with
        their anchor, and the generation stamp folds each height's
        reference (salt v2) - committed operations read Stale once after
        the update and Apply clears it;
  - the honesty invariant extends: an unresolved reference draws no
        plane and no number, echoes what is missing and where to fix it,
        and blocks generation by name - a guessed z never reaches the
        viewport or the post.
  Deferred by decision: rotations never tilt a height reference - a WCS
  slot contributes its origin's world Z (frame transforms are the
  2.5D/3D milestones' territory), and stepdown/feeds stay plain deltas.
- ✅ **M1.5.4 hand-test rework** (the three issues from the fourth
      hand-test session, commits `871418e` → `047e327`):
  - the warning flood folds: a shank/holder check that collides on
        sixteen moves printed sixteen pane lines; the display groups by
        kind (and by part for a shank hit) now - one line, its first
        offender named, "repeated on N more moves (to X)" appended -
        while the persisted `Vec<Warning>` stays move-by-move, exactly
        what the file format promises;
  - contour learned which side to cut: a picked ring whose bounded area
        is mostly air is a bore, and Auto machines inside it - the fill
        law asked of the very walk generation runs, with a note saying
        "machining inside the ring" - so the wall is contoured from the
        axis side instead of around the outside, the hand-test's
        mis-cut. Explicit Inside/Outside always override the verdict;
        Inside on a whole-section source is refused by name (its `+`
        law already cuts hole walls from inside by parity); a climb cut
        on the inside reverses its travel to keep the material on the
        tool's left, and the leads bulge into the bore's air;
  - `.hsmproj` v7 stores the side as the user's ask (absent key = Auto,
        the serde default is the migration); the generation stamp folds
        it, so existing contour ops surface Stale once after the update
        and Apply clears them;
  - the height rows came back into view: From and Offset stack in two
        rows instead of one clipped `same_line` chain, so the offset box
        and the `(= Z …)` echo survive the narrow docked pane, and
        "Fixed value" labels its box "Value" - the z itself, no hidden
        `+ 0`.
  Deferred by decision: Auto's verdict never rewrites the picked loops
  (preview stays consistent by construction) and stays inert, with a
  note, for selections that resolve to open runs only; the fill law
  scores a lone ring against the selection silhouette and never reuses
  Face's complement comparison, which misfires on a small boss.
- ✅ **WCS face/vertex snap**: done 2026-11-06 under M13.4 - "Set origin"
      on a picked vertex or face (centroid; Z only on horizontal faces)

Known limits from the M1.5.1-M1.5.4 reworks - each one refuses by name or
degrades by rule, none guesses silently:

- the material-side rule: an open run or chain-only ring is scored
      against the part material silhouette and the side bounding more
      material is machined, outward-biased by `OUTSIDE_BIAS` (0.5 in
      `operation.rs`); a convex boss picked by one arc therefore faces
      the whole boss - by design, the opposite side is "pick the other
      run"
- run extensions are chord-lens-probed (no strip may cross the run
      that bounds it) and a ray that misses, or a self-crossing
      strip, refuses as ambiguous - "use a closed chain or a Face
      pick"; straight (unbent) runs have no inside and score both
      sides, so a straight run with no stock face can face the whole
      part deterministically
- Trace refuses open runs by name (close the pick or use Contour);
      Pocket always refused them, also by name
- stitching is per-entity - two runs meeting across two model
      entities do not yet compose into one ring
- chain, face and patch ids are geometry digests: the smoother chaining
      rules re-key picks on affected meshes, so old v4 projects show
      "missing" on a handful of picks and need one re-pick (stale ids
      refuse safely); patch ids live in their own namespace, so a weld
      never claims a flat face's id, and re-welding a mesh re-keys
      patches the same way
- facing rows are still the M1.3 bbox serpentine, staged down since
      M1.5.2: the region's holes bound contour and pocket passes, not
      facing rows

### M1.6 - Drilling
**Goal: a complete hole list is machined correctly.** — ✅

- ✅ **Drill cycles**: spot, drill, peck (chip-breaking), deep-hole with
      dwell - `drilling::generate` expands each hole into a chained move
      sequence (Rapid over, Plunge, retract), and `CycleChoice::Auto`
      picks the shape from the hole: depth ≤ d drills in one plunge,
      ≤ 4d pecks with a chip-break hop, deeper pecks with a full
      retract and dwell; Spot is never automatic, only a spot tool or
      an explicit choice
- ✅ **Rigid tapping** (simple, small pitch first): Tap cycles descend
      and retract as Feeds at rpm × pitch (the Tool's `pitch` field),
      the spindle-sync shape the post will emit as G84. **Thread
      milling stays open** - no helical-interp post exists yet (M1.7+)
      (done since: M2.4 `machining2d/thread_mill.rs`)
- ✅ **Boring and back-boring**: basic cycles - Bore feeds down and
      rapids out, BackBore feeds down and feeds up; the tool's `kind`
      (with `tip_angle` for drills, `pitch` for taps) drives the
      collision profile
- ✅ **Hole recognition**: `drilling::recognize` works on the raw
      world-transformed facets, not features - welded connected
      components of near-vertical wall facets, closed-rim and
      normal-span tested so slots and block sides never qualify, a
      Kasa circle fit per component; the **Detect holes** button on the
      Drill page turns the result into `Hole` rows on the operation's
      params (editable, undoable, stale on model change)
- ✅ **Canned-cycle representation**: posting itself is M1.7, but the
      path now carries the intent - `Toolpath.cycles` records one
      `Cycle` per hole (kind, xy, R/B/entry planes, peck, dwell, first
      move) beside the fully expanded chain, the `.cl` golden writer
      emits them as `; CYCLE` comments, and a law test pins the two
      views agree. The post will consume them instead of
      re-deriving G73/G81/G83/G84 from moves

### M1.7 - Simulation & Post (Release 0.1.0)
**Goal: verified output - first public release.** — ✅

- ✅ **Move simulation**: rapid vs feed differentiation, feedrate readout, play/pause/scrub -
      one timed `Script` per document (chain-is-truth; cycles contribute dwell
      seconds only), transport bar with play/pause/scrub and 1-8x speed, live
      op/feed readout and a projected tool marker; machine-file traverse rates
      pace the rapids
- ✅ **Validation warnings**: rapid-move violations and holder-vs-stock contact, blocking the post (M1.7b gate)
- ✅ **Post engine + CPS posts**: not a hard-coded writer - the Fusion 360
      post architecture adopted as file-format conventions, specced
      from Autodesk's published post-processor documentation
      (behavior only) per the Clean-Room Protocol; the word semantics
      the engine emits - RS-274 address words, modal groups,
      canned-cycle geometry (G73/G80-G89/G84), G41/G42 compensation,
      G90/G91 distance modes, tolerance math - are specced from the
      public NIST reference NISTIR 6556, "The NIST RS274NGC Interpreter
      - Version 3" (Kramer/Proctor/Messina 2000), ingested at
      `raw/nist_823374.pdf` and in the knowledge graph as the
      RS274NGC spec community; the engine is our own, written from
      scratch in Rust - three file roles and one engine:
  - **`.cps` post scripts** (our own format implementation, our own
        text): a manifest header (`description`, `vendor`, `legal`,
        `certificationLevel`, `minimumRevision`, `extension`,
        `setCodePage`, `capabilities`) plus the lifecycle hook set an
        intermediate drives - `onOpen`, `onParameter`, `onSection`/
        `onSectionEnd`, `onCommand`, `onLinear`/`onRapid`/`onCircular`,
        `onCycle`/`onCyclePoint`/`onCycleEnd`, `onDwell`,
        `onSpindleSpeed`, `onRadiusCompensation`, `onComment`,
        `onClose` - reading the CL stream through `getProperty`/
        `setProperty` and emitting text through the write primitives.
        The control dialect lives in the script, never in the engine
  - **`.mch` machine configurations**: JSON definitions - capabilities
        ("milling"), a kinematics tree (static → head with spindle
        min/max speed and a tool station listing supported coolants →
        driven X→Y→Z axes with direction, reversal, rapid/normal
        speeds, zero offsets and TCP → table) and the rotation
        convention; they feed the post's WCS/offset and machine-limit
        decisions
  - **our own `.cnc` intermediate**: the CL stream the post consumes -
        our `.cl` Toolpath dump, carrying per-operation parameters and
        the M1.6 `; CYCLE` records, is the posting input (we define the
        format; only its *role* matches the reference tool's
        intermediate); engine CLI mirrors the reference:
        `post [OPTIONS] <config> <intermediate> <output>` with
        `--machine`, `--log` and counted warning reporting, plus the
        same in-app post path
  - the reference `post.exe` and the root fixture trio
        (`05-xyz-2.cps`, `1001.cnc`, `MIllingmachine.mch`) are local
        development validators only - our `.cps` files must run
        through the reference engine as a format check; the vendor
        binary is never bundled or redistributed as product content,
        no vendor script text is ported into our files, and every
        shipped post, machine file and our `post` binary are original
        work
- ✅ **Fanuc post**: the first `.cps` we ship - program header, tool
      changes (T../M6), work offsets (G54+ from the machine/setup),
      canned cycles emitted from `Toolpath.cycles` (G73/G81/G83/G84
      with R/Z/Q/P/F words, G98/G99 retract modes), G2/G3 arc motion,
      G40/G41/G42 radius compensation, coolant M-codes (read from the
      `.mch` tool-station coolant list - the in-app machine workflow
      for those files is M3.3's addition, not a prerequisite)
- ✅ **G-code viewer**: inspect the posted program in-app, with toolpath
      highlighting on scroll - a successful post keeps the exact text it
      wrote, the viewer resolves each line to the move that line posts
      (from the post's own stream records, pre-origin-shift world mm), and
      scrolling the listing draws that move amber in the viewport. The
      window follows the tool editor's law; a document switch retires it.
- ✅ **CI**: build matrix (`windows-latest` axis, structured to widen - RECOMMENDED.md),
      golden regression suite named first, then app + UI suites, clippy
      `-D warnings`, headless app run (.github/workflows/ci.yml)
- ✅ **Release build**: portable zip (`hsmworks-ui.exe`, our `post.exe`, the
      `.cps`/`.mch` data the CLI reads from disk) assembled and published on
      a `v*` tag, version taken from the shipped binary itself; release
      notes in `RELEASE_NOTES.md`. Unsigned - no code-signing certificate
      is owned yet; the installer (vs the zip) is a post-0.1.0 discussion

---

## Phase 2 — 3D Machining

From prismatic parts to molds and dies. Roughing that holds constant tool
load, finishing that meets a surface tolerance, and simulation you trust
before touching metal.

> Reference on the graph: two published papers on tool-path generation and
> path efficiency sit in `pdf-sources/`, extracted into the knowledge graph
> as this phase's speccing literature (public technical literature; nothing
> from HSMWorks):
>
> - `pdf-sources/book2.pdf` — Kumazawa, Feng & Barakchi Fard, "Preferred
>   feed direction field: a new tool path generation method for efficient
>   sculptured surface machining", Computer-Aided Design 67–68 (2015) 1–12:
>   a preferred-feed-direction (PFD) tensor field over the surface — at
>   each point the feed direction that maximizes machining-strip width —
>   whose degenerate points and separatrices segment the surface so
>   iso-scallop paths follow each region's field with less strip overlap
>   and a shorter total path. Speccing literature for M2.2's parallel and
>   scallop placement laws and M2.5's steep-and-shallow partition.
> - `pdf-sources/book1.pdf` — Zhou, Zhao & Li, "CNC double spiral tool-path
>   generation based on parametric surface mapping", Computer-Aided Design
>   67–68 (2015) 87–106: tool paths as isothermal lines of a
>   thermal-conductivity PDE solved in a flattened parametric domain,
>   smoothed by a bounded-curvature optimization and mapped back onto the
>   NURBS surface; the double-spiral tour links without retractions.
>   Speccing literature for the polar/spiral placements (M2.2), M2.5's
>   morphed-spiral and flow items, and M11.1's smoothness thread.

### M2.1 - 3D Roughing
**Goal: high-efficiency roughing that keeps the tool loaded and cool.**

- ✅ **Adaptive clearing** (M2.1c): constant-engagement rings from the
      published HEM relation step = r·(1 − cos θ) — the stepover that holds a
      chosen contact angle at a convex wall — fed to the waterline engine, so
      the ring family is a function of tool radius and engagement angle, not a
      fixed distance (the *tiling law*: Adaptive ≡ Waterline at the law step).
      The 3D pocket gains the `Adaptive` strategy and the 2D pocket an alias;
      `engage_deg` (default 45°, legal in (0°, 90°]) replaces stepover on the
      page. Chaining the rings into tours grew the *join law*: a chord may link
      rings only if it clears the area AND properly crosses none of the level's
      planned paths — containment is not non-crossing (the boss bug); chords of
      different chains can cross without touching any loop, so a post-chaining
      sweep cuts the later-listed such chord into an honest rapid hop; and ring
      closure is a property of the point DATA, never a motion the walker adds
      (the closing diagonal a verify test caught). Trochoidal slot paths and
      corner-contact/trochoidal fallbacks stay open (M2.1d feed modulation
      closed alongside them, separately below).
- ✅ **3D Pocket** (M2.1a): mesh-driven, not sketch-driven — the *level law*
      makes each Z-plane's target the silhouette footprint minus the columns
      the solid still fills at that plane (`machining3d::material_above`), so
      islands are computed from the model at every level and part geometry
      standing between two levels floors its columns at its own surface (at
      most one stepdown of stair, reported, never hidden). Entries: plunge,
      ramp and helix, each dive-point legality proved against the previous
      cut level (superset law: deeper planes' islands contain shallower
      ones, so a descent never enters live wall); an entry that cannot be
      proved falls back to a slower vertical dive rather than a gouge.
      Waterline rings (concentric-offset spirals) and zigzag scans share the
      level generator; zigzag can serve as Waterline under `Auto`.
- ✅ **Rest material** (M2.1b): the stock model is a column-wise Z-map
      (`stock_sim::Dexel`) — the *verification layer*, never a second source
      of truth: operation geometry and the tolerance laws still answer against
      the mesh. Subtraction is the exact reach-interval of each swept disc on
      the move's segment (an empty window cuts nothing — no endpoint
      phantom-cuts; arcs ride their sagitta chords, so the sim over-reports
      rest and never invents a gouge; rapids touch nothing), the part surface
      is read off the mesh's own facet planes, and `measure` reports rest,
      gouge and gross removal against it. `pocket3d_with_rest` runs the level
      ladder, re-simulates it, and plans a half-step closing ladder over only
      what stands (`above(z)` per level), entering always by plain plunge (the
      nesting law makes deeper columns superset-clear) with the all-clear
      verdict as success; `rest_rough` rides the operation params (stamped,
      undoable; the v12 step-up is a serde-default no-op). The *corner law*
      came out of the honest reports: no disc center reaches a square
      pocket's corner columns — the report names them, and fillet or
      point-pass strategies (M2.1c+) are what close them. Tool-vs-tool rest
      (previous-tool stock) inherits the same grid the moment operations
      share one.
- ✅ **Roughing simulation**: the same dexel grid, as verdict not shader —
      a generated pocket path measures zero gouge cells against the model
      surface before it is trusted, and the staircase it leaves is reported
      in volume, cells and height, never averaged away (color-mapped
      deviation against the final model is M2.3's)
- ✅ **Feed-rate optimization** (M2.1d): engagement-based feed modulation
      within tool limits - `optimize_feed` re-runs the finished path through
      the dexel sim and scales every planar feed word by
      `median/engagement`, clamped to `[FFO_FLOOR, 1]` of the programmed
      feed (the *median-anchor law*: the path's own median loaded bite is
      what runs at the programmed feed; heavier peaks - full-width first
      cuts, links into fresh shelf - slow toward it, never past it, because
      the op's feed *is* the tool limit this build models). The probe is a
      two-station perpendicular chord (mid and 3/4 along the move), the
      trapezoid of the swept side width, so a stepover link reads its real
      half-bite instead of the trough it exits; air and zero-depth skin
      read 0, keep the programmed feed and hold no vote in the median.
      Geometry is byte-identical - only F words move. Vertical feeds are
      untouched (axial engagement is a different curve). Toggle
      `feed_opt` on the operation params (stamped, undoable; v14 step-up
      is a serde-default no-op).

### M2.2 - 3D Finishing
**Goal: mold-quality surfaces at a stated tolerance.** — ✅

- ✅ **Parallel** (M2.2b): XY raster with Z-follow and the 45-deg
      auto-split - shallow terrain (|m_z| ≥ 1/√2, the scallop law's own
      jurisdiction line) is ridden by tangent ball-center lines at the
      stepover cos-law spacing `s = √(8·r·tol)·cos α`, every support
      sample the topmost valid tangent center (floor- and
      ceiling-outranks laws, ball-clear like the ring's), lines broken
      at ground discontinuities rather than straddled, and the steep
      flanks of the same model carried by the waterline ladder's rings
      at their own levels - one op, one toolpath, both truths measured
      by the envelope kernel. Multi-direction runs the raster twice
      (0°/90°) with retract hops between pieces; stepover override
      rides the params (stored ≠ derived, persistence v16).
- ✅ **Waterline** (M2.2a): Z-level finishing as the forbidden-center
      tangent locus - each ring is the boundary of (model ⊕ ball) clipped
      at a center plane, so the ball rolls tangent, never into, the
      surface; stepped by the scallop law `Δz = 2√(r·tol)` from top to
      bottom (snapped), XY chords fitted to the surface tolerance,
      walked by the climb XOR at `retract_z`-clamped hops. The cut
      rings are the region's OUTER loops only (interior wavefronts are
      unreachable from outside; islands are separate CCW components),
      refused clean on flat-bottomed tools, bad tolerances and an
      empty model. Feed at tip-Z; the center rides `z + r` - one law,
      every altitude in the kernel.
- ✅ **Scallop** (M2.2c): the raster family rotated to the diagonals
      (±45°, the directions no axis-aligned feature runs parallel to)
      - the constant cusp measured per law (f), never bought with an
      analytic √2 discount a union-of-removals argument says a crossed
      pair never had on flat ground. One generator with Parallel, all
      its refusals; `verify`'s Crossing law stands down in
      `Mode::Envelope` (crossed feeds at shared depth are the pattern,
      not a defect), 2D verdicts untouched; the pattern word slots into
      the v16 grammar without a version bump.
- ✅ **Radial and Spiral** (M2.2d): the polar ground layouts ridden by
      the march's own laws - Radial sends `N = ceil(π·R/s)` chords
      through the shallow footprint's center, so arc spacing at the rim
      meets the cos-law stepover (the overlap toward the center is
      correct, merely redundant: the cusp law is a bound, never a
      quota); Spiral winds ONE Archimedean curve in from the silhouette
      and stops a half stepover from the pole, where the next turn
      would only retrace. Both reuse the raster's machinery verbatim -
      the adaptive march (validity bisection, quarter-budget chords,
      the discontinuity break), the hop re-entry, and the rings that
      take the steep flanks - and slot into the v16 grammar as
      append-only ordinals, no version bump. The parameterized march
      ends by a labeled break now: the spiral always arrives at its
      end-of-range on valid ground, where a bare break returns to a
      still-true loop condition and spins (the same latent exit the
      raster's fixtures had only been hiding by always dying at the
      bbox edge).
- ✅ **Hidden rest, the solid truth** (M2.2d): the envelope's rest
      number pays M2.2b's logged debt. A vertical column through the
      model is a ray: paired crossings bound material intervals - the
      claims law (coincident-plane groups count interior claims, merge
      bare-boundary ones into one, and report an odd column as
      unknown, never as material). Every interval top the swept ball
      never touched, sheltered under a ceiling, raises `rest_mm` when
      it exceeds the height-field figure - and only then: ridge
      predicate, rest_cells, gouge and every golden byte-untouched
      (proved on the full release suite). Over the 1.2 M-cell /
      40 M-claim budgets the solid scan answers with the stated zero -
      the field's debt stays on the books.
- ✅ **Contour and Project** (M2.2e): the around-the-model families -
      Contour shrinks the terrain's CENTER-SHADOW (every shallow
      facet's XY triangle translated by `r·m_xy`: the set of XY points
      where a ball can rest tangent AT SOME HEIGHT) one stepover per
      pass until it collapses at its medial axis; Project marches
      only that shadow's boundary - the model's silhouette, XY kept,
      Z lifted by the march's own tangent-and-clear law. A closed
      loop marched on unbroken ground COMES BACK CLOSED: the clamp
      sample at arc length L lands on the entry point, and the
      duplicate law keeps it as the return move, not a stutter. Both
      reuse `march_curve` verbatim (two truths, one code), close on
      the ring ladder like every family, and enter the v16 grammar
      as append-only ordinals 6 and 7. The stepover COST question
      (the ring-cap guard) speaks only where spacing marches -
      Project's one rim pays nothing for it - while `validate`'s
      early gate still refuses a numberless spacing from every
      pattern, the Waterline precedent intact.
- ✅ **Ramp entries** for the raster families (M2.2e): the diagonal
      chain law - with `ramp` on, each between-lines link is
      certified by sampling the straight cut against the terrain's
      own support (`support_at`, within tolerance); where it
      certifies, the hop's up-rapid-plunge collapses to one Feed,
      where it doesn't - sunk chords on domes, open air, spans past
      the 4096-probe cap - the hop stands. First contacts and the
      ring families never route through the chain, and `ramp: false`
      (every pre-v17 file) is byte-identical to the old behavior.
- ✅ **Surface finish parameters** (M2.2a–E): tolerance, stepover,
      minimum step, ramp angle - all four now live and all four
      MEASURED, not assumed. `tolerance` is the deviation the path
      promises and the envelope keeps (A); `stepover` the cusp
      between parallel lines, exact under the scallop law (B);
      `min_step` a floor under the march's refinement - the chord
      halving halts AT it while the run edges stay the material's
      own (E); `ramp_angle` the slope the diagonal chain may
      descend, 45 deg by default - v17's implicit gift spelled, so
      every pre-v18 path is byte-identical (E). Wire v18; the
      corrugated belt and the tilted wedge are the witnesses.
- ✅ **Tolerance verification** (M2.2a): the deviation is MEASURED, not
      assumed - `stock_sim::envelope_scan`'s two truths against the
      swept ball: gouge as dexel deviation (every ball's lowest point
      over a column vs the mesh's own surface) and rest as distance at
      RIDGE columns (two band contacts inside `r + tol + cell²/2R`; a
      lone graze or an untouched column was never promised anything by
      this pass - law, not a silent zero), gated at
      `tol + cell²/2R`. `Mode::Envelope` speaks it in the verifier;
      a scan that cannot measure reports a gouge at the first cut,
      never silence (color map + post gate remain M2.3's).

### M2.3 - Verification (Release 0.2.0)
**Goal: the toolpath is trusted before the machine sees it.** — ✅

- ✅ **Material simulation** (M2.3 A-B): the machined block maintained as an
      adaptive voxel / dexel stock grid, subtracted per cutting move, with
      color-mapped deviation against the final model. The program chains
      the CACHED ops in posting order (the sim's gate is the post's gate,
      `posting::program_ops`, verbatim); the lattice shifts with the
      stock's placement, never the moves, and a rotated stock is refused
      instead of resampled. The viewer paints the judged sheet in the
      stock's slot: steel clean, amber rest, red gouge, gray never-judged.
- ✅ **Gouge and collision checking** (M2.3 A-B, vs fixtures deferred to
      fixture modeling in M3.2): tool vs stock judged with LOCATIONS pinpointed - the
      worst rest and gouge cells carry XY and the sim's own z, ringed in
      the viewport and one click (Frame rest / Frame gouge) flies the
      camera there. The per-op verifier's holder/collar bands stand
      beside this program-level truth; fixture-aware checking waits for
      fixture modeling (M3.2). (done since: M3.2 B - fixture solids,
      live-gate collision law)
- ✅ **Stock comparison** (M2.3 A-B): color map of over/under-cut against
      the final geometry - the deviation field IS the program's contract
      (sim top minus model surface per column), aggregated into the one
      sentence the verdict bar shows: counts, mm³, worst depth, WHERE.
      Within DEV_TOL (10 um, a display law, not an op's authoring
      tolerance) reads clean; NaN reads unjudged, never a false zero.
- ✅ **Machine limits** (M2.3 C): axis travel, feed, and spindle validation
      from a machine definition - a `.mch` file mounted as the document's
      setup furniture (the LoadMachine command; v18→v19 persistence, an
      unstated machine writes no key). An unstated limit checks nothing:
      the zeros files carry for rates nobody configured are not a refusal
      to let the machine rapids. Stroke is our own `"travel"` extension on
      linear parts, read in PROGRAM coordinates through the post's own
      origin law, so a G54 slide shifts what passes exactly as it shifts
      the G-code; feeds bill against the slowest stated axis they drive,
      and rapids pay the stroke bill too. `check_document` walks
      `posting::program_ops` - the judgment is over exactly what would
      post. Stage C reports (the Deviate clause, located lines capped at
      40 + an honest more); stage D makes it block.
- ✅ **Post gating** (M2.3 D): the verification result IS the gate that
      releases the NC program (Never Gouge, made structural). At post
      time `verification_gate` re-runs the two program-level judgments
      Deviate informs with - gouge cells and stated machine-limit
      violations refuse the write entirely (file and viewer untouched).
      Rest material is not unsafe (a roughing program posts on its
      promise of a finish op - Deviate shows the shortfall), and a sim
      that cannot run judges nothing rather than vetoing everything -
      the per-op WARN gate remains the net under it. Evidence is
      computed now, never carried in from the viewer: the PostView law
      applied to a gate. All three judgments walk
      `posting::program_ops`, so *what is judged is what would post*
      holds by construction.

### M2.4 - 2D Completeness
**Goal: every 2D name in the catalog exists.** — ✅

- **2D Adaptive roughing**: the M2.1 engagement discipline brought to the
      plane - trochoidal step-in and constant-load passes specced from
      published roughing literature, rest judged by the M2.3 dexel sim
- **2D Offset pocket roughing** (style-A): concentric offsets from the
      boundary inward; islands and tool-radius clearance by the same
      computation M2.1 uses for its staircase. (The 3D offset-pocket ⚠️
      once rode this bullet - it is not a 2D name, so the goal line
      cannot carry it; its promise moved to M2.5, row 78 retagged.)
- **Engrave**, **2D Chamfer** (chamfer mill along selected edges),
      **Circular** (circular pocket / vault), **Bore** as a standalone op
      beside M1.6's boring cycles
- **Thread (2D)**: helical thread milling - the move chain is 3-axis
      interpolation; the spindle-sync word the post needs rides M3.3's
      machine definitions (a C-axis-less machine gets the honest refusal
      M1.6 established for tap without sync)

### M2.5 - 3D Strategy Completeness
**Goal: 3-axis finishing answers every surface type.** — ✅

- **Steep-and-shallow**: flat regions to the parallel/waterline law, steep
      to scallop - M2.2's 45-deg auto-split becomes the partition, not
      just a scallop switch; carries M2.2's open surface-finish parameters
- **Flat-area finishing** and **Pencil**: the floors, and the corner locus
      a big ball can never reach, found by the hidden-rest machinery
- **Ramp**, **Morphed spiral**, **Geodesic**: sloped paths and two new
      placements on M2.2's polar law
- **Blend / Morph / Corner / Project-generalization**, **Flow** (flowline
      on freeform; ruled-surface flow waits for M5.1's axis control -
      done since: M5.1 D swarf off the ruling fit),
      **Deburr** (edge-break toolpaths; pairs with M2.4's chamfer)
- **3D Offset pocket roughing**: the catalog ⚠️ M2.4 retired to here -
      roughing by OFFSET SHELLS of the model (the boundary eroded in
      the surface's own normals, waterlined where the shell turns
      steep), not the level ladder's XY rings; the M2.3 dexel sim
      judges the rest, as ever
- **Reference revisit — the placements the papers challenge** (⚠️ reopens
      done features for review, not for repair): M2.1d's feed modulation,
      M2.2's parallel/scallop and polar-spiral placements and the ring-tour
      join law were verified before the `pdf-sources` references (Phase 2
      note) entered the graph. Re-run the efficiency comparison against
      them on the standard finishing fixtures — PFD region segmentation vs.
      the 45-deg auto-split, double-spiral domain-mapped tours vs. the
      spiral placement — and let measured wins (shorter travel, fewer
      retractions, tolerance held) re-strategize the done paths; absent a
      win, the current strategies stand re-confirmed.
      **Verdict (2026-11-05): done, and nothing re-strategized.** The PFD
      paper's 1.9-8.2% wins are measured against iso-scallop seeds on
      free-form PFD-topology surfaces; on its own no-degenerate-point cone
      the field reduces to circumferential iso-scallop — our ring ladder's
      family — and `SPLIT_MZ = cos 45°` is the scallop law's own
      jurisdiction limit, which the paper never crosses. The double-spiral
      paper's 25.9%/16.4% wins are against zig-zag/contour-parallel on a
      fish pocket; on our revolution fixtures the map degenerates to
      concentric circles, its corner scallop violations are what our
      envelope lane would veto, and its mesh-native cousin already shipped
      as Geodesic (M2.5 C). The hop-vs-smooth-link choice rides the
      recorded tour-closure law. M2.1d's feed modulation is unchallenged
      by either paper; the NIST doc is the RS-274 language, not a
      strategy. Full evidence with page cites: RECOMMENDED.md.

---

## Archived phases (the ledgers moved to DONE.md)

The phases below passed every gate; their prose lives in `DONE.md`
(AGENTS.md "Done ledger" law), their status rows stay in the registry
table at the foot of this file, and their decisions in `RECOMMENDED.md`.

- **Phase 3 — Associative CAM & Production Workflow** (M3.1-M3.3, Release 0.3.0) — ✅ (M3.3's owed power/speed check landed with M13.2, 2026-11-06)
- **Phase 4 - Refactoring and reuseability.** (M4.1-M4.4, M4.2b) — ✅
- **Phase 5 — Power: Multi-Axis, Turning, Scale** (M5.0-M5.3, Release 0.4.0) — ✅ (M5.2 owed items reopened 2026-11-05 -> M13.5)
- **Phase 6 — UI refactoring.** (M6.1-M6.2) — ✅
- **Phase 8 - Cutting order: the generator review & refactor.** (M8.1-M8.3, Release 0.5.0) — ✅ (M8.1 "Adaptive earns its name" reopened 2026-11-05 -> M11.4 C)
- **Phase 9 - The workpane: every operation opens its own page.** (M9.1, Release 0.6.0) - ✅
- **Phase 10 — The tool law: an operation opens only where a tool can cut it** (M10.1) — ✅

---

## Phase 7 — Implicit preview: the SDF stock engine.

The M1.7 open question - "adaptive dexel vs. sparse voxel octree vs.
signed distance field" - was settled for dexel because the work of
Phases 1-2 was 2.5D and 3-axis Z-plunge cutting, where a column of
material under the spindle is exactly the right grid. The engine's own
header admits the premise (`stock_sim/dexel.rs`: "a Z-plunge world
whose tools never machine under an overhang... undercuts would need a
voxel octree"), and Phases 5-6 break it: the rotary families machine
radially into an axis that may lie anywhere in space, M5.2's lathe work
is a solid of revolution whose "top" is its radius, and simultaneous
5-axis cuts sweep in every direction a heightfield cannot read without
stair-steps. So the simulation's preview is revisited: not a finer
voxel grid, but the voxel preview engine replaced by an **implicit
Signed Distance Function** - the machined volume IS the stock's
distance field MINUS the distance field of the tool's swept envelope,
and it RENDERS as that function: sphere-traced in the 3D view,
resolution-independent, exact corners where voxels round them off. The
mesh stays the source of truth - the verification-layer law survives
this rewrite untouched; the field is preview and verdict display, never
geometry.

> Reference on the graph: the video "Making a game engine with signed
> distance fields" (https://www.youtube.com/watch?v=il-TXbn5iMA, creator
> "Mike") was transcribed to `raw/yt_5c0a63a714c5.transcript.txt` and
> extracted into the knowledge graph (public reference material; nothing
> from HSMWorks). It validates this phase's architecture from an adjacent
> domain - a world entirely represented as an editable SDF, rendered fast
> by caching distances on a grid, stored sparse in 8x8x8 brick maps,
> level-of-detail'd with nested clip maps, and kept live by BVH-tracked
> incremental brick regeneration: the same four pillars M7.1 proposes,
> proven at interactive rates on dynamic geometry.

### M7.1 - The field and the subtraction kernel
**Goal: a signed-distance representation of remaining material behind
the existing `stock_sim` seam, where subtracting a tool sweep is one
`max()` per sample and every "what is left here" query in the codebase
answers with a real distance, not a column top.**

- **The stock field**: a closed triangle mesh → a narrow-band SDF grid.
  Generation from the boundary by feature-extrusion scan conversion
  (each face/edge/vertex extrudes along its own normal region, nearest
  distance wins - the characteristic/scan-conversion method, GPU
  single-pass T4 variant), or by the multi-source BFS distance
  transform (Barilla's - simple, linear, CPU-honest). The sign is
  ray-parity against the watertight shell (the M2.2 parity law already
  in this codebase). The band truncates a few cells wide - a level-set
  field throughout; nothing is stored far from the surface.
- **Sparse by brick, not octree**: the grid is fixed-size bricks (8³ or
  16³ cells) under a flat brick map; only near-surface bricks are
  allocated (a part's band is ~1-5% of the nominal grid), dirty ranges
  come from a sweep's AABB, and the shallow structure keeps GPU cache
  coherence and update speed - deep octrees only pay off in static
  scenes. Same conclusion the streaming/voxel-hashing line reached
  independently: hash or brick, don't pointer-chase.
- **The swept-tool law**: every CL move's envelope has a closed-form
  distance IF the move is a segment or arc and the tool is a profile of
  revolution - and that is every move the generators emit. Evaluate
  in the TOOL's frame, never carry the tool's geometry to the world
  (the inverted-trajectory law: the sample point rides the inverse
  pose, one 3x3 + translation, and the axis stamp every CL move has
  carried since stage A IS that pose): ball nose sweep over a segment
  = capsule (point-to-segment distance minus R, exact); flat endmill =
  swept capped cylinder; bull-nose = swept rounded-rect profile;
  taper = frustum; a lathe or rotary cut = torus/annular-sector
  sweeps - all closed forms from the published primitive-distance
  catalogue. A general profile is the min of its few profile primitives
  (a revolution profile is a stack of capsules and frusta); general
  motion of general geometry falls back to the sampled-pose minimum
  (the swept-volume SDF construction) - DISPLAY ONLY, never a verdict
  source: pose-sampling of a true field is not conservative, and this
  codebase does not ship non-conservative answers to safety questions.
- **The subtraction law**: φ_new(p) = max(φ_old(p), −χ_sweep(p)) -
  difference is max-with-negation, exact outside and slightly
  underestimated in the pseudo-SDF regions CSG and sweeps create.
  The direction is chosen ON PURPOSE: an underestimated distance means
  clearance queries round toward danger, which rest/gouge verdicts read
  as over-reported material and never a hidden gouge - the dexel
  header's "over-reporting rest is honest noise" law, promoted to a
  field. Where sphere tracing wants the gradient honest, re-normalize
  the band with one eikonal sweep before the render pass (fast, local).
  Sequential cuts accumulate as a running minimum over the sweep
  fields - order-independent and seam-free by construction (the
  one-authoritative-distance law of the distance-accumulating digital
  twins).
- **Same API as today**: construct from (mesh, stock box); `subtract`
  per CL move; `query(p) → signed distance`; `measure/deviate/envelope`
  equivalents - so the gouge check, the deviation map, rest material
  (M3.1 D) and `machining3d::material_above`'s sim twin all get real
  distances at their existing call sites. Drill cycles pose nothing
  and stop, and a stopped tool is a solid of revolution at a station -
  a field subtracts them exactly where the dexel cannot.

### M7.2 - The preview render - seeing the machined volume
**Goal: the 3D view draws the implicit field directly. The viewport
sphere-traces stock-minus-tool-sweeps each frame; a marching-cubes mesh
becomes a derivation for consumers that need triangles, not the thing
being previewed.**

- **The viewport pass**: a wgpu compute shader writes a depth+material
  image the existing egui-wgpu stack (M1.4.2's graduated pipeline, no
  new render engine) composites beside the model and tool chrome.
  Per-pixel sphere tracing (Hart's law: step by the returned distance
  - the nearest surface can be no closer): start at the stock AABB
  entry via the brick map's HDDA walk so empty space costs a table
  lookup, not a sample; ~64 steps; surface normal = the field's own
  gradient (the distance field's gradient IS the normal - shading is
  free); cheap AO by one secondary march. The machined look is the
  feature: the scallop steps, the radii, the shine on the finished
  floor, with no voxel staircase in sight.
- **Progressive playback**: the sim scrub becomes "subtract this move,
  re-render" - playback is the field's truth unfolding. Backward seek
  re-applies from a checkpoint; dirty-brick snapshots every N moves are
  the undo journal, and the brick is its natural unit.
- **The deviation color map, continuous**: sample the TARGET part's
  field at each hit point - the signed value IS the deviation, exact
  and located wherever the eye lands. Replaces the dexel column-top
  differencing (M2.3) with the same located-not-just-counted verdict
  law, NaN where either body is absent, never a zero dressed as a
  clean bill of health.
- **A mesh when asked**: iso-surface extraction (marching cubes; dual
  contouring where sharp interior corners must survive) regenerates
  triangles for the G-code viewer's stock display, picking, and STL
  export of "what the program leaves". The law: the mesh is a cache of
  the field, never a second truth (rebuild on any cut appended, keyed
  like the cached toolpaths are).
- **CPU fallback, because "never crash" includes "never black-screen"**:
  the same field ray-marched at low resolution and upsampled, or
  extract-and-shade through the existing mesh path - every machine that
  runs the app today runs the preview; the GPU path is a speedup, never
  a dependency.

### M7.3 - Retirement and reconciliation
**Goal: the SDF engine answers verdict-for-verdict like the dexel on
every golden, then the voxel preview engine retires.**

- **Both backends side by side behind the trait** until agreement: the
  heavy `--ignored` lane runs the same programs through both engines
  and demands rest/gouge/deviation verdicts agree within the field's
  declared epsilon - a disagreement is a bug in the new engine, never
  a rounding to accept (the same law the M4.2b dome goldens taught).
  The 10M-triangle M5.3 fixtures measure the claim; they don't assume it.
- **Cost law**: an update touches only the sweep's dirty bricks - work
  per program is proportional to material removed, not to grid size ×
  move count; moves batch per frame so a 100k-move program never
  re-throots the same brick a thousand times.
- **The retirement**: `stock_sim`'s column engine goes when its last
  consumer (the feed-engagement sampler included) proves verdict-
  identical - one clean commit, goldens byte-comparable where the
  verdicts are, and the Open-questions note below closed with a link.

Provenance: composite adaptively-sampled distance fields for NC milling
simulation (MERL TR2012-025 - the swept-tool Boolean difference, the
inverted-trajectory evaluation, the octree-BVH localization); GPU SDF
subtractive ops with real-time marching-cubes iso-surfaces for
material-removal digital twins (Purdue MMRL-CMS 2026 - ~105 FPS, ~35x
mesh booleans); distance-accumulating scalar voxel twins (arXiv
2608.29955); narrow-band generation by characteristic scan conversion
(GPU Gems 3 ch. 34) and BFS distance transforms (arXiv 1807.07111);
sphere tracing (Hart 1996, the ray-marching teaching literature);
closed-form primitive and swept-primitive distances
(iquilezles.org/distfunctions); swept-volume SDF for arbitrary motion
(libigl); brick-map sparsity for real-time fields (voxel hashing,
NVIDIA sparse distance fields). Everything composed from these public
sources; all Rust/wgpu/WGSL code here is ours; no HSMWorks or vendor
material of any kind.

## Phase 11 — Smooth transitions: the arc move at the turns

Every turn in a path is a stop today: a facing row reverses on a sharp 180°
corner, a between-pass link jogs at an angle, an entry drops straight down.
Each is a direction discontinuity the machine must decelerate into, and the
surface shows it. The moves to spend are arcs — RS-274 / ISO 6983 G2/G3
circular interpolation, which the post has emitted since M1.7 — applied to
the transitions themselves: tangential blends at turning points, and
circular (arc) entries that curve the cutter down to the material instead
of a straight plunge. Specced from the public G-code standard and general
machining knowledge (path filleting, circular-ramp entry), clean-room.

### M11.1 - Arc blends and arc entries
**Goal: the tool changes direction and enters material continuously - turns ride G2/G3, not G1 stops.** — ✅ (line-to-arc and arc-to-arc corner fillets landed 2026-11-06; a closed path's seam corner stays sharp by decision - RECOMMENDED "M11.1: line-to-arc" (3))

- **Turning-point blends**: direction changes in feed motion - the facing
  serpentine's end reversals (the named example), contour corners,
  between-pass links - carry a fitted tangential arc (a fillet of the path
  at the corner) emitted as one G2/G3 move, not a chord fan and not a G1
  stop. The blend radius is bounded by tool radius, the op tolerance and
  live geometry; whatever rest a rounding leaves, the sim reports (Prime
  Directive: measured, never silent), and where no legal arc exists the
  sharp corner stands, named. The generators emit the arc move; the post
  needs no new word (M1.7's G2/G3 covers it).
- **Arc entry**: the M2.1 entry family (plunge / ramp / helix) gains the
  circular ramp - the cutter reaches the material along an arc in a
  vertical plane, engaging tangentially instead of dropping a straight
  line - dive-point legality proved by the same laws the ramp and helix
  obey, with the same honest fallback when nothing can be proved.

### M11.2 - Plane-aware transitions: arcs off the work plane, keep-down links
**Goal: moving from one cutting plane to the next is planned motion the user
chooses - true G18/G19 arcs where engagement eases in, helix/ramp/plunge/arc
between levels on every multi-level operation, and keep-down links as a
definable strategy, never an unasked trip to the safe plane.** — ✅

(User review, 2026-11-05: "the path generation doesn't seem to understand that
arcs when entering material can be in the G19/G18 planes rather than the G17 ...
when linking between the layers, why would we need to go to safe distance unless
we choose to? keepdown is a common strategy we should honor and make definable.")

- **A - the plane in the CL and the posts**: the CL arc carries its plane
      (XY / XZ / YZ - RS-274's G17/G18/G19, ISO 6983), the five built-in posts
      state the plane word modally and emit the arc in it (each control's own
      dialect), conformance goldens per post; every existing CL and NC golden
      stays byte-identical (an XY arc states nothing new).
- **B - arcs off the work plane where they belong**: the vertical-plane arc
      entry (M11.1 B) and vertical lead-in/out arcs emit as single G18/G19
      arcs instead of chord fans; a lead may engage in the vertical plane
      (the arc rolls the tool onto the material) as well as in XY.
- **C - level transitions, one family, every multi-level op**: the descent
      between levels - plunge / ramp (angle) / helix (radius, pitch) / arc
      (vertical-plane, radius) - is a user parameter on every level-ladder
      operation (contour, pocket, slot, adaptive, offset, circular, bore,
      3D roughing), legality proved by the existing ramp/helix/lead-room laws,
      fallback named in the notes.
- **D - keep-down links, definable**: between levels and between passes in a
      level, the link is the user's: retract to clearance / retract to the
      retract plane / keep down (with a maximum stay-down distance and the
      link proved clear of the part, else it lifts - named). Contour's
      `level_link` (2026-11-05) is the first instance; D generalises it.
- **Done (2026-11-06)**: A (91907b4), C+D (e09e90d), B and the C+D
      blind-floor sliver (0d86d50). Verdicts, not owed: an oblique entry
      chord stays a chord fan with one note (no standard plane), and
      Heidenhain posts vertical arcs as 1 um chord fans (RECOMMENDED
      "M11.2 A" (5), "3D ladder blind floor + M11.2 B" (3)).

### M11.3 - Lead in / lead out: the full parameter set, and heights from the selection
**Goal: every entry, lead and engagement choice a CAM user expects is a
parameter on the operation, grouped under its own "Lead in / Lead out"
section, and every height can be measured from the operation's own
selection.** — ⚠️ partial (owed: the 3D ladder's and contour bore side's second dives, Rotary Parallel radial infeeds and Swarf first entry can't ramp - see the Owed bullet below)

(User review, 2026-11-05: "All heights should have the option Selection ...
Entry - Arc should have an option of setting radius and length of the arc.
Entry - Ramp lacks all usual settings, like angle or length of ramp. All entry
methods are missing extension values. These should be under its own section ...
called Lead in / Lead out as is common in most CAM packages. We are also missing
plunge data or for that matter how much the lead in / lead-out strategies should
handle engagement.")

- **A - heights from the selection**: Clearance, Retract, Top and Bottom each
      gain a "Selection" reference (the op's picked geometry - its top / bottom
      / the picked face's height), beside Stock / Model / Work offset /
      Absolute, resolved by the same height law and shown with its resolved Z.
- **B - entry parameters, complete**: Ramp - angle, maximum ramp length,
      ramp-in-to-depth vs per-level; Helix - radius, pitch (or angle),
      direction, continuous helical G2/G3 (a hand-test found stepped flat
      arcs - fixed under M11.2); Arc - radius and sweep length (arc angle),
      plane (vertical / horizontal); Plunge - plunge height above the
      material, plunge feed.
- **C - extensions and lead geometry**: every lead-in / lead-out carries
      its extension values (linear extension before/after the arc, vertical
      lead height, overlap past the start), tangential / perpendicular /
      arc / linear lead shapes with their sizes.
- **D - engagement and feed control at entry**: entry/ramp/plunge feeds,
      lead-in feed reduction, and an engagement limit the entry honours
      (the M2.1 engagement law applied to the entry and the lead) - all named
      parameters with defaults that reproduce today's output.
- **The section**: entry, leads, transitions and links live under one
      "Lead in / Lead out" tab on every kind that has them (moved out of
      Passes); defaults reproduce every existing golden byte for byte.
- **Landed (2026-11-06)**: A (5b487cb), B-D and the section (874507a).
- **Owed** (RECOMMENDED "M11.3 A" (5), "M11.3 B-D" (4)/(8), "M11.3
      owed" (3)-(5)): [the horizontal-plane arc entry for pockets and the 3D first-stretch
      slowing - DONE 2026-11-06, RECOMMENDED "M11.3: a horizontal arc entry for
      pockets" / "M11.3: 3D ladders slow only the first diameter"]; the 3D ladder's and the contour bore side's named
      second dives; the centre-cutting plunge check in finishing3d,
      drilling and face - DONE 2026-11-06 (RECOMMENDED "M11.3: the centre-cutting
      law in finish, drilling and face"); Face entry fallback and 5-axis posed guards also DONE
      2026-11-06; the 3-axis law uniform with the posed 30° cone also DONE 2026-11-06;
      the post-stamp audit for Multi-Axis Rough / Rotary Offset
      and an app-level 5-axis test also DONE 2026-11-06; the 3D ladder's and contour bore's second dives also DONE 2026-11-06
      (one_descent.rs: 1 descent/level on 48 configs); still owed: the bore's LINEAR
      lead (start 4.5 µm outside the ring) keeps a named second dive; Rotary
      Parallel's radial infeeds and Swarf's first raster entry always plunge (the
      ramp retry can't help); Rotary Offset's true cylinder solid. Done and dropped from this list:
      between-pass centre-cutting checks (`plunge_law`, 515525c),
      wall-start zigzag/slot second dives (none exist, test-pinned),
      Selection heights in the clearance notes ("Small owed UI items" (2)).

### M11.4 - One region whatever was picked; adaptive by HSM principles
**Goal: the operation's region is what the selection encloses - a picked face
and its picked boundary edges give the same region and the same toolpath - and
adaptive clearing is planned by high-speed-machining principles, specified the
way machinists specify it.** — ⚠️ partial (C rounds 2-5 landed; owed: slot entry-orbit kinks (ratcheted), island late returns, arc output, Pocket3D port - see RECOMMENDED "M11.4 C round 5")

(User review, 2026-11-05: "there is a misconception that there should be
different path generations depending on if it's a pocket's edge chosen or a
face.. Pocket should most def. be inside the selection, but just because it's a
round hole doesn't mean it needs to be circular paths when choosing adaptive ...
there is no path optimization in accordance to the high speed machining
principles. Adaptive has engagement based on percentage of the cutter - not a
degree.")

- **A - selection-agnostic regions**: face picks, edge/loop picks and model
      sources resolve through one region law to "the area the selection
      encloses" (pocket family: inside it); the strategy the user chose runs on
      that region - never a different generator because of the pick type or the
      region's shape (a round region under Adaptive is cleared adaptively, not
      as forced concentric circles). Tests: face vs edge-loop picks of the same
      pocket give byte-identical paths.
- **B - adaptive engagement as machinists set it**: optimal load as a
      percentage of the tool diameter (and/or a distance), the engagement angle
      derived from it internally; old files' degree value converted on load in
      memory (no data loss, the saved word kept readable).
- **C - HSM path quality** (also closes the reopened M2.1 trochoidal/corner-contact items and M2.4's 2D Adaptive spec): constant radial engagement held along the path
      (the M2.1 engagement-step law), curvature-continuous motion - no sharp
      direction changes in cut (arc blends / smoothing at every corner of the
      adaptive path), trochoidal motion where a slot-width channel would
      overload the tool, links that stay down and smooth (M11.2 D), minimum
      air-cutting; measured, not asserted: max engagement, max curvature and
      air-time reported per path and gated by tests.
- **Landed (2026-11-05)**: A+B (f3b3a2c).
- **Owed - C, round 2** (RECOMMENDED "Merge batch 2026-11-06"): round 1
      was held back, nothing of C is merged. Round 2 must pass the batch
      gate's two acceptance tests (no self-crossing on the 60x40
      Adaptive pocket; 2 descents, not 4, on the user's bore) and the
      unmet targets (peak load at the 14.6% target, not 19.9%/26.7%; no
      small kinks; air not 70x up); L-pocket, island and slot regions
      still fall back to rings by name; the Pocket3D port of the same
      planner. Also open for the user (RECOMMENDED "M11.4 A+B" (6)): a
      whole-model Pocket source silently clears the part's own section.

### M11.5 - Area control: the selection is where, the boundary settings are how far
**Goal: every operation's affected area is user-controlled - the selection
names the general area, and containment, offset, extension and extra
boundaries decide exactly where the tool may go.** — ⚠️ partial (owed: open boundaries on Contour, boundaries on 3+2-indexed ops, the group on the remaining area kinds)

(User review, 2026-11-05: "Facing operation stays strictly within selection,
and there is no ability to extend it outside, or expand its area in any way.
Which makes no sense. We should have control over the area an operation
affects, selection is just determining the general area. This is a general
problem with the operations, there is lack of control on area affected.")

- **A - tool containment**: per operation, the tool inside the boundary /
      tool centre on the boundary / tool outside (past) the boundary - the
      region law of M11.4 A applies it once, every strategy inherits it.
- **B - boundary offset and extension**: grow or shrink the area by a
      distance (with corner style); extend past open/exterior edges by a
      distance (facing clears off the part's edge: default for Face is past
      the stock outline by a stated amount, so the existing facing goldens
      stay byte-identical only if the default reproduces today - else the
      change is named and the goldens re-blessed with reason).
- **C - additional boundaries**: extra include / exclude regions (picked
      faces, sketches/loops, the stock outline, the silhouette of the part),
      combined by the same 2D boolean law (clipper2).
- **E - open boundaries**: a boundary may be an open chain (picked edges /
      a sketch curve that does not close). The machined area is the side of
      it the user picks (machine inside / outside - i.e. which side of the
      open curve), closed off by the stock outline: the path starts outside
      the stock (in air, the facing-style approach) and works in up to the
      open boundary. (User review, 2026-11-05: "We should also be able to
      choose open boundaries. then the path should expand to outside of the
      stock as a starting point and work up to the boundary. But there
      should also be options to choose inside or outside of boundary to
      machine.") Closed boundaries keep the same inside / outside choice.
- **D - every op, one place**: the controls live in a "Geometry" /
      "Boundary" group on every kind with an area (2D and 3D families), with
      defaults that reproduce today's output; legality unchanged - an area
      that would put the tool in the part still meets the depth-aware verify
      and the post gate.
- **Landed (2026-11-06)**: A-E (42d9f71), boundary picking rework
      (257da38).
- **Owed** (RECOMMENDED "M11.5" (1)/(5)): an open boundary on Contour,
      and any boundary on a 3+2-indexed op, refuse by name "for now";
      the controls are valid on Face, Contour, Pocket, Adaptive2D,
      Offset2D and Pocket3D only - every other kind with an area refuses
      non-defaults, so D's "every op" is not yet met.

## Phase 12 — Geometry kernel: Solidcore

The kernel decision open since M1.3 ("benchmark truck vs an OpenCascade
binding") has a third answer: [tangency-labs/solidcore](https://github.com/tangency-labs/solidcore) -
"A Rust port of NVIDIA's SMLib B-rep kernel (CAD subset): safe, deterministic,
wasm-compatible" (Apache-2.0; the upstream it ports is NVIDIA's open
usd-brep release, and the port is "not affiliated with or endorsed by
NVIDIA"). A Rust-native kernel from an open source replaces the deferred
modeling engine with no C++ toolchain risk and no proprietary internals kept
at arm's length - the clean-room cuts toward reading SMLib, not away from it.

### M12.1 - Solidcore replaces the B-rep engine
**Goal: one Rust-native kernel behind the geometry - booleans, offsets and fillets stop being deferred kernel territory or mesh workarounds.** — 📋

- **The swap**: `solidcore` enters the workspace as the modeling kernel
      (topology, booleans, offset/shell work, fillet/chamfer, intersections,
      healing), and the M1.3 ⏸ territory it unlocks - face/island
      recognition from a real B-rep, boolean stock ops, B-rep editing -
      becomes buildable. Truck does not exit wholesale: STEP→mesh import
      (M1.2) stays on truck-stepio until a ported translator exists -
      solidcore's scope is the kernel, upstream translators explicitly
      outside it.
- **The retirement gate**: the port is experimental (0.0.x, "partial units
      and known mismatches; integration incomplete"), so it replaces truck
      only after proving against the laws already in this codebase - a
      dual-kernel agreement bench (truck vs solidcore verdicts on the
      boolean/offset fixtures; a disagreement is a bug filed against the
      port, never a rounding to accept - the M4.2b dome-golden law), the
      corrupt-input corpus passing unchanged (the kernel's own "no panics
      on bad input" goal restates the Prime Directive), and the heavy lane.
- **Stays buildable everywhere**: the kernel builds for
      `wasm32-unknown-unknown`, so M4.2's wasm-compile CI job extends to
      it, and "no `unsafe` in the kernel" means no new FFI surface
      anywhere.
- **Provenance**: an Apache-2.0 port of a publicly released kernel -
      license-clean to link; the notice rides `assets/ui/LICENSES.md`
      (the clipper2-rust precedent).

## Phase 13 — Owed from done milestones

The 2026-11-05 roadmap audit found promises made inside milestones whose
rows read ✅ that no open milestone owned - written into a done
milestone's prose, then left there when the row closed. This phase is
their tracked home: nothing here is new scope, it is the debt. Each
bullet quotes the original promise and names where it was made; the
source prose stays where it is, and the source rows now read ⚠️ with a
pointer here (or to M7.3 / M11.4 C, where an open milestone already owns
the gap).

### M13.1 - Stock engine completeness
**Goal: the stock field cuts every tool the library holds and serves
every consumer Phase 7 promised it to.** — ⚠️ partial (owed: lathe/rotary annular-sector sweeps, eikonal re-normalization, the mesh for viewer/picking, dual contouring, the field answering an over-budget hidden-rest scan)

- **Bull-nose and lathe/rotary swept solids** (M7.1, "bull-nose = swept
      rounded-rect profile; taper = frustum; a lathe or rotary cut =
      torus/annular-sector sweeps"): today any tool that is not ball or
      cone cuts the field as Flat (`crates/sim/src/stock_sim/field_solid.rs`,
      `backend.rs:259-266`, `display.rs:52-57`) - a bull-nose's corner
      radius is removed in the field where the real tool leaves it
- **The eikonal band re-normalization** (M7.1, "re-normalize the band
      with one eikonal sweep before the render pass (fast, local)"):
      handed to M7.2 for the render, not built there
- **The extracted mesh for the viewer and picking** (M7.2, "regenerates
      triangles for the G-code viewer's stock display, picking, and STL
      export"): the `MeshCache` serves STL export only
- **Dual contouring** (M7.2, "dual contouring where sharp interior
      corners must survive"): only the marching-tetrahedra extraction
      shipped
- **The solid hidden-rest scan past its budget** (M2.2, "Over the 1.2
      M-cell / 40 M-claim budgets the solid scan answers with the stated
      zero - the field's debt stays on the books"): the scan still
      returns the stated zero over budget
      (`crates/sim/src/stock_sim/scan.rs:261-264`); the field is to
      answer there instead
- **Landed (2026-11-06, 257da38)**: the exact bull-nose swept solid on
      both engines (agreement suite +4 bull replays, ~1e-11); lathe and
      rotary cuts are refused by name in `plan_program` instead of cut as
      Flat; an over-budget hidden-rest scan reads `RestUnjudged` ("NOT
      JUDGED") and blocks the post instead of the stated zero.
- **Owed** (RECOMMENDED "Merge batch 2026-11-06", M13.1 (3)): the
      lathe/rotary torus and annular-sector sweeps; the eikonal band
      re-normalization; the extracted mesh for the viewer's stock display
      and picking; dual contouring; the field answering past the
      hidden-rest budget (today an honest blocking refusal, not an
      answer).

- **Traced playback leftovers** (RECOMMENDED "Traced playback off the UI
      thread"): dirty-brick GPU upload DONE 2026-11-06 (2.02 MB -> 57 KB per
      picture, RECOMMENDED "M13.1: playback uploads only the dirty bricks"); jump throughput (a full-length jump 14.8 s vs 12.4 s before the
      worker), and the CPU-fallback eye still drawing on the UI thread.

### M13.2 - Machine limits and cutting data
**Goal: a program is judged against the machine's spindle and the
material, not only its travel and feed.** — ✅

- **Spindle speed and power checks** (M3.3 C, "spindle speed and power
      limits (the ceilings the tool library's per-material speeds and
      feeds are checked against)"): the machine definition stores
      `spindle_power` but checks nothing
      (`crates/post/src/machine/def.rs:107-112` - "checking it would
      compare against nothing")
- **The homegrown materials database** (Open question "Cutting data":
      "grow a homegrown materials/tools database under an open license,
      seeded from public vendor catalogs"): the per-material speeds &
      feeds library the checks above read
- **Done (2026-11-06, 257e166)**: spindle power joins the machine-check
      gate (claims only when everything it reads is stated) with rpm/feed
      checks now test-proven; a homegrown six-material table, document
      material, tool material + flutes (edited from the Feeds tab's
      "Cutting data" group), suggestions applied as one undo step. Not
      owed: spindle efficiency is a fixed 0.8 - a `.mch` key is a
      possible follow-on, not a promise (RECOMMENDED "M13.2" (3)).

### M13.3 - Builds and distribution
**Goal: every target builds and ships the way M1.7 and M4.2 said it
would.** — 📋

- **Code signing and the installer-vs-zip decision** (M1.7, "Unsigned -
      no code-signing certificate is owned yet; the installer (vs the
      zip) is a post-0.1.0 discussion")
- **Docker builds for every target** (M4.2, "Docker: Using docker builds
      for every target" - left open in the row as tooling death)
- **The app/UI on wasm** (M4.2, "The app/UI-on-wasm story is the
      web-sys-mvp rung"): only geometry/cl/post compile for wasm32 today
- **A headless binary per crate** (Phase 4 intro, "Each crate should
      produce a standalone executable that can be headless tested"):
      today only app (`camcraft-cli`), post and ui carry binaries

- ~~**Themes resolve from the build machine's source tree**~~ - fixed 2026-11-06
      (`ui/theme_dir.rs`: env, beside the exe, repo layout, manifest last; RECOMMENDED
      "M13.3: themes resolved beside the exe").
- **`crates/app/src/operation/material.rs` over the 400-line law** (720 lines):
      split `region_of` / `region_turn_of` into their own module.

### M13.4 - Small owed polish
**Goal: the named cheap follow-ons stop being named and get done.** — ✅ (vertex picking, Set origin on a vertex or face, dialog polish - all done 2026-11-06)

- **ColorVision remaps** (M6.1, "Their `ColorVision` layer
      (protan/deutan/tritan remaps applied on top of any theme) is a
      cheap follow-on once the token table exists"): the token table
      exists; the remaps do not
- **The tool-law refusal suggests a starter-catalog entry** (M10.1,
      "Stretch, deferred: the refusal naming a starter-catalog entry")
- **Vertex picking and WCS face/vertex snapping** (M1.5, "vertex picking
      deferred: ... it lands with the features that ask for it (hole
      centers, M1.6; WCS snap, below)"; M1.2, "True face/vertex snapping
      waits on M1.5's vertex pick mode"): only M1.5's ⏸ held it, and
      M1.5 schedules nothing
- **Landed (2026-11-06, 42d9f71)**: the catalog hint in the tool-law
      refusal; the protan/deutan/tritan remaps (Machado 2009, CIEDE2000
      >= 20 gate, Off pixel-identical).
- **Owed** (RECOMMENDED "M13.4" (4), "M13.4: vertex picking and Set origin"):
      vertex picking and Set origin DONE 2026-11-06; snapping the WCS origin to a
      picked face DONE 2026-11-06 (RECOMMENDED "M13.4: snap the work origin to a picked face").
- ~~**Operation dialog polish**~~ - done 2026-11-06 (RECOMMENDED "Operation dialog polish").
- **Continuous C-axis live-tooling operations** (M5.2, "Mill-turn: live
      tooling operations indexed and continuous (C-axis)"): only the
      indexed half shipped
- **Axial drilling** (M5.2, "Turning holes: radial and axial drilling
      through live tooling"): only indexed radial drilling shipped (F2)

### M13.6 - Feature recognition beyond holes
**Goal: pockets and slots are recognized, not only holes.** — 📋

- **Pocket/slot recognition** (Open question "Feature recognition":
      "automatic hole/pocket/slot recognition beyond M1.5 selection and
      M1.6 basics"; M12.1, "face/island recognition from a real B-rep ...
      becomes buildable"): M12.1 makes it buildable but schedules
      nothing; this milestone is where it is built, on the Solidcore
      B-rep once M12.1 lands

### M13.7 - Verifier completeness
**Goal: the exact solid-law verifier judges every move with the real
tool, against the real stock, in the real pose - the backstop every
generator relies on has no known blind spot.** — ⚠️ partial (owed: mid-move pose)

The 2026-11-06 solid-law verifier (0e18091) replaced plane sections for
the tool-vs-part check; its own entry names what it still does not
judge exactly (RECOMMENDED "The exact solid-law verifier" (4)/(8)).
None of these may stay open behind a ✅.

- ~~**The tip shape for 2D ops**~~ - done 2026-11-06 (RECOMMENDED "M13.7 A"): `VerifyContext` carried no `Cutter`, so
      2D ops are checked with the square stack (may over-report a ball or
      bull, never hide); drill, bore and thread already use their real
      tips. Carry `Cutter` into `VerifyContext`.
- ~~**Uncut stock under overhangs**~~ - done 2026-11-06 (RECOMMENDED "M13.7: stock
      under overhangs"); owed from it: judge each move against the cuts BEFORE it
      (the column model ignores cut order and can under-report) - DONE 2026-11-06 (cut_index.rs).
- **Mid-move pose changes**: a move whose pose changes along it is
      judged at its end pose only; judge the swept pose.
- ~~**Rotary ops can't post**~~ - resolved 2026-11-06: a pivot bug
      (posed ball tools posted the centre, not the tip) plus vertical
      judgement of wrapped paths; fixed by the pivot law, posed rapids
      and `Mode::Rotary` radial verify (RECOMMENDED "Rotary and 5-axis:
      the pivot law"). Leftovers live in M13.8.
- ~~**Posed moves through motion's direct `Dexel::subtract`**~~ - done 2026-11-06 (`subtract_upright` refuses by name): the column
      engine refuses posed moves by name at the stock-sim door, but
      motion's direct `Dexel::subtract` calls are not gated for posed
      moves - gate them (refuse by name) or route them through the door.
- ~~**`sim/verify/engine.rs` over the 400-line law**~~ - done 2026-11-06:
      split into crossing/rest/section/reach/rapids/finish (RECOMMENDED "M13.7 B" (1)).
- ~~**An app-level real-tip Contour test**~~ - done with a bull nose (ball
      ends are refused for 2D ops at creation) - `real_tip_contour.rs`.
- ~~**Law (b) leave sign**~~ - fixed 2026-11-06: a positive leave hid a
      gouge of that depth (RECOMMENDED "M13.7 B" (3)).
- ~~**Ladder vs verifier tolerance**~~ - fixed 2026-11-06: ladder slack 0.008 mm
      (RECOMMENDED "Ladder tolerance and the all-air finish" (1)).
- ~~**swarf_multi coarse rung**~~ - fixed 2026-11-06: rungs relax only
      their spacing (RECOMMENDED "M13.8 B" (1)).
- ~~**The rotary first move**~~ - done 2026-11-06 (RECOMMENDED "M13.8 A"
      (1)): opens on the clearance pose above the first cut.
- ~~**An upright all-air finish**~~ - done 2026-11-06: proven all-air finishes get
      a non-blocking "cuts no material" note.
- ~~**The ridge rest law**~~ - decided 2026-11-06: proven dead, kept as a
      reading; the reachable law moves to M13.9 with the generator fixes.
- ~~**Rotary Offset** from the stock radius; **Rotary Parallel** radial
      retracts~~ - done 2026-11-06 (RECOMMENDED "M13.8 A" (2)/(3)).
- ~~**Rotary Offset air laps**~~ - fixed 2026-11-06: 11,967 -> 7,082 moves
      on a d20 bar (RECOMMENDED "M13.8 B" (2)).
- ~~**Radial retracts vs fixtures**~~ - fixed 2026-11-06: posed moves sweep the whole tool
      against fixtures (RECOMMENDED "M13.8: posed moves against the fixtures"); owed: spindle
      on posed rapids, continuous pose, fixed (non-rotating) fixtures in the path frame.


### M13.9 - The scallop law and finish stepovers
**Goal: a finish whose scallop exceeds its tolerance is named, never
verified silently - and the shipped finish generators meet their own
tolerance.** — 📋

The ridge rest law can never convict (RECOMMENDED "M13.8 B" (3)). A
reachable version (saved as ridge-flank-law.patch) convicts 9-12
shipped finish fixtures at 0.13-0.35 mm on shallow ground (dome
waterline, geodesic dome, multidirection raster, dome flow). Fix
those generators' stepover/scallop control, then land the law, with
the heavy lane green.

| Milestone | Goal | Key Features | Status |
|-----------|------|--------------|--------|
| **M1.1** | Model in, project saved, never crash | Mesh import, save/load, undo/redo, robustness, document units (IGES deferred) | ⚠️ |
| **M1.2** | Define a job | Shell, dockable viewport, STEP import, stock + WCS setup, toolpath display | ✅ |
| **M1.3** | Cut a 2.5D plate | Face, contour, pocket, slot, trace, tool library, parameters, verification warnings, tangential leads; G41/G42 output, spiral/helical entries open | ⚠️ |
| **M1.4** | The workflow on the surface | Ribbon with icons, browser tree, contextual Operations pane, no silent refusals, what-gets-cut preview; display modes, the tabbed-panels overhaul and the graphical tool editor window all done - riding the graduated egui stack | ⚠️ |
| **M1.4.1** | The look and feel, fixed at the root | Dear ImGui → egui: decisions recorded (egui adopted, clay considered and rejected, no dual-stack), spike done - GO (`docs/spike-egui-go-no-go.md`) | ✅ |
| **M1.4.2** | egui is the shell | Per-panel graduation M-a…M-g: ribbon/browser, property pages (tabbed), dialogs + file ops, viewport interaction, shared actions law; dear-imgui deps and the imgui surfaces retired, Tabler glyphs as egui textures | ✅ |
| **M1.4.3** | A home for every operation | Family-sectioned ribbon per the Operation Catalog, overflow dropdowns, user pinning (a document setting); the home law was overtaken by M5.1's ten ribbon-less kinds - the promise moved to M6.2, which landed it | ✅ |
| **M1.5** | Pick a face, not just a model | Face/edge clustering, face/edge and welded-patch pick modes, selection-driven contour and facing (open runs extend to stock), arc moves, staged facing, retract hops, readable selection rows, honest height planes, From/Offset height references, bore-aware contour side (Auto/Inside/Outside), translucent cut-region fills, folded collision warnings, v5→v7 persistence, view cube + nav bar; vertex picking and WCS snapping done via M13.4 (2026-11-06) | ⚠️ |
| **M1.6** | Machine the hole list | Spot/drill/peck/deep/tap/bore cycles, hole recognition into editable params, canned-cycle representation beside the move chain; thread milling and the post itself open (done since: thread milling M2.4 thread_mill.rs, the post M1.7) | ✅ |
| **M1.7** | Verified first output | Move simulation, validation, CPS post engine (`.cps`/`.mch`/our intermediate) + the Fanuc post as its first script, G-code viewer, CI matrix + portable-zip release, 0.1.0 | ✅ |
| **M2.1** | Rough 3D at constant load | Done: 3D Pocket, rest material and adaptive clearing (level law, computed islands, entries; dexel sim, half-step closing ladder, corner law; engagement-step law, ring join law, closure-as-data) and feed modulation (median-anchor law, two-station chord) - REOPENED 2026-11-05 (user review): trochoidal slot paths and the corner-contact/trochoidal fallbacks were left "open" in the prose while the row read done; owed under M11.4 C | ⚠️ |
| **M2.2** | Finish 3D to tolerance | Waterline, the tolerance-verification kernel, the parallel raster and scallop, the polar placements (radial, spiral), the solid-truth hidden rest and the center-shadow families (contour, project) done (tangent-locus rings and Z-follow lines, two-truth envelope, measured verdicts, 45-deg auto-split, the diagonal crossing pair, ray-parity material intervals, closed-loop march, the certified diagonal chain between raster lines); the four surface-finish params (tolerance, stepover, min_step, ramp_angle) went live and MEASURED at M2.5 A, which closes this row's last open clause | ✅ |
| **M2.3** | Trust before cutting | Done: program material sim (chained cached ops), deviation color map with located verdicts, machine-definition checker (.mch travel/feed/spindle), verification made the post gate, 0.2.0 | ✅ |
| **M2.4** | 2D, complete | Done: 2D adaptive + offset pocket roughing, engrave, 2D chamfer, circular, standalone bore, helical thread milling (row 78's offset-shell roughing rides on to M2.5 F - done since: M2.5, 3D Offset pocket roughing) - REOPENED 2026-11-05 (user review + M11.4 measurement): 2D Adaptive was specced as trochoidal step-in at constant load but shipped as concentric offset rings stepped by the engagement law (a round pocket gets circles); the real constant-engagement/trochoidal adaptive is owed under M11.4 C | ⚠️ |
| **M2.5** | 3-axis, complete | Done: steep-and-shallow, flat-area, pencil, ramp, morphed spiral, geodesic, blend/morph/corner, flow (freeform), deburr, and 3D Offset pocket roughing (the shell); M2.2's surface-finish params; reference revisit vs. the `pdf-sources` papers (PFD field, double spiral) on the M2.1d/M2.2 placements closed - both placements stand re-confirmed on the literature | ✅ |
| **M3.1** | Model changes, toolpaths update | Done: stable feature identity + `UpdateModel` revision (A), stamp inputs - tool/WCS/FromModel stock propagate, staleness rides the copy (B), live-question `stale_reason` + one-undo-unit `RegenerateAll` (C), re-MACHining - rest masks from predecessors' recorded paths, converges in two rounds (D) | ✅ |
| **M3.2** | Production setups | Done: multi-setup with implicit-setup law + datum transfer (A); fixtures - user STEP solids wearing a marker, spindle/table bands from the `.mch`, live-gate collision law (B); per-setup tool list in change order (C); setup sheets, one form-fed page per setup, no timestamps (D); workspaces by machine kind, axis-inventory inference (E) | ✅ |
| **M3.3** | Every control covered | Done: post template engine - mounted `.mch` shapes output, `SelectPost` rides the undo history, user `.cps` refuse by name (A); five built-in posts - Fanuc, Haas, 840D, Heidenhain, Mazak - by-name registry, conformance goldens (B); machine definitions - declared kind gates the op families, full numbers mirrored, coolant M-codes, changer capacity, draft-law editor (C); RTCP - G43.4 delegates pivots to the controller, rotaries mirror as geometry, angle words modal both ways; block-by-block post trace with derived selection (D); 0.3.0; spindle power/speed checks landed with M13.2 (257e166) | ✅ |
| **M4.1** | Code refactor | Done: six subcrates on the seam map (geometry R1, cl+tools R2, post R3, sim+motion R4 with the stock_sim/fixture knot dissolved); app's operation and project monoliths split along section seams (R5); tests moved crate-by-crate, coverage law held 785==785 at every rung; CI + release lanes run per crate (M4.1) | ✅ |
| **M4.2** | Multibuilds | Done: GPU backend pick - `--gpu-backend`/env, refuse-by-name, preferred→auto→all honest chain, adapter log line; Linux native check + suites (18/20 goldens; 2 dome goldens drift on Linux libm - named skip, M4.2b fix owed, RECOMMENDED); macOS + wasm-compile CI jobs (geometry/cl/post green on wasm32); Docker-for-every-target left open (tooling death, RECOMMENDED) | ✅ |
| **M4.3** | Defer Code | Done: hsmworks→camcraft everywhere user-facing or self-referential - crates/packages, `camcraft-ui.exe`, `.camproj` (+filter+untitled name), window title, cps vendor line, dist zip, thread name, doc titles; hash salts moved (one-time Stale, by design), wire magics kept (format = compatibility contract), Autodesk-trademark true-sentences kept; full suite green on the new names (595 pass / 792 `#[test]` law held / clippy -D warnings), goldens byte-stable, post `--version` intact | ✅ |
| **M4.4** | Test cost shape | Done: law/verdict split - finish3d 39 tests (27 `#[ignore]`d), verify's two envelope verdicts moved; release heavy lane 30 min → 3.4 min (CI runs it); Gate-map headers in every suite; debug ladder ~2 s where it was a 30-min tail | ✅ |
| **M4.2b** | Dome-golden fix | Done: the drift was never the sampler's rates - it was float-bit ties in the facet scan's argmax at mathematically-exact tangencies, resolved differently by each platform's libm. Fix: scan points born on the micron lattice, support compared on the lattice, convex hull in exact i64/i128 integers, arc splits `(lattice(q)+1e-12).ceil()`. Windows goldens regenerated once (7 files, coarser by 8-24 moves; envelope + verify lanes prove no tolerance broke); both named CI `--skip`s retired; Linux parity 20/20 (RECOMMENDED: law override) | ✅ |
| **M5.0** | Structure prelude | Done: the kind-fan-out collapse (M5.0.2 `KindInfo::ALL` registry - labels, captions, digest tags, verify modes, retype seeds in one table; three silent UI catch-alls removed) + seven oversized-file splits (operation/types, machining2d, finishing3d, verify, stock_sim, drilling, machining3d) + M4.2b paid; lets M5.1/M5.2 add ~25 op kinds at one-table-row cost. Goldens byte-identical (structure, not behavior). Coverage 793==793, 29 ignores kept. See docs/M5-PHASE5-PLAN.md. | ✅ |
| **M5.1** | Five axes | Done: the pose seam - `Move.axis` rides every move, prismatic byte-for-byte (A); 3+2 tilted-plane indexing on the five plane kinds (B); lead/lag/max-tilt over the waterline and raster ladders (C); swarf and advanced swarf off the ruling fit (D); rotary contour/parallel on the fitted axis (E), rotary offset roughing in developed wrap (E2), multi-axis roughing (E3); collision-aware shank dodge (F); kinematic machine sim with the pose-grid sweep law (G); flowline finish that curves with the part (I); drive-curve multi-axis contour (J); the envelope yardstick measures arcs at the generator's own tol/4 chord law, every family verdict green (close-out) | ✅ |
| **M5.2** | Lathes too | Done: the lathe branch substrate - X/Z axis model, stock as revolved solid, blade tool profiles (A); facing, profile rough/finish (B); grooving, parting, the revolved blade verdict (C); threading on the wave verdict (D); chamfer, taper, multi-pass ladders (E), form turning (E2); boring on the inside-out revolve (F), radial drilling + hole recognition on turned solids (F2); part orientation and the bar feeder's load/unload intents (G/G.2); mill-turn offers both families through the machine-kind gate; tagged `m5.2-done` (RECOMMENDED: the tag and the vehicle); owed: continuous C-axis live tooling and axial drilling (-> M13.5) | ⚠️ |
| **M5.3** | Shop scale | Done: the entity-scoped digest memo - recompute only what moved (A); rayon batch generation, threaded-equals-serial bit-for-bit (B); part transfer intents + spindle inventory (C); in-process probe asks + composed deviation verdict (D); 72-byte shared mesh, streaming STL import at 30M facets, tiled-sparse stock grid (E/E1/E2); the boa job-script host - `cam.*` verbs, TS bindings generated from the verb table (F); `camcraft-cli` + the golden-model numerical regression suite in CI (G); released as 0.4.0 | ✅ |
| **M6.1** | Token architecture | Adopt the solidcraft `Tokens`/`theme_files` design: one named table drives widgets + painted chrome, radii/spacing/sizes themeable, command-law switcher. Stages A+B+C landed: the table, derived visuals, the exhaustiveness guard, geometry clamps; the `camcraft-theme/1` reader - caps, named refusals, discovery, parse cache; the switcher as a command (`ThemeRef`/`SetTheme`, `Tokens::LIGHT`, setup-page picker, frame-edge sync, v41 persistence) - and the NewDocument singleton-leak fix it found; D - the contrast module (WCAG ratios, the follow law for unspelled accent states, on-fill ink that caught the hand-typed white's 3.83:1 active-state bug, the readability gate that refuses an unreadable file by name) and LIGHT's golden through the switcher's own door, the three snapshot tests under one lock. All seven bullets landed; the ColorVision remaps remain the named cheap follow-on, deferred | ✅ |
| **M6.2** | A home for every operation, kept | M1.4.3's debt, paid: the registry's Section column (Family derived, one truth), `shown_for` the single machine filter - hiding not darkening; the fold, then re-folded: first width-driven (40px floor, fattest group's tail behind a "{Section}..." door, the dropdown reaches every kind, global-rect law the first click-through proved), then structural at the user's verdict - a crowd that only folds when squeezed is still a crowd: every family behind one labeled door at any width, at most 4 pins on the bar, a pinned kind leaves its door (one home per kind); the machine door (Setup group) switches Milling/Turning/Mill-turning by mounting that kind's reference machine - a LoadBuiltinMachine command, so the switch rides undo; pins as a document setting outside the history (RibbonPins, v42, absent = unpinned; undo replays cuts, not layout); the combo grouped by the same taxonomy | ✅ |
| **M7.1** | Stock becomes a field | Narrow-band brick SDF from the mesh; closed-form swept-tool subtraction (`max` with the tool-frame distance, inverted-trajectory law); same stock_sim API, real distances at every verdict call site; owed: rest material still reads the column engine (-> M7.3), bull-nose and lathe/rotary sweeps cut as Flat (-> M13.1) | ⚠️ |
| **M7.2** | The machined volume in the 3D view | Done: sphere-traced viewport render on the egui-wgpu stack - compute march + composite with sorted depth (A/B), progressive per-move playback with checkpointed backward seek (C), continuous deviation classes from the target's field per pixel, marching-tetrahedra mesh as a cache keyed on the field view + STL export of what the program leaves (D), CPU fallback at thumbnail resolution; the status-line verdict stays on the column engine until M7.3; owed: the mesh never reached the viewer's stock display or picking, no dual contouring, no eikonal re-normalization (-> M13.1) | ⚠️ |
| **M7.3** | Dexel retirement | Both engines verdict-identical on the heavy lane (rest/gouge/deviation within declared epsilon), per-brick update cost law, column engine retires | 📋 |
| **M8.1** | Roughing entry & order | Level-0 obeys the entry family (true helix/ramp into metal, no silent plunge fallback), roughing pass order reaches the middle off the wall, `Adaptive` trochoid-or-renamed, substitutions diagnosed; owed: "Adaptive earns its name" was not taken - still rings, neither trochoid nor renamed (-> M11.4 C) | ⚠️ |
| **M8.2** | Finishing & planar order | Done: contour Z-entry on the entry family, certified against the destination bore air, wall-side opt-in refused by note (A); waterline tangent-ring entry re-proved on steep parts, ramped first contact opt-in, normals measured not assumed (B); slot ramp/helix certified on the convex cutter-center stadium (`Cert`), full-width substitution says its name, tabbed levels stand the arms down on the pre-A byte, tall-tab hop raise, trace plunge theorem (C); M1.3's spiral/helical debt swept; heavy lane green | ✅ |
| **M8.3** | Canned/posed/revolved order | Done: drilling - R-plane law kept per cycle, the bore's fit door refuses an oversize tool and a stepped (counterbore-folded) hole by name, Spot dims off the mouth, FIT_TOL unified as one trust wearing three laws; helical bore entries declined as a verdict (a wide slide is contour's verb); multi-axis - a dive's lead now rides the cut it lands into (the stamp matches curve.rs's first station; ALONG→AT corrected in the archived prose), cone-dodge-at-entry, wrap radial dives and tilt-posed rapids recorded as verdicts with their backstop chains; turning - order law stated in the catalog rows, the verifier's stands-downs documented, no entry-defect analog (ridge, kerf-slide and rho_clear laws refuse or prevent by name); whole registry annotated, heavy lane green on every family | ✅ |
| **M9.1** | The operation opens its own page | Done: the retype dropdown and its dialog retired - the ribbon creates or opens by kind, the tree selection opens the same pane in place (A); the six tabs folded into one stacked pane of collapsing sections per the Blender verdict, `op_tab` retired, the drift guard restated exact (B); the pane law walks all 30 registry kinds - one header per group, fields exactly once, catching the Passes header/field collision, renamed Pass count (C); toolpath goldens byte-identical, snapshots re-blessed; heavy lane green; ledger in RECOMMENDED.md | ✅ |
| **M10.1** | A tool that can cut the operation | Done: the tool law - `KindInfo.tool_kinds` one table (A), `CreateOperation` / tool reassignment / op retype / tool retype refuse the mismatch at command construction by name and the ribbon darkens with the reason (B), an op whose tool is gone or wrong reads BLOCKED - generation and the post refuse, nothing stored, old files load untouched (C), the law tests (D); generator shape refusals stand as the backstop | ✅ |
| **M11.1** | Smooth transitions | Done: facing reversals (3a73bdf), contour corners and pocket links (ad91fd6) as single tangent G2/G3 fillets, opt-in; circular-ramp arc entry (6d22f07, later true G18/G19 via M11.2 B); line-to-arc and arc-to-arc corner fillets (M11.1 RECOMMENDED); a closed path's seam corner stays sharp by decision | ✅ |
| **M11.2** | Plane-aware transitions | CL arc plane + G17/G18/G19 in every post; vertical-plane entry/lead arcs as true arcs; plunge/ramp/helix/arc level transitions on every multi-level op; keep-down links with a stay-down limit, all user-chosen; done 2026-11-06 (axis-aligned entries post G18/G19, oblique stays chords by verdict) | ✅ |
| **M11.3** | Lead in / lead out, complete | Heights from the selection; ramp angle/length, continuous helix radius/pitch, arc radius/sweep, plunge height/feed; lead extensions and shapes; entry feeds and an engagement limit; one Lead in / Lead out tab; pocket G17 arc entry, 3D first-stretch slowing, 3D ladder / bore second dives done 2026-11-06; owed: the bore's linear-lead second dive, Rotary Parallel infeed / Swarf first-entry ramps (-> M11.3 Owed bullet) | ⚠️ |
| **M11.4** | Pick-agnostic regions, HSM adaptive | Face / edge / model picks resolve to one enclosed region and the chosen strategy; adaptive optimal load as % of tool diameter; constant engagement, curvature-continuous HSM motion, trochoids in overloaded channels, measured per path; A+B done; C rounds 2-5 done (spiral, crossing bucketing, microlift returns + hop budget: slot 0.9 m air, island named ring fallback); star-plan seams at 0 kinks past 5° (done); owed: the slot's entry-orbit 2 breaks, island legal late returns (round 6 finding: the 9 hops are the west-wall corner pieces k=585..631 whose leads read 16.5-18 deg vs the 15 deg bite on the RE-ORDERED stock - next: re-mark late pieces' leads with out.ras.walk_marks at schedule time and trim them to where they read <= bite, so the graze joins the cut and high() takes them), arc output for <20k moves, Pocket3D port (-> M11.4 Owed bullet) | ⚠️ |
| **M11.5** | Area control | Tool containment (inside / on / outside the boundary), boundary offset and edge extension, extra include/exclude boundaries, one boundary group on every op with an area; open-boundary Contour band/extensions done; owed: boundaries on 3+2-indexed ops, the group on kinds beyond Face/Contour/Pocket/Adaptive2D/Offset2D/Pocket3D (-> M11.5 Owed bullet) | ⚠️ |
| **M12.1** | Geometry kernel | Replace the current B-rep engine with Solidcore (Rust port of NVIDIA's SMLib kernel - Apache-2.0, wasm-ready, no-unsafe): booleans/offsets/fillets become kernel-driven; STEP IO rides truck until a ported translator; the dual-kernel agreement bench + corrupt-input corpus are truck's retirement gate | 📋 |
| **M13.1** | Stock engine completeness | Bull-nose and lathe/rotary swept solids, eikonal band re-normalization (M7.1); the field mesh for the viewer's stock display and picking, dual contouring (M7.2); the solid hidden-rest scan past its budget (M2.2); bull-nose done, over-budget scan blocks as NOT JUDGED; owed: lathe/rotary annular-sector sweeps (refused by name meanwhile), eikonal, viewer/picking mesh, dual contouring, the field answering past the budget (-> M13.1 Owed bullet) | ⚠️ |
| **M13.2** | Machine limits and cutting data | Spindle speed/power checks against a per-material speeds & feeds library (M3.3 C); the homegrown materials database (Open question "Cutting data"); done 2026-11-06 (fixed 0.8 spindle efficiency is a follow-on, not owed) | ✅ |
| **M13.3** | Builds and distribution | Code signing + installer-vs-zip (M1.7); Docker builds for every target, app/UI on wasm (M4.2); a headless binary per crate (Phase 4 intro) | 📋 |
| **M13.4** | Small owed polish | ColorVision protan/deutan/tritan remaps (M6.1); the tool-law refusal suggesting a starter-catalog entry (M10.1); vertex picking + WCS face/vertex snapping (M1.2/M1.5); all done: remaps, catalog hint, text colours, dialog polish, vertex picking, Set origin on a vertex or face | ✅ |
| **M13.5** | Mill-turn completeness | Continuous C-axis live-tooling operations and axial drilling (M5.2) | 📋 |
| **M13.6** | Feature recognition beyond holes | Pocket/slot recognition on the B-rep (Open question; buildable after M12.1) | 📋 |
| **M13.7** | Verifier completeness | The solid-law verifier's named limits: `Cutter` tip shape in `VerifyContext` for 2D ops (done); shank/sweep/rapid-in-stock laws off plain sections (uncut stock under overhangs); mid-move pose changes judged along the move, not at the end pose; rotary ops postable (done, b09e885); motion's direct `Dexel::subtract` gated for posed moves (done); engine.rs split, law (b) leave sign (done); ladder tolerance (done); overhang laws (done); cut-order clearing (done); owed: mid-move pose | ⚠️ |
| **M13.8** | Multi-axis leftovers | Done: rotary first move, Offset from stock radius, Parallel radial links, swarf_multi rung, Offset air laps; ridge law decided (-> M13.9); all-air finish noted, posed moves vs fixtures; spindle on posed rapids, fixed fixtures in the machine frame; generate-time check on the workholding laws, Fixed toggle; owed: continuous pose, the setup sheet printing the flag | ⚠️ |
| **M13.9** | Scallop law and finish stepovers | Make the ridge/scallop law reachable and fix the finish generators it convicts (0.13-0.35 mm over a 0.12 gate) | 📋 |

## Release train

A release is a cut, not a goal (user, 2026-11-06: "make sure to hit the marks
so we don't spend ages between stable test releases"). Two trains a week on
fixed dates; the scope is what is green at the cut. Each train carries at most
three MUSTS; a must that is not green 4 h before the cut slips to the next
train, never the date. The last 4 h are the freeze: no new agents, the full
gate (every crate, ui from crates/ui, clippy, all heavy lanes), bump, lock
sync, tag, zip - the 0.7.0 recipe in RECOMMENDED "Release 0.7.0". Nothing is
pushed by the session; the user pushes the tag.

| Train | Cut (local) | Theme | Musts | Nice-to-have |
|---|---|---|---|---|
| **0.8.0** | Mon 2026-11-10 12:00 | Every 0.7.0 known limit closed; the zip runs on any machine | themes resolved beside the exe (M13.3); M11.3 ✅; M13.7 ✅ (mid-move pose decided) | M11.4 island on HSM + slot entry kinks; M13.8 ✅; material.rs split; `scripts/release.ps1` (one command: gate → bump → zip) |
| **0.9.0** | Thu 2026-11-13 12:00 | Area control and stock everywhere | M11.5 ✅ (3+2 boundaries, group on every kind); M13.1 lathe/rotary swept solids; M13.9 scallop law live | viewer/picking mesh, jump throughput, CPU-fallback eye (M13.1); M7.3 retirement if unblocked |
| **0.10.0** | Mon 2026-11-17 12:00 | Install it, turn it | M13.3 installer-vs-zip decision + signed build path; M13.5 C-axis live tooling; M11.4 arc output + Pocket3D port | M1.x ⚠️ leftovers audit; M13.6 feature-recognition spike |

The table is the authority: when a train cuts, its row gets the tag and the
commit, and the next three rows are planned at the same time, so the train
never runs out of dates.

## Open questions & deferred

- **M4.2b (resolved)**: the dome-golden drift was NOT the sampler's
  sin/cos rates reaching the cull threshold - ring dumps proved the two
  platforms emit different VERTEX COUNTS for the same facet ring (222 vs
  220), an argmax tie in the scan-point search at horizontal-edge
  facets, where vertex and edge support are exactly equal in math and
  only float bits choose. Fixed by construction: lattice-quantized scan
  points + exact-integer hull. The owed "regenerate nothing" clause
  proved impossible - the old Windows goldens encoded Windows' tie-bits
  at those exact ties - so seven goldens were regenerated, coarser by
  8-24 moves, with the heavy envelope/verify lanes as the tolerance
  proof. See RECOMMENDED.md.
- **Geometry kernel** (answered - replaced, not benched): [tangency-labs/solidcore](https://github.com/tangency-labs/solidcore)
  (Apache-2.0), a Rust/wasm port of NVIDIA's SMLib B-rep kernel, replaces
  truck as the modeling engine - the decision and gates are Phase 12 /
  M12.1, not a truck-vs-OpenCascade bench any more. Until that swap lands,
  truck (ricosjp, also Apache-2.0) still carries STEP→mesh import (M1.2),
  and its boolean/offset behavior is the reference the dual-kernel
  agreement bench measures against
- **Post scripting runtime** (answered - boa): which embedded language (QuickJS / Lua / Rust-native DSL) -
  decide before M3.3; must be sandboxed like other scripts. Answered: the boa JS engine hosts
  `.cps` posts (M3.3) and job scripts (M5.3 F)
- **Simulation stock representation** (answered, revisited by design): decided dexel before M1.7 - right for the Z-plunge work of
  Phases 1-2 - and the multi-axis phases broke the column model's premise. Phase 7 answers the question the way the parenthetical
  always suspected: an implicit SDF engine replaces the voxel/dexel preview, keeps the exact remaining-material queries gouge
  checks need, and renders the machined volume directly. The grid models the *machined block for simulation only* - operation
  geometry and tolerance checks still evaluate against B-rep/mesh; that law is Phase 7's load-bearing premise, not its casualty
- **Cutting data** (claimed by M13.2): no licensed handbook is affordable long-term - grow a homegrown
  materials/tools database under an open license, seeded from public vendor catalogs
- **Patent check (FTO)** (answered - waived): high-efficiency milling and some 5-axis orientation techniques
  were patented in the 2000s-2010s; run a freedom-to-operate review before M2.1 and M5.1
  to confirm the literature we're implementing from is unencumbered. Answered: waived by
  the user 2026-10-19 (RECOMMENDED.md, 'Freedom-to-operate review WAIVED', line ~1410)
- **Probing / workpiece verification** (answered): deferred until post Phase 2 (which probing protocol
  sets first - RS238 or Renishaw-style macros?). Answered by M5.3 D: the PROBE request on the
  stream, emitted per post through its own `onProbe`
- **Feature recognition** (claimed by M13.6): automatic hole/pocket/slot recognition beyond M1.5 selection
  and M1.6 basics - deferred; M3.1 associativity will need some of it anyway
- **Machine simulation kinematics fidelity**: full controller emulation is out of scope;
  we simulate geometry and limits, not the control loop
- **SolidWorks link**: HSMWorks was a SolidWorks add-in; this implementation is a
  standalone application - add-in/host integration deferred indefinitely (and any
  file-format interop done from published formats only)
- **Naming** (answered - camcraft, M4.3): the project currently uses the HSMWorks name descriptively; pick our own
  product name before 0.1.0 to stay clear of trademark confusion (clean-room clears
  copyright, not brands)

## Updating this roadmap

Maintenance rules:

- Update whenever a feature changes status - a milestone moves to ✅ only when all its
  features are done, their tests are green, and the milestone's provenance review passed
- Record decisions in the relevant milestone, not just chat
- Prune completed rows to links + one-liners after each release
- Move a phase's prose to `DONE.md` when its table rows are all ✅ (AGENTS.md
  "Done ledger" law) - the registry row and the pointer section stay here
- New features must be specced from public documentation or published literature - a
  feature motivated by inspecting HSMWorks internals is not adopted (Clean-Room Protocol)
