//! #198: a box stock fitted by a separate allowance per side - the
//! command, its undo and refusal, and the Stock page's "Per side" fields.
//! Provenance: our own page law.

use super::*;
use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::stock::StockShape;

fn stock_of(st: &State) -> (StockShape, [f64; 3]) {
    let name = camcraft_app::setup::stock_entity_name(&st.world, None);
    let e = model::find_by_name(&st.world, &name).expect("stock");
    let shape = model::stock_def_of(&st.world, &e).expect("stock shape");
    let p = model::transform_of(&st.world, &e).unwrap_or_default().pos;
    (shape, [p.x, p.y, p.z])
}

#[test]
fn a_box_is_fitted_by_each_sides_allowance_and_undoes() {
    let mut st = State::new();
    let block = cube(Point3::new(-10.0, -5.0, 0.0), Point3::new(30.0, 15.0, 12.0));
    st.history.execute(&st.world, ImportModel::from_mesh("block", block));
    st.doc_version += 1;
    st.pending.push_back(CommandKind::StockFitSides { sides: [1.0, 3.0, 0.0, 2.0, 0.5, 4.0] });
    st.drain_actions();
    assert_eq!(st.status, "stock fitted to models");
    let (shape, pos) = stock_of(&st);
    assert_eq!(shape, StockShape::Box { dx: 44.0, dy: 22.0, dz: 16.5 });
    assert_eq!(pos, [11.0, 6.0, -0.5], "each face stands its own allowance off the block");

    st.pending.push_back(CommandKind::StockFitSides { sides: [1.0, 1.0, -1.0, 1.0, 1.0, 1.0] });
    st.drain_actions();
    assert!(st.status.contains("must be zero or more"), "{}", st.status);
    assert_eq!(stock_of(&st).0, StockShape::Box { dx: 44.0, dy: 22.0, dz: 16.5 }, "a refusal changes nothing");

    st.pending.push_back(CommandKind::Undo);
    st.drain_actions();
    let name = camcraft_app::setup::stock_entity_name(&st.world, None);
    assert!(model::find_by_name(&st.world, &name).is_none(), "undo takes the fitted stock away");
}

#[test]
fn the_stock_page_offers_per_side_fields_for_a_box() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Stock;
    harness.run();
    assert!(harness.query_by_label("Fit margin Top").is_none(), "one margin by default");
    harness.get_by_label("Per side").click();
    harness.run();
    assert!(harness.state().state.stock_ui.fit_per_side);
    for name in ["-X", "+X", "-Y", "+Y", "Bottom", "Top"] {
        assert!(harness.query_by_label(&format!("Fit margin {name}")).is_some(), "{name} field");
    }
}
