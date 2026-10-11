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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_facets_face_outward() {
        let m = cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0));
        assert_eq!(m.len(), 12);
        let center = Point3::new(0.5, 0.5, 0.5);
        for t in &m.facets {
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
}
