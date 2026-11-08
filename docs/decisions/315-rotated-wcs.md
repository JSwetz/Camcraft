## #315 - a rotated work offset posts unrotated coordinates

- **Chosen:** Refuse by name in the post's verification gate
  (`posting::verification_report`, so the one-file post, the per-op post
  and the script API all see it): "setup <S>: its work offset G54 is
  rotated <deg> deg about <axis>; posting a rotated work offset is not
  supported yet - re-align it without rotation or post from an unrotated
  setup (operations: ...)". One line per offset entity, listing every
  posted op that uses it. Legacy documents without setups say "work offset
  G54 ...".
- **Evidence:** `rotated_wcs::the_post_stream_carries_no_offset_rotation`
  pins the defect - the record stream for a G54 turned 90 deg about Z is
  identical to the square one at the same spot; no shipped post can see
  the rotation, so before this change the program posted silently in the
  world frame.
- **Tolerance:** angle (via `2*atan2(|xyz|, |w|)`, well-conditioned near
  identity) at most 1e-9 rad counts as unrotated: 1 nm at 1 m, far above
  normalization noise. Identity offsets post byte-identical (pinned), and
  no golden or existing post test changes.
- **Slot law shared:** the slot an op posts to moved into
  `wcs_rotation::wcs_slot`, read by both `wcs_context` and the clause, so
  the gate judges exactly the offset the post uses.
- **Considered and dropped (for now):** emitting G68 / G68.2 (Fanuc),
  CYCLE800 / ROT (Siemens), PLANE / CYCL 10 (Heidenhain), or baking the
  rotation into the coordinates in the post. Each needs per-control
  options (G68 is an option on many Fanucs), interacts with arcs (G17
  plane after a tilt), cycles, the machine-limit check and the sim, and
  has to be pinned against hand-computed programs per dialect - not
  trivially verifiable, so it is #315 step 3 / follow-up work, a user
  parameter with a default when it lands. Refusing in generation too was
  dropped: generation is in world coordinates and correct; only the post
  frame is wrong, and the gate is where metal is released.
