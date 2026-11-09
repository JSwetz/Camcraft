## #317 - Program sim: the machined body carried into the next setup

**Decided: one body, re-indexed, never re-sampled (#308's research
decision).** The column body is kept on ONE lattice and viewed through
each setup's transform (`setup_relative::transform_between`, #316). A turn
that keeps the tool axis vertical (flip over X or Y, quarter turns about
Z, their products: the eight signed permutations keeping Z on +-Z) maps
every column centre exactly onto a column centre of the image lattice,
so the carry moves numbers between cells and loses nothing
(`camcraft_sim::stock_sim::{Body, LatticeTurn}`). Considered and dropped:
re-sampling the dexel onto each setup's grid (loses accuracy at the grid
pitch, which is what #308 rejected); carrying the field/SDF engine and
cutting each later setup's moves posed in the first frame (the field's
verdict reads heights marched from above, so a posed verdict is not
judged today - the same reason posed programs are refused).

**Decided: the body is two-sided.** Each column holds a floor as well as
a top (the stock's bottom: the box/cylinder placement, a from-model mesh
read from below). A flip swaps the sides (`top' = tz - floor`, `floor' =
tz - top`); a column cut through stays cut through. Once the floor has
faced a tool (after a flip) the verdict judges it against the part's
bottom as well as the top against the part's top; per column the worse
stands. A body never flipped is judged by exactly the old one-sided
column verdict, so single-setup verdicts (and goldens) do not move. A
cut-through column is air to the eye and a gouge to the verdict.

**Decided: when the body is carried.** Entering a setup that holds the
part differently from the one before it, or one linked to it with
#316's "Relative to previous setup" (including "Same as"), carries.
Two setups holding the part alike and not linked keep the M3.2 A CLIP.
This is #320's planned default ("from previous setup once a transform
is set, new block otherwise") before #320 makes it a per-setup choice.
A carried setup's own stock entity is not read by the sim: the incoming
stock is the carried body. The verdict is read in the frame of the
setup the program ends in.

**Refused by name (not verified):** a turn the lattice cannot carry
exactly (side-on turns, Z turns off the quarter, mirrors:
`CARRY_TURN_REFUSAL`, naming both setups); a lathe or mill-turn machine;
lathe, rotary and posed programs (refused before the walk, as before);
setups with different machines (#390's refusal). Fixtures are not part
of this verdict (the collision gate's), so nothing changes there.

**Exposed for the next issues:** `stock_sim::incoming_body(world,
setup)` - the body a setup starts from, in its frame - for #318 (rest
across setups) and #320 (incoming-stock display). The field view and
playback still draw one setup (their refusal is reworded to say the
program verdict carries the stock).

**Performance:** a carry is one pass over the lattice (O(cells), at most
1023 x 1023 per the `auto_cell` law) and the two-sided verdict is a
second `deviate` pass only after a flip; no new budget.
