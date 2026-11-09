//! The operation list's Pause / Home entries (#155) in the browser: the
//! entry row, and the menu rows that add one (on an operation, an entry,
//! or a setup's Operations row) or change one. Every row queues one
//! undoable command; delete is `DeleteByName`, as for any named row.
//! Provenance: our own tree law (issue #155); no vendor material consulted.

use flecs_ecs::prelude::Entity;

use camcraft_app::operation::AfterOperation;
use camcraft_app::program_entry::{self, action_label};
use camcraft_app::schema::ProgramEntry;

use crate::state::{CommandKind, EntryVerb, State};

use super::browser_tree::{push_row, Leaf};
use super::icons::Icon;
use super::painted;

/// Menu items, keyed past every kind ordinal and `tree_menu`'s own verbs.
pub(super) const ADD_PAUSE: usize = usize::MAX - 16;
pub(super) const ADD_HOME: usize = usize::MAX - 17;
pub(super) const ADD_OPTIONAL: usize = usize::MAX - 18;
const DELETE: usize = usize::MAX - 19;
const SUPPRESS: usize = usize::MAX - 20;
const STEP_ACTION: usize = usize::MAX - 21;

/// The actions an entry can ask, in menu order.
const ACTIONS: [AfterOperation; 3] = [AfterOperation::ProgramStop, AfterOperation::GoHome, AfterOperation::OptionalStop];

/// One entry row: its key, what it does, and SUPPRESSED when left out.
pub(super) fn entry_row(
    state: &State,
    name: &str,
    entity: &Entity,
    depth: usize,
    rows: &mut Vec<painted::Row>,
    leaves: &mut Vec<Leaf>,
) {
    let entry = program_entry::entry_of(&state.world, entity);
    let mut suffix = entry.as_ref().map_or_else(String::new, |e| action_label(e.action).to_owned());
    if entry.as_ref().is_some_and(|e| e.suppressed) {
        suffix.push_str("  SUPPRESSED");
    }
    let selected = camcraft_app::model::is_selected(&state.world, entity);
    push_row(rows, leaves, name.to_owned(), suffix, painted::Kind::Section, Some(Icon::Post), selected, false, depth, Leaf::Entry(name.to_owned()));
}

/// The add rows every operation-list menu carries.
pub(super) fn add_lines() -> [(&'static str, bool, Option<usize>); 3] {
    [
        ("Add pause (M0) after", true, Some(ADD_PAUSE)),
        ("Add go home after", true, Some(ADD_HOME)),
        ("Add optional stop (M1) after", true, Some(ADD_OPTIONAL)),
    ]
}

/// Queue the add `item` names, after the row `after` names (`None` - at
/// the active setup's end). Returns false when `item` is no add row.
pub(super) fn add(state: &mut State, item: usize, after: Option<&str>) -> bool {
    let action = match item {
        ADD_PAUSE => AfterOperation::ProgramStop,
        ADD_HOME => AfterOperation::GoHome,
        ADD_OPTIONAL => AfterOperation::OptionalStop,
        _ => return false,
    };
    state.pending.push_back(CommandKind::ProgramEntry(EntryVerb::Add { action, after: after.map(str::to_owned) }));
    true
}

/// The entry menu's rows.
pub(super) fn entry_lines(entry: &ProgramEntry) -> Vec<(&'static str, bool, Option<usize>)> {
    let mut lines = vec![("Entry", false, None)];
    lines.push((step_label(entry.action), true, Some(STEP_ACTION)));
    lines.push((if entry.suppressed { "Unsuppress entry" } else { "Suppress entry" }, true, Some(SUPPRESS)));
    lines.push(("Delete entry (undoable)", true, Some(DELETE)));
    lines.extend(add_lines());
    lines
}

fn step_label(a: AfterOperation) -> &'static str {
    match a {
        AfterOperation::GoHome => "Does: go home (click to change)",
        AfterOperation::OptionalStop => "Does: optional stop (M1)",
        _ => "Does: pause (M0)",
    }
}

/// Act on an entry menu row.
pub(super) fn act(state: &mut State, item: usize, name: &str, entry: &ProgramEntry) {
    if add(state, item, Some(name)) {
        return;
    }
    let verb = match item {
        DELETE => {
            state.pending.push_back(CommandKind::DeleteByName(name.to_owned()));
            return;
        }
        SUPPRESS => EntryVerb::Suppress { name: name.to_owned(), suppressed: !entry.suppressed },
        STEP_ACTION => {
            let k = ACTIONS.iter().position(|a| *a == entry.action).unwrap_or(0);
            let action = ACTIONS.get((k + 1) % ACTIONS.len()).copied().unwrap_or(AfterOperation::ProgramStop);
            EntryVerb::SetAction { name: name.to_owned(), action }
        }
        _ => return,
    };
    state.pending.push_back(CommandKind::ProgramEntry(verb));
}
