//! Lay the part on a G17 / G18 / G19 plane from picks (#330): one flat
//! face, or two straight edges, turned so the face's outward normal (or
//! the edges' plane normal) runs along the plane's axis - +Z for G17 (XY),
//! +Y for G18 (ZX), +X for G19 (YZ), or the opposite way with `flip` - and
//! the first edge along the plane's first axis (X for G17 / G18, Y for
//! G19). The result is a setup's part alignment (#326): a rigid transform
//! from the document frame into the setup frame, handed to
//! `AlignPartToSetup::new`. Pure: nothing here writes the world, and the
//! work offset is never touched.
//!
//! Provenance: our own design over plain fixture practice (lay a datum
//! face on the table); the shortest-arc quaternion between two unit
//! vectors and the frame-to-frame rotation R = T * S^T are textbook
//! rigid-body algebra (Shoemake, "Animating rotation with quaternion
//! curves", SIGGRAPH 1985). No vendor material consulted.

use flecs_ecs::prelude::World;

use crate::cl::ArcPlane;
use crate::geometry::feature::FaceKind;
use crate::geometry::{Point3, normalize_quat, quat_conjugate, quat_mul, quat_rotate_point, transform_point};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};
use crate::schema::Location;
use crate::setup_align;

use super::align::{cross, dot, gone, norm, outward_normal, quat_from_axes, scale, straight_edge, sub};

/// Two edges closer than this to parallel (degrees) span no plane worth
/// trusting.
const MIN_EDGE_ANGLE_DEG: f64 = 1.0;
/// Two parallel edges this close (mm) lie on one line: coincident.
const COINCIDENT_MM: f64 = 0.01;
/// A face with less area than this (mm^2) is degenerate.
const MIN_FACE_AREA_MM2: f64 = 1e-9;
/// The turned normal must land on the target axis this closely (rad).
const LAND_TOL_RAD: f64 = 1e-9;

/// How to lay the part: the target plane, which way its normal faces, and
/// whether the picked face (or the first edge) is set onto the plane's
/// level (0 along the plane's axis) or the part keeps its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneLay {
    pub plane: ArcPlane,
    /// Face the opposite way: -Z for G17, -Y for G18, -X for G19.
    pub flip: bool,
    /// Set the picked face onto the plane's level (e.g. the face to Z 0).
    pub place: bool,
}

/// The plain words for a target plane: "G17 (XY)", "G18 (ZX)", "G19 (YZ)".
#[must_use]
pub fn plane_label(plane: ArcPlane) -> &'static str {
    match plane {
        ArcPlane::Xy => "G17 (XY)",
        ArcPlane::Xz => "G18 (ZX)",
        ArcPlane::Yz => "G19 (YZ)",
    }
}

/// (first axis, normal axis index): the plane's first in-plane axis and
/// the axis its normal runs along.
fn axes_of(plane: ArcPlane) -> ([f64; 3], usize) {
    match plane {
        ArcPlane::Xy => ([1.0, 0.0, 0.0], 2),
        ArcPlane::Xz => ([1.0, 0.0, 0.0], 1),
        ArcPlane::Yz => ([0.0, 1.0, 0.0], 0),
    }
}

/// The plane's normal as laid: its axis, negated with `flip`.
#[must_use]
pub fn target_normal(lay: PlaneLay) -> [f64; 3] {
    let (_, k) = axes_of(lay.plane);
    let mut n = [0.0; 3];
    n[k] = if lay.flip { -1.0 } else { 1.0 };
    n
}

/// What the picks fix: a source normal, an optional in-plane direction
/// (the first edge), and the point that goes to the plane's level.
struct Datum {
    normal: [f64; 3],
    along: Option<[f64; 3]>,
    anchor: [f64; 3],
}

/// The part alignment (document frame -> setup frame) that lays the
/// picked face or edges on `lay`'s plane, turned about the parts' bounds
/// centre like the Setups page presets. Every refusal names what to
/// re-pick. The identity comes back as an identity `Location`; the
/// command stores it as "as modelled".
pub fn lay_on_plane(world: &World, picks: &[GeoSel], lay: PlaneLay) -> Result<Location, String> {
    let datum = datum_of(world, picks)?;
    let t3 = target_normal(lay);
    let rot = match datum.along {
        None => shortest_arc(datum.normal, t3, lay.plane),
        Some(d1) => {
            let (t1, _) = axes_of(lay.plane);
            let t2 = cross(t3, t1);
            let s2 = cross(datum.normal, d1);
            let q_t = quat_from_axes(t1, t2, t3);
            let q_s = quat_from_axes(d1, s2, datum.normal);
            q_t.zip(q_s).and_then(|(t, s)| normalize_quat(quat_mul(t, quat_conjugate(s))))
        }
    }
    .ok_or("the picks do not make a usable turn - re-pick them")?;
    // Prove the landing: a turn that misses the axis is refused, never
    // stored as a subtly tilted part.
    // The sine of the miss (the cross product), not a cosine: cos(1e-9)
    // rounds to 1.0 in f64.
    let landed = quat_rotate_point(rot, p3(datum.normal));
    let landed = [landed.x, landed.y, landed.z];
    if !(dot(landed, t3) > 0.0 && norm(cross(landed, t3)) <= LAND_TOL_RAD) {
        return Err("the picks do not make a usable turn - re-pick them".into());
    }
    let c = setup_align::part_centre(world).ok_or("there is no part to align - import a model first")?;
    let rc = quat_rotate_point(rot, c);
    let mut pos = Point3::new(c.x - rc.x, c.y - rc.y, c.z - rc.z);
    if lay.place {
        let a = transform_point(p3(datum.anchor), pos, rot);
        match axes_of(lay.plane).1 {
            0 => pos.x -= a.x,
            1 => pos.y -= a.y,
            _ => pos.z -= a.z,
        }
    }
    let finite = [pos.x, pos.y, pos.z].iter().chain(&rot).all(|v| v.is_finite());
    if !finite {
        return Err("the picks do not make a finite turn - re-pick them".into());
    }
    Ok(Location { pos, rot })
}

/// Read the picks as a datum, in the DOCUMENT frame (the alignment is
/// absolute: picking the same face again in an aligned setup gives the
/// same answer). Exactly one flat face, or exactly two straight edges.
fn datum_of(world: &World, picks: &[GeoSel]) -> Result<Datum, String> {
    let mut faces = Vec::new();
    let mut edges = Vec::new();
    for g in picks {
        let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
        if !setup_align::is_part(world, &e) {
            return Err(format!("'{}' is not the part - pick a face or edge of the model", g.entity));
        }
        match g.feature {
            FeatureSel::Face(_) => faces.push(g),
            FeatureSel::Chain(_) => edges.push(g),
            FeatureSel::Patch(_) => {
                return Err(format!("{} of '{}' is curved - pick a flat face", g.feature, g.entity));
            }
            FeatureSel::Vertex(_) => {
                return Err(format!(
                    "{} of '{}' is a vertex - pick a flat face, or two straight edges",
                    g.feature, g.entity
                ));
            }
        }
    }
    match (faces.as_slice(), edges.as_slice()) {
        ([g], []) => face_datum(world, g),
        ([], [a, b]) => edge_datum(world, a, b),
        _ => Err("pick one flat face, or two straight edges".into()),
    }
}

fn face_datum(world: &World, g: &GeoSel) -> Result<Datum, String> {
    let FeatureSel::Face(fid) = g.feature else { return Err(gone(g)) };
    let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
    let features = model::features_of(world, &e).ok_or_else(|| gone(g))?;
    let tris = model::facets_of(world, &e).ok_or_else(|| gone(g))?;
    let f = features.faces.iter().find(|f| f.id == fid).ok_or_else(|| gone(g))?;
    if f.kind == FaceKind::Degenerate {
        return Err(format!("{} of '{}' is degenerate (no usable plane) - pick another face", g.feature, g.entity));
    }
    let normal = outward_normal(world, g, model::transform_of)?
        .ok_or_else(|| format!("{} of '{}' is not flat - pick a flat face", g.feature, g.entity))?;
    let loc = model::transform_of(world, &e).unwrap_or_default();
    // Area-weighted centroid, placed: the point that goes to the level.
    let (mut area, mut sum) = (0.0_f64, [0.0_f64; 3]);
    for t in f.facets.iter().filter_map(|&i| tris.get(i as usize)) {
        let n = (t.b - t.a).cross(t.c - t.a);
        let a = 0.5 * (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
        if !a.is_finite() {
            continue;
        }
        let m = [(t.a.x + t.b.x + t.c.x) / 3.0, (t.a.y + t.b.y + t.c.y) / 3.0, (t.a.z + t.b.z + t.c.z) / 3.0];
        area += a;
        sum = [sum[0] + a * m[0], sum[1] + a * m[1], sum[2] + a * m[2]];
    }
    if area.is_nan() || area <= MIN_FACE_AREA_MM2 {
        return Err(format!("{} of '{}' has no area - pick a face with a surface", g.feature, g.entity));
    }
    let local = Point3::new(sum[0] / area, sum[1] / area, sum[2] / area);
    let w = transform_point(local, loc.pos, loc.rot);
    Ok(Datum { normal, along: None, anchor: [w.x, w.y, w.z] })
}

fn edge_datum(world: &World, first: &GeoSel, second: &GeoSel) -> Result<Datum, String> {
    let role = "to set the plane";
    let (a1, b1) = straight_edge(world, first, model::transform_of, role)?;
    let (a2, b2) = straight_edge(world, second, model::transform_of, role)?;
    let d1 = unit(sub(b1, a1)).ok_or_else(|| gone(first))?;
    let d2 = unit(sub(b2, a2)).ok_or_else(|| gone(second))?;
    let c = cross(d1, d2);
    let spread = norm(c);
    if spread.is_nan() || spread < MIN_EDGE_ANGLE_DEG.to_radians().sin() {
        // Parallel: on one line (coincident) or side by side.
        let off = sub(a2, a1);
        let gap = norm(sub(off, scale(d1, dot(off, d1))));
        return Err(if gap <= COINCIDENT_MM {
            "the two edges are coincident - pick two edges that meet at an angle".into()
        } else {
            "the two edges are parallel - pick two edges that meet at an angle".into()
        });
    }
    let mut n = unit(c).ok_or("the two edges span no plane - re-pick them")?;
    // The plane's outside is the side away from the parts' centre (two
    // edges carry no winding); on a plane through the centre, d1 x d2.
    let mid = scale([a1[0] + b1[0] + a2[0] + b2[0], a1[1] + b1[1] + a2[1] + b2[1], a1[2] + b1[2] + a2[2] + b2[2]], 0.25);
    if let Some(ctr) = setup_align::part_centre(world)
        && dot(n, sub(mid, [ctr.x, ctr.y, ctr.z])) < 0.0
    {
        n = scale(n, -1.0);
    }
    Ok(Datum { normal: n, along: Some(d1), anchor: a1 })
}

/// The shortest turn taking unit `from` onto unit `to`. Antiparallel: a
/// half turn about the plane's first axis when it lies square to `from`
/// (a G17 flip is "flip over X"), else about any axis square to `from`.
fn shortest_arc(from: [f64; 3], to: [f64; 3], plane: ArcPlane) -> Option<[f64; 4]> {
    let w = 1.0 + dot(from, to);
    if w > 1e-12 {
        let v = cross(from, to);
        return normalize_quat([v[0], v[1], v[2], w]);
    }
    let (t1, _) = axes_of(plane);
    let axis = [t1, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .into_iter()
        .map(|a| sub(a, scale(from, dot(a, from))))
        .find(|a| norm(*a) > 0.5)?;
    let axis = unit(axis)?;
    Some([axis[0], axis[1], axis[2], 0.0])
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let l = norm(a);
    (l.is_finite() && l > 0.0).then(|| scale(a, 1.0 / l))
}

fn p3(a: [f64; 3]) -> Point3 {
    Point3::new(a[0], a[1], a[2])
}
