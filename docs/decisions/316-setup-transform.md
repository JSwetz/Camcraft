## #316 - Setups: the rigid transform between setups

**Decided: one transform per setup, not two.** #326 already stores each
setup's part alignment (a rigid transform document frame -> setup frame).
#316 adds no second transform: the transform from setup A to setup B is
derived, `T = A_B * A_A^-1` (`setup_relative::transform_between(world,
from, to) -> Result<Location, String>`). It composes (A -> C = (B -> C) *
(A -> B)), inverts (B -> A = (A -> B)^-1), is the identity for a setup and
itself, and refuses by name a name that is no setup, a missing setup, and a
stored alignment that is not finite or not unit-rotation - never an
identity guess. #317 (carry the machined body) and #318 (rest across
setups) read it. `carry_point` is the one-point convenience.

**Decided: a setup may be placed relative to another, stored as a link
only.** `SetupDef::relative_to: Option<String>` names the setup this one is
a re-clamp of. The turn is NOT stored beside it: `SetSetupRelative::link
(world, setup, parent, [x, y, z] degrees)` resolves "flip over X / turn 90
about Z / custom angles from <parent>" into the setup's part alignment
(`A = Turn_about_centre_in_parent_frame * A_parent`, through
`AlignPartToSetup`, so the following stock is rebuilt exactly as #326 does)
and writes the link; the relation shown ("Flip over X from Setup1") is read
back from the two alignments. `unlink` drops the link and moves nothing.
Considered and dropped: storing the relative turn and resolving the
alignment on every read (every #326 reader would have to walk links, and a
parent edit would move a child's part without its stock following -
gouge-class).

**Decided: a parent's change carries; it is never left behind silently.**
`AlignPartToSetup` on a setup with linked setups re-places every one of them
(transitively, parents first) so each keeps the transform it had to its
parent, with its following stock, in the same undo unit
(`carried_count()` says how many). If any linked setup cannot follow (its
cylinder stock would lie on its side, its stock is shared), the WHOLE change
is refused naming that setup. Considered and dropped: refusing every edit of
a linked parent (forces unlink/relink for the common "I re-did setup 1"
case) and a stale flag (a flagged child would still machine in the old
frame until someone noticed). A child's own absolute alignment edit keeps
the link: the relation then reads whatever the new difference is, truthfully.

**Decided: loops and orphans are refused.** A link to itself or one that
would close a loop is refused at command creation; `DeleteSetup` refuses a
setup another is placed relative to, naming it ("unlink it first"); the
loader refuses a link naming no setup or a loop of links as a corrupt file
before any world write.

**Decided: datum transfer does not link.** `CreateSetup` with `datum_from`
already copies the source's alignment (#326), so `transform_between(source,
clone)` is the identity at creation and stays derivable afterwards - the
datum transfer's transform needs no extra store. It does NOT set
`relative_to`: a datum transfer is a clone (its stock and WCS do not follow
the source's later edits either, AGENTS.md / M3.2 A "clone, not alias"), and
making only the alignment follow would surprise. The link is the explicit
choice on the setup page.

**Decided: nothing is derived from the work offset.** The work offset is a
plain translation placed in the setup frame (`G54`..`G59` always from
`setup::wcs_slot`, one WCS per setup, rotated offsets still refused at the
post). It says where program zero is, not how the part moved between
clamps; reading a part transform from two origins would conflate the two
and is exactly the wrong-frame failure the maintainer rule forbids. The
transform is on the part.

**Project file v56:** `SetupDto::relative_to` (absent = not linked, so v55
and older files load unchanged); an old reader refuses v56.

**UI:** the setup's settings page (#390) shows "Relative to previous setup"
for every setup after the first: Not linked / Same as <prev> / the five
flip and turn presets "from <prev>", plus typed angles with Apply. Each
queues `CommandKind::SetSetupRelative` (undoable); refusals go to the
status line.
