//! Issue #96: law (d) judges a vertical rapid DOWN by its column - a
//! drill's rapid onto an R plane under the stock top crosses uncut
//! stock and is a rapid in stock; a peck's re-entry above its own
//! earlier plunge, a rapid stopping above the stock top, and a retract
//! up the tool's own kerf stay clean.
//!
//! Provenance: our own fixtures, specced from the Prime Directive and the
//! shop rule "never traverse through uncut stock"; no vendor material.
//!
//! Gate map (#96): covers camcraft_sim::verify (rapids, rest).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

fn rect() -> Region {
    let l = Loop::new(vec![
        Point2::new(-40.0, -40.0),
        Point2::new(40.0, -40.0),
        Point2::new(40.0, 40.0),
        Point2::new(-40.0, 40.0),
    ]);
    match l.and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("rect: {e:?}"),
    }
}

fn mv(kind: MoveKind, z0: f64, z1: f64) -> Move {
    Move {
        kind,
        from: Point3::new(0.0, 0.0, z0),
        to: Point3::new(0.0, 0.0, z1),
        feed: if kind == MoveKind::Rapid { None } else { Some(80.0) },
        arc: None,
        axis: None,
    }
}

/// `moves` on a drill over a stock box whose top is Z 25, clearance 35.
fn judge(moves: Vec<Move>) -> Vec<Warning> {
    let p = Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), probes: Vec::new() };
    let region = rect();
    let tool = ToolGeom {
        radius: 2.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 2.0,
        shank_len: 20.0,
        holder_radius: 2.0,
        holder_len: 1.0,
    };
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 25.0,
        clearance_z: 35.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
    };
    verify(&p, &cx)
}

#[test]
fn a_vertical_rapid_down_to_an_r_plane_under_the_stock_top_is_a_rapid_in_stock() {
    let w = judge(vec![
        mv(MoveKind::Rapid, 35.0, 20.0),
        mv(MoveKind::Plunge, 20.0, 12.0),
        mv(MoveKind::Rapid, 12.0, 35.0),
    ]);
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }], "{w:?}");
}

#[test]
fn the_same_drill_with_its_r_plane_over_the_stock_top_is_clean() {
    let w = judge(vec![
        mv(MoveKind::Rapid, 35.0, 26.0),
        mv(MoveKind::Plunge, 26.0, 12.0),
        mv(MoveKind::Rapid, 12.0, 35.0),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_peck_re_entry_above_its_own_earlier_plunge_is_clean() {
    let w = judge(vec![
        mv(MoveKind::Rapid, 35.0, 26.0),
        mv(MoveKind::Plunge, 26.0, 18.0),
        mv(MoveKind::Rapid, 18.0, 26.0),
        mv(MoveKind::Rapid, 26.0, 18.5),
        mv(MoveKind::Plunge, 18.5, 12.0),
        mv(MoveKind::Rapid, 12.0, 35.0),
    ]);
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_peck_re_entry_past_its_own_earlier_plunge_is_a_rapid_in_stock() {
    let w = judge(vec![
        mv(MoveKind::Rapid, 35.0, 26.0),
        mv(MoveKind::Plunge, 26.0, 18.0),
        mv(MoveKind::Rapid, 18.0, 26.0),
        mv(MoveKind::Rapid, 26.0, 16.0),
        mv(MoveKind::Plunge, 16.0, 12.0),
        mv(MoveKind::Rapid, 12.0, 35.0),
    ]);
    assert_eq!(w, vec![Warning::RapidInStock { at: 3 }], "{w:?}");
}
