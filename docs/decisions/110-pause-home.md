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

## #110 - review fixes: suppression against the program ahead, afterModes (2026-11-08)

- **Suppression ahead of a dependent refuses (review B1).** The
  "considered and dropped" above is reversed. Re-machining and the
  generation-time rapid verdicts walk `setup::operation_order` with no
  suppressed filter, so a later op is always generated against every
  earlier same-setup cut - suppressed ones included. The post cannot
  know the part is already cut that far, and the program sim does not
  judge rapids through stock, so posting without that cut is the
  silent wrong answer. **Chosen:** a clause in
  `posting::verification_report` (`posting_suppressed.rs`): for each
  posted op whose verdicts read the program ahead
  (`operation::reads_prior` - un-posed ops, Pocket3D, Drill), every
  suppressed op ahead of it in its setup that cut something (non-empty
  path, not a probe; `rest::suppressed_cuts_ahead`) refuses by name:
  "operation Rest2 was generated against the cut of suppressed Rough1;
  unsuppress Rough1 or suppress Rest2 too". The per-op post runs the
  same gate. Suppressing the last op of a setup, an op another setup
  follows, or one only posed ops follow still posts.
  **Considered and dropped:** a per-post opt-in that keeps the
  debugging case (the review's alternative) - a header line would be
  the only guard on a program that rapids through steel; the user can
  suppress the dependents too, and a rerun of one later op is what
  "one file per operation" (#111) already gives. Regenerating the
  dependents against the posted list instead (a suppressed filter in
  `ahead_of`) - would make suppression change geometry and stale every
  op after it, the opposite of what the issue asks of a toggle.
- **`afterModes = true` (review B2).** The `coolantModes` pattern
  (#150): the six shipped posts that write `afterOp()` declare it; for
  a script without it the engine refuses any `PARAM AfterOperation`
  by operation name ("operation X: asks a program stop after it, but
  this post script does not declare afterModes ..."), a counted
  warning that holds the post. Tormach (added on main after this
  branch) does not write the after-action yet, so it refuses the ask
  rather than running on past it.
- **Review notes taken:** Fanuc drops TCPM (`G49`) before a Go home
  reference return when G43.4 is armed; the Heidenhain home refusal and
  every post's unknown-ask refusal name the operation. The double home
  on a last-op Go home (afterOp, then onClose) is left - harmless.
- **Merge with main:** main's `PostOptions` (#34, threading) and this
  branch's were one idea twice; one singleton now carries
  `{ threading, skip_stops, per_op }`, one `SetPostOptions` command.
  The file keeps `skip_stops` / `per_op_files` as top-level keys
  (already shipped in this PR's format), `post_options.threading`
  where main put it.
