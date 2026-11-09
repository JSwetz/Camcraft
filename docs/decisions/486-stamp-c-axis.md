## #486 - the box fixture laws under the spindle C

**Reproduced first.** Both readings the issue suspected missed real
hits on `main`:

- *Fixed steel* (a steady, a tailstock, the table) under a C turn was
  read in the PART frame: `workholding::judge` hands fixed boxes to the
  machine-frame sweep, which cannot stand a horizontal pose upright about
  a vertical line (`upright_turn` -> Unjudged), so the part-frame verdict
  stood. The machine's tool never leaves the +X station, so steel there
  is struck at every C - the part-frame reading saw it only near C 0.
  Pinned in `crates/sim/tests/c_axis_fixture.rs` (a quarter turn
  descending past a block at the station, feed and rapid) and end to end
  in `crates/app/tests/c_axis_fixture.rs` (a generated Rotary Contour on
  a mill-turn C, a 2 mm steady declared fixed at the station: the cut on
  the far side of the bar, C ~180, was silent at generation and at the
  post gate).
- *Steel riding the part* (a chuck jaw) was read at the wrong pose:
  `cl::stamp_c` gave every piece of a split move the WHOLE move's axis,
  so mid-turn the stack was judged along the end pose (+Y on a quarter
  turn) while the machine's radial tool stood on the diagonal. A jaw on
  the diagonal at the holder's radius passed silent.

So the issue was a real miss (P0 by its own terms).

**Decided:**

1. `cl::stamp_c` states on every piece the station pose at that piece's
   own C - the move's axis turned about Z by the turn left to the move's
   end; the last piece keeps the move's axis bit for bit. Every reader of
   the pose (stock sim, contact and solid laws, the box laws) now sees
   the radial tool where the machine holds it. The post is unchanged: a
   C move posts its C and no A/B, so the stream and the helix golden are
   byte-identical.
2. The workholding laws read a C move's pose from its C, not its stated
   axis (`c_steel::station_poses`: `Rz(-C) x`). The post never writes the
   axis of a C move, so the C is the truth the machine runs; this also
   keeps paths cached before this fix (whole-move axes) honest at the
   gate without a regenerate.
3. Fixed steel under C is also read in the MACHINE frame
   (`c_steel::fixed_hits`): the tip runs straight from `(c + rho0, z0)`
   to `(c + rho1, z1)` on the station ray (X, Z, C linear, Y on the
   line), the pose +X throughout - one constant-pose posed sweep, exact
   up to the sweep's own enclosure. This reading only ADDS hits to the
   part-frame verdict.
4. The spindle line is recovered from the path (two stamped tips whose
   C differ by more than ~5.7 deg stand on two rays from it), else the
   op's stated rotary axis where it crosses z = 0 (the stamp's own
   placement), and is used only when EVERY stamped tip stands on its ray
   `-C` within 1e-4 mm. No line (or an arc / non-finite C move) fails
   closed: the C move reports a fixture hit against fixed steel.

**Why add-only for fixed steel (not replace):** the machine-frame
reading is the true one, and the part-frame reading of fixed steel
under C over-reports (it swings the steel round with the part). But
dropping those hits would loosen verdicts beyond this issue's scope,
and #339's fixture mesh (slice 3) already reads C moves at the station
for fixed steel to ACQUIT box hits - the over-report has its owner.
Kept verdicts: every existing suite's verdict stands (none changed);
the only new verdicts are the newly caught hits above.

**Why recover the line from the path first:** the C words and the tips
are what the post turns into X/C - the post itself refuses a tip off
its station by 0.005 mm - so the line they agree on is the machine's.
The op's fitted axis is the fallback for a path that never turns C
enough to place it. A line the tips disagree with is no frame, and a
guess would be a silent miss.

**Considered and dropped:**
- *Only the stamp fix:* convicts the chuck-jaw case, but fixed steel
  stays in the wrong frame - the quarter-turn repro still missed the
  mid-turn piece with the per-piece poses in place.
- *Only reading the C in the laws, leaving the stamp's axes:* fixes the
  box laws but leaves every other pose reader (stock sim, contact/solid
  laws) on the whole-move axis. Both are cheap; both done.
- *Fail closed on every C move with fixed steel when the line is
  unknown, even with the op's axis:* the axis, checked against every
  tip, is as good as the path's own recovery; refusing there would
  block every C program on a machine with a table.
- *Recovering the line per move (slice 3's way):* a move that does not
  turn C (a radial plunge at a held C) places no line, and that is the
  simplest fixed-steel crash there is; one line per path covers it.

**Changed verdicts:** none in the existing suites (cl, sim, and the
app c_axis_live / rotary_fixtures / fixtures / fixture_law_gaps /
rotary_verify / rotary_letter / carried_fixtures_cache suites keep
theirs). New hits only where the repro tests place steel.

**Cost to change later:** one module (`sim::verify::c_steel`) behind
two calls in `workholding`; the stamp change is local to `split`.
Turning the add-only reading into a replace means letting the
machine-frame verdict overwrite the part-frame one for C moves in
`workholding` (as `judge` already does for A/B), once the mesh law's
acquittal is merged and trusted.
