//! The lay-on-a-plane pick draft (#330): while a Setups page row collects
//! picks, a viewport Face/Edge click lands here instead of in the open
//! operation's geometry. One flat face, or up to two straight edges (a
//! face click replaces the draft; an edge click toggles that edge and
//! keeps the last two). Ephemeral view state like the boundary draft -
//! nothing here writes the world; Apply queues the undoable
//! `AlignPartToSetup` through `CommandKind::LayPartOnPlane`.
//!
//! Provenance: our own UI over plain datum-face practice; no vendor
//! material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel};

use crate::state::{GeoPick, State};

/// The picks being collected for one setup's lay-on-a-plane.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanePick {
    /// The setup the picks will align.
    pub setup: String,
    pub picks: Vec<GeoSel>,
}

/// Start (or restart) collecting picks for `setup`: Edge mode, so a click
/// near an edge takes the edge and one on a face takes the face.
pub fn arm(state: &mut State, setup: &str) {
    crate::axis_pick::disarm(state); // #557: one draft collects at a time
    state.plane_pick = Some(PlanePick { setup: setup.to_owned(), picks: Vec::new() });
    state.geo_pick = GeoPick::Edge;
    state.status = format!("{setup}: pick a flat face, or two straight edges, to lay on the plane");
}

/// Stop collecting; the draft is dropped.
pub fn disarm(state: &mut State) {
    state.plane_pick = None;
}

/// One viewport pick into the draft.
pub fn take(state: &mut State, pick: GeoSel) {
    let Some(draft) = state.plane_pick.as_mut() else { return };
    match pick.feature {
        FeatureSel::Chain(_) => {
            draft.picks.retain(|g| matches!(g.feature, FeatureSel::Chain(_)));
            if let Some(i) = draft.picks.iter().position(|g| *g == pick) {
                draft.picks.remove(i);
            } else {
                draft.picks.push(pick);
                let extra = draft.picks.len().saturating_sub(2);
                draft.picks.drain(..extra);
            }
        }
        _ => draft.picks = vec![pick],
    }
    let n = draft.picks.len();
    state.status = format!("{}: {n} pick{} for the plane", draft.setup, if n == 1 { "" } else { "s" });
}
