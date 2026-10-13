//! Slicer tests: mesh-crossing cycles in, normalized regions out. Goldens
//! are hand-computable prisms; degenerate meshes must error, never panic
//! (Prime Directive).

use hsmworks_app::geo2d::Point2;
use hsmworks_app::geometry::slice::{self, SliceError};
use hsmworks_app::geometry::{cube, cylinder, Point3, Triangle};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * b.max(1.0).abs()
}

/// Vertical walls of an extruded closed polygon (XY ring, z in [z0, z1]).
/// Caps are irrelevant to an interior slice, so prisms are walls only.
fn prism(poly: &[(f64, f64)], z0: f64, z1: f64) -> Vec<Triangle> {
    let mut f = Vec::new();
    let n = poly.len();
    for i in 0..n {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % n];
        let (a, b) = (Point3::new(x0, y0, z0), Point3::new(x1, y1, z0));
        let (c, d) = (Point3::new(x1, y1, z1), Point3::new(x0, y0, z1));
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    }
    f
}

#[test]
fn cube_mid_slice_yields_one_exact_loop() {
    let m = cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0));
    let r = slice::slice(&m.facets, 2.5).expect("interior slice");
    assert_eq!(r.loops().len(), 1);
    assert!(close(r.area(), 100.0));
    assert!(r.contains(Point2::new(5.0, 5.0)));
    assert!(!r.contains(Point2::new(10.5, 5.0)));
}

#[test]
fn half_open_in_z_top_finds_the_outline_bottom_finds_nothing() {
    // The deliberate asymmetry of the straddle rule, pinned: z-max slices
    // hit (their vertices are "at-or-above"), z-min slices miss.
    let m = cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0));
    let top = slice::slice(&m.facets, 5.0).expect("z-max is a closed outline");
    assert!(close(top.area(), 100.0));
    let bottom = slice::slice(&m.facets, 0.0).expect("z-min is empty");
    assert!(bottom.is_empty());
}

#[test]
fn plate_with_through_hole_slices_outer_and_hole() {
    let mut facets = prism(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)], 0.0, 3.0);
    facets.extend(prism(&[(8.0, 8.0), (12.0, 8.0), (12.0, 12.0), (8.0, 12.0)], 0.0, 3.0));
    let r = slice::slice(&facets, 1.5).expect("interior slice");
    assert_eq!(r.loops().len(), 2, "outer plus hole, orientation normalized");
    assert!(close(r.area(), 400.0 - 16.0));
    assert!(r.contains(Point2::new(1.0, 1.0)));
    assert!(!r.contains(Point2::new(10.0, 10.0)), "in the hole");
}

#[test]
fn open_sheet_slice_is_refused_not_half_used() {
    let facets = vec![Triangle::new(
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(10.0, 0.0, 0.0),
        Point3::new(5.0, 5.0, 10.0),
    )];
    let e = slice::slice(&facets, 5.0);
    assert!(
        matches!(e, Err(SliceError::Open { dangling: 2, .. })),
        "one chord has two dangling ends: {e:?}"
    );
}

#[test]
fn cylinder_slice_is_the_inscribed_polygon() {
    // Same 32-gon the viewport renders and pick reads: chord at mid height.
    let m = cylinder(20.0, 6.0, 32);
    let r = slice::slice(&m.facets, 3.0).expect("interior slice");
    assert_eq!(r.loops().len(), 1);
    let exact = 0.5 * 10.0f64.powi(2) * 32.0 * (2.0 * std::f64::consts::PI / 32.0).sin();
    assert!(close(r.area(), exact), "{} vs {exact}", r.area());
    // Wall triangulation diagonals cross the plane too, so each polygon
    // edge carries a collinear midpoint vertex alongside its corners:
    // radii run from r*cos(pi/32) (edge midpoints) to r (corners).
    for p in r.loops()[0].points() {
        let rad = (p.x * p.x + p.y * p.y).sqrt();
        assert!((9.951..=10.001).contains(&rad), "vertex {p:?} radius {rad}");
    }
}

#[test]
fn l_prism_footprint_unions_its_slices() {
    let l = [(0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (4.0, 4.0), (4.0, 10.0), (0.0, 10.0)];
    let facets = prism(&l, 0.0, 2.0);
    let r = slice::project(&facets, 8).expect("prism projects");
    assert_eq!(r.loops().len(), 1);
    assert!(close(r.area(), 100.0 - 36.0));
    assert!(r.contains(Point2::new(8.0, 2.0)));
    assert!(!r.contains(Point2::new(7.0, 7.0)), "cutout corner");
}

#[test]
fn figure_eight_section_decomposes_into_lobes() {
    // Two lobes touching along a vertical line: a real (non-manifold
    // contact) solid. Slicing must keep both lobes - the even-odd reading
    // of the crossing - where sketch input would be refused.
    let facets = prism(&[(0.0, 0.0), (2.0, 2.0), (2.0, 0.0), (0.0, 2.0)], 0.0, 3.0);
    let r = slice::project(&facets, 4).expect("lobes project");
    assert_eq!(r.loops().len(), 2, "the crossing splits into two lobes");
    assert!(close(r.area(), 2.0), "{}", r.area());
    assert!(r.contains(Point2::new(0.5, 1.0)));
    assert!(r.contains(Point2::new(1.5, 1.0)));
    assert!(!r.contains(Point2::new(1.0, 0.2)), "outside both lobes");
}

#[test]
fn flat_model_and_garbage_are_refused() {
    let sheet = vec![Triangle::new(
        Point3::new(0.0, 0.0, 2.0),
        Point3::new(1.0, 0.0, 2.0),
        Point3::new(0.0, 1.0, 2.0),
    )];
    assert!(matches!(
        slice::project(&sheet, 4),
        Err(SliceError::Flat { .. })
    ));
    assert!(slice::project(&[], 4).map(|r| r.is_empty()).unwrap_or(false));
    assert!(matches!(
        slice::slice(&sheet, f64::NAN),
        Err(SliceError::Invalid(_))
    ));
    let huge = vec![Triangle::new(
        Point3::new(1e13, 0.0, 0.0),
        Point3::new(0.0, 1.0, 1.0),
        Point3::new(1.0, 0.0, 1.0),
    )];
    assert!(matches!(
        slice::slice(&huge, 0.5),
        Err(SliceError::Geo(hsmworks_app::geo2d::GeoError::OutOfRange))
    ));
    assert!(matches!(
        slice::slice(&[sheet[0], huge[0]][..], 0.5),
        Err(SliceError::Geo(_))
    ));
}

#[test]
fn degenerate_soup_never_panics() {
    // Deterministic LCG soup: zero-area, collinear, dust-scale, grazing
    // heights. Every Ok/Err is fine; a hang or panic fails the run.
    let mut state = 0xDEADBEEFCAFEBABEu64;
    let mut rnd = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 40) as f64 / (1u64 << 24) as f64) - 0.5
    };
    for case in 0..200 {
        let span = match case % 3 {
            0 => 5.0,
            1 => 1e-6,
            _ => 400.0,
        };
        let mut facets = Vec::new();
        for _ in 0..(3 + case % 5) {
            facets.push(Triangle::new(
                Point3::new(rnd() * span, rnd() * span, rnd() * span),
                Point3::new(rnd() * span, rnd() * span, rnd() * span),
                Point3::new(rnd() * span, rnd() * span, rnd() * span),
            ));
        }
        let _ = slice::slice(&facets, 0.0);
        let _ = slice::slice(&facets, span * 0.5);
        let _ = slice::slice(&facets, -span);
        let _ = slice::project(&facets, 6);
    }
}
