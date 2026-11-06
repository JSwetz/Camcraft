//! Contour along an open chain (moved out of `contour.rs` by M11.2 C,
//! which taught it the level link): the cutter center rides a picked
//! run's own points level by level. Provenance: open-profile contouring
//! of published CAM practice (lead off the run's ends, plunge at the
//! start, retract between levels); RS-274 / ISO 6983 linear and circular
//! interpolation. No vendor material consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::Entry3D;
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::Link;

use super::arcs::{fit_arcs, lead_in, lead_out, push_segs, walk_tangent};
use super::contour::{ContourParams, air_side};
use super::face::GenError;

/// Contour along an open chain: a run whose ends stop mid-edge instead of
/// a loop around material. The cutter center rides the chain's own
/// points: plunge at one end, feed through every vertex at each level,
/// retract at the other. Climb travels the chain in stored order,
/// conventional reversed, and the same air-side rule sends the lead-in
/// and lead-out off the ends - an open run has no material to its side to
/// keep off, so the ends themselves are where the lead lives. The tab
/// fields do not apply (there is nothing left held between passes), and
/// the level ladder, clearance plane and lead geometry are `contour`'s
/// reused. Between levels the tool lifts to clearance, or to the retract
/// plane when that is the level link; a keep-down link has no area to be
/// proved against here and lifts to the retract plane, named.
///
/// Half the tool overhangs the run on either side; a verifier told about
/// a surrounding region reads that as the wall it is, and says so.
pub fn contour_along_chain(
    pts: &[Point2],
    p: &ContourParams,
) -> Result<Toolpath, GenError> {
    p.validate()?;
    if pts.len() < 2 {
        return Err(GenError::Empty(
            "a centerline run needs at least two points".into(),
        ));
    }
    let poly: Vec<Point2> = if p.climb {
        pts.to_vec()
    } else {
        let mut v = pts.to_vec();
        v.reverse();
        v
    };
    let zs = p.levels();
    let walk = fit_arcs(&poly, p.tolerance);
    let t0 = walk_tangent(&walk, &poly, true);
    let (entry, lead) = lead_in(poly[0], t0, air_side(t0, p.climb), p.lead);
    let n = poly.len();
    let t1 = walk_tangent(&walk, &poly, false);
    let out = lead_out(poly[n - 1], t1, air_side(t1, p.climb), p.lead_out_shape());

    let mut b = PathBuilder::new(Point3::new(0.0, 0.0, p.clearance_z));
    if p.blend_turns {
        // No region, no side: nothing a fillet could be proved against.
        b.note("turn blends: an open centerline run bounds no area to prove a fillet against - its corners kept sharp");
    }
    // An open run bounds no area, so no dive on it can be certified
    // against air that exists: the chosen entry family (and transition)
    // is refused by name once, and every level dives at the lead-in's
    // start as the chain has always done (M8.2 A).
    let arm = p.links.arm(p.entry);
    for (i, e) in [p.entry, arm].into_iter().enumerate() {
        if e != Entry3D::Plunge && !(i == 1 && e == p.entry) {
            let w = match e {
                Entry3D::Ramp => "ramp",
                Entry3D::Helix => "helix",
                Entry3D::Arc => "arc",
                Entry3D::Plunge => "plunge",
            };
            b.note(format!(
                "entry: an open centerline run bounds no air to certify the {w} against - dive \
                 at the run start, unchanged"
            ));
        }
    }
    let hop = p.retract_z.min(p.clearance_z);
    let lift_z = match p.links.level {
        Link::Clearance => p.clearance_z,
        Link::Retract => hop,
        Link::KeepDown => {
            if zs.len() > 1 {
                b.note(
                    "link: an open centerline run bounds no area to prove a keep-down link \
                     against - lifted to the retract plane between levels",
                );
            }
            hop
        }
    };
    for (k, &z) in zs.iter().enumerate() {
        if k > 0 {
            let cur = b.at();
            b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, lift_z), None);
        }
        let cur = b.at();
        b.push(MoveKind::Rapid, Point3::new(entry.x, entry.y, cur.z), None);
        b.push(MoveKind::Plunge, Point3::new(entry.x, entry.y, z), Some(p.plunge_feed));
        push_segs(&mut b, &lead, z, p.feed);
        push_segs(&mut b, &walk, z, p.feed);
        push_segs(&mut b, &out, z, p.feed);
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);

    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}
