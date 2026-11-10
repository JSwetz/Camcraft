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
