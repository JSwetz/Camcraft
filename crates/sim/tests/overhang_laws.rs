//! M13.7 B: uncut stock under an overhang convicts the reach law (c),
//! its mid-turn twin (c3) and the rapid law (d). A plane section at the
//! band's or the rapid's height shows only the post under a ledge, never
//! the stock the ledge hides from a vertical tool - so a necked shank or
//! a short-flute tool's rapid sliding under the ledge through that stock
//! was silent. The same moves above the ledge, or beside a part without
//! one, stay clean.
//!
//! Provenance: our own fixtures, specced from the Prime Directive and the
//! shop rule "never traverse through uncut stock"; no vendor material.
//!
//! M13.7 cut order: a move is judged against the cuts BEFORE it - a
//! rapid through stock a later feed removes convicts, column law and
//! hidden-stock backstop alike.
//!
//! Gate map (M13.7 B, cut order): covers camcraft_sim::verify (rest,
//! cut_index, hidden, reach, sweep, rapids).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::{Point3, Triangle, cube};
use camcraft_sim::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let l = Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ]);
    match l.and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("rect: {e:?}"),
    }
}

fn mv(kind: MoveKind, from: Point3, to: Point3) -> Move {
    Move {
        kind,
        from,
        to,
        feed: if kind == MoveKind::Rapid { None } else { Some(500.0) },
        arc: None,
        axis: None,
    }
}

fn path(moves: Vec<Move>) -> Toolpath {
    Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new() }
}

/// A post X/Y [-5, 5], Z [0, 10] carrying a ledge X [-5, 15], Y [-5, 5],
/// Z [10, 12]: the ledge overhangs X (5, 15] - stock under it is hidden
/// from above. `ledge = false` is the bare post.
fn part(ledge: bool) -> Vec<Triangle> {
    let mut f: Vec<Triangle> = cube(Point3::new(-5.0, -5.0, 0.0), Point3::new(5.0, 5.0, 10.0)).facets.to_vec();
    if ledge {
        f.extend(cube(Point3::new(-5.0, -5.0, 10.0), Point3::new(15.0, 5.0, 12.0)).facets.iter().cloned());
    }
    f
}

/// Check `p` against the part with a stock box Z top 20 and `tool`.
fn judge(p: &Toolpath, facets: &[Triangle], tool: &ToolGeom) -> Vec<Warning> {
    let region = rect(-40.0, -40.0, 60.0, 60.0);
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: Some(facets),
        tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 20.0,
        clearance_z: 30.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
    };
    verify(p, &cx)
}

/// A short-flute tool with a NARROW neck: its 4 mm flute disc can sit
/// under the ledge while the neck stands clear of it - the only stack
/// whose body the solid law (b2) does not already convict there.
fn necked() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 4.0,
        shank_radius: 1.5,
        shank_len: 40.0,
        holder_radius: 1.5,
        holder_len: 10.0,
    }
}

/// A tool whose WIDE shank band Z [cut + 4, cut + 8] stays under the
/// ledge's underside at a floor cut; its neck-thin holder is not a band.
fn wide_shank() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 4.0,
        shank_radius: 6.0,
        shank_len: 4.0,
        holder_radius: 1.5,
        holder_len: 1.0,
    }
}

/// Floor cuts at X 17 on both sides of the ledge (the far side first,
/// reached over the clearance plane), then a lateral rapid at `z` along
/// X 17 from one cleared column to the other: the disc X [14, 20]
/// crosses the hidden band X [14, 15] under the ledge. The rapid's three
/// column samples are all cleared BEFORE it (M13.7 cut order - the old
/// fixture cut the far side after the rapid, and only the order-blind
/// rest model called that column clear), so the section law alone stays
/// silent. The judged rapid is move [`RAPID`].
fn rapid_under(z: f64) -> Toolpath {
    path(rapid_under_moves(z))
}

/// The judged lateral rapid's index in [`rapid_under`].
const RAPID: usize = 6;

fn rapid_under_moves(z: f64) -> Vec<Move> {
    vec![
        mv(MoveKind::Feed, Point3::new(17.0, 45.0, 0.0), Point3::new(17.0, 8.5, 0.0)),
        mv(MoveKind::Rapid, Point3::new(17.0, 8.5, 0.0), Point3::new(17.0, 8.5, 30.0)),
        mv(MoveKind::Rapid, Point3::new(17.0, 8.5, 30.0), Point3::new(17.0, -25.0, 30.0)),
        mv(MoveKind::Feed, Point3::new(17.0, -25.0, 30.0), Point3::new(17.0, -25.0, 0.0)),
        mv(MoveKind::Feed, Point3::new(17.0, -25.0, 0.0), Point3::new(17.0, -8.5, 0.0)),
        mv(MoveKind::Rapid, Point3::new(17.0, -8.5, 0.0), Point3::new(17.0, -8.5, z)),
        mv(MoveKind::Rapid, Point3::new(17.0, -8.5, z), Point3::new(17.0, 45.0, z)),
        mv(MoveKind::Rapid, Point3::new(17.0, 45.0, z), Point3::new(17.0, 45.0, 0.0)),
    ]
}

/// A floor cut at height `z` along X 19 up to Y 0: the shank annulus
/// (3, 6] about its end reaches X 13 - under the ledge at a floor cut.
fn shank_under(z: f64, x: f64) -> Toolpath {
    path(vec![mv(MoveKind::Feed, Point3::new(x, -25.0, z), Point3::new(x, 0.0, z))])
}

#[test]
fn a_rapid_under_the_ledge_through_hidden_stock_is_a_rapid_in_stock() {
    let w = judge(&rapid_under(5.0), &part(true), &necked());
    assert_eq!(w, vec![Warning::RapidInStock { at: RAPID }], "{w:?}");
}

#[test]
fn the_same_rapid_above_the_ledge_is_clean() {
    let w = judge(&rapid_under(12.5), &part(true), &necked());
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn the_same_rapid_beside_a_post_without_a_ledge_is_clean() {
    let w = judge(&rapid_under(5.0), &part(false), &necked());
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_shank_under_the_ledge_in_hidden_stock_is_a_shank_hit() {
    let w = judge(&shank_under(0.0, 19.0), &part(true), &wide_shank());
    assert_eq!(w, vec![Warning::ShankHit { at: 0, part: false }], "{w:?}");
}

#[test]
fn the_same_shank_above_the_ledge_is_clean() {
    let w = judge(&shank_under(12.5, 19.0), &part(true), &wide_shank());
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_shank_whose_annulus_misses_the_hidden_stock_is_clean() {
    // X 22: the annulus reaches X 16, past the ledge's edge at X 15.
    let w = judge(&shank_under(0.0, 22.0), &part(true), &wide_shank());
    assert!(w.is_empty(), "{w:?}");
    // And the bare post never hides anything.
    let w = judge(&shank_under(0.0, 19.0), &part(false), &wide_shank());
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_2d_path_without_a_model_is_unchanged() {
    // No model: no shadow to read, the section laws alone speak.
    let region = rect(-40.0, -40.0, 60.0, 60.0);
    let tool = necked();
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 20.0,
        clearance_z: 30.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
    };
    let w = verify(&rapid_under(5.0), &cx);
    assert!(w.is_empty(), "{w:?}");
}

// --- M13.7 cut order: a move is judged against the cuts BEFORE it ---

/// The no-model context of [`a_2d_path_without_a_model_is_unchanged`].
fn judge_2d(p: &Toolpath, tool: &ToolGeom) -> Vec<Warning> {
    let region = rect(-40.0, -40.0, 60.0, 60.0);
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: None,
        tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 20.0,
        clearance_z: 30.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
    };
    verify(p, &cx)
}

#[test]
fn a_rapid_through_stock_a_later_feed_removes_is_a_rapid_in_stock() {
    // The rapid at Z 5 slides through full stock; the feed that clears
    // its columns down to Z 5 comes after it. An order-blind rest model
    // read those columns as cleared and stayed silent.
    let p = path(vec![
        mv(MoveKind::Rapid, Point3::new(0.0, 0.0, 5.0), Point3::new(20.0, 0.0, 5.0)),
        mv(MoveKind::Feed, Point3::new(20.0, 0.0, 5.0), Point3::new(0.0, 0.0, 5.0)),
    ]);
    let w = judge_2d(&p, &necked());
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }], "{w:?}");
}

#[test]
fn the_same_rapid_after_the_feed_is_clean() {
    let p = path(vec![
        mv(MoveKind::Feed, Point3::new(0.0, 0.0, 5.0), Point3::new(20.0, 0.0, 5.0)),
        mv(MoveKind::Rapid, Point3::new(20.0, 0.0, 5.0), Point3::new(0.0, 0.0, 5.0)),
    ]);
    let w = judge_2d(&p, &necked());
    assert!(w.is_empty(), "{w:?}");
}

/// The floor pass along X 17 the whole way under the ledge: its disc
/// X [14, 20] clears the hidden band X [14, 15] the judged rapid crosses.
fn clearing_pass(from_y: f64, to_y: f64) -> Move {
    mv(MoveKind::Feed, Point3::new(17.0, from_y, 0.0), Point3::new(17.0, to_y, 0.0))
}

#[test]
fn a_rapid_under_the_ledge_before_the_clearing_pass_is_a_rapid_in_stock() {
    // The pass comes after the rapid: the hidden stock still stands when
    // the rapid slides through it - the backstop's own cut-order clause.
    let mut moves = rapid_under_moves(5.0);
    moves.push(clearing_pass(45.0, -25.0));
    let w = judge(&path(moves), &part(true), &necked());
    assert_eq!(w, vec![Warning::RapidInStock { at: RAPID }], "{w:?}");
}

#[test]
fn the_same_rapid_after_the_clearing_pass_is_clean() {
    let mut moves = vec![clearing_pass(-25.0, 45.0)];
    moves.extend(rapid_under_moves(5.0));
    let w = judge(&path(moves), &part(true), &necked());
    assert!(w.is_empty(), "{w:?}");
}
