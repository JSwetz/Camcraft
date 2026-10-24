

use super::*;

impl OpParams {
    /// The digest-relevant scalars, written to a hasher in a fixed
    /// order. A height contributes its reference (tag, plus slot for a
    /// work offset) and its offset bits; a non-Fixed reference ALSO
    /// contributes its resolved Z (or an unresolved marker). The source
    /// digests in `current_stamp` only cover the operation's own source
    /// entity - a stock-sourced Face with a `TopOfModel` cut height must
    /// go Stale when a model moves, and nothing else feeds that world in.
    /// Resolved Zs are not double-counted for Fixed heights: their
    /// offset bits are the whole truth.
    pub(super) fn stamp_with(
        &self,
        world: &World,
        setup: Option<&str>,
        h: &mut std::hash::DefaultHasher,
    ) {
        let w = |h: &mut std::hash::DefaultHasher, v: f64| h.write(&v.to_bits().to_le_bytes());
        let wh = |h: &mut std::hash::DefaultHasher, v: Height| {
            let (tag, slot) = match v.reference {
                HeightReference::Fixed => (0u8, 0u8),
                HeightReference::TopOfStock => (1, 0),
                HeightReference::BottomOfStock => (2, 0),
                HeightReference::TopOfModel => (3, 0),
                HeightReference::BottomOfModel => (4, 0),
                HeightReference::WorkOffset(i) => (5, i.min(255) as u8),
            };
            h.write_u8(tag);
            h.write_u8(slot);
            w(h, v.offset);
            if tag != 0 {
                match resolve_height(world, setup, v) {
                    Some(z) => {
                        h.write_u8(1);
                        w(h, z);
                    }
                    None => h.write_u8(0),
                }
            }
        };
        let wh_opt = |h: &mut std::hash::DefaultHasher, v: Option<Height>| match v {
            Some(v) => {
                h.write_u8(1);
                wh(h, v);
            }
            None => h.write_u8(0),
        };
        let wp2 = |h: &mut std::hash::DefaultHasher, p: Point2| {
            w(h, p.x);
            w(h, p.y);
        };
        // The 3+2 index (M5.1 stage B): a tilt changes every move the
        // generator makes, so it changes the digest. Presence first,
        // then the two letters and the anchor - the same shape the
        // optional scalars above use.
        let wt = |h: &mut std::hash::DefaultHasher, t: Option<Tilt>| match t {
            Some(t) => {
                h.write_u8(1);
                w(h, t.a_deg);
                w(h, t.b_deg);
                for v in t.origin {
                    w(h, v);
                }
            }
            None => h.write_u8(0),
        };
        // The kind tag opens the digest (M5.0.2): what was a magic
        // literal at the head of each arm is now the registry row's
        // data - the bytes written are unchanged.
        h.write_u8(self.kind().info().tag);
        match self {
            OpParams::Face {
                stepover,
                clearance_z,
                cut_z,
                top_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
                tilt,
            } => {
                w(h, *stepover);
                wh(h, *clearance_z);
                wh(h, *cut_z);
                w(h, *feed);
                w(h, *plunge_feed);
                h.write_u8(u8::from(*climb));
                // Appended at v5-era staging: presence first, then the
                // value. Even a legacy (None, None) face digest differs
                // from a pre-stamp face op - the mechanism saying so.
                wh_opt(h, *top_z);
                match *stepdown {
                    Some(v) => {
                        h.write_u8(1);
                        w(h, v);
                    }
                    None => h.write_u8(0),
                }
                wt(h, *tilt);
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
                side,
                tolerance,
                tab_count,
                tab_width,
                tab_height,
                climb,
                tilt,
            } => {
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *retract_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed, *tolerance, *tab_width, *tab_height] {
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
                    Lead::Tangential { radius } => {
                        h.write_u8(3);
                        w(h, *radius);
                    }
                }
                // Appended at the v7-era side field: even a pre-Side
                // contour op digests differently - the mechanism saying
                // so, so nothing quietly reinterprets a committed path
                // under the new Auto law.
                h.write_u8(match side {
                    Side::Auto => 0,
                    Side::Inside => 1,
                    Side::Outside => 2,
                });
                wt(h, *tilt);
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
                climb,
                strategy,
                tilt,
            } => {
                w(h, *stepover);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                h.write_u8(match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Auto => 2,
                    PocketStrategy::Adaptive => 3,
                });
                // v5-era hop plane: presence, then the value (same shape
                // as Face's staging fields appended above).
                wh_opt(h, *retract_z);
                wt(h, *tilt);
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
                climb,
                strategy,
                entry,
                helix_radius,
                rest_rough,
                max_engagement,
                feed_opt,
            } => {
                w(h, *stepover);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed, *helix_radius, *max_engagement] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                h.write_u8(match strategy {
                    PocketStrategy::Zigzag => 0,
                    PocketStrategy::Waterline => 1,
                    PocketStrategy::Auto => 2,
                    PocketStrategy::Adaptive => 3,
                });
                h.write_u8(match entry {
                    Entry3D::Plunge => 0,
                    Entry3D::Ramp => 1,
                    Entry3D::Helix => 2,
                });
                wh_opt(h, *retract_z);
                // v12-era rest pass: the flag is the generator's
                // switch, so it belongs in the digest (a model edit
                // must mark a rest path Stale like any other).
                h.write_u8(u8::from(*rest_rough));
                // Same law for the v14 feed modulation: flipping it
                // changes every F word, so it changes the stamp.
                h.write_u8(u8::from(*feed_opt));
            }
            OpParams::Finish3D {
                pattern,
                stepover,
                tolerance,
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                feed,
                plunge_feed,
                climb,
                ramp,
                min_step,
                ramp_angle,
            } => {
                // v16: the pattern chooses a different kinematic (and
                // the raster fields only answer to Parallel), so they
                // belong in the digest; the Some/None split keeps a
                // derived stepover from digesting as the number 0.
                h.write_u8(*pattern as u8);
                match stepover {
                    Some(s) => {
                        h.write_u8(1);
                        w(h, *s);
                    }
                    None => h.write_u8(0),
                }
                w(h, *tolerance);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
                // v17 (M2.2e): ramp changes the chain between lines,
                // so it belongs in the digest; the one-time staleness
                // it hands old ops is the step-up law, same as ever.
                h.write_u8(u8::from(*ramp));
                // v18 (M2.2e): the surface-finish pair. min_step
                // floors the march's refinement (same Some/None
                // split as stepover above - None must not digest as
                // the number 0), ramp_angle caps the chain's slope
                // - both change moves, both belong in the digest.
                match min_step {
                    Some(s) => {
                        h.write_u8(1);
                        w(h, *s);
                    }
                    None => h.write_u8(0),
                }
                w(h, *ramp_angle);
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
                climb,
                tilt,
            } => {
                wp2(h, *a);
                wp2(h, *b);
                w(h, *width);
                w(h, *stock_to_leave);
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *stepover, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
                wt(h, *tilt);
            }
            OpParams::Trace {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                stepdown,
                feed,
                plunge_feed,
                climb,
                tilt,
            } => {
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh(h, *bottom_z);
                for v in [*stepdown, *feed, *plunge_feed] {
                    w(h, v);
                }
                h.write_u8(u8::from(*climb));
                wh_opt(h, *retract_z);
                wt(h, *tilt);
            }
            OpParams::Drill {
                clearance_z,
                retract_z,
                top_z,
                bottom_z,
                cycle,
                peck,
                dwell,
                feed,
                plunge_feed,
                holes,
            } => {
                wh(h, *clearance_z);
                wh(h, *top_z);
                wh_opt(h, *bottom_z);
                wh_opt(h, *retract_z);
                for v in [*feed, *plunge_feed, *peck, *dwell] {
                    w(h, v);
                }
                h.write_u8(match cycle {
                    CycleChoice::Auto => 0,
                    CycleChoice::Spot => 1,
                    CycleChoice::Drill => 2,
                    CycleChoice::Peck => 3,
                    CycleChoice::Tap => 4,
                    CycleChoice::Bore => 5,
                    CycleChoice::BackBore => 6,
                });
                // The hole list is params data, so it digests: recognition
                // output, a typed edit, or a moved model (whose re-
                // detect changes the numbers) all mark the op Stale
                // through here or through the source arm.
                h.write_u32(holes.len() as u32);
                for hole in holes {
                    wp2(h, hole.xy);
                    w(h, hole.radius);
                    w(h, hole.entry_z);
                    w(h, hole.floor_z);
                    h.write_u8(u8::from(hole.through));
                }
            }
        }
    }
}
