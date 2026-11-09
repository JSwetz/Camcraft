## #368 - tool bodies from STEP, judged by their revolved envelope (milling)

- **Chosen: a separate `ToolBody` component** on the tool entity
  (`schema.rs`): source name, the numeric placement (model axis
  direction, model origin's height above the tip), the facets placed in
  the tool frame (tip at origin, +Z to the spindle), and the derived
  `HolderProfile`. Dropped: new fields on `Tool` / `ToolDef` - every
  catalog entry, test literal and the UI editor build a `Tool`, and a
  mesh does not belong in a value type that is cloned per frame.
- **Envelope (`camcraft_tools::holder_profile`):** fine bands (>= 0.25 mm,
  <= 2048) along the axis; every facet is clipped exactly to each band it
  spans (Sutherland-Hodgman) and the band takes the farthest clipped
  vertex. This is exact, because |(x, y)| is convex and its max over a
  convex polygon is at a vertex, so a facet crossing a band with no
  vertex in it still counts. A 1e-6 mm pad covers float dust. The bands
  are then merged greedily, each merge at the larger radius and the
  least added volume, down to <= 32 bands. Merging only ever grows the
  envelope. Dropped: vertex-only binning (it under-reads long facets)
  and an uncapped band count (the posed sweep costs one capsule per band
  per interval).
- **Seam: two reads, both conservative.**
  1. *The fixture laws* (c2)/(d2), prismatic and posed (`laws.rs`,
     `posed_steel.rs`, `workholding.rs`), take a `HolderStack` (the bands
     above the shank, and the height the spindle housing rides on).
     `HolderStack::cylinder(geom)` is today's holder built from the same
     float sums, so the old public functions are thin wrappers and give
     bit-identical verdicts. The `*_held` variants take a body's bands.
     These are the laws that name a collision, so they read the real
     profile.
  2. *Every other law* (rest-material reach (c), the solid law (b2), the
     multi-axis shank dodge, drill reach, cross-drill holder checks, the
     machine's max tool length) reads one holder cylinder. They get the
     tool with its holder replaced by **the cylinder enclosing the body**
     (`HolderProfile::enclose`): the holder runs from the shank top to
     the body top at the body's widest above the shank, and the shank is
     widened to the body's widest beside it. That is never smaller than
     the body, so those laws can only over-report. `tool_body::judged`
     swaps it in at the three world reads (`collect_job`, the post gate's
     fixture walk, the machine check). Everything downstream of the
     job's `tool` follows automatically.
  Dropped: adding a band list to `ToolGeom`. It is `Copy` and built as a
  struct literal in ~55 test files, so that would be a huge,
  conflict-prone diff. A band-exact reading of the one-cylinder laws is
  follow-up work. Until then a stepped holder can over-report there.
- **Gap and flutes:** if the body's foot is above the shank top, the
  stack carries the shank radius up to the foot (the shank runs into the
  holder, so the gap is not left unjudged). A body reaching below the
  flute top is refused by name ("reaches into the flutes"): at
  `SetToolBody`, at `ChangeTool` (an edit that would put the body over
  the flutes), at generation and in the post gate.
- **Refusals by name** (envelope + placement): no facets, non-finite,
  every facet zero-area, no length along the axis, below the tip, XY box
  not surrounding the axis ("does not surround the tool axis - check the
  axis choice and placement"), larger than 500 mm from the axis or
  2000 mm long, a zero or non-finite axis direction, a non-finite or
  absurd offset. The lathe insert kinds (Turning/Groove/Thread/Form) take
  no body yet (#369). "Surrounds the axis" is the body's XY bounding box
  containing the axis. Dropped: a per-band test, which would refuse real
  holders with one-sided bosses or stepped bores.
- **Placement:** an arbitrary model direction becomes +Z through the
  model origin, plus an offset. +Z maps X->X and Y->Y. A rotation about
  the axis is not taken because the revolved envelope cannot see it;
  #371 (display) can add it.
- **Stamp:** the body adds bytes to the op digest only when present (its
  envelope bands). No-body digests do not move, so no churn. Unlike
  workholding, a body is part of the tool: it changes generated paths
  (the multi-axis dodge, drill reach), so it rides the stamp.
- **Project v58.** `body` is an optional key on the tool row
  (`ToolBodyDto`: source, axis, offset, placed facets). The envelope is
  never written; it is re-derived on load, and a body the envelope law
  refuses makes the file refuse by name. The loader does not need the
  bump. **The old-reader law does:** a v57 build would silently drop the
  key and judge the smaller cylinder (the v51/v53 reasoning). PR #354
  (one WCS per setup), #343 (per-setup machine/post), #316 (setup links)
  and #320 (stock source) merged first and took v54-v57, so this rung is
  v58; it rides the
  `53..=PROJECT_VERSION` block and needs no rewrite.
- **Undo/delete:** `SetToolBody` snapshots prev/next (set, replace,
  clear). `DeleteTool` and `DeleteEntity` snapshot the body and restore
  it on undo. Destructing the entity frees the component (Rust drop).
- **Not in this slice:** UI, the tool library file (#348), lathe holders
  (#369), ISO 13399 frames (#370), viewport/sim display (#371).
