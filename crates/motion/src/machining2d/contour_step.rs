//! A contour level's first engagement (moved out of `contour.rs` by
//! M11.2 C): the level link's lift - clearance or the retract plane -
//! and the descent by the entry (first engagement) or the user's
//! transition (every later level): the bore side's certified arms, the
//! tabbed bore's axial dive, the wall side's graze.
//!
//! Provenance: standard multi-depth profile entries of published CAM
//! practice (lead-in from air, plunge/ramp/helix where bounded air
//! certifies them); the certification laws are `levels.rs`'s (M8.1/M8.2).
//! No vendor material consulted.

use crate::cl::{MoveKind, PathBuilder};
use crate::entry::Entry3D;
use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::links::Link;

use super::arcs::Seg;
use super::contour_entry::{Engage, bore_entry};

/// What one level's engagement reads beyond the shared [`Engage`].
pub(super) struct Step {
    /// The motion: the entry on the path's first engagement, the user's
    /// transition after it.
    pub arm: Entry3D,
    pub e2: Point2,
    pub z: f64,
    /// The plane the descent starts from (clearance on the first level).
    pub prev_z: f64,
    pub first: bool,
    pub inside: bool,
    pub tabbed: bool,
}

/// Structural refusals, each said once per path.
#[derive(Default)]
pub(super) struct Said {
    refused: bool,
    tabbed: bool,
}

/// The arm's name in a note.
fn word(arm: Entry3D) -> &'static str {
    match arm {
        Entry3D::Ramp => "ramp",
        Entry3D::Helix => "helix",
        Entry3D::Arc => "arc",
        Entry3D::Plunge => "plunge",
    }
}

/// Lift by the level link (never on the first entry - the tool already
/// stands at clearance) and descend onto `(e2, z)`.
pub(super) fn engage_level(b: &mut PathBuilder, eng: &Engage, s: &Step, lead: &[Seg], walk: &[Seg], said: &mut Said) -> Option<Point2> {
    let p = eng.p;
    // The level link's plane. Keep down reaches here only where the
    // stay-down descent could not be planned (a new ring, a tab, a ring
    // too short): retract and re-enter, as before the option.
    let tz = match (s.first, p.links.level) {
        (true, _) => p.clearance_z,
        (false, Link::Retract) => eng.m.hop_z,
        (false, _) => p.clearance_z,
    };
    let cur = b.at();
    if !s.first {
        b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, tz), None);
    }
    let (e2, z) = (s.e2, s.z);
    if s.inside && s.tabbed {
        // M8.2 C: the arm stands down where a tab is material. dive_room
        // and chord_clears certify against the eroded region - the FULL-
        // depth air - but a standing tab is material INSIDE that region
        // at this level: a helix orbit or a ramp chord certified here
        // could run straight through the tab it was told to leave.
        // Tabbed levels therefore take the clearance traverse and the
        // axial dive at the lead-in; the tab gets its cleanup at the
        // level above, where it is air.
        if s.arm != Entry3D::Plunge && !said.tabbed {
            b.note(
                "entry: no arm stands where a tab is material no \
                 certification can see - axial dive at the lead-in on \
                 tabbed levels",
            );
            said.tabbed = true;
        }
        let cur = b.at();
        b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, cur.z), None);
        b.push(MoveKind::Plunge, Point3::new(e2.x, e2.y, z), Some(p.plunge_feed));
        None
    } else if s.inside {
        // The bore side: the eroded region states the bounded air
        // exactly, so every arm certifies against it with the pocket
        // ladders' own machinery - the dive stands in the floor's room,
        // never dwells on the finished wall. A transition's helix pitch
        // rides along; the first entry keeps the entry's own law.
        let pitch = if s.first { 0.0 } else { p.links.helix_pitch };
        bore_entry(b, eng, s.arm, pitch, e2, z, s.prev_z, tz, lead, walk)
    } else {
        // The wall side (explicit Outside, and every direct Auto): the
        // graze this generator has always posted, and the only one that
        // can be certified - the grown region holds the part's own
        // interior, so a dive_room or a chord on it would certify
        // descents THROUGH the body (M8.1's rule). The other arms are
        // refused by name - once, because the reason is structural.
        if !matches!(s.arm, Entry3D::Plunge | Entry3D::Arc) && !said.refused {
            b.note(format!(
                "entry: the {} needs bounded air only a bore (Side::Inside) \
                 has - grazing dive at the lead-in, unchanged",
                word(s.arm)
            ));
            said.refused = true;
        }
        let cur = b.at();
        b.push(MoveKind::Rapid, Point3::new(e2.x, e2.y, cur.z), None);
        b.push(MoveKind::Plunge, Point3::new(e2.x, e2.y, z), Some(p.plunge_feed));
        None
    }
}
