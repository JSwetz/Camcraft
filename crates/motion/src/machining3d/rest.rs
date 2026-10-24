//! Split from `machining3d.rs` by M5.0.1; structure only.


use crate::geo2d::{Point2, Region};
use crate::geometry::slice::{self, SliceError};
use crate::geometry::Triangle;
use crate::machining2d::{
    chord_crosses_path, GenError,
};
/// Half-open slices sit exactly on vertices for tessellated steps; this
/// is the same lattice the weld uses, so a grazing plane nudges rather
/// than lies.
pub(super) const EPS: f64 = 1e-6;

pub(super) fn from_slice(e: SliceError) -> GenError {
    match e {
        SliceError::Geo(g) => GenError::Geo(g),
        other => GenError::Invalid(other.to_string()),
    }
}

/// The model's cross-section strictly above `z`, nudge-retried when a
/// plane grazes a tessellated shelf (reading an errored slice as
/// no-island would be a silent gouge - the retry lands above the shelf
/// instead, failing safe). Public because it is the islands law itself:
/// the generator reads it to shape each level, the verify pass to prove
/// them, and the tests to pin the monotonicity the ladder assumes -
/// one law, three readers (M2.1a).
pub fn material_above(facets: &[Triangle], z: f64) -> Result<Region, GenError> {
    slice::material_above(facets, z).map_err(from_slice)
}

/// Would a feed chord `a -> b` along this level's plane properly cross
/// the level's own planned paths? `chord_clears` proves the chord stays
/// inside the cutter-center area, but on a holed area (an island, a
/// boss) a chord from one family's ring to another's crosses every ring
/// in between, and `verify` reports each such crossing at the cut depth.
/// A chord that fails here is flown over instead: rapids, traverse,
/// plunge. This is the same law `waterline_passes` chains its rings by,
/// applied where the ladder joins one planned pass to the next.
pub(super) fn chord_hits_plan(a: Point2, b: Point2, passes: &[Vec<Point2>]) -> bool {
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
pub(super) fn dive_point(passes: &[Vec<Point2>], leg_prev: &Region, leg_here: &Region) -> Option<Point2> {
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
