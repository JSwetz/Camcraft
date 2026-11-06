//! M11.3 owed (RECOMMENDED "M11.3 owed: the centre-cutting law on every
//! plunge" (5)), through the application: the tool's own
//! `center_cutting` reaches Face and Drill. A stock-sourced Face enters
//! a radius off the stock - air - so a non-centre-cutting face mill
//! posts the very path a centre-cutting one does; a drill cycle with a
//! non-centre-cutting drill is refused by name (the M10.1 tool law is a
//! family match and lets it through, so the cycle's door says no).
//!
//! Gate map (M11.3): covers app::operation (generate, plunge_solid),
//! app::drilling, app::machining2d (face).

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, History, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;
use camcraft_motion::approach::CENTER_CUT_REFUSAL;

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog entry")
}

/// A box stock and the catalog tool `base` twice: as itself, and as
/// "NC", the same body marked not centre-cutting.
fn world_with(base: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
    h.execute(&world, stock.expect("box stock"));
    let t = catalog_tool(base);
    h.execute(&world, AddTool::new(&world, base, t.clone()).expect("tool"));
    h.execute(&world, AddTool::new(&world, "NC", Tool { center_cutting: false, ..t }).expect("tool"));
    (world, h)
}

fn op(tool: &str, params: OpParams) -> Operation {
    Operation { tool: tool.into(), src: SrcRef::Stock, params, stamp: None, setup: None, area: Default::default() }
}

#[test]
fn a_stock_face_enters_from_air_for_a_tool_that_is_not_centre_cutting() {
    let (world, _h) = world_with("EM-0800-4FL");
    for (top_z, stepdown) in [(None, None), (Some(Height::fixed(20.0)), Some(1.0))] {
        let face = OpParams::Face {
            blend_turns: false,
            tilt: None,
            stepover: 4.0,
            clearance_z: Height::fixed(30.0),
            cut_z: Height::fixed(18.0),
            top_z,
            stepdown,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        };
        let (cc, _) = operation::generate(&world, &op("EM-0800-4FL", face.clone()), "F").expect("cc face");
        let (nc, _) = operation::generate(&world, &op("NC", face), "F").expect("the facer enters from air");
        assert_eq!(cc.moves, nc.moves, "the same path for either tool");
    }
}

#[test]
fn a_drill_cycle_with_a_tool_that_is_not_centre_cutting_is_refused_by_name() {
    let (world, _h) = world_with("DR-0400");
    let drill = OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(22.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes: vec![Hole { xy: Point2::new(10.0, 10.0), radius: 2.0, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false }],
        cross: Vec::new(),
    };
    operation::generate(&world, &op("DR-0400", drill.clone()), "D").expect("a centre-cutting drill");
    let e = operation::generate(&world, &op("NC", drill), "D").expect_err("a plunge into solid");
    assert!(format!("{e:?}").contains(CENTER_CUT_REFUSAL), "refused, but not by the law: {e:?}");
}
