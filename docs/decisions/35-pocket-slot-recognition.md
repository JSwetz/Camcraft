## #35 - pocket and slot recognition, headless

- **Chosen:** a new app module `recognize` (`mod`, `walls`, `shape`, `op`)
  reads the model's planar-face topology (`geometry::feature`, the B-rep
  the pick tools already use) and returns, per recognised floor, its kind
  (closed pocket, open pocket, slot), outer ring (world XY,
  counter-clockwise), island count, floor Z, top Z, depth, area, tightest
  inside corner radius, open spans and - for a slot - its centreline
  ends, width, length and end kinds (open, round, square).
  `recognize::operation_for` turns one into `(SrcRef, OpParams)` for
  `CreateOperation`, and the MCP tool `detect_pockets {model, create?,
  tool?, name?}` lists them and optionally creates the operation, one
  undo step. The GUI entry point is #511.
- **Recognition rule:** a level, up-facing face is a floor when the
  facets across its outer ring's spans rise from it and lean into it (a
  concave edge - the depression rule of the feature-recognition
  literature), and those walls face each other across the floor (two
  inward wall directions with cosine < -0.9). A floor walled all round is
  a closed pocket; one with wall-less spans is an open pocket. That
  "opposite walls" rule is what keeps a shoulder (one wall) and a corner
  step (two adjacent walls) out, and the top face of a part (no walls)
  and a plate around a boss (the boss is an inner ring; the outer ring
  has no walls) never qualify. Dropped: a pure "every span concave"
  rule - it misses every open pocket and slot.
- **Top Z** is the highest point of a flood from the wall facets over
  every non-level facet at or above the floor, held within a quarter of
  the floor's size of its footprint. Taking the highest (not the lowest)
  rim means a pocket in a sloped or stepped top starts where the highest
  wall does - the op may cut a little air, never start under material.
- **Slot rule:** minimum-area enclosing rectangle over the ring's edge
  directions; a slot is at least 2 widths long, has no islands, both
  long sides walled along their whole length, each end wholly walled or
  wholly open, and an area within 3% of the rectangle / stadium it
  claims. A closed end whose vertices sit on the cap circle is round,
  otherwise square.
- **Operation mapping:** a slot with no square closed end becomes a
  **Slot** (centreline ends and width, fixed heights, source = stock):
  an open end's endpoint is the floor's edge, so the full width runs out
  of the part and the round cap overhangs into stock or air. Every
  pocket, and a slot with a square closed end (a round cap would leave
  its corners standing), becomes a **2D Pocket** on a pick of the floor
  face, `bottom_z = selection bottom + 0`, `top_z = selection bottom +
  depth`, so its area (and islands) is re-read at every generate and its
  depths follow a moved part. Feeds, clearance and the rest come from the
  MCP seed (the same seed `add_operation` uses), all editable afterwards.
  Dropped: the Slot's source as the model (its region law then judges
  the cut against the part's section, which the overhanging cap leaves).
- **Known gap, filed:** any typed Slot whose cutter rides its round cap
  is convicted of a gouge today (#510, likely the stadium region's
  chorded caps against `GOUGE_TOL`), so a recognised slot's operation
  inherits that until #510 lands; the slot tests pin the path geometry
  exactly instead of the verifier's verdict.
- **Not recognised (by design for now):** through pockets / windows (no
  floor face), pockets on non-level floors or in a tilted setup frame
  (the read is world +Z), multi-level floors as one feature (each level
  is its own pocket), curved slots, and T-slots / undercuts. An open
  pocket's open side is not yet written into the area controls' open
  boundary - the pocket stays inside the floor ring, which removes the
  floor's material but leaves the tool's corner radius at an open
  corner.
- **Cost to change later:** small - the module is self-contained; the
  thresholds are named constants at the top of `walls.rs` and `shape.rs`.
