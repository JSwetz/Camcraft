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

use std::hash::Hasher;

use flecs_ecs::prelude::World;

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Triangle;
use crate::geometry::slice::{self, SliceError};
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
    let name = match src {
        SrcRef::Stock => crate::schema::STOCK_NAME,
        SrcRef::Entity(name) => name.as_str(),
    };
    let Some(entity) = model::find_by_name(world, name) else {
        return Err(GenError::Invalid(match src {
            SrcRef::Stock => "no stock: set up stock first".into(),
            SrcRef::Entity(_) => format!("no entity named '{name}'"),
        }));
    };
    let facets = model::world_facets(world, &entity).unwrap_or_default();
    let top = model::entity_world_aabb(world, &entity)
        .map(|b| b.max.z)
        .unwrap_or(f64::NAN);
    if !facets.iter().all(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite())
        || !top.is_finite()
    {
        return Err(GenError::Invalid(format!(
            "'{name}' has no usable geometry to machine"
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
        SrcRef::Entity(_) => model::find_by_name(world, crate::schema::STOCK_NAME)
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
            let region = slice::project(&material.facets, FOOTPRINT_SAMPLES)
                .map_err(from_slice)?;
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
            let region = section(&material.facets, *top_z)?;
            let path = machining2d::contour(
                &region,
                &ContourParams {
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
                },
            )?;
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
            let region = section(&material.facets, *top_z)?;
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
            let region = stadium(*a, *b, width * 0.5)?;
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
            let region = section(&material.facets, *top_z)?;
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
