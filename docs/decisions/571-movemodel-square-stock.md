## #571 - Turning a part on the Model page keeps a square stock square (2026-11-09)

- **Chosen: one rule for every turn.** Plain `MoveModel` (the Model
  page's position and roll/pitch/yaw) now uses the #561 square-stock rule
  that locate already used: when the move turns the part, a box or
  cylinder stock whose axes lie along the setup's (any quarter turn)
  keeps its own orientation and is re-fitted round the part's new place
  with the allowance it had on each side, measured in the stock's frame
  before the move (`crate::stock_follow::keep_square`). A 30-degree turn
  leaves the blank square to the machine and wrapping the part, instead
  of tilting it. `MoveModel::locating` is now the same move as
  `MoveModel::new` (kept as a named door for the locate).
  Why: the vise clamps the blank square whatever angle the part is
  modelled at; a tilted box made ops (Face, heights "from stock top")
  read a tilted blank's top.
- **Quarter turns refit too, they do not rotate the block.** A square
  stock would stay square either way, so rigid rotation on a quarter
  turn was considered: it keeps the blank's dimensions exactly and
  carries each allowance round with the part's faces. Dropped because
  (1) it makes two rules for one gesture with a seam at 90 degrees - an
  89.99-degree turn refits and a 90-degree one rotates, giving different
  blanks for nearly the same input; (2) on a tip-over (roll/pitch 90) the
  "+top for facing" allowance would land on a side, while the refit keeps
  top on top, which is what the stock dialog's per-side allowances mean;
  (3) a yaw quarter turn of a box that was set round the part refits to
  the same dimensions anyway (the pin shows it: 4 x 4 x 2 stays 4 x 4 x 2,
  moved to wrap the part). Cost to change later: a `turns_by_quarter`
  branch in `MoveModel::new` choosing the rigid path; tests in
  `move_model_stock` / `commands_undo` would name it.
- **A move that does not turn the part is exactly as before.** When the
  part's rotation is unchanged (q and -q alike), the stock takes the old
  rigid carry, bytes for bytes - no refit and no mesh rebuild (the refit
  would agree up to rounding; the rigid path keeps translation-only
  output identical to shipped behaviour). This also applies to a locate
  that only slides the part.
- **Unchanged rigid follow:** a stock copied from the model
  (`FromModel`) and a box/cylinder already off the setup's axes (turned
  with the part before) still turn with the part. A stock that does not
  follow at all (a multi-part document's parametric stock) is untouched,
  as before.
- **Refusal wording:** a refit that cannot keep its allowance (the stock
  did not wrap the part) is still refused by name; the message now says
  "round the turned part" (it said "located part", which is wrong for a
  Model-page turn).
- **Pins:** `commands_undo::moving_one_part_carries_existing_stock_and_undo_restores_both`
  keeps its intent (the stock follows the part, undo/redo restore both)
  but now checks the block stays square and wraps the quarter-turned part
  with its allowances; new `move_model_stock` covers a 30-degree turn with
  a box and with a cylinder, translation-only rigid carry, the off-axis
  and copied-stock rigid follow, and exact undo/redo of shape, place and
  facets.
