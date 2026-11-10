//! The fixture laws (c2)/(d2) over workholding that does and does not
//! turn (M13.8 (2)): a chuck rides the rotary with the part, so its box
//! is judged where the path is written - the part's own frame. A table
//! or a tailstock stands still while the part turns; judged in the
//! part frame it swings round with every index, a phantom crash below
//! a bar the tool only ever meets from above. Fixed boxes are judged in
//! the MACHINE frame instead: each posed sample carried through the
//! rotary turn that stands its pose upright ([`super::posed_steel`]).
//!
//! The law only ever REMOVES a part-frame verdict where the machine
//! frame is stated for every sample of the move and the sweep there is
//! clear: an upright move (rotary at zero - both frames agree), a pose
//! the rotary alone cannot stand upright, or no axis stated keep the
//! part-frame laws byte-identical. The turning boxes keep theirs
//! untouched. Under the spindle C (#486, [`super::c_steel`]) every
//! box is read at the station pose the C states, and fixed boxes are
//! read in the machine frame as well - that reading only ADDS hits.
//!
//! Provenance: rigid rotation of a rotary work about its axis line is
//! textbook machine kinematics (RS-274 rotary addresses; 4-axis
//! machine-tool handbooks). Our own design; no vendor source consulted.

use crate::cl::{MoveKind, Toolpath};
use crate::fixture::FixtureBox;
use crate::tools::ToolGeom;
use camcraft_tools::holder_profile::HolderStack;

use super::c_steel::{fixed_hits, merge, station_poses};
use super::laws::{bands_vs_fixtures_held, rapids_vs_fixtures_held};
use super::posed_steel::{sweep, Verdict};
use super::rotary::RotaryAxis;
use super::types::{FixturePart, Warning};

/// The setup's steel, split by whether it turns with the part.
#[derive(Clone, Copy, Debug)]
pub struct Workholding<'a> {
    /// Rides the rotary (a chuck, a fixture plate on the trunnion):
    /// judged in the path frame, exactly as [`bands_vs_fixtures`].
    pub turning: &'a [FixtureBox],
    /// Stands still while the part turns (the table, a tailstock),
    /// stated at rotary zero - where the path frame and the machine
    /// frame coincide.
    pub fixed: &'a [FixtureBox],
    /// The rotary's axis line in the path frame. None: no machine
    /// frame can be stated, so the fixed boxes are judged as turning
    /// (the part-frame verdict, as before M13.8 (2)).
    pub axis: Option<RotaryAxis>,
}

/// Law (c2) over split workholding: every non-rapid move.
#[must_use]
pub fn bands_vs_workholding(
    path: &Toolpath,
    tool: &ToolGeom,
    wh: &Workholding<'_>,
    spindle: Option<(f64, f64)>,
    chord_tol: f64,
) -> Vec<Warning> {
    bands_vs_workholding_held(path, tool, &HolderStack::cylinder(tool), wh, spindle, chord_tol)
}

/// [`bands_vs_workholding`] with what stands above the shank stated
/// (#368): a modelled body's bands in place of the holder cylinder.
#[must_use]
pub fn bands_vs_workholding_held(
    path: &Toolpath,
    tool: &ToolGeom,
    holder: &HolderStack,
    wh: &Workholding<'_>,
    spindle: Option<(f64, f64)>,
    chord_tol: f64,
) -> Vec<Warning> {
    // Under the spindle C (#486): the pose the machine holds, and the
    // fixed steel read in the machine frame too ([`super::c_steel`]).
    let path = &*station_poses(path);
    let warns = judge(
        path,
        wh,
        false,
        |boxes| bands_vs_fixtures_held(path, tool, holder, boxes, spindle, chord_tol),
        |boxes, ax| sweep(path, tool, holder, boxes, spindle, chord_tol, false, Some(ax)),
    );
    merge(warns, &fixed_hits(path, tool, holder, wh.fixed, wh.axis.as_ref(), spindle, chord_tol, false))
}

/// Law (d2) over split workholding: every rapid, the spindle housing
/// swept along a posed rapid when the machine states it.
#[must_use]
pub fn rapids_vs_workholding(
    path: &Toolpath,
    tool: &ToolGeom,
    wh: &Workholding<'_>,
    spindle: Option<(f64, f64)>,
) -> Vec<Warning> {
    rapids_vs_workholding_held(path, tool, &HolderStack::cylinder(tool), wh, spindle)
}

/// [`rapids_vs_workholding`] with what stands above the shank stated
/// (#368).
#[must_use]
pub fn rapids_vs_workholding_held(
    path: &Toolpath,
    tool: &ToolGeom,
    holder: &HolderStack,
    wh: &Workholding<'_>,
    spindle: Option<(f64, f64)>,
) -> Vec<Warning> {
    let path = &*station_poses(path);
    let warns = judge(
        path,
        wh,
        true,
        |boxes| rapids_vs_fixtures_held(path, tool, holder, boxes, spindle),
        |boxes, ax| sweep(path, tool, holder, boxes, spindle, 0.01, true, Some(ax)),
    );
    merge(warns, &fixed_hits(path, tool, holder, wh.fixed, wh.axis.as_ref(), spindle, 0.01, true))
}

/// The merge: per move the turning boxes' verdict first; else the
/// fixed boxes' - the machine-frame sweep where it judged the move,
/// the part-frame law where it could not. One report per move, move
/// order.
fn judge(
    path: &Toolpath,
    wh: &Workholding<'_>,
    rapids: bool,
    part_frame: impl Fn(&[FixtureBox]) -> Vec<Warning>,
    machine_frame: impl Fn(&[FixtureBox], &RotaryAxis) -> Vec<Verdict>,
) -> Vec<Warning> {
    let Some(ax) = wh.axis.filter(|_| !wh.fixed.is_empty()) else {
        let all: Vec<FixtureBox> = wh.turning.iter().chain(wh.fixed).copied().collect();
        return part_frame(&all);
    };
    let n = path.moves.len();
    let mut out: Vec<Option<FixturePart>> = vec![None; n];
    let put = |ws: Vec<Warning>, slot: &mut Vec<Option<FixturePart>>| {
        for w in ws {
            if let Warning::FixtureHit { at, fixture_part } = w
                && let Some(s) = slot.get_mut(at)
                && s.is_none()
            {
                *s = Some(fixture_part);
            }
        }
    };
    put(part_frame(wh.turning), &mut out);
    let mut fixed_old: Vec<Option<FixturePart>> = vec![None; n];
    put(part_frame(wh.fixed), &mut fixed_old);
    let machine = machine_frame(wh.fixed, &ax);
    for (i, m) in path.moves.iter().enumerate() {
        if out[i].is_some() || (m.kind == MoveKind::Rapid) != rapids {
            continue;
        }
        out[i] = match machine.get(i).copied().unwrap_or(Verdict::Unjudged) {
            Verdict::Hit(p) => Some(p),
            Verdict::Clear => None,
            Verdict::NotPosed | Verdict::Unjudged => fixed_old[i],
        };
    }
    out.into_iter()
        .enumerate()
        .filter_map(|(at, p)| p.map(|fixture_part| Warning::FixtureHit { at, fixture_part }))
        .collect()
}
