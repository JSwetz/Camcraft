//! Setup pane: stock solid and work-coordinate editing.
//!
//! Provenance: our own pane, specced from public CAM setup conventions;
//! the G54..G59 offset codes are the published RS-274 / ISO 6983 ones.
//!
//! Like every panel this one never touches the world: it edits the
//! `State::stock_ui` / `State::wcs_ui` drafts and queues `CommandKind`s.
//! Reads of the world go through the `model::*` helpers only.

use std::borrow::Cow;

use dear_imgui_rs::{Ui, WindowKey};
use hsmworks_app::model;
use hsmworks_app::schema::{STOCK_NAME, WCS_SLOTS};
use hsmworks_app::stock::StockShape;

use hsmworks_app::geometry::{quat_from_axis_angle, quat_from_euler, quat_mul, quat_to_euler};

use crate::state::{CommandKind, State, quat_yaw_deg};

/// Quarter-turn buttons: `(label, world axis, sign)`, right-hand rule.
const QUARTER_TURNS: [(&str, usize, f64); 6] = [
    ("X+", 0, 1.0),
    ("X-", 0, -1.0),
    ("Y+", 1, 1.0),
    ("Y-", 1, -1.0),
    ("Z+", 2, 1.0),
    ("Z-", 2, -1.0),
];

pub fn setup_panel(ui: &Ui, state: &mut State) {
    let Ok(key) = WindowKey::new("setup", "Setup") else {
        return;
    };
    ui.window(key.label("Setup")).build(|| {
        state.sync_setup_ui();
        ui.text("Stock");
        stock_group(ui, state);
        ui.separator();
        ui.text("Work Coordinate System");
        wcs_group(ui, state);
    });
}

fn stock_group(ui: &Ui, state: &mut State) {
    ui.radio_button_int("Box", &mut state.stock_ui.shape, 0);
    ui.same_line();
    ui.radio_button_int("Cylinder", &mut state.stock_ui.shape, 1);
    ui.same_line();
    ui.radio_button_int("From model", &mut state.stock_ui.shape, 2);

    match state.stock_ui.shape {
        1 => {
            ui.input_double("Diameter##stk_d", &mut state.stock_ui.diameter);
            ui.input_double("Height##stk_h", &mut state.stock_ui.height);
        }
        2 => {
            let names: Vec<String> = model::model_entities(&state.world)
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            if names.is_empty() {
                ui.text("No models to take stock from.");
            } else {
                let mut idx = names
                    .iter()
                    .position(|n| *n == state.stock_ui.source)
                    .unwrap_or(names.len() - 1);
                if ui.combo("Source##stk_src", &mut idx, &names, |n| {
                    Cow::Borrowed(n.as_str())
                })
                    && let Some(name) = names.get(idx)
                {
                    state.stock_ui.source = name.clone();
                }
            }
        }
        _ => {
            ui.input_double("Length (X)##stk_dx", &mut state.stock_ui.dx);
            ui.input_double("Width (Y)##stk_dy", &mut state.stock_ui.dy);
            ui.input_double("Height (Z)##stk_dz", &mut state.stock_ui.dz);
        }
    }

    ui.spacing();
    ui.input_double("X center##stk_x", &mut state.stock_ui.x);
    ui.input_double("Y center##stk_y", &mut state.stock_ui.y);
    ui.input_double("Z bottom##stk_z", &mut state.stock_ui.z);
    ui.input_double("Rotation (yaw deg)##stk_yaw", &mut state.stock_ui.yaw_deg);

    ui.spacing();
    ui.input_double("Fit margin##stk_margin", &mut state.stock_ui.fit_margin);
    if ui.button("Apply##stk") {
        let shape = match state.stock_ui.shape {
            1 => StockShape::Cylinder {
                diameter: state.stock_ui.diameter,
                height: state.stock_ui.height,
            },
            2 => StockShape::FromModel {
                source: state.stock_ui.source.clone(),
            },
            _ => StockShape::Box {
                dx: state.stock_ui.dx,
                dy: state.stock_ui.dy,
                dz: state.stock_ui.dz,
            },
        };
        state.pending.push_back(CommandKind::SetStock {
            shape,
            pos: [state.stock_ui.x, state.stock_ui.y, state.stock_ui.z],
            yaw_deg: state.stock_ui.yaw_deg,
        });
    }
    ui.same_line();
    if ui.button("Fit to Models") {
        state.pending.push_back(CommandKind::StockFit {
            margin: state.stock_ui.fit_margin,
            shape: state.stock_ui.shape,
        });
    }
    let has_stock = model::find_by_name(&state.world, STOCK_NAME)
        .is_some_and(|entity| model::is_stock(&state.world, &entity));
    if has_stock {
        ui.same_line();
        if ui.button("Delete##stk") {
            state
                .pending
                .push_back(CommandKind::DeleteByName(STOCK_NAME.to_string()));
        }
    }
}

fn wcs_group(ui: &Ui, state: &mut State) {
    if ui.combo(
        "Offset##wcs_slot",
        &mut state.wcs_ui.slot,
        WCS_SLOTS.as_slice(),
        |s| Cow::Borrowed(*s),
    ) && let Some(slot) = WCS_SLOTS.get(state.wcs_ui.slot)
    {
        // Switching slots must show that slot's real numbers now, not the
        // previous slot's draft - a stale readout invites an accidental move.
        let loaded = model::find_by_name(&state.world, slot)
            .and_then(|entity| model::transform_of(&state.world, &entity));
        match loaded {
            Some(loc) => {
                state.wcs_ui.x = loc.pos.x;
                state.wcs_ui.y = loc.pos.y;
                state.wcs_ui.z = loc.pos.z;
                let (r, p, y) = quat_to_euler(loc.rot);
                state.wcs_ui.roll_deg = r.to_degrees();
                state.wcs_ui.pitch_deg = p.to_degrees();
                state.wcs_ui.yaw_deg = y.to_degrees();
                state.wcs_ui.placed = true;
            }
            None => {
                // Never-placed slot: machine zero is the honest default.
                state.wcs_ui.x = 0.0;
                state.wcs_ui.y = 0.0;
                state.wcs_ui.z = 0.0;
                state.wcs_ui.roll_deg = 0.0;
                state.wcs_ui.pitch_deg = 0.0;
                state.wcs_ui.yaw_deg = 0.0;
                state.wcs_ui.placed = false;
            }
        }
    }
    ui.input_double("X origin##wcs_x", &mut state.wcs_ui.x);
    ui.input_double("Y origin##wcs_y", &mut state.wcs_ui.y);
    ui.input_double("Z origin##wcs_z", &mut state.wcs_ui.z);
    // Full orientation: not every machine axes its coordinates the same way,
    // so the offset needs to rotate to fit the machine, not just yaw. The
    // workflow (pick an origin, orient the axes) is specced from published
    // CAM setup documentation; numeric roll/pitch/yaw is ours. Composition:
    // R = Rz(yaw) * Ry(pitch) * Rx(roll) about the world axes.
    ui.input_double("Tilt about X (roll)##wcs_roll", &mut state.wcs_ui.roll_deg);
    ui.input_double("Tilt about Y (pitch)##wcs_pitch", &mut state.wcs_ui.pitch_deg);
    ui.input_double("Turn about Z (yaw)##wcs_yaw", &mut state.wcs_ui.yaw_deg);
    ui.text("Quarter-turns about world axes:");
    for (i, (label, axis, sign)) in QUARTER_TURNS.iter().enumerate() {
        if i > 0 {
            ui.same_line();
        }
        if ui.button(format!("Turn {label}##wcs_turn{i}")) {
            let q = quat_from_euler(
                state.wcs_ui.roll_deg.to_radians(),
                state.wcs_ui.pitch_deg.to_radians(),
                state.wcs_ui.yaw_deg.to_radians(),
            );
            let turn = quat_from_axis_angle(*axis, sign * 90.0f64.to_radians());
            let (r, p, y) = quat_to_euler(quat_mul(turn, q));
            state.wcs_ui.roll_deg = r.to_degrees();
            state.wcs_ui.pitch_deg = p.to_degrees();
            state.wcs_ui.yaw_deg = y.to_degrees();
        }
    }
    ui.same_line();
    if ui.button("Level##wcs_level") {
        state.wcs_ui.roll_deg = 0.0;
        state.wcs_ui.pitch_deg = 0.0;
        state.wcs_ui.yaw_deg = 0.0;
    }

    // Box-point anchoring (feedback 5, revised): the reference box is an
    // explicit choice - stock or model - and the nine points of the chosen
    // level can be clicked in the viewport as markers instead of hunting
    // through the button grid. The buttons and the markers share
    // `crate::app::anchor_points`, so what you click, what highlights, and
    // what places are the same points by construction. Behavior specced
    // from published CAM setup practice; the grid and its math are ours.
    ui.spacing();
    const SOURCES: [&str; 2] = ["Stock box", "Model bounds"];
    ui.combo(
        "Anchor on##wcs_src",
        &mut state.wcs_ui.anchor_source,
        &SOURCES,
        |s| Cow::Borrowed(*s),
    );
    match crate::app::anchor_reference(state) {
        Some((_, what)) => {
            let chosen = *SOURCES
                .get(state.wcs_ui.anchor_source.min(1))
                .unwrap_or(&SOURCES[0]);
            if !chosen.eq_ignore_ascii_case(what) {
                ui.text(format!("No {chosen} defined - anchoring on {what}."));
            }
            let mut z_level = state.wcs_ui.anchor_z.min(2) as i32;
            ui.radio_button_int("Bottom", &mut z_level, 0);
            ui.same_line();
            ui.radio_button_int("Middle", &mut z_level, 1);
            ui.same_line();
            ui.radio_button_int("Top", &mut z_level, 2);
            state.wcs_ui.anchor_z = z_level.clamp(0, 2) as usize;
            ui.checkbox(
                "Pick markers in viewport##wcs_pick",
                &mut state.wcs_ui.pick_anchor,
            );
            if state.wcs_ui.pick_anchor {
                ui.text("Click a yellow marker in the viewport to set the origin.");
            }
            let pts = crate::app::anchor_points(state);
            for row in 0..3usize {
                for col in 0..3usize {
                    let k = row * 3 + col;
                    if col > 0 {
                        ui.same_line();
                    }
                    let code = pts.get(k).map_or("?", |(_, name)| *name);
                    let clicked = if state.wcs_ui.anchor_xy == k {
                        ui.button(format!("[{code}]"))
                    } else {
                        ui.button(code)
                    };
                    if clicked
                        && let Some((pos, _)) = pts.get(k)
                    {
                        state.wcs_ui.anchor_xy = k;
                        let slot = *WCS_SLOTS
                            .get(state.wcs_ui.slot.min(WCS_SLOTS.len() - 1))
                            .unwrap_or(&WCS_SLOTS[0]);
                        state.pending.push_back(CommandKind::SetWcs {
                            slot: slot.to_string(),
                            pos: *pos,
                            roll_deg: state.wcs_ui.roll_deg,
                            pitch_deg: state.wcs_ui.pitch_deg,
                            yaw_deg: state.wcs_ui.yaw_deg,
                        });
                    }
                }
            }
        }
        None => ui.text("Import a model or set stock to anchor the origin on a point."),
    }

    if ui.button("Set Origin") {
        let slot = *WCS_SLOTS
            .get(state.wcs_ui.slot.min(WCS_SLOTS.len() - 1))
            .unwrap_or(&WCS_SLOTS[0]);
        state.pending.push_back(CommandKind::SetWcs {
            slot: slot.to_string(),
            pos: [state.wcs_ui.x, state.wcs_ui.y, state.wcs_ui.z],
            roll_deg: state.wcs_ui.roll_deg,
            pitch_deg: state.wcs_ui.pitch_deg,
            yaw_deg: state.wcs_ui.yaw_deg,
        });
    }

    ui.spacing();
    if let Some(table) = ui.begin_table("wcs_readout", 2) {
        for slot in WCS_SLOTS {
            ui.table_next_row();
            ui.table_next_column();
            ui.text(slot);
            ui.table_next_column();
            let loc = model::find_by_name(&state.world, slot)
                .and_then(|entity| model::transform_of(&state.world, &entity));
            match loc {
                Some(loc) => ui.text(format!(
                    "{:.2}, {:.2}, {:.2}  yaw {:.1}",
                    loc.pos.x,
                    loc.pos.y,
                    loc.pos.z,
                    quat_yaw_deg(loc.rot)
                )),
                None => ui.text("-"),
            }
        }
        table.end();
    }
}
