## #110 - pause and home after an operation, suppression, skip switch (2026-11-07)

- **Chosen:** a per-operation `after` field on `Operation` (the post
  crate's `AfterOperation`: Continue / Optional stop / Program stop /
  Go home), default **Continue**, so the stream says nothing and every
  golden stays byte-exact. Any other choice rides one
  `PARAM AfterOperation optional-stop|stop|home` ahead of the section
  (the #112 coolant pattern); each post writes it in `afterOp()` right
  after the section's spindle stop, then resets the ask.
- **Dialect words:** Fanuc / Haas / Mazak `M1` / `M0`, home
  `G91 G28 Z0` then `G28 X0 Y0` then `G90` (Z straight up first, back to
  absolute before the next section - the onClose sequence plus the G90 a
  mid-program home needs). Siemens 840D `M1` / `M0`, home `G0 G53 Z0`
  then `G0 G53 X0 Y0` (machine coordinates, its onClose law). Fanuc Lathe
  `M01` / `M00`, home `G28 U0` then `G28 W0` - X clear of the part first,
  two blocks rather than one diagonal `G28 U0 W0`. Heidenhain `M1` /
  `M0`; **home refuses by name** - #50 removed its homing because G53 is
  a datum shift in DIN/ISO mode and M91 rides a builder-set datum, so a
  guessed home move would be the silent wrong answer.
- **Suppressed:** a per-op `suppressed` bool, toggled from the operation
  list's context menu ("Suppress operation" / "Unsuppress operation"),
  one undo unit through `EditOperationParams::with_suppressed`; the stamp
  and the toolpath stay. `posting::program_ops` drops suppressed ops
  BEFORE its freshness law (a stale op the user left out never holds the
  post), so the program sim and the machine-limit check - which walk the
  same list - judge exactly what posts. All ops suppressed refuses by
  name. The tree row says `SUPPRESSED` / `then <action>`.
  Considered and dropped: refusing the post when a suppressed op sits
  ahead of a later op in the same setup (rest material of the later op
  assumed the suppressed op's cut). The debugging use the issue names is
  exactly that - the part is already cut that far - so the choice stays
  the user's; the program sim still judges the posted list.
- **Skip pauses and homes:** a document singleton `PostOptions
  { skip_stops }`, switched by the `SetPostOptions` command (undoable)
  from a checkbox under the Post picker on the setup page, saved as a
  top-level `skip_stops` key written only when on. The ops keep their
  choices; the stream just carries no `AfterOperation` word.
- **Menu, not page row:** the after-action lives in the op's context
  menu ("After this operation" rows) rather than an op-page row, so the
  shell snapshots do not move; a page row can follow.
- **Owed:** standalone "Pause" / "Home" entries in the operation list
  (entities with no toolpath) - every program walk (`program_ops`, rest,
  sim, setup sheet, tool list) assumes an op has a tool and a path, so a
  toolpath-less entry is a larger change than the timebox; the per-op
  after-action covers the same NC. Filed as #155.
