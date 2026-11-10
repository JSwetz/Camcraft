//! The recent-files drop-downs (issue #662) under the ribbon's Open
//! (projects) and Model (models) buttons. With a recent list on hand
//! the button opens its list - the files newest first by name (full
//! path on hover; a gone file greyed with "(missing)" and refused by
//! name), "Clear recent", and "Browse…" for today's file dialog. With
//! no recents (or the feature off) the button opens the dialog straight
//! away, exactly as before. A pick queues `OpenRecent` / `ClearRecent`
//! / the dialog command, so the unsaved-changes prompt and the import
//! worker are Open's and Model's own; the door itself is view state.
//! Provenance: our own code, the hand-drawn dropdown law of the Theme
//! and View doors (theme_drop.rs, layout/mod.rs); no HSMWorks material.

use camcraft_app::recent::{RecentEntry, RecentKind};
use egui::{Rect, Sense, Ui, Vec2};

use super::panes::{Cell, Cmd, DROP_GAP};
use super::theme::Tokens;
use crate::actions;
use crate::state::{CommandKind, State};

/// The list's width: wider than the other doors, file names run long.
pub(super) const RECENT_W: f32 = 300.0;
/// The bottom rows' captions (also their AccessKit labels).
pub(super) const CLEAR: &str = "Clear recent";
pub(super) const BROWSE: &str = "Browse…";

/// One row of a recent-files list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum RecentRow {
    /// A remembered file.
    Entry(RecentEntry),
    /// Empty this list.
    Clear,
    /// Today's file dialog.
    Browse,
}

impl RecentRow {
    /// What the row reads (and its AccessKit label).
    pub(super) fn label(&self) -> String {
        match self {
            RecentRow::Entry(e) if e.missing => format!("{} (missing)", e.name),
            RecentRow::Entry(e) => e.name.clone(),
            RecentRow::Clear => CLEAR.to_owned(),
            RecentRow::Browse => BROWSE.to_owned(),
        }
    }

    fn live(&self) -> bool {
        !matches!(self, RecentRow::Entry(e) if e.missing)
    }
}

/// Which list a ribbon cell carries.
pub(super) fn kind_of(c: Cmd) -> Option<RecentKind> {
    match c {
        Cmd::Project => Some(RecentKind::Project),
        Cmd::Model => Some(RecentKind::Model),
        _ => None,
    }
}

/// The cell a list hangs under.
fn cmd_of(kind: RecentKind) -> Cmd {
    match kind {
        RecentKind::Project => Cmd::Project,
        RecentKind::Model => Cmd::Model,
    }
}

/// The dialog the Browse row (and an empty list's button) opens.
fn browse(state: &mut State, kind: RecentKind) {
    match kind {
        RecentKind::Project => actions::open_project(state),
        RecentKind::Model => actions::open_model_dialog(state),
    }
}

/// The Open / Model button: toggle its list when it has entries, else
/// the dialog as before. Opening a list closes every other door.
pub(super) fn press(state: &mut State, kind: RecentKind) {
    if state.recent.list(kind).is_empty() {
        state.ribbon_recent_open = None;
        browse(state, kind);
        return;
    }
    if state.ribbon_recent_open == Some(kind) {
        state.ribbon_recent_open = None;
        return;
    }
    state.ribbon_recent_open = Some(kind);
    state.ribbon_more = None;
    state.ribbon_mode_open = false;
    state.ribbon_view_open = false;
    state.ribbon_theme_open = false;
}

/// The list's rows, pure geometry in the strip's global frame (the
/// Theme door's law): hung under the button, pulled left so the list
/// never runs off the strip's right edge. The tests click these rects.
pub(super) fn rows_of(
    state: &State,
    kind: RecentKind,
    button: Rect,
    strip: Rect,
    pal: &Tokens,
) -> Vec<(RecentRow, Rect)> {
    let mut rows: Vec<RecentRow> = state.recent_entries(kind).into_iter().map(RecentRow::Entry).collect();
    rows.push(RecentRow::Clear);
    rows.push(RecentRow::Browse);
    let left = button.left().min(strip.right() - RECENT_W).max(strip.left());
    rows.into_iter()
        .enumerate()
        .map(|(k, r)| {
            let min = egui::Pos2::new(left, button.bottom() + DROP_GAP + k as f32 * pal.row_height);
            (r, Rect::from_min_size(min, Vec2::new(RECENT_W, pal.row_height)))
        })
        .collect()
}

/// Draw the open list, if any, under its button in `cells`. A list
/// whose button or entries are gone closes itself.
pub(super) fn draw(ui: &mut Ui, state: &mut State, pal: &Tokens, id: egui::Id, cells: &[Cell], strip: Rect) {
    let Some(kind) = state.ribbon_recent_open else { return };
    let button = cells.iter().find(|c| c.cmd == cmd_of(kind)).map(|c| c.rect);
    match button {
        Some(button) if !state.recent.list(kind).is_empty() => {
            let rows = rows_of(state, kind, button, strip, pal);
            show(ui, state, kind, pal, id, button, &rows);
        }
        _ => state.ribbon_recent_open = None,
    }
}

/// The open list: a foreground Area of rows. A press on a row acts and
/// closes the list; an outside press closes it.
fn show(
    ui: &mut Ui,
    state: &mut State,
    kind: RecentKind,
    pal: &Tokens,
    id: egui::Id,
    button: Rect,
    rows: &[(RecentRow, Rect)],
) {
    let Some(top) = rows.first().map(|(_, r)| r.min) else {
        state.ribbon_recent_open = None;
        return;
    };
    let panel = Rect::from_min_size(top, Vec2::new(RECENT_W, rows.len() as f32 * pal.row_height));
    let mut chose: Option<RecentRow> = None;
    egui::Area::new(id)
        .fixed_pos(panel.min)
        .order(egui::Order::Foreground)
        .movable(false)
        .show(ui.ctx(), |ui| {
            let p = ui.painter();
            p.rect_filled(panel, pal.radius_menu, pal.ink);
            p.rect_stroke(panel, pal.radius_menu, egui::Stroke::new(1.0, pal.border), egui::StrokeKind::Outside);
            for (k, (row, r)) in rows.iter().enumerate() {
                let resp = ui.interact(*r, id.with(("recent-row", k)), Sense::click());
                let (label, live) = (row.label(), row.live());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, live, &label));
                if resp.hovered() && live {
                    p.rect_filled(*r, 0.0, pal.field_hover);
                }
                if matches!(row, RecentRow::Clear) {
                    p.hline(r.x_range(), r.top(), egui::Stroke::new(1.0, pal.border));
                }
                p.with_clip_rect(r.shrink2(Vec2::new(4.0, 0.0))).text(
                    r.left_center() + Vec2::new(8.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    &label,
                    egui::FontId::proportional(pal.font_row),
                    if live { pal.text } else { pal.text_faint },
                );
                let clicked = resp.clicked();
                if let RecentRow::Entry(e) = row {
                    resp.on_hover_text(e.path.display().to_string());
                }
                if clicked {
                    chose = Some(row.clone());
                }
            }
        });
    match chose {
        Some(row) => {
            state.ribbon_recent_open = None;
            match row {
                // Refused by name, never a crash: the file is gone.
                RecentRow::Entry(e) if e.missing => state.status = e.refusal(kind),
                RecentRow::Entry(e) => state.pending.push_back(CommandKind::OpenRecent(kind, e.path)),
                RecentRow::Clear => state.pending.push_back(CommandKind::ClearRecent(kind)),
                RecentRow::Browse => browse(state, kind),
            }
        }
        None => {
            let outside = ui.input(|i| {
                i.pointer.button_pressed(egui::PointerButton::Primary)
                    && i.pointer.interact_pos().is_none_or(|p| {
                        !button.expand(DROP_GAP).contains(p) && !rows.iter().any(|(_, r)| r.contains(p))
                    })
            });
            if outside {
                state.ribbon_recent_open = None;
            }
        }
    }
}
