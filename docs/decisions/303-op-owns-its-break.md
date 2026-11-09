## #303 - the chamfer and the deburr own their edge break in the program sim

**Decided.** A 2D Chamfer and a Deburr declare the edge break they were
asked to cut. The declaration is a new `cl::EdgeBreak` record on the path:
the edge as a polyline at its own height, the bevel's depth at the edge,
and its slope. The generator writes it from the op's own numbers (the
chamfer: the picked loops at the top plane, `width * tan(angle)`,
`tan(angle)`; the deburr: the edge runs it found, its drop, the cone's
`cot(half-angle)`). It is not read back off the moves. After `deviate`,
the program sim (`stock_sim::credit_breaks`) re-judges each gouge column
that lies within a declared break's plan width, under a model top no
higher than that edge. It is judged against the bevel (the cone swept
along the declared edge) instead of the sharp model. Down to the bevel
within `DEV_TOL` is clean. Below it is still a gouge, charged at its depth
under the bevel. Rest is unchanged. The record is persisted as an additive
`breaks` key in the cached path (absent = none).

**Why.** This is the issue's suggested option 1, "the op owns its break",
with one change. The bevel comes from the op's stated parameters, not from
its swept moves, so a ride deeper than its stated break still convicts.
A break on an edge the op never picked still convicts too, because nothing
declared it. The rule is also engine-agnostic: one pass over the verdict,
with no change to `deviate`/`deviate_field`, so it stays clear of #481's
wall-slack change in those functions.

**Deburr: steep edges are skipped.** Reproducing showed the deburr's
deeper gouge columns were not link rapids. They were the vertical corner
edges: a vertical cone riding a vertical edge, apex `drop` under each
point, countersinks the corner down to the window floor. With the default
window that is 9.8 mm on the 10 mm brick, a real gouge the sim was right
to refuse. The generator now skips any edge that climbs steeper than the
cone's flank (`|dz| > cot(half) * plan length`) and says so in a path
note. Two kernel tests that encoded the corner ride were updated (the
box's level creases still ride, and the window-clip law is now pinned on
a sloped rim).

**Considered and dropped.**
- A per-op tolerance (option 2) needs per-column cut provenance in the
  sim and still cannot tell "deeper than stated" from "stated".
- Crediting the op's own swept cone (option 1 as written) would credit an
  over-deep ride: the moves would be the declaration.
- Refusing at generate (option 3) means no chamfer could ever post on a
  model drawn without the chamfer, which is the op's whole use.
- Recomputing the declaration at sim time from the op's region would mean
  re-running `region_for_job` in the sim. A record on the path is cheaper,
  and it is persisted like `probes`/`threads`.

**Limits, accepted.** A tilted (3+2) frame drops the breaks: a posed
program is refused by the verdict anyway. In a program spanning setups
that hold the part differently, only the final frame's ops' breaks are
credited, which is stricter, never looser. A cache saved before this
change has no `breaks` key and is judged against the sharp model until it
regenerates.

**Cost to change later.** The rule is small (`edge_break.rs` in cl and
sim). Changing the record format is an additive file key.
