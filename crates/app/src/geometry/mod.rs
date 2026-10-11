//! Core geometry types and mesh import.
//!
//! Provenance: STL/OBJ are public formats (binary STL layout and the OBJ line
//! grammar are widely documented); parsers are written from those public
//! specs only.
//!
//! Robustness bar (AGENTS.md Prime Directive): every parser path is total -
//! malformed input returns `ImportError`, never panics. All float input is
//! validated finite before it reaches a mesh. Facet/vertex counts are capped
//! before any bulk allocation, so a corrupt header cannot OOM us.

pub mod import;
mod primitives;

pub use import::{ImportError, load_model};
pub use primitives::cube;

/// A point in world space. f64 throughout the domain layer; the GPU converts
/// to f32 at the draw boundary, never the other way around.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Point3 {
    pub const ZERO: Point3 = Point3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Point3 { x, y, z }
    }

    /// False for NaN/inf; the only sanctioned gate for imported coordinates.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }


    pub fn cross(self, other: Point3) -> Point3 {
        Point3::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn dot(self, other: Point3) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
}

impl std::ops::Sub for Point3 {
    type Output = Point3;
    fn sub(self, other: Point3) -> Point3 {
        Point3::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

/// One planar facet. The stored `normal` field is reserved for file-declared
/// values; importers currently always leave it zero. Orientation comes from
/// `Triangle::normal` (recomputed from vertices) - never trust the field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub a: Point3,
    pub b: Point3,
    pub c: Point3,
    pub normal: Point3,
}

impl Triangle {
    pub fn new(a: Point3, b: Point3, c: Point3) -> Self {
        Triangle {
            a,
            b,
            c,
            normal: Point3::ZERO,
        }
    }

    /// Right-hand-rule facet normal; zero if the triangle is degenerate.
    pub fn normal(&self) -> Point3 {
        let n = (self.b - self.a).cross(self.c - self.a);
        let len2 = n.dot(n);
        if len2 > 0.0 {
            let inv = 1.0 / len2.sqrt();
            Point3::new(n.x * inv, n.y * inv, n.z * inv)
        } else {
            Point3::ZERO
        }
    }
}

/// Axis-aligned bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Point3,
    pub max: Point3,
}

impl Aabb {
    pub fn center(self) -> Point3 {
        Point3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }
}

/// A triangulated surface: the Phase-1 working representation of a model and
/// of stock. `Triangle`-only storage (no vertex sharing) keeps facet order
/// authoritative for later stock comparison; dedup happens at import for OBJ.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub facets: Vec<Triangle>,
}

impl Mesh {
    pub fn new() -> Self {
        Mesh::default()
    }

    pub fn from_facets(facets: Vec<Triangle>) -> Self {
        Mesh { facets }
    }

    pub fn is_empty(&self) -> bool {
        self.facets.is_empty()
    }

    pub fn len(&self) -> usize {
        self.facets.len()
    }

    pub fn bounds(&self) -> Option<Aabb> {
        let mut iter = self.facets.iter();
        let first = iter.next()?;
        let mut min = Point3::new(
            first.a.x.min(first.b.x).min(first.c.x),
            first.a.y.min(first.b.y).min(first.c.y),
            first.a.z.min(first.b.z).min(first.c.z),
        );
        let mut max = Point3::new(
            first.a.x.max(first.b.x).max(first.c.x),
            first.a.y.max(first.b.y).max(first.c.y),
            first.a.z.max(first.b.z).max(first.c.z),
        );
        for t in iter {
            for p in [t.a, t.b, t.c] {
                min.x = min.x.min(p.x);
                min.y = min.y.min(p.y);
                min.z = min.z.min(p.z);
                max.x = max.x.max(p.x);
                max.y = max.y.max(p.y);
                max.z = max.z.max(p.z);
            }
        }
        Some(Aabb { min, max })
    }

    /// Reject any non-finite coordinate; called by importers before a Mesh
    /// exists in the world (Prime Directive: validate early).
    fn all_finite(&self) -> bool {
        self.facets
            .iter()
            .all(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangle_normal_right_hand_rule() {
        let t = Triangle::new(
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        );
        let n = t.normal();
        assert!(n.z > 0.99);
    }

    #[test]
    fn degenerate_normal_is_zero_not_nan() {
        let p = Point3::new(1.0, 2.0, 3.0);
        let t = Triangle::new(p, p, p);
        assert_eq!(t.normal(), Point3::ZERO);
    }

    #[test]
    fn aabb_from_facets() {
        let m = cube(Point3::ZERO, Point3::new(2.0, 2.0, 2.0));
        let b = m.bounds().expect("cube has bounds");
        assert!(b.min.x == 0.0 && b.max.z == 2.0);
        assert!(m.all_finite());
    }
}
