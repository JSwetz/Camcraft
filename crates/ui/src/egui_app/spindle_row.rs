//! The Feeds tab's editable spindle speed (#225): rpm, or the cutting
//! (surface) speed through the tool's diameter - m/min in a millimetre
//! document, SFM in an inch one. The speed lives on the op's tool (the
//! post reads `Tool::rpm`), so a committed value is one undoable
//! `ChangeTool`, the same law the tool-material and flutes rows beside it
//! keep; a drag commits on release, typing on Enter / focus loss, never
//! once per frame. Provenance: the handbook relation Vc = pi D n
//! (machine-tool handbooks; `camcraft_tools::cutting` uses the same).

use egui::{DragValue, Id, Ui};

use camcraft_app::tools::Tool;
use camcraft_app::units::Unit;

use super::layout::prop_row_hint;
use crate::state::{CommandKind, State};

const MM_PER_INCH: f64 = 25.4;

/// The cutting speed of `rpm` on a cutter of diameter `d_mm`: m/min in a
/// millimetre document, SFM (surface feet per minute) in an inch one.
pub(super) fn surface_speed(rpm: f64, d_mm: f64, u: Unit) -> f64 {
    let m_per_min = std::f64::consts::PI * d_mm * rpm / 1000.0;
    match u {
        Unit::Millimeters => m_per_min,
        Unit::Inches => m_per_min * 1000.0 / MM_PER_INCH / 12.0,
    }
}

/// The rpm that gives cutting speed `vc` ([`surface_speed`]'s unit) on a
/// cutter of diameter `d_mm`; `None` for a diameter or speed that names no
/// spindle speed (zero, negative, non-finite).
pub(super) fn rpm_for(vc: f64, d_mm: f64, u: Unit) -> Option<f64> {
    if !(vc.is_finite() && vc > 0.0 && d_mm.is_finite() && d_mm > 0.0) {
        return None;
    }
    let m_per_min = match u {
        Unit::Millimeters => vc,
        Unit::Inches => vc * 12.0 * MM_PER_INCH / 1000.0,
    };
    let rpm = m_per_min * 1000.0 / (std::f64::consts::PI * d_mm);
    rpm.is_finite().then_some(rpm)
}

/// The `ChangeTool` that sets `tool`'s spindle speed to `rpm`, or `None`
/// when `rpm` is not a usable speed or is the tool's already.
pub(super) fn set_rpm(name: &str, tool: &Tool, rpm: f64) -> Option<CommandKind> {
    if !(rpm.is_finite() && rpm > 0.0) || (rpm - tool.rpm).abs() < 1e-6 {
        return None;
    }
    Some(CommandKind::ChangeTool { name: name.to_owned(), tool: Tool { rpm, ..tool.clone() } })
}

/// The row: the speed in the chosen unit, the rpm / surface switch, and
/// the commit law above.
pub(super) fn row(ui: &mut Ui, state: &mut State, name: &str, tool: &Tool) {
    let u = state.units();
    let as_vc = state.spindle_as_vc;
    let shown = if as_vc { surface_speed(tool.rpm, tool.diameter, u) } else { tool.rpm };
    let suffix = match (as_vc, u) {
        (false, _) => " rpm",
        (true, Unit::Millimeters) => " m/min",
        (true, Unit::Inches) => " SFM",
    };
    let help = format!("edits tool '{name}' - every operation using it");
    let mut flip = false;
    let mut commit = None;
    prop_row_hint(ui, "Spindle speed", &help, |ui| {
        let label = if as_vc { "surface" } else { "rpm" };
        if ui.small_button(label).on_hover_text("show the speed as rpm or as surface (cutting) speed").clicked() {
            flip = true;
        }
        // A draft rides egui's temp memory while the field is held, so
        // a drag or a half-typed number is not a command per frame.
        let id = Id::new(("spindle_speed", name, as_vc));
        let mut v = ui.data(|d| d.get_temp::<f64>(id)).unwrap_or(shown);
        let resp = ui.add(DragValue::new(&mut v).range(0.0..=200_000.0).speed(10.0).max_decimals(1).suffix(suffix));
        if resp.dragged() || resp.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, v));
        }
        if resp.drag_stopped() || resp.lost_focus() || (resp.changed() && !resp.dragged() && !resp.has_focus()) {
            ui.data_mut(|d| d.remove::<f64>(id));
            commit = Some(v);
        }
    });
    if flip {
        state.spindle_as_vc = !as_vc;
    }
    if let Some(v) = commit {
        let rpm = if as_vc { rpm_for(v, tool.diameter, u) } else { Some(v) };
        match rpm.and_then(|r| set_rpm(name, tool, r)) {
            Some(cmd) => state.pending.push_back(cmd),
            None if rpm.is_none_or(|r| r.is_nan() || r <= 0.0) => {
                state.status = String::from("spindle speed not changed: it must be above zero");
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_speed_and_rpm_invert_in_both_units() {
        // 10 mm cutter at 3183.1 rpm is 100 m/min (pi * 10 * 3183.1 / 1000).
        let vc = surface_speed(3183.0988618379, 10.0, Unit::Millimeters);
        assert!((vc - 100.0).abs() < 1e-6, "{vc}");
        // 1/2" (12.7 mm) at 1000 rpm: pi * 0.5 * 1000 / 12 = 130.9 SFM.
        let sfm = surface_speed(1000.0, 12.7, Unit::Inches);
        assert!((sfm - 130.899_693_899_6).abs() < 1e-6, "{sfm}");
        for u in [Unit::Millimeters, Unit::Inches] {
            let r = rpm_for(surface_speed(2500.0, 6.0, u), 6.0, u).expect("a speed");
            assert!((r - 2500.0).abs() < 1e-6, "{u:?}: {r}");
        }
        assert!(rpm_for(100.0, 0.0, Unit::Millimeters).is_none(), "no diameter, no rpm");
        assert!(rpm_for(-1.0, 6.0, Unit::Millimeters).is_none(), "a negative speed is none");
    }

    #[test]
    fn a_new_speed_is_one_change_tool_and_a_same_or_bad_one_is_none() {
        let (name, tool) = camcraft_app::tools::catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").expect("catalog");
        match set_rpm(name, &tool, 9000.0) {
            Some(CommandKind::ChangeTool { name: n, tool: t }) => {
                assert_eq!(n, name);
                assert_eq!(t.rpm, 9000.0);
                assert_eq!(Tool { rpm: tool.rpm, ..t }, tool, "only the speed changes");
            }
            other => panic!("expected a ChangeTool, got {other:?}"),
        }
        assert!(set_rpm(name, &tool, tool.rpm).is_none(), "the same speed is no command");
        assert!(set_rpm(name, &tool, 0.0).is_none() && set_rpm(name, &tool, f64::NAN).is_none());
    }
}
