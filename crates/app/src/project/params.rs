use super::*;

use crate::operation::Tilt;

/// The file mirror of `operation::Tilt` (M5.1 stage B): plain f64
/// letters and anchor - serde's shortest-representation floats parse
/// back to the same bits, so a round trip needs no lattice (unlike
/// the normalized axis vectors the CL stream stores).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct TiltDto {
    pub a_deg: f64,
    pub b_deg: f64,
    pub origin: [f64; 3],
}

impl From<Tilt> for TiltDto {
    fn from(t: Tilt) -> Self {
        TiltDto {
            a_deg: t.a_deg,
            b_deg: t.b_deg,
            origin: t.origin,
        }
    }
}

impl From<TiltDto> for Tilt {
    fn from(d: TiltDto) -> Self {
        Tilt {
            a_deg: d.a_deg,
            b_deg: d.b_deg,
            origin: d.origin,
        }
    }
}

/// The file mirror of `operation::OpParams`, tool-free by the same rule as
/// the domain type: the diameter lives in the tool, never a parameter.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ParamsDto {
    Face {
        stepover: f64,
        clearance_z: HeightDto,
        cut_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Declared at v5, wired in step 4: absent (or the defaults) is
        /// exactly the legacy single-level face.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        top_z: Option<HeightDto>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepdown: Option<f64>,
        /// Declared at v22-era M5.1 B: absent is no index - the
        /// prismatic op every earlier file held.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    Contour {
        stock_to_leave: f64,
        clearance_z: HeightDto,
        retract_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        lead: LeadDto,
        tolerance: f64,
        tab_count: u32,
        tab_width: f64,
        tab_height: f64,
        /// Declared at v7, wired with the side feature: absent is exactly
        /// `auto`, the choice every pre-v7 contour file silently had -
        /// Auto resolves at generate, so the stored path never changes.
        #[serde(default)]
        side: SideDto,
        /// 3+2 index; see the Face arm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    Pocket {
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: StrategyDto,
        /// Declared at v5, wired in step 5: absent is exactly "hop to
        /// clearance", the behavior every pre-v5 pocket had.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        /// Absent means `plunge` - the climb-and-dive every pocket
        /// posted before M5.1 H gave the planar ladder an entry field
        /// at all (additive defaults: the format version does not move).
        #[serde(default)]
        entry: EntryDto,
        /// Read only when `entry` is Helix; absent means the seed's
        /// 1.0 mm, the same default the 3-D pocket carries.
        #[serde(default = "default_helix_radius")]
        helix_radius: f64,
        /// Read only when `strategy` is Adaptive; absent means the
        /// law's 45° (`pocket3d`'s law, same word). Pre-H files that
        /// stored Adaptive meant the waterline alias - they now ring
        /// at r(1 - cos 45); RECOMMENDED.md owns that statement.
        #[serde(default = "default_engagement_deg")]
        max_engagement: f64,
        /// 3+2 index; see the Face arm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    Slot {
        a: [f64; 2],
        b: [f64; 2],
        width: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        stepover: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// 3+2 index; see the Face arm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    Trace {
        clearance_z: HeightDto,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// 3+2 index; see the Face arm.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tilt: Option<TiltDto>,
    },
    /// Declared at v10 with the drilling operation. The hole list is
    /// params data, not a live query: what recognition (or a typed edit)
    /// produced is what the file holds, and the source digest makes a
    /// moved model mark the op Stale beside it.
    Drill {
        clearance_z: HeightDto,
        /// Absent means the hop-to-clearance every drilling default
        /// uses, like the pocket family's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        /// The whole-pattern depth override; absent is each hole's own
        /// floor - the normal shape, so `skip_serializing_if` keeps the
        /// common file lean.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bottom_z: Option<HeightDto>,
        cycle: CycleChoiceDto,
        peck: f64,
        dwell: f64,
        feed: f64,
        plunge_feed: f64,
        holes: Vec<HoleDto>,
        /// The radial list (M5.2 F2). Absent - the common shape, since
        /// a mill document's list is empty and `skip` keeps the file
        /// byte-identical to a pre-v40 one - is "no cross holes".
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cross: Vec<CrossHoleDto>,
    },
    /// Declared at v11 with the 3-D pocket. No footprint on the wire:
    /// a 3-D op's machine area is the model's silhouette at generation
    /// time (the level law reads the mesh, not picks), and the source
    /// digest is what marks a moved model Stale beside it. The rename
    /// states the tag - the automatic snake_case of `Pocket3D` would be
    /// `pocket3_d`, and a file contract deserves the cleaner word.
    #[serde(rename = "pocket3d")]
    Pocket3D {
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: StrategyDto,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        #[serde(default)]
        entry: EntryDto,
        /// Read only when `entry` is Helix; a stored number is the
        /// user's choice, never a stale derivation from the tool.
        helix_radius: f64,
        /// Absent means off: the v11 files predate the rest pass, and
        /// the plain level ladder is exactly what they generated.
        #[serde(default)]
        rest_rough: bool,
        /// Max engagement angle, degrees; read only when `strategy` is
        /// `Adaptive`. Absent means the law's 45° - v12 files can carry
        /// no Adaptive ring family to feed it.
        #[serde(default = "default_engagement_deg")]
        max_engagement: f64,
        /// Absent means off: v13 files posted every F word at the
        /// programmed feed, which is exactly what "no modulation"
        /// means for them.
        #[serde(default)]
        feed_opt: bool,
    },
    /// Declared at v15 with the waterline. The rename states the tag
    /// for the same reason `pocket3d` does - the automatic snake_case
    /// of `Finish3D` would be `finish3_d`, and a file contract
    /// deserves the cleaner word. No stepdown key: the scallop law
    /// derives it from tolerance and tool, so storing one could only
    /// disagree with the two numbers that author it. v16 added the
    /// pattern (absent means `waterline`, what every v15 file ran)
    /// and the raster stepover - the one number the law does not
    /// author, because a stored override states a machinist's
    /// decision, not the tolerance's derivation.
    #[serde(rename = "finish3d")]
    Finish3D {
        /// Absent means Waterline (the v15 truth).
        #[serde(default)]
        pattern: FinishPatternDto,
        /// Absent follows the stepover cos-law from the tolerance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        /// Absent means no ramping - the hop, what every pre-v17 file
        /// ran (M2.2e). Written always, like `climb`.
        #[serde(default)]
        ramp: bool,
        /// Absent means the march's own floor (delta0/4096), what
        /// every pre-v18 file ran (M2.2e surface finish). Absent
        /// spells None exactly like `stepover`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_step: Option<f64>,
        /// Absent means 45 degrees - the slope limit the raster's
        /// shallow jurisdiction already grants, hence exactly what
        /// v17's chain certified (M2.2e surface finish). Written
        /// always, like `ramp`.
        #[serde(default = "default_ramp_angle")]
        ramp_angle: f64,
    },
    /// Declared at v24 with the simultaneous-axis kinds (M5.1 stage
    /// C). The renames state the tags for the familiar reason - the
    /// automatic snake_case of `Contour5D` would be `contour5_d`,
    /// and a file contract deserves the cleaner word. No `pattern`
    /// key: the kind IS the choice of kinematic (the two-kinds-not-
    /// one-table decision), and no stepdown/ramp/min_step keys: the
    /// scallop law authors the ladder and the plunge owns the
    /// entry. The axis trio is written always - it is the whole
    /// content of the kind, and an absent lean would silently
    /// re-post someone's tilted program vertical.
    #[serde(rename = "contour5d")]
    Contour5D {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Absent means off - the pose law every v28 file ran (M5.1
        /// stage F).
        #[serde(default)]
        shank_avoid: bool,
    },
    /// Declared at v24, the parallel twin (M5.1 stage C). The one
    /// extra field is the raster's optional stepover, absent-means-
    /// derived exactly as the finish's.
    #[serde(rename = "parallel5d")]
    Parallel5D {
        tolerance: f64,
        /// Absent follows the stepover cos-law from the tolerance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Absent means off, the contour's twin above.
        #[serde(default)]
        shank_avoid: bool,
    },
    /// Declared at v25 with the swarf family (M5.1 stage D). The
    /// parallel's shape field for field - the ruling is not a
    /// parameter but a reading of the model, and the file that
    /// carries no model can no more state it than a contour file
    /// can state an edge's hash that isn't there.
    #[serde(rename = "swarf")]
    Swarf {
        tolerance: f64,
        /// Absent follows the stepover cos-law from the tolerance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// The ladder's one extra word: the count of tolerance rungs,
    /// written always - the kind IS the ladder, and an absent count
    /// would silently re-post someone's roughing program as a
    /// single finish pass. The range 1..4 is the door's, at load.
    #[serde(rename = "advanced_swarf")]
    AdvancedSwarf {
        tolerance: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        passes: u32,
    },
    /// Declared at v26 with the rotary twins (M5.1 stage E). The
    /// contour's shape field for field - the axis of revolution is a
    /// reading of the model, not a parameter, and the automatic
    /// snake_case needs no rename here: the names were never awkward.
    RotaryContour {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// The parallel twin's shape: the raster's optional stepover,
    /// absent-means-derived exactly as the finish's (M5.1 stage E).
    RotaryParallel {
        tolerance: f64,
        /// Absent follows the stepover cos-law from the tolerance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// Declared at v27 with the wrap ladder (M5.1 stage E2). The
    /// pocket's shape, read the wrap's way - and deliberately no
    /// `retract_z` key: the kernel names its own rapids plane one
    /// infeed and a tool radius off the skin, so a stored hop height
    /// could only ever be ignored, and the file contract says what
    /// the machine hears.
    RotaryOffset {
        tolerance: f64,
        /// Developed arc-length spacing, a plain number like the
        /// pocket's - the ladder has no cos-law to derive from.
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        /// Radial infeed per wrap level.
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: StrategyDto,
        #[serde(default)]
        entry: EntryDto,
        /// Read only when `entry` is Helix; the pocket's law.
        helix_radius: f64,
        #[serde(default = "default_engagement_deg")]
        max_engagement: f64,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
    },
    /// Declared at v28 with the stamped ladder (M5.1 stage E3). The
    /// pocket's shape field for field, minus the `rest_rough` and
    /// `feed_opt` switches the stamp has never been shown to commute
    /// with, plus the twins' pose trio and the verify budget - the
    /// file contract says what the generator reads, and the
    /// generator reads these and nothing else.
    MultiAxisRough {
        tolerance: f64,
        stepover: f64,
        stock_to_leave: f64,
        clearance_z: HeightDto,
        /// Absent means hop-to-clearance, the pocket family's law.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        strategy: StrategyDto,
        #[serde(default)]
        entry: EntryDto,
        /// Read only when `entry` is Helix; the pocket's law.
        helix_radius: f64,
        #[serde(default = "default_engagement_deg")]
        max_engagement: f64,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Absent means off, the twins' law - here it is the ladder's
        /// floors that answer to it, posed only when the shank speaks.
        #[serde(default)]
        shank_avoid: bool,
    },
    /// Declared at v31 with flowline finishing (M5.1 stage I). The
    /// parallel's shape field for field - the direction field is a
    /// reading of the model, not a parameter, exactly as the ruling
    /// and the rotary axis before it; the rename states the tag for
    /// the familiar reason - the automatic snake_case of `Flow5D`
    /// would be `flow5_d`.
    #[serde(rename = "flow5d")]
    Flow5D {
        tolerance: f64,
        /// Absent follows the stepover cos-law from the tolerance.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stepover: Option<f64>,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Absent means off, the twins' law.
        #[serde(default)]
        shank_avoid: bool,
    },
    /// Declared at v32 with the drive-curve contour (M5.1 stage J).
    /// Flow's shape minus the stepover - the picked chains ARE the
    /// path, there is no spacing to author - and the automatic
    /// snake_case needs no rename here: `multi_axis_contour` is
    /// already the tag.
    MultiAxisContour {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
        climb: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retract_z: Option<HeightDto>,
        lead_deg: f64,
        lag_deg: f64,
        max_tilt_deg: f64,
        /// Absent means off, the twins' law.
        #[serde(default)]
        shank_avoid: bool,
    },
    /// Declared at v33 with the turning substrate (M5.2 stage A).
    /// Three heights, a radial infeed and two feeds - the whole
    /// authored shape of the facing ladder; the axis it rides is
    /// model data and needs no key. The automatic snake_case is
    /// already the tag.
    Facing {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v34 with the profile kinds (M5.2 stage B): the
    /// station window's three heights, the radial rung spacing, the
    /// leave and two feeds - the whole authored shape of the rough
    /// ladder; the silhouette it eats is model data.
    ProfileRough {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        stock_to_leave: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v34 (M5.2 stage B): the window and the feeds -
    /// one ride is the whole pattern.
    ProfileFinish {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v35 with the blade kinds (M5.2 stage C): the
    /// window's three heights, the rung infeed, the leave and two
    /// feeds - the blade's width rides the TOOL row, not a number
    /// here; the kerf's width IS the tool.
    GrooveRough {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        stock_to_leave: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v35 (M5.2 stage C): the window and the feeds -
    /// one plunge per center is the whole pattern.
    GrooveFinish {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v35 (M5.2 stage C): the bar's top and the parting
    /// plane (`bottom_z`'s name in this variant), the rung infeed
    /// and the feeds. The feeder signals grew in at v40 (M5.2 G.2)
    /// with `#[serde(default)]` — an old file steps up silent-off,
    /// no version bump: the same shape every grown field takes. The
    /// handoff step grew in the same way (M5.3 C): absence is a
    /// program that asks no transfer.
    Part {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        feed: f64,
        plunge_feed: f64,
        #[serde(default)]
        bar_load: bool,
        #[serde(default)]
        bar_advance: f64,
        #[serde(default)]
        bar_unload: bool,
        #[serde(default)]
        bar_transfer: bool,
    },
    /// Declared at v36 with the threading kind (M5.2 stage D): the
    /// window of turns' three heights and the wave's own numbers -
    /// pitch, depth, ladder infeed, feeds. The insert's V rides the
    /// TOOL row, the blade's width law again.
    Threading {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        pitch: f64,
        depth: f64,
        infeed: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// Declared at v37 with the contour kinds (M5.2 stage E): the
    /// ride trio of heights and the kind's own promise-numbers -
    /// the authored angle, the authored diameters, the ladder
    /// numbers and the ride count. The nose that rides them rides
    /// the TOOL row, the profile kinds' law again.
    Chamfering {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        angle_deg: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// See [`Self::Chamfering`] - the taper's way.
    TaperTurning {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        bottom_dia: f64,
        top_dia: f64,
        feed: f64,
        plunge_feed: f64,
    },
    /// See [`Self::Chamfering`] - the multi-pass ladder's way.
    MultiPassTurning {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        stepdown: f64,
        stock_to_leave: f64,
        passes: u32,
        feed: f64,
        plunge_feed: f64,
    },
    /// See [`Self::Chamfering`] - the crown gauge's way: the window
    /// and the feeds, and no depth to file, because the model
    /// states it (M5.2 stage E2). The crown and the band ride the
    /// TOOL row - `nose_radius` and `width`, keys v37 files already
    /// carry.
    FormTurning {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
    },
    /// See [`Self::Chamfering`] - the boring bar's way (M5.2 stage
    /// F): the window, the tolerance and the two feeds, and no
    /// size to file, because the hole's wall is in the model and
    /// the nose that holds it to size is TOOL steel - `nose_radius`,
    /// a key v38 files already carry.
    BoringTurning {
        tolerance: f64,
        clearance_z: HeightDto,
        top_z: HeightDto,
        bottom_z: HeightDto,
        feed: f64,
        plunge_feed: f64,
    },
}

/// The angle an absent `ramp_angle` means: 45°, the ground the
/// raster's own law admits - every certifyable v17 chain already
/// descended no faster.
#[must_use]
pub fn default_ramp_angle() -> f64 {
    45.0
}

/// The file mirror of `finishing3d::FinishPattern`; `default` is
/// Waterline, the only pattern a v15 file can have run.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishPatternDto {
    #[default]
    Waterline,
    Parallel,
    ParallelMulti,
    /// M2.2c: a new word in an existing grammar - v16 files never
    /// carried it and still read unchanged, so no version step-up.
    Scallop,
    /// M2.2d, same law: chords through the footprint center.
    Radial,
    /// M2.2d, same law: one Archimedean curve in from the rim.
    Spiral,
    /// M2.2e, same law: concentric shadows of the terrain, lifted.
    Contour,
    /// M2.2e, same law: the silhouette lifted onto the surface.
    Project,
}

impl From<FinishPatternDto> for crate::finishing3d::FinishPattern {
    fn from(v: FinishPatternDto) -> Self {
        match v {
            FinishPatternDto::Waterline => crate::finishing3d::FinishPattern::Waterline,
            FinishPatternDto::Parallel => crate::finishing3d::FinishPattern::Parallel,
            FinishPatternDto::ParallelMulti => crate::finishing3d::FinishPattern::ParallelMulti,
            FinishPatternDto::Scallop => crate::finishing3d::FinishPattern::Scallop,
            FinishPatternDto::Radial => crate::finishing3d::FinishPattern::Radial,
            FinishPatternDto::Spiral => crate::finishing3d::FinishPattern::Spiral,
            FinishPatternDto::Contour => crate::finishing3d::FinishPattern::Contour,
            FinishPatternDto::Project => crate::finishing3d::FinishPattern::Project,
        }
    }
}

impl From<crate::finishing3d::FinishPattern> for FinishPatternDto {
    fn from(v: crate::finishing3d::FinishPattern) -> Self {
        match v {
            crate::finishing3d::FinishPattern::Waterline => FinishPatternDto::Waterline,
            crate::finishing3d::FinishPattern::Parallel => FinishPatternDto::Parallel,
            crate::finishing3d::FinishPattern::ParallelMulti => FinishPatternDto::ParallelMulti,
            crate::finishing3d::FinishPattern::Scallop => FinishPatternDto::Scallop,
            crate::finishing3d::FinishPattern::Radial => FinishPatternDto::Radial,
            crate::finishing3d::FinishPattern::Spiral => FinishPatternDto::Spiral,
            crate::finishing3d::FinishPattern::Contour => FinishPatternDto::Contour,
            crate::finishing3d::FinishPattern::Project => FinishPatternDto::Project,
        }
    }
}

pub(super) fn default_engagement_deg() -> f64 {
    45.0
}

/// The seed's helix centerline radius, mm (M5.1 H gave the planar
/// pocket an `entry` field; its files predate the number).
fn default_helix_radius() -> f64 {
    1.0
}

/// The file mirror of `machining3d::Entry3D`; `default` is Plunge, the
/// engagement every pre-3-D op had and the one no legality gate can
/// refuse.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryDto {
    #[default]
    Plunge,
    Ramp,
    Helix,
}

/// One hole of a drill op's list, on the wire: world XY, the model's
/// hole radius, the entry surface and floor it was recognized between,
/// and `through` - stored, because it selects the exit allowance at
/// generation and a live re-query of the mesh would let a model edit
/// silently redefine a hole nobody re-detected.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct HoleDto {
    pub xy: [f64; 2],
    pub radius: f64,
    pub entry_z: f64,
    pub floor_z: f64,
    pub through: bool,
}

impl HoleDto {
    pub(super) fn from_hole(h: &Hole) -> Self {
        HoleDto {
            // The lattice snap is idempotent, and recognition's output is
            // already snapped; this only levels a typed value onto the
            // file's fixed-point law (see MoveDto).
            xy: [lattice(h.xy.x), lattice(h.xy.y)],
            radius: lattice(h.radius),
            entry_z: lattice(h.entry_z),
            floor_z: lattice(h.floor_z),
            through: h.through,
        }
    }

    pub(super) fn to_hole(self) -> Hole {
        Hole {
            xy: Point2::new(self.xy[0], self.xy[1]),
            radius: self.radius,
            entry_z: self.entry_z,
            floor_z: self.floor_z,
            through: self.through,
        }
    }
}

/// One RADIAL hole of a drill op's cross list, on the wire (M5.2 F2):
/// the mouth's world point, the plunge direction (recognition only
/// emits axis-aligned unit dirs), the hole's own radius, and the two
/// travels along that direction - `entry_d` (the mouth convention:
/// 0 at the surface the hole opens on) and `floor_d` (negative =
/// depth). `through` selects the exit allowance at generation, the
/// same stored-not-requeried law [`HoleDto`] keeps.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CrossHoleDto {
    pub origin: [f64; 3],
    pub dir: [f64; 3],
    pub radius: f64,
    pub entry_d: f64,
    pub floor_d: f64,
    pub through: bool,
}

impl CrossHoleDto {
    pub(super) fn from_hole(h: &CrossHole) -> Self {
        CrossHoleDto {
            origin: [
                lattice(h.origin.x),
                lattice(h.origin.y),
                lattice(h.origin.z),
            ],
            dir: [h.dir.x, h.dir.y, h.dir.z],
            radius: lattice(h.radius),
            entry_d: lattice(h.entry_d),
            floor_d: lattice(h.floor_d),
            through: h.through,
        }
    }

    pub(super) fn to_hole(self) -> CrossHole {
        CrossHole {
            origin: Point3::new(self.origin[0], self.origin[1], self.origin[2]),
            dir: Point3::new(self.dir[0], self.dir[1], self.dir[2]),
            radius: self.radius,
            entry_d: self.entry_d,
            floor_d: self.floor_d,
            through: self.through,
        }
    }
}

/// Which canned cycle a drill op runs; the file mirror of
/// `drilling::CycleChoice`. `Auto` resolves at generate time (tool kind
/// first, then the depth-vs-diameter law), so the file stores only the
/// user's ask, like `StrategyDto::Auto` and `SideDto::Auto`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CycleChoiceDto {
    #[default]
    Auto,
    Spot,
    Drill,
    Peck,
    Tap,
    Bore,
    BackBore,
}

/// The reference part of a stored height. Serialized as a plain string;
/// `work_offset` carries its slot number.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HeightRefDto {
    Fixed,
    TopOfStock,
    BottomOfStock,
    TopOfModel,
    BottomOfModel,
    WorkOffset { slot: usize },
}

/// One stored height: `{"reference": ..., "offset": z}`. A bare number
/// still deserializes (`HeightWire`) as a `Fixed` expression carrying
/// exactly that number - the exact v5 value, so every pre-v6 file reads
/// unchanged with its saved z preserved to the bit. Writing always
/// produces the expression form.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct HeightDto {
    pub reference: HeightRefDto,
    pub offset: f64,
}

impl<'de> Deserialize<'de> for HeightDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        HeightWire::deserialize(deserializer).map(HeightDto::from)
    }
}

/// The two shapes a height takes on the wire. `Expr`'s fields are spelled
/// inline rather than nesting `HeightDto`, whose own `Deserialize` routes
/// back through here - the indirection must stay one-way, or parsing an
/// expression form recurses until the stack is gone.
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum HeightWire {
    Legacy(f64),
    Expr {
        reference: HeightRefDto,
        #[serde(default)]
        offset: f64,
    },
}

impl From<HeightWire> for HeightDto {
    fn from(v: HeightWire) -> Self {
        match v {
            HeightWire::Legacy(z) => HeightDto {
                reference: HeightRefDto::Fixed,
                offset: z,
            },
            HeightWire::Expr { reference, offset } => HeightDto { reference, offset },
        }
    }
}

impl From<crate::operation::Height> for HeightDto {
    fn from(h: crate::operation::Height) -> Self {
        HeightDto {
            reference: h.reference.into(),
            offset: h.offset,
        }
    }
}

impl From<HeightDto> for crate::operation::Height {
    fn from(d: HeightDto) -> Self {
        crate::operation::Height {
            reference: d.reference.into(),
            offset: d.offset,
        }
    }
}

impl From<crate::operation::HeightReference> for HeightRefDto {
    fn from(r: crate::operation::HeightReference) -> Self {
        use crate::operation::HeightReference as R;
        match r {
            R::Fixed => HeightRefDto::Fixed,
            R::TopOfStock => HeightRefDto::TopOfStock,
            R::BottomOfStock => HeightRefDto::BottomOfStock,
            R::TopOfModel => HeightRefDto::TopOfModel,
            R::BottomOfModel => HeightRefDto::BottomOfModel,
            R::WorkOffset(slot) => HeightRefDto::WorkOffset { slot },
        }
    }
}

impl From<HeightRefDto> for crate::operation::HeightReference {
    fn from(d: HeightRefDto) -> Self {
        use crate::operation::HeightReference as R;
        match d {
            HeightRefDto::Fixed => R::Fixed,
            HeightRefDto::TopOfStock => R::TopOfStock,
            HeightRefDto::BottomOfStock => R::BottomOfStock,
            HeightRefDto::TopOfModel => R::TopOfModel,
            HeightRefDto::BottomOfModel => R::BottomOfModel,
            HeightRefDto::WorkOffset { slot } => R::WorkOffset(slot),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LeadDto {
    None,
    Linear {
        length: f64,
    },
    Arc {
        radius: f64,
    },
    /// Added at v9 with the tangential lead: older files carry no key
    /// change at all (`lead` has been required since v1), only this new
    /// tag can appear in newer ones.
    Tangential {
        radius: f64,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StrategyDto {
    Zigzag,
    Waterline,
    /// Added at v5 with the auto strategy: the choice itself is not
    /// stored - resolution runs at generate time - only that the user
    /// asked for it.
    Auto,
    /// Added at v13 with the constant-engagement ring strategy (the 3-D
    /// pocket's `Adaptive`; in the 2-D pocket it is the documented
    /// waterline alias - the geometry family has no engagement field to
    /// feed it).
    Adaptive,
}

/// Which side of its geometry a contour runs, on the wire. Added at v7
/// with the contour side choice; like `StrategyDto::Auto` the stored
/// value is the user's ask, not the resolution - an absent key reads as
/// `auto` and the selection layer decides at generate time.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SideDto {
    /// Resolved by the selection layer: a lone ring bounding air
    /// machines inside, everything else outside.
    #[default]
    Auto,
    Inside,
    Outside,
}
