## #555 - Setup alignment: lay the part on a plane through three picked points

**What was decided.** A third plane-alignment input beside the face pick
(#330/#554): three picked model corners span a plane, which is laid on
the setup's G17 / G18 / G19 plane. `pick::lay_on_points` reads the three
vertices in the DOCUMENT frame (absolute, like the face lay), builds the
same datum the face pick builds - a unit normal and an anchor (point 1) -
and hands it to the one shared `pick::plane::lay_datum`, which the face
pick now also goes through. So a face and three corners of it give the
same alignment (pinned for every plane, flip and level choice), and the
stock refits the same way. It is written through the existing undoable
`AlignPartToSetup` (new doors `lay_on_points` / `check_points`): one undo
unit, ops of the setup go stale and undo un-stales them, linked setups
(#316) and the following stock behave exactly as for the face lay, the
work offset is never touched. No new command type, no schema or project
format change.

**Which side is the outside.** A face has its winding; three points have
none. The points' outside is the side of their plane the part's material
reaches LESS far into (max signed distance of every part vertex on each
side). For points on the hull of the part - the normal case - that is
the side with no material at all, i.e. exactly the face's outward normal,
so the two modes agree and the pick order does not matter (pinned: 1-2-3
and 3-2-1 give the same lay). A plane through the middle of the part
(equal reach both ways, within 1e-6 relative) keeps the pick order's
right-hand normal (1 -> 2 -> 3); Flip turns either. The unflipped sense
is the face row's: the outside faces the plane's axis (+Z on G17, part
below the plane); Flip faces it the other way - G17 + Flip rests the
points on the table with the part above, the 3-2-1 primary-datum case.
Considered: making "rest on the table" the points row's default - dropped
so the two rows' Flip means the same thing on the same plane (the issue's
"they can't disagree"); the MCP description and hover text say it.

**Refusals, by name, at command creation.** Not exactly three picks ("pick
three points - N picked"); a face or edge pick; a pick on the stock or
any non-part body ("is not the part"); an entity gone from the model; no
model ("there is no part to align - import a model first"); a non-finite
vertex; two points closer than 0.001 mm or the same pick twice ("points i
and j coincide"); and (nearly) collinear points: refused when the
triangle's least height is under sin(1°) x its longest side (about 1.75 %
- a 1° wedge), "the three points are (nearly) collinear - they span less
than 1° across". The threshold is scale-free so a large plate and a
small part are judged alike; below it a micron of pick noise tilts the
plane visibly. Every refusal also greys the page's Apply with its text.

**Vertices only - no points on faces.** `GeoSel` carries a feature id
(face, chain, patch, vertex), not a hit point; a free point on a face
would need a new persisted selection kind (project format, MCP ids,
feature re-resolution). Not cheap, so this ships with model corners only;
a "point on a face" pick is a follow-up if castings without usable
corners need it.

**The 3-2-1 frame (done).** Optional `frame`: after the shared lay (with
`place` off), the part turns about the target plane's normal so point
1 -> 2 runs along the plane's first axis (+X for G17 / G18, +Y for G19,
since X is the G19 normal), then moves so point 1 is the setup frame's
origin. The frame implies the level. It turns the part, not the work
offset: the setup's WCS is untouched (a set work offset keeps its
place; point 1 is at the setup origin, which is program zero when no
offset is set). Landings are proven (1 -> 2 on the axis to a 1e-9 sine,
point 1 at the origin to 1e-9 mm per mm) or refused.

**Composition with #557's in-plane turn.** The three-point lay is
absolute like the face lay: applying it replaces whatever the setup held,
an earlier axis turn included. A later "Axes in the plane" pick composes
onto it as it does onto a face lay (it turns about the part centre, so
after a frame lay it also moves point 1 off the origin). The latest
explicit command wins; nothing is locked. Considered: refusing an axis
pick after a frame lay - dropped, the user asked for the later change.

**UI.** A separate row "Lay on a plane - pick three points" right under
the face row "Lay on a plane - pick a face", with its own controls (Pick
three points / Stop picking points, plane, Flip points, Point 1 to plane
level, 3-2-1 frame). Armed, it prompts Point 1 / 2 / 3, offers Remove
last and Clear points, and Apply points greyed with the reason. Arming
it sets Vertex mode and stops the face and axis drafts (one draft
collects at a time); a face, an edge or a fourth point is refused by name
in the status line. Considered: a mode switch inside the face row -
dropped, the two take different picks and #554 just made the face row
say "a face" plainly.

**MCP.** New tool `lay_part_on_points` (setup, points, plane g17/g18/g19,
flip, place default true, frame) beside `set_part_axes`, through the same
command; returns the alignment and the outside in words.

**Cost to change later.** The outside rule is one function
(`material_side`); the thresholds are two public constants
(`COINCIDENT_MM`, `MIN_SPREAD_DEG`). Adding points on faces means a new
`FeatureSel`-like pick kind; the lay path would not change.
