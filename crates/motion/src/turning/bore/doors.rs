//! The boring doors: what the MODEL must answer before a path
//! exists. A min-envelope over a bar with no hole reads its own
//! skin as wall and rides tangent IN steel - tangency is not
//! gouging, so the facet verdict would measure nothing wrong.
//! These doors close that gap by asking the mesh directly whether
//! the spindle axis stands in the hole's air, and whether the
//! ball's own axial reach clears the floor and ceiling.
//!
//! Provenance: the even-odd ray-parity point-in-solid test is
//! Ericson's Real-Time Collision Detection, ch. 5, in its 3D form;
//! the three-angle vote against tessellation grazing is ours.

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::turning::profile::Frame;

use super::HAIR;

/// What the model must say before a path exists: that its window
/// stands in open bore air - at the window's ends, its middle and
/// every wall vertex inside it - and that the ball's own axial
/// reach clears the floor below and the ceiling above. Each refusal
/// names the station it stands on.
pub(super) fn bore_doors(
    facets: &[Triangle],
    f: &Frame,
    wall: &[(f64, f64)],
    s_lo: f64,
    s_hi: f64,
    d: f64,
) -> Result<(), GenError> {
    let mut stations: Vec<f64> = vec![s_lo, 0.5 * (s_lo + s_hi), s_hi];
    stations.extend(
        wall.iter()
            .map(|q| q.0)
            .filter(|&s| s_lo < s && s < s_hi),
    );
    stations.sort_unstable_by(f64::total_cmp);
    stations.dedup();
    let solid: Vec<f64> = stations
        .iter()
        .filter(|&&s| axis_in_steel(facets, f, s))
        .copied()
        .collect();
    if solid.len() == stations.len() {
        return Err(GenError::Empty(format!(
            "nothing to bore - no hole's air stands open in the window: at station \
             {s_lo:.3} the axis is solid metal, and drilling is not this op's promise",
        )));
    }
    if let Some(&s) = solid.first() {
        return Err(GenError::Invalid(format!(
            "the bore's air does not run open through the window - at station {s:.3} \
             the bar's path stands in solid metal: boring rides one open hole, and \
             this window does not fit inside one",
        )));
    }
    // The ball's own reach past the ride's ends: a floor below or
    // a ceiling above that the nose cannot pass without biting. The
    // number to type names the OBSTACLE's station, not the window's:
    // the steel the test point stands in is a face's interior, and
    // that face's boundary on the axis is a vertex - hence a wall
    // point at radius ~0 - so the closest axis vertex below (above)
    // the end is the floor (ceiling) itself, and end-of-the-window
    // plus the nose is what clears it. The window-plus-nose number is
    // the honest fallback for a solid with no axis vertex to name.
    if axis_in_steel(facets, f, s_lo - d + HAIR) {
        let floor_at = wall
            .iter()
            .filter(|q| q.1 <= HAIR && q.0 <= s_lo + HAIR)
            .map(|q| q.0)
            .fold(Option::<f64>::None, |m: Option<f64>, s| Some(m.map_or(s, |v| v.max(s))));
        return Err(GenError::Invalid(format!(
            "the window's bottom stands within the nose's reach of the hole's floor - \
             a ball of radius {d:.3} would bite it: raise bottom Z to at least \
             {:.3}",
            floor_at.map_or(s_lo + d, |fl| fl + d)
        )));
    }
    if axis_in_steel(facets, f, s_hi + d - HAIR) {
        let ceil_at = wall
            .iter()
            .filter(|q| q.1 <= HAIR && q.0 >= s_hi - HAIR)
            .map(|q| q.0)
            .fold(Option::<f64>::None, |m: Option<f64>, s| Some(m.map_or(s, |v| v.min(s))));
        return Err(GenError::Invalid(format!(
            "the window's top stands within the nose's reach of a ceiling - a ball of \
             radius {d:.3} would bite it: lower top Z to at most {:.3}",
            ceil_at.map_or(s_hi - d, |cl| cl - d)
        )));
    }
    Ok(())
}

/// The point-in-solid answer at the axis of a station: an even
/// number of surface crossings out of an axis point means air
/// (the hole), odd means steel. Voted at three angles around the
/// axis so a ray that grazes a tessellation vertex or rim cannot
/// swing the answer alone; a ray along the frame's own radial is
/// angle 0 and the other two are its 120-degree turns, which are
/// never band vertices of a revolve tessellated at 16 (or any
/// multiple of 4 that this suite's fixtures and the importer's
/// defaults produce).
fn axis_in_steel(facets: &[Triangle], f: &Frame, s: f64) -> bool {
    let o = f.point(0.0, s);
    let u0 = f.point(1.0, s) - o;
    // dir x radial, the frame's other in-plane unit (both unit,
    // both perpendicular to each other and the axis).
    let dir = f.point(0.0, s + 1.0) - o;
    let w = cross(dir, u0);
    let mut inside = 0;
    for k in 0..3 {
        let (c, sn) = if k == 0 {
            (1.0, 0.0)
        } else if k == 1 {
            (-0.5, 0.866_025_403_784_438_6)
        } else {
            (-0.5, -0.866_025_403_784_438_6)
        };
        let u = Point3::new(c * u0.x + sn * w.x, c * u0.y + sn * w.y, c * u0.z + sn * w.z);
        let mut hits = 0usize;
        for t in facets {
            if ray_hits(o, u, t) {
                hits += 1;
            }
        }
        if hits % 2 == 1 {
            inside += 1;
        }
    }
    inside >= 2
}

/// Moller-Trumbore: does the ray o + t*u (t > HAIR) meet the
/// triangle's interior? A parallel ray is a miss by convention -
/// grazing is the voter's job, not this one's.
fn ray_hits(o: Point3, u: Point3, t: &Triangle) -> bool {
    let e1 = t.b - t.a;
    let e2 = t.c - t.a;
    let p = cross(u, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-12 {
        return false;
    }
    let tv = o - t.a;
    let a1 = dot(tv, p) / det;
    if a1 < 0.0 || a1 > 1.0 {
        return false;
    }
    let q = cross(tv, e1);
    let a2 = dot(u, q) / det;
    if a2 < 0.0 || a1 + a2 > 1.0 {
        return false;
    }
    dot(e2, q) / det > HAIR
}

fn dot(a: Point3, b: Point3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn cross(a: Point3, b: Point3) -> Point3 {
    Point3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}
