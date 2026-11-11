//! #672: the plasma through-cut laws - cut order, kerf crossing, tip-up,
//! and malformed programs - each tripped by one fixture and passed by
//! another.
//!
//! Gate map (#672): covers sim::verify::plasma.
//! Provenance: our own fixtures.

use camcraft_sim::geo2d::Point2;
use camcraft_sim::verify::plasma::{judge, CutRole, PlasmaCut, PlasmaFinding, PlasmaProgram};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// A closed square, from its lower-left corner, side `s`.
fn square(x: f64, y: f64, s: f64) -> Vec<Point2> {
    vec![p(x, y), p(x + s, y), p(x + s, y + s), p(x, y + s), p(x, y)]
}

fn cut(contour: u32, role: CutRole, parent: Option<u32>, path: Vec<Point2>) -> PlasmaCut {
    PlasmaCut { contour, role, parent, small: false, path }
}

use CutRole::{Inner, Open, Outer};

/// A 100 mm plate part (outer 1) with a 20 mm hole (inner 2) in it.
fn plate(hole_first: bool) -> PlasmaProgram {
    let hole = cut(2, Inner, Some(1), square(40.0, 40.0, 20.0));
    let outer = cut(1, Outer, None, square(0.0, 0.0, 100.0));
    let cuts = if hole_first { vec![hole, outer] } else { vec![outer, hole] };
    PlasmaProgram { kerf: 1.5, cuts, rapids: vec![None, None] }
}

#[test]
fn holes_before_the_outline_pass_clean() {
    assert_eq!(judge(&plate(true)), vec![]);
}

#[test]
fn an_outline_before_its_hole_is_refused_by_name() {
    let f = judge(&plate(false));
    assert_eq!(f, vec![PlasmaFinding::CutOrder { contour: 2, enclosing: 1 }]);
    assert!(f[0].refuses());
}

#[test]
fn a_cut_within_a_kerf_of_an_earlier_one_is_warned() {
    // Two parts side by side, 1 mm apart with a 1.5 mm kerf: the second
    // outline runs inside the first one's open kerf.
    let a = cut(1, Outer, None, square(0.0, 0.0, 50.0));
    let b = cut(2, Outer, None, square(51.0, 0.0, 50.0));
    let f = judge(&PlasmaProgram { kerf: 1.5, cuts: vec![a.clone(), b], rapids: vec![None, None] });
    assert!(matches!(f.as_slice(), [PlasmaFinding::KerfCrossing { contour: 2, earlier: 1, .. }]), "{f:?}");
    assert!(!f[0].refuses(), "a warning, not a refusal");
    // 5 mm apart: clear.
    let c = cut(2, Outer, None, square(55.0, 0.0, 50.0));
    assert_eq!(judge(&PlasmaProgram { kerf: 1.5, cuts: vec![a, c], rapids: vec![None, None] }), vec![]);
}

#[test]
fn a_rapid_over_a_freed_part_is_warned() {
    // Part 1 (0..50) is cut free, then the rapid to part 3 (at x 120)
    // starts from the far side of part 1 and crosses it.
    let a = cut(1, Outer, None, square(0.0, 0.0, 50.0));
    let b = cut(3, Outer, None, square(120.0, 0.0, 30.0));
    let over = Some((p(-10.0, 25.0), p(120.0, 0.0)));
    let f = judge(&PlasmaProgram { kerf: 1.5, cuts: vec![a.clone(), b.clone()], rapids: vec![None, over] });
    assert!(matches!(f.as_slice(), [PlasmaFinding::TipUp { contour: 3, freed: 1, .. }]), "{f:?}");
    // Around it, below the part: clear.
    let around = Some((p(0.0, -10.0), p(120.0, -10.0)));
    assert_eq!(judge(&PlasmaProgram { kerf: 1.5, cuts: vec![a, b], rapids: vec![None, around] }), vec![]);
}

#[test]
fn a_malformed_program_is_refused_by_name() {
    for kerf in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let f = judge(&PlasmaProgram { kerf, ..plate(true) });
        assert!(matches!(f.as_slice(), [PlasmaFinding::Malformed(m)] if m.contains("kerf")), "{kerf}: {f:?}");
    }
    let mut bad = plate(true);
    bad.cuts[0].path[1] = p(f64::NAN, 0.0);
    assert!(matches!(judge(&bad).as_slice(), [PlasmaFinding::Malformed(m)] if m.contains("non-finite")));
    let mut empty = plate(true);
    empty.cuts[1].path.truncate(1);
    assert!(matches!(judge(&empty).as_slice(), [PlasmaFinding::Malformed(m)] if m.contains("no cut path")));
}

/// Review of #675: an island (an outer contour inside a hole) cut after
/// its hole is refused - the hole freed the slug the island sits in.
#[test]
fn an_island_after_its_hole_is_refused() {
    let outline = cut(1, Outer, None, square(0.0, 0.0, 100.0));
    let hole = cut(2, Inner, Some(1), square(30.0, 30.0, 40.0));
    let island = cut(3, Outer, Some(2), square(45.0, 45.0, 10.0));
    let good = PlasmaProgram { kerf: 1.5, cuts: vec![island.clone(), hole.clone(), outline.clone()], rapids: vec![None; 3] };
    assert_eq!(judge(&good), vec![], "island, hole, outline");
    let bad = PlasmaProgram { kerf: 1.5, cuts: vec![hole, island, outline], rapids: vec![None; 3] };
    assert_eq!(judge(&bad), vec![PlasmaFinding::CutOrder { contour: 3, enclosing: 2 }]);
}

/// Review of #675: parents, ids and rapids that can't be judged are
/// refused by name, never silently skipped.
#[test]
fn bad_parents_ids_and_rapids_are_refused_by_name() {
    let mal = |p: PlasmaProgram, word: &str| {
        let f = judge(&p);
        assert!(matches!(f.as_slice(), [PlasmaFinding::Malformed(m)] if m.contains(word)), "{word}: {f:?}");
    };
    let mut p = plate(true);
    p.cuts[0].parent = Some(9);
    mal(p, "no cut carries");
    let mut p = plate(true);
    p.cuts[0].parent = Some(2);
    mal(p, "itself");
    let mut p = plate(true);
    p.cuts[0].parent = None;
    mal(p, "no parent");
    let mut p = plate(true);
    p.cuts[1].contour = 2;
    mal(p, "twice");
    let mut p = plate(true);
    p.rapids.pop();
    mal(p, "one rapid entry per cut");
    let mut p = plate(true);
    p.rapids[1] = Some((p.cuts[0].path[0], Point2::new(f64::NAN, 0.0)));
    mal(p, "rapid into contour 1");
}

/// A centre-pierced small hole may be a single point; an open path is
/// never a freed area for tip-up.
#[test]
fn small_holes_and_open_paths() {
    let mut dot = cut(2, Inner, Some(1), vec![p(50.0, 50.0)]);
    dot.small = true;
    let outline = cut(1, Outer, None, square(0.0, 0.0, 100.0));
    assert_eq!(judge(&PlasmaProgram { kerf: 1.5, cuts: vec![dot, outline], rapids: vec![None, None] }), vec![]);
    // A slit (open) cut first, then a rapid across it: not a tip-up.
    let slit = cut(1, Open, None, vec![p(0.0, 0.0), p(50.0, 0.0), p(50.0, 50.0), p(0.0, 50.0), p(0.0, 0.0)]);
    let next = cut(3, Outer, None, square(120.0, 0.0, 30.0));
    let over = Some((p(-10.0, 25.0), p(120.0, 0.0)));
    assert_eq!(judge(&PlasmaProgram { kerf: 1.5, cuts: vec![slit, next], rapids: vec![None, over] }), vec![]);
}
