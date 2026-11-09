//! The tool editor's Profile section (#93): a milling tool's holder as a
//! spun (revolved) half-outline - Parametric (the cylinder its numbers
//! state) or Custom (a table of points with optional arcs), a flat side
//! view of cutter, shank and holder with their key dimensions that
//! follows every keystroke, the live refusal by name, and Apply profile,
//! which queues the one command. Turning tools are not spun profiles:
//! for them the column stays the Body section alone (owner, #93).
//! Provenance: our own pane over `app::tool_body_view` and
//! `app::spun`; the side view is the handbook's tool drawing. No vendor
//! material consulted.

use egui::{Align2, Color32, FontId, Pos2, Sense, Shape, Stroke, Ui, Vec2};

use camcraft_app::spun::{SpunPoint, SpunProfile};
use camcraft_app::tool_body_view;
use camcraft_app::tools::Tool;
use camcraft_app::units::{self, Unit};

use crate::state::{State, ToolColumn, is_spun_kind};

/// The side view's size, logical px.
const VIEW: Vec2 = Vec2::new(260.0, 230.0);
const NOTE: Color32 = Color32::from_rgb(0x9E, 0xA8, 0xB8);
const HOLDER: Color32 = Color32::from_rgb(230, 160, 60);
const BAD: Color32 = Color32::from_rgb(230, 90, 80);

/// The editor's third column: Body / Profile tabs for a milling tool,
/// the Body section alone for a turning tool.
pub(super) fn third_column(ui: &mut Ui, state: &mut State, exists: bool, u: Unit) {
    let milling = is_spun_kind(state.tool_ui.tool_kind());
    if milling {
        ui.horizontal(|ui| {
            let c = state.tool_profile_ui.column;
            if ui.selectable_label(c == ToolColumn::Body, "Body").on_hover_text("import a holder or assembly mesh").clicked() {
                state.tool_profile_ui.column = ToolColumn::Body;
            }
            if ui
                .selectable_label(c == ToolColumn::Profile, "Profile")
                .on_hover_text("draw the holder as a revolved outline")
                .clicked()
            {
                state.tool_profile_ui.column = ToolColumn::Profile;
            }
        });
    }
    if milling && state.tool_profile_ui.column == ToolColumn::Profile {
        profile_section(ui, state, exists, u);
    } else {
        super::tool_body_page::body_section(ui, state, exists, u);
    }
}

fn profile_section(ui: &mut Ui, state: &mut State, exists: bool, u: Unit) {
    ui.strong("Holder profile");
    if !exists {
        ui.label("Add the tool first, then draw its holder.");
        return;
    }
    state.sync_tool_profile_ui();
    let landed = state.editor_tool_body();
    ui.label(match &landed {
        Some(b) if b.spun.is_some() => "landed: a drawn profile".to_owned(),
        Some(b) => format!("landed: imported body {} - Apply profile replaces it", b.source),
        None => "landed: the holder cylinder from the numbers".to_owned(),
    });
    let custom = state.tool_profile_ui.custom;
    ui.horizontal(|ui| {
        if ui
            .selectable_label(!custom, "Parametric")
            .on_hover_text("the holder is the Ø × length cylinder of the numbers, redrawn as they change")
            .clicked()
        {
            state.set_profile_custom(false);
        }
        if ui.selectable_label(custom, "Custom").on_hover_text("edit the outline point by point; number edits leave it alone").clicked() {
            state.set_profile_custom(true);
        }
    });
    points_table(ui, state, u);
    let tool = state.tool_ui.to_tool();
    let points = state.tool_profile_ui.points.clone();
    let check = if points.is_empty() { Err("no holder - the tool states none".to_owned()) } else { tool_body_view::spun_preview(&tool, &points) };
    side_view(ui, &tool, &points, check.as_ref().ok(), u);
    match &check {
        Ok(s) => {
            state.retire_profile_refusal();
            let top = s.points().last().map_or(0.0, |p| p.z);
            let foot = s.points().first().map_or(0.0, |p| p.z);
            let wide = s.max_radius();
            ui.label(format!(
                "holder Ø {} × {} {}, gauge {} {}",
                units::len(2.0 * wide, u, 1),
                units::len(top - foot, u, 1),
                u.suffix(),
                units::len(top, u, 1),
                u.suffix()
            ));
        }
        Err(e) => {
            ui.colored_label(BAD, e);
        }
    }
    let state_custom = state.tool_profile_ui.custom;
    let can = state_custom || landed.is_some();
    if ui
        .add_enabled(can, egui::Button::new("Apply profile"))
        .on_hover_text(if state_custom {
            "land the drawn holder as the tool's body (one undo step)"
        } else {
            "drop the landed body: the holder cylinder is judged again (one undo step)"
        })
        .clicked()
    {
        state.queue_apply_tool_profile();
    }
}

/// The points, tip-up: z, r, arc radius (0 = line), insert after,
/// remove. Read-only in Parametric.
fn points_table(ui: &mut Ui, state: &mut State, u: Unit) {
    let custom = state.tool_profile_ui.custom;
    let n = state.tool_profile_ui.points.len();
    let mut insert = None;
    let mut remove = None;
    egui::ScrollArea::vertical().max_height(150.0).id_salt("tool_profile_points").show(ui, |ui| {
        ui.add_enabled_ui(custom, |ui| {
            egui::Grid::new("tool_profile_grid").num_columns(6).spacing([4.0, 2.0]).show(ui, |ui| {
                for h in ["#", &format!("z {}", u.suffix()), &format!("r {}", u.suffix()), "arc R", "", ""] {
                    ui.label(h);
                }
                ui.end_row();
                for i in 0..n {
                    ui.label(format!("{}", i + 1));
                    if let Some(p) = state.tool_profile_ui.points.get_mut(i) {
                        len_cell(ui, u, &mut p.z);
                        len_cell(ui, u, &mut p.r);
                        len_cell(ui, u, &mut p.arc);
                    }
                    if i + 1 < n && ui.small_button("+").on_hover_text("insert a point after this one").clicked() {
                        insert = Some(i);
                    } else if i + 1 >= n {
                        ui.label("");
                    }
                    if ui.small_button("×").on_hover_text("remove this point").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        });
    });
    if let Some(i) = insert {
        state.insert_profile_point(i);
    }
    if let Some(i) = remove {
        state.remove_profile_point(i);
    }
}

/// A compact unit-lens length cell (the `fields::num` law, narrower).
fn len_cell(ui: &mut Ui, u: Unit, v: &mut f64) {
    let w = [54.0, ui.spacing().interact_size.y];
    if u == Unit::Millimeters {
        ui.add_sized(w, egui::DragValue::new(v).speed(0.1));
    } else {
        let mut d = units::to_display(*v, u);
        if ui.add_sized(w, egui::DragValue::new(&mut d).speed(0.1 / 25.4)).changed() {
            *v = units::from_display(d, u);
        }
    }
}

/// The flat side view: cutter and shank from the tool's numbers, the
/// holder from the draft (amber when it validates, a red outline when
/// not), to scale, tip at the bottom, with the key dimensions.
fn side_view(ui: &mut Ui, tool: &Tool, points: &[SpunPoint], spun: Option<&SpunProfile>, u: Unit) {
    const PAD: f32 = 16.0;
    let (resp, painter) = ui.allocate_painter(VIEW, Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 26, 31));
    let cutter = SpunProfile::cutter_of(tool).map(|c| c.outline()).unwrap_or_default();
    let holder: Vec<(f64, f64)> = spun.map_or_else(|| points.iter().map(|p| (p.z, p.r)).collect(), SpunProfile::outline);
    let all = || cutter.iter().chain(holder.iter()).filter(|q| q.0.is_finite() && q.1.is_finite());
    let r_max = all().fold(0.0f64, |a, q| a.max(q.1));
    let z_max = all().fold(0.0f64, |a, q| a.max(q.0));
    if r_max <= 0.0 || z_max <= 0.0 {
        painter.text(rect.center(), Align2::CENTER_CENTER, "enter dimensions", FontId::proportional(11.0), NOTE);
        return;
    }
    // The tool sits left of centre; the right margin holds the labels.
    let axis_x = rect.min.x + rect.width() * 0.38;
    let base_y = rect.max.y - PAD;
    let scale = ((rect.width() * 0.34) / r_max as f32).min((rect.height() - 2.0 * PAD) / z_max as f32).max(f32::EPSILON);
    let at = |z: f64, r: f64| Pos2::new(axis_x + r as f32 * scale, base_y - z as f32 * scale);
    painter.line_segment([Pos2::new(axis_x, rect.min.y + 4.0), Pos2::new(axis_x, rect.max.y - 4.0)], Stroke::new(1.0, Color32::from_rgb(70, 74, 84)));
    let zc = tool.flute_len.max(0.0);
    let slices = |o: &[(f64, f64)], fill: &dyn Fn(f64) -> Color32| {
        for w in o.windows(2) {
            let ((z0, r0), (z1, r1)) = (w[0], w[1]);
            if z1 > z0 && [z0, z1, r0, r1].iter().all(|v| v.is_finite()) {
                painter.add(Shape::convex_polygon(vec![at(z0, -r0), at(z0, r0), at(z1, r1), at(z1, -r1)], fill((z0 + z1) * 0.5), Stroke::NONE));
            }
        }
    };
    slices(&cutter, &|zm| if zm < zc { Color32::from_rgb(168, 178, 192) } else { Color32::from_rgb(122, 130, 142) });
    let ok = spun.is_some();
    if ok {
        slices(&holder, &|_| Color32::from_rgba_unmultiplied(230, 160, 60, 90));
    }
    let outline = |o: &[(f64, f64)], ink: Color32| {
        let mut pts: Vec<Pos2> = o.iter().map(|&(z, r)| at(z, r)).collect();
        pts.extend(o.iter().filter(|q| q.1 > 0.0).rev().map(|&(z, r)| at(z, -r)));
        if pts.len() >= 2 {
            painter.add(Shape::closed_line(pts, Stroke::new(1.0, ink)));
        }
    };
    outline(&cutter, Color32::from_rgb(214, 220, 228));
    outline(&holder, if ok { HOLDER } else { BAD });
    // Heights: flute top, the holder's foot (stickout) and top (gauge).
    let small = FontId::proportional(10.0);
    let tick = |z: f64, what: &str| {
        if z.is_finite() && z > 0.0 {
            let y = base_y - z as f32 * scale;
            painter.line_segment([Pos2::new(axis_x + 4.0, y), Pos2::new(rect.max.x - 4.0, y)], Stroke::new(0.5, Color32::from_rgb(80, 86, 98)));
            painter.text(Pos2::new(rect.max.x - 4.0, y - 1.0), Align2::RIGHT_BOTTOM, format!("{what} {}", units::len(z, u, 1)), small.clone(), NOTE);
        }
    };
    tick(zc, "flutes");
    if let (Some(foot), Some(top)) = (holder.first().map(|q| q.0), holder.last().map(|q| q.0)) {
        tick(foot, "stickout");
        tick(top, "gauge");
    } else {
        tick(zc + tool.shank_len.max(0.0), "shank");
    }
    painter.text(Pos2::new(axis_x, rect.max.y - 2.0), Align2::CENTER_BOTTOM, format!("Ø {}", units::len(tool.diameter, u, 2)), small.clone(), NOTE);
    painter.text(
        rect.min + Vec2::new(6.0, 4.0),
        Align2::LEFT_TOP,
        format!("shank Ø {} {}", units::len(tool.shank_d, u, 2), u.suffix()),
        small,
        NOTE,
    );
}
