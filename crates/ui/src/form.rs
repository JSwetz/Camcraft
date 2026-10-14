//! Shared form plumbing for the context pages: a field's title goes ABOVE
//! its control, which then spans the page - the stacked property-form
//! pattern (hand-test: side labels forced every dialog wider than the
//! docked pane it lives in).
//!
//! Provenance: our own helper, specced from the form pages of public CAM
//! shells; strings and layout metrics are ours (AGENTS.md clean-room).
//!
//! Radios and checkboxes deliberately keep side labels: a tick or dot
//! read with its text is one object, not a titled field.

use dear_imgui_rs::Ui;

/// One stacked field: the label line, then full page width for whatever
/// widget comes next (`SetNextItemWidth` applies to exactly one item).
pub(crate) fn field(ui: &Ui, label: &str) {
    ui.text(label);
    ui.new_line();
    // Clamped to a sane floor: a pane narrower than the min is unusable
    // anyway, and the width must stay finite and positive for the widget.
    let avail = ui.content_region_avail()[0];
    ui.set_next_item_width(avail.max(40.0));
}
