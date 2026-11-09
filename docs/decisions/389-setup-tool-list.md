## #389 - Setup Tool list page: its own tool-list export, and per-setup post options (2026-11-08)

- **Chosen: post options per setup as an override.** `SetupDef` gains
  `post_options: Option<PostOptions>`; `None` (every older file, every new
  setup) follows the document's `PostOptions` singleton - #343's own /
  follow-the-document law for the post itself. New undoable command
  `SetSetupPostOptions`; a datum transfer carries the source's options
  with its post. The setup's Post page offers "Use the document's post
  options" / "This setup has its own post options" (a copy of what it
  posts with now, so nothing changes until edited); the existing option
  widgets (Skip pauses and homes, Output, Threading) then land on the
  active setup's own options when it has them, else the document's (the
  `state::setup_mount` routing law). Why: one law for every mount-like
  choice; the widgets did not have to move.
- **Chosen: which options a program posts with.** `posting::post_options`
  now reads the ACTIVE setup's options (own, else the document's), like
  `posting::post_of`; the document singleton is
  `setup_post_options::document`, which the save and `SetPostOptions`
  read. "Skip pauses and homes" is per operation by nature (it mutes each
  op's after-action), so each op reads its own setup's switch and setups
  may differ. The Output (one file per op) and the thread output shape
  the whole program, so a program whose setups disagree on either is
  refused by name in the verification gate (beside #343's one-mount
  clause) instead of being posted half one way. Dropped: refusing any
  disagreement (needlessly blocks the common "skip stops on the proven
  setup only" case); silently using the active setup's for all (a silent
  wrong answer for the other setup).
- **Chosen: additive file key, no version step.** `SetupDto.post_options`
  (`{"threading": "g32"|"g76", "skip_stops", "per_op_files"}`, spelled in
  full - absent already means "follows") is skipped when `None`, so a
  document that never set per-setup options saves byte for byte as before
  and keeps version 57. This follows the document's own #110 / #111
  switches, which were also additive. Cost: an older build opening a file
  with per-setup options ignores them and posts with the document's
  options (as the #110/#111 keys already behave); a version step would
  have changed every saved file and conflicted with the other open
  setup-file PRs. Changing it later is one version rung.
- **Chosen: tool-list export as CSV or text by extension.** "Export tool
  list…" on the setup's Tool list page asks for a path (CSV filter first,
  default `<setup>-tools.csv`); a `.csv` path writes RFC 4180 CSV (CRLF,
  quoted fields), anything else a fixed-column text list. Columns: T
  number, tool name, diameter (mm, three decimals - the setup sheet's
  voice), kind, holder (`D x L` mm or `none`), operations (labels, in
  program order). Rows in first-use order from `tool_list::for_setup`, so
  the export, the sheet and the post agree on the list. A pure read in
  `app::tool_list_export`; the UI writes the file, nothing enters the
  history (the setup-sheet law). Dropped: a separate format picker
  (the save dialog's extension is enough); document units (the sheet is
  mm-only today; a unit-aware sheet and list belong together).

**Merged with main's #155 program entries and #388 per-mount files (review fix):**
- `program_entry::posted_afters` takes a per-op skip predicate instead of one document-wide bool. Each op's after-action is muted by its own setup's switch.
- A Pause/Home entry folding into a muted op's gap is muted with it, so it cannot refuse. An entry ahead of every posted op follows the first posted op's switch.
- The Output/thread agreement check (`setup_post_options::program_problems`) moved into the program gate, since #388 replaced the one-mount refusal it used to ride. It stays program-wide, which is stricter than #388's per-mount files. Relaxing it to "agree within each mount's file" is possible later.
