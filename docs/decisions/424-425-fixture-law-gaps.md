## #424, #425 - the fixture laws read the whole stack over its true z span

- **Chosen:** A new module, `sim::verify::stack_span`, gives both fixture
  laws one reading of "where the stack is along a move". Each straight
  piece of the tip path (a straight move, or an arc's 3-D chords from
  `Move::segments3`) gets a z span `[min(floor, piece low), piece high]`.
  A band `above..above + len` is then judged over the window
  `[z_lo + above, z_hi + above + len]`. That is the band's window at both
  ends of the piece, from min to max, swept along the piece's XY with the
  existing `fixture::band_hits_box`.
  - **Law (c2), `bands_vs_fixtures` (#425):** every band (shank, holder,
    spindle housing, flute column) is judged over that window, floored at
    the move's deepest end. A ramp's high end lifts the whole stack with
    it. The #425 repro (ramp z 0 to 40, arm at z 100..120 over the high
    end, holder top 130 there) now reports `FixtureHit (Holder)`. The
    leaning enclosure takes the same span: its z planes are the axis
    reach at both window ends.
  - **Law (d2), `rapids_vs_fixtures*` (#424):** an upright rapid keeps its
    flute-disc test first, with the same verdict and the same `Rapid`
    label. If the disc is clear, the shank, holder and spindle housing
    (when the `.mch` states it) are judged over the rapid's span and
    named when struck, then the flute column above the tip (reported as
    `Rapid`, the tool body on a rapid). The issue's vertical retract
    beside a bolt reports `Holder`. A low traverse past a tall bolt
    reports `Shank` (or `Spindle` for a column only the housing reaches).
- **Why an enclosing window, floored at the old bottom:** the window only
  ever grows over the one each law always read. A level move's window
  is the old one, computed with the same sums, so every existing verdict
  stands. Every new hit is steel the stack reaches somewhere along the
  move's span. On a long ramp the enclosure can report a graze that the
  real sheared sweep would clear. It can never hide a crash, the same
  trade the bands law's lean enclosure already makes.
- **Strict-dip silences kept:** the z test is still `band_hits_box`'s
  strict overlap. Every band stands at or above the tip, so a retract
  off a mounting face keeps the whole stack at or above the steel and
  stays silent. A plunge landing on a face, a traverse riding it, and a
  band floored exactly on a box top are flush contact, also silent.
  Penetration by a thousandth reports. All of these are pinned in
  `sim/tests/fixture_stack_span.rs`.
- **Non-finite z fails closed:** a piece with a non-finite z (or floor)
  gets a NaN window, which `band_hits_box` reads as a hit. The old
  `min`/`max` on `from.z`/`to.z` dropped a NaN silently.
- **Considered and dropped:**
  - *An exact sheared sweep* (the window at parameter t riding z(t), a
    linear feasibility test inside the XY slab interval). It is tighter
    on ramps, but it drops some hits the old pinned window reported (a
    box low under a ramp's high end). Those verdicts would change, and
    the issue asked that existing verdicts stand. The mesh law (#339
    slice 1, PR #430) already sweeps the real sheared band when it judges
    a meshed fixture, so the tighter reading arrives there.
  - *Relabelling every upright-rapid hit by band*, the disc included.
    The disc's `Rapid` label is a persisted verdict (cached warnings,
    golden wording), so it stays. Only the newly judged upper bands are
    named.
  - *Putting the logic inline in `laws.rs`*: that file was at 380 lines.
    The shared helper keeps it at 335, and both laws read one span rule.
- **Cost to change later:** the window rule lives in `stack_span::pieces`
  and `band_struck`. Tightening it to the sheared sweep is local, but it
  needs a pass over the fixture suites for the over-reports it would
  drop.
- **Merging with #381 (HolderStack) and #430 (mesh):** law signatures are
  unchanged. #381 builds c2's bands from `HolderStack` and the loop body
  here calls `band_struck` per band, so that composes as is. The d2 upper
  bands come from `stack_span::upper_bands(tool, spindle)`, which reads
  the holder cylinder. When #381 lands, that helper must take the
  `HolderStack` (shank, `holder.bands`, spindle at `holder.top`) so that
  upright rapids judge a modelled body. Nothing will fail to compile if
  this is missed: it has to be done by hand. #430 wraps the box laws and
  only acquits. Its rapid mesh reading labels a convicted move `Rapid`,
  so on a meshed fixture an upright-rapid holder hit reads `Rapid`
  rather than `Holder`. That is a label difference only, and the post is
  blocked either way.
