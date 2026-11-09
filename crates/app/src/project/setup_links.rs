//! v56 (#316): the setup links as files hold them - each setup may name
//! the setup it is placed relative to (`SetupDto::relative_to`). Checked
//! on the whole file before any world write: a link naming no setup, a
//! setup linked to itself, or a loop of links is a corrupt file, refused
//! by name - never guessed (a broken link would let an edit of one setup
//! silently leave its re-clamp behind).
//!
//! Provenance: our own file grammar; no vendor material consulted.

use std::collections::HashMap;

use super::*;

/// Refuse a file whose setup links do not form a forest.
pub(super) fn check(file: &ProjectFile) -> Result<(), ProjectError> {
    let links: HashMap<&str, Option<&str>> = file
        .entities
        .iter()
        .filter_map(|e| e.setup.as_ref().map(|s| (e.name.as_str(), s.relative_to.as_deref())))
        .collect();
    for (&name, &to) in &links {
        let Some(to) = to else { continue };
        if !links.contains_key(to) {
            return Err(corrupt(format!("setup '{name}' is placed relative to '{to}', which is not a setup")));
        }
        // Walk up at most one step per setup: more is a loop.
        let mut at = Some(to);
        for _ in 0..links.len() {
            match at {
                Some(n) if n == name => {
                    return Err(corrupt(format!("setup '{name}' is placed relative to itself through a loop of links")));
                }
                Some(n) => at = links.get(n).copied().flatten(),
                None => break,
            }
        }
        if at.is_some() {
            return Err(corrupt(format!("the setup links from '{name}' loop")));
        }
    }
    Ok(())
}
