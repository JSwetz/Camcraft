## #212 - the scallop law reads to the ball diameter; a crown or an edge is told apart by the nearest feet, one per pass (2026-11-08)

**What was decided.** The ridge flank (`stock_sim::ridge_scan::flank`) now
reads a column past the band within one ball diameter
(`ridge::reach = max(2r, band + 3·tol)`), not `band + 3·tol`. #37 measured
that widening alone convicting 11 finish3d verdict fixtures (cone apex 1.68,
dome waterline 1.10, box walls 0.42, ...). Three rules make "flanked" mean
"between two passes", the way a cusp is:

1. **Equidistant - only the nearest feet count.** A scallop is the ridge
   where two adjacent balls meet, so the column under it stands equally far
   from both passes, up to the lattice's slack (`max(cell, 3·tol)`: half a
   cell off the ridge moves the two distances apart by at most a cell; never
   narrower than M13.8's `3·tol` past the band). A column much nearer one
   pass than the other is where that pass alone decides: a crown or an edge
   bulging past it. Without this rule, 7 release-lane fixtures go red (blind
   pocket 0.16, multidirection rasters / scallop diagonals 0.20, ramp 0.17,
   hill contour/project 0.18, hill radial/spiral and ring families 0.17).
2. **One foot per pass.** The nearest segments group into passes: runs of
   consecutive, joined segments (a rapid's gap breaks a run), merged by
   closed iso-Z ring (M13.9), never one ring into another. Each pass counts
   once, at its nearest point (the classical foot of a point on a curve). A
   pass whose nearest point is a free end of the path (stream start or end,
   a rapid's edge) gives no side: the point stands past where the pass
   stops, not beside it. Without this rule, 3 fixtures go red (cone apex
   2.19, hill contour/project and ring families 1.90), plus unasserted
   readings (the ball-radius law test 3.75, the split's cone 1.54).
   M13.8's per-chord feet made one curved pass a fan that a 120-degree test
   reads as two sides - the box walls' 0.42 was a corner fan (feet 3.44 and
   5.94 away, exactly 120 degrees apart), the cone/dome crowns were rings
   and laps round the apex.
3. **In contact.** A cusp lies between two contact paths: a pass whose
   ball at its foot stands clear of the model by more than the band
   (`ridge::Access::touches`) rides air or skims stock there, and what
   stands under it is the ladder's leftover - rest's ground. Found by the
   app gate, not the finish3d lane:
   `rotary_offset_peels_the_wrap_ladder_and_verifies_clean` (a ROUGH: the rotary Offset ladder on the lathe bar) read 1.157 mm on
   the 6.0 → 5.0 shoulder between two wrap rings riding the stock's r6.2
   skin, 4.38 / 4.09 mm from the shoulder point, their balls ~0.2 / ~1.05
   mm clear of the part - the step the test's own comment says the ladder "honestly leaves
   to the finishing kind". With the rule it is silent again; an air row
   beside a contact row pins it in `a_pass_clear_of_the_part_flanks_nothing`.

The near flank (`Flank`, per-chord feet within `band + 3·tol`) is retired:
one law serves the whole reach. Inside the old window it reads the same
cusp on flat ground (the M13.8 pins are unchanged and green).

**Readings (release lane: finish3d 88/88, flow 15/15, pivot5d 8/8,
deburr 7/7 green; app `verify` and `operations` heavy lanes green).** Every
scallop-asserted fixture reads 0.0000 before and after. One unasserted
reading moved: `morphed_spiral_dome_bites_nothing` 0.1321 → 0 - the
spiral's last turn round the dome apex, one contiguous pass (segments
645-665) that the old per-chord fans read as two sides. Without the
contact rule the step pocket's Flat floors (in
`the_split_leaves_nothing_uncut_on_either_side_of_the_border`) read 0.1948
against an air pass 2 mm over the pocket floor; with it they read their
old 0.1444. Flat raster on an r3 ball, tol 0.1: every spacing
s = 3.0 ... 5.9 now reads its floor point `sqrt(r² + (s/2)²) - r` to the
lattice (s = 4: 0.606; s = 5.9: 1.23); before #212 anything past s ≈ 2.9
read 0. One app assertion moved with the new reach:
`verify::envelope_verdict_follows_the_stated_tol` judged tol-0.1 cone
rings at a 0.01 budget and asserted an empty list; the rings' own cusps
(~0.1, 0.085 at the lattice, on the flank at rho 16.8) are now read there,
past the old 0.04 reach. The test now asserts exactly one non-blocking
`Scallop` note, over the 0.01 gate and under the 0.1 the rings were
laddered to - no envelope verdict, as before.

**Why.** The generator ceiling (#37) stops a stated stepover at `2r`, but
between `4·tol` past the ball and `2r` a too-wide spacing verified silent.
All three rules are the cusp's own geometry (adjacent contact passes, the
ridge where their balls meet), so they excuse crowns and corners without a fixture
list or a tuned constant.

**Considered and dropped.**
- *The cusp-height ray* (the swept balls must close over the point within
  `r` up its normal: a cusp's tip is never taller than `r` while the balls
  still meet). Measured: it excused the cone apex's top columns but closed
  over most false readings (box corner entry 0.53, hill rim 0.23, cone
  flank 2.2) - too weak, and redundant once the two rules above held.
- *Pass identity by staying inside the reach* (a run is one pass while the
  path stays within `2r`), and *by path length* (one pass while the path
  between the feet is under `PI` chords): both split an off-centre helix
  lap round a crown into two passes (its far side 6.2 mm away; its long
  side 3.17 chords) - the nearest-point rule has no threshold to miss.
- *Excusing convex edges outright*: #46 treated a shelf's last line
  stopping short of its rim as a real gap (the rim row); no fixture asks
  for it, so two contact passes across an edge still read.
- *Silencing the scallop note for rough kinds* (the rotary Offset case
  above): needs the op kind in `VerifyContext`; the contact rule fixes
  the case on geometry alone, and a rough whose contact rows leave a wide
  cusp is still read, honestly, as a non-blocking note.
- *Keeping the M13.8 near flank and adding a far tier on top*: the near
  flank's per-chord fans already read false crowns (a helix's seam: 0.23 in
  `a_crown_above_the_top_lap_stays_rest_not_scallop`, the morphed spiral's
  0.13), so a second law beside it would keep that bug.

**Cost to change.** `ridge::reach` and the `slack` line in
`ridge_scan::ridge_rest` are the two numbers; the grouping is one loop in
`flank`, the contact test one filter on its sides. The verifier only judges: no generated toolpath or posted output
changes; the `Scallop` warning stays non-blocking.
