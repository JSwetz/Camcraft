//! Each setup's post options (#389): skip pauses and homes, one file per
//! operation and the thread output, owned by a setup or followed from the
//! document's - the same own / follow-the-document law as the setup's
//! post (#343, `crate::setup_mount`). A setup that owns none (`None`,
//! every earlier file) reads the document's `PostOptions` singleton, so
//! an untouched document posts byte for byte as before.
//!
//! ## Which options a program posts with
//!
//! - "Skip pauses and homes" is an operation-level effect (it mutes each
//!   operation's after-action), so each operation reads its OWN setup's
//!   choice ([`of`]) - a setup that skips posts without its stops while
//!   the next one keeps them.
//! - The Output (one file per operation) and the thread output shape the
//!   whole program, so the program posts with the ACTIVE setup's options
//!   ([`in_force`]) - the one-mount law of the post itself - and a program
//!   holding a setup that disagrees on either is refused by name
//!   ([`program_problems`]), never posted half one way.
//!
//! Provenance: our own design (the #343 mount law restated for the post
//! options); no vendor implementation inspected.

use flecs_ecs::core::WorldGet;
use flecs_ecs::prelude::World;

use crate::model;
use crate::schema::PostOptions;
use crate::setup;

/// The document's post options as stored (the singleton), what setups
/// that own none follow. The default when none was ever set.
#[must_use]
pub fn document(world: &World) -> PostOptions {
    world.try_get::<&PostOptions>(|p| *p).unwrap_or_default()
}

/// The setup's own options, `None` when it follows the document. `None`
/// as the declared context resolves to the first setup (implicit-setup
/// law); a document without setups owns none.
#[must_use]
pub fn own(world: &World, declared: Option<&str>) -> Option<PostOptions> {
    let name = setup::effective(world, declared)?;
    setup::def_of(world, &name)?.post_options
}

/// The options a declared setup context posts with: its own, else the
/// document's.
#[must_use]
pub fn of(world: &World, declared: Option<&str>) -> PostOptions {
    own(world, declared).unwrap_or_else(|| document(world))
}

/// The options the program posts with: the active setup's.
#[must_use]
pub fn in_force(world: &World) -> PostOptions {
    of(world, setup::active(world).as_deref())
}

/// The program-level clause: every unsuppressed operation's setup must
/// agree with the active setup on the Output and the thread output (the
/// skip switch is per operation and may differ). Each disagreeing setup
/// is named once. Empty whenever no setup owns its options.
#[must_use]
pub fn program_problems(world: &World) -> Vec<String> {
    let Some(active) = setup::active(world) else {
        return Vec::new();
    };
    let want = of(world, Some(&active));
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
        let has = of(world, Some(&here));
        if has.per_op != want.per_op || has.threading != want.threading {
            out.push(format!(
                "setup '{here}' posts with other post options (output or threading) than the active setup '{active}' - post the setups one at a time"
            ));
        }
        seen.push(here);
    }
    out
}
