## #570 - probe search limit and protected approach

Found on the contributor's real Tormach 770MX (#521's hands-on test, 2026-11-09).

- **Default search limit:** a new Probe op's `overtravel` ("Probe distance") is `default_overtravel(tolerance) = max(2 x tolerance, 1 mm)`, which gives 1 mm for the seeded 0.5 mm tolerance. Before this it was a fixed 5 mm.
  - **Why:** a feature inside the tolerance still trips well before the limit, and a missing one alarms 1 mm past instead of 5, so the stylus is never driven toward the end of its travel.
  - **Dropped:** a fixed 2 mm default. It ignores the tolerance the op already states.
- **Too-deep search is advisory, not refused:** a new non-blocking `Warning::ProbeSearch` names any limit past `SEARCH_LIMIT_MM = 2 mm`. Like `Scallop`, it never holds the post.
  - **Why:** existing documents keep their stated value and still post, as the issue requires. The probe distance is the operator's choice.
  - **Dropped:**
    - **Refusing past a cap:** it would break every saved probe op at 5 mm.
    - **Citing the probe's rated overtravel:** `Tool` states none today. That needs a tool field first, so it is left for a follow-up if wanted.
- **Protected approach:** below the safe plane a probe only feeds.
  - **What changed in the path:** it used to open with a rapid straight down to the approach point, the ball 5 mm over the feature, which the operator found alarming. It now opens with a positioning rapid that ends at the safe plane over the first start. Then it feeds at the seek feed down to the approach point.
  - **Why:** standard touch-probe practice. A stylus that meets something on the way down trips at a feed it can absorb.
  - **Positioning rapid:** this is a zero-length rapid at the path's start, because the path does not know the tool-change position. Every post writes it as the positioning block (`G00 X Y Z<safe>`).
  - **Dropped:** relying on the posts' own XY-first positioning. Only the Tormach post has it, and the Fanuc post would have fed diagonally from the tool change.
- **Goldens:** the three Fanuc probe goldens gain the safe-plane rapid plus one feed line. No snapshot changed.
- **UI help:** the probe distance hint now says "search LIMIT ... not a destination" and that over 2 mm is flagged.
