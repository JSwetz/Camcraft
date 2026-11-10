//! M13.4 law: the pure vertex-pick function on a cube seen down its body
//! diagonal - the front corner and the hidden back corner project onto
//! the same pixel, and only the front one may win.
//!
//! Provenance: our own test, standard picking geometry.

use camcraft_app::geometry::{Point3, cube};
use camcraft_app::pick::sight_blocked;

use crate::vertex_pick::{nearest_vertex, sight_origin};
use crate::viewport::{Camera, ProjectionMode};

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

/// A 4 mm block centred at `c`: the occluder of the orthographic test.
fn block(c: [f64; 3]) -> Vec<camcraft_app::geometry::Triangle> {
    cube(Point3::new(c[0] - 2.0, c[1] - 2.0, c[2] - 2.0), Point3::new(c[0] + 2.0, c[1] + 2.0, c[2] + 2.0)).facets.to_vec()
}

#[test]
fn orthographic_visibility_runs_along_the_view_direction_not_to_the_eye() {
    // #610: two corners 30 mm off the view axis. A block sits on the
    // first corner's parallel view ray (hidden in ortho) but off its
    // line to the eye; another sits on the second corner's line to the
    // eye but off its parallel ray (visible in ortho). The eye-point
    // test reads both backwards; the parallel one reads them as the
    // screen shows them. Perspective keeps the eye-point segment.
    let mut cam = diagonal_cam();
    cam.projection = ProjectionMode::Orthographic;
    let (r, _, f) = cam.view_basis();
    let eye = cam.eye();
    let at = |base: [f64; 3], k: f64, d: [f64; 3]| [base[0] + k * d[0], base[1] + k * d[1], base[2] + k * d[2]];
    let (r, f) = ([r.0, r.1, r.2], [f.0, f.1, f.2]);
    let hidden_corner = at([0.0; 3], 30.0, r);
    let shown_corner = at([0.0; 3], -30.0, r);
    let mut tris = block(at(hidden_corner, -20.0, f)); // 20 mm up its view ray
    let to_eye = [eye[0] - shown_corner[0], eye[1] - shown_corner[1], eye[2] - shown_corner[2]];
    tris.extend(block(at(shown_corner, 0.2, to_eye))); // 20 mm up its eye line
    let pts = [hidden_corner, shown_corner];
    let ortho_visible = |i: usize| sight_origin(&cam, pts[i]).is_some_and(|o| !sight_blocked(&tris, o, pts[i], 1e-3));
    assert!(!ortho_visible(0), "a corner behind a block along the view direction is hidden in ortho");
    assert!(ortho_visible(1), "a corner with a clear parallel view ray is visible in ortho");
    // The old eye-point law reads both the other way round - the bug.
    assert!(!sight_blocked(&tris, eye, pts[0], 1e-3) && sight_blocked(&tris, eye, pts[1], 1e-3));

    // Through the pick itself: the cursor on the hidden corner yields
    // nothing; on the shown one, that corner.
    let mvp = cam.mvp(1.0);
    let px = |q: [f64; 3]| {
        let uv = crate::interact::project_uv(&mvp, q).expect("on screen");
        [uv[0] * 400.0, uv[1] * 400.0]
    };
    assert_eq!(nearest_vertex(&pts, &mvp, RECT, px(hidden_corner), 8.0, ortho_visible), None);
    assert_eq!(nearest_vertex(&pts, &mvp, RECT, px(shown_corner), 8.0, ortho_visible), Some(1));

    // Perspective: the occlusion ray still starts at the eye.
    cam.projection = ProjectionMode::Perspective;
    for q in pts {
        assert_eq!(sight_origin(&cam, q), Some(cam.eye()));
    }
    // A point behind the ortho eye plane has nothing on screen to test.
    cam.projection = ProjectionMode::Orthographic;
    assert_eq!(sight_origin(&cam, at(eye, -1.0, f)), None);
}
