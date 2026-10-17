# RECOMMENDED.md — decisions taken unattended, for later discussion

> _Growing file. The user stepped away on 2026-10-17 and mandated: run the
> rest of ROADMAP.md solo, every recommended decision is the way forward,
> log each one here as a point of discussion after the roadmap is cleared.
> Nothing in this file is a blocker - work shipped with each choice made -
> but any of them may be revisited._

Format per entry: **what was decided**, **why**, **what it'd cost to change
later**. Newest milestone first.

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
