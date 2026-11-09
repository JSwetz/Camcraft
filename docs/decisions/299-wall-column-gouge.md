## #299 - The gouge verdict grants `tol` sideways at a wall

**Decided.** The program sim's gouge verdict (`deviate`, column engine,
and `deviate_field`, field arm) judges a column that reads more than
`tol` below the part's top at its centre against the LOWEST part top
within `tol` of that centre (the centre plus an eight-direction ring at
radius `tol`; air counts as lowest). Within `tol` of that, the column is
clean (`stock_sim::wall_slack`). A column past the slack is still a
gouge, charged at the centre's own depth as before. Rest is unchanged.

**Why.** A height verdict measures error along Z only, so its
normal-distance tolerance is `tol * cos(slope)`: at a vertical wall that
is zero, and a column centre a few microns inside the wall, reached by a
tool that finishes the wall within tolerance, was charged the wall's
whole height. Measured on the user's part (`Milling-test.step`): a 3D
Finish on model stock was refused for 10.21 mm at a column 1-3 um inside
the D's wall, and in the 70 x 70 x 27 box stock for 37 cells, worst
22.01 mm, all ball-tangent-to-wall. Granting the same `tol` sideways
keeps the normal-distance tolerance between `tol` and `sqrt(2) * tol` at
every slope. Both engines share the one law (the stage-C parity rule).

**Considered and dropped.**
- The issue's `gate_tol(DEV_TOL, cell)` slack (0.066 mm at a 0.375 mm
  cell): it ties a safety verdict to the lattice size and absolves wall
  intrusions six times the stated tolerance. `tol` alone cleared every
  measured case (the contour case was 9 um, the finish case under 3 um).
- Judging by the distance from the marched surface point to the model
  (the preview's eye law): a larger rewrite of both verdict arms and the
  carried body's floor verdict, for the same answer at walls.
- Slack on rest too: material standing on a floor beside a wall would
  read as the wall's own and hide rest, and rest does not refuse a post.

**Cost to change later.** Local: `wall_slack::settle` is the one place
the slack is applied; widening or narrowing it is one constant. Note
the lattice still reads only its column centres: a wall intrusion that
no column centre past the slack lies in is invisible, as it always was.
`kernel::measure` / `measure_field` (per-op reports, not the post gate)
keep the centre-only law.
