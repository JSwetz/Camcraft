//! Split from `types.rs` by M5.0.1; structure only.

use crate::machining2d::PocketStrategy;

use crate::operation::types::params::OpParams;
use crate::operation::types::sel::lead_sane;


impl OpParams {
    /// Shallow sanity for command construction, no world: finite numbers,
    /// positive increments, in-range slots. Ordering needs resolved
    /// heights and runs in [`crate::operation::depth_sane`] wherever a command holds the
    /// world; the generators own the deep rules (stepover vs diameter,
    /// lead sizes), which still fire inside `GenerateOperation`'s
    /// constructor.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        let pos = |v: f64| v.is_finite() && v > 0.0;
        let nneg = |v: f64| v.is_finite() && v >= 0.0;
        // The 3+2 index answers at the door (M5.1 stage B): a tilt is
        // only ever storable on the planar kinds, so one check covers
        // all eight arms - `tilt()` is None for the rest by the
        // accessor's exhaustive law. The mounted machine's REACH is a
        // different question and belongs to generation, which names
        // the machine when the angle is past it.
        let tilt_ok = self.tilt().is_none_or(|t| t.is_sane());
        tilt_ok
            && match self {
            OpParams::Face {
                stepover,
                clearance_z,
                cut_z,
                top_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                pos(*stepover)
                    && clearance_z.is_sane()
                    && cut_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && top_z.is_none_or(|t| t.is_sane())
                    && stepdown.is_none_or(|s| s.is_finite() && s > 0.0)
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
                    && clearance_z.is_sane()
                    && retract_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
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
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                strategy,
                max_engagement,
                ..
            } => {
                // The entry family joined the planar pocket at M5.1 H,
                // so its door mirrors Pocket3D's word for word: the
                // generator owns the radius-vs-tool check (it owns the
                // tool), and the engagement angle is finite always,
                // bounded (0, 90] only when Adaptive reads it.
                pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
                    && (max_engagement.is_finite()
                        && (!matches!(strategy, PocketStrategy::Adaptive)
                            || (*max_engagement > 0.0 && *max_engagement <= 90.0)))
            }
            OpParams::Pocket3D {
                stepover,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                strategy,
                max_engagement,
                ..
            } => {
                // Structural only, like the pocket's: the helix radius
                // is checked against the tool radius by the generator,
                // which is the only reader that owns a tool. A Plunge
                // entry never consults it, so any positive finite value
                // - the default 1.0 among them - is sane here. Same
                // law for the engagement angle: it must be finite
                // always, and inside its own (0, 90] only when the
                // Adaptive strategy actually reads it - the generator
                // repeats the bound as the refusal of record.
                pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
                    && (max_engagement.is_finite()
                        && (!matches!(strategy, PocketStrategy::Adaptive)
                            || (*max_engagement > 0.0 && *max_engagement <= 90.0)))
            }
            OpParams::Finish3D {
                stepover,
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                min_step,
                ramp_angle,
                ..
            } => {
                // No stepdown to check: the scallop law derives it from
                // tolerance and the tool radius, and the generator's
                // level cap refuses a tolerance that would need more
                // rings than the machine of record may hold.
                pos(*tolerance)
                    && stepover.is_none_or(pos)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && min_step.is_none_or(pos)
                    // (0, 90): a flat ramp never descends - that is
                    // a traverse with a name it doesn't earn; a
                    // vertical one is a plunge, and the run's first
                    // contact already owns those. Both refusals
                    // happen at the gate, never at the emitter.
                    && (0.0..90.0).contains(ramp_angle)
            }
            OpParams::Contour5D {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            | OpParams::Parallel5D {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            // The swarf door (M5.1 stage D) is the raster's door with
            // the ruling added at generation (a dome refuses there,
            // by name - nothing in these numbers says "ruled"): the
            // leans still pose the unruled floors and rims the blade
            // family shares its paths with.
            | OpParams::Swarf {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            // The rotary door (M5.1 stage E) is the same door: the
            // leans now pose off the RADIAL instead of the normal,
            // but a quadrant is just as far as a tool may ever look
            // along the wall it rides. The axis the fit reads is
            // model data - it refuses at generation, by name, and
            // needs no number here.
            | OpParams::RotaryContour {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            | OpParams::RotaryParallel {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            // Flow's door (M5.1 I) is the raster twins' - the field
            // that bends the lines is model data, refused or honored
            // at generation, and needs no number here. The
            // drive-curve contour stands in the same doorway (M5.1 J):
            // no stepover to question - the chain is the path - so it
            // falls to the inner match's `_` arm, and its top/bottom
            // are the window the picked vertices must live in.
            | OpParams::Flow5D {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            }
            | OpParams::MultiAxisContour {
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            } => {
                // The finish door's shape (no stepdown - the scallop
                // law authors it), plus the pose door (M5.1 stage C):
                // the two leans are angles OFF the normal, so each is
                // confined to a quadrant - past 90 degrees the tool
                // would point along the surface it is cutting, and no
                // combination of lead and lag states is meant to reach
                // for that. The clamp is a cone half-angle: above 180
                // nothing is excluded, and at or below 0 the cone
                // holds no direction at all - both are typos about the
                // machine's reach, refused here rather than silently
                // ignored at the stamp. The raster kinds' optional
                // stepover rides the same Some-or-derived law as the
                // finish's.
                let angle = |v: &f64| v.is_finite() && v.abs() <= 90.0;
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && angle(lead_deg)
                    && angle(lag_deg)
                    && *max_tilt_deg > 0.0
                    && *max_tilt_deg <= 180.0
                    && (match self {
                        OpParams::Parallel5D { stepover, .. }
                        | OpParams::Swarf { stepover, .. }
                        | OpParams::RotaryParallel { stepover, .. }
                        | OpParams::Flow5D { stepover, .. } => stepover.is_none_or(pos),
                        _ => true,
                    })
            }
            // The wrap ladder's door (M5.1 stage E2): the Pocket3D
            // door's numbers - plain stepover, radial stepdown, the
            // helix and engagement read by the entry law - crossed
            // with the rotary door's pose clause. The tolerance is
            // the verify budget here, not a scallop input, but a
            // sphere sweep still needs a positive one to be measured
            // against. There is no retract to check: the field does
            // not exist - the kernel names its own rapids plane.
            OpParams::RotaryOffset {
                tolerance,
                stepover,
                stock_to_leave,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                strategy,
                max_engagement,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            } => {
                let angle = |v: &f64| v.is_finite() && v.abs() <= 90.0;
                pos(*tolerance)
                    && pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
                    && (max_engagement.is_finite()
                        && (!matches!(strategy, PocketStrategy::Adaptive)
                            || (*max_engagement > 0.0 && *max_engagement <= 90.0)))
                    && angle(lead_deg)
                    && angle(lag_deg)
                    && *max_tilt_deg > 0.0
                    && *max_tilt_deg <= 180.0
            }
            // The stamped ladder's door (M5.1 stage E3): the Pocket3D
            // door verbatim - the hop plane included, which the plain
            // pocket stores and the wrap discards - plus the pose
            // clause and the verify budget. Positions are the
            // pocket's, so what is sane for one is sane for the
            // other; only the angles are new questions.
            OpParams::MultiAxisRough {
                tolerance,
                stepover,
                stock_to_leave,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                helix_radius,
                strategy,
                max_engagement,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                ..
            } => {
                let angle = |v: &f64| v.is_finite() && v.abs() <= 90.0;
                pos(*tolerance)
                    && pos(*stepover)
                    && nneg(*stock_to_leave)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && pos(*helix_radius)
                    && (max_engagement.is_finite()
                        && (!matches!(strategy, PocketStrategy::Adaptive)
                            || (*max_engagement > 0.0 && *max_engagement <= 90.0)))
                    && angle(lead_deg)
                    && angle(lag_deg)
                    && *max_tilt_deg > 0.0
                    && *max_tilt_deg <= 180.0
            }
            OpParams::AdvancedSwarf {
                tolerance,
                stepover,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                lead_deg,
                lag_deg,
                max_tilt_deg,
                passes,
                ..
            } => {
                // The swarf door, plus the ladder count: 1..4 is the
                // generator's named range (motion refuses 0 and 5+ by
                // the same numbers) - a rough above that is a file
                // typo, not a machinist's ladder, and the door says so
                // at command creation, not deep in a pass loop.
                let angle = |v: &f64| v.is_finite() && v.abs() <= 90.0;
                pos(*tolerance)
                    && stepover.is_none_or(pos)
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && angle(lead_deg)
                    && angle(lag_deg)
                    && *max_tilt_deg > 0.0
                    && *max_tilt_deg <= 180.0
                    && (1..=4).contains(passes)
            }
            OpParams::Slot {
                a,
                b,
                width,
                stock_to_leave,
                clearance_z,
                retract_z,
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
                    && clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*stepover)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Trace {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                ..
            } => {
                clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::Drill {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                peck,
                dwell,
                feed,
                plunge_feed,
                holes,
                ..
            } => {
                // An empty hole list is sane here: a new Drill op exists
                // before its holes are detected, and creation must be
                // allowed to get the panel on screen. Generation is the
                // gate that refuses it, by name.
                clearance_z.is_sane()
                    && retract_z.is_none_or(|r| r.is_sane())
                    && top_z.is_sane()
                    && bottom_z.is_none_or(|b| b.is_sane())
                    && pos(*feed)
                    && pos(*plunge_feed)
                    && peck.is_finite()
                    && *peck >= 0.0
                    && dwell.is_finite()
                    && *dwell >= 0.0
                    && holes.iter().all(|h| h.is_usable())
            }
            // The facing ladder's door (M5.2 stage A): the wrap
            // ladder's numbers without the pattern's - a positive
            // verify budget, heights, feeds, and an axial infeed
            // that must be a positive distance. Which radii the
            // stations meet is the axis fit's and the profile's
            // question: refused at generation, by name.
            OpParams::Facing {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
            } => {
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            // The profile ladders' doors (M5.2 stage B): the facing
            // numbers plus a leave that may be zero (the ride owns
            // the skin then) but never negative - the kernel refuses
            // stock to REMOVE past the door it stands behind, and so
            // does this one, by name; the stepdown ceiling against
            // the nose is the TOOL's question and stays the
            // generator's to ask.
            OpParams::ProfileRough {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                stepdown,
                stock_to_leave,
                feed,
                plunge_feed,
            } => {
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*stepdown)
                    && stock_to_leave.is_finite()
                    && *stock_to_leave >= 0.0
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
            OpParams::ProfileFinish {
                tolerance,
                clearance_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
            } => {
                pos(*tolerance)
                    && clearance_z.is_sane()
                    && top_z.is_sane()
                    && bottom_z.is_sane()
                    && pos(*feed)
                    && pos(*plunge_feed)
            }
        }
    }

}
