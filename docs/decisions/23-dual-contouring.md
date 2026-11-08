## #23 - dual contouring for the machined-stock mesh

- **Chosen:** Dual contouring (Ju, Losasso, Schaefer & Warren, SIGGRAPH 2002)
  over the same cell-centre samples marching tetrahedra already read
  (`stock_sim::dual`, `hermite`, `qef`), selected by a new
  `iso::IsoMethod { Dual, Marching }` with `Dual` the default. `iso::extract` /
  `extract_at` (the STL export and the viewer's `MeshCache`) now return the
  dual mesh; `extract_with(.., IsoMethod::Marching)` keeps the old one by name.
- **Why default, not an option:** every existing stock/sim/export law stayed
  green with it (sim crate suites, the app `stock_export` suite, the ui
  machined-mesh tests) and it is strictly closer to the field: a 10x8x6 box at
  a 1 mm cell extracts to 480.000 mm^3 (marching: 472.3), and a slot's
  floor/wall edge is reproduced to 0.0000 cells (marching chamfers it by 0.36).
  The mesh is a display/export cache of the field, not a machining strategy,
  so it is an API parameter with a default rather than a user-facing op
  parameter; no UI toggle or command was added.
- **Safety:** a dual mesh with any open edge is never returned - the marching
  mesh of the same samples stands in, logged (`eprintln!`), and the app's
  existing open-edge refusal still guards the STL. QEF vertices are clamped
  into their cell; truncated pseudo-inverse (singular values under 0.1 of the
  largest) keeps the mass point along ill-conditioned directions; NaN/zero
  normals give no plane; the Jacobi solver is sweep-capped.
- **Hermite normals - own estimator:** plain central-difference gradients blend
  the two faces across the ridge a corner throws, and the crossings beside a
  corner are themselves biased by that ridge. Measured: gradient normals gave
  0.35 cells of corner error (no better than marching). So normals are read
  from the slope of the crossing sheet across neighbouring parallel edges (the
  sheet as a height field), a crossing whose endpoint magnitudes fall short of
  the plane law (|va| + |vb| < 0.95 h |n_k|) is flagged untrusted, and a cell
  holding an untrusted crossing solves its QEF from the trusted, same-facing
  planes of its 3x3x3 block. Smooth cells read only their own planes, as in
  the paper. Gradient blend (eikonal-weighted) is the fallback normal.
- **Considered and dropped:** gradient-only Hermite normals (measured no gain);
  borrowing neighbour planes in every cell (would bulge small-radius curved
  surfaces); making DC a user op parameter (no CAM strategy fork - display
  fidelity only); replacing marching outright (it stays as the fallback and as
  a named method).
- **Cost to change later:** flip `IsoMethod`'s `#[default]`; the extractor API
  is unchanged for callers. A thin wall under ~1 cell can still read one face's
  plane from the other side's block only if the normals face the same way -
  opposite-facing planes are filtered out.
