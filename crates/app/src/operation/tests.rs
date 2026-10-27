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
    // `is_sane` is structural - finite numbers, positive increments,
    // in-range slots; ordering needs resolved heights and lives in
    // `depth_sane`, which commands run with the world in hand. An
    // unresolved reference is never a lie there: refusing happens at
    // generate, by name.
    let world = crate::world::new_world();
    let ok = OpParams::Face { tilt: None,
        stepover: 8.0,
        clearance_z: Height::fixed(5.0),
        cut_z: Height::fixed(-0.5),
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    assert!(ok.is_sane() && depth_sane(&world, None, &ok));
    // Staged facing is sane when it descends: a top below the
    // finished height, or a zero stepdown, is not.
    let staged = OpParams::Face { tilt: None,
        stepover: 8.0,
        clearance_z: Height::fixed(5.0),
        cut_z: Height::fixed(-0.5),
        top_z: Some(Height::fixed(1.0)),
        stepdown: Some(0.5),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    assert!(staged.is_sane() && depth_sane(&world, None, &staged));
    let low = OpParams::Face { tilt: None,
        stepover: 8.0,
        clearance_z: Height::fixed(5.0),
        cut_z: Height::fixed(-0.5),
        top_z: Some(Height::fixed(-0.6)),
        stepdown: Some(0.5),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    assert!(low.is_sane(), "structurally fine - the order is the flaw");
    assert!(
        !depth_sane(&world, None, &low),
        "staged top below the finished height"
    );
    let no_step = OpParams::Face { tilt: None,
        stepover: 8.0,
        clearance_z: Height::fixed(5.0),
        cut_z: Height::fixed(-0.5),
        top_z: Some(Height::fixed(1.0)),
        stepdown: Some(0.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    assert!(!no_step.is_sane(), "stepdown must be positive when set");
    let bad = OpParams::Pocket { tilt: None,
        stepover: f64::NAN,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(0.0),
        bottom_z: Height::fixed(-4.0),
        stepdown: 2.0,
        feed: 400.0,
        plunge_feed: 80.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        max_engagement: 45.0,
    };
    assert!(!bad.is_sane());
    // A depth range that runs upward is nonsense, refused at command
    // creation - not by the shape of the numbers but by their z's.
    let up = OpParams::Trace { tilt: None,
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(-4.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 1.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    assert!(up.is_sane());
    assert!(!depth_sane(&world, None, &up));
    // An out-of-range slot is structural: there is no seventh offset.
    let ghost = OpParams::Trace { tilt: None,
        clearance_z: Height::fixed(5.0),
        retract_z: None,
        top_z: Height::fixed(5.0),
        bottom_z: Height::of(HeightReference::WorkOffset(6), 0.0),
        stepdown: 1.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    assert!(!ghost.is_sane(), "work-offset references must name a real slot");
}

#[test]
fn stamp_follows_every_input() {
    // Tool numerics, params and source must each move the digest; the
    // world pieces are exercised in tests/operations.rs, the pure
    // param path is deterministic here (Fixed heights resolve without
    // touching the world).
    let world = crate::world::new_world();
    let a = OpParams::Face { tilt: None,
        stepover: 8.0,
        clearance_z: Height::fixed(5.0),
        cut_z: Height::fixed(-0.5),
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    // (No struct-update syntax on enum variants: write it out.)
    let b = OpParams::Face { tilt: None,
        stepover: 8.0,
        clearance_z: Height::fixed(5.0),
        cut_z: Height::fixed(-0.6),
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let c = OpParams::Face { tilt: None,
        stepover: 8.0,
        clearance_z: Height::fixed(5.0),
        cut_z: Height::fixed(-0.5),
        top_z: Some(Height::fixed(1.0)),
        stepdown: Some(0.5),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let mut ha = std::hash::DefaultHasher::new();
    let mut hb = std::hash::DefaultHasher::new();
    let mut hc = std::hash::DefaultHasher::new();
    a.stamp_with(&world, None, &mut ha);
    b.stamp_with(&world, None, &mut hb);
    c.stamp_with(&world, None, &mut hc);
    assert_ne!(ha.finish(), hb.finish());
    assert_ne!(ha.finish(), hc.finish(), "staging moves the stamp");
    // The registry law (M5.0.2): one row per kind, table order ==
    // OpType::ordinal == digest tag, labels exactly as Display quotes
    // them. The tags are digest bytes - drift here greys every cache.
    // (Stage E moved this pin with its two rows, stage E2 with the
    // wrap ladder's, stage E3 with the stamped one, stage I with the
    // flowline's, stage J with the drive-curve's: the registry law is
    // checked from the table itself, so the count is the only stale
    // number a stage can leave behind - and that is exactly what
    // happened once: Facing arrived in M5.2 A without a bump, and
    // stage B's two profile rows carry the count to 21 - stage C's
    // three blade rows to its true 24.)
    assert_eq!(KindInfo::ALL.len(), 24);
    for (i, row) in KindInfo::ALL.iter().enumerate() {
        assert_eq!(row.ty.ordinal(), i, "row {i} out of step with ordinal");
        assert_eq!(row.tag as usize, i, "digest tag must equal table order");
        assert_eq!(KindInfo::ALL[row.ty.ordinal()].ty, row.ty);
        assert_eq!(row.ty.to_string(), ALL_LABELS[i], "label drift");
    }
}

#[test]
fn face_stamp_moves_with_stepdown() {
    // A cached single-level facing must never be replayed as the
    // answer for a staged one - and two stagings that cut different
    // stacks must not claim each other's cache either.
    let world = crate::world::new_world();
    let face_with = |top_z: Option<Height>, stepdown: Option<f64>| OpParams::Face { tilt: None,
        stepover: 8.0,
        clearance_z: Height::fixed(5.0),
        cut_z: Height::fixed(-0.5),
        top_z,
        stepdown,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let digest = |p: &OpParams| {
        let mut h = std::hash::DefaultHasher::new();
        p.stamp_with(&world, None, &mut h);
        h.finish()
    };
    let legacy = face_with(None, None);
    let staged = face_with(Some(Height::fixed(1.0)), Some(0.5));
    let deeper = face_with(Some(Height::fixed(1.0)), Some(0.4));
    assert_ne!(digest(&legacy), digest(&staged), "staging invalidates");
    assert_ne!(digest(&staged), digest(&deeper), "step size is geometry");
    assert_eq!(
        digest(&staged),
        digest(&face_with(Some(Height::fixed(1.0)), Some(0.5)))
    );
}

#[test]
fn advanced_swarf_stamp_moves_with_passes() {
    // The ladder's rung count is geometry (M5.1 stage D): a cached
    // one-rung swarf must never answer for a rough-plus-finish ladder,
    // and the plain kind is a DIFFERENT digest tag (row 10 vs row 11),
    // not the ladder's bytes wearing different trousers.
    let world = crate::world::new_world();
    let ladder = |passes: u32| OpParams::AdvancedSwarf {
        tolerance: 0.05,
        stepover: None,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(-5.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
        passes,
    };
    let digest = |p: &OpParams| {
        let mut h = std::hash::DefaultHasher::new();
        p.stamp_with(&world, None, &mut h);
        h.finish()
    };
    assert_ne!(digest(&ladder(1)), digest(&ladder(2)), "a rung is geometry");
    assert_eq!(digest(&ladder(2)), digest(&ladder(2)));
    let plain = OpParams::Swarf {
        tolerance: 0.05,
        stepover: None,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(-5.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    };
    assert_ne!(digest(&plain), digest(&ladder(1)), "the family's rows must digest apart");
}

#[test]
fn rotary_twins_stamp_apart_and_with_their_stepover() {
    // M5.1 stage E, the registry's law at the digest: rows 12 and 13
    // differ from every earlier row by their tag byte, the twins
    // differ from each other (the contour mints no stepover bytes at
    // all), and the parallel's Some-or-derived spacing must never
    // digest as the number 0 - the raster kinds' split, ride again.
    let world = crate::world::new_world();
    let contour = OpParams::RotaryContour {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(-5.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    };
    let parallel = |stepover: Option<f64>| OpParams::RotaryParallel {
        tolerance: 0.05,
        stepover,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(-5.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 90.0,
    };
    let digest = |p: &OpParams| {
        let mut h = std::hash::DefaultHasher::new();
        p.stamp_with(&world, None, &mut h);
        h.finish()
    };
    assert_ne!(digest(&contour), digest(&parallel(None)), "the twins are different rows");
    assert_ne!(digest(&parallel(None)), digest(&parallel(Some(1.5))), "a derived spacing is not the number 0");
    assert_eq!(digest(&parallel(Some(1.5))), digest(&parallel(Some(1.5))));
}

#[test]
fn rotary_offset_stamp_moves_with_its_wrap_numbers() {
    // M5.1 stage E2, row 14: the ladder's own bytes - a spacing that
    // is a plain number (no Some-or-derived dance), the radial
    // infeed, the strategy byte - must each move the stamp, and the
    // kind must digest apart from the twins it shares the axis with
    // purely by its tag.
    let world = crate::world::new_world();
    let offset = |stepover: f64, stepdown: f64, strategy: PocketStrategy| {
        OpParams::RotaryOffset {
            tolerance: 0.05,
            stepover,
            stock_to_leave: 0.0,
            clearance_z: Height::fixed(25.0),
            top_z: Height::fixed(10.0),
            bottom_z: Height::fixed(-5.0),
            stepdown,
            feed: 800.0,
            plunge_feed: 150.0,
            climb: true,
            strategy,
            entry: crate::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            max_engagement: 45.0,
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: 90.0,
        }
    };
    let digest = |p: &OpParams| {
        let mut h = std::hash::DefaultHasher::new();
        p.stamp_with(&world, None, &mut h);
        h.finish()
    };
    let auto = PocketStrategy::Auto;
    assert_ne!(
        digest(&offset(4.0, 3.0, auto)),
        digest(&offset(5.0, 3.0, auto)),
        "the developed spacing is geometry"
    );
    assert_ne!(
        digest(&offset(4.0, 3.0, auto)),
        digest(&offset(4.0, 2.0, auto)),
        "a shallower radial infeed is a different ladder"
    );
    assert_ne!(
        digest(&offset(4.0, 3.0, auto)),
        digest(&offset(4.0, 3.0, PocketStrategy::Waterline)),
        "the strategy byte is read by the kernel"
    );
    assert_eq!(
        digest(&offset(4.0, 3.0, auto)),
        digest(&offset(4.0, 3.0, auto))
    );
}

#[test]
fn multiaxis_rough_stamp_moves_with_its_ladder_and_cone() {
    // M5.1 stage E3, row 15: the stamped ladder shares Pocket3D's
    // bridge bytes, so the tag must split them even at identical
    // numbers - and the cone the poses live inside is kernel input
    // here (the stamp reads it), not window dressing: each angle
    // moves the stamp on its own.
    let world = crate::world::new_world();
    let rough = |lead: f64, lag: f64, tilt: f64| OpParams::MultiAxisRough {
        tolerance: 0.05,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(-5.0),
        stepdown: 3.0,
        feed: 800.0,
        plunge_feed: 150.0,
        climb: true,
        strategy: PocketStrategy::Auto,
        entry: crate::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        max_engagement: 45.0,
        lead_deg: lead,
        lag_deg: lag,
        max_tilt_deg: tilt,
        shank_avoid: false,
    };
    let digest = |p: &OpParams| {
        let mut h = std::hash::DefaultHasher::new();
        p.stamp_with(&world, None, &mut h);
        h.finish()
    };
    assert_ne!(digest(&rough(0.0, 0.0, 90.0)), digest(&rough(10.0, 0.0, 90.0)), "lead poses every flank");
    assert_ne!(digest(&rough(0.0, 0.0, 90.0)), digest(&rough(0.0, 7.0, 90.0)), "lag is the stamp's too");
    assert_ne!(digest(&rough(0.0, 0.0, 90.0)), digest(&rough(0.0, 0.0, 45.0)), "the cone caps the lean");
    let pocket = OpParams::Pocket3D {
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(-5.0),
        stepdown: 3.0,
        feed: 800.0,
        plunge_feed: 150.0,
        climb: true,
        strategy: PocketStrategy::Auto,
        entry: crate::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        rest_rough: false,
        max_engagement: 45.0,
        feed_opt: false,
    };
    assert_ne!(
        digest(&rough(0.0, 0.0, 90.0)),
        digest(&pocket),
        "the same ladder with poses is a different kind - the tag splits them"
    );
    assert_eq!(digest(&rough(0.0, 0.0, 90.0)), digest(&rough(0.0, 0.0, 90.0)));
}
