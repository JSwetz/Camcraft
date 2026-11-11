//! M5.2 stage D threading law tests: the helical wave verdict and
//! the kernel that earns silence from it. What "right" means here:
//! a full-form pass over a modeled thread leaves the judge NOTHING
//! to say - flanks by coincidence, crest flats by their own shell
//! (rest zero), between-footprint air by the window law; a lattice
//! out of phase with the model is convicted; the ladder's last
//! rung stands exactly at the root, the chain lands every half
//! pitch and crosses the axis every half turn; and every door
//! names its number - the valley that is not there, the insert
//! that cannot reach or cannot fit, the bar that is not plumb.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (thread kernel,
//! thread_warnings, the wave clause of revolve::scan).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{thread, thread_warnings, ThreadParams};
use camcraft_motion::verify::Warning;

const TAU: f64 = std::f64::consts::TAU;
const TOL: f64 = 0.05;
const MAJOR: f64 = 6.0;
const PITCH: f64 = 2.0;
const DEPTH: f64 = 1.2;
const ROOT: f64 = MAJOR - DEPTH;
/// A 60-degree full-form V: half-width = depth * tan(30).
const TAN30: f64 = 0.577_350_269_189_625_8;
const HALF: f64 = DEPTH * TAN30;
const WIDTH: f64 = 2.0 * HALF;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The closed-solid revolve the turning suites share.
fn revolve(n_circ: usize, meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / n_circ as f64;
        p3(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..n_circ {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                t.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                t.push(Triangle::new(a1, a2, b2));
            } else {
                t.push(Triangle::new(a1, a2, b2));
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    t
}

/// The threaded bar: wall r 6 to z 7.307, a 60-degree V every 2.0
/// from z 8 to z 14, crest to z 20, capped. Valleys at stations
/// 8, 10, 12 and 14 - the model a threading kernel is asked to
/// match, crest flats 0.614 wide between them.
fn threaded() -> Vec<Triangle> {
    thread_at(8.0, PITCH, DEPTH)
}

/// The same idea with the valley train's PHASE as chosen (and its
/// pitch, for the drift fixtures).
fn thread_at(phase: f64, pitch: f64, depth: f64) -> Vec<Triangle> {
    let half = depth * (30.0_f64).to_radians().tan();
    let root = MAJOR - depth;
    let mut m = vec![(0.0, -16.0), (MAJOR, -16.0), (MAJOR, phase - half)];
    let mut c = phase;
    while c <= 14.0 + 1e-9 {
        m.push((root, c));
        m.push((MAJOR, c + half));
        // The crest bridge ends where the NEXT valley's flank
        // starts; the tail's last point is written after the loop.
        if c + pitch <= 14.0 + 1e-9 {
            m.push((MAJOR, c + pitch - half));
        }
        c += pitch;
    }
    m.push((MAJOR, 14.0 + half));
    // The bar runs past the thread train to a length the axis-fit
    // second moment reads as a revolve, not a dome (the blade
    // suites' proportion).
    m.push((MAJOR, 40.0));
    m.push((0.0, 40.0));
    revolve(16, &m)
}

/// A plain wall: the threadless bar every thread door must refuse.
fn wall() -> Vec<Triangle> {
    revolve(16, &[(0.0, -16.0), (6.0, -16.0), (6.0, 40.0), (0.0, 40.0)])
}

fn th(top_z: f64, bottom_z: f64) -> ThreadParams {
    ThreadParams {
        tolerance: TOL,
        clearance_z: 24.0,
        top_z,
        bottom_z,
        pitch: PITCH,
        depth: DEPTH,
        infeed: 0.4,
        feed: 0.0,
        plunge_feed: 40.0,
        insert_width: WIDTH,
        form_angle: 60.0,
        hand: camcraft_motion::cl::ThreadHand::Right,
    }
}

/// The kernel's own thread over the full valley train.
fn run(p: &ThreadParams) -> Toolpath {
    thread(&threaded(), p).expect("the thread cuts")
}

fn sr(f: &Frame, q: Point3) -> (f64, f64) {
    f.sr(q)
}

/// Which side of the axis a CL point stands on, along the frame's
/// own radial (the meridian the whole suite lives in).
fn side(f: &Frame, q: Point3) -> f64 {
    let r = f.point(1.0, 0.0) - f.point(0.0, 0.0);
    let a = f.point(0.0, sr(f, q).0);
    (q.x - a.x) * r.x + (q.y - a.y) * r.y + (q.z - a.z) * r.z
}

// ------------------------------------------------------- the verdict

#[test]
fn a_full_form_thread_leaves_the_judge_silent() {
    // The calibration test, blade-suite law: the kernel's own path
    // judged against the very model it was computed from. Flanks
    // coincide, crest flats rest zero against their own shell,
    // and no station the passes were meant to reach stands uncut.
    let t = run(&th(14.3, 7.7));
    let warns = thread_warnings(&threaded(), &t, WIDTH, 60.0, TOL);
    assert!(warns.is_empty(), "a flush thread reported: {warns:?}");
}

#[test]
fn a_quarter_pitch_late_is_convicted() {
    // The kernel reads the model's OWN valleys, so phase error
    // enters the world as a path judged against a thread shifted
    // by p/4: every V rides one flank's air and the crest's
    // steel. The gouge clause must convict at once.
    let t = run(&th(14.3, 7.7));
    let warns = thread_warnings(&thread_at(8.5, PITCH, DEPTH), &t, WIDTH, 60.0, TOL);
    match warns.as_slice() {
        [Warning::Envelope { gouge_mm, .. }] => {
            assert!(*gouge_mm > TOL, "a p/4 miss judged as clean: {warns:?}");
        }
        other => panic!("a phase miss should convict, got {other:?}"),
    }
}

#[test]
fn a_pitch_that_drifts_is_convicted() {
    // Same law read the other way: right phase, wrong pitch - the
    // lattice leaves the valley train behind, and by the far end
    // stands on the flanks outright.
    let mut p = th(14.3, 7.7);
    p.pitch = PITCH * 1.1;
    let t = run(&p);
    let warns = thread_warnings(&threaded(), &t, WIDTH, 60.0, TOL);
    assert!(!warns.is_empty(), "a 10% pitch drift judged silent");
}

#[test]
fn a_shallow_root_is_uncovered() {
    let mut p = th(14.3, 7.7);
    p.depth = DEPTH - 0.5;
    let t = run(&p);
    let warns = thread_warnings(&threaded(), &t, WIDTH, 60.0, TOL);
    match warns.as_slice() {
        [Warning::Envelope { gouge_mm, rest_mm, .. }] => {
            assert!(*rest_mm >= 0.4 && *gouge_mm <= TOL + 1e-6, "a shallow cut mis-said: {warns:?}");
        }
        other => panic!("a shallow thread should rest, got {other:?}"),
    }
}

#[test]
fn too_deep_is_a_gouge() {
    // The phase belongs to the model, the depth to the numbers:
    // an overcut thread GENERATES (the valley train is found by
    // the model's own floor) and the verdict convicts every flank
    // it rode 0.3 inside. Steel ground for the deeper form (same
    // 60 degrees - a parallel-flank overcut; a 0.5 overcut would
    // want a V wider than the pitch admits, and the fit door
    // says so first).
    let mut p = th(14.3, 7.7);
    p.depth = DEPTH + 0.3;
    p.insert_width = 2.0 * p.depth * (30.0_f64).to_radians().tan();
    let t = thread(&threaded(), &p).expect("the overcut threads");
    let warns = thread_warnings(&threaded(), &t, p.insert_width, 60.0, TOL);
    match warns.as_slice() {
        [Warning::Envelope { gouge_mm, .. }] => {
            assert!(*gouge_mm >= 0.25, "a 0.3 overcut understated: {warns:?}");
        }
        other => panic!("a deep thread should gouge, got {other:?}"),
    }
}

#[test]
fn a_missed_turn_is_charged_the_window() {
    // The over-report law's threading face: judge a thread cut at
    // half the lattice density (pitch doubled on the path, model
    // unchanged) - whole model valleys stand uncalled-for, and the
    // verdict's silence is never bought that way.
    let mut p = th(14.3, 7.7);
    p.pitch = PITCH * 2.0;
    let t = run(&p);
    let warns = thread_warnings(&threaded(), &t, WIDTH, 60.0, TOL);
    assert!(!warns.is_empty(), "a thread every other valley judged silent");
}

// ------------------------------------------------------- the path law

#[test]
fn the_ladder_ends_at_the_root_and_the_arcs_land_every_half_pitch() {
    // Four valleys, 1.2 deep at 0.4 infeed: three passes, each a
    // plunge plus six half-turns; every second endpoint returns
    // to the frame's meridian one pitch on, the depth constant
    // within a pass, the last rung the root exactly.
    let t = run(&th(14.3, 7.7));
    let f = Frame::about(&threaded()).expect("plumb");
    let feeds: Vec<_> = t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).collect();
    assert_eq!(feeds.len(), 3 * 7, "three passes of plunge + six arcs: {}", feeds.len());
    assert_eq!(t.arc_count(), 18);
    assert!(feeds[0].arc.is_none(), "a pass opens with the radial plunge");
    let (s0, r0) = sr(&f, feeds[0].to);
    // #629: toward the chuck - the pass opens on the TOP centre.
    assert!((s0 - 14.0).abs() < 1e-9 && (r0 - (MAJOR - 0.4)).abs() < 1e-9, "first rung: {r0} at {s0}");
    for m in &feeds[1..=6] {
        assert!(m.arc.is_some(), "the threading chain is arcs");
        let (s, r) = sr(&f, m.to);
        assert!((r - (MAJOR - 0.4)).abs() < 1e-9, "constant depth through a pass: {r}");
        let _ = s;
    }
    for w in feeds[1..=6].windows(2) {
        let (sa, _) = sr(&f, w[0].to);
        let (sb, _) = sr(&f, w[1].to);
        assert!((sa - sb - PITCH * 0.5).abs() < 1e-6, "the half-pitch walk down: {sb} after {sa}");
    }
    // The final feed move is the last pass's last arc, at the root.
    let (s_end, r_end) = sr(&f, feeds[feeds.len() - 1].to);
    assert!((r_end - ROOT).abs() < 1e-9, "the last rung is the root: {r_end}");
    assert!((s_end - 8.0).abs() < 1e-6, "the run ends on the bottom center: {s_end}");
    assert!(t.is_chained(), "arcs and all, the chain holds");
}

#[test]
fn the_sides_alternate_about_the_axis() {
    // A half-turn lands across the spindle from where it stood:
    // the XY-circle law that makes one turn exactly one pitch.
    let t = run(&th(14.3, 7.7));
    let f = Frame::about(&threaded()).expect("plumb");
    let feeds: Vec<_> = t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).collect();
    let mut prev = side(&f, feeds[0].to);
    for m in &feeds[1..=6] {
        let s = side(&f, m.to);
        assert!(s * prev < 0.0, "the helix must cross the axis every half turn");
        assert!((s.abs() - (MAJOR - 0.4)).abs() < 1e-9, "the side reading is the radius");
        prev = s;
    }
}

#[test]
fn a_one_valley_window_is_a_ring_groove() {
    // A window around a single valley (n = 0 turns): the pass is
    // a plunge, the cut is a ring V - and on that valley's model
    // it judges clean.
    let t = run(&th(8.75, 7.25));
    assert_eq!(t.arc_count(), 0, "one center has no turns to walk");
    let warns = thread_warnings(&threaded(), &t, WIDTH, 60.0, TOL);
    assert!(warns.is_empty(), "a ring groove on its valley: {warns:?}");
}

#[test]
fn rapids_clear_the_whole_bar() {
    let t = run(&th(14.3, 7.7));
    let f = Frame::about(&threaded()).expect("plumb");
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
        assert!(sr(&f, m.to).1 >= MAJOR + TOL - 1e-9, "a low rapid: {:?}", m.to);
    }
}

// ------------------------------------------------------------ doors

#[test]
fn a_smooth_bar_has_no_valley_and_says_so() {
    let e = thread(&wall(), &th(14.3, 7.7)).expect_err("wall threads");
    let m = format!("{e:?}");
    assert!(m.contains("nothing to thread") && m.contains("MODELED"), "{m}");
}

#[test]
fn an_insert_that_cannot_reach_the_crest_is_refused_by_name() {
    let mut p = th(14.3, 7.7);
    p.insert_width = WIDTH * 0.5;
    let e = thread(&threaded(), &p).expect_err("narrow insert");
    let m = format!("{e:?}");
    assert!(m.contains("crest") && m.contains("wide"), "{m}");
}

#[test]
fn an_insert_wider_than_the_pitch_is_refused() {
    // The V would stand in the neighbouring groove's steel.
    let mut p = th(14.3, 7.7);
    p.insert_width = PITCH * 1.5;
    let e = thread(&threaded(), &p).expect_err("oversized insert");
    let m = format!("{e:?}");
    assert!(m.contains("pitch") && m.contains("wider"), "{m}");
}

#[test]
fn a_thread_past_the_axis_is_refused() {
    let mut p = th(14.3, 7.7);
    p.depth = MAJOR + 0.5;
    p.insert_width = 2.0 * p.depth * (30.0_f64).to_radians().tan();
    let e = thread(&threaded(), &p).expect_err("thread the whole bar");
    assert!(format!("{e:?}").contains("past the axis"), "{e:?}");
}

#[test]
fn nonsense_numbers_are_refused() {
    let mut p = th(14.3, 7.7);
    p.pitch = 0.0;
    assert!(thread(&threaded(), &p).is_err());
    p.pitch = PITCH;
    p.form_angle = 190.0;
    assert!(thread(&threaded(), &p).is_err());
    p.form_angle = 60.0;
    p.infeed = -1.0;
    assert!(thread(&threaded(), &p).is_err());
    p.infeed = 0.4;
    p.depth = 0.01;
    assert!(thread(&threaded(), &p).is_err(), "a thread thinner than its tolerance has no valley to hold");
    let mut q = th(f64::NAN, 7.7);
    q.form_angle = 60.0;
    assert!(thread(&threaded(), &q).is_err(), "a height that is not a number");
}

#[test]
fn a_tilted_bar_is_mill_turn_work() {
    // Lean the whole fixture 2 degrees about X: the fitted axis
    // still passes the turning door, but the XY circles square to
    // the spindle - the pass that makes one turn one pitch - are
    // no longer a thing.
    let th2 = 2.0_f64.to_radians();
    let (c, s) = (th2.cos(), th2.sin());
    let rot = |p: Point3| p3(p.x, p.y * c - p.z * s, p.y * s + p.z * c);
    let tilted: Vec<Triangle> = threaded()
        .iter()
        .map(|t| Triangle::new(rot(t.a), rot(t.b), rot(t.c)))
        .collect();
    let e = thread(&tilted, &th(14.3, 7.7)).expect_err("tilted thread");
    let m = format!("{e:?}");
    assert!(m.contains("mill-turn") && m.contains("M5.2 G"), "{m}");
}

#[test]
fn the_flat_blade_kinds_are_unaffected_by_the_wave_clause() {
    // The generalization's promise to stage C: BandMove.wave is
    // None for blades, floor_at adds nothing, and the blade suite
    // (turning_thread's sibling) keeps its exact verdicts. Here:
    // a zero-degree form has no flanks at all - the door says so.
    let mut p = th(14.3, 7.7);
    p.form_angle = 0.0;
    assert!(thread(&threaded(), &p).is_err());
    p.form_angle = 60.0;
    let t = run(&p);
    let warns = thread_warnings(&threaded(), &t, WIDTH, 60.0, TOL);
    assert!(warns.is_empty());
    // A degenerate verdict ask - form angle no V - judges noise:
    // the gouge-at-the-first-cut law, never silence.
    let w = thread_warnings(&threaded(), &t, WIDTH, 0.0, TOL);
    assert!(!w.is_empty(), "a judge that cannot measure");
}

#[test]
fn coarse_infeed_still_reaches_the_root() {
    // One pass, straight to the root (the rung snaps there, HAIR
    // law shared with the blades): the deepest wave alone tiles
    // the model - and the path holds exactly one pass.
    let mut p = th(14.3, 7.7);
    p.infeed = 10.2;
    let t = run(&p);
    let feeds: Vec<_> = t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).collect();
    assert_eq!(feeds.len(), 7, "one pass: plunge + six arcs, got {}", feeds.len());
    let f = Frame::about(&threaded()).expect("plumb");
    for m in &feeds[1..] {
        assert!((sr(&f, m.to).1 - ROOT).abs() < 1e-9, "the single rung is the root");
    }
    let warns = thread_warnings(&threaded(), &t, WIDTH, 60.0, TOL);
    assert!(warns.is_empty(), "the one-pass thread: {warns:?}");
}
