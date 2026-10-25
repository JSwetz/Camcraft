//! The rotation-axis finder of the rotary kinds (M5.1 stage E). A
//! turned part is a SURFACE OF REVOLUTION, and the classical law for
//! one is written by its normals: every surface normal line meets the
//! axis (on a cylinder it meets it squarely, on a cone or a bowl
//! obliquely). So the axis is the line that best meets all the
//! normal lines of the mesh - a least-squares concurrency fit:
//! direction from the extremal eigenvector of the area-weighted
//! second moment of the normals, position from the linear equations
//! `(n x u) . o = (n x u) . p` each normal line gives, pinned to the
//! canonical `o . u = 0` foot of the axis.
//!
//! Each facet's line is its plane normal anchored at the triangle's
//! CIRCUMCENTER, and that anchor is the whole craft of this: a
//! facet of a turned mesh has two vertices on one ring, so its
//! circumcenter - equidistant of all three - sits on the ring
//! chord's perpendicular bisector, which is a plane through the
//! true axis. The facet's normal is perpendicular to that chord
//! too, so the line stands in the meridian half-plane of its own
//! bearing: on a true revolve it meets the axis EXACTLY, at any
//! tessellation. Anchor the same normal at the centroid instead and
//! the line misses by a sixth of the arc first-order; weld the
//! mesh and average normals at a shared vertex and the triangle
//! fan's one-diagonal asymmetry misses by a quarter of it - both
//! refuse well-turned parts. What circumcenter anchoring cannot
//! see is flatness masquerading as roundness, so coarseness gets
//! its own honest gate: a turned zone's facets sweep their bearings
//! around the axis; a facade whose widest bearing skip is g stands
//! in for a wall whose chords sag `rho * g^2 / 8` - the skip must
//! stay inside the sagitta budget. Facing flats bear no azimuth
//! and justly take no part: shoulders are sharp where lathes leave
//! them so. Miss concurrency everywhere and the model is not a
//! surface of revolution, and the refusal says which kind wants it
//! instead. A sphere answers EVERY axis equally well (its normals
//! are concurrent at a point) and is refused as what it is: domed
//! ground for the 5-axis kinds of stage C.
//!
//! Provenance: axis-of-revolution detection by normal-line
//! concurrency is standard point-cloud geometry (the published
//! survey of revolved-surface fitting); the least-squares line fit
//! from plucker-like rows is textbook. Written from those sources
//! and our own design notes - nothing of any vendor's consulted.

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::linalg::{solve3, sym_eigen, unit};

/// The fitted axis: a canonical point (`o . u = 0` from the mesh's
/// own coordinates) and a unit direction. The generators rotate the
/// work about this line into their local frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub origin: Point3,
    pub dir: Point3,
}

/// Fraction of the answering (non-pole) facet area whose normal line
/// must meet the fitted axis within the tolerance for the axis to be
/// claimed at all.
const OK_SHARE: f64 = 0.9;
/// The concurrency tolerance, as a fraction of the part's mean radius
/// about the candidate - the sagitta budget, applied both to the
/// residual (a fine mesh's vertices are the true surface's) and to
/// the azimuthal skip that measures the same faceting from below.
const REL_TOL: f64 = 0.03;
/// Eigen ratios that say "every direction fits equally" - a sphere's
/// normals are concurrent at a POINT, so no axis claims the fit over
/// any other and both candidate axes would pass the concurrency test.
const ISO: f64 = 0.5;

/// One facet's claim: its plane normal, the circumcenter it belongs
/// at, and its area.
struct Datum {
    n: Point3,
    p: Point3,
    a: f64,
}

/// Read the rotation axis off the mesh, or refuse by name with the
/// kind that speaks for this geometry.
pub fn fit(facets: &[Triangle]) -> Result<Axis, GenError> {
    let mut data: Vec<Datum> = Vec::with_capacity(facets.len());
    let mut a_total = 0.0;
    for f in facets {
        let n = f.normal();
        if !n.is_finite() {
            continue;
        }
        let (u, v) = (f.b - f.a, f.c - f.a);
        let cr = u.cross(v);
        let area = 0.5 * cr.dot(cr).sqrt();
        if !area.is_finite() || area <= 0.0 {
            continue; // degenerate facet: no claim, no vote
        }
        let Some(p) = circumcenter(f) else {
            continue; // a sliver's circumcenter runs off to nowhere;
        }; // its good neighbors' areas carry the vote anyway
        data.push(Datum { n, p, a: area });
        a_total += area;
    }
    if data.is_empty() || a_total <= 0.0 {
        return Err(GenError::Invalid(
            "rotary has no surface to read a rotation axis from".into(),
        ));
    }
    // The second moment of the normals, area-weighted: symmetric,
    // stored in its six independent entries.
    let mut m = [[0.0f64; 3]; 3];
    for d in &data {
        for (i, ni) in [d.n.x, d.n.y, d.n.z].iter().enumerate() {
            for (j, nj) in [d.n.x, d.n.y, d.n.z].iter().enumerate() {
                m[i][j] += d.a * ni * nj;
            }
        }
    }
    let Some((vals, vecs)) = sym_eigen(m) else {
        return Err(GenError::Invalid(
            "the model's normals are not finite - the mesh is corrupt".into(),
        ));
    };
    if vals[2] <= 0.0 || !vals[2].is_finite() {
        return Err(GenError::Invalid(
            "rotary found no normals to turn about - this model has no surface".into(),
        ));
    }
    if vals[0] > ISO * vals[2] && vals[1] > ISO * vals[2] {
        return Err(GenError::Invalid(
            "every axis fits this model equally - it curves the same the world around; domed ground is 5-Axis Contour or Parallel work".into(),
        ));
    }
    // Two candidates: a surface of revolution's normals concentrate
    // their second moment in the plane perpendicular to the axis
    // (cylindrical zones: the axis is the LEAST eigendirection), while
    // steeply-sloped revolves (cones past 45 degrees) can tip past it -
    // so both extremes are tried and the concurrency residual decides.
    let cands = [
        Point3::new(vecs[0][0], vecs[1][0], vecs[2][0]),
        Point3::new(vecs[0][2], vecs[1][2], vecs[2][2]),
    ];
    let mut best: Option<(f64, Axis)> = None;
    for c in cands {
        if let Some(sol) = try_axis(&data, c, a_total)
            .filter(|s| best.as_ref().is_none_or(|(r, _)| s.0 < *r))
        {
            best = Some(sol);
        }
    }
    match best {
        Some((_, ax)) => Ok(ax),
        None => Err(GenError::Invalid(
            "the model's normals share no rotation axis - a turned workpiece is rotary work; finish a freeform with the 5-Axis kinds".into(),
        )),
    }
}

/// Fit the axis LINE about the candidate direction `u`; `None` when
/// the normals do not answer the candidate at all (no constraining
/// rows, an unrecoverable solve, a concurrency residual that says
/// this is not a surface of revolution about any line in this
/// direction, or a bearing skip too wide to call the walls round).
/// Returns (root-mean-square residual, the axis) for ranking.
fn try_axis(data: &[Datum], u0: Point3, a_total: f64) -> Option<(f64, Axis)> {
    let u = unit(u0)?;
    // Determinately signed: the axis points up, or (for a flat-lying
    // fit) along +x, or +y - the generators read stations along the
    // direction, so its sign is part of the answer.
    let u = if u.z < 0.0 || (u.z == 0.0 && (u.x < 0.0 || (u.x == 0.0 && u.y < 0.0))) {
        Point3::new(-u.x, -u.y, -u.z)
    } else {
        u
    };
    let mut m = [[0.0f64; 3]; 3];
    let mut b = [0.0f64; 3];
    let mut rows = 0usize;
    let mut area_ok = 0.0;
    let mut area_ans = 0.0;
    let mut r_wsum = 0.0;
    for d in data {
        let k = d.n.cross(u);
        let kn2 = k.dot(k);
        if kn2 <= 1e-8 {
            continue; // a pole facet's normal runs along the axis: no row
        }
        let kn = kn2.sqrt();
        // The equation k.hat . o = k.hat . p, area-weighted: the
        // normal equations of minimizing sum(a_i * dist_i^2).
        let (kx, ky, kz) = (k.x / kn, k.y / kn, k.z / kn);
        let kp = kx * d.p.x + ky * d.p.y + kz * d.p.z;
        for (i, kv) in [kx, ky, kz].iter().enumerate() {
            for (j, ku) in [kx, ky, kz].iter().enumerate() {
                m[i][j] += d.a * kv * ku;
            }
            b[i] += d.a * kv * kp;
        }
        rows += 1;
        let s = d.p.dot(u);
        let cp = Point3::new(d.p.x - u.x * s, d.p.y - u.y * s, d.p.z - u.z * s);
        let rr = cp.dot(cp).sqrt();
        if rr.is_finite() {
            r_wsum += rr * d.a;
        }
        area_ans += d.a;
    }
    if rows < 2 {
        return None; // one azimuth says nothing of where the axis stands
    }
    // The canonical anchor: the rows are blind along u, so pin o to
    // the foot of the axis on the plane through the origin
    // perpendicular to it. One small row that only chooses WHERE on
    // the line o sits - it cannot bend the residuals.
    let wp = 1e-3 * (a_total / rows as f64).max(1.0);
    for (i, uv) in [u.x, u.y, u.z].iter().enumerate() {
        for (j, ku) in [u.x, u.y, u.z].iter().enumerate() {
            m[i][j] += wp * uv * ku;
        }
        b[i] += 0.0; // rhs 0: o . u = 0
    }
    // Area-weighted mean radius about u sets the scale of the
    // sagitta budget the mesh must meet.
    if area_ans <= 0.0 || r_wsum <= 0.0 {
        return None;
    }
    let rho = r_wsum / area_ans;
    let o = solve3(m, b)?;
    // A well-centered revolve's rows cancel to machine noise at the
    // origin, and the noise is a rounding of the solve, not a fact
    // about the part - the LINE is what carries the law. Drown the
    // noise floor so an upright fit rides the identity frame bit for
    // bit, the way the golden paths demand.
    let nz = 32.0 * f64::EPSILON * rho;
    let o = Point3::new(
        if o.x.abs() <= nz { 0.0 } else { o.x },
        if o.y.abs() <= nz { 0.0 } else { o.y },
        if o.z.abs() <= nz { 0.0 } else { o.z },
    );
    let tol = (REL_TOL * rho).max(1e-9);
    let mut sq = 0.0;
    for d in data {
        let k = d.n.cross(u);
        let kn = k.dot(k).sqrt();
        if kn <= 1e-4 {
            continue;
        }
        let dist = (k.x * (o.x - d.p.x) + k.y * (o.y - d.p.y) + k.z * (o.z - d.p.z)).abs() / kn;
        if !dist.is_finite() {
            return None;
        }
        if dist <= tol {
            area_ok += d.a;
        }
        sq += d.a * dist * dist;
    }
    if area_ans <= 0.0 || area_ok < OK_SHARE * area_ans {
        return None;
    }
    // The coarseness gate: the widest empty skip between facet
    // bearings about u. sqrt(8 * REL_TOL) radians is exactly the
    // chord-sagitta budget written in angle - a turn coarse enough
    // to skip further is a polygon pretending to be a wall.
    let any = if u.z.abs() < 0.9 {
        Point3::new(0.0, 0.0, 1.0)
    } else {
        Point3::new(1.0, 0.0, 0.0)
    };
    let e1 = unit(u.cross(any))?;
    let e2 = u.cross(e1);
    let mut angs: Vec<f64> = Vec::new();
    for d in data {
        let s = d.n.dot(u);
        let p = Point3::new(d.n.x - u.x * s, d.n.y - u.y * s, d.n.z - u.z * s);
        if p.dot(p) <= 1e-4 {
            continue; // pole facet: bears no azimuth
        }
        angs.push(p.dot(e2).atan2(p.dot(e1)).rem_euclid(std::f64::consts::TAU));
    }
    if angs.len() <= 2 {
        return None; // too few bearings to call anything round
    }
    angs.sort_by(|x, y| x.total_cmp(y));
    let mut gap = angs[1] - angs[0];
    for w in angs.windows(2).skip(1) {
        gap = gap.max(w[1] - w[0]);
    }
    gap = gap.max(std::f64::consts::TAU - angs[angs.len() - 1] + angs[0]);
    if gap > (8.0 * REL_TOL).sqrt() {
        return None;
    }
    Some(((sq / a_total).sqrt(), Axis { origin: o, dir: u }))
}

/// The triangle's circumcenter, in its own plane; `None` when the
/// three vertices are too collinear for a center to mean anything.
fn circumcenter(t: &Triangle) -> Option<Point3> {
    let (u, v) = (t.b - t.a, t.c - t.a);
    let (b1, b2, c1) = (u.dot(u), v.dot(v), u.dot(v));
    let den = b1 * b2 - c1 * c1;
    if !den.is_finite() || den <= 1e-9 * (b1 * b2).max(1e-30) {
        return None;
    }
    let al = b2 * (b1 - c1) / (2.0 * den);
    let be = b1 * (b2 - c1) / (2.0 * den);
    let p = Point3::new(
        t.a.x + u.x * al + v.x * be,
        t.a.y + u.y * al + v.y * be,
        t.a.z + u.z * al + v.z * be,
    );
    p.is_finite().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Triangle;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn a_facet_of_a_turned_wall_centers_on_its_bearing() {
        // Two ring points and the wall point above their chord: the
        // circumcenter stands IN the chord's mid-bearing half-plane,
        // and the normal line through it meets the axis exactly -
        // the geometric fact the whole concurrency law rests on.
        let (r, d) = (2.0f64, 20f64.to_radians());
        let t = Triangle::new(
            p(r * (-d / 2.0).cos(), r * (-d / 2.0).sin(), 0.0),
            p(r * (d / 2.0).cos(), r * (d / 2.0).sin(), 0.0),
            p(r, 0.0, 5.0),
        );
        let c = circumcenter(&t).unwrap();
        assert!((c.y).abs() < 1e-9, "{c:?} off the mid bearing");
        let n = t.normal();
        let k = n.cross(Point3::new(0.0, 0.0, 1.0));
        let dist = (k.x * c.x + k.y * c.y + k.z * c.z).abs() / k.dot(k).sqrt();
        assert!(dist < 1e-9, "the normal line missed the axis by {dist}");
    }

    #[test]
    fn an_empty_mesh_refuses_by_name() {
        let e = fit(&[]).err().unwrap();
        assert!(e.to_string().contains("no surface"), "{e}");
    }
}
