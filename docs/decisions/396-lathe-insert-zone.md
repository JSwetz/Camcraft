## #396 - an insert zone on a lathe holder body

- **Chosen: an optional box in the turning tool frame about the tip**
  (`camcraft_tools::lathe_insert_zone::InsertZone { lo, hi }`, on
  `BodyFrame::Lathe { zone, .. }`). Whatever of the body lies inside it
  is left out of the clearance bands; every facet crossing a face keeps
  each part of it outside, exactly (one Sutherland-Hodgman clip per
  face's outer half space). The mesh, the display and the placement are
  untouched; the stamp moves because the bands do. Why a box: an insert
  plate is a slab under the rake face, so three spans say it, and a box
  is what a user can read off a vendor drawing. Considered and dropped:
  - **A length along the cutting edge.** It needs the edge's direction,
    which no tool field states (it is #370's vendor insert frame).
  - **Excusing the zone from stock only, never from the part.** The
    plate's box reaches the nose circle's bottom, on the finished skin,
    so a zone that still read the part would not clear a plain OD pass.
- **The bound: one insert, by the tool's own record, never the whole
  body.** Refused by name at the command, at `ChangeTool` and on load:
  - The box must hold the tip (the plate carries the cutting edge) and
    be finite.
  - Along each axis it may reach from the tip no farther than the
    tool's body size (`diameter`, the shank or bar the insert sits on -
    no insert clamped on it outgrows it). Along the spindle, a grooving
    or form blade's zone reaches at most half its `width`, because the
    tip is the band centre and the blade is the band. A thread insert's
    laydown plate is wider than its V, so it takes the body size.
  - Every cap is held to 25.4 mm, the largest ISO turning insert's
    inscribed circle (the same figure `MAX_TIP_GAP` doubles).
  - Something of the body must stand outside it.

  Why: no tool field states the insert's size, so the tightest honest
  cap is the steel the insert sits on. Dropped: a fixed 25.4 mm alone,
  which on a 10 mm boring bar would excuse 25 mm of bar. **Residual,
  stated plainly:** within that cap the zone excuses whatever it holds.
  A zone stretched over the holder's own nose region (inside one
  insert's reach) hides that region; the holder beyond it is judged as
  before, and the shoulder rub test pins that a zone over the plate
  never hides the shank's foot. The insert's own cutting is still
  judged by its modelled footprint in every turning law; the zone only
  stops the holder law reading the plate a second time.
- **Command: both routes, one command.** `LatheBodySpec` gains `zone`,
  and `SetToolBody::insert_zone(world, tool, Option<InsertZone>)` sets
  or clears the zone of the lathe body already on a tool, keeping its
  mesh and placement. Both are `SetToolBody` (snapshot prev/next), so
  undo/redo and the delete snapshots need nothing new. Dropped: a new
  component or command type. A zone on a tool with no lathe body is
  refused by name. `lathe_from_file` keeps its signature (no zone);
  the zone follows with `insert_zone`.
- **Project file: no version rung.** The zone is an optional
  `insert_zone: {lo, hi}` key beside `lathe`, absent when there is
  none, so a zoneless body's bytes are unchanged. The project DTOs do
  not `deny_unknown_fields`, so a reader that predates #396 skips the
  key and judges the whole body, plate and all. That is an over-report
  that blocks the post, never one that passes a rub, so the old reader
  is safe without a rung. Its re-save drops the zone, which only brings
  the refusal back. A rung would also collide with the other open PRs
  bumping the version (#381 is at v58, unreleased). A spindle body
  carrying `insert_zone` is refused on load by name.
