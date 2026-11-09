//! The material an operation machines: the world-transformed source
//! facets and their top (`material_of`), the hole lists read from them,
//! the section/stadium builders and the shared height refusals. The
//! region builds live in `material_region` and `material_source`, split
//! along the domain seam (400-line law) and re-exported here.
//!
//! Provenance: our own domain model - specced from public documentation of
//! commercial CAM workflows; no vendor source was consulted.

use super::*;

pub use super::material_region::*;
pub(super) use super::material_source::*;

/// The height expressions of a params set (the `for_each_height` soup
/// collected once, for scanning).
pub(super) fn collect_heights(params: &OpParams) -> Vec<Height> {
    let mut out = Vec::new();
    params.for_each_height(|h| out.push(h));
    out
}

/// Gouge law's radial slack: half the geo2d round-join chord error plus
/// room for float chains; below this, a "bite" is an artifact, above it a
/// real overrun of the finish surface (see `tests/verify.rs` rationale).
pub(crate) const GOUGE_TOL: f64 = 0.01;
/// "Below the clearance plane" / "moved in XY" margin for the rapid law.
pub(super) const Z_TOL: f64 = 1e-6;
/// Z samples for a silhouette footprint; convergence needs far fewer.
pub(super) const FOOTPRINT_SAMPLES: usize = 16;
/// One micron under a grazing section plane, for the slice retry.
pub(super) const SLICE_NUDGE: f64 = 1e-3;
/// Points per semicircular slot cap when building the stadium region.
pub(super) const CAP_SEGMENTS: usize = 16;

/// The source geometry in world coordinates, plus its top.
pub(super) struct Material {
    pub(super) facets: Vec<Triangle>,
    pub(super) top: f64,
    /// The per-entity index ranges of `facets` when the source is a
    /// selection across several models (M13.11): the depth law and the
    /// verify read a plane as the 2D union of the bodies' own sections,
    /// so two models butted face to face are one solid, not a
    /// non-manifold refusal. Empty for one body - the plain read.
    pub(super) bodies: Vec<std::ops::Range<usize>>,
}

/// The world facets an operation's generator stands in - exactly
/// `material_of`'s read, for a consumer outside the generator that
/// must re-derive what the generator derived from them (the stock
/// sim's lathe frame, #19: the turning kernels fit their axis here).
pub(crate) fn source_facets(world: &World, setup: Option<&str>, src: &SrcRef) -> Result<Vec<Triangle>, String> {
    material_of(world, setup, src).map(|m| m.facets).map_err(|e| e.to_string())
}

pub(super) fn material_of(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
) -> Result<Material, GenError> {
    match src {
        SrcRef::Stock => {
            let name = crate::setup::stock_entity_name(world, setup);
            named_material(world, setup, &name, "no stock: set it up in Setup first")
        }
        SrcRef::Entity(name) => named_material(world, setup, name, &format!("no entity named '{name}'")),
        SrcRef::Selection(items) => {
            // Union of the distinct selected entities (stock is a legal
            // pick): what the generators stand in and what verification
            // sweeps. The region itself comes from the features.
            let mut facets = Vec::new();
            let mut top = f64::NEG_INFINITY;
            let mut seen: Vec<&str> = Vec::new();
            let mut bodies = Vec::new();
            for g in items {
                if seen.contains(&g.entity.as_str()) {
                    continue;
                }
                seen.push(g.entity.as_str());
                let m = named_material(
                    world,
                    setup,
                    &g.entity,
                    &format!("the selection names '{}', which is gone", g.entity),
                )?;
                let start = facets.len();
                facets.extend(m.facets);
                if facets.len() > start {
                    bodies.push(start..facets.len());
                }
                top = top.max(m.top);
            }
            let mut m = usable(facets, top, "the selection")?;
            // Two or more bodies: each is read as its own solid and the
            // planes are united; one body keeps the plain read.
            if bodies.len() > 1 {
                m.bodies = bodies;
            }
            Ok(m)
        }
    }
}

/// The hole list of one source, for the panel's Detect holes button:
/// the SAME world-transformed facets generation machines, so what is
/// detected is what will be drilled - a local-frame list would drift
/// from a moved part. A pure read; the caller (the UI, through a
/// command) decides what to do with the list.
pub fn holes_of(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
) -> Result<Vec<crate::drilling::Hole>, GenError> {
    let m = material_of(world, setup, src)?;
    Ok(crate::drilling::recognize(&m.facets))
}

/// The radial hole list of one source, for Detect holes on a document
/// that holds a turret (M5.2 F2): the same world-transformed facets,
/// read a second way - closed cylindrical walls whose AXIS runs at
/// the spindle, found by the same weld law the Z pass runs.
pub fn cross_holes_of(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
) -> Result<Vec<crate::drilling::CrossHole>, GenError> {
    let m = material_of(world, setup, src)?;
    Ok(crate::drilling::recognize_cross(&m.facets))
}

/// Detect holes (M2.4 C; shared with the MCP server by #312): `params`
/// with its source's holes, read NOW, written into whichever hole list
/// the kind has - the drill, the boring ride and the thread mill store
/// the same `holes` shape - plus the drill's radial list when the
/// document's machine holds a turret (M5.2 F2; a plain mill asks nothing
/// of the mesh's sides). `name` only words the refusals. The caller
/// commits the result as one undoable `EditOperationParams`, so the
/// app's Detect holes button and the MCP `detect_holes` tool store the
/// same list.
pub fn detect_holes(
    world: &World,
    name: &str,
    params: &OpParams,
    setup: Option<&str>,
    src: &SrcRef,
) -> Result<OpParams, String> {
    let mut params = params.clone();
    let is_drill = matches!(params, OpParams::Drill { .. });
    let holes: &mut Vec<_> = match &mut params {
        OpParams::Drill { holes, .. } | OpParams::Bore { holes, .. } | OpParams::Thread { holes, .. } => holes,
        _ => return Err(format!("'{name}' is not a drilling operation")),
    };
    let axial = holes_of(world, setup, src).map_err(|e| e.to_string())?;
    let radial = if !is_drill || crate::machine::kind_of(world) == crate::machine::Kind::Mill {
        Vec::new()
    } else {
        cross_holes_of(world, setup, src).map_err(|e| e.to_string())?
    };
    if axial.is_empty() && radial.is_empty() {
        return Err(if is_drill {
            "no drilled holes found in the source model - Z or radial".to_string()
        } else {
            "no Z-axis holes found in the source model - this ride reads only those".to_string()
        });
    }
    *holes = axial;
    if let OpParams::Drill { cross, .. } = &mut params {
        *cross = radial;
    }
    Ok(params)
}

/// A named entity's facets in its setup's frame (#326: a part of an
/// aligned setup reads turned; the stock and an unaligned setup read
/// exactly the world facets they always did).
pub(super) fn named_material(world: &World, setup: Option<&str>, name: &str, gone: &str) -> Result<Material, GenError> {
    let Some(entity) = model::find_by_name(world, name) else {
        return Err(GenError::Invalid(gone.to_string()));
    };
    let facets = crate::setup_align::facets_in(world, &entity, setup).unwrap_or_default();
    let top = crate::setup_align::entity_aabb_in(world, &entity, setup)
        .map(|b| b.max.z)
        .unwrap_or(f64::NAN);
    usable(facets, top, name)
}

/// The shared sanity gate: every vertex finite, a real top.
pub(super) fn usable(facets: Vec<Triangle>, top: f64, label: &str) -> Result<Material, GenError> {
    if !facets
        .iter()
        .all(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite())
        || !top.is_finite()
    {
        return Err(GenError::Invalid(format!(
            "'{label}' has no usable geometry to machine"
        )));
    }
    Ok(Material { facets, top, bodies: Vec::new() })
}

pub(super) fn from_slice(e: SliceError) -> GenError {
    match e {
        SliceError::Geo(g) => GenError::Geo(g),
        other => GenError::Invalid(other.to_string()),
    }
}

/// The section at `z`, retried a micron below when the plane grazes a
/// vertex or edge (a tangential slice is an error by contract, and a
/// 1 um shift cannot change what a cutter sees).
pub(super) fn section(facets: &[Triangle], z: f64) -> Result<Region, GenError> {
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
pub(super) fn stadium(a: Point2, b: Point2, h: f64) -> Result<Region, GenError> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if len > 1e-9 {
        (dx / len, dy / len)
    } else {
        (1.0, 0.0)
    };
    let (nx, ny) = (-uy, ux);
    let mut pts = Vec::with_capacity(2 * CAP_SEGMENTS + 2);
    for (c, t0, t1) in [(b, -90.0f64, 90.0), (a, 90.0, 270.0)] {
        for i in 0..=CAP_SEGMENTS {
            let th = (t0 + (t1 - t0) * i as f64 / CAP_SEGMENTS as f64).to_radians();
            let (ca, sa) = (th.cos(), th.sin());
            pts.push(Point2::new(
                c.x + h * (ca * ux + sa * nx),
                c.y + h * (ca * uy + sa * ny),
            ));
        }
    }
    let loop_ = Loop::new(pts)?;
    Ok(Region::new(vec![loop_])?)
}

/// The shared refusal phrasing for an unresolved height, named by field.
/// One source so `resolve_params`, the region builders and every command
/// error read identically.
pub fn unresolved_msg(
    what: &str,
    h: Height,
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
) -> String {
    format!(
        "{what} (From: {ref}, offset {off}) is unresolved: {why}",
        ref = h.reference,
        off = h.offset,
        why = missing_source(world, setup, src, h.reference)
            .unwrap_or("the source has no finite height"),
    )
}

pub(super) fn unresolved_err(
    what: &str,
    h: Height,
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
) -> GenError {
    GenError::Invalid(unresolved_msg(what, h, world, setup, src))
}

/// A required height slot of an already-`Ok` `ResolvedHeights` is `Some`
/// by that function's contract (per kind); the error arm exists so no
/// call site needs an unwrap.
pub(super) fn req_z(v: Option<f64>, what: &str) -> Result<f64, GenError> {
    v.ok_or_else(|| GenError::Invalid(format!("{what} is unresolved")))
}
