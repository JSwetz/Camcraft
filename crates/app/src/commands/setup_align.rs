//! `AlignPartToSetup` (#326): set how the part sits in one setup - the
//! rigid transform from the document frame into the setup frame - or
//! clear it back to "as modelled". The work offset is not touched: it is
//! a plain translation placed in the setup frame (`SetSetupWorkOffset`,
//! `SetWcsOrigin`).
//!
//! Provenance: our own design; re-clamping a part flipped or turned on
//! the table is general shop practice, and the transform math is
//! standard rigid-body algebra. No vendor implementation inspected.
//!
//! ## The laws this command keeps
//!
//! - **Validated and snapshotted at construction.** A finite position
//!   and a quaternion that normalizes (`check_location`), a live setup,
//!   a change from what is stored. `apply`/`invert` are pure writes.
//! - **Identity is absence.** An alignment that does nothing is stored
//!   as `None`, so "As modelled" restores the pre-#326 bytes.
//! - **The stock follows like it follows `MoveModel`.** A stock made
//!   from a part, or the stock of a one-part document, is re-placed by
//!   the change and rebuilt square to the setup frame (a box or a
//!   cylinder re-fits to the turned block's bounds). A cylinder whose
//!   axis would leave the vertical, or a following stock another setup
//!   shares, is refused by name - never silently left behind.
//! - **The presets pivot on the parts' centre.** A preset turns the
//!   parts about the centre of their bounds, so a flip leaves them on the
//!   same spot of the table. Typed angles ([`AlignPartToSetup::turn`],
//!   #606) pivot on the setup's WCS origin by default - the part keeps
//!   its relation to the origin - or on the centre, by choice. [`AlignPartToSetup::new`]
//!   takes any rigid transform - the door the face pick (#330) uses,
//!   through [`AlignPartToSetup::lay_on_plane`], three points (#555),
//!   through [`AlignPartToSetup::lay_on_points`], and the in-plane axis
//!   picks (#557), through [`AlignPartToSetup::turn_in_plane`].
//! - **Linked setups follow (#316).** Every setup placed relative to
//!   this one (directly or through others, [`crate::setup_relative`])
//!   keeps its transform to it: the command re-places each of them, with
//!   its following stock, in the same undo unit - or refuses the whole
//!   change, naming the setup that cannot follow.

use flecs_ecs::prelude::World;

use crate::commands::fixture::check_location;
use crate::commands::{Command, CommandError};
use crate::geometry::{Point3, Triangle, transform_point};
use crate::model;
use crate::schema::{Location, SetupDef, StockDef};
use crate::setup_align::{self, AlignPreset};
use crate::setup_turn::TurnPivot;
use crate::stock::{self, StockShape};

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)

type StockState = (StockShape, Location, std::sync::Arc<[Triangle]>);

/// The setup's (held, new) alignment for `alignment`, normalised (the
/// identity is "as modelled"); refused when nothing would change.
fn change_of(
    world: &World,
    setup: &str,
    alignment: Option<Location>,
) -> Result<(Option<Location>, Option<Location>), CommandError> {
    let entity = model::find_by_name(world, setup).ok_or_else(|| CommandError::NotFound(setup.to_string()))?;
    let prev = entity
        .entity_view(world)
        .try_get::<&SetupDef>(|d| d.part_alignment)
        .ok_or_else(|| CommandError::Invalid(format!("'{setup}' is not a setup")))?;
    let next = match alignment {
        None => None,
        Some(a) => {
            let a = check_location(a, "the part alignment")?;
            let still = a.pos == Point3::ZERO && setup_align::is_identity_rot(a.rot);
            (!still).then_some(a)
        }
    };
    if next == prev {
        return Err(CommandError::Invalid(format!("setup '{setup}' already holds the part that way")));
    }
    Ok((prev, next))
}

/// One setup's alignment change, with its following stock.
struct Step {
    setup: String,
    prev: Option<Location>,
    next: Option<Location>,
    /// The following stock: entity name, before, after.
    stock: Option<(String, StockState, StockState)>,
}

/// Write (or clear) a setup's part alignment.
pub struct AlignPartToSetup {
    main: Step,
    /// #316: the setups linked below this one, re-placed with it (parents
    /// before children).
    carried: Vec<Step>,
}

impl AlignPartToSetup {
    /// Any rigid transform document -> setup frame; `None` clears.
    pub fn new(world: &World, setup: &str, alignment: Option<Location>) -> Result<Self, CommandError> {
        Self::build(world, setup, alignment, false)
    }

    /// The alignment a locate writes (#561): this setup's square box or
    /// cylinder stock keeps its allowance on each side round the part
    /// (`crate::stock_follow`) instead of re-fitting the turned block.
    pub fn locating(world: &World, setup: &str, alignment: Option<Location>) -> Result<Self, CommandError> {
        Self::build(world, setup, alignment, true)
    }

    fn build(world: &World, setup: &str, alignment: Option<Location>, square: bool) -> Result<Self, CommandError> {
        let (prev, next) = change_of(world, setup, alignment)?;
        let stock = following_stock(world, setup, prev, next, square)?;
        let mut carried = Vec::new();
        for (child, c_prev, c_next) in crate::setup_relative::carried(world, setup, prev, next) {
            let follow = |e: CommandError| {
                let why = match e {
                    CommandError::Invalid(m) => m,
                    other => other.to_string(),
                };
                CommandError::Invalid(format!(
                    "setup '{child}' is placed relative to '{setup}' and must follow the change, but {why}"
                ))
            };
            let c_next = c_next.map(|a| check_location(a, "the carried part alignment")).transpose().map_err(follow)?;
            let stock = following_stock(world, &child, c_prev, c_next, false).map_err(follow)?;
            carried.push(Step { setup: child, prev: c_prev, next: c_next, stock });
        }
        Ok(AlignPartToSetup { main: Step { setup: setup.to_string(), prev, next, stock }, carried })
    }

    /// The main setup's following stock where the change leaves it
    /// (setup frame) - what a locate's interpenetration check reads.
    #[must_use]
    pub fn moved_stock(&self) -> Vec<(String, Vec<Triangle>)> {
        self.main
            .stock
            .iter()
            .map(|(name, _, (_, at, mesh))| {
                let tris = mesh
                    .iter()
                    .map(|t| Triangle::new(
                        transform_point(t.a, at.pos, at.rot),
                        transform_point(t.b, at.pos, at.rot),
                        transform_point(t.c, at.pos, at.rot),
                    ))
                    .collect();
                (name.clone(), tris)
            })
            .collect()
    }

    /// How many linked setups this change re-places with it (#316).
    #[must_use]
    pub fn carried_count(&self) -> usize {
        self.carried.len()
    }

    /// Would [`Self::lay_on_plane`] be accepted? The same refusals, by
    /// name ("already holds the part that way" included), without
    /// building the command: the Setups page greys Apply out with it.
    pub fn check_lay(
        world: &World,
        setup: &str,
        picks: &[crate::operation::GeoSel],
        lay: crate::pick::PlaneLay,
    ) -> Result<(), CommandError> {
        let at = crate::pick::lay_on_plane(world, picks, lay).map_err(CommandError::Invalid)?;
        change_of(world, setup, Some(at)).map(|_| ())
    }

    /// Lay the part on a G17 / G18 / G19 plane from a pick (#330): one
    /// flat face, read by [`crate::pick::lay_on_plane`] (an edge is
    /// refused by name, #554).
    /// A refused pick is refused here, by name, before anything is written.
    pub fn lay_on_plane(
        world: &World,
        setup: &str,
        picks: &[crate::operation::GeoSel],
        lay: crate::pick::PlaneLay,
    ) -> Result<Self, CommandError> {
        let at = crate::pick::lay_on_plane(world, picks, lay).map_err(CommandError::Invalid)?;
        Self::new(world, setup, Some(at))
    }

    /// Would [`Self::lay_on_points`] be accepted? The same refusals, by
    /// name, without building the command (the Setups page's Apply).
    pub fn check_points(
        world: &World,
        setup: &str,
        picks: &[crate::operation::GeoSel],
        pl: crate::pick::PointsLay,
    ) -> Result<(), CommandError> {
        let at = crate::pick::lay_on_points(world, picks, pl).map_err(CommandError::Invalid)?;
        change_of(world, setup, Some(at)).map(|_| ())
    }

    /// Lay the part on a G17 / G18 / G19 plane through three picked
    /// points (#555), read by [`crate::pick::lay_on_points`] through the
    /// face pick's own apply path; with `frame`, point 1 is the origin and
    /// 1 -> 2 the plane's first axis (3-2-1). Refused by name before any
    /// write.
    pub fn lay_on_points(
        world: &World,
        setup: &str,
        picks: &[crate::operation::GeoSel],
        pl: crate::pick::PointsLay,
    ) -> Result<Self, CommandError> {
        let at = crate::pick::lay_on_points(world, picks, pl).map_err(CommandError::Invalid)?;
        Self::new(world, setup, Some(at))
    }

    /// Would [`Self::turn_in_plane`] be accepted? The same refusals, by
    /// name, without building the command (the Setups page's Apply).
    pub fn check_turn(world: &World, setup: &str, axes: &crate::pick::InPlaneAxes) -> Result<(), CommandError> {
        let at = crate::pick::turn_in_plane(world, setup, axes).map_err(CommandError::Invalid)?;
        change_of(world, setup, Some(at)).map(|_| ())
    }

    /// Turn the part in the work plane from picks (#557): a straight edge
    /// or two points for +Y and/or +X, read by
    /// [`crate::pick::turn_in_plane`] in the setup's current frame and
    /// composed onto its alignment. Refused by name before any write.
    pub fn turn_in_plane(world: &World, setup: &str, axes: &crate::pick::InPlaneAxes) -> Result<Self, CommandError> {
        let at = crate::pick::turn_in_plane(world, setup, axes).map_err(CommandError::Invalid)?;
        Self::new(world, setup, Some(at))
    }

    /// One of the Setups page's quick choices.
    pub fn preset(world: &World, setup: &str, preset: AlignPreset) -> Result<Self, CommandError> {
        Self::angles(world, setup, preset.angles())
    }

    /// Turn the parts about X, then Y, then Z (degrees) about the centre
    /// of their bounds (the presets' pivot). All zero is "as modelled".
    pub fn angles(world: &World, setup: &str, deg: [f64; 3]) -> Result<Self, CommandError> {
        Self::turn(world, setup, deg, TurnPivot::PartCentre)
    }

    /// Turn the parts to X, then Y, then Z (degrees, from as modelled)
    /// about `pivot` (#606): the WCS origin keeps the part's point on the
    /// origin there; the part centre keeps the bounds centre put. All zero
    /// is "as modelled".
    pub fn turn(world: &World, setup: &str, deg: [f64; 3], pivot: TurnPivot) -> Result<Self, CommandError> {
        let rot = setup_align::rotation_from_angles(deg)
            .ok_or_else(|| CommandError::Invalid("the alignment angles must be finite numbers".into()))?;
        let at = crate::setup_turn::turned(world, setup, rot, pivot).map_err(CommandError::Invalid)?;
        Self::new(world, setup, at)
    }
}

impl Step {
    fn write(&self, world: &World, alignment: Option<Location>, stock: Option<&StockState>) {
        if let Some(entity) = model::find_by_name(world, &self.setup) {
            let view = entity.entity_view(world);
            if let Some(mut def) = view.try_get::<&SetupDef>(|d| d.clone()) {
                def.part_alignment = alignment;
                view.set(def);
            }
        }
        if let (Some((name, before, after)), Some((shape, loc, facets))) = (&self.stock, stock)
            && let Some(entity) = model::find_by_name(world, name)
        {
            model::set_transform(world, &entity, *loc);
            // Re-mesh only a block that changed size: a flip that keeps
            // the box moves it, and its geometry revision stays put.
            if before.0 != after.0 {
                model::set_facets(world, &entity, facets.clone());
                entity.entity_view(world).set(StockDef { shape: shape.clone() });
            }
        }
    }
}

impl Command for AlignPartToSetup {
    fn apply(&self, world: &World) {
        for step in std::iter::once(&self.main).chain(&self.carried) {
            step.write(world, step.next, step.stock.as_ref().map(|(_, _, after)| after));
        }
    }

    fn invert(&self, world: &World) {
        for step in self.carried.iter().rev().chain(std::iter::once(&self.main)) {
            step.write(world, step.prev, step.stock.as_ref().map(|(_, before, _)| before));
        }
    }
}

/// The setup's stock as it must stand after the alignment changes from
/// `prev` to `next`, when it follows the part; `None` when it does not
/// follow (or there is no stock yet).
fn following_stock(
    world: &World,
    setup: &str,
    prev: Option<Location>,
    next: Option<Location>,
    square: bool,
) -> Result<Option<(String, StockState, StockState)>, CommandError> {
    let name = crate::setup::stock_entity_name(world, Some(setup));
    let Some(entity) = model::find_by_name(world, &name) else { return Ok(None) };
    let Some(shape) = model::stock_def_of(world, &entity) else { return Ok(None) };
    let parts = model::model_entities(world)
        .into_iter()
        .filter(|(_, e)| !model::is_fixture(world, e))
        .map(|(n, _)| n)
        .collect::<Vec<_>>();
    let follows = match &shape {
        StockShape::FromModel { source } => parts.contains(source),
        _ => parts.len() == 1,
    };
    if !follows {
        return Ok(None);
    }
    if crate::setup::is_furniture_of_other(world, &name, setup) {
        return Err(CommandError::Invalid(format!(
            "the stock '{name}' is shared with another setup - turning the part here would move it there too; \
             give this setup its own stock first"
        )));
    }
    let loc = model::transform_of(world, &entity).unwrap_or_default();
    let facets = model::facets_of(world, &entity).unwrap_or_default();
    // The change as one transform: undo the old alignment, apply the new.
    let undo = prev.map_or_else(Location::default, setup_align::inverse);
    let delta = setup_align::compose(next.unwrap_or_default(), undo);
    if square && let Some(after) = square_refit(world, &name, &shape, (loc, &facets), &parts, (prev, next))? {
        return Ok(Some((name, (shape, loc, facets), after)));
    }
    let after = match &shape {
        StockShape::FromModel { .. } => {
            let moved = check_location(setup_align::compose(delta, loc), &format!("stock '{name}'"))?;
            (shape.clone(), moved, facets.clone())
        }
        StockShape::Box { dx, dy, dz } => {
            let corners = [-0.5, 0.5]
                .into_iter()
                .flat_map(|x| [-0.5, 0.5].map(|y| (x, y)))
                .flat_map(|(x, y)| [0.0, 1.0].map(|z| Point3::new(x * dx, y * dy, z * dz)));
            let moved: Vec<Point3> = corners.map(|p| place(delta, loc, p)).collect();
            refit_box(&name, &moved)?
        }
        StockShape::Cylinder { diameter, height } => {
            let bottom = place(delta, loc, Point3::ZERO);
            let top = place(delta, loc, Point3::new(0.0, 0.0, *height));
            let axis = Point3::new(top.x - bottom.x, top.y - bottom.y, top.z - bottom.z);
            // Written so a NaN reads as "not upright" and refuses.
            let upright = axis.z.abs() >= height * (1.0 - 1e-9);
            if !upright {
                return Err(CommandError::Invalid(format!(
                    "the cylinder stock '{name}' would lie on its side - only turns that keep it upright \
                     can carry it; set the stock again after aligning"
                )));
            }
            let pos = Point3::new(bottom.x, bottom.y, bottom.z.min(top.z));
            let shape = StockShape::Cylinder { diameter: *diameter, height: *height };
            let mesh = mesh_of(&shape, &name)?;
            (shape, check_location(Location { pos, ..Location::default() }, &format!("stock '{name}'"))?, mesh)
        }
    };
    Ok(Some((name, (shape, loc, facets), after)))
}

/// #561: the square block re-fitted round the one part with its
/// allowance kept (`None`: not a square parametric block - the turned
/// block is re-fitted as before).
fn square_refit(
    world: &World,
    name: &str,
    shape: &StockShape,
    (loc, facets): (Location, &std::sync::Arc<[Triangle]>),
    parts: &[String],
    (prev, next): (Option<Location>, Option<Location>),
) -> Result<Option<StockState>, CommandError> {
    let [part] = parts else { return Ok(None) };
    let Some(pe) = model::find_by_name(world, part) else { return Ok(None) };
    let Some(tris) = model::facets_of(world, &pe) else { return Ok(None) };
    let doc = model::transform_of(world, &pe).unwrap_or_default();
    let seen = |a: Option<Location>| {
        let at = a.map_or(doc, |a| setup_align::compose(a, doc));
        tris.iter().flat_map(|t| [t.a, t.b, t.c]).map(|p| transform_point(p, at.pos, at.rot)).collect::<Vec<_>>()
    };
    let fit = crate::stock_follow::keep_square(name, shape, loc, &seen(prev), &seen(next)).map_err(CommandError::Invalid)?;
    let Some((sized, at)) = fit else { return Ok(None) };
    let at = check_location(at, &format!("stock '{name}'"))?;
    let mesh = if &sized == shape { facets.clone() } else { mesh_of(&sized, name)? };
    Ok(Some((sized, at, mesh)))
}

/// A stock-local point through its placement, then through the change.
fn place(delta: Location, loc: Location, p: Point3) -> Point3 {
    let world_p = transform_point(p, loc.pos, loc.rot);
    transform_point(world_p, delta.pos, delta.rot)
}

/// The box square to the setup frame that holds the turned block.
fn refit_box(name: &str, pts: &[Point3]) -> Result<StockState, CommandError> {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in pts {
        for (i, v) in [p.x, p.y, p.z].into_iter().enumerate() {
            lo[i] = lo[i].min(v);
            hi[i] = hi[i].max(v);
        }
    }
    // A micron of float noise on a quarter turn is not a new size.
    let snap = |v: f64| (v * 1e6).round() / 1e6;
    let shape = StockShape::Box {
        dx: snap(hi[0] - lo[0]),
        dy: snap(hi[1] - lo[1]),
        dz: snap(hi[2] - lo[2]),
    };
    let mesh = mesh_of(&shape, name)?;
    let pos = Point3::new(snap(0.5 * (lo[0] + hi[0])), snap(0.5 * (lo[1] + hi[1])), snap(lo[2]));
    let at = check_location(Location { pos, ..Location::default() }, &format!("stock '{name}'"))?;
    Ok((shape, at, mesh))
}

fn mesh_of(shape: &StockShape, name: &str) -> Result<std::sync::Arc<[Triangle]>, CommandError> {
    if !shape.is_sane() || !shape.is_bounded() {
        return Err(CommandError::Invalid(format!("the stock '{name}' cannot be rebuilt in the turned frame")));
    }
    stock::stock_mesh(shape)
        .map(Into::into)
        .ok_or_else(|| CommandError::Invalid(format!("the stock '{name}' cannot be rebuilt in the turned frame")))
}
