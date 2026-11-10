//! The Setups page's "Lay on a plane" row (#330): pick one flat face in
//! the viewport (#554: a face only - an edge cannot set a plane; edges
//! set an axis in the "Axes in the plane" rows), choose G17 (XY), G18
//! (ZX) or G19 (YZ), flip it, optionally set the face onto the plane's
//! level, and Apply. The pick draft is view state (`crate::plane_pick`); Apply queues
//! `CommandKind::LayPartOnPlane`, the undoable `AlignPartToSetup`. The
//! app says whether the picks make a plane and, if not, why - shown on
//! the disabled Apply. The work offset is not touched.
//!
//! Provenance: our own UI over plain datum-face practice; no vendor
//! material consulted.

use egui::{ComboBox, RichText, Ui};

use camcraft_app::cl::ArcPlane;
use camcraft_app::commands::AlignPartToSetup;
use camcraft_app::pick::{self, PlaneLay};

use crate::state::{CommandKind, State};

const PLANES: [ArcPlane; 3] = [ArcPlane::Xy, ArcPlane::Xz, ArcPlane::Yz];

/// The row's options, kept between frames (view state).
#[derive(Clone, Copy)]
struct Draft {
    plane: ArcPlane,
    flip: bool,
    place: bool,
}

impl Default for Draft {
    fn default() -> Self {
        Draft { plane: ArcPlane::Xy, flip: false, place: true }
    }
}

/// One setup row's control.
pub(super) fn row(ui: &mut Ui, state: &mut State, setup: &str) {
    let id = ui.id().with(("setup_plane_draft", setup));
    let mut d: Draft = ui.data_mut(|m| *m.get_temp_mut_or_insert_with(id, Draft::default));
    if !state.setup_pick_rows.iter().any(|s| s == setup) {
        state.setup_pick_rows.push(setup.to_owned()); // #664
    }
    let mine = state.plane_pick.as_ref().is_some_and(|p| p.setup == setup);
    let armed = mine && crate::plane_pick::collecting(state);
    ui.horizontal(|ui| {
        ui.label("    Lay on a plane - pick a face");
        let pick_text = if armed { "Stop picking" } else { "Pick a face" };
        if ui
            .selectable_label(armed, pick_text)
            .on_hover_text("Click one flat face in the viewport; it is laid on the plane.")
            .clicked()
        {
            if armed {
                crate::plane_pick::disarm(state);
            } else {
                crate::plane_pick::arm(state, setup);
            }
        }
        ComboBox::from_id_salt(("setup_plane", setup))
            .selected_text(pick::plane_label(d.plane))
            .show_ui(ui, |ui| {
                for p in PLANES {
                    ui.selectable_value(&mut d.plane, p, pick::plane_label(p));
                }
            })
            .response
            .on_hover_text("The plane the picked face is laid on.");
        ui.checkbox(&mut d.flip, "Flip")
            .on_hover_text("Face the other way: down (-Z) for G17, -Y for G18, -X for G19.");
        ui.checkbox(&mut d.place, "Face to plane level")
            .on_hover_text("Set the picked face onto the plane (e.g. Z 0 for G17); off keeps the part where it is.");
    });
    if mine {
        let picks = state.plane_pick.as_ref().map(|p| p.picks.clone()).unwrap_or_default();
        let lay = PlaneLay { plane: d.plane, flip: d.flip, place: d.place };
        // The command's own check: a refused pick, or one that gives the
        // alignment the setup already holds, greys Apply out with the why.
        let check = AlignPartToSetup::check_lay(&state.world, setup, &picks, lay).map_err(|e| e.to_string());
        let side = pick::plane_side(&state.world, &picks).ok();
        ui.horizontal(|ui| {
            let words: Vec<String> = picks.iter().map(|g| format!("{} of '{}'", g.feature, g.entity)).collect();
            let text = if words.is_empty() { "nothing picked yet".to_owned() } else { words.join(", ") };
            ui.label(RichText::new(format!("      Picked: {text}")).weak());
            let tip = match &check {
                Ok(()) => format!("Turn the part so the pick lies on {}.", pick::plane_label(d.plane)),
                Err(why) => why.clone(),
            };
            if ui
                .add_enabled(check.is_ok(), egui::Button::new("Apply"))
                .on_hover_text(&tip)
                .on_disabled_hover_text(&tip)
                .clicked()
            {
                state.pending.push_back(CommandKind::LayPartOnPlane { setup: setup.to_owned(), picks, lay });
            }
        });
        if let Some(side) = &side {
            ui.label(RichText::new(format!("      {side}")).weak())
                .on_hover_text("The side of the pick taken as its outside: it faces the plane's axis (or away, with Flip).");
        }
    }
    ui.data_mut(|m| m.insert_temp(id, d));
}
