//! Arc and circle centres of round feature-edge runs (#692): a hole rim
//! or a boss edge tessellated into chords still has one centre, and a
//! point pick that asks for centres can snap to it.
//!
//! Provenance: the circumcentre of three points (standard vector form,
//! c = p0 + ((|a|^2 b - |b|^2 a) x (a x b)) / (2 |a x b|^2)) checked
//! against every vertex of the run - textbook analytic geometry; our own
//! code, no vendor material consulted.

/// Every vertex of a run must sit this close to the fitted circle,
/// relative to its radius. Tessellated rims put their vertices ON the
/// circle (only chords cut inside it), so this is float noise plus lattice
/// welding (1 µm), not a fit tolerance.
pub const ARC_REL_TOL: f64 = 1e-3;

/// Absolute floor of the same tolerance, mm (a tiny hole at the 1 µm weld).
pub const ARC_ABS_TOL: f64 = 1e-3;

/// A run needs at least this many edges before it is called round: a
/// single chord or a two-segment kink is not an arc.
pub const ARC_MIN_EDGES: usize = 3;

/// The centre of one round feature-edge run, local frame.
#[derive(Clone, Debug, PartialEq)]
pub struct ArcCentre {
    pub centre: [f64; 3],
    /// Unit normal of the circle's plane (sign follows the run's walk).
    pub normal: [f64; 3],
    pub radius: f64,
    /// A full circle (the run closes on itself) rather than an arc.
    pub closed: bool,
    /// The run's edges, as indices into `Features::edges`.
    pub edges: Vec<u32>,
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The circle through three points: (centre, unit normal, radius), or
/// `None` when they are (nearly) collinear or the math is not finite.
#[must_use]
pub fn circumcircle(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3]) -> Option<([f64; 3], [f64; 3], f64)> {
    let (a, b) = (sub(p1, p0), sub(p2, p0));
    let n = cross(a, b);
    let nn = dot(n, n);
    // Collinear (sin of the angle at p0 under 1e-4): no circle - a
    // straight run with weld noise must not fit a kilometre-wide arc.
    if !(nn.is_finite() && nn > 1e-8 * dot(a, a) * dot(b, b)) {
        return None;
    }
    let (aa, bb) = (dot(a, a), dot(b, b));
    let w = [aa * b[0] - bb * a[0], aa * b[1] - bb * a[1], aa * b[2] - bb * a[2]];
    let off = cross(w, n);
    let c = [p0[0] + off[0] / (2.0 * nn), p0[1] + off[1] / (2.0 * nn), p0[2] + off[2] / (2.0 * nn)];
    let r = dot(sub(p0, c), sub(p0, c)).sqrt();
    let m = nn.sqrt();
    let unit = [n[0] / m, n[1] / m, n[2] / m];
    (c.iter().chain(unit.iter()).all(|v| v.is_finite()) && r.is_finite() && r > 0.0).then_some((c, unit, r))
}

/// Fit a circle to a walked run's vertices (`pts`, in walk order; a closed
/// run repeats its start at the end). `None` unless the run has at least
/// [`ARC_MIN_EDGES`] edges and every vertex lies on one circle, in one
/// plane, within tolerance.
#[must_use]
pub fn fit_arc(pts: &[[f64; 3]], closed: bool, edges: Vec<u32>) -> Option<ArcCentre> {
    if edges.len() < ARC_MIN_EDGES || pts.len() < ARC_MIN_EDGES + 1 {
        return None;
    }
    // Three well-spread samples: thirds of the run.
    let n = if closed { pts.len() - 1 } else { pts.len() };
    let (i1, i2) = if closed { (n / 3, 2 * n / 3) } else { (n / 2, n - 1) };
    let (centre, normal, radius) = circumcircle(*pts.first()?, *pts.get(i1)?, *pts.get(i2)?)?;
    let tol = (radius * ARC_REL_TOL).max(ARC_ABS_TOL);
    let on_circle = pts.iter().all(|p| {
        let d = sub(*p, centre);
        let r = dot(d, d).sqrt();
        (r - radius).abs() <= tol && dot(d, normal).abs() <= tol
    });
    on_circle.then_some(ArcCentre { centre, normal, radius, closed, edges })
}
