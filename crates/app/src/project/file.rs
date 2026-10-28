use super::*;

pub const PROJECT_VERSION: u32 = 38;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported project version {0} (this build writes {PROJECT_VERSION})")]
    UnsupportedVersion(u32),
    #[error("malformed project file: {0}")]
    Corrupt(String),
}

#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    /// The document's display unit ("mm" | "in"), v8+. Absent means
    /// millimeters - exactly what every pre-v8 file displayed, so the
    /// serde default is itself the whole migration. A millimeter document
    /// omits the field (byte-shape of the pre-v8 files, plus the stamp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    /// The document's machine (M2.3 C), v19+. An absent key means no
    /// machine was ever loaded - exactly what every v18 file meant,
    /// since the checker did not exist to read one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<MachineDto>,
    /// The document's explicit active-setup choice (M3.2 A), v21+. An
    /// absent key means no choice was ever made, which resolves to the
    /// first setup by name - what every v20 document did. A name that
    /// matches no setup row is legal, not corrupt: it reads through the
    /// same existence fallback the live singleton does (the fallback that
    /// also protects an undo of `DeleteSetup` mid-history).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_setup: Option<String>,
    /// The document's post script (M3.3 A/B), v23+. An absent key
    /// means the default post - what every v22 file ran, since the
    /// registry then held one script. `{"file": path}` names a user
    /// `.cps`; `{"named": name}` picks a shipped registry entry BY
    /// NAME (stage B grew the registry to five, and an index into it
    /// would make a re-order silently change a saved document's post -
    /// names survive growth, indices don't). Chosen values are stored
    /// as chosen: opening does not re-validate (the `SelectPost`
    /// constructor did), and a script that went missing - file or
    /// registry name - refuses by name at post time, never silently
    /// falling back (`script_source`'s law).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<PostDto>,
    pub entities: Vec<EntityDto>,
}

/// The file mirror of the `PostRef` singleton (M3.3): exactly the two
/// choices a document can carry beyond the default, each spelled for
/// what it is - a registry name or a path, never one string standing
/// ambiguously for both.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PostDto {
    Named(String),
    File(String),
}

/// The file mirror of the `MachineDef` singleton: the parsed machine
/// and the `.mch` path it came from, kept for display so opening a
/// project restates where the setup's numbers live. `Machine` is
/// serde-transparent (our own struct, our own field semantics).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MachineDto {
    pub path: String,
    pub machine: crate::machine::Machine,
}

/// What an entity is, in file terms. Absent means `Model` (the only kind
/// v1 and early v2 files could hold).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    Model,
    Stock,
    Wcs,
    Tool,
    Operation,
    Setup,
}

/// The file mirror of `StockShape`, kept separate so the domain enum can
/// evolve without editing the persistence contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StockDto {
    Box { dx: f64, dy: f64, dz: f64 },
    Cylinder { diameter: f64, height: f64 },
    FromModel { source: String },
}

impl StockDto {
    pub(super) fn from_shape(shape: &StockShape) -> Self {
        match shape {
            StockShape::Box { dx, dy, dz } => StockDto::Box {
                dx: *dx,
                dy: *dy,
                dz: *dz,
            },
            StockShape::Cylinder { diameter, height } => StockDto::Cylinder {
                diameter: *diameter,
                height: *height,
            },
            StockShape::FromModel { source } => StockDto::FromModel {
                source: source.clone(),
            },
        }
    }

    pub(super) fn to_shape(&self) -> StockShape {
        match self {
            StockDto::Box { dx, dy, dz } => StockShape::Box {
                dx: *dx,
                dy: *dy,
                dz: *dz,
            },
            StockDto::Cylinder { diameter, height } => StockShape::Cylinder {
                diameter: *diameter,
                height: *height,
            },
            StockDto::FromModel { source } => StockShape::FromModel {
                source: source.clone(),
            },
        }
    }
}

/// The file mirror of `tools::Tool`; same reason as `StockDto`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolDto {
    pub kind: ToolKindDto,
    pub diameter: f64,
    pub corner_r: f64,
    /// M1.6 additions: absent from a v9 file, serde-defaults to the
    /// inert 0.0 both fields were behaviorally equal to before.
    #[serde(default)]
    pub tip_angle: f64,
    #[serde(default)]
    pub pitch: f64,
    /// M5.2 addition (v33): absent from any older file, serde-defaults
    /// to the inert 0.0 every mill kind already meant - the `tip_angle`
    /// / `pitch` law restated.
    #[serde(default)]
    pub nose_radius: f64,
    /// M5.2 addition (v35): absent from any older file, serde-defaults
    /// to the inert 0.0 every non-blade kind already meant - the same
    /// law a third time.
    #[serde(default)]
    pub width: f64,
    pub flute_len: f64,
    pub shank_d: f64,
    pub shank_len: f64,
    pub holder_d: f64,
    pub holder_len: f64,
    pub material_group: String,
    pub rpm: f64,
    pub feed: f64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolKindDto {
    EndMill,
    BallNose,
    BullNose,
    Drill,
    Tap,
    Bore,
    /// The turning insert (M5.2 A): a new word in the kind grammar -
    /// v32 files never carried it, so no migration is needed, exactly
    /// as `bore` needed none at its arrival.
    Turning,
    /// The grooving/parting blade (M5.2 C): the same arrival law -
    /// v34 files never carried the word, and the `width` key it
    /// needs serde-defaults to the inert 0.0 every other kind means.
    Groove,
    /// The threading insert (M5.2 D): the same arrival law again -
    /// v35 files never carried the word, and the `width` +
    /// `tip_angle` keys it reads both serde-default to the inert
    /// 0.0 every other kind means.
    Thread,
    /// The form gauge (M5.2 E2): the same arrival law - v37 files
    /// never carried the word, and it reads the `width` +
    /// `nose_radius` keys both already filed, the crown taking the
    /// nose circle's place in the row for this kind alone.
    Form,
}

impl ToolDto {
    pub(super) fn from_tool(t: &Tool) -> Self {
        ToolDto {
            kind: match t.kind {
                ToolKind::EndMill => ToolKindDto::EndMill,
                ToolKind::BallNose => ToolKindDto::BallNose,
                ToolKind::BullNose => ToolKindDto::BullNose,
                ToolKind::Drill => ToolKindDto::Drill,
                ToolKind::Tap => ToolKindDto::Tap,
                ToolKind::Bore => ToolKindDto::Bore,
                ToolKind::Turning => ToolKindDto::Turning,
                ToolKind::Groove => ToolKindDto::Groove,
                ToolKind::Thread => ToolKindDto::Thread,
                ToolKind::Form => ToolKindDto::Form,
            },
            diameter: t.diameter,
            corner_r: t.corner_r,
            tip_angle: t.tip_angle,
            pitch: t.pitch,
            nose_radius: t.nose_radius,
            width: t.width,
            flute_len: t.flute_len,
            shank_d: t.shank_d,
            shank_len: t.shank_len,
            holder_d: t.holder_d,
            holder_len: t.holder_len,
            material_group: t.material_group.clone(),
            rpm: t.rpm,
            feed: t.feed,
        }
    }

    pub(super) fn to_tool(&self) -> Tool {
        Tool {
            kind: match self.kind {
                ToolKindDto::EndMill => ToolKind::EndMill,
                ToolKindDto::BallNose => ToolKind::BallNose,
                ToolKindDto::BullNose => ToolKind::BullNose,
                ToolKindDto::Drill => ToolKind::Drill,
                ToolKindDto::Tap => ToolKind::Tap,
                ToolKindDto::Bore => ToolKind::Bore,
                ToolKindDto::Turning => ToolKind::Turning,
                ToolKindDto::Groove => ToolKind::Groove,
                ToolKindDto::Thread => ToolKind::Thread,
                ToolKindDto::Form => ToolKind::Form,
            },
            diameter: self.diameter,
            corner_r: self.corner_r,
            tip_angle: self.tip_angle,
            pitch: self.pitch,
            nose_radius: self.nose_radius,
            width: self.width,
            flute_len: self.flute_len,
            shank_d: self.shank_d,
            shank_len: self.shank_len,
            holder_d: self.holder_d,
            holder_len: self.holder_len,
            material_group: self.material_group.clone(),
            rpm: self.rpm,
            feed: self.feed,
        }
    }
}

/// The file mirror of `operation::SrcRef` (names, like everywhere else).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SrcDto {
    Stock,
    Entity { name: String },
    Selection { items: Vec<GeoSelDto> },
}

/// The file mirror of `operation::GeoSel`: a named entity and one of its
/// feature ids. Ids are geometry hashes (stable ids), not positions, so a
/// saved pick survives save/load and facet reorder.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GeoSelDto {
    pub entity: String,
    pub feature: FeatureSelDto,
}

/// The file mirror of `operation::FeatureSel`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FeatureSelDto {
    Face { id: u64 },
    Chain { id: u64 },
    Patch { id: u64 },
}
