## #413 - The field engine and the envelope scan sweep XY arcs on the true circle

**Decided:** both places that still drew an XY arc (G2/G3 in G17) as
inscribed chords now read the true arc, the way #402 made the column
engine do.

- **Field engine** (`stock_sim/field_arc.rs`): an upright mill tool's XY
  arc becomes `Sweep::Arc` pieces. A piece is at most a quarter turn and
  uses the same chord budget as before, so the lattice windows and the
  per-move cell ceiling still refuse exactly what they refused before.
  Each piece measures the true arc:
  - **Level piece:** closed form. The nearest pose sits at the query's
    bearing clamped into the piece, with the column engine's tie laws
    (`flat_tie`, `bull_tie`) and the closed-form flat-floor ray entry.
  - **Helical piece:** searched with `dexel_arc::piece_min` (samples plus a
    golden section). Results are real evaluations only, with the
    conservative 1e-12 hair added, so the search never claims removal the
    tool did not make. Its `residual` is the 1-Lipschitz bound of half a
    sample step.
  - **Windows and stadium:** the window grows by the piece's sagitta, and
    the stadium shrinks by sagitta plus the piece's Z change.
- **Envelope scan** (`stock_sim/scan_arc.rs`): each XY arc piece keeps
  the chord `arc_chords` drew, so bins, boxes, ring ids and the ridge
  law's pass counting are unchanged. Every distance the verdict reads off
  a piece now comes from the arc itself:
  - point distances for rest, flank and hidden rest, in closed form when
    the piece is level;
  - facet distances for gouge, searched only when the chord is within its
    bulge of the ball.
  - Bin walks, boxes and the `SegGrid` ring prune grow by the bulge
    (`sag + |dz|`, or the diameter for a piece longer than half a turn).
- `stock_bull_nose::both_engines_cut_a_bull_nose_alike` is back to
  1e-5 parity on the helix arc; `arc_parity` is gone.

**Why:** an inscribed chord is wrong on both sides.
- At a convex corner it reads a false cut: the displayed stock, and up
  to `tol/4` of phantom gouge in the scan.
- At a concave corner or on a bull-nose helix it hides a real cut: a rim
  column cut 2.5 mm deep by the true arc stood uncut on the field, and a
  wall over-cut by less than a sagitta read clean in the scan.

Only the true arc is right on both sides, which is #402's argument again.

**Considered and dropped:**
- *Straddling chords* (vertices pushed out by half a sagitta): this halves
  the error but keeps it on both sides, so the convex corner still reads
  a phantom gouge.
- *Finer chords:* these only shrink the error. Driving it under 1e-5 costs
  roughly 30x the sweeps on a typical arc.
- *One whole-arc field sweep:* its window is the arc's box, which is far
  larger than the chord windows. A full circle at a fine cell would hit
  `MAX_CELLS_PER_MOVE` and refuse moves that are payable today.
- *Making the scan's chord foot (`side_foot`, the flank's side test) an
  arc foot:* it only picks a side, and within a sagitta that side is the
  same, so the foot stays on the chord.

**Not changed:**
- XZ/YZ arcs, tilted poses (field and scan) and lathe tools keep their
  chords, as in #402.
- The rotary field (`field_rotary`) has its own chord law and is not
  touched.

**Cost to change later:** the arc code is local to `field_arc.rs` and
`scan_arc.rs`. Each consumer touches it in a single place
(`Sweep::Arc`, `seg_dist`, the gouge walk's `dist_tri`), so removing it
means returning `None` from `arc_sweeps` and `xy_arc_pieces`.
