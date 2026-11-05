//! The operation page's Boundary group (M11.5): tool containment, the
//! grow / shrink offset with its corner style, the extension past the
//! exterior edges, extra include / exclude boundaries and the open
//! boundary with its side. It sits on the Geometry tab, under the picks,
//! because it answers the same question the selection does - where the
//! tool may go - and a seventh tab would split one question over two
//! pages. Every field edits the page's draft (`OpUi::area`); Apply
//! commits it with the parameters as one undoable edit. Viewport picks
//! for a boundary are routed here by `State::boundary_pick` - transient
//! view state, never a command.
//!
//! Provenance: our own UI; no vendor material consulted.

use egui::Ui;

use camcraft_app::operation::{
    AreaControls, BoundaryMode, BoundaryShape, ChainSide, Containment, CornerStyle, ExtraBoundary, GeoSel,
    OpenBoundary, area_applies,
};

use crate::state::{BoundaryPick, State};

use super::pages::{combo, field, num};

/// Draw the group for the open operation's draft; nothing for a kind
/// without a machining area.
pub(super) fn boundary_group(ui: &mut Ui, state: &mut State) {
    if !area_applies(state.op_ui.params.kind()) {
        state.boundary_pick = None;
        return;
    }
    let u = state.units();
    let set = !state.op_ui.area.is_default();
    ui.separator();
    egui::CollapsingHeader::new("Boundary")
        .id_salt("op_boundary")
        .default_open(set)
        .show(ui, |ui| body(ui, state, u));
}

fn body(ui: &mut Ui, state: &mut State, u: camcraft_app::units::Unit) {
    let a = &mut state.op_ui.area;
    field(ui, "Tool containment");
    let mut c = match a.containment {
        None => 0,
        Some(Containment::Inside) => 1,
        Some(Containment::On) => 2,
        Some(Containment::Outside) => 3,
    };
    combo(ui, "area_containment", &mut c, &["As the kind cuts", "Inside", "Centre on", "Outside"]);
    a.containment = match c {
        1 => Some(Containment::Inside),
        2 => Some(Containment::On),
        3 => Some(Containment::Outside),
        _ => None,
    };
    field(ui, "Boundary offset (+ grows)");
    num(ui, u, &mut a.offset);
    let mut corner = usize::from(a.corner == CornerStyle::Sharp);
    combo(ui, "area_corner", &mut corner, &["Round corners", "Sharp corners"]);
    a.corner = if corner == 1 { CornerStyle::Sharp } else { CornerStyle::Round };
    field(ui, "Extend past open edges");
    num(ui, u, &mut a.extension);
    if a.extension < 0.0 {
        a.extension = 0.0;
    }

    field(ui, "Extra boundaries");
    let mut remove = None;
    for (i, b) in a.extra.iter().enumerate() {
        let what = match &b.shape {
            BoundaryShape::Picks(p) => format!("{} pick(s)", p.len()),
            BoundaryShape::Stock => "stock outline".to_owned(),
            BoundaryShape::Silhouette => "part silhouette".to_owned(),
        };
        let mode = if b.mode == BoundaryMode::Include { "Include" } else { "Exclude" };
        ui.horizontal(|ui| {
            ui.label(format!("{mode}: {what}"));
            if ui.small_button("×").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        a.extra.remove(i);
    }
    ui.horizontal_wrapped(|ui| {
        for (label, mode, shape) in [
            ("+ Include stock", BoundaryMode::Include, BoundaryShape::Stock),
            ("+ Exclude stock", BoundaryMode::Exclude, BoundaryShape::Stock),
            ("+ Include silhouette", BoundaryMode::Include, BoundaryShape::Silhouette),
            ("+ Exclude silhouette", BoundaryMode::Exclude, BoundaryShape::Silhouette),
        ] {
            if ui.small_button(label).clicked() {
                a.extra.push(ExtraBoundary { mode, shape });
            }
        }
    });

    field(ui, "Open boundary");
    let mut clear = false;
    match &mut a.open {
        None => {
            ui.label("None - the selection bounds the area.");
        }
        Some(o) => {
            ui.label(format!("{} pick(s), closed off by the stock outline", o.picks.len()));
            let mut side = usize::from(o.side == ChainSide::Outside);
            combo(ui, "area_open_side", &mut side, &["Machine inside", "Machine outside"]);
            o.side = if side == 1 { ChainSide::Outside } else { ChainSide::Inside };
            clear = ui.small_button("Clear open boundary").clicked();
        }
    }
    if clear {
        a.open = None;
    }

    ui.horizontal_wrapped(|ui| {
        for (label, target) in [
            ("Pick include", BoundaryPick::Include),
            ("Pick exclude", BoundaryPick::Exclude),
            ("Pick open boundary", BoundaryPick::Open),
        ] {
            let on = state.boundary_pick == Some(target);
            if ui.selectable_label(on, label).clicked() {
                state.boundary_pick = if on { None } else { Some(target) };
            }
        }
    });
    if state.boundary_pick.is_some() {
        ui.label("Face/Edge clicks now fill this boundary (Ctrl adds) - click the button again to pick the selection.");
    }
    if ui.small_button("Reset boundary").clicked() {
        state.op_ui.area = AreaControls::default();
        state.boundary_pick = None;
    }
}

/// A Face/Edge pick while the group is collecting: into the draft's
/// boundary (plain click starts a new one, Ctrl/Shift adds to the last).
/// A pure draft write - Apply commits it, undo is the history's.
pub(crate) fn take_pick(state: &mut State, target: BoundaryPick, pick: GeoSel, append: bool) {
    let a = &mut state.op_ui.area;
    let label = format!("{} of '{}'", pick.feature, pick.entity);
    match target {
        BoundaryPick::Include | BoundaryPick::Exclude => {
            let mode = if target == BoundaryPick::Include { BoundaryMode::Include } else { BoundaryMode::Exclude };
            let last = a.extra.iter_mut().rev().find(|b| b.mode == mode && matches!(b.shape, BoundaryShape::Picks(_)));
            match (append, last) {
                (true, Some(ExtraBoundary { shape: BoundaryShape::Picks(p), .. })) => {
                    if !p.contains(&pick) {
                        p.push(pick);
                    }
                }
                _ => a.extra.push(ExtraBoundary { mode, shape: BoundaryShape::Picks(vec![pick]) }),
            }
        }
        BoundaryPick::Open => match (&mut a.open, append) {
            (Some(o), true) => {
                if !o.picks.contains(&pick) {
                    o.picks.push(pick);
                }
            }
            _ => {
                let side = a.open.as_ref().map_or(ChainSide::Inside, |o| o.side);
                a.open = Some(OpenBoundary { picks: vec![pick], side });
            }
        },
    }
    state.status = format!("{label} added to the boundary draft - Apply commits it");
}
