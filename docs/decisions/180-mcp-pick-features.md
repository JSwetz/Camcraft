## #180 - MCP: pick faces, edges and loops for an operation's area

**Decided.** Four new MCP tools, in their own modules under
`crates/app/src/mcp/` (`features.rs`, `pick.rs`, `area.rs`) with only the
registration rows added to `tools.rs` / `mod.rs`:

- `list_features {model, kinds?, limit?}` - a model's planar faces (normal,
  up/down/side/inclined, area, bounds, the welded smooth face it belongs
  to, and each boundary loop with the edge chains that run it), tangent
  edge chains (closed flag, length, ends, bounds), welded smooth faces
  ("patches") and corner vertices. World coordinates.
- `set_operation_geometry {operation, picks: [{model, id}]}` - through
  `SetOperationGeometry`, one undo step.
- `get_operation_geometry {operation}` - source, picks, area controls and
  the selection notes the page shows.
- `set_operation_area {operation, area}` - through
  `EditOperationParams::with_area` with the op's params as they stand.

- **Ids are strings, `"<kind>:0x<16 hex>"`** (`face:`, `chain:`, `patch:`,
  `vertex:`). The geometry hashes are full 64-bit values; a JSON number
  loses everything past 2^53 in JavaScript-based MCP clients, and a
  rounded id would silently name nothing (or, worse, something else).
  The kind prefix also makes the `FeatureSel` variant explicit. Dropped:
  the project file's `{chain: {id: <u64>}}` shape (exact in Rust, lossy
  in the clients that matter here).
- **A loop is its chains, not a new pick kind.** `SrcRef::Selection` has
  no "loop" variant, and the region walk already stitches open runs into
  rings. So each face loop in `list_features` lists the tangent-chain ids
  that cover it (`complete: false` if some span of the ring is not a
  feature edge - then the face itself is the pick). A square pocket floor
  is four chains; a round one is one closed chain. Dropped: a synthetic
  `loop:` id expanded at the door (a second id family that is not
  stored, so `get_operation_geometry` could never echo it back).
- **Feature-kind door at the tool, mirroring generation's own
  refusals:** Multi-Axis Contour takes edge chains only, and every kind
  whose picks author its region (`operation::selection_drives_region`,
  now `pub(crate)`) refuses a vertex ("a point bounds no region"). The
  rest (unknown ids, non-solids, a wall rim that is a line from above,
  heights that the new picks cannot answer) is `SetOperationGeometry`'s
  own construction door. Dropped: an MCP-only per-kind acceptance table
  (a second law that could drift from the generators').
- **`set_operation_area` replaces the whole controls**; omitted keys take
  their defaults, so `{}` resets. A merge patch over nested pick lists
  has no clean meaning for "remove the second include boundary", and the
  op page's Apply also writes the whole group. Unknown keys are refused by
  name, like `edit_operation`'s patches.
- **`list_features` is capped at 500 rows per kind by default** (`limit`
  up to 20 000, `truncated` says when rows were cut), so a dense STEP
  import cannot flood one reply.
- **Test fixture written by the test.** No existing STL fixture has a
  pocket (`testdata/models/cube.stl` is a plain cube), so
  `tests/mcp_pick.rs` writes a 40 x 40 x 20 block with a 20 x 20 blind
  pocket as ASCII STL, as `tests/mcp.rs` does for its box.

Cost to change later: the id string format is the tool contract; changing
it breaks agents that cached ids, so it should stay.
