## #406 - the incoming stock in the viewport (2026-11-08)

- **Chosen:** while a setup's settings page is open (`UiTarget::SetupNode(name,
  Settings)`) and its effective stock source is "What <previous setup> leaves
  behind", the scene carries one extra stock-slot part: the incoming body's
  sheet (`Body::display().surface(&[])`), painted by `fs_stock` with a new
  class value 4 - a fixed teal (0.12, 0.66, 0.74) at alpha 0.45. The
  setup's own Stock node keeps drawing in its steel tint beside it.
- **Why:** the stock-slot path already draws translucent sheets with
  per-vertex classes (the Deviate sheet, the machined mesh); one more class
  in the same shader is the smallest change that gives a distinct look,
  with its own wire (crease edges) like every stock part. Teal is far from
  the steel clean tint, the amber rest, the red gouge and the gray
  unjudged, and a mid-luminance hue at 0.45 alpha reads on both the light
  and dark viewport backgrounds.
- **Measured once per (setup, active setup, `doc_version`).** The cache is
  `State::incoming_view` (`crates/ui/src/incoming_view.rs`), refreshed at
  the end of `State::drain_actions`: it measures only when the key moves
  and clears the moment the page closes, another page opens or the setup
  takes its own stock - the mesh is never held for a page nobody sees. The
  page's box/volume text reads the same measurement when it stands (one
  program sim per version instead of two), falling back to its own egui
  cache otherwise.
- **The frame.** `incoming_body` is in the receiving setup's frame; the
  viewport draws in the ACTIVE setup's frame. The app function
  (`camcraft_app::setup_stock_view::incoming_shape`) carries the sheet by
  `A_active * A_setup^-1` (identity in the usual case: the browser
  activates a setup before it opens its node). The box/volume text stays
  in the receiving setup's frame, as #320 printed it.
- **A refusal draws nothing.** It is logged once when measured (stderr, not
  per frame) and the page already prints the reason; a body with no
  material or a sheet with no facet is named, never an empty mesh.
- **Dropped:** a new `GpuPart` flag and pipeline (more GPU plumbing for
  the same blend); reusing the region-fill orange (reads as the selected
  operation's area); a palette (colour-vision) token for the tint (the
  shader uniform's layout would change for one preview colour - can follow
  if a user asks); keeping the mesh until the next document change after
  the page closes (a leak by the issue's own law); a closed outer shell
  (the issue asks for the display sheet).
- **Known limit:** when the traced stock eye paints the stock slot (a
  whole-program sim view on screen), it replaces every stock-slot part,
  so the preview hides under it like the Stock node does; wireframe mode
  shows only its wire, like any stock part.
- **Cost to change later:** the tint is one shader line; moving it to the
  palette is a uniform field plus one token.
