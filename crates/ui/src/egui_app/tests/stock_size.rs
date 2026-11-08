//! #197: the Stock page shows the models' size through the real shell.
//! Provenance: our own page law.

use super::*;

/// #197: the Stock page shows the models' size above the stock fields -
/// the seeded 60 x 60 x 30 block - so stock can be sized against it.
#[test]
fn the_stock_page_shows_the_models_size() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Stock;
    harness.run();
    assert!(
        harness.query_by_label("Models: 60.000 x 60.000 x 30.000 mm").is_some(),
        "the stock page names the seeded block's size"
    );
}
