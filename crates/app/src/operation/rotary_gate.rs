//! The rotary kinds' axis-letter gate (M13.5 B): which machine
//! rotary turns the part a Rotary Contour/Parallel/Offset poses
//! about. The letter is the MACHINE's word, never the kind's: a mill
//! table's trunnion is A or B, a mill-turn's spindle is C, and the
//! fitted axis the generator turned about must be one the mounted
//! machine states - refused by name where it states none.
//!
//! The C half is PINNED, not delivered: the post's pose law
//! (`cl::axis_to_angles`) renders a stated tool AXIS as [A, B, 0] -
//! a direction vector has two degrees of freedom and C is the one it
//! cannot carry - so a part turning about the spindle would post as
//! A/B swings the mill-turn does not have. Until the stream carries
//! a rotary ANGLE beside the tool axis, a C-axis rotary op refuses
//! here, by name, instead of posting a wrong letter.
//!
//! Provenance: our own wiring of the machine inventory to the pose
//! law; axis lettering per RS-274 / ISO 6983 convention. No vendor
//! material consulted.

use crate::machine::AxisLimits;
use crate::machining2d::GenError;

/// A cosine this close to one reads two unit directions as the same
/// line (either sign - a rotary turns both ways).
const SAME_LINE: f64 = 0.999;

/// The letter the mounted machine gives the rotary line `dir` (unit,
/// world frame) for `kind`. `None` machine, or a machine whose
/// rotaries state no axis vector, checks nothing (the unstated-limit
/// law) and answers `Ok(None)`. A stated inventory must hold a rotary
/// on that line, and that rotary must not be C (pinned, see the
/// module doc).
pub(super) fn rotary_letter(
    machine: Option<&(String, Vec<AxisLimits>)>,
    dir: [f64; 3],
    kind: &str,
) -> Result<Option<char>, GenError> {
    let Some((model, rotaries)) = machine else {
        return Ok(None);
    };
    let stated: Vec<&AxisLimits> = rotaries.iter().filter(|a| a.axis.is_some()).collect();
    if stated.is_empty() {
        return Ok(None);
    }
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if !(len.is_finite() && len > 1e-12) {
        return Err(GenError::Invalid(format!("{kind}: the fitted rotary axis states no direction")));
    }
    let hit = stated.iter().find(|a| {
        a.axis.is_some_and(|v| {
            let vl = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            vl > 1e-12
                && ((v[0] * dir[0] + v[1] * dir[1] + v[2] * dir[2]) / (vl * len)).abs() >= SAME_LINE
        })
    });
    let Some(ax) = hit else {
        let names: Vec<&str> = stated.iter().map(|a| a.name.as_str()).collect();
        return Err(GenError::Invalid(format!(
            "{kind} turns the part about ({:.3}, {:.3}, {:.3}); {model} states no rotary on that line (its rotaries: {}) - a mill table's A/B or a mill-turn spindle's C must hold it",
            dir[0] / len,
            dir[1] / len,
            dir[2] / len,
            if names.is_empty() { "none".to_owned() } else { names.join(", ") }
        )));
    };
    let letter = ax.name.chars().next().map(|c| c.to_ascii_uppercase());
    if letter == Some('C') {
        return Err(GenError::Invalid(format!(
            "{kind} would turn the part on {model}'s {} (the spindle as C axis): the post's pose law states a tool axis as A/B only and carries no C word yet - C-axis live tooling is pinned until the stream carries the rotary angle (M13.5 B)",
            ax.name
        )));
    }
    Ok(letter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rot(name: &str, axis: [f64; 3]) -> AxisLimits {
        AxisLimits {
            name: name.into(),
            rotary: true,
            axis: Some(axis),
            ..Default::default()
        }
    }

    #[test]
    fn no_machine_checks_nothing() {
        assert_eq!(rotary_letter(None, [1.0, 0.0, 0.0], "Rotary Contour").ok(), Some(None));
    }

    #[test]
    fn a_table_trunnion_names_its_letter_either_way_round() {
        let m = ("Bench".to_owned(), vec![rot("A", [1.0, 0.0, 0.0])]);
        assert_eq!(rotary_letter(Some(&m), [1.0, 0.0, 0.0], "k").ok(), Some(Some('A')));
        assert_eq!(rotary_letter(Some(&m), [-1.0, 0.0, 0.0], "k").ok(), Some(Some('A')));
    }

    #[test]
    fn a_missing_line_and_the_spindle_both_refuse_by_name() {
        let b = ("Bench".to_owned(), vec![rot("B", [0.0, 1.0, 0.0])]);
        let e = rotary_letter(Some(&b), [1.0, 0.0, 0.0], "Rotary Offset").err();
        assert!(e.is_some_and(|e| e.to_string().contains("states no rotary on that line")));
        let c = ("MT".to_owned(), vec![rot("C", [0.0, 0.0, 1.0])]);
        let e = rotary_letter(Some(&c), [0.0, 0.0, 1.0], "Rotary Contour").err();
        assert!(e.is_some_and(|e| e.to_string().contains("pinned")));
    }
}
