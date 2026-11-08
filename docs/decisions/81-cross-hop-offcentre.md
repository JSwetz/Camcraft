<!-- moved from RECOMMENDED.md for PR #209 (docs/decisions rule, PR #250) -->

## #81 - the cross-drill station hop measures its chord (2026-11-07)

- **Chosen:** `traverse_s` takes the two holes, not their directions. It
  keeps the closed form `hop_s * sqrt(2 / (1 + cos dtheta))` whenever
  that plane already clears (every on-centre pair - existing chains do
  not move by a bit), and otherwise measures the chord between the two
  real hop points (each on its own hole's column) and grows the plane
  until the chord's closest approach to the spindle reaches `hop_s`,
  then bisects back to the smallest clear plane.
- **Why:** off-centre mouths move the columns; the closed form assumed
  them on the station axes, so an (8,-6)/(-6,8) pair on an r10 bar
  chorded 10.757 mm from the spindle and #77's rapid proof refused it.
- **Dropped:** a fixed extra margin for off-centre holes (over-lifts
  most pairs and can still be short for a far-off mouth); refusing
  off-centre pairs outright (they are ordinary cross holes).
