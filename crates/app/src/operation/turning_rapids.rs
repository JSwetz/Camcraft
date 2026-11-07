//! Issue #96: the turned family's axial rapids stay the turning kernel's traverse promise, not the mill column's verdict - our own reading of the sim crate's rapid law (d) on a lathe path (the `Mode::Revolve` doctrine); no vendor material consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::verify::Warning;

use super::kinds::Section;
use super::OpParams;

/// `warns` without the rapid law's convictions of a turned path's
/// AXIAL rapids (a rapid with no XY travel). Since #96 law (d) judges a
/// vertical rapid down by its XY column - right for a mill, whose
/// spindle is the column. A lathe path stands its spindle axis on
/// world Z and its insert is a point in the meridian plane: a rapid
/// along the axis beside the bar is the lathe's traverse, and the
/// column over that point holds the blank's top all the way down (the
/// stock box, the revolved bar), whatever radius the earlier rungs
/// or the part itself leave there. The column cannot see the meridian,
/// so its verdict on that move is no verdict; the turned family keeps
/// exactly the reading it had before #96 (lateral rapids still judged,
/// the kernel's own approach laws still asked). Every other family
/// keeps every conviction.
pub(super) fn drop_axial_rapids(params: &OpParams, path: &Toolpath, z_tol: f64, warns: &mut Vec<Warning>) {
    if params.kind().info().section != Section::Turning {
        return;
    }
    warns.retain(|w| {
        let Warning::RapidInStock { at } = *w else { return true };
        let Some(m) = path.moves.get(at) else { return true };
        let (dx, dy) = (m.to.x - m.from.x, m.to.y - m.from.y);
        !(m.kind == MoveKind::Rapid && dx * dx + dy * dy <= z_tol * z_tol)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::Move;
    use crate::geometry::Point3;
    use crate::operation::Height;

    fn rapid(from: Point3, to: Point3) -> Move {
        Move { kind: MoveKind::Rapid, from, to, feed: None, arc: None, axis: None }
    }

    #[test]
    fn a_turned_path_keeps_its_sideways_conviction_and_drops_only_the_axial_one() {
        let params = OpParams::ProfileRough {
            tolerance: 0.05,
            clearance_z: Height::fixed(25.0),
            top_z: Height::fixed(16.0),
            bottom_z: Height::fixed(-16.0),
            stepdown: 0.8,
            stock_to_leave: 0.25,
            feed: 150.0,
            plunge_feed: 80.0,
        };
        let path = Toolpath {
            moves: vec![
                // Axial: straight down the spindle axis beside the bar.
                rapid(Point3::new(20.0, 0.0, 25.0), Point3::new(20.0, 0.0, 10.0)),
                // Sideways: a radial rapid in toward the axis.
                rapid(Point3::new(20.0, 0.0, 10.0), Point3::new(5.0, 0.0, 10.0)),
            ],
            cycles: Vec::new(),
            notes: Vec::new(),
            comps: Vec::new(),
            probes: Vec::new(),
        };
        let mut warns = vec![Warning::RapidInStock { at: 0 }, Warning::RapidInStock { at: 1 }];
        drop_axial_rapids(&params, &path, 1e-6, &mut warns);
        assert_eq!(warns, vec![Warning::RapidInStock { at: 1 }]);
    }
}
