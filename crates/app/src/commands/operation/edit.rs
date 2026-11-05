//! Operation edit commands: parameter edits, kind changes, and tool/source link retargeting.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the import/stock siblings.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::operation::{self, OpParams, SrcRef};
use crate::schema::Operation;

use super::gates::{check_tool, family_gate, tool_gate, valid_src};

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
