<!-- moved from RECOMMENDED.md for PR #199 (docs/decisions rule, PR #250) -->
## #122 - Lathe Facing faces the stock's allowance, not only the part (2026-11-07)

- **Chosen:** `turning::facing` takes the setup's stock facets
  (`Option<&[Triangle]>`) beside the part's. The spindle axis is still
  fitted to the part alone; the stock is read in that frame, and its
  end and radius widen the ladder's top (`z_top`), every rung's band
  (the reach sweep runs over part and stock triangles) and the rapids
  radius. A part at its finished length under a longer bar now faces
  the allowance down to the part's end.
- **Why:** turning a bar to length is the first op of most lathe jobs,
  and the allowance lives in the stock, not the model.
- **Safety:** the passes still stand on the facing plane's tangency
  (the no-gouge construction is untouched); stock only adds rungs
  above the part and pushes pass starts and rapids further out, which
  can only cut more air, never into the part.
- **Not covered here:** posting the whole face-then-drill program on a
  lathe is still refused by the footprint rapid law judging the inward
  rapids (#159, same refusal on main); the pin asserts the facing and
  drill verdicts and should gain the post line once #159 lands.
- **Dropped:** fitting the axis to part + stock (a box or offset stock
  would lean the spindle); a separate "stock top" height parameter
  (the stock already says it).
