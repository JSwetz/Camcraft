//! The swarf link's re-entry under the centre-cutting audit (#17, owed
//! from the swarf pose laws): a pose turn past the link threshold
//! retracts the ball along the contact normal, swings in air, and
//! plunges back down the same normal onto the SAME ball centre the last
//! cut finished on. Where the incoming pose stands within the core's
//! cone of that normal (a near-vertical re-entry), the tip-column
//! reading convicts it - no earlier cut ran down the new axis, since the
//! pose turned - though the ball only retraces its own retract and lands
//! in the sphere the previous cut emptied. For a ball tool the audit now
//! reads the already-cut sphere and acquits it; any other plunge, and
//! every non-ball tool, is read by the tip column exactly as before.
//!
//! Provenance: the centre-cutting / non-centre-cutting end mill
//! distinction of machining handbooks; the sphere-invariance argument is
//! the stamp's pivot law. Our own design notes; no vendor material.
//!
//! Gate map (M4.4): covers camcraft_motion::plunge_law, multiaxis (stamp_link).
//! Lane shape (M4.4): milliseconds.

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::multiaxis::link_swings;
use camcraft_motion::plunge_law::{audit_as, first_solid_plunge, first_solid_plunge_as, Solid};

const R: f64 = 3.0;
const CLEARANCE: f64 = 25.0;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Two floor cuts whose poses turn 50 degrees between them, each 25
/// degrees off the floor's normal: the hand-off is linked, and the
/// re-entry runs down -Z under a pose inside the core's 30-degree cone.
fn linked() -> Toolpath {
    let t = 25f64.to_radians();
    let (pa, pb) = ([-t.sin(), 0.0, t.cos()], [t.sin(), 0.0, t.cos()]);
    let tip = |cx: f64, a: [f64; 3]| p(cx - R * a[0], -R * a[1], R - R * a[2]);
    let cut = |from: Point3, to: Point3, a: [f64; 3]| Move::new(MoveKind::Feed, from, to, Some(500.0)).with_axis(a);
    let path = Toolpath::new(vec![
        cut(tip(-4.0, pa), tip(0.0, pa), pa),
        cut(tip(0.0, pa), tip(4.0, pb), pb),
    ]);
    let floor = vec![Triangle::new(p(-50.0, -50.0, 0.0), p(50.0, -50.0, 0.0), p(0.0, 50.0, 0.0))];
    let (linked, links) = link_swings(&path, &floor, R, CLEARANCE, 100.0).expect("the link law takes the turn to air");
    assert_eq!(links, 1, "one hand-off past the threshold");
    linked
}

#[test]
fn a_near_vertical_reentry_into_the_cut_sphere_is_no_plunge_for_a_ball() {
    let path = linked();
    let solid = Solid { top: 10.0, footprint: None };
    let plunge = path.moves.iter().position(|m| m.kind == MoveKind::Plunge).expect("the re-entry plunge");
    // The tip column convicts the re-entry (the false refusal #17 owed).
    let column = first_solid_plunge(&path, R, &solid);
    let at = path.moves[plunge].to;
    assert!(
        column.is_some_and(|(xy, z)| (xy.x - at.x).abs() < 1e-9 && (z - at.z).abs() < 1e-9),
        "the tip-column reading convicts the re-entry: {column:?}"
    );
    // The ball reads the sphere the last cut emptied: acquitted.
    assert_eq!(first_solid_plunge_as(&path, R, &solid, true), None, "the ball re-enters cut material");
    assert!(audit_as(&path, false, R, &solid, true, "swarf").is_ok());
    // A non-ball tool keeps the column reading verbatim.
    assert_eq!(first_solid_plunge_as(&path, R, &solid, false), column);
    assert!(audit_as(&path, false, R, &solid, false, "swarf").is_err(), "a flat tool is refused by name");
}

#[test]
fn a_reentry_that_misses_the_cut_sphere_is_still_a_plunge() {
    let solid = Solid { top: 10.0, footprint: None };
    // The plunge lands one millimetre past the centre the cut finished
    // on: fresh material under the core.
    let mut off = linked();
    let k = off.moves.iter().position(|m| m.kind == MoveKind::Plunge).expect("the re-entry plunge");
    off.moves[k].to.x += 1.0;
    off.moves[k + 1].from.x += 1.0;
    assert!(first_solid_plunge_as(&off, R, &solid, true).is_some(), "a miss of the sphere convicts");
    // The plunge starts somewhere its retract never went: the ball's
    // way down is not the way it came up.
    let mut wide = linked();
    wide.moves[k].from.y += 2.0;
    wide.moves[k - 1].to.y += 2.0;
    assert!(first_solid_plunge_as(&wide, R, &solid, true).is_some(), "an unretraced descent convicts");
}
