## #326 - Part alignment per setup (first slice)

**Decided.** A setup carries `part_alignment: Option<Location>` on its
`SetupDef`: the rigid transform from the document frame into that setup's
frame. `None` is "as modelled" and is never stored as `Some(identity)`.
Only parts (models that are not fixtures) read through it; the setup's
stock, work offsets and fixtures are setup furniture already placed in
the setup frame, so they read their own `Location` unchanged. The work
offset (`setup::wcs_slot`, G54..G59) is not touched and stays a plain
translation; rotated offsets stay refused at the post.

- **Command.** `AlignPartToSetup` (undoable, validated and snapshotted at
  construction). `new(world, setup, Option<Location>)` takes any rigid
  transform, which is the door #330's face/edge picks will use.
  `preset(..)` and `angles(.., [x, y, z] degrees)` build the transform as
  a turn about X, then Y, then Z **about the centre of the parts'
  bounds**, so a flip leaves the part on the same spot of the table.
  Quarter turns are built from exact quaternion components, so a flip
  moves vertices by exact sign changes.
- **Presets are absolute.** Each one replaces the setup's alignment; they
  do not stack. The typed angles cover combinations. Dropped: stacking
  presets (it makes "what is held now" depend on click history).
- **Stock follows like it follows `MoveModel`.** A stock made from a part,
  or the stock of a one-part document, is moved by the change. A box or
  cylinder is rebuilt square to the setup frame (re-fit to the turned
  block's bounds, rotation identity, so the program sim's axis-aligned
  grid still judges it). A cylinder that would lie on its side, and a
  following stock shared with another setup, are refused by name. A box
  that keeps its size (a flip about the centre) is only moved, not
  re-meshed, so its geometry revision stays put.
- **MoveModel composes.** A stock that follows a document move but sits in
  an aligned setup takes the move as that setup sees it (the move
  conjugated by the alignment).
- **Datum transfer carries it.** `CreateSetup` with `datum_from` copies the
  source's alignment along with its stock and offsets (they sit in that
  frame).
- **Project v53.** `SetupDto.part_alignment {translation, rotation}`,
  absent = as modelled, so every v52 file loads unchanged. A non-rigid
  transform in a file is refused as corrupt. Not breaking: nothing that
  loaded before is refused; an old build refuses a v53 file by version,
  which is the point (it would machine a flipped setup unflipped).
- **Stamps.** An entity source's pose is digested in the setup frame, and
  an aligned setup writes its alignment into every op's stamp. Nothing is
  written for an as-modelled setup, so no existing stamp moves; clearing
  the alignment brings an op back to Ok without regenerating.
- **Reads wired to the setup frame:** the op's material (`named_material`),
  Top/Bottom of Model heights, the program sim's part, stock fit to models
  (Fit and Fit sides on the Stock page), stock from model, the part-size
  readout, the viewport's part drawing and picks (`setup_align::view_pose`,
  the active setup's frame), face/vertex/3-2-1 pick geometry.

**Refused by name in an aligned setup (this slice).** Generation is wired
and pinned for Face, 2D Contour, 2D Pocket and 2D Adaptive over a whole
model or the stock. Every other kind, picked faces/edges as the source,
area controls, a 3+2 tilt, and any non-mill machine (lathe, mill-turn)
are refused at `collect_job`, the one door every generation passes - so
an aligned setup never cuts in the unaligned frame. The program sim
refuses a program that spans setups holding the part differently
(carrying cut stock between alignments is #317). An op generated before
an alignment change is Stale and never posts.

**Not done here (follow-ups).** 3-2-1 / face-edge picks writing the
alignment (#330); the transform between setups (#316, derived from two
alignments); the move gizmo still edits the document pose and is drawn
at it; the setup sheet reads the document pose. Widening the verified
kinds means wiring each kind's remaining world reads (selection walk,
area controls, drill features, rest prior) through `setup_align` and
pinning it, then dropping it from the refusal.

**Cost to change later.** The stored field and the file key are the
long-term shape (#316 shares them). The refusal list is meant to shrink
kind by kind.
