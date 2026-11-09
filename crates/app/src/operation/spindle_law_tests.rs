//! Tests of the spindle law's refusal text (issue #113): the untrusted
//! fit - an axis read through off-centre features the fit could not set
//! aside - says the reading is biased and that moving the work offset
//! may not fix it, and never gives the misplaced part's advice alone.
//! Provenance: our own readings; no vendor material.

use super::{refusal, SpindleReading, SPINDLE_TOL};

fn reading(trusted: bool, outlier_share: f64) -> SpindleReading {
    SpindleReading {
        lean_deg: 0.25,
        offset_mid: 0.137,
        mid_z: 0.0,
        radial: 0.137,
        axial: 0.02,
        outlier_share,
        trusted,
    }
}

#[test]
fn an_untrusted_fit_is_refused_as_biased_naming_its_share() {
    let e = refusal(&reading(false, 0.42), "G54").to_string();
    for s in [
        "cannot be read to the lathe spindle's",
        &format!("{SPINDLE_TOL} mm"),
        "42.0% of the part's turned (non-end-face) surface does not revolve about any one axis",
        "0.137 mm off the G54 origin",
        "leaning 0.250 degrees",
        "biased by off-centre features",
        "moving the work offset may not fix it",
        "machine those features in a later setup",
    ] {
        assert!(e.contains(s), "the untrusted refusal names '{s}': {e}");
    }
    assert!(!e.contains("move the work offset onto it"), "a biased fit is not a misplaced part: {e}");
}

#[test]
fn an_untrusted_fit_never_shows_a_zero_share() {
    let e = refusal(&reading(false, 1e-6), "G55").to_string();
    assert!(e.contains("0.1% of the part's turned"), "{e}");
    assert!(!e.contains(" 0.0%"), "{e}");
}

#[test]
fn a_trusted_fit_off_the_spindle_is_refused_as_misplaced() {
    let e = refusal(&reading(true, 0.0), "G54").to_string();
    for s in ["is not the lathe spindle", "0.137 mm off the G54 origin", "move the work offset onto it"] {
        assert!(e.contains(s), "the misplaced refusal names '{s}': {e}");
    }
    assert!(!e.contains("biased"), "{e}");
    assert!(!e.contains("set aside"), "nothing set aside, nothing named: {e}");
}
