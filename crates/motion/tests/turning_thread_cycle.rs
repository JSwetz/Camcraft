//! The thread kernel's cycle record (issue #34): beside its explicit
//! passes the thread path carries one `ThreadCycle` stating the same
//! thread - start on the first centre at the clearance radius, end at
//! the root on the last centre, the ladder's rung as first and minimum
//! depth, radial infeed - and the passes it spans are exactly the
//! passes the sim and verify read.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (thread kernel's
//! attach_cycle), camcraft_cl::cl::ThreadCycle.
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::{thread, ThreadParams};

const TAU: f64 = std::f64::consts::TAU;
const MAJOR: f64 = 6.0;
const DEPTH: f64 = 1.2;

/// The motion threading suite's bar: a 60-degree V every 2.0 from
/// z 8 to z 14 on a wall of r 6, 16 ways round.
fn threaded() -> Vec<Triangle> {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    let mut m = vec![(0.0, -16.0), (MAJOR, -16.0), (MAJOR, 8.0 - half)];
    let mut c = 8.0;
    while c <= 14.0 + 1e-9 {
        m.push((MAJOR - DEPTH, c));
        m.push((MAJOR, c + half));
        if c + 2.0 <= 14.0 + 1e-9 {
            m.push((MAJOR, c + 2.0 - half));
        }
        c += 2.0;
    }
    m.extend([(MAJOR, 14.0 + half), (MAJOR, 40.0), (0.0, 40.0)]);
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for seg in m.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
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

#[test]
fn the_thread_path_carries_its_cycle_record() {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    let p = ThreadParams {
        tolerance: 0.05,
        clearance_z: 24.0,
        top_z: 14.3,
        bottom_z: 7.7,
        pitch: 2.0,
        depth: DEPTH,
        infeed: 0.4,
        feed: 0.0,
        plunge_feed: 40.0,
        insert_width: 2.0 * half,
        form_angle: 60.0,
    };
    let path = thread(&threaded(), &p).expect("the thread cuts");
    assert_eq!(path.threads.len(), 1, "one record: {:?}", path.notes);
    let t = path.threads[0];
    assert!(t.is_sane(&path.moves));
    let rho = |q: Point3| (q.x * q.x + q.y * q.y).sqrt();
    assert!((t.start.z - 8.0).abs() < 1e-9 && (t.end.z - 14.0).abs() < 1e-9, "{t:?}");
    assert!((rho(t.end) - (MAJOR - DEPTH)).abs() < 1e-9, "the end stands at the root: {t:?}");
    assert!(rho(t.start) > MAJOR, "the start clears the crest: {t:?}");
    assert!((t.major_diameter - 2.0 * MAJOR).abs() < 1e-9 && (t.minor_diameter - 2.0 * (MAJOR - DEPTH)).abs() < 1e-9);
    assert!((t.lead - 2.0).abs() < 1e-12 && (t.height - DEPTH).abs() < 1e-9);
    assert!((t.first_depth - 0.4).abs() < 1e-9 && (t.min_depth - 0.4).abs() < 1e-9);
    assert_eq!((t.finish_allowance, t.infeed_angle, t.spring_passes, t.pull_out), (0.0, 0.0, 0, 0.0));
    // The span is every pass: the moves before it and after it are the
    // approach and the out-rapid, and every arc lies inside it.
    assert!(path.moves[..t.first].iter().all(|m| m.kind == MoveKind::Rapid));
    assert!(path.moves[t.last + 1..].iter().all(|m| m.kind == MoveKind::Rapid));
    let arcs_in = path.moves[t.first..=t.last].iter().filter(|m| m.arc.is_some()).count();
    assert_eq!(arcs_in, path.arc_count(), "every half-turn inside the span");
    assert_eq!(path.moves[t.last].kind, MoveKind::Rapid, "the span ends on the retract");
}
