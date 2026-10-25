# AGENTS.md

## What This Is

A clean-room implementation of HSMWorks: a CAM system that turns 3D models into
NC programs (G-code) for CNC milling machines. Rust, Flecs ECS, egui.

"HSMWorks" is a trademark of Autodesk. This project is independent and
unaffiliated — it reimplements user-visible behavior (operation types,
parameters, workflow) from public documentation and general machining
knowledge, nothing else.

## Autonomous Work Law

The user trusts the recommendations made in their absence and reviews them
afterwards - the work must not bother them.

- **Autonomous always.** Run the ROADMAP without asking for decisions. When
  a choice appears, take the recommended option and keep moving; occasional
  one-line status feedback is fine, a blocking question is not.
- **Every recommendation is recorded.** Each decision taken unattended gets
  an entry in `RECOMMENDED.md` - what was chosen, why, what was considered
  and dropped - as the discussion ledger for after the roadmap is cleared.
- **Never stop, never recap mid-roadmap.** There is no "wrap up and check
  in" state: the next ROADMAP item starts when the current gate is green.
  Session summaries are for compaction, not for the user.
- **Counter-measure against stalling.** If the work risks stopping before
  the roadmap is done (idle after a gate, waiting on nothing, end-of-turn
  with open items), spawn a continuation nudge subagent to remind the
  session to pick up the next ROADMAP item. A stall is a bug.
- **Side work rides subagents.** All testing, documentation, and knowledge
  graphing (the graphify updates) are done by subagents dispatched as
  subtasks, never inline: their completion notification is what triggers
  the progression to the next ROADMAP item. This keeps the main agent
  running continuously, as "Never stop" above mandates - the main session
  plans, codes, and decides; the subagents verify, record, and graph.

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

- `CreateOperation` / `DeleteOperation` → operation entities
- `ChangeTool` / `EditOperationParams` → operations and setups that had a
  tool, stock, or depth changed (write the new value on command creation,
  not via systems; undo must restore the previous value)
- `SelectEntity` → selection (which operations/models are highlighted)
- `ActivateSetup` → the active setup
- Ephemeral UI state (hover, drag previews, simulation playback position) is
  immediate-mode only - not undoable

Systems and UI never write these fields ad hoc; they only respond to commands.
If a UI change has no command for it, that's a bug - add one.

Components are auto-registered: declare them in `crates/app/src/schema.rs`
and the startup hook (`flecs_bits::register_schema`) handles the rest, in every
binary. Current policy: components are Rust-owned types (`Vec`/`Box` fields),
freed by the delete hooks flecs_ecs registers for them. The invariant that
matters is unchanged: deleting an entity must free ALL associated resources
(e.g. the cached toolpath and rest-material meshes of a deleted operation).
Per-type arenas for C-style `raw_data` storage are a deferred performance
optimization (M5.3-era), not a current requirement; if a type ever adopts
`raw_data`, it must use a per-type arena - never a global allocator - and
must carry a delete hook that frees everything it owns.

## Assets

`assets/ui/` holds PNGs for toolbar and tree icons; `assets/ui/fonts/` holds
the fonts. The UI must render them — loading the files alone isn't enough;
wired-in usage in each panel's draw code is required.

Icon sources: free, attribution-safe sets only - MIT/ISC/Apache-2.0/BSL
licensed libraries may be bundled without crediting end users; primary pick is
[Tabler Icons](https://tabler-icons.io) (MIT). Never icons8's free tier
(requires attribution), never paid-without-need. 32x32 PNGs are fine; render
at 16px. Keep the copyright/license texts of any bundled set in
`assets/ui/LICENSES.md` (this is also the home for third-party notices like
clipper2-rust's BSL text) - retention is a license condition, not a credit.
Operation-specific glyphs are drawn original when no generic set fits. All
icons referenced from `crates/ui/src/egui_app/icons.rs`. Icons and post-processor
templates (see below) must be original or licensed without attribution —
never adapted from HSMWorks material.

## Dependencies

Layering is strict: ui → app → flecs. UI panels, widgets, and dialogs must
not use sys_* or world-level direct calls; put those in app. Only app-layer systems and commands use raw Flecs calls.

Schema components live in the schema module (`crates/app/src/schema.rs`):
the `#[derive(Component)]` structs and their CPOD decls. Other app modules
must not declare components.
Geometry parts are components (`app::model::Part`); use `model::child_parts` /
`model::child_parts_of` to traverse - in UI and systems. Components don't
reference each other. `Part` stores `child: Entity` (not parent); systems
query directly (no helper accessors). `decompose` stores facets directly into
`Part`.

Check only touched files: `-p` per touched crate - `camcraft-ui`,
`camcraft-app`, `camcraft-motion`, `camcraft-sim`, `camcraft-post`,
`camcraft-tools`, `camcraft-cl`, `camcraft-geometry` (not the whole
workspace, which rebuilds flecs). The M4.1 split landed: geometry/cl/tools/
post/sim/motion are their own crates; app's `operation` and `project` modules
are split along section seams.

## Code shape (Phase 4 onward)

Phase 4 refactors the codebase for reuse; work in the shape it is heading to:

- **400-line files.** No source file grows past ~400 LOC; split along the
  domain seam instead of stacking another section onto a large file.
- **One concern per subcrate.** Motion types, operations, tool management,
  heights/offsets, movement and simulation become isolated subcrates (M4.1).
  Until the split lands, keep those boundaries honest inside `app`: new code
  goes in the module that will become its crate's seed, and cross-domain
  calls go through the seams that will become crate APIs - not through
  private helpers of another domain.
- **Stays buildable everywhere.** No platform-specific assumptions in core
  code (M4.2 ships Windows/Linux/macOS builds from the same source).
- **Neutral naming.** M4.3 renamed the product label hsmworks to camcraft
  everywhere, crates included; nothing - string, comment, or identifier -
  carries the old brand. Legitimate trademark mentions ("no HSMWorks
  material consulted", "a clean-room implementation of HSMWorks") name
  Autodesk's product and stay: they're true statements, not branding.

## Testing

`crates/app/tests/` (app + commands) and `crates/ui/tests/` (UI/event tests
which spawn the real app via `test_harness.rs`; helpers in `ui::test_harness`,
app-level fixtures in `crates/app/tests/common/mod.rs`). Each future subcrate
carries its own tests inside its crate (M4.1).

Gate yourself narrowly, not by re-running everything: build once with
`cargo test -p camcraft-app -p camcraft-ui --no-run`, then run the individual
test binaries the diff touches (`target\debug\deps\<name>-<hash>.exe`, in
parallel - they take no cargo lock). Each suite's header carries the module
map (`Gate map (M4.4): covers app::x, app::y...`) - pick binaries from it,
not from the whole tree. The debug ladder's budget is under ~5 min wall;
everything slower is in the heavy lane below, not in yours. Under "Side
work rides subagents" this ladder is run BY a gate subagent: it reports
pass/fail per binary with failure output, and its report is what lets the
main session commit. Never hand the subagent both the code change and its
verification - the main session owns the code and the commit decision.

Laws and verdicts run in different lanes (M4.4): tests that MEASURE geometry
clean (envelope scans, full-precision projections) or generate over the big
fixtures are `#[ignore]`d - `cargo test --release -p camcraft-app --
--ignored` executes them in minutes and belongs at milestone close-out and
pre-push, never per-edit. The ignore reason names its lane. Coverage must
not ride the split away: `#[test]` count before and after must match, and
the ignored set keeps its golden asserts. UI snapshot binaries resolve their
goldens from `crates/ui` - run them with that CWD.

`VisualRunner` is auto-enabled: windows show, run fast, and stay open if closed
early; hidden runs cap at 30s; auto-close after 10s when no test is pending; on
crash or timeout, the window is left open.

Add headless tests for toolpath generation (golden CLFile/G-code outputs
committed under `tests/golden/`), stock simulation, and post processing; fuzz
tests for the geometry importers and the toolpath generators (degenerate input
must error, never crash or hang).

## Knowledge Graph

`graphify-out/` holds a graphify knowledge graph of this repo: code extracted
structurally (AST), docs and images semantically. Generated output - never
hand-edit it, never commit it.

Query it before falling back to grep for structural questions:

- `graphify query "<question>"` - how does X work, what calls Y, trace a flow
- `graphify path "A" "B"` - shortest connection between two concepts
- `graphify explain "X"` - plain-language summary of a node and its neighbors
- `graphify affected "X" --relation calls` - who breaks when X changes
  (the graph is strongest here: it knows the test-to-command wiring)

Keep it current as we work:

- Code-only changes → `graphify update` (re-extracts changed files via AST,
  free and no LLM; run it as part of wrapping up a task)
- ROADMAP.md / AGENTS.md / icon changes → `/graphify . --update` instead, so
  the semantic side is re-extracted too
- After renames or refactors that move many files → full `/graphify .` rebuild;
  the shrink guard refuses a smaller graph unless `--force` is passed
- Expected warning: `tests/golden/*.cl` read as Common Lisp and contribute
  nothing to the graph (our CLFile dumps, not source) - ignore it

Trust calibration: every edge carries EXTRACTED (1.0, read from source),
INFERRED (0.55-0.95, model-reasoned), or AMBIGUOUS (0.1-0.3) - verify an
INFERRED edge against the code before acting on it, never act on AMBIGUOUS.

## Commits and Releases

[Reference](https://github.com/qoomon/conventional-commits)

Format: `<type>(<optional scope>): <subject>` - e.g. `fix:` or
`feat:`.

Version is `0.x.0` until the clean-room audit and release.
