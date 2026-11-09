## #339 - posed and turned moves judged against the fixture mesh (slice 3)

**Decided:** the fixture mesh law (slice 1, acquit-only) now also reads
the moves slice 1 left to the box: posed moves (a 4/5-axis index, a
tilted 3+2 pass, a coordinated lean), moves under the spindle C, and
XZ/YZ arcs (`camcraft_sim::verify::mesh_posed`, dispatched from
`mesh_steel::mesh_refined`). It still only ACQUITS a move the box laws
flagged; where it does not, the report is the box laws' own, label and
all. A tilted face between closed vise jaws now posts; the same pass with
a jaw moved into the cutter, an index leaning into a jaw, an XZ arc
dipping into a vise floor, and a C turn swinging the flute into a jaw
that rides the part still block.

**The enclosure (why "clear" is a real clearance):** a posed move is
walked in stretches cut by the pose's turn alone (`midpose::turn_intervals`
- the posed sweep's own walk without the tip-travel cut, since the tip
segment is swept exactly). Over a stretch the tip is on `p0..p1` (an
arc's chord, within its sagitta pad) and the pose within `swing` of the
stretch's axis (midpose's proven letter-rate bound). Every band point is
`p(t) + Q(t) y` with `|Q(t) y - y| <= swing * |y|` and
`|y| <= hypot(max(|lo|,|hi|), r)`, so the band lies in
`segment + oriented cylinder + ball(chord + swing * hypot(..))` - convex;
GJK sweeps the segment and cylinder exactly (`FixtureMesh::band_meets_along`,
the slice-1 `SweptBand` given an axis) and the ball rides on the proven
margin. A rigid turn on top (steel in another frame) adds
`dth * rho_max`, `rho_max` the farthest the stretch's set stands from the
turn's line (convexity: the parallelogram's corners plus `r + pad`).
Stretches overlap at their shared instants, so with every enclosure
proven clear of every facet one point per band decides inside/outside.

**The frames:** steel riding the part, or any steel with no rotary axis
stated, is read in the path frame. Steel declared fixed under a rotary
op's axis is read in the machine frame, each stretch carried by the turn
standing its pose upright (`posed_steel::upright_turn`) - the frame
`workholding` reads those boxes in; a pose the rotary alone cannot stand
upright stays a box verdict. Under the spindle C the line is not in the
path, so it is recovered per move from the two stated C angles (the tip
stands at azimuth `-C` at both ends: two rays meeting at the line, the
convention `cl::rotary` states). The machine-frame motion is then exact
(a straight tip run, the station tool along +X): fixed steel is read
there in one sweep; steel riding the part is read on the spiral the
machine traces - machine stretches carried by `-C`, padded by the C turn
inside each stretch - not on the CL chord.

**Why the station pose, not the stated axis, under C:** `cl::stamp_c`
splits a turning move into pieces that all keep the WHOLE move's axis,
which is off the station by the rest of the turn on every piece but the
last. The machine's live tool never moves; the stated axis is only
checked to be horizontal (a radial tool - an axial or leaning pose is
not the tool this law places, and keeps the box).

**Unjudged (box verdict stands):** a C move whose start C is not the
previous move's stated C ending at its start (a section's first move, a
jump); C held off zero for fixed steel (nothing places the line); C turns
under 1e-4 rad or near 180 deg (the rays do not cross well); a C move
with an arc; fixed steel with BOTH a rotary axis and a C (two turns, no
one frame); every refusal of slice 1.

**Measured:** debug, 29,700-facet sphere, 80 posed moves (40 tilted
passes each swinging B 30 deg + 40 in-place indexes) in 0.07 s. Release
heavy lane, 203,320 facets, 2000 posed moves in 0.33 s (slice 1's 2000
upright passes: 0.016 s; its grazing worst case 0.73 s unchanged).

**Considered and dropped:**
- *Capsules (posed_steel's own enclosure) against the mesh:* simpler,
  but the capsule reaches a radius below the tip - a false hit on a
  vise floor under a tilted tool, the case the mesh exists for.
- *Reusing `midpose::intervals` as is:* it also cuts by tip travel
  (0.5 mm), which the exact segment sweep does not need - 200 stretches
  per 100 mm constant-pose pass instead of one.
- *Reading C moves on the stated chord and pose (as the box laws do):*
  `stamp_c` bounds the chord only at its quarter points and leaves the
  pieces' axes off the station; acquitting on that reading could clear a
  move whose real spiral strikes. Recovering the line costs two cross
  products per move.
- *Unjudging every C move:* safe, but leaves the mill-turn live-tool
  case (the spindle-C kinds) with the box's false crashes.

**Changed in slice 1's pins:** `fixture_mesh::posed_moves_and_unpaired_boxes_keep_the_box_verdict`
asserted a posed gap pass keeps its box hit; it is now acquitted (renamed
`unpaired_boxes_keep_the_box_verdict_and_posed_moves_are_read`).

**Rapid naming (merged with the parent's #424 fix):** a rapid the mesh
keeps names its struck band the way law (d2) names an upright rapid
since #424 - shank, holder or spindle by its part, the flute as Rapid -
on the posed and spindle-C readings too, through the one shared
`mesh_steel::label`. Chosen over keeping Rapid for every posed band so
one rapid reads the same whichever reading the mesh took. The posed box
law itself still names a posed rapid's whole-stack hit Rapid, so the
named band shows only where the mesh acquits another flagging box and
keeps this one (pinned by `a_posed_rapid_kept_by_the_mesh_names_its_band`).

**Found, not fixed here:** the box laws read a `stamp_c` piece's stated
axis (the whole move's), which is off the live-tool station by the rest
of the turn, and read C moves on the CL chord in the part frame for
fixed steel too. Neither is this slice's (acquit-only cannot add hits);
worth an issue against the box laws.

**Cost to change later:** one module behind the slice-1 dispatch; the
GJK band gained an axis field (upright stays float-for-float slice 1's).
A tighter swing pad needs only a finer `turn_intervals` step.
