//! M5.1 stage C generator law tests: the 5-axis contour and parallel
//! kinds cut the SAME ball-center ladder their stage A/B siblings cut
//! (a sphere's envelope is rotation-invariant) and add the pose.
//! The exact angle identities of the pose law are settled at the
//! stamp (`multiaxis::stamp`'s unit tests, one clean facet); what is
//! tested HERE is the composition: positions bit-for-bit unchanged,
//! rapids upright, every stated pose inside its cone (with the dome
//! mesh's own angular discretization budgeted), and the refusals.
//!
//! Gate map (M4.4): covers camcraft_motion::cl, camcraft_motion::finishing3d,
//! camcraft_motion::geometry, camcraft_motion::machining2d, camcraft_motion::multiaxis.
//! Lane shape (M4.4): all tests here run in seconds; envelope
//! verdicts over these paths ride the app's ignored lane.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::finishing3d::{Finish3DParams, FinishPattern, waterline};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;
use camcraft_motion::multiaxis::{AxisControl, contour5d, parallel5d};

const TAU: f64 = std::f64::consts::TAU;
const R: f64 = 3.0;
const TOL: f64 = 0.05;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn unit(v: Point3) -> Point3 {
    let l = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    p3(v.x / l, v.y / l, v.z / l)
}

fn angle(a: Point3, b: Point3) -> f64 {
    let cx = a.cross(b);
    let n = (cx.x * cx.x + cx.y * cx.y + cx.z * cx.z).sqrt();
    n.atan2(a.dot(b).clamp(-1.0, 1.0))
}

fn finish(top_z: f64, bottom_z: f64) -> Finish3DParams {
    Finish3DParams {
        pattern: FinishPattern::Waterline,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: TOL,
        clearance_z: 20.0,
        retract_z: None,
        top_z,
        bottom_z,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        // M8.2 B: the ring entry keeps its plunge unless asked.
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
    }
}

/// A spherical cap centered at the origin: every surface point q
/// satisfies |q| = radius, so the radial direction is the test's
/// independent truth for the contact normal, off by at most the
/// mesh's band angle (the facets stand in for the sphere).
fn dome(radius: f64, polar_max: f64, n_lat: usize, n_lon: usize) -> Vec<Triangle> {
    let mut facets = Vec::new();
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = TAU * j as f64 / n_lon as f64;
                p3(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let apex = p3(0.0, 0.0, radius);
    let first = ring(1);
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
    let mut prev = first;
    for k in 2..=n_lat {
        let cur = ring(k);
        for j in 0..n_lon {
            facets.push(Triangle::new(prev[j], cur[j], cur[(j + 1) % n_lon]));
            facets.push(Triangle::new(prev[j], cur[(j + 1) % n_lon], prev[(j + 1) % n_lon]));
        }
        prev = cur;
    }
    facets
}

fn control(lead: f64, lag: f64, clamp: Option<f64>) -> AxisControl {
    AxisControl {
        lead_deg: lead,
        lag_deg: lag,
        max_tilt_deg: clamp,
    }
}

/// Every axis a cut move states, with its move's END position.
fn cut_axes(path: &camcraft_motion::cl::Toolpath) -> Vec<(Point3, Point3)> {
    path.moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .filter_map(|m| m.axis.map(|a| (m.to, p3(a[0], a[1], a[2]))))
        .collect()
}

/// The angular slack a coarse cap mesh buys: a facet's normal stands
/// off the radial by at most the longitude wedge's half-angle.
const MESH_BAND: f64 = 0.22; // rad, generous for 16 longitudes at this cap

#[test]
fn contour5d_positions_are_the_waterlines_bit_for_bit() {
    let facets = dome(15.0, 0.9, 8, 16);
    let base = waterline(&facets, &finish(12.0, 9.8)).unwrap();
    let got = contour5d(&facets, &finish(12.0, 9.8), &control(20.0, 0.0, Some(45.0)), None).unwrap();
    assert_eq!(got.moves.len(), base.moves.len());
    for (g, b) in got.moves.iter().zip(&base.moves) {
        assert_eq!((g.from.x, g.from.y, g.from.z), (b.from.x, b.from.y, b.from.z));
        assert_eq!((g.to.x, g.to.y, g.to.z), (b.to.x, b.to.y, b.to.z));
        assert_eq!(g.kind, b.kind);
        assert_eq!(g.feed, b.feed);
        assert_eq!(g.arc.is_some(), b.arc.is_some());
    }
    assert!(!got.is_prismatic(), "a dome's rings must lean somewhere");
    assert!(got.is_chained());
}

#[test]
fn rapids_hold_no_angle() {
    let facets = dome(15.0, 0.9, 8, 16);
    let got = contour5d(&facets, &finish(12.0, 9.8), &control(30.0, 0.0, None), None).unwrap();
    assert!(
        got.moves.iter().all(|m| m.kind != MoveKind::Rapid || m.axis.is_none()),
        "a rapid stated a tool angle"
    );
    assert!(
        !cut_axes(&got).is_empty(),
        "a ring ladder stated no pose anywhere"
    );
}

#[test]
fn zero_lean_holds_the_contact_normal_radial_to_the_budget() {
    // Identity at generator scale: with no lean asked, every pose
    // points OUT of the dome (and off radial by no more than the
    // mesh's own band angle).
    // One ring at cz 12: its tangent band (q_z = cz·R/(R+r) = 10)
    // lies inside the cap (rim at z 9.32), so every contact normal
    // is a face normal of the cap - radial to the mesh band. A ring
    // cut below the rim would ride the cap's boundary EDGE, whose
    // contact normals are not radial; that is no one's law, so the
    // fixture keeps every ring on the face.
    let facets = dome(15.0, 0.9, 8, 16);
    let got = contour5d(&facets, &finish(12.0, 11.9), &control(0.0, 0.0, Some(90.0)), None).unwrap();
    let axes = cut_axes(&got);
    assert!(axes.len() > 10, "only {} poses on a whole ladder", axes.len());
    for (c, a) in axes {
        assert!(c.dot(a) > 0.0, "an axis leaned into the dome at {c:?}: {a:?}");
        assert!(
            angle(unit(c), a) <= MESH_BAND,
            "axis {a:?} at {c:?} is off radial by more than the mesh band"
        );
    }
}

#[test]
fn the_cone_holds_at_generator_scale() {
    // Lead 75 / lag 30 against a 40-degree clamp, on the dome: every
    // pose the path states sits inside the clamp cone of its contact
    // normal - which here is radial (both rings ride the cap's face,
    // above its rim), plus the mesh band.
    let facets = dome(15.0, 0.9, 8, 16);
    let got = contour5d(&facets, &finish(12.0, 11.4), &control(75.0, 30.0, Some(40.0)), None).unwrap();
    let axes = cut_axes(&got);
    assert!(!axes.is_empty());
    for (c, a) in axes {
        assert!(
            angle(unit(c), a) <= 40f64.to_radians() + MESH_BAND + 1e-9,
            "axis {a:?} at {c:?} broke the 40-degree cone past the mesh band"
        );
    }
}

#[test]
fn parallel5d_poses_a_followed_raster() {
    // The raster's choreography through the same stamp: positions
    // equal the plain parallel path, and the lines lean.
    let facets = dome(15.0, 1.1, 8, 16);
    let p = finish(12.0, 6.0);
    let base = camcraft_motion::finishing3d::parallel(&facets, &p).unwrap();
    let got = parallel5d(&facets, &p, &control(15.0, 10.0, Some(60.0)), None).unwrap();
    assert_eq!(got.moves.len(), base.moves.len());
    for (g, b) in got.moves.iter().zip(&base.moves) {
        assert_eq!(g.to, b.to);
        assert_eq!(g.kind, b.kind);
    }
    let axes = cut_axes(&got);
    assert!(!axes.is_empty(), "a raster over a dome stated no pose");
    for (c, a) in axes {
        assert!(
            angle(unit(c), a) <= 60f64.to_radians() + MESH_BAND + 1e-9,
            "raster axis {a:?} at {c:?} broke its cone"
        );
    }
}

#[test]
fn a_broken_control_holds_vertical_never_garbage() {
    // The generator's backstop (the app's sane door refuses the
    // param first): a NaN lead states no direction, and no move may
    // carry a guessed one - the path stays prismatic.
    let facets = dome(15.0, 0.9, 8, 16);
    let got = contour5d(&facets, &finish(12.0, 9.8), &control(f64::NAN, 0.0, None), None).unwrap();
    assert!(got.is_prismatic());
    assert!(got.all_finite());
}

#[test]
fn an_empty_model_refuses_like_the_waterline() {
    // The wrapper adds no generation of its own, so the waterline's
    // own refusal is the refusal - by its own name.
    let e = contour5d(&[], &finish(1.0, 0.0), &control(0.0, 0.0, None), None).err();
    assert!(matches!(e, Some(GenError::Empty(_))), "got {e:?}");
}
