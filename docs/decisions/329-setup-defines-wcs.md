## #329 - the setup defines the work offset, not the operation's heights

- **Chosen: `HeightReference::WorkOffset` is a unit variant** - it names
  no slot. Resolving it reads `setup::wcs_slot(world, setup)` (the op's
  setup pick, else G54), the same law the post reads, so an operation
  cannot disagree with its setup by construction. Dropped: keeping the
  `usize` field and ignoring it on read - a field that is stored, shown
  in `Debug` and compared but means nothing invites the next reader to
  trust it, and every construction site would keep passing a slot.
- **The law moved to `setup::wcs_slot(world, setup)`**; the posting
  module's `wcs_slot` delegates and `wcs_context` lost its `params`
  argument (heights no longer feed the slot). The #196 "height on
  another slot than the pick" post refusal (`slot_problems`) is gone:
  that conflict cannot be built any more. The one-file shared-word
  refusal stays.
- **Unpicked = G54.** The Setups page's "Auto (from heights, else G54)"
  entry is now "G54 (default)"; the stored `None` keeps its byte shape
  and every G54 document is unchanged. The setup sheet says "program
  offset: G54 (the default - not picked)".
- **Stamp:** a work-offset height digests tag 5 plus the RESOLVED setup
  slot, so a migrated op's stamp is bit-identical to its v51 stamp (no
  forced regeneration, goldens unchanged), and picking another offset
  stales exactly the ops whose heights moved.
- **Project v52.** The wire keeps one tag, `{"work_offset": {...}}`, with
  an optional `slot` the writer never emits (`{"work_offset": {}}`). The
  v51 -> v52 rung (`project/wcs_migrate.rs`) reads the old slots from the
  operation DTOs' JSON before any world write:
  - per setup, the slots in use are its pick, each op's v51 posted slot
    (pick, else first height slot, else G54) and every height's slot -
    suppressed ops included, since their heights still generate;
  - one slot: it becomes the setup's `work_offset` (G54 left unwritten);
    posting and the stamp are unchanged;
  - several: **the load is refused by name** (`ProjectError::
    WorkOffsetConflict`), each setup listed with every op and the slots
    it used, e.g. `setup Setup1: Face1 (G55); Face2 (G56)`.
  - documents without setups (or an op naming no setup row) have nowhere
    to keep a pick: all-G54 loads, anything else is refused by name
    ("no setup to carry G55 (operations: ...)").
  - a v52 file carrying a slot was hand-edited: refused as corrupt.
- **Why refuse the load rather than load and refuse the post:** loading
  would need the per-height slots kept in memory (the very model this
  issue removes) or would re-point the op the moment it is regenerated.
  A refused load changes nothing and names exactly what to fix; the
  release that wrote the file can still open it, make the setup's
  operations agree (or add a setup), and save. Dropped: picking the
  majority slot (a silent re-point for the minority), and auto-splitting
  a setup per slot (invents setups and reorders the program).
- **Heights page:** the Work offset reference shows a read-only
  "Work offset" row - `G55 (setup Setup1 - set on the Setups page)` -
  with a hover hint, no combo. Its Display chip is "work offset" (it has
  no world to name the setup's word).
