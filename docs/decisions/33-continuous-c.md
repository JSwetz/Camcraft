## #33 - Continuous C-axis live tooling: the stated C angle, the split law, the lathe post's C section (2026-11-08)

**The cl move states its C beside the tool axis, it is never derived.**
`cl::Move::rotary: Option<f64>` is the spindle's C in degrees, UNWRAPPED
(a part turned one and a half times reads -540, never -180). `to` and
`axis` stay in the PART frame, so every verifier, sim and law reads a C
path exactly as it reads any posed path; the angle rides beside them. Why:
`cl::axis_to_angles` turns a direction into [A, B, 0] - a vector cannot
carry C - and an atan2 of the tip cannot carry the winding count either.
Absent on every move but a C cut, so every golden, stream and project file
without one is byte-identical (the project field skips serialization, the
axis-v22 precedent, no version bump).

**The convention: Rz(C) turns the part-frame tip onto the +X station.**
A tip at azimuth phi stands under the tool at C = -phi. The tool axis is the
station's (radial, tip to shank along +X once turned); a pose that leans off
it is refused - a fixed live tool cannot hold lead/lag. Machines that count
C the other way round are not modelled: the `.mch` grammar has no C-sense
word yet (owed; until then the shipped mill-turn reads RS-274's right-hand
rotation about +Z).

**The split law (`cl::stamp_c`): the machine's motion is the chord that was
verified, to the op's tolerance.** A control moves X (radius), Z and C
linearly between blocks - a spiral in the part frame, not the chord. Every
move is bisected collinearly (the same geometry) until the spiral and the
chord agree within the op's verify tolerance at the quarter points; a move
that cannot be split in 2^12 pieces (it brushes the spindle line) is
refused by name. Arcs, poseless moves and paths carrying cycles /
compensation / thread / probe records are refused on a C turn. Considered:
interpolating C linearly and letting the verifier read the spiral - dropped,
it would put a second motion model into every law.

**Which ops: the rotary kinds on the spindle's C.** The M13.5 B letter gate
now answers 'C' for a fitted line the machine holds as C (it refused
"pinned" before); Rotary Contour / Parallel / Offset then stamp their paths
(`operation::rotary_gate::stamp_spindle_c`). The line must stand along
world Z. No new op kind: a wrapped/cylindrical cut on a mill-turn IS the
rotary ladder with the part turning instead of the tool leaning. Considered:
a dedicated "C-axis contour" kind - dropped, it would duplicate the ladders.

**The letter gate reads the stated C.** `machine::check_document` asks a C
move for a rotary named C (and its stated travel); it no longer reads the
part-frame axis as A/B swings for such a move. An unwrapped path that winds
past a stated C travel (the shipped millturn.mch states -360..360) is refused
by name; unwinding on retracted links is owed.

**The stream: `PARAM LiveTool radial` + `A 0 B 0 C <c>` on every move.**
The engine refuses the section by name for a script that does not declare
`liveTooling = true` (the `coolantModes` pattern), so no mill post can post a
radial live-tool cut as XYZ for a vertical tool. Only fanuc-lathe.cps
declares it.

**fanuc-lathe.cps C section.** The tip is turned by C and must land on the
station within 5 um (lattice + milli-degree words), else refused - never
projected. X = 2r, Z, C. G98 per-minute in the section (no turning spindle
to count revolutions), G99 restored after. F is scaled so the control's
composite rate - X radius and Z in mm, C degrees counted as mm, the rotary
feed law of Fanuc-style controls - moves the tip at the planned rate along
the part-frame chord. That feed law is a stated assumption about the
control family; a control that counts rotary feed otherwise (inverse time,
true tangential) would cut at the wrong speed, not the wrong place - to be
confirmed on a real control. A rapid in the section is a G01 at the
machine's stated maximum feed, never G00: a G00 with C moves each axis on
its own and follows no verified path; no stated maximum, refused. The first
move must be a rapid (C positions, then X Z). Arcs and cycles in a C
section are refused.

**The engage words are the machine's, never the post's.** C-axis mode on/off
and the live-tool spindle start/stop are builder-specific M words - no public
standard fixes them - so they are a new `.mch` statement,
`tool_station.live_tool { c_axis_on, c_axis_off, spindle_on, spindle_off }`
(all four or none). Unstated, the lathe post refuses the section by name.
The shipped millturn.mch does not state them (we know no honest values for a
generic machine): an in-app C program posts once the user's machine file
states its builder's words. Considered: post options with defaults -
dropped, a default M word is a guess.

**What remains (owed on #33):** a machine C-sense word; unwinding C on links
against a stated travel; axial (face) live-tool stations and polar
interpolation (G12.1) for face milling; arcs on a C turn; the setup sheet and
the in-app posting gate test on a world document (the stream and post laws
are pinned by `c_axis_live.rs`).
