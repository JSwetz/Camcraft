## #25 - machined mesh: hovered facet highlight and deviation classes

- **Chosen (classes):** each facet of the extracted machined mesh is judged by
  the GPU eye's own law - the model's `DistanceLattice` (`pack_target`, the
  lattice the trace pass binds) read at the facet centroid, gated by
  `gate_tol(DEV_TOL, view cell)` - into the shared four-class vocabulary
  (0 clean, 1 rest, 2 gouge, 3 unjudged). The scene expands the per-facet
  class to the per-vertex `GpuPart::classes` buffer, so the mesh paints the
  same legend uniforms as the Deviate sheet and the traced eye. New module
  `app::machined_classes`; cached in `stock_export::MeshCache` next to the
  mesh (`classify` / `viewed_classes`), judged once in `ui::machined::prime`
  with the extraction and retired with the view. The STL export path never
  pays for it (`get` does not classify).
- **Chosen (hover):** `machined::filter_hit_facet` returns the mesh triangle
  the stock hover's ray landed on; it rides `State::hover_mesh` (view state,
  cleared on every hover refresh, never a command). The shell draws it as a
  screen-space veil plus outline in the hover ink (the same yellow as the
  feature-hover outline), projected each frame through the camera - no GPU
  re-upload when the pointer moves. A corner behind the camera, or no mesh
  standing, draws nothing.
- **Considered and dropped:** the exact `SdfField::true_distance` + parity per
  facet (linear in the model's facets per query - `facets x model facets`
  per mesh, far over budget; the eye's lattice is exact in the band, where
  the gate matters); the column `Deviation` grid (a top-height verdict with
  no lattice origin, blind to walls and undersides); a new GPU highlight
  pass for the hovered triangle (a re-upload per pointer move); new
  thresholds for the mesh (would let the mesh and the eye disagree).
- **Limits:** a model that cannot be signed or a target lattice past the pack
  budget paints the mesh unjudged (class 3), never clean. The centroid sample
  carries the mesh's own chord error; the eye's landing slack (0.15 cell) in
  the gate covers it.
