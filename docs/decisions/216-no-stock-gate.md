<!-- moved from RECOMMENDED.md for PR #242 (docs/decisions rule, PR #250) -->

## #191 - a refused Create click takes you to the fix (2026-11-07)

- **Chosen:** a dark Create cell or dropdown row that is clicked anyway
  keeps its reason on the status line and then routes (view state only,
  nothing queued): no tool / no fitting tool → the Tool library page
  (`UiTarget::Tool("")`, " - opened the tool library"); a kind the mounted
  machine cannot run → the machine door opens on its workspace list
  (" - pick a workspace"). `gate_route` mirrors `gate_note`'s cases in its
  order, so the words and the destination cannot disagree.
- **Why:** the reporter read the status line as an error to hunt for;
  the click now lands where the fix is.
- **Not done:** a WCS route - creating an operation has no WCS gate (an
  unplaced slot means G54 at the origin), so there is nowhere to send
  the user yet. Opening the tool *editor* on a catalog draft instead of
  the library page was dropped: the library shows what is racked and the
  catalog suggestion the reason already names.

## #216 - no stock darkens the Create cells (2026-11-07)

- **Chosen:** every ribbon-made operation machines the active setup's
  stock (`SrcRef::Stock`), so without one the Create cells darken - tip
  "No stock yet - set up stock first (Setup > Stock)" - and a click names
  the same and opens the Stock page (the #191 route). The predicate is the
  command door's own resolution (`setup::stock_entity_name` + `is_stock`).
  Gate order: no tool → wrong machine family → no stock → tool law (stock
  cannot fix a family mismatch, so that is said first).
- **Dropped:** creating the op against "the models" when stock is missing
  (it would silently machine a different source than the one shown).
