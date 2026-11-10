//! The Debug info window (#157): one plain-text report of what a bug
//! report needs - build, platform, GPU, document and machine - with a
//! Copy button, opened from the status strip's bug glyph.
//!
//! Provenance: our own; the fields are the ones a crash or wrong-output
//! report is triaged by (version, OS/arch, adapter, the document's shape).
//! No vendor material consulted.

use egui::{Button, Context, Id, Image, Response, RichText, Ui, Vec2, Window};

use camcraft_app::{machine, model, operation, setup, units};

use super::icons::{Icon, IconSet};
use crate::build_info;
use crate::state::State;

/// The status strip's door: the glyph at the strip's 16 px when the icon
/// set is up, the words when it is not. Both answer to "Debug info".
pub(super) fn button(ui: &mut Ui, icons: Option<&IconSet>) -> Response {
    let resp = match icons.and_then(|s| s.texture(Icon::DebugInfo)) {
        // The glyph is white at load; tint it with the body ink (as the
        // ribbon cells do) so it reads on the light theme too.
        Some(tex) => {
            let ink = ui.visuals().text_color();
            let glyph = Image::new(tex).fit_to_exact_size(Vec2::splat(16.0)).tint(ink);
            ui.add(Button::image(glyph).frame(false)).on_hover_text("Debug info")
        }
        None => ui.small_button("Debug info"),
    };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Debug info"));
    resp
}

/// The report, one `key: value` line each - plain text, so it pastes into
/// an issue as it reads here.
pub(crate) fn report(state: &State) -> String {
    let world = &state.world;
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let mut lines = vec![
        format!("CamCraft {} ({profile} build)", build_info::VERSION),
        format!("Build time: {}", build_info::BUILT_UTC),
        format!("Source commit: {}", build_info::SOURCE_SHA),
        format!("Platform: {} {}", std::env::consts::OS, std::env::consts::ARCH),
        format!("GPU: {}", state.gpu_line.as_deref().unwrap_or("none (headless)")),
        format!(
            "Project: {}",
            state.project_path.as_ref().map_or_else(|| "unsaved".to_owned(), |p| p.display().to_string())
        ),
        format!("Units: {}", units::of(world).label()),
    ];
    let (path, def) = machine::of(world);
    let kind = format!("{:?}", machine::kind_of(world));
    lines.push(match def {
        Some(m) => format!("Machine: {} {} ({kind}, from {})", m.vendor, m.model, or_none(&path)),
        None => format!("Machine: none stated ({kind})"),
    });
    let entities = model::named_entities(world);
    let models = entities.iter().filter(|(_, e)| model::is_model(world, e)).count();
    let tools = entities.iter().filter(|(_, e)| model::is_tool(world, e)).count();
    let ops: Vec<_> = entities.iter().filter(|(_, e)| model::is_operation(world, e)).collect();
    let status = |s: operation::OpStatus| ops.iter().filter(|(_, e)| operation::status_of(world, e) == s).count();
    lines.push(format!(
        "Document: {} setup(s), {} model(s), {} tool(s), {} operation(s) ({} ok, {} stale, {} never generated)",
        setup::names(world).len(),
        models,
        tools,
        ops.len(),
        status(operation::OpStatus::Ok),
        status(operation::OpStatus::Stale),
        status(operation::OpStatus::Never),
    ));
    lines.push(format!("Status: {}", or_none(&state.status)));
    lines.join("\n")
}

fn or_none(s: &str) -> &str {
    if s.is_empty() { "none" } else { s }
}

/// The window: the report as selectable monospace text and a Copy
/// button. Allocates nothing while closed.
pub(crate) fn window(ctx: &Context, state: &mut State) {
    if !state.debug_info_open {
        return;
    }
    let text = report(state);
    let mut open = true;
    let mut copied = false;
    Window::new("Debug info")
        .id(Id::new("debug-info"))
        .open(&mut open)
        .default_size([480.0, 220.0])
        .show(ctx, |ui| {
            if ui.button("Copy").clicked() {
                ui.ctx().copy_text(text.clone());
                copied = true;
            }
            ui.separator();
            ui.add(egui::Label::new(RichText::new(text.as_str()).monospace()).selectable(true));
        });
    if copied {
        state.status = "debug info copied to the clipboard".into();
    }
    state.debug_info_open = open;
}
