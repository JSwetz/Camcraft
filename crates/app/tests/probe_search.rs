//! #570, found on a real Tormach 770MX: a probe op searched 5 mm past the
//! expected face by default (deep enough to bottom a stylus when the face
//! is missing) and rapided down to the approach point over the part. Now:
//! a new op searches `max(2 x tolerance, 1 mm)`, a search past 2 mm is
//! named by a non-blocking advisory, and below the safe plane the probe
//! only ever feeds.
//!
//! Gate map (#570): covers app::operation::probe_op (default_overtravel,
//! search_advisory), app::operation::kinds (seed_probe), motion::probing
//! (probe_cycle's opening).
//! Provenance: touch-probe practice (protected positioning, rated stylus
//! overtravel); our own fixtures.

use camcraft_app::cl::MoveKind;
use camcraft_app::operation::probe_op::{SEARCH_LIMIT_MM, default_overtravel, search_advisory};
use camcraft_app::operation::{self, Height, HeightReference, OpParams, OpType};
use camcraft_app::probing::{ProbeDir, ProbeMode, ProbeParams, probe_cycle};
use camcraft_app::geometry::Point3;
use camcraft_app::verify::Warning;

fn params(mode: ProbeMode, overtravel: f64) -> ProbeParams {
    ProbeParams {
        mode,
        dir: ProbeDir::ZNeg,
        dir_y: ProbeDir::YPos,
        target: Point3::new(0.0, 0.0, 10.0),
        radius: 10.0,
        ball_diameter: 4.0,
        approach: 5.0,
        overtravel,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        clearance_z: 50.0,
    }
}

#[test]
fn a_new_probe_op_searches_a_short_way_past_the_face() {
    assert_eq!(default_overtravel(0.5), 1.0);
    assert_eq!(default_overtravel(0.1), 1.0, "never under 1 mm");
    assert_eq!(default_overtravel(0.8), 1.6, "twice a looser tolerance");
    // What the Create ribbon / Type combo seeds: a fresh Probe draft from
    // an op whose own probe distance was the old 5 mm - the seed never
    // carries it over.
    let old = OpParams::Probe {
        clearance_z: Height::of(HeightReference::TopOfStock, 15.0),
        mode: ProbeMode::Face,
        dir: ProbeDir::ZNeg,
        dir_y: ProbeDir::YPos,
        target: [0.0; 3],
        radius: 10.0,
        approach: 5.0,
        overtravel: 5.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        set_wcs: Some(0),
        tolerance: 0.5,
    };
    let seeded = operation::retype_params(&old, OpType::Probe.ordinal());
    let OpParams::Probe { overtravel, tolerance, .. } = seeded else {
        panic!("a probe seed: {seeded:?}");
    };
    assert_eq!(overtravel, default_overtravel(tolerance), "the seed follows its tolerance");
    assert!(overtravel <= SEARCH_LIMIT_MM, "a new op draws no advisory");
}

#[test]
fn a_deep_search_is_named_and_never_holds_the_post() {
    let path = probe_cycle(&params(ProbeMode::Face, 5.0)).expect("cycle");
    let w = search_advisory(&path, 5.0).expect("5 mm past the face is named");
    assert!(matches!(w, Warning::ProbeSearch { mm, limit_mm, .. } if mm == 5.0 && limit_mm == SEARCH_LIMIT_MM));
    assert!(!w.blocks_post(), "advisory: an old document still posts");
    assert!(w.to_string().contains("probe distance 5.000 mm"), "{w}");
    let short = probe_cycle(&params(ProbeMode::Face, SEARCH_LIMIT_MM)).expect("cycle");
    assert_eq!(search_advisory(&short, SEARCH_LIMIT_MM), None, "at the limit: silent");
}

#[test]
fn below_the_safe_plane_a_probe_only_feeds() {
    for mode in [ProbeMode::Face, ProbeMode::Edge, ProbeMode::Corner, ProbeMode::Bore, ProbeMode::Boss] {
        let mut p = params(mode, 1.0);
        if mode != ProbeMode::Face {
            p.dir = ProbeDir::XPos; // an edge or corner is touched along X / Y
        }
        let path = probe_cycle(&p).expect("cycle");
        // The path states tips; the safe plane is a tip height.
        let safe = p.clearance_z - 1e-9;
        let first = path.moves.first().expect("a move");
        assert_eq!(first.kind, MoveKind::Rapid, "{mode:?}: opens with the positioning rapid");
        assert!(first.to.z >= safe, "{mode:?}: positioning ends at the safe plane, not {}", first.to.z);
        for (i, m) in path.moves.iter().enumerate() {
            if m.kind == MoveKind::Rapid {
                assert!(
                    m.from.z >= safe && m.to.z >= safe,
                    "{mode:?}: move {i} rapids below the safe plane ({} -> {})",
                    m.from.z,
                    m.to.z
                );
            }
        }
    }
}
