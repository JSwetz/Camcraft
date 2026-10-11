# AGENTS.md

## What This Is

A clean-room implementation of HSMWorks: a CAM system that turns 3D models into
NC programs (G-code) for CNC milling machines. Rust, Flecs ECS, Dear ImGui.

"HSMWorks" is a trademark of Autodesk. This project is independent and
unaffiliated — it reimplements user-visible behavior (operation types,
parameters, workflow) from public documentation and general machining
knowledge, nothing else.

## Clean-Room Protocol

- No reading, decompiling, disassembling, or tracing HSMWorks binaries or
  source. No code derived from their code.
- No copying their text: UI strings, docs, help content, parameter names where
  original, comments, or bundled post processors.
- No assets of any kind from their installer or install tree — icons, images,
  tool/NC libraries are all made from scratch.
- Reference material is limited to: Autodesk's published user documentation
  (behavior only, not implementation), public G-code standards (RS-274, ISO
  6983), machine-tool handbooks, and our own design notes.
- New geometry or toolpath algorithms must be written from published technical
  literature, not from inspection of HSMWorks output forensics.
- Every module carries a one-line provenance comment: what it implements and
  from which public source it was specced.

## Prime Directive: Never Crash, Never Gouge

A CAM program drives a spindle in front of a human. The output is either
correct, or visibly refused with a diagnostic. Silent wrong answers — a crash
or a toolpath that gouges, collides, or cuts where it shouldn't — are the
worst failure mode, without exception.

- Handle errors or propagate them - do not use unwrap() or expect().
- Degenerate input (zero-area triangles, self-intersecting shells, nonsense
  tolerances, corrupt STL/STEP models) → clean error, not a crash or hang.
- Anything a collision check or simulation flags as unsafe must block the
  post, never pass through silently.
- No panics in systems or on any user-triggered path. `unwrap()`, `expect()`,
  and `panic!()` are forbidden outside tests and `#[cold]` unreachable arms.
  Prefer `Option`/`Result` and graceful fallbacks. A crash is a bug, always.
  No exceptions.
- Use safe numeric types for geometry: saturating, checked, or clamped ops -
  no unchecked floats in bounds/index math.
- Validate early: reject malformed setups and parameters at command
  creation, not deep inside a generator where the failure is opaque.
- Prefer recoverable states: if an operation can only be partly computed,
  mark it "incomplete" rather than emit a subtly wrong toolpath.
- Log diagnostics, never silence.
- If a leak is found, fix it now, not later. Treat leaks as critical bugs:
  they crash the app eventually, and "we'll clean it up later" always makes
  it worse.

## Everything Is a Command

UI state changes are not ad-hoc field pokes - they are commands with entity
references. Undo (Ctrl+Z) and redo (Ctrl+Y) iterate the command history and
invert: the history is the single source of truth for what undo/redo can do.
Any UI-driven change goes through it, including parameter edits in the
operation property panel.

- `CreateOperation` / `DeleteOperation` → operation entities (create/delete
  the entity; no raw_data)
- `ChangeTool` / `EditOperationParams` → operations and setups that had a
  tool, stock, or depth changed (write raw_data directly on command creation,
  not via systems; undo must restore the previous value)
- `SelectEntity` → selection (which operations/models are highlighted)
- `ActivateSetup` → the active setup
- Ephemeral UI state (hover, drag previews, simulation playback position) is
  immediate-mode only - not undoable

Systems and UI never write these fields ad hoc; they only respond to commands.
If a UI change has no command for it, that's a bug - add one.

Components are auto-registered: declare them in `crates/app/src/schema.rs`
and the startup hook (`flecs_bits::register_schema`) handles the rest, in every
binary. CPODs are self-contained - `Copy` types with no raw pointers.
Types with `raw_data` use per-type arenas, never global allocators; a type
with `raw_data` must be explicitly deleted to free its allocation, and delete
hooks must free ALL associated resources (e.g. the cached toolpath and
rest-material meshes of a deleted operation). If a type has no
delete hook, it must not have `raw_data` (e.g. `Setup` owns `OpRefs` raw_data
but has no delete hook - only `World::fini()` frees it; `Stock` and `Toolpath`
free their data on delete).

## Assets

`assets/ui/` holds PNGs for toolbar and tree icons; `assets/ui/fonts/` holds
the fonts. The UI must render them — loading the files alone isn't enough;
wired-in usage in each panel's draw code is required.

Icon sources: `icons8.com` - buy the
paid "designer pack" (attribution-free) - never the free tier (requires
attribution). 32x32 PNGs are fine; render at 16px. All icons referenced from
`crates/ui/src/icons.rs`. Icons and post-processor templates (see below) must
be original or licensed without attribution — never adapted from HSMWorks
material.

## Dependencies

Layering is strict: ui → app → flecs. UI panels, widgets, and dialogs must
not use sys_* or world-level direct calls; put those in app. Only app-layer systems and commands use raw Flecs calls.

Schema components live in the schema module (`crates/app/src/schema.rs`):
component decls (POD/CPOD), extern impls (`raw_data`, `type_id`), and the
`#[derive(Component)]` structs. Other app modules must not declare components.
Geometry parts are components (`app::model::Part`); use `model::child_parts` /
`model::child_parts_of` to traverse - in UI and systems. Components don't
reference each other. `Part` stores `child: Entity` (not parent); systems
query directly (no helper accessors). `decompose` stores facets directly into
`Part`. `SceneGeometry::ensure()` is forbidden in main-thread command/undo
paths (use the async path or rebuild on demand).

Check only touched files: `cargo check -p hsmworks-ui` / `-p hsmworks-app` (not
the whole workspace, which rebuilds flecs).

## Testing

`crates/app/tests/` (app + commands) and `crates/ui/tests/` (UI/event tests
which spawn the real app via `test_harness.rs`; helpers in `ui::test_harness`,
app-level fixtures in `crates/app/tests/common/mod.rs`).

`VisualRunner` is auto-enabled: windows show, run fast, and stay open if closed
early; hidden runs cap at 30s; auto-close after 10s when no test is pending; on
crash or timeout, the window is left open.

Add headless tests for toolpath generation (golden CLFile/G-code outputs
committed under `tests/golden/`), stock simulation, and post processing; fuzz
tests for the geometry importers and the toolpath generators (degenerate input
must error, never crash or hang).

## Commits and Releases

[Reference](https://github.com/qoomon/conventional-commits)

Format: `<type>(<optional scope>): <subject>` - e.g. `fix:` or
`feat:`.

Version is `0.x.0` until the clean-room audit and release.
