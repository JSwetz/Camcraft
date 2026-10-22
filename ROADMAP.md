# ROADMAP.md

> _Last updated: 2026-10-17_

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

## Operation Catalog — the source of truth

The complete list of operations the product must offer, grouped the way the
ribbon shows them (M1.4.3). Status is what shipped by 0.2.0; Home is the
milestone that owns each name. A new operation is added here first — if it
isn't on this list it isn't on the roadmap, and if it is, it has a ribbon
home.

### Milling — 2D

| Operation | Status | Home |
|-----------|--------|------|
| Face | ✅ | M1.3 |
| 2D Contour | ✅ | M1.3, M1.5 |
| Slot | ✅ | M1.3 |
| Trace | ✅ | M1.3 |
| 2D Pocket (zig / winding) | ✅ | M1.3 |
| 2D Adaptive roughing | 📋 | M2.4 |
| 2D Offset pocket roughing | 📋 | M2.4 |
| Engrave | 📋 | M2.4 |
| 2D Chamfer | 📋 | M2.4 |
| Circular (pocket / vault) | 📋 | M2.4 |
| Thread (helical milling; tapping is M1.6) | 📋 | M2.4 |
| Bore (standalone op; the cycle is M1.6) | ⚠️ cycle only | M2.4 |

### Milling — 3D

| Operation | Status | Home |
|-----------|--------|------|
| 3D Adaptive roughing | ✅ | M2.1 |
| 3D Offset pocket roughing | ⚠️ level staircase today, not offsets | M2.4 |
| 3D Contour | ✅ | M2.2 |
| Horizontal (waterline) | ✅ | M2.2 |
| Parallel | ✅ | M2.2 |
| Scallop | ✅ | M2.2 |
| Spiral | ✅ | M2.2 |
| Radial | ✅ | M2.2 |
| Project | ✅ | M2.2 |
| Steep and shallow | 📋 | M2.5 |
| Flat (flat-area finishing) | 📋 | M2.5 |
| Pencil | 📋 | M2.5 |
| Ramp | 📋 | M2.5 |
| Morphed spiral | 📋 | M2.5 |
| Geodesic | 📋 | M2.5 |
| Blend | 📋 | M2.5 |
| Morph | 📋 | M2.5 |
| Corner | 📋 | M2.5 |
| Flow (flowline; ruled-surface flow waits for M5.1) | 📋 | M2.5 |
| Deburr | 📋 | M2.5 |
| 3+2 roughing (tilted plane, indexed) | 📋 | M5.1 |

### Milling — Drilling

| Operation | Status | Home |
|-----------|--------|------|
| Drill (spot / drill / peck / deep / tap / bore cycles) | ✅ | M1.6 |
| Hole recognition | ✅ | M1.6 |

### Milling — Multi-axis

| Operation | Status | Home |
|-----------|--------|------|
| Rotary offset roughing | 📋 | M5.1 |
| Multi-axis roughing | 📋 | M5.1 |
| Swarf | 📋 | M5.1 |
| Advanced swarf | 📋 | M5.1 |
| Multi-axis contour | 📋 | M5.1 |
| Rotary parallel | 📋 | M5.1 |
| Rotary contour | 📋 | M5.1 |
| Multi-axis finishing | 📋 | M5.1 |

### Turning — Operations

| Operation | Status | Home |
|-----------|--------|------|
| Facing | 📋 | M5.2 |
| Profile roughing | 📋 | M5.2 |
| Profile finishing | 📋 | M5.2 |
| Roughing groove | 📋 | M5.2 |
| Finishing groove | 📋 | M5.2 |
| Adaptive roughing (turning) | 📋 | M5.2 |
| Turning groove | 📋 | M5.2 |
| Turning single groove | 📋 | M5.2 |
| Turning threading | 📋 | M5.2 |
| Turning boring | 📋 | M5.2 |
| Turning parting | 📋 | M5.2 |
| Turning chamfering | 📋 | M5.2 |
| Taper turning | 📋 | M5.2 |
| Form turning | 📋 | M5.2 |
| Multi-pass turning | 📋 | M5.2 |
| Drill (radial/axial, live tooling) | 📋 | M5.2 |
| Hole recognition (on the turned solid) | 📋 | M5.2 |

### Turning — Part handling

| Operation | Status | Home |
|-----------|--------|------|
| Part orientation | 📋 | M3.2 placement flip; full rotation M5.2 |
| Part clamping | ✅ | M3.2 (fixtures) |
| Part loading | 📋 | M5.2 (bar feeder) |
| Part unloading | 📋 | M5.2 (bar feeder) |
| Part transfer | 📋 | M5.3 (automation) |
| Part inspection | 📋 | M5.3 (automation) |

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
**Goal: the whole catalog is reachable without drowning the ribbon.** — 📋

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
- ⚠️ **Pick modes**: whole model, face (raycast through the facet map)
      and edge (nearest feature-edge search; Ctrl/Shift multi-add) -
      **vertex picking deferred**: nothing machines a bare vertex today;
      it lands with the features that ask for it (hole centers, M1.6;
      WCS snap, below)
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
- ⏸ **WCS face/vertex snap**: deferred again, honestly - it needs the
      vertex pick above; M1.2's box-point placement stands

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
      (behavior only) per the Clean-Room Protocol; the engine is our
      own, written from scratch in Rust - three file roles and one
      engine:
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
**Goal: mold-quality surfaces at a stated tolerance.**

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
      fixture modeling (M3.2).
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
**Goal: every 2D name in the catalog exists.** — 📋

- **2D Adaptive roughing**: the M2.1 engagement discipline brought to the
      plane - trochoidal step-in and constant-load passes specced from
      published roughing literature, rest judged by the M2.3 dexel sim
- **2D Offset pocket roughing** (style-A): concentric offsets from the
      boundary inward; islands and tool-radius clearance by the same
      computation M2.1 uses for its staircase, and the 3D offset-pocket
      ⚠️ in the catalog clears here too
- **Engrave**, **2D Chamfer** (chamfer mill along selected edges),
      **Circular** (circular pocket / vault), **Bore** as a standalone op
      beside M1.6's boring cycles
- **Thread (2D)**: helical thread milling - the move chain is 3-axis
      interpolation; the spindle-sync word the post needs rides M3.3's
      machine definitions (a C-axis-less machine gets the honest refusal
      M1.6 established for tap without sync)

### M2.5 - 3D Strategy Completeness
**Goal: 3-axis finishing answers every surface type.** — 📋

- **Steep-and-shallow**: flat regions to the parallel/waterline law, steep
      to scallop - M2.2's 45-deg auto-split becomes the partition, not
      just a scallop switch; carries M2.2's open surface-finish parameters
- **Flat-area finishing** and **Pencil**: the floors, and the corner locus
      a big ball can never reach, found by the hidden-rest machinery
- **Ramp**, **Morphed spiral**, **Geodesic**: sloped paths and two new
      placements on M2.2's polar law
- **Blend / Morph / Corner / Project-generalization**, **Flow** (flowline
      on freeform; ruled-surface flow waits for M5.1's axis control),
      **Deburr** (edge-break toolpaths; pairs with M2.4's chamfer)

---

## Phase 3 — Associative CAM & Production Workflow

The workflow that justified HSMWorks' name: the model changes, and the toolpaths
update. Re-MACHining, multi-setup jobs, posts for the machines people actually
own. The operations catalog completes here too - every 2D/3D name gets a
registry home before Phase 5's multi-axis and turning families arrive.

### M3.1 - Associativity & Regeneration
**Goal: a model revision no longer means rebuilding the job.**

- ✅ **Geometry association**: operations reference model features and curves, not
      stale copies - the law already held structurally (picks are live feature ids,
      re-validated by `SetOperationGeometry`/`valid_pick`, never stored geometry), and
      stage C adds the workflow on top of it: `stale_reason` answers WHY an operation
      reads Stale by asking the world the live questions (vanished tool, deleted
      source, moved pick, unreachable height plane), the properties page prints it,
      and `RegenerateAll` fixes the whole stale job in one undo unit, skip-not-fatal
      for anything whose generator refuses (M3.1 stage C)
- ✅ **Model update detection**: a same-named re-import lands as `UpdateModel` on
      the one entity the name keys (revision counter, undo restores the previous
      geometry exactly); the diff is a set difference over stable content-hashed
      feature ids - faces, welded faces, edge chains - and staleness of dependent
      operations falls out of the stamp's revision signature, so unchanged
      re-imports churn nothing and same-count-different-solid updates cannot pass
      in silence (M3.1 stage A)
- ✅ **Re-MACHining**: automatic rest against the areas previous operations
      machined - `rest::prior_state` replays every predecessor's CACHED moves
      through the stock_sim dexel grid and hands the 3D pocket's level ladder a
      mask that only ever removes cuts (a masked run is a subset of the
      unmasked one, so air-cutting shelves vanish and the islands law still
      guards the part). The predecessor's term in a dependent's stamp is the
      hash of its recorded path, not of its intentions: an edited,
      unregenerated operation moves nobody, and a program converges within two
      `RegenerateAll` rounds. A fully-cut duplicate refuses as Empty with the
      "already machined" word; program order is the post's order - names sorted,
      the same sequence `posting::program_ops` walks (M3.1 stage D)
- ✅ **Parameter propagation**: changed stock, tool, or WCS flows correctly
      through dependents - the tool and height arms already digest live world
      state, and the copy that could go stale was `FromModel` stock:
      `UpdateModel` now re-derives the stock's material inside the same undo
      unit, and the stock stamp digests the copy's facet signature, so a
      source-model update reaches stock-based operations through the metal
      and a stock whose source was deleted stays honestly fresh
      (M3.1 stage B)

### M3.2 - Setups & Documentation
**Goal: multi-setup production jobs with paperwork.**

- ✅ **Multiple setups**: per-setup stock / WCS resolved through each operation's
      declared setup (the implicit-setup law: none == the legacy globals, the first
      CreateSetup materializes and never renames, private furniture is spawned on
      first use); the flip is placement (rotation arrives with M3.3); datum transfer
      clones a setup's stock and placed WCS into its own names, `datum_from` kept as
      provenance; program order is setup rank then name, rest masks same-setup only
      (M3.2 stage A)
- ✅ **Seperate workspaces**: the workspace follows the machine's kind, inferred from
      the `.mch` axis inventory it already declares (X/Y/Z → milling, +rotary →
      mill-turning, X/Z without Y → turning, undecidable → milling - the assumption
      every other line of code already makes); a turning document darkens the milling
      `Create` cells with a tip, because generating an invented turning workspace
      would be the silent wrong answer - the real turning family lands with M5.2,
      and M3.3's "machine declares its kind" replaces this inference with a stated
      field (M3.2 stage E)
- ✅ **Fixture modeling**: a fixture is a model wearing a marker - the user loads the
      shop's real workholding as STEP and marks it for a setup; collision checking
      knows the solids (shank/holder/spindle bands + rapid-air law), the spindle band
      and the table come from the machine file, and nothing about a fixture silently
      changes toolpaths - workholding rides the post's live gate, never the stamp,
      so moved steel blocks the post by name without ever staling a fresh path
      (the M2.3 law, extended; M3.2 stage B)
- ✅ **Setup sheets**: printable text sheets generated from the CAM data itself -
      one page per setup, form feeds between them, everything resolved live (tools,
      offsets, stock, fixtures, the tool list); no timestamps, so the same document
      renders byte-identical (M3.2 stage D)
- ✅ **Tool list**: per-setup inventory in tool-change order with change counts,
      cut/rapid travel and estimated run time - a read of what the spindle will
      actually hold, never an editable list (M3.2 stage C)

### M3.3 - Post System & Machine Library (Release 0.3.0)
**Goal: real G-code for real controls, written by the user, not by us guessing.**

- ✅ **Post template engine**: generalizes M1.7's CPS engine so users own
      their posts in the app (same clean-room line: our format, our
      text, no adaptation of any vendor's post files) - the document's
      mounted `.mch` now shapes the output, a `SelectPost` command picks
      a user `.cps` through the undo history, and a missing or broken
      script refuses by name instead of falling back (M3.3 stage A)
- **Built-in posts**: Fanuc, Haas, Siemens 840D, Heidenhain, Mazak - each with conformance tests against a golden G-code suite
- **Machine definitions**: a machine group in the Setup pane - select or edit a
      machine, its name shown in the setup, driving simulation limits and post
      output. A machine declares its kind - mill, lathe, or mill-turn - and
      the operation families a setup offers follow the kind; today every
      assumption in the app is a mill. The definition carries the machine's
      numbers: X/Y/Z travel and rapid rates, spindle speed and power limits
      (the ceilings the tool library's per-material speeds and feeds are
      checked against), rotary axes where present
      (A/B/C travel, rapid speed, positioning resolution), tool changer
      capacity, and coolant options (none / mist / flood / high-pressure /
      through-spindle) that reach the post as M-codes (M1.7's posts read
      the coolant set from the `.mch` machine file). The user-facing workflow (pick a machine,
      edit it, see it on the setup) is specced from published CAM user
      documentation only; no vendor strings, templates, or post files are
      copied or adapted
- **RTCP output**: G43.4 / A-angles correctly emitted for trunnion and tilting-head machines
- **Post debugging**: block-by-block trace from toolpath to line of G-code

---

## Phase 4 - Refactoring and reuseability.

The rust codebase is to complicated and convoluted. All motion types, operations, tool management,
heights and offsets, movement, simulation. Should be isolated as smaller subcreates. Code
files should be no larger than 400 lines of code to keep it maintainable. All tests should be isolated
to each subcrate to make testing cleaner.

### M4.1 - Seperating the code into crates
**Goal: Sepearting all components into smaller crates.**

- **Identify code blocks**: Identify and map out code for operations, setups, machines etc etc. all the parts that make out the application
- **Chunk the parts up**: Cut all the parts apart and refactor.
- **Move tests**: Move tests to their relevant crates.
- **Run tests**: Make sure each crates test can be run individuall.
- **Clean up**: Make sure all tests pass, code compiles and applicatiotn passes a user test.

### M4.2 - Multi plattform build targets.
**Goal: Building the same application, for multiple OS environments.**

- **Windows**: First usecase, its what we got.
- **Linux**: Prioritized build case, where we will have most adopters.
- **MacOSX**: Secondary prioritsation.
- **WASM**: A web version for online use.
- **Docker**: Using docker builds for every target.

### M4.3 - Defer HSMWorks label thru out the code.
**Goal: Removing the HSMWorks branding as its conflicting**

- **Debrand**: Find every instance of where "hsmworks" been used.
- **Rename**: New name is "camcraft"

### M4.4 - Test cost shape: fast, targeted gates
**Goal: a gate run costs what the diff cost — no 30-minute suite tail**

The dev-time tax, measured: `crates/app/tests/finish3d.rs` alone runs
~30 min single-threaded in the debug gate, and it holds up every ladder
regardless of what changed. The cost is concentrated in the
envelope-VERDICT tests (`scallop_crosses_the_diagonals_and_measures_clean`,
`the_hill_under_radial_and_spiral_is_envelope_clean`,
`the_hill_under_contour_and_project_is_envelope_clean`) — each one is a
grid `envelope_scan` over a 12×96 dome plus a full `verify()` in
Envelope mode with its own `slice::project(facets, 64)`, at debug speed.

- **Split LAW from VERDICT.** Law tests assert generation shape —
  bucket membership, refusal strings, move and link counts — and must
  run in milliseconds: generate one small path, assert the structure,
  done. Verdict tests assert the geometry *machines clean* (scan,
  project, envelope-verify) and are minutes each. Every suite gets this
  cut, finish3d first: the law half stays in the default binary, the
  verdict half moves to its own `#[test] #[ignore]` lane or a separate
  `--test` binary so `cargo test -p hsmworks-app --test finish3d` is
  seconds, not the tail that decides the milestone.
- **The heavy lane runs on release, opt-in.** `#[ignore]`d verdicts
  execute under `cargo test --release -- --ignored` — an hour of debug
  grind is minutes optimized — scheduled at milestone closes and
  pre-push, never per-edit. The split must not lose coverage: the count
  of law + verdict tests before and after must match, and the verdict
  lane keeps its own golden asserts.
- **Targeted gate selection.** The gate ladder learns to run only the
  test binaries the diff touches (`cargo test -p <crate> --test <name>`
  per changed file; suites are already one binary each, so selection is
  free), with the full ladder reserved for milestone commits. Document
  the map in the test file headers: which binary covers which module.
- **Budget.** Default debug ladder target: under ~5 min wall on this
  32-core box — the finish3d split is the single biggest lever (it is
  the current longest pole), so it lands first; then sweep the other
  suites for the same verdict-in-the-law-lane shape (`machining3d`,
  `rest`, `verify` envelope tests carry the same `project`+`verify`
  pattern in smaller doses).

---

## Phase 5 — Power: Multi-Axis, Turning, Scale

The Ultimate-edition feature set on top of a mature product: continuous
multi-axis, mill-turn, and the performance and automation to run a shop
programming department.

### M5.1 - 5-Axis Milling
**Goal: simultaneous-axis and tilted machining done safely.**

- **Swarf and advanced swarf**: bladed and ruled-surface finishing,
      tolerance-aware multi-pass (the catalog's swarf pair)
- **5-axis contour and parallel** with tool axis control: lead/lag, tilt step, max tilt
- **The rotary family**: rotary offset roughing, multi-axis roughing,
      multi-axis contour, rotary parallel, rotary contour, multi-axis
      finishing - the catalog's multi-axis block lands here, each riding
      the orientation and machine-sim laws below
- **3+2 roughing**: the planar law of Phases 1-2 on a tilted, indexed work
      plane; position commands between cuts, no simultaneous-axis motion
- **Collision-aware orientation**: auto-tilt to avoid shank contact, with shortest-travel smoothing
- **Drive curves**: machine to a curve on a surface with orientation rules
- **Machine simulation**: full kinematic simulation of the post's A/B/C motion model, not just the toolpath

### M5.2 - Turning & Mill-Turn
**Goal: lathe parts and C-axis parts from the same system.**

- **Lathe machine model**: the turning side of M3.3's machine kinds - a
      lathe-kind machine offers turning operations on its own axis model
      (X/Z, spindle as the cutting axis), so the mill-centric plumbing
      finally branches; mill-turn machines offer both families
- **The turning operation set** (the catalog's list, made checkable):
      facing, profile roughing and finishing, roughing/finishing groove,
      turning groove and single groove, adaptive roughing, threading,
      boring, parting, chamfering, taper turning, form turning,
      multi-pass turning - inserts from the tool library, passes judged
      against the M2.3-style material sim on the revolved axis model
- **Turning holes**: radial and axial drilling through live tooling -
      M1.6's cycles and hole recognition reused on the turned solid
- **Bar feeder patterns**: catch/transfer support for production turning
- **Mill-turn**: live tooling operations indexed and continuous (C-axis)

### M5.3 - Performance & Automation (Release 0.4.0)
**Goal: big models, headless jobs, scripting.**

- **Large models**: GPU-resident sparse voxel / SDF stock simulation, streaming mesh and B-rep handling, 10M+ triangle models
- **Generation performance**: threaded toolpath generation, incremental recompute after model changes
- **Scripting API**: Typescript bindings for batch ops - generate, verify, post, report
- **Part handling**: loading, unloading, and transfer choreography as
      post-emittable motion intents (the catalog's part-handling block;
      orientation and clamping are already M3.2's setups and fixtures) -
      M5.2's bar feeders get their general form here
- **In-process inspection**: probe-style measurement hooks after ops,
      feeding the deviation verdict a re-run decision, not a report tail
- **CLI**: headless project processing and regression runs
- **Regression suite**: golden-model CI (import → generate → verify → post) with numerical tolerances

---

## Phase 6 — GPU acceleration.

The whole appplication are starting to having serious issues with graphical performance running ontop of EGUI. And we most likely
need to figure out how to offload simulation, voxel body calculation and tool preview etc onto the GPU.

### M6.1 - Explore the validity of Adding GPU acceleration support.
**Goal: Explore the options for GPU acceleration, Vulcan is prob the best option as its properly cross platform.**

- **Which runtime works?**: We have touched on GPU acceleration earlier, but need to revisits optimization.
- **Cross-platform.**: It needs to work on windows, linux - first of all. MacOSX should work if linux does.

## Milestone Summary

| Milestone | Goal | Key Features | Status |
|-----------|------|--------------|--------|
| **M1.1** | Model in, project saved, never crash | Mesh import, save/load, undo/redo, robustness, document units (IGES deferred) | ⚠️ |
| **M1.2** | Define a job | Shell, dockable viewport, STEP import, stock + WCS setup, toolpath display | ✅ |
| **M1.3** | Cut a 2.5D plate | Face, contour, pocket, slot, trace, tool library, parameters, verification warnings, tangential leads; G41/G42 output, spiral/helical entries open | ⚠️ |
| **M1.4** | The workflow on the surface | Ribbon with icons, browser tree, contextual Operations pane, no silent refusals, what-gets-cut preview; display modes, the tabbed-panels overhaul and the graphical tool editor window all done - riding the graduated egui stack | ⚠️ |
| **M1.4.1** | The look and feel, fixed at the root | Dear ImGui → egui: decisions recorded (egui adopted, clay considered and rejected, no dual-stack), spike done - GO (`docs/spike-egui-go-no-go.md`) | ✅ |
| **M1.4.2** | egui is the shell | Per-panel graduation M-a…M-g: ribbon/browser, property pages (tabbed), dialogs + file ops, viewport interaction, shared actions law; dear-imgui deps and the imgui surfaces retired, Tabler glyphs as egui textures | ✅ |
| **M1.4.3** | A home for every operation | Family-sectioned ribbon per the Operation Catalog, overflow dropdowns, user pinning (a document setting); lands before any new-family op - the home law | 📋 |
| **M1.5** | Pick a face, not just a model | Face/edge clustering, face/edge and welded-patch pick modes, selection-driven contour and facing (open runs extend to stock), arc moves, staged facing, retract hops, readable selection rows, honest height planes, From/Offset height references, bore-aware contour side (Auto/Inside/Outside), translucent cut-region fills, folded collision warnings, v5→v7 persistence, view cube + nav bar; vertex picking, WCS snapping deferred | ⚠️ |
| **M1.6** | Machine the hole list | Spot/drill/peck/deep/tap/bore cycles, hole recognition into editable params, canned-cycle representation beside the move chain; thread milling and the post itself open | ✅ |
| **M1.7** | Verified first output | Move simulation, validation, CPS post engine (`.cps`/`.mch`/our intermediate) + the Fanuc post as its first script, G-code viewer, CI matrix + portable-zip release, 0.1.0 | ✅ |
| **M2.1** | Rough 3D at constant load | Done: 3D Pocket, rest material and adaptive clearing (level law, computed islands, entries; dexel sim, half-step closing ladder, corner law; engagement-step law, ring join law, closure-as-data) and feed modulation (median-anchor law, two-station chord) | ✅ |
| **M2.2** | Finish 3D to tolerance | Waterline, the tolerance-verification kernel, the parallel raster and scallop, the polar placements (radial, spiral), the solid-truth hidden rest and the center-shadow families (contour, project) done (tangent-locus rings and Z-follow lines, two-truth envelope, measured verdicts, 45-deg auto-split, the diagonal crossing pair, ray-parity material intervals, closed-loop march, the certified diagonal chain between raster lines); surface-finish params open | ⚠️ |
| **M2.3** | Trust before cutting | Done: program material sim (chained cached ops), deviation color map with located verdicts, machine-definition checker (.mch travel/feed/spindle), verification made the post gate, 0.2.0 | ✅ |
| **M2.4** | 2D, complete | 2D adaptive + offset pocket roughing, engrave, 2D chamfer, circular, standalone bore, helical thread milling | 📋 |
| **M2.5** | 3-axis, complete | Steep-and-shallow, flat-area, pencil, ramp, morphed spiral, geodesic, blend/morph/corner, flow (freeform), deburr; M2.2's surface-finish params | 📋 |
| **M3.1** | Model changes, toolpaths update | Done: stable feature identity + `UpdateModel` revision (A), stamp inputs - tool/WCS/FromModel stock propagate, staleness rides the copy (B), live-question `stale_reason` + one-undo-unit `RegenerateAll` (C), re-MACHining - rest masks from predecessors' recorded paths, converges in two rounds (D) | ✅ |
| **M3.2** | Production setups | Done: multi-setup with implicit-setup law + datum transfer (A); fixtures - user STEP solids wearing a marker, spindle/table bands from the `.mch`, live-gate collision law (B); per-setup tool list in change order (C); setup sheets, one form-fed page per setup, no timestamps (D); workspaces by machine kind, axis-inventory inference (E) | ✅ |
| **M3.3** | Every control covered | Post engine, 5 posts, machine definitions (mill/lathe kinds, axis limits, speeds and feeds, coolant), RTCP, 0.3.0 | 📋 |
| **M4.1** | Code refactor | Refactor of codebase, test, and code organisation. | 📋 |
| **M4.2** | Multibuilds | Refactor of codebase is done and application builds for multipls OS and environments. | 📋 |
| **M4.3** | Defer Code | All references to "hsmworks" in the code needs to be renamed and re-referenced to camcraft | 📋 |
| **M4.4** | Test cost shape | Law/verdict test split, `#[ignore]`+release heavy lane, diff-targeted gate selection - finish3d's 30-min tail first | 📋 |
| **M5.1** | Five axes | Swarf, tilt control, collision-aware orientation, machine sim | 📋 |
| **M5.2** | Lathes too | Lathe machine model, turning cycles, bar feeder, mill-turn | 📋 |
| **M5.3** | Shop scale | 10M tri performance, Lua, CLI, regression CI, 0.4.0 | 📋 |
| **M6.1** | GPU acceleration | Simulation, preview, SDF, Voxel, B-Rep - acceleration | 📋 |

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
- **Simulation stock representation**: adaptive dexel vs. sparse voxel octree vs. signed distance field - decide before M1.7.
  The voxel grid models the *machined block for simulation only*: operation geometry and tolerance checks still evaluate against
  B-rep/mesh, so discretized stock is the verification layer, not the source of truth (an SDF is the closest cousin to a particle
  approach and keeps the exact remaining-material queries gouge checks need)
- **Cutting data**: no licensed handbook is affordable long-term - grow a homegrown
  materials/tools database under an open license, seeded from public vendor catalogs
- **Patent check (FTO)**: high-efficiency milling and some 5-axis orientation techniques
  were patented in the 2000s-2010s; run a freedom-to-operate review before M2.1 and M5.1
  to confirm the literature we're implementing from is unencumbered
- **Probing / workpiece verification**: deferred until post Phase 2 (which probing protocol
  sets first - RS238 or Renishaw-style macros?)
- **Feature recognition**: automatic hole/pocket/slot recognition beyond M1.5 selection
  and M1.6 basics - deferred; M3.1 associativity will need some of it anyway
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
