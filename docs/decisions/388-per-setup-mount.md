## #388 - Setups on different machines/posts: each op on its own mount, one NC file per mount

**What was decided.** #343 gave each setup its own machine and post but
kept one mounted machine (the active setup's) and refused work that
crossed mounts. Now every law that judges an EXISTING operation reads that
operation's own setup's mount; only the surfaces that act on the active
setup (the ribbon, the Setup pane, the new-op door) read the active one.
The two #343 refusals are retired:

- generation: `setup_mount::generation_problem` ("activate 'SetupN'
  first") is gone - an inactive setup's op generates against its own
  machine without activating it. No replacement refusal is needed: a
  setup's machine is parsed when it is mounted or the document is opened
  (`SetSetupMachine`, `mount_dto::to_def` refuse a bad `.mch` by name
  there), so at generation every setup's machine is already in hand.
- posting: `setup_mount::program_problems` ("one program is written by one
  post for one machine") is gone from the gate. A program whose setups
  span several mounts posts one file per mount (`posting::post_mounts`):
  setups sharing one machine AND one post share a file (program order of
  first use); each file is that mount's ops' streams, joined exactly as
  the single-file post joins them, run through its own post with its own
  machine - byte-identical to posting that setup alone (pinned).

**Inventory of mount readers** (before -> after):

| Reader | Context | Now reads |
|---|---|---|
| `operation::job::collect_job` (`machine_kind`, `is_mill`, spindle housing, rotaries) | op | `machine::kind_of_setup` / `of_setup` of the op's setup |
| `setup_mount::generation_problem` (job door) | op | retired |
| `operation::spindle_law::turns` (job, post_problem, stamp) | op | op's setup (`turns(world, setup, params)`) |
| `rest::prior_plan` lathe column-grid rule | op | consumer op's setup |
| `setup_align::generation_problem` | op | op's setup |
| `stock_sim::carry::carry_turn` (mill-only carry) | two setups | BOTH ends must be mills |
| `fixture::table_box` / `boxes_for` / `workholding_for` / `list` | setup | the declared setup's machine (`table_box(world, declared)`) |
| `posting::fixture_problems` spindle housing | op | op's setup's machine |
| `machine::check_document` in the gate | program | `posting::machine_checks`: one check per machine over the ops its setups run (`machine::check_ops`); the gate words each block as before |
| `Machine::inverted_post_refusal` in the gate | program | per machine of the program |
| `setup_mount::program_problems` | program | retired |
| `posting::post_setup` / `post_script` (one file) | program | the program's ONE mount (`mounts::single`), not the active one; a program over several refuses by name and points at the per-mount post |
| `posting::post_per_op` (per-op files) | op | each op through its own setup's post and machine |
| `cam.post` / MCP `post` / CLI `process -o` | program | `post_mounts` / `post_mounts_script`; one file answers as before, several add `files` and `mounts` |
| UI Post command | program | `post_mounts`; several files: status line names each, viewer shows the first |
| UI machine vote (status line) | program | sums `machine_checks` |
| `commands::operation::gates::family_gate` | op | Create: active setup (the new op joins it); Paste: the op's setup; ChangeOperationKind: the op's setup |
| UI tree menu kind rows | op / new op | an op's retype rows: its setup; the setups' "new op" rows: active |
| UI drill radial holes (`state.rs`) | op | op's setup |
| MCP `get_fixture_findings` | setup | each `workholding` entry carries its own setup's `machine` (`of_setup`); the top-level `machine` stays the active setup's, kept for older callers (the findings themselves are per op already). Considered: making the top-level field a list - dropped, it breaks callers for no gain over the per-entry field |
| `cutting::...` power estimate, `setup_sheet` machine axes | op / setup | already `of_setup` (#343) |

**Left on the active mount, and why.** These act on the active setup by
definition, or have no op/setup context:

- `machine::of` / `kind_of` / `posting::post_of` / `post_label` /
  `post_offer` / `script_source` / `machine_value` - the ribbon and pane
  accessors (the Setup pane, Machine/Post pages, machine editor, debug
  info, `set_machine_mode`), and new-op gating: a new op joins the active
  setup, so the active machine IS its machine.
- `ui spindle_row` (the tool's speed row hint: a tool is shared by ops in
  any setup; the hint names the active machine's ceiling - display only).
- `ui shell::sim_script` traverse clock (the playback timeline is one
  clock over the whole program; display only, judges nothing). Per-op
  traverse rates would need `sim::script` to take one per path - a
  follow-up if timelines across machines need exact rapids.
- `ui post_options` lathe options visibility (post options are document
  level until #389).

**Why.** A law that judged an inactive setup's op against the active
setup's steel would be the silent wrong answer the Prime Directive
forbids; reading the op's own setup makes activation irrelevant to every
verdict (and to every stamp - `spindle_law::turns` in the stamp no longer
flips with the active setup). One file per mount is what a shop does when
a job moves from a mill to a lathe: two programs, two controls.

**Considered and dropped.**
- One file per SETUP always (simpler naming): rejected - setups that share
  a machine and post (re-clamps on one mill) belong in one program, as they
  did before, and a one-mount program must stay byte-identical.
- Making `post_setup` write several files: rejected - its callers expect
  one program; it refuses a multi-mount program by name instead (the UI,
  scripts, MCP and CLI all post through `post_mounts`).
- Judging each file with its own gate run (scoped program): rejected -
  the material sim and the carry laws are physical facts about the whole
  job; the gate runs once over the whole program, each machine law per op,
  and each file adds only its own one-register (#196) clause.

**Remainders (conservative, never wrong):**
- The tool-number clash (#221) is still judged over the whole program, so
  two different tools sharing a T number on DIFFERENT machines refuse
  where posting each setup alone would not. Safe (a refusal), but a
  follow-up should judge it per file.
- File naming is `<stem>_<nn>_<setups>.<ext>` beside the chosen path, the
  extension each post's own (as per-op files do).

**Cost to change later.** Low: the grouping lives in
`setup_mount::program_mounts` and the per-file post in
`posting_mounts.rs`; a per-setup split would change only the grouping key.

**Merged with #404 / #470 (2026-11-09).**
- `stock_sim::carry::carry_turn`: main's #404 refuses a carry between
  setups on different machines (`CARRY_MACHINE_CHANGE_REFUSAL`, via
  `one_machine`, which reads each setup's own machine), then gates the
  kind of that one shared machine. That subsumes this PR's "both ends
  must be mills" (one machine, one kind) and is stricter: a carry between
  two DIFFERENT mills, which this PR let through, now refuses. Kept the
  stricter refusal (material never crosses a machine change); a new block
  on another machine is still judged (#404's review fix).
- #470's MCP readers of the machine table moved onto the per-setup
  reader: `get_fixture_findings`' workholding view reads each setup's own
  table (`fixture::table_box(w, setup)`), and `set_setup_machine` reports
  the table of the machine it just named - the setup's own or the
  document's (`fixture::table_of`, split out of `table_box`) - never the
  active setup's.
