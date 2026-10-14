//! Setup pages: stock solid and work-coordinate editing, rendered inside
//! the Operations pane when the browser points at Stock or a G5x slot.
//!
//! Provenance: our own pane, specced from public CAM setup conventions;
//! the G54..G59 offset codes are the published RS-274 / ISO 6983 ones.
//!
//! Like every panel this one never touches the world: it edits the
//! `State::stock_ui` / `State::wcs_ui` drafts and queues `CommandKind`s.
//! Reads of the world go through the `model::*` helpers only.

use std::borrow::Cow;

use dear_imgui_rs::Ui;
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

/// The Stock page (Operations pane, `UiTarget::Stock`).
pub(crate) fn stock_page(ui: &Ui, state: &mut State) {
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

/// A WCS page (Operations pane, `UiTarget::WcsSlot(slot)`): the slot is
/// chosen in the browser tree, not by a combo here, so the draft is
/// reloaded whenever the page disagrees with it.
pub(crate) fn wcs_page(ui: &Ui, state: &mut State, slot: usize) {
    if state.wcs_ui.slot != slot {
        load_wcs_slot(state, slot);
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
    ui.text("Quarter-turns about world axes (apply immediately):");
    for (i, (label, axis, sign)) in QUARTER_TURNS.iter().enumerate() {
        if i > 0 {
            ui.same_line();
        }
        if ui.button(format!("Turn {label}##wcs_turn{i}")) {
            turn_wcs(state, *axis, *sign);
        }
    }
    ui.same_line();
    if ui.button("Level##wcs_level") {
        state.wcs_ui.roll_deg = 0.0;
        state.wcs_ui.pitch_deg = 0.0;
        state.wcs_ui.yaw_deg = 0.0;
        apply_wcs_drafts(state);
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
                        state.wcs_ui.x = pos[0];
                        state.wcs_ui.y = pos[1];
                        state.wcs_ui.z = pos[2];
                        apply_wcs_drafts(state);
                    }
                }
            }
        }
        None => ui.text("Import a model or set stock to anchor the origin on a point."),
    }

    if ui.button("Set Origin") {
        apply_wcs_drafts(state);
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

/// Point the WCS draft at `slot` and show that slot's real numbers now,
/// not the previous slot's draft - a stale readout invites an accidental
/// move. A never-placed slot reads machine zero, which is the honest
/// default and what `Set Origin` will record first.
fn load_wcs_slot(state: &mut State, slot: usize) {
    let slot = slot.min(WCS_SLOTS.len() - 1);
    state.wcs_ui.slot = slot;
    let loaded = WCS_SLOTS
        .get(slot)
        .and_then(|name| model::find_by_name(&state.world, name))
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

/// Queue the WCS drafts (position + orientation) as one `SetWcs` command.
/// The spatial controls - turns, Level, the compass grid, viewport marker
/// clicks - use it so the gizmo follows the button press instead of waiting
/// for a re-apply; the numeric fields still wait for Set Origin, so typing
/// doesn't write one command per keystroke. A refused command leaves the
/// drafts visible for correction (validation lives in the command).
fn apply_wcs_drafts(state: &mut State) {
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

/// Quarter-turn the draft orientation about a world axis and apply, so the
/// triad moves where the user is looking (their feedback: turning should
/// update the gizmo, not wait for a re-apply). Composition is `turn * draft`
/// - about world axes, matching the label over the buttons.
fn turn_wcs(state: &mut State, axis: usize, sign: f64) {
    let q = quat_from_euler(
        state.wcs_ui.roll_deg.to_radians(),
        state.wcs_ui.pitch_deg.to_radians(),
        state.wcs_ui.yaw_deg.to_radians(),
    );
    let turn = quat_from_axis_angle(axis, sign * 90.0f64.to_radians());
    let (r, p, y) = quat_to_euler(quat_mul(turn, q));
    state.wcs_ui.roll_deg = r.to_degrees();
    state.wcs_ui.pitch_deg = p.to_degrees();
    state.wcs_ui.yaw_deg = y.to_degrees();
    apply_wcs_drafts(state);
}

#[cfg(test)]
mod turn_tests {
    //! The turn/Level buttons are commands of their own now: the world -
    //! and with it the viewport gizmo - moves on the press.

    use super::*;
    use hsmworks_app::geometry::{Point3, transform_point};

    fn place(state: &mut State, pos: [f64; 3], yaw_deg: f64) {
        state.pending.push_back(CommandKind::SetWcs {
            slot: "G54".to_string(),
            pos,
            roll_deg: 0.0,
            pitch_deg: 0.0,
            yaw_deg,
        });
        state.drain_actions();
    }

    fn location(state: &State, slot: &str) -> hsmworks_app::schema::Location {
        let entity = model::find_by_name(&state.world, slot).expect("offset entity");
        model::transform_of(&state.world, &entity).expect("location")
    }

    #[test]
    fn a_turn_moves_the_world_not_just_the_drafts() {
        let mut state = State::new();
        place(&mut state, [10.0, 0.0, 0.0], 0.0);
        state.sync_setup_ui();
        turn_wcs(&mut state, 0, 1.0); // X+ quarter about the world X
        state.drain_actions();
        let loc = location(&state, "G54");
        assert_eq!([loc.pos.x, loc.pos.y, loc.pos.z], [10.0, 0.0, 0.0]);
        // Right-hand +90 about X takes +Z to -Y.
        let p = transform_point(Point3::new(0.0, 0.0, 1.0), loc.pos, loc.rot);
        assert!([(p.x - 10.0), (p.y + 1.0), p.z].iter().all(|v| v.abs() < 1e-9), "{p:?}");
    }

    #[test]
    fn four_turns_about_one_axis_return_to_the_start() {
        let mut state = State::new();
        place(&mut state, [0.0, 0.0, 0.0], 30.0);
        state.sync_setup_ui();
        for _ in 0..4 {
            turn_wcs(&mut state, 2, 1.0);
        }
        state.drain_actions();
        let loc = location(&state, "G54");
        // Same rotation as the 30-degree start: quaternion equal up to sign.
        let start = quat_from_euler(0.0, 0.0, 30.0f64.to_radians());
        let dot: f64 = start.iter().zip(loc.rot).map(|(a, b)| a * b).sum();
        assert!((dot.abs() - 1.0).abs() < 1e-9, "closed the circle: {:?}", loc.rot);
    }

    #[test]
    fn turning_an_unplaced_slot_applies_the_typed_position() {
        let mut state = State::new();
        state.wcs_ui.slot = 1; // G55
        state.wcs_ui.x = 5.0;
        turn_wcs(&mut state, 1, -1.0); // Y- quarter
        state.drain_actions();
        let loc = location(&state, "G55");
        assert_eq!([loc.pos.x, loc.pos.y, loc.pos.z], [5.0, 0.0, 0.0]);
        // Right-hand -90 about Y takes +X to +Z: the drafts, not just a
        // rotation, rode along.
        let p = transform_point(Point3::new(1.0, 0.0, 0.0), loc.pos, loc.rot);
        assert!([(p.x - 5.0), p.y, (p.z - 1.0)].iter().all(|v| v.abs() < 1e-9), "{p:?}");
    }
}
