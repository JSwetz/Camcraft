//! The entry family: how the cutter first engages material at a level,
//! and the engagement law that spaces the `Adaptive` rings.
//!
//! Provenance: standard CNC entry motions - straight plunge, linear
//! ramp, and helical interpolation - as described in public G-code
//! manuals and general machining handbooks; the laws are ours
//! (Clean-Room Protocol).
//!
//! The family specced at M2.1 for the 3-D ladder's level changes. M5.1
//! H lifts it to a crate seam so the planar pocket's ladder can engage
//! its levels the same way - the motions read only planar geometry and
//! the move chain, which both ladders share. `machining3d` keeps
//! re-exporting the vocabulary (`machining3d::Entry3D`), so no caller
//! path changed; the level-crossing legality arguments (the islands
//! superset: a chord legal at the deepest cut plane is legal at every
//! plane above) live where each ladder proves them.

use crate::cl::{lattice, MoveKind, PathBuilder};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::chord_crosses_path;

/// How the cutter first engages material at a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Entry3D {
    #[default]
    /// Axial dive at the pass entry, the 2-D pocket motion.
    Plunge,
    /// One straight diagonal feed from the previous level's plane down
    /// to this one (first level: falls back to a plunge - there is no
    /// open floor to start a ramp from).
    Ramp,
    /// Helical interpolation: orbit while descending one stepdown (the
    /// widened arc law, see `cl.rs`). Air orbits above stock are never
    /// posted: the first level's dive is the surface itself.
    Helix,
}

/// One helical dive between two planes: at least one full turn (split
/// into half-circle arcs so no move is a zero-sweep full circle), Z
/// descending linearly with the orbit. The builder must stand at `xy`
/// on plane `z0` (the caller's rapid to the dive point); an even
/// number of half turns ends back on `xy` at `z1` by construction -
/// each turn is two 180° arcs whose XY net is zero. Both ladders'
/// Helix arm ends here.
pub(crate) fn helix_down(b: &mut PathBuilder, xy: Point2, z0: f64, z1: f64, rh: f64, feed: f64) {
    let span = z0 - z1;
    if span <= 0.0 || rh <= 0.0 {
        return;
    }
    // At least one turn per dive; one more when the span is deeper than
    // the circle's own reach, so the bore's slope stays inside the tool
    // diameter's width.
    let turns = (span / (rh * 2.0)).ceil().max(1.0);
    let half_turns = ((turns * 2.0).ceil() as u32).max(2);
    // Orbit about the circle whose near side is the entry point: center
    // offset +rh in X. Endpoints sit lattice-snapped (the arc law wants
    // a 1 um center; the builder keeps the chord endpoints exact).
    let center = Point2::new(lattice(xy.x + rh), lattice(xy.y));
    for h in 0..half_turns {
        let frac = f64::from(h + 1) / f64::from(half_turns);
        let tz = z0 - span * frac;
        // Alternating far/near point of the circle; even counts close on
        // xy at the last leg.
        let px = if h % 2 == 0 { xy.x + 2.0 * rh } else { xy.x };
        // ccw: right -> left over the +Y side.
        b.push_arc(
            MoveKind::Feed,
            Point3::new(lattice(px), lattice(xy.y), lattice(tz)),
            Some(feed),
            center,
            true,
        );
    }
}

/// The engagement law, in one line of trigonometry: a pass that bites
/// a wall face radially by `a` with a disc of radius `r` contacts it
/// over a half-angle `theta` with `a = r (1 - cos theta)` - the sagitta
/// of the contact arc. Inverting it gives the biggest ring step whose
/// contact stays under a chosen angle: `a = r (1 - cos theta_max)`.
/// Published trochoidal/HEM literature is all computations of this
/// same disc-against-wall geometry; this is our own reading of it,
/// pinned by `the_engagement_law_is_the_sagitta` (M2.1c, lifted to the
/// seam shared by both roughing ladders in M5.1 H).
#[must_use]
pub fn engagement_step(tool_r: f64, max_engagement_deg: f64) -> f64 {
    tool_r * (1.0 - max_engagement_deg.to_radians().cos())
}

/// Would a feed chord `a -> b` along this level's plane properly cross
/// the level's own planned paths? `chord_clears` proves the chord stays
/// inside the cutter-center area, but on a holed area (an island, a
/// boss) a chord from one family's ring to another's crosses every ring
/// in between, and `verify` reports each such crossing at the cut depth.
/// A chord that fails here is flown over instead: rapids, traverse,
/// plunge. This is the same law `waterline_passes` chains its rings by,
/// applied where the ladder joins one planned pass to the next.
pub(crate) fn chord_hits_plan(a: Point2, b: Point2, passes: &[Vec<Point2>]) -> bool {
    // The planned passes store their full vertex chains - a waterline
    // ring repeats its entry point at the end, so walking consecutive
    // pairs covers its closing leg too - so consecutive segments, never
    // a phantom close, is the exact set of this level's cut paths.
    passes.iter().any(|p| chord_crosses_path(a, b, p, false))
}

/// The helix's dive point: the midpoint of the longest straight run in
/// the level plan that has room for the widened disc at both planes it
/// crosses (the leg regions, each the level's center eroded by the
/// helix radius plus a micron's insurance against buffer-of-buffer
/// quantization). A row's midpoint is the furthest from every wall a
/// planar pass offers; the scanline ends sit on the boundary by
/// construction. First choice is the longest run - the most room, and
/// the row a climb pass would most want to start mid-span on.
pub(crate) fn dive_point(passes: &[Vec<Point2>], leg_prev: &Region, leg_here: &Region) -> Option<Point2> {
    let mut best: Option<(f64, Point2)> = None;
    for pass in passes {
        for pair in pass.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let len = (b.x - a.x).hypot(b.y - a.y);
            if len <= 0.0 || best.is_some_and(|(l, _)| l >= len) {
                continue;
            }
            let mid = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            if leg_prev.contains(mid) && leg_here.contains(mid) {
                best = Some((len, mid));
            }
        }
    }
    best.map(|(_, d)| d)
}
