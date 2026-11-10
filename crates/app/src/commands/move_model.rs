//! MoveModel: place a part in the document - position plus any rigid
//! orientation (M5.2 stage G, "part orientation"). The full rotation the
//! ROADMAP deferred from M3.2 ("placement flip; full rotation M5.2") is
//! exactly this command's extra two angles: the data model always
//! carried a quaternion and every geometry read applies it, so the
//! rotation became an editor change, not a schema change.
//!
//! Provenance: our own design; a workpiece sitting in the machine at an
//! arbitrary orientation (trunnied, tilted on a sine plate) is plain
//! shop practice, and R = Rz ⊗ Ry ⊗ Rx is the standard yaw-pitch-roll
//! composition `SetWcsOrigin` already speaks.
//!
//! ## The laws this command keeps
//!
//! - **Placement is a Location, validated at construction.** Same
//!   chokepoint as every pose write (`check_location`, shared from the
//!   fixture module so there is one law): a finite position and a
//!   quaternion that normalizes, or the command never enters history.
//! - **The model's `Location` is the single truth for its geometry.**
//!   The same rigid placement change carries stock made from that model,
//!   or the stock of a document with one part. Existing operation stamps
//!   read the changed placements and become stale.
//! - **Pages own their verbs.** A fixture is placed on the Fixtures
//!   page (`MoveFixture`), a work offset on the Work Offsets page
//!   (`SetWcsOrigin`), and the machine table is derived prose - all
//!   refused by name, because silently accepting them here would put
//!   two commands on one piece of furniture and split the undo story.
//! - **`apply` and `invert` are pure writes**, so redo replays bytes
//!   and never re-derives.

use flecs_ecs::prelude::World;

use crate::commands::fixture::check_location;
use crate::commands::{Command, CommandError};
use crate::fixture::MACHINE_TABLE;
use crate::geometry::{quat_conjugate, quat_mul, transform_inverse_point, transform_point};
use crate::model;
use crate::schema::Location;
use crate::setup_align;
use crate::stock::StockShape;

/// The part alignment of the setup that names `stock` as its stock, when
/// that setup holds the part turned (#326).
fn aligned_owner(world: &World, stock: &str) -> Option<Location> {
    crate::setup::names(world)
        .into_iter()
        .filter(|s| crate::setup::def_of(world, s).is_some_and(|d| d.stock == stock))
        .find_map(|s| setup_align::stored(world, &s))
}

/// Place a model: the same rigid transform any page sets, applied to
/// the named part. Construction snapshots the previous `Location` for
/// the inverse.
pub struct MoveModel {
    name: String,
    prev: Location,
    next: Location,
    stock: Vec<(String, Location, Location)>,
}

impl MoveModel {
    pub fn new(world: &World, name: &str, location: Location) -> Result<Self, CommandError> {
        if name == MACHINE_TABLE {
            return Err(CommandError::Invalid(
                "the machine table is derived from the machine file - it is not a part you can move".into(),
            ));
        }
        let entity =
            model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.into()))?;
        if model::is_fixture(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is a fixture - the Fixtures page places it"
            )));
        }
        if model::is_wcs(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is a work offset - the Work Offsets page places it"
            )));
        }
        let next = check_location(location, &format!("'{name}'"))?;
        let prev = model::transform_of(world, &entity).unwrap_or_default();
        let single_part = model::model_entities(world)
            .iter()
            .filter(|(_, entity)| !model::is_fixture(world, entity))
            .count()
            == 1;
        let stock = model::named_entities(world)
            .into_iter()
            .filter_map(|(stock_name, entity)| {
                let shape = model::stock_def_of(world, &entity)?;
                let follows = match shape {
                    StockShape::FromModel { source } => source == name,
                    _ => single_part,
                };
                follows.then(|| {
                    let before = model::transform_of(world, &entity).unwrap_or_default();
                    // #326: a stock placed in a setup that holds the part
                    // turned follows the move as that setup sees it - the
                    // document move conjugated by the setup's alignment.
                    if let Some(a) = aligned_owner(world, &stock_name) {
                        let delta = setup_align::compose(next, setup_align::inverse(prev));
                        let seen = setup_align::compose(a, setup_align::compose(delta, setup_align::inverse(a)));
                        return (stock_name, before, setup_align::compose(seen, before));
                    }
                    let local = transform_inverse_point(before.pos, prev.pos, prev.rot);
                    let after = Location {
                        pos: transform_point(local, next.pos, next.rot),
                        rot: quat_mul(quat_mul(next.rot, quat_conjugate(prev.rot)), before.rot),
                    };
                    (stock_name, before, after)
                })
            })
            .map(|(stock_name, before, after)| {
                check_location(after, &format!("stock '{stock_name}'"))
                    .map(|after| (stock_name, before, after))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(MoveModel {
            name: name.to_string(),
            prev,
            next,
            stock,
        })
    }

    /// The stock this move carries with the part (stock made from it, or
    /// the stock of a one-part document), by name. The MCP `move_model`
    /// tool (#716) reports them, because a part move that also moves the
    /// stock is easy to miss from the part's own pose.
    pub fn following_stock(&self) -> impl Iterator<Item = &str> {
        self.stock.iter().map(|(name, _, _)| name.as_str())
    }
}

impl Command for MoveModel {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::set_transform(world, &entity, self.next);
        }
        for (name, _, next) in &self.stock {
            if let Some(entity) = model::find_by_name(world, name) {
                model::set_transform(world, &entity, *next);
            }
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::set_transform(world, &entity, self.prev);
        }
        for (name, prev, _) in &self.stock {
            if let Some(entity) = model::find_by_name(world, name) {
                model::set_transform(world, &entity, *prev);
            }
        }
    }
}
