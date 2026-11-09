//! Each setup's machine and post (#343): which `.mch` a setup is clamped
//! on and which post script writes its program. A setup either owns its
//! mount (`SetupDef::machine` / `SetupDef::post` set) or follows the
//! document's (`None` - every file before v55), so an untouched document
//! mounts and posts byte for byte as before.
//!
//! ## The one-mount law (this slice)
//!
//! The whole app reads ONE mounted machine and ONE post at a time - the
//! ACTIVE setup's ([`crate::machine::of`], [`crate::posting::post_of`]):
//! the ribbon, the kind gates, generation, the limit checker and the
//! post all see the same steel. Work that would cross mounts is refused
//! by name instead of being judged against the wrong machine:
//!
//! - generating an operation whose setup mounts another machine or post
//!   than the active one ([`generation_problem`]) - activate its setup;
//! - posting a program with an operation in such a setup
//!   ([`program_problems`]) - a program is written by one post for one
//!   machine; setups with different mounts post one at a time (one file
//!   per post is a follow-up).
//!
//! With no setup owning a mount (the default) neither refusal can fire.
//!
//! Provenance: our own design; a setup being one clamping on one
//! machine, posted by that machine's post, is general machine-shop
//! practice - no vendor implementation inspected.

use flecs_ecs::core::WorldGet;
use flecs_ecs::prelude::World;

use crate::machine::Machine;
use crate::model;
use crate::schema::{MachineDef, PostRef};
use crate::setup;

/// The document's machine as stored (the singleton), what setups that
/// own no machine follow. `("", None)` when none was ever loaded.
#[must_use]
pub fn document_machine(world: &World) -> (String, Option<Machine>) {
    world
        .try_get::<&MachineDef>(|d| (d.path.clone(), d.machine.clone()))
        .unwrap_or_default()
}

/// The document's post as stored (the singleton).
#[must_use]
pub fn document_post(world: &World) -> PostRef {
    world.try_get::<&PostRef>(|p| p.clone()).unwrap_or_default()
}

/// The machine a declared setup context mounts: its own, else the
/// document's. `None` resolves to the first setup (the implicit-setup
/// law); a document without setups mounts the document's machine.
#[must_use]
pub fn machine_of(world: &World, declared: Option<&str>) -> (String, Option<Machine>) {
    match own_machine(world, declared) {
        Some(m) => (m.path, m.machine),
        None => document_machine(world),
    }
}

/// The post a declared setup context runs: its own, else the document's.
#[must_use]
pub fn post_of(world: &World, declared: Option<&str>) -> PostRef {
    own_post(world, declared).unwrap_or_else(|| document_post(world))
}

/// The setup's own machine, `None` when it follows the document.
#[must_use]
pub fn own_machine(world: &World, declared: Option<&str>) -> Option<MachineDef> {
    let name = setup::effective(world, declared)?;
    setup::def_of(world, &name)?.machine
}

/// The setup's own post, `None` when it follows the document.
#[must_use]
pub fn own_post(world: &World, declared: Option<&str>) -> Option<PostRef> {
    let name = setup::effective(world, declared)?;
    setup::def_of(world, &name)?.post
}

/// A machine's display name: the file's model, else its path, else
/// "no machine".
#[must_use]
pub fn machine_label(m: &(String, Option<Machine>)) -> String {
    match &m.1 {
        Some(def) if !def.model.is_empty() => def.model.clone(),
        _ if !m.0.is_empty() => m.0.clone(),
        Some(_) => "unnamed machine".to_owned(),
        None => "no machine".to_owned(),
    }
}

/// Why a declared setup context cannot be worked while the active setup
/// is mounted: it mounts another machine or post. `None` when they agree
/// (always, unless a setup owns its mount).
#[must_use]
pub fn mount_problem(world: &World, declared: Option<&str>) -> Option<String> {
    let here = setup::effective(world, declared)?;
    let active = setup::active(world)?;
    if here == active {
        return None;
    }
    let (m_here, m_active) = (machine_of(world, Some(&here)), machine_of(world, Some(&active)));
    if m_here != m_active {
        return Some(format!(
            "setup '{here}' is machined on {} but the active setup '{active}' mounts {} - activate '{here}' first",
            machine_label(&m_here),
            machine_label(&m_active),
        ));
    }
    let (p_here, p_active) = (post_of(world, Some(&here)), post_of(world, Some(&active)));
    if p_here != p_active {
        return Some(format!(
            "setup '{here}' posts with {} but the active setup '{active}' posts with {} - activate '{here}' first",
            crate::posting::post_ref_label(&p_here),
            crate::posting::post_ref_label(&p_active),
        ));
    }
    None
}

/// The generation door's mount clause: an operation is generated against
/// the mounted (active) machine only when its own setup mounts it too.
#[must_use]
pub fn generation_problem(world: &World, op: &crate::schema::Operation) -> Option<String> {
    mount_problem(world, op.setup.as_deref())
}

/// The post's mount clause: one program, one post, one machine. Every
/// unsuppressed operation's setup must mount what the active setup
/// mounts; each disagreeing setup is named once.
#[must_use]
pub fn program_problems(world: &World) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (_, e) in setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed {
            continue;
        }
        let Some(here) = setup::effective(world, op.setup.as_deref()) else {
            continue;
        };
        if seen.contains(&here) {
            continue;
        }
        if let Some(why) = mount_problem(world, Some(&here)) {
            out.push(format!("{why} (one program is written by one post for one machine)"));
        }
        seen.push(here);
    }
    // #389: the program-level post options ride the same law.
    out.extend(crate::setup_post_options::program_problems(world));
    out
}
