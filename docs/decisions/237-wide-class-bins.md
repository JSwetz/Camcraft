<!-- moved from RECOMMENDED.md for PR #247 (docs/decisions rule, PR #250) -->

## #237 - flats hiding in a fine ball's wide latitude class (2026-11-07, P0)

- **Chosen:** inside each WIDE latitude class of a pencil (a fine dome's
  continuous latitudes, which `ringless` leaves to `off_cone`), cut the
  class into 0.02-wide latitude bins and read each bin's bearings: the
  area-weighted resultant of the 1st to 4th bearing harmonics. A revolve's
  bin cancels them; a rod's two opposite faces keep nearly all of the
  2nd, a flat bar's sides and end caps (a quarter turn apart) the 4th. A
  bin past 0.5 counts its area times that fold; a pencil whose folded
  area passes 2 % of the inliers does not bear the axis out (refused).
- **Why the 2 % (not the 10 % bias share):** the 6x6x80 rod's faces are
  ~7 % of the inliers - under the general bias budget, yet a rod sticking
  50 mm out of a ball is never a revolve about a cross axis. The fold
  measure only reads wide classes, which a turned part's narrow rings
  never are, so the tighter share cannot touch the ring laws.
- **Known consequence (fail closed):** a square section inside a dome,
  judged about its true axis, is refused too (4th harmonic); a hex (6th)
  is not. Refusal is the safe side.
- **Dropped:** taking large flat faces out of the wide class and running
  them through the ring check (they then only cost their area against
  the 10 % budget - the 6x6 rod still read X).
