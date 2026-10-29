use super::*;

/// One CL move: kind, endpoints, contact feed (absent for rapids).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct MoveDto {
    pub kind: MoveKindDto,
    pub from: [f64; 3],
    pub to: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<f64>,
    /// Added at v5: the circular segment this move traverses, absent for
    /// the straight-line moves every pre-v5 path held (and every path
    /// with no arcs still writes - the field skips serialization).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc: Option<ArcDto>,
    /// Added at v22 (M5.1 stage A, no version bump - the arc-v5
    /// precedent): the tool pose the move states, as the unit axis's
    /// lattice-snapped components. Absent means no angle stated -
    /// what every path this app has ever generated held - so a save
    /// of a prismatic document is byte-shape identical to its
    /// pre-seam self, and the field skips serialization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<[f64; 3]>,
}

/// An arc move's center, in the XY plane, with direction. `kind` stays
/// the motion class (rapid/feed/plunge); the arc is geometry.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ArcDto {
    pub center: [f64; 2],
    pub ccw: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MoveKindDto {
    Rapid,
    Feed,
    Plunge,
}

/// The cached generation output: path plus the warnings that came with it,
/// plus - from v10 - the canned-cycle list that rides alongside the moves
/// of a drilling path (see `crate::cl::Cycle`). Absent from any pre-v10
/// cache, which is exactly right: no pre-v10 build ever generated a cycle.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PathDto {
    pub moves: Vec<MoveDto>,
    pub warnings: Vec<Warning>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cycles: Vec<CycleDto>,
}

/// One cached cycle: the post's source of truth for a hole, stored beside
/// (not instead of) the moves it expands to. Floats take the same lattice
/// snap `MoveDto` gives its coordinates, so a cycle round-trips
/// byte-identically and stays bit-exact with its moves' depths - what the
/// agreement law pins.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CycleDto {
    pub kind: CycleKindDto,
    pub xy: [f64; 2],
    pub r_z: f64,
    pub bottom_z: f64,
    pub entry_z: f64,
    pub peck: f64,
    pub dwell: f64,
    pub first_move: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CycleKindDto {
    Spot,
    Drill,
    Peck,
    Deep,
    Tap,
    Bore,
    BackBore,
}

impl CycleDto {
    fn from_cycle(c: &Cycle) -> Self {
        CycleDto {
            kind: match c.kind {
                CycleKind::Spot => CycleKindDto::Spot,
                CycleKind::Drill => CycleKindDto::Drill,
                CycleKind::Peck => CycleKindDto::Peck,
                CycleKind::Deep => CycleKindDto::Deep,
                CycleKind::Tap => CycleKindDto::Tap,
                CycleKind::Bore => CycleKindDto::Bore,
                CycleKind::BackBore => CycleKindDto::BackBore,
            },
            xy: [lattice(c.xy.x), lattice(c.xy.y)],
            r_z: lattice(c.r_z),
            bottom_z: lattice(c.bottom_z),
            entry_z: lattice(c.entry_z),
            peck: lattice(c.peck),
            dwell: lattice(c.dwell),
            first_move: c.first_move as u64,
        }
    }

    pub(super) fn to_cycle(self) -> Cycle {
        Cycle {
            kind: match self.kind {
                CycleKindDto::Spot => CycleKind::Spot,
                CycleKindDto::Drill => CycleKind::Drill,
                CycleKindDto::Peck => CycleKind::Peck,
                CycleKindDto::Deep => CycleKind::Deep,
                CycleKindDto::Tap => CycleKind::Tap,
                CycleKindDto::Bore => CycleKind::Bore,
                CycleKindDto::BackBore => CycleKind::BackBore,
            },
            xy: Point2::new(self.xy[0], self.xy[1]),
            r_z: self.r_z,
            bottom_z: self.bottom_z,
            entry_z: self.entry_z,
            peck: self.peck,
            dwell: self.dwell,
            first_move: usize::try_from(self.first_move).unwrap_or(usize::MAX),
        }
    }
}

/// A setup as files hold it (M3.2 A): the furniture it owns, by name.
/// `wcs` is indexed by [`WCS_SLOTS`]; an entry may name an entity the
/// file does not contain - that slot is simply not placed in this setup,
/// exactly what a missing `G55` entity has always meant. `datum_from` is
/// clone provenance for the setup sheet (stage D); every stamp ignores
/// it, so it defaults and rides along for the reader, not the machinery.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct SetupDto {
    pub stock: String,
    #[serde(default)]
    pub wcs: Vec<String>,
    #[serde(default)]
    pub datum_from: String,
}

/// The fixture marker as files hold it (M3.2 stage B): a tag on a model
/// entity - the solid itself is that model's own `facets`, its placement
/// the entity's `translation`/`rotation`, and the machine's table is
/// never stored here (it derives from the `machine` key). `setup` is the
/// DECLARED binding, same grammar as `OperationDto::setup`: absent
/// means the first setup by name (the implicit-setup law), so a tagged
/// model in a setup-free document saves as one key and nothing more.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct FixtureDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
}

/// An operation as files hold it: definition, the stamp the last generate
/// saw (raw digest; its meaning lives in `operation::current_stamp`), and
/// the cache that stamp was taken for. `stamp` present without a cache (or
/// the reverse, e.g. after an edit clears the stamp but keeps the old path
/// on screen) is legitimate and reads Stale.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OperationDto {
    pub tool: String,
    pub src: SrcDto,
    pub params: ParamsDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<PathDto>,
    /// The setup this operation machines in (M3.2 A). Absent means the
    /// first setup by name - exactly what every pre-v21 operation was
    /// (the implicit-setup law), so a v21 save of a setup-free document
    /// is byte-identical to its v20 save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
}

impl MoveDto {
    /// Coordinates stand on the 1 um lattice ([`crate::cl::lattice`]):
    /// every float a file carries is then a fixed point of write-parse,
    /// so save-load-save is byte-identical - an off-lattice double's
    /// shortest decimal can parse back one ulp over from what was
    /// written, and a cache that drifts a ulp per reopen is a file
    /// that never repeats itself. Generated coordinates are already
    /// lattice values (clipper's grid, user-typed feeds); the snap
    /// only ever moves anything computed - a lead tangent point - and
    /// does so by at most half a micron.
    fn from_move(m: &Move) -> Self {
        let lat = |p: Point3| [lattice(p.x), lattice(p.y), lattice(p.z)];
        MoveDto {
            kind: match m.kind {
                MoveKind::Rapid => MoveKindDto::Rapid,
                MoveKind::Feed => MoveKindDto::Feed,
                MoveKind::Plunge => MoveKindDto::Plunge,
            },
            from: lat(m.from),
            to: lat(m.to),
            feed: m.feed.map(lattice),
            arc: m.arc.map(|a| ArcDto {
                center: [lattice(a.center.x), lattice(a.center.y)],
                ccw: a.ccw,
            }),
            // The exact vertical stays exactly (0, 0, 1) through the
            // snap (zeros and ones are lattice fixed points), so a
            // stated-up path saves and reloads into the prismatic
            // code paths the sim and dump branch on.
            axis: m
                .axis
                .map(|a| [lattice(a[0]), lattice(a[1]), lattice(a[2])]),
        }
    }

    pub(super) fn to_move(self) -> Move {
        let m = Move::new(
            match self.kind {
                MoveKindDto::Rapid => MoveKind::Rapid,
                MoveKindDto::Feed => MoveKind::Feed,
                MoveKindDto::Plunge => MoveKind::Plunge,
            },
            Point3::new(self.from[0], self.from[1], self.from[2]),
            Point3::new(self.to[0], self.to[1], self.to[2]),
            self.feed,
        );
        let m = match self.arc {
            Some(a) => m.with_arc(Point2::new(a.center[0], a.center[1]), a.ccw),
            None => m,
        };
        // VERBATIM, not `with_axis`: the file's numbers are the law -
        // re-normalizing on load would move a saved axis by bits and
        // break the save-load-save byte identity (the lattice snap in
        // from_move is already the normalization gate at write time).
        m.attach_axis(self.axis)
    }
}

impl PathDto {
    pub(super) fn from_cache(t: &ToolpathCache) -> Self {
        PathDto {
            moves: t.path.moves.iter().map(MoveDto::from_move).collect(),
            warnings: t.warnings.clone(),
            cycles: t.path.cycles.iter().map(CycleDto::from_cycle).collect(),
        }
    }

    pub(super) fn to_cache(&self) -> ToolpathCache {
        let mut path = Toolpath::new(self.moves.iter().map(|m| m.to_move()).collect());
        path.cycles = self
            .cycles
            .iter()
            .copied()
            .map(CycleDto::to_cycle)
            .collect();
        ToolpathCache {
            path,
            warnings: self.warnings.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct EntityDto {
    pub name: String,
    pub translation: [f64; 3],
    /// Unit quaternion (x, y, z, w). Defaulted so v1 files parse; the v1 -> v2
    /// migration below makes that choice explicit rather than implicit.
    #[serde(default = "identity_rotation")]
    pub rotation: [f64; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facets: Option<Vec<[[f64; 3]; 3]>>, // a, b, c per triangle
    /// Geometry-revision counter (M3.1, `schema::Revision`). Absent means 0,
    /// which is what every pre-v20 file held; a model always carries >= 1
    /// (import stamps the first landing), so this key never appears on
    /// setup furniture and a v20 save of a pre-M3.1 document is byte-shape
    /// identical to its v19 save.
    #[serde(default, skip_serializing_if = "is_unrevised")]
    pub rev: u64,
    /// Absent means Model; only non-model entities carry the tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<EntityKind>,
    /// Stock parameters; present exactly when `kind == Some(Stock)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock: Option<StockDto>,
    /// Tool definition; present exactly when `kind == Some(Tool)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolDto>,
    /// Operation definition (+ cached path); present exactly when
    /// `kind == Some(Operation)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<OperationDto>,
    /// Setup definition; present exactly when `kind == Some(Setup)`
    /// (M3.2 A).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<SetupDto>,
    /// Fixture marker (M3.2 B); present exactly on an UNTAGGED-kind
    /// (Model) entity - a fixture is a model that collision checks
    /// respect, so it borrows the model row's geometry, never a kind
    /// of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture: Option<FixtureDto>,
}
