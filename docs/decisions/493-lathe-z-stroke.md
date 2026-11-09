## #493 - Built-in Lathe and Mill-turn Z stroke restated in program coordinates

**Decided:** apply the #457 convention (PR #477: travel bands are
PROGRAM coordinates through the work offset; a slide of stroke L is
written `-L..L`) to the two turning built-ins' Z slides. `lathe.mch`
Z goes from `-615..0` to `-615..615`, `millturn.mch` Z from `-620..0` to
`-620..620`. X stays as shipped (`0..320`, `0..650`), and so does the
mill-turn's Y (`-100..100`, already symmetric). No checker code changed.

**Why:** reproduced on main before the fix. A face-and-turn job with
its Z0 on the finished face and a 5 mm clearance in front of it was
refused on both built-ins, down to the facing approach at Z 0.3 and
0.8 (`Face1: move 0 reaches 0.800 in Z, the stroke is -615.000..0.000`).
The file cannot know where the work offset sits along the Z slide, so
the full stroke each way is the only band that never refuses a
reachable point. A program that asks for more than L from its own
origin still fits no placement and is refused by name (pinned at
clearance L + 20).

X is kept one-sided on purpose. A lathe X is measured from the spindle
centreline, and the centreline is the program's X0 on every turning
setup, so `0..R` already IS a program-frame band. Widening it to `-R..R`
would accept moves past the centreline the cross-slide cannot make.

**Considered and dropped:**
- Widening X too, for symmetry with the mills. Wrong for a lathe: its
  X origin is fixed by the spindle, not chosen by the operator.
- A machine-frame anchor key (where G54 sits on the slide). Same reason
  as #457: feature-sized, not a fix.
- Editing `assets/machines/AGENTS.md` / the `app::machine` doc here.
  PR #477 writes the convention there; repeating it on this branch
  would only make the two PRs conflict.

**Open for the maintainer:** the mill-turn's X `0..650` also judges
its milling ops (live tooling). A milled feature off the centreline in
-X would read as out of stroke. That is a separate question (how far a
mill-turn's X may cross centre), not part of this Z fix.

**Cost to change later:** two numbers in two JSON files.
