## #369 - lathe holder bodies judged against the revolved stock

- **Chosen: the one `ToolBody` component, now carrying its frame**
  (`tool_body::BodyFrame::{Spindle, Lathe}`). A milling body keeps
  #368's axis/offset placement and revolved envelope. A lathe body
  carries its turning-frame placement and its clearance bands. Why: a
  tool has one body. `SetToolBody` (new `lathe` / `lathe_from_file`
  constructors), the undo snapshots in `DeleteTool` / `DeleteEntity`,
  the project file and the op stamp all follow without new plumbing.
  `fits` routes by frame, so `ChangeTool` and the loader refuse a
  mismatch by name. Dropped: a separate `LatheBody` component, which
  would need its own snapshot in both delete commands, its own file key
  and its own old-reader guard. Also dropped: a dummy `HolderProfile` on
  a lathe body, which would be a sentinel a milling law could misread.
- **The turning tool frame** (`camcraft_tools::lathe_holder`), taken
  from the existing turning model rather than invented:
  - The **tip** is the point the moves carry. For an insert or boring
    bar that is the nose centre (the M5.2 E nose-centre convention). For
    a grooving, threading or form blade it is the tip at its band centre
    (#103, #21).
  - **+x is radial**, away from the spindle, in the path's own meridian
    half plane (`SignedMeridian`: the side the first off-axis point
    stands on, the side the tool works). On a facing pass past centre
    the tip's signed radius goes negative and the holder stays on its
    side, as a real holder does.
  - **+z is lathe +Z**: the station grows from the chuck toward the free
    end facing faces (world up along the fitted axis, `Frame::about`).
  - **+y = z x x**. The part spins, so only |y| is ever read.
- **Placement: nine numbers.** The model direction that is +x, the
  model direction that is +z, and the model point that is the tip.
  Directions within 0.01 degree of square are squared; anything worse
  is refused. Dropped: reusing #368's axis+offset, because one
  direction cannot fix a lathe frame (a holder has no symmetry axis).
  Automatic placement from vendor frames is #370.
- **Refusals by name:**
  - A zero or non-finite direction, directions not square, a tip point
    that is non-finite or more than 10 m out.
  - An empty, non-finite or degenerate mesh, or one with no length along
    the spindle. A body reaching more than 2000 mm from its tip.
  - A tip more than 50.8 mm from the body's box. That is twice the
    largest ISO insert inscribed circle: a holder carries its insert, so
    a tip that far out is a placement typo.
  - A lathe body on anything but Turning, Groove, Thread, Form or Bore.
    A spindle body on the four insert kinds is still refused, and its
    message keeps "lathe holders".
  - Bore takes either frame. A lathe body on a Bore used by a
    non-turning op (a drill cycle) refuses generation by name
    (`collect_job`), because no milling law can read it.
- **Clearance description: per fine band along z** (at least 0.25 mm,
  at most 2048, not merged): x_lo, x_hi, |y|_lo, |y|_hi. Each facet is
  clipped exactly to each band (#368's Sutherland-Hodgman clip, shared).
  With the tip at signed radius X:
  - r_lo = hypot(dist(0, [X+x_lo, X+x_hi]), |y|_lo)
  - r_hi = hypot(max(|X+x_lo|, |X+x_hi|), |y|_hi)

  Every body point in the band lies in [r_lo, r_hi]. This is the box
  hull, so it can only over-report. The brief suggested "min x only,
  ignore y". That is right for the floor: y only adds distance, and an
  OD holder rubs from above. It is not enough on its own:
  - An ID bar rubs the bore wall with its far side across the axis, or
    with its flats. That is the ceiling, and it needs max |X+x| and
    max |y|.
  - |y|_lo lifts the floor when a band does not straddle the tool plane
    (the tightest bound separable per band).

  Dropped: merging to 32 bands. #368 merges because its swept-capsule
  cost grows per band. Here the cost is linear and fine bands are
  tighter.
- **Judging (`operation/turning_holder.rs`, `turning_holder_scene.rs`)**
  happens at generation, after the bar feed law (#146).
  - **Which moves:** every move, feeds and rapids alike, read at 0.25 mm
    steps. Meridian arcs are chorded at 1e-3 mm, and a thread's helix is
    read at its constant radius (`kerf_of`). A move off the turning
    plane is refused by name, never passed unjudged.
  - **What stands:** the part, always. The stock stands less the cuts
    before the instant:
    - the ops ahead's nose kerfs and the axial pilots;
    - the program ahead's facing (`faced_end`, simulated only after a
      stock-only conviction);
    - this path's earlier cutting moves;
    - this move's own sweep up to the instant.

    That is #146's reading, generalised to every footprint through the
    sim's own `LatheProfile::swept_outline` (the convex footprint the
    field cuts). A station line meets the swept footprint dilated by
    its corner plus the margin in one interval, computed exactly
    (polygon, edge bands, vertex discs). It is credited on both signs
    of the radius.
  - **The station table:** every knot of the part and stock outlines,
    every facet vertex station, and a 0.25 mm grid. Between two table
    stations a skin runs straight.
  - **How a band is read:** at both of its ends (skins interpolated from
    readings just inside the interval) and at every table station
    inside it. So the worst reading against a skin is exact. A hole
    wall takes the narrower of its two neighbours, which is exact for a
    revolve's tessellation. The cuts are read exactly at those stations
    and sampled between them, as every meridian law samples.
  - **Dropped:** the first cut of this law read each table station with
    the body's bands dilated over its neighbouring intervals. That
    convicted a grooving-blade holder 0.05 mm inside its band. The
    band-end reading replaced it.
- **The excluded (insert) zone is the tool's own modelled footprint**,
  swept from the move's start to the instant: the nose disc, the blade
  rectangle, the thread V, the form crown. It is grown by the margin,
  as every meridian law's kerf credit is. This is the conservative
  choice: it is exactly what the cut model says the insert removed, so
  the holder over the insert's flank never rubs the insert's own chip,
  and nothing else is excused. Considered and dropped:
  - **A fixed "insert radius" about the tip.** No tool field states the
    insert size, and it would hide a holder lug inside it.
  - **Crediting the move's whole kerf, or the whole pass.** That hides a
    body part leading the edge into uncut stock.

  Consequence: a mesh that models the insert plate beyond the nose
  circle (a 95 degree holder assembly with its insert) can report the
  side edge's chip as a rub. That is an over-report, the safe
  direction, so import the holder without the insert. **Follow-up:** an
  optional insert-zone number, or #370's insert frame, to separate them.
- **The finding: a new `Warning::HolderHit { at, part, short_mm }`.** It
  blocks the post, as every warning but the scallop does. `short_mm` is
  the clearance shortfall: the radial overlap of the body's span with
  the standing material at the worst instant, the part's in preference
  to the stock's. The post refusal reads "<op>: holder body at move N
  rubs the part - X mm short of clearance". Dropped: reusing
  `ShankHit`, which carries no shortfall and is the bar law's word.
  `Warning` is a persisted enum, and this variant is additive.
- **"holder not judged".** A turning op whose turning steel has no lathe
  body carries `tool_body_lathe::HOLDER_NOTE` in its path notes. It is
  informational and never blocks. CL goldens and post goldens carry no
  path notes, and every turning, lathe, golden, project and undo suite
  stayed green with it, so it was kept.
- **Project file: no version bump.** A lathe body is written as `lathe:
  {radial, along, tip}` with `axis` and `offset` absent. A v58 reader
  that predates #369 then refuses the file (missing field `axis`)
  rather than reading lathe-placed facets as a spindle body. v58 itself
  is unreleased (#381). A spindle body's bytes are unchanged. The
  clearance bands are re-derived on load and never written.
- **Stamp:** a lathe body adds `lathe-body` and its clearance numbers to
  the op digest. No digest moves without one.
- **Cost to change later:**
  - The frame and placement are file format (the `lathe` key).
  - The judge sits behind one call in `generate`.
  - Swapping the band hull for per-band x sub-intervals (tighter for
    bodies with radial gaps) is local to `lathe_holder.rs`.
