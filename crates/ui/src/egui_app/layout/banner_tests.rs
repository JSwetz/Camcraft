//! #261: the status banner's located detail lines - a warning line that
//! names a move is a link, a click on it reports its index (the op page
//! frames that move), and a plain line stays plain text.
//! Provenance: our own page law.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::status_banner_located;

#[derive(Default)]
struct Probe {
    clicked: Option<usize>,
}

fn banner() -> Harness<'static, Probe> {
    Harness::builder().with_size(egui::Vec2::new(480.0, 240.0)).build_ui_state(
        |ui, p: &mut Probe| {
            let details = ["warning: move 84 is warned".to_owned(), "entry: plain note".to_owned(), "warning: move 9".to_owned()];
            if let Some(i) = status_banner_located(ui, "t", "OK · 3 warnings", egui::Color32::YELLOW, &details, &[true, false, true]) {
                p.clicked = Some(i);
            }
        },
        Probe::default(),
    )
}

#[test]
fn a_located_warning_line_reports_its_click_and_a_plain_line_does_not() {
    let mut h = banner();
    h.run();
    assert!(h.query_by_label_contains("move 84").is_none(), "collapsed until the line is clicked");
    h.get_by_label_contains("3 warnings").click();
    h.run();
    h.get_by_label_contains("plain note").click();
    h.run();
    assert_eq!(h.state().clicked, None, "a plain detail is no link");
    h.get_by_label_contains("move 9").click();
    h.run();
    assert_eq!(h.state().clicked, Some(2), "the third line, the second link");
}
