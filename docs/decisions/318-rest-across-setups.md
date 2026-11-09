## #318 - Rest machining across setups, and staleness that crosses them

**Decided: rest crosses a setup boundary exactly where the program sim
carries the body (#317), by call.** `rest::across::chain_of` walks back
from the consumer's setup through the program; at every boundary it asks
`stock_sim::carry::entry` - the function the sim's own walk uses - and
extends the chain on `Carry`, ends it on `Clip`. The rest grid for a
crossing chain is the origin setup's block, each segment's cut moves
swept, the body carried at each boundary (`rest::across::carried_grid`);
the verify prior (`PriorPlan`) does the same on the job thread, with the
origin body built on the caller's thread. Considered and dropped: calling
`stock_sim::incoming_body` directly - it walks `posting::program_ops`,
which refuses the whole program over any Stale or never-generated op
(including the consumer being regenerated), so it cannot serve
generation; the shared seam is `carry::entry` + `Body::carry` instead.

**Decided: a clip-entered setup keeps its own fresh block (the pre-#318
law).** Two setups that hold the part alike and are not linked are
entered by the M3.2 A clip. Whether the same part or a new block is
loaded there is not stated until #320's per-setup stock-source choice;
the fresh block is the side that never assumes material gone (re-cuts air
at worst). Considered and dropped: reading the clip's "earlier cuts stand
below the new top" for rest - if the user loaded a new blank, that
assumes material gone that is there (full-width engagement, rapid into
stock: gouge-class).

**Decided: a refused carry refuses the rest reading by name.** If the
consumer's chain must cross a boundary `carry::entry` refuses (a turn the
lattice cannot carry exactly, a lathe or mill-turn), every op that reads
the program ahead (`operation::reads_prior` - Pocket3D, Drill and every
unposed op) is refused at `collect_job` with `rest::REST_CARRY_REFUSAL`
plus the carry's own reason naming both setups. Never generated against
the setup's own block. Considered and dropped: refusing only when
something ahead actually cut - the incoming stock of a carried setup is
the earlier setup's block even uncut, which may stand outside the later
setup's declared stock.

**Decided: the rest grid reads the carried body's tops.** A column cut
through on its far side still reads its standing top (more material,
never less); masks only subtract, so the worst case is air cutting.
Considered and dropped: `Body::display()` (cut-through columns as air) -
exact, but it would hand the mask a less-conservative reading for no
pinned gain.

**Decided: the stamp.** A crossing chain's digest (`rest::prior_digest`)
is the predecessors' path digest (unchanged law: cached paths, not
intentions) plus the chain's own term: the setups crossed, each lattice
turn (round-trip exact), and the origin setup's stock definition. So
regenerating a Setup1 op, changing the re-clamp (alignment, link,
unlink), changing Setup1's stock, deleting/reordering ahead all stale the
later setup's rest op; undo restores the old digest. A refused chain
digests its refusal text. A chain that does not cross writes exactly the
old digest - single-setup stamps and goldens are bit-for-bit unchanged.
Suppression does not move the digest (the mask is what was planned, as
in #161); `rest::suppressed_cuts_ahead` now walks the chain, so the post
refuses a Setup2 op generated against a suppressed Setup1 cut by name.

**Left to #319:** the raw-stock laws in a later setup (rapid-in-stock
convictions read the setup's own declared stock before the prior can
acquit; Face's top-stock credit, area/region builds) still read the
setup's own block. The carried prior only acquits where the carried body
truly is cut, which is safe; convicting against carried material that
stands OUTSIDE the declared block is #319's verification work.
Same-setup only, unchanged: `drills_ahead` / `turned_ahead` (the turned
family runs on a lathe, where the carry is refused anyway).
