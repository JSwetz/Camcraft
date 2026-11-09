//! The locate-in-fixture tools (#561), drawn on the Fixtures page (a
//! "Locate part…" verb per fixture row) and on the Model page (a part's
//! "Locate in fixture" row, a fixture's "Locate part…"): pick a part face
//! or point, then the fixture face or point it sits on - one to three
//! mates (floor, jaw, stop) - choose which body moves, nudge X / Y / Z
//! and turn, Apply. The picks are view state (`crate::locate_pick`);
//! Apply queues `CommandKind::Locate`, one undoable `Locate`. The app
//! says whether the mates solve and, if not, why - shown on the disabled
//! Apply.
//!
//! Provenance: our own UI over 3-2-1 fixturing practice; no vendor
//! material consulted.

use egui::{RichText, Ui};

use camcraft_app::fixture_locate::{self, Mover};

use crate::locate_pick::{self, LocateDraft};
use crate::state::{CommandKind, State};

use super::fields::{num, num_plain};
use super::layout::prop_row;

/// The verb that arms (or stops) the mate picks for `fixture`; `part`
/// when the page already knows the part.
pub(super) fn arm_button(ui: &mut Ui, state: &mut State, label: &str, fixture: &str, part: Option<&str>) {
    let armed = armed_for(state, fixture, part);
    if ui
        .selectable_label(armed, if armed { "Stop locating" } else { label })
        .on_hover_text("Pick a part face or point, then the fixture face or point it sits on - up to three mates.")
        .clicked()
    {
        if armed {
            locate_pick::disarm(state);
        } else {
            locate_pick::arm(state, fixture, part);
        }
    }
}

/// Is the draft collecting for this fixture (and this part, when given)?
pub(super) fn armed_for(state: &State, fixture: &str, part: Option<&str>) -> bool {
    state
        .locate_pick
        .as_ref()
        .is_some_and(|d| d.fixture == fixture && part.is_none_or(|p| d.part.is_empty() || d.part == p))
}

/// The armed draft's form: the mates, which body moves, the nudge, Apply.
pub(super) fn form(ui: &mut Ui, state: &mut State) {
    let Some(d) = state.locate_pick.clone() else { return };
    let u = state.units();
    ui.label(RichText::new(format!("Locate {} in {}", if d.part.is_empty() { "the part" } else { &d.part }, d.fixture)).strong());
    for (i, m) in d.mates.iter().enumerate() {
        ui.label(format!("  Mate {}: {} of '{}' on {} of '{}'", i + 1, m.part.feature, m.part.entity, m.fixture.feature, m.fixture.entity));
    }
    if let Some(h) = &d.half {
        ui.label(format!("  Mate {}: {} of '{}' on …", d.mates.len() + 1, h.feature, h.entity));
    }
    ui.label(RichText::new(format!("  Next: {}", locate_pick::next_hint(&d))).weak());
    let mut mover = d.mover;
    ui.horizontal(|ui| {
        ui.radio_value(&mut mover, Mover::Part, "Move the part")
            .on_hover_text("The part (and the stock that follows it) goes into the fixture.");
        ui.radio_value(&mut mover, Mover::Fixture, "Move the fixture")
            .on_hover_text("The fixture comes to the part; the part keeps its place.");
    });
    let mut off = d.offset;
    prop_row(ui, "Offset X", |ui| num(ui, u, &mut off.dx));
    prop_row(ui, "Offset Y", |ui| num(ui, u, &mut off.dy));
    prop_row(ui, "Offset Z", |ui| num(ui, u, &mut off.dz));
    prop_row(ui, "Turn about mate normal", |ui| num_plain(ui, &mut off.turn_deg));
    if let Some(live) = state.locate_pick.as_mut() {
        live.mover = mover;
        live.offset = off;
    }
    let draft = LocateDraft { mover, offset: off, ..d };
    // The solve the command will run: a refused one greys Apply out,
    // with the why on hover and on the line below.
    let check = fixture_locate::plan(&state.world, &draft.spec()).and_then(|p| {
        if fixture_locate::is_still(p.delta) { Err("nothing would move".to_owned()) } else { Ok(()) }
    });
    ui.horizontal_wrapped(|ui| {
        ui.add_space(12.0);
        let tip = match &check {
            Ok(()) => "Move it there - one undo step.".to_owned(),
            Err(why) => why.clone(),
        };
        if ui
            .add_enabled(check.is_ok(), egui::Button::new("Apply locate"))
            .on_hover_text(&tip)
            .on_disabled_hover_text(&tip)
            .clicked()
        {
            state.pending.push_back(CommandKind::Locate { spec: draft.spec(), mover });
        }
        if ui.small_button("Remove last").clicked() {
            locate_pick::remove_last(state);
        }
    });
    if let (Err(why), false) = (&check, draft.mates.is_empty()) {
        ui.label(RichText::new(format!("  {why}")).weak());
    }
}

/// The Model page's row: a part picks its fixture, a fixture locates the
/// part into itself.
pub(super) fn model_row(ui: &mut Ui, state: &mut State, name: &str) {
    let Some(e) = camcraft_app::model::find_by_name(&state.world, name) else { return };
    let is_fixture = camcraft_app::model::is_fixture(&state.world, &e);
    ui.add_space(6.0);
    ui.label("Locate in a fixture");
    if is_fixture {
        ui.horizontal_wrapped(|ui| arm_button(ui, state, "Locate part…", name, None));
        if armed_for(state, name, None) {
            form(ui, state);
        }
        return;
    }
    if !camcraft_app::setup_align::is_part(&state.world, &e) {
        return;
    }
    let fixtures: Vec<String> = camcraft_app::model::model_entities(&state.world)
        .into_iter()
        .filter(|(_, f)| camcraft_app::model::is_fixture(&state.world, f))
        .map(|(n, _)| n)
        .collect();
    if fixtures.is_empty() {
        ui.label(RichText::new("  No fixture yet - import one on a setup's Fixtures page.").weak());
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for f in &fixtures {
            arm_button(ui, state, &format!("Locate in {f}…"), f, Some(name));
        }
    });
    if fixtures.iter().any(|f| armed_for(state, f, Some(name))) {
        form(ui, state);
    }
}
