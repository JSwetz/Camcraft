//! Law (d) for `Mode::Radial` (#79): a cross-drill rapid's tool body against the raw stock solid, minus the kerf the program's own cuts swept before it - the shop practice of traversing only outside the stock (machine-tool handbooks), read as a solid instead of a height field; our own design, no vendor material consulted.
//!
//! The column law ([`super::rapids`]) reads a rapid as a vertical tool
//! over a height field, and a horizontal bore is a tunnel no height
//! field can see - so for radial programs it stood down (M5.2 F2) and
//! trusted the kernel's traverse law. That left the post gate blind:
//! a station change through uncut stock (#77) posted with no warning.
//! This reading needs no column:
//!
//! - The stock is the prism the verifier already holds - the stock
//!   footprint (`cx.stock`, or the region standing in for it, exactly
//!   as [`Rest::in_stock`] reads it) under the stock top, unbounded
//!   below (a chuck-held bar's foot is in the jaws: reading it as
//!   stock only adds convictions). A turned bar or a box about the
//!   spindle is a Z prism, so the footprint IS the solid measured
//!   about the spindle, wherever the work offset puts it.
//! - The tool is its flute body: discs of radius `r - gouge_tol`
//!   (the mill law's disc, [`Rest::hidden_along`]) square to the pose,
//!   at the tip and every [`BODY_STEP`] back along the pose up to the
//!   flute length, under the controller's modal pose rule
//!   ([`super::midpose::intervals`], [`STEP`] of travel at the far end
//!   between samples).
//! - The kerf is what the feeds BEFORE the rapid swept (cut order, as
//!   M13.7 reads the column): a cylinder of the tool radius about each
//!   cutting move, extended back along the pose by the flute length
//!   when the move plunges along its own axis (the body followed the
//!   tip in). A retract up a just-drilled hole - and the peck's chip
//!   break - stands wholly in that kerf, so it is acquitted by the
//!   stock reading, never by the path's description of itself.
//!
//! A sample of the body in stock and in no earlier kerf convicts the
//! rapid: [`Warning::RapidInStock`], the mill law's own blocking word.
//! Sampled, so bounded rather than exact: a sliver of stock thinner
//! than the sample spacing between two discs can slip through; the
//! spacing is the posed laws' own (0.5 mm of travel).

use super::midpose;
use super::rest::Rest;
use super::types::{VerifyContext, Warning};

use crate::cl::{normalize_axis, MoveKind, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// Axial spacing of the body discs behind the tip, mm.
const BODY_STEP: f64 = 1.0;
/// Rim samples of the outer disc ring (half as many on the inner).
const RIM: usize = 16;
/// Most body discs one sample carries.
const MAX_BODY: usize = 256;

/// One earlier cut's kerf: the tool-radius cylinder about `a..b`, and
/// the index of the move that cut it.
struct Kerf {
    at: usize,
    a: Point3,
    b: Point3,
}

/// Law (d) for radial programs: one [`Warning::RapidInStock`] per rapid
/// whose flute body enters stock no earlier cut removed.
pub(super) fn radial_rapids(path: &Toolpath, cx: &VerifyContext, rest: &Rest, warns: &mut Vec<Warning>) {
    let r = cx.tool.radius;
    let flute = cx.tool.flute_len;
    if !(r.is_finite() && r > 0.0 && flute.is_finite()) {
        return; // the reach laws refuse such a tool by name
    }
    let flute = flute.max(0.0);
    let disc = (r - cx.gouge_tol.max(0.0)).max(0.5 * r);
    let poses = midpose::poses(path);
    let up = [0.0, 0.0, 1.0];
    let unit = |p: midpose::Pose| p.map_or(Some(up), normalize_axis).unwrap_or(up);
    let kerfs: Vec<Kerf> = path
        .moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.kind != MoveKind::Rapid && m.from.is_finite() && m.to.is_finite())
        .flat_map(|(j, m)| {
            let a = poses.get(j).map_or(up, |p| unit(p.1));
            let back = if along_axis(m.from, m.to, a) { flute } else { 0.0 };
            let tail = Point3::new(m.from.x + a[0] * back, m.from.y + a[1] * back, m.from.z + a[2] * back);
            m.segments3(cx.gouge_tol.max(1e-3)).into_iter().map(move |(p, q)| Kerf {
                at: j,
                a: q,
                b: if (p.x, p.y, p.z) == (m.from.x, m.from.y, m.from.z) { tail } else { p },
            })
        })
        .collect();
    let ring = rings(disc);
    let k = ((flute / BODY_STEP).ceil() as usize).clamp(0, MAX_BODY);
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind != MoveKind::Rapid || !(m.from.is_finite() && m.to.is_finite()) {
            continue; // non-finite moves are NonFinite's word
        }
        let (a0, a1) = poses.get(i).map_or((up, up), |p| (unit(p.0), unit(p.1)));
        let earlier: Vec<&Kerf> = kerfs.iter().filter(|c| c.at < i).collect();
        let bad = midpose::intervals(m, a0, a1, flute, cx.gouge_tol)
            .into_iter()
            .flat_map(|iv| [(iv.p0, iv.a0), (iv.pm, iv.am), (iv.p1, iv.a1)])
            .any(|(tip, a)| {
                let (u, v) = basis(a);
                (0..=k).any(|s| {
                    let h = if k == 0 { 0.0 } else { flute * s as f64 / k as f64 };
                    let c = Point3::new(tip.x + a[0] * h, tip.y + a[1] * h, tip.z + a[2] * h);
                    ring.iter().any(|&(du, dv)| {
                        let p = Point3::new(
                            c.x + u[0] * du + v[0] * dv,
                            c.y + u[1] * du + v[1] * dv,
                            c.z + u[2] * du + v[2] * dv,
                        );
                        in_stock(cx, rest, p) && !earlier.iter().any(|kf| in_kerf(p, kf, r, cx.z_tol))
                    })
                })
            });
        if bad {
            warns.push(Warning::RapidInStock { at: i });
        }
    }
}

/// Whether `p` stands in the raw stock prism: inside the footprint,
/// under the top.
fn in_stock(cx: &VerifyContext, rest: &Rest, p: Point3) -> bool {
    p.z < cx.stock_top_z - cx.z_tol && rest.in_stock(Point2::new(p.x, p.y))
}

/// Whether `p` is inside the tool-radius cylinder about the kerf's
/// segment (flat ends, `tol` of slack along it).
fn in_kerf(p: Point3, kf: &Kerf, r: f64, tol: f64) -> bool {
    let d = [kf.b.x - kf.a.x, kf.b.y - kf.a.y, kf.b.z - kf.a.z];
    let w = [p.x - kf.a.x, p.y - kf.a.y, p.z - kf.a.z];
    let len2 = dot(d, d);
    if !(len2.is_finite() && len2 > 1e-18) {
        return dot(w, w) <= r * r;
    }
    let t = dot(w, d) / len2;
    let slack = tol / len2.sqrt();
    if !(-slack..=1.0 + slack).contains(&t) {
        return false;
    }
    let t = t.clamp(0.0, 1.0);
    let e = [w[0] - d[0] * t, w[1] - d[1] * t, w[2] - d[2] * t];
    dot(e, e) <= r * r
}

/// Whether the move from `p` to `q` runs along axis `a` (either way).
fn along_axis(p: Point3, q: Point3, a: [f64; 3]) -> bool {
    let d = [q.x - p.x, q.y - p.y, q.z - p.z];
    let l2 = dot(d, d);
    if !(l2.is_finite() && l2 > 1e-18) {
        return true; // no travel: the tip's own disc
    }
    let c = dot(d, a);
    l2 - c * c <= 1e-12 * l2.max(1.0)
}

/// The disc's sample offsets in its own (u, v) plane: the centre, an
/// inner ring at half radius, the rim.
fn rings(r: f64) -> Vec<(f64, f64)> {
    let mut out = vec![(0.0, 0.0)];
    for (n, rad) in [(RIM / 2, 0.5 * r), (RIM, r)] {
        for j in 0..n {
            let t = std::f64::consts::TAU * j as f64 / n as f64;
            out.push((rad * t.cos(), rad * t.sin()));
        }
    }
    out
}

/// Two unit vectors square to unit `a` and to each other.
fn basis(a: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let h = if a[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = norm(cross(a, h));
    (u, cross(a, u))
}

fn dot(p: [f64; 3], q: [f64; 3]) -> f64 {
    p[0] * q[0] + p[1] * q[1] + p[2] * q[2]
}

fn cross(p: [f64; 3], q: [f64; 3]) -> [f64; 3] {
    [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]]
}

fn norm(p: [f64; 3]) -> [f64; 3] {
    let l = dot(p, p).sqrt();
    if l > 0.0 && l.is_finite() { [p[0] / l, p[1] / l, p[2] / l] } else { [1.0, 0.0, 0.0] }
}
