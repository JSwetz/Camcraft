## #630 - lathe posts write the tool's control point, not the nose centre

**Decided:** design option (a). Turning inserts and boring bars carry a lathe
tool orientation (`Tool.orientation: Option<u8>`, LinuxCNC's 1-9 chart;
`None` reads as the kind's default). At post time, and only there, a
nose-centre turning path (Facing, Profile Rough/Finish, Chamfering, Taper
Turning, Multi-Pass Turning, Boring Turning) is translated to the tool's
control point: `posted = nose centre + r * tip_offset(q)`
(`camcraft_post::post::nose_tip`, called once from `posting::op_streams`
through `posting_nose.rs`). Both lathe posts keep G40 and write the stream's
point as-is; `PARAM ToolOrientation` rides the section and the Fanuc Lathe
tool note names it. The cached toolpath stays the nose centre, so the
verifier, the stock sim, the envelope/gouge laws and the CL goldens are
unchanged.

**The chart (LinuxCNC Lathe User Information, section 3 figure and section
7.1 "Control point"; figure drawn X+ toward the operator, Z+ right):** the
control point is where the X- and Z-parallel tangents of the nose meet, at
`centre + r * (X, Z)`: 1 (-1,+1), 2 (-1,-1), 3 (+1,-1), 4 (+1,+1),
5 (0,+1), 6 (-1,0), 7 (0,-1), 8 (+1,0), 9 (0,0). It is stated in X/Z
coordinates, so the number does not depend on the turret side. Defaults:
turning insert 2 (OD, cutting toward the chuck - the right-hand turning
tool), boring bar 3 (ID, toward the chuck). The editor shows
"default: 2 - ..." for an unstated code, so the default is visible.
Fanuc's imaginary-tool-nose tip numbers are a different chart (the usual OD
tool is tip 3 there); no post writes either number to the control, so
only the LinuxCNC chart is modelled.

**Why a translation is exact:** the kernel's CL is already the profile's
offset by r (the nose centre); the insert is rigid, so under G40 the
control point stands at one fixed offset from the nose centre for the
whole cut. Moving the whole CL by that offset is exactly the path a
control's own nose-radius compensation would derive - diameters, faces,
tapers and arcs alike (arc centres move with it, radii unchanged). The
r(1 - cos) taper/radius error belongs to programming the PART profile as
the tip path; it does not arise here. Pinned by the post crate's
`nose_tip` tests (OD finish, face, taper, arc, every orientation).

**Refusal by name:** an orientation fits an operation when its nose can
present every contact normal the cut needs - a corner tip (1-4) within 45
degrees of its tip direction, an axis tip (5-8) within 90, 9 always. Needs:
facing {-Z face, -X leading (passes run inward)}, outside turns {-X},
boring {+X}. A mismatch refuses the post naming the operation, the needed
contacts, the list that fits and the tool's code - the same reading under
which PathPilot's own refusal ("... call for orientation [1, 5, 6]") comes
out for a -X/+Z cut. Checked at post time (every post reads the live tool),
not at generation: the orientation does not change the toolpath, so it is
not in the op stamp and an orientation edit never stales an operation.

**Considered and dropped:**
- (b) G41/G42 with the tip number and the part profile: every post would
  need nose-comp lead-in/out choreography and a part-profile stream the
  turning kernels do not produce; PathPilot's own conversational posts G40
  with compensated coordinates, which is what (a) produces.
- Shifting per post script (`.cps`): the issue asks for one place; a
  script-side shift would be re-derived per dialect and could disagree.
- Shifting the cached toolpath: would move the sim and the gouge laws off
  the real insert.
- A leading-direction contact for profiles (the travel direction along Z):
  the profile kernels walk bottom-up (toward +Z, away from the chuck), so
  it would refuse the default orientation-2 insert on every existing
  profile op; the lead-angle law needs the insert's approach angle, which
  the library does not model. Recorded as follow-up work, not done here.

**Not changed (follow-ups worth filing):** grooving/parting blades post
their band centre (a blade is touched off on a corner - LinuxCNC section
4.2's parting example - so a width/2 Z offset is owed, like this one);
threading inserts and form gauges are posted as before (#629 owns the
threading code). The PathPilot lathe post (PR #626) consumes the same
converted stream when it merges.

**Cost to change later:** the chart and coverage live in
`crates/tools/src/orientation.rs`, the geometry in
`crates/post/src/post/nose_tip.rs`, the op families in
`crates/app/src/posting_nose.rs`; the project key is additive (absent =
default), so a different default or chart is a one-file change plus the
Fanuc Lathe goldens.

### The built-in Lathe's X floor reaches a little past centre

**Decided:** `assets/machines/lathe.mch` X travel `0..320` becomes
`-2.5..320` (radius from the centreline, the checker's reading of a lathe
X: -2.5 is Ø-5). Posting the control point means a facing pass that
faces clean to centre - nose centre on X0 - posts its orientation-2 tip at
-r, X-2r on diameter (X-1.6 for the catalog's TN-1600-0R8, r0.8). A slide
that stops at X0 cannot reach the point the program commands.

**Arithmetic:** the largest nose posted at a control point in the catalog
is r0.8 (TN-1600-0R8; BR-1000 is r0.4; the form gauges' r6/r12 crowns are
not nose-centre paths and post as before), so the deepest catalog point
is X-0.8 (Ø-1.6). The floor -2.5 (Ø-5) is 3.1x that, and still covers
the largest common ISO insert nose, r2.4 (Ø-4.8), for a user-added tool.
A real over-travel past centre (X-10, Ø-20) stays outside the band.

**Evidence (behaviour reference only):** the contributor's Tormach
PathPilot Hub 8L conversational facing program (PathPilot Hub 8L
conversational v2.14.4, 2026-11-09; G20, tool orientation 3, tool radius
0.0158 in) ends every pass, the finish included, at `G1 X -0.0174` on
diameter. Our geometry for that tool puts the nose centre on X0 and the
control point at -2r = -0.0316 in on diameter; PathPilot stops at about
0.55 x 2r - past centre, but not the full 2r. A control's own facing goes
past centre, so a control-point post must too. We keep -2r: it is the
point at which the nose's round reaches the axis, so the face is clean to
centre with no pip; stopping short at 0.55 x 2r leaves a small boss the
nose round never touches.

**The rule overturned:** `lathe_stroke::lathe_x_stays_a_centreline_band`
(#493) pinned `0..R` - "X is measured from the spindle centreline ... the
one-sided band is right and is kept". The centreline reading stands (X0
is the axis, the band is program-frame); only the floor's value was
wrong once the post writes the control point rather than the nose
centre. The test now pins `-2.5..320`, and
`a_real_x_overtravel_past_centre_still_refuses_by_name` /
`a_facing_pass_to_centre_fits_the_built_in_lathe_x_band` pin the rest.

**Considered and dropped:** a floor of exactly -r for the catalog tool
(-0.8): no margin, and a user's r1.2 insert would refuse a legal facing
pass; a much larger floor (-10 or more): reads a real crash past the
centreline as legal. Widening the built-in Mill-turn (`0..650`) and the
Tormach lathe definitions: not on this branch (no Tormach lathe `.mch`
exists here; the Mill-turn would need the same floor when it posts
control points - follow-up).

**Observed while pinning it (follow-up):** the travel check judges a
move's world X against the X band, but the turning kernels lay their
meridian along world Y (the cached facing path is (0, y, z)), so the
lathe X band never sees the turning radius - neither the nose centre nor
the control point. The floor is right for when it does; making the
checker read the meridian radius (and the posted control point) is owed
work.
