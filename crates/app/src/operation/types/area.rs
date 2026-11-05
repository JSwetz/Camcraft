//! The area controls of an operation (M11.5): the selection says WHERE,
//! these say HOW FAR - tool containment against the boundary, a grow /
//! shrink offset, an extension past exterior edges, extra include /
//! exclude boundaries, and an open (or closed) boundary with a side.
//! Plain data only; the law that applies them is
//! `operation::boundary`, the world reads are `operation::boundary_ctx`.
//!
//! Provenance: our own design notes on 2.5D machining boundaries (tool
//! inside / on / outside a boundary and offset-with-corner-style are the
//! textbook vocabulary of any machining handbook's 2D toolpath chapter);
//! no vendor material consulted.

use super::params::OpParams;
use super::sel::{GeoSel, OpType};

/// Where the tool may stand relative to the boundary. `None` on
/// [`AreaControls::containment`] is the kind's own law, unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Containment {
    /// The whole tool stays inside the boundary (its edge touches it).
    Inside,
    /// The tool centre rides the boundary.
    On,
    /// The tool passes the boundary until its edge touches it from
    /// outside - the area is cleared past its own edge.
    Outside,
}

/// Corner style of a grow / shrink offset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CornerStyle {
    /// Arcs round the convex corners - the offset's exact law.
    #[default]
    Round,
    /// Corners stay sharp (mitred), as a rectangle grows into a rectangle.
    Sharp,
}

/// Whether an extra boundary adds to the area or keeps the tool out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryMode {
    Include,
    Exclude,
}

/// What an extra boundary is drawn from.
#[derive(Clone, Debug, PartialEq)]
pub enum BoundaryShape {
    /// Picked faces / closed edge chains (their XY loops).
    Picks(Vec<GeoSel>),
    /// The setup stock's outline.
    Stock,
    /// The silhouette of the operation's source part.
    Silhouette,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExtraBoundary {
    pub mode: BoundaryMode,
    pub shape: BoundaryShape,
}

/// Which side of an open (or closed) boundary is machined. Inside is the
/// side that holds more of the operation's own area (what the selection
/// or source encloses) - the part's side; Outside the other one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainSide {
    Inside,
    Outside,
}

/// A boundary the user draws instead of the selection's own: an open
/// chain (closed off by the stock outline) or a closed ring.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenBoundary {
    pub picks: Vec<GeoSel>,
    pub side: ChainSide,
}

/// The area controls. `Default` is the identity: every region, every
/// stamp byte and every saved word is exactly what it was before M11.5.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AreaControls {
    pub containment: Option<Containment>,
    /// Grow (+) or shrink (-) the boundary, mm.
    pub offset: f64,
    pub corner: CornerStyle,
    /// Extend past exterior edges (edges with no part material beyond
    /// them), mm; 0 is off.
    pub extension: f64,
    pub extra: Vec<ExtraBoundary>,
    pub open: Option<OpenBoundary>,
}

/// Largest offset / extension the door accepts, mm: far past any
/// machine table, far inside the region kernel's horizon.
pub const AREA_DISTANCE_MAX: f64 = 10_000.0;

impl AreaControls {
    /// The identity - nothing stamped, nothing saved, nothing changed.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == AreaControls::default()
    }

    /// Door sanity, no world: finite in-range distances, no empty pick
    /// list, and nothing but the identity on a kind without an area.
    #[must_use]
    pub fn is_sane_for(&self, params: &OpParams) -> bool {
        if self.is_default() {
            return true;
        }
        if !area_applies(params.kind()) {
            return false;
        }
        let dist = |v: f64| v.is_finite() && v.abs() <= AREA_DISTANCE_MAX;
        dist(self.offset)
            && dist(self.extension)
            && self.extension >= 0.0
            && self.extra.iter().all(|b| match &b.shape {
                BoundaryShape::Picks(p) => !p.is_empty(),
                BoundaryShape::Stock | BoundaryShape::Silhouette => true,
            })
            && self.open.as_ref().is_none_or(|o| !o.picks.is_empty())
    }

    /// Every pick the controls reference (boundaries and the open one),
    /// for the stamp and the stale-reason walk.
    pub fn picks(&self) -> impl Iterator<Item = &GeoSel> {
        self.extra
            .iter()
            .filter_map(|b| match &b.shape {
                BoundaryShape::Picks(p) => Some(p.iter()),
                _ => None,
            })
            .flatten()
            .chain(self.open.iter().flat_map(|o| o.picks.iter()))
    }
}

/// The kinds whose region IS the area the tool works in, so the area
/// controls mean something: the facing and clearing kinds, the contour
/// ride, and the 3D pocket ladder. Every other kind's region is a
/// shadow for the verifier or authored geometry (slot, circle, holes),
/// and the controls stay the identity there (refused at the door).
#[must_use]
pub fn area_applies(kind: OpType) -> bool {
    matches!(
        kind,
        OpType::Face
            | OpType::Contour
            | OpType::Pocket
            | OpType::Adaptive2D
            | OpType::Offset2D
            | OpType::Pocket3D
    )
}
