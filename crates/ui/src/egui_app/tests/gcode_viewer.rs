//! #256 in the shell: the G-code viewer lists every program line from
//! the same left edge, the way a program reads, and a line wider than
//! the window stays on its one row so the scroll cursor stays exact.
//! Provenance: our own tests over our own viewer; no HSMWorks material
//! involved.

use super::*;

/// A program whose lines differ a lot in length, plus one comment far
/// wider than the viewer's default 560 px.
const PROGRAM: [&str; 5] = [
    "%",
    "G00 G90 G54 X12.5 Y-40. S8000 M3",
    "M30",
    "(A COMMENT FAR WIDER THAN THE VIEWER WINDOW - TOOL 1 IS A 6 MM FLAT END MILL, 3 FLUTES, 20 MM FLUTE LENGTH, 50 MM STICKOUT)",
    "%",
];

/// The viewer's label for program line `i` (0-based): the right-aligned
/// line number, two spaces, then the line (`pages::gcode_listing`).
fn row_label(i: usize) -> String {
    format!("{:>4}  {}", i + 1, PROGRAM[i])
}

/// The shell with the viewer open on [`PROGRAM`], laid out.
fn viewer() -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    harness.state_mut().state.post_view = Some(PostView {
        path: std::path::PathBuf::from("p.nc"),
        program: PROGRAM.join("\n"),
        line_pos: vec![None; PROGRAM.len()],
        open: true,
        line: 0,
        blocks: Vec::new(),
        trace: false,
        not_judged: None,
    });
    harness.run();
    harness
}

/// Where the frame painted program line `i`'s text: the left edge of
/// its galley, read off the frame's shapes. The accessibility rect is
/// the row's box, which spans the row whether the text inside it is
/// centred or not, so it cannot tell the two apart.
fn painted_left(h: &Harness<'static, ShellUi>, i: usize) -> f32 {
    fn find(shape: &egui::Shape, text: &str) -> Option<f32> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos.x + t.galley.rect.left()),
            egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, text)),
            _ => None,
        }
    }
    let text = row_label(i);
    h.output()
        .shapes
        .iter()
        .find_map(|c| find(&c.shape, &text))
        .unwrap_or_else(|| panic!("the frame painted no {text:?}"))
}

#[test]
fn every_gcode_line_starts_at_the_listings_left_edge() {
    let harness = viewer();
    let left = |i: usize| painted_left(&harness, i);
    let (short, long) = (left(2), left(1));
    assert!(
        (short - long).abs() < 0.5,
        "\"M30\" starts at x {short}, the longer motion line at x {long}: centred, not listed"
    );
    assert!(
        (left(0) - long).abs() < 0.5,
        "the '%' line starts there too"
    );
}

#[test]
fn a_line_wider_than_the_viewer_stays_on_one_row() {
    let harness = viewer();
    let rect = |i: usize| harness.get_by_label(&row_label(i)).rect();
    let (wide, short) = (rect(3), rect(2));
    assert!(
        (wide.height() - short.height()).abs() < 0.5,
        "the wide comment is {} tall, a one-line row {}: it wrapped",
        wide.height(),
        short.height()
    );
    assert!(
        (wide.left() - short.left()).abs() < 0.5,
        "and it starts at the same left edge"
    );
}
