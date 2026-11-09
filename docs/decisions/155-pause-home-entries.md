## #155 - Standalone Pause and Home entries in the operation list

**Decided: an entry is its own entity kind, not an operation.** A
`ProgramEntry` component (action, setup, suppressed) plus the
`OperationMeta` position its siblings already use. Because it carries no
`Operation`, `setup::operation_order` - and every walk built on it (the
post's tool and freshness gate, `rest::ahead_of`, the program sim, the
setup sheet, the tool list, verify, `RegenerateAll`) - never sees one: the
carve-out is by construction, not a check in each walk. Only two walks
know entries exist, both in `app::program_entry`: `sequence` (list order:
ops and entries, the op order's own sort) and `posted_afters` (the post).
Dropped: a new `OpParams` variant (every `match` on the kinds, every
tool/toolpath walk to carve by hand - the cost #110 deferred on).

**Decided: an entry posts as the after-action of the posted operation
before it.** `posted_afters` folds each entry into the gap it stands in,
and `op_streams` writes that gap's `PARAM AfterOperation` word - the
per-op choice's exact word, so a Pause entry after Face1 posts
byte-identically to Face1 set to Program stop (pinned for all three
actions, Fanuc and Haas). No post script changed; a document without
entries streams exactly as before (all existing goldens untouched).
Consequences, each refused by name rather than guessed:
- an entry before every posted operation (nothing to stop after) refuses
  the post - suppress it or move it;
- a stop and a trip home asked of the same gap (op's own choice plus an
  entry, or two entries) refuse: the stream carries one word per gap.
  Equal asks merge; a program stop absorbs an optional stop (M0 stops
  whenever M1 would). Dropped: a compound stream word (`home stop`) -
  it would need every afterModes script taught to split it; a follow-up
  if anyone wants Home-then-Pause in one gap.
- an entry after a suppressed op joins the last POSTED op's gap; an entry
  at a setup's head joins the previous setup's last op (one program).
- Tormach's post does not read `AfterOperation` (#110), so an entry there
  refuses exactly as the per-op choice does.
Suppress and "Skip pauses and homes" mute entries like op actions; an
entry never holds the post as "not generated" (it has no status).

**Decided: commands.** `AddProgramEntry` (after a named row, or at the
active setup's end; pins every row of that list at 0..n so undo puts
back exactly what it found) and `EditProgramEntry` (action / suppressed).
Reorder reuses `ReorderOperation` and delete reuses `DeleteEntity` (Del
and the menu); both, plus `CreateOperation`'s append and `DeleteSetup`'s
refusal, now count positions over `program_entry::sequence`, which on a
document without entries is `operation_order` itself.

**Decided: project file, no version rung.** An entry saves as an entity
row `"kind": "entry"` with an `entry` payload (`action`, `order`,
`setup`, `suppressed`). A document without entries writes neither key,
so its bytes are unchanged and PROJECT_VERSION stays put (no clash with
the open PRs bumping it). A pre-#155 reader meets the unknown kind word
and refuses the file by name (serde "unknown variant `entry`") - never a
silent load without the stops.

**Decided: UI.** Every operation-list menu (an op row, an entry row, a
setup's Operations row) carries "Add pause (M0) after", "Add go home
after", "Add optional stop (M1) after" - placed after the op verbs so
every existing row keeps its place. An entry row (post glyph, its action
and SUPPRESSED in the suffix) has its own menu: step its action, suppress,
delete, add after. Entries drag like op rows. The setup sheet and the
tool list do not list entries (they are tool / operation documents).
