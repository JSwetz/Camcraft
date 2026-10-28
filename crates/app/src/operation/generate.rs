

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
            entry,
            helix_radius,
            max_engagement,
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
                    entry: *entry,
                    helix_radius: *helix_radius,
                    max_engagement: *max_engagement,
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
            shank_avoid,
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
            shank_avoid,
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
            // Stage F's dodge is opt-in and adds nothing when off:
            // None means the stamp never measures the shank, so the
            // paths of every v28 document regenerate byte-identical.
            let shank = (*shank_avoid).then(|| shank_of(&tool));
            let path = if is_contour {
                crate::multiaxis::contour5d(&material.facets, &fp, &axis, shank.as_ref())?
            } else {
                crate::multiaxis::parallel5d(&material.facets, &fp, &axis, shank.as_ref())?
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
        // M5.1 stage E3: the stamped ladder. The positions are the
        // pocket's - so is this whole arm, down to the re-machining
        // mask and the Empty remap - with the stamp's pose law
        // written over the finished path and the ball-nose gate the
        // tangency search needs (the stamp reads contact off a
        // sphere; a flat end's flank is a cylinder the law does not
        // own). No rest pass, no feed modulation: the stamp has
        // never been shown to commute with either, and silence
        // about a flag is a lie waiting to be filed (RECOMMENDED).
        OpParams::MultiAxisRough {
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
            shank_avoid,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "Multi-Axis Rough needs a ball-nose tool, '{}' is a {}",
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
            let _ = tolerance;
            let prior = crate::rest::prior_state(world, op_name, setup, d * 0.5)?;
            let mask = prior.grid.as_ref();
            // Same opt-in law as the twins' arm above: off, the ladder
            // stamps exactly as stage E3 shipped it.
            let shank = (*shank_avoid).then(|| shank_of(&tool));
            let path = match crate::multiaxis::multiaxis_rough_prev(
                &material.facets, &region, &pp, &axis, mask, shank.as_ref(),
            ) {
                Ok(path) => path,
                // The pocket arm's Empty remap, verbatim: with a mask
                // the truer sentence names the program, not the model.
                Err(GenError::Empty(_)) if mask.is_some() => {
                    return Err(GenError::Empty(
                        "nothing left to cut - the operations ahead in the program already \
                         machined this area"
                            .into(),
                    ));
                }
                Err(e) => return Err(e),
            };
            (region, path)
        }
        // M5.1 tail, stage I: flowline finishing. The pass is the
        // parallel's law run on lines that TURN - the direction read
        // off the model's own least-curving field, with the fallback
        // law degrading plain ground to straight raster passes so
        // flat interiors are never left uncut. The pose is stage C's
        // lean law with stage F's dodge, and the numbers are the
        // twins', field for field.
        OpParams::Flow5D {
            tolerance,
            stepover,
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "5-Axis Flow needs a ball-nose tool, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let fp = Finish3DParams {
                // The field replaces the pattern's choice of lines;
                // the geometry that remains is the raster's exactly.
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
            // Stage F's law again, opt-in: None stamps the flow lines
            // with the plain lean, as every v30 path regenerates.
            let shank = (*shank_avoid).then(|| shank_of(&tool));
            let path = crate::multiaxis::flow5d(&material.facets, &fp, &axis, shank.as_ref())?;
            (region, path)
        }
        // The drive-curve contour (M5.1 J): the picked chains, in
        // world 3D, machined WHERE THEY STAND. No pattern is asked of
        // the model - the curve is the path - so there is no stepover
        // to derive, no ladder to author, and the tolerance is not
        // even read here: it belongs to the verify pass below, the
        // envelope budget. The facets are the union `material_of`
        // collected from the selected entities, whole: the ball rests
        // on the model while the chain tells it where to go.
        OpParams::MultiAxisContour {
            feed,
            plunge_feed,
            climb,
            lead_deg,
            lag_deg,
            max_tilt_deg,
            shank_avoid,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::BallNose {
                return Err(GenError::Invalid(format!(
                    "Multi-Axis Contour needs a ball-nose tool, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let top = req_z(hl.top, "Top of Part Z")?;
            let bottom = req_z(hl.bottom, "Bottom of Part Z")?;
            let chains = drive_chains_of(world, op)?;
            // The window applies to the curve itself: a picked chain
            // standing outside the operation's top/bottom is a
            // disagreement between the pick and the heights that no
            // pose can fix - refused by vertex, never quietly
            // clipped (the Prime Directive's kind of wrong).
            for (ci, ch) in chains.iter().enumerate() {
                if let Some(v) = ch.iter().find(|v| v.z < bottom - Z_TOL || v.z > top + Z_TOL) {
                    return Err(GenError::Invalid(format!(
                        "picked chain {} stands at z {:.3}, outside the window [{:.3}, {:.3}] - widen Top/Bottom of Part Z or re-pick",
                        ci + 1,
                        v.z,
                        bottom,
                        top
                    )));
                }
            }
            let cp = crate::multiaxis::CurveParams {
                tool_diameter: d,
                clearance_z: hl.clearance,
                retract_z: hl.retract,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
            };
            let axis = crate::multiaxis::AxisControl {
                lead_deg: *lead_deg,
                lag_deg: *lag_deg,
                max_tilt_deg: Some(*max_tilt_deg),
            };
            let shank = (*shank_avoid).then(|| shank_of(&tool));
            let path =
                crate::multiaxis::contour_curve(&chains, &material.facets, &cp, &axis, shank.as_ref())?;
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
        // M5.2 stage A: the first turning kind. Which documents may
        // HOLD a Facing is the family door in CreateOperation; the
        // door in this arm is the TOOL - the kernel's no-gouge
        // construction IS the nose ball, so a mill cutter named
        // here is refused by name before a move is computed. The
        // facets are the world's (no tilt: the height laws below
        // already read the planes the axis fit walks).
        OpParams::Facing { stepdown, feed, plunge_feed, tolerance, .. } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Facing needs a turning insert, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            // `tolerance` is the VERIFY budget (the envelope law's,
            // as in RotaryOffset) - the kernel itself chords the
            // profile by that law's own tol/4 promise.
            let _ = tolerance;
            let path = crate::turning::facing(
                &material.facets,
                &crate::turning::FacingParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    facing_z: req_z(hl.bottom, "Facing Plane Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                },
            )?;
            (region, path)
        }
        // M5.2 stage B: the profile kinds. The same tool door as
        // facing's - the no-gouge construction IS the nose ball
        // rolled along the silhouette's offset - and the same height
        // law: world Z planes read as stations on the fitted axis.
        // `tolerance` is NOT ignored here: the kernel chords the
        // silhouette and the ride at it (quarter budgets each), so
        // it shapes the path exactly as the verdict measures it.
        OpParams::ProfileRough { stepdown, stock_to_leave, feed, plunge_feed, tolerance, .. } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Profile Rough needs a turning insert, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = crate::turning::profile_rough(
                &material.facets,
                &crate::turning::ProfileRoughParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    stock_to_leave: *stock_to_leave,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                    tolerance: *tolerance,
                },
            )?;
            (region, path)
        }
        OpParams::ProfileFinish { feed, plunge_feed, tolerance, .. } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Profile Finish needs a turning insert, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = crate::turning::profile_finish(
                &material.facets,
                &crate::turning::ProfileFinishParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                    tolerance: *tolerance,
                },
            )?;
            (region, path)
        }
        // M5.2 stage C: the blade kinds. The tool door names the
        // grooving blade - the kerf's width IS the tool, so a
        // turning insert could no more groove than an end mill
        // part off a bar. The heights read the axis stations the
        // twins ride; `tolerance` is load-bearing twice, the
        // profile kinds' law - the silhouette chords at it and the
        // verdict gates at it.
        OpParams::GrooveRough {
            stepdown,
            stock_to_leave,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Groove {
                return Err(GenError::Invalid(format!(
                    "Groove Rough needs a grooving blade, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = crate::turning::groove_rough(
                &material.facets,
                &crate::turning::GrooveRoughParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    stock_to_leave: *stock_to_leave,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    blade_width: tool.width,
                    tolerance: *tolerance,
                },
            )?;
            (region, path)
        }
        OpParams::GrooveFinish {
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Groove {
                return Err(GenError::Invalid(format!(
                    "Groove Finish needs a grooving blade, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = crate::turning::groove_finish(
                &material.facets,
                &crate::turning::GrooveFinishParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    blade_width: tool.width,
                    tolerance: *tolerance,
                },
            )?;
            (region, path)
        }
        OpParams::Part {
            stepdown,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Groove {
                return Err(GenError::Invalid(format!(
                    "Parting needs a grooving blade, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = crate::turning::part(
                &material.facets,
                &crate::turning::PartParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    parting_z: req_z(hl.bottom, "Parting Plane Z")?,
                    stepdown: *stepdown,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    blade_width: tool.width,
                    tolerance: *tolerance,
                },
            )?;
            (region, path)
        }
        // M5.2 stage D: threading. The tool door names the V-form
        // insert - the wave's profile IS the tool, so a grooving
        // blade (rectangular footprint, no flanks) could no more
        // cut a thread than an end mill could. The kernel's own
        // doors read the pair against the pitch (fit, full-form
        // reach) and refuse by name; the heights frame the window
        // of turns the lattice fills.
        OpParams::Threading {
            pitch,
            depth,
            infeed,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Thread {
                return Err(GenError::Invalid(format!(
                    "Threading needs a threading insert, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = crate::turning::thread(
                &material.facets,
                &crate::turning::ThreadParams {
                    tolerance: *tolerance,
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    pitch: *pitch,
                    depth: *depth,
                    infeed: *infeed,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    insert_width: tool.width,
                    form_angle: tool.tip_angle,
                },
            )?;
            (region, path)
        }
        // M5.2 stage E: the contour rides. The tool door names the
        // turning insert again - the ride is the nose's, and the
        // profile kinds' ball-of-the-nose law is what stands a
        // tolerance's promise up. The kernel's own door reads the
        // authored face against the model and refuses the lie by
        // name; these arms only spend the window and the numbers.
        OpParams::Chamfering {
            angle_deg,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Chamfering needs a turning insert, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = crate::turning::chamfer(
                &material.facets,
                &crate::turning::ChamferParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    angle_deg: *angle_deg,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                    tolerance: *tolerance,
                },
            )?;
            (region, path)
        }
        OpParams::TaperTurning {
            bottom_dia,
            top_dia,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Taper Turning needs a turning insert, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = crate::turning::taper(
                &material.facets,
                &crate::turning::TaperParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    bottom_dia: *bottom_dia,
                    top_dia: *top_dia,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                    tolerance: *tolerance,
                },
            )?;
            (region, path)
        }
        // Multi-pass spends stage B's ladder law verbatim: the same
        // rung numbers into the same door, plus the ride count the
        // spring law shares the leave over.
        OpParams::MultiPassTurning {
            stepdown,
            stock_to_leave,
            passes,
            feed,
            plunge_feed,
            tolerance,
            ..
        } => {
            if tool.kind != crate::tools::ToolKind::Turning {
                return Err(GenError::Invalid(format!(
                    "Multi-Pass Turning needs a turning insert, '{}' is a {}",
                    op.tool, tool.kind
                )));
            }
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, m_gen, frame.as_ref())?;
            let path = crate::turning::multi_pass(
                &material.facets,
                &crate::turning::MultiPassParams {
                    clearance_z: hl.clearance,
                    top_z: req_z(hl.top, "Top of Part Z")?,
                    bottom_z: req_z(hl.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    stock_to_leave: *stock_to_leave,
                    passes: *passes,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    nose_radius: tool.nose_radius,
                    tolerance: *tolerance,
                },
            )?;
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
        | OpParams::RotaryParallel { .. }
        // Flow bends the lines, not the material law. The drive-curve
        // contour removes what the chain says, nothing more - the
        // ball's envelope at the curve's stations is the whole law.
        | OpParams::Flow5D { .. }
        | OpParams::MultiAxisContour { .. }
        // Facing leaves no shelf: the ladder steps to the facing
        // plane and stops - the tolerance measures the finish, a
        // stock-to-leave number would only move the verdict (M5.2 A).
        // The finish ride is that answer on the skin: the tolerance
        // IS its chord budget, and a leave would only move the
        // verdict (M5.2 B).
        | OpParams::Facing { .. }
        // The finish plunge is the floor itself - the leave would
        // only move the rest verdict, and the verdict already reads
        // it (M5.2 C). Parting leaves nothing: the bar that stands
        // past the kerf is the scrap, and it goes.
        | OpParams::ProfileFinish { .. }
        | OpParams::GrooveFinish { .. }
        | OpParams::Part { .. }
        // Threading leaves no shelf either: the full-form pass owns
        // the whole wave, and the shallower rungs before it only
        // widen it - what stands is what the wave scan measures,
        // with nothing added to the floor (M5.2 D). The contour
        // rides own their skin outright: a chamfer and a taper are
        // finished by definition, and a multi-pass program ends ON
        // the profile - its leave is spent inside the ladder, not
        // standing outside the verdict (M5.2 E).
        | OpParams::Threading { .. }
        | OpParams::Chamfering { .. }
        | OpParams::TaperTurning { .. }
        | OpParams::MultiPassTurning { .. } => 0.0,
        OpParams::Contour { stock_to_leave, .. }
        | OpParams::Pocket { stock_to_leave, .. }
        | OpParams::Slot { stock_to_leave, .. }
        | OpParams::Pocket3D { stock_to_leave, .. }
        // The ladder is the one rotary that LEAVES a shelf on purpose:
        // the finishing kinds own the ledge under a column's last cut,
        // so the wrap reads this number as a radial wall (E2).
        | OpParams::RotaryOffset { stock_to_leave, .. }
        // The stamped ladder leaves the pocket's shelf, same law.
        | OpParams::MultiAxisRough { stock_to_leave, .. }
        // The rung ladder leaves a radial wall read as a distance to
        // the part - which is exactly what the scan measures
        // (M5.2 B): every feed point must stand nose+leave off.
        // The blade ladder leaves the same skin on the groove floor,
        // and the meridian verdict gates REST by exactly tol+leave
        // (M5.2 C).
        | OpParams::ProfileRough { stock_to_leave, .. }
        | OpParams::GrooveRough { stock_to_leave, .. } => *stock_to_leave,
    };
    match &frame {
        None => {
            let mut warns = verify::verify(
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
                            | OpParams::MultiAxisRough { .. }
                            | OpParams::Flow5D { .. }
                            // The chain rides the model: the scan
                            // measures the rest against these facets,
                            // the same depth-dependent law (M5.1 J).
                            | OpParams::MultiAxisContour { .. }
                            // Facing rides the fitted axis and its
                            // stations stop at the profile's own
                            // radius: the rest is what the facets
                            // say, measured of revolution (M5.2 A).
                            | OpParams::Facing { .. }
                            // The profile rungs and the ride read
                            // the same revolution's rest - the
                            // silhouette IS the model's own envelope
                            // (M5.2 B).
                            | OpParams::ProfileRough { .. }
                            | OpParams::ProfileFinish { .. }
                            // The blades judge against the same
                            // facets - the meridian scan reads this
                            // very silhouette as E(s), the finished
                            // part's own chord polyline (M5.2 C).
                            | OpParams::GrooveRough { .. }
                            | OpParams::GrooveFinish { .. }
                            | OpParams::Part { .. }
                            // The wave rides the same facets: the
                            // modeled thread's silhouette IS what the
                            // lattice must land in (M5.2 D). The
                            // contour rides promise the very skin
                            // this scan measures - door and verdict
                            // read the same polyline (M5.2 E).
                            | OpParams::Threading { .. }
                            | OpParams::Chamfering { .. }
                            | OpParams::TaperTurning { .. }
                            | OpParams::MultiPassTurning { .. }
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
                        // law's budget - its only reading (E2), and
                        // the stamped ladder reads it the same way
                        // (E3): the positions are the pocket's, the
                        // verdict measures them under the pose.
                        | OpParams::RotaryOffset { tolerance, .. }
                        | OpParams::MultiAxisRough { tolerance, .. }
                        // Flow is the finish's law on turning lines:
                        // the tolerance is the envelope budget, as
                        // always in this group (M5.1 I). The
                        // drive-curve contour reads it the same way -
                        // its ONLY reading, the path itself comes from
                        // the picks (M5.1 J).
                        | OpParams::Flow5D { tolerance, .. }
                        | OpParams::MultiAxisContour { tolerance, .. }
                        // Facing reads it the same way - its ONLY
                        // reading: the stations come from the
                        // profile, the budget judges them (M5.2 A).
                        // The profile kinds read it TWICE (the
                        // kernel chords its ride at it) and stand
                        // before this same budget (M5.2 B).
                        | OpParams::Facing { tolerance, .. }
                        | OpParams::ProfileRough { tolerance, .. }
                        | OpParams::ProfileFinish { tolerance, .. }
                        // The contour rides read it the same way the
                        // profile kinds do - TWICE: the kernel chords
                        // the ride at it, the door judges the promised
                        // line at it, and the envelope scan measures
                        // the same skin under this budget (M5.2 E).
                        // Without it the verdict cannot measure, and
                        // an unmeasurable Envelope op refuses.
                        | OpParams::Chamfering { tolerance, .. }
                        | OpParams::TaperTurning { tolerance, .. }
                        | OpParams::MultiPassTurning { tolerance, .. } => Some(*tolerance),
                        _ => None,
                    },
                    fixtures: &fixture_boxes,
                    spindle,
                },
            );
            // The revolved-blade verdict (M5.2 C): the meridian
            // scan of what the bands leave. No sphere can speak it
            // - a grooved part is undercut BY DESIGN, material
            // standing beside the kerf out of every ball's reach
            // along Z - so the envelope law stands the family down
            // and the truth is measured here, against these very
            // facets, move-attributed into the same vocabulary.
            if let Some(tol) = match &op.params {
                OpParams::GrooveRough { tolerance, .. }
                | OpParams::GrooveFinish { tolerance, .. }
                | OpParams::Part { tolerance, .. } => Some(*tolerance),
                _ => None,
            } {
                warns.extend(crate::turning::blade_warnings(
                    &material.facets,
                    &path,
                    tool.width,
                    tol,
                    stl,
                ));
            }
            // The threading verdict (M5.2 D): the path's own helix
            // states the lattice - every second arc endpoint is a
            // meridian crossing, where the V's floor belongs - and
            // the same meridian scan reads it with the WAVE clause:
            // the tool's V, not a boxcar, says what stands between
            // the turns. The insert's crest width and included
            // angle ride from the tool, the blade's law again.
            if let OpParams::Threading { tolerance, .. } = &op.params {
                warns.extend(crate::turning::thread_warnings(
                    &material.facets,
                    &path,
                    tool.width,
                    tool.tip_angle,
                    *tolerance,
                ));
            }
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

/// The tool above the ball, as stage F's dodge reads it: the same
/// stack `verify`'s law (c) measures the posted path against, built
/// here at the seam where the ToolDef's diameters halve to radii.
/// One builder for all three axis-law kinds, so the generator and the
/// post-time verifier can never disagree about what the shank is.
fn shank_of(tool: &crate::tools::Tool) -> crate::multiaxis::Shank {
    crate::multiaxis::Shank {
        flute_len: tool.flute_len,
        shank_r: tool.shank_d * 0.5,
        shank_len: tool.shank_len,
        holder_r: tool.holder_d * 0.5,
        holder_len: tool.holder_len,
    }
}
