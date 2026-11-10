//! The operation page: the Probe kind's fields (#108) - the mode, the
//! touch directions and the expected contact on Geometry, the safe
//! plane on Heights, the seek / measure numbers on Passes and Feeds,
//! and the result action on Machine. Every value edits the page draft;
//! Apply rides the command history (EditOperationParams, undoable).
//!
//! Provenance: our own UI over our own probe parameters (touch-probe
//! practice and NIST RS274NGC G38.2); no vendor material or text.

use super::fields::*;
use camcraft_app::probing::{ProbeDir, ProbeMode};

const MODES: [&str; 5] = ["Face", "Edge", "Outside corner", "Bore centre", "Boss centre"];
const DIRS: [&str; 5] = ["+X", "-X", "+Y", "-Y", "-Z"];
const XDIRS: [&str; 2] = ["+X", "-X"];
const YDIRS: [&str; 2] = ["+Y", "-Y"];
const RESULTS: [&str; 7] = ["Report and stop", "Set G54", "Set G55", "Set G56", "Set G57", "Set G58", "Set G59"];

const MODE_OF: [ProbeMode; 5] = [ProbeMode::Face, ProbeMode::Edge, ProbeMode::Corner, ProbeMode::Bore, ProbeMode::Boss];
const DIR_OF: [ProbeDir; 5] = [ProbeDir::XPos, ProbeDir::XNeg, ProbeDir::YPos, ProbeDir::YNeg, ProbeDir::ZNeg];

fn dir_combo(ui: &mut Ui, id: &str, d: &mut ProbeDir, names: &[&str], of: &[ProbeDir]) {
    let mut i = of.iter().position(|x| x == d).unwrap_or(0);
    combo(ui, id, &mut i, names);
    if let Some(v) = of.get(i) {
        *d = *v;
    }
}

pub(super) fn fields(ui: &mut Ui, world: &World, params: &mut OpParams, cx: &mut Cx<'_>) {
    let (u, tab, op_ctx) = (cx.u, cx.tab, cx.op_ctx);
    let OpParams::Probe {
        clearance_z,
        mode,
        dir,
        dir_y,
        target,
        radius,
        approach,
        overtravel,
        backoff,
        fast_feed,
        slow_feed,
        depth,
        inset,
        set_wcs,
        tolerance,
    } = params
    else {
        return;
    };
    if tab == SEC_GEO {
        let mut mi = MODE_OF.iter().position(|m| m == mode).unwrap_or(0);
        prop_row(ui, "Find", |ui| combo(ui, "probe.mode", &mut mi, &MODES));
        if let Some(m) = MODE_OF.get(mi) {
            *mode = *m;
        }
        match *mode {
            ProbeMode::Face | ProbeMode::Edge => {
                prop_row(ui, "Touch toward", |ui| dir_combo(ui, "probe.dir", dir, &DIRS, &DIR_OF));
            }
            ProbeMode::Corner => {
                prop_row(ui, "X touch toward", |ui| {
                    dir_combo(ui, "probe.dirx", dir, &XDIRS, &[ProbeDir::XPos, ProbeDir::XNeg]);
                });
                prop_row(ui, "Y touch toward", |ui| {
                    dir_combo(ui, "probe.diry", dir_y, &YDIRS, &[ProbeDir::YPos, ProbeDir::YNeg]);
                });
            }
            ProbeMode::Bore | ProbeMode::Boss => {
                prop_row(ui, "Radius", |ui| num(ui, u, radius));
            }
        }
        field(ui, "Expected contact");
        prop_row(ui, "X", |ui| num(ui, u, &mut target[0]));
        prop_row(ui, "Y", |ui| num(ui, u, &mut target[1]));
        prop_row(ui, "Z", |ui| num(ui, u, &mut target[2]));
        hint(ui, "face: the point touched; edge / corner: the top edge point or vertex; bore / boss: the centre at its rim");
    }
    if tab == SEC_HEIGHTS {
        height_editor(ui, "Clearance (safe plane)", world, op_ctx.as_deref(), clearance_z, "probe.clearance");
    }
    if tab == SEC_PASSES {
        prop_row(ui, "Approach clearance", |ui| num(ui, u, approach));
        prop_row(ui, "Probe distance", |ui| num(ui, u, overtravel));
        prop_row(ui, "Back-off", |ui| num(ui, u, backoff));
        prop_row(ui, "Depth below edge", |ui| num(ui, u, depth));
        prop_row(ui, "Corner inset", |ui| num(ui, u, inset));
        hint(ui, "the probe distance is a search LIMIT past the expected contact, not a destination: the probe stops where it touches, and alarms if nothing trips by then - keep it small (over 2 mm is flagged)");
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Seek feed", |ui| num_feed(ui, u, fast_feed));
        prop_row(ui, "Measure feed", |ui| num_feed(ui, u, slow_feed));
    }
    if tab == SEC_MACHINE {
        let mut ri = set_wcs.map_or(0, |s| usize::from(s) + 1);
        prop_row(ui, "Result", |ui| combo(ui, "probe.result", &mut ri, &RESULTS));
        *set_wcs = ri.checked_sub(1).and_then(|s| u8::try_from(s).ok()).filter(|s| *s <= 5);
        prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
        hint(ui, "a found position further than this from the model stops the program with a message");
    }
}
