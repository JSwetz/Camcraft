## #677 - Refusals name their spot at the remaining located sites

**Decided.** Every refusal on #677's list that is about one hole now
names that hole with `GenError::invalid_at`, in the frame its
generator runs in, so `run_job` (and the cross-hole spindle frame's
`in_world`) carry it to world:

- Axial drilling (app): the flute-reach, holder and shank-fit refusals
  (`drill_reach`) and the R-plane refusal (`drill_top`) mark the hole
  at the material top over it - the surface the sentence's depth is
  measured from (the entry Z when that top is unknown).
- Cross drilling: the flute-reach refusal (`cross_reach`, already world)
  and the finished-part refusal (`cross_part`, judged on the world
  chain) mark the modelled mouth; the kernel's plunge-axis and
  bottom-above-entry refusals (`drilling/cross.rs`) and the raw-stock
  plunge-direction refusal (`cross_stock`) mark the local mouth, which
  `in_world` shifts to world.
- Lathe axial drilling (`drilling/axial.rs`): the off-axis and
  drill-wider-than-bore refusals mark the mouth; the past-the-bar-end
  refusal marks the floor, the place that is wrong.

Sites that name no spot carry a one-line comment saying why: the
stitch fork (`stitch_cross::fork_refusal`) runs while the job is
collected, in the walk's tilted frame, before `run_job`'s frame map;
the turning chamfer/taper doors (`turning/contour/rides.rs`) name a
station in the turning meridian, which has no world map yet; the
deburr, rotary-height, bore-bar and thread-axis refusals are about a
size or an angle, not a place.

**Why.** The issue's "Done means": each listed site either names its
spot with a test asserting the world spot, or says why not.

**Details chosen.**
- `cross_stock::to_raw_mouths` was the one spindle-frame refusal not
  passed through `in_world`; it now is, so its spot reaches world. Its
  sentence for an off-origin work offset now quotes the world mouth,
  like every other cross-hole refusal (on-origin, the only case the
  tests cover, it is unchanged).
- The axial spots use the hole's XY with the top the law measured, not
  the cycle's R plane: the mark sits on the material the sentence names.

**Considered and dropped.**
- Locating the stitch fork by mapping collect-time refusals through the
  op's frame: a second frame map in a second place for one site; left
  for when more collect-time refusals want a spot.
- A meridian-to-world map for turning stations: needs the lathe's axis
  placement in the generator, a change of its own.

**Cost to change later.** Small: the same one-line swap per site.
