## #339 - fixture collisions judged against the fixture's real mesh (slice 1)

**Decided:** the world-AABB fixture laws (c2)/(d2) stay the broad phase,
and a tagged model's closed world mesh gets the last word on every
upright move its box flagged - it can only ACQUIT, never add a hit
(`camcraft_sim::verify::mesh_refined`, `camcraft_sim::fixture_mesh`). A
path down a vise's gap (inside its one box, nowhere near its steel) now
posts; a flute clipping a jaw corner, a rapid dipping into a jaw top, a
holder reaching over a jaw, and a tool standing wholly inside steel
still block the post. Same verdict at generation and at the post gate.

**Why acquit-only:** the box laws carry deliberate silences the mesh
must not undo (the strict-dip z rule: stock seated on a plate, a retract
off the mounting face). Acquit-only keeps every one of them, and means a
false hit can only survive where the box already hit - the conservative
direction. Where nothing is acquitted the report is the box laws' own,
byte for byte (labels included); every existing fixture suite is
unchanged (their fixtures are single triangles - open meshes - so they
keep the box verdict by law).

**The narrow phase:** each band of the stack (shank, holder, spindle
housing when stated, flute - the box laws' order) is an upright cylinder
swept along the tip's straight segment: exactly a convex Minkowski sum,
so GJK (Gilbert-Johnson-Keerthi 1988, van den Bergen's lower-bound
termination) decides it against each triangle. "Clear" is returned only
on a PROOF - a support-plane gap bigger than `FIXTURE_HIT_TOL` (+ the
chord's deviation on an arc) - so float trouble in the simplex can only
cost a false hit. A band clear of every facet is either wholly outside
the solid or wholly inside it; one point decides, by ray-crossing
winding number + parity with six fixed non-axis-aligned directions, any
edge/vertex/grazing crossing discarding that ray, all-ambiguous reading
as inside. A BVH (object-median AABB tree) over the facets, pruned with
the same GJK proof at every node, keeps it near log n.

**Measured (release, Apple Silicon):** 203,320-facet closed sphere -
BVH + validation 0.05 s on first use; 2000 passes across it 0.015 s;
the worst case (2000 passes grazing the ball 0.05 mm off, every one in
its box, every gap proven) 0.74 s. Debug: 30k facets, 100 passes 3 ms.

**Considered and dropped:**
- *Ball-swept (capsule) enclosure* of each band: simpler distance test,
  but the ball reaches a radius below the tip - a false hit on every
  floor within r of the cutter, which is the vise case itself.
- *Surface distance only, no inside test:* would acquit a tool buried in
  a solid block (far from every facet) - a silent crash. Dropped.
- *Generalized winding number* (Jacobson et al. 2013) for open meshes:
  robust to cracks but O(n) per query without a hierarchy; deferred.
  Today an open mesh (unpaired edges, T-junctions) refuses by name
  (`MeshError::Open`, logged) and keeps the box verdict.
- *Judging posed moves by mesh:* their pose changes along the move (the
  swept stack is no longer convex); they keep the box/posed-steel
  verdicts. So do moves with a turned C, XZ/YZ arcs, a lathe's tools
  (insert, blade, thread, form - not an upright stack), mill-turn and
  lathe machines, and a box two fixtures share.

**Limits stated:** a fixture of overlapping solids with OPPOSITE
orientation reads the overlap as outside (a broken model; parity and
winding both fail there). Vertices pair by exact bits; a cracked STEP
needs `RepairModel` first. The BVH is rebuilt per generation job and once
per setup per post-gate pass (no cross-call cache yet).

**Cost to change later:** the mesh law is one function behind one
closure seam - #368's `HolderStack` replaces `steel_stack`'s holder band
and the closures call the `_held` laws; nothing else moves.

**Merged with #464 (stack_span, 2026-11-09):** law (d2) now reads an
upright rapid's whole stack and names a band above the flute by its part
(the flute column stays `Rapid`); law (c2) reads each band over the
move's true z span. `mesh_refined` already wraps both laws through its
closure, and `steel_stack` is the same band list as
`stack_span::upper_bands` plus the flute, so the bands judged agree.
Chosen: the mesh's rapid verdict now names the band the same way (only
the flute reads as `Rapid`), pinned by
`a_rapid_kept_by_the_mesh_names_its_band_like_law_d2`. Dropped: keeping
every mesh rapid hit as `Rapid` - after #464 that renamed a shank or
holder strike the box law itself names.
