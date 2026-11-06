//! Rotary offset roughing (M5.1 stage E2): the pocket ladder run in
//! WRAP SPACE - the developed surface of the fitted axis [`super::axis`]
//! - so a turned work is roughed by the machinery that already knows
//!   how to erode regions, dodge islands and enter levels, applied to
//!   a cylinder instead of a table.
//!
//! The map: every vertex keeps its axial station `v = (p - o)·û` and
//! its radius `ρ = |(p - o) ⊥ û|`, and its azimuth becomes the
//! developed coordinate `s = R_ref·θ` (θ measured about the `e1/e2`
//! basis [`axis`] fits; R_ref the work's largest radius, so `s` is a
//! true arc length on the skin). A facet with one vertex ON the axis
//! closes the lathe onto it (a pole, or the fan across a flat cap)
//! and sweeps a full developed RECTANGLE between its two rim
//! vertices' columns; every other facet develops as itself, and an
//! axis vertex never lands on an affine blend of a meaningless
//! azimuth - which is what lets a fan of wedges tile one closed
//! chain of chords per level instead of opening on every spoke.
//! Each facet also picks its own lap: a plain one when its columns
//! fit within half a turn, a seam one (one column riding exactly
//! 2π low) when they straddle the ±π branch, so the same mesh vertex
//! keeps one developed coordinate in every facet that shares it.
//! The mesh is then duplicated one full period (+2πR_ref in `s`) so
//! every chord chain runs into its own copy at the seam, and the
//! meridian's cross-section stands as a WALL at each end of the
//! cover, closing the chain's two far ends. The wrap is a
//! closed solid exactly when the model was: a revolve's ring slices
//! to a closed contour at every level the way block work expects,
//! and a pass running most of a lap rewraps to a closed ring
//! instead of leaving a rest ridge at the seam. Levels in AIR - above
//! the part's skin, over a fatter bar - meet no facet and need no
//! copy: they clear one lap plus the seam pad (M13.8).
//!
//! With ρ as height, `pocket3d`'s island law reads exactly right:
//! part material stands above the current radius wherever the skin
//! reaches further out than the level, so a level's target is
//! precisely the columns already turned inside that radius, the
//! ladder peels ρ_max → ρ_min (ρ_max the STOCK's outer radius when
//! the caller hands one over a fatter bar - M13.8, where the levels
//! above the part's skin are the whole lap), and the ledge under a column's last
//! legal level is the finishing kinds' job - the same staircase law,
//! one seam over. End caps ride the wedge law too: their rectangle
//! stands at the cap's station between the axis and the rim, a real
//! chord closing every slice it spans.
//!
//! The world exit is [`wrap_exit`]: ball centers back at ρ plus one
//! tool radius with the tip posted under them (M13 pivot law), arcs
//! chorded, s-sweeps resampled, rapids and cut moves standing
//! RADIAL with the lean of [`super::tilt`], as in [`super::rotary`].
//! Unlike the twins there is no LEAN refusal: the developed cylinder
//! is neutral to which way the axis points - a bar may stand or lie
//! and the rough ladder runs the same. The caller's world
//! `top_z`/`bottom_z` window the STATIONS (the contour kind's own
//! `(h - o.z)/u.z` law) and are silently inert on a bar lying flat,
//! where no world plane names a station.
//!
//! Provenance: cylindrical/unwrapped toolpath generation and radial
//! infeed roughing from public CAM literature and turning practice;
//! composed on this codebase's own pocket, axis-fit and lean laws.
//! Nothing of any vendor's consulted.

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::machining3d::{self, Pocket3DParams};

use super::axis;
use super::tilt::AxisControl;
use super::wrap_exit;

/// World up, spelled once (Point3::new is not a const fn).
const UP: Point3 = Point3 {
    x: 0.0,
    y: 0.0,
    z: 1.0,
};
/// The axis' up-component below which world heights cannot name axial
/// stations (a plane nearly parallel to the axis meets it, if at all,
/// a million stations away): the twins' own sin-1 degree boundary.
const LEAN_MIN: f64 = 0.0175;
/// One full lap of development, in radians.
const TAU: f64 = std::f64::consts::TAU;

/// The rotary offset rough: axis fit, wrap map, the pocket ladder in
/// developed space, the radial-pose world exit.
///
/// `p` carries the pocket's own numbers (diameter, stepover, leave,
/// stepdown as the RADIAL infeed, feeds, strategy, entry law); its
/// height fields are read differently here: `top_z`/`bottom_z` window
/// the stations (above), the ρ ladder - `top_z = rho_max`,
/// `bottom_z = rho_min` - is computed from the mesh, and the rapids
/// plane sits one infeed and one tool radius outside the skin.
/// With no stock to read the part's own skin is the top: see
/// [`rotary_offset_in`].
pub fn rotary_offset(
    facets: &[Triangle],
    p: &Pocket3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    rotary_offset_in(facets, None, p, c)
}

/// The rotary offset rough over its setup's `stock` (M13.8): the ρ
/// ladder's top, its rapids cylinder and the development's arc-length
/// radius are the STOCK's outer radius about the fitted axis (its
/// farthest vertex - a round bar's skin, a block's corner), never less
/// than the part's - a ladder peeling from the part's skin inside a
/// fatter bar hops through skin it never cut. The stepover then holds
/// as an arc length on the outermost level and only tightens inward.
pub fn rotary_offset_in(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    p: &Pocket3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    if facets.is_empty() {
        return Err(GenError::Invalid("no facets to wrap".into()));
    }
    let ax = axis::fit(facets)?;
    let u = ax.dir;
    // The axis.rs basis law, verbatim: any vector off the axis, then
    // the right-handed pair it completes.
    let any = if u.z.abs() < 0.9 {
        UP
    } else {
        Point3::new(1.0, 0.0, 0.0)
    };
    let e1 = match normalize(&u.cross(any)) {
        Some(v) => v,
        None => {
            return Err(GenError::Invalid(
                "the fitted axis is not a direction - the mesh is corrupt".into(),
            ))
        }
    };
    let e2 = u.cross(e1);

    // The mesh's own radial and station extents.
    let (mut rho_max, mut rho_min) = (0.0f64, f64::INFINITY);
    let (mut v_lo, mut v_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for t in facets {
        for q in [t.a, t.b, t.c] {
            let d = q - ax.origin;
            let v = d.dot(u);
            let w = Point3::new(d.x - u.x * v, d.y - u.y * v, d.z - u.z * v);
            let rho = (w.dot(w)).sqrt();
            rho_max = rho_max.max(rho);
            rho_min = rho_min.min(rho);
            v_lo = v_lo.min(v);
            v_hi = v_hi.max(v);
        }
    }
    if rho_max <= 0.0 || !rho_max.is_finite() {
        return Err(GenError::Invalid(
            "the work has no radius to wrap - every facet sits on the axis".into(),
        ));
    }
    let rho_stock = stock.map_or(0.0, |s| {
        s.iter()
            .flat_map(|t| [t.a, t.b, t.c])
            .map(|q| {
                let d = q - ax.origin;
                let v = d.dot(u);
                let w = Point3::new(d.x - u.x * v, d.y - u.y * v, d.z - u.z * v);
                w.dot(w).sqrt()
            })
            .filter(|x| x.is_finite())
            .fold(0.0f64, f64::max)
    });
    let rho_ref = rho_max.max(rho_stock);

    // The world stations window, on standing axes only (doc header).
    let uz = u.dot(UP);
    if uz.abs() >= LEAN_MIN {
        let s_top = (p.top_z - ax.origin.z) / uz;
        let s_bot = (p.bottom_z - ax.origin.z) / uz;
        v_lo = v_lo.max(s_bot.min(s_top));
        v_hi = v_hi.min(s_top.max(s_bot));
        if v_lo >= v_hi {
            return Err(GenError::Empty(
                "the height window leaves no station of the work to rough".into(),
            ));
        }
    }

    // The wrap mesh and its developed extent. A facet is read by
    // which of its vertices stand on the axis: exactly one (the
    // triangle closing a pole or a cap fan) develops as the whole
    // RECTANGLE it sweeps between its rim vertices' columns, never
    // as an affine blend - an axis vertex's own azimuth is
    // meaningless and a blended one would shear every radial edge
    // off its rim's column, slicing the fan open. Each facet also
    // picks its lap: measured from its lowest column unless those
    // straddle the ±π branch, where the high seam column rides one
    // turn below instead, so a mesh vertex keeps ONE developed
    // coordinate in every facet sharing it. The mesh is then
    // duplicated a full period so the seam's chords meet their own
    // copy's ends.
    let r = p.tool_diameter * 0.5;
    let period = TAU * rho_ref;
    let rho_eps = 1e-12 * rho_max;
    let v_eps = 1e-9 * rho_max.max(1.0);
    let mut wrap: Vec<Triangle> = Vec::with_capacity(facets.len() * 3);
    let (mut s_lo, mut s_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut stations: Vec<(f64, f64)> = Vec::new(); // (v, widest rho there)
    let dev = |q0: Point3| -> (f64, f64, f64) {
        let d = q0 - ax.origin;
        let v = d.dot(u);
        let w = Point3::new(d.x - u.x * v, d.y - u.y * v, d.z - u.z * v);
        let rho = (w.dot(w)).sqrt();
        let th = if rho <= rho_eps {
            0.0
        } else {
            w.dot(e2).atan2(w.dot(e1))
        };
        (th, v, rho)
    };
    for t in facets {
        for q in [t.a, t.b, t.c] {
            let (_, v, rho) = dev(q);
            stations.push((v, rho));
        }
    }
    for shift in [0.0, period] {
        for t in facets {
            let (ta, va, ra) = dev(t.a);
            let (tb, vb, rb) = dev(t.b);
            let (tc, vc, rc) = dev(t.c);
            let mn = ta.min(tb).min(tc);
            let mx = ta.max(tb).max(tc);
            let base = if mx - mn > std::f64::consts::PI {
                mx
            } else {
                mn
            };
            let s = |th: f64| -> f64 {
                // Measured (Pi - x).rem_euclid so a column exactly
                // half a turn off the base rides +Pi, never its -Pi
                // alias: the two spellings must agree across every
                // facet that shares the vertex, or the weld splits.
                rho_ref
                    * (base
                        + std::f64::consts::PI
                        - (std::f64::consts::PI - (th - base)).rem_euclid(TAU))
                    + shift
            };
            let n_off = usize::from(ra > rho_eps)
                + usize::from(rb > rho_eps)
                + usize::from(rc > rho_eps);
            if n_off == 2 {
                // The wedge: axis row running under both rim columns,
                // rim row the meridian segment itself.
                let (axv, b, c) = if ra <= rho_eps {
                    ((va, ra), (s(tb), vb, rb), (s(tc), vc, rc))
                } else if rb <= rho_eps {
                    ((vb, rb), (s(ta), va, ra), (s(tc), vc, rc))
                } else {
                    ((vc, rc), (s(ta), va, ra), (s(tb), vb, rb))
                };
                let (a0, a1) = (
                    Point3::new(b.0, axv.0, axv.1),
                    Point3::new(c.0, axv.0, axv.1),
                );
                let (pb, pc) = (Point3::new(b.0, b.1, b.2), Point3::new(c.0, c.1, c.2));
                wrap.push(Triangle::new(a0, a1, pc));
                wrap.push(Triangle::new(a0, pc, pb));
                s_lo = s_lo.min(b.0).min(c.0);
                s_hi = s_hi.max(b.0).max(c.0);
            } else {
                let pt = |th: f64, v: f64, rho: f64| Point3::new(s(th), v, rho);
                wrap.push(Triangle::new(pt(ta, va, ra), pt(tb, vb, rb), pt(tc, vc, rc)));
                s_lo = s_lo.min(s(ta)).min(s(tb)).min(s(tc));
                s_hi = s_hi.max(s(ta)).max(s(tb)).max(s(tc));
            }
        }
    }

    // The cover's two ends are the chain's loose ends: the meridian's
    // cross-section (its stations, widest rho at each, floored at the
    // axis where the profile dips inside it) stands as a WALL at s_lo
    // and s_hi, so every level's chord chain runs into a closed wall
    // at each end and its contour is a ring, not a loose thread.
    stations.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut st: Vec<(f64, f64)> = Vec::new();
    for (v, rho) in stations {
        match st.last_mut() {
            Some(l) if (l.0 - v).abs() <= v_eps => l.1 = l.1.max(rho),
            _ => st.push((v, rho)),
        }
    }
    if st.len() >= 2 {
        let floor = rho_min.max(0.0);
        for w in [s_lo, s_hi] {
            for pr in st.windows(2) {
                let (v0, r0) = pr[0];
                let (v1, r1) = pr[1];
                let wall = |v: f64, rho: f64| Point3::new(w, v, rho);
                wrap.push(Triangle::new(
                    wall(v0, floor),
                    wall(v1, floor),
                    wall(v1, r1),
                ));
                wrap.push(Triangle::new(
                    wall(v0, floor),
                    wall(v1, r1),
                    wall(v0, r0),
                ));
            }
        }
    }

    // The ladder in wrap space, run by pocket3d's own entry guards.
    let mut q = *p;
    q.top_z = rho_ref;
    q.bottom_z = rho_min.max(0.0);
    q.clearance_z = rho_ref + p.stepdown + r;
    q.retract_z = None;
    if q.top_z < q.bottom_z {
        return Err(GenError::Empty(
            "the wrapped work has no radial extent to rough".into(),
        ));
    }
    // The pad is the SEAM's margin only: in v the rect stops at the
    // work's own (windowed) stations - erosion already keeps a
    // centerline r inside a boundary while its tool edge reaches the
    // boundary itself, and padding past an end face would invent
    // phantom air-cut bands where the slice sees no material.
    let pad = r + q.stock_to_leave;
    let rect = wrap_rect(s_lo - pad, s_hi + pad, v_lo, v_hi);
    // Air levels (M13.8): a plane above the part's skin plus its
    // leave meets no wrapped facet, so its target is the whole cover -
    // and the cover is two laps, the seam copy included. One lap plus
    // the seam pad each side clears every azimuth once (the eroded
    // centres reach both pads, the tool edge one radius past them),
    // so those planes sweep one lap; every plane that can meet the
    // part keeps both, the seam copy closing its rings.
    let one_lap = wrap_rect(s_lo - pad, (s_lo + period + pad).min(s_hi + pad), v_lo, v_hi);
    let air_above = rho_max + q.stock_to_leave.max(0.0);
    let base = machining3d::pocket3d_air_split(&wrap, &one_lap, &rect, air_above, &q)?;
    wrap_exit::wrap_exit(&base, &ax, &e1, &e2, rho_ref, r, c)
}

/// The wrap's footprint: one rectangle over the covered development
/// and the (windowed) station range - every wrapped facet is inside,
/// and the period copies keep the seam's slices honest.
fn wrap_rect(s0: f64, s1: f64, v0: f64, v1: f64) -> Region {
    let p = |x: f64, y: f64| Point2::new(x, y);
    match Loop::new(vec![p(s0, v0), p(s1, v0), p(s1, v1), p(s0, v1)]) {
        Ok(l) => match Region::new(vec![l]) {
            Ok(rg) => rg,
            Err(_) => Region::empty(),
        },
        Err(_) => Region::empty(),
    }
}

fn normalize(v: &Point3) -> Option<Point3> {
    let l = (v.dot(*v)).sqrt();
    if !l.is_finite() || l <= f64::EPSILON {
        None
    } else {
        Some(Point3::new(v.x / l, v.y / l, v.z / l))
    }
}
