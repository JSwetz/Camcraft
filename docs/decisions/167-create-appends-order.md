<!-- moved from RECOMMENDED.md for PR #233 (docs/decisions rule, PR #250) -->
## #167 - a created operation appends to its setup's program order (2026-11-07)

- **Chosen:** `CreateOperation` computes the op's position at command
  creation the way `PasteOperation` did - pin every sibling of the
  same effective setup at its current `operation_order` index (0..n)
  and give the new op `n` - shared as `append_position`. Undo restores
  each sibling's previous `OperationMeta` (an older project's `None`
  comes back as `None`), so the command is exactly invertible.
- **Why:** the post, the tree and the prior laws all walk
  `setup::operation_order`; an op with no position sorted by name, so
  'Finish' posted before 'Rough' whatever order they were made in.
- **Old projects:** load is untouched - unordered ops keep name order
  until the next op is created in their setup, which pins them at
  their current (name-order) positions; nothing already in a program
  moves.
- **Dropped:** `max(existing order) + 1` without pinning the siblings -
  an unordered sibling sorts after every explicit index (`u32::MAX`),
  so the new op would land before legacy ops. A load-time migration
  pinning every op - rewrites files the user didn't touch.
