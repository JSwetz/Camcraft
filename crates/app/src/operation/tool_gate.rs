//! The tool law's gate half (M10.1 B): the world-reading questions the
//! commands and the ribbon ask of the `tool_kinds` table - does the
//! active setup hold a tool that can cut this kind, does this named tool
//! fit it, and would a tool retype strand an operation that uses it.
//!
//! Provenance: our own command-layer design (validate early, AGENTS.md
//! Prime Directive), read against the table in `tool_law`; no vendor
//! material consulted.
//!
//! Every function here is a pure read returning `None` when the door is
//! open and the refusal text when it is shut, so `CreateOperation`,
//! `EditOperationLinks`, `ChangeOperationKind`, `ChangeTool` and the
//! ribbon's darkening all speak one sentence for one fact. Tools are a
//! document-wide library; "the active setup's tools" is that library as
//! seen from the setup new work joins, and the refusal names the setup.
//! Opening, viewing or generating an existing operation is never asked
//! here - viewing is not cutting (the after-state is M10.1 C's).

use flecs_ecs::prelude::World;

use crate::model;
use crate::tools::ToolKind;

use super::KindInfo;

/// The kinds as one phrase: "drill", "flat end mill or bull-nose",
/// "drill, tap or boring bar".
#[must_use]
pub fn kinds_phrase(kinds: &[ToolKind]) -> String {
    let words: Vec<String> = kinds.iter().map(ToString::to_string).collect();
    match words.split_last() {
        None => String::from("tool"),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
    }
}

/// The setup's name as a refusal speaks it: the active setup, or - in a
/// document that declares none (the implicit-setup law) - "the setup",
/// the name the browser's single Setup row wears.
fn setup_words(world: &World) -> String {
    match crate::setup::active(world) {
        Some(name) => format!("{name}'s"),
        None => String::from("the setup's"),
    }
}

/// The tool library, name-sorted, with each tool's kind.
fn library(world: &World) -> Vec<(String, ToolKind)> {
    model::named_entities(world)
        .into_iter()
        .filter_map(|(name, e)| model::tool_of(world, &e).map(|t| (name, t.kind)))
        .collect()
}

/// The tools (by name, sorted) the kind's family accepts.
#[must_use]
pub fn fitting_tools(world: &World, info: &KindInfo) -> Vec<String> {
    library(world)
        .into_iter()
        .filter(|(_, k)| info.accepts(*k))
        .map(|(n, _)| n)
        .collect()
}

/// Why a kind cannot be created right now: no tool in the active
/// setup's list belongs to its family. `None` when one does.
#[must_use]
pub fn create_refusal(world: &World, info: &KindInfo) -> Option<String> {
    if library(world).iter().any(|(_, k)| info.accepts(*k)) {
        return None;
    }
    Some(format!(
        "No {} in {} tools - a {} operation cuts with one; add it to the tool library",
        kinds_phrase(info.tool_kinds),
        setup_words(world),
        info.label
    ))
}

/// Why the named tool cannot cut this kind. `None` when it can, and
/// also when the name is not a tool - existence is `check_tool`'s door,
/// not this one. When the whole setup holds nothing that fits, the
/// setup-level sentence answers instead (it names the real fix).
#[must_use]
pub fn tool_refusal(world: &World, info: &KindInfo, tool: &str) -> Option<String> {
    let kind = model::find_by_name(world, tool).and_then(|e| model::tool_of(world, &e))?.kind;
    if info.accepts(kind) {
        return None;
    }
    if let Some(none_fit) = create_refusal(world, info) {
        return Some(none_fit);
    }
    Some(format!(
        "'{tool}' is a {kind}; a {} operation cuts with a {}",
        info.label,
        kinds_phrase(info.tool_kinds)
    ))
}

/// Why a tool's definition cannot change kind from `prev` to `next`:
/// an operation that names it would be left holding a tool its family
/// cannot cut with. A kind-keeping edit (diameter, lengths) is never
/// refused here - a mismatch that predates this law is C's after-state,
/// not a lock on the library.
#[must_use]
pub fn retype_refusal(world: &World, tool: &str, prev: ToolKind, next: ToolKind) -> Option<String> {
    if prev == next {
        return None;
    }
    let stranded: Vec<(String, &'static KindInfo)> = model::named_entities(world)
        .into_iter()
        .filter_map(|(name, e)| model::operation_of(world, &e).map(|op| (name, op)))
        .filter(|(_, op)| op.tool == tool)
        .map(|(name, op)| (name, op.params.kind().info()))
        .filter(|(_, info)| !info.accepts(next))
        .collect();
    let (first, info) = stranded.first()?;
    let more = match stranded.len().saturating_sub(1) {
        0 => String::new(),
        1 => String::from(" (and 1 more operation)"),
        n => format!(" (and {n} more operations)"),
    };
    Some(format!(
        "'{tool}' cannot become a {next}: {first}{more} uses it, and a {} operation cuts with a {} - give it another tool first",
        info.label,
        kinds_phrase(info.tool_kinds)
    ))
}
