//! Mesh repair (M13.11): welding closes a split seam, hole filling puts a
//! missing face back with outward winding, and every malformed input
//! refuses by name - never a panic (Prime Directive).
//!
//! Gate map (M4.4): covers camcraft_geometry::geometry::repair,
//! camcraft_geometry::geometry.

use camcraft_geometry::geo2d::GeoError;
use camcraft_geometry::geometry::repair::{self, open_edge_count};
use camcraft_geometry::geometry::{cube, Mesh, Point3, Triangle};

fn unit_cube() -> Mesh {
    cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))
}

/// Divergence theorem on the facet soup: positive for outward winding.
fn signed_volume(m: &Mesh) -> f64 {
    m.facets.iter().map(|t| t.a.dot(t.b.cross(t.c)) / 6.0).sum()
}

fn same(p: Point3, q: Point3) -> bool {
    p == q
}

/// The unit cube with corner (1,1,1) split: the top face spells it
/// 10 um higher than the two side faces that share it.
fn split_seam_cube() -> Mesh {
    let corner = Point3::new(1.0, 1.0, 1.0);
    let lifted = Point3::new(1.0, 1.0, 1.0 + 1e-5);
    let facets: Vec<Triangle> = unit_cube()
        .facets
        .iter()
        .enumerate()
        .map(|(i, t)| {
            // facets 2 and 3 are the top (z+) quad in `cube`
            if (2..4).contains(&i) {
                let fix = |p: Point3| if same(p, corner) { lifted } else { p };
                Triangle::new(fix(t.a), fix(t.b), fix(t.c))
            } else {
                *t
            }
        })
        .collect();
    Mesh::from_facets(facets)
}

fn without_bottom() -> Mesh {
    Mesh::from_facets(unit_cube().facets[2..].to_vec())
}

fn message(e: GeoError) -> String {
    e.to_string()
}

#[test]
fn split_seam_welds_to_eight_vertices_and_twelve_triangles() {
    let m = split_seam_cube();
    assert_eq!(open_edge_count(&m), 4, "the lifted corner opens its two top edges on both sides");
    let r = repair::weld(&m, 1e-3).expect("welds");
    assert_eq!((r.vertices_before, r.vertices_after), (9, 8));
    assert_eq!((r.triangles_before, r.triangles_after), (12, 12));
    assert_eq!((r.welded, r.dropped), (1, 0));
    assert!(r.changed());
    assert_eq!(open_edge_count(&r.mesh), 0, "the seam is closed");
    // The representative is the first occurrence (the lifted corner), so
    // the solid moved by at most the 10 um split - never more than `tol`.
    assert!((signed_volume(&r.mesh) - 1.0).abs() < 1e-4);
    // Below the split the seam stays open: welding is by the stated tolerance.
    let tight = repair::weld(&m, 1e-9).expect("welds nothing");
    assert_eq!(tight.vertices_after, 9);
    assert!(!tight.changed());
}

#[test]
fn weld_drops_collapsed_zero_area_and_duplicate_triangles() {
    let mut facets = unit_cube().facets.to_vec();
    let p = Point3::new(0.5, 0.5, 1.0);
    facets.push(Triangle::new(p, p, p)); // collapsed
    facets.push(Triangle::new(
        Point3::new(0.0, 0.0, 1.0),
        Point3::new(0.5, 0.0, 1.0),
        Point3::new(1.0, 0.0, 1.0),
    )); // collinear, zero area
    facets.push(facets[0]); // exact duplicate
    let t = facets[1];
    facets.push(Triangle::new(t.a, t.c, t.b)); // duplicate, flipped
    let r = repair::weld(&Mesh::from_facets(facets), 0.0).expect("welds");
    assert_eq!(r.dropped, 4);
    assert_eq!(r.triangles_after, 12);
    assert_eq!(r.vertices_after, 8, "vertices only the dropped facets used are gone");
}

#[test]
fn missing_face_comes_back_outward() {
    let m = without_bottom();
    assert_eq!(open_edge_count(&m), 4);
    let r = repair::fill_holes(&m, 64).expect("fills");
    assert_eq!((r.holes_filled, r.triangles_added), (1, 2));
    assert_eq!(r.triangles_after, 12);
    assert_eq!(open_edge_count(&r.mesh), 0);
    assert!((signed_volume(&r.mesh) - 1.0).abs() < 1e-9, "outward winding, {}", signed_volume(&r.mesh));
    // A closed mesh has nothing to fill, and says so without changing.
    let closed = repair::fill_holes(&r.mesh, 64).expect("no holes");
    assert!(!closed.changed());
    assert_eq!(closed.mesh, r.mesh);
}

#[test]
fn weld_then_fill_reads_as_one_report() {
    let m = Mesh::from_facets(split_seam_cube().facets[2..].to_vec());
    let r = repair::weld(&m, 1e-3).expect("welds");
    let r = r.clone().then(repair::fill_holes(&r.mesh, 64).expect("fills"));
    assert_eq!((r.welded, r.holes_filled, r.triangles_added), (1, 1, 2));
    assert_eq!((r.triangles_before, r.triangles_after), (10, 12));
    assert!(r.describe().contains("filled 1 hole"), "{}", r.describe());
}

#[test]
fn non_manifold_edge_refuses_by_name() {
    let mut facets = without_bottom().facets.to_vec();
    // A fin on the top face's diagonal: three facets on one edge.
    facets.push(Triangle::new(
        Point3::new(0.0, 0.0, 1.0),
        Point3::new(1.0, 1.0, 1.0),
        Point3::new(0.5, 0.5, 2.0),
    ));
    let err = message(repair::fill_holes(&Mesh::from_facets(facets), 64).expect_err("refuses"));
    assert!(err.contains("non-manifold edge") && err.contains("3 facets"), "{err}");
}

#[test]
fn inconsistent_winding_refuses_by_name() {
    let mut facets = without_bottom().facets.to_vec();
    let t = facets[0];
    facets[0] = Triangle::new(t.a, t.c, t.b);
    let err = message(repair::fill_holes(&Mesh::from_facets(facets), 64).expect_err("refuses"));
    assert!(err.contains("inconsistently wound"), "{err}");
}

/// A cone without its base: the hole is the whole ring.
fn open_cone(edges: usize) -> Mesh {
    let apex = Point3::new(0.0, 0.0, 1.0);
    let ring = |i: usize| {
        let a = std::f64::consts::TAU * (i % edges) as f64 / edges as f64;
        Point3::new(a.cos(), a.sin(), 0.0)
    };
    Mesh::from_facets((0..edges).map(|i| Triangle::new(apex, ring(i), ring(i + 1))).collect())
}

#[test]
fn hole_over_the_budget_refuses_by_name_and_under_it_fills() {
    let m = open_cone(1000);
    assert_eq!(open_edge_count(&m), 1000);
    let err = message(repair::fill_holes(&m, 256).expect_err("over budget"));
    assert!(err.contains("1000 edges") && err.contains("256 edge budget"), "{err}");
    let r = repair::fill_holes(&m, 1000).expect("within budget");
    assert_eq!((r.holes_filled, r.triangles_added), (1, 998));
    assert_eq!(open_edge_count(&r.mesh), 0);
    let v = signed_volume(&r.mesh);
    assert!((v - std::f64::consts::PI / 3.0).abs() < 1e-3, "outward cone volume {v}");
}

#[test]
fn non_planar_and_self_crossing_loops_refuse_by_name() {
    // The open cone with the ring pulled into a crumple: alternate
    // vertices lifted to z = 1 is a saddle well off any plane.
    let crumpled: Vec<Triangle> = open_cone(16)
        .facets
        .iter()
        .map(|t| {
            let lift = |p: Point3| {
                if p.z == 0.0 && p.y > 0.0 && p.x > 0.0 {
                    Point3::new(p.x, p.y, 0.9)
                } else {
                    p
                }
            };
            Triangle::new(lift(t.a), lift(t.b), lift(t.c))
        })
        .collect();
    let err = message(repair::fill_holes(&Mesh::from_facets(crumpled), 64).expect_err("non-planar"));
    assert!(err.contains("non-planar"), "{err}");

    // A bow-tie boundary: the ring crosses itself in its plane (unequal
    // lobes, so the plane fit itself survives and the crossing is what
    // refuses).
    let apex = Point3::new(0.0, 0.0, 1.0);
    let q = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(2.0, 2.0, 0.0),
        Point3::new(2.0, 0.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    ];
    let bowtie = Mesh::from_facets((0..4).map(|i| Triangle::new(apex, q[i], q[(i + 1) % 4])).collect());
    let err = message(repair::fill_holes(&bowtie, 64).expect_err("self-crossing"));
    assert!(err.contains("self-intersecting"), "{err}");
    // Equal lobes cancel to no plane at all: still a named refusal.
    let q = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 1.0, 0.0),
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    ];
    let flat = Mesh::from_facets((0..4).map(|i| Triangle::new(apex, q[i], q[(i + 1) % 4])).collect());
    let err = message(repair::fill_holes(&flat, 64).expect_err("no plane"));
    assert!(err.contains("degenerate hole boundary"), "{err}");
}

#[test]
fn nonsense_parameters_refuse_by_name() {
    let m = without_bottom();
    for tol in [-1.0, f64::NAN, f64::INFINITY] {
        let err = message(repair::weld(&m, tol).expect_err("bad tolerance"));
        assert!(err.contains("weld tolerance"), "{err}");
    }
    for budget in [0, 2] {
        let err = message(repair::fill_holes(&m, budget).expect_err("bad budget"));
        assert!(err.contains("hole budget"), "{err}");
    }
    let far = Mesh::from_facets(vec![Triangle::new(
        Point3::new(1e300, 0.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
        Point3::new(0.0, 0.0, 1.0),
    )]);
    assert_eq!(repair::weld(&far, 1e-3), Err(GeoError::OutOfRange));
}

#[test]
fn garbage_meshes_error_cleanly_never_panic() {
    let nan = Point3::new(f64::NAN, 0.0, 0.0);
    let inf = Point3::new(f64::INFINITY, 1.0, 1.0);
    let p = Point3::new(1.0, 2.0, 3.0);
    let soups: Vec<Vec<Triangle>> = vec![
        vec![],
        vec![Triangle::new(nan, p, p)],
        vec![Triangle::new(inf, inf, inf)],
        vec![Triangle::new(p, p, p); 7],
        vec![Triangle::new(p, Point3::new(2.0, 4.0, 6.0), Point3::new(3.0, 6.0, 9.0))],
        vec![Triangle::new(Point3::new(-0.0, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0))],
    ];
    for facets in soups {
        let m = Mesh::from_facets(facets.clone());
        let finite = m.all_finite();
        match repair::weld(&m, 1e-3) {
            Ok(r) => assert!(finite && r.mesh.all_finite()),
            Err(_) => assert!(!finite, "finite input welds: {facets:?}"),
        }
        if let Ok(r) = repair::fill_holes(&m, 64) {
            assert!(finite && r.mesh.all_finite());
        }
        let _ = open_edge_count(&m);
    }
}
