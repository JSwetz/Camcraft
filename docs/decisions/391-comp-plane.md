## #391 - cutter compensation engages in G17, never in a stale G18/G19

- **Chosen:** In each post that writes control compensation (Fanuc, Haas,
  Mazak, Siemens 840D) the block carrying G41/G42/G40 states G17 when an
  XZ/YZ arc left G18/G19 modal (`compPlaneWord`, the plane word written
  first in that same block). An XZ/YZ arc arriving while compensation is
  engaged is refused by name (`compPlaneRefuse`, a counted warning, which
  blocks the post) and is not written. RS-274 / ISO 6983: the control
  offsets in the active plane, and every mill generator compensates in XY.
- **Where:** per script, with the same two helpers next to `restorePlane`
  in each. Plane and comp state live in the scripts, not the post engine,
  so an engine-level fix would have to parse the scripts' own words.
- **Considered and dropped:** a separate `G17` block before the engage
  (same effect, one more block; the shared block keeps a plane-free program's
  output byte-identical); cancelling comp automatically before an XZ/YZ arc
  and re-engaging after (moves the tool off the path the generator planned:
  a silent change of geometry); writing G17 on every engage block (changes
  every existing golden for no gain).
- **Not affected:** Heidenhain, Tormach and the Fanuc lathe refuse control
  compensation already. A program that never leaves G17 posts exactly as
  before.
