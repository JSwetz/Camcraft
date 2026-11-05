//! Shared construction gates for the operation commands: source, tool, family, and tool-law checks.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the import/stock siblings.

use flecs_ecs::prelude::World;

use crate::commands::CommandError;
use crate::model;
use crate::operation::{self, OpParams, SrcRef};

pub(super) fn valid_src(world: &World, setup: Option<&str>, src: &SrcRef) -> Result<(), CommandError> {
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

pub(super) fn check_tool(world: &World, tool: &str) -> Result<(), CommandError> {
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
pub(super) fn family_gate(world: &World, params: &OpParams) -> Result<(), CommandError> {
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
pub(super) fn tool_gate(world: &World, params: &OpParams, tool: &str) -> Result<(), CommandError> {
    match operation::tool_gate::tool_refusal(world, params.kind().info(), tool) {
        Some(msg) => Err(CommandError::Invalid(msg)),
        None => Ok(()),
    }
}
