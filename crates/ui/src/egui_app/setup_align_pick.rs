//! The Setups page's part-alignment control (#326): how the part sits on
//! the table in one setup - flipped, turned, or as modelled - by a quick
//! choice (turned about the part's centre) or by typed angles (turned
//! about the setup's WCS origin by default, #606, or the part's centre,
//! by choice - the row says which). Every change queues the undoable
//! `AlignPartToSetup` (through `CommandKind::AlignPart`); nothing here
//! writes the world. The work offset is a separate row (its own picker).
//!
//! Provenance: our own UI over plain shop practice (re-clamping a part
//! face-down or turned); no vendor material consulted.

use egui::{ComboBox, DragValue, Ui};

use camcraft_app::setup_align::{self, AlignPreset};
use camcraft_app::setup_turn::TurnPivot;

use crate::state::{CommandKind, State};

/// The words and angles for a stored alignment live in the app (the
/// setup sheet prints the same words); the page reads them from there.
pub(crate) use camcraft_app::setup_align::{angles_of, describe as align_text};

/// One setup row's control: the quick-choice combo, then the typed angles,
/// then the pivot and Apply on a line of their own (the properties pane
/// is narrow - #330's controls ran off its edge on one long line).
pub(super) fn picker(ui: &mut Ui, state: &mut State, setup: &str) {
    let current = setup_align::stored(&state.world, setup);
    let mut queued: Option<([f64; 3], TurnPivot)> = None;
    ui.horizontal(|ui| {
        ui.label("    Part alignment");
        ComboBox::from_id_salt(("setup_align", setup))
            .selected_text(align_text(current))
            .show_ui(ui, |ui| {
                for p in AlignPreset::ALL {
                    let held = align_text(current) == p.label();
                    if ui.selectable_label(held, p.label()).clicked() && !held {
                        queued = Some((p.angles(), TurnPivot::PartCentre));
                    }
                }
            })
            .response
            .on_hover_text(
                "How the part sits on the table in this setup, turned about the part's centre. \
                 The stock follows and is rebuilt square to the table; the work offset is set \
                 separately below.",
            );
    });
    let id = ui.id().with(("setup_align_draft", setup));
    let (mut draft, mut pivot): ([f64; 3], TurnPivot) =
        ui.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || (angles_of(current), TurnPivot::default())));
    ui.horizontal(|ui| {
        ui.label("      Turn");
        for (axis, v) in ["X", "Y", "Z"].into_iter().zip(draft.iter_mut()) {
            ui.label(axis);
            ui.add(DragValue::new(v).speed(1.0).range(-360.0..=360.0).suffix("°"));
        }
    });
    ui.horizontal(|ui| {
        ui.label("      about");
        ComboBox::from_id_salt(("setup_align_pivot", setup))
            .selected_text(pivot.label())
            .show_ui(ui, |ui| {
                for p in TurnPivot::ALL {
                    ui.selectable_value(&mut pivot, p, p.label());
                }
            })
            .response
            .on_hover_text(
                "What stays put as the part turns: the WCS origin keeps the part's point on \
                 the origin there; the part centre keeps the centre of its bounds put.",
            );
        let tip = match pivot {
            TurnPivot::WcsOrigin => {
                "Turn the part to these angles (X, then Y, then Z, from as modelled) about the WCS \
                 origin: the part keeps its place relative to the origin."
            }
            TurnPivot::PartCentre => {
                "Turn the part to these angles (X, then Y, then Z, from as modelled) about the \
                 centre of its bounds."
            }
        };
        if ui.button("Apply").on_hover_text(tip).clicked() {
            queued = Some((draft, pivot));
        }
    });
    if let Some((angles, by)) = queued {
        draft = angles;
        state.pending.push_back(CommandKind::AlignPart { setup: setup.to_owned(), angles, pivot: by });
    }
    ui.data_mut(|d| d.insert_temp(id, (draft, pivot)));
}
