## #172 - the probe's protected-move law sweeps the stem and the body

- **Chosen:** The Probe op keeps its ball law (`probe_op::stylus_hits`,
  unchanged) and adds a reach law, `probe_op::stylus_reach_hits`. The
  reach law runs the verifier's solid law (`verify::solid_hits`, the one
  the cutting ops' shank bands are judged by) over every probe move with
  a probe stack built by `probe_op::stylus_cutter`: the ball is the whole
  cutting atom, the stylus stem above it is the shank band (ball radius,
  up to the stylus length `flute_len`), and the probe body (`shank_*`)
  plus any holder is the second band. A stem or body that meets the part
  on any move is a `ShankHit { part: true }`; a rapid that meets anything
  is a `RapidInStock`. Both block the post like every cached warning. A
  move the ball law already convicted is not reported twice. The slack is
  the ball law's own `STYLUS_SLACK` (0.01 mm).
- **Why:** Reusing `solid_hits` through its public seam (`Cutter`,
  `SolidLaw`) gives the probe the same swept-solid judgement the cutters
  get, read off the mesh, with no copied geometry. Only the ball is meant
  to touch, so the expected contact excuses nothing for the stem or the
  body: they are judged on every move, end points included. Tangent
  meetings (a stem riding beside the face it touches) pass, because the
  solid law convicts only penetration beyond the slack.
- **Stem radius:** `Tool` states no stem diameter, so the stem is taken at
  the ball radius (the flute band `Tool::profile` already draws). A real
  stem is thinner, so this can only over-report; on the usual touches the
  stem rides tangent to the touched face and stays clean.
- **Body and holder as one band:** `Cutter` carries two bands, and the stem
  takes the first. The body and any holder above it share the second, at
  the wider radius over both lengths - never smaller than the real pair.
  Catalog probes have no holder, so this changes nothing for them.
- **Named as a shank hit, not a new warning kind:** `Warning` is also a
  file format (cached paths), and `ShankHit` on the part already reads
  "shank/holder ... collides with the part or an island" everywhere a
  warning is named (verification report, MCP, cross-part). Stem and body
  share that name. A separate `StylusHit` variant was dropped: a file
  format change for wording only.
- **Ball law kept as is:** replacing `stylus_hits` with the solid law's
  ball verdict was dropped: the existing tests pin its `Gouge` reading,
  and a ball the distance law misses (wholly inside the solid) is still
  kept from the solid law as a `Gouge`, so nothing is lost.
- **Commit type `feat(sim)`:** the issue is filed as an enhancement (a new
  judgement the probe law never made), so it waits for the next minor cut.
- **Cost to change later:** Small. A stem diameter field on `Tool` would
  feed `stylus_cutter`'s shank radius; a third band in `Cutter` would split
  the body from the holder. Both are local to `stylus_cutter`.
