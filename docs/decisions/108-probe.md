<!-- moved from RECOMMENDED.md by PR #178 (docs/decisions rule, PR #250) -->

## #108 - Probe: face, edge, corner and bore / boss centre finding (2026-11-07)

- **Chosen:** one new kind, `Probe`, APPENDED as kind id 38 (review,
  2026-11-07): stored kind ids, digest tags and ribbon pins never move,
  so there is no project-format rung (files stay v50 - the probe and the
  probe tool are additive words no older file contains) and no op goes
  Stale on load. The menu lists it in the Drilling section by display
  order: `Section::kinds` reads a section's rows in table order, and the
  ribbon laws now ask for that order instead of a contiguous run. The
  first draft seated it at 13 and slid every row from Pocket3D up - a
  v51 pin migration and a regenerate-everything for nothing - dropped.
  A new tool kind `ToolKind::Probe`: `diameter`
  is the stylus ball, `flute_len` the stylus length, corner = ball
  radius by law. The starter catalog gains one, `PRB-0400` (4 mm ball,
  30 mm stylus), so the empty-rack refusal names a real entry for the
  Probe kind like every other (the M13.4 law).
- **Modes:** face (one touch, +-X / +-Y / -Z), edge (side face under a
  picked edge, `depth` below it, X or Y), outside corner (an X and a Y
  touch, each `inset` from the vertex), bore centre (four touches out
  from the centre), boss centre (four touches in from outside). The op
  stores the expected contact (`target`) and every number has a default
  (approach 5, probe distance 5, back-off 1, seek 300, measure 30 mm/min,
  depth 3, inset 5, tolerance 0.5, set G54).
- **Motion stream:** not a new `MoveKind` (68 exhaustive match sites
  across seven crates). The moves stay the nominal truth - the stylus
  runs to the EXPECTED trip - and `Toolpath::probes` (`cl::ProbeTouch`,
  the `CompSpan` precedent) marks which feeds are probing moves, their
  probe target (trip + overtravel) and the measurement ordinal. The
  path cache files the touches (`PathDto.probes`); a touch that does not
  sit on its move is dropped on load and the dump refuses a probe op
  with no touches - a probing move must never post as G1.
- **Post:** three stream records - `TOUCH` (probe toward the target),
  `RETREAT` (the back-off after a trip, INCREMENTAL so a part nearer than
  the model is never driven into by an absolute back-off) and `FOUND`
  (the axes the measured trips find, the mean of opposite touches for a
  centre, plus the action and tolerance). Hooks `onProbeMove`,
  `onProbeRetreat`, `onProbeResult` have NO prelude default: a script
  without them is refused by name (`probing: operation "Probe1": ...`).
  Only `fanuc.cps` voices them this slice, as the G31 skip function with
  custom macro B (Fanuc's published operator documentation): the skip
  position is read from #5061-#5063 (workpiece coordinates) and kept in
  #100+3k. G31 ends silently at its target when no skip signal fires, so
  every skip block is followed by a no-contact check that alarms
  (`#3000=2`) before any back-off. The deviation and its tolerance stop
  are `IF[ABS[..]GT tol]THEN#3000=1`, the set-offset `G10 L2 Pn` from the
  workpiece offset variables (#5221 + 20 x slot + axis), report-and-stop
  `#3006=1`. The first draft wrote G38.2 (NIST RS274NGC), which a Fanuc
  does not take - dropped: wrong probing code on a real machine is a
  crash. Not yet proved on a control: the golden locks the text only. A touch probe
  section writes no spindle start and no coolant.
- **Verify:** the probe op skips the cutting laws; its own law
  (`operation::probe_op::stylus_hits`) sweeps the ball along every move
  against the model and convicts any move that comes nearer than the
  ball radius (0.01 mm slack for the tangent touch) as a `Gouge`, which
  holds the post. The stock sim and the rest masks skip probe ops: a
  probe removes no stock.
- **Dropped:** a `MoveKind::Probe` (blast radius); G38.2 on the Fanuc
  dialect (above); judging the stylus against the stock (a probe at the
  model's contact on unmachined stock would always "collide" - the
  expectation is the model's); a prelude default that comments the
  probe out (a silently skipped measurement then sets no offset while
  the program runs on).
- **Owed (filed: #171 picking, #172 shank/body, #173 dialect hooks,
  #174 protected descent):** creating the op from a picked
  face / edge / vertex / cylinder (the op page takes the contact point
  typed today); the stylus shank and probe body in the protected-move
  law (the ball alone is judged); probing hooks for the Haas, Mazak,
  Siemens and Heidenhain dialects (they refuse by name now);
  protected positioning moves (G38.3-style) for the approach descent.


## #108 - the probe programs the stylus TIP, not the ball centre (2026-11-08, review of 0beeec4)

- **Chosen:** the probe path states the stylus TIP - the ball centre
  less the ball radius along +Z - exactly as every ball tool in this
  codebase does (the pivot law: centre = tip + r along the axis; the
  3-D finishing generators and `sim::verify::contact` read the move
  point as the tip). `motion::probing::probe_cycle` still plans on the
  ball centre (contact - r along the travel, the edge / corner / bore
  `depth` is still the CENTRE's depth below the feature) and shifts
  every move point and probe target down by r when it lays them down.
  The safe plane is a tip height like every tool's (the ball centre
  rides at clearance + r). The protected-move law
  (`operation::probe_op::stylus_hits`) sweeps the ball about each
  point raised by r, so the judged geometry is unchanged.
- **Why:** a spindle probe's length register is gauged to the ball's
  bottom (the trigger point) like a ball mill's. Posting the centre
  read every -Z face r high: the default face op (tolerance 0.5,
  r = 2) alarmed on every run, and with a looser tolerance `G10 L2`
  moved the work zero r into the part - every following op cut r too
  deep, a gouge the verifier cannot see. XY touches also landed r
  higher than asked, so an edge touch at `depth` 3 met a chamfer.
  After the fix a -Z touch on a face at Z 20 posts `G31 ... Z15`
  (overtravel 5) and expects its trip at Z 20; XY expectations stay
  the spindle axis (contact - r along the travel), which is what
  `#5061`/`#5062` read.
- **Dropped:** (b) keep the centre and state "gauge H to the ball
  centre" in the NC header and the setup sheet - an operator note is
  the weaker guard, and it breaks the one convention every other tool
  follows; correcting only in the post (the pivot law dropped that
  too: the CLFile and the verifier would disagree with the G-code).
- **Pinned by:** `probe_op::the_posted_g31_z_is_the_tip_at_the_expected_contact`
  (face and bore-centre G31 Z against the expected contact) and the
  regenerated `nc/fanuc/probe_bore_centre.nc` golden (every Z exactly
  2 mm lower except the safe plane, which is now the tip at 50).
