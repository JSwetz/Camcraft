# RECOMMENDED.md — decisions taken unattended, for later discussion

> _Growing file. The user stepped away on 2026-10-17 and mandated: run the
> rest of ROADMAP.md solo, every recommended decision is the way forward,
> log each one here as a point of discussion after the roadmap is cleared.
> Nothing in this file is a blocker - work shipped with each choice made -
> but any of them may be revisited._

Format per entry: **what was decided**, **why**, **what it'd cost to change
later**. Newest milestone first.

## M1.1 — document units, millimeters and inches (commit `feat(app,ui): document units - millimeters and inches everywhere including the post law`)

- **The unit is a lens, not a storage choice.** Every stored number -
  geometry, the micron lattice, toolpaths, the goldens, entity and draft
  parameters - is millimeters, always. The document's unit is one flecs
  world singleton (`schema::Units`), read at paint time through
  `units::of` and on no entity, so it never appears in a browser row or a
  saved entity list and needs no mirror in UI state that could go stale.
  This is the international inch (exactly 25.4 mm) and the
  internal-metric/display-at-the-edge pattern of CAD/CAM systems and
  G-code (`G20`/`G21`). Cost to change: storing dual representations or
  per-widget unit state - precisely what the singleton exists to prevent.
- **SetUnits is a command, and setting the unit the document already has
  is refused at construction.** The lens change is undoable like every
  other UI-visible change (its inverse writes the previous value back, so
  undo restores exactly what was on screen), and a no-op does not belong
  in the history. The control itself is a two-radio pair on the stock
  page - document-global, one place, not per-pane toggles. Cost to
  change: a silent history step, cheap to add and impossible to notice.
- **Widgets convert on a display copy with `speed ÷ 25.4`.** One physical
  pixel moves the same millimeters in either mode, so drag muscle memory
  is unit-fair; and because egui's smart-aim rounds the *shown* value
  inside a one-pixel radius, the identical ±0.1 mm aim window applies in
  both modes - the assist never favors one unit. `from_display` snaps
  inch-typed values onto the micron lattice (the same quantum as
  `cl::lattice`), so `0.5 in` stores exactly `12.7 mm` and no stored
  number ever carries float-conversion dust - the goldens keep their
  bits. mm mode stays identity: the historical DragValue, untouched.
  Cost to change: none wanted; the snap is the only rounding in the
  path.
- **Display precision keeps mm's bits and widens inches only where the
  unit demands it.** `units::len` prints the site's own millimeter
  decimals verbatim in mm mode and mm+2 decimals in inch mode (a mil is
  finer than a hundredth of a mm, so an inch readout must not lose
  information the mm one carried); feeds print bare in mm and `÷25.4` at
  two decimals in inches; areas divide twice and get +3. Every pre-existing
  string in mm mode is byte-identical - both snapshot gates passed with
  zero churn, which is the empirical proof the lens never leaked into
  storage or formatting. Cost to change: rounding in `len`, which is the
  round-trip-through-a-string bug this law exists to avoid.
- **Project file v8: an absent `units` key means millimeters.** The
  chained migration adds the key (`v7→8`), making the whole v7 corpus its
  own migration - older files reach v8 through the existing ladder, never
  skipping a step. Cost to change: a reader that defaults instead of
  migrating - rejected because the ladder is the file format's law.
- **"Correct everywhere including posts" is half specced, honestly.**
  There is no post yet (M1.7 ships the first), so nothing that exists can
  be wrong; the post's rule - convert at output time, declare the mode
  with `G20`/`G21` - is written into `app::units`' provenance header so
  the M1.7 post inherits it rather than reinventing it. Discussion point:
  confirm the Fanuc post keeps mm-internal numbers and converts per-line,
  not a whole-program mm→inch pass at the end.

## M1.4 — graphical tool editor (commit `feat(ui): graphical tool editor - its own window with a live to-scale preview`)

- **The editor is an in-app `egui::Window`, not an OS viewport.** egui's
  multi-viewport machinery (`show_viewport`) needs an OS window per
  surface - unhittable by the kittest harness, invisible to the snapshot
  gate, and unprovable by any test we can run headless. An in-app floating
  surface is the only windowing form this project can *verify*, and it
  behaves identically to the user (draggable, ✕-closeable, floats over the
  dock). The single-root-viewport law from the egui graduation stands
  untouched - this is an overlay, not a second winit window. Cost to
  change: swap `Window::show` for a deferred viewport and lose both the
  flow test and the snapshot.
- **Dock = library, window = editor - a considered revisit of this
  milestone's "standalone tool library window is gone" decision.** That
  retirement removed a *second browsing surface* duplicating the dock; the
  roadmap then asked for the editor to be "its own window". Both survive:
  the dock keeps the one library (rows, catalog, summaries), and the only
  window is the editor for one tool. The decision didn't reverse; it found
  the shape the two roadmap lines actually agree on. Cost to change:
  re-fusing editor fields back into the dock page (that's the pre-M1.4
  code - easy to resurrect, harder to keep uncramped at 280 px).
- **The preview draws `tools::Tool::profile`, and `profile` segments on
  purpose.** The side view's band breakpoints are verbatim
  `verify`'s shank/holder collision bands (cross-ref comment in
  `tools.rs`), zero-length bands skipped exactly as the checker skips
  them, and the drift test `profile_segments_match_the_collision_bands`
  fails if the picture and the checker ever disagree about where a band
  starts. One segmentation law, drawn and enforced from the same numbers.
  Cost to change: none wanted - drift here is gouge-adjacent.
- **Label ownership: the dock summary is composed text, so every field
  label resolves in exactly one surface.** `query_by_label("Diameter")`
  must never be ambiguous between dock and window - the dock shows a
  tool's numbers as three formatted `format!` lines (no widgets), the
  window owns all field labels. Buttons split the same way: "Add" is the
  catalog, "Add tool" the window's blank form, "Apply" window-only,
  "Delete" dock-only. Cost to change: any labeled widget added to the
  dock silently breaks the flow test's queries.
- **Closing is ✕-plus-ghost; no Esc binding.** The `open` mirror flips
  `State::tool_editor` to `None` on the ✕, and the ghost law mirrors
  `pane_target`'s: at the end of `drain_actions`, a window whose tool no
  longer exists (Delete, undo-of-Add, document swap) closes itself in one
  line. Esc was deliberately not wired to close - Esc must stay a no-op
  while a `TextEdit` has focus, and half-wiring it invites the bug where
  Esc-in-a-field both cancels the edit and kills the editor. Cost to
  change: one keyboard hook and a focus question.
- **`tool_editor: Option<String>` is view state on `State`, like
  `op_tab`** - never a command, never saved, never undoable; `Some("")`
  is the blank new-tool form so the "numbered past taken names" prefill
  rides `new_tool_draft` unchanged. Opening queues nothing (asserted);
  all drafting still lives in `ToolUi`, so the window reuses the
  existing sync/stage/Apply loop with zero new command surface. Cost to
  change: field relocation only.

## M1.4 — tabbed property pages (commit `feat(ui): tabbed property pages - the operation page is five named sections`)

- **Status lives above the tab bar, not inside a tab.** The stale badge and
  the folded collision warning render between the Type/Tool/Source block and
  the tabs, so a warning can never hide behind an unopened section. The
  report-visibility law outranked the tidier look of folding it into
  Geometry. Cost to change: move one block in `operation_page`.
- **The open tab is `State::op_tab`, deliberately not a field of `OpUi`.**
  `sync_ops_ui` rebuilds the drafts wholesale whenever the operation
  identity or doc version changes; a tab stored inside them would reset to
  Geometry on every Apply and every drained command - reads as a bug. On
  `State` it survives edits like the other transient view state (geo_pick,
  display mode): never a command, never saved, never undoable. Cost to
  change: one field relocation.
- **No dead tabs; an out-of-set tab falls back to Geometry.** A kind renders
  only the sections it has fields for (Trace has no Passes, non-Contour has
  no Leads & Links), and if `op_tab` names a section the kind lacks (the
  user switched the Type combo while on Leads), the page resolves to
  Geometry instead of clamping to an empty pane. The drift test asserts the
  cell is absent *and* the fallback paints. Cost to change: `has_section` +
  the one-line resolution.
- **Field-to-section placement** (the judgment call, one field per section,
  within-section order preserved from the long stack): Geometry takes the
  pick row + selection echoes, Contour's Side/Tolerance, Slot's A/B points
  and width; Heights takes every Z plane including Stepdown (a stepdown is
  a height law, not a pass count); Passes takes Stepover, Stock to leave,
  Strategy; Leads & Links takes Contour's leads and tabs only; Feeds takes
  the three feed fields. Stock-to-leave under Passes (it's how much stays
  per cut) rather than Heights. Cost to change: one `if tab ==` gate per
  group + the test table's placement list.
- **The tab bar wraps** (`horizontal_wrapped`): the properties pane is ~280
  px in the docked default, five cells do not fit on one row, and the
  M1.4-pane-width law says an overflowing widget keeps its a11y node but
  its clicks land in the wrong pane. Two tidy rows beat a clipped row.
  Cost to change: layout only.
- **The roadmap's M-c entry said "pages arrived already tabbed" - it was
  wrong and is corrected** (they arrived as one scroll; parity first). The
  redesign is this commit. Discussion point: whether the ROADMAP phrasing
  misled the hand-tests.

## M1.4 — viewport display modes (commit `feat(ui): viewport display modes - wireframe with honest hidden lines`)

- **Wireframe is a hidden-line drawing, not an x-ray.** Faces still
  rasterize - into the depth buffer only (a fourth pipeline, color writes
  off). The feature-edge wire then respects occlusion exactly as it does
  when lit: back-side lines stay hidden behind the solid. A see-through
  line soup was the cheaper build and reads as clutter on any real model.
  Cost to change: one branch in `View3D::render`.
- **The stock draws its full wire in wireframe mode** - its faces skip
  the depth pass. A stock box is a volume of material, not a surface;
  hiding its far edges behind itself would misrepresent it (and hid
  nothing real - you see through stock to plan). The two GPU tests pin
  both halves: `wireframe_paints_wire_not_faces` and
  `wireframe_hides_wire_behind_solids`.
- **Wire color is mode-aware in the shader** (`u.wire` flag, piggybacked
  in the uniform's tail padding): imgui-era dark wire is invisible over
  the dark background, so wireframe brightens it to cool gray - and lifts
  the selected-part warm tint to the hot amber the fills use. No second
  edge pipeline. Cost to change: two `select`s in `fs_edge`.
- **The toggle is browser-pane radios ("Solids: Shaded | Wireframe"),
  not a ribbon cell.** Direct view-state field flip under the same law as
  the axes/paths checkboxes - and the M-f tip table (imgui's 18-cell
  parity law) stays untouched; the ribbon has no key to promise. Radios
  over a ComboBox: the combo popup was unhittable in the kittest harness,
  and the whole choice is legible without a click - same reasoning as the
  Geometry page's pick-mode row. Cost to change: the row, the test, and
  the snapshot.
- **"Per view" read as per viewport** - this app has one viewport panel,
  so the mode lives once in `State`, like the camera. If multi-viewport
  docking ever comes, the field moves into the viewport tab's state.

## M1.4.2 M-g — imgui retired (commit `feat(ui): retire imgui - egui becomes the shell`)

- **Quit is now Ctrl+Q.** The only quit surface in imgui was its File menu;
  the egui shell has no menu. Rather than ship a migration that quietly
  loses an app-level capability, the shortcut layer gained a `Ctrl+Q →
  actions::quit` arm (same `!typing` gate as the file keys; same
  `Quit → request_quit → el.exit()` path as the window close button).
  Cost to change: one arm in `panes::shortcuts`.
- **Two actions helpers deleted, not ported:** `new_operation` (blank the
  target to show the new-op form) and `delete_operation` (menu-selected
  op delete). Both existed only for imgui menus; the ribbon's Create
  cells and Delete cell + Del key already produce identical effects
  through the same module. The `DeleteOperation` command kind itself
  stays (the Operations pane's own button pushes it, pages.rs). Cost to
  change: re-add a helper; the command law is untouched.
- **Bare `cargo run` now opens the egui window; `--ui imgui` refuses with
  rc 2** instead of silently drawing the old shell. A named refusal beats
  a fallback that no longer exists. If scripts of yours pinned `--ui
  imgui`, they now need one word changed.
- **The S0 "one wgpu" coercion test retired** (`wgpu_types_are_unified`):
  with `dear_imgui_wgpu` gone there is exactly one wgpu in the tree by
  construction - `egui_wgpu::wgpu` - and nothing left to coerce.
- **The imgui headless probe retired the same way** (`headless_frames_run`
  deleted): the egui `--headless` proof runs the same command world
  through the real `run()` entry; `tests/shell.rs` keeps the CLI-contract
  and state-law tests that were never imgui-specific.
- **Icon law: PNGs ride egui textures, vectors stay the fallback.**
  `egui_app/icons.rs` registers the same Tabler MIT glyphs via
  `ctx.load_texture` (CPU-side registration, so the kittest harness
  proves the PNG path without a GPU); every consumer checks the texture
  first and paints the vector twin when it's absent. Layout is
  GPU-independent by construction - the ribbon geometry matches imgui's
  own formula (CELL_H 35, RIBBON_H 61) rather than the spike's squeezed
  49px strip. Cost to change: none - this is how Rerun-scale apps do it.

## How this file ends

When the roadmap's open items are exhausted, this file gets its final
entry summarizing what "done" meant, and the discussion happens over it.
