## #491 - the fixture laws pad an arc's chords by its bulge

- **Chosen:** A new module, `sim::verify::arc_bulge`, says how far an
  arc's true path can stand off the chords the fixture laws sweep
  (`Move::segments3` at the chord tolerance). Each chord of half-angle
  `psi` on a circle of radius `r` gets two offsets: across it, the
  sagitta `r (1 - cos psi)`, and along it, `r (psi - sin psi)`. These
  bound the true point against the chord's point at the same sweep
  fraction (sine is concave on `[0, pi/2]`). A chord over a half turn
  is bounded by the diameter, `2r`. Every arc chord also takes
  `ARC_PAD` = 1 um, because arc centres stand on the 1 um lattice while
  endpoints stay exact. This is the same pad the mesh law's chord
  reading uses.
  - **Box laws (c2)/(d2), `stack_span::band_struck`:** each piece
    carries a radius pad. In XY it is the whole deviation,
    `hypot(across, along) + ARC_PAD`. In XZ/YZ only the horizontal part
    pads the band's radius: `along * |t_h| + across * |n_h| + ARC_PAD`,
    with `t` the chord direction in the arc's plane and `n` its normal.
    The upright column and the lean enclosure both add it to their
    radius. The upright-rapid stack reading (#424) goes through the same
    pieces, so it is padded too.
  - **Vertical arcs' z window:** the arc's true z range between the
    joints. The builder splits every XZ/YZ arc at its circle's top and
    bottom, so between two joints z is monotone and the joints bound it.
    No z pad is added there. An arc built without the builder that turns
    past an extremum inside a chord has that piece's window carried to
    the extremum, plus `ARC_PAD`. An extremum within `ARC_PAD` of a
    joint is the builder's own lattice-snapped split, and the Z-monotone
    law reads it.
  - **Posed sweep (`posed_steel`):** every capsule of an arc move grows
    by the 3-D deviation, `hypot(across, along) + ARC_PAD`, added to the
    interval's tip travel. The walk carries the pose by sweep fraction,
    and the bound is fraction-matched.
- **Why:** the measured repro (R30 arc, r3 flute, sagitta 0.0094 mm,
  box face half a sagitta inside the true reach) returned no hit. Now
  it reports `FixtureHit (Flute)`, and the holder band the same way.
  Padding per chord, not per move, keeps the over-report on a long arc
  to what each chord can actually bulge. The horizontal/vertical split
  keeps the strict-dip law. A z pad on a monotone vertical arc would
  make a lead arc landing tangent on a mounting face report flush
  contact as a crash. Pinned: `an_xz_arc_landing_tangent_on_a_face_stays_silent`.
- **Straight moves:** they pad nothing (`r + 0.0`, `travel + 0.0`, and
  the same floor/min sums), so their windows and verdicts are
  byte-identical. Every existing fixture suite keeps its verdicts.
- **Fail closed:** an arc whose centre or radius is not a number gets a
  NaN pad. The band and capsule predicates read NaN as a hit. Before,
  such an arc fell back to its straight chord and was judged as a line.
- **Considered and dropped:**
  - *One pad per move (the largest chord's) on the box laws.* It is
    simpler but over-reports along the whole arc. All of a move's chords
    have equal steps, so the per-chord pad is the same number in XY.
    Per-chord matters for vertical arcs, whose horizontal share varies
    chord by chord.
  - *A full 3-D pad on vertical arcs' z windows.* It breaks the
    strict-dip silence at tangent landings (above).
  - *Finer chords instead of a pad.* This would cost more chords per
    arc and only shrink the sliver, never close it.
  - *Leaving the posed sweep alone.* Its interval travel (up to about
    0.25 mm) already covers the sagitta in every realistic case, short
    of an excess of around `sag^2 / (2 travel)`, about 1e-4 mm. The pad
    makes the enclosure provable for every interval length, at the cost
    of at most a sagitta more over-report on posed arcs.
- **Left alone:** law (d2)'s flute disc (`rapid_hits_box`) reads a rapid
  as its straight from-to segment. An arc rapid's flute is also judged
  by the padded flute column (`upright_rapid_struck`), so nothing is
  missed. The leaning-rapid capsule in `rapids_vs_fixtures_held` reads
  the straight segment too, but the posed sweep runs first on every
  leaning rapid and carries the pad. #413 (the field engine and envelope
  scan drawing XY arcs as inside chords) is the same family but a
  different law. It stays its own issue.
- **Cost to change later:** the bound lives in `arc_bulge::Bulge`, and
  the laws only read `horizontal`, `z_span` and `deviation`. The mesh
  law (#339, `mesh_steel::chords`) keeps its own `ARC_PAD`. When it
  lands, it could read `Bulge::deviation` instead, so there is one bulge
  law.
