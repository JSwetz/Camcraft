//! Pocket and slot recognition (#35, M13.6): the flat floors of a model
//! whose boundary is held by walls rising on opposite sides, read off the
//! planar-face topology `geometry::feature` builds (the B-rep this repo
//! machines from). Each recognised floor is a closed pocket, an open
//! pocket (walls broken by open spans), or a slot - a straight channel of
//! constant width whose ends are open, round or square. The answer feeds
//! an editable operation (`operation_for`), the way hole recognition
//! feeds a drill.
//!
//! Provenance: our own algorithm, specced from the published
//! feature-recognition literature's face-adjacency rule (a depression
//! feature is a face whose boundary edges are concave, after Joshi &
//! Chang, "Graph-based heuristics for recognition of machined features
//! from a 3D solid model", CAD 20(2), 1988) and machining-handbook slot
//! and pocket definitions; no vendor material or output was consulted.
//!
//! Robustness (Prime Directive): total on any mesh. Non-finite or
//! degenerate facets never weld, faces that refuse a plane or a closed
//! ring are skipped, and a model with no geometry is a refusal by name.

mod op;
mod shape;
mod walls;

pub use op::{op_kind_of, operation_for};

use crate::geo2d::Point2;
use crate::geometry::feature::{FaceId, FaceKind, LoopKind};
use crate::model;
use flecs_ecs::prelude::World;

/// What a recognised floor is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PocketKind {
    /// Walls all round the floor's outer ring.
    Closed,
    /// Walls on opposite sides, broken by one or more open spans.
    Open,
    /// A straight channel of constant width (see [`SlotShape`]).
    Slot,
}

/// How one end of a slot finishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotEnd {
    /// No wall: the channel runs out of the part there.
    Open,
    /// A semicircular wall of the slot's width.
    Round,
    /// A wall across the channel (square or small-radius corners).
    Square,
}

/// A slot's straight-channel reading, world XY.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotShape {
    /// The centreline ends a slot cut runs between: the cap centre of a
    /// round end, the floor's edge at an open or square end.
    pub a: Point2,
    pub b: Point2,
    pub width: f64,
    /// Overall floor length along the centreline.
    pub length: f64,
    pub ends: [SlotEnd; 2],
}

/// One recognised pocket or slot, world coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Pocket {
    /// The model it was read from.
    pub model: String,
    /// The floor face's stable id (`FeatureSel::Face`).
    pub floor: FaceId,
    pub kind: PocketKind,
    /// The floor's outer ring, counter-clockwise from above.
    pub boundary: Vec<Point2>,
    /// Inner rings of the floor (bosses standing in it, or holes
    /// through it).
    pub islands: usize,
    pub floor_z: f64,
    /// The highest point of the walls standing on the floor.
    pub top_z: f64,
    pub depth: f64,
    /// Floor area, mm².
    pub area: f64,
    /// The tightest inside corner between two walls: 0 for a sharp
    /// corner, `None` when no two walls meet.
    pub min_corner_radius: Option<f64>,
    /// The spans of the outer ring no wall stands on, as segments.
    pub open_spans: Vec<[Point2; 2]>,
    pub slot: Option<SlotShape>,
}

/// The pockets and slots of one named model, ordered by floor Z (deepest
/// first) then by position, so the list reads the same every call.
///
/// # Errors
/// A name that is no model, or a model with no usable geometry.
pub fn pockets_of(world: &World, name: &str) -> Result<Vec<Pocket>, String> {
    let e = model::find_by_name(world, name).ok_or_else(|| format!("no model named '{name}'"))?;
    if !model::is_model(world, &e) {
        return Err(format!("'{name}' is not a model"));
    }
    let tris = model::facets_of(world, &e).ok_or_else(|| format!("'{name}' has no geometry"))?;
    if tris.is_empty() {
        return Err(format!("'{name}' has no geometry"));
    }
    let loc = model::transform_of(world, &e).unwrap_or_default();
    let mesh = walls::Mesh::new(&tris, &loc);
    if mesh.usable == 0 {
        return Err(format!("'{name}' has no usable facets (every one degenerate or non-finite)"));
    }
    let features = model::features_of_local(&tris);
    let mut out = Vec::new();
    for (fi, face) in features.faces.iter().enumerate() {
        if !matches!(face.kind, FaceKind::Planar) {
            continue;
        }
        let Some(outer) = face.loops.iter().find(|l| matches!(l.kind, LoopKind::Outer)) else {
            continue;
        };
        let Some(floor) = mesh.floor_of(&features, fi, face, outer) else {
            continue;
        };
        let islands = face.loops.iter().filter(|l| matches!(l.kind, LoopKind::Hole)).count();
        if let Some(p) = shape::classify(name, face.id, floor, islands) {
            out.push(p);
        }
    }
    out.sort_by(|a, b| {
        a.floor_z
            .total_cmp(&b.floor_z)
            .then(a.boundary.first().map_or(0.0, |p| p.x).total_cmp(&b.boundary.first().map_or(0.0, |p| p.x)))
            .then(a.boundary.first().map_or(0.0, |p| p.y).total_cmp(&b.boundary.first().map_or(0.0, |p| p.y)))
    });
    Ok(out)
}
