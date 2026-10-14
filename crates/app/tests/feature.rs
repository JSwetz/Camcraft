//! Feature-edge weld: crease/boundary extraction off a triangle soup.
//! Lifted from `crates/ui/src/viewport.rs` (its `edge_tests`) in M1.5
//! step 1, re-based on the 1 µm f64 lattice, plus the lattice pins the
//! old f32 version could not express.

use hsmworks_app::geometry::cube;
use hsmworks_app::geometry::feature::{feature_edges, EdgeKind};
use hsmworks_app::geometry::{Mesh, Point3};

fn flat(mesh: &Mesh) -> Vec<[f64; 3]> {
    mesh.facets
        .iter()
        .flat_map(|t| [[t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]])
        .collect()
}

fn flat_row() -> Vec<[f64; 3]> {
    // A 2x1 rectangle tiled with four coplanar triangles: no interior
    // seam may be drawn, all border edges must be.
    let quad = |x0: f64| {
        [
            [[x0, 0.0, 0.0], [x0 + 1.0, 1.0, 0.0], [x0 + 1.0, 0.0, 0.0]],
            [[x0, 0.0, 0.0], [x0, 1.0, 0.0], [x0 + 1.0, 1.0, 0.0]],
        ]
    };
    quad(0.0).into_iter().chain(quad(1.0)).flatten().collect()
}

fn kinds(edges: &[hsmworks_app::geometry::feature::FeatureEdge]) -> Vec<EdgeKind> {
    edges.iter().map(|e| e.kind).collect()
}

#[test]
fn coplanar_seam_is_not_an_edge() {
    // The 2x1 rectangle: 6 border edges, but neither the internal
    // vertical nor either diagonal (all coplanar seams).
    let edges = feature_edges(&flat_row());
    assert_eq!(edges.len(), 6);
    assert!(
        kinds(&edges).iter().all(|k| *k == EdgeKind::Boundary),
        "the visible edges are rims, not creases"
    );
}

#[test]
fn closed_cube_shows_its_twelve_box_edges() {
    let edges = feature_edges(&flat(&cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))));
    assert_eq!(edges.len(), 12, "a cube is 12 crease edges, no seams");
    assert!(
        kinds(&edges).iter().all(|k| *k == EdgeKind::Crease),
        "every box edge is a 90-degree bend"
    );
}

#[test]
fn empty_and_degenerate_inputs_stay_finite() {
    assert!(feature_edges(&[]).is_empty());
    let nan = [
        [f64::NAN, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
    ];
    // Non-finite corners never alias the lattice; their edges are drawn
    // as suspect, never dropped, never a panic.
    let e = feature_edges(&nan);
    assert_eq!(e.len(), 3);
}

#[test]
fn sub_micron_jitter_welds_the_seam() {
    // Two coplanar triangles sharing the (0,0)-(1,0) seam, the far side
    // jittered by 0.4 µm: inside the 1 µm quantum, so the seam welds and
    // (being coplanar) stays hidden; only the 4 rim edges remain.
    let d = 4e-7f64;
    let pts = [
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, -1.0, 0.0]],
        [[1.0 + d, 0.0, 0.0], [d, 0.0, 0.0], [0.5, 1.0, 0.0]],
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<[f64; 3]>>();
    assert_eq!(feature_edges(&pts).len(), 4);
    // Past the quantum (4 µm) the corners are distinct vertices: the seam
    // counts as two unshared rim edges, so six lines are emitted instead.
    let d = 4e-6f64;
    let pts = [
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, -1.0, 0.0]],
        [[1.0 + d, 0.0, 0.0], [d, 0.0, 0.0], [0.5, 1.0, 0.0]],
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<[f64; 3]>>();
    let edges = feature_edges(&pts);
    assert_eq!(edges.len(), 6);
    assert!(kinds(&edges).iter().all(|k| *k == EdgeKind::Boundary));
}

#[test]
fn non_manifold_edge_is_emitted_once_and_tagged() {
    // Three facets share the (0,0)-(1,0) seam: suspect, but drawn - and
    // drawn once, not once per incident facet.
    let pts = [
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, -1.0, 0.0]],
        [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.5, 1.0, 0.0]],
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, 0.0, 1.0]],
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<[f64; 3]>>();
    let edges = feature_edges(&pts);
    let nman = edges.iter().filter(|e| e.kind == EdgeKind::NonManifold).count();
    assert_eq!(nman, 1, "the shared seam is one visible line");
    assert_eq!(edges.len(), 7, "plus the six remaining rims");
}
