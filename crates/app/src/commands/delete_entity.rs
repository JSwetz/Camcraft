//! DeleteEntity: snapshot the named entity's persistent state at
//! construction, destruct on apply, recreate identically on invert.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::Triangle;
use crate::model;
use crate::schema::{Location, StockDef, Wcs};
use crate::stock::StockShape;

pub struct DeleteEntity {
    name: String,
    location: Location,
    facets: Option<Vec<Triangle>>,
    /// Non-`None` restores the `StockDef` component; `wcs` restores the
    /// `Wcs` tag. Together they keep stock and work offsets undoable with
    /// their kind intact.
    stock: Option<StockShape>,
    wcs: bool,
}

impl DeleteEntity {
    /// Reads the world now (construction time). The target must exist;
    /// validating here is what keeps undo/redo exact later.
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        Ok(DeleteEntity {
            name: name.to_string(),
            location: model::transform_of(world, &entity).unwrap_or_default(),
            facets: model::facets_of(world, &entity),
            stock: model::stock_def_of(world, &entity),
            wcs: model::is_wcs(world, &entity),
        })
    }
}

impl Command for DeleteEntity {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }

    fn invert(&self, world: &World) {
        if model::find_by_name(world, &self.name).is_some() {
            return; // already restored (idempotent redo path)
        }
        let entity = model::spawn(world, &self.name);
        model::set_transform(world, &entity, self.location);
        if let Some(tris) = &self.facets {
            model::set_facets(world, &entity, tris.clone());
        }
        if let Some(shape) = &self.stock {
            entity
                .entity_view(world)
                .set(StockDef { shape: shape.clone() });
        }
        if self.wcs {
            entity.entity_view(world).add(Wcs);
        }
    }
}
