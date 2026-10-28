

use super::*;

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
}

pub(super) fn material_of(world: &World, setup: Option<&str>, src: &SrcRef) -> Result<Material, GenError> {
    match src {
        SrcRef::Stock => {
            let name = crate::setup::stock_entity_name(world, setup);
            named_material(world, &name, "no stock: set it up in Setup first")
        }
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

pub(super) fn named_material(world: &World, name: &str, gone: &str) -> Result<Material, GenError> {
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
pub(super) fn usable(facets: Vec<Triangle>, top: f64, label: &str) -> Result<Material, GenError> {
    if !facets.iter().all(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite())
        || !top.is_finite()
    {
        return Err(GenError::Invalid(format!(
            "'{label}' has no usable geometry to machine"
        )));
    }
    Ok(Material { facets, top })
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
    // The 3+2 index (M5.1 B): a tilted planar op draws the cutting
    // plane's own shadow - the local region through the frame's door,
    // at the plane its `region_height` names. A tilt that commands no
    // direction (only a non-finite letter could, refused at the door
    // long before here) and a picked selection under tilt (generation
    // refuses it by name) both draw nothing: a preview generate would
    // refuse is exactly the lie this function exists to avoid.
    let frame = match op.params.tilt() {
        None => None,
        Some(t) => Some(t.frame()?),
    };
    if frame.is_some() && matches!(&op.src, SrcRef::Selection(_)) {
        return None;
    }
    if let SrcRef::Selection(items) = &op.src
        && !matches!(
            op.params,
            OpParams::Slot { .. }
                | OpParams::Drill { .. }
                | OpParams::Pocket3D { .. }
                | OpParams::Finish3D { .. }
                // The drive-curve contour's picks are the PATH, not a
                // boundary: the 2D selection walk below would try to
                // bound an area with them (and refuse an open run),
                // where the honest preview is the material's shadow -
                // what the ball rides is the 3D chain itself, visible
                // once generated (M5.1 J).
                | OpParams::MultiAxisContour { .. }
        )
    {
        return selection_region(world, op.setup.as_deref(), items, &op.params).ok();
    }
    let material = material_of(world, op.setup.as_deref(), &op.src).ok()?;
    match &frame {
        None => region_for(world, op.setup.as_deref(), &material, &op.params, None).ok(),
        Some(f) => {
            let local = Material {
                facets: f.local_facets(&material.facets),
                top: material.top - f.origin.z,
            };
            let local = region_for(world, op.setup.as_deref(), &local, &op.params, Some(f)).ok()?;
            let zl = resolve_height(world, op.setup.as_deref(), op.params.region_height())?
                - f.origin.z;
            f.world_region(&local, zl).ok()
        }
    }
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
    match selection_geometry(world, op.setup.as_deref(), items, &op.params) {
        Ok((_, chains)) => chains,
        Err(_) => Vec::new(),
    }
}

/// The drive chains of one operation as the world stands now, in
/// WORLD 3D (M5.1 J): the Multi-Axis Contour's source must be a
/// selection of edge-chain picks - the picks ARE the path, machined
/// where they stand. Any other source is a named refusal, never a
/// silent whole-model guess: a Stock or Entity source spells "cut
/// the model's own pattern", which is what the mesh-driven 5-axis
/// kinds exist for. The facet geometry the ball rests ON is NOT read
/// here: it is `material_of`'s union of the picked entities, whole -
/// the curve tells the ball where to go, the model says how high it
/// sits there.
pub(super) fn drive_chains_of(world: &World, op: &Operation) -> Result<Vec<Vec<Point3>>, GenError> {
    match &op.src {
        SrcRef::Selection(items) => drive_chains(world, items),
        SrcRef::Stock => Err(GenError::Invalid(
            "Multi-Axis Contour machines picked edge chains - pick them in the Geometry section, \
             or use 5-Axis Contour to machine the stock's own pattern"
                .into(),
        )),
        SrcRef::Entity(name) => Err(GenError::Invalid(format!(
            "Multi-Axis Contour machines picked edge chains, not a whole entity ('{name}') - \
             pick edges in its Geometry section, or use 5-Axis Contour for that model's pattern"
        ))),
    }
}

/// The source dispatch shared by `generate` and `region_of`: a selection
/// resolves its picked features, anything else slices its material, and
/// Slot keeps its closed-form stadium regardless of source (its boundary
/// is the two endpoints, not the model). Drill likewise ignores a
/// selection source: its geometry is the hole list in the params, so
/// picking features could only disagree with it. Pocket3D too: its
/// footprint is the model's own silhouette - the level law reads the
/// mesh at every Z, so a picked loop could only disagree with it.
/// `frame` is the 3+2 index this region belongs to (M5.1 B): Some
/// means `material`'s facets are already the frame's LOCAL ones and
/// every section plane is the world height read as axis distance from
/// the anchor (`z - origin.z`); `region_of`'s preview and `generate`'s
/// local pass share this one law so the drawn outline cannot
/// disagree with the cut. None is the prismatic path, bit-identical.
pub(super) fn region_for_src(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
    material: &Material,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Result<Region, GenError> {
    match src {
        SrcRef::Selection(items)
            if !matches!(
                params,
                OpParams::Slot { .. }
                    | OpParams::Drill { .. }
                    | OpParams::Pocket3D { .. }
                    | OpParams::Finish3D { .. }
                    // The 5-axis kinds are mesh-driven like the finish:
                    // the rings and raster lines chase the model at
                    // every plane, so a picked loop could only disagree
                    // with the path (M5.1 C).
                    | OpParams::Contour5D { .. }
                    | OpParams::Parallel5D { .. }
                    // The swarf family is mesh-driven the same way, and
                    // more strictly so: its pass lines ARE the ruling
                    // the picked loop could only contradict (M5.1 D).
                    | OpParams::Swarf { .. }
                    | OpParams::AdvancedSwarf { .. }
                    // The rotary twins ride the fitted axis, not a pick:
                    // the turn's own geometry authors the path (M5.1 E).
                    | OpParams::RotaryContour { .. }
                    | OpParams::RotaryParallel { .. }
                    // The rough ladder wraps the same axis - its islands
                    // come from the mesh in wrap space, never a pick.
                    | OpParams::RotaryOffset { .. }
                    // The stamped ladder is the 3-D pocket's islands
                    // verbatim - mesh sections at every plane, and the
                    // stamp only ever writes poses onto them. Flow
                    // chases the mesh's own direction field; a picked
                    // loop could only contradict it (M5.1 I).
                    | OpParams::MultiAxisRough { .. }
                    | OpParams::Flow5D { .. }
                    // The drive-curve contour is the one kind whose
                    // picks ARE the path: the region is still the
                    // material's shadow - a picked loop could not
                    // disagree with the pattern because the picks
                    // ARE it (M5.1 J).
                    | OpParams::MultiAxisContour { .. }
                    // The facing ladder's limit is the turned
                    // profile's own silhouette at every station:
                    // the meridian projection authors it from the
                    // mesh, and a pick could only disagree with
                    // the turn (M5.2 A). The profile kinds ride
                    // that very silhouette - same answer (M5.2 B).
                    | OpParams::Facing { .. }
                    | OpParams::ProfileRough { .. }
                    | OpParams::ProfileFinish { .. }
                    // The blade stands in the turned part's own
                    // silhouette - the band is model-bounded, not
                    // pick-bounded (M5.2 C).
                    | OpParams::GrooveRough { .. }
                    | OpParams::GrooveFinish { .. }
                    | OpParams::Part { .. }
                    // The lattice is the modeled thread's own - a
                    // pick could only disagree with the helix (D).
                    // So is a face: the window says which cone the
                    // ride walks, and the model says it too (E).
                    | OpParams::Threading { .. }
                    | OpParams::Chamfering { .. }
                    | OpParams::TaperTurning { .. }
                    // So is a crown plunge: the window says which
                    // valleys the gauge stamps, and the model says
                    // it too (M5.2 E2).
                    | OpParams::MultiPassTurning { .. }
                    | OpParams::FormTurning { .. }
            ) =>
        {
            // The one door's lock (M5.1 B): a picked edge is a world
            // feature; reading it in a tilted plane means projecting
            // the pick onto the plane, and nothing here does that
            // yet. Refuse by name rather than cut a shadow.
            if frame.is_some() {
                return Err(GenError::Invalid(
                    "a tilted plane cannot yet index a picked selection - pick a whole model instead"
                        .into(),
                ));
            }
            selection_region(world, setup, items, params)
        }
        _ => region_for(world, setup, material, params, frame),
    }
}

/// The per-kind region build, shared by `generate` and `region_of` so the
/// preview outline cannot disagree with what the generators cut. The
/// section height resolves through the world like every other height in
/// the params: a preview with an unresolvable `top_z` shows nothing (the
/// `Err` folds to `None` in `region_of`), and a generation refuses by
/// name.
pub(super) fn region_for(
    world: &World,
    setup: Option<&str>,
    material: &Material,
    params: &OpParams,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Result<Region, GenError> {
    // The section plane as the material's coordinates read it: the
    // world height minus the anchor's z is the frame's own distance
    // law (M5.1 B); None frame leaves every number untouched.
    let oz = frame.map_or(0.0, |f| f.origin.z);
    match params {
        OpParams::Face { .. } => {
            slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)
        }
        // The 3D pocket's machine area is a silhouette, not a section:
        // columns whose surface sits below the first level are still
        // cut, down to that surface - a section would miss them. The
        // per-level islands come from the mesh, in the generator and in
        // the verify pass, never from this region.
        OpParams::Pocket3D { .. } => {
            slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)
        }
        // Same silhouette law for the finish - and for its 5-axis
        // twins, whose paths ARE the finish's paths with a stamped
        // pose: the rings chase the mesh at every plane, the region
        // only says where the model shadows.
        // The rotary twins chase the mesh around the fitted axis, so
        // the same shadow answers for them (stage E).
        OpParams::Finish3D { .. }
        | OpParams::Contour5D { .. }
        | OpParams::Parallel5D { .. }
        | OpParams::Swarf { .. }
        | OpParams::AdvancedSwarf { .. }
        // The wrap ladder is mesh-driven the same way: its columns are
        // the shadow's, unrolled (stage E2).
        | OpParams::RotaryContour { .. }
        | OpParams::RotaryParallel { .. }
        | OpParams::RotaryOffset { .. }
        // The stamped ladder shadows the same silhouette: leaning the
        // spindle moves no position, so no footprint moves either -
        // and flow turns the lines inside the same shadow (M5.1 I).
        | OpParams::MultiAxisRough { .. }
        | OpParams::Flow5D { .. }
        // The drive-curve contour's ball rests on this same shadow's
        // solid; the chains it rides are resolved separately, in 3D,
        // at generation (M5.1 J).
        | OpParams::MultiAxisContour { .. }
        // Facing rides the same shadow for the panels: the station
        // limits are the meridian profile's own, computed in the
        // axis frame by the kernel (M5.2 A) - and so are the
        // profile kinds' rungs, the same silhouette read as skin
        // instead of as a floor (M5.2 B).
        | OpParams::Facing { .. }
        | OpParams::ProfileRough { .. }
        | OpParams::ProfileFinish { .. }
        // The blade's shadow is the bar's own disc - the kerf's
        // width lives in the meridian, not the footprint (M5.2 C).
        | OpParams::GrooveRough { .. }
        | OpParams::GrooveFinish { .. }
        // Threading's shadow is the bar's own disc too - the wave
        // lives in the meridian, not the footprint (M5.2 D). The
        // contour rides stand on the same disc: a face of
        // revolution casts the turned part's shadow (M5.2 E).
        | OpParams::Part { .. }
        | OpParams::Threading { .. }
        | OpParams::Chamfering { .. }
        | OpParams::TaperTurning { .. }
        // The crown gauge stands on the same disc: a face of
        // revolution casts the turned part's shadow (M5.2 E2).
        | OpParams::MultiPassTurning { .. }
        | OpParams::FormTurning { .. } => {
            slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)
        }
        OpParams::Contour { top_z, .. }
        | OpParams::Pocket { top_z, .. }
        | OpParams::Trace { top_z, .. } => {
            let z = resolve_height(world, setup, *top_z)
                .ok_or_else(|| unresolved_err("Top of Part Z", *top_z, world, setup))?;
            section(&material.facets, z - oz)
        }
        OpParams::Slot { a, b, width, .. } => stadium(*a, *b, width * 0.5),
        OpParams::Drill { holes, .. } => {
            // The silhouette, plus a rim loop for every hole (a blind
            // hole's mouth sits inside the material's shadow and the
            // projection never sees it; a through wall does show up,
            // but as the mesh's own coarse polygon). The rims are what
            // make the gouge law's tangency test mean something at a
            // plunge: d(center, rim) = hole radius, so a matched drill
            // passes silent and an oversized tool is honestly flagged.
            // A silhouette loop tracing a hole is REPLACED, not kept:
            // a 16-facet wall's apothem sits a chord-sag inside its own
            // radius - far more than the gouge law's slack at drill
            // diameters - and the law would read that tessellation as
            // tool bite. The hole's parameters are the truth here.
            let base = slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)?;
            if holes.is_empty() {
                return Ok(base);
            }
            let mut loops: Vec<Loop> = base
                .loops()
                .iter()
                .filter(|l| !holes.iter().any(|h| rim_is(l, *h)))
                .cloned()
                .collect();
            for hole in holes {
                loops.push(rim_loop(*hole)?);
            }
            Ok(Region::new(loops)?)
        }
    }
}

/// Rim loop segments: enough that the chord sag below the circle,
/// r·(1−cos(π/n)), stays under RIM_SAG_BUDGET - half the tolerance the
/// gouge law grants, so a matched tool clears its own rim by the other
/// half, never by an accident of the segment count. That is 48 sides at
/// drill diameters and ~110 at a 12 mm bore radius; the floor keeps
/// tiny holes from degenerating and the ceiling keeps huge ones cheap.
pub(super) const RIM_SAG_BUDGET: f64 = 0.005;
pub(super) const DRILL_RIM_SEGMENTS: usize = 48;
pub(super) const DRILL_RIM_MAX_SEGMENTS: usize = 4096;

pub(super) fn rim_segments(r: f64) -> usize {
    let x = (RIM_SAG_BUDGET / r).clamp(0.0, 2.0);
    let n = (std::f64::consts::PI / (1.0 - x).acos()).ceil() as usize;
    n.clamp(DRILL_RIM_SEGMENTS, DRILL_RIM_MAX_SEGMENTS)
}

/// A polygonal circle at a hole's XY - the loop the rim of `h` adds to a
/// drill region. `Region::new` orients by nesting, so the winding here
/// is free.
pub(super) fn rim_loop(h: Hole) -> Result<Loop, GenError> {
    let n = rim_segments(h.radius);
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let th = std::f64::consts::TAU * i as f64 / n as f64;
        pts.push(Point2::new(h.xy.x + h.radius * th.cos(), h.xy.y + h.radius * th.sin()));
    }
    Ok(Loop::new(pts)?)
}

/// Whether `l` is `h`'s rim already: enough vertices, and every one of
/// them on the hole's circle within a band that forgives tessellation
/// (the fitted radius and the sliced rim share the mesh's own vertices
/// for a through wall; only quantization separates them). A square, a
/// slot end, or a different concentric circle fails.
pub(super) fn rim_is(l: &Loop, h: Hole) -> bool {
    let pts = l.points();
    if pts.len() < 8 {
        return false;
    }
    let tol = (0.02 * h.radius).max(0.1);
    pts.iter().all(|p| ((p.x - h.xy.x).hypot(p.y - h.xy.y) - h.radius).abs() <= tol)
}

/// The shared refusal phrasing for an unresolved height, named by field.
/// One source so `resolve_params`, the region builders and every command
/// error read identically.
pub fn unresolved_msg(
    what: &str,
    h: Height,
    world: &World,
    setup: Option<&str>,
) -> String {
    format!(
        "{what} (From: {ref}, offset {off}) is unresolved: {why}",
        ref = h.reference,
        off = h.offset,
        why = missing_source(world, setup, h.reference)
            .unwrap_or("the source has no finite height"),
    )
}

pub(super) fn unresolved_err(what: &str, h: Height, world: &World, setup: Option<&str>) -> GenError {
    GenError::Invalid(unresolved_msg(what, h, world, setup))
}

/// A required height slot of an already-`Ok` `ResolvedHeights` is `Some`
/// by that function's contract (per kind); the error arm exists so no
/// call site needs an unwrap.
pub(super) fn req_z(v: Option<f64>, what: &str) -> Result<f64, GenError> {
    v.ok_or_else(|| GenError::Invalid(format!("{what} is unresolved")))
}
