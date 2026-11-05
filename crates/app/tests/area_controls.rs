//! Area control (M11.5), the region laws: the selection says where, the
//! Boundary settings say how far. Face extended past the part edge
//! clears to the stock outline plus the extension; tool containment
//! Inside / On / Outside moves the tool against the boundary on a pocket
//! and a contour; the offset grows and shrinks with round or sharp
//! corners; include / exclude boundaries combine by the region booleans;
//! an empty result refuses by name. (`area_open` carries the open
//! boundary, the legality, the command and the file.)
//!
//! Gate map (M11.5): covers app::operation (area, boundary, boundary_ctx,
//! job, generate), geo2d (offset_sharp), app::verify.

mod area_support;
mod common;

use area_support::*;

#[test]
fn face_extended_past_the_part_edge_clears_to_the_stock_outline_plus_the_extension() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let world = world_with(Some(ImportModel::new(&path).expect("the user's STEP imports")), -7.0, 27.0);
    let name = model::named_entities(&world)
        .into_iter()
        .map(|(n, _)| n)
        .find(|n| n != "Stock" && n != TOOL)
        .expect("the imported part");
    let src = sel(&name, top_face(&world, &name, 20.0));
    let (lo, hi) = stock_box(&world);
    // Today's law: the facing stays on the picked face.
    let (base, warns) = run(&world, &op(src.clone(), face(20.0), AreaControls::default())).expect("default face");
    assert_eq!(warns, 0);
    let bf = feeds(&base);
    assert!(max_by(&bf, |p| p.y) + R < hi.y - 1.0, "the default stays inside the selection");
    // Extended by 5: off the part's edge, out to the stock and 5 past it.
    let ext = AreaControls { extension: 5.0, ..Default::default() };
    let (t, warns) = run(&world, &op(src, face(20.0), ext)).expect("extended face");
    assert_eq!(warns, 0, "the extended face must verify clean (depth-aware): {:?}", t.notes);
    let f = feeds(&t);
    assert!((max_by(&f, |p| p.y) + R - (hi.y + 5.0)).abs() < EPS, "rows reach the stock edge + 5");
    assert!((max_by(&f, |p| -p.y) + R - (5.0 - lo.y)).abs() < EPS, "both sides");
    assert!(max_by(&f, |p| p.x) - R >= hi.x + 5.0 - EPS, "the X runs leave the stock + 5 in air");
    assert!(max_by(&f, |p| -p.x) - R >= 5.0 - lo.x - EPS);
    assert!(f.iter().all(|p| (p.z - 20.0).abs() < 1e-9));
}

fn pocket_reach(world: &World, c: Option<Containment>, offset: f64) -> Result<f64, String> {
    let a = AreaControls { containment: c, offset, ..Default::default() };
    let (t, warns) = run(world, &op(SrcRef::Stock, pocket(8.0, 4.0), a))?;
    assert_eq!(warns, 0);
    Ok(max_by(&feeds(&t), |p| p.x))
}

#[test]
fn containment_inside_on_outside_moves_the_pocket_tool_against_the_boundary() {
    let world = world_with(None, 0.0, 10.0);
    let (_, hi) = stock_box(&world);
    let default = pocket_reach(&world, None, 0.0).expect("default");
    let inside = pocket_reach(&world, Some(Containment::Inside), 0.0).expect("inside");
    let on = pocket_reach(&world, Some(Containment::On), 0.0).expect("on");
    let outside = pocket_reach(&world, Some(Containment::Outside), 0.0).expect("outside");
    assert!((default - (hi.x - R)).abs() < EPS && (inside - default).abs() < 1e-9, "Inside is the pocket's own law");
    assert!((on - hi.x).abs() < EPS, "On: the centre rides the boundary ({on})");
    assert!((outside - (hi.x + R)).abs() < EPS, "Outside: the tool clears past it ({outside})");
}

#[test]
fn the_offset_grows_and_shrinks_with_round_or_sharp_corners() {
    let world = world_with(None, 0.0, 10.0);
    let (_, hi) = stock_box(&world);
    assert!((pocket_reach(&world, None, 3.0).expect("grow") - (hi.x + 3.0 - R)).abs() < EPS);
    assert!((pocket_reach(&world, None, -3.0).expect("shrink") - (hi.x - 3.0 - R)).abs() < EPS);
    let area_of = |corner: CornerStyle| {
        let a = AreaControls { offset: 3.0, corner, ..Default::default() };
        operation::region_of(&world, &op(SrcRef::Stock, pocket(8.0, 4.0), a)).expect("preview").area().abs()
    };
    let (round, sharp) = (area_of(CornerStyle::Round), area_of(CornerStyle::Sharp));
    assert!((sharp - 76.0 * 76.0).abs() < 0.01, "sharp keeps the square ({sharp})");
    assert!(round < sharp - 1.0, "round rounds the corners ({round})");
}

fn contour_reach(world: &World, c: Option<Containment>) -> (f64, String) {
    let a = AreaControls { containment: c, ..Default::default() };
    let (t, warns) = run(world, &op(SrcRef::Stock, contour(8.0, 4.0, Side::Auto), a)).expect("contour");
    assert_eq!(warns, 0);
    (max_by(&feeds(&t), |p| p.x), common::gen_to_cltext(&t))
}

#[test]
fn containment_inside_on_outside_moves_the_contour_tool_against_the_boundary() {
    let world = world_with(None, 0.0, 10.0);
    let (_, hi) = stock_box(&world);
    let (default, cl0) = contour_reach(&world, None);
    let (_outside, cl1) = contour_reach(&world, Some(Containment::Outside));
    let (on, _) = contour_reach(&world, Some(Containment::On));
    let (inside, _) = contour_reach(&world, Some(Containment::Inside));
    assert!((default - (hi.x + R)).abs() < EPS && cl0 == cl1, "Outside is the outside contour's own law");
    assert!((on - hi.x).abs() < EPS, "On ({on})");
    assert!((inside - (hi.x - R)).abs() < EPS, "Inside ({inside})");
}

#[test]
fn include_and_exclude_boundaries_combine_by_the_region_booleans() {
    let world = block_world();
    let (lo, hi) = stock_box(&world);
    let top = top_face(&world, "part", 10.0);
    // Exclude the part's top face from a stock pocket: the tool body
    // never enters the 40 x 40 square.
    let ex = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Picks(pick("part", top)) }],
        ..Default::default()
    };
    let (t, warns) = run(&world, &op(SrcRef::Stock, pocket(8.0, 4.0), ex)).expect("excluded pocket");
    assert_eq!(warns, 0);
    let f = feeds(&t);
    assert!(!f.is_empty());
    let off_square = |p: &Point3| ((p.x.abs() - 20.0).max(0.0)).hypot((p.y.abs() - 20.0).max(0.0));
    let worst = f.iter().copied().fold((f64::INFINITY, Point3::new(0.0, 0.0, 0.0)), |b, p| if off_square(&p) < b.0 { (off_square(&p), p) } else { b });
    assert!(worst.0 >= R - 0.01, "the tool stays off the excluded square: {worst:?}");
    // Include the stock outline on a Face of the part's top: the face
    // clears the whole stock.
    let inc = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Include, shape: BoundaryShape::Stock }],
        ..Default::default()
    };
    let (t, warns) = run(&world, &op(sel("part", top), face(10.0), inc)).expect("included face");
    assert_eq!(warns, 0, "{:?}", t.notes);
    let f = feeds(&t);
    assert!((max_by(&f, |p| p.y) + R - hi.y).abs() < EPS && (max_by(&f, |p| -p.y) + R + lo.y).abs() < EPS);
}

#[test]
fn a_boundary_that_leaves_nothing_refuses_by_name() {
    let world = world_with(None, 0.0, 10.0);
    let a = AreaControls { offset: -40.0, ..Default::default() };
    let err = run(&world, &op(SrcRef::Stock, pocket(8.0, 4.0), a)).expect_err("nothing left");
    assert!(err.contains("Boundary settings leave no area") && err.contains("boundary offset"), "{err}");
    let world = block_world();
    let top = top_face(&world, "part", 10.0);
    let a = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Stock }],
        ..Default::default()
    };
    let err = run(&world, &op(SrcRef::Stock, pocket(8.0, 4.0), a)).expect_err("all excluded");
    assert!(err.contains("exclude boundaries"), "{err}");
    // Face cannot follow a hole: an exclude inside its band refuses.
    let a = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Picks(pick("part", top)) }],
        ..Default::default()
    };
    let err = run(&world, &op(SrcRef::Stock, face(10.0), a)).expect_err("face over a hole");
    assert!(err.contains("an exclude boundary"), "{err}");
}
