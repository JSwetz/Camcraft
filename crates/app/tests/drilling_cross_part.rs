//! Issue #117: a cross (radial) drill is judged against the FINISHED
//! PART, the radial twin of the axial hole verdict. A cross hole the
//! model does not carry, or one plunged past the modelled hole's floor,
//! cuts steel the part keeps: the op is refused by name (the hole and
//! how deep the tip got) and nothing posts. A hole drilled to its
//! modelled depth stays clean.
//!
//! Gate map (M4.4): covers app::operation (cross_part, the cross arm's
//! part law; cross_spindle).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own fixtures (the F2 drilled bar and a solid bar of
//! the same envelope); no vendor material.

mod cross_support;

use cross_support::*;

/// The F2 bar's envelope with no hole in it: X [-15, 15], Y [-5, 5],
/// Z [-15, 15], six faces, closed.
fn solid_bar() -> Vec<Triangle> {
    let p = Point3::new;
    let mut t = Vec::new();
    t.extend(quad(p(-15.0, 5.0, -15.0), p(-15.0, 5.0, 15.0), p(15.0, 5.0, 15.0), p(15.0, 5.0, -15.0)));
    t.extend(quad(p(-15.0, -5.0, -15.0), p(15.0, -5.0, -15.0), p(15.0, -5.0, 15.0), p(-15.0, -5.0, 15.0)));
    t.extend(quad(p(-15.0, -5.0, 15.0), p(15.0, -5.0, 15.0), p(15.0, 5.0, 15.0), p(-15.0, 5.0, 15.0)));
    t.extend(quad(p(-15.0, -5.0, -15.0), p(-15.0, 5.0, -15.0), p(15.0, 5.0, -15.0), p(15.0, -5.0, -15.0)));
    t.extend(quad(p(-15.0, -5.0, -15.0), p(-15.0, -5.0, 15.0), p(-15.0, 5.0, 15.0), p(-15.0, 5.0, -15.0)));
    t.extend(quad(p(15.0, -5.0, -15.0), p(15.0, 5.0, -15.0), p(15.0, 5.0, 15.0), p(15.0, -5.0, 15.0)));
    t
}

/// The bar-stock world of the cross suites, with `facets` as the part.
fn world_of(facets: Vec<Triangle>) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -15.0), ..Location::default() };
    let stock = StockShape::Box { dx: 30.0, dy: 10.0, dz: 30.0 };
    history.execute(&world, SetStock::new(&world, stock, at).expect("box stock"));
    let cmd = LoadMachine::new(&world, "bench.mch".into(), Some(machine("mill-turn")));
    history.execute(&world, cmd.expect("machine"));
    let mut drill = catalog_tool("DR-0400");
    drill.flute_len = 50.0;
    history.execute(&world, AddTool::new(&world, "DR-0400", drill).expect("long-series drill"));
    history.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(facets)));
    (world, history)
}

fn op(cross: Vec<CrossHole>) -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity("bar".into()),
        params: cross_params(cross, None),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
    }
}

/// The generator's refusal of `cross` on the world's bar.
fn refusal(world: &World, cross: Vec<CrossHole>) -> String {
    match operation::generate(world, &op(cross), "") {
        Ok((t, w)) => panic!("refusal expected, drilled {} moves ({w:?})", t.moves.len()),
        Err(e) => e.to_string(),
    }
}

/// The F2 bar's blind hole (mouth x = 15, z = 10), `depth` deep.
fn blind(depth: f64) -> Vec<CrossHole> {
    vec![cross(Point3::new(15.0, 0.0, 10.0), nx(), 2.0, -depth, false)]
}

#[test]
fn a_cross_drill_into_a_solid_bar_is_refused_by_name() {
    let (world, _h) = world_of(solid_bar());
    let why = refusal(&world, blind(10.0));
    assert!(why.contains("cross hole at (15, 0, 10)"), "names the hole: {why}");
    assert!(why.contains("finished part") && why.contains("no hole there"), "says why: {why}");
}

#[test]
fn a_cross_hole_deeper_than_the_modelled_hole_is_refused() {
    // The modelled blind hole is 10 deep (floor x = 5); 13 asked.
    let (world, _h) = world_of(radial_bar());
    let why = refusal(&world, blind(13.0));
    assert!(why.contains("cross hole at (15, 0, 10)"), "names the hole: {why}");
    assert!(why.contains("past the modelled hole's floor"), "says why: {why}");
}

#[test]
fn a_cross_hole_matching_its_modelled_hole_is_clean() {
    let (world, mut history) = world_of(radial_bar());
    create(&world, &mut history, "CD", cross_params(typed(), None));
    let c = cache(&world, "CD").expect("cached");
    assert_eq!(status(&world, "CD"), OpStatus::Ok, "{:?}", c.warnings);
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
}
