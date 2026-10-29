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
//! - **The model's own `Location` is the single truth.** Everything
//!   downstream - facet transforms, the stock solid, the staleness
//!   digest that hashes this entity's `SrcRef::Entity` - already reads
//!   or hashes it, so moving a part marks its operations Stale with no
//!   cascade write here. The command writes one component, like
//!   `MoveFixture`.
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
use crate::model;
use crate::schema::Location;

/// Place a model: the same rigid transform any page sets, applied to
/// the named part. Construction snapshots the previous `Location` for
/// the inverse.
pub struct MoveModel {
    name: String,
    prev: Location,
    next: Location,
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
        Ok(MoveModel {
            name: name.to_string(),
            prev,
            next,
        })
    }
}

impl Command for MoveModel {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::set_transform(world, &entity, self.next);
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::set_transform(world, &entity, self.prev);
        }
    }
}
