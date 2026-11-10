## #529 - Edge-break records are bounded by their own operation

**Decided:** the program sim credits a cached edge-break record only when its plan width (depth ÷ slope) is no wider than the width its own operation states (`Chamfer2D` / `Deburr` `width`), with a micron of slack for the stored lattice. A record on any other kind is not credited. A dropped record is logged by name, and its columns are judged against the sharp model. The check sits where the sim reads each op's records (`stock_sim::breaks::bounded`), so it covers a fresh generation and a loaded file alike.

**Why:** the record is file data, and its depth set how deep a cut was forgiven. A hand-edited or corrupted project could hide a real gouge under an inflated bevel (review note on PR #502).

**Considered and dropped:**
- Bounding in `EdgeBreak::is_sane`. The record alone does not know what its op asked for.
- Bounding at project load. The op's params and its cache are read separately there, while the sim already holds both.
- An absolute depth cap (tool flute length). It is looser than the op's own width and needs the tool.

**Cost to change later:** low. One function and one call site.
