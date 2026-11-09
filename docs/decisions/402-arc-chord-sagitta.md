## #402 - the column sim sweeps XY arcs exactly, not as inscribed chords

- **Chosen:** The column engine (`Dexel`, the program sim that the post
  gate reads) sweeps every XY arc along the true circle, for all four
  atoms (disc, ball, cone, bull nose): `stock_sim::dexel_arc`. For each
  column, the reach window on the arc has a closed form
  (`4 R rho sin^2(u/2) <= r^2 - (R - rho)^2`, met by the travelled sweep
  in up to two pieces). The bottom over the window is then exact. A
  disc or a level arc takes its minimum at a window end or at the
  clamped bearing. A helical ball, cone or bull is split where its
  bottom's curvature can change sign, and each piece is searched by
  samples and a golden section that only returns real evaluations. The
  arc's end discs are judged too. XZ and YZ arcs keep their in-plane
  chords. The gouge verdict (`deviate`) is unchanged.
- **Why this is conservative both ways:** The swept set is the set the
  real G2/G3 sweeps, so neither side of the arc is biased.
  - Inscribed chords stand one sagitta nearer the arc's centre. On a
    convex corner (centre = the part's corner) that over-cut toward the
    part. The issue's false 10 mm gouge came from a column 0.088 mm
    inside the riser's corner.
  - On a concave corner (part outside the arc) the same chords stopped
    a sagitta short of the wall, so a real gouge of up to `cell/4`
    (0.094 mm here) was hidden. `arc_sweep.rs`'s concave case fails on
    the chord code for exactly that reason.
  - A helix arc in `stock_bull_nose.rs` showed the field engine (still
    chorded) missing a column the true arc cuts 2.5 mm deep. The
    chords under-cut on the outer side, too.
  - The old module doc said chords "only over-report rest". That was
    true only on the convex arc's outer side.
- **Considered and dropped:**
  - *Circumscribed chords* (vertices at `R / cos(theta/2)`): these fix
    the convex side but under-cut a concave wall by up to the sagitta,
    swapping a false positive for a hidden gouge. No single chord
    placement is honest on both sides, because the part can be on
    either side of an arc.
  - *Chords with tool-radius compensation* (inscribed chords for the
    centre side, circumscribed for the outer side): this is two sweeps
    whose union over-cuts both sides by the sagitta, so the false
    positive comes back.
  - *Judging contact against the exact arc only in the verdict, near
    arcs*: this moves geometry into the verdict and needs the move
    list inside `deviate`. It also leaves the sim's stock (display,
    rest, later ops' prior stock) wrong. It is close to the lateral
    slack of #299 / #303, which is the owner's call and not touched
    here.
  - *Finer chords*: these shrink the error without removing it. The
    lattice phase still decides which corner convicts, and they cost
    more per arc.
- **Behaviour changes:**
  - XY arcs are no longer affected by the chord budget (`chord_tol`) in
    the column engine. `stock_sim.rs`'s old law
    (`arc_chords_under_sweep_so_the_sim_only_over_reports`) is rewritten
    as `xy_arcs_sweep_the_true_arc_whatever_the_chord_budget`.
  - Column and field engines now agree exactly only on straight moves.
    On an arc, the bull parity test checks them against the field's
    chord error (`arc_parity`).
  - Rest material and 3D roughing's prior stock (`machining3d::feed`
    uses `Dexel::subtract`) read the exact arc too.
- **Left open (follow-up):** The field engine (`field_cut::chords`,
  used for display and the lathe/rotary eye) and the generator-time
  envelope scan (`scan.rs`) still ride inscribed XY-arc chords. The
  same two-sided sagitta error applies there, but neither feeds the
  post gate's column verdict. An exact arc sweep for the field's SDF
  sweeps is a separate piece of work and should get its own issue.
- **Cost:**
  - Level arcs and every disc arc cost one closed-form window per
    column (cheaper than about 20 chords each).
  - Helical ball, cone and bull arcs cost a few hundred evaluations
    per column in reach.
  - Reverting means restoring the four chord loops. The module is
    self-contained.
