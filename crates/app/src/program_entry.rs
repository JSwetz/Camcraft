//! Standalone Pause and Home entries in the operation list (#155): where
//! they stand among the operations, and what the post makes of them.
//! Provenance: the RS-274 / ISO 6983 program stop (M0), optional stop
//! (M1) and reference return, as #110's per-operation after-action
//! already posts them; our own sequencing law, no vendor material.
//!
//! The law: an entry is NOT an operation. It owns no tool and no
//! toolpath, so `setup::operation_order` - and every walk built on it
//! (the post's tool and freshness gate, `rest::ahead_of`, the program
//! sim, the setup sheet, the tool list, verify) - never sees one. Only
//! two walks know entries exist: [`sequence`] (the operation list's
//! order, which reorder and append number) and [`posted_afters`] (the
//! post, which folds each entry into the after-action of the posted
//! operation before it - the same `PARAM AfterOperation` word the
//! per-op choice writes, so a document without entries posts byte for
//! byte as before).

use std::collections::HashMap;

use flecs_ecs::core::EntityViewGet; // not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

use crate::model;
use crate::operation::AfterOperation;
use crate::schema::ProgramEntry;
use crate::setup;

/// The entry on `entity`, if it is one.
#[must_use]
pub fn entry_of(world: &World, entity: &Entity) -> Option<ProgramEntry> {
    entity.entity_view(world).try_get::<&ProgramEntry>(Clone::clone)
}

/// Is this named entity a program entry?
#[must_use]
pub fn is_entry(world: &World, entity: &Entity) -> bool {
    entity.entity_view(world).try_get::<&ProgramEntry>(|_| ()).is_some()
}

/// An operation or an entry: a row of the operation list.
#[must_use]
pub fn is_sequenced(world: &World, entity: &Entity) -> bool {
    model::is_operation(world, entity) || is_entry(world, entity)
}

/// The setup an operation or an entry declares (`None` - the first).
#[must_use]
pub fn declared_setup(world: &World, entity: &Entity) -> Option<String> {
    model::operation_of(world, entity)
        .map(|op| op.setup)
        .or_else(|| entry_of(world, entity).map(|e| e.setup))
        .flatten()
}

/// The setup an operation or an entry machines in, resolved.
#[must_use]
pub fn effective_setup(world: &World, entity: &Entity) -> Option<String> {
    setup::effective(world, declared_setup(world, entity).as_deref())
}

/// Every operation AND entry in list order: setup rank, then position,
/// then key - `setup::operation_order`'s exact sort, entries included.
/// On a document without entries it IS that order.
#[must_use]
pub fn sequence(world: &World) -> Vec<(String, Entity)> {
    let order = setup::names(world);
    let mut rows: Vec<(usize, u32, String, Entity)> = model::named_entities(world)
        .into_iter()
        .filter(|(_, e)| is_sequenced(world, e))
        .map(|(n, e)| {
            let declared = declared_setup(world, &e);
            let position = model::operation_meta_of(world, &e).order.unwrap_or(u32::MAX);
            (setup::rank(&order, declared.as_deref()), position, n, e)
        })
        .collect();
    rows.sort_by_key(|(r, position, _, _)| (*r, *position)); // stable: key order breaks ties
    rows.into_iter().map(|(_, _, n, e)| (n, e)).collect()
}

/// The list row's words for an entry's action.
#[must_use]
pub fn action_label(action: AfterOperation) -> &'static str {
    match action {
        AfterOperation::None => "Continue",
        AfterOperation::OptionalStop => "Optional stop (M1)",
        AfterOperation::ProgramStop => "Pause (M0)",
        AfterOperation::GoHome => "Go home",
    }
}

/// The key prefix a new entry's name takes.
#[must_use]
pub fn key_prefix(action: AfterOperation) -> &'static str {
    match action {
        AfterOperation::GoHome => "Home",
        AfterOperation::OptionalStop => "OptionalStop",
        _ => "Pause",
    }
}

/// Two after-actions in one gap, as one: equal asks are one ask, and a
/// program stop holds everything an optional stop could (M0 always
/// stops), so it absorbs it. A stop and a trip home are two different
/// motions with no single word - `None`, and the caller refuses.
fn merge(a: AfterOperation, b: AfterOperation) -> Option<AfterOperation> {
    use AfterOperation::{None as Carry, OptionalStop, ProgramStop};
    match (a, b) {
        (Carry, x) | (x, Carry) => Some(x),
        (x, y) if x == y => Some(x),
        (OptionalStop, ProgramStop) | (ProgramStop, OptionalStop) => Some(ProgramStop),
        _ => None,
    }
}

/// What each posted operation does once it is done, entries folded in:
/// `ops` is the program `posting::program_ops` blessed (suppressed ops
/// already out). An entry joins the gap after the last posted operation
/// before it in list order; a suppressed entry is out of the program.
/// `skip` answers "Skip pauses and homes" per operation (#389: each op
/// reads its own setup's switch): a skipped op ends with nothing, and an
/// entry folding into a skipped op's gap is muted with it - it cannot
/// refuse, exactly as the old document-wide switch muted every gap. An
/// entry ahead of every posted op follows the first posted op's switch. Refusals name the entry: one before every posted
/// operation (there is no section end to stop after), and a stop and a
/// trip home asked of the same gap (one word per gap).
pub fn posted_afters(
    world: &World,
    ops: &[(String, Entity)],
    skip: impl Fn(&Entity) -> bool,
) -> Result<HashMap<String, AfterOperation>, String> {
    let mut out: HashMap<String, AfterOperation> = HashMap::with_capacity(ops.len());
    let mut muted: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (name, e) in ops {
        let after = if skip(e) {
            muted.insert(name.clone());
            AfterOperation::None
        } else {
            model::operation_of(world, e).map_or(AfterOperation::None, |op| op.after)
        };
        out.insert(name.clone(), after);
    }
    let mut last: Option<String> = None;
    for (name, e) in sequence(world) {
        if out.contains_key(&name) && model::is_operation(world, &e) {
            last = Some(name);
            continue;
        }
        let Some(entry) = entry_of(world, &e) else {
            continue; // a suppressed operation: not a gap of the program
        };
        if entry.suppressed || entry.action == AfterOperation::None {
            continue;
        }
        let Some(op) = last.as_ref() else {
            // Ahead of every posted op: it stands in the first posted op's
            // program, so that op's switch decides (#389) - muted when its
            // setup skips pauses and homes, refused by name otherwise.
            if ops.first().is_some_and(|(n, _)| muted.contains(n)) {
                continue;
            }
            return Err(format!(
                "{name}: a {} entry before the first posted operation has no operation to follow - move it after one, or suppress it",
                action_label(entry.action)
            ));
        };
        if muted.contains(op) {
            continue; // the gap's op skips pauses and homes: the entry is muted
        }
        let now = out.get(op).copied().unwrap_or_default();
        match merge(now, entry.action) {
            Some(m) => {
                out.insert(op.clone(), m);
            }
            None => {
                let label = model::find_by_name(world, op)
                    .map_or_else(|| op.clone(), |oe| model::operation_label(world, op, &oe));
                return Err(format!(
                    "{name}: {label} already ends with {} - the post writes one after-action per gap, and {} after it would need a second; remove one of them",
                    action_label(now),
                    action_label(entry.action)
                ));
            }
        }
    }
    Ok(out)
}
