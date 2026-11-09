## #182 - MCP: render the simulated stock as a PNG

- **Chosen:** a `render_stock` MCP tool. It runs the column verdict's
  own program sim (`stock_sim::simulate_document`, the same one the post
  gate reads), paints its sheet by deviation class (clean, rest, gouge,
  unjudged), walls the sheet's open edges down to the stock floor so the
  picture reads as a solid, and rasterises it on the CPU
  (`crates/app/src/raster.rs`: edge-function fill, z-buffer, flat Lambert
  shading, fixed orthographic iso or top camera fitted to the scene). An
  optional `toolpath` overlay draws every op's moves (feed blue, plunge
  amber, rapid red) over the stock without depth test. The answer is a
  text block with the JSON summary (rest/gouge cells, volume, max and
  where, the rest's top bounds, the drawn stock's bounds, judged cell
  count, image facts) plus an MCP image block (`image/png`, base64).
- **Why:** headless and GPU-free, so CI and the MCP server can render;
  single-threaded with a scene-fitted camera, so the bytes are identical
  run to run (pinned by `mcp_render`). The column sheet is the cheap sim
  that already carries per-column classes and the rest volume, so the
  picture and the numbers come from one verdict.
- **Wire shape:** a handler returns its PNG under a reserved key
  (`_png_base64`); `tool_ok` lifts it into the image block, so neither the
  text JSON nor `structuredContent` carries the base64. This kept the
  tool table's signature unchanged (one row in `tools.rs`, PR #314 also
  edits that file).
- **Dependency:** `png = "0.18"` added to the workspace and to
  camcraft-app. It was already in `Cargo.lock` (pulled by `image` for the
  UI icons), so no new crate enters the tree. Base64 is a 15-line
  RFC 4648 encoder in `mcp/render.rs` rather than a second dependency.
- **Dropped:** rendering the field view's iso-extracted closed mesh
  (`stock_view` + `stock_export::machined_mesh`) - a truer solid, but the
  field cut costs minutes on big fixtures and its classes need a separate
  judgement pass; a later `quality: "field"` option could add it. Also
  dropped: depth-tested toolpath lines (the tip sits exactly on the cut
  surface, so z-fighting hides most of the path).
- **Clipped to the stock floor:** the column grid reaches below the
  block (a semi-infinite column), so a cut to or through the floor would
  otherwise draw a sheet hanging under the stock. Columns at or below
  the stock's floor are air in the picture, walls stop there, and a
  program that leaves nothing above the floor is refused by name ("no
  surface to render") - pinned by `render_stock_refuses_when_no_material_stands`.
- **Limits:** posed (5-axis), rotary and lathe programs are refused by
  name, exactly as the column verdict refuses them; the "rest" bounds are
  the bounds of the sheet over rest columns (the rest runs down from
  there by up to `rest.max`), not a closed rest solid.
- **Later cost:** changing the palette or camera changes every rendered
  byte - fine, nothing stores goldens of it beyond the determinism test.
