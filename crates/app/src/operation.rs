//! Machining operations: what to cut, from where, and the generation
//! orchestration that turns an operation into a verified toolpath.
//!
//! Provenance: our own domain model - operation types, source geometry and
//! staleness tracking are specced from public documentation of commercial
//! CAM workflows; the algorithms invoked here live in `machining2d` and
//! `verify` with their own provenance. No vendor source was consulted.
//!
//! The operation is data (`SrcRef`, `OpParams`, `GenStamp`); the behavior
//! is the two free functions at the bottom:
//!
//! - [`generate`] resolves tool and source geometry in the current world,
//!   builds the cutting region, runs the matching `machining2d` generator
//!   with the tool's diameter injected, and verifies the result. Pure read:
//!   it never touches the world, so commands can run it at construction
//!   and replay its stored output byte-identically through undo/redo.
//! - [`current_stamp`] digests everything the cached path depends on.
//!   Staleness is derived from a stamp mismatch (AGENTS.md: no stored
//!   `Stale` flag), so an edit anywhere the digest reads - tool geometry,
//!   stock size and placement, model move, parameters - turns the cache
//!   grey without any command writing a status.
//!
//! Region rule per operation, at the level the paths live in:
//! - Face: the XY silhouette of the source (`slice::project`), cut plane
//!   from `cut_z`.
//! - Contour / Pocket / Trace: the source's cross-section at `top_z`
//!   (`slice::slice`, half-open; a grazing slice is retried one micron in).
//! - Slot: the closed-form stadium between the two endpoints.
//!
//! Meshes are sliced because STEP import is tessellate-only (no curve data
//! survives), which is also what makes STL, OBJ and STEP sources take the
//! identical path.
//!
//! A `SrcRef::Selection` builds its region from the picked features
//! instead: faces contribute their boundary loops projected to XY, closed
//! edge chains their ring projected to XY, coincident rims dedupe. Face
//! ops demand a near-horizontal face, Pocket demands a closed boundary,
//! and an open run is either a Contour centerline pass along its points
//! or, for a facing op, a limit extended out to the stock (M1.5.1) -
//! never a silent drop. Preview and generation read the same walk, so
//! the two cannot disagree about where that pass or that boundary runs.

use std::collections::HashSet;
use std::hash::Hasher;

use flecs_ecs::prelude::World;

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Triangle;
use crate::geometry::feature::{self, LoopKind};
use crate::geometry::slice::{self, SliceError};
use crate::geometry::{Point3, transform_point};
use crate::machining2d::{
    self, ContourParams, FaceParams, GenError, Lead, PocketParams, PocketStrategy, SlotParams,
    TraceParams,
};
use crate::model;
use crate::schema::Operation;
use crate::stock::StockShape;
use crate::verify::{self, Mode, VerifyContext, Warning};

/// Where the operation's boundary geometry comes from. Names, never
/// entity ids (AGENTS.md: identity by name keeps commands invertible).
#[derive(Clone, Debug, PartialEq)]
pub enum SrcRef {
    /// The setup's stock solid.
    Stock,
    /// Another named entity's geometry.
    Entity(String),
    /// A feature selection: faces and edge chains picked off named
    /// entities, resolved by their geometry-hash ids at generation
    /// (M1.5). An empty list is refused by the command layer.
    Selection(Vec<GeoSel>),
}

/// One selected feature of one named entity. Identity is the feature's
/// geometry hash, not an index or a facet count: re-imports, facet
/// reorder and rigid moves all keep a pick pointing at the same feature
/// (see `geometry::feature`'s stable-id design).
#[derive(Clone, Debug, PartialEq)]
pub struct GeoSel {
    pub entity: String,
    pub feature: FeatureSel,
}

/// What kind of feature was picked; the numbers are [`FaceId`] and
/// [`ChainId`] hashes, never positional.
///
/// [`FaceId`]: crate::geometry::feature::FaceId
/// [`ChainId`]: crate::geometry::feature::ChainId
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FeatureSel {
    Face(crate::geometry::feature::FaceId),
    Chain(crate::geometry::feature::ChainId),
    /// One logical face of a smooth wall: every planar strip the wall was
    /// tessellated into, welded by their shared `facet_patch` root (see
    /// [`patch_rings`]). Only exists where that weld has more than one
    /// strip - a flat face is already whole, and stays a `Face`.
    ///
    /// [`patch_rings`]: crate::geometry::feature::patch_rings
    Patch(crate::geometry::feature::PatchId),
}

impl core::fmt::Display for FeatureSel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FeatureSel::Face(id) => write!(f, "face {id:#x}"),
            FeatureSel::Chain(id) => write!(f, "edge chain {id:#x}"),
            FeatureSel::Patch(id) => write!(f, "welded face {id:#x}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpType {
    Face,
    Contour,
    Pocket,
    Slot,
    Trace,
}

impl core::fmt::Display for OpType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            OpType::Face => "Face",
            OpType::Contour => "2D Contour",
            OpType::Pocket => "2D Pocket",
            OpType::Slot => "Slot",
            OpType::Trace => "Trace",
        };
        f.write_str(s)
    }
}

/// Everything an operation needs beyond its tool and source. The tool
/// diameter is deliberately absent - it lives in the tool definition and
/// is injected at generation, so a tool edit cannot disagree with a
/// parameter field.
#[derive(Clone, Debug, PartialEq)]
pub enum OpParams {
    Face {
        stepover: f64,
        clearance_z: f64,
        cut_z: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
    },
    Contour {
        stock_to_leave: f64,
        clearance_z: f64,
        retract_z: f64,
        top_z: f64,
        bottom_z: f64,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead: Lead,
        tolerance: f64,
        tab_count: u32,
        tab_width: f64,
        tab_height: f64,
    },
    Pocket {
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: f64,
        top_z: f64,
        bottom_z: f64,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: PocketStrategy,
    },
    Slot {
        a: Point2,
        b: Point2,
        width: f64,
        stock_to_leave: f64,
        clearance_z: f64,
        top_z: f64,
        bottom_z: f64,
        stepdown: f64,
        stepover: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
    },
    Trace {
        clearance_z: f64,
        top_z: f64,
        bottom_z: f64,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
    },
}

impl OpParams {
    #[must_use]
    pub fn kind(&self) -> OpType {
        match self {
            OpParams::Face { .. } => OpType::Face,
            OpParams::Contour { .. } => OpType::Contour,
            OpParams::Pocket { .. } => OpType::Pocket,
            OpParams::Slot { .. } => OpType::Slot,
            OpParams::Trace { .. } => OpType::Trace,
        }
    }

    #[must_use]
    pub fn clearance_z(&self) -> f64 {
        match self {
            OpParams::Face { clearance_z, .. }
            | OpParams::Contour { clearance_z, .. }
            | OpParams::Pocket { clearance_z, .. }
            | OpParams::Slot { clearance_z, .. }
            | OpParams::Trace { clearance_z, .. } => *clearance_z,
        }
    }

    /// The Z level the cutting region lives at: the section/stadium plane
    /// for the depth-bounded kinds, the skimming plane for Face. The
    /// viewport's "what gets cut" preview draws the region loops here.
    #[must_use]
    pub fn region_z(&self) -> f64 {
        match self {
            OpParams::Face { cut_z, .. } => *cut_z,
            OpParams::Contour { top_z, .. }
            | OpParams::Pocket { top_z, .. }
            | OpParams::Slot { top_z, .. }
            | OpParams::Trace { top_z, .. } => *top_z,
        }
    }

    /// Shallow sanity for command construction: finite numbers, positive
    /// increments, a downward depth range. The generators own the deep
    /// rules (stepover vs diameter, lead sizes); those still fire at
    /// generation time inside `GenerateOperation`'s constructor.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let pos = |v: f64| v.is_finite() && v > 0.0;
        let nneg = |v: f64| v.is_finite() && v >= 0.0;
        match self {
            OpParams::Face { stepover, clearance_z, cut_z, feed, plunge_feed, .. } => {
                pos(*stepover) && clearance_z.is_finite() && cut_z.is_finite()
                    && pos(*feed) && pos(*plunge_feed)
            }
            OpParams::Contour {
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                lead,
                tolerance,
                tab_width,
                tab_height,
                ..
            } => {
                nneg(*stock_to_leave)
                    && clearance_z.is_finite()
                    && retract_z.is_finite()
                    && top_z.is_finite()
                    && bottom_z.is_finite()
                    && top_z > bottom_z
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*tolerance)
                    && nneg(*tab_width)
                    && nneg(*tab_height)
                    && lead_sane(lead)
            }
            OpParams::Pocket {
                stepover,
                stock_to_leave,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_finite()
                    && top_z.is_finite()
                    && bottom_z.is_finite()
                    && top_z > bottom_z
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Slot {
                a,
                b,
                width,
                stock_to_leave,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                stepover,
                feed,
                plunge_feed,
                ..
            } => {
                a.x.is_finite()
                    && a.y.is_finite()
                    && b.x.is_finite()
                    && b.y.is_finite()
                    && pos(*width)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_finite()
                    && top_z.is_finite()
                    && bottom_z.is_finite()
                    && top_z > bottom_z
                    && pos(*stepdown)
                    && pos(*stepover)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Trace {
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                clearance_z.is_finite()
                    && top_z.is_finite()
                    && bottom_z.is_finite()
                    && top_z > bottom_z
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
        }
    }

    /// The digest-relevant scalars, written to a hasher in a fixed order.
    fn stamp_with(&self, h: &mut std::hash::DefaultHasher) {
        let w = |h: &mut std::hash::DefaultHasher, v: f64| h.write(&v.to_bits().to_le_bytes());
        let wp2 = |h: &mut std::hash::DefaultHasher, p: Point2| {
            w(h, p.x);
            w(h, p.y);
        };
        match self {
            OpParams::Face { stepover, clearance_z, cut_z, feed, plunge_feed, climb } => {
                h.write_u8(0);
                for v in [*stepover, *clearance_z, *cut_z, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
            }
            OpParams::Contour {
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                lead,
                tolerance,
                tab_count,
                tab_width,
                tab_height,
                climb,
            } => {
                h.write_u8(1);
                for v in [
                    *stock_to_leave,
                    *clearance_z,
                    *retract_z,
                    *top_z,
                    *bottom_z,
                    *stepdown,
                    *feed,
                    *plunge_feed,
                    *tolerance,
                    *tab_width,
                    *tab_height,
                ] {
                    w(h, v);
                }
                h.write_u32(*tab_count);
                h.write_u8(u8::from(*climb));
                match lead {
                    Lead::None => h.write_u8(0),
                    Lead::Linear { length } => {
                        h.write_u8(1);
                        w(h, *length);
                    }
                    Lead::Arc { radius } => {
                        h.write_u8(2);
                        w(h, *radius);
                    }
                }
            }
            OpParams::Pocket {
                stepover,
                stock_to_leave,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
                strategy,
            } => {
                h.write_u8(2);
                for v in [
                    *stepover,
                    *stock_to_leave,
                    *clearance_z,
                    *top_z,
                    *bottom_z,
                    *stepdown,
                    *feed,
                    *plunge_feed,
                ] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                h.write_u8(match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                });
            }
            OpParams::Slot {
                a,
                b,
                width,
                stock_to_leave,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                stepover,
                feed,
                plunge_feed,
                climb,
            } => {
                h.write_u8(3);
                wp2(h, *a);
                wp2(h, *b);
                for v in [
                    *width,
                    *stock_to_leave,
                    *clearance_z,
                    *top_z,
                    *bottom_z,
                    *stepdown,
                    *stepover,
                    *feed,
                    *plunge_feed,
                ] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
            }
            OpParams::Trace {
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
            } => {
                h.write_u8(4);
                for v in [*clearance_z, *top_z, *bottom_z, *stepdown, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
            }
        }
    }
}

fn lead_sane(lead: &Lead) -> bool {
    match lead {
        Lead::None => true,
        Lead::Linear { length } => length.is_finite() && *length >= 0.0,
        Lead::Arc { radius } => radius.is_finite() && *radius > 0.0,
    }
}

/// The digest of everything a generated path depends on. Not a
/// cross-version identifier - the algorithm behind it is `DefaultHasher`,
/// stable within a build, and a spurious mismatch after a toolchain bump
/// only asks for a re-generate, never cuts anything wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenStamp(pub u64);

/// What the tree and properties panel show beside an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpStatus {
    /// Never generated (or the cache is gone).
    Never,
    /// Cached, and every input still matches the stamp taken at generation.
    Ok,
    /// Cached, but some input moved. The path on screen may not match the
    /// model or tool behind it.
    Stale,
}

/// Inputs an operation depends on: tool (name and numerics), parameters,
/// and the source's placement and shape. `None` when a referenced entity
/// is gone - an unresolvable digest is never equal to a stored one, which
/// is exactly the "Stale" reading a deleted tool should give.
#[must_use]
pub fn current_stamp(
    world: &World,
    tool_name: &str,
    src: &SrcRef,
    params: &OpParams,
) -> Option<GenStamp> {
    let mut h = std::hash::DefaultHasher::new();
    h.write(b"hsmw-op-stamp-v1");
    let w = |h: &mut std::hash::DefaultHasher, v: f64| h.write(&v.to_bits().to_le_bytes());
    h.write(tool_name.as_bytes());
    h.write_u8(0xff);
    let tool = model::find_by_name(world, tool_name)
        .and_then(|e| model::tool_of(world, &e))?;
    h.write_u8(match tool.kind {
        crate::tools::ToolKind::EndMill => 0,
        crate::tools::ToolKind::BallNose => 1,
        crate::tools::ToolKind::BullNose => 2,
    });
    for v in [
        tool.diameter,
        tool.corner_r,
        tool.flute_len,
        tool.shank_d,
        tool.shank_len,
        tool.holder_d,
        tool.holder_len,
    ] {
        w(&mut h, v);
    }
    params.stamp_with(&mut h);
    match src {
        SrcRef::Stock => {
            h.write_u8(b'S');
            let entity = model::find_by_name(world, crate::schema::STOCK_NAME)?;
            let loc = model::transform_of(world, &entity).unwrap_or_default();
            for v in [loc.pos.x, loc.pos.y, loc.pos.z, loc.rot[0], loc.rot[1], loc.rot[2], loc.rot[3]]
            {
                w(&mut h, v);
            }
            match model::stock_def_of(world, &entity)? {
                StockShape::Box { dx, dy, dz } => {
                    h.write_u8(0);
                    for v in [dx, dy, dz] {
                        w(&mut h, v);
                    }
                }
                StockShape::Cylinder { diameter, height } => {
                    h.write_u8(1);
                    w(&mut h, diameter);
                    w(&mut h, height);
                }
                StockShape::FromModel { source } => {
                    h.write_u8(2);
                    h.write(source.as_bytes());
                }
            }
        }
        SrcRef::Entity(name) => {
            h.write_u8(b'E');
            h.write(name.as_bytes());
            let entity = model::find_by_name(world, name)?;
            let loc = model::transform_of(world, &entity).unwrap_or_default();
            for v in [loc.pos.x, loc.pos.y, loc.pos.z, loc.rot[0], loc.rot[1], loc.rot[2], loc.rot[3]]
            {
                w(&mut h, v);
            }
            let tris = model::facets_of(world, &entity)?;
            h.write_usize(tris.len());
            if let Some(b) = model::entity_world_aabb(world, &entity) {
                for v in [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z] {
                    w(&mut h, v);
                }
            }
        }
        SrcRef::Selection(items) => {
            // b'L', not b'S': the Stock arm already owns that byte, and a
            // digest that collides across source kinds is a staleness bug.
            h.write_u8(b'L');
            h.write_usize(items.len());
            let mut digested: Vec<&str> = Vec::new();
            for g in items {
                h.write_u8(match g.feature {
                    FeatureSel::Face(_) => 0,
                    FeatureSel::Chain(_) => 1,
                    FeatureSel::Patch(_) => 2,
                });
                h.write_usize(g.entity.len());
                h.write(g.entity.as_bytes());
                let id = match g.feature {
                    FeatureSel::Face(id) | FeatureSel::Chain(id) | FeatureSel::Patch(id) => id,
                };
                h.write_u64(id);
                // The pick resolves against this entity's LOCAL features
                // (ids are transform-free) and projects through its
                // transform, so both belong in the digest. The order-free
                // geometry digest means a re-import that keeps the same
                // features stays fresh, and any real change goes stale -
                // including a different mesh that happens to repeat the
                // facet count.
                let entity = model::find_by_name(world, &g.entity)?;
                if !digested.contains(&g.entity.as_str()) {
                    digested.push(g.entity.as_str());
                    let loc = model::transform_of(world, &entity).unwrap_or_default();
                    for v in [
                        loc.pos.x,
                        loc.pos.y,
                        loc.pos.z,
                        loc.rot[0],
                        loc.rot[1],
                        loc.rot[2],
                        loc.rot[3],
                    ] {
                        w(&mut h, v);
                    }
                    let tris = model::facets_of(world, &entity)?;
                    h.write_u64(feature::digest_of(&tris));
                }
            }
        }
    }
    Some(GenStamp(h.finish()))
}

/// Derived status of one entity: never stored, always recomputed from the
/// cache, the stamp and the live world (AGENTS.md: commands own fields).
#[must_use]
pub fn status_of(world: &World, entity: &flecs_ecs::prelude::Entity) -> OpStatus {
    let Some(op) = model::operation_of(world, entity) else {
        return OpStatus::Never;
    };
    let Some(cache) = model::toolpath_of(world, entity) else {
        return OpStatus::Never;
    };
    if cache.path.is_empty() {
        return OpStatus::Never; // a cache with no moves is no cache
    }
    match (op.stamp, current_stamp(world, &op.tool, &op.src, &op.params)) {
        (Some(stored), Some(now)) if stored == now => OpStatus::Ok,
        _ => OpStatus::Stale,
    }
}

/// Gouge law's radial slack: half the geo2d round-join chord error plus
/// room for float chains; below this, a "bite" is an artifact, above it a
/// real overrun of the finish surface (see `tests/verify.rs` rationale).
const GOUGE_TOL: f64 = 0.01;
/// "Below the clearance plane" / "moved in XY" margin for the rapid law.
const Z_TOL: f64 = 1e-6;
/// Z samples for a silhouette footprint; convergence needs far fewer.
const FOOTPRINT_SAMPLES: usize = 16;
/// One micron under a grazing section plane, for the slice retry.
const SLICE_NUDGE: f64 = 1e-3;
/// Points per semicircular slot cap when building the stadium region.
const CAP_SEGMENTS: usize = 16;

/// The source geometry in world coordinates, plus its top.
struct Material {
    facets: Vec<Triangle>,
    top: f64,
}

fn material_of(world: &World, src: &SrcRef) -> Result<Material, GenError> {
    match src {
        SrcRef::Stock => named_material(world, crate::schema::STOCK_NAME, "no stock: set up stock first"),
        SrcRef::Entity(name) => {
            named_material(world, name, &format!("no entity named '{name}'"))
        }
        SrcRef::Selection(items) => {
            // Union of the distinct selected entities (stock is a legal
            // pick): what the generators stand in and what verification
            // sweeps. The region itself comes from the features.
            let mut facets = Vec::new();
            let mut top = f64::NEG_INFINITY;
            let mut seen: Vec<&str> = Vec::new();
            for g in items {
                if seen.contains(&g.entity.as_str()) {
                    continue;
                }
                seen.push(g.entity.as_str());
                let m = named_material(
                    world,
                    &g.entity,
                    &format!("the selection names '{}', which is gone", g.entity),
                )?;
                facets.extend(m.facets);
                top = top.max(m.top);
            }
            usable(facets, top, "the selection")
        }
    }
}

fn named_material(world: &World, name: &str, gone: &str) -> Result<Material, GenError> {
    let Some(entity) = model::find_by_name(world, name) else {
        return Err(GenError::Invalid(gone.to_string()));
    };
    let facets = model::world_facets(world, &entity).unwrap_or_default();
    let top = model::entity_world_aabb(world, &entity)
        .map(|b| b.max.z)
        .unwrap_or(f64::NAN);
    usable(facets, top, name)
}

/// The shared sanity gate: every vertex finite, a real top.
fn usable(facets: Vec<Triangle>, top: f64, label: &str) -> Result<Material, GenError> {
    if !facets.iter().all(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite())
        || !top.is_finite()
    {
        return Err(GenError::Invalid(format!(
            "'{label}' has no usable geometry to machine"
        )));
    }
    Ok(Material { facets, top })
}

fn from_slice(e: SliceError) -> GenError {
    match e {
        SliceError::Geo(g) => GenError::Geo(g),
        other => GenError::Invalid(other.to_string()),
    }
}

/// The section at `z`, retried a micron below when the plane grazes a
/// vertex or edge (a tangential slice is an error by contract, and a
/// 1 um shift cannot change what a cutter sees).
fn section(facets: &[Triangle], z: f64) -> Result<Region, GenError> {
    match slice::slice(facets, z) {
        Ok(r) if !r.is_empty() => Ok(r),
        Ok(_) => Err(GenError::Empty(format!("no material at z = {z:.3}"))),
        Err(e) => match slice::slice(facets, z - SLICE_NUDGE) {
            Ok(r) if !r.is_empty() => Ok(r),
            Ok(_) => Err(GenError::Empty(format!("no material at z = {z:.3}"))),
            // Both the plane and the nudged plane are unusable: the
            // original error describes the level the user asked for.
            Err(_) => Err(from_slice(e)),
        },
    }
}

/// Closed-form stadium: `a`/`b` centers, half-width `h`, semicircles of
/// `CAP_SEGMENTS` points at each end. The region only feeds verification
/// (the slot generator computes the path in closed form), and any finite
/// inputs build a simple closed ring.
fn stadium(a: Point2, b: Point2, h: f64) -> Result<Region, GenError> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if len > 1e-9 { (dx / len, dy / len) } else { (1.0, 0.0) };
    let (nx, ny) = (-uy, ux);
    let mut pts = Vec::with_capacity(2 * CAP_SEGMENTS + 2);
    for (c, t0, t1) in [(b, -90.0f64, 90.0), (a, 90.0, 270.0)] {
        for i in 0..=CAP_SEGMENTS {
            let th = (t0 + (t1 - t0) * i as f64 / CAP_SEGMENTS as f64).to_radians();
            let (ca, sa) = (th.cos(), th.sin());
            pts.push(Point2::new(c.x + h * (ca * ux + sa * nx), c.y + h * (ca * uy + sa * ny)));
        }
    }
    let loop_ = Loop::new(pts)?;
    Ok(Region::new(vec![loop_])?)
}

/// The cutting region of an operation as the world stands now - the same
/// construction `generate` runs, without the path (and without the tool,
/// which the region never consults). This is the viewport's "what gets
/// cut" outline. None when there is nothing honest to draw: a vanished
/// source or an unsliceable level shows nothing, never a stale loop
/// (Prime Directive: a wrong preview is as bad as a wrong toolpath).
#[must_use]
pub fn region_of(world: &World, op: &Operation) -> Option<Region> {
    if let SrcRef::Selection(items) = &op.src
        && !matches!(op.params, OpParams::Slot { .. })
    {
        return selection_region(world, items, &op.params).ok();
    }
    let material = material_of(world, &op.src).ok()?;
    region_for(&material, &op.params).ok()
}

/// The centerline runs `generate` would add to `region_of`'s outline for
/// this operation as the world stands now - the open chains of a Contour
/// selection, in the order they were picked. Drawn and generated through
/// this one call, so the viewport can never show a run the path does not
/// cut (or hide one it does). Unresolvable selections yield nothing, like
/// a vanished source in `region_of`.
#[must_use]
pub fn chains_of(world: &World, op: &Operation) -> Vec<Vec<Point2>> {
    let SrcRef::Selection(items) = &op.src else {
        return Vec::new();
    };
    match selection_geometry(world, items, &op.params) {
        Ok((_, chains)) => chains,
        Err(_) => Vec::new(),
    }
}

/// The source dispatch shared by `generate` and `region_of`: a selection
/// resolves its picked features, anything else slices its material, and
/// Slot keeps its closed-form stadium regardless of source (its boundary
/// is the two endpoints, not the model).
fn region_for_src(
    world: &World,
    src: &SrcRef,
    params: &OpParams,
    material: &Material,
) -> Result<Region, GenError> {
    match src {
        SrcRef::Selection(items) if !matches!(params, OpParams::Slot { .. }) => {
            selection_region(world, items, params)
        }
        _ => region_for(material, params),
    }
}

/// The per-kind region build, shared by `generate` and `region_of` so the
/// preview outline cannot disagree with what the generators cut.
fn region_for(material: &Material, params: &OpParams) -> Result<Region, GenError> {
    match params {
        OpParams::Face { .. } => {
            slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)
        }
        OpParams::Contour { top_z, .. }
        | OpParams::Pocket { top_z, .. }
        | OpParams::Trace { top_z, .. } => section(&material.facets, *top_z),
        OpParams::Slot { a, b, width, .. } => stadium(*a, *b, width * 0.5),
    }
}

/// Largest tilt from horizontal, in degrees, a Face operation accepts for
/// a selected planar face: past it the facing footprint would lie about
/// the area being cleared.
const FACE_TILT_MAX_DEG: f64 = 60.0;
/// Shoelace area below which a projected loop is a traced line rather
/// than a boundary. 1e-9 mm² is far under weld-lattice noise and far
/// above an actual face.
const DEGENERATE_AREA: f64 = 1e-9;
/// The material-side rule's threshold (M1.5.1 issue C): when the area
/// inside a chain-only Face region holds less than this fraction of the
/// material the region disagrees with - inside vs outside - the outside
/// is taken instead. 0.5 means "strictly less material inside than
/// outside": a bore rim rings air (inside ≈ 0) and machines the annulus;
/// a box-top ring holds all its material inside (ratio ≈ 1) and stays.
const OUTSIDE_BIAS: f64 = 0.5;

/// The region loops and centerline runs a feature selection resolves to,
/// as the world stands now: each picked face gives its closed boundary
/// loops, each closed chain its ring, projected to XY, coincident rims
/// counted once. An open chain is no boundary at all: for a Contour it
/// comes back in the second half - the one kind that machines a run -
/// and for a Face it runs out to the stock edge, the run acting as the
/// limit (M1.5.1 issue C). Refusals are the Prime Directive's kind - a
/// tilted face for a facing pass, an open chain for a Pocket or a Trace,
/// an open sheet, a run-out whose side cannot be told, each named in the
/// message, each with the fix stated. Never a silent drop: an unconsumed
/// run either bounds something or the message says why it cannot.
///
/// Picked open runs are stitched before this routing (M1.5.1 issue C):
/// several runs that meet end to end spell one ring, and a ring is a
/// ring however it is spelled - see `stitch_open_runs`. A facing region
/// spelled only by chains is then clipped to the material that actually
/// stands there - and taken on the far side when the picked rings bound
/// air, which is what "machine out to the stock, the run is the limit"
/// means on a bore rim (the material-side rule, [`OUTSIDE_BIAS`]).
fn selection_geometry(
    world: &World,
    items: &[GeoSel],
    params: &OpParams,
) -> Result<(Vec<Loop>, Vec<Vec<Point2>>), GenError> {
    selection_geometry_impl(world, items, params, None)
}

/// [`selection_geometry`] with a human-readable account of what the
/// selection resolved to (`notes`) accumulated on the side, so the
/// account can never disagree with the geometry - they are one walk.
fn selection_geometry_impl(
    world: &World,
    items: &[GeoSel],
    params: &OpParams,
    mut notes: Option<&mut Vec<String>>,
) -> Result<(Vec<Loop>, Vec<Vec<Point2>>), GenError> {
    let facing = matches!(params, OpParams::Face { .. });
    let pocket = matches!(params, OpParams::Pocket { .. });
    let contour = matches!(params, OpParams::Contour { .. });
    // The two silhouettes a run-out consults, computed once and shared
    // with the assembly below: the source's own footprint (the Face arm
    // of `region_for` and the verification project, so preview and path
    // agree) and the stock around it (what a run runs out to, where
    // stock exists).
    let mut material_sil = None;
    let mut envelope = None;
    let mut chain_only = false;
    if facing {
        let material = material_of(world, &SrcRef::Selection(items.to_vec()))?;
        chain_only = items.iter().all(|g| !matches!(g.feature, FeatureSel::Face(_)));
        let sil = silhouette(&material.facets);
        envelope = model::find_by_name(world, crate::schema::STOCK_NAME)
            .and_then(|e| model::world_facets(world, &e))
            .and_then(|f| slice::project(&f, FOOTPRINT_SAMPLES).ok())
            .filter(|r| !r.is_empty())
            .or_else(|| sil.clone());
        material_sil = sil;
    }
    let mut loops: Vec<Loop> = Vec::new();
    let mut chains: Vec<Vec<Point2>> = Vec::new();
    let mut strips: Vec<Loop> = Vec::new();
    let mut rims: Vec<Vec<(i64, i64)>> = Vec::new();
    let (rings, stitched) = stitch_open_runs(world, items);
    let mut stitched_runs = 0usize;
    let mut stitched_rings = 0usize;
    for (label, pts, runs) in rings {
        if runs > 1 {
            stitched_runs += runs;
            stitched_rings += 1;
        }
        push_loop(&label, pts, &mut loops, &mut rims)?;
    }
    if let Some(n) = &mut notes
        && stitched_runs > 0
    {
        n.push(format!(
            "{stitched_runs} open run{} stitched into {stitched_rings} ring{}",
            if stitched_runs == 1 { "" } else { "s" },
            if stitched_rings == 1 { "" } else { "s" },
        ));
    }
    for (gi, g) in items.iter().enumerate() {
        let Some(entity) = model::find_by_name(world, &g.entity) else {
            return Err(GenError::Invalid(format!(
                "the selection names '{}', which is gone",
                g.entity
            )));
        };
        let Some(features) = model::features_of(world, &entity) else {
            return Err(GenError::Invalid(format!(
                "'{}' has no geometry to select from",
                g.entity
            )));
        };
        let loc = model::transform_of(world, &entity).unwrap_or_default();
        let label = format!("{} of '{}'", g.feature, g.entity);
        match g.feature {
            FeatureSel::Face(fid) => {
                let Some(f) = features.faces.iter().find(|f| f.id == fid) else {
                    return Err(GenError::Invalid(format!(
                        "{label} is no longer in the model: re-pick it"
                    )));
                };
                let n = transform_point(
                    Point3::new(f.normal[0], f.normal[1], f.normal[2]),
                    Point3::ZERO,
                    loc.rot,
                );
                if facing && n.z.abs() < FACE_TILT_MAX_DEG.to_radians().cos() {
                    return Err(GenError::Invalid(format!(
                        "{label} is not horizontal - a facing pass needs a floor; machine its boundary with a 2D Contour instead"
                    )));
                }
                if f.loops.iter().all(|l| matches!(l.kind, LoopKind::Open)) {
                    return Err(GenError::Invalid(format!(
                        "{label} is an open sheet; select its edge chain instead"
                    )));
                }
                for l in &f.loops {
                    if matches!(l.kind, LoopKind::Open) {
                        continue;
                    }
                    let pts: Vec<Point2> = l
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize))
                        .map(|p| {
                            let w =
                                transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                            Point2::new(w.x, w.y)
                        })
                        .collect();
                    push_loop(&label, pts, &mut loops, &mut rims)?;
                }
            }
            FeatureSel::Chain(cid) => {
                let cs = feature::chains(&features);
                let Some(c) = cs.iter().find(|c| c.id == cid) else {
                    return Err(GenError::Invalid(format!(
                        "{label} is no longer in the model: re-pick it"
                    )));
                };
                let xy: Vec<Point2> = feature::chain_points(&features, c)
                    .iter()
                    .map(|p| {
                        let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                        Point2::new(w.x, w.y)
                    })
                    .collect();
                if !c.closed {
                    if stitched.contains(&gi) {
                        // The stitch pass closed this run's ends into a
                        // ring with its siblings; the ring is already
                        // in `loops`, routed like any closed chain.
                        continue;
                    }
                    if pocket {
                        return Err(GenError::Invalid(format!(
                            "{label} is an open run - a 2D Pocket cannot end mid-edge; select a closed chain"
                        )));
                    }
                    if contour {
                        // The run's consumer: a centerline pass along
                        // these exact points, XY like everything else
                        // here - the Z ladder is the operation's.
                        chains.push(xy);
                    } else if facing {
                        // M1.5.1 issue C: the run is the limit - face
                        // from it out to the stock edge. No stock and
                        // no source footprint means nowhere to run to,
                        // said by name rather than dropped.
                        let Some(env) = &envelope else {
                            return Err(GenError::Invalid(format!(
                                "{label} is an open run and nothing bounds the plane around it - set up stock or close the pick into a chain"
                            )));
                        };
                        strips.push(extend_run_to_footprint(
                            &label,
                            &xy,
                            env,
                            material_sil.as_ref(),
                        )?);
                    } else {
                        // Trace used to drop the run in silence; the
                        // Prime Directive's answer is a named refusal.
                        return Err(GenError::Invalid(format!(
                            "{label} is an open run - Trace machines a closed boundary; use a 2D Contour or close the pick into a chain"
                        )));
                    }
                    continue;
                }
                push_loop(&label, xy, &mut loops, &mut rims)?;
            }
            FeatureSel::Patch(pid) => {
                let Some(rings) = feature::patch_rings(&features, pid) else {
                    return Err(GenError::Invalid(format!(
                        "{label} is no longer in the model: re-pick it"
                    )));
                };
                // The Face arm's horizontal-tilt rule deliberately does not
                // apply: a patch is exactly how one asks for a wall. The
                // wall's rims go through the same seams as any closed
                // boundary - `push_loop`'s canonical-rim dedupe drops the
                // bottom rim of a vertical wall as the XY twin of the top,
                // so the region is the footprint the weld encloses, never
                // an even-odd sliver of its own skin - and the facing
                // material-side rule below turns a bore wall's rim around
                // the air it rings into the annulus the cut needs.
                let mut drew = false;
                for r in rings.iter().filter(|r| r.closed) {
                    let pts: Vec<Point2> = r
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize))
                        .map(|p| {
                            let w =
                                transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                            Point2::new(w.x, w.y)
                        })
                        .collect();
                    push_loop(&label, pts, &mut loops, &mut rims)?;
                    drew = true;
                }
                if !drew {
                    return Err(GenError::Invalid(format!(
                        "{label} bounds no closed ring - re-pick it"
                    )));
                }
            }
        }
    }
    if facing && (chain_only || !strips.is_empty()) {
        // The material-side rule, assembled once at the end so the
        // stitching, the run-outs and the clip all report through the
        // same walk: rings (or nothing), then the run-out strips, then
        // one test against the metal - if what the selection bounds
        // holds less material than what it does not, the answer is the
        // complement inside the material. A bore rim rings air, and a
        // convex rim's outward side is all air; a Face region that
        // named the void as its own answer would machine nothing,
        // which is gouging by another route.
        let mut region = if loops.is_empty() {
            Region::empty()
        } else {
            Region::new(loops.clone())?
        };
        let extended = strips.len();
        for s in strips {
            region = region.union(&Region::new(vec![s])?)?;
        }
        let mut flipped = false;
        if chain_only && let Some(mat) = &material_sil {
            let keep = region.intersect(mat)?.area().abs();
            let whole = mat.area().abs();
            if keep < OUTSIDE_BIAS * (whole - keep) {
                region = mat.difference(&region)?;
                flipped = true;
            } else {
                region = region.intersect(mat)?;
            }
        }
        if let Some(n) = &mut notes {
            if extended > 0 {
                n.push(format!(
                    "{extended} open run{} extended to the stock boundary",
                    if extended == 1 { "" } else { "s" },
                ));
            }
            if flipped {
                n.push("machining outside the run - the selection bounds air".into());
            }
        }
        loops = region.loops().to_vec();
    }
    Ok((loops, chains))
}

/// The region half of `selection_geometry` in the shape `region_of` and
/// every non-contour generator wants: bounds, or a named refusal - never
/// a silently empty area. A Contour of pure open runs is the case that
/// skips this and reads `selection_geometry` directly.
fn selection_region(
    world: &World,
    items: &[GeoSel],
    params: &OpParams,
) -> Result<Region, GenError> {
    let (loops, _) = selection_geometry(world, items, params)?;
    if loops.is_empty() {
        return Err(GenError::Empty(
            "nothing in the selection bounds an area".into(),
        ));
    }
    Region::new(loops).map_err(Into::into)
}

/// A human-readable account of what a feature selection machines beyond
/// the bare pick list: runs stitched into rings, runs extended out to
/// the stock boundary, and a region taken on the far side of its rings
/// because the rings bound air. Computed on read from the very walk the
/// region takes - the account cannot disagree with the geometry, and a
/// selection that refuses says so by owning nothing.
///
/// Deliberately *not* a persisted `verify::Warning` variant: those names
/// are the v4 project-file contract, and this is derived state, not a
/// verification finding.
#[must_use]
pub fn selection_notes(world: &World, items: &[GeoSel], params: &OpParams) -> Vec<String> {
    let mut notes = Vec::new();
    match selection_geometry_impl(world, items, params, Some(&mut notes)) {
        Ok(_) => notes,
        // The refusal itself is `region_of`'s and generation's to make,
        // with detail; notes stay empty rather than contradict it.
        Err(_) => notes,
    }
}

/// The footprint a piece of material projects to, XY: the same
/// `slice::project` the Face arm of `region_for` and the generation
/// verifier run, so a preview that consults it and the path it previews
/// agree by construction (M1.5.1: a run-out strip is clipped to this).
/// A model with height sweeps to a silhouette with its through-holes
/// intact; a zero-thickness sheet has no Z span to section, so its
/// facets' planar union is its footprint instead.
fn silhouette(facets: &[Triangle]) -> Option<Region> {
    if let Ok(r) = slice::project(facets, FOOTPRINT_SAMPLES)
        && !r.is_empty()
    {
        return Some(r);
    }
    let loops: Vec<Loop> = facets
        .iter()
        .filter_map(|t| {
            Loop::new(vec![
                Point2::new(t.a.x, t.a.y),
                Point2::new(t.b.x, t.b.y),
                Point2::new(t.c.x, t.c.y),
            ])
            .ok()
            .filter(|l| l.area().abs() > DEGENERATE_AREA)
        })
        .collect();
    if loops.is_empty() {
        return None;
    }
    Region::new(loops)
        .ok()
        .filter(|r| !r.is_empty())
}

/// The first point and edge index where ray `o + t·d` (t > 0) hits a
/// polygon ring; `None` when it misses (or runs along the ring without
/// reaching it). The run-out's geometric question, verbatim.
fn ray_hit(ring: &[Point2], o: Point2, d: Point2) -> Option<(Point2, usize)> {
    let mut best: Option<(f64, usize)> = None;
    let n = ring.len();
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let e = Point2::new(b.x - a.x, b.y - a.y);
        let den = d.x * e.y - d.y * e.x;
        if den.abs() < 1e-12 {
            continue; // parallel; an overlapping collinear hit is the ambiguity's own job to name
        }
        let oa = Point2::new(a.x - o.x, a.y - o.y);
        let t = (oa.x * e.y - oa.y * e.x) / den;
        let s = (oa.x * d.y - oa.y * d.x) / den;
        if t > 1e-9 && (0.0..=1.0).contains(&s) && best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, i));
        }
    }
    let (t, i) = best?;
    Some((Point2::new(o.x + t * d.x, o.y + t * d.y), i))
}

/// Walk `ring` from the edge index+hit of one point to that of another,
/// forward along the ring's own order or back against it. Returns the
/// strict interior vertices, then the destination point - what a
/// closure needs between the two ray hits. `None` on a degenerate ring.
fn boundary_path(
    ring: &[Point2],
    from: (usize, Point2),
    to: (usize, Point2),
    forward: bool,
) -> Option<Vec<Point2>> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let mut out = Vec::new();
    if from.0 == to.0 {
        // Same edge: forward = the whole ring from here and back to the
        // destination; reverse = nothing between (the direct hop).
        if forward {
            for k in 1..=n {
                out.push(ring[(from.0 + k) % n]);
            }
        }
        out.push(to.1);
        return Some(out);
    }
    if forward {
        let mut i = (from.0 + 1) % n;
        loop {
            out.push(ring[i]);
            if i == to.0 {
                break;
            }
            i = (i + 1) % n;
        }
    } else {
        // Backward: the destination sits mid-edge, entered against the
        // ring's order from that edge's END vertex - walking to the one
        // before it would step past the hit and force the closing edge
        // to jump back over the interior.
        let mut i = from.0;
        loop {
            out.push(ring[i]);
            if i == (to.0 + 1) % n {
                break;
            }
            i = (i + n - 1) % n;
        }
    }
    out.push(to.1);
    Some(out)
}

/// Close one open run into a boundary by casting each free end straight
/// out to the envelope's outer ring and walking that ring between the
/// hits (M1.5.1 issue C: "an open loop should just let us generate to
/// the outside of the stock box - the open loop selected should act as
/// a limit"). The run is first oriented so the chord-closing shoelace is
/// positive, which pins the outward normal (a 90° clockwise turn of the
/// travel direction) to the material-free side at each end; the walk
/// direction that keeps the strip on that side is chosen by material
/// overlap when a footprint is known, shortest boundary otherwise, with
/// the forward walk winning ties - and candidates that cross into the
/// run's own chord lens are out of the race, because the run is a limit.
/// Every failure mode - no stock ahead of an end, a self-crossing strip,
/// a zero-area strip - is the named ambiguity, never a silent skip.
fn extend_run_to_footprint(
    label: &str,
    run: &[Point2],
    envelope: &Region,
    material: Option<&Region>,
) -> Result<Loop, GenError> {
    let ambiguous = || {
        GenError::Invalid(format!(
            "{label} and the selection geometry is ambiguous; use a closed chain or a Face pick"
        ))
    };
    let Some(outer) = envelope
        .loops()
        .iter()
        .max_by(|a, b| total_cmp_signed(a.area(), b.area()))
    else {
        return Err(ambiguous());
    };
    let ring = outer.points();
    if ring.len() < 3 || run.len() < 2 {
        return Err(ambiguous());
    }
    // Orient: shoelace over run + chord. A straight run shoelaces to ~0
    // and keeps its order; the normal choice is still deterministic.
    let mut twice = 0.0f64;
    let m = run.len();
    for i in 0..m {
        let a = run[i];
        let b = run[(i + 1) % m];
        twice += a.x * b.y - b.x * a.y;
    }
    let pts: Vec<Point2> = if twice < 0.0 {
        run.iter().rev().copied().collect()
    } else {
        run.to_vec()
    };
    // Outward = clockwise turn of travel at each free end: (d.y, -d.x).
    let d0 = Point2::new(pts[1].x - pts[0].x, pts[1].y - pts[0].y);
    let d1 = Point2::new(pts[m - 1].x - pts[m - 2].x, pts[m - 1].y - pts[m - 2].y);
    let n0 = Point2::new(d0.y, -d0.x);
    let n1 = Point2::new(d1.y, -d1.x);
    // A straight run laces to nothing, so "away from the bulge" names
    // no side - both normals are tried and the overlap score picks the
    // material side. A curved run's chord-CCW orientation already fixed
    // one honest outward direction; trying the other would let a run
    // that rings material (a bore rim) claim the far half of itself.
    let straight = twice.abs() < DEGENERATE_AREA;
    // The lens the run closes against its own chord: material or air on
    // the run's concave side. A strip that reaches into it crosses the
    // run - and "the open loop acts as a limit" forbids crossing. One
    // probe per segment, a hair left of travel (the chord-CCW
    // orientation above puts lens interior on the left); any candidate
    // containing one is out of the race, so overlap can no longer be
    // gamed by wrapping the whole envelope behind the run's back. A
    // straight run closes no lens: pure overlap decides, as scribbled.
    let mut probes: Vec<Point2> = Vec::new();
    if !straight {
        for i in 0..m - 1 {
            let (a, b) = (pts[i], pts[i + 1]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len = (dx * dx + dy * dy).sqrt();
            if len <= f64::EPSILON {
                continue;
            }
            let eps = len * 1e-3;
            probes.push(Point2::new(
                (a.x + b.x) * 0.5 - dy / len * eps,
                (a.y + b.y) * 0.5 + dx / len * eps,
            ));
        }
    }
    let mut candidates: Vec<(f64, f64, usize, Loop)> = Vec::new();
    let sides: &[f64] = if straight { &[1.0, -1.0] } else { &[1.0] };
    for (si, &sgn) in sides.iter().enumerate() {
        let Some(hit_start) = ray_hit(ring, pts[0], Point2::new(n0.x * sgn, n0.y * sgn)) else {
            continue;
        };
        let Some(hit_end) = ray_hit(ring, pts[m - 1], Point2::new(n1.x * sgn, n1.y * sgn)) else {
            continue;
        };
        for &forward in [true, false].iter() {
            let Some(between) = boundary_path(
                ring,
                (hit_end.1, hit_end.0),
                (hit_start.1, hit_start.0),
                forward,
            ) else {
                continue;
            };
            let mut closed = pts.clone();
            closed.push(hit_end.0);
            closed.extend(between.iter().copied());
            let Ok(loop_) = Loop::new(closed) else {
                continue;
            };
            if loop_.area().abs() < DEGENERATE_AREA {
                continue;
            }
            // Self-crossing is the ambiguity guard: a crossing loop is
            // no boundary, and `Region::new` is the one that says so.
            let Ok(region) = Region::new(vec![loop_.clone()]) else {
                continue;
            };
            if probes.iter().any(|p| region.contains(*p)) {
                continue;
            }
            // More material inside the strip than less - the side that
            // machines metal, not air.
            let overlap = match material {
                Some(mat) => match region.intersect(mat) {
                    Ok(r) => r.area().abs(),
                    Err(_) => continue,
                },
                None => 0.0,
            };
            // Then the shorter boundary walk: the corridor hugging the
            // run's normal side beats the candidate that wraps the
            // whole envelope, and with no footprint to consult this is
            // the whole decision.
            let mut walk = 0.0;
            let mut prev = hit_end.0;
            for &p in &between {
                walk += ((p.x - prev.x).powi(2) + (p.y - prev.y).powi(2)).sqrt();
                prev = p;
            }
            candidates.push((overlap, -walk, si, loop_));
        }
    }
    // Highest score wins; ties break to the shorter walk, then to the
    // first side tried, so the answer is a pure function of the pick.
    match candidates
        .into_iter()
        .max_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then(a.1.total_cmp(&b.1))
                .then(a.2.cmp(&b.2))
        })
    {
        Some((_, _, _, loop_)) => Ok(loop_),
        None => Err(ambiguous()),
    }
}

/// Order two signed areas by magnitude - the outer loops of a region
/// are the big positive ones; a stray negative inside an `outer` pick
/// should never win.
fn total_cmp_signed(a: f64, b: f64) -> std::cmp::Ordering {
    a.abs().total_cmp(&b.abs())
}

/// The 1e6 lattice `push_loop` keys rims on, in one place so the
/// stitcher's endpoint matches and the dedupe agree to the nanometre.
fn qkey(p: &Point2) -> (i64, i64) {
    (
        (p.x * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
        (p.y * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
    )
}

/// One distinct picked open run in the shape the stitcher works in:
/// world XY points with the entity's transform applied (where
/// `push_loop` expects every boundary to live), its quantized endpoints,
/// and every item index that picked it - the same chain twice is one
/// run picked twice.
struct Run {
    items: Vec<usize>,
    entity: String,
    cid: u64,
    pts: Vec<Point2>,
    start: (i64, i64),
    end: (i64, i64),
}

/// What the stitcher hands back: rings composed from picked runs (label,
/// points, run count) and the item indices those rings consumed.
type Stitched = (Vec<(String, Vec<Point2>, usize)>, HashSet<usize>);

/// M1.5.1 issue C: tessellation - STEP adaptive chords especially - can
/// break a geometrically closed rim into several chains, each stopping
/// where a chord's turn exceeds the merge angle. The user picks them
/// all, and to them it is obviously one loop. So before routing, stitch
/// the picked open runs of each entity end to end and return every walk
/// that closes back onto its start as one ring, with its contributing
/// picks marked consumed so routing treats them as the closed chain
/// they describe.
///
/// Junctions match either endpoint of a candidate because a loop's runs
/// are stored with whatever direction their edge orientations allow -
/// across the rim of a split circle the two halves meet end-to-end on
/// one side and start-to-start on the other, so a flipped continuation
/// IS the ring, not a corruption of it. Committed rings are shoelace-
/// canonicalized to CCW so pick order can never flip the winding. A
/// doubled pick is deduped by chain id - the same segments twice close
/// no ring, they close a zero-area spike.
///
/// Deliberately conservative: only picked runs take part (nothing
/// unpicked is pulled in), cross-entity rings are not attempted (a
/// recorded limit), a greedy fork that dead-ends commits nothing, and
/// runs that do not close route exactly as they always did. Failed
/// lookups are not errors here; the routing loop owns those refusals
/// and will name them.
fn stitch_open_runs(world: &World, items: &[GeoSel]) -> Stitched {
    let mut runs: Vec<Run> = Vec::new();
    for (i, g) in items.iter().enumerate() {
        let FeatureSel::Chain(cid) = g.feature else {
            continue;
        };
        if let Some(r) = runs.iter_mut().find(|r| r.entity == g.entity && r.cid == cid) {
            r.items.push(i);
            continue;
        }
        let Some(entity) = model::find_by_name(world, &g.entity) else {
            continue;
        };
        let Some(features) = model::features_of(world, &entity) else {
            continue;
        };
        let Some(c) = feature::chains(&features).into_iter().find(|c| c.id == cid) else {
            continue;
        };
        if c.closed {
            continue;
        }
        let loc = model::transform_of(world, &entity).unwrap_or_default();
        let pts: Vec<Point2> = feature::chain_points(&features, &c)
            .iter()
            .map(|p| {
                let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
                Point2::new(w.x, w.y)
            })
            .collect();
        let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) else {
            continue;
        };
        runs.push(Run {
            items: vec![i],
            entity: g.entity.clone(),
            cid,
            pts,
            start: qkey(&first),
            end: qkey(&last),
        });
    }
    let used = |r: &Run, consumed: &HashSet<usize>| r.items.iter().any(|it| consumed.contains(it));
    // Each committed ring carries the count of runs that spelled it, so
    // `selection_notes` can say what the stitching did.
    let mut rings: Vec<(String, Vec<Point2>, usize)> = Vec::new();
    let mut consumed: HashSet<usize> = HashSet::new();
    for s in 0..runs.len() {
        if used(&runs[s], &consumed) {
            continue;
        }
        // Walk the cycle through run `s` forward: `anchor` is where we
        // started, `far` the junction to extend from. `in_seq` is why a
        // walk can never loop.
        let anchor = runs[s].start;
        let mut far = runs[s].end;
        let mut seq: Vec<(usize, bool)> = vec![(s, false)];
        let mut in_seq = vec![false; runs.len()];
        in_seq[s] = true;
        let closed = loop {
            if far == anchor {
                break true;
            }
            let mut next: Option<(usize, bool)> = None;
            for (j, r) in runs.iter().enumerate() {
                if in_seq[j] || used(r, &consumed) || r.entity != runs[s].entity {
                    continue;
                }
                if r.start == far {
                    next = Some((j, false));
                    break;
                }
                if r.end == far {
                    next = Some((j, true));
                    break;
                }
            }
            let Some((j, rev)) = next else { break false };
            in_seq[j] = true;
            seq.push((j, rev));
            far = if rev { runs[j].start } else { runs[j].end };
        };
        if !closed {
            continue;
        }
        // Concatenate the walk, flipping what the junctions asked for.
        // Shared seam vertices arrive as consecutive repeats and
        // closing wraps - `push_loop` and `Loop::new` drop exactly
        // those, so the ring needs no special handling here.
        let mut pts: Vec<Point2> = Vec::new();
        for (j, rev) in &seq {
            for &it in &runs[*j].items {
                consumed.insert(it);
            }
            if *rev {
                pts.extend(runs[*j].pts.iter().rev().copied());
            } else {
                pts.extend(runs[*j].pts.iter().copied());
            }
        }
        let mut twice = 0.0;
        let n = pts.len();
        for i in 0..n.saturating_sub(1) {
            let (a, b) = (pts[i], pts[i + 1]);
            twice += a.x * b.y - a.y * b.x;
        }
        if twice < 0.0 {
            pts.reverse();
        }
        rings.push((
            format!("the stitched rim of '{}'", runs[s].entity),
            pts,
            seq.len(),
        ));
    }
    (rings, consumed)
}

/// Accept one selected boundary: refuse what collapses to a line seen
/// from above, and count a rim already in the region once - two faces
/// meeting at an edge share it, and a coincident double loop would be a
/// zero-wide sliver.
fn push_loop(
    label: &str,
    pts: Vec<Point2>,
    loops: &mut Vec<Loop>,
    rims: &mut Vec<Vec<(i64, i64)>>,
) -> Result<(), GenError> {
    let mut key: Vec<(i64, i64)> = pts.iter().map(qkey).collect();
    // Same drop Loop::new does - consecutive repeats and the closing
    // wrap - so the collapse check below sees the ring the loop would
    // actually be.
    key.dedup();
    if key.len() > 1 && key.first() == key.last() {
        key.pop();
    }
    if key.len() < 3 {
        // A vertical wall seen from above, or a chain that runs up one:
        // nothing encloses.
        return Err(GenError::Invalid(format!(
            "{label} projects to a line from above and bounds no area; select a horizontal face or a closed edge chain around the part"
        )));
    }
    let canon = canon_ring(&key);
    if rims.contains(&canon) {
        return Ok(());
    }
    let loop_ = Loop::new(pts)?;
    if loop_.area().abs() < DEGENERATE_AREA {
        return Err(GenError::Invalid(format!(
            "{label} projects to a line from above and bounds no area; select a horizontal face or a closed edge chain around the part"
        )));
    }
    rims.push(canon);
    loops.push(loop_);
    Ok(())
}

/// Rotate- and mirror-canonical form of a quantized ring: the
/// lexicographic minimum over its 2n walks. Compared exactly, never by
/// hash - a colliding dedupe key would silently drop a real boundary.
fn canon_ring(ring: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let n = ring.len();
    if n == 0 {
        return Vec::new();
    }
    let mut best: Option<Vec<(i64, i64)>> = None;
    for start in 0..n {
        for step in [1usize, 2] {
            let walk: Vec<(i64, i64)> = (0..n).map(|k| ring[(start + k * step) % n]).collect();
            if best.as_ref().is_none_or(|b| walk < *b) {
                best = Some(walk);
            }
        }
    }
    best.unwrap_or_default()
}

/// Run one generated path after another. Both start and end on the
/// clearance plane, so the joint is a clearance rapid: append and re-anchor
/// the added path's opening `from` to the previous move's `to` - a stale
/// `(0, 0)` there would read as a teleport to the origin.
fn stitch(mut a: Toolpath, mut b: Toolpath) -> Toolpath {
    if let (Some(to), Some(first)) = (a.moves.last().map(|m| m.to), b.moves.first_mut()) {
        first.from = to;
    }
    a.moves.append(&mut b.moves);
    a
}

/// Generate the path for an operation as the world stands now, and verify
/// it. Reads only; the caller stores whatever comes back. The error class
/// is `GenError` for both setup problems (missing tool/stock, unusable
/// geometry) and generator refusals - commands surface them at
/// construction, so nothing half-generated reaches the world.
pub fn generate(
    world: &World,
    op: &Operation,
) -> Result<(Toolpath, Vec<Warning>), GenError> {
    // Tool: the diameter drives the path, the body drives the reach law.
    let Some(tool_entity) = model::find_by_name(world, &op.tool) else {
        return Err(GenError::Invalid(format!("no tool named '{}'", op.tool)));
    };
    let Some(tool) = model::tool_of(world, &tool_entity) else {
        return Err(GenError::Invalid(format!("'{}' is not a tool", op.tool)));
    };
    let d = tool.diameter;

    // Source material (the slot reads its own endpoints instead, but the
    // verification still wants to know what stands around it).
    let material = material_of(world, &op.src)?;
    // Setup stock, when it is not the source itself: the swarf column
    // around a smaller model. Island-vs-swarf in the reach law keys on
    // `region.contains`, so this only widens what counts as material.
    let stock_region = match &op.src {
        SrcRef::Stock => None,
        SrcRef::Entity(_) | SrcRef::Selection(_) => model::find_by_name(world, crate::schema::STOCK_NAME)
            .and_then(|e| model::world_facets(world, &e))
            .and_then(|f| slice::project(&f, FOOTPRINT_SAMPLES).ok())
            .filter(|r| !r.is_empty()),
    };
    let stock_top_z = material
        .top
        .max(model::stock_world_aabb(world).map(|b| b.max.z).unwrap_or(f64::NEG_INFINITY));

    let mode = match op.params.kind() {
        OpType::Face | OpType::Contour => Mode::Boundary,
        OpType::Pocket | OpType::Slot => Mode::Area,
        OpType::Trace => Mode::Engrave,
    };

    // Region + path per kind.
    let (region, path) = match &op.params {
        OpParams::Face { stepover, clearance_z, cut_z, feed, plunge_feed, climb } => {
            let region = region_for_src(world, &op.src, &op.params, &material)?;
            let path = machining2d::face(
                &region,
                &FaceParams {
                    tool_diameter: d,
                    stepover: *stepover,
                    clearance_z: *clearance_z,
                    cut_z: *cut_z,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            (region, path)
        }
        OpParams::Contour {
            stock_to_leave,
            clearance_z,
            retract_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            lead,
            tolerance,
            tab_count,
            tab_width,
            tab_height,
        } => {
            let cp = ContourParams {
                tool_diameter: d,
                stock_to_leave: *stock_to_leave,
                clearance_z: *clearance_z,
                retract_z: *retract_z,
                top_z: *top_z,
                bottom_z: *bottom_z,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                lead: *lead,
                tolerance: *tolerance,
                tab_count: *tab_count,
                tab_width: *tab_width,
                tab_height: *tab_height,
            };
            let (region, path) = match &op.src {
                SrcRef::Selection(items) => {
                    // The boundary path cuts the loops the picks bound;
                    // each open run in the picks adds a centerline pass
                    // along its own points. Either half alone is enough;
                    // together they machine the whole pick list.
                    let (loops, runs) = selection_geometry(world, items, &op.params)?;
                    if loops.is_empty() && runs.is_empty() {
                        return Err(GenError::Empty(
                            "nothing in the selection bounds an area or a run".into(),
                        ));
                    }
                    let region = if loops.is_empty() {
                        Region::empty()
                    } else {
                        Region::new(loops)?
                    };
                    let mut path = if region.is_empty() {
                        Toolpath::new(Vec::new())
                    } else {
                        machining2d::contour(&region, &cp)?
                    };
                    for run in &runs {
                        path = stitch(path, machining2d::contour_along_chain(run, &cp)?);
                    }
                    (region, path)
                }
                _ => {
                    let region = region_for_src(world, &op.src, &op.params, &material)?;
                    let path = machining2d::contour(&region, &cp)?;
                    (region, path)
                }
            };
            (region, path)
        }
        OpParams::Pocket {
            stepover,
            stock_to_leave,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
        } => {
            let region = region_for_src(world, &op.src, &op.params, &material)?;
            let path = machining2d::pocket(
                &region,
                &PocketParams {
                    tool_diameter: d,
                    stepover: *stepover,
                    stock_to_leave: *stock_to_leave,
                    clearance_z: *clearance_z,
                    top_z: *top_z,
                    bottom_z: *bottom_z,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                    strategy: *strategy,
                },
            )?;
            (region, path)
        }
        OpParams::Slot {
            a,
            b,
            width,
            stock_to_leave,
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            stepover,
            feed,
            plunge_feed,
            climb,
        } => {
            let path = machining2d::slot(
                *a,
                *b,
                &SlotParams {
                    tool_diameter: d,
                    width: *width,
                    stock_to_leave: *stock_to_leave,
                    clearance_z: *clearance_z,
                    top_z: *top_z,
                    bottom_z: *bottom_z,
                    stepdown: *stepdown,
                    stepover: *stepover,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            let region = region_for_src(world, &op.src, &op.params, &material)?;
            (region, path)
        }
        OpParams::Trace {
            clearance_z,
            top_z,
            bottom_z,
            stepdown,
            feed,
            plunge_feed,
            climb,
        } => {
            let region = region_for_src(world, &op.src, &op.params, &material)?;
            let path = machining2d::trace(
                &region,
                &TraceParams {
                    clearance_z: *clearance_z,
                    top_z: *top_z,
                    bottom_z: *bottom_z,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            (region, path)
        }
    };

    let geom = tool.geom();
    let warns = verify::verify(
        &path,
        &VerifyContext {
            region: &region,
            stock: stock_region.as_ref(),
            tool: &geom,
            mode,
            stock_top_z,
            clearance_z: op.params.clearance_z(),
            stock_to_leave: match &op.params {
                OpParams::Face { .. } | OpParams::Trace { .. } => 0.0,
                OpParams::Contour { stock_to_leave, .. }
                | OpParams::Pocket { stock_to_leave, .. }
                | OpParams::Slot { stock_to_leave, .. } => *stock_to_leave,
            },
            gouge_tol: GOUGE_TOL,
            z_tol: Z_TOL,
        },
    );
    Ok((path, warns))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Point2;

    #[test]
    fn stadium_is_a_closed_simple_ring() {
        let r = stadium(Point2::new(0.0, 0.0), Point2::new(50.0, 0.0), 10.0).expect("finite");
        assert_eq!(r.loops().len(), 1);
        // The center and both caps' extents are inside; a point beyond the
        // cap is not.
        assert!(r.contains(Point2::new(25.0, 0.0)));
        assert!(r.contains(Point2::new(59.9, 0.0)));
        assert!(!r.contains(Point2::new(60.5, 0.0)));
        // The caps reach 10 mm past each endpoint, so just inside a cap is
        // in, and a point past its arc is out.
        assert!(r.contains(Point2::new(-9.5, 0.0)));
        assert!(!r.contains(Point2::new(-10.5, 0.0)));
    }

    #[test]
    fn zero_length_stadium_is_a_circle() {
        let r = stadium(Point2::new(1.0, 2.0), Point2::new(1.0, 2.0), 5.0).expect("finite");
        assert!(r.contains(Point2::new(1.0, 2.0)));
        assert!(!r.contains(Point2::new(1.0 + 5.5, 2.0)));
    }

    #[test]
    fn params_sanity_gates() {
        let ok = OpParams::Face {
            stepover: 8.0,
            clearance_z: 5.0,
            cut_z: -0.5,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        assert!(ok.is_sane());
        let bad = OpParams::Pocket {
            stepover: f64::NAN,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            top_z: 0.0,
            bottom_z: -4.0,
            stepdown: 2.0,
            feed: 400.0,
            plunge_feed: 80.0,
            climb: true,
            strategy: PocketStrategy::Zigzag,
        };
        assert!(!bad.is_sane());
        // A depth range that runs upward is nonsense, refuse it early.
        let up = OpParams::Trace {
            clearance_z: 5.0,
            top_z: -4.0,
            bottom_z: 0.0,
            stepdown: 1.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        assert!(!up.is_sane());
    }

    #[test]
    fn stamp_follows_every_input() {
        // Tool numerics, params and source must each move the digest; the
        // world pieces are exercised in tests/operations.rs, the pure
        // param path is deterministic here.
        let a = OpParams::Face {
            stepover: 8.0,
            clearance_z: 5.0,
            cut_z: -0.5,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        // (No struct-update syntax on enum variants: write it out.)
        let b = OpParams::Face {
            stepover: 8.0,
            clearance_z: 5.0,
            cut_z: -0.6,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        };
        let mut ha = std::hash::DefaultHasher::new();
        let mut hb = std::hash::DefaultHasher::new();
        a.stamp_with(&mut ha);
        b.stamp_with(&mut hb);
        assert_ne!(ha.finish(), hb.finish());
    }
}
