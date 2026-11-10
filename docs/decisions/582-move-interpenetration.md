## #582 - a Model-page move or turn warns when the part or its stock would sink into a fixture

- **Chosen: one shared check, `crate::fixture_clash`.** Locate's sampled
  test (facet centres pushed 0.05 mm into their own body, tested inside
  the other closed mesh, both ways) moved out of `fixture_locate` into
  its own module with the by-name messages; locate now calls it, word
  for word as before. `MoveModel::new` runs it over every fixture of
  every setup (the part read through each setup's alignment, #326; the
  stock that follows the move, re-fitted or rigid, judged in the setup
  whose stock it is) and carries `warning()`. Dropped: copying locate's
  check into `move_model.rs` (two laws to keep in step).
- **A warning, never a refusal**, as locate: a vise is often modelled
  closed on a nominal part, and the gate still judges every toolpath
  against the steel. The UI appends it to the status line exactly as
  locate does (`moved part - warning: ...`, one shared `with_warning`).
- **No fixtures, no check.** A setup without tagged workholding builds
  no mesh and reads no facets.
- **`MoveModel::locating` / `MoveFixture::locating` run no check.** A
  locate already runs its own, worded for mates ("check the mates and
  the offset"); running both would build the meshes twice and could put
  two near-identical warnings on one status line.
- **`MoveFixture` warns too.** Same helper (`after_fixture_move`): the
  moved fixture against every part of its setup and the setup's stock;
  the Fixtures page status line and the MCP `move_fixture` result carry
  it. Cheap to add once the check was shared, and the same hazard.
- **Warns on the result, not on the change.** A part already drawn
  inside a closed vise warns on every nudge, as locate does. Considered
  and dropped: warning only when the move newly enters the steel - it
  would hide a placement that is still wrong. Cost to change later: one
  extra check of the before-pose in `after_part_move`.
- **Only stock that moves is judged.** A stock left where it was cannot
  newly sink by this move; judging it would warn about state the move
  did not make.
- **Runs on every `MoveModel::new`, unthrottled.** Each Model-page nudge
  builds the part's and the following stock's placed facets and samples
  them against every fixture mesh. It is cheap at vise-and-part sizes,
  so nothing caches or throttles it now. Considered and dropped for
  now: debouncing a drag, or caching the placed fixture meshes. Either
  can be added later in `after_part_move` without changing the
  command's contract.
- **A part without facets still has its stock judged.** If a part's
  facets cannot be read, the part check for that setup is skipped, but
  its stock is still checked and warnings already found in earlier
  setups are kept. Dropped: returning `None`, which threw away warnings
  already gathered (review on PR #597).
