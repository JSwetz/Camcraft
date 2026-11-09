## #93 - spun (revolved-profile) holder editor for milling tools

- **Chosen: the drawn holder is a `ToolBody`** - a spun profile is a body
  whose envelope is known exactly. `SetToolBody::spun` builds one from
  the outline (`tool_body::build_spun`): its facets are the revolve of
  the outline (so the simulation's tool assembly, the Body tab and any
  reader that only knows facets see it), its `HolderProfile` is the
  outline's own bands (not re-derived from the facets), and a new
  optional `ToolBody::spun` keeps the points for re-editing. Everything
  downstream of #368 follows for free: the fixture laws read the bands,
  the one-cylinder laws read `enclose`, the stamp hashes the bands, the
  flutes/lathe fit law refuses by name, undo/redo/delete snapshot it.
  Dropped: a separate component or new `Tool` fields - two writers of
  "the holder" would have to agree, and every `Tool` literal would move.
- **Storage: no new project rung.** `ToolBodyDto` gains an optional
  `spun` key (`[z, r, arc]` per point), written only for a drawn body;
  the facets are written too. A loader with `spun` re-draws the body
  from it (refusing a bad outline by name: "holder profile: point 3
  ..."). A v58 reader without the key loads the facets and judges their
  envelope, whose ring vertices sit on the conservative outline - so it
  can only over-report, never judge less steel. That is the old-reader
  law's test, and it passes without a bump (#381 stays at v58).
- **Outline law (`camcraft_tools::spun`):** (z, r) points tip-up, each
  segment a line or a circular arc of signed radius (positive bulges
  away from the axis). Refused by name: < 3 or > 64 points, non-finite,
  r < 0, z < 0, an end off the axis, an interior point on it, a segment
  < 0.01 mm, an arc tighter than half its chord, an outline that runs
  back down the axis (also catches an arc on a flat step), no length,
  past the 500 mm / 2000 mm body caps. Point numbers are 1-based as the
  table shows them.
- **Conservative arcs:** a convex arc flattens to its circumscribed
  tangent polygon, a concave one to chords on the arc - neither ever
  falls inside the drawn solid. Bands: one per climbing segment at its
  larger radius, merged past 32 (merging only grows). A stepped profile
  reads its bands exactly (no 1e-6 pad - there is no clipping dust).
- **Parametric = no body.** `SpunProfile::holder_of(tool)` is the holder
  cylinder from the very sums `HolderStack::cylinder` makes (pinned
  bit-identical for every catalog mill), and `cutter_of(tool)` is
  `Tool::profile` up to the shank top (cutter + holder rejoin to exactly
  `Tool::profile`, pinned). Apply in Parametric mode clears any body, so
  a tool that never went Custom keeps byte-identical stamps and paths.
  Dropped: landing the parametric cylinder as a body - it would add
  digest bytes and restale every op for no change in steel.
- **UI: Body / Profile tabs in the editor's third column**, milling
  kinds only; a turning tool's column is the Body section alone, as
  before (owner, #93: lathe and milling tools keep separate editors;
  turning tools are not spun profiles). Why tabs: the window is already
  ~760 px wide and ~600 px tall (#371 note); a fourth column or a
  section under the columns would push Apply off a 720 px screen. The
  Profile tab: Parametric / Custom, the points table (z, r, arc R; +
  inserts after a row, × removes; read-only in Parametric), a flat side
  view to scale (cutter/shank from the numbers, holder from the draft -
  amber when it validates, red outline when not) with flute, stickout
  (holder foot) and gauge (holder top) ticks, Ø and shank Ø; the live
  refusal by name under it; Apply profile. Parametric redraws from the
  draft numbers every frame; Custom starts from the parametric outline
  and keeps hand edits through number edits. The draft re-mirrors when
  the world body changes by identity (Apply/Undo/Redo/Open), like the
  Body draft: a drawn body re-opens as Custom, anything else as
  Parametric. An imported body is replaced by Apply profile (said in
  the tab).
- **Snapshot:** the editor's pixels change (the tab row). Pixel
  snapshots are release-lane (#255/#359); `egui_tool_editor.png` is not
  regenerated. Structural tests pin the flow.
- **Not in this slice (follow-ups):** a Custom cutter/neck/shank profile
  (needs its own storage and the law "drawn ⊆ the numbers the cutting
  laws read"; the type and `cutter_of` are in place), arcs drawn in the
  table as fillets at a vertex rather than per-segment radii, dragging
  points in the side view, and a spun profile in the tool library file
  (#348 stores no bodies yet).
