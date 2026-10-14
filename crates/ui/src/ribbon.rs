//! The ribbon: a fixed command strip under the menu bar, grouped
//! File | Setup | Create | Modify | Inspect, each button an icon over a
//! short label - the CAM "big buttons" pattern (layout reference: public
//! screenshots of commercial CAM shells; strings and glyphs are ours,
//! AGENTS.md clean-room).
//!
//! Provenance: our own strip. Effects live in `crate::actions` - this
//! module only decides what is enabled, what it is called, and which
//! glyph it wears. Everything is one row of cells so every group's
//! caption sits on the same baseline; headless (no `State::icons`)
//! renders the same cells text-only, so layout never depends on the GPU.

use dear_imgui_rs::{Condition, StyleVar, Ui, WindowFlags};

use crate::actions;
use crate::icons::{Icon, ICON_PX};
use crate::state::{State, UiTarget};
use hsmworks_app::schema::WCS_SLOTS;

/// Cell width, fixed: labels are single words, so the strip stays even.
const CELL_W: f32 = 56.0;
/// Horizontal gap between cells (and across dividers) - matches the
/// ItemSpacing pushed in `bar`, so group widths are computable.
const GAP: f32 = 4.0;
/// The window padding `bar` pushes (h, v). `height` must account for the
/// vertical pair - content starts `PAD_V` below the window top and the
/// captions hang `PAD_V` above the bottom - a height without them clipped
/// the group names (hand-test: "the titles under the icons are not
/// visible").
const PAD_H: f32 = 8.0;
const PAD_V: f32 = 4.0;

/// One ribbon button: what it shows, whether it acts, what it does.
/// `run` is a plain fn pointer (no captures) so a surface can never
/// fork logic - it can only pick which action to name.
struct Item {
    label: &'static str,
    icon: Icon,
    tip: &'static str,
    run: fn(&mut State),
    /// Computed this frame: false dims the cell and the tooltip adds the
    /// reason (pre-validation at the click site, feedback 1).
    enabled: bool,
    /// For two-state view toggles: draws the pressed backdrop while set.
    toggled: bool,
}

fn item(
    label: &'static str,
    icon: Icon,
    tip: &'static str,
    run: fn(&mut State),
    enabled: bool,
) -> Item {
    Item {
        label,
        icon,
        tip,
        run,
        enabled,
        toggled: false,
    }
}

fn run_stock(state: &mut State) {
    actions::target(state, UiTarget::Stock);
}

fn run_wcs(state: &mut State) {
    let slot = state.wcs_ui.slot.min(WCS_SLOTS.len() - 1);
    actions::target(state, UiTarget::WcsSlot(slot));
}

fn run_tool_lib(state: &mut State) {
    actions::target(state, UiTarget::Tool(String::new()));
}

fn run_new_face(state: &mut State) {
    actions::add_operation(state, 0);
}
fn run_new_contour(state: &mut State) {
    actions::add_operation(state, 1);
}
fn run_new_pocket(state: &mut State) {
    actions::add_operation(state, 2);
}
fn run_new_slot(state: &mut State) {
    actions::add_operation(state, 3);
}
fn run_new_trace(state: &mut State) {
    actions::add_operation(state, 4);
}

fn run_delete(state: &mut State) {
    actions::delete_selected(state);
}

fn run_toggle_paths(state: &mut State) {
    state.show_all_paths = !state.show_all_paths;
}

/// Cell content height: glyph, 2px gap, label line.
#[must_use]
fn cell_h(ui: &Ui) -> f32 {
    (ICON_PX + 2.0 + ui.text_line_height()).ceil()
}

/// Ribbon height from font metrics - the dock host starts below it, so
/// panels can never slide under the strip.
#[must_use]
pub(crate) fn height(ui: &Ui) -> f32 {
    2.0 * PAD_V + cell_h(ui) + 2.0 + 1.0 + 2.0 + ui.text_line_height()
}

/// The whole strip at `y` (just below the menu bar).
pub(crate) fn bar(ui: &Ui, state: &mut State, y: f32) {
    let display = ui.io().display_size();
    ui.window("##ribbon")
        .flags(
            WindowFlags::NO_TITLE_BAR
                | WindowFlags::NO_COLLAPSE
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_RESIZE
                | WindowFlags::NO_DOCKING
                | WindowFlags::NO_BRING_TO_FRONT_ON_FOCUS
                | WindowFlags::NO_NAV_INPUTS
                | WindowFlags::NO_SCROLLBAR
                | WindowFlags::NO_SCROLL_WITH_MOUSE,
        )
        .position([0.0, y], Condition::Always)
        .size([display[0], height(ui)], Condition::Always)
        .build(|| {
            let _pad = ui.push_style_var(StyleVar::WindowPadding([PAD_H, PAD_V]));
            let _spc = ui.push_style_var(StyleVar::ItemSpacing([GAP, 2.0]));
            let h = cell_h(ui);

            let has_tool = !state.tool_names().is_empty();
            let no_tool_tip = "No tool yet - add one (Setup > Tools)";
            let mut create = [
                item("Face", Icon::Face, "Add a Face mill operation", run_new_face, has_tool),
                item("Contour", Icon::Contour, "Add a 2D Contour operation", run_new_contour, has_tool),
                item("Pocket", Icon::Pocket, "Add a 2D Pocket operation", run_new_pocket, has_tool),
                item("Slot", Icon::Slot, "Add a Slot operation", run_new_slot, has_tool),
                item("Trace", Icon::Trace, "Add a Trace operation", run_new_trace, has_tool),
            ];
            if !has_tool {
                for it in &mut create {
                    it.tip = no_tool_tip;
                }
            }

            group(
                ui,
                state,
                h,
                "File",
                &[
                    item("New", Icon::New, "New document\tCtrl+N", actions::new_document, true),
                    item("Model", Icon::OpenModel, "Open a mesh model\tCtrl+O", actions::open_model_dialog, true),
                    item("Project", Icon::OpenProject, "Open a project\tCtrl+Shift+O", actions::open_project, true),
                    item("Save", Icon::Save, "Save the project\tCtrl+S", actions::save_project, true),
                ],
            );
            divider(ui);
            group(
                ui,
                state,
                h,
                "Setup",
                &[
                    item("Stock", Icon::Stock, "Stock definition", run_stock, true),
                    item("Origin", Icon::WorkOffset, "Work offset (WCS)", run_wcs, true),
                    item("Tools", Icon::Tools, "Tool library", run_tool_lib, true),
                ],
            );
            divider(ui);
            group(ui, state, h, "Create", &create);
            divider(ui);
            group(
                ui,
                state,
                h,
                "Modify",
                &[
                    item("Undo", Icon::Undo, "Undo\tCtrl+Z", actions::undo, state.history.can_undo()),
                    item("Redo", Icon::Redo, "Redo\tCtrl+Y", actions::redo, state.history.can_redo()),
                    item(
                        "Run",
                        Icon::Generate,
                        "Generate the selected operation",
                        actions::generate_selected,
                        state.selected_operation().is_some(),
                    ),
                    item(
                        "Delete",
                        Icon::Delete,
                        "Delete the selection\tDel",
                        run_delete,
                        state.has_selection(),
                    ),
                ],
            );
            divider(ui);
            group(
                ui,
                state,
                h,
                "Inspect",
                &[
                    item("Frame", Icon::FrameAll, "Frame all\tF", actions::frame_all, true),
                    Item {
                        toggled: state.show_all_paths,
                        ..item(
                            "Paths",
                            Icon::Toolpaths,
                            "Show every cached toolpath",
                            run_toggle_paths,
                            true,
                        )
                    },
                ],
            );
        });
}

fn divider(ui: &Ui) {
    ui.same_line();
    ui.separator_vertical();
    ui.same_line();
}

/// One labeled row of cells with a hairline and the group name under it.
/// The group's extent is measured (n cells, fixed width) rather than
/// asked of the layout, so the rule and caption align on one baseline.
fn group(ui: &Ui, state: &mut State, h: f32, title: &str, items: &[Item]) {
    let [x0, row_y] = ui.cursor_screen_pos();
    for (i, it) in items.iter().enumerate() {
        if i > 0 {
            ui.same_line();
        }
        cell(ui, state, it, h);
    }
    // Rule and caption hang below the row's top at a fixed offset - draw
    // list ink, not items, so every group shares the one baseline.
    let gw = items.len() as f32 * CELL_W + (items.len() - 1) as f32 * GAP;
    let y = row_y + h + 2.0;
    let dl = ui.get_window_draw_list();
    dl.add_line([x0, y], [x0 + gw, y], [0.55, 0.55, 0.6, 1.0]).build();
    let [tw, _] = ui.calc_text_size(title);
    dl.add_text(
        [x0 + (gw - tw) * 0.5, y + 3.0],
        [0.75, 0.75, 0.8, 1.0],
        title,
    );
}

fn cell(ui: &Ui, state: &mut State, item: &Item, h: f32) {
    let top = ui.cursor_screen_pos();
    let bot = [top[0] + CELL_W, top[1] + h];
    let clicked = ui.invisible_button(item.label, [CELL_W, h]);
    let fired = clicked && item.enabled;
    let hovered = ui.is_item_hovered();

    let dl = ui.get_window_draw_list();
    if item.toggled || hovered || fired {
        let bg = if fired {
            [0.32, 0.42, 0.58, 1.0]
        } else if item.toggled {
            [0.24, 0.3, 0.4, 1.0]
        } else {
            [0.26, 0.28, 0.32, 1.0]
        };
        dl.add_rect_filled_multicolor(top, bot, bg, bg, bg, bg);
    }
    // White glyphs multiply by the tint, so "disabled" is just grey ink -
    // no BeginDisabled, and the cell stays hoverable to deliver its tip.
    let ink = if item.enabled { 1.0 } else { 0.45 };
    if let Some(icons) = state.icons.as_ref()
        && let Some(tex) = icons.texture(item.icon)
    {
        let cx = top[0] + (CELL_W - ICON_PX) * 0.5;
        dl.add_image(
            tex,
            [cx, top[1]],
            [cx + ICON_PX, top[1] + ICON_PX],
            [0.0, 0.0],
            [1.0, 1.0],
            [ink, ink, ink, 1.0],
        );
    }
    let [tw, _] = ui.calc_text_size(item.label);
    dl.add_text(
        [top[0] + (CELL_W - tw) * 0.5, top[1] + ICON_PX + 2.0],
        [ink, ink, ink, 1.0],
        item.label,
    );

    if fired {
        (item.run)(state);
    }
    if hovered {
        let tip = item.tip;
        let enabled = item.enabled;
        ui.tooltip(move || {
            ui.text(tip);
            if !enabled {
                ui.text_disabled("not available right now");
            }
        });
    }
}
