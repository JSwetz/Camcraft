## #27 - M12.1 stage 2: union/intersect and edge blends on the kernel seam, benched (2026-11-08)

(1) **The seam grew three operations, nothing else.** `boolean_union`,
`boolean_intersect` and `blend_edges(solid, picks, radius, Round | Chamfer)`
join stage 1's subtract/offset/measure. Both engines implement the booleans
(truck through truck-shapeops `or` / `and`); truck has no edge blend, so its
`blend_edges` is `Unsupported` and the blend bench is solidcore against closed
forms only - recorded as a one-engine bench, never as cross-engine agreement.
No production path moved: nothing outside `camcraft-kernel` depends on it.
Dropped: variable-radius and surface-surface fillets (no closed form we can
pin cheaply, and no consumer yet); tessellation (still unported upstream).

(2) **Edges are named by points, not ids.** A pick is a model-space point
that must lie within `PICK_TOL` (1e-3) of exactly one edge: the distance is
256 samples per edge plus a golden-section refinement. A pick within
tolerance of two edges (a vertex, or a rim point on a cylinder's seam line)
is refused as ambiguous, as is a second pick on an already-picked edge. Why:
engine edge ids are not stable across engines or across a boolean, and the
first probe showed a rim pick at the seam silently resolving to the seam
edge. Dropped: picking by face pair (needs a face-naming scheme the seam
does not have yet - face/island recognition, M13.x).

(3) **The port stays on its default path; the arc-chamfer finding is pinned
as "not yet".** A chamfer along a circular rim leaves the Pappus closed form
by ~1.9e-6 relative on SMLib 1.0.1's default path (upstream WR-421,
solidcore#420: the chamfer is traced as an offset intersection, not made as
a cone). The port's opt-in fix `fixes.fillet_chamfer_cone_on_arc` closes it
to 4e-8. The seam keeps the default (stage 1's rule: bench numbers mean
something only against the pinned upstream behaviour);
`not_yet_rim_chamfer_default_path_stays_a_finding` pins the miss and
`rim_chamfer_agrees_with_cone_on_arc_fix` pins the fix. Cost to change:
flip the flag in `SolidcoreKernel::default()` and promote the fixture - do it
when a production path first chamfers arcs.

(4) **The blend radius guard is conservative on purpose.** `blend_edges`
refuses a radius of half the solid's thinnest extent or more, then requires
the result to be a closed manifold solid, inside the source's bounding box,
whose volume actually moved. The port computes an r6 round on a 10-cube
edge correctly (pinned in the stage-2 port table), but two such rounds on
one face would collide, and the seam cannot tell those cases apart without
per-face geometry. A visible refusal beats a guessed pass. Like stage 1's
offset guard, it sees only the overall extent - a per-feature check is owed
with face recognition.

(5) **Empty boolean operands are refused at the seam.** The port answers
`box & empty` with the whole box (`Ok solid V=1000`) - a wrong answer
dressed as a solid, pinned in `KNOWN_PORT_VERDICTS_STAGE2` and owed
upstream. The seam refuses a face-less operand for all three booleans
before the port sees it (`box - empty` was right, but an empty operand is a
caller bug). Stage 1's pinned table is unchanged; the new rows sit in their
own table.

(6) **Agreement verdict, stage 2.** Union and intersection agree on the four
new fixtures: solidcore within 1e-8 of the closed forms, truck within its
mesh budget, cross-engine gap at most 3.4e-2 (truck's tessellation, and it
closes at the heavy lane's finer tolerance). Eleven blend fixtures (cube
rounds/chamfers on 1, 4 and 12 edges, concave L-block blends, cylinder and
hole rims) match their closed forms within 1e-6 relative; the arc
chamfer agrees only with the opt-in fix (decision 3). Still owed before truck's kernel role retires: a
corrupt-input corpus through a solidcore importer (none exists - STEP stays
on truck-stepio), M4.2's wasm CI job taking the crate (a local `cargo check
--target wasm32-unknown-unknown -p camcraft-kernel` passes), healing, and
the first production consumer switched behind this bench.
