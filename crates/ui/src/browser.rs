//! Browser tree: every object in the document, one persistent pane.
//! Provenance: our own pane, specced from public CAM browser trees.
//!
//! The tree is the document's outline, not a filter of it: Setup (stock and
//! the G5x offsets), Models, Tools and Operations each get a group. A leaf
//! click aims the context pane (`UiTarget`) and, for world entities,
//! mirrors a `Select` - the target is view state, the selection is the
//! command (AGENTS.md). Tool leaves deliberately push no `Select`: a tool
//! node must not arm the Del key against the delete path models use.
//!
//! Rows are drawn as one full-width bar (a `###`-id'd Selectable) with the
//! type glyph and label laid over it - the same invisible-button-plus-ink
//! pattern the ribbon cells use - and models/operations carry a right-edge
//! eye: visibility is view state like the checkboxes (not a command, not
//! saved), and purely display: a hidden model still feeds stock and paths,
//! a hidden path still posts. Hiding must never silently change what the
//! machine does.
//!
//! Status badges are computed on read (`operation::status_of`) - never
//! stored - so a line cannot lie about the world.

use dear_imgui_rs::{Ui, WindowKey};
use hsmworks_app::model;
use hsmworks_app::operation::{self, OpStatus, OpType};
use hsmworks_app::schema::{STOCK_NAME, WCS_SLOTS};
use hsmworks_app::tools::ToolKind;

use crate::actions;
use crate::icons::Icon;
use crate::state::{CommandKind, State, UiTarget};

/// Tree icon edge (AGENTS.md: 32px sources render at 16).
const INK: f32 = 16.0;

pub fn browser_panel(ui: &Ui, state: &mut State) {
    let Ok(key) = WindowKey::new("browser", "Browser") else {
        return;
    };
    // Persistent docked pane: always in the layout, what changes is what
    // the open groups hold.
    ui.window(key.label("Browser")).build(|| {
        contents(ui, state);
    });
}

/// Shared with the Operations pane's tool page for its summary line.
pub(crate) fn kind_name(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::EndMill => "end mill",
        ToolKind::BallNose => "ball nose",
        ToolKind::BullNose => "bull nose",
    }
}

fn op_icon(kind: OpType) -> Icon {
    match kind {
        OpType::Face => Icon::Face,
        OpType::Contour => Icon::Contour,
        OpType::Pocket => Icon::Pocket,
        OpType::Slot => Icon::Slot,
        OpType::Trace => Icon::Trace,
    }
}

fn contents(ui: &Ui, state: &mut State) {
    // View toggles, not commands (AGENTS.md ephemeral UI state, like the
    // camera): the world-origin axes and the non-selected toolpaths.
    ui.checkbox("Show world origin axes", &mut state.show_world_axes);
    ui.checkbox("Show all toolpaths", &mut state.show_all_paths);

    let target = state.pane_target();
    group(ui, state, "Setup", |ui, state| setup_group(ui, state, &target));
    group(ui, state, "Models", |ui, state| models_group(ui, state, &target));
    group(ui, state, "Tools", |ui, state| tools_group(ui, state, &target));
    group(
        ui,
        state,
        "Operations",
        |ui, state| operations_group(ui, state, &target),
    );
}

/// A tree row spec: id, glyph, texts, states. `eye` marks rows whose
/// visibility the user can toggle; `hidden` then draws the name and glyph
/// dimmed with an eye-off at the right edge.
#[derive(Clone, Copy)]
struct Row<'a> {
    id: &'a str,
    icon: Icon,
    label: &'a str,
    suffix: &'a str,
    active: bool,
    eye: bool,
    hidden: bool,
}

impl<'a> Row<'a> {
    fn plain(id: &'a str, icon: Icon, label: &'a str, suffix: &'a str, active: bool) -> Self {
        Row {
            id,
            icon,
            label,
            suffix,
            active,
            eye: false,
            hidden: false,
        }
    }
}

/// The row's two ImGui ID strings: the eye's invisible button and the
/// full-width Selectable. They must NOT resolve to the same ID - whatever
/// follows the `###` is what ImGui hashes, so `eye###bX` and `###bX` would
/// be two items with one ID, and the shared `active_id` routing breaks the
/// row click (it lands on the invisible eye). The eye keeps winning its own
/// pixels by being submitted first, not by an ID collision.
fn row_ids(spec_id: &str) -> (String, String) {
    (format!("eye###eb{spec_id}"), format!("###b{spec_id}"))
}

/// Draw one row; returns (row clicked, eye clicked). The eye's invisible
/// button is submitted BEFORE the full-width Selectable so it wins the
/// hover-id, and the Selectable's `ItemHoverable` then refuses clicks
/// landing on the eye - one pass-through per pixel, no double action.
fn row(ui: &Ui, state: &State, spec: &Row) -> (bool, bool) {
    let line = ui.text_line_height();
    let row_h = (INK + 2.0).max(line + 4.0);
    let [x, y] = ui.cursor_screen_pos();
    let avail = ui.content_region_avail()[0];
    let mut eye_hit = false;
    let mut eye_hot = false;
    let eye_x = x + avail - INK - 6.0;
    let text_y = y + (row_h - line) * 0.5;
    let (eye_id, row_id) = row_ids(spec.id);
    if spec.eye {
        ui.set_cursor_screen_pos([eye_x, y + (row_h - INK) * 0.5]);
        eye_hit = ui.invisible_button(eye_id.as_str(), [INK, INK]);
        // The eye's own hover, sampled now: after the Selectable the "last
        // item" is the row, not the button.
        eye_hot = ui.is_item_hovered();
        ui.set_cursor_screen_pos([x, y]);
    }
    let clicked = ui
        .selectable_config(row_id.as_str())
        .selected(spec.active)
        .size([0.0, row_h])
        .build();

    let dl = ui.get_window_draw_list();
    let ink = if spec.hidden {
        [0.5, 0.5, 0.55, 1.0]
    } else {
        [0.92, 0.92, 0.95, 1.0]
    };
    let iy = y + (row_h - INK) * 0.5;
    if let Some(tex) = state.icon(spec.icon) {
        let g = if spec.hidden { 0.55 } else { 1.0 };
        dl.add_image(
            tex,
            [x + 2.0, iy],
            [x + 2.0 + INK, iy + INK],
            [0.0, 0.0],
            [1.0, 1.0],
            [g, g, g, 1.0],
        );
    }
    dl.add_text([x + INK + 8.0, text_y], ink, spec.label);
    if !spec.suffix.is_empty() {
        let [lw, _] = ui.calc_text_size(spec.label);
        dl.add_text(
            [x + INK + 8.0 + lw + 8.0, text_y],
            [0.62, 0.62, 0.68, 1.0],
            spec.suffix,
        );
    }
    if spec.eye {
        let which = if spec.hidden { Icon::EyeOff } else { Icon::Toolpaths };
        if let Some(tex) = state.icon(which) {
            let g = if spec.hidden { 0.55 } else { 0.85 };
            dl.add_image(
                tex,
                [eye_x, y + (row_h - INK) * 0.5],
                [eye_x + INK, y + (row_h - INK) * 0.5 + INK],
                [0.0, 0.0],
                [1.0, 1.0],
                [g, g, g, 1.0],
            );
        }
        if eye_hot {
            let tip = if spec.hidden { "Show" } else { "Hide" };
            ui.tooltip_text(format!("{tip} in the viewport (view state; geometry still cuts)"));
        }
    }
    (clicked, eye_hit)
}

/// Group header plus open scope, drawing `children` only when the node is
/// open. The push token pops the tree on DROP, so children must render
/// while it is alive - hence the closure, not a bool the caller acts on.
/// Full-width hover bar (`span_avail_width`), open by default.
fn group<F: FnOnce(&Ui, &mut State)>(ui: &Ui, state: &mut State, label: &str, children: F) {
    let id = format!("{label}###browser_{}", label.to_lowercase());
    let Some(_node) = ui
        .tree_node_config(id.as_str())
        .default_open(true)
        .span_avail_width(true)
        .push()
    else {
        return;
    };
    children(ui, state);
}

fn setup_group(ui: &Ui, state: &mut State, target: &UiTarget) {
    // The stock page doubles as the create-stock form, so the node is a
    // destination whether or not the entity exists yet.
    let spec = Row::plain(
        "stock",
        Icon::Stock,
        "Stock",
        "",
        *target == UiTarget::Stock,
    );
    let (hit, _) = row(ui, state, &spec);
    if hit {
        actions::target(state, UiTarget::Stock);
        if model::find_by_name(&state.world, STOCK_NAME).is_some() {
            state.pending.push_back(CommandKind::Select(Some(STOCK_NAME.to_string())));
        }
    }
    for (i, slot) in WCS_SLOTS.iter().enumerate() {
        let placed = model::find_by_name(&state.world, slot).is_some();
        let spec = Row::plain(
            "wcs",
            Icon::WorkOffset,
            slot,
            if placed { "" } else { "not placed" },
            *target == UiTarget::WcsSlot(i),
        );
        // id must stay unique across the same-named rows: fold the slot in.
        let (hit, _) = row_named(ui, state, &spec, &format!("wcs{i}"));
        if hit {
            actions::target(state, UiTarget::WcsSlot(i));
            if placed {
                state.pending.push_back(CommandKind::Select(Some((*slot).to_string())));
            }
        }
    }
}

/// `row` with an id override, for rows whose display label repeats
/// (the six WCS slots read their entity name).
fn row_named(ui: &Ui, state: &State, spec: &Row, id: &str) -> (bool, bool) {
    let owned = Row {
        id,
        ..*spec
    };
    row(ui, state, &owned)
}

fn models_group(ui: &Ui, state: &mut State, target: &UiTarget) {
    // Setup furniture (stock, offsets) and CAM entities are filtered out of
    // the model list by `model_entities` itself.
    let entities = model::model_entities(&state.world);
    if entities.is_empty() {
        ui.text_wrapped(
            "No models yet.\n\nFile > Open Model (Ctrl+O)\nor drag a file onto the window.\n\n\
             In the viewport: click selects, middle-drag orbits, right-drag pans, \
             wheel zooms toward the cursor, F frames all.",
        );
    }
    for (name, entity) in entities {
        let selected = model::is_selected(&state.world, &entity);
        let hidden = state.hidden_models.contains(&name);
        let active = *target == UiTarget::Model(name.clone()) || selected;
        let spec = Row {
            id: "model",
            icon: Icon::Model,
            label: &name,
            suffix: if hidden { " (hidden)" } else { "" },
            active,
            eye: true,
            hidden,
        };
        let (hit, eye) = row_named(ui, state, &spec, &format!("model{name}"));
        if eye {
            State::toggle_visibility(&mut state.hidden_models, &name);
        }
        if hit {
            actions::target(state, UiTarget::Model(name.clone()));
            state.pending.push_back(CommandKind::Select(Some(name)));
        }
    }
}

fn tools_group(ui: &Ui, state: &mut State, target: &UiTarget) {
    let tools: Vec<(String, hsmworks_app::tools::Tool)> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_tool(&state.world, e))
        .filter_map(|(name, e)| model::tool_of(&state.world, &e).map(|t| (name, t)))
        .collect();
    if tools.is_empty() {
        ui.text_wrapped("None yet - the Tool page has a catalog to add from.");
    }
    for (name, tool) in tools {
        let suffix = format!("Ø{} {}", tool.diameter, kind_name(tool.kind));
        let active = *target == UiTarget::Tool(name.clone());
        let spec = Row::plain("tool", Icon::Tools, &name, &suffix, active);
        let (hit, _) = row_named(ui, state, &spec, &format!("tool{name}"));
        if hit {
            // Target only: no Select, so browsing tools never arms Del.
            actions::target(state, UiTarget::Tool(name));
        }
    }
}

fn operations_group(ui: &Ui, state: &mut State, target: &UiTarget) {
    let names: Vec<String> = model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
        .map(|(name, _)| name)
        .collect();
    if names.is_empty() {
        ui.text_wrapped(
            "No operations yet.\n\nAdd a tool (Tools above), then\nribbon > Create, or\nCAM > New Operation.",
        );
        return;
    }
    for name in names {
        let Some(entity) = model::find_by_name(&state.world, &name) else {
            continue;
        };
        let (kind, icon) = model::operation_of(&state.world, &entity)
            .map(|op| {
                let k = op.params.kind();
                (k.to_string(), op_icon(k))
            })
            .unwrap_or_else(|| (String::new(), Icon::Generate));
        let tag = match operation::status_of(&state.world, &entity) {
            OpStatus::Never => "never",
            OpStatus::Ok => "ok",
            OpStatus::Stale => "STALE",
        };
        let warns = model::toolpath_of(&state.world, &entity)
            .map_or(0, |tp| tp.warnings.len());
        let warn_text = if warns == 1 {
            String::from("  1 warning")
        } else if warns > 1 {
            format!("  {warns} warnings")
        } else {
            String::new()
        };
        let suffix = format!("{kind}  {tag}{warn_text}");
        let hidden = state.hidden_paths.contains(&name);
        let active = *target == UiTarget::Operation(name.clone());
        let spec = Row {
            id: "op",
            icon,
            label: &name,
            suffix: &suffix,
            active,
            eye: true,
            hidden,
        };
        let (hit, eye) = row_named(ui, state, &spec, &format!("op{name}"));
        if eye {
            State::toggle_visibility(&mut state.hidden_paths, &name);
        }
        if hit {
            actions::target(state, UiTarget::Operation(name.clone()));
            state.pending.push_back(CommandKind::Select(Some(name)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::row_ids;

    /// The eye button and the row Selectable must never hash to the same
    /// ImGui ID: a shared ID routes row clicks to the invisible eye ("2
    /// visible items with conflicting ID", operations stop selecting).
    #[test]
    fn row_and_eye_never_share_an_imgui_id() {
        // The id-text after `###` is what ImGui actually hashes.
        fn id_part(s: &str) -> &str {
            match s.rsplit_once("###") {
                Some((_, t)) => t,
                None => s,
            }
        }
        for id in [
            "opOperation1",
            "opOperation2",
            "modelMilling-test",
            "toolEM-0800-4FL",
            "wcs0",
            "wcs5",
        ] {
            let (eye, row) = row_ids(id);
            assert_ne!(id_part(&eye), id_part(&row), "id collision for {id}");
        }
    }
}
