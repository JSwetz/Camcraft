//! #225: the Feeds tab shows the op tool's spindle speed in an editable
//! row with an rpm / surface speed switch. Provenance: our own page law.

use super::*;

#[test]
fn the_feeds_tab_offers_the_spindle_speed_and_its_unit_switch() {
    let mut harness = shell_harness();
    harness.run();
    harness.get_by_label("Feeds").click();
    harness.run();
    assert!(harness.query_by_label("Spindle speed").is_some(), "the row stands on the Feeds tab");
    assert!(!harness.state().state.spindle_as_vc);
    harness.get_by_label("rpm").click();
    harness.run();
    assert!(harness.state().state.spindle_as_vc, "the switch shows surface speed");
    assert!(harness.query_by_label("surface").is_some(), "the switch now reads 'surface'");
}
