//! The all-air finish note (M13.8): our own wiring of the sim crate's `verify::cuts_no_material` verdict onto a generated 3D finish; no vendor material consulted.

use crate::cl::Toolpath;
use crate::geometry::Triangle;
use crate::tools::ToolGeom;
use crate::verify::{self, Mode};

use super::material::GOUGE_TOL;

/// The note an all-air finish carries. The envelope laws speak only
/// where the swept ball meets the part, so a finish whose every cut
/// stands above all material verifies silent - and that silence must
/// not read as a finished surface. Not a warning: nothing unsafe
/// happens, so it does not hold the post; it says by name that the op
/// removes nothing.
pub const AIR_NOTE: &str = "finish cuts no material: every cutting move stands above all stock \
                            and part material - check the op's heights";

/// `path` with [`AIR_NOTE`] added when it is an Envelope (3D finish)
/// path that `verify::cuts_no_material` proves clean of all material:
/// the stock top and the part's highest vertex, plus the gouge
/// tolerance. Any path it cannot prove is returned untouched - its
/// verdict stays with the laws that judged it.
pub(super) fn air_noted(
    mut path: Toolpath,
    mode: Mode,
    facets: &[Triangle],
    stock_top_z: f64,
    geom: &ToolGeom,
) -> Toolpath {
    if mode == Mode::Envelope
        && verify::cuts_no_material(&path, Some(facets), stock_top_z, geom, GOUGE_TOL)
        && !path.notes.iter().any(|n| n == AIR_NOTE)
    {
        path.notes.push(AIR_NOTE.to_owned());
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{Move, MoveKind};
    use crate::geometry::Point3;

    fn geom() -> ToolGeom {
        ToolGeom {
            radius: 3.0,
            nose_radius: 0.0,
            flute_len: 20.0,
            shank_radius: 3.0,
            shank_len: 30.0,
            holder_radius: 15.0,
            holder_len: 40.0,
        }
    }

    /// One part facet whose top vertex stands at Z 10.
    fn part() -> Vec<Triangle> {
        let p = Point3::new;
        vec![Triangle::new(p(0.0, 0.0, 0.0), p(10.0, 0.0, 0.0), p(0.0, 10.0, 10.0))]
    }

    fn pass(z: f64) -> Toolpath {
        let p = Point3::new;
        Toolpath::new(vec![
            Move::new(MoveKind::Rapid, p(0.0, 0.0, 30.0), p(0.0, 0.0, z), None),
            Move::new(MoveKind::Feed, p(0.0, 0.0, z), p(10.0, 0.0, z), Some(500.0)),
        ])
    }

    #[test]
    fn an_all_air_finish_carries_the_note_once_and_a_cutting_one_none() {
        let f = part();
        // Above the part top (10) and the stock top (12): noted, once
        // even when re-run on an already noted path.
        let t = air_noted(pass(15.0), Mode::Envelope, &f, 12.0, &geom());
        assert_eq!(t.notes, vec![AIR_NOTE.to_owned()]);
        let t = air_noted(t, Mode::Envelope, &f, 12.0, &geom());
        assert_eq!(t.notes.len(), 1);
        // Between the part top and the stock top: it may cut stock, so
        // no proof and no note.
        assert!(air_noted(pass(11.0), Mode::Envelope, &f, 12.0, &geom()).notes.is_empty());
        // Down at the part: it cuts.
        assert!(air_noted(pass(5.0), Mode::Envelope, &f, 12.0, &geom()).notes.is_empty());
        // Not a finish: the other kinds' air is their own laws' matter.
        assert!(air_noted(pass(15.0), Mode::Boundary, &f, 12.0, &geom()).notes.is_empty());
    }
}
