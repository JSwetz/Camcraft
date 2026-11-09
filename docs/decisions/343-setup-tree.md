## #343 - Browser tree: each setup owns its WCS, Stock, Machine, Post, Fixtures and Tool list (2026-11-09)

- **Chosen: tree shape.** The Setup section holds the Setups manager row,
  then one node per setup with a radio (filled = active) and its children
  in the maintainer's order: WCS, Stock, Machine, Post, Fixtures (each
  fixture a child row), Tool list, Operations (its 1+n operations). A
  document without setups shows the same children straight under Setup
  for its implicit setup. The document-wide Operations section is gone;
  Models and the document's Tools library stay top-level.
- **Chosen: the radio is `ActivateSetup`** (undoable); the setup row opens
  the setup's settings page (active radio, stock source, part alignment,
  plane pick, WCS label, delete). The stock source shows "its own stock"
  and, from the second setup on, a disabled "the stock <previous setup>
  leaves behind" arm - the hook for #317 / #320, not implemented here.
- **Chosen: a node click activates its setup first.** The Stock, WCS,
  Fixtures, Machine and Post editors edit the ACTIVE setup's furniture and
  mount, so clicking a node of an inactive setup queues `ActivateSetup`
  before targeting the page - one undoable step, and no editor can change
  a setup the clicked node does not belong to. A setup page left open
  after a later radio click shows "activate it to edit" instead of its
  editors.
- **Chosen: machine and post per setup as an override.** `SetupDef` gains
  `machine: Option<MachineDef>` and `post: Option<PostRef>`; `None` (every
  older file, every new setup) follows the document's singleton. New
  undoable commands `SetSetupMachine` / `SetSetupPost` (validated like
  `LoadMachine` / `SelectPost`); a datum transfer carries the source's
  mount. The Machine / Post pages offer "use the document's" vs "this
  setup has its own" (a copy of what it mounts now, so nothing changes
  until edited), then the existing selector - whose verbs land on the
  active setup's own mount when it has one, the document's otherwise.
  Post options (skip stops, per-op output, threading) stay document-wide.
- **Chosen: the one-mount law.** The app reads one mounted machine and post
  - the active setup's (`machine::of`, `kind_of`, `posting::post_of`). Work
  crossing mounts is refused by name: generating an operation whose setup
  mounts other steel than the active one ("activate 'SetupN' first"), and
  posting a program whose setups disagree on machine or post (the gate's
  "one program is written by one post for one machine"). With no setup
  owning a mount neither refusal can fire, so every existing document
  generates and posts byte-identically.
- **Chosen: posting a program spanning different posts refuses by name**
  (over one NC file per post). Refusing is the safe, visible arm today;
  one file per post/setup is follow-up work.
- **Chosen: tool list (b)** - the document's tool library plus a per-setup
  filtered view: the Tool list page tables the tools the setup's
  operations use (number, size, ops, Edit) and says that editing a tool
  edits it in every setup using it; the tool-number clash law is shown
  for the setup's tools. The export is the existing setup-sheet export
  (it carries each setup's tool list).
- **Chosen: project v55**, no breaking change: v54 files open unchanged
  (absent keys = follow the document); a v55 file is refused by older
  builds, which would otherwise post every setup on the document's machine.
- **Chosen: split instead of grow.** The browser code left `panes.rs` for
  `browser_tree.rs` (rows), `browser_pane.rs` (click law) and
  `tree_menu.rs`; the tree glyphs left `painted.rs` for `tree_glyphs.rs`;
  the routed machine/post verbs live in `state_setup_mount.rs`.
- **Dropped:** threading the setup through every machine reader so an
  inactive setup's operations generate against their own machine (large,
  touches every generator law - follow-up); per-setup post options;
  copying the document's mount into every setup at load (makes every
  file re-save differently for no behaviour); a per-setup tool library
  (option (a) - duplicate tools, numbers diverging per copy).
- **Cost to change:** low for the tree and pages; moderate to lift the
  one-mount law (every `machine::kind_of(world)` reader learns a setup).
