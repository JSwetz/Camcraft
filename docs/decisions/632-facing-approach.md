## #632 - lathe facing approaches at rapid and lifts off the face before retracting

**Decided:** the facing kernel (`crates/motion/src/turning/face.rs`) now
(1) opens its path on the spindle axis on the clearance plane and makes its
first move a RAPID out to the clearance point `(rho_clear, clearance)` -
the posted program's first motion after the tool call is `G00` to that
point, then the existing plunge feed down to the first station; and (2)
ends every pass with a rapid LIFT along the axis off the face just cut,
then the radial rapid out at the lifted station (the final pass too, before
the rapid home to the clearance plane). The lift is
`LIFT = 0.2 mm`, held to half the stepdown (`min(0.2, stepdown / 2)`).

**Why the axis start:** a `PathBuilder` drops a zero-length move, and the
cached path's first `from` is what the verifier judges, so the rapid to the
clearance point needs a real start. The mill kernels start at the program
origin on the clearance plane (`(0, 0, clearance)`); the lathe's reading of
that is the spindle axis on the clearance plane, so the approach is a
traverse along the clearance plane - judged by the meridian rapid law
(#103, `turning_rapids::judge_axial_rapids`) against the revolved stock and
part. A clearance plane set inside the bar now convicts the approach
(`RapidInStock { at: 0 }`) and blocks the post, pinned by
`a_clearance_plane_inside_the_bar_is_convicted_not_posted`. Before, the
first move was a feed with no judged approach at all.

**Why 0.2 mm and the stepdown cap:** 0.2 mm clears a spring-back and chip
film on a fresh face without a long dwell in the air. The cap keeps the
lifted rapid inside air the ladder already cleared: the pass above swept the
band `[s + stepdown, s + stepdown + 2 nose]` up to its own reach radius, so
a lift no taller than the stepdown leaves the lifted nose inside what this
pass and the one above removed (and the reach sweep's rung windows bound
anything wider). Half the stepdown is margin under that bound.

**Kept:** the axial descent at `rho_clear` from the clearance plane to the
first station stays a feed (the kernel's "controlled axial stationing",
#103 - a stock wider than the model it was told about would stand there);
only the approach traverse was missing. The lift is a rapid, not a feed: it
moves away from the face into cleared air.

**Considered and dropped:** a zero-length rapid inserted after the builder
(breaks the "from == to never reaches a move" invariant the sim and laws
rely on); starting the path at a fabricated turret park point (no such
point is known to the kernel or the machine model); a rapid straight down
to just above the first station instead of to the clearance point (the
issue asks for the clearance point, and the traverse along the clearance
plane is what the verifier can judge as air); a lift by a fraction of the
nose radius (the stepdown, not the nose, bounds where cleared air ends).

**Profile Rough (checked, not changed):** its ride already opens with a
rapid (from the clearance point down to the rung's first station at
`rho_clear`), and each rung exits radially OUTWARD at the window's top -
away from the turned diameter, through air or at feed where the bar runs on
(#229) - so no retract drags across a finished surface. Its first posted
G0 goes to `(rho_clear, first station)`, not via the clearance point; that
is a rapid, not a feed, and was left as is.

**Cost to change later:** the lift is one constant in `face.rs`; the
approach start is one line. The Fanuc Lathe facing golden and the
`turning_face` / `turning_face_approach` pins would move with either.
