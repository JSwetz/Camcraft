//! The planar ladder's dives (split out of `descent.rs` by M11.3 at the
//! 400-line seam): the axial feed down from just over the material, the
//! at-depth link from a dive to the pass start - or the named second
//! dive - and M8.1's substituted dive. Provenance: standard CNC plunge
//! entries per public G-code manuals and machining handbooks; the laws
//! are this crate's (M8.1's plan law, M11.3's one-entry law). No vendor
//! material consulted.

use crate::approach::CENTER_CUT_REFUSAL;
use crate::cl::{MoveKind, PathBuilder};
use crate::entry::chord_hits_plan;
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::descent::{Lvl, dive_point};
use super::levels::{Cert, Motion};

/// The axial feed down onto `(xy, z)` from where the tool stands over
/// `xy` (M11.3, the user's hand-test: no feed through tens of mm of
/// air): a rapid first to the plunge height over `mat_top` - the
/// material's top at this level, air above by the superset law - held
/// under the retract plane, then the feed. A zero plunge height feeds
/// the whole way, as every pre-M11.3 path did.
pub(super) fn feed_down(b: &mut PathBuilder, xy: Point2, z: f64, mat_top: f64, m: &Motion, feed: f64) {
    let h = m.links.approach.plunge_height;
    let hz = (mat_top + h).min(m.hop_z.max(mat_top));
    if h > 0.0 && hz.is_finite() && hz < b.at().z && hz > z {
        b.push(MoveKind::Rapid, Point3::new(xy.x, xy.y, hz), None);
    }
    b.push(MoveKind::Plunge, Point3::new(xy.x, xy.y, z), Some(feed));
}

/// The link from a just-completed dive at `d` on plane `z` to the pass
/// start `e`: nothing when the dive ended on it; a feed at depth when the
/// chord is proven (inside the center, crossing none of the level's
/// planned paths) - the cut out of the descent, at the lead-in feed
/// (M11.3 D); else hop, traverse and dive again - the second dive M11.3
/// names, never silent. Returns the dive point when a link was cut.
pub(super) fn dive_link(b: &mut PathBuilder, l: &Lvl, d: Point2, e: Point2, z: f64, m: &Motion) -> Option<Point2> {
    if d.x == e.x && d.y == e.y {
        return None;
    }
    link_at_depth(b, l.cen, l.passes, d, e, z, l.mat_top, m);
    Some(d)
}

/// [`dive_link`]'s law for any caller (the contour's bore side).
#[allow(clippy::too_many_arguments)]
pub(super) fn link_at_depth(
    b: &mut PathBuilder,
    cen: &Cert,
    passes: &[Vec<Point2>],
    d: Point2,
    e: Point2,
    z: f64,
    mat_top: f64,
    m: &Motion,
) {
    if cen.chord_ok(d, e) && !chord_hits_plan(d, e, passes) {
        b.push(MoveKind::Feed, Point3::new(e.x, e.y, z), Some(m.links.approach.lead_feed(m.feed)));
    } else {
        if !m.links.approach.center_cutting {
            b.note_once(&format!(
                "{CENTER_CUT_REFUSAL} - the pass start ({:.3}, {:.3}) is reached only by a second dive into solid",
                e.x, e.y
            ));
        }
        b.note(format!(
            "entry: a second dive at the pass start ({:.3}, {:.3}) - the link from the entry at ({:.3}, {:.3}) \
             is not proved clear at depth",
            e.x, e.y, d.x, d.y
        ));
        b.push(MoveKind::Rapid, Point3::new(d.x, d.y, m.hop_z), None);
        b.push(MoveKind::Rapid, Point3::new(e.x, e.y, m.hop_z), None);
        feed_down(b, e, z, mat_top, m, m.plunge_feed);
    }
}

/// The entry family's honorable failure (M8.1): the stated motion
/// could not be honored at this level for `why`, so the ladder
/// substitutes the axial dive - at the pass start where it stands off
/// the wall (M11.3), else at the floor's own room - and says so on the
/// status line with where it dived. A floor with no room either falls
/// back to the wall dive the pocket used to post, still loud. `tz` is
/// the traverse plane.
pub(super) fn substituted_dive(
    b: &mut PathBuilder,
    l: &Lvl,
    e: Point2,
    z: f64,
    m: &Motion,
    why: &str,
    tz: f64,
) -> Option<Point2> {
    let pf = m.links.approach.plunge(m.plunge_feed);
    match dive_point(l, e, m) {
        Some(d) => {
            b.note(format!("entry: {why} - axial dive on the floor instead at ({:.3}, {:.3})", d.x, d.y));
            let cur = b.at();
            if cur.x != d.x || cur.y != d.y {
                b.push(MoveKind::Rapid, Point3::new(d.x, d.y, tz), None);
            }
            feed_down(b, d, z, l.mat_top, m, pf);
            dive_link(b, l, d, e, z, m)
        }
        None => {
            b.note(format!(
                "entry: {why} - the floor has no dive room either; dived at the pass start"
            ));
            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, tz), None);
            feed_down(b, e, z, l.mat_top, m, pf);
            None
        }
    }
}
