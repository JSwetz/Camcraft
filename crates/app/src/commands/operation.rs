//! Operation commands: create, edit, delete, and generate.
//!
//! `GenerateOperation` is where this layer's central design choice lives:
//! generation runs in the constructor, while the world is being read
//! anyway and errors can still refuse the command (validate early). The
//! resulting path, warnings and stamp become plain fields, so `apply` and
//! `invert` are pure component writes - an undo of an unrelated edit and
//! a redo of the generation replay the identical bytes, no re-run.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the import/stock siblings.

use flecs_ecs::prelude::{ComponentId, World};

use crate::cl::Toolpath as ClPath;
use crate::commands::{Command, CommandError};
use crate::geometry::feature;
use crate::model;
use crate::operation::{self, FeatureSel, GeoSel, OpParams, SrcRef};
use crate::schema::{Location, Operation, STOCK_NAME, Toolpath};
use crate::verify::Warning;

fn valid_src(world: &World, src: &SrcRef) -> Result<(), CommandError> {
    match src {
        SrcRef::Stock => match model::find_by_name(world, STOCK_NAME) {
            Some(e) if model::is_stock(world, &e) => Ok(()),
            Some(_) => Err(CommandError::Invalid(format!(
                "'{STOCK_NAME}' exists but is not stock"
            ))),
            None => Err(CommandError::Invalid("no stock: set up stock first".into())),
        },
        SrcRef::Entity(name) => {
            if name.is_empty() {
                return Err(CommandError::Invalid("empty geometry source name".into()));
            }
            match model::find_by_name(world, name) {
                Some(e) if model::is_wcs(world, &e) => Err(CommandError::Invalid(format!(
                    "'{name}' is a work offset, not a solid"
                ))),
                Some(_) => Ok(()),
                None => Err(CommandError::NotFound(name.clone())),
            }
        }
        SrcRef::Selection(items) => {
            if items.is_empty() {
                return Err(CommandError::Invalid(
                    "a selection source needs at least one feature".into(),
                ));
            }
            // Feature ids resolve at generation (they are geometry
            // hashes, not pointers); the entity behind each must exist
            // and be a solid, and that is all construction can promise.
            for g in items {
                if g.entity.is_empty() {
                    return Err(CommandError::Invalid(
                        "a selected feature names no entity".into(),
                    ));
                }
                match model::find_by_name(world, &g.entity) {
                    Some(e) if model::is_wcs(world, &e) => {
                        return Err(CommandError::Invalid(format!(
                            "'{}' is a work offset, not a solid",
                            g.entity
                        )));
                    }
                    Some(_) => {}
                    None => return Err(CommandError::NotFound(g.entity.clone())),
                }
            }
            Ok(())
        }
    }
}

fn check_tool(world: &World, tool: &str) -> Result<(), CommandError> {
    match model::find_by_name(world, tool) {
        Some(e) if model::is_tool(world, &e) => Ok(()),
        Some(_) => Err(CommandError::Invalid(format!("'{tool}' is not a tool"))),
        None => Err(CommandError::NotFound(tool.to_string())),
    }
}

/// Create an operation entity. Deliberately does not generate: a new
/// operation reads "never generated" until `GenerateOperation` runs, the
/// same lifecycle the tree and status surface are built around.
pub struct CreateOperation {
    name: String,
    operation: Operation,
}

impl CreateOperation {
    pub fn new(
        world: &World,
        name: &str,
        tool: &str,
        src: SrcRef,
        params: OpParams,
    ) -> Result<Self, CommandError> {
        if name.is_empty() {
            return Err(CommandError::Invalid("operation name must not be empty".into()));
        }
        if model::find_by_name(world, name).is_some() {
            return Err(CommandError::NameConflict(name.to_string()));
        }
        check_tool(world, tool)?;
        valid_src(world, &src)?;
        if !params.is_sane() {
            return Err(CommandError::Invalid(format!(
                "operation parameters are not usable for a {} operation",
                params.kind()
            )));
        }
        Ok(CreateOperation {
            name: name.to_string(),
            operation: Operation {
                tool: tool.to_string(),
                src,
                params,
                stamp: None,
            },
        })
    }
}

impl Command for CreateOperation {
    fn apply(&self, world: &World) {
        let entity = match model::find_by_name(world, &self.name) {
            Some(entity) => entity,
            None => model::spawn(world, &self.name),
        };
        entity.entity_view(world).set(self.operation.clone());
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }
}

/// Replace an operation's parameters. The cache is left alone and the
/// stamp is dropped, so the operation reads Stale immediately: the old
/// path stays on screen, visibly marked as not matching its definition.
pub struct EditOperationParams {
    name: String,
    prev: Operation,
    next: Operation,
}

impl EditOperationParams {
    pub fn new(world: &World, name: &str, params: OpParams) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        if !params.is_sane() {
            return Err(CommandError::Invalid(format!(
                "operation parameters are not usable for a {} operation",
                params.kind()
            )));
        }
        let next = Operation {
            params,
            stamp: None,
            ..prev.clone()
        };
        Ok(EditOperationParams { name: name.to_string(), prev, next })
    }
}

impl Command for EditOperationParams {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}

/// Retarget an operation (tool or source). Same staleness story as a
/// parameter edit: the cache survives, the stamp drops.
pub struct EditOperationLinks {
    name: String,
    prev: Operation,
    next: Operation,
}

impl EditOperationLinks {
    pub fn new(
        world: &World,
        name: &str,
        tool: Option<&str>,
        src: Option<SrcRef>,
    ) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        let mut next = Operation {
            stamp: None,
            ..prev.clone()
        };
        if let Some(tool) = tool {
            check_tool(world, tool)?;
            next.tool = tool.to_string();
        }
        if let Some(src) = src {
            valid_src(world, &src)?;
            next.src = src;
        }
        Ok(EditOperationLinks { name: name.to_string(), prev, next })
    }
}

impl Command for EditOperationLinks {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}

/// Set which features an operation machines - the pick list behind
/// [`SrcRef::Selection`]. Unlike a links edit the stamp survives:
/// staleness is derived from the digest, so re-picking the same
/// geometry stays fresh, and a different pick flips the operation
/// Stale on its own. Empty picks are refused - clearing reverts the
/// source with [`EditOperationLinks`] instead.
pub struct SetOperationGeometry {
    name: String,
    prev: Operation,
    next: Operation,
}

impl SetOperationGeometry {
    pub fn new(world: &World, name: &str, items: Vec<GeoSel>) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        if items.is_empty() {
            return Err(CommandError::Invalid(
                "a selection needs at least one feature - revert the source to clear it".into(),
            ));
        }
        // The pick list is the user's: order kept, exact repeats (same
        // entity and feature) collapsed - a doubled ring would be a
        // zero-wide sliver where it bounds.
        let mut deduped: Vec<GeoSel> = Vec::with_capacity(items.len());
        for g in items {
            if !deduped.iter().any(|d| d.entity == g.entity && d.feature == g.feature) {
                deduped.push(g);
            }
        }
        for g in &deduped {
            valid_pick(world, g)?;
        }
        let next = Operation {
            src: SrcRef::Selection(deduped),
            ..prev.clone()
        };
        Ok(SetOperationGeometry { name: name.to_string(), prev, next })
    }
}

/// One pick, checked against the model as it stands: the entity exists,
/// is solid, has geometry to select from, and the feature id resolves
/// right now (interactive adds never queue a stale pick; ids that
/// vanish later go Stale at read time, not Invalid at load).
fn valid_pick(world: &World, g: &GeoSel) -> Result<(), CommandError> {
    if g.entity.is_empty() {
        return Err(CommandError::Invalid(
            "a selected feature names no entity".into(),
        ));
    }
    let Some(e) = model::find_by_name(world, &g.entity) else {
        return Err(CommandError::NotFound(g.entity.clone()));
    };
    if model::is_wcs(world, &e) {
        return Err(CommandError::Invalid(format!(
            "'{}' is a work offset, not a solid",
            g.entity
        )));
    }
    let Some(features) = model::features_of(world, &e) else {
        return Err(CommandError::Invalid(format!(
            "'{}' has no geometry to select from",
            g.entity
        )));
    };
    let found = match g.feature {
        FeatureSel::Face(id) => features.faces.iter().any(|f| f.id == id),
        FeatureSel::Chain(id) => feature::chains(&features).iter().any(|c| c.id == id),
    };
    if !found {
        return Err(CommandError::Invalid(format!(
            "{} of '{}' is not in the model",
            g.feature, g.entity
        )));
    }
    Ok(())
}

impl Command for SetOperationGeometry {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}

/// Delete an operation with its cached path; the snapshot restores both.
pub struct DeleteOperation {
    name: String,
    operation: Operation,
    cache: Option<(ClPath, Vec<Warning>)>,
    location: Location,
}

impl DeleteOperation {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let operation = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        let cache = model::toolpath_of(world, &entity).map(|t| (t.path, t.warnings));
        Ok(DeleteOperation {
            name: name.to_string(),
            operation,
            cache,
            location: model::transform_of(world, &entity).unwrap_or_default(),
        })
    }
}

impl Command for DeleteOperation {
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
        entity.entity_view(world).set(self.operation.clone());
        if let Some((path, warnings)) = &self.cache {
            entity.entity_view(world).set(Toolpath {
                path: path.clone(),
                warnings: warnings.clone(),
            });
        }
    }
}

/// Generate (or re-generate) an operation: the constructor runs the
/// whole pipeline - region, generator, verification, stamp - so a
/// failure refuses the command instead of half-writing the world, and a
/// success carries its finished output as inert fields.
pub struct GenerateOperation {
    name: String,
    prev_operation: Operation,
    prev_cache: Option<Toolpath>,
    next_operation: Operation,
    output: Toolpath,
}

impl GenerateOperation {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev_operation = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        let prev_cache = model::toolpath_of(world, &entity);
        let (path, warnings) = operation::generate(world, &prev_operation)
            .map_err(|e| CommandError::Machining(format!("{name}: {e}")))?;
        let stamp = operation::current_stamp(
            world,
            &prev_operation.tool,
            &prev_operation.src,
            &prev_operation.params,
        )
        .ok_or_else(|| {
            CommandError::Machining(format!(
                "{name}: its tool or geometry source vanished mid-generation"
            ))
        })?;
        let next_operation = Operation {
            stamp: Some(stamp),
            ..prev_operation.clone()
        };
        Ok(GenerateOperation {
            name: name.to_string(),
            prev_operation,
            prev_cache,
            next_operation,
            output: Toolpath { path, warnings },
        })
    }
}

impl Command for GenerateOperation {
    fn apply(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.name) else {
            return; // the tree deleted it underneath us; nothing to write
        };
        entity.entity_view(world).set(self.next_operation.clone());
        entity
            .entity_view(world)
            .set(self.output.clone());
    }

    fn invert(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.name) else {
            return;
        };
        entity.entity_view(world).set(self.prev_operation.clone());
        match &self.prev_cache {
            Some(cache) => {
                entity.entity_view(world).set(cache.clone());
            }
            None => {
                // No cache existed before: remove the component by its id
                // (flecs removals are id-based for data components; the
                // value form only works for tags).
                entity
                    .entity_view(world)
                    .remove(Toolpath::entity_id(world));
            }
        }
    }
}
