## #319 - Generation and verification in a later setup judge the carried body

**Decided.** When an operation's setup is entered with a carried body
(#317's `carry::entry`, through #318's chain), every stock-reading law of
its generation judges a stock that never holds less than the body really
there:

- *Cheap check first.* The origin setup's stock box, carried uncut through
  every turn of the chain, bounds the incoming body (cuts only remove
  material). If the setup's declared stock is a box that holds it (XY and
  top; the bottom is not read by anything judged from above), nothing
  changes - the verdict is the pre-#319 one, byte for byte.
- *Otherwise the real body.* The carried grid (the incoming body, without
  the consumer setup's own earlier cuts) is cut, its material cells
  bounded, and clamped to the uncut block (two supersets, so their
  intersection is one). If the declared box holds that, nothing changes.
  An unreadable body falls back to the uncut block - never to air.
- *Otherwise substitute.* The job's `stock_facets` / `stock_aabb` become
  the box enclosing the declared stock and the carried body. Every law
  that reads them - the rapid-in-stock column and footprint rings, the
  stock top, the entry/plunge solid, the reach and swarf laws, the top-
  stock note, the tilted frame's slab - then judges at least the real
  material. The prior acquittal (#96, carried by #318) still clears a
  dive into what earlier setups really cut. The post gate's WARN records
  are those cached warnings, so the post agrees with generation.
- *The mismatch is said by name, and blocks where it is cut* (PR #410
  review, finding 3). The op's machining region is still the declared
  stock, so a feed move (lead-in, side offset, plunge, or the tool body
  under the carried top) reaching carried material outside the declared
  stock engages stock no law verified. Such a path carries a
  `CARRIED_OUTSIDE_DECLARED` note naming both setups, the overhang (mm
  beside, mm above) and the first move that reaches it. The post gate
  (`operation::carried_problems`, in `verification_report`) refuses on it
  by name and tells the user to enlarge the later setup's stock. Where no
  feed move reaches the overhang, it is only noted
  (`CARRIED_OUTSIDE_NOTED`): the rapids, entries and links were judged
  against the enclosing box, so posting is safe.
  - *Which test: the cheap one, per feed move.* Each feed chord's
    footprint box (chord bounds grown by the tool radius) is clipped to
    the carried body. Then the carried grid's columns under it are read.
    The move engages when a column stands above the move's tip and either
    sits (by its whole cell square) outside the declared box, or stands
    above the declared top. A column cut through still reads its top, and
    an unreadable one engages. If the grid itself could not be cut, the
    whole body box counts as standing to its top. So a near miss refuses,
    but a real cut never passes. The grid is the one already simulated
    for the body's bounds, so there is no second simulation. A first cut
    read the body's box alone: one corner cell left high anywhere in the
    body then refused every feed move. Dropped: refusing whenever the
    overhang exists. That refuses the common "declared stock smaller than
    the earlier setup's blank, op nowhere near the rim" case for nothing.
  - *Why a gate clause over the cached note, not a new `Warning` kind.* The
    warning enum is `Copy` (no setup names), and a new kind ripples
    through the UI, MCP and serde. The note is part of the stamped cache,
    so a fresh op's note is the verdict on the world as it stands. The
    clause blocks exactly like a WARN record and names both setups.
- *Declared material beyond the carried block is present* (#405 review).
  Where setup 2's declared stock reaches past the uncut carried block in
  XY, the carried grid cannot speak for that material - verify's prior
  acquitted a dive there because its footprint disc still caught the
  grid's low edge cells. Then neither the prior (verify) nor the rest
  mask (Pocket3D / Multi-Axis Rough) is read: the raw stock judges and the
  rest pass cuts the declared stock (air where setup 1 cut, never a skip).
  Inside the carried block the carried body stays the truth (#317's law:
  the sim does not read a carried setup's own stock there), so a declared
  block that only sits higher than what setup 1 left does not undo #318.
- *A refused carry refuses every op in that setup*, posed ones included
  (#318 refused only the prior-reading ones); a posed op in a carried
  setup now digests the same rest chain in its stamp
  (`rest::carried_digest`), so its substituted stock can never go stale
  silently. No other stamp moves.

**Why.** Prime Directive: a rapid judged clear through material that is
really there is gouge-class. One substitution at the job seam reaches
every law at once, instead of patching each law; the box is a superset,
so it can only add convictions. The cheap containment check keeps the
common case (declared stock = the block the earlier setup held) and every
single-setup and clip-entered job unchanged.

**Considered and dropped.**
- *Union of footprints as a region* (declared polygon plus the body's
  cells): tighter, but a polygon union per op and a cell-boundary region
  are costly and only the rapid law reads a region - the entry, reach and
  tilted laws read facets and boxes. The enclosing box serves all.
- *Refusing every mismatch*: a user who declares setup 2's stock as the
  finished size of setup 1 (a common habit) would be refused even though
  no feed move goes near the overhang. Only a feed move that reaches it
  refuses.
- *Acquitting lateral rapids against the carried grid*: the same-setup
  law acquits only vertical dives (#96); widening that is its own change
  to every setup's verdict, out of scope here.

**Not done here (owed).** The machining REGION of non-rest kinds (Face,
2D pocket/adaptive, contour) in a later setup is still the declared
stock: carried material outside it is not machined (the post refuses
when a feed move reaches it, and the note says so otherwise),
and a declared stock larger than the carried body still cuts air there
(harmless). Pocket3D and Multi-Axis Rough already bound their cut by the
carried rest mask (#318). Bounding every kind's region by the carried
body is left for #320 (the setup's stock source parameter), which decides
whether the declared stock or the carried body is the setup's stock.

**#320 / PR #408.** That PR adds a per-setup stock source (own block vs
carried). Everything here is keyed on `carry::entry` through #318's chain,
so whichever of the two lands second makes the chain honour the source
(an Own setup breaks the chain like the clip does) - no second rule.

**Cost to change later.** Local: `operation::carried_stock` and two
functions in `rest::across`. A tighter judge (the body's own surface
instead of its box) would replace `judged`'s box with the carried grid as
the job's stock, behind the same seam.
