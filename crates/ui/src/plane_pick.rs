//! The lay-on-a-plane pick draft (#330): while a Setups page row collects
//! a pick, a viewport Face click lands here instead of in the open
//! operation's geometry. One flat face; a click replaces it. #554: faces
//! only - arming sets Face mode, so a click near an edge still takes the
//! face, and an edge that arrives anyway is refused by name (an edge
//! cannot set a plane; edges set an axis, `crate::axis_pick`). Ephemeral
//! view state like the boundary draft - nothing here writes the world;
//! Apply queues the undoable `AlignPartToSetup` through
//! `CommandKind::LayPartOnPlane`.
//!
//! Provenance: our own UI over plain datum-face practice; no vendor
//! material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel};

use crate::state::{GeoPick, State};

/// The pick being collected for one setup's lay-on-a-plane.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanePick {
    /// The setup the pick will align.
    pub setup: String,
    pub picks: Vec<GeoSel>,
}

/// Start (or restart) collecting a face for `setup`: Face mode, so a
/// click anywhere on a face - near its edge included - takes the face.
pub fn arm(state: &mut State, setup: &str) {
    crate::axis_pick::disarm(state); // #557: one draft collects at a time
    state.points_pick = None; // #555
    state.plane_pick = Some(PlanePick { setup: setup.to_owned(), picks: Vec::new() });
    state.geo_pick = GeoPick::Face;
    state.status = format!("{setup}: pick a flat face to lay on the plane");
}

/// Stop collecting; the draft is dropped.
pub fn disarm(state: &mut State) {
    state.plane_pick = None;
}

/// One viewport pick into the draft: a face replaces it; anything else
/// is refused by name and changes nothing.
pub fn take(state: &mut State, pick: GeoSel) {
    let Some(draft) = state.plane_pick.as_mut() else { return };
    let setup = draft.setup.clone();
    state.status = match pick.feature {
        FeatureSel::Face(_) => {
            draft.picks = vec![pick];
            format!("{setup}: face picked for the plane")
        }
        FeatureSel::Chain(_) => format!(
            "{setup}: {} of '{}' is an edge - an edge cannot set a plane; pick a flat face \
             (edges set an axis: Axes in the plane)",
            pick.feature, pick.entity
        ),
        FeatureSel::Patch(_) | FeatureSel::Vertex(_) => {
            format!("{setup}: {} of '{}' is not a flat face - pick a flat face", pick.feature, pick.entity)
        }
    };
}
