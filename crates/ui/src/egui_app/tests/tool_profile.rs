//! #93: the tool editor's Profile section - a milling tool shows Body /
//! Profile tabs (a turning tool the Body section alone), Parametric
//! follows the numbers, Custom keeps hand edits, Apply profile lands one
//! undoable command, and a bad outline is refused by name, live and at
//! Apply. Provenance: our own page law; the outlines are our own.

use super::*;
use camcraft_app::spun::SpunPoint;

const TOOL: &str = "EM-0600-4FL";

fn editor() -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    harness.run();
    crate::actions::open_tool_editor(&mut harness.state_mut().state, TOOL);
    harness.run();
    harness.run();
    harness
}

/// The editor with the Profile tab open.
fn profile_tab() -> Harness<'static, ShellUi> {
    let mut harness = editor();
    harness.get_by_label("Profile").click();
    harness.run();
    harness.run();
    harness
}

fn drawn(state: &State) -> Option<Vec<SpunPoint>> {
    model::find_by_name(&state.world, TOOL)
        .and_then(|e| camcraft_app::tool_body::body_of(&state.world, &e))
        .and_then(|b| b.spun)
        .map(|s| s.points().to_vec())
}

#[test]
fn a_milling_tool_offers_the_profile_tab_and_a_turning_tool_does_not() {
    let mut harness = profile_tab();
    for label in ["Parametric", "Custom", "Apply profile", "Holder profile"] {
        assert!(harness.query_by_label(label).is_some(), "{label} stands in the Profile tab");
    }
    assert!(harness.query_by_label("Import body…").is_none(), "the Body section is behind its tab");
    // A turning insert: no tabs, the Body section as before.
    harness.state_mut().state.tool_ui.kind = 6;
    harness.run();
    harness.run();
    assert!(harness.query_by_label("Profile").is_none(), "turning tools are not spun profiles");
    assert!(harness.query_by_label("Import body…").is_some());
}

#[test]
fn parametric_follows_the_numbers_and_custom_keeps_hand_edits() {
    let mut harness = profile_tab();
    let d0 = harness.state().state.tool_ui.holder_d;
    let r = |h: &Harness<'static, ShellUi>| h.state().state.tool_profile_ui.points.get(1).map(|p| p.r);
    assert_eq!(r(&harness), Some(d0 * 0.5), "parametric: the holder cylinder");
    harness.state_mut().state.tool_ui.holder_d = d0 + 10.0;
    harness.run();
    assert_eq!(r(&harness), Some((d0 + 10.0) * 0.5), "parametric redraws as the numbers change");
    harness.get_by_label("Custom").click();
    harness.run();
    harness.state_mut().state.tool_profile_ui.points[1].r = 30.0;
    harness.state_mut().state.tool_ui.holder_d = d0;
    harness.run();
    assert_eq!(r(&harness), Some(30.0), "custom keeps the hand edit");
    assert!(harness.state().state.pending.is_empty(), "editing the draft is not a command");
    assert!(drawn(&harness.state().state).is_none());
}

#[test]
fn apply_profile_lands_one_command_and_undo_restores() {
    let mut harness = profile_tab();
    harness.get_by_label("Custom").click();
    harness.run();
    harness.state_mut().state.tool_profile_ui.points[1].r = 25.0;
    harness.state_mut().state.tool_profile_ui.points[2].r = 25.0;
    harness.run();
    harness.get_by_label("Apply profile").click();
    harness.run();
    harness.run();
    let landed = drawn(&harness.state().state).expect("the drawn holder landed");
    assert_eq!(landed[1].r, 25.0, "status: {}", harness.state().state.status);
    assert!(harness.state().state.tool_profile_ui.custom, "a landed profile re-edits as Custom");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "one undo takes it off");
    assert!(!harness.state().state.tool_profile_ui.custom, "and the draft is parametric again");
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run();
    assert!(drawn(&harness.state().state).is_some(), "redo puts it back");
    // Parametric + Apply is the way back: one command, undoable.
    harness.run();
    harness.get_by_label("Parametric").click();
    harness.run();
    harness.get_by_label("Apply profile").click();
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "parametric again");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert!(drawn(&harness.state().state).is_some(), "undo restores the drawn holder");
}

#[test]
fn a_bad_outline_is_refused_by_name_live_and_at_apply() {
    let mut harness = profile_tab();
    harness.get_by_label("Custom").click();
    harness.run();
    let last = harness.state().state.tool_profile_ui.points.len() - 1;
    harness.state_mut().state.tool_profile_ui.points[last].r = 5.0;
    harness.run();
    harness.run();
    assert!(harness.query_all_by_label_contains("must end on the axis").next().is_some(), "the live refusal names it");
    harness.get_by_label("Apply profile").click();
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "nothing lands");
    assert!(harness.query_all_by_label_contains("holder profile not set").next().is_some(), "Apply refuses by name");
}

#[test]
fn insert_and_remove_edit_the_table() {
    let mut harness = profile_tab();
    let n = harness.state().state.tool_profile_ui.points.len();
    harness.state_mut().state.insert_profile_point(1);
    harness.run();
    let st = &harness.state().state;
    assert_eq!(st.tool_profile_ui.points.len(), n + 1);
    assert!(st.tool_profile_ui.custom, "an edit makes the draft custom");
    harness.state_mut().state.remove_profile_point(2);
    harness.run();
    assert_eq!(harness.state().state.tool_profile_ui.points.len(), n);
}
