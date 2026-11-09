## #171 - Probe op created from a picked face, edge, vertex or round wall (2026-11-08)

- **Chosen:** the seed rides the existing pick command. On a Probe op,
  `SetOperationGeometry` reads its one pick through
  `operation::probe_from_picks` and writes the seeded mode, directions,
  target (and radius for a round pick) into the op's params in the SAME
  command - one undo restores both the source and the typed values. A
  pick the probe cannot touch refuses the command by name; the op stays
  as it was. Creating straight from a pick is `probe_from_picks` +
  `CreateOperation` with `SrcRef::Selection([pick])`. The seed is written
  once, at the command (AGENTS.md: values at command creation); generation
  still reads only the typed fields, so a hand-tuned target after the pick
  is kept until the next pick.
- **Rules:** planar face -> Face, travel against the outward normal
  (normal from facet winding, since the stored plane normal is
  winding-blind), within 1 deg of +-X, +-Y or +Z; a downward face refuses
  (the stylus only descends). Target: the face's area centroid when it
  lies on a facet, else the nearest facet's centroid (a face round a hole
  has its centroid in air). Straight level open edge -> Edge on the one
  vertical X- or Y-facing face hanging below it, target its midpoint.
  Vertex -> outside Corner: every face there axis-facing, exactly one X
  wall and one Y wall below the vertex, and each wall must run away from
  the vertex the way the other touch travels (an inside corner refuses).
  Closed rim chain or welded round wall (patch) -> Bore or Boss: circle
  from `circle_of_region` (Circular's fit) on the top level ring, target
  its centre at rim height. **Bore vs Boss rule:** the area-weighted sign
  of the vertical walls' outward normals against the radial direction -
  pointing at the axis is a hole's wall (Bore), pointing away is a boss
  (Boss); a tilted (cone/fillet) wall refuses.
- **"The op page shows the picked source":** no new persisted field and
  no project-format rung. The pick is already the op's persisted
  `SrcRef::Selection` and the Geometry tab already draws it as a chip; the
  probe branch of `selection_account` (which the tab prints under the
  chips) adds "Probe from the pick: <seed>" or the refusal. No UI code
  changed.
- **Considered and dropped:** re-reading the pick live at generation like
  Circular does (the typed fields would then be dead while a pick stood,
  and the op page would show numbers that are not what runs); a
  `probe_source` label persisted on the params (a v53 rung for text the
  selection already carries); a separate "Seed from pick" button (two
  steps where the pick itself is the intent); a clicked-point target
  (`GeoSel` names a feature by hash, not a hit point - adding one is a
  format change).
- **Not pinned:** an open *curved* edge refuses as "not straight" by the
  same chord test, but no fixture builds one; wall height vs `depth` is
  not checked at the pick (a wall shorter than the touch depth is judged
  by the stylus law at generation).
- **Cost to change later:** low - the seed is one app module
  (`operation/probe_pick.rs`, `probe_pick_round.rs`) and a five-line hook
  in `SetOperationGeometry`.
