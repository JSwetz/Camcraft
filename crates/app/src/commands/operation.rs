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
use crate::schema::{Location, Operation, Toolpath};
use crate::verify::Warning;

fn valid_src(world: &World, setup: Option<&str>, src: &SrcRef) -> Result<(), CommandError> {
    match src {
        SrcRef::Stock => {
            let name = crate::setup::stock_entity_name(world, setup);
            match model::find_by_name(world, &name) {
                Some(e) if model::is_stock(world, &e) => Ok(()),
                Some(_) => {
                    Err(CommandError::Invalid(format!("'{name}' exists but is not stock")))
                }
                None => Err(CommandError::Invalid("no stock: set up stock first".into())),
            }
        }
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

/// The family gate (M5.2 stage A): which machine a kind lives on is
/// registry data - the row's [`Family`] read against the document's
/// machine kind - so the door opens on the table in `kinds.rs`, not
/// on a hand-typed list that forgets the twenty-second kind. A
/// mill-turn admits both families; an unstated machine is a mill
/// (the kind_of law the whole app already reads the document by).
/// The ribbon's darkening is a courtesy; this door is the law - an
/// op whose family the steel cannot spin never enters the document,
/// and the refusal names both sides by name.
fn family_gate(world: &World, params: &OpParams) -> Result<(), CommandError> {
    use crate::operation::Family;
    let family = params.kind().info().family();
    let machine = crate::machine::kind_of(world);
    if family.admits(machine) {
        return Ok(());
    }
    Err(CommandError::Invalid(format!(
        "a {} operation is {} work; the document's machine is a {} - mount a {} (or a mill-turn) first",
        params.kind(),
        match family {
            Family::Mill => "milling",
            Family::Turn => "turning",
        },
        machine.word(),
        match family {
            Family::Mill => "mill",
            Family::Turn => "lathe",
        }
    )))
}

/// The tool law's door (M10.1 B): the named tool must belong to the
/// kind's family per the `tool_kinds` table, and when the active
/// setup's tools hold nothing that fits, the refusal says so by kind
/// and setup name. Runs at construction, before any entity exists, so
/// every surface (UI, CLI, scripts, undo replay) meets the same wall.
fn tool_gate(world: &World, params: &OpParams, tool: &str) -> Result<(), CommandError> {
    match operation::tool_gate::tool_refusal(world, params.kind().info(), tool) {
        Some(msg) => Err(CommandError::Invalid(msg)),
        None => Ok(()),
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
        // The new operation JOINS the active setup (M3.2 A) - declared at
        // creation, never inferred later, so undo replays membership
        // exactly. With no setups the name is `None`: the implicit-setup
        // law makes that the first/legacy context byte-identically.
        let setup = crate::setup::active(world);
        valid_src(world, setup.as_deref(), &src)?;
        if !params.is_sane() {
            return Err(CommandError::Invalid(format!(
                "operation parameters are not usable for a {} operation",
                params.kind()
            )));
        }
        // Ordering needs resolved heights, and the world is in hand right
        // here: an operation that would cut upward is refused at creation
        // (AGENTS.md: validate early), not deep in the generator.
        if !crate::operation::depth_sane(world, setup.as_deref(), &params) {
            return Err(CommandError::Invalid(format!(
                "a {} operation must cut downward: its top height resolves below its bottom height (check the From references)",
                params.kind()
            )));
        }
        if let Some(msg) = crate::operation::side_sane(&params, &src) {
            return Err(CommandError::Invalid(msg));
        }
        family_gate(world, &params)?;
        tool_gate(world, &params, tool)?;
        Ok(CreateOperation {
            name: name.to_string(),
            operation: Operation {
                tool: tool.to_string(),
                src,
                params,
                stamp: None,
                setup,
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
        // The `OpParams` variant IS the operation's kind, so an edit that
        // carries a different variant would retype the operation in place -
        // replacing one kind's work with another's and reading as the op
        // vanishing from the list. The page carries no kind control at
        // all (M9.1); this is the defense-in-depth refusal behind it, and
        // it names the real door: ChangeOperationKind, the row's menu.
        if params.kind() != prev.params.kind() {
            return Err(CommandError::Invalid(format!(
                "'{name}' is a {} operation; change its kind with the retype menu on its browser row",
                prev.params.kind()
            )));
        }
        if !params.is_sane() {
            return Err(CommandError::Invalid(format!(
                "operation parameters are not usable for a {} operation",
                params.kind()
            )));
        }
        if !crate::operation::depth_sane(world, prev.setup.as_deref(), &params) {
            return Err(CommandError::Invalid(format!(
                "a {} operation must cut downward: its top height resolves below its bottom height (check the From references)",
                params.kind()
            )));
        }
        if let Some(msg) = crate::operation::side_sane(&params, &prev.src) {
            return Err(CommandError::Invalid(msg));
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

/// Change an operation's kind in place (M9.1): the retype the page's
/// Type combo used to do as a draft edit plus an Apply, now one honest
/// command from the op's context menu. The numbers ride
/// `operation::retype_params` - the SAME carry law a ribbon quick-add
/// and the old combo used, so re-typing never throws away feeds and
/// heights - and the result passes every door a creation passes:
/// family against the mounted machine, sanity, depth, side. The stamp
/// drops (the work definition changed; the cached path is another
/// kind's answer), and undo puts the whole old operation back.
pub struct ChangeOperationKind {
    name: String,
    prev: Operation,
    next: Operation,
}

impl ChangeOperationKind {
    pub fn new(world: &World, name: &str, kind: usize) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        let params = operation::retype_params(&prev.params, kind);
        if params.kind() == prev.params.kind() {
            return Err(CommandError::Invalid(format!(
                "'{name}' is already a {} operation",
                prev.params.kind()
            )));
        }
        family_gate(world, &params)?;
        if !params.is_sane() {
            return Err(CommandError::Invalid(format!(
                "operation parameters are not usable for a {} operation",
                params.kind()
            )));
        }
        if !crate::operation::depth_sane(world, prev.setup.as_deref(), &params) {
            return Err(CommandError::Invalid(format!(
                "a {} operation must cut downward: its top height resolves below its bottom height (check the From references)",
                params.kind()
            )));
        }
        if let Some(msg) = crate::operation::side_sane(&params, &prev.src) {
            return Err(CommandError::Invalid(msg));
        }
        // The retype is a creation of the new kind against the op's
        // current tool: the same tool-law door (M10.1 B).
        tool_gate(world, &params, &prev.tool)?;
        let next = Operation { params, stamp: None, ..prev.clone() };
        Ok(ChangeOperationKind { name: name.to_string(), prev, next })
    }
}

impl Command for ChangeOperationKind {
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
            // Reassigning the tool meets the tool law (M10.1 B); naming
            // the tool the op already holds is no reassignment, so a
            // source-only edit never trips over a legacy mismatch.
            if tool != prev.tool {
                tool_gate(world, &next.params, tool)?;
            }
            next.tool = tool.to_string();
        }
        if let Some(src) = src {
            valid_src(world, prev.setup.as_deref(), &src)?;
            // Retargeting onto a whole-model source under Inside would
            // arm the mirror law; refused here, and backstopped at
            // generate for commands that arrive via file load.
            if let Some(msg) = crate::operation::side_sane(&next.params, &src) {
                return Err(CommandError::Invalid(msg));
            }
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
        FeatureSel::Patch(id) => feature::patches(&features).iter().any(|p| p.id == id),
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

/// The constructor split at M5.3 B's seam: [`stage_generation`] does
/// EVERY world read - the entity, the components, the whole job
/// collection, the stamp - and [`Staged::finish`] runs the pure result.
/// One op does stage and finish together, exactly as before; a batch
/// stages all and finishes in parallel, and because a constructor
/// never writes the world, that batch is byte-identical to the serial
/// loop it replaces.
struct Staged {
    name: String,
    prev_operation: Operation,
    prev_cache: Option<Toolpath>,
    job: operation::GenJob,
    stamp: Option<operation::GenStamp>,
}

fn stage_generation(world: &World, name: &str) -> Result<Staged, CommandError> {
    let Some(entity) = model::find_by_name(world, name) else {
        return Err(CommandError::NotFound(name.to_string()));
    };
    let prev_operation = model::operation_of(world, &entity)
        .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
    let prev_cache = model::toolpath_of(world, &entity);
    let job = operation::collect_job(world, &prev_operation, name)
        .map_err(|e| CommandError::Machining(format!("{name}: {e}")))?;
    let stamp = operation::current_stamp(
        world,
        &prev_operation.tool,
        &prev_operation.src,
        &prev_operation.params,
        name,
        prev_operation.setup.as_deref(),
    );
    Ok(Staged {
        name: name.to_string(),
        prev_operation,
        prev_cache,
        job,
        stamp,
    })
}

impl Staged {
    fn finish(self) -> Result<GenerateOperation, CommandError> {
        let (path, warnings) = operation::run_job(self.job)
            .map_err(|e| CommandError::Machining(format!("{}: {e}", self.name)))?;
        let stamp = self.stamp.ok_or_else(|| {
            CommandError::Machining(format!(
                "{}: its tool or geometry source vanished mid-generation",
                self.name
            ))
        })?;
        let next_operation = Operation {
            stamp: Some(stamp),
            ..self.prev_operation.clone()
        };
        Ok(GenerateOperation {
            name: self.name,
            prev_operation: self.prev_operation,
            prev_cache: self.prev_cache,
            next_operation,
            output: Toolpath { path, warnings },
        })
    }
}

impl GenerateOperation {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        stage_generation(world, name)?.finish()
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

/// The M3.1 payoff command: one click brings the job current after a
/// model revision, instead of rebuilding it op by op. Construction
/// decides everything - it finds the Stale operations in tree order and
/// runs each generator exactly as `GenerateOperation` would (validate
/// early, errors still land here) - so `apply` and `invert` are the
/// batch's pure writes. An operation whose regeneration fails is
/// SKIPPED, not fatal: one broken pick must not block the rest of the
/// job (never crash, never gouge - the failed op keeps its old cache and
/// its STALE tag, honestly). Undo inverts the whole batch in reverse,
/// one undo unit for one user action, and operations that were only
/// skipped were never touched, so nothing needs restoring for them.
pub struct RegenerateAll {
    parts: Vec<GenerateOperation>,
    skipped: Vec<(String, String)>,
}

impl RegenerateAll {
    /// Runs every stale operation's generator against the current world.
    /// `Never` and `Ok` operations are left alone - this command is for
    /// what a revision invalidated, not for first-time generation.
    ///
    /// The M5.3 B shape: stage every op first (all world reads, tree
    /// order, the same status gate the serial loop had), then finish the
    /// jobs through rayon - ordered `collect`, so `parts` is tree order
    /// and every result is what the serial batch produced, byte for
    /// byte. `skipped` lists stage failures before run failures; the
    /// set is unchanged.
    pub fn new(world: &World) -> Self {
        let mut parts = Vec::new();
        let mut skipped = Vec::new();
        let mut staged = Vec::new();
        // THE program order (M3.2 A): a rest-aware op regenerates AFTER
        // what it re-machines, so one round converges - the same walk
        // the post makes. Nothing here writes the world (constructors
        // only read), so the frozen batch every job collects against
        // is the same frozen batch the serial loop ran on.
        for (name, entity) in crate::setup::operation_order(world) {
            if operation::status_of(world, &entity) != operation::OpStatus::Stale {
                continue;
            }
            match stage_generation(world, &name) {
                Ok(job) => staged.push(job),
                Err(err) => skipped.push((name, err.to_string())),
            }
        }
        use rayon::prelude::*;
        let results: Vec<(String, Result<GenerateOperation, CommandError>)> = staged
            .into_par_iter()
            .map(|job| {
                let name = job.name.clone();
                (name, job.finish())
            })
            .collect();
        for (name, result) in results {
            match result {
                Ok(part) => parts.push(part),
                Err(err) => skipped.push((name, err.to_string())),
            }
        }
        RegenerateAll { parts, skipped }
    }

    /// Nothing will be written: the UI skips the history entirely then.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    pub fn regenerated(&self) -> usize {
        self.parts.len()
    }

    /// The names whose generators refused, with the refusal text.
    pub fn skipped(&self) -> &[(String, String)] {
        &self.skipped
    }
}

impl Command for RegenerateAll {
    fn apply(&self, world: &World) {
        for part in &self.parts {
            part.apply(world);
        }
    }

    fn invert(&self, world: &World) {
        for part in self.parts.iter().rev() {
            part.invert(world);
        }
    }
}
