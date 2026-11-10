//! The Setups page's "Axes in the plane" rows (#557): set the part's +Y
//! and/or +X in the work plane from a straight edge or two points picked
//! in the viewport, flip either, choose which one wins when both are
//! picked, and Apply. The pick draft is view state (`crate::axis_pick`);
//! Apply queues `CommandKind::TurnPartInPlane`, the undoable
//! `AlignPartToSetup`. The app says whether the picks make a turn and,
//! if not, why - shown on the disabled Apply. The work offset is not
//! touched.
//!
//! Provenance: our own UI over plain fixture practice (indicate an edge
//! along the table's travel); no vendor material consulted.

use egui::{RichText, Ui};

use camcraft_app::commands::AlignPartToSetup;
use camcraft_app::pick::{self, InPlaneAxis};

use crate::axis_pick::{self, AxisBy};
use crate::state::{CommandKind, State};

/// The rows of one setup.
pub(super) fn rows(ui: &mut Ui, state: &mut State, setup: &str) {
    state.axis_rows_shown = true;
    if !state.setup_pick_rows.iter().any(|s| s == setup) {
        state.setup_pick_rows.push(setup.to_owned()); // #664
    }
    ui.label("    Axes in the plane - pick an edge or two points for +Y and/or +X");
    for axis in [InPlaneAxis::Y, InPlaneAxis::X] {
        axis_row(ui, state, setup, axis);
    }
    let Some(d) = state.axis_pick.as_ref().filter(|d| d.setup == setup).cloned() else { return };
    let axes = d.wanted();
    if axes.y.is_none() && axes.x.is_none() {
        return;
    }
    // The command's own check: a refused pick, or one that gives the
    // alignment the setup already holds, greys Apply out with the why.
    // Just applied, the same picks are a zero turn: say Flip re-applies
    // rather than "already holds" (#557 hand check).
    let turn = pick::in_plane_turn_deg(&state.world, setup, &axes).ok();
    let held = d.applied && turn.is_some_and(|deg| deg.abs() < 1e-9);
    let check = AlignPartToSetup::check_turn(&state.world, setup, &axes).map_err(|e| {
        if held {
            "applied - tick Flip +Y / Flip +X and Apply again to turn the part the other way".to_owned()
        } else {
            e.to_string()
        }
    });
    ui.horizontal(|ui| {
        ui.label("      Primary");
        let mut primary = d.axes.primary;
        for axis in [InPlaneAxis::Y, InPlaneAxis::X] {
            ui.radio_value(&mut primary, axis, axis.label())
                .on_hover_text("With both axes picked, this one is kept exactly; the other is made square to it.");
        }
        if primary != d.axes.primary {
            axis_pick::draft_for(state, setup).axes.primary = primary;
        }
        let tip = match &check {
            Ok(()) => "Turn the part about Z so the picks run along their axes.".to_owned(),
            Err(why) => why.clone(),
        };
        if ui
            .add_enabled(check.is_ok(), egui::Button::new("Apply axes"))
            .on_hover_text(&tip)
            .on_disabled_hover_text(&tip)
            .clicked()
        {
            state.pending.push_back(CommandKind::TurnPartInPlane { setup: setup.to_owned(), axes: axes.clone() });
        }
        if ui.button("Clear axes").on_hover_text("Drop the axis picks.").clicked() {
            state.axis_pick = None;
        }
    });
    let words = match (turn, &check) {
        (_, Err(why)) => why.clone(),
        (Some(deg), Ok(())) => format!("turns the part {} about Z", deg_words(deg)),
        (None, Ok(())) => String::new(),
    };
    if !words.is_empty() {
        ui.label(RichText::new(format!("      {words}")).weak());
    }
}

fn deg_words(deg: f64) -> String {
    let r = (deg * 1000.0).round() / 1000.0;
    format!("{}°", if r == 0.0 { 0.0 } else { r })
}

/// One axis: what it is picked by, the pick toggle, flip, and the picks.
fn axis_row(ui: &mut Ui, state: &mut State, setup: &str, axis: InPlaneAxis) {
    let draft = state.axis_pick.as_ref().filter(|d| d.setup == setup);
    let by = draft.map_or(AxisBy::Edge, |d| d.by(axis));
    let armed = draft.is_some_and(|d| d.picking == Some(axis));
    let slot = draft.and_then(|d| match axis {
        InPlaneAxis::Y => d.axes.y.clone(),
        InPlaneAxis::X => d.axes.x.clone(),
    });
    let picks = slot.as_ref().map(|s| s.picks.clone()).unwrap_or_default();
    let mut flip = slot.is_some_and(|s| s.flip);
    let name = axis.label();
    ui.horizontal(|ui| {
        ui.label(format!("      {name}"));
        for (b, text) in [(AxisBy::Edge, "Edge"), (AxisBy::Points, "Two points")] {
            if ui
                .selectable_label(by == b, format!("{name} by {text}"))
                .on_hover_text(format!("Set {name} from {} - picks at once.", b.noun()))
                .clicked()
            {
                // Choosing a mode arms its pick (#557 hand check).
                axis_pick::set_by(state, setup, axis, b);
            }
        }
        let what = match by {
            AxisBy::Edge => "edge",
            AxisBy::Points => "points",
        };
        let text = if armed { format!("Stop picking {name}") } else { format!("Pick {name} {what}") };
        if ui
            .selectable_label(armed, text)
            .on_hover_text(format!("Click {} in the viewport; it runs along {name}.", by.noun()))
            .clicked()
        {
            if armed {
                axis_pick::disarm(state);
            } else {
                axis_pick::arm(state, setup, axis);
            }
        }
        if ui
            .checkbox(&mut flip, format!("Flip {name}"))
            .on_hover_text("Take the pick the other way (end to start).")
            .changed()
        {
            axis_pick::draft_for(state, setup).slot(axis).flip = flip;
        }
    });
    // Its own line, so it is never cut short: what and where, in the
    // document's units, never a feature id (#557 hand check).
    let text = crate::axis_pick::picks_text(&state.world, &picks, camcraft_app::units::of(&state.world));
    ui.label(RichText::new(format!("        {name}: {text}")).weak());
}
