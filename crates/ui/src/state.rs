//! The shell's document state: world + history + deferred actions.
//!
//! Provenance: our own app state, specced from public CAM session behavior.
//! Panel code never mutates the world directly - it queues a `CommandKind`,
//! which is executed once per frame outside the ImGui borrow (dialog code
//! must not run mid-frame).
//!
//! File imports run the parse/tessellation on a worker thread so a heavy
//! STEP never freezes the window; the resulting `ImportModel` command is
//! executed on this thread, so the history stays the single source of truth.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::time::{Duration, Instant};

use camcraft_app::commands::{
    ActivateSetup, AddTool, ChangeTool, CommandError, CreateOperation, CreateSetup, DeleteEntity,
    DeleteFixture, DeleteOperation, DeleteSetup, DeleteTool, EditOperationLinks,
    EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine, MoveFixture,
    MoveModel, RegenerateAll, SelectEntity, SelectPost, SetOperationGeometry, SetOperationSetup,
    SetStock, SetUnits, SetWcsOrigin, TagFixture, UntagFixture, UpdateModel,
};
use camcraft_app::geometry::{
    Point3, feature, quat_from_euler, quat_from_yaw, quat_to_euler, transform_point,
};
use camcraft_app::operation::{
    self, FeatureSel, GeoSel, Height, HeightReference, OpParams, SrcRef,
};
use camcraft_app::schema::{Location, Operation as OperationDef, PostRef, WCS_SLOTS};
use camcraft_app::stock::{self, StockShape};
use camcraft_app::tools::{Tool, ToolKind, catalog};
use camcraft_app::units::{self, Unit};
use camcraft_app::{machine, model, posting, project, world::new_world};
use flecs_ecs::prelude::World;

use crate::viewport::Camera;

#[derive(Debug)]
pub enum CommandKind {
    OpenModelDialog,
    ImportFile(PathBuf),
    NewDocument,
    OpenProjectDialog,
    OpenProjectFile(PathBuf),
    SaveProjectDialog,
    SaveProjectFile(PathBuf),
    /// Post NC through the save-dialog law (M1.7b): path via rfd, gate
    /// via `posting::post_setup`.
    PostProgramDialog,
    /// The same post at a known path - what the headless tests run.
    PostProgramFile(PathBuf),
    Undo,
    Redo,
    DeleteSelected,
    DeleteByName(String),
    Select(Option<String>),
    FrameAll,
    Quit,
    /// The Setup pane's shape+placement draft; the command itself is built
    /// here, so validation and status reporting live in one place.
    SetStock {
        shape: StockShape,
        pos: [f64; 3],
        yaw_deg: f64,
    },
    /// Stock that wraps every model's world AABB plus `margin` mm, shaped
    /// like the pane's radio selection at click time (0 box, 1 cylinder).
    StockFit {
        margin: f64,
        shape: i32,
    },
    /// The document's display unit (M1.1): a lens over numbers that all
    /// stay millimeters in the world. Undo restores the previous lens.
    SetUnits(Unit),
    /// The document's machine (M2.3 C): setup furniture like the units.
    /// The dialog arm reads a `.mch` through the file-pick law; the path
    /// arm is what tests run. Clearing states the absence - undo puts
    /// the previous machine back.
    LoadMachineDialog,
    LoadMachineFile(PathBuf),
    ClearMachine,
    /// M3.3 stage C, the machine group's other two hands: pick a
    /// machine from the built-in library BY NAME (a name missing from
    /// the library refuses, it never mounts a neighbor), or open/apply
    /// the editor. Editing is a command too - the draft in the window
    /// is ephemeral, and only Apply writes through `LoadMachine`, so
    /// undo puts the machine's previous numbers back exactly.
    LoadBuiltinMachine(String),
    EditMachineDialog,
    EditMachineApply {
        path: String,
        machine: Box<camcraft_app::machine::Machine>,
    },
    /// The post script (M3.3 A): the built-in arm ships, the dialog arm
    /// picks a user `.cps` through the file-pick law, and the path arm
    /// is what the headless tests run. Selection is a command like
    /// every other piece of setup furniture - undo puts the previous
    /// script back, and the file is compiled at selection, refused by
    /// name if it does not run.
    SelectPostBuiltin,
    SelectPostDialog,
    SelectPostFile(PathBuf),
    /// Pick a built-in from the shipped registry by NAME (M3.3 B) -
    /// the combo's index is screen furniture; what the document stores
    /// is the name, so a registry re-growth never re-points a saved
    /// file's post.
    SelectPostNamed(String),
    /// Work offset: slot origin plus full orientation as yaw-pitch-roll
    /// degrees (R = Rz ⊗ Ry ⊗ Rx). The world stores a unit quaternion;
    /// pure-yaw setups use roll = pitch = 0.
    SetWcs {
        slot: String,
        pos: [f64; 3],
        roll_deg: f64,
        pitch_deg: f64,
        yaw_deg: f64,
    },
    /// Tool library: create a tool entity (catalog row or hand-typed page).
    AddTool {
        name: String,
        tool: Tool,
    },
    /// Tool library: replace a tool entity's numbers (referencing
    /// operations read Stale from their stamp - no cascade writes).
    ChangeTool {
        name: String,
        tool: Tool,
    },
    DeleteTool(String),
    /// New operation entity; deliberately never generated until
    /// [`CommandKind::GenerateOperation`] runs.
    CreateOperation {
        name: String,
        tool: String,
        src: SrcRef,
        params: OpParams,
    },
    /// The properties page's parameter draft.
    EditOperationParams {
        name: String,
        params: OpParams,
    },
    /// Retarget an operation's tool and/or source (combo changes).
    EditOperationLinks {
        name: String,
        tool: Option<String>,
        src: Option<SrcRef>,
    },
    /// The op's full feature pick list (Geometry section, M1.5 step 7
    /// drives this; adding a chip sends the list with it appended).
    SetOperationGeometry {
        name: String,
        items: Vec<GeoSel>,
    },
    DeleteOperation(String),
    /// Generate and verify the cached path. The command does the work at
    /// construction, synchronously; status arrives with the result.
    GenerateOperation(String),
    /// The batch fix (M3.1 C): regenerate every Stale operation in one
    /// undo unit. What it finds stale at dispatch is what it fixes.
    RegenerateAll,
    /// Setups (M3.2 A): the context a clamping creates. `datum_from`
    /// clones a setup's stock and work offsets into the new one's own
    /// entities (the transfer law); `None` starts with empty furniture -
    /// except on a document with no setups at all, where the command is
    /// by law the MATERIALIZATION of the implicit first setup.
    CreateSetup {
        name: Option<String>,
        datum_from: Option<String>,
    },
    DeleteSetup(String),
    /// Move the active-setup cursor. Inert for every stamp - it decides
    /// where NEW things join, never what already cuts.
    ActivateSetup(Option<String>),
    /// Which setup an operation machines in (the Properties page's Setup
    /// combo). Keeps the operation's stamp; the resolved-Z digest answers
    /// Ok-or-Stale from the two setups' placements.
    SetOperationSetup {
        name: String,
        setup: Option<String>,
    },
    /// Fixtures (M3.2 stage B): the marker is the whole fixture, so these
    /// are the marker and placement verbs a model already knows - tag
    /// marks one as the setup context's workholding steel, untag demotes
    /// it (the solid stays put, the collision law stops seeing it),
    /// `MoveFixture` is the placement every part shares, `DeleteFixture`
    /// removes the solid. `ImportFixtureDialog` rides the model-import
    /// worker and tags the landed name on finish - a command constructor
    /// never reads disk, so the STEP tessellates off-thread as always.
    TagFixture {
        name: String,
        setup: Option<String>,
    },
    UntagFixture(String),
    MoveFixture {
        name: String,
        pos: [f64; 3],
        yaw_deg: f64,
    },
    DeleteFixture(String),
    ImportFixtureDialog,
    /// Part orientation (M5.2 G): a model's full rigid placement -
    /// position plus yaw-pitch-roll degrees (R = Rz ⊗ Ry ⊗ Rx, the
    /// `SetWcs` voice). The world stores a unit quaternion; the
    /// M3.2 flip is this arm with roll = pitch = 0.
    MoveModel {
        name: String,
        pos: [f64; 3],
        roll_deg: f64,
        pitch_deg: f64,
        yaw_deg: f64,
    },
    /// Setup sheets (M3.2 stage D): render every setup and write the
    /// flip chart, one page per setup, form feeds as page breaks. The
    /// export is a VIEW of the document - no command, no history, no
    /// world write, exactly like posting a program. The dialog arm is
    /// the save-dialog law; the path arm is what headless tests run.
    SetupSheetDialog,
    SetupSheetFile(PathBuf),
}

/// Editable fields of the Setup pane's stock group. Transient UI state,
/// never authoritative: the world's `StockDef` is, refetched by
/// [`State::sync_setup_ui`] whenever the document moves under the pane.
#[derive(Clone, Debug)]
pub struct StockUi {
    /// 0 = box, 1 = cylinder, 2 = from model (radio order in the pane).
    pub shape: i32,
    pub dx: f64,
    pub dy: f64,
    pub dz: f64,
    pub diameter: f64,
    pub height: f64,
    pub source: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw_deg: f64,
    pub fit_margin: f64,
}

impl Default for StockUi {
    fn default() -> Self {
        // A billet that fits a hand-held part: plausible first numbers beat
        // zeros, which the command layer would only refuse.
        StockUi {
            shape: 0,
            dx: 60.0,
            dy: 60.0,
            dz: 30.0,
            diameter: 40.0,
            height: 20.0,
            source: String::new(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            yaw_deg: 0.0,
            fit_margin: 2.0,
        }
    }
}

/// Editable fields of the Setup pane's WCS group; see [`StockUi`] for the
/// transient-ness rule. `slot` indexes [`WCS_SLOTS`].
#[derive(Clone, Debug, Default)]
pub struct WcsUi {
    pub slot: usize,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Orientation as yaw-pitch-roll degrees (see [`CommandKind::SetWcs`]).
    /// The quick-turn buttons in the pane compose onto these, so the draft
    /// always round-trips with the world's quaternion.
    pub roll_deg: f64,
    pub pitch_deg: f64,
    pub yaw_deg: f64,
    /// XY grid point last clicked (row-major north-to-south, west-to-east;
    /// 4 = center). Display-only highlight; the world stores the position.
    pub anchor_xy: usize,
    /// Z level grid clicks use: 0 bottom, 1 middle, 2 top of the anchor box.
    pub anchor_z: usize,
    /// Which box the anchor points live on: 0 = stock box, 1 = model bounds.
    /// The pane states it explicitly; a chosen source that doesn't exist
    /// falls back to the other, with a note (interact::anchor_reference).
    pub anchor_source: usize,
    /// Viewport picking mode: the anchor points of the current level show as
    /// clickable markers in the viewport. Transient UI mode, not undoable
    /// (AGENTS.md) - the origin change it triggers is the command.
    pub pick_anchor: bool,
    /// Whether the selected slot currently has a placed origin entity. Lets
    /// `sync_setup_ui` zero the readout when an origin is undone without
    /// wiping typing into a never-placed slot.
    pub(crate) placed: bool,
}

/// Editable fields of the Setups page (M3.2 A). Transient like [`StockUi`]:
/// the world's SetupDef entities and the active singleton are the truth.
/// `pick` indexes the page's setup list (clamped at draw, like
/// `wcs_ui.slot`); `datum` rides the New button's checkbox - transfer the
/// current setup's stock and offsets into the new one, the shop's
/// re-clamp law.
#[derive(Clone, Debug, Default)]
pub struct SetupsUi {
    pub pick: usize,
    pub datum: bool,
}

/// The fixtures section's placement draft (M3.2 stage B): one fixture's
/// pose, editable before Apply queues the `MoveFixture` - the [`StockUi`]
/// law, and re-synced from the world whenever the row or document moves
/// under the pane.
#[derive(Clone, Debug, Default)]
pub struct FixtureUi {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw_deg: f64,
}

/// The Model page's placement draft (M5.2 G): one part's full rigid
/// pose - position plus yaw-pitch-roll degrees - editable before Apply
/// queues the `MoveModel`, the [`FixtureUi`] law with the two angles
/// the parts page adds. Re-synced from the world whenever the
/// selection or the document moves under the pane; a draft whose
/// model vanished retires with it (the pane never edits a ghost).
#[derive(Clone, Debug, Default)]
pub struct ModelUi {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub roll_deg: f64,
    pub pitch_deg: f64,
    pub yaw_deg: f64,
}

/// Editable fields of the Properties pane's operation page. Transient UI
/// state, never authoritative: the world's `Operation` component is,
/// refetched by [`State::sync_ops_ui`] whenever the selection or the
/// document moves under the pane (same typing rule as [`StockUi`]). With
/// no operation selected the same draft is the "new operation" form.
#[derive(Clone, Debug)]
pub struct OpUi {
    /// Operation this draft mirrors; empty = nothing selected.
    pub name: String,
    /// Tool entity the operation cuts with.
    pub tool: String,
    /// 0 = the stock solid, 1 = a named model, 2 = feature picks (`SrcRef`).
    pub src_kind: usize,
    /// Model name when `src_kind == 1`.
    pub src_name: String,
    /// The pick list when `src_kind == 2`, mirrored for the Geometry
    /// section's chips and for `src` below. Never edited through the pane
    /// fields: viewport picks and chip removal queue commands, and the
    /// next sync re-mirrors what the world actually holds.
    pub geo: Vec<GeoSel>,
    /// The parameter set; the kind the fields show is `params.kind()`.
    pub params: OpParams,
}

impl Default for OpUi {
    fn default() -> Self {
        OpUi {
            name: String::new(),
            tool: String::new(),
            src_kind: 0,
            src_name: String::new(),
            geo: Vec::new(),
            // A facing pass over a small billet, in references: 25 above
            // the stock top, skimming the stock top itself. `retype`
            // carries this clearance into every kind, so the draft
            // default must already be the seed shape the panes display -
            // quick-add and the creation form then say the same thing.
            params: OpParams::Face {
                tilt: None,
                stepover: 3.0,
                clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
                cut_z: Height::of(HeightReference::TopOfStock, 0.0),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        }
    }
}

impl OpUi {
    /// Snapshot an operation component into draft form.
    #[must_use]
    pub fn from_op(name: &str, op: &OperationDef) -> Self {
        let (src_kind, src_name, geo) = match &op.src {
            SrcRef::Stock => (0, String::new(), Vec::new()),
            SrcRef::Entity(n) => (1, n.clone(), Vec::new()),
            // Feature picks mirror as kind 2 plus the list itself: the
            // combo cannot express them (the Geometry section draws the
            // chips from `geo`), but a links edit must never rewrite a
            // selection's source by accident - `src` below round-trips it.
            SrcRef::Selection(items) => (2, String::new(), items.clone()),
        };
        OpUi {
            name: name.to_string(),
            tool: op.tool.clone(),
            src_kind,
            src_name,
            geo,
            params: op.params.clone(),
        }
    }

    /// The source the drafts describe, for the queue.
    #[must_use]
    pub fn src(&self) -> SrcRef {
        match self.src_kind {
            0 => SrcRef::Stock,
            2 => SrcRef::Selection(self.geo.clone()),
            _ => SrcRef::Entity(self.src_name.clone()),
        }
    }
}

/// One Geometry-section row: the pane-facing view of a pick-list entry.
/// `label` is the whole display text - a readable descriptor of the
/// geometry the pick names, falling back to entity and short id when the
/// id no longer resolves; `missing` is the amber flag for exactly that.
#[derive(Clone, Debug, PartialEq)]
pub struct GeoChip {
    pub sel: GeoSel,
    pub label: String,
    pub missing: bool,
}

/// A feature id as the chips show it: the 16 hex digits, shortened to
/// head and tail with an ellipsis. Long enough to tell two picks of one
/// model apart at a glance, short enough for a chip row to stay one row.
fn short_id(id: u64) -> String {
    let h = format!("{id:016x}");
    format!("{}…{}", &h[..4], &h[14..])
}

/// XY vertex set of a ring, quantized and sorted: the identity a patch
/// chip dedupes coincident rims by (top and bottom rims of a vertical
/// wall share it; a cone's differ, as they must). Display-only helper -
/// the generator's dedupe is exact and lives in `operation::push_loop`.
fn xy_set_key(pts: &[[f64; 3]]) -> Vec<(i64, i64)> {
    let mut k: Vec<(i64, i64)> = pts
        .iter()
        .map(|p| {
            (
                (p[0] * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
                (p[1] * 1e6).round().clamp(i64::MIN as f64, i64::MAX as f64) as i64,
            )
        })
        .collect();
    k.sort_unstable();
    k.dedup();
    k
}

/// XY footprint span (w × h) of a point run; 0.0 if it has no extent.
fn span(pts: &[[f64; 3]]) -> f64 {
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in pts {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (hi[0] - lo[0]).max(0.0) * (hi[1] - lo[1]).max(0.0)
}

/// |Area| of a closed XY ring by the shoelace sum.
fn shoelace(pts: &[[f64; 3]]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..pts.len() {
        let (p, q) = (&pts[i], &pts[(i + 1) % pts.len()]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    (a * 0.5).abs()
}

/// Kåsa circle fit (mean-radius refinement): Some((center, r)) when the
/// points are too many and too non-collinear to leave the fit undefined.
/// Display only, for the chip's "bore Ø" descriptor.
fn fit_circle(pts: &[[f64; 3]]) -> Option<([f64; 2], f64)> {
    let n = pts.len();
    if n < 3 {
        return None;
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for p in pts {
        sx += p[0];
        sy += p[1];
    }
    let (mx, my) = (sx / n as f64, sy / n as f64);
    let (mut sxx, mut sxy, mut syy, mut bxx, mut byy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for p in pts {
        let (x, y) = (p[0] - mx, p[1] - my);
        let z = x * x + y * y;
        sxx += x * x;
        sxy += x * y;
        syy += y * y;
        bxx += x * z;
        byy += y * z;
    }
    let det = sxx * syy - sxy * sxy;
    if !det.is_finite() || det.abs() <= f64::EPSILON {
        return None;
    }
    let u = (0.5 * bxx * syy - 0.5 * sxy * byy) / det;
    let v = (0.5 * byy * sxx - 0.5 * sxy * bxx) / det;
    let c = [mx + u, my + v];
    let r = pts
        .iter()
        .map(|p| ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2)).sqrt())
        .sum::<f64>()
        / n as f64;
    (r.is_finite() && r > 0.0).then_some((c, r))
}

/// Editable fields of the Tool Library pane's tool page; see [`StockUi`]
/// for the transient-ness rule. `exists` records whether `name` resolved
/// to a tool entity when the draft was fetched: Apply edits, Add creates.
#[derive(Clone, Debug)]
pub struct ToolUi {
    pub name: String,
    pub exists: bool,
    /// 0 end mill, 1 ball nose, 2 bull nose, 3 drill, 4 tap, 5 bore,
    /// 6 turning insert, 7 grooving blade, 8 threading insert,
    /// 9 form gauge.
    pub kind: usize,
    pub diameter: f64,
    pub corner_r: f64,
    /// Included degrees of a drill's conical point; inert on the other
    /// kinds (the same rule the app's `Tool` field follows).
    pub tip_angle: f64,
    /// Thread pitch, mm per turn; the tap cycle feeds at rpm x pitch.
    pub pitch: f64,
    /// Insert nose radius; the turning kind's cutting ball, inert on
    /// the mills (the app's `Tool` field's law, mirrored).
    pub nose_radius: f64,
    /// Blade width; the groove kind's cutting edge along the axis -
    /// the kerf's width IS the tool, inert on the others (the app's
    /// `Tool` field's law, mirrored a fourth time).
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

impl Default for ToolUi {
    fn default() -> Self {
        // An unnamed copy of the catalog's smallest mill: plausible
        // starting numbers beat zeros the commands would only refuse.
        let starter = catalog().remove(0);
        ToolUi::from_tool("", &starter.1, false)
    }
}

impl ToolUi {
    #[must_use]
    pub fn from_tool(name: &str, t: &Tool, exists: bool) -> Self {
        ToolUi {
            name: name.to_string(),
            exists,
            kind: match t.kind {
                ToolKind::EndMill => 0,
                ToolKind::BallNose => 1,
                ToolKind::BullNose => 2,
                ToolKind::Drill => 3,
                ToolKind::Tap => 4,
                ToolKind::Bore => 5,
                ToolKind::Turning => 6,
                ToolKind::Groove => 7,
                ToolKind::Thread => 8,
                ToolKind::Form => 9,
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

    #[must_use]
    pub fn to_tool(&self) -> Tool {
        Tool {
            kind: match self.kind {
                1 => ToolKind::BallNose,
                2 => ToolKind::BullNose,
                3 => ToolKind::Drill,
                4 => ToolKind::Tap,
                5 => ToolKind::Bore,
                6 => ToolKind::Turning,
                7 => ToolKind::Groove,
                8 => ToolKind::Thread,
                9 => ToolKind::Form,
                _ => ToolKind::EndMill,
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

/// Yaw in degrees of a (pure-yaw) quaternion: the exact inverse of
/// `quat_from_yaw`, and the Z-twist of any other rotation. Display/draft
/// only - the data model keeps the full quaternion through Apply.
#[must_use]
pub fn quat_yaw_deg(q: [f64; 4]) -> f64 {
    (2.0 * q[2].atan2(q[3])).to_degrees()
}

/// Which viewport drag is live; ephemeral UI state, deliberately not
/// undoable (AGENTS.md). The shell's pointer layer owns it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode {
    Orbit,
    Pan,
}

/// What a viewport click selects. Transient view state - the mode
/// itself is not undoable; what the click does with it is (a pick in
/// Face/Edge mode queues `CommandKind::SetOperationGeometry`). The
/// same rule `wcs_ui.pick_anchor` follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeoPick {
    /// Whole entities: click raycasts to a facet and selects the model.
    Model,
    /// One planar face of a feature: click raycasts, then maps the facet
    /// to its feature face id.
    Face,
    /// One tangent-continuous edge chain: click finds the nearest visible
    /// feature edge in screen space (Alt narrows it to that single
    /// segment). Falls back to Face behavior when nothing is close.
    Edge,
}

/// How the viewport paints solid geometry. Transient view state - like
/// the camera and [`GeoPick`], never a command, never saved. The wire is
/// always the same feature-edge set (`geometry::feature`, crease and
/// boundary); the modes differ in what happens *behind* it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayMode {
    /// Flat-shaded faces with the CAD wire over them - the default look.
    #[default]
    ShadedEdges,
    /// Wireframe only: faces go to the depth buffer and nowhere else, so
    /// the model reads as a hidden-line drawing; the stock - never a
    /// surface you trust to occlude anything - draws as its full wire.
    Wireframe,
}

/// The thing under the cursor, from the viewport hover pass: the named
/// entity, the facet index the raycast resolved (into `model::facets_of`'s
/// order for that entity), and - in Face/Edge pick mode - the feature the
/// hover resolves to, so the highlight draws the face boundary loops or the
/// chain instead of one triangle. Ephemeral view state, not a command
/// (AGENTS.md) - it only drives the screen-space highlight, never selection.
#[derive(Clone, Debug, PartialEq)]
pub struct HoverHit {
    pub entity: String,
    pub facet: usize,
    pub feature: Option<FeatureSel>,
}

/// M1.7c move-simulation playhead. View state like the camera and
/// [`DisplayMode`] - never a command, never saved (AGENTS.md exempts
/// playback position explicitly). The timeline itself is not stored
/// here: the viewport pass rebuilds it from the world every frame, so
/// a document switch needs no reset - the same `t` simply indexes the
/// new program.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sim {
    /// Playhead, seconds into the script.
    pub t: f64,
    pub playing: bool,
    /// Playback multiplier; real programs run minutes, so watching one
    /// at 1× is not a workflow - 1/2/4/8 are the transport's entries.
    pub speed: f64,
}

impl Sim {
    /// Arm the simulation at the start of the timeline, playing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            t: 0.0,
            playing: true,
            speed: 1.0,
        }
    }

    /// The playback law, pure so the ui tests pin it deterministically
    /// (frames under the harness advance by ~0): time moves only while
    /// playing, clamps at the timeline's end, and stops there - no
    /// wrap. Pathological seconds (NaN dt or total) freeze the
    /// playhead rather than poison it.
    pub fn advance(&mut self, dt: f64, total: f64) {
        if !self.playing || !(dt.is_finite() && total.is_finite()) {
            return;
        }
        self.t = (self.t + dt.max(0.0) * self.speed.max(0.0)).min(total.max(0.0));
        if self.t >= total {
            self.playing = false;
        }
    }
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

/// The M1.7d G-code viewer's content: the program one successful post
/// wrote, plus its scroll cursor. Unlike [`Sim`] - which is a clock over
/// live toolpaths and rebuilds itself every frame - this holds frozen
/// evidence of one specific file, so a document switch retires it (New
/// and Open clear it; an in-document edit does not: the program still
/// truthfully says what was posted, and re-posting replaces it).
#[derive(Clone, Debug)]
pub struct PostView {
    /// The file the program went to - the window's title.
    pub path: std::path::PathBuf,
    pub program: String,
    /// Per program line, the world position of the move it writes
    /// (`posting::Posted::line_pos`).
    pub line_pos: Vec<Option<[f64; 3]>>,
    /// Window visibility (egui::Window::open).
    pub open: bool,
    /// Sync cursor: the program line at the viewer's middle. Immediate
    /// view state like the playhead (AGENTS.md), written by the window's
    /// pass and read by the viewport's highlight.
    pub line: usize,
    /// The block-by-block trace of this same run (M3.3 stage D): which
    /// record wrote which lines. Frozen evidence like the text beside
    /// it - the viewer shows the trace the post actually did, never a
    /// re-derivation.
    pub blocks: Vec<camcraft_app::post::TraceBlock>,
    /// The trace pane's visibility. View state like the camera - not
    /// undoable, not saved; the block list belongs to the one program
    /// on screen, so a document switch retires the whole viewer.
    pub trace: bool,
}

pub struct State {
    pub world: World,
    pub history: History,
    pub status: String,
    /// Bumped whenever documents changed; drives GPU re-upload of meshes.
    pub doc_version: u64,
    /// Bumped whenever selection changed; re-sends the vertex stream (the
    /// selection tint is baked per vertex) without re-uploading geometry.
    pub selection_version: u64,
    /// Viewport image rect in logical screen coords; the shell publishes it
    /// each frame, `interact` hit-tests pointer events against it.
    pub viewport_rect: ([f32; 2], [f32; 2]),
    /// Live viewport drag (orbit/pan), driven by the shell's pointer layer.
    pub drag: Option<DragMode>,
    /// Facet currently under the cursor (`interact` refreshes it from the
    /// shell's pointer events with a filtered raycast; it clears when the
    /// pointer leaves the viewport). View state like the camera and the
    /// drag - not undoable, not saved.
    pub hover: Option<HoverHit>,
    /// True when the viewport panel was under the pointer last frame (the
    /// shell writes it, and gates pointer drags on it - a click on a menu
    /// or popup over the image must never orbit).
    pub viewport_hot: bool,
    pub cam: Camera,
    /// Models hidden in the viewport (eye toggle in the browser). View
    /// state like the camera - not undoable, not saved; a hidden model
    /// still cuts: visibility is display, not suppression (a toolpath
    /// computed from it keeps running - hiding geometry must never
    /// silently change what the machine does).
    pub hidden_models: Vec<String>,
    /// Operations whose cached path stays undrawn (eye toggle per op).
    /// Same rule: the path still exists and posts.
    pub hidden_paths: Vec<String>,

    /// Show the dimmed world origin axes in the viewport. View state like
    /// the camera - not undoable, not saved - toggled from the Models panel;
    /// the triads of real work offsets are always shown, because they are
    /// setup, not decoration. Hiding the world axes is for the moment a
    /// G5x offset sits away from machine zero and two gizmos read as clutter.
    pub show_world_axes: bool,
    /// Show cached toolpaths beyond the selected operation's: with this on,
    /// every generated path draws (dimmed where unselected) instead of only
    /// the one the tree highlights. View state like the camera - not
    /// undoable, not saved; the overlay rebuild gate reads it.
    pub show_all_paths: bool,
    /// How solids paint: shaded-with-wire or hidden-line wireframe. View
    /// state like the camera - not undoable, not saved; the render call
    /// reads it, nothing in the document does.
    pub display: DisplayMode,
    /// M1.7c move simulation: None while off, [`Sim`] while the
    /// transport bar is up. View state like the camera - what it holds
    /// is only the playhead; the timeline is rebuilt from the world
    /// each frame, so editing or switching documents never strands it.
    pub sim: Option<Sim>,
    /// M1.7d G-code viewer: None after any document switch or refused
    /// post, `Some` while a posted program is being reviewed.
    pub post_view: Option<PostView>,
    /// M2.3 program material sim: None while the Deviate cell is dark,
    /// `Some` while its sheet paints the stock's place. Frozen evidence
    /// of one program state, exactly like [`PostView`]'s text - never a
    /// command, never saved, retired by a document switch; an edit does
    /// not silently re-run it, re-arming re-judges.
    pub program_sim: Option<camcraft_app::stock_sim::ProgramSim>,
    pub pending: VecDeque<CommandKind>,
    pub request_quit: bool,
    /// Receiver for the in-flight import worker (None when idle). The worker
    /// runs `ImportModel::new` off-thread and sends the finished command back.
    import_rx: Option<mpsc::Receiver<Result<ImportModel, CommandError>>>,
    /// Path currently being imported on the worker; drives the status text.
    import_file: Option<PathBuf>,
    /// Wall-clock start of the live import, for an elapsed-seconds readout.
    import_started: Option<Instant>,
    /// Imports queued while one is already running (drained FIFO on finish).
    import_queue: VecDeque<PathBuf>,
    /// Setup pane drafts (ephemeral; see [`StockUi`]).
    pub stock_ui: StockUi,
    pub wcs_ui: WcsUi,
    /// Setups page draft (ephemeral; see [`SetupsUi`]).
    pub setups_ui: SetupsUi,
    /// Fixtures section placement draft (ephemeral; see [`FixtureUi`]).
    pub fixture_ui: FixtureUi,
    /// Model page placement draft (ephemeral; see [`ModelUi`]).
    pub model_ui: ModelUi,
    /// The setups-page context (setup binding) the next finished import
    /// gets tagged with - `Some` only between ImportFixtureDialog and the
    /// import's finish; discarded with the import itself.
    /// Set by the import-fixture dialog, consumed by `finish_import`: the
    /// binding the landed model is tagged with. `Some(None)` is the
    /// implicit first setup. Public so tests run the dialog hand-off
    /// without a modal file picker.
    pub import_as_fixture: Option<Option<String>>,
    /// Properties / Tool Library pane drafts (ephemeral; see [`OpUi`]).
    pub op_ui: OpUi,
    /// Viewport click routing: whole models vs feature faces vs edge
    /// chains. Ephemeral view state like the drafts above - the mode is
    /// set from the Operations page radio row, never saved or undone.
    pub geo_pick: GeoPick,
    /// The Geometry list row under the pointer this frame, if any: the
    /// pane answers "which one is that?" by lighting the feature in the
    /// viewport, like pointer hover does. Ephemeral view state (the
    /// hover itself is the precedent) - refreshed every frame, never a
    /// command, never an overlay rebuild.
    pub geo_hover: Option<GeoSel>,
    /// Which named section of the operation page is showing (the M1.4
    /// tabbing). Ephemeral pane state like the drafts it reveals - never a
    /// command, never saved; a kind without the section falls back to
    /// Geometry. Deliberately NOT inside [`OpUi`]: the drafts rebuild on
    /// every sync, and a tab reset on Apply would read as a bug.
    pub op_tab: usize,
    pub tool_ui: ToolUi,
    /// The tool editor window (M1.4): `None` closed, `Some(name)` editing
    /// that tool, `Some("")` the blank creation form. Ephemeral window
    /// state like [`State::op_tab`] - never a command, never saved, never
    /// undoable; the edits it drives are. `State::drain_actions` closes it
    /// when the named tool vanishes (the ghost law, mirrored from
    /// [`State::pane_target`]).
    pub tool_editor: Option<String>,
    /// The machine editor window (M3.3 stage C): `None` closed. An
    /// ephemeral draft like [`State::tool_editor`] - the numbers under
    /// edit are a copy of the machine until Apply pushes `LoadMachine`,
    /// which is the command, which is the undo.
    pub machine_editor: Option<crate::egui_app::machine_editor::MachineEditor>,
    /// `doc_version` the setup drafts were last refetched at.
    ui_synced_doc: u64,
    /// (draft identity, `doc_version` it was fetched at) per pane; see
    /// [`State::sync_ops_ui`] for the rule.
    op_synced: (String, u64),
    tool_synced: (String, u64),
    /// Which entity the docked context panes edit right now. Ephemeral view
    /// state like the camera - not a command, not undoable (AGENTS.md); the
    /// selection its clicks mirror is the command. [`State::pane_target`]
    /// validates it against the world every frame, so a target that
    /// disappears under it (delete, undo) falls back to the selection on
    /// its own. Replaces the old per-pane show-flags and the `.opened()`
    /// dance: the panes are persistent, what changes is their subject.
    pub ui_target: UiTarget,
    /// The last command's refusal, tagged with the pane it belongs to, so
    /// the failing pane can show the reason inline. The bottom status strip
    /// alone was invisible enough that "Add does nothing" read as a dead
    /// button. Cleared by every command that runs (see [`State::run`]).
    pub feedback: Option<(ErrorScope, String)>,
}

/// Which page of the Operations pane a refused command's message belongs
/// to (so a refusal lands where the user is looking, not only in the
/// status strip).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ErrorScope {
    /// The Tool page (add/update/delete a tool).
    Tool,
    /// The Operation page (create/edit/generate/delete an operation).
    Operation,
    /// The Stock and WCS pages.
    Setup,
}

/// What the docked context panes should edit. Set by browser-tree and
/// ribbon clicks and the menu through [`crate::actions::target`]; view
/// state, never a command (the `Select` a click mirrors into the world is
/// the command).
#[derive(Clone, Debug, PartialEq)]
pub enum UiTarget {
    /// Nothing targeted: panes fall back to the world selection.
    None,
    /// The stock solid (its page doubles as the creation form).
    Stock,
    /// One of [`camcraft_app::schema::WCS_SLOTS`], by index.
    WcsSlot(usize),
    /// A model entity: the info page.
    Model(String),
    /// A tool entity, or the empty name for the tool page as destination
    /// (the catalog / new-draft form, e.g. CAM > Tool Library).
    Tool(String),
    /// An operation entity: its properties.
    Operation(String),
    /// The Setups page (M3.2 A): always resolves - it doubles as the
    /// creation form, like Stock and the empty-name Tool page.
    Setups,
}

impl UiTarget {
    /// Whether this target still has something to show: named entities
    /// must exist with the component they were targeted for (an undone
    /// operation stops holding the pane), slot indexes must be real, and
    /// the pages that double as creation forms - Stock, and the tool page
    /// with an empty name - survive having no entity yet.
    #[must_use]
    pub fn resolves_in(&self, world: &World) -> bool {
        match self {
            UiTarget::None => false,
            UiTarget::Stock => true,
            UiTarget::WcsSlot(i) => *i < WCS_SLOTS.len(),
            UiTarget::Model(name) => model::model_entities(world)
                .into_iter()
                .any(|(n, _)| *name == n),
            UiTarget::Tool(name) => {
                name.is_empty()
                    || model::find_by_name(world, name).is_some_and(|e| model::is_tool(world, &e))
            }
            UiTarget::Operation(name) => {
                model::find_by_name(world, name).is_some_and(|e| model::is_operation(world, &e))
            }
            UiTarget::Setups => true,
        }
    }
}

impl State {
    #[must_use]
    pub fn new() -> Self {
        State {
            world: new_world(),
            history: History::default(),
            status: String::from("ready"),
            doc_version: 0,
            selection_version: 0,
            viewport_rect: ([0.0, 0.0], [0.0, 0.0]),
            drag: None,
            hover: None,
            viewport_hot: false,
            geo_pick: GeoPick::Model,
            geo_hover: None,
            op_tab: 0,
            hidden_models: Vec::new(),
            hidden_paths: Vec::new(),
            cam: Camera::default(),
            show_world_axes: true,
            show_all_paths: false,
            display: DisplayMode::default(),
            sim: None,
            post_view: None,
            program_sim: None,
            pending: VecDeque::new(),
            request_quit: false,
            import_rx: None,
            import_file: None,
            import_started: None,
            import_queue: VecDeque::new(),
            stock_ui: StockUi::default(),
            wcs_ui: WcsUi {
                // Neutral highlight: center-middle, before any pick.
                anchor_xy: 4,
                anchor_z: 1,
                ..WcsUi::default()
            },
            setups_ui: SetupsUi::default(),
            fixture_ui: FixtureUi::default(),
            model_ui: ModelUi::default(),
            import_as_fixture: None,
            // First-launch drafts carry plausible names: an empty Name
            // field made the Add buttons look dead (refused silently).
            op_ui: OpUi {
                name: String::from("Operation1"),
                ..OpUi::default()
            },
            tool_ui: ToolUi {
                name: String::from("Tool1"),
                ..ToolUi::default()
            },
            tool_editor: None,
            machine_editor: None,
            ui_synced_doc: 0,
            op_synced: (String::new(), 0),
            tool_synced: (String::new(), 0),
            ui_target: UiTarget::None,
            feedback: None,
        }
    }

    /// The refusal a pane of this scope should show inline, if the last
    /// command to run was a refusal that belongs to it.
    #[must_use]
    pub fn error_for(&self, scope: ErrorScope) -> Option<&str> {
        self.feedback
            .as_ref()
            .filter(|(s, _)| *s == scope)
            .map(|(_, msg)| msg.as_str())
    }

    /// A blank tool page with a numbered name: New starts a real draft,
    /// not an empty field the Add button would only refuse.
    #[must_use]
    pub fn new_tool_draft(&self) -> ToolUi {
        let n = self.tool_names().len() + 1;
        let starter = catalog().remove(0);
        ToolUi::from_tool(&format!("Tool{n}"), &starter.1, false)
    }

    /// Record a refusal: status strip for everyone, tagged message for the
    /// pane that caused it.
    fn refuse(&mut self, scope: ErrorScope, msg: String) {
        self.status.clone_from(&msg);
        self.feedback = Some((scope, msg));
    }

    /// Refetch the Setup pane's drafts from the world after any document
    /// change (Apply, Undo, Open...). Between bumps the user's in-progress
    /// typing is left alone; a refused command changes nothing, so the
    /// offending values stay visible for correction (Prime Directive).
    pub fn sync_setup_ui(&mut self) {
        if self.ui_synced_doc == self.doc_version {
            return;
        }
        self.ui_synced_doc = self.doc_version;
        // The drafts mirror the ACTIVE setup's furniture (M3.2 A): with
        // no setups declared this is the legacy `Stock`/`G54` entities,
        // byte for byte; with them, the pane edits where the user works.
        let active = camcraft_app::setup::active(&self.world);
        let stock_name = camcraft_app::setup::stock_entity_name(&self.world, active.as_deref());
        if let Some(entity) = model::find_by_name(&self.world, &stock_name)
            && model::is_stock(&self.world, &entity)
        {
            if let Some(loc) = model::transform_of(&self.world, &entity) {
                self.stock_ui.x = loc.pos.x;
                self.stock_ui.y = loc.pos.y;
                self.stock_ui.z = loc.pos.z;
                self.stock_ui.yaw_deg = quat_yaw_deg(loc.rot);
            }
            match model::stock_def_of(&self.world, &entity) {
                Some(StockShape::Box { dx, dy, dz }) => {
                    self.stock_ui.shape = 0;
                    self.stock_ui.dx = dx;
                    self.stock_ui.dy = dy;
                    self.stock_ui.dz = dz;
                }
                Some(StockShape::Cylinder { diameter, height }) => {
                    self.stock_ui.shape = 1;
                    self.stock_ui.diameter = diameter;
                    self.stock_ui.height = height;
                }
                Some(StockShape::FromModel { source }) => {
                    self.stock_ui.shape = 2;
                    self.stock_ui.source = source;
                }
                None => {}
            }
        }
        let placed =
            camcraft_app::setup::wcs_entity(&self.world, active.as_deref(), self.wcs_ui.slot)
                .and_then(|entity| model::transform_of(&self.world, &entity));
        match placed {
            Some(loc) => {
                self.wcs_ui.x = loc.pos.x;
                self.wcs_ui.y = loc.pos.y;
                self.wcs_ui.z = loc.pos.z;
                let (roll, pitch, yaw) = quat_to_euler(loc.rot);
                self.wcs_ui.roll_deg = roll.to_degrees();
                self.wcs_ui.pitch_deg = pitch.to_degrees();
                self.wcs_ui.yaw_deg = yaw.to_degrees();
                self.wcs_ui.placed = true;
            }
            None => {
                // Show zero only when an origin this slot *had* disappeared
                // (undo): stale numbers for a deleted entity are a lie.
                // Typing into a never-placed slot survives other edits.
                if self.wcs_ui.placed {
                    self.wcs_ui.x = 0.0;
                    self.wcs_ui.y = 0.0;
                    self.wcs_ui.z = 0.0;
                    self.wcs_ui.roll_deg = 0.0;
                    self.wcs_ui.pitch_deg = 0.0;
                    self.wcs_ui.yaw_deg = 0.0;
                    self.wcs_ui.placed = false;
                }
            }
        }
        // The Model page's placement draft (M5.2 G) follows the same
        // law as the WCS one: whenever the document moves under the
        // pane - an undo, a sibling command - the world's numbers
        // re-enter the draft for the model actually on screen.
        if let UiTarget::Model(name) = self.pane_target()
            && let Some(entity) = model::find_by_name(&self.world, &name)
            && let Some(loc) = model::transform_of(&self.world, &entity)
        {
            self.model_ui.name = name;
            self.model_ui.x = loc.pos.x;
            self.model_ui.y = loc.pos.y;
            self.model_ui.z = loc.pos.z;
            let (roll, pitch, yaw) = quat_to_euler(loc.rot);
            self.model_ui.roll_deg = roll.to_degrees();
            self.model_ui.pitch_deg = pitch.to_degrees();
            self.model_ui.yaw_deg = yaw.to_degrees();
        }
    }

    /// What the context panes actually edit this frame: the explicit
    /// target while it resolves, else the world selection, else nothing
    /// (the creation forms). Panes and drafts route through this instead
    /// of reading the raw target - a vanished one never leaves them
    /// pointing at a ghost.
    #[must_use]
    pub fn pane_target(&self) -> UiTarget {
        if self.ui_target.resolves_in(&self.world) {
            return self.ui_target.clone();
        }
        match self.selected_operation() {
            Some(name) => UiTarget::Operation(name),
            None => UiTarget::None,
        }
    }

    /// Refetch the Properties pane's draft after any document or selection
    /// change. While a real operation is targeted (or falls out of the
    /// selection) its draft always mirrors the world; the "new operation"
    /// form survives document bumps, since there is no reality to refetch
    /// until it is added.
    pub fn sync_ops_ui(&mut self) {
        let ident = match self.pane_target() {
            UiTarget::Operation(name) => name,
            _ => String::new(),
        };
        if self.op_synced == (ident.clone(), self.doc_version) {
            return;
        }
        let identity_changed = self.op_synced.0 != ident;
        self.op_synced = (ident, self.doc_version);
        if !self.op_synced.0.is_empty() {
            let name = self.op_synced.0.clone();
            if let Some(entity) = model::find_by_name(&self.world, &name)
                && let Some(op) = model::operation_of(&self.world, &entity)
            {
                self.op_ui = OpUi::from_op(&name, &op);
            }
        } else if identity_changed {
            // Number the prefilled name past the *taken* names, not just
            // past the count - a document with holes in its numbering (a
            // deleted middle op) would otherwise prefill one that is in
            // use and make Add look dead. Same loop the ribbon quick-add
            // walks (`actions::add_operation`).
            let mut n = self.operation_count() + 1;
            let name = loop {
                let candidate = format!("Operation{n}");
                if model::find_by_name(&self.world, &candidate).is_none() {
                    break candidate;
                }
                n += 1;
            };
            self.op_ui = OpUi {
                name,
                tool: self.tool_names().pop().unwrap_or_default(),
                ..OpUi::default()
            };
        }
    }

    /// Arm the Operations page's Apply click: a draft that differs from
    /// the operation queues the edit for the next drain (validated,
    /// undoable, as always); a draft that does not differ changes nothing,
    /// so nothing goes into the history - but the click still answers
    /// through the status strip. Silence on a click reads as a dead
    /// button; this is the same ephemeral-ack pattern the feature picks
    /// use, not a command.
    pub fn apply_operation_params(&mut self, name: &str) {
        let unchanged = model::find_by_name(&self.world, name)
            .and_then(|e| model::operation_of(&self.world, &e))
            .is_some_and(|op| op.params == self.op_ui.params);
        if unchanged {
            self.status = String::from("parameters already applied - nothing changed");
        } else {
            self.pending.push_back(CommandKind::EditOperationParams {
                name: name.to_string(),
                params: self.op_ui.params.clone(),
            });
        }
    }

    /// Arm the Generate click (the pane's button and the menu's item for
    /// the selected operation both route here): a draft that differs from
    /// the operation's committed parameters is applied first - one click,
    /// two honest commands - so the path is never generated from stale
    /// numbers while the panel shows edited ones. A draft that mirrors a
    /// different page (another op's mirror, the new-op form) never flushes
    /// into this operation; it generates the committed parameters alone.
    pub fn generate_operation(&mut self, name: &str) {
        let dirty = name == self.op_ui.name
            && model::find_by_name(&self.world, name)
                .and_then(|e| model::operation_of(&self.world, &e))
                .is_some_and(|op| op.params != self.op_ui.params);
        if dirty {
            self.pending.push_back(CommandKind::EditOperationParams {
                name: name.to_string(),
                params: self.op_ui.params.clone(),
            });
        }
        self.pending
            .push_back(CommandKind::GenerateOperation(name.to_string()));
    }

    /// Arm the Drill page's "Detect holes" click: recognize against the
    /// operation's source geometry *now* - one pass per click, never per
    /// paint frame - and commit the result as an undoable parameter edit.
    /// The geometry comes from the draft when this pane targets the
    /// operation (what the user sees is what gets stored, so a dirty
    /// draft's other edits ride along and the panel never shows holes the
    /// world does not have); a click aimed at another operation refetches
    /// from the world instead of silently flushing some other pane's
    /// unsaved typing.
    pub fn detect_holes(&mut self, name: &str) {
        let Some(entity) = model::find_by_name(&self.world, name) else {
            self.refuse(
                ErrorScope::Operation,
                format!("no operation named '{name}'"),
            );
            return;
        };
        let Some(op) = model::operation_of(&self.world, &entity) else {
            self.refuse(
                ErrorScope::Operation,
                format!("'{name}' is not an operation"),
            );
            return;
        };
        let draft = name == self.op_ui.name;
        let mut params = if draft {
            self.op_ui.params.clone()
        } else {
            op.params.clone()
        };
        let OpParams::Drill { holes, cross, .. } = &mut params else {
            self.refuse(
                ErrorScope::Operation,
                format!("'{name}' is not a drilling operation"),
            );
            return;
        };
        // A document whose machine holds a turret gets the radial
        // reading beside the mill's (M5.2 F2): the same mesh, its
        // other half - walls whose axis runs AT the spindle. A plain
        // mill asks nothing of the mesh's sides; its cross list stays
        // the empty one it was (an op typed at a machine that cannot
        // plunge it is the generator's refusal, not this click's).
        let radial = if machine::kind_of(&self.world) == machine::Kind::Mill {
            Ok(Vec::new())
        } else {
            operation::cross_holes_of(&self.world, op.setup.as_deref(), &op.src)
        };
        match (
            operation::holes_of(&self.world, op.setup.as_deref(), &op.src),
            radial,
        ) {
            (Err(err), _) | (_, Err(err)) => self.refuse(ErrorScope::Operation, err.to_string()),
            (Ok(z), Ok(x)) if z.is_empty() && x.is_empty() => self.refuse(
                ErrorScope::Operation,
                "no drilled holes found in the source model - Z or radial".to_string(),
            ),
            (Ok(z), Ok(x)) => {
                *holes = z;
                *cross = x;
                if draft {
                    self.op_ui.params = params.clone();
                }
                self.pending.push_back(CommandKind::EditOperationParams {
                    name: name.to_string(),
                    params,
                });
            }
        }
    }

    /// The document's display unit - a read of the flecs singleton, so it
    /// is always current whatever ran (Apply, Undo, Open): there is no
    /// mirror to go stale. Millimeters until `SetUnits` or a v8 file says
    /// otherwise (M1.1).
    #[must_use]
    pub fn units(&self) -> Unit {
        units::of(&self.world)
    }

    /// The Geometry section's list: one row per pick-list entry, in order,
    /// resolved against the model as it stands now. Each row *describes*
    /// its pick in machining terms - a face says where it is and how big
    /// ("Face part · z=5.00 · 100.0 mm²"), a chain says whether it closes
    /// and how long ("Chain part · open · 10.0 mm · 2 of 3") - so the
    /// list answers "what did I select?" without a hash decoder. A row
    /// whose id no longer resolves (the model was deleted, or re-imported
    /// with new geometry) reads amber and falls back to its short id with
    /// "missing" - removing it is always safe, because the reduced list
    /// only gets shorter.
    pub fn geo_chips(&self) -> Vec<GeoChip> {
        // Open runs are pieces of one loop the user watches get
        // completed: the "2 of 3" in a run's label counts chain picks
        // of the same entity, in pick order.
        let mut run_totals: HashMap<String, usize> = HashMap::new();
        for g in &self.op_ui.geo {
            if matches!(g.feature, FeatureSel::Chain(_)) {
                *run_totals.entry(g.entity.clone()).or_default() += 1;
            }
        }
        let mut run_seq: HashMap<String, usize> = HashMap::new();
        // Which stitched ring each pick took part in, from the stitcher's
        // own walk - the same one the region geometry is built through,
        // so a row's `· → ring k` cannot disagree with the preview.
        let stitched = operation::stitched_rings(&self.world, &self.op_ui.geo);
        // One lens for the whole row set: chip numbers follow the
        // document's unit (M1.1); the mm law underneath is untouched.
        let unit = units::of(&self.world);
        self.op_ui
            .geo
            .iter()
            .enumerate()
            .map(|(gi, g)| {
                let (mut label, missing) = match g.feature {
                    FeatureSel::Face(id) => {
                        let face = model::find_by_name(&self.world, &g.entity)
                            .and_then(|e| model::features_of(&self.world, &e))
                            .and_then(|f| f.faces.iter().find(|fa| fa.id == id).cloned());
                        match face {
                            Some(f) => {
                                // Which axis the face sits at, on the
                                // sign-canonical normal's own axis: the
                                // offset is signed, so top and bottom
                                // both read as plain z.
                                let (axis, v) = if f.normal[2].abs()
                                    >= f.normal[0].abs().max(f.normal[1].abs())
                                {
                                    ('z', f.offset)
                                } else if f.normal[0].abs() >= f.normal[1].abs() {
                                    ('x', f.offset)
                                } else {
                                    ('y', f.offset)
                                };
                                let mut label = format!(
                                    "Face {} · {axis}={} · {} {}",
                                    g.entity,
                                    units::len(v, unit, 2),
                                    units::area(f.area, unit),
                                    unit.area_suffix()
                                );
                                if f.loops.len() > 1 {
                                    label.push_str(&format!(" · {} loops", f.loops.len()));
                                }
                                (label, false)
                            }
                            None => (
                                format!("Face {} · {} · missing", g.entity, short_id(id)),
                                true,
                            ),
                        }
                    }
                    FeatureSel::Chain(id) => {
                        let found = model::find_by_name(&self.world, &g.entity)
                            .and_then(|e| model::features_of(&self.world, &e))
                            .map(|f| {
                                let c = feature::chains(&f).into_iter().find(|c| c.id == id)?;
                                let pts = feature::chain_points(&f, &c);
                                Some((c, pts))
                            })
                            .unwrap_or(None);
                        match found {
                            Some((c, pts)) => {
                                // Length along the polyline; a closed
                                // chain's points do not repeat the start
                                // vertex, so the ring closure counts too.
                                let mut len = 0.0;
                                for w in pts.windows(2) {
                                    let (dx, dy, dz) =
                                        (w[1][0] - w[0][0], w[1][1] - w[0][1], w[1][2] - w[0][2]);
                                    len += (dx * dx + dy * dy + dz * dz).sqrt();
                                }
                                if c.closed && pts.len() > 1 {
                                    let (a, b) = (&pts[0], &pts[pts.len() - 1]);
                                    let (dx, dy, dz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
                                    len += (dx * dx + dy * dy + dz * dz).sqrt();
                                }
                                let label = if c.closed {
                                    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
                                    for p in &pts {
                                        for k in 0..2 {
                                            lo[k] = lo[k].min(p[k]);
                                            hi[k] = hi[k].max(p[k]);
                                        }
                                    }
                                    format!(
                                        "Chain {} · closed · {} {} · {} × {}",
                                        g.entity,
                                        units::len(len, unit, 1),
                                        unit.suffix(),
                                        units::len(hi[0] - lo[0], unit, 1),
                                        units::len(hi[1] - lo[1], unit, 1)
                                    )
                                } else {
                                    let i = run_seq.entry(g.entity.clone()).or_default();
                                    *i += 1;
                                    format!(
                                        "Chain {} · open · {} {} · {} of {}",
                                        g.entity,
                                        units::len(len, unit, 1),
                                        unit.suffix(),
                                        i,
                                        run_totals.get(&g.entity).copied().unwrap_or(1)
                                    )
                                };
                                (label, false)
                            }
                            None => {
                                let label =
                                    format!("Chain {} · {} · missing", g.entity, short_id(id));
                                (label, true)
                            }
                        }
                    }
                    FeatureSel::Patch(id) => {
                        // Resolve everything while the features live:
                        // the rings come back as plain point runs,
                        // XY-deduped by vertex set so a closed wall's
                        // coincident rims read as the one outline they
                        // are. Display only - the generator applies the
                        // same dedupe exactly, through `push_loop`.
                        let found = model::find_by_name(&self.world, &g.entity)
                            .and_then(|e| model::features_of(&self.world, &e))
                            .and_then(|f| {
                                let strips = feature::patches(&f)
                                    .into_iter()
                                    .find(|p| p.id == id)?
                                    .faces
                                    .len();
                                let mut seen: Vec<Vec<(i64, i64)>> = Vec::new();
                                let mut closed: Vec<Vec<[f64; 3]>> = Vec::new();
                                let mut open = false;
                                for r in feature::patch_rings(&f, id)? {
                                    let pts: Vec<[f64; 3]> = r
                                        .verts
                                        .iter()
                                        .filter_map(|&v| f.verts.get(v as usize).copied())
                                        .collect();
                                    open |= !r.closed;
                                    if !r.closed || pts.len() < 3 {
                                        continue;
                                    }
                                    let key = xy_set_key(&pts);
                                    if seen.contains(&key) {
                                        continue;
                                    }
                                    seen.push(key);
                                    closed.push(pts);
                                }
                                Some((strips, closed, open))
                            });
                        match found {
                            Some((strips, closed, _open)) => {
                                let label = match closed
                                    .iter()
                                    .max_by(|a, b| span(a).total_cmp(&span(b)))
                                {
                                    Some(rim) => {
                                        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
                                        for p in rim {
                                            for k in 0..2 {
                                                lo[k] = lo[k].min(p[k]);
                                                hi[k] = hi[k].max(p[k]);
                                            }
                                        }
                                        let shape = match fit_circle(rim) {
                                            Some((c, r))
                                                if rim.iter().all(|p| {
                                                    let d = ((p[0] - c[0]).powi(2)
                                                        + (p[1] - c[1]).powi(2))
                                                    .sqrt();
                                                    (d - r).abs() <= r.max(1e-3) * 0.02
                                                }) =>
                                            {
                                                format!("bore Ø{}", units::len(2.0 * r, unit, 1))
                                            }
                                            _ => format!(
                                                "{} × {}",
                                                units::len(hi[0] - lo[0], unit, 1),
                                                units::len(hi[1] - lo[1], unit, 1)
                                            ),
                                        };
                                        format!(
                                            "Welded face {} · {shape} · {} {} · {strips} strips",
                                            g.entity,
                                            units::area(shoelace(rim), unit),
                                            unit.area_suffix()
                                        )
                                    }
                                    None => format!("Welded face {} · open band", g.entity),
                                };
                                (label, false)
                            }
                            None => {
                                let label = format!(
                                    "Welded face {} · {} · missing",
                                    g.entity,
                                    short_id(id)
                                );
                                (label, true)
                            }
                        }
                    }
                };
                if !missing && let Some(k) = stitched.get(gi).copied().flatten() {
                    label.push_str(&format!(" · → ring {}", k + 1));
                }
                GeoChip {
                    sel: g.clone(),
                    label,
                    missing,
                }
            })
            .collect()
    }

    /// Refetch the Tool Library pane's draft after document changes. The
    /// name decides the mode: it resolves to a tool entity (mirror it,
    /// `exists = true` - even after an undo brought it back), to nothing
    /// (keep the typed numbers with `exists = false`, so a deleted tool's
    /// page doubles as re-add), or is empty (the untouched New page).
    pub fn sync_tool_ui(&mut self) {
        if self.tool_synced == (self.tool_ui.name.clone(), self.doc_version) {
            return;
        }
        let ident = self.tool_ui.name.clone();
        self.tool_synced = (ident.clone(), self.doc_version);
        if ident.is_empty() {
            return;
        }
        match model::find_by_name(&self.world, &ident).and_then(|e| model::tool_of(&self.world, &e))
        {
            Some(tool) => self.tool_ui = ToolUi::from_tool(&ident, &tool, true),
            None => self.tool_ui.exists = false,
        }
    }

    /// Name of the selected entity if it is an operation (single selection
    /// in practice; the first one wins).
    #[must_use]
    pub fn selected_operation(&self) -> Option<String> {
        model::selected_names(&self.world).into_iter().find(|name| {
            model::find_by_name(&self.world, name)
                .is_some_and(|e| model::is_operation(&self.world, &e))
        })
    }

    /// Names of the tool entities, in document order.
    #[must_use]
    pub fn tool_names(&self) -> Vec<String> {
        model::named_entities(&self.world)
            .into_iter()
            .filter(|(_, e)| model::is_tool(&self.world, e))
            .map(|(name, _)| name)
            .collect()
    }

    #[must_use]
    pub(crate) fn operation_count(&self) -> usize {
        model::named_entities(&self.world)
            .into_iter()
            .filter(|(_, e)| model::is_operation(&self.world, e))
            .count()
    }

    /// Flip one name's visibility in a browser eye list.
    pub(crate) fn toggle_visibility(list: &mut Vec<String>, name: &str) {
        match list.iter().position(|n| n == name) {
            Some(i) => drop(list.remove(i)),
            None => list.push(name.to_string()),
        }
    }

    /// Run every queued action, then poll the import worker once. Safe to
    /// call every frame; never blocks.
    pub fn drain_actions(&mut self) {
        while let Some(action) = self.pending.pop_front() {
            self.run(action);
        }
        self.poll_import();
        // The editor window's ghost law (M1.4), mirroring `pane_target`:
        // an open editor never keeps pointing at a name the document lost
        // under it - deleted, undone, or swapped by New/Open all land here.
        // The blank form creates rather than points, so it survives.
        if let Some(name) = &self.tool_editor
            && !name.is_empty()
            && !model::find_by_name(&self.world, name)
                .is_some_and(|e| model::is_tool(&self.world, &e))
        {
            self.tool_editor = None;
        }
    }

    /// Drive the queue and the import worker to completion, blocking. For
    /// tests and headless runs only - the interactive loop never calls this.
    pub fn settle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            self.drain_actions();
            if self.pending.is_empty() && self.import_rx.is_none() && self.import_queue.is_empty() {
                return;
            }
            if Instant::now() >= deadline {
                self.abandon_imports();
                self.status = String::from("import timed out");
                eprintln!("settle: queued work did not finish before the deadline");
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn run(&mut self, action: CommandKind) {
        // Whatever ran, the banner shows only the newest outcome: a later
        // command (even a successful one) retires an old refusal.
        self.feedback = None;
        match action {
            CommandKind::OpenModelDialog => {
                if let Some(path) = pick_mesh_file() {
                    self.import_path(path);
                }
            }
            CommandKind::ImportFile(path) => self.import_path(path),
            CommandKind::NewDocument => {
                self.abandon_imports();
                model::clear_named(&self.world);
                // Units belong to the document, and clear_named cannot
                // reach a singleton: reset the lens so a fresh blank is
                // millimeters, never the last document's inches. The
                // machine (M2.3 C) is the same furniture - a blank never
                // inherits the last document's limits.
                units::set(&self.world, Unit::default());
                machine::set(&self.world, "", None);
                self.history.clear();
                // Frozen evidence of the old document retires with it.
                self.post_view = None;
                self.program_sim = None;
                self.machine_editor = None;
                self.doc_version += 1;
                self.status = String::from("new document");
            }
            CommandKind::OpenProjectDialog => {
                if let Some(path) = pick_project_file() {
                    self.open_project_path(path);
                }
            }
            CommandKind::OpenProjectFile(path) => self.open_project_path(path),
            CommandKind::SaveProjectDialog => {
                if let Some(path) = save_project_dialog() {
                    match project::save(&self.world, &path) {
                        Ok(()) => self.status = format!("saved {}", path.display()),
                        Err(err) => self.status = format!("save failed: {err}"),
                    }
                }
            }
            CommandKind::SaveProjectFile(path) => match project::save(&self.world, &path) {
                Ok(()) => self.status = format!("saved {}", path.display()),
                Err(err) => self.status = format!("save failed: {err}"),
            },
            CommandKind::PostProgramDialog => {
                if let Some(path) = post_program_dialog() {
                    self.post_program_path(path);
                }
            }
            CommandKind::PostProgramFile(path) => self.post_program_path(path),
            CommandKind::Undo => {
                self.status = if self.history.undo(&self.world) {
                    self.doc_version += 1;
                    String::from("undo")
                } else {
                    String::from("nothing to undo")
                };
            }
            CommandKind::Redo => {
                self.status = if self.history.redo(&self.world) {
                    self.doc_version += 1;
                    String::from("redo")
                } else {
                    String::from("nothing to redo")
                };
            }
            CommandKind::DeleteSelected => {
                let names = model::selected_names(&self.world);
                let mut done = 0usize;
                for name in names {
                    match DeleteEntity::new(&self.world, &name) {
                        Ok(command) => {
                            self.history.execute(&self.world, command);
                            done += 1;
                        }
                        Err(err) => self.status = format!("delete failed: {err}"),
                    }
                }
                if done > 0 {
                    self.doc_version += 1;
                    self.status = format!("deleted {done} model(s)");
                }
            }
            CommandKind::Select(name) => {
                History::execute_ephemeral(
                    &self.world,
                    SelectEntity::new(&self.world, name.as_deref()),
                );
                self.selection_version = self.selection_version.wrapping_add(1);
            }
            CommandKind::FrameAll => self.frame_all(),
            CommandKind::Quit => self.request_quit = true,
            CommandKind::DeleteByName(name) => match DeleteEntity::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("deleted {name}");
                }
                Err(err) => self.refuse(ErrorScope::Setup, format!("delete failed: {err}")),
            },
            CommandKind::SetStock {
                shape,
                pos,
                yaw_deg,
            } => {
                let location = Location {
                    pos: Point3::new(pos[0], pos[1], pos[2]),
                    rot: quat_from_yaw(yaw_deg.to_radians()),
                };
                match SetStock::new(&self.world, shape, location) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = String::from("stock set");
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("stock not set: {err}")),
                }
            }
            CommandKind::SetUnits(next) => match SetUnits::new(&self.world, next) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("units: {}", next.label());
                }
                Err(err) => self.refuse(ErrorScope::Setup, format!("units not set: {err}")),
            },
            CommandKind::LoadMachineDialog => {
                if let Some(path) = pick_machine_file() {
                    self.load_machine_path(path);
                }
            }
            CommandKind::LoadMachineFile(path) => self.load_machine_path(path),
            CommandKind::ClearMachine => match LoadMachine::new(&self.world, String::new(), None) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = String::from("machine cleared");
                }
                Err(err) => self.refuse(ErrorScope::Setup, format!("machine not cleared: {err}")),
            },
            CommandKind::LoadBuiltinMachine(name) => {
                match LoadMachine::builtin(&self.world, &name) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.machine_editor = None;
                        self.status = format!("machine: {name} (built-in)");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("machine not loaded: {err}"))
                    }
                }
            }
            CommandKind::EditMachineDialog => {
                let (path, m) = machine::of(&self.world);
                match m {
                    Some(m) => {
                        self.machine_editor =
                            Some(crate::egui_app::machine_editor::MachineEditor {
                                path,
                                machine: m,
                            });
                    }
                    None => self.refuse(
                        ErrorScope::Setup,
                        String::from("no machine to edit - load one first"),
                    ),
                }
            }
            CommandKind::EditMachineApply { path, machine } => {
                match LoadMachine::new(&self.world, path, Some(*machine)) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.machine_editor = None;
                        self.status = String::from("machine edited");
                    }
                    // Refused edits keep the window open: the draft is
                    // someone's work, and a silent close would be the
                    // silent wrong answer with the work thrown away.
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("machine edit refused: {err}"))
                    }
                }
            }
            CommandKind::SelectPostBuiltin => self.select_post(PostRef::Builtin),
            CommandKind::SelectPostDialog => {
                if let Some(path) = pick_post_file() {
                    self.select_post(PostRef::File(path.display().to_string()));
                }
            }
            CommandKind::SelectPostFile(path) => {
                self.select_post(PostRef::File(path.display().to_string()));
            }
            CommandKind::SelectPostNamed(name) => self.select_post(PostRef::Named(name)),
            CommandKind::StockFit {
                margin,
                shape: radio,
            } => {
                if radio == 2 {
                    // From-model stock IS its source's shape; there is
                    // nothing to wrap. Refuse visibly, never guess.
                    self.refuse(
                        ErrorScope::Setup,
                        String::from(
                            "from-model stock follows its source: choose Box or Cylinder to fit",
                        ),
                    );
                    return;
                }
                let Some(bounds) = model::solid_world_aabb(&self.world) else {
                    self.refuse(
                        ErrorScope::Setup,
                        String::from("nothing to fit: no models in the document"),
                    );
                    return;
                };
                let Some((shape, pos)) = (if radio == 1 {
                    stock::fit_cylinder(&bounds, margin)
                } else {
                    stock::fit_box(&bounds, margin)
                }) else {
                    self.refuse(
                        ErrorScope::Setup,
                        String::from("stock fit refused: models have no usable extent"),
                    );
                    return;
                };
                // Fit replaces size and XY/Z placement; a deliberately
                // oriented stock keeps its rotation.
                // The fit replaces the ACTIVE setup's stock, and keeps
                // that entity's deliberate rotation (M3.2 A: the name is
                // resolved here; the command re-resolves at construction).
                let stock_name = camcraft_app::setup::stock_entity_name(
                    &self.world,
                    camcraft_app::setup::active(&self.world).as_deref(),
                );
                let rot = model::find_by_name(&self.world, &stock_name)
                    .and_then(|entity| model::transform_of(&self.world, &entity))
                    .map_or_else(|| Location::default().rot, |loc| loc.rot);
                let location = Location { pos, rot };
                match SetStock::new(&self.world, shape, location) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = String::from("stock fitted to models");
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("stock not fitted: {err}")),
                }
            }
            CommandKind::SetWcs {
                slot,
                pos,
                roll_deg,
                pitch_deg,
                yaw_deg,
            } => {
                // The axis-angle builders quietly substitute identity for
                // non-finite angles; refuse that here rather than silently
                // dropping a NaN rotation (Prime Directive).
                if !roll_deg.is_finite() || !pitch_deg.is_finite() || !yaw_deg.is_finite() {
                    self.refuse(
                        ErrorScope::Setup,
                        format!("offset not set: {slot} rotation angles must be finite"),
                    );
                    return;
                }
                let location = Location {
                    pos: Point3::new(pos[0], pos[1], pos[2]),
                    rot: quat_from_euler(
                        roll_deg.to_radians(),
                        pitch_deg.to_radians(),
                        yaw_deg.to_radians(),
                    ),
                };
                match SetWcsOrigin::new(&self.world, &slot, location) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("{slot} origin set");
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("offset not set: {err}")),
                }
            }
            CommandKind::AddTool { name, tool } => match AddTool::new(&self.world, &name, tool) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("tool {name} added");
                }
                Err(err) => self.refuse(ErrorScope::Tool, format!("tool not added: {err}")),
            },
            CommandKind::ChangeTool { name, tool } => {
                match ChangeTool::new(&self.world, &name, tool) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("tool {name} updated");
                    }
                    Err(err) => self.refuse(ErrorScope::Tool, format!("tool not updated: {err}")),
                }
            }
            CommandKind::DeleteTool(name) => match DeleteTool::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("deleted tool {name}");
                }
                Err(err) => self.refuse(ErrorScope::Tool, format!("tool not deleted: {err}")),
            },
            CommandKind::CreateOperation {
                name,
                tool,
                src,
                params,
            } => match CreateOperation::new(&self.world, &name, &tool, src, params) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("operation {name} created");
                }
                Err(err) => {
                    self.refuse(
                        ErrorScope::Operation,
                        format!("operation not created: {err}"),
                    );
                }
            },
            CommandKind::EditOperationParams { name, params } => {
                match EditOperationParams::new(&self.world, &name, params) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("parameters updated for {name}");
                    }
                    Err(err) => {
                        self.refuse(
                            ErrorScope::Operation,
                            format!("parameters not updated: {err}"),
                        );
                    }
                }
            }
            CommandKind::EditOperationLinks { name, tool, src } => {
                let links = EditOperationLinks::new(&self.world, &name, tool.as_deref(), src);
                match links {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("links updated for {name}");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Operation, format!("links not updated: {err}"));
                    }
                }
            }
            CommandKind::SetOperationGeometry { name, items } => {
                match SetOperationGeometry::new(&self.world, &name, items) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("geometry selection updated for {name}");
                    }
                    Err(err) => {
                        self.refuse(
                            ErrorScope::Operation,
                            format!("geometry selection not updated: {err}"),
                        );
                    }
                }
            }
            CommandKind::DeleteOperation(name) => match DeleteOperation::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("deleted operation {name}");
                }
                Err(err) => {
                    self.refuse(
                        ErrorScope::Operation,
                        format!("operation not deleted: {err}"),
                    );
                }
            },
            CommandKind::GenerateOperation(name) => {
                // Generation happens in the constructor (a refusal writes
                // nothing); the status line reports what was produced.
                match GenerateOperation::new(&self.world, &name) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        let cache = model::find_by_name(&self.world, &name)
                            .and_then(|e| model::toolpath_of(&self.world, &e));
                        // The feed in the summary reads in the document's
                        // unit (M1.1); the path itself is mm law, always.
                        let unit = self.units();
                        self.status = match cache {
                            Some(tp) => tp.path.status_summary(&name, tp.warnings.len(), unit),
                            None => format!("generated {name}"),
                        };
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Operation, format!("generate failed: {err}"))
                    }
                }
            }
            CommandKind::RegenerateAll => {
                // Construction decides everything (validate early: each
                // stale op's generator runs before anything is written),
                // so an empty batch touches no history at all.
                let command = RegenerateAll::new(&self.world);
                let skipped: Vec<String> = command
                    .skipped()
                    .iter()
                    .map(|(name, err)| format!("{name}: {err}"))
                    .collect();
                if command.is_empty() {
                    match command.skipped().first() {
                        Some((name, err)) => self.refuse(
                            ErrorScope::Operation,
                            format!("nothing regenerated - {name}: {err}"),
                        ),
                        None => self.status = "nothing is stale".to_owned(),
                    }
                } else {
                    let n = command.regenerated();
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = if skipped.is_empty() {
                        format!("regenerated {n} operation(s)")
                    } else {
                        format!(
                            "regenerated {n} operation(s); {} skipped - {}",
                            skipped.len(),
                            skipped.join("; ")
                        )
                    };
                }
            }
            CommandKind::CreateSetup { name, datum_from } => {
                match CreateSetup::new(&self.world, name.as_deref(), datum_from.as_deref()) {
                    Ok(command) => {
                        let made = command.name().to_owned();
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("setup {made} created and active");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("setup not created: {err}"));
                    }
                }
            }
            CommandKind::DeleteSetup(name) => match DeleteSetup::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("deleted setup {name}");
                }
                Err(err) => {
                    self.refuse(ErrorScope::Setup, format!("setup not deleted: {err}"));
                }
            },
            CommandKind::ActivateSetup(name) => {
                match ActivateSetup::new(&self.world, name.as_deref()) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = match &name {
                            Some(n) => format!("setup {n} active"),
                            None => "active-setup cursor cleared".to_owned(),
                        };
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("setup not activated: {err}"));
                    }
                }
            }
            CommandKind::SetOperationSetup { name, setup } => {
                match SetOperationSetup::new(&self.world, &name, setup.as_deref()) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!(
                            "moved {name} to {}",
                            setup.as_deref().map_or_else(
                                || "the first setup".to_owned(),
                                |s| format!("'{s}'"),
                            )
                        );
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Operation, format!("setup not moved: {err}"));
                    }
                }
            }
            CommandKind::TagFixture { name, setup } => {
                match TagFixture::new(&self.world, &name, setup.as_deref()) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = match &setup {
                            Some(s) => format!("marked {name} as {s}'s fixture"),
                            None => format!("marked {name} as a fixture"),
                        };
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("fixture not marked: {err}"));
                    }
                }
            }
            CommandKind::UntagFixture(name) => match UntagFixture::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("unmarked {name} - the solid is a plain model again");
                }
                Err(err) => {
                    self.refuse(ErrorScope::Setup, format!("fixture not unmarked: {err}"));
                }
            },
            CommandKind::MoveFixture { name, pos, yaw_deg } => {
                let location = Location {
                    pos: Point3::new(pos[0], pos[1], pos[2]),
                    rot: quat_from_yaw(yaw_deg.to_radians()),
                };
                match MoveFixture::new(&self.world, &name, location) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.status = format!("moved fixture {name}");
                    }
                    Err(err) => {
                        self.refuse(ErrorScope::Setup, format!("fixture not moved: {err}"));
                    }
                }
            }
            CommandKind::DeleteFixture(name) => match DeleteFixture::new(&self.world, &name) {
                Ok(command) => {
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = format!("deleted fixture {name}");
                }
                Err(err) => {
                    self.refuse(ErrorScope::Setup, format!("fixture not deleted: {err}"));
                }
            },
            CommandKind::MoveModel {
                name,
                pos,
                roll_deg,
                pitch_deg,
                yaw_deg,
            } => {
                // The `SetWcs` guard: the axis-angle builders quietly
                // substitute identity for non-finite angles, so refuse
                // here rather than silently drop a NaN rotation.
                if !roll_deg.is_finite() || !pitch_deg.is_finite() || !yaw_deg.is_finite() {
                    self.refuse(
                        ErrorScope::Setup,
                        "part not moved: rotation angles must be finite".to_string(),
                    );
                    return;
                }
                let location = Location {
                    pos: Point3::new(pos[0], pos[1], pos[2]),
                    rot: quat_from_euler(
                        roll_deg.to_radians(),
                        pitch_deg.to_radians(),
                        yaw_deg.to_radians(),
                    ),
                };
                match MoveModel::new(&self.world, &name, location) {
                    Ok(command) => {
                        self.history.execute(&self.world, command);
                        self.doc_version += 1;
                        self.model_ui = ModelUi {
                            name: name.clone(),
                            x: pos[0],
                            y: pos[1],
                            z: pos[2],
                            roll_deg,
                            pitch_deg,
                            yaw_deg,
                        };
                        self.status = format!("moved {name}");
                    }
                    Err(err) => self.refuse(ErrorScope::Setup, format!("part not moved: {err}")),
                }
            }
            CommandKind::ImportFixtureDialog => {
                if let Some(path) = pick_mesh_file() {
                    // Bind to the context the page shows: the active
                    // setup, or the implicit-first `None` of a document
                    // with no setups at all.
                    self.import_as_fixture = Some(camcraft_app::setup::active(&self.world));
                    self.import_path(path);
                }
            }
            CommandKind::SetupSheetDialog => {
                if let Some(path) = save_sheet_dialog() {
                    self.setup_sheet_path(path);
                }
            }
            CommandKind::SetupSheetFile(path) => self.setup_sheet_path(path),
        }
    }

    #[must_use]
    pub fn has_selection(&self) -> bool {
        !model::selected_names(&self.world).is_empty()
    }

    /// Is there anything to simulate: an operation carrying a cached
    /// path. The walk mirrors the post gate's exactly
    /// (`posting::stream_records`: `named_entities` filtered by
    /// `is_operation`) - and display-hidden paths are included, by
    /// the same law that keeps them posted: hidden is display, not
    /// suppression, and the simulation rehearses the machine's
    /// program, not the picture.
    #[must_use]
    pub fn has_toolpaths(&self) -> bool {
        model::named_entities(&self.world)
            .into_iter()
            .filter(|(_, e)| model::is_operation(&self.world, e))
            .any(|(_, e)| model::has_toolpath(&self.world, &e))
    }

    /// Center the orbit camera on the document's bounding sphere.
    fn frame_all(&mut self) {
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        let mut any = false;
        for (_, entity) in model::named_entities(&self.world) {
            let Some(tris) = model::facets_of(&self.world, &entity) else {
                continue;
            };
            let loc = model::transform_of(&self.world, &entity).unwrap_or_default();
            for t in tris.iter() {
                for p in [t.a, t.b, t.c] {
                    let p = transform_point(p, loc.pos, loc.rot);
                    for axis in 0..3 {
                        let v = [p.x, p.y, p.z][axis];
                        if v.is_finite() {
                            min[axis] = min[axis].min(v);
                            max[axis] = max[axis].max(v);
                            any = true;
                        }
                    }
                }
            }
        }
        // A work offset parked far from the parts is still "all": frame
        // their origins even though they carry no geometry.
        for (_, entity) in model::named_entities(&self.world) {
            if !model::is_wcs(&self.world, &entity) {
                continue;
            }
            let Some(loc) = model::transform_of(&self.world, &entity) else {
                continue;
            };
            for axis in 0..3 {
                let v = [loc.pos.x, loc.pos.y, loc.pos.z][axis];
                if v.is_finite() {
                    min[axis] = min[axis].min(v);
                    max[axis] = max[axis].max(v);
                    any = true;
                }
            }
        }
        if !any {
            self.cam = Camera::default();
            self.status = String::from("nothing to frame");
            return;
        }
        let center = std::array::from_fn(|i| (min[i] + max[i]) * 0.5);
        let half: [f64; 3] = std::array::from_fn(|i| (max[i] - min[i]) * 0.5);
        let radius = half
            .iter()
            .fold(0.0f64, |acc, r| acc.max(*r))
            .max(f64::EPSILON)
            * 3.0f64.sqrt();
        self.cam = Camera::frame(center, radius);
    }

    fn import_path(&mut self, path: PathBuf) {
        // Parsing happens at command construction, like everything else -
        // it just happens to be off-thread now, so a heavy STEP tessellates
        // while the window keeps painting.
        if self.import_rx.is_some() {
            self.status = format!("queued {} for import", file_label(&path));
            self.import_queue.push_back(path);
            return;
        }
        self.begin_import(path);
    }

    fn begin_import(&mut self, path: PathBuf) {
        self.status = format!("importing {}…", file_label(&path));
        self.import_file = Some(path.clone());
        self.import_started = Some(Instant::now());
        let (tx, rx) = mpsc::channel();
        let worker_path = path.clone();
        let spawned = std::thread::Builder::new()
            .name("camcraft-import".to_string())
            .spawn(move || {
                // No World here: pure file + geometry work, Send across.
                let result = ImportModel::new(&worker_path);
                // A dropped rx means the document changed or we are closing;
                // the command is simply never applied.
                if tx.send(result).is_err() {
                    eprintln!("import: result discarded, no receiver");
                }
            });
        match spawned {
            Ok(_) => self.import_rx = Some(rx),
            Err(err) => {
                // Thread exhaustion must not lose the import: run inline.
                eprintln!("import: worker spawn failed ({err}); running inline");
                let result = ImportModel::new(&path);
                self.finish_import(result);
            }
        }
    }

    /// Called on this thread when a worker result arrives (or inline
    /// fallback completes). Execution stays a normal history command.
    fn finish_import(&mut self, result: Result<ImportModel, CommandError>) {
        self.import_file = None;
        let secs = self
            .import_started
            .take()
            .map_or(0.0, |t| t.elapsed().as_secs_f64());
        match result {
            Ok(command) => {
                let name = command.name.clone();
                let timed = |verb: &str, note: &str| {
                    if secs >= 2.0 {
                        format!("{verb} {name}{note} in {secs:.1}s")
                    } else {
                        format!("{verb} {name}{note}")
                    }
                };
                // M3.1: a name that already carries a model is an UPDATE,
                // not a silently-dropped re-import. The decision lives here,
                // on the main thread, because only the world can say whether
                // the name exists; the worker stayed world-blind on purpose.
                // UpdateModel::new classifies further: setup furniture
                // under this name is refused as Invalid, never overwritten.
                if model::find_by_name(&self.world, &name).is_some() {
                    match UpdateModel::new(&self.world, &name, command.mesh) {
                        Ok(upd) => {
                            let summary = upd.changes().describe();
                            self.history.execute(&self.world, upd);
                            self.doc_version += 1;
                            self.status = timed("updated", &format!(" ({summary})"));
                        }
                        Err(err) => self.status = format!("import refused: {err}"),
                    }
                } else {
                    // Fresh model: the spawn path, unchanged.
                    self.history.execute(&self.world, command);
                    self.doc_version += 1;
                    self.status = timed("imported", "");
                }
                // Stage B: an import-as-fixture tags whatever landed.
                // Two undo units (the import, then the marker) is the
                // honest shape - each is invertible on its own, and the
                // worker never needed a world to begin with.
                if let Some(binding) = self.import_as_fixture.take() {
                    match TagFixture::new(&self.world, &name, binding.as_deref()) {
                        Ok(tag) => {
                            self.history.execute(&self.world, tag);
                            self.doc_version += 1;
                            let note = match &binding {
                                Some(s) => format!("; marked as {s}'s fixture"),
                                None => "; marked as a fixture".to_string(),
                            };
                            self.status.push_str(&note);
                        }
                        Err(err) => self.refuse(
                            ErrorScope::Setup,
                            format!("imported, but not marked as a fixture: {err}"),
                        ),
                    }
                }
            }
            Err(err) => self.status = format!("import refused: {err}"),
        }
    }

    /// Non-blocking worker check; call once per frame. Also starts the next
    /// queued import when the worker is free.
    fn poll_import(&mut self) {
        // take() first so no borrow of self straddles the mutation arms.
        let outcome = match &self.import_rx {
            Some(rx) => match rx.try_recv() {
                Ok(result) => Some(Ok(result)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(())),
            },
            None => None,
        };
        match outcome {
            Some(Ok(result)) => {
                self.import_rx = None;
                self.finish_import(result);
            }
            Some(Err(())) => {
                // Thread died without sending (a panic in the parser): the
                // Prime Directive says surface it, never hang waiting.
                self.import_rx = None;
                self.import_file = None;
                self.import_started = None;
                self.status = String::from("import failed: worker lost");
                eprintln!("import: worker thread exited without a result");
            }
            None => {}
        }
        // Still running: keep the elapsed readout honest in the status bar.
        if self.import_rx.is_some()
            && let Some(t0) = self.import_started
        {
            let secs = t0.elapsed().as_secs_f64();
            if secs >= 1.0 {
                let label = self
                    .import_file
                    .as_deref()
                    .map_or_else(|| "model".to_string(), file_label);
                self.status = format!("importing {label}… {secs:.0}s");
            }
        }
        if self.import_rx.is_none()
            && let Some(next) = self.import_queue.pop_front()
        {
            self.begin_import(next);
        }
    }

    /// Drop any live/queued imports because the document is being replaced.
    /// A worker still running finishes in the background and discards itself.
    fn abandon_imports(&mut self) {
        if self.import_rx.take().is_some() {
            eprintln!("import: document changed; pending import discarded");
        }
        self.import_queue.clear();
        self.import_file = None;
        self.import_started = None;
        self.import_as_fixture = None;
    }

    /// Mount a `.mch` machine definition on the document (M2.3 C). The
    /// parse lives in `LoadMachine::from_file` - command-construction
    /// validation, so a bad file is a visible refusal, never a
    /// half-written setup.
    fn load_machine_path(&mut self, path: PathBuf) {
        match LoadMachine::from_file(&self.world, &path) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                let summary = machine::of(&self.world)
                    .1
                    .map(|m| m.summary())
                    .unwrap_or_default();
                self.status = format!("machine: {summary}");
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("machine not loaded: {err}")),
        }
    }

    /// Choose the post script (M3.3 A). The validation - read and
    /// compile - lives in `SelectPost::new`, command-construction time,
    /// so a script that cannot run is a visible refusal at the pick,
    /// never a surprise at the post.
    fn select_post(&mut self, next: PostRef) {
        match SelectPost::new(&self.world, next) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = format!("post: {}", posting::post_label(&self.world));
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("post not selected: {err}")),
        }
    }

    fn open_project_path(&mut self, path: PathBuf) {
        self.abandon_imports();
        model::clear_named(&self.world);
        self.history.clear();
        self.post_view = None; // the old document's program is not this one's
        self.program_sim = None; // and neither is its stock verdict
        self.machine_editor = None; // nor a draft over its machine's numbers
        match project::load(&self.world, &path) {
            Ok(()) => {
                self.doc_version += 1;
                self.status = format!("opened {}", path.display());
            }
            Err(err) => self.status = format!("open failed: {err}"),
        }
    }

    /// The file law's third effect (M1.7b): post, or be refused by name.
    /// Like save/open, the outcome is a status line, not a pane error -
    /// and the gate writes nothing on refusal (posting's own law), so a
    /// refused post leaves both the document and any previous program on
    /// disk exactly as they were.
    fn post_program_path(&mut self, path: PathBuf) {
        match posting::post_setup(&self.world, &path) {
            Ok(posted) => {
                self.status = format!(
                    "posted {} op(s), {} lines to {}",
                    posted.ops,
                    posted.lines,
                    path.display()
                );
                // The viewer shows what the gate just wrote - text and
                // positions from the same engine run, so highlighting a
                // line highlights the move that line posts.
                self.post_view = Some(PostView {
                    path,
                    program: posted.program,
                    line_pos: posted.line_pos,
                    open: true,
                    line: 0,
                    blocks: posted.blocks,
                    trace: false,
                });
            }
            // A refused post leaves any previous program (file and viewer)
            // exactly as it was - the gate writes nothing on refusal.
            Err(err) => self.status = format!("post refused: {err}"),
        }
    }

    /// Setup sheets (M3.2 stage D): write every setup's sheet as one
    /// text file, form feeds between pages so any print path paginates
    /// the same way the screen does. A pure read like posting - the
    /// sheet is derived from the document, nothing about it goes in
    /// the history, and the status line is the only effect.
    fn setup_sheet_path(&mut self, path: PathBuf) {
        let sheets = camcraft_app::setup_sheet::render_all(&self.world);
        let mut out = String::new();
        for (i, (_title, sheet)) in sheets.iter().enumerate() {
            if i > 0 {
                out.push('\u{0c}');
                out.push('\n');
            }
            out.push_str(sheet);
        }
        match std::fs::write(&path, out) {
            Ok(()) => {
                self.status = format!(
                    "{} setup sheet(s) written to {}",
                    sheets.len(),
                    path.display()
                )
            }
            Err(err) => self.status = format!("sheet write failed: {err}"),
        }
    }
}

/// Short human name for a file path, for status text.
fn file_label(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model")
        .to_string()
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

fn pick_mesh_file() -> Option<PathBuf> {
    // STEP arrives tessellated through truck (mesh, not B-rep).
    rfd::FileDialog::new()
        .add_filter("Mesh", &["stl", "obj", "step", "stp"])
        .pick_file()
}

fn pick_project_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("CamCraft Project", &["camproj"])
        .pick_file()
}

fn pick_machine_file() -> Option<PathBuf> {
    // The `.mch` layout is the machine-definition JSON format we reuse
    // by user instruction (M2.3 C); the filter just names it.
    rfd::FileDialog::new()
        .add_filter("Machine Definition", &["mch"])
        .pick_file()
}

fn pick_post_file() -> Option<PathBuf> {
    // The `.cps` script format is the one we reuse by user instruction
    // (M3.3 A) - plain JavaScript, the user's own; the filter names it.
    rfd::FileDialog::new()
        .add_filter("Post Script", &["cps"])
        .pick_file()
}

fn save_project_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("CamCraft Project", &["camproj"])
        .set_file_name("untitled.camproj")
        .save_file()
}

fn post_program_dialog() -> Option<PathBuf> {
    // The extension law lives with the script (fanuc.cps's `extension`
    // is "nc"); the dialog just offers it.
    rfd::FileDialog::new()
        .add_filter("NC Program", &["nc"])
        .set_file_name("program.nc")
        .save_file()
}

fn save_sheet_dialog() -> Option<PathBuf> {
    // Plain text on purpose: the sheet is meant to be read on paper
    // next to the machine, so the format is whatever Notepad opens.
    rfd::FileDialog::new()
        .add_filter("Text", &["txt"])
        .set_file_name("setup-sheets.txt")
        .save_file()
}
