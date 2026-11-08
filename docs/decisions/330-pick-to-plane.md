## #330 - Lay the part on a G17 / G18 / G19 plane from picks

**Decided.** One flat face, or two straight edges, picked on the part,
laid on G17 (XY, normal +Z), G18 (ZX, normal +Y) or G19 (YZ, normal +X),
with a Flip toggle (the opposite normal) and "Face to plane level" (the
picked face, or the first edge's plane, set to level 0 along the plane's
axis; off keeps the part where it is). Two edges: the plane they span,
the first edge laid along the plane's first axis (X for G17 and G18, Y
for G19). The result is written as the setup's part alignment (#326)
through `AlignPartToSetup::lay_on_plane`, which builds the rigid
transform (`pick::lay_on_plane`) and enters through
`AlignPartToSetup::new` - one undoable command, the stock following as
it follows the presets. The work offset is never touched; rotated work
offsets stay refused.

- **Absolute, in the document frame.** The picks are read through the
  part's own placement (`model::transform_of`), not the active setup's
  view, so the answer is the whole alignment, not a turn on top of the
  current one. Picking the same face again in the aligned setup gives
  the same alignment and is refused as "already holds the part that
  way". Dropped: composing onto the current alignment (the answer would
  depend on click history, the reason the #326 presets are absolute).
- **Pivot.** Turned about the parts' bounds centre, like the presets, so
  with "Face to plane level" off the part stays on its spot of the table.
- **Face only: the shortest turn.** The rotation about the plane normal
  is left as it falls (the shortest arc from the face normal to the
  axis). Antiparallel (e.g. the bottom onto G17) is a half turn about the
  plane's first axis when it lies square to the normal - so bottom-up on
  G17 equals the "Flip over X" preset - else about the least-aligned
  world axis. The landing is proven before storing: the turned normal
  must sit within 1e-9 rad of the target axis, else refused.
- **Two edges: which side is out.** Two edges carry no winding, so the
  plane's outward normal is the side away from the parts' bounds centre
  (d1 x d2 when the plane runs through the centre). Edges closer than
  1 degree to parallel are refused: "coincident" when they lie on one
  line (within 0.01 mm), else "parallel". Skew (non-coplanar) edges are
  accepted: their directions still span the plane; the first edge's
  start is the point set to the level.
- **Which setup.** The row's setup on the Setups page (each setup row
  carries the control, like the #326 presets), so a second setup can lay
  a different face down. With no setup there is no row: #326 keeps the
  alignment per setup and has no document-level surface, so this matches
  it (the issue's `MoveModel` fallback was for "until #326 lands").
- **Refused by name:** a curved face (a welded patch), a degenerate face
  (`FaceKind::Degenerate`) or one with no area, a vertex, a closed (round)
  or curved edge, parallel or coincident edges, any other pick count
  ("pick one flat face, or two straight edges"), a pick on the stock or a
  fixture ("is not the part"), a pick no longer in the model.
- **UI: a pick draft, then Apply.** "Pick face or edges" arms a draft
  (`State::plane_pick`, view state like the boundary draft) and Edge
  mode, so one click mode takes both edges and faces. A face click
  replaces the draft; an edge click toggles that edge and keeps the last
  two. The draft picks are drawn in the violet pick outline. Apply is
  enabled only when the app's own reading of the picks succeeds; the
  disabled button's tooltip says why. Only Apply is a command.
- **Shared readers.** The 3-2-1's outward-normal and straight-edge readers
  in `pick::align` take the frame as a parameter (the 3-2-1 keeps the
  active setup's view; #330 reads the document frame); behaviour of the
  3-2-1 is unchanged.

**Cost to change later.** Small: no file key, no new component - the
stored alignment is #326's. Dropping the absolute rule or the edge
outward rule changes only `pick::plane`.
