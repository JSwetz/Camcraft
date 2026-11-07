//! Issue #54: the rotary wrap on a bar that is not a pure revolve - a
//! D-flat (a planar flat ground part of the way round the bar). Specced
//! from the wrap/development construction in `multiaxis::rotary_offset`
//! (a closed solid develops to a sheet whose only free edges are the
//! seam's two copies) and plain solid-modelling topology (a closed
//! 2-manifold has no free edge); no vendor material consulted.
//!
//! Read here: a D-flat bar wraps in every placement (flat through the
//! ends, a closed flat beside the seam, a closed flat across it) and its
//! Offset-rings rough is cut; a bar with a facet missing is still
//! refused by name; and issue #9's seam law end to end - under Adaptive,
//! a developed level whose centre component reaches a cover end short of
//! a whole lap (the flat's channel, at a radius only the flat reaches)
//! is refused by name and cut by Offset rings.
//!
//! Gate map (M4.4): covers camcraft_motion::multiaxis (rotary_offset,
//! rotary_seam) and camcraft_motion::machining3d (hsm's seam law), on
//! this suite's own prism fixtures.
//! Lane shape (M4.4): the rings wraps run in seconds in debug; the
//! Adaptive seam case plans one window of levels.

use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::cl::MoveKind;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams};
use camcraft_motion::multiaxis::{rotary_offset_developed, rotary_offset_in, AxisControl};

const TAU: f64 = std::f64::consts::TAU;
/// The head of the HSM planner's seam refusal (`hsm::SEAM_RINGS`).
const SEAM_RINGS: &str = "adaptive: the developed level crosses the wrap's seam";
const RAD: f64 = 6.0;

/// One cross-section ring of `n` points at station `z`: the circle of
/// radius RAD, or (with `flat`) the D its flat leaves - every point
/// past the flat's plane (distance `d` along the direction `phi`)
/// pushed back onto it.
fn ring(n: usize, z: f64, flat: Option<(f64, f64)>) -> Vec<Point3> {
    (0..n)
        .map(|j| {
            let th = TAU * j as f64 / n as f64;
            let (mut x, mut y) = (RAD * th.cos(), RAD * th.sin());
            if let Some((phi, d)) = flat {
                let (nx, ny) = (phi.cos(), phi.sin());
                let over = x * nx + y * ny - d;
                if over > 0.0 {
                    x -= over * nx;
                    y -= over * ny;
                }
            }
            Point3::new(x, y, z)
        })
        .collect()
}

fn same(a: Point3, b: Point3) -> bool {
    (a - b).dot(a - b) < 1e-24
}

fn tri(f: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3) {
    if !(same(a, b) || same(b, c) || same(c, a)) {
        f.push(Triangle::new(a, b, c));
    }
}

/// A closed prism through the rings in order (consecutive rings joined
/// by quads - a zero-length rung collapses its quad to the triangle it
/// is), capped by fans from the axis at both ends. Outward winding.
fn prism(rings: &[Vec<Point3>]) -> Vec<Triangle> {
    let mut f = Vec::new();
    for w in rings.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let n = a.len();
        for j in 0..n {
            let k = (j + 1) % n;
            tri(&mut f, a[j], a[k], b[k]);
            tri(&mut f, a[j], b[k], b[j]);
        }
    }
    let (Some(lo), Some(hi)) = (rings.first(), rings.last()) else { return f };
    let n = lo.len();
    let (c0, c1) = (Point3::new(0.0, 0.0, lo[0].z), Point3::new(0.0, 0.0, hi[0].z));
    for j in 0..n {
        let k = (j + 1) % n;
        tri(&mut f, c0, lo[k], lo[j]);
        tri(&mut f, c1, hi[j], hi[k]);
    }
    f
}

/// A bar of radius RAD over stations `z0..z1`, `n` round, with a D-flat
/// at depth `d` along `phi`: `None` for flat through both ends, else the
/// flat ground over the stations `lo..hi` only, round either side.
fn dflat_bar(n: usize, z0: f64, z1: f64, phi: f64, d: f64, span: Option<(f64, f64)>) -> Vec<Triangle> {
    let fl = Some((phi, d));
    match span {
        None => prism(&[ring(n, z0, fl), ring(n, z1, fl)]),
        Some((lo, hi)) => prism(&[
            ring(n, z0, None),
            ring(n, lo, None),
            ring(n, lo, fl),
            ring(n, hi, fl),
            ring(n, hi, None),
            ring(n, z1, None),
        ]),
    }
}

/// A coaxial round bar stock of radius `rho` over `z0..z1`.
fn stock(rho: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    prism(&[ring(64, z0, None), ring(64, z1, None)])
        .into_iter()
        .map(|t| {
            let s = |p: Point3| Point3::new(p.x * rho / RAD, p.y * rho / RAD, p.z);
            Triangle::new(s(t.a), s(t.b), s(t.c))
        })
        .collect()
}

/// The wrap kind's pocket numbers; `top_z`/`bottom_z` window the
/// stations (the axis stands on world Z).
fn params(strategy: PocketStrategy, d: f64, window: f64) -> Pocket3DParams {
    Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: d,
        stepover: 0.6 * d,
        stock_to_leave: 0.1,
        clearance_z: 50.0,
        top_z: window,
        bottom_z: -window,
        stepdown: 0.5,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: None,
        entry: Entry3D::Helix,
        helix_radius: 0.25 * d,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

fn control() -> AxisControl {
    AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: None }
}

/// Flat placements: +x, the two y half-axes (one of them lies on the
/// wrap's +-pi branch, so its flat crosses the seam), and an odd one.
const PHIS: [f64; 4] = [0.0, std::f64::consts::FRAC_PI_2, -std::f64::consts::FRAC_PI_2, 2.3];

/// The flat across the seam (the branch the wrap's basis puts at -y
/// for this fixture's fitted axis): the placement main refused.
const SEAM_PHI: f64 = -std::f64::consts::FRAC_PI_2;

fn feeds(t: &camcraft_motion::cl::Toolpath) -> impl Iterator<Item = &camcraft_motion::cl::Move> {
    t.moves.iter().filter(|m| m.kind == MoveKind::Feed)
}

/// Every placement of a D-flat wraps: a flat ground through both ends
/// (r5.8) and one closed over the middle stations (r5); the closed one's
/// channel is roughed under the round's skin. On main the flat across
/// the seam was refused as "the mesh is open" (4 dangling chord ends):
/// the cover-end wall stood in the extreme column's plane and met none
/// of the ragged seam column's edges.
#[test]
fn a_d_flat_bar_wraps_in_every_placement() {
    let st = stock(6.5, -20.0, 20.0);
    let p = params(PocketStrategy::OffsetRings, 2.0, 12.0);
    for (name, d, span) in [("through", 5.8, None), ("closed", 5.0, Some((-8.0, 8.0)))] {
        for phi in PHIS {
            let f = dflat_bar(64, -20.0, 20.0, phi, d, span);
            let t = match rotary_offset_in(&f, Some(&st), &p, &control()) {
                Ok(t) => t,
                Err(e) => panic!("{name} flat at phi {phi}: {e}"),
            };
            assert!(t.all_finite(), "{name} flat at phi {phi}: a non-finite move");
            let cuts = feeds(&t).count();
            assert!(cuts > 50, "{name} flat at phi {phi}: only {cuts} cut moves");
            // No cut tip stands inside the part: inside the round's skin
            // AND (over the flat's stations) behind the flat's plane, both
            // by more than a sag tolerance (the leave is 0.1).
            let (nx, ny) = (phi.cos(), phi.sin());
            for m in feeds(&t) {
                let q = m.to;
                let flat_here = span.is_none_or(|(lo, hi)| q.z >= lo && q.z <= hi);
                let behind = !flat_here || q.x * nx + q.y * ny < d - 0.05;
                assert!(
                    !(q.x.hypot(q.y) < RAD - 0.05 && behind),
                    "{name} flat at phi {phi}: a cut tip at {q:?} stands inside the part"
                );
            }
            let low = feeds(&t).map(|m| m.to.x.hypot(m.to.y)).fold(f64::INFINITY, f64::min);
            if span.is_some() {
                assert!(low < 5.6, "{name} flat at phi {phi}: the flat's channel is not roughed (lowest tip r{low:.3})");
            }
        }
    }
}

/// A closed mesh wraps; one with a facet missing from its skin is
/// refused by name before the wrap - even where no level of the ladder
/// would ever slice through the hole.
#[test]
fn a_bar_with_a_hole_in_its_skin_is_still_refused_by_name() {
    let mut f = dflat_bar(64, -20.0, 20.0, 0.0, 5.0, Some((-8.0, 8.0)));
    let Some(i) = f.iter().position(|t| {
        let (cx, cz) = ((t.a.x + t.b.x + t.c.x) / 3.0, (t.a.z + t.b.z + t.c.z) / 3.0);
        cx < -5.0 && cz > 10.0 && cz < 19.0
    }) else {
        panic!("no facet to drop")
    };
    f.remove(i);
    let p = params(PocketStrategy::OffsetRings, 2.0, 12.0);
    match rotary_offset_in(&f, Some(&stock(6.5, -20.0, 20.0)), &p, &control()) {
        Ok(_) => panic!("an open mesh wrapped"),
        Err(e) => {
            let msg = e.to_string();
            assert!(msg.contains("the mesh is open"), "refused, but not by name: {msg}");
        }
    }
}

/// Issue #9's seam law end to end: a long bar (so the axis fit reads
/// the round, not the flat) clamped at its own skin, a deep D-flat
/// across the seam over the windowed stations. The levels under the
/// skin find stock only in the flat's channel, and its centre component
/// reaches the cover's ends short of a lap: under Adaptive each such
/// level is refused by name and Offset rings cut it.
#[test]
fn a_flat_channel_across_the_seam_is_refused_adaptive_by_name_and_rings_cut_it() {
    let f = dflat_bar(64, -40.0, 40.0, SEAM_PHI, 4.0, Some((-8.0, 8.0)));
    let p = Pocket3DParams { stepdown: 1.0, ..params(PocketStrategy::Adaptive, 4.0, 5.0) };
    let dev = match rotary_offset_developed(&f, None, &p) {
        Ok(t) => t,
        Err(e) => panic!("the developed seam window: {e}"),
    };
    assert!(dev.is_chained() && dev.all_finite());
    println!("notes: {:?}", dev.notes);
    let seam: Vec<&String> = dev.notes.iter().filter(|n| n.starts_with(SEAM_RINGS)).collect();
    assert!(!seam.is_empty(), "no developed level was refused at the seam: {:?}", dev.notes);
    for n in &seam {
        // The note names its level's radius; rings cut that level.
        let Some(r) = n.rsplit("radius ").next().and_then(|t| t.split(' ').next()).and_then(|t| t.parse::<f64>().ok()) else {
            panic!("a seam refusal that names no radius: {n}")
        };
        assert!(r < RAD, "the seam refusal names r{r}, not a level under the skin");
        let at = feeds(&dev).filter(|m| (m.from.z - r).abs() < 1e-3 && (m.to.z - r).abs() < 1e-3).count();
        let arcs = feeds(&dev).filter(|m| (m.from.z - r).abs() < 1e-3 && (m.to.z - r).abs() < 1e-3 && m.arc.is_some()).count();
        assert!(at > 0, "the seam-refused level r{r} is not cut");
        assert_eq!(arcs, 0, "the seam-refused level r{r} posts HSM arcs, not rings");
    }
}
