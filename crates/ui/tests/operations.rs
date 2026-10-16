//! Operation and tool wiring through `State`: the queue arms, statuses,
//! drafts and derived staleness the CAM panes ride on. The command layer
//! itself is pinned in `crates/app/tests/operations.rs`; these tests own
//! the ui-side half - statuses, drafts, and undo/redo through the queue.
//! Provenance: our own tests, specced from the panes' contract.

use hsmworks_app::model;
use hsmworks_app::operation::{self, OpParams, OpStatus, SrcRef};
use hsmworks_app::schema::STOCK_NAME;
use hsmworks_app::stock::StockShape;
use hsmworks_app::tools::{Tool, catalog};
use hsmworks_app::machining2d::Lead;
use hsmworks_ui::state::{CommandKind, ErrorScope, OpUi, State, ToolUi, UiTarget};

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog tool")
}

fn face_params() -> OpParams {
    OpParams::Face {
        stepover: 3.0,
        clearance_z: 25.0,
        cut_z: 19.5,
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

fn box_stock(dz: f64) -> CommandKind {
    CommandKind::SetStock {
        shape: StockShape::Box {
            dx: 60.0,
            dy: 60.0,
            dz,
        },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    }
}

/// 60x60x20 box stock at the origin and a catalog end mill, ready for a
/// face operation.
fn stock_and_tool(state: &mut State) {
    state.pending.push_back(box_stock(20.0));
    state.pending.push_back(CommandKind::AddTool {
        name: "EM-0400-2FL".into(),
        tool: catalog_tool("EM-0400-2FL"),
    });
    state.drain_actions();
    assert!(state.status.contains("tool EM-0400-2FL added"), "{:?}", state.status);
}

fn create_face(state: &mut State) {
    state.pending.push_back(CommandKind::CreateOperation {
        name: "Face1".into(),
        tool: "EM-0400-2FL".into(),
        src: SrcRef::Stock,
        params: face_params(),
    });
    state.drain_actions();
    assert_eq!(state.status, "operation Face1 created");
}

fn status(state: &State, name: &str) -> OpStatus {
    let e = model::find_by_name(&state.world, name).expect("operation entity");
    operation::status_of(&state.world, &e)
}

fn cache(state: &State, name: &str) -> Option<hsmworks_app::cl::Toolpath> {
    let e = model::find_by_name(&state.world, name).expect("operation entity");
    model::toolpath_of(&state.world, &e).map(|tp| tp.path)
}

fn stored_params(state: &State, name: &str) -> OpParams {
    let e = model::find_by_name(&state.world, name).expect("operation entity");
    model::operation_of(&state.world, &e)
        .expect("op")
        .params
}

#[test]
fn tool_add_update_delete_undo_and_refusals() {
    let mut state = State::new();
    state.pending.push_back(box_stock(20.0));
    state.drain_actions();

    state.pending.push_back(CommandKind::AddTool {
        name: "EM-0400-2FL".into(),
        tool: catalog_tool("EM-0400-2FL"),
    });
    state.drain_actions();
    assert_eq!(state.status, "tool EM-0400-2FL added");
    assert!(state.tool_names().iter().any(|t| t == "EM-0400-2FL"), "combo lists it");

    // Same name twice: refused, history untouched.
    state.pending.push_back(CommandKind::AddTool {
        name: "EM-0400-2FL".into(),
        tool: catalog_tool("EM-0200-2FL"),
    });
    state.drain_actions();
    assert!(state.status.starts_with("tool not added"), "{:?}", state.status);

    // Update the numbers, then undo exactly.
    let mut fat = catalog_tool("EM-0400-2FL");
    fat.diameter = 8.0;
    state.pending.push_back(CommandKind::ChangeTool {
        name: "EM-0400-2FL".into(),
        tool: fat,
    });
    state.drain_actions();
    assert_eq!(state.status, "tool EM-0400-2FL updated");
    let e = model::find_by_name(&state.world, "EM-0400-2FL").expect("tool entity");
    assert_eq!(model::tool_of(&state.world, &e).map(|t| t.diameter), Some(8.0));
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    let e = model::find_by_name(&state.world, "EM-0400-2FL").expect("still there");
    assert_eq!(model::tool_of(&state.world, &e).map(|t| t.diameter), Some(4.0));

    state.pending.push_back(CommandKind::DeleteTool("EM-0400-2FL".into()));
    state.drain_actions();
    assert_eq!(state.status, "deleted tool EM-0400-2FL");
    assert!(!state.tool_names().iter().any(|t| t == "EM-0400-2FL"));
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(state.tool_names().iter().any(|t| t == "EM-0400-2FL"), "undo restores it");
}

#[test]
fn tool_page_keeps_numbers_after_delete_and_resyncs_on_undo() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    state.tool_ui = ToolUi::from_tool("EM-0400-2FL", &catalog_tool("EM-0400-2FL"), true);
    state.sync_tool_ui();
    assert!(state.tool_ui.exists && state.tool_ui.diameter == 4.0);

    state.pending.push_back(CommandKind::DeleteTool("EM-0400-2FL".into()));
    state.drain_actions();
    state.sync_tool_ui();
    assert!(!state.tool_ui.exists, "the page flips to re-add mode");
    assert_eq!(state.tool_ui.diameter, 4.0, "the numbers survive");

    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    state.sync_tool_ui();
    assert!(state.tool_ui.exists, "undo re-attaches the page to the entity");
}

#[test]
fn create_generate_undo_redo_replays_the_same_path() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    assert_eq!(status(&state, "Face1"), OpStatus::Never, "a new op is not generated");
    assert!(cache(&state, "Face1").is_none());

    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();
    assert!(state.status.starts_with("generated Face1:"), "{:?}", state.status);
    assert_eq!(status(&state, "Face1"), OpStatus::Ok);
    let first = cache(&state, "Face1").expect("cache after generate");
    assert!(!first.is_empty());

    // Undo removes the cache (there was none before); redo replays it
    // move-for-move - generation ran once, in the command's constructor.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert!(cache(&state, "Face1").is_none());
    assert_eq!(status(&state, "Face1"), OpStatus::Never);
    state.pending.push_back(CommandKind::Redo);
    state.drain_actions();
    assert_eq!(cache(&state, "Face1"), Some(first.clone()), "redo is byte-identical");
    assert_eq!(status(&state, "Face1"), OpStatus::Ok);

    // A parameter edit marks it stale but keeps the old path on screen.
    let moved = OpParams::Face {
        stepover: 4.0,
        clearance_z: 25.0,
        cut_z: 19.5,
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    state.pending.push_back(CommandKind::EditOperationParams {
        name: "Face1".into(),
        params: moved.clone(),
    });
    state.drain_actions();
    assert_eq!(state.status, "parameters updated for Face1");
    assert_eq!(status(&state, "Face1"), OpStatus::Stale);
    assert_eq!(cache(&state, "Face1"), Some(first.clone()), "stale keeps the cache");

    // Undoing the edit restores the stamp the cache was cut under.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(status(&state, "Face1"), OpStatus::Ok);
    assert_eq!(stored_params(&state, "Face1"), face_params());
}

#[test]
fn stock_resize_makes_the_operation_stale_and_regenerate_clears_it() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();
    assert_eq!(status(&state, "Face1"), OpStatus::Ok);

    state.pending.push_back(box_stock(30.0));
    state.drain_actions();
    assert_eq!(status(&state, "Face1"), OpStatus::Stale, "the stamp reads the stock params");

    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();
    assert!(state.status.starts_with("generated Face1:"), "{:?}", state.status);
    assert_eq!(status(&state, "Face1"), OpStatus::Ok);
}

#[test]
fn generate_failure_reports_and_writes_nothing() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();
    let first = cache(&state, "Face1").expect("cache");

    // Remove the source under the operation: the op reads Stale, and a
    // generation attempt fails with a diagnostic, leaving the cache alone.
    state.pending.push_back(CommandKind::DeleteByName(STOCK_NAME.into()));
    state.drain_actions();
    assert_eq!(state.status, "deleted Stock");
    assert_eq!(status(&state, "Face1"), OpStatus::Stale);

    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();
    assert!(state.status.starts_with("generate failed"), "{:?}", state.status);
    assert_eq!(cache(&state, "Face1"), Some(first.clone()), "nothing was rewritten");

    // Bring the stock back and regenerate cleanly.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(status(&state, "Face1"), OpStatus::Ok, "undo restores the exact stamp match");
}

#[test]
fn selecting_an_operation_mirrors_it_into_the_draft() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);

    state.pending.push_back(CommandKind::Select(Some("Face1".into())));
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(state.selected_operation().as_deref(), Some("Face1"));
    assert_eq!(state.op_ui.name, "Face1");
    assert_eq!(state.op_ui.tool, "EM-0400-2FL");
    assert_eq!(state.op_ui.src_kind, 0, "stock source reads as Stock");
    assert_eq!(state.op_ui.params, face_params());

    // With nothing selected the draft becomes the new-operation form,
    // pre-named past the existing operations.
    state.pending.push_back(CommandKind::Select(None));
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(state.op_ui.name, "Operation2");
    assert_eq!(state.op_ui.tool, "EM-0400-2FL", "prefilled with the last tool");
}

#[test]
fn retargeting_a_tool_and_bad_params() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    state.pending.push_back(CommandKind::AddTool {
        name: "EM-0600-4FL".into(),
        tool: catalog_tool("EM-0600-4FL"),
    });
    state.drain_actions();
    assert!(state.tool_names().iter().any(|t| t == "EM-0600-4FL"), "combo lists both");

    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();
    assert_eq!(status(&state, "Face1"), OpStatus::Ok);

    // A tool change is a discrete event; the cache predates it, so Stale.
    state.pending.push_back(CommandKind::EditOperationLinks {
        name: "Face1".into(),
        tool: Some("EM-0600-4FL".into()),
        src: None,
    });
    state.drain_actions();
    assert_eq!(state.status, "links updated for Face1");
    assert_eq!(status(&state, "Face1"), OpStatus::Stale);
    state.pending.push_back(CommandKind::Select(Some("Face1".into())));
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(state.op_ui.tool, "EM-0600-4FL", "the draft follows the world");

    // Degenerate parameters are refused at construction: status explains,
    // and the operation keeps its exact previous values and stamp.
    let junk = OpParams::Face {
        stepover: 0.0,
        clearance_z: 25.0,
        cut_z: 19.5,
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    state.pending.push_back(CommandKind::EditOperationParams {
        name: "Face1".into(),
        params: junk,
    });
    state.drain_actions();
    assert!(state.status.starts_with("parameters not updated"), "{:?}", state.status);
    assert_eq!(stored_params(&state, "Face1"), face_params(), "world unchanged");
}

#[test]
fn first_launch_drafts_carry_names_not_blanks() {
    let state = State::new();
    assert_eq!(state.tool_ui.name, "Tool1", "Add is never aimed at an empty name");
    assert_eq!(state.op_ui.name, "Operation1");
    // New drafts number past what the document already holds.
    let mut state = state;
    state.pending.push_back(box_stock(20.0));
    state.pending.push_back(CommandKind::AddTool {
        name: "Tool1".into(),
        tool: catalog_tool("EM-0400-2FL"),
    });
    state.drain_actions();
    assert_eq!(state.new_tool_draft().name, "Tool2");
}

#[test]
fn refusals_are_tagged_to_their_pane_and_cleared_by_the_next_command() {
    let mut state = State::new();
    state.pending.push_back(box_stock(20.0));
    // A nameless tool draft is refused; the message belongs to the Tool page.
    state.pending.push_back(CommandKind::AddTool {
        name: String::new(),
        tool: catalog_tool("EM-0400-2FL"),
    });
    state.drain_actions();
    assert!(state.status.starts_with("tool not added"), "{:?}", state.status);
    assert!(state.error_for(ErrorScope::Tool).is_some());
    assert_eq!(state.error_for(ErrorScope::Operation), None, "not this pane's business");

    // Any command that runs retires the banner, successful or not.
    state.pending.push_back(CommandKind::AddTool {
        name: "EM-0400-2FL".into(),
        tool: catalog_tool("EM-0400-2FL"),
    });
    state.drain_actions();
    assert_eq!(state.error_for(ErrorScope::Tool), None, "the success cleared it");

    // And an operation refusal lands on the operation scope, not the tool's.
    state.pending.push_back(CommandKind::CreateOperation {
        name: "Face1".into(),
        tool: String::new(),
        src: SrcRef::Stock,
        params: face_params(),
    });
    state.drain_actions();
    assert!(state.status.starts_with("operation not created"), "{:?}", state.status);
    assert!(state.error_for(ErrorScope::Operation).is_some());
    assert_eq!(state.error_for(ErrorScope::Tool), None);
}

#[test]
fn pane_target_routes_then_falls_back_to_selection() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);

    // Selection alone routes the pane (the fallback).
    state.pending.push_back(CommandKind::Select(Some("Face1".into())));
    state.drain_actions();
    assert_eq!(state.pane_target(), UiTarget::Operation("Face1".into()));

    // An explicit target wins over the selection...
    state.ui_target = UiTarget::Tool("EM-0400-2FL".into());
    assert_eq!(state.pane_target(), UiTarget::Tool("EM-0400-2FL".into()));

    // ...and a target whose entity disappears stops holding the pane by
    // itself: the fallback (the selected operation) takes over.
    state.pending.push_back(CommandKind::DeleteTool("EM-0400-2FL".into()));
    state.drain_actions();
    assert_eq!(state.pane_target(), UiTarget::Operation("Face1".into()));

    // The stock page is a target even with no stock yet: creating it is
    // what that page is for.
    state.pending.push_back(CommandKind::DeleteByName(STOCK_NAME.into()));
    state.drain_actions();
    state.ui_target = UiTarget::Stock;
    assert_eq!(state.pane_target(), UiTarget::Stock);
}

#[test]
fn the_empty_tool_target_is_a_destination_not_a_ghost() {
    // CAM > Tool Library (and the Tools group) point at the tool page
    // without naming a tool - like Stock, the page doubles as the catalog
    // / new-draft form, so an empty name must resolve. A *named* tool that
    // no longer exists must not: the pane falls back instead of editing a
    // ghost.
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    state.pending.push_back(CommandKind::Select(Some("Face1".into())));
    state.drain_actions();

    state.ui_target = UiTarget::Tool(String::new());
    assert_eq!(state.pane_target(), UiTarget::Tool(String::new()));

    state.ui_target = UiTarget::Tool("ghost".into());
    assert_eq!(state.pane_target(), UiTarget::Operation("Face1".into()));
}

#[test]
fn targeting_an_operation_mirrors_the_draft_without_selection() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    state.ui_target = UiTarget::Operation("Face1".into());
    state.sync_ops_ui();
    assert_eq!(state.op_ui.name, "Face1");
    assert_eq!(state.op_ui.tool, "EM-0400-2FL");
    assert_eq!(state.op_ui.params, face_params());
}

#[test]
fn new_operation_falls_from_target_and_selection_to_the_form() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    state.pending.push_back(CommandKind::Select(Some("Face1".into())));
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(state.op_ui.name, "Face1");
    // The menu action: blank the target and clear the selection (that is
    // the command part), and the pane re-prices the creation form.
    state.ui_target = UiTarget::None;
    state.pending.push_back(CommandKind::Select(None));
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(state.pane_target(), UiTarget::None);
    assert_eq!(state.op_ui.name, "Operation2");
}

#[test]
fn delete_operation_round_trips_through_the_queue() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();
    let first = cache(&state, "Face1").expect("cache");

    state.pending.push_back(CommandKind::DeleteOperation("Face1".into()));
    state.drain_actions();
    assert_eq!(state.status, "deleted operation Face1");
    assert!(model::find_by_name(&state.world, "Face1").is_none());

    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(cache(&state, "Face1"), Some(first), "the cached path rides the undo snapshot");
    assert_eq!(status(&state, "Face1"), OpStatus::Ok);
}

// ---------------------------------------------------------------------------
// M1.5 step 7: the Geometry section's chip list, driven through the State
// seams it rides on - queue_feature_pick (the viewport's entrance) and the
// CommandKinds the chip buttons queue - and mirrored back by sync_ops_ui.
// The pane's drawing code itself is the same read-only walk of these.

use hsmworks_app::commands::ImportModel;
use hsmworks_app::geometry::{cube, feature, Point3};
use hsmworks_app::operation::{FeatureSel, GeoSel};
use hsmworks_ui::app::queue_feature_pick;

/// A cube part (so faces and chains exist to pick) with Face1 targeted as
/// the pane's draft, exactly as selecting it in the browser would.
fn part_and_target(state: &mut State) {
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    state.ui_target = UiTarget::Operation("Face1".into());
    state.sync_ops_ui();
}

/// The top (or bottom) horizontal planar face of `name` as a pick. Face
/// normals are sign-canonical, so top and bottom share (0,0,1) and only
/// the signed offset tells them apart.
fn face_pick(state: &State, name: &str, top: bool) -> GeoSel {
    let e = model::find_by_name(&state.world, name).expect("entity");
    let f = model::features_of(&state.world, &e).expect("features");
    let mut hs: Vec<_> = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2].abs() > 0.9)
        .collect();
    hs.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    let fa = if top { hs.pop() } else { hs.into_iter().next() }.expect("a horizontal face");
    GeoSel { entity: name.to_string(), feature: FeatureSel::Face(fa.id) }
}

/// The first chain of `name` - a box gives twelve open single-edge ones.
fn chain_pick(state: &State, name: &str) -> GeoSel {
    let e = model::find_by_name(&state.world, name).expect("entity");
    let f = model::features_of(&state.world, &e).expect("features");
    let c = feature::chains(&f).into_iter().next().expect("a box edge");
    GeoSel { entity: name.to_string(), feature: FeatureSel::Chain(c.id) }
}

fn src_of(state: &State, name: &str) -> SrcRef {
    let e = model::find_by_name(&state.world, name).expect("operation entity");
    model::operation_of(&state.world, &e).expect("operation").src
}

#[test]
fn chips_round_trip_clear_restores_stock_and_undo_is_exact() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    part_and_target(&mut state);

    // A plain pick replaces the source; one chip, resolved, named right.
    let top = face_pick(&state, "part", true);
    queue_feature_pick(&mut state, top.clone(), false);
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(src_of(&state, "Face1"), SrcRef::Selection(vec![top.clone()]));
    let chips = state.geo_chips();
    assert_eq!(chips.len(), 1, "one chip per pick");
    assert!(!chips[0].missing);
    assert!(chips[0].label.starts_with("Face part · "), "{}", chips[0].label);

    // Ctrl adds a chain: the list is kind-blind and order-keeping.
    let chain = chain_pick(&state, "part");
    queue_feature_pick(&mut state, chain.clone(), true);
    state.drain_actions();
    state.sync_ops_ui();
    let chips = state.geo_chips();
    assert_eq!(chips.len(), 2);
    assert!(chips[1].label.starts_with("Chain part · "), "{}", chips[1].label);
    assert!(
        chips[1].label.contains(" · open · ") && chips[1].label.ends_with(" · 1 of 1"),
        "a lone box edge is one open run, length and count spelled out: {}",
        chips[1].label
    );

    // The chip's x queues the reduced list, in the world's order.
    let mut rest = state.op_ui.geo.clone();
    rest.remove(0);
    state.pending.push_back(CommandKind::SetOperationGeometry { name: "Face1".into(), items: rest });
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(src_of(&state, "Face1"), SrcRef::Selection(vec![chain.clone()]));
    assert_eq!(state.geo_chips().len(), 1);

    // Clear reverts the source - an empty selection is not a thing.
    state.pending.push_back(CommandKind::EditOperationLinks {
        name: "Face1".into(),
        tool: None,
        src: Some(SrcRef::Stock),
    });
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(src_of(&state, "Face1"), SrcRef::Stock);
    assert_eq!(state.op_ui.src_kind, 0, "the Source combo reads Stock again");
    assert!(state.geo_chips().is_empty());

    // Undo lands the exact pre-Clear list back, order and all.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(src_of(&state, "Face1"), SrcRef::Selection(vec![chain]));
    assert_eq!(state.op_ui.src_kind, 2, "kind 2 rides the mirror, not a guess");
}

#[test]
fn a_duplicate_pick_queues_nothing() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    part_and_target(&mut state);
    let top = face_pick(&state, "part", true);
    queue_feature_pick(&mut state, top.clone(), false);
    state.drain_actions();
    state.sync_ops_ui();

    let before = state.pending.len();
    queue_feature_pick(&mut state, top.clone(), true);
    assert_eq!(state.pending.len(), before, "the repeat adds no command");
    assert!(state.status.contains("already selected"), "{}", state.status);
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(src_of(&state, "Face1"), SrcRef::Selection(vec![top]));
    assert_eq!(state.geo_chips().len(), 1, "and the chip row never doubled");
}

// ---------------------------------------------------------------------------
// M1.5.1 step 1: the Apply click always answers. A changed draft queues the
// undoable edit; an unchanged draft is a true no-op for the history - and
// says so through the status strip instead of meeting the click with
// silence.

#[test]
fn apply_with_no_changes_reports_nothing_changed() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    part_and_target(&mut state);
    // The draft mirrors the operation exactly, so this Apply is the no-op.
    state.apply_operation_params("Face1");
    assert_eq!(state.status, "parameters already applied - nothing changed");
    assert!(state.pending.is_empty(), "a no-op queues no command: {:?}", state.pending);
}

#[test]
fn apply_reports_queued_then_updated_on_drain() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    part_and_target(&mut state);
    // Spinbox typing is what this mirrors: the draft differs from live.
    state.op_ui.params = OpParams::Face {
        stepover: 3.0,
        clearance_z: 25.0,
        cut_z: 18.0,
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    state.apply_operation_params("Face1");
    assert_eq!(state.pending.len(), 1, "the changed draft queued the edit");
    state.drain_actions();
    assert_eq!(state.status, "parameters updated for Face1");
    assert!(
        matches!(stored_params(&state, "Face1"),
            OpParams::Face { cut_z, .. } if (cut_z - 18.0).abs() < 1e-12),
        "the operation carries the draft's cut height"
    );
}

#[test]
fn a_missing_model_turns_the_chip_amber_and_undo_restores_it() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    part_and_target(&mut state);
    let top = face_pick(&state, "part", true);
    queue_feature_pick(&mut state, top.clone(), false);
    state.drain_actions();
    state.sync_ops_ui();
    let was = state.geo_chips()[0].label.clone();

    // The model goes away under the pick list: stale, not silently gone.
    state.pending.push_back(CommandKind::DeleteByName("part".into()));
    state.drain_actions();
    state.sync_ops_ui();
    let chips = state.geo_chips();
    assert_eq!(chips.len(), 1, "the pick survives its model, visibly broken");
    assert!(chips[0].missing, "amber: {}", chips[0].label);
    assert!(chips[0].label.ends_with(" · missing"), "{}", chips[0].label);

    // Undo is byte-exact: the model, its features, and the id on them.
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(src_of(&state, "Face1"), SrcRef::Selection(vec![top]));
    let chips = state.geo_chips();
    assert!(!chips[0].missing, "the id resolves again: {}", chips[0].label);
    assert_eq!(chips[0].label, was, "the very same chip, not a new one");
}

// ---------------------------------------------------------------------------
// M1.5.1 step 2: rows read as humans talk about the model - where a face
// sits and how big it is, how long a run is and which piece of the loop it
// is - and a pick whose geometry is gone says so instead of a bare digest.

#[test]
fn selection_rows_read_as_human_descriptors() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    part_and_target(&mut state);

    // Face: axis locator and area. The 10 x 10 top of the box sits at z 5.
    let top = face_pick(&state, "part", true);
    queue_feature_pick(&mut state, top, false);
    state.drain_actions();
    state.sync_ops_ui();
    let chips = state.geo_chips();
    assert!(!chips[0].missing);
    assert!(chips[0].label.starts_with("Face part · z=5.00"), "{}", chips[0].label);
    assert!(chips[0].label.contains("100.0 mm"), "area spelled out: {}", chips[0].label);

    // Chain: open or closed, length, and its number among the runs picked
    // on that entity. A lone box edge is one open run of one.
    let chain = chain_pick(&state, "part");
    queue_feature_pick(&mut state, chain, true);
    state.drain_actions();
    state.sync_ops_ui();
    let chips = state.geo_chips();
    assert!(chips[1].label.starts_with("Chain part · open · "), "{}", chips[1].label);
    assert!(chips[1].label.contains(" mm · 1 of 1"), "{}", chips[1].label);

    // Gone model: the digest resurfaces and the row admits it cannot read
    // the geometry - never a blank, never a crash, never an edited id.
    state.pending.push_back(CommandKind::DeleteByName("part".into()));
    state.drain_actions();
    state.sync_ops_ui();
    let chips = state.geo_chips();
    assert_eq!(chips.len(), 2, "both picks survive their model, visibly broken");
    assert!(chips.iter().all(|c| c.missing));
    assert!(chips[0].label.ends_with(" · missing"), "{}", chips[0].label);
    assert!(chips[1].label.ends_with(" · missing"), "{}", chips[1].label);
}

// ---------------------------------------------------------------------------
// M1.5.1 step 6: the Geometry section's note lines ride the same
// `selection_notes` walk the preview takes - through the state seam the
// pane draws from, an open run on a facing operation must say out loud
// that it runs out to the stock, and a pick whose geometry is gone must
// fall to silence that generation then owns with its refusal.

/// First box edge whose XY projection is a real segment - a vertical
/// edge projects to a point, which is not the extension the note is
/// about. Any horizontal edge behaves alike: the material-side rule
/// takes the strip inward, over the whole 10 x 10 top.
fn box_run_pick(state: &State, name: &str) -> GeoSel {
    let e = model::find_by_name(&state.world, name).expect("entity");
    let f = model::features_of(&state.world, &e).expect("features");
    for c in feature::chains(&f) {
        if c.closed {
            continue;
        }
        let p = feature::chain_points(&f, &c);
        if p.len() >= 2 && (p[0][0] != p[1][0] || p[0][1] != p[1][1]) {
            return GeoSel { entity: name.to_string(), feature: FeatureSel::Chain(c.id) };
        }
    }
    panic!("a box has horizontal edges to pick");
}

/// A Contour parameter set with a different variant than the face fixture
/// carries - the draft a retype would have produced (M1.5.2 step 1: the
/// Type control no longer builds these over a live operation, but the
/// refusal must stand behind the pane).
fn contour_draft() -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: 25.0,
        retract_z: 30.0,
        top_z: 20.0,
        bottom_z: 15.0,
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        lead: Lead::Linear { length: 8.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
    }
}

// ---------------------------------------------------------------------------
// M1.5.2 step 1: an operation's kind is fixed at creation - the Type combo
// belongs to the creation form, and anything that still reaches the command
// with a foreign variant is refused by name; Generate never runs under a
// dirty draft; and the creation form's prefilled name lands free even when
// the document's numbering has holes in it.

#[test]
fn retyping_a_live_operation_is_refused_not_silent() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    // Whatever reached the queue (a stale pane, some future surface), the
    // command layer refuses: the variant is the kind, kinds do not swap.
    state.pending.push_back(CommandKind::EditOperationParams {
        name: "Face1".into(),
        params: contour_draft(),
    });
    state.drain_actions();
    assert!(state.status.starts_with("parameters not updated"), "{:?}", state.status);
    assert!(
        state.error_for(ErrorScope::Operation).is_some_and(|m| m.contains("new operation")),
        "the refusal names the way to change kind: {:?}",
        state.error_for(ErrorScope::Operation)
    );
    assert_eq!(stored_params(&state, "Face1"), face_params(), "the operation kept its kind");
}

#[test]
fn generate_flushes_a_dirty_draft_before_generating() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    part_and_target(&mut state);
    state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    state.drain_actions();
    assert_eq!(status(&state, "Face1"), OpStatus::Ok);

    // Spinbox typing the frame has not Applied: the panel shows a new cut
    // height while the operation still holds the old one. Generating under
    // that draft used to cut the old numbers silently.
    state.op_ui.params = OpParams::Face {
        stepover: 3.0,
        clearance_z: 25.0,
        cut_z: 18.0,
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    state.generate_operation("Face1");
    assert_eq!(state.pending.len(), 2, "the edit rides the click, then the generate");
    state.drain_actions();
    assert!(state.status.starts_with("generated Face1:"), "{:?}", state.status);
    assert!(
        matches!(stored_params(&state, "Face1"),
            OpParams::Face { cut_z, .. } if (cut_z - 18.0).abs() < 1e-12),
        "the path came from what the panel showed"
    );
    assert_eq!(status(&state, "Face1"), OpStatus::Ok, "fresh, not stale: it matched the edit");

    // A clean draft flushes nothing - one command, no pointless history.
    state.generate_operation("Face1");
    assert_eq!(state.pending.len(), 1, "nothing to apply first");
    state.drain_actions();
}

#[test]
fn generate_never_flushes_a_draft_from_another_page() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    // The new-op form is open (its draft names no live operation): the
    // menu's Generate on the selected operation must not pour the form's
    // numbers into it. The draft's name is what decides.
    state.op_ui = OpUi {
        name: "Operation9".into(),
        tool: "EM-0400-2FL".into(),
        ..OpUi::default()
    };
    state.pending.push_back(CommandKind::Select(Some("Face1".into())));
    state.drain_actions();
    state.generate_operation("Face1");
    assert_eq!(state.pending.len(), 1, "only the generation rides the queue");
    state.drain_actions();
    assert_eq!(stored_params(&state, "Face1"), face_params(), "the form's draft stayed out of it");
}

#[test]
fn new_operation_name_prefill_skips_taken_names() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    // Numbering with a hole that a high name fills: count+1 (2 operations)
    // would prefill the taken "Operation3" and make Add look dead.
    for name in ["Operation1", "Operation3"] {
        state.pending.push_back(CommandKind::CreateOperation {
            name: name.into(),
            tool: "EM-0400-2FL".into(),
            src: SrcRef::Stock,
            params: face_params(),
        });
    }
    state.drain_actions();
    // Mirror one op into the draft, then fall back to the creation form:
    // the prefill arm runs on that identity change.
    state.ui_target = UiTarget::Operation("Operation1".into());
    state.sync_ops_ui();
    assert_eq!(state.op_ui.name, "Operation1");
    state.ui_target = UiTarget::None;
    state.sync_ops_ui();
    assert_eq!(state.op_ui.name, "Operation4", "numbered past the taken names, not the count");
    assert!(model::find_by_name(&state.world, &state.op_ui.name).is_none());
}

#[test]
fn geometry_notes_report_through_the_panel_seam() {
    let mut state = State::new();
    stock_and_tool(&mut state);
    create_face(&mut state);
    part_and_target(&mut state);

    let run = box_run_pick(&state, "part");
    queue_feature_pick(&mut state, run, false);
    state.drain_actions();
    state.sync_ops_ui();
    assert_eq!(state.op_ui.src_kind, 2, "the pick reached the draft");
    let notes = operation::selection_notes(&state.world, &state.op_ui.geo, &state.op_ui.params);
    assert!(
        notes.iter().any(|n| n.contains("extended to the stock boundary")),
        "the pane would draw: {notes:?}"
    );
    assert!(
        !notes.iter().any(|n| n.contains("outside")),
        "the material side won outright, nothing to flip: {notes:?}"
    );

    // Gone model: the note walk falls to empty rather than contradicting
    // the refusal generation will make.
    state.pending.push_back(CommandKind::DeleteByName("part".into()));
    state.drain_actions();
    state.sync_ops_ui();
    let notes = operation::selection_notes(&state.world, &state.op_ui.geo, &state.op_ui.params);
    assert!(notes.is_empty(), "stale picks say nothing: {notes:?}");
}

// M1.5.2 step 5: Auto joins the pocket's Strategy combo. The label list
// is the single source the combo renders from, so the pane and the test
// cannot drift apart.
#[test]
fn strategy_combo_offers_auto() {
    assert_eq!(
        hsmworks_ui::operations::strategy_names(),
        &["Zigzag", "Waterline", "Auto"],
        "the combo offers Auto beside the explicit choices"
    );
}
