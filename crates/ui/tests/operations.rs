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
use hsmworks_ui::state::{CommandKind, ErrorScope, State, ToolUi, UiTarget};

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
