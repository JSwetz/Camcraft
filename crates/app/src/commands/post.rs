//! SelectPost: set the document's post script (M3.3 A).
//!
//! Setup furniture, like `LoadMachine`: no geometry changes, the
//! command writes only the world singleton and its inverse writes the
//! previous value back. A file script is validated at command
//! construction - read, then compiled and run on an empty stream with
//! no machine - so a broken `.cps` is a refusal at the front door,
//! never a surprise at post time (validate early).

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::post::engine;
use crate::posting;
use crate::schema::PostRef;

#[derive(Debug)]
pub struct SelectPost {
    prev: PostRef,
    next: PostRef,
}

impl SelectPost {
    /// Choose a post script. A `File` reference must be readable and
    /// must run: an unreadable path or a script that throws at open
    /// names itself in the refusal (the Prime Directive has no
    /// "we'll find out when we post" clause). A `Named` reference must
    /// exist in the shipped registry (M3.3 B): the picker only offers
    /// real names, so a stranger is a corrupt project, and a corrupt
    /// project's post choice is refused the same way.
    pub fn new(world: &World, next: PostRef) -> Result<Self, CommandError> {
        match &next {
            PostRef::File(path) => {
                let text = std::fs::read_to_string(path)
                    .map_err(|e| CommandError::Invalid(format!("post script {path}: {e}")))?;
                engine::run(&text, &serde_json::Value::Null, "")
                    .map_err(|e| CommandError::Invalid(format!("post script {path} does not run: {e}")))?;
            }
            PostRef::Named(name) => {
                if !posting::BUILTINS.iter().any(|(n, _)| n == name) {
                    return Err(CommandError::Invalid(format!(
                        "built-in post {name} is not in the registry"
                    )));
                }
            }
            PostRef::Builtin => {}
        }
        let prev = posting::post_of(world);
        if prev == next {
            return Err(CommandError::Invalid(
                "that post is already selected".to_owned(),
            ));
        }
        Ok(Self { prev, next })
    }
}

impl Command for SelectPost {
    fn apply(&self, world: &World) {
        posting::post_set(world, &self.next);
    }

    fn invert(&self, world: &World) {
        posting::post_set(world, &self.prev);
    }
}

/// SetPostOptions: switch the document's post-time options (#110) -
/// today the one "skip pauses and homes" switch. Setup furniture like
/// `SelectPost`: the singleton is written, the inverse writes the
/// previous value back; no operation is touched, so every op keeps its
/// after-action for the day the switch goes off again.
#[derive(Debug)]
pub struct SetPostOptions {
    prev: crate::schema::PostOptions,
    next: crate::schema::PostOptions,
}

impl SetPostOptions {
    /// Set the options. Setting what is already set is refused - an
    /// undo step that changes nothing is noise in the history.
    pub fn new(world: &World, next: crate::schema::PostOptions) -> Result<Self, CommandError> {
        let prev = posting::post_options_of(world);
        if prev == next {
            return Err(CommandError::Invalid("those post options are already set".to_owned()));
        }
        Ok(Self { prev, next })
    }
}

impl Command for SetPostOptions {
    fn apply(&self, world: &World) {
        posting::post_options_set(world, self.next);
    }

    fn invert(&self, world: &World) {
        posting::post_options_set(world, self.prev);
    }
}
