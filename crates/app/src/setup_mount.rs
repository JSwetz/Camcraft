//! Each setup's machine and post (#343): which `.mch` a setup is clamped
//! on and which post script writes its program. A setup either owns its
//! mount (`SetupDef::machine` / `SetupDef::post` set) or follows the
//! document's (`None` - every file before v55), so an untouched document
//! mounts and posts byte for byte as before.
//!
//! ## Every op on its own mount (#388)
//!
//! The ACTIVE setup's mount ([`crate::machine::of`],
//! [`crate::posting::post_of`]) is what the ribbon, the Setup pane and
//! the new-op door read - a new op joins the active setup. Every law
//! that judges an EXISTING operation reads that operation's own setup
//! instead ([`machine_of`], [`crate::machine::kind_of_setup`],
//! [`crate::posting::post_of_setup`]): generation, the spindle law, the
//! rest reading, the fixture table, the limit check. So an inactive
//! setup's op is generated against its own machine without activating
//! it - never against whatever steel happens to be active.
//!
//! A program whose setups mount different machines or posts posts as
//! one file per [`Mount`] ([`program_mounts`];
//! `posting::post_mounts`), each through its own post for its own
//! machine; with one mount (the default) the post is one file, byte for
//! byte as before.
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

/// The kind of the machine a declared setup context mounts (#388) -
/// what every law judging one operation reads, so an inactive setup's
/// op is judged on its own steel (`crate::machine::kind_of_setup`).
/// Reads the singleton without a clone unless the setup owns its machine.
#[must_use]
pub fn kind_of_setup(world: &World, declared: Option<&str>) -> crate::machine::Kind {
    if let Some(own) = own_machine(world, declared) {
        return crate::machine::kind(own.machine.as_ref());
    }
    world
        .try_get::<&MachineDef>(|d| crate::machine::kind(d.machine.as_ref()))
        .unwrap_or(crate::machine::Kind::Mill)
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

/// One mount of the program (#388): the setups (program order) that
/// share one machine and one post - what one NC file is written by.
#[derive(Clone, Debug, PartialEq)]
pub struct Mount {
    /// Effective setup names, program order; `""` for the implicit
    /// setup of a document that declares none.
    pub setups: Vec<String>,
    pub machine: (String, Option<Machine>),
    pub post: PostRef,
}

impl Mount {
    /// The mount as a refusal or a status line names it.
    #[must_use]
    pub fn label(&self) -> String {
        let names = if self.setups.iter().all(String::is_empty) {
            "the setup".to_owned()
        } else {
            self.setups.join(", ")
        };
        format!(
            "{names} ({}, {})",
            machine_label(&self.machine),
            crate::posting::post_ref_label(&self.post)
        )
    }
}

/// The program's mounts in program order of first use: every
/// unsuppressed operation's setup, grouped with the setups that mount
/// the same machine and - when `by_post` - the same post (a caller
/// overriding the post groups by machine alone). One entry for every
/// document whose setups own no mount; empty when nothing would post.
#[must_use]
pub fn program_mounts(world: &World, by_post: bool) -> Vec<Mount> {
    let mut out: Vec<Mount> = Vec::new();
    for (_, e) in setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed {
            continue;
        }
        let here = setup::effective(world, op.setup.as_deref());
        let name = here.clone().unwrap_or_default();
        if out.iter().any(|m| m.setups.contains(&name)) {
            continue;
        }
        let machine = machine_of(world, here.as_deref());
        let post = post_of(world, here.as_deref());
        match out
            .iter_mut()
            .find(|m| m.machine == machine && (!by_post || m.post == post))
        {
            Some(m) => m.setups.push(name),
            None => out.push(Mount { setups: vec![name], machine, post }),
        }
    }
    out
}
