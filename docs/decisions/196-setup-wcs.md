## #196 - pick G54-G59 per setup

- **Chosen:** `SetupDef.work_offset: Option<usize>` (a `WCS_SLOTS`
  index). `None` is "Auto": the inferred per-operation law every older
  document posted with (the first slot an op's heights reference, else
  G54). `Some(i)` makes every operation of the setup post at
  `WCS_SLOTS[i]`, measured from that slot's placement in the setup. The
  one law is `posting::wcs_slot::wcs_slot(world, setup, params)`, read by
  `wcs_context` (generation, stamp, spindle law, machine travel check,
  the post) and by the #315 rotation clause.
- **Default Auto, not a stored G54:** new setups also start unpicked, so
  every existing fixture, golden and test that references a slot through
  its heights keeps posting byte-identically; a stored G54 would have
  silently re-routed ops whose heights say G55. Dropped: a hard G54
  default on every new setup.
- **Command:** `SetSetupWorkOffset` (validated at construction: setup
  exists, slot < 6, not the current value; apply/invert write the
  snapshotted values). A datum transfer (`CreateSetup` with
  `datum_from`) copies the source's pick - the slot is part of the datum
  being cloned.
- **Height on another slot than the pick -> refused by name** in the
  verification gate ("Face1: its heights are measured from G56 but setup
  Setup1 posts at G55 - measure them from G55 or pick G56 for the
  setup"). Dropped: letting the op's height reference override the pick
  (the #308 note's suggestion) - that would make one setup post two words
  without the user choosing it, and the pick would lie on the sheet.
  Dropped: letting the pick silently win - an op that used to post at G56
  would move offsets unasked.
- **Two setups, one word, different placements -> one-file post refused
  by name** (`post_script_body`, so the single file and the script
  path): the controller holds one G54 register, and the stream has no
  setup-change stop, so the second setup would run in the first one's
  zero. This also covers documents that pick nothing (Setup2's private
  G54 placed elsewhere), which posted that silently before. Per-op files
  are exempt (each file is one setup's, its offset set for it), so the
  clause is not in `verification_report` (which the per-op post reads).
  The fix the message offers is exactly this feature: pick another
  offset for the later setup.
- **File format:** v51, `"work_offset": "G55"` on the setup payload,
  omitted when unpicked; v50 steps up by the stamp (absent = Auto). Bumped
  rather than a silent optional key because an older reader would post a
  picked G55 setup at G54 (the v50 precedent). An unspellable word is a
  corrupt file.
- **Sheet:** the WCS OFFSETS section ends with `program offset: G55
  (picked for this setup)` or `program offset: per operation - its
  height references, else G54 (not picked)`.
- **UI:** a "Work offset" combo under each setup row on the Setups page
  (Auto + G54..G59), queuing the command. Documents without setups show
  no picker: the first setup has to be created (materialized) first.
- **Not in scope:** heights stay world-resolved (a height referencing
  stock top is the same world Z whatever the pick); the post subtracts
  the picked slot's origin, which is what "heights read against that
  zero" means for the NC (pinned: coordinates shift by exactly the
  offset). Verify/sim already work in world space, so their verdicts are
  unchanged by a pick.
