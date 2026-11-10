//! Lathe tool orientation (#630): where a nose-radiused insert's
//! CONTROL POINT stands relative to its nose circle's centre, numbered
//! 1-9 the way the LinuxCNC tool table numbers it.
//!
//! The control point is the point a control drives along the
//! programmed path when nose compensation is off (G40): the
//! intersection of the X-parallel and Z-parallel lines tangent to the
//! nose circle - the "imaginary tip" the operator touches the tool off
//! on (LinuxCNC Lathe User Information, section 7.1 "Control point",
//! and its section 3 "Lathe Tool Orientation" figure). Read off that
//! figure, in the control's own X (radius) and Z coordinates, the tip
//! stands at `centre + r * tip_offset(q)`:
//!
//! | q | tip offset (X, Z) | the tool, read in program words |
//! |---|---|---|
//! | 1 | (-1, +1) | OD side, cutting away from the chuck (+Z) |
//! | 2 | (-1, -1) | OD side, cutting toward the chuck (-Z): a right-hand turning tool |
//! | 3 | (+1, -1) | ID side, cutting toward the chuck: a boring bar |
//! | 4 | (+1, +1) | ID side, cutting away from the chuck |
//! | 5 | ( 0, +1) | pointing +Z (X is read at the nose centre) |
//! | 6 | (-1,  0) | pointing -X, neutral (Z is read at the nose centre) |
//! | 7 | ( 0, -1) | pointing -Z |
//! | 8 | (+1,  0) | pointing +X |
//! | 9 | ( 0,  0) | the nose centre itself (touched off on the centre) |
//!
//! The numbering is stated in X/Z coordinates, so it does not change
//! with the turret side: the figure is drawn for a front tool post
//! (X+ toward the operator) and a rear turret reads the same numbers
//! with its own X+. Fanuc-style "imaginary tool nose" tip numbers are
//! a different chart (the usual OD tool is tip 3 there); no post of
//! ours writes either number to the control - they post G40 and the
//! control point itself - so the LinuxCNC numbering is the one the
//! user edits and the PathPilot tool table checks against.
//!
//! What an orientation can cut: an insert presents its cutting nose
//! arc around its tip direction. A corner orientation (1-4) cuts with
//! the quarter of the nose between its two tangent lines - a contact
//! normal within 45 degrees of the tip direction; an axis orientation
//! (5-8) presents the half of the nose facing its point - within 90
//! degrees; 9 says nothing about the insert and refuses nothing. This
//! is the reading under which PathPilot's conversational refusal
//! ("the X and Z inputs call for orientation [1, 5, 6]") is
//! reproduced for a cut needing the -X and +Z contacts.
//!
//! Blades (#641): a grooving/parting blade has no nose circle - its
//! CL is the middle of its tip edge (the band's centre at the tip
//! line), and the operator touches it off on one CORNER of that edge
//! (LinuxCNC Lathe User Information section 4.2 works its parting-tool
//! example from the left side). The same chart names the corner: the
//! blade plunges toward the axis, so its tip is on the -X side and
//! only the Z sign is left to say -
//!
//! | q | the blade's control point |
//! |---|---|
//! | 1 | the +Z corner of the tip edge (right, toward the free end) |
//! | 2 | the -Z corner (left, toward the chuck) - the default |
//! | 6 | the middle of the tip edge (touched off on the width centre) |
//! | 9 | read as 6 (a blade states no nose circle to centre on) |
//!
//! Every other code states an ID or a face blade, which no OD groove or
//! part-off rides: [`blade_corner`] says None and the post refuses it by
//! name. A radiused corner changes nothing: the imaginary tip of a
//! rounded corner is where its tip line and flank line meet - the
//! sharp rectangle's corner, `width / 2` from the edge's middle.
//!
//! Provenance: LinuxCNC Lathe User Information (public docs, sections
//! 3, 4.1, 4.2 and 7.1) for the numbering and the control point; machine-
//! tool handbook practice for nose-radius compensation. Behaviour
//! only - no vendor text or post consulted.

use crate::tools::ToolKind;

/// The tool table's centre code: the control point IS the nose centre.
pub const CENTRE: u8 = 9;

/// A contact normal the cut needs the insert's nose to present, in the
/// control's X (radius, + away from the axis) and Z (+ away from the
/// chuck) - the direction from the nose centre to the material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Contact {
    PlusX,
    MinusX,
    PlusZ,
    MinusZ,
}

impl Contact {
    fn vector(self) -> (f64, f64) {
        match self {
            Contact::PlusX => (1.0, 0.0),
            Contact::MinusX => (-1.0, 0.0),
            Contact::PlusZ => (0.0, 1.0),
            Contact::MinusZ => (0.0, -1.0),
        }
    }
}

impl core::fmt::Display for Contact {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Contact::PlusX => "+X",
            Contact::MinusX => "-X",
            Contact::PlusZ => "+Z",
            Contact::MinusZ => "-Z",
        })
    }
}

/// The unit offset from the nose centre to the control point for
/// orientation `q`, in (radius X, Z) - multiply by the nose radius.
/// None for anything outside 1..=9.
#[must_use]
pub fn tip_offset(q: u8) -> Option<(f64, f64)> {
    Some(match q {
        1 => (-1.0, 1.0),
        2 => (-1.0, -1.0),
        3 => (1.0, -1.0),
        4 => (1.0, 1.0),
        5 => (0.0, 1.0),
        6 => (-1.0, 0.0),
        7 => (0.0, -1.0),
        8 => (1.0, 0.0),
        9 => (0.0, 0.0),
        _ => return None,
    })
}

/// Whether an orientation is one the tool table knows.
#[must_use]
pub fn is_valid(q: u8) -> bool {
    (1..=9).contains(&q)
}

/// Can an insert of orientation `q` present a nose contact with normal
/// `c`? See the module law: 45 degrees about a corner tip, 90 about an
/// axis tip, anything at the centre code.
#[must_use]
pub fn covers(q: u8, c: Contact) -> bool {
    let Some((ox, oz)) = tip_offset(q) else { return false };
    let len = (ox * ox + oz * oz).sqrt();
    if len == 0.0 {
        return true;
    }
    let (nx, nz) = c.vector();
    let cos = (ox * nx + oz * nz) / len;
    // Corner tips (|offset| = sqrt 2) reach 45 degrees, axis tips 90.
    let limit = if len > 1.5 { core::f64::consts::FRAC_1_SQRT_2 } else { 0.0 };
    cos >= limit - 1e-9
}

/// The orientations 1-8 that present every contact in `need` - the
/// list a refusal names. (9 is left out: it states no insert.)
#[must_use]
pub fn fitting(need: &[Contact]) -> Vec<u8> {
    (1..=8u8).filter(|q| need.iter().all(|c| covers(*q, *c))).collect()
}

/// The orientation a kind takes when the library states none: the
/// right-hand OD turning tool cutting toward the chuck (2) for a
/// turning insert, the boring bar cutting toward the chuck (3) for a
/// boring bar, the left (-Z, chuck-side) corner (2) for a grooving or
/// parting blade (#641). None for every kind whose posted point is not
/// a touched-off tip (the mills, threading V, form gauges, cycle
/// tools).
#[must_use]
pub fn default_for(kind: ToolKind) -> Option<u8> {
    match kind {
        ToolKind::Turning | ToolKind::Groove => Some(2),
        ToolKind::Bore => Some(3),
        _ => None,
    }
}

/// The codes an OD grooving/parting blade can be touched off as
/// (#641): its +Z corner, its -Z corner, its width centre.
pub const BLADE_FITTING: [u8; 3] = [1, 2, 6];

/// Which way a blade's control point stands from the middle of its tip
/// edge, in half-widths along Z: +1 the +Z corner, -1 the -Z corner,
/// 0 the middle. None for a code that states no OD blade.
#[must_use]
pub fn blade_corner(q: u8) -> Option<f64> {
    match q {
        1 => Some(1.0),
        2 => Some(-1.0),
        6 | 9 => Some(0.0),
        _ => None,
    }
}

/// A short label for the editor's list, read for a blade.
#[must_use]
pub fn blade_label(q: u8) -> &'static str {
    match q {
        1 => "1 - right corner (+Z, toward the free end)",
        2 => "2 - left corner (-Z, toward the chuck)",
        6 => "6 - width centre",
        9 => "9 - width centre",
        _ => "not an OD blade",
    }
}

/// A short label for the editor's list.
#[must_use]
pub fn label(q: u8) -> &'static str {
    match q {
        1 => "1 - OD, cuts away from the chuck",
        2 => "2 - OD, cuts toward the chuck",
        3 => "3 - ID, cuts toward the chuck",
        4 => "4 - ID, cuts away from the chuck",
        5 => "5 - points +Z",
        6 => "6 - points -X (neutral OD)",
        7 => "7 - points -Z",
        8 => "8 - points +X (neutral ID)",
        9 => "9 - nose centre",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corner_tips_sit_on_both_tangent_lines() {
        // Orientation 2: an OD tool toward the chuck touches the
        // diameter at -X and the face at -Z of its nose.
        assert_eq!(tip_offset(2), Some((-1.0, -1.0)));
        assert_eq!(tip_offset(3), Some((1.0, -1.0)));
        assert_eq!(tip_offset(9), Some((0.0, 0.0)));
        assert_eq!(tip_offset(0), None);
        assert_eq!(tip_offset(10), None);
    }

    #[test]
    fn the_pathpilot_listing_is_reproduced() {
        // A cut needing the -X and +Z contacts fits 1, 5 and 6 only.
        assert_eq!(fitting(&[Contact::MinusX, Contact::PlusZ]), vec![1, 5, 6]);
        // An OD diameter alone: every tip with a -X reach.
        assert_eq!(fitting(&[Contact::MinusX]), vec![1, 2, 5, 6, 7]);
        // A bore wall: the +X family.
        assert_eq!(fitting(&[Contact::PlusX]), vec![3, 4, 5, 7, 8]);
        // Facing inward: the face below and the pass's leading -X.
        assert_eq!(fitting(&[Contact::MinusX, Contact::MinusZ]), vec![2, 6, 7]);
    }

    #[test]
    fn defaults_are_the_lathe_kinds_only() {
        assert_eq!(default_for(ToolKind::Turning), Some(2));
        assert_eq!(default_for(ToolKind::Bore), Some(3));
        assert_eq!(default_for(ToolKind::EndMill), None);
        assert_eq!(default_for(ToolKind::Groove), Some(2), "the left corner (#641)");
        assert_eq!(default_for(ToolKind::Thread), None);
        assert!(covers(9, Contact::PlusX) && covers(9, Contact::MinusZ));
    }

    #[test]
    fn a_blade_names_its_corner_by_the_chart() {
        assert_eq!(blade_corner(1), Some(1.0));
        assert_eq!(blade_corner(2), Some(-1.0));
        assert_eq!(blade_corner(6), Some(0.0));
        assert_eq!(blade_corner(9), Some(0.0));
        for q in [0, 3, 4, 5, 7, 8, 10] {
            assert_eq!(blade_corner(q), None, "{q} states no OD blade");
        }
        assert!(BLADE_FITTING.iter().all(|q| blade_corner(*q).is_some()));
    }
}
