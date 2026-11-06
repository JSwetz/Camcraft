//! The shared kind editors the family pages call (split out of
//! `pages.rs` with the property grid, user review 2026-11-06): the
//! roughing twins' pane, the 5-axis lead / lag / tilt trio, the shank
//! avoidance switch, the swarf and rotary panels. Rows ride the `layout`
//! form helpers; every value edits the page draft.
//!
//! Provenance: our own port of the M-c / M5.1 page code (each motion law
//! named in its doc comment lives in its own crate with its sources); no
//! vendor material involved.

use super::fields::*;

/// The roughing twins' shared pane (M2.4 A): the pocket's fields minus
/// the Strategy combo - the kind IS the strategy, so the choice that
/// would undo the name is not offered. `tag` scopes the height-editor
/// ids ("adaptive"/"offset"); the engagement door rides along as an
/// Option so the law "a section header renders exactly when its kind
/// has fields" stays one code path for both catalog names.
#[allow(clippy::too_many_arguments)]
pub(super) fn roughing2d_pane(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    op_ctx: Option<&str>,
    u: Unit,
    tab: usize,
    tag: &str,
    stepover: &mut f64,
    stock_to_leave: &mut f64,
    clearance_z: &mut Height,
    retract_z: &mut Option<Height>,
    top_z: &mut Height,
    bottom_z: &mut Height,
    stepdown: &mut f64,
    feed: &mut f64,
    plunge_feed: &mut f64,
    climb: &mut bool,
    entry: &mut camcraft_app::machining3d::Entry3D,
    helix_radius: &mut f64,
    optimal_load: Option<&mut f64>,
    links: &mut camcraft_app::links::Links,
    tilt: &mut Option<Tilt>,
) {
    if tab == SEC_HEIGHTS {
        orientation_editor(ui, u, tilt);
        height_editor(ui, "Clearance Z", world, op_ctx, clearance_z, &format!("{tag}.clearance"));
        height_editor(ui, "Top Z", world, op_ctx, top_z, &format!("{tag}.top"));
        height_editor(ui, "Bottom Z", world, op_ctx, bottom_z, &format!("{tag}.bottom"));
        group(ui, LEVELS, None, |ui| {
            retract_hop(ui, world, op_ctx, retract_z, top_z, clearance_z, &format!("{tag}.retract"));
            prop_row(ui, "Stepdown", |ui| num(ui, u, stepdown));
        });
    }
    if tab == SEC_PASSES {
        prop_row(ui, "Stepover", |ui| num(ui, u, stepover));
        prop_row(ui, "Stock to leave", |ui| num(ui, u, stock_to_leave));
        match optimal_load {
            Some(m) => super::load_field::optimal_load_field(ui, m),
            None => {
                hint(ui, "concentric rings from the boundary inward, one stepover apart");
            }
        }
    }
    if tab == SEC_LEADS {
        lead_page::leads_tab(ui, u, tag, Some((entry, helix_radius)), links);
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
        prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
        check_row(ui, "Climb milling", climb);
    }
}

/// The axis trio both 5-axis kinds carry (M5.1 stage C). The law lives
/// in `camcraft_app::multiaxis::tilt`: lead leans along the feed, lag
/// leans across it, and the max tilt is the cone half-angle the
/// spindle may open from the surface normal - past it the commanded
/// direction folds onto the cone's edge. All three are degrees, so
/// they ride `num_plain`: the unit is a lens on lengths and feeds,
/// never on angles.
pub(super) fn axis_editor(ui: &mut Ui, lead_deg: &mut f64, lag_deg: &mut f64, max_tilt_deg: &mut f64) {
    prop_row(ui, "Tool lead angle", |ui| num_unit(ui, lead_deg, "°"));
    hint(ui, "lean with the feed, off the surface normal - tips the shank away from the wall ahead of the cut");
    prop_row(ui, "Tool lag angle", |ui| num_unit(ui, lag_deg, "°"));
    hint(ui, "lean across the feed (side tilt) - clears the wall the tool has just cut");
    prop_row(ui, "Max tilt", |ui| num_unit(ui, max_tilt_deg, "°"));
    hint(ui, "the cone the spindle may open from the normal; where the surface demands more, the lean folds onto the cone's edge");
}

/// Stage F's one switch (M5.1): where the posed axis would drive the
/// shank into the part, the spindle tilts to the nearest clear angle
/// inside the max-tilt cone - the shortest travel from the pose it
/// held, so neighboring cuts do not swing the head back and forth.
/// The ball's envelope is rotation-invariant, so this can only ever
/// change which way the spindle stands, never where the cut lands;
/// and where no angle inside the cone clears, the operation refuses
/// by name rather than post a path the verifier would warn on.
pub(super) fn shank_avoid_editor(ui: &mut Ui, shank_avoid: &mut bool) {
    check_row(ui, "Avoid shank collisions", shank_avoid);
    hint(ui, "tilt the spindle off a leaning pose that would rub the shank; positions never move, and a wall no tilt clears refuses the job");
}

/// The swarf family's panel (M5.1 stage D): the raster's sections
/// worded for the ruling, in one body both kinds share. `passes` is
/// Some only for the ladder - the or-pattern cannot name a field one
/// variant lacks, so the arms hand their extra word in as data.
#[allow(clippy::too_many_arguments)] // flat panel args, same shape as `cell`
pub(super) fn swarf_editor(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    u: Unit,
    tab: usize,
    tolerance: &mut f64,
    stepover: &mut Option<f64>,
    clearance_z: &mut Height,
    retract_z: &mut Option<Height>,
    top_z: &mut Height,
    bottom_z: &mut Height,
    feed: &mut f64,
    plunge_feed: &mut f64,
    climb: &mut bool,
    lead_deg: &mut f64,
    lag_deg: &mut f64,
    max_tilt_deg: &mut f64,
    passes: Option<&mut u32>,
) {
    if tab == SEC_HEIGHTS {
        height_editor(ui, "Clearance Z", world, setup, clearance_z, "swarf.clearance");
        height_editor(ui, "Top Z", world, setup, top_z, "swarf.top");
        height_editor(ui, "Bottom Z", world, setup, bottom_z, "swarf.bottom");
        hint(ui, "Top is the first line's plane, Bottom the last one's");
        group(ui, LEVELS, None, |ui| {
            retract_hop(ui, world, setup, retract_z, top_z, clearance_z, "swarf.retract");
        });
    }
    if tab == SEC_PASSES {
        auto_opt(ui, "Stepover", "Auto", stepover, "scallop law", (24.0 * tolerance.max(1e-9)).sqrt(), |ui, s| {
            num(ui, u, s)
        });
        hint(ui, "Auto: from tolerance and tool by the scallop law, across the ruling. Stated: spacing ACROSS the ruling; along it the lines never part - the surface's own direction spaces them");
        // The ladder's rung count, the rung-count law: a plain number
        // clamped into the generator's named range at the draft, so
        // no panel edit ever holds a 0 or a 9. Named "Pass count" - a
        // field reading exactly "Passes" would echo its own tab's label
        // (M9.1 B named this by pinning headers; the tab bar keeps it).
        if let Some(passes) = passes {
            let mut n = f64::from(*passes);
            prop_row(ui, "Pass count", |ui| num_plain(ui, &mut n));
            *passes = n.clamp(1.0, 4.0).round() as u32;
            hint(ui, "coarse-to-fine runs: each rough pass relaxes the tolerance fourfold (and doubles the spacing), the last cuts to the value set below");
        }
        axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
        prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
        hint(ui, "the finish budget: lines are spaced from it, so the cusp along the flank stays within it");
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
        prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
        check_row(ui, "Climb milling", climb);
    }
}

/// The rotary twins' panel (M5.1 stage E): the swarf body worded for
/// the turn. `stepover` is Some only for the parallel kind - the
/// contour's stations drop by the scallop law alone, and an or-pattern
/// cannot name a field one variant lacks, so the arms hand their
/// spacing in as data.
#[allow(clippy::too_many_arguments)] // flat panel args, same shape as `swarf_editor`
pub(super) fn rotary_editor(
    ui: &mut Ui,
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    u: Unit,
    tab: usize,
    tolerance: &mut f64,
    stepover: Option<&mut Option<f64>>,
    clearance_z: &mut Height,
    retract_z: &mut Option<Height>,
    top_z: &mut Height,
    bottom_z: &mut Height,
    feed: &mut f64,
    plunge_feed: &mut f64,
    climb: &mut bool,
    lead_deg: &mut f64,
    lag_deg: &mut f64,
    max_tilt_deg: &mut f64,
) {
    if tab == SEC_HEIGHTS {
        height_editor(ui, "Clearance Z", world, setup, clearance_z, "rotary.clearance");
        height_editor(ui, "Top Z", world, setup, top_z, "rotary.top");
        height_editor(ui, "Bottom Z", world, setup, bottom_z, "rotary.bottom");
        hint(ui, "the world planes map onto stations along the fitted axis - Top where the run starts, Bottom where it ends");
        group(ui, LEVELS, None, |ui| {
            retract_hop(ui, world, setup, retract_z, top_z, clearance_z, "rotary.retract");
        });
    }
    if tab == SEC_PASSES {
        if let Some(stepover) = stepover {
            auto_opt(ui, "Stepover", "Auto", stepover, "scallop law", (24.0 * tolerance.max(1e-9)).sqrt(), |ui, s| {
                num(ui, u, s)
            });
            hint(ui, "Auto: from tolerance and tool by the scallop law, around the turn. Stated: spacing AROUND the turn; along the axis the lines never part - the cylinder's own direction spaces them");
        }
        rotary_axis_editor(ui, lead_deg, lag_deg, max_tilt_deg);
        prop_row(ui, "Tolerance", |ui| num(ui, u, tolerance));
        hint(ui, "the finish budget: stations and lines follow from it, so the cusp under the ball stays within it");
    }
    if tab == SEC_FEEDS {
        prop_row(ui, "Feed", |ui| num_feed(ui, u, feed));
        prop_row(ui, "Plunge feed", |ui| num_feed(ui, u, plunge_feed));
        check_row(ui, "Climb milling", climb);
    }
}

/// The axis trio, worded for the rotary twins: the leans pose off the
/// RADIAL, not the surface normal - the wall of a turn answers for the
/// whole ring at once, and the law lives in
/// `camcraft_motion::multiaxis::rotary` (M5.1 stage E).
pub(super) fn rotary_axis_editor(ui: &mut Ui, lead_deg: &mut f64, lag_deg: &mut f64, max_tilt_deg: &mut f64) {
    prop_row(ui, "Tool lead angle", |ui| num_unit(ui, lead_deg, "°"));
    hint(ui, "lean with the feed, off the radial from the fitted axis - tips the shank away from the wall ahead of the cut");
    prop_row(ui, "Tool lag angle", |ui| num_unit(ui, lag_deg, "°"));
    hint(ui, "lean across the feed (side tilt), off the radial - clears the wall the tool has just cut");
    prop_row(ui, "Max tilt", |ui| num_unit(ui, max_tilt_deg, "°"));
    hint(ui, "the cone the spindle may open from the radial; where the stance demands more, the lean folds onto the cone's edge");
}
