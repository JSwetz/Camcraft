//! Collision-aware orientation (M5.1 stage F): the spindle dodges the
//! part WITHOUT moving the path. The whole kind rides law (f) - a
//! sphere's envelope is rotation-invariant about its center - so the
//! axis at a CL point is free while the cut it makes is fixed: leaning
//! the spindle cannot gouge a micron the vertical pose did not already
//! own. What stage F adds to stage C's pose law is the second opinion:
//! where the commanded pose would drive the SHANK into material, the
//! spindle tilts to the nearest clear angle inside the max-tilt cone,
//! and where no angle inside the cone clears, the path refuses by name
//! instead of shipping a warning post-time would have to catch.
//!
//! The collision test mirrors `camcraft_sim::verify`'s law (c) exactly
//! where it can: each shank band's swept solid is enclosed in a
//! vertical column - shifted to the band's mid-height point on the
//! leaning axis, widened by the band's half-length spread, bottomed at
//! the lower axis end minus the band radius - and the annulus about
//! the column center inside the cutting radius is the flute's own
//! kerf, excused. The motion side tests the CONTINUOUS facets (the
//! verifier samples them), so a graze may be over-reported here and
//! stay clean there: over-report a graze, never hide a gouge. The
//! verifier remains the law - this module only ever changes poses,
//! and a pose F escapes to is still judged by `verify` at post time.
//!
//! Provenance: tool-axis tilt optimization against shank collision is
//! published 5-axis practice (the sphere-envelope freedom is the
//! geometric reason it is exact); the enclosure is this codebase's
//! own law (c) - including its upright case, where the column floor
//! stands AT the band's lower end (the floor being cut is the
//! flute's kerf, never a graze) and the discount appears only for a
//! leaning band, whose swept rim dips below the axis-plane end. No
//! vendor material consulted.

use crate::geometry::{Point3, Triangle};

/// The kerf margin: material within the cutting radius plus this of
/// the band axis is the surface the tool itself owns, tilt or no tilt.
const EXC: f64 = 1e-6;

/// The upright test, as strict as the `camcraft_cl::is_vertical` the
/// verifier branches on: a pose stated as exactly vertical keeps its
/// band's floor at the band's own bottom end.
const VERT_EPS: f64 = 1e-12;

/// The tool above the ball, in the stack `verify`'s law (c) reads:
/// a shank standing on the flute, a holder on the shank. Lengths
/// stack upward from the tool center; radii are the physical bands'
/// (the flute's own body is the ball's, not a band).
#[derive(Clone, Copy, Debug)]
pub struct Shank {
    pub flute_len: f64,
    pub shank_r: f64,
    pub shank_len: f64,
    pub holder_r: f64,
    pub holder_len: f64,
}

impl Shank {
    /// The two bands as `(above, radius, length)`, zeroed where they do
    /// not exist - not wider than the cutter plus the kerf margin, or
    /// of no length. Same filter law (c) applies.
    fn bands(&self, r: f64) -> [(f64, f64, f64); 2] {
        let mut b = [
            (self.flute_len, self.shank_r, self.shank_len),
            (self.flute_len + self.shank_len, self.holder_r, self.holder_len),
        ];
        for e in &mut b {
            if e.1 <= r + EXC || e.2 <= 0.0 {
                *e = (0.0, 0.0, 0.0);
            }
        }
        b
    }
}

/// Does the shank at `p` posing `a` (unit, tip-to-spindle) touch
/// material? Facets come with their AABB shells; the box gate carries
/// both bands' worst-case reach, so the exact test only runs where a
/// box says the column could even stand.
pub(crate) fn shank_hits(
    p: Point3,
    a: [f64; 3],
    r: f64,
    s: &Shank,
    facets: &[Triangle],
    boxes: &[[f64; 6]],
) -> bool {
    for &(above, br, bl) in &s.bands(r) {
        if bl <= 0.0 {
            continue;
        }
        let mid = above + bl * 0.5;
        let axy = f64::hypot(a[0], a[1]);
        let reach = br + bl * 0.5 * axy + 1e-9;
        let (cx, cy) = (p.x + mid * a[0], p.y + mid * a[1]);
        // The column floor mirrors law (c) exactly: an upright band
        // starts AT its lower end - the floor being cut stands below
        // that plane and is the flute's own kerf, never a graze - and
        // only a LEANING band pays the disc-radius discount, because
        // only its swept rim dips below the axis-plane end.
        let ends = (p.z + above * a[2]).min(p.z + (above + bl) * a[2]);
        let h0 = ends - if axy <= VERT_EPS { 0.0 } else { br };
        for (f, bx) in facets.iter().zip(boxes) {
            // The box must REACH ABOVE the column floor, as law (c)'s
            // `h0 < surface(q)` demands: a facet standing no higher
            // than the floor is below the band's own feet, not a
            // graze. The strict form keeps the floor being cut from
            // ever reporting itself.
            if bx[5] <= h0
                || bx[3] < cx - reach
                || bx[0] > cx + reach
                || bx[4] < cy - reach
                || bx[1] > cy + reach
            {
                continue;
            }
            if face_in_column(f, cx, cy, reach, r + EXC) {
                return true;
            }
        }
    }
    false
}

/// The continuous form of the column test on one triangle: some point
/// of it stands at column distance AND outside the kerf AND at or
/// above the column floor. The floor rides the box gate (a box fully
/// below h0 never reaches here); the XY part is exact because distance
/// to a fixed point is convex - its image over the triangle is the
/// interval `[dmin, dmax]` with `dmax` at a vertex - and a band always
/// reaches past the kerf (`reach > r + EXC`, the band filter).
fn face_in_column(f: &Triangle, cx: f64, cy: f64, reach: f64, kerf: f64) -> bool {
    let (da, db, dc) = (dxy(f.a, cx, cy), dxy(f.b, cx, cy), dxy(f.c, cx, cy));
    if da.max(db.max(dc)) <= kerf {
        return false; // the whole face stands in the flute's own kerf
    }
    dist_tri_2d(cx, cy, da, db, dc, f) <= reach
}

/// XY distance from a point to a triangle's projection.
fn dist_tri_2d(cx: f64, cy: f64, da: f64, db: f64, dc: f64, f: &Triangle) -> f64 {
    if da == 0.0 || db == 0.0 || dc == 0.0 || inside_2d(cx, cy, f) {
        return 0.0;
    }
    seg_dist(cx, cy, f.a, f.b)
        .min(seg_dist(cx, cy, f.b, f.c))
        .min(seg_dist(cx, cy, f.c, f.a))
        .min(da.min(db.min(dc)))
}

fn dxy(q: Point3, cx: f64, cy: f64) -> f64 {
    f64::hypot(q.x - cx, q.y - cy)
}

/// Same-side-of-all-edges test, in XY only.
fn inside_2d(cx: f64, cy: f64, f: &Triangle) -> bool {
    let cr = |a: Point3, b: Point3| (b.x - a.x) * (cy - a.y) - (b.y - a.y) * (cx - a.x);
    let (r1, r2, r3) = (
        cr(f.a, f.b),
        cr(f.b, f.c),
        cr(f.c, f.a),
    );
    (r1 >= 0.0 && r2 >= 0.0 && r3 >= 0.0) || (r1 <= 0.0 && r2 <= 0.0 && r3 <= 0.0)
}

fn seg_dist(cx: f64, cy: f64, a: Point3, b: Point3) -> f64 {
    let (ex, ey) = (b.x - a.x, b.y - a.y);
    let ee = ex * ex + ey * ey;
    let t = if ee > 0.0 {
        (((cx - a.x) * ex + (cy - a.y) * ey) / ee).clamp(0.0, 1.0)
    } else {
        0.0
    };
    f64::hypot(cx - a.x - t * ex, cy - a.y - t * ey)
}

/// The angle between two unit directions, degrees.
fn ang(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
    d.acos().to_degrees()
}

/// Tilt off `n` within the cone until the shank clears, choosing the
/// pose that rotates the spindle least from the `prev` one (the
/// shortest-travel law: between two cuts the A/C head moves as little
/// as any clear pose allows - which is also what keeps an escape from
/// oscillating between neighboring moves). The cone is the one
/// `command_axis` clamps into: half-angle around the contact normal.
/// Returns `None` when nothing inside the cone clears - the caller
/// refuses by name, never posts a path whose shank is already known
/// to touch.
#[allow(clippy::too_many_arguments)] // flat geometry args, like shank_hits
pub(crate) fn escape_axis(
    p: Point3,
    a0: [f64; 3],
    n: Point3,
    prev: [f64; 3],
    cone_deg: f64,
    r: f64,
    s: &Shank,
    facets: &[Triangle],
    boxes: &[[f64; 6]],
) -> Option<[f64; 3]> {
    // Orthonormal frame around the cone's axis n.
    let dn = a0[0] * n.x + a0[1] * n.y + a0[2] * n.z;
    let mut u = Point3::new(a0[0] - n.x * dn, a0[1] - n.y * dn, a0[2] - n.z * dn);
    let ul = u.dot(u).sqrt();
    if ul <= 1e-12 {
        // The pose stands ON the normal: its azimuth is whatever
        // leaves a horizontal frame - z's answer first, x's when n is z.
        u = if n.z.abs() < 0.999 {
            let w = Point3::new(0.0, 0.0, 1.0);
            let c = w.cross(n);
            let cl = c.dot(c).sqrt();
            Point3::new(c.x / cl, c.y / cl, c.z / cl)
        } else {
            Point3::new(1.0, 0.0, 0.0)
        };
    } else {
        u = Point3::new(u.x / ul, u.y / ul, u.z / ul);
    }
    let v = n.cross(u);
    let th0 = ang(a0, [n.x, n.y, n.z]);
    let cap = cone_deg.max(th0).min(85.0);
    // The candidate net: the tilt grid spans the cone's edge and the
    // nominal's own latitude; the azimuths are eight spokes, so an
    // away-from-the-collision direction is never more than 22.5 deg
    // from one. Nominal itself is not a candidate - it is what hit.
    let mut cands: Vec<([f64; 3], f64, f64)> = Vec::new();
    let thetas = [0.0, cap, cap * 0.5, cap * 0.25, cap * 0.75, th0, cap / 3.0, cap * 2.0 / 3.0];
    for th in thetas {
        let (ct, st) = (th.to_radians().cos(), th.to_radians().sin());
        for m in 0..8 {
            let ph = m as f64 * 45.0f64.to_radians();
            let (cp, sp) = (ph.cos(), ph.sin());
            let a = [
                n.x * ct + st * (u.x * cp + v.x * sp),
                n.y * ct + st * (u.y * cp + v.y * sp),
                n.z * ct + st * (u.z * cp + v.z * sp),
            ];
            let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            let a = [a[0] / l, a[1] / l, a[2] / l];
            if ang(a, a0) <= 1e-6 || ang(a, [n.x, n.y, n.z]) > cap + 1e-6 {
                continue;
            }
            cands.push((a, ang(a, prev), ang(a, a0)));
        }
    }
    // Shortest axis travel first; the nominal's own lean breaks ties.
    cands.sort_by(|x, y| x.1.total_cmp(&y.1).then(x.2.total_cmp(&y.2)));
    cands.dedup_by(|x, y| ang(x.0, y.0) <= 1e-9);
    cands
        .into_iter()
        .map(|(a, _, _)| a)
        .find(|&a| !shank_hits(p, a, r, s, facets, boxes))
}
