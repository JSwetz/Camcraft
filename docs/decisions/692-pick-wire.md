## #692 - one pick rule for the whole app (step 2: every picker on it)

- **Chosen:** every viewport Face/Edge/Vertex click and hover goes through
  one UI router, `ui::feature_click`. It builds the camera's `PickRay`,
  states the kinds the armed tool accepts, and calls
  `app::pick::pick_selection`, which is `pick_feature` plus a mapping to the
  stored `FeatureSel`. The answer goes to whoever already owned the click:
  the axis or plane draft, the stray-click hint, the boundary draft, or the
  open operation's geometry (edges keep the Ctrl/Shift law of
  `edge_pick`). The local rules are gone: interact's screen-space
  `edge_nearest` (it had no occlusion test, and its candidates were every
  `Features::edges` entry, facet seams included), `feature_of_facet`, the
  raycast face pick, and vertex_pick's `nearest_vertex`, `sight_origin` and
  `vertex_nearest` (its candidates were every endpoint of `Features::edges`).
- **Kinds per picker:**
  - Plane row: faces only (#554), whatever the mode.
  - Axis by edge: edges only (a run). Axis by two points: points only.
  - Operation geometry in Face mode: faces. In Edge mode: edges, and the
    face between them; a tangent chain, a run while a boundary collects,
    one segment with Alt. In Vertex mode: points.
  - A boundary refuses a point by name, as before.
  - The armed Setups row decides the kinds before the pick mode does, so an
    axis row never snaps to a face and the plane row never snaps to an edge.
  - The old refuse-by-name arms in `axis_pick::take` and `plane_pick::take`
    stay as defence.
- **Radius:** the pixel radius is 12 px for edges and 8 px for points
  (unchanged). In ortho it becomes world mm once: 2 x ortho half-height /
  pane height. In perspective it becomes mm per mm of depth through a new
  `PickQuery::radius_per_depth` (2 tan 30 deg / pane height), so it holds
  at the silhouette, where no surface hit anchors `radius`. Step 1's
  hit-anchored `radius` stays the default when the new field is 0.
- **Pick through: not offered.** `through` is `false` at every call site. A
  toggle needs a ribbon or view-cube control, its state and display, and a
  test of its own. That is not cheap here, and the rule is safe without it.
  The app API keeps `through` for when a page asks for it.
- **Performance:**
  - Graphs are cached per facet allocation (`feature_graph_shared`): a
    `Weak<[Triangle]>` pointer compare, cap 32, dead entries pruned on each
    call. This works because `Facets` is an immutable `Arc` that every
    landing replaces. Hover no longer hashes every mesh each frame.
  - Chains and runs are memoized on the cached graph.
  - At most 32 candidates per tier pay for a sight ray. Without a cap, a
    finely tessellated far rim cost one full ray per hidden chord: a
    100k-facet cylinder took more than 0.4 s per hover pick in debug.
    Before choosing 32, I tried 8: it lost 9 of 16 visible point hits on
    sample_ap214, while 32 loses none.
  - Release timings, 400 hover picks:
    - sample_ap214.step (1,550 facets): 10-32 us mean.
    - A 100k-facet cylinder: 0.4 ms mean for faces, 2.8 ms mean for edges
      (36 ms worst, the first call, which builds the chain memo).
- **Pick bodies:** `pick_bodies_of` now leaves out the stocks of non-active
  setups, because the viewport draws only the active one. Before this,
  `first_hit_f` (the face pick) could land on an undrawn stock.
- **Model mode is unchanged:** it still uses `first_hit_f` and the machined
  mesh filter. It selects entities, not features. In the feature modes the
  machined-mesh hover triangle (`hover_mesh`) no longer shows; the feature
  highlight replaces it.
- **MCP:** `list_features` lists chains made of feature edges only, and
  feature points only as vertices. A face-loop span over a facet seam
  therefore reads `complete: false` (its documented "pick the face"
  answer). Every pick by id (`picks_of`: geometry, area, axes) goes through
  `pick::check_pickable`. It refuses a vertex that is not a feature point,
  and a chain that runs along facet seams. Stale ids are left to the
  commands.
- **Changed behaviour users will see:**
  - The rim vertices of round holes and bosses are no longer pickable
    points.
  - Facet seams of smooth walls are no longer pickable edges.
  - Edges and points behind the part can no longer be picked: the Edge-mode
    pick had no occlusion test at all before.
- **Not here:**
  - Fixture locate (#561) is unmerged, in PR #566. It must call
    `feature_click`/`pick_selection` when it lands.
  - Round-edge centres are not stored by any page yet, so no `FeatureSel`
    variant was added.
  - The WCS anchor markers stay a screen-space marker pick: they are
    overlay points, not model features.
  - The user-docs note (#688) is owed.
- **Cost to change:**
  - Adding pick-through is one `Ask` flag, one control, and `through:
    true`.
  - A page that wants centres adds a `FeatureSel` variant and a
    `selection_of` arm.
