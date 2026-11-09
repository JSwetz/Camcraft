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
                // A source is stock or one of the user's parts - never a
                // tool, an operation, a setup, the stock entity by name,
                // or workholding (a fixture is a model the collision
                // checker guards, not geometry to machine).
                Some(e) if model::is_fixture(world, &e) => Err(CommandError::Invalid(format!(
                    "'{name}' is a fixture - workholding is checked for collisions, not machined"
                ))),
                Some(e) if !model::is_model(world, &e) => Err(CommandError::Invalid(format!(
                    "'{name}' is not a model - an operation's source is the stock or a part"
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
/// registry data - the row's [`Family`] read against the kind of the
/// machine the op's setup mounts (#388) - so the door opens on the table in `kinds.rs`, not
/// on a hand-typed list that forgets the twenty-second kind. A
/// mill-turn admits both families; an unstated machine is a mill
/// (the kind_of law the whole app already reads the document by).
/// The ribbon's darkening is a courtesy; this door is the law - an
/// op whose family the steel cannot spin never enters the document,
/// and the refusal names both sides by name.
pub(super) fn family_gate(world: &World, context: Option<&str>, params: &OpParams) -> Result<(), CommandError> {
    use crate::operation::Family;
    let info = params.kind().info();
    let family = info.family();
    // #388: the machine of the setup the op lives in - the active one
    // for a new op, the op's own for a paste or a retype.
    let machine = crate::machine::kind_of_setup(world, context);
    // The per-kind word (M13.5 A): the family table, plus the Drill
    // kind's admission on a plain lathe for the axial hole.
    if info.admits(machine) {
        return Ok(());
    }
    Err(CommandError::Invalid(format!(
        "a {} operation is {} work; the setup's machine is a {} - mount a {} (or a mill-turn) first",
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

/// The heights door (M11.3 A), one call for every command that writes
/// params or a source: ordering on resolved values (an op that would cut
/// upward), then every From: Selection height resolving against the
/// op's own source - an empty or vanished selection refuses by name
/// here, never reaches a generator as a silent 0.
pub(super) fn heights_gate(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
) -> Result<(), CommandError> {
    if let Some(why) = operation::selection_heights_sane(world, setup, src, params) {
        return Err(CommandError::Invalid(why));
    }
    if !operation::depth_sane(world, setup, Some(src), params) {
        return Err(CommandError::Invalid(format!(
            "a {} operation must cut downward: its top height resolves below its bottom height (check the From references)",
            params.kind()
        )));
    }
    Ok(())
}

/// The flat-area door: a pocket-family selection that collapses to a
/// line seen from above (a vertical wall's rim, a non-horizontal face)
/// is refused when it is picked or the kind is set, with the fix, not
/// only later at Generate - which keeps the same refusal as backstop.
pub(super) fn area_gate(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
    params: &OpParams,
) -> Result<(), CommandError> {
    match operation::selection_refusal(world, setup, src, params) {
        Some(why) => Err(CommandError::Invalid(why)),
        None => Ok(()),
    }
}
