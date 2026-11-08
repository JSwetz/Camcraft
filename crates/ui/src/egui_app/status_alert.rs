//! The status strip's alert reading (#192): a refusal on the status line
//! is drawn so it cannot be missed - the palette's error ink, bold, on an
//! error-tinted band, behind a "!" marker - until the next status
//! replaces it. Ordinary status text (saved, generated, ready) stays as
//! it was, dim. Provenance: our own UI law.

use crate::state::State;

/// Phrasings the shell's own refusals use when they set the status line
/// directly rather than through `State::refuse` (a dark cell's gate note,
/// an import or export that could not run, an origin that was not set).
const REFUSAL_MARKS: [&str; 7] = [
    " refused",
    " failed",
    " not set:",
    " not changed:",
    " not aligned:",
    " no longer ",
    "not generated",
];

/// Does the status line carry a refusal? A command refused through the
/// feedback path (its message IS the status), a gate note ("cannot …"),
/// or one of the [`REFUSAL_MARKS`] phrasings.
pub(super) fn is_alert(state: &State) -> bool {
    let s = state.status.as_str();
    if s.is_empty() {
        return false;
    }
    state.feedback.as_ref().is_some_and(|(_, m)| m == s)
        || s.starts_with("cannot ")
        || REFUSAL_MARKS.iter().any(|mark| s.contains(mark))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::CommandKind;

    #[test]
    fn ordinary_status_is_not_an_alert() {
        let mut state = State::new();
        for s in ["ready", "saved /tmp/a.camproj", "generated Contour1: 105 moves", "stock set", ""] {
            state.status = s.to_owned();
            assert!(!is_alert(&state), "{s:?} read as an alert");
        }
    }

    #[test]
    fn refusals_are_alerts() {
        let mut state = State::new();
        for s in [
            "cannot add an operation: no tool in the document - add one (Setup > Tools)",
            "import refused: not a mesh",
            "save failed: permission denied",
            "origin not set: the picked face is gone",
            "WCS not aligned: pick a flat face - it sets the Z axis",
            "the picked vertex of 'block' is no longer in the model",
        ] {
            state.status = s.to_owned();
            assert!(is_alert(&state), "{s:?} not read as an alert");
        }
    }

    #[test]
    fn a_command_refused_through_the_feedback_path_is_an_alert() {
        let mut state = State::new();
        // Deleting what is not there is refused by the command door.
        state.pending.push_back(CommandKind::DeleteByName("nobody".into()));
        state.drain_actions();
        assert!(state.feedback.is_some(), "the refusal is recorded: {}", state.status);
        assert!(is_alert(&state), "{}", state.status);
        state.status = "ready".into();
        assert!(!is_alert(&state), "a later status retires the alert");
    }
}
