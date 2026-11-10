## #697 - one approach law for every clearance-seeded mill path

**Chosen:** `PathBuilder::approached(seed)`, a builder seeded on the
clearance plane whose path always opens with a rapid. A generator's own
rapid to its start is dropped when the start is the seed (zero length).
In that case, if the first move would be anything but a rapid, the
builder states a zero-length rapid at the seed ahead of it. That rapid is
the program's approach (`G00 X0 Y0 Z<clearance>`). All 19 motion files
that seeded `PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z))` now
use it, Face included. #694's Face-only insert is removed in favour of
the shared rule.

**Why in the builder, at the first move:** inserting at `finish()` would
shift every recorded move index (cycles, comp spans). The builder states
the approach before any move exists, so nothing is shifted. It is opt-in
(`approached`), so builders seeded elsewhere (drilling cycles, ladders)
and the lathe keep their own openings byte for byte.

**Guard:** `origin_approach::no_generator_seeds_the_clearance_plane_without_the_approach_law`
scans `crates/motion/src` and fails on any new `PathBuilder::new` seeded
at X0 Y0 on the clearance plane.

**Not covered:** the lathe generators (blade, parting, contour door and
rides) seed at a clearance radius and station and follow their own
approach rules. They're left as they are and noted on the issue.
