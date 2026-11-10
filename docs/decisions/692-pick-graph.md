## #692 - one feature graph and pick rule (step 1: the app-layer service)

- **Chosen:** `app::pick::feature_graph` builds, per mesh, the only things a
  click may land on: welded faces (one `geometry::feature` `facet_patch`
  root each), feature edges, feature points and round-edge centres.
  `app::pick::feature_pick` resolves a ray against it: the front-most
  surface hit (models beat stock, as `first_hit_f`), then a snap within the
  radius by point > centre > edge > face, only among the kinds the caller
  accepts, each edge/point candidate occlusion-tested unless `through`.
  The UI pickers and MCP tools are not rewired yet (step 2).
- **Reuse, not a second weld:** the welded faces are `geometry::feature`'s
  smooth patches (its 40-degree dihedral union), the same ones
  `list_features` reports, so the MCP view and the click agree; the graph
  is built from the content-cached `model::features_of_local` and cached
  itself the same way (cap-8 MRU by facet digest). `list_features` is
  unchanged.
- **Thresholds:**
  - Feature edge: open boundary (one facet with area), non-manifold fold
    (two or more facets with area), or a crease between two DIFFERENT
    welded faces; inside one welded face only if it bends past
    `CREASE_DEG` = 40 degrees (the weld's own rule, so they cannot
    disagree). `Features::edges` alone was not enough: it keeps the
    "tangent seams" between exact-plane strips - every facet edge of a
    tessellated cylinder - which is the issue's bug.
  - Zero-area / non-finite facets make no feature edge (slivers bound
    nothing; this is also what keeps junk meshes pick-free).
  - Feature point: a vertex with 3+ feature edges (corner), exactly 1 (open
    chain end), or 2 turning by more than `TURN_DEG` = 30 degrees - the
    same angle as `feature::CHAIN_TANGENT_DEG`, so a point is exactly where
    a chain would split. A 48-segment rim (7.5 degrees per vertex) has no
    points; a box corner always does.
  - Arc centre: a run of 3+ feature edges between feature points (or a
    closed loop with none) whose vertices all lie on the circumcircle of
    three spread samples within max(0.1 % of r, 1 µm), in plane to the same
    tolerance; samples with sin(angle) under 1e-4 count as straight (no
    kilometre-wide "arcs" from weld noise on a straight edge).
  - Visibility: the sight segment from the candidate back to the eye
    (perspective) or along the parallel view direction to the view plane
    (ortho) must not cross a non-stock facet (or the candidate's own body)
    earlier than max(1 µm, 1e-4 x distance) before the candidate - the
    vertex picker's existing `depth_tol`, so the slack is unchanged.
  - Radius: world units at the surface hit; in perspective it scales with
    depth (radius x depth / hit depth) so it stays one screen size. A
    non-finite or negative radius snaps to nothing (faces still pick).
- **Dropped:** a screen-space API taking the MVP and pixel radius (keeps
  the service camera-free and testable; the UI converts px to mm at the
  hit in step 2); occlusion against the stock (translucent, and a model
  beats it at any depth already); feature points at every feature-edge
  endpoint (`feature::corner_verts`, today's vertex pick) - it makes every
  rim vertex of a round hole pickable, which the issue forbids. That
  function stays for the existing `FeatureSel::Vertex` ids.
- **Cost to change later:** the angles are named constants in
  `pick/feature_graph.rs` and `pick/feature_arc.rs`; the pin tests in
  `crates/app/tests/feature_pick.rs` say which cases each one protects.
