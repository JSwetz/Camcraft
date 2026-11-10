//! #670: the Plasma Profile appears on a plasma table's ribbon - its
//! own Plasma section, lit, with a glyph - and on no other workspace's
//! (mill, lathe, mill-turn), where its cell is dark and the tip names
//! the plasma table to mount. The ribbon and the command door read the
//! one registry word (`KindInfo::admits`).
//! Provenance: our own tests over our own ribbon.

use super::panes::{self, Cmd, icon_of, ribbon_plan};
use crate::state::{CommandKind, State};
use camcraft_app::machine::Kind;
use camcraft_app::operation::{OpType, Section};

fn sections(kind: Kind) -> Vec<Section> {
    ribbon_plan(kind, &[]).into_iter().filter_map(|g| g.section).collect()
}

fn mounted(machine: &str, kind: Kind) -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 40.0, dz: 6.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.pending.push_back(CommandKind::LoadBuiltinMachine(machine.into()));
    state.pending.push_back(CommandKind::AddTool {
        name: "PT-KERF-150".into(),
        tool: camcraft_app::tools::torch_tool(1.5),
    });
    state.drain_actions();
    assert_eq!(camcraft_app::machine::kind_of(&state.world), kind, "{machine} mounted");
    state
}

#[test]
fn the_plasma_profile_shows_on_a_plasma_tables_ribbon_only() {
    let plasma = OpType::PlasmaProfile.ordinal();
    assert_eq!(sections(Kind::Plasma), vec![Section::Plasma], "a plasma table shows its own section alone");
    assert_eq!(Section::Plasma.kinds_for(Kind::Plasma), vec![plasma]);
    for kind in [Kind::Mill, Kind::Lathe, Kind::MillTurn] {
        assert!(!sections(kind).contains(&Section::Plasma), "{kind:?} shows no plasma section");
        assert!(ribbon_plan(kind, &[plasma]).iter().all(|g| !g.cells.contains(&Cmd::Create(plasma))), "{kind:?}: a plasma pin stands nowhere");
    }

    let state = mounted("Tormach 1300PL", Kind::Plasma);
    assert!(panes::enabled(Cmd::Create(plasma), &state), "the plasma cell is lit on the 1300PL");
    assert!(!panes::enabled(Cmd::Create(OpType::Contour.ordinal()), &state), "no milling on a plasma table");
    assert!(icon_of(Cmd::More(Section::Plasma), &state).is_some(), "the plasma door wears a glyph");
    assert!(icon_of(Cmd::Create(plasma), &state).is_some());
    for group in ribbon_plan(Kind::Plasma, &[]) {
        for cell in group.cells {
            if !matches!(cell, Cmd::View | Cmd::Theme) {
                assert!(icon_of(cell, &state).is_some(), "{cell:?} in '{}' has no glyph", group.title);
            }
        }
    }

    let mill = mounted("VMC 3-axis", Kind::Mill);
    assert!(!panes::enabled(Cmd::Create(plasma), &mill), "a mill strikes no arc");
    let tip = panes::tip_of(Cmd::Create(plasma), &mill);
    assert!(tip.contains("plasma table") && !tip.contains("mill-turn"), "{tip}");
}

/// The tool editor's draft speaks the torch (#670): its kind index
/// round-trips, and the kind list the combo reads names it.
#[test]
fn the_tool_editor_draft_round_trips_a_torch() {
    let torch = camcraft_app::tools::torch_tool(1.5);
    let ui = crate::state::ToolUi::from_tool("PT", &torch, false);
    assert_eq!(ui.tool_kind(), camcraft_app::tools::ToolKind::Torch);
    assert_eq!(ui.to_tool(), torch);
}
