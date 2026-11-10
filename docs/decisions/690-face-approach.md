## #690 - Face opens with an approach rapid

**Chosen:** the facer's path starts its cursor where the other mill
generators do, at (0, 0, clearance), and its first move is a rapid on the
clearance plane to the entry point. The plunge follows from there. Every
post now writes `G00 X<entry> Y<entry> Z<clearance>` as the first motion
after the tool call, then the plunge feed.

**Why:** the path used to start *at* its entry point in the air, so its
first move was the plunge. Posts write a move's end point only, so the
plunge's air start was never commanded. The first posted motion was a
3-axis feed from wherever the tool change left the spindle, straight to
cutting depth. Tormach's script hid this with its own #131 approach. The
Fanuc, Haas, Siemens, Heidenhain and Mazak posts did not.

**Considered and dropped:** a generic post-layer rule that emits a rapid
to the first move's `from` whenever a toolpath does not open with one.
It would also catch any future generator with the same gap, but it runs
through the cycle, thread and probe ordering in `post::mod`, and lathe
paths have their own approach. That is a wider change than this bug, so
it is left for its own issue if another generator turns out to need it.

**The X0 Y0 edge:** an entry at exactly X0 Y0 is the cursor's own start,
and the builder drops zero-length moves, so the approach would vanish
and the plunge would again be the first move. Face states the approach
outright there: a zero-length rapid at the entry on the clearance plane,
which the post writes as `G00 X0 Y0 Z<clearance>`. It's pinned by
`a_face_entering_at_the_origin_still_opens_with_its_approach`. Other
generators that start their cursor at (0, 0, clearance) may share this
edge. They weren't checked here, and that's noted on the issue.
