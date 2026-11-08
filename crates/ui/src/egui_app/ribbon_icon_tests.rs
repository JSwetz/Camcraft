//! Issue #207: on a fresh install (no pins) every ribbon cell wore its
//! glyph except the operation-section doors and the machine door, so the
//! whole Create run read as text - "missing icons" on Windows and Fedora.
//! Every cell of every workspace's ribbon now names a glyph, except the
//! View door (a layout list, captioned by its own words).
//! Provenance: our own tests over our own ribbon.

use super::panes::{Cmd, icon_of, ribbon_plan};
use crate::state::{CommandKind, State};

#[test]
fn every_ribbon_cell_of_every_workspace_wears_a_glyph() {
    for (kind, machine) in [
        (camcraft_app::machine::Kind::Mill, "VMC 3-axis"),
        (camcraft_app::machine::Kind::Lathe, "Lathe"),
        (camcraft_app::machine::Kind::MillTurn, "Mill-turn"),
    ] {
        let mut state = State::new();
        state.pending.push_back(CommandKind::LoadBuiltinMachine(machine.into()));
        state.drain_actions();
        assert_eq!(camcraft_app::machine::kind_of(&state.world), kind, "{machine} mounted");
        for group in ribbon_plan(kind, &[]) {
            for cell in group.cells {
                if cell == Cmd::View {
                    continue;
                }
                assert!(
                    icon_of(cell, &state).is_some(),
                    "{machine}: the {:?} cell in '{}' has no glyph",
                    cell,
                    group.title
                );
            }
        }
    }
}

#[test]
fn the_machine_door_wears_its_workspace() {
    let mut state = State::new();
    let mill = icon_of(Cmd::Machine, &state);
    state.pending.push_back(CommandKind::LoadBuiltinMachine("Lathe".into()));
    state.drain_actions();
    let lathe = icon_of(Cmd::Machine, &state);
    assert!(mill.is_some() && lathe.is_some() && mill != lathe, "{mill:?} / {lathe:?}");
}
