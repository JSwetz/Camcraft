//! SetStock: define or replace the setup's raw-material solid.
//!
//! Parameters are validated and the mesh is resolved at construction, so
//! undo/redo replays a finished world write and malformed stock never enters
//! the history (AGENTS.md: validate early). Stock is stored as parameters on
//! the `StockDef` component; the mirrored `Facets` keep render and pick
//! free of stock-specific paths.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::{Triangle, normalize_quat};
use crate::model;
use crate::schema::{Location, STOCK_NAME, StockDef};
use crate::stock::{self, MAX_STOCK_DIM, StockShape};

pub struct SetStock {
    shape: StockShape,
    location: Location,
    facets: Vec<Triangle>,
    /// Stock's state before this command; `None` means the command creates
    /// the stock entity, so its inverse removes it.
    prev: Option<(StockShape, Location, Vec<Triangle>)>,
}

impl SetStock {
    /// Validates against the world now. `location` places the stock; for
    /// `FromModel`, an identity `location` means "adopt the source's
    /// placement" so the copy sits exactly on the model it came from.
    pub fn new(world: &World, shape: StockShape, location: Location) -> Result<Self, CommandError> {
        if !shape.is_sane() {
            return Err(CommandError::Invalid(format!(
                "stock parameters must be finite and positive: {shape:?}"
            )));
        }
        if !shape.is_bounded() {
            return Err(CommandError::Invalid(format!(
                "stock dimensions exceed {MAX_STOCK_DIM:e} mm"
            )));
        }
        let mut location = location;
        location.rot = normalize_quat(location.rot)
            .ok_or_else(|| CommandError::Invalid("stock rotation is not a quaternion".into()))?;
        if !location.pos.is_finite() {
            return Err(CommandError::Invalid("stock placement must be finite".into()));
        }

        let facets = match &shape {
            StockShape::FromModel { source } => {
                if source == STOCK_NAME {
                    return Err(CommandError::Invalid(
                        "stock cannot be made from itself".into(),
                    ));
                }
                let Some(entity) = model::find_by_name(world, source) else {
                    return Err(CommandError::NotFound(source.clone()));
                };
                if model::is_wcs(world, &entity) {
                    return Err(CommandError::Invalid(format!(
                        "'{source}' is a work offset, not a solid"
                    )));
                }
                let tris = model::facets_of(world, &entity)
                    .filter(|f| !f.is_empty())
                    .ok_or_else(|| {
                        CommandError::Invalid(format!("'{source}' has no geometry for stock"))
                    })?;
                if location == Location::default() {
                    // Copy the source's placement too: same facets, same spot.
                    location = model::transform_of(world, &entity).unwrap_or_default();
                }
                tris
            }
            other => stock::stock_mesh(other)
                .ok_or_else(|| CommandError::Invalid("stock meshing failed".into()))?,
        };

        let prev = match model::find_by_name(world, STOCK_NAME) {
            Some(entity) => {
                let prev_shape = model::stock_def_of(world, &entity).ok_or_else(|| {
                    CommandError::Invalid(format!(
                        "an entity named '{STOCK_NAME}' exists but is not stock"
                    ))
                })?;
                Some((
                    prev_shape,
                    model::transform_of(world, &entity).unwrap_or_default(),
                    model::facets_of(world, &entity).unwrap_or_default(),
                ))
            }
            None => None,
        };
        Ok(SetStock {
            shape,
            location,
            facets,
            prev,
        })
    }
}

impl Command for SetStock {
    fn apply(&self, world: &World) {
        // Pure world write from stored fields: idempotent under redo.
        let entity = match model::find_by_name(world, STOCK_NAME) {
            Some(entity) => entity,
            None => model::spawn(world, STOCK_NAME),
        };
        model::set_transform(world, &entity, self.location);
        model::set_facets(world, &entity, self.facets.clone());
        entity
            .entity_view(world)
            .set(StockDef { shape: self.shape.clone() });
    }

    fn invert(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, STOCK_NAME) else {
            return; // already undone (idempotent undo path)
        };
        match &self.prev {
            Some((shape, location, facets)) => {
                model::set_transform(world, &entity, *location);
                model::set_facets(world, &entity, facets.clone());
                entity
                    .entity_view(world)
                    .set(StockDef { shape: shape.clone() });
            }
            None => model::remove(world, &entity), // we created it
        }
    }
}
