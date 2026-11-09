//! M13.4 law: the pure vertex-pick function on a cube seen down its body
//! diagonal - the front corner and the hidden back corner project onto
//! the same pixel, and only the front one may win.
//!
//! Provenance: our own test, standard picking geometry.

use camcraft_app::geometry::{Point3, cube};
use camcraft_app::pick::sight_blocked;

use crate::vertex_pick::nearest_vertex;
use crate::viewport::Camera;

const RECT: ([f32; 2], [f32; 2]) = ([0.0, 0.0], [400.0, 400.0]);
const CENTER: [f32; 2] = [200.0, 200.0];

/// Looking from +X+Y+Z at the origin: (10,10,10) faces the eye and
/// (-10,-10,-10) sits straight behind it.
fn diagonal_cam() -> Camera {
    Camera {
        target: [0.0, 0.0, 0.0],
        yaw: std::f64::consts::FRAC_PI_4,
        pitch: (1.0f64 / 2.0f64.sqrt()).atan(),
        dist: 100.0,
        ..Camera::default()
    }
}

fn corners() -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    for x in [-10.0, 10.0] {
        for y in [-10.0, 10.0] {
            for z in [-10.0, 10.0] {
                out.push([x, y, z]);
            }
        }
    }
    out
}

#[test]
fn nearest_visible_corner_wins_and_the_hidden_back_corner_never_does() {
    let cam = diagonal_cam();
    let mvp = cam.mvp(1.0);
    let eye = cam.eye();
    let tris = cube(Point3::new(-10.0, -10.0, -10.0), Point3::new(10.0, 10.0, 10.0)).facets;
    let pts = corners();
    let visible = |i: usize| !sight_blocked(&tris, eye, pts[i], 1e-3);
    let front = pts.iter().position(|p| *p == [10.0, 10.0, 10.0]).expect("front");
    let back = pts.iter().position(|p| *p == [-10.0, -10.0, -10.0]).expect("back");

    // Both project to the image center; the occlusion ray decides.
    assert_eq!(nearest_vertex(&pts, &mvp, RECT, CENTER, 8.0, visible), Some(front));

    // The back corner alone, at the exact pixel it projects to: hidden.
    let only_back = [pts[back]];
    let back_visible = |_: usize| !sight_blocked(&tris, eye, only_back[0], 1e-3);
    assert_eq!(nearest_vertex(&only_back, &mvp, RECT, CENTER, 8.0, back_visible), None, "a corner behind the part is never picked");

    // Without the depth test the same pixel finds a corner (proof the
    // test above refused on depth, not on distance).
    assert!(nearest_vertex(&only_back, &mvp, RECT, CENTER, 8.0, |_| true).is_some());

    // Far from every corner: nothing within the radius.
    assert_eq!(nearest_vertex(&pts, &mvp, RECT, [5.0, 5.0], 8.0, visible), None);
}

#[test]
fn degenerate_inputs_pick_nothing() {
    let cam = diagonal_cam();
    let mvp = cam.mvp(1.0);
    let pts = corners();
    assert_eq!(nearest_vertex(&pts, &mvp, ([0.0, 0.0], [0.0, 0.0]), CENTER, 8.0, |_| true), None, "no image rect");
    assert_eq!(nearest_vertex(&pts, &mvp, RECT, CENTER, f32::NAN, |_| true), None, "NaN radius");
    assert_eq!(nearest_vertex(&[[f64::NAN; 3]], &mvp, RECT, CENTER, 8.0, |_| true), None, "NaN vertex");
    assert_eq!(nearest_vertex(&[], &mvp, RECT, CENTER, 8.0, |_| true), None);
}
