//! Analytic primitives, used by stock definition and by tests.

use super::{Mesh, Point3, Triangle};

/// Axis-aligned box as 12 outward-facing triangles (CCW from outside).
pub fn cube(min: Point3, max: Point3) -> Mesh {
    let p = [
        Point3::new(min.x, min.y, min.z), // 0 - -
        Point3::new(max.x, min.y, min.z), // 1 + -
        Point3::new(max.x, max.y, min.z), // 2 + +
        Point3::new(min.x, max.y, min.z), // 3 - +
        Point3::new(min.x, min.y, max.z), // 4 - -
        Point3::new(max.x, min.y, max.z), // 5 + -
        Point3::new(max.x, max.y, max.z), // 6 + +
        Point3::new(min.x, max.y, max.z), // 7 - +
    ];
    // quads as (a, b, c, d) CCW seen from outside
    let quads = [
        (3, 2, 1, 0), // bottom z-
        (4, 5, 6, 7), // top z+
        (0, 1, 5, 4), // front y-
        (2, 3, 7, 6), // back y+
        (1, 2, 6, 5), // right x+
        (3, 0, 4, 7), // left x-
    ];
    let mut facets = Vec::with_capacity(12);
    for (a, b, c, d) in quads {
        facets.push(Triangle::new(p[a], p[b], p[c]));
        facets.push(Triangle::new(p[a], p[c], p[d]));
    }
    Mesh::from_facets(facets)
}

/// Closed cylinder of `4 * segments` triangles, CCW seen from outside. Local
/// frame: axis on Z, XY-centered about the origin, z in [0, height] - the
/// stock convention (the app crate's `stock` module).
///
/// Total function: degenerate input (non-finite or non-positive size, fewer
/// than 3 segments) yields an empty mesh, never nonsense triangles; the
/// command layer turns "empty" into the user-visible error.
pub fn cylinder(diameter: f64, height: f64, segments: u32) -> Mesh {
    if !diameter.is_finite()
        || !height.is_finite()
        || diameter <= 0.0
        || height <= 0.0
        || segments < 3
    {
        return Mesh::default();
    }
    let r = diameter * 0.5;
    let n = segments as usize;
    let ring = |i: usize, z: f64| -> Point3 {
        let a = std::f64::consts::TAU * (i as f64) / (n as f64);
        Point3::new(r * a.cos(), r * a.sin(), z)
    };
    let mut facets = Vec::with_capacity(4 * n);
    for i in 0..n {
        let j = (i + 1) % n;
        let (b0, b1) = (ring(i, 0.0), ring(j, 0.0));
        let (t0, t1) = (ring(i, height), ring(j, height));
        // Side quad split so both triangles wind outward ((b0,b1,t1) has
        // normal +x at theta 0 - checked by the tests below).
        facets.push(Triangle::new(b0, b1, t1));
        facets.push(Triangle::new(b0, t1, t0));
        // Cap fans from the axis centers: top CCW from +z, bottom from -z.
        facets.push(Triangle::new(Point3::new(0.0, 0.0, height), t0, t1));
        facets.push(Triangle::new(Point3::ZERO, b1, b0));
    }
    Mesh::from_facets(facets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_facets_face_outward() {
        let m = cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0));
        assert_eq!(m.len(), 12);
        let center = Point3::new(0.5, 0.5, 0.5);
        for t in m.facets.iter() {
            // facet centroid to outside must agree with the normal direction
            let cx = Point3::new(
                (t.a.x + t.b.x + t.c.x) / 3.0,
                (t.a.y + t.b.y + t.c.y) / 3.0,
                (t.a.z + t.b.z + t.c.z) / 3.0,
            );
            let outward = cx - center;
            let n = t.normal();
            assert!(n.dot(outward) > 0.0, "facet points inward: {t:?}");
        }
    }

    #[test]
    fn cylinder_facets_face_outward() {
        let m = cylinder(10.0, 4.0, 16);
        assert_eq!(m.len(), 64, "2 sides + 2 cap tris per segment");
        assert!(m.all_finite());
        // The cylinder is convex, so the centroid trick from the cube test
        // applies with any interior point: here the axis midpoint.
        let center = Point3::new(0.0, 0.0, 2.0);
        for t in m.facets.iter() {
            let cx = Point3::new(
                (t.a.x + t.b.x + t.c.x) / 3.0,
                (t.a.y + t.b.y + t.c.y) / 3.0,
                (t.a.z + t.b.z + t.c.z) / 3.0,
            );
            let outward = cx - center;
            let n = t.normal();
            assert!(n.dot(outward) > 0.0, "facet points inward: {t:?}");
        }
    }

    #[test]
    fn cylinder_sits_on_z_zero_and_is_xy_centered() {
        let m = cylinder(8.0, 3.0, 24);
        let b = m.bounds().expect("mesh");
        assert!(b.min.z == 0.0 && (b.max.z - 3.0).abs() < 1e-12);
        assert!(b.min.x < -3.9 && b.max.x > 3.9, "r=4: {b:?}");
        assert!(b.min.y < -3.9 && b.max.y > 3.9);
    }

    #[test]
    fn cylinder_degenerate_input_is_empty_not_nan() {
        assert!(cylinder(0.0, 5.0, 16).is_empty());
        assert!(cylinder(-1.0, 5.0, 16).is_empty());
        assert!(cylinder(f64::NAN, 5.0, 16).is_empty());
        assert!(cylinder(5.0, f64::INFINITY, 16).is_empty());
        assert!(cylinder(5.0, 1.0, 2).is_empty());
    }
}
