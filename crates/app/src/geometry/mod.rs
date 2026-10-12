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

/// A rotation as a unit quaternion, stored (x, y, z, w); identity is
/// `[0, 0, 0, 1]`.
///
/// Provenance: standard rigid-transform mathematics - quaternion rotation
/// as documented in any graphics reference (Shoemake, "Rotating Elegantly",
/// ACM SIGGRAPH 1994 course notes); reimplemented from the published
/// formulas, no third-party code.
pub const IDENTITY_Q: [f64; 4] = [0.0, 0.0, 0.0, 1.0];

/// Scale `q` to unit norm; `None` for a zero or non-finite quaternion (the
/// caller decides between keeping the old value and falling back to
/// identity - we never silently "fix" garbage).
#[must_use]
pub fn normalize_quat(q: [f64; 4]) -> Option<[f64; 4]> {
    let n2 = q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3];
    if n2.is_finite() && n2 > 0.0 {
        let inv = 1.0 / n2.sqrt();
        Some([q[0] * inv, q[1] * inv, q[2] * inv, q[3] * inv])
    } else {
        None
    }
}

/// Pure Z-axis rotation (yaw) by `rad` radians; non-finite input yields
/// identity rather than NaN.
#[must_use]
pub fn quat_from_yaw(rad: f64) -> [f64; 4] {
    if rad.is_finite() {
        let (s, c) = (0.5f64 * rad).sin_cos();
        [0.0, 0.0, s, c]
    } else {
        IDENTITY_Q
    }
}

/// The inverse of a UNIT quaternion (rotation part negated).
#[must_use]
pub fn quat_conjugate(q: [f64; 4]) -> [f64; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}

/// Rotate `p` by unit quaternion `q` (v' = v + w·t + qv×t with t = 2·qv×v).
/// Pass the conjugate for the inverse rotation.
#[must_use]
pub fn quat_rotate_point(q: [f64; 4], p: Point3) -> Point3 {
    let qv = Point3::new(q[0], q[1], q[2]);
    let c = qv.cross(p);
    let t = Point3::new(2.0 * c.x, 2.0 * c.y, 2.0 * c.z);
    let u = qv.cross(t);
    Point3::new(
        p.x + q[3] * t.x + u.x,
        p.y + q[3] * t.y + u.y,
        p.z + q[3] * t.z + u.z,
    )
}

/// local -> world: rotate by `rot` (unit), then translate by `pos`.
#[must_use]
pub fn transform_point(p: Point3, pos: Point3, rot: [f64; 4]) -> Point3 {
    let r = quat_rotate_point(rot, p);
    Point3::new(r.x + pos.x, r.y + pos.y, r.z + pos.z)
}

/// world -> local: undo the translation, then the rotation. Exact inverse of
/// [`transform_point`] for any unit `rot`.
#[must_use]
pub fn transform_inverse_point(p: Point3, pos: Point3, rot: [f64; 4]) -> Point3 {
    quat_rotate_point(
        quat_conjugate(rot),
        Point3::new(p.x - pos.x, p.y - pos.y, p.z - pos.z),
    )
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

    fn close(a: Point3, b: Point3) -> bool {
        (a - b).dot(a - b) < 1e-24
    }

    #[test]
    fn identity_quat_is_a_no_op() {
        let p = Point3::new(1.5, -2.25, 3.0);
        assert!(close(quat_rotate_point(IDENTITY_Q, p), p));
    }

    #[test]
    fn yaw_90_maps_x_to_y_exactly() {
        let q = quat_from_yaw(std::f64::consts::FRAC_PI_2);
        let p = quat_rotate_point(q, Point3::new(1.0, 0.0, 0.0));
        assert!(close(p, Point3::new(0.0, 1.0, 0.0)), "{p:?}");
        // z is invariant under a z-rotation
        assert!(close(quat_rotate_point(q, Point3::new(0.0, 0.0, 7.0)), Point3::new(0.0, 0.0, 7.0)));
    }

    #[test]
    fn transform_round_trip() {
        let pos = Point3::new(10.0, -4.0, 0.5);
        let rot = quat_from_yaw(0.7);
        let p = Point3::new(1.25, 2.5, -3.75);
        let w = transform_point(p, pos, rot);
        assert!(close(transform_inverse_point(w, pos, rot), p));
    }

    #[test]
    fn normalize_garbage_is_none_not_nan() {
        assert!(normalize_quat([0.0, 0.0, 0.0, 0.0]).is_none());
        assert!(normalize_quat([f64::NAN, 0.0, 0.0, 1.0]).is_none());
        assert!(normalize_quat([f64::INFINITY, 0.0, 0.0, 0.0]).is_none());
        let q = normalize_quat([2.0, 0.0, 0.0, 0.0]).expect("unit x-quat");
        assert!((q[3] - 0.0).abs() < 1e-15 && (q[0] - 1.0).abs() < 1e-15);
        // A unit quat normalizes to itself (rotation unchanged).
        let yaw = quat_from_yaw(1.1);
        let again = normalize_quat(yaw).expect("already unit");
        assert!(yaw.iter().zip(again).all(|(a, b)| (a - b).abs() < 1e-12));
    }

    #[test]
    fn yaw_of_nonfinite_is_identity() {
        assert_eq!(quat_from_yaw(f64::NAN), IDENTITY_Q);
        assert_eq!(quat_from_yaw(f64::INFINITY), IDENTITY_Q);
    }
}
