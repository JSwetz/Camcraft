//! The inverted-machine-axis note on the viewport's origin triad (#375).
//!
//! Provenance: our own design. The triad stays the right-handed ISO 841
//! program frame the whole CAM side keeps; a machine whose slide runs
//! the other way (#373, posted mirrored by #374) is stated beside the
//! axis letter in the same words as the setup sheet and the NC header
//! (`machine::inverted_axis_line`) - never by flipping the arrow, which
//! would draw a left-handed frame the toolpaths are not in.

use crate::state::State;

/// The triad's axis letters, in [`triad_notes`] order.
const LETTERS: [&str; 3] = ["X", "Y", "Z"];

/// One note per triad axis (X, Y, Z): `Some("Machine X+ moves left
/// (inverted axis)")` when the machine the shown setup mounts states
/// that linear axis inverted, `None` otherwise - all `None` for a
/// standard machine or no machine at all.
pub(crate) fn triad_notes(state: &State) -> [Option<String>; 3] {
    let setup = crate::interact::shown_setup(state);
    let (_, machine) = camcraft_app::machine::of_setup(&state.world, setup.as_deref());
    let inverted: Vec<String> = machine
        .map(|m| m.inverted_axes().into_iter().map(str::to_owned).collect())
        .unwrap_or_default();
    LETTERS.map(|l| inverted.iter().any(|i| i == l).then(|| camcraft_app::machine::inverted_axis_line(l)))
}

/// How far below its axis letter a note sits, in viewport uv (the
/// letter is drawn at 1.4x the body font, so a note under it clears it).
pub(crate) const NOTE_DROP_UV: f32 = 0.045;
