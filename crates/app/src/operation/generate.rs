

use super::*;

/// Generate the path for an operation as the world stands now, and verify
/// it. Reads only; the caller stores whatever comes back. The error class
/// is `GenError` for both setup problems (missing tool/stock, unusable
/// geometry) and generator refusals - commands surface them at
/// construction, so nothing half-generated reaches the world.
/// `op_name` is the operation's own name, needed by re-machining to find
/// the program ahead of it (M3.1 D); callers generating a params preview
/// for an operation that has no entity (or none that matters) pass `""`,
/// which reads as "nothing ahead" and generates against the bare stock.
pub fn generate(
    world: &World,
    op: &Operation,
    op_name: &str,
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
    let setup = op.setup.as_deref();
    let material = material_of(world, setup, &op.src)?;
    // Setup stock, when it is not the source itself: the swarf column
    // around a smaller model. Island-vs-swarf in the reach law keys on
    // `region.contains`, so this only widens what counts as material -
    // and it is THIS operation's setup's stock (M3.2 A), never a
    // neighbor setup's block.
    let stock_facets = match &op.src {
        SrcRef::Stock => None,
        SrcRef::Entity(_) | SrcRef::Selection(_) => {
            model::find_by_name(world, &crate::setup::stock_entity_name(world, setup))
                .and_then(|e| model::world_facets(world, &e))
        }
    };
    let stock_region = stock_facets
        .as_deref()
        .and_then(|f| slice::project(f, FOOTPRINT_SAMPLES).ok())
        .filter(|r| !r.is_empty());
    let stock_top_z = material.top
       .max(
            model::stock_world_aabb_for(world, setup)
                .map(|b| b.max.z)
                .unwrap_or(f64::NEG_INFINITY),
        );

    // Which verify laws the path answers to is registry data (M5.0.2):
    // the row's comment explains each choice.
    let mode = op.params.kind().info().mode;

    // The height seam: the params' From+offset expressions become the
    // generators' plain f64s right here. Every unresolved reference is
    // refused by name before a single move is computed - never a
    // guessed z. `machining2d`'s params keep taking resolved values, so
    // the generators (and the goldens) know nothing of references.
    let hs = resolve_params(world, setup, &op.params).map_err(GenError::Invalid)?;

    // The 3+2 wrap (M5.1 stage B). A planar op with a tilt generates
    // exactly the path the upright kind would on the model as seen
    // FROM the tilted plane: the model's facets come through the
    // frame's `local_facets`, every resolved height is read as
    // distance along the frame's axis from the plane anchor
    // (`h - origin.z`, the identity-exact law: an upright frame at the
    // world origin shifts nothing, so every prismatic file and golden
    // rides on untouched), and the finished LOCAL path leaves through
    // the frame's one door, `world_path`. The generators themselves
    // never see a frame - rotation invariance is the whole argument
    // that this is sound - and the laws that verify the path below run
    // on the local one for exactly the same reason.
    let frame = match op.params.tilt() {
        None => None,
        Some(t) => match t.frame() {
            Some(f) => Some(f),
            // Only a non-finite letter could get here (the parameter
            // door refuses it); naming it beats inventing a plane.
            None => {
                return Err(GenError::Invalid(
                    "the 3+2 index commands no direction - its angles are not finite".into(),
                ))
            }
        },
    };
    let oz = frame.map_or(0.0, |f| f.origin.z);
    let hl = ResolvedHeights {
        clearance: hs.clearance - oz,
        top: hs.top.map(|v| v - oz),
        bottom: hs.bottom.map(|v| v - oz),
        face_top: hs.face_top.map(|v| v - oz),
        retract: hs.retract.map(|v| v - oz),
    };
    // The one refusal the wrap owes (M5.1 B): a picked selection is
    // read as world geometry by the selection walk - projecting a pick
    // onto a tilted plane is not implemented, and a shadow of the
    // pick is not what was picked. Refused by name, never silently
    // cut as a projection.
    if frame.is_some() && matches!(op.src, SrcRef::Selection(_)) {
        return Err(GenError::Invalid(
            "a tilted plane cannot yet index a picked selection - pick a whole model instead"
                .into(),
        ));
    }
    let material_local = frame
        .as_ref()
        .map(|f| Material { facets: f.local_facets(&material.facets), top: material.top - oz });
    let m_gen = material_local.as_ref().unwrap_or(&material);

    // Region + path per kind.
    let (region, path) = match &op.params {
        OpParams::Face { stepover, stepdown, feed, plunge_feed, climb, .. } => {
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = machining2d::face(
                &region,
                &FaceParams {
                    tool_diameter: d,
                    stepover: *stepover,
                    clearance_z: hl.clearance,
                    cut_z: req_z(hl.bottom, "Cut Z")?,
                    top_z: hl.face_top,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            (region, path)
        }
        OpParams::Contour {
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            side,
            lead,
            tolerance,
            tab_count,
            tab_width,
            tab_height,
            ..
        } => {
            // Backstop for a committed file that carries Inside on a
            // whole-model source (commands refuse it at creation): the
            // mirror-gouge law must never reach a path.
            if let Some(msg) = side_sane(&op.params, &op.src) {
                return Err(GenError::Invalid(msg));
            }
            let cp = ContourParams {
                tool_diameter: d,
                stock_to_leave: *stock_to_leave,
                clearance_z: hl.clearance,
                retract_z: req_z(hl.retract, "Retract Z")?,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                side: *side,
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
                    let (loops, runs, auto_inside) =
                        selection_geometry_impl(world, setup, items, &op.params, None)?;
                    if loops.is_empty() && runs.is_empty() {
                        return Err(GenError::Empty(
                            "nothing in the selection bounds an area or a run".into(),
                        ));
                    }
                    let mut cp = cp;
                    cp.side = match side {
                        Side::Auto if auto_inside => Side::Inside,
                        Side::Auto => Side::Outside,
                        s => *s,
                    };
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
                    let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
                    let path = machining2d::contour(&region, &cp)?;
                    (region, path)
                }
            };
            (region, path)
        }
        OpParams::Pocket {
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            ..
        } => {
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = machining2d::pocket(
                &region,
                &PocketParams {
                    tool_diameter: d,
                    stepover: *stepover,
                    stock_to_leave: *stock_to_leave,
                    clearance_z: hl.clearance,
                    retract_z: hl.retract,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                    strategy: *strategy,
                },
            )?;
            (region, path)
        }
        OpParams::Pocket3D {
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            entry,
            helix_radius,
            rest_rough,
            max_engagement,
            feed_opt,
            ..
        } => {
            // The region is the model's own silhouette (see
            // `region_for`); the level law sections the facets
            // themselves at every plane, so there is nothing here for a
            // picked loop to add - and nothing to erode by hand either:
            // one erosion per level, from that level's target.
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let pp = Pocket3DParams {
                tool_diameter: d,
                stepover: *stepover,
                stock_to_leave: *stock_to_leave,
                clearance_z: hl.clearance,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                strategy: *strategy,
                retract_z: hl.retract,
                entry: *entry,
                helix_radius: *helix_radius,
                max_engagement: *max_engagement,
            };
            // Re-machining (M3.1 D): what the program ahead of this
            // pocket left standing bounds the coarse run, so it cuts
            // the shelves its predecessors already cleared only if the
            // mask says material still stands there. Nothing ahead (or
            // nothing cut) is `None`, and the path is what it was.
            let prior = crate::rest::prior_state(world, op_name, setup, d * 0.5)?;
            let mask = prior.grid.as_ref();
            // The rest pass's verdict rides the path: what the sim
            // still reports after the closing ladder is the closing
            // report's business (the simulation view), not a silent
            // extra here.
            let generated = if *rest_rough {
                machining3d::pocket3d_with_rest_prev(&material.facets, &region, &pp, mask)
                    .map(|(p, _)| p)
            } else {
                machining3d::pocket3d_prev(&material.facets, &region, &pp, mask)
            };
            let path = match generated {
                Ok(path) => path,
                // The ladder's words are "the model fills the pocket"
                // - true of an unmasked nothing-to-cut. With a mask
                // the truer sentence is this one: the program already
                // machined here, and the mask law only ever removes
                // cuts, so the whole path is air.
                Err(GenError::Empty(_)) if mask.is_some() => {
                    return Err(GenError::Empty(
                        "nothing left to cut - the operations ahead in the program already \
                         machined this area"
                            .into(),
                    ));
                }
                Err(e) => return Err(e),
            };
            // The feed modulation is a re-read of the finished path
            // against the sim (M2.1d): same moves, new F words - it
            // runs after both ladder variants, never instead of one.
            let path = if *feed_opt {
                machining3d::optimize_feed(&path, &material.facets, &region, &pp)?
            } else {
                path
            };
            (region, path)
        }
        OpParams::Finish3D {
            pattern,
            stepover,
            tolerance,
            feed,
            plunge_feed,
            climb,
            ramp,
            min_step,
            ramp_angle,
            ..
        } => {
            // Ball jurisdiction: the rings are the tangent locus of a
            // sphere. A flat-bottomed or torus-bottomed cutter's
            // forbidden set is not this facetwise support scan (the
            // Minkowski sum differs), so an end mill here refuses by
            // name - never a ball path dressed as something else.
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "3D Finish needs a ball-nose tool, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let fp = Finish3DParams {
                pattern: *pattern,
                stepover: *stepover,
                tool_diameter: d,
                tolerance: *tolerance,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                ramp: *ramp,
                min_step: *min_step,
                ramp_angle: *ramp_angle,
            };
            // The pattern picks the generator; Waterline never
            // reaches the raster code at all.
            let path = match pattern {
                finishing3d::FinishPattern::Waterline => {
                    finishing3d::waterline(&material.facets, &fp)?
                }
                finishing3d::FinishPattern::Parallel
                | finishing3d::FinishPattern::ParallelMulti
                | finishing3d::FinishPattern::Scallop => {
                    finishing3d::parallel(&material.facets, &fp)?
                }
                finishing3d::FinishPattern::Radial => {
                    finishing3d::radial(&material.facets, &fp)?
                }
                finishing3d::FinishPattern::Spiral => {
                    finishing3d::spiral(&material.facets, &fp)?
                }
                finishing3d::FinishPattern::Contour => {
                    finishing3d::contour(&material.facets, &fp)?
                }
                finishing3d::FinishPattern::Project => {
                    finishing3d::project(&material.facets, &fp)?
                }
            };
            (region, path)
        }
        // The simultaneous-axis kinds (M5.1 stage C). The ball-center
        // path is the finish's own - a sphere's envelope is rotation-
        // invariant, so leaning the spindle changes what the SHANK
        // sees, not what the ball leaves - and the kind adds the POSE
        // to every cut move. No ramp, no min_step: the plunge owns
        // the first contact, and the scallop law alone authors the
        // ladder. The two arms are the same generator pair the finish's
        // pattern combo runs, with the axis stamp bolted on.
        OpParams::Contour5D {
            tolerance,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        }
        | OpParams::Parallel5D {
            tolerance,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        } => {
            let is_contour = matches!(&op.params, OpParams::Contour5D { .. });
            // The contour has no stepover field to bind (the or-pattern
            // binds one name on one side and E0642s); read it here,
            // where the parallel's own arm answers.
            let stepover = match &op.params {
                OpParams::Parallel5D { stepover, .. } => *stepover,
                _ => None,
            };
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "{} needs a ball-nose tool, '{}' is a {}",
                    if is_contour { "5-Axis Contour" } else { "5-Axis Parallel" },
                    op.tool,
                    tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let fp = Finish3DParams {
                pattern: if is_contour {
                    finishing3d::FinishPattern::Waterline
                } else {
                    finishing3d::FinishPattern::Parallel
                },
                stepover: if is_contour { None } else { stepover },
                tool_diameter: d,
                tolerance: *tolerance,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                ramp: false,
                min_step: None,
                ramp_angle: 45.0,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            let path = if is_contour {
                crate::multiaxis::contour5d(&material.facets, &fp, &axis)?
            } else {
                crate::multiaxis::parallel5d(&material.facets, &fp, &axis)?
            };
            (region, path)
        }
        // M5.1 stage D: the swarf family. The path is the raster run
        // along the ruling the model states, so the numbers ride the
        // parallel's law; what the kind ADDS is where the ruling comes
        // from - the generator reads it off the facets, and refuses by
        // name (a dome, a Z-wall) when the surface has none. The
        // ladder's count is the one field the plain kind has no name
        // for, so it reads back below, the twins' stepover trick.
        OpParams::Swarf {
            tolerance,
            stepover,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        }
        | OpParams::AdvancedSwarf {
            tolerance,
            stepover,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        } => {
            let is_ladder = matches!(&op.params, OpParams::AdvancedSwarf { .. });
            let passes = match &op.params {
                OpParams::AdvancedSwarf { passes, .. } => *passes,
                _ => 1,
            };
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "{} needs a ball-nose tool, '{}' is a {}",
                    if is_ladder { "Advanced Swarf" } else { "Swarf" },
                    op.tool,
                    tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let fp = Finish3DParams {
                // The ruling replaces the pattern's choice of lines;
                // the geometry that remains is the parallel's exactly.
                pattern: finishing3d::FinishPattern::Parallel,
                stepover: *stepover,
                tool_diameter: d,
                tolerance: *tolerance,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                ramp: false,
                min_step: None,
                ramp_angle: 45.0,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            let path = if is_ladder {
                crate::multiaxis::swarf5d_advanced(&material.facets, &fp, &axis, passes)?
            } else {
                crate::multiaxis::swarf5d(&material.facets, &fp, &axis)?
            };
            (region, path)
        }
        // M5.1 stage E: the rotary twins. The path is the finish's
        // own ladder seen through a frame the FITTED axis defines, so
        // the numbers ride the same law; what the kinds add is the
        // axis - model data, refused by name when the model says
        // nothing of turning (and by the other twin when the part's
        // stance is its sibling's) - and the radial pose in place of
        // the surface normal's.
        OpParams::RotaryContour {
            tolerance,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        }
        | OpParams::RotaryParallel {
            tolerance,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        } => {
            let is_contour = matches!(&op.params, OpParams::RotaryContour { .. });
            let stepover = match &op.params {
                OpParams::RotaryParallel { stepover, .. } => *stepover,
                _ => None,
            };
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "{} needs a ball-nose tool, '{}' is a {}",
                    if is_contour { "Rotary Contour" } else { "Rotary Parallel" },
                    op.tool,
                    tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let fp = Finish3DParams {
                // The kind names the ladder - waterline rings about
                // the axis, or passes along it - so the pattern byte
                // answers the kernel's own reading.
                pattern: finishing3d::FinishPattern::Waterline,
                stepover: if is_contour { None } else { stepover },
                tool_diameter: d,
                tolerance: *tolerance,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                ramp: false,
                min_step: None,
                ramp_angle: 45.0,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            let path = if is_contour {
                crate::multiaxis::rotary_contour(&material.facets, &fp, &axis)?
            } else {
                crate::multiaxis::rotary_parallel(&material.facets, &fp, &axis)?
            };
            (region, path)
        }
        // M5.1 stage E2: the rotary rough. The pocket ladder runs in
        // WRAP SPACE about the fitted axis, so the numbers ride
        // Pocket3D's shape - but three of them are read the wrap's
        // way: `stepdown` infeeds RADII, `top_z`/`bottom_z` window
        // the STATIONS (the kernel's own `(h - o.z)/u.z` law), and
        // the rapids plane is the kernel's, one infeed and a tool
        // radius off the skin - what resolves here only stands when
        // the axis stands up. The refusal for a model that says
        // nothing of turning comes from the same axis fit the twins
        // invoke, by name.
        OpParams::RotaryOffset {
            stepover,
            stock_to_leave,
            stepdown,
            feed,
            plunge_feed,
            climb,
            strategy,
            entry,
            helix_radius,
            max_engagement,
            tolerance,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "Rotary Offset needs a ball-nose tool, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let pp = Pocket3DParams {
                tool_diameter: d,
                stepover: *stepover,
                stock_to_leave: *stock_to_leave,
                clearance_z: hl.clearance,
                top_z: req_z(hl.top, "Top of Part Z")?,
                bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                strategy: *strategy,
                retract_z: hl.retract,
                entry: *entry,
                helix_radius: *helix_radius,
                max_engagement: *max_engagement,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            // `tolerance` is the VERIFY budget (the envelope law's),
            // stated here so the field's double life is on record:
            // the kernel chords by its own arc-fit tolerance.
            let _ = tolerance;
            let path = crate::multiaxis::rotary_offset(&material.facets, &pp, &axis)?;
            (region, path)
        }
        OpParams::Slot {
            a,
            b,
            width,
            stock_to_leave,
            stepdown,
            stepover,
            feed,
            plunge_feed,
            climb,
            ..
        } => {
            let path = machining2d::slot(
                *a,
                *b,
                &SlotParams {
                    tool_diameter: d,
                    width: *width,
                    stock_to_leave: *stock_to_leave,
                    clearance_z: hl.clearance,
                    retract_z: hl.retract,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    stepover: *stepover,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            (region, path)
        }
        OpParams::Trace { stepdown, feed, plunge_feed, climb, .. } => {
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = machining2d::trace(
                &region,
                &TraceParams {
                    clearance_z: hl.clearance,
                    retract_z: hl.retract,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            (region, path)
        }
        OpParams::Drill { cycle, peck, dwell, feed, plunge_feed, holes, .. } => {
            // No region feeds the generator - the hole list is the
            // geometry - but the verify pass below needs one, and it is
            // built the same shared way as every other kind's.
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = drilling::drill(&drilling::CycleParams {
                clearance_z: hl.clearance,
                r_z: req_z(hl.top, "Top of Part Z")?,
                hop_z: hl.retract,
                bottom_z: hl.bottom,
                cycle: *cycle,
                peck: *peck,
                dwell: *dwell,
                feed: *feed,
                plunge_feed: *plunge_feed,
                tool: &tool,
                holes,
            })?;
            (region, path)
        }
    };

    // M3.2 stage B: what the program clamps to. The op's setup context
    // decides - user fixtures bound to it (tagged models, implicit-
    // setup resolution on both sides) plus the machine table when the
    // .mch states a travel the table can sit in.
    let fixture_boxes = crate::fixture::boxes_for(world, op.setup.as_deref());
    let geom = tool.geom();
    // The machine's own steel, if it states any (M3.2 stage B): the
    // unstated-limit law means a file that names no spindle body adds
    // no band, exactly as it adds no feed or travel check.
    let spindle = crate::machine::of(world)
        .1
        .and_then(|m| m.spindle_d.zip(m.spindle_len))
        .map(|(d, l)| (d * 0.5, l));
    let stl = match &op.params {
        OpParams::Face { .. }
        | OpParams::Trace { .. }
        | OpParams::Drill { .. }
        | OpParams::Finish3D { .. }
        // The 5-axis kinds leave no stock - the ball path is the
        // finish's path, and the pose adds no material law. The swarf
        // family included: lying WITH the flank changes what the
        // shank sees, not what the ball removes.
        | OpParams::Contour5D { .. }
        | OpParams::Parallel5D { .. }
        | OpParams::Swarf { .. }
        | OpParams::AdvancedSwarf { .. }
        // Rotary leaves no stock either: the posed path is the
        // finish's path, the axis fit adds no material law.
        | OpParams::RotaryContour { .. }
        | OpParams::RotaryParallel { .. } => 0.0,
        OpParams::Contour { stock_to_leave, .. }
        | OpParams::Pocket { stock_to_leave, .. }
        | OpParams::Slot { stock_to_leave, .. }
        | OpParams::Pocket3D { stock_to_leave, .. }
        // The ladder is the one rotary that LEAVES a shelf on purpose:
        // the finishing kinds own the ledge under a column's last cut,
        // so the wrap reads this number as a radial wall (E2).
        | OpParams::RotaryOffset { stock_to_leave, .. } => *stock_to_leave,
    };
    match &frame {
        None => {
            let warns = verify::verify(
                &path,
                &VerifyContext {
                    region: &region,
                    stock: stock_region.as_ref(),
                    // Only the mesh-driven kinds' laws are depth-dependent;
                    // every other op verifies against its static region,
                    // untouched.
                    model: matches!(
                        &op.params,
                        OpParams::Pocket3D { .. }
                            | OpParams::Finish3D { .. }
                            | OpParams::Contour5D { .. }
                            | OpParams::Parallel5D { .. }
                            | OpParams::Swarf { .. }
                            | OpParams::AdvancedSwarf { .. }
                            | OpParams::RotaryContour { .. }
                            | OpParams::RotaryParallel { .. }
                            | OpParams::RotaryOffset { .. }
                    )
                    .then_some(&material.facets[..]),
                    tool: &geom,
                    mode,
                    stock_top_z,
                    clearance_z: hl.clearance,
                    stock_to_leave: stl,
                    gouge_tol: GOUGE_TOL,
                    z_tol: Z_TOL,
                    // The finish op's envelope budget is its own tolerance;
                    // no other kind runs the envelope law - and the
                    // 5-axis kinds ARE the envelope law with a leaning
                    // shank, so they carry their tolerance the same way
                    // (the ball's path is identical; the envelope scan
                    // never sees the difference).
                    finish_tol: match &op.params {
                        OpParams::Finish3D { tolerance, .. }
                        | OpParams::Contour5D { tolerance, .. }
                        | OpParams::Parallel5D { tolerance, .. }
                        | OpParams::Swarf { tolerance, .. }
                        | OpParams::AdvancedSwarf { tolerance, .. }
                        | OpParams::RotaryContour { tolerance, .. }
                        | OpParams::RotaryParallel { tolerance, .. }
                        // The rough ladder's tolerance is exactly this
                        // law's budget - its only reading (E2).
                        | OpParams::RotaryOffset { tolerance, .. } => Some(*tolerance),
                        _ => None,
                    },
                    fixtures: &fixture_boxes,
                    spindle,
                },
            );
            Ok((path, warns))
        }
        Some(f) => {
            // The split verdict (M5.1 B): the planar laws walk the
            // LOCAL path, because they are rigid-invariant - XY
            // distances, containments and half-planes measured in the
            // cutting plane ARE the machine-truth question (a tilted
            // rapid never "dips" in world Z; it travels the plane).
            // The stock column's cap takes the frame-image of the
            // world stock box's corners: a world-vertical prism is a
            // tilted slab seen from the plane, and the corner max is
            // never below the slab over any sampled column - an over-
            // report, never a hide. The hardware laws walk the WORLD
            // path: the steel of the machine does not move with the
            // index, and stage A's lean enclosures read the pose off
            // the moves' stamped axes. One path is posted; both are
            // measured.
            let path_world = f.world_path(&path, machining2d::ARC_FIT_TOL);
            let mut st_local = stock_top_z - oz;
            let mut boxes: Vec<(crate::geometry::Point3, crate::geometry::Point3)> = Vec::new();
            if let Some(b) = model::stock_world_aabb_for(world, setup) {
                boxes.push((b.min, b.max));
            }
            // The model's own corners too: `stock_top_z` already holds
            // its top, but the tilted slab rides over its XY span, and
            // the affine local-z map takes its max at a corner.
            let mut lo = crate::geometry::Point3::new(f64::MAX, f64::MAX, f64::MAX);
            let mut hi = crate::geometry::Point3::new(f64::MIN, f64::MIN, f64::MIN);
            for t in &material.facets {
                for p in [t.a, t.b, t.c] {
                    lo.x = lo.x.min(p.x);
                    lo.y = lo.y.min(p.y);
                    lo.z = lo.z.min(p.z);
                    hi.x = hi.x.max(p.x);
                    hi.y = hi.y.max(p.y);
                    hi.z = hi.z.max(p.z);
                }
            }
            boxes.push((lo, hi));
            for (b0, b1) in boxes {
                for i in 0..8 {
                    let p = crate::geometry::Point3::new(
                        [b0.x, b1.x][(i >> 2) & 1],
                        [b0.y, b1.y][(i >> 1) & 1],
                        [b0.z, b1.z][i & 1],
                    );
                    st_local = st_local.max(f.z_local(p));
                }
            }
            let stock_region_local = stock_facets
                .as_deref()
                .map(|t| f.local_facets(t))
                .and_then(|t| slice::project(&t, FOOTPRINT_SAMPLES).ok())
                .filter(|r| !r.is_empty());
            let mut warns = verify::verify(
                &path,
                &VerifyContext {
                    region: &region,
                    stock: stock_region_local.as_ref(),
                    model: None,
                    tool: &geom,
                    mode,
                    stock_top_z: st_local,
                    clearance_z: hl.clearance,
                    stock_to_leave: stl,
                    gouge_tol: GOUGE_TOL,
                    z_tol: Z_TOL,
                    finish_tol: None,
                    // Steel runs against the world path below.
                    fixtures: &[],
                    spindle: None,
                },
            );
            if !fixture_boxes.is_empty() {
                warns.extend(verify::bands_vs_fixtures(
                    &path_world,
                    &geom,
                    &fixture_boxes,
                    spindle,
                    GOUGE_TOL,
                ));
                warns.extend(verify::rapids_vs_fixtures(
                    &path_world, &geom, &fixture_boxes,
                ));
            }
            Ok((path_world, warns))
        }
    }
}
