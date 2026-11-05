//! The adaptive "Optimal load (% of D)" field (M11.4 B): edits the
//! ae/D ratio the params store as a percentage of the tool diameter and
//! quotes the derived contact angle `acos(1 - 2 ae/D)` as a hint.
//! Provenance: straight-wall radial-engagement geometry of peripheral
//! milling (machining handbooks; `camcraft_app::load` carries the law).

use egui::{DragValue, Ui};

use camcraft_app::load::degrees_of_load;

/// The field's label - one spelling for every adaptive pane.
pub(super) const OPTIMAL_LOAD_LABEL: &str = "Optimal load (% of D)";

/// Draw the percent editor over `load` (a ratio). The draft is written
/// only when the user moved the number, so a value loaded from a file
/// (the exact 45-degree default, 14.6446...%) is never re-rounded by a
/// redraw - the EditOperationParams commit path sees no phantom edit.
/// No clamp: the panel never normalizes, the parameter door refuses an
/// out-of-range load by name (0 < ae/D <= 50%).
pub(super) fn optimal_load_field(ui: &mut Ui, load: &mut f64) {
    super::pages::field(ui, OPTIMAL_LOAD_LABEL);
    let mut pct = *load * 100.0;
    let resp = ui.add(
        DragValue::new(&mut pct)
            .speed(0.1)
            .max_decimals(2)
            .suffix(" %"),
    );
    if resp.changed() {
        *load = pct / 100.0;
    }
    ui.label(format!(
        "radial width ae = {:.2}% of the tool diameter - about {:.1} deg of cutter contact (acos(1 - 2 ae/D)); rings step by ae",
        *load * 100.0,
        degrees_of_load(*load)
    ));
}
