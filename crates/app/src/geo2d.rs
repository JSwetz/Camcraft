//! 2D regions: closed loops with even-odd semantics, booleans and buffering.
//!
//! Provenance: the region model (closed boundaries, even-odd containment) is
//! the standard polygon-clipping formulation from the published literature;
//! the boolean/offset engine behind the bridge below is `clipper2-rust`, a
//! pure-Rust port (BSL-1.0) of the published Clipper2 library - used as a
//! dependency, not adapted code, and zero vendor CAM provenance either way.
//! The millimeter bridge, validation gates and measurements are ours.
//!
//! Units: the public surface is millimeters in `f64`. Clipper's domain is
//! integers, so every crossing quantizes to the fixed [`SCALE`] (1 unit =
//! 1 um, far finer than any CAM tolerance). Inputs must be finite and far
//! inside the i64 horizon; anything else becomes a `GeoError`, never a
//! garbage number or a panic (Prime Directive).

use thiserror::Error;

use clipper2_rust::{
    EndType, FillRule, JoinType, Path64, Paths64, Point64, inflate_paths_64, intersect_64,
    difference_64, union_64, union_subjects_64,
};

/// Millimeters to Clipper units and back: 1e6, so a unit is a micron.
pub const SCALE: f64 = 1e6;

/// Coordinate magnitude guard: |mm| must stay far below i64::MAX / SCALE
/// (about 9.2e12) so quantization can never wrap. Half that horizon is still
/// a thousand times any machine travel.
pub(crate) const COORD_LIMIT: f64 = 1e12;

/// Round-join arc flattening: maximum radial error in units. 5e3 = 5 um -
/// finer than CAM gouge tolerances, coarse enough that a full circle at
/// tool radius stays in the dozens of segments.
const ARC_TOL_UNITS: f64 = 5e3;

/// Miter limit, unused while joins are round; Clipper's default.
const MITER_LIMIT: f64 = 2.0;

#[derive(Error, Debug, PartialEq)]
pub enum GeoError {
    #[error("invalid geometry: {0}")]
    Invalid(String),
    #[error("coordinates outside the machinable range")]
    OutOfRange,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub fn new(x: f64, y: f64) -> Self {
        Point2 { x, y }
    }

    fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// One closed boundary, an ordered ring of points. Orientation is not part
/// of the contract: `Region` interprets its loops by even-odd parity, and
/// `Region::new` normalizes whatever it is handed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loop {
    pts: Vec<Point2>,
}

impl Loop {
    /// Validate for external input (sketch points, sliced geometry): finite,
    /// in range, at least three distinct vertices. Consecutive duplicates
    /// are dropped, not refused - quantization round-trips create them.
    pub fn new(pts: Vec<Point2>) -> Result<Self, GeoError> {
        for p in &pts {
            if !p.is_finite() {
                return Err(GeoError::Invalid("non-finite vertex".into()));
            }
            if p.x.abs() >= COORD_LIMIT || p.y.abs() >= COORD_LIMIT {
                return Err(GeoError::OutOfRange);
            }
        }
        let looped = Self::dedup(pts);
        if looped.len() < 3 {
            return Err(GeoError::Invalid("loop has fewer than 3 vertices".into()));
        }
        Ok(Loop { pts: looped })
    }

    /// Drop consecutive repeats, including the closing wrap (input may or
    /// may not repeat the first vertex).
    fn dedup(pts: Vec<Point2>) -> Vec<Point2> {
        let mut out: Vec<Point2> = Vec::with_capacity(pts.len());
        for p in pts {
            if out.last().is_none_or(|q| q.x != p.x || q.y != p.y) {
                out.push(p);
            }
        }
        if out.len() > 1 && out[0].x == out[out.len() - 1].x && out[0].y == out[out.len() - 1].y {
            out.pop();
        }
        out
    }

    pub fn points(&self) -> &[Point2] {
        &self.pts
    }

    /// Signed shoelace area in mm^2; positive when the ring runs
    /// counterclockwise. Zero only for collinear (degenerate) rings.
    pub fn area(&self) -> f64 {
        let mut sum = 0.0f64;
        for i in 0..self.pts.len() {
            let a = self.pts[i];
            let b = self.pts[(i + 1) % self.pts.len()];
            sum += a.x * b.y - b.x * a.y;
        }
        sum * 0.5
    }

    pub fn is_ccw(&self) -> bool {
        self.area() > 0.0
    }

    fn contains_xy(&self, p: Point2) -> bool {
        // Ray parity to +x with the half-open edge rule: an edge counts if
        // p.y is in [yi, yj) (lower-inclusive, upper-exclusive, after the
        // swap), which makes shared vertices and horizontal rays exact.
        let mut inside = false;
        let n = self.pts.len();
        for i in 0..n {
            let a = self.pts[i];
            let b = self.pts[(i + 1) % n];
            let (ylo, yhi) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
            if p.y >= ylo && p.y < yhi && n != 0 {
                let x_at = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if p.x < x_at {
                    inside = !inside;
                }
            }
        }
        inside
    }

    fn to_path64(&self) -> Result<Path64, GeoError> {
        let mut path = Path64::with_capacity(self.pts.len());
        for p in &self.pts {
            path.push(Point64 {
                x: quantize(p.x)?,
                y: quantize(p.y)?,
            });
        }
        Ok(path)
    }

    fn from_path64(path: &Path64) -> Loop {
        Loop {
            pts: path
                .iter()
                .map(|q| Point2 {
                    x: q.x as f64 / SCALE,
                    y: q.y as f64 / SCALE,
                })
                .collect(),
        }
    }
}

/// An even-odd region: a set of closed loops whose combined interior is the
/// material. After construction the loops are disjoint, so containment is
/// plain parity and the area is a signed sum by nesting depth.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Region {
    loops: Vec<Loop>,
}

impl Region {
    /// Accept any loop set - overlapping outer loops from several solids,
    /// holes in either orientation - and normalize. This is the only entry
    /// from sketched/raw geometry; never build Region by assignment (see
    /// also `from_slices`, which relaxes the self-crossing refusal for
    /// slicer output). The semantics: loops are shells, so non-nesting
    /// loops carry material jointly (two overlapping boxes union, their
    /// overlap is not a void), while a loop nested in another at odd depth
    /// is a hole. Implementation: orient by nesting parity, then fuse with
    /// a non-zero union (see the Clipper2 fill-rule docs).
    pub fn new(loops: Vec<Loop>) -> Result<Region, GeoError> {
        Self::normalize(loops, false)
    }

    /// Region from sliced-shell output, where a self-crossing cycle is a
    /// real boundary (two lobes touching along a line: the shell encloses
    /// both) rather than ambiguous user input, so the even-odd decomposition
    /// is kept instead of refused. Everything else is `new`'s contract.
    pub fn from_slices(loops: Vec<Loop>) -> Result<Region, GeoError> {
        Self::normalize(loops, true)
    }

    fn normalize(loops: Vec<Loop>, decompose_crossings: bool) -> Result<Region, GeoError> {
        // 1. A loop that crosses itself is not a boundary of anything a
        //    sketch could mean; its "area" is convention dependent. For
        //    sketch input (`new`) refuse rather than half-compute (Prime
        //    Directive); for slicer output (`from_slices`) the even-odd
        //    decomposition IS the canonical reading, so keep the pieces.
        //    A simple loop yields exactly one path either way.
        let mut simple: Vec<Loop> = Vec::new();
        for l in &loops {
            let dec = union_subjects_64(&vec![l.to_path64()?], FillRule::EvenOdd);
            if dec.len() > 1 && !decompose_crossings {
                return Err(GeoError::Invalid("loop crosses itself".into()));
            }
            for o in dec {
                let o = Loop::from_path64(&o);
                if o.pts.len() >= 3 {
                    simple.push(o);
                }
            }
        }
        // 2. Orient by nesting depth: even depth CCW (material), odd CW
        //    (hole). Nesting is all-vertices-inside, not one anchor point
        //    inside - a partial overlap must read as two outers (a non-zero
        //    union fuses them), not as a hole punched in the first.
        let oriented: Vec<Loop> = simple
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let depth = simple
                    .iter()
                    .enumerate()
                    .filter(|(j, o)| {
                        *j != i && l.points().iter().all(|p| o.contains_xy(*p))
                    })
                    .count();
                if (depth % 2 == 0) == l.is_ccw() {
                    l.clone()
                } else {
                    Loop {
                        pts: l.pts.iter().rev().copied().collect(),
                    }
                }
            })
            .collect();
        // 3. Fuse: overlapping outers become one material; nesting stays.
        Ok(Self::from_paths(union_subjects_64(
            &to_paths(&oriented)?,
            FillRule::NonZero,
        )))
    }

    pub fn empty() -> Region {
        Region::default()
    }

    pub fn is_empty(&self) -> bool {
        self.loops.is_empty()
    }

    pub fn loops(&self) -> &[Loop] {
        &self.loops
    }

    /// Axis-aligned XY box of all vertices, (min, max); None when empty.
    /// Generators band their passes with this; it bounds the boundary
    /// points, which for the polygonized regions here is exact enough -
    /// round-join arcs bulge past their chord by at most ARC_TOL_UNITS.
    pub fn bounds(&self) -> Option<(Point2, Point2)> {
        let mut it = self.loops.iter().flat_map(|l| l.points().iter());
        let first = it.next()?;
        let mut box_ = (Point2::new(first.x, first.y), Point2::new(first.x, first.y));
        for p in it {
            box_.0.x = box_.0.x.min(p.x);
            box_.0.y = box_.0.y.min(p.y);
            box_.1.x = box_.1.x.max(p.x);
            box_.1.y = box_.1.y.max(p.y);
        }
        Some(box_)
    }

    fn from_paths(paths: Paths64) -> Region {
        Region {
            loops: paths
                .iter()
                .map(Loop::from_path64)
                .filter(|l| l.pts.len() >= 3)
                .collect(),
        }
    }

    fn to_paths(&self) -> Result<Paths64, GeoError> {
        to_paths(&self.loops)
    }

    /// Containment: parity of the total ray crossings over all loops.
    /// (On the normalized, disjoint loop set this equals winding, which is
    /// what the overlap-union contract in `new` guarantees.) On an exact
    /// edge the half-open ray rule decides; gouge checks stay a tolerance
    /// away from boundaries anyway.
    pub fn contains(&self, p: Point2) -> bool {
        let mut inside = false;
        for l in &self.loops {
            inside ^= l.contains_xy(p);
        }
        inside
    }

    /// Region area in mm^2: the signed loop-area sum. On a normalized
    /// region the winding number equals the material indicator (outer
    /// CCW = +1, hole CW brings its interior back to 0, an island inside
    /// a hole to +1, ...), so the integral is the sum of signed shoelace
    /// areas - exact, O(n), and unlike a per-loop containment test it
    /// cannot be fooled by loops that touch at a shared vertex.
    pub fn area(&self) -> f64 {
        self.loops.iter().map(Loop::area).sum()
    }

    pub fn union(&self, other: &Region) -> Result<Region, GeoError> {
        Ok(Self::from_paths(union_64(
            &self.to_paths()?,
            &other.to_paths()?,
            FillRule::NonZero,
        )))
    }

    pub fn intersect(&self, other: &Region) -> Result<Region, GeoError> {
        Ok(Self::from_paths(intersect_64(
            &self.to_paths()?,
            &other.to_paths()?,
            FillRule::NonZero,
        )))
    }

    pub fn difference(&self, other: &Region) -> Result<Region, GeoError> {
        Ok(Self::from_paths(difference_64(
            &self.to_paths()?,
            &other.to_paths()?,
            FillRule::NonZero,
        )))
    }

    /// Buffer the region by `d` mm: positive grows, negative shrinks
    /// (a shrink past the medial axis collapses to empty - a clean empty
    /// region, not an error: the generator decides what that means).
    /// Round joins keep corners circular within [`ARC_TOL_UNITS`].
    pub fn offset(&self, d: f64) -> Result<Region, GeoError> {
        if !d.is_finite() {
            return Err(GeoError::Invalid("non-finite offset distance".into()));
        }
        if d.abs() >= COORD_LIMIT {
            return Err(GeoError::OutOfRange);
        }
        Ok(Self::from_paths(inflate_paths_64(
            &self.to_paths()?,
            d * SCALE,
            JoinType::Round,
            EndType::Polygon,
            MITER_LIMIT,
            ARC_TOL_UNITS,
        )))
    }

    pub fn square(side: f64) -> Result<Region, GeoError> {
        Region::new(vec![Loop::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(side, 0.0),
            Point2::new(side, side),
            Point2::new(0.0, side),
        ])?])
    }
}

fn to_paths(loops: &[Loop]) -> Result<Paths64, GeoError> {
    let mut paths = Vec::with_capacity(loops.len());
    for l in loops {
        paths.push(l.to_path64()?);
    }
    Ok(paths)
}

fn quantize(mm: f64) -> Result<i64, GeoError> {
    if !mm.is_finite() {
        return Err(GeoError::Invalid("non-finite coordinate".into()));
    }
    if mm.abs() >= COORD_LIMIT {
        return Err(GeoError::OutOfRange);
    }
    Ok((mm * SCALE).round() as i64)
}
