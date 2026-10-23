

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
    let stock_region = match &op.src {
        SrcRef::Stock => None,
        SrcRef::Entity(_) | SrcRef::Selection(_) => {
            model::find_by_name(world, &crate::setup::stock_entity_name(world, setup))
                .and_then(|e| model::world_facets(world, &e))
                .and_then(|f| slice::project(&f, FOOTPRINT_SAMPLES).ok())
                .filter(|r| !r.is_empty())
        }
    };
    let stock_top_z = material.top
       .max(
            model::stock_world_aabb_for(world, setup)
                .map(|b| b.max.z)
                .unwrap_or(f64::NEG_INFINITY),
        );

    let mode = match op.params.kind() {
        // Drill verifies as Boundary on purpose: its region's rim loops
        // put the gouge law's tangency test at the hole wall, where a
        // matched drill passes and an oversized tool is flagged. What
        // the move-by-move laws cannot see for a straight deep plunge
        // (it clears its own kerf, so the band sampling never fires)
        // the cycle reach law in `verify` checks over `path.cycles`.
        OpType::Face | OpType::Contour | OpType::Drill => Mode::Boundary,
        // The 3-D pocket verifies as Area with the mesh in hand (the
        // `model` field below): the region is its silhouette, the
        // per-level islands come from the facets.
        OpType::Pocket | OpType::Slot | OpType::Pocket3D => Mode::Area,
        OpType::Trace => Mode::Engrave,
        // The waterline's verdict is measured, not deduced from
        // tangency: law (f) of `verify` scans the swept ball against
        // the mesh (the `finish_tol` field below carries the budget).
        OpType::Finish3D => Mode::Envelope,
    };

    // The height seam: the params' From+offset expressions become the
    // generators' plain f64s right here. Every unresolved reference is
    // refused by name before a single move is computed - never a
    // guessed z. `machining2d`'s params keep taking resolved values, so
    // the generators (and the goldens) know nothing of references.
    let hs = resolve_params(world, setup, &op.params).map_err(GenError::Invalid)?;

    // Region + path per kind.
    let (region, path) = match &op.params {
        OpParams::Face { stepover, stepdown, feed, plunge_feed, climb, .. } => {
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let path = machining2d::face(
                &region,
                &FaceParams {
                    tool_diameter: d,
                    stepover: *stepover,
                    clearance_z: hs.clearance,
                    cut_z: req_z(hs.bottom, "Cut Z")?,
                    top_z: hs.face_top,
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
                clearance_z: hs.clearance,
                retract_z: req_z(hs.retract, "Retract Z")?,
                top_z: req_z(hs.top, "Top of Part Z")?,
                bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
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
                    let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
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
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let path = machining2d::pocket(
                &region,
                &PocketParams {
                    tool_diameter: d,
                    stepover: *stepover,
                    stock_to_leave: *stock_to_leave,
                    clearance_z: hs.clearance,
                    retract_z: hs.retract,
                    top_z: req_z(hs.top, "Top of Part Z")?,
                    bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
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
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let pp = Pocket3DParams {
                tool_diameter: d,
                stepover: *stepover,
                stock_to_leave: *stock_to_leave,
                clearance_z: hs.clearance,
                top_z: req_z(hs.top, "Top of Part Z")?,
                bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
                stepdown: *stepdown,
                feed: *feed,
                plunge_feed: *plunge_feed,
                climb: *climb,
                strategy: *strategy,
                retract_z: hs.retract,
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
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let fp = Finish3DParams {
                pattern: *pattern,
                stepover: *stepover,
                tool_diameter: d,
                tolerance: *tolerance,
                clearance_z: hs.clearance,
                retract_z: hs.retract,
                top_z: req_z(hs.top, "Top of Part Z")?,
                bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
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
                    clearance_z: hs.clearance,
                    retract_z: hs.retract,
                    top_z: req_z(hs.top, "Top of Part Z")?,
                    bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
                    stepdown: *stepdown,
                    stepover: *stepover,
                    feed: *feed,
                    plunge_feed: *plunge_feed,
                    climb: *climb,
                },
            )?;
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            (region, path)
        }
        OpParams::Trace { stepdown, feed, plunge_feed, climb, .. } => {
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let path = machining2d::trace(
                &region,
                &TraceParams {
                    clearance_z: hs.clearance,
                    retract_z: hs.retract,
                    top_z: req_z(hs.top, "Top of Part Z")?,
                    bottom_z: req_z(hs.bottom, "Bottom of Part Z")?,
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
            let region = region_for_src(world, op.setup.as_deref(), &op.src, &op.params, &material)?;
            let path = drilling::drill(&drilling::CycleParams {
                clearance_z: hs.clearance,
                r_z: req_z(hs.top, "Top of Part Z")?,
                hop_z: hs.retract,
                bottom_z: hs.bottom,
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
                OpParams::Pocket3D { .. } | OpParams::Finish3D { .. }
            )
            .then_some(&material.facets[..]),
            tool: &geom,
            mode,
            stock_top_z,
            clearance_z: hs.clearance,
            stock_to_leave: match &op.params {
                OpParams::Face { .. }
                | OpParams::Trace { .. }
                | OpParams::Drill { .. }
                | OpParams::Finish3D { .. } => 0.0,
                OpParams::Contour { stock_to_leave, .. }
                | OpParams::Pocket { stock_to_leave, .. }
                | OpParams::Slot { stock_to_leave, .. }
                | OpParams::Pocket3D { stock_to_leave, .. } => *stock_to_leave,
            },
            gouge_tol: GOUGE_TOL,
            z_tol: Z_TOL,
            // The finish op's envelope budget is its own tolerance; no
            // other kind runs the envelope law.
            finish_tol: match &op.params {
                OpParams::Finish3D { tolerance, .. } => Some(*tolerance),
                _ => None,
            },
            fixtures: &fixture_boxes,
            spindle,
        },
    );
    Ok((path, warns))
}
