//! The turned profile: the axis frame the lathe reads the model in,
//! and the silhouette extents the facing kernel lives inside (M5.2 A).
//!
//! The frame is not new geometry: the axis is exactly what stage E's
//! [`crate::multiaxis::axis::fit`] claims from the facet normals -
//! one truth for "where the spindle is" across families - with the
//! twins' own two guards restated: a directionless fit is a corrupt
//! mesh, and an axis lying near the table makes world Z heights
//! meaningless AS stations, so it is refused by name, not worked
//! around.
//!
//! Provenance: the meridian section of a surface of revolution as a
//! (station, radius) reading of the mesh is classical descriptive
//! geometry as any machining handbook draws it; written from that
//! and our own design notes. No vendor material consulted.

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

/// World up, spelled once (Point3::new is not a const fn) - the
/// rotary twins' constant, restated because turning must not reach
/// into multiaxis' privates.
const UP: Point3 = Point3 {
    x: 0.0,
    y: 0.0,
    z: 1.0,
};
/// The axis' up-component below which world Z planes cannot name
/// stations along it (a plane nearly parallel to the axis meets it,
/// if at all, a million stations away): the twins' sin-1-degree
/// boundary, the same number for the same reason.
const LEAN_MIN: f64 = 0.0175;

/// The frame the turning kernels walk in: a point on the axis, the
/// unit direction in the world's +z sense (so a station grows with
/// height, whichever way the fit happened to point), and ONE radial
/// direction - every facing pass runs in the plane that pair spans,
/// which is how a turret-mounted insert meets a spinning part.
pub struct Frame {
    origin: Point3,
    dir: Point3,
    radial: Point3,
    /// `dir.z`; strictly above [`LEAN_MIN`], the division's promise.
    uz: f64,
}

impl Frame {
    /// Fit, upright-sign canonicalize, lean-guard and frame in one
    /// call: the model must say "revolved" (axis::fit's own refusal,
    /// by name) and must say it about an axis the height parameters
    /// can walk.
    pub fn about(facets: &[Triangle]) -> Result<Frame, GenError> {
        let ax = crate::multiaxis::axis::fit(facets)?;
        let d = ax.dir;
        let n = (d.dot(d)).sqrt();
        if !n.is_finite() || n <= 0.0 {
            return Err(GenError::Invalid(
                "the fitted axis is not a direction - the mesh is corrupt".into(),
            ));
        }
        let mut u = Point3::new(d.x / n, d.y / n, d.z / n);
        if u.z < 0.0 {
            u = Point3::new(-u.x, -u.y, -u.z);
        }
        if u.z < LEAN_MIN {
            return Err(GenError::Invalid(format!(
                "the model's axis of revolution stands {:.0} degrees off vertical - \
                 turning heights are world Z planes read as stations, and a lying \
                 axis makes them meaningless (the rotary kinds own that posture)",
                90.0 - u.z.clamp(0.0, 1.0).acos().to_degrees()
            )));
        }
        // axis.rs' basis law, verbatim: any vector off the axis, then
        // the direction it completes against.
        let any = if u.z < 0.9 { UP } else { Point3::new(1.0, 0.0, 0.0) };
        let radial = match normalize(u.cross(any)) {
            Some(v) => v,
            None => {
                return Err(GenError::Invalid(
                    "the fitted axis is not a direction - the mesh is corrupt".into(),
                ))
            }
        };
        Ok(Frame {
            origin: ax.origin,
            dir: u,
            radial,
            uz: u.z,
        })
    }

    /// The station a world Z plane meets the axis at.
    pub fn station_of(&self, world_z: f64) -> f64 {
        (world_z - self.origin.z) / self.uz
    }

    /// The world height a point ON the axis stands at that station
    /// (diagnostic wording only - the geometry never needs it).
    pub fn height_of(&self, station: f64) -> f64 {
        self.origin.z + station * self.uz
    }

    /// The world point at radius `rho` from the axis, at `station`,
    /// in the plane the frame chose.
    pub fn point(&self, rho: f64, station: f64) -> Point3 {
        Point3::new(
            self.origin.x + self.dir.x * station + self.radial.x * rho,
            self.origin.y + self.dir.y * station + self.radial.y * rho,
            self.origin.z + self.dir.z * station + self.radial.z * rho,
        )
    }
}

/// The body's own extents in the frame: the radius no material
/// exceeds and the station no material tops.
pub struct Profile {
    pub r_max: f64,
    pub z_top: f64,
}

/// Sweep the facets once. The distance to a line is CONVEX, and the
/// body is the hull of all its vertices, so the WHOLE-BODY maximum
/// is taken at a vertex: `r_max` and `z_top` are exact from vertices
/// alone. That is all this fold promises: per station it says
/// nothing. A band's material can stand inside the station window
/// while every vertex of the facet that carries it stands outside
/// it - a cylinder wall between its two rim rings - and an
/// infeed radius read from window-filtered vertices is then too
/// SMALL, which is a rapid through skin. face.rs's reach sweep
/// asks triangles, not vertices, which stations they occupy.
pub fn profile_of(facets: &[Triangle], f: &Frame) -> Result<Profile, GenError> {
    let (mut r_max, mut z_top) = (0.0f64, f64::NEG_INFINITY);
    for t in facets {
        for q in [t.a, t.b, t.c] {
            if !q.is_finite() {
                return Err(GenError::Invalid(
                    "the model carries a non-finite vertex - the mesh is corrupt".into(),
                ));
            }
            let (s, rho) = f.sr(q);
            r_max = r_max.max(rho);
            z_top = z_top.max(s);
        }
    }
    Ok(Profile { r_max, z_top })
}

impl Frame {
    /// A vertex read as (station, radius): the meridian section's
    /// own coordinates, the pair every turning kernel thinks in.
    pub fn sr(&self, q: Point3) -> (f64, f64) {
        let d = q - self.origin;
        let s = d.dot(self.dir);
        let w = Point3::new(
            d.x - self.dir.x * s,
            d.y - self.dir.y * s,
            d.z - self.dir.z * s,
        );
        (s, (w.dot(w)).sqrt())
    }
}

fn normalize(v: Point3) -> Option<Point3> {
    let n2 = v.dot(v);
    if n2.is_finite() && n2 > 0.0 {
        let n = n2.sqrt();
        Some(Point3::new(v.x / n, v.y / n, v.z / n))
    } else {
        None
    }
}
