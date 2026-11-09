//! DeleteEntity: snapshot the named entity's persistent state at
//! construction, destruct on apply, recreate identically on invert.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::Triangle;
use crate::model;
use crate::schema::{FixtureDef, Location, Operation, OperationMeta, StockDef, ToolDef, Toolpath, Wcs};
use crate::stock::StockShape;
use crate::tools::Tool;

pub struct DeleteEntity {
    name: String,
    location: Location,
    facets: Option<std::sync::Arc<[Triangle]>>,
    /// Non-`None` restores the `StockDef` component; `wcs` restores the
    /// `Wcs` tag. Together with the CAM components below they keep every
    /// kind of named entity undoable with its kind intact.
    stock: Option<StockShape>,
    wcs: bool,
    tool: Option<Tool>,
    /// #221: a deleted tool's number, restored with it on undo.
    tool_number: Option<u32>,
    /// #368: a deleted tool's modelled body, restored with it on undo.
    tool_body: Option<crate::schema::ToolBody>,
    operation: Option<Operation>,
    operation_meta: Option<OperationMeta>,
    /// A model deleted from the tree keeps its fixture marker (M3.2
    /// stage B) through undo too - the marker is part of the entity's
    /// identity, and an undo that quietly demoted a vise to a raw
    /// solid would change what the collision law sees.
    fixture: Option<FixtureDef>,
    /// The generated path and its warnings, restored with the operation
    /// they belong to (AGENTS.md: deleting frees, undoing must restore
    /// ALL associated resources - in both directions).
    path: Option<Toolpath>,
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
            tool: model::tool_of(world, &entity),
            tool_number: model::tool_number_of(world, &entity),
            tool_body: crate::tool_body::body_of(world, &entity),
            operation: model::operation_of(world, &entity),
            operation_meta: model::is_operation(world, &entity)
                .then(|| model::operation_meta_of(world, &entity)),
            path: model::toolpath_of(world, &entity),
            fixture: crate::fixture::def_of(world, &entity),
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
        if let Some(tool) = &self.tool {
            entity
                .entity_view(world)
                .set(ToolDef { tool: tool.clone(), number: self.tool_number });
            crate::tool_body::set_body(world, &entity, self.tool_body.as_ref());
        }
        if let Some(operation) = &self.operation {
            entity.entity_view(world).set(operation.clone());
        }
        if let Some(meta) = &self.operation_meta {
            entity.entity_view(world).set(meta.clone());
        }
        if let Some(path) = &self.path {
            entity.entity_view(world).set(path.clone());
        }
        if let Some(fixture) = &self.fixture {
            entity.entity_view(world).set(fixture.clone());
        }
    }
}
