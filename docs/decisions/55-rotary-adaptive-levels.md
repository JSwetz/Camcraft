## #55 - Rotary Adaptive: the r6.2 travel refusals, the narrowed helix, and the thin load margin (2026-11-08)

**Decided (three parts):**

1. **The known red was float noise in the air split.** On current main
   `adaptive_plans_the_developed_levels_under_the_load_and_kink_laws` failed
   at radius 6.5 with 19.9 % D against the 19.7 % law. The wrap's ladder
   plane there reads 6.500000000000001. The part's leave envelope
   (`air_above` = r6 wall + 0.5 leave) is 6.5. `pocket3d_air_split` sorted
   the planes with a bare `>`, so the last bit of the wrap's radius
   arithmetic sent that plane to the AIR leg (one lap) instead of the
   material leg (two laps, the seam law). The one-lap plan's opening spiral
   read 55.0 deg (19.9 %) on the measuring stock. Now a plane within
   `EPS` (1e-6) of the envelope is a material plane
   (`machining3d::entry::air_levels`). The bar reads exactly round 2's
   numbers again.
2. **A developed level too narrow for the asked helix is refused before
   planning, by name.** `hsm::plan_level`'s wrap arm asks the planner's own
   entry law first (`machining2d::choose_entry`, now `pub(crate)`). If the
   law would narrow the helix, Offset rings cut the level. The note reads:
   "adaptive: the developed level is too narrow for the helix entry
   (radius 1.000 asked, 0.900 fits with room to spiral out) - Offset rings
   cut the developed level at radius R instead". Planar 3D pockets are
   untouched: the check runs only when the wrap's seam is present.
3. **The r6.2 neck refusals are decided final for this issue.** Levels 5.7
   and 5.2 stay refused for out-of-cut travel, by name, and Offset rings
   cut them. The skin level 6.2 is still HSM-planned. The test
   `the_r62_neck_levels_are_refused_for_travel_by_name` pins it.

**Readings** (release, turned bar wall r6 / neck r5, BN D6, leave 0.5,
helix r1, law 14.6 % D + 35 % = 19.7 %):

| Fixture | Before | After |
|---|---|---|
| r7 bar, radius 6.5 | 7,067 planar moves, 19.9 % D (red) | 22,528 moves, 8,694 arcs, 19.5 % D |
| r7 bar, radius 6.0 / 5.5 | 19.5 % / 18.8 % D | unchanged |
| r7 bar, world program | - | 160,989 moves, 0 in-cut breaks past 5 deg (round 2's number) |
| 10 long window (stations 6..16), sd 2, r7 | helix narrowed to 0.900; "worst planned load 15.7 % of D ... exceeds the 14.6 % optimal load by 7 %" at radius 7.0; 5.0 refused after 197 revolutions of planning | both levels named "too narrow for the helix entry (1.000 asked, 0.900 fits)", rings cut them, no overload note; under a second in debug |
| r6.2 bar, radius 5.7 | planner travel: cut 2,529 mm, out of cut 8,208 mm (3.2x) | refusal kept, by name |
| r6.2 bar, radius 5.2 | cut 2,254 mm, out of cut 8,828 mm (3.9x) | refusal kept, by name |

**Why the r6.2 refusal is final here:**
- The 5.7 centre area is a band about 78 x 7.8 round the developed neck
  (5.2: about 78 x 5.8). The growth spiral re-walks the whole cleared ring
  every revolution, so only its microlift returns keep it in cut.
- The microlift plan reads 48.0 deg against the plan's 47.5 deg bound. The
  planner then posts the spiral unlifted ("the microlift plan read 48.0
  deg over the spiral's 47.5"). Unlifted, it rides 3.2x to 3.9x its cut
  out of cut, past the 1.5x budget.
- Accepting the lifted plan would post a named overload, the very thing
  this issue removes. Relaxing the 1.5x budget would post a plan slower
  than the rings it replaces.
- A thin band's own plan is the planar planner's thin-band work (#44, and
  the trochoid in #47 / #6), not the wrap's. When that lands, these levels
  reach it through the same `adaptive_passes` door.

**Considered and dropped:**
- *Turning the trochoid fallback on for wrap levels* (`adaptive_passes(...,
  true)`). It was switched off on purpose for 3D in f796e52: its returns
  land in cut (#47, claimed and open).
- *Relaxing the microlift acceptance by half a degree* in
  `machining2d::adaptive::lift`. It would change planar pockets, and the
  accepted plan would carry a named overload note anyway.
- *Refusing after planning whenever the plan names its own overload.* That
  is more general, but it pays the full planning cost first, and the
  overload on this window lives only in thin bands that the entry law
  already flags. Kept as a fallback idea if an overload shows up in a wide
  wrap level.
- *Widening the heavy test's 35 % allowance.* That would move the law, not
  the plan. The margin stays thin: 19.5 against 19.7. That figure is the
  planar planner's reading characteristic at arc junctions near the band
  ends (54.5 deg bins on the 0.1 mm dexel rim), not something this fix
  claims to improve.

**Cost to change later:**
- The air-split tolerance is one constant. A level within 1 micron of the
  envelope can now only ever be a material plane, which is the documented
  "at or below" rule.
- The narrowed-helix door is one check in `plan_level`. Removing it puts
  thin wrap bands back on the planner once it holds its load there.
