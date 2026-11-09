## #48 - Swarf: a raster line's exit rise is a rapid under the lying pose (2026-11-08, Prime Directive)

The #17 entry left this owed: every swarf raster line left the blade on
the base raster's straight feed up Z, whose end stands in air, so the
stamp stated it upright - a 90 deg turn off the lying pose on every
raster end, taken through a retract / swing / re-enter air link, and
turned back on the next line's plunge.

Readings on the motion suite's debug blade (`swarf_pose_continuity`):

| run | air links on main | after |
|---|---|---|
| swarf, debug band | 34 | 23 |
| ladder, debug band | 53 | 34 |
| swarf, full band | 55 | 44 |
| ladder, full band | 86 | 67 |

Main was already red on the ladder's two ratchets (53 > 51, 86 > 84):
the advanced count had crept up by two since #17 set them. The same
ratchet; this change takes both counts far under the old pins, and the
pins are lowered to the new counts.

(1) **The exit rise is a rapid, not an excused feed.** Chosen of the
two options #17 named. `stamp_rise::rapid_rise` turns a swarf station
into a rapid that holds the pose the line cut with when ALL of: it is a
feed straight up Z (no XY travel); the program has leaned and the held
pose is a lean, not under level; its end ball stands in air (past the
stamp's contact band); the ball never comes nearer the part on the way
up than it stood at the start (`min(d0, r)`, the link law's floor);
and the held pose's flank is proved in air (`flank_clear`, the stamp's
own proof) at every half-radius sample up the rise. The ball's path is
the plan's own (a sphere about its centre), only the flank differs, and
it is proved. Anything not proved keeps the upright feed and the link
law takes the turn through air as before. The rapid then hands the
lying pose to the clearance rapid after it (rapids inherit) and the
next line's plunge states its own - on the blade the same lying pose,
so no turn happens at all. Why a rapid: a rapid claims no contact, so
the verifier's posed-contact law (which convicted the posed feed in air
with a 14.8 mm Envelope rest in #17's first draft) has nothing to read;
its posed-rapid solid law judges the swept stack as it judges every
link retract, which already leaves contact on a rapid. Both app tests
(`swarf_generates_blade_lying_axes_and_verifies_clean`,
`advanced_swarf_adds_the_coarse_rungs`) and their heavy full-band
twins verify clean; the pivot5d envelope scan (release lane, the judge
of record #17 deferred) passes.

Dropped: (a) excusing a posed feed that leaves along the normal in the
posed-contact law - it changes a safety law to fit one generator, and
the base rise goes up Z, not along the normal, so the excuse would need
a second rule about which direction counts; (b) retracting the rise
along the contact normal (the link law's retract) - a longer and more
complex path for the same air, when the Z rise is already the plan's
own ball path and its flank is proved.

Cost to change later: one module (`multiaxis/stamp_rise.rs`) and a
`kinds` override in `stamp.rs`; posted swarf programs change (a `G0`
rise under the lying angles instead of an air link plus an upright
`G1` rise). No golden covers swarf output, so none moved. Pinned by
`swarf_pose_continuity::every_raster_exit_rise_is_a_rapid_holding_the_lying_pose`,
the lowered ratchets in the same suite, and the three unit tests in
`stamp_rise`.
