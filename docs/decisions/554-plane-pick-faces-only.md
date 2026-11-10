## #554 - Setup alignment: the plane pick takes faces only

**What was decided.** The Setups page's "Lay on a plane" row takes one
flat face and nothing else. Arming it ("Pick a face") sets the viewport
to Face mode, so a click anywhere on a face - right beside its edge
included - takes the face; the edge-snap pass that kept stealing the
click is not run. The #330 two-edge plane mode is removed, not hidden:
`pick::lay_on_plane` refuses any edge pick by name ("is an edge - an
edge cannot set a plane; pick a flat face (edges set an axis: Axes in
the plane)"), so the command, the app API and the page can
never reach it half-way. An edge or a point that reaches the plane
draft anyway (the user switched the pick mode by hand) is refused by
name in the status line and the drafted face stays.

**Why.** An edge cannot define a plane on its own, and two edges only
did so through the face they both bound (#330 review: their outside
came from that shared face's winding) - i.e. picking the face itself
was always the honest pick. Edge snapping on a face pick was the
reported bug. Edges keep a real job on the same page: they set an
in-plane axis (#557, "Axes in the plane - pick an edge or two points").
The page now says which pick each row takes: "Lay on a plane - pick a
face" and "Axes in the plane - pick an edge or two points for +Y and/or
+X".

**What was dropped with it.** The two-edge datum code (`edge_datum`,
`shared_face`, `bounded_faces`, the first-edge in-plane rotation of the
lay) and its tests: the concave L-bracket tests now pin the step FACE
laying outward up (the winding still decides the outside, never the
part's centre), and the edge cases pin the refusal. The 3-2-1 work
offset alignment (#187) and operation-geometry edge picking are
untouched.

**Considered and dropped.** Keeping the two-edge mode reachable from
the app API only - dropped, a half-reachable mode is what the issue
says not to leave. Snapping to faces only near the centre of a face -
dropped, Face mode already does it exactly.

**Cost to change later.** Low: the edge refusal is one match arm in
`pick/plane.rs::datum_of`; the draft's mode is set in `plane_pick::arm`.
