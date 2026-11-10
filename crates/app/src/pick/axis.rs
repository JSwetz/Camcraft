//! Turn the part in the work plane from picks (#557): a straight edge, or
//! two points, sets the setup's +Y (and/or +X). The picked direction is
//! read in the setup frame (through the setup's current part alignment,
//! so it composes after a face-to-plane lay, #330), projected into the
//! work plane (the setup's XY, normal +Z), and the part is turned about Z
//! until it runs along the axis. With both axes picked the primary one
//! (Y by default) wins and the other is only checked: not parallel, and
//! on the right-handed side (X = Y x Z). With one, the other follows
//! right-handed. Pure: the result is the setup's new part alignment,
//! handed to `AlignPartToSetup::new`; the work offset is never touched.
//!
//! Provenance: our own design over plain fixture practice (indicate an
//! edge square to the table's Y travel); the projection onto a plane and
//! the turn about one axis are textbook vector algebra. No vendor
//! material consulted.

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, feature, quat_rotate_point, transform_point};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};
use crate::schema::Location;
use crate::setup_align;

use super::align::{gone, norm, straight_edge, sub};

/// A direction must lean at least this far out of the plane's normal
/// (degrees) to give an in-plane direction worth trusting.
const MIN_TILT_FROM_NORMAL_DEG: f64 = 5.0;
/// Two picked axes closer than this to parallel (degrees) are refused.
const MIN_AXES_ANGLE_DEG: f64 = 1.0;
/// Two picked points closer than this (mm) are one point.
const COINCIDENT_MM: f64 = 1e-3;
/// The turned direction must land on its axis this closely (sine).
const LAND_TOL: f64 = 1e-9;

/// Which in-plane axis a pick sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InPlaneAxis {
    #[default]
    Y,
    X,
}

impl InPlaneAxis {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            InPlaneAxis::Y => "+Y",
            InPlaneAxis::X => "+X",
        }
    }
}

/// One axis's picks: one straight edge (start to end, its walk order),
/// or two points (first to second); `flip` reverses the sense.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AxisPick {
    pub picks: Vec<GeoSel>,
    pub flip: bool,
}

/// What the in-plane turn is read from: +Y and/or +X, and which one wins
/// when both are given (the other is made perpendicular to it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InPlaneAxes {
    pub y: Option<AxisPick>,
    pub x: Option<AxisPick>,
    pub primary: InPlaneAxis,
}

/// The turn about the setup's Z (degrees, in (-180, 180]) that lays the
/// picks along their axes. Every refusal names what to re-pick.
pub fn in_plane_turn_deg(world: &World, setup: &str, axes: &InPlaneAxes) -> Result<f64, String> {
    let held = setup_align::stored(world, setup).unwrap_or_default();
    let uy = axes.y.as_ref().map(|p| in_plane(world, held, p, InPlaneAxis::Y)).transpose()?;
    let ux = axes.x.as_ref().map(|p| in_plane(world, held, p, InPlaneAxis::X)).transpose()?;
    if let (Some(y), Some(x)) = (uy, ux) {
        let s = y[0] * x[1] - y[1] * x[0];
        if s.abs() < MIN_AXES_ANGLE_DEG.to_radians().sin() {
            return Err("the +X and +Y picks are parallel - pick directions that cross".into());
        }
        // Right-handed with Z up: X = Y x Z = (y.y, -y.x), i.e. Y runs
        // counter-clockwise from X - the cross X x Y must point up.
        if s > 0.0 {
            return Err("the +X and +Y picks make a left-handed frame (X = Y x Z) - flip one of them".into());
        }
    }
    let angle = match (uy, ux, axes.primary) {
        (Some(y), Some(_), InPlaneAxis::Y) | (Some(y), None, _) => 90.0 - y[1].atan2(y[0]).to_degrees(),
        (_, Some(x), _) => -x[1].atan2(x[0]).to_degrees(),
        (None, None, _) => return Err("pick a straight edge or two points for +Y or +X".into()),
    };
    let mut deg = angle.rem_euclid(360.0);
    if deg > 180.0 {
        deg -= 360.0;
    }
    // A quarter turn within float noise is the exact quarter turn.
    let quarter = (deg / 90.0).round() * 90.0;
    if (deg - quarter).abs() < 1e-9 {
        deg = if quarter == -180.0 { 180.0 } else { quarter };
    }
    if !deg.is_finite() {
        return Err("the picks do not make a finite turn - re-pick them".into());
    }
    Ok(deg)
}

/// The setup's new part alignment: its current one, then the in-plane
/// turn about the parts' centre (as the setup holds them), so the part
/// stays on its spot of the table. An identity result comes back as an
/// identity `Location`; the command stores it as "as modelled".
pub fn turn_in_plane(world: &World, setup: &str, axes: &InPlaneAxes) -> Result<Location, String> {
    let deg = in_plane_turn_deg(world, setup, axes)?;
    let held = setup_align::stored(world, setup).unwrap_or_default();
    let rot = setup_align::axis_quat(2, deg).ok_or("the picks do not make a finite turn - re-pick them")?;
    let c = setup_align::part_centre(world).ok_or("there is no part to align - import a model first")?;
    let c = transform_point(c, held.pos, held.rot);
    let rc = quat_rotate_point(rot, c);
    let turn = Location { pos: Point3::new(c.x - rc.x, c.y - rc.y, c.z - rc.z), rot };
    let out = setup_align::compose(turn, held);
    // Prove the landing: the primary pick runs along its axis now.
    let primary = match (&axes.y, &axes.x, axes.primary) {
        (Some(y), Some(_), InPlaneAxis::Y) | (Some(y), None, _) => Some((y, InPlaneAxis::Y)),
        (_, Some(x), _) => Some((x, InPlaneAxis::X)),
        _ => None,
    };
    if let Some((p, axis)) = primary {
        let d = in_plane(world, out, p, axis)?;
        let want = if axis == InPlaneAxis::Y { [0.0, 1.0] } else { [1.0, 0.0] };
        let miss = d[0] * want[1] - d[1] * want[0];
        if !(d[0] * want[0] + d[1] * want[1] > 0.0 && miss.abs() <= LAND_TOL) {
            return Err("the picks do not make a usable turn - re-pick them".into());
        }
    }
    let finite = [out.pos.x, out.pos.y, out.pos.z].iter().chain(&out.rot).all(|v| v.is_finite());
    if !finite {
        return Err("the picks do not make a finite turn - re-pick them".into());
    }
    Ok(out)
}

/// One axis's pick as a unit direction in the work plane ([x, y]),
/// read through the alignment `held` (document frame -> setup frame).
fn in_plane(world: &World, held: Location, pick: &AxisPick, axis: InPlaneAxis) -> Result<[f64; 2], String> {
    let name = axis.label();
    let (a, b) = ends(world, pick, name)?;
    let d = sub(b, a);
    let d = quat_rotate_point(held.rot, Point3::new(d[0], d[1], d[2]));
    let d = if pick.flip { [-d.x, -d.y, -d.z] } else { [d.x, d.y, d.z] };
    let len = norm(d);
    let flat = d[0].hypot(d[1]);
    if !(len.is_finite() && flat.is_finite()) || len <= 0.0 {
        return Err(format!("the {name} pick has no length - re-pick it"));
    }
    if flat < len * MIN_TILT_FROM_NORMAL_DEG.to_radians().sin() {
        return Err(format!(
            "the {name} pick runs (nearly) along the work plane's normal (Z) - pick an edge or two points \
             that run across the plane"
        ));
    }
    Ok([d[0] / flat, d[1] / flat])
}

/// The pick's (start, end) in the DOCUMENT frame: one straight edge, or
/// two points.
fn ends(world: &World, pick: &AxisPick, name: &str) -> Result<([f64; 3], [f64; 3]), String> {
    for g in &pick.picks {
        let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
        if !setup_align::is_part(world, &e) {
            return Err(format!("'{}' is not the part - pick an edge or points of the model for {name}", g.entity));
        }
        if matches!(g.feature, FeatureSel::Face(_) | FeatureSel::Patch(_)) {
            return Err(format!(
                "{} of '{}' is a face - a face sets no direction; pick a straight edge or two points for {name}",
                g.feature, g.entity
            ));
        }
    }
    let role = format!("for {name}");
    match pick.picks.as_slice() {
        [] => Err(format!("pick a straight edge or two points for {name}")),
        [g] if matches!(g.feature, FeatureSel::Chain(_)) => straight_edge(world, g, model::transform_of, &role),
        [g] => Err(format!("{} of '{}' is one point - pick a second point for {name}, or a straight edge", g.feature, g.entity)),
        [p, q] if matches!((p.feature, q.feature), (FeatureSel::Vertex(_), FeatureSel::Vertex(_))) => {
            let (a, b) = (point(world, p)?, point(world, q)?);
            let gap = norm(sub(b, a));
            if p == q || !gap.is_finite() || gap <= COINCIDENT_MM {
                return Err(format!("the two points for {name} coincide - pick two points apart"));
            }
            Ok((a, b))
        }
        _ => Err(format!("pick one straight edge, or two points, for {name}")),
    }
}

/// Where one axis pick sits, in the DOCUMENT frame (mm), for readouts:
/// a vertex's point, or a straight edge's (start, end). `None` for a
/// face, a curved or closed chain, or a pick no longer in the model.
#[must_use]
pub fn pick_coords(world: &World, g: &GeoSel) -> Option<Vec<[f64; 3]>> {
    match g.feature {
        FeatureSel::Vertex(_) => point(world, g).ok().map(|p| vec![p]),
        FeatureSel::Chain(_) => straight_edge(world, g, model::transform_of, "").ok().map(|(a, b)| vec![a, b]),
        FeatureSel::Face(_) | FeatureSel::Patch(_) => None,
    }
}

/// A picked vertex in the document frame.
fn point(world: &World, g: &GeoSel) -> Result<[f64; 3], String> {
    let FeatureSel::Vertex(id) = g.feature else { return Err(gone(g)) };
    let e = model::find_by_name(world, &g.entity).ok_or_else(|| gone(g))?;
    let features = model::features_of(world, &e).ok_or_else(|| gone(g))?;
    let vi = feature::resolve_vertex(&features, id).ok_or_else(|| gone(g))?;
    let v = features.verts.get(vi as usize).ok_or_else(|| gone(g))?;
    let loc = model::transform_of(world, &e).unwrap_or_default();
    let w = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
    let out = [w.x, w.y, w.z];
    if !out.iter().all(|c| c.is_finite()) {
        return Err(format!("{} of '{}' is not a finite point - re-pick it", g.feature, g.entity));
    }
    Ok(out)
}
