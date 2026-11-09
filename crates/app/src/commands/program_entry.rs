//! Pause / Home entries of the operation list (#155) as undoable
//! commands: add one (after a row, or at its setup's end) and edit one
//! (its action, its suppression). Delete is `DeleteEntity`, reorder is
//! `ReorderOperation` - both already speak for entries.
//! Provenance: our own document model; the M0 / M1 / reference-return
//! meanings are RS-274 / ISO 6983 (see `program_entry`).

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::operation::AfterOperation;
use crate::program_entry::{declared_setup, effective_setup, entry_of, is_sequenced, key_prefix, sequence};
use crate::schema::{OperationMeta, ProgramEntry};
use crate::setup;

/// `(key, before, after)` for each list row whose position the add pins.
type Moved = Vec<(String, OperationMeta, OperationMeta)>;

fn set_metas(world: &World, rows: &Moved, forward: bool) {
    for (name, before, next) in rows {
        if let Some(entity) = model::find_by_name(world, name) {
            entity.entity_view(world).set(if forward { next.clone() } else { before.clone() });
        }
    }
}

/// Add a Pause / Home entry: right after the row `after` names, or - with
/// `None` - at the end of the active setup's list. Every row of that
/// setup's list is pinned at its position (0..n), the new entry among
/// them, so undo puts back exactly the positions it found.
pub struct AddProgramEntry {
    name: String,
    entry: ProgramEntry,
    meta: OperationMeta,
    moved: Moved,
}

impl AddProgramEntry {
    pub fn new(world: &World, action: AfterOperation, after: Option<&str>) -> Result<Self, CommandError> {
        if action == AfterOperation::None {
            return Err(CommandError::Invalid("an entry must stop or go home - Continue is no entry".into()));
        }
        let (setup, effective) = match after {
            Some(key) => {
                let e = model::find_by_name(world, key)
                    .filter(|e| is_sequenced(world, e))
                    .ok_or_else(|| CommandError::NotFound(key.to_owned()))?;
                (declared_setup(world, &e), effective_setup(world, &e))
            }
            None => {
                let active = setup::active(world);
                let effective = setup::effective(world, active.as_deref());
                (active, effective)
            }
        };
        let prefix = key_prefix(action);
        let name = (1..=u32::MAX)
            .map(|n| format!("{prefix}{n}"))
            .find(|n| model::find_by_name(world, n).is_none())
            .ok_or_else(|| CommandError::Invalid("no free entry name".into()))?;
        let mut rows: Vec<String> = sequence(world)
            .into_iter()
            .filter(|(_, e)| effective_setup(world, e) == effective)
            .map(|(n, _)| n)
            .collect();
        let at = match after {
            Some(key) => rows
                .iter()
                .position(|n| n == key)
                .map(|i| i + 1)
                .ok_or_else(|| CommandError::NotFound(key.to_owned()))?,
            None => rows.len(),
        };
        rows.insert(at, name.clone());
        let mut moved = Vec::with_capacity(rows.len());
        let mut meta = OperationMeta::default();
        for (index, key) in rows.into_iter().enumerate() {
            let order = u32::try_from(index)
                .map_err(|_| CommandError::Invalid("too many operations".into()))?;
            if key == name {
                meta.order = Some(order);
                continue;
            }
            let Some(e) = model::find_by_name(world, &key) else {
                return Err(CommandError::NotFound(key));
            };
            let before = model::operation_meta_of(world, &e);
            let mut next = before.clone();
            next.order = Some(order);
            moved.push((key, before, next));
        }
        Ok(Self {
            name,
            entry: ProgramEntry { action, setup, suppressed: false },
            meta,
            moved,
        })
    }

    /// The new entry's key (what the UI selects and the status names).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Command for AddProgramEntry {
    fn apply(&self, world: &World) {
        set_metas(world, &self.moved, true);
        let entity = match model::find_by_name(world, &self.name) {
            Some(entity) => entity,
            None => model::spawn(world, &self.name),
        };
        entity.entity_view(world).set(self.entry.clone());
        entity.entity_view(world).set(self.meta.clone());
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
        set_metas(world, &self.moved, false);
    }
}

/// Change an entry's action or its suppression - one undo unit.
pub struct EditProgramEntry {
    name: String,
    before: ProgramEntry,
    after: ProgramEntry,
}

impl EditProgramEntry {
    fn edit(world: &World, key: &str, change: impl FnOnce(&mut ProgramEntry)) -> Result<Self, CommandError> {
        let before = model::find_by_name(world, key)
            .and_then(|e| entry_of(world, &e))
            .ok_or_else(|| CommandError::NotFound(key.to_owned()))?;
        let mut after = before.clone();
        change(&mut after);
        if after == before {
            return Err(CommandError::Invalid(format!("'{key}' already reads that way")));
        }
        Ok(Self { name: key.to_owned(), before, after })
    }

    /// Leave the entry out of the posted program, or put it back.
    pub fn with_suppressed(world: &World, key: &str, suppressed: bool) -> Result<Self, CommandError> {
        Self::edit(world, key, |e| e.suppressed = suppressed)
    }

    /// What the machine does at the entry (never `Continue`).
    pub fn with_action(world: &World, key: &str, action: AfterOperation) -> Result<Self, CommandError> {
        if action == AfterOperation::None {
            return Err(CommandError::Invalid("an entry must stop or go home - Continue is no entry".into()));
        }
        Self::edit(world, key, |e| e.action = action)
    }
}

impl Command for EditProgramEntry {
    fn apply(&self, world: &World) {
        if let Some(e) = model::find_by_name(world, &self.name) {
            e.entity_view(world).set(self.after.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(e) = model::find_by_name(world, &self.name) {
            e.entity_view(world).set(self.before.clone());
        }
    }
}
