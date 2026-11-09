## #457 - Built-in VMC stroke bands restated in program coordinates

**Decided:** keep the checker's documented convention (travel bands are
PROGRAM coordinates through the active work offset) and restate the
shipped mill bands to match it: a slide of stroke L is written `-L..L`,
the full stroke either way. `vmc.mch` and `tilthead.mch` go from
X -300..300 / Y -250..250 / Z -500..0 to X -600..600 / Y -500..500 /
Z -500..500. The checker code is unchanged; the rule is now spelled out
in the `app::machine` module doc and `assets/machines/AGENTS.md`.

**Why:** the file cannot know where the work offset sits on the slide,
so the only band that never refuses a reachable point is the union over
every possible offset, which is `-L..L`. This is the rule the Tormach
files already follow (b189c960, "full stroke each way"), so every
shipped mill now reads one way. A genuine over-travel still refuses by
name: a program asking for more than L away from its own origin (Z 520
on the VMC) cannot fit any placement.

**Considered and dropped:**
- A machine-frame anchor key in `.mch` (where G54 sits in machine
  coordinates) that the checker converts through. Correct and tighter,
  but it is a new grammar key, a new setup parameter the user must
  state per job, and an editor field - feature-sized, not a fix.
  Worth an enhancement issue if a shop wants absolute travel checks.
- Dropping `travel` from `vmc.mch` like `3axis-vmc.mch`. Loses the
  check entirely; the `-L..L` band still catches a program that cannot
  fit the machine at all.
- A span rule (program extent per axis must fit within L) in the
  checker. Tighter than `-L..L`, but it would change the meaning of
  every stated band, including the Tormach files' - out of scope.

**Not changed:** `lathe.mch` (X 0..320, Z -615..0) and `millturn.mch`
(X 0..650, Z -620..0) still state one-sided bands. A lathe X measured
from the spindle centreline is a genuine program-frame band, but the
one-sided Z may refuse approach moves in front of the face the same way;
left for its own issue rather than widened blind.

**Cost to change later:** an anchor key would read alongside these
bands; the numbers in two JSON files are all that moves.
